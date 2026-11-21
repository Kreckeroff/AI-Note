use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use super::{
    PageAddress, Store,
    page_history::record_current_snapshot,
    tasks::{TaskScope, lock_task_area},
};
use crate::{clock::now_ms, document_path::page_path, text::fold_name};

#[cfg(test)]
mod tests;

const RUN_LEASE_MS: i64 = 120_000;
pub(crate) const SOURCE_EXCERPT_LIMIT: i32 = 2_200;

#[derive(FromRow)]
pub(crate) struct Approach {
    pub instruction: String,
    pub source_page_id: Uuid,
    pub source_revision: i64,
    pub source_excerpt: String,
    pub require_source: bool,
    pub version: i64,
}

impl Approach {
    pub(crate) fn permits_task_source(&self, task_source: Option<Uuid>) -> bool {
        !self.require_source || task_source == Some(self.source_page_id)
    }
}

#[derive(FromRow)]
pub(crate) struct ApproachRun {
    pub id: Uuid,
    pub status: String,
    pub output: Option<String>,
    pub result_page_id: Option<Uuid>,
    pub source_page_id: Uuid,
    pub source_revision: i64,
    pub spec_version: i64,
}

pub(crate) struct RunInput {
    pub instruction: String,
    pub source: String,
    pub task: String,
}

#[derive(Clone, Copy)]
pub(crate) struct ApproachVersions {
    pub spec: i64,
    pub source: i64,
    pub task: i64,
}

pub(crate) enum StartApproach {
    Started(RunInput),
    AlreadyStarted,
    Unavailable,
    TaskDone,
    SourceRequired,
    PreviewChanged,
    ActiveRun,
}

#[derive(FromRow)]
struct ApproachTask {
    title: String,
    status: String,
    version: i64,
    source_page_id: Option<Uuid>,
}

async fn owner(tx: &mut PgConnection, scope: TaskScope) -> Result<bool, sqlx::Error> {
    Ok(lock_task_area(tx, scope, None)
        .await?
        .is_some_and(|area| area.can_manage))
}

async fn read_approach(
    connection: &mut PgConnection,
    scope: TaskScope,
) -> Result<Option<Approach>, sqlx::Error> {
    sqlx::query_as(
        "SELECT c.instruction,c.source_page_id,p.content_revision AS source_revision,
                left(p.markdown,$4) AS source_excerpt,c.require_source,c.version
         FROM workspace_approaches c
         JOIN pages p ON (p.tenant_id,p.space_id,p.id) = (c.tenant_id,c.space_id,c.source_page_id)
         CROSS JOIN LATERAL page_permissions($1,$2,$3,p.id) access
         WHERE (c.tenant_id,c.space_id)=($1,$2) AND access.can_read",
    )
    .bind(scope.tenant)
    .bind(scope.space)
    .bind(scope.account)
    .bind(SOURCE_EXCERPT_LIMIT)
    .fetch_optional(connection)
    .await
}

impl Store {
    pub(crate) async fn approach(&self, scope: TaskScope) -> Result<Option<Approach>, sqlx::Error> {
        read_approach(&mut *self.0.acquire().await?, scope).await
    }

