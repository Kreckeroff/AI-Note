use sqlx::FromRow;
use uuid::Uuid;

use super::{
    Store,
    sections::decode_title,
    tasks::{Task, TaskArea, TaskScope, area, visible_tasks},
};

#[cfg(test)]
mod tests;

#[derive(FromRow)]
struct PageRow {
    id: Uuid,
    parent_id: Option<Uuid>,
    revision: i64,
    title: Vec<u8>,
    at: Option<i64>,
    can_edit: bool,
}

pub(crate) struct GraphPage {
    pub id: Uuid,
    pub parent_id: Option<Uuid>,
    pub revision: i64,
    pub title: String,
    pub at: Option<i64>,
}

#[derive(FromRow)]
struct RevisionRow {
    page: Uuid,
    revision: i64,
    title: Vec<u8>,
    at: i64,
}

pub(crate) struct GraphRevision {
    pub page: Uuid,
    pub revision: i64,
    pub title: String,
    pub at: i64,
}

#[derive(FromRow)]
pub(crate) struct GraphResult {
    pub task: Uuid,
    pub page: Uuid,
    pub at: i64,
}

pub(crate) struct GraphSpace {
    pub area: TaskArea,
    pub pages: Vec<GraphPage>,
    pub history: Vec<GraphRevision>,
    pub tasks: Vec<Task>,
    pub results: Vec<GraphResult>,
}

impl Store {
    pub(crate) async fn graph_space(
        &self,
        scope: TaskScope,
    ) -> Result<Option<GraphSpace>, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        let Some(area) = area(&mut tx, scope).await? else {
            return Ok(None);
        };
        let tasks = visible_tasks(&mut tx, scope, Default::default()).await?;
        let rows: Vec<PageRow> = sqlx::query_as(
            "SELECT p.id, p.parent_id, p.content_revision AS revision, p.title,
                (extract(epoch FROM current.recorded_at) * 1000)::bigint AS at, permitted.can_edit
             FROM pages p
             CROSS JOIN LATERAL page_capabilities($1,$2,$3,p.id) permitted
             LEFT JOIN page_revisions current ON (current.tenant_id,current.space_id,current.page_id,current.revision)
                 = (p.tenant_id,p.space_id,p.id,p.content_revision)
             WHERE (p.tenant_id,p.space_id)=($1,$2) AND permitted.can_read
             ORDER BY p.position,p.id",
        ).bind(scope.tenant).bind(scope.space).bind(scope.account).fetch_all(&mut *tx).await?;
        let editable: Vec<_> = rows
            .iter()
            .filter(|row| row.can_edit)
            .map(|row| row.id)
            .collect();
        let pages = rows
            .into_iter()
            .map(|row| {
                Ok(GraphPage {
                    id: row.id,
                    parent_id: row.parent_id,
                    revision: row.revision,
                    title: decode_title(row.title, "pages.title")?,
                    at: row.at,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;

        let history: Vec<RevisionRow> = sqlx::query_as(
            "SELECT page_id AS page, revision, title,
                (extract(epoch FROM recorded_at) * 1000)::bigint AS at
             FROM page_revisions
             WHERE (tenant_id,space_id)=($1,$2) AND page_id=ANY($3) AND recorded_at IS NOT NULL
             ORDER BY page_id,revision",
        )
        .bind(scope.tenant)
        .bind(scope.space)
        .bind(&editable)
        .fetch_all(&mut *tx)
        .await?;
        let history = history
            .into_iter()
            .map(|row| {
                Ok(GraphRevision {
                    page: row.page,
                    revision: row.revision,
                    title: decode_title(row.title, "page_revisions.title")?,
                    at: row.at,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;

        // Acceptance creates a dated first revision in the same transaction as the run result.
        // Both endpoints are restricted to objects visible in this snapshot.
        let task_ids: Vec<_> = tasks.iter().map(|task| task.id).collect();
        let page_ids: Vec<_> = pages.iter().map(|page| page.id).collect();
        let results: Vec<GraphResult> = sqlx::query_as(
            "SELECT run.task_id AS task, run.result_page_id AS page,
                (extract(epoch FROM first.recorded_at) * 1000)::bigint AS at
             FROM workspace_approach_runs run
             JOIN page_revisions first ON (first.tenant_id,first.space_id,first.page_id,first.revision)
                 = (run.tenant_id,run.space_id,run.result_page_id,1)
             WHERE (run.tenant_id,run.space_id)=($1,$2) AND run.status='accepted'
               AND run.task_id=ANY($3) AND run.result_page_id=ANY($4) AND first.recorded_at IS NOT NULL
             ORDER BY first.recorded_at,run.id",
        ).bind(scope.tenant).bind(scope.space).bind(&task_ids).bind(&page_ids).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        Ok(Some(GraphSpace {
            area,
            pages,
            history,
            tasks,
            results,
        }))
    }
}