    pub(crate) async fn set_approach(
        &self,
        scope: TaskScope,
        version: i64,
        instruction: &str,
        source: Uuid,
        require_source: bool,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        if !owner(&mut tx, scope).await? {
            return Ok(false);
        }
        let read =
            sqlx::query_scalar::<_, bool>("SELECT can_read FROM page_permissions($1,$2,$3,$4)")
                .bind(scope.tenant)
                .bind(scope.space)
                .bind(scope.account)
                .bind(source)
                .fetch_optional(&mut *tx)
                .await?
                .unwrap_or(false);
        if !read {
            return Ok(false);
        }
        let current = sqlx::query_scalar::<_, i64>(
            "SELECT version FROM workspace_approaches WHERE (tenant_id,space_id)=($1,$2)",
        )
        .bind(scope.tenant)
        .bind(scope.space)
        .fetch_optional(&mut *tx)
        .await?
        .unwrap_or(0);
        if current != version {
            return Ok(false);
        }
        sqlx::query("INSERT INTO workspace_approaches (tenant_id,space_id,instruction,source_page_id,require_source,version) VALUES ($1,$2,$3,$4,$5,$6)
             ON CONFLICT (tenant_id,space_id) DO UPDATE SET instruction=EXCLUDED.instruction,
             source_page_id=EXCLUDED.source_page_id,require_source=EXCLUDED.require_source,version=EXCLUDED.version")
            .bind(scope.tenant).bind(scope.space).bind(instruction).bind(source).bind(require_source)
            .bind(current+1).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }

    pub(crate) async fn approach_runs(
        &self,
        scope: TaskScope,
        task: Uuid,
    ) -> Result<Vec<ApproachRun>, sqlx::Error> {
        sqlx::query_as(
            "SELECT r.id,
                    CASE WHEN r.status='running' AND r.started_at <= $5 THEN 'expired'
                         WHEN r.status IN ('running','review') AND r.task_version <> t.version THEN 'superseded'
                         ELSE r.status END AS status,
                    CASE WHEN (r.actor_id=$3 OR access.can_manage)
                          AND (r.status <> 'review' OR r.task_version=t.version) THEN r.output END AS output,
                    CASE WHEN EXISTS (SELECT 1 FROM page_permissions($1,$2,$3,r.result_page_id) a WHERE a.can_read)
                         THEN r.result_page_id END AS result_page_id,
                    r.source_page_id,r.source_revision,r.spec_version
             FROM workspace_approach_runs r
             JOIN workspace_tasks t ON (t.tenant_id,t.space_id,t.id)=(r.tenant_id,r.space_id,r.task_id)
             CROSS JOIN LATERAL page_capabilities($1,$2,$3,t.source_page_id) access
             CROSS JOIN LATERAL page_permissions($1,$2,$3,r.source_page_id) source
             WHERE (r.tenant_id,r.space_id,r.task_id)=($1,$2,$4) AND source.can_read
               AND access.can_browse_space AND (t.source_page_id IS NULL OR access.can_read)
               AND (r.actor_id=$3 OR access.can_manage OR r.status='accepted') ORDER BY r.started_at DESC,r.id LIMIT 15",
        ).bind(scope.tenant).bind(scope.space).bind(scope.account).bind(task)
            .bind(now_ms() - RUN_LEASE_MS).fetch_all(&self.0).await
    }

    pub(crate) async fn start_approach(
        &self,
        scope: TaskScope,
        task: Uuid,
        id: Uuid,
        expected: ApproachVersions,
    ) -> Result<StartApproach, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        if !owner(&mut tx, scope).await? {
            return Ok(StartApproach::Unavailable);
        }
        let task_data: Option<ApproachTask> = sqlx::query_as(
            "SELECT t.title,t.status,t.version,t.source_page_id FROM workspace_tasks t
             LEFT JOIN LATERAL page_permissions($1,$2,$3,t.source_page_id) task_source ON t.source_page_id IS NOT NULL
             WHERE (t.tenant_id,t.space_id,t.id)=($1,$2,$4)
               AND (t.source_page_id IS NULL OR task_source.can_read) FOR UPDATE OF t",
        ).bind(scope.tenant).bind(scope.space).bind(scope.account).bind(task).fetch_optional(&mut *tx).await?;
        let Some(task_data) = task_data else {
            return Ok(StartApproach::Unavailable);
        };
        let already_started: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM workspace_approach_runs
             WHERE (tenant_id,space_id,actor_id,task_id,id)=($1,$2,$3,$4,$5))",
        )
        .bind(scope.tenant)
        .bind(scope.space)
        .bind(scope.account)
        .bind(task)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if already_started {
            return Ok(StartApproach::AlreadyStarted);
        }
        let Some(config) = read_approach(&mut tx, scope).await? else {
            return Ok(StartApproach::Unavailable);
        };
        if task_data.status == "done" {
            return Ok(StartApproach::TaskDone);
        }
        if !config.permits_task_source(task_data.source_page_id) {
            return Ok(StartApproach::SourceRequired);
        }
        if config.version != expected.spec
            || config.source_revision != expected.source
            || task_data.version != expected.task
        {
            return Ok(StartApproach::PreviewChanged);
        }

        let now = now_ms();
        sqlx::query("UPDATE workspace_approach_runs SET status='failed',output=NULL,finished_at=coalesce(finished_at,$4)
                     WHERE (tenant_id,space_id,task_id)=($1,$2,$3) AND status IN ('running','review')
                       AND (task_version <> $5 OR (status='running' AND started_at <= $6))")
            .bind(scope.tenant).bind(scope.space).bind(task).bind(now).bind(task_data.version)
            .bind(now - RUN_LEASE_MS).execute(&mut *tx).await?;
        let saved=sqlx::query("INSERT INTO workspace_approach_runs
            (tenant_id,space_id,id,task_id,actor_id,source_page_id,source_revision,spec_version,task_version,status,started_at)
            VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'running',$10) ON CONFLICT DO NOTHING")
            .bind(scope.tenant).bind(scope.space).bind(id).bind(task).bind(scope.account)
            .bind(config.source_page_id).bind(config.source_revision).bind(config.version).bind(task_data.version)
            .bind(now).execute(&mut *tx).await?.rows_affected();
        tx.commit().await?;
        Ok(if saved == 1 {
            StartApproach::Started(RunInput {
                instruction: config.instruction,
                source: config.source_excerpt,
                task: task_data.title,
            })
        } else {
            StartApproach::ActiveRun
        })
    }

    pub(crate) async fn finish_approach(
        &self,
        scope: TaskScope,
        run: Uuid,
        output: Option<String>,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.0.begin().await?;
        let may_finish = owner(&mut tx, scope).await?;
        let now = now_ms();
        let current_task: Option<(Uuid, i64)> = if output.is_some() && may_finish {
            // Use the same task-before-run lock order as launch and review.
            sqlx::query_as(
                "SELECT t.id,t.version FROM workspace_approach_runs r
                 JOIN workspace_tasks t ON (t.tenant_id,t.space_id,t.id)=(r.tenant_id,r.space_id,r.task_id)
                 CROSS JOIN LATERAL page_permissions($1,$2,$4,r.source_page_id) source
                 LEFT JOIN LATERAL page_permissions($1,$2,$4,t.source_page_id) task_source ON t.source_page_id IS NOT NULL
                 WHERE (r.tenant_id,r.space_id,r.id,r.actor_id)=($1,$2,$3,$4)
                   AND r.status='running' AND r.started_at > $5 AND source.can_read
                   AND t.status<>'done' AND t.version=r.task_version
                   AND (t.source_page_id IS NULL OR task_source.can_read) FOR UPDATE OF t",
            )
            .bind(scope.tenant)
            .bind(scope.space)
            .bind(run)
            .bind(scope.account)
            .bind(now - RUN_LEASE_MS)
            .fetch_optional(&mut *tx)
            .await?
        } else {
            None
        };
        let (status, output) = if current_task.is_some() {
            ("review", output)
        } else {
            ("failed", None)
        };
        let changed = sqlx::query("UPDATE workspace_approach_runs SET status=$5,output=$6,finished_at=$7,task_version=coalesce($8::bigint+1,task_version)
            WHERE (tenant_id,space_id,id,actor_id)=($1,$2,$3,$4) AND status='running'")
            .bind(scope.tenant).bind(scope.space).bind(run).bind(scope.account).bind(status).bind(output).bind(now)
            .bind(current_task.map(|(_, version)| version))
            .execute(&mut *tx).await?.rows_affected();
        if let Some((task, _)) = current_task.filter(|_| changed == 1) {
            sqlx::query(
                "UPDATE workspace_tasks SET status='review',updated_at=greatest(updated_at,$4),version=version+1
                 WHERE (tenant_id,space_id,id)=($1,$2,$3)",
            ).bind(scope.tenant).bind(scope.space).bind(task).bind(now).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn review_approach(
        &self,
        scope: TaskScope,
        task: Uuid,
        run: Uuid,
        accept: bool,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        if !owner(&mut tx, scope).await? {
            return Ok(false);
        }
        let version: Option<i64> = sqlx::query_scalar(
            "SELECT version FROM workspace_tasks
             WHERE (tenant_id,space_id,id)=($1,$2,$3) AND status='review' FOR UPDATE",
        )
        .bind(scope.tenant)
        .bind(scope.space)
        .bind(task)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(version) = version else {
            return Ok(false);
        };
        let data: Option<(String,Uuid,Uuid,i64,String)> = sqlx::query_as(
            "SELECT r.output,r.source_page_id,p.section_id,r.source_revision,t.title
             FROM workspace_approach_runs r JOIN pages p ON (p.tenant_id,p.space_id,p.id)=(r.tenant_id,r.space_id,r.source_page_id)
             JOIN workspace_tasks t ON (t.tenant_id,t.space_id,t.id)=(r.tenant_id,r.space_id,r.task_id)
             CROSS JOIN LATERAL page_permissions($1,$2,$3,p.id) source
             LEFT JOIN LATERAL page_permissions($1,$2,$3,t.source_page_id) task_source ON t.source_page_id IS NOT NULL
             WHERE (r.tenant_id,r.space_id,r.task_id,r.id)=($1,$2,$4,$5) AND r.status='review'
               AND r.task_version=$6 AND source.can_read AND (t.source_page_id IS NULL OR task_source.can_read)
             FOR UPDATE OF r",
        ).bind(scope.tenant).bind(scope.space).bind(scope.account).bind(task).bind(run).bind(version).fetch_optional(&mut *tx).await?;
        let Some((output, source, section, revision, task_title)) = data else {
            return Ok(false);
        };
        let result_page = if accept {
            sqlx::query(
                "SELECT 1 FROM sections WHERE (tenant_id,space_id,id)=($1,$2,$3) FOR NO KEY UPDATE",
            )
            .bind(scope.tenant)
            .bind(scope.space)
            .bind(section)
            .execute(&mut *tx)
            .await?;
            let title = format!(
                "Спецификация · {} · {}",
                task_title.chars().take(55).collect::<String>(),
                &run.simple().to_string()[..12]
            );
            let address = page_path(scope.tenant, scope.space, source);
            let markdown =
                format!("Источник редакции {revision}: [Документ]({address})\n\n{output}");
            sqlx::query("INSERT INTO pages(tenant_id,space_id,section_id,id,creator_account_id,title,title_key,position,markdown,content_is_markdown)
                VALUES ($1,$2,$3,$4,$5,$6,$7,(SELECT COALESCE(MAX(position),0)+1 FROM pages WHERE (tenant_id,space_id,section_id)=($1,$2,$3) AND parent_id IS NULL),$8,true)")
                .bind(scope.tenant).bind(scope.space).bind(section).bind(run).bind(scope.account)
                .bind(title.as_bytes()).bind(fold_name(&title).into_bytes()).bind(markdown)
                .execute(&mut *tx).await?;
            record_current_snapshot(
                &mut tx,
                &PageAddress {
                    tenant: scope.tenant,
                    space: scope.space,
                    page: run,
                    account: scope.account,
                },
                Some(scope.account),
                None,
            )
            .await?;
            Some(run)
        } else {
            None
        };
        sqlx::query("UPDATE workspace_approach_runs SET status=$5,result_page_id=$6 WHERE (tenant_id,space_id,task_id,id)=($1,$2,$3,$4)")
            .bind(scope.tenant).bind(scope.space).bind(task).bind(run)
            .bind(if accept {"accepted"} else {"returned"}).bind(result_page).execute(&mut *tx).await?;
        sqlx::query("UPDATE workspace_tasks SET status=$4,updated_at=greatest(updated_at,$5),version=version+1 WHERE (tenant_id,space_id,id)=($1,$2,$3)")
            .bind(scope.tenant).bind(scope.space).bind(task).bind(if accept {"done"} else {"work"}).bind(now_ms())
            .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }
}
