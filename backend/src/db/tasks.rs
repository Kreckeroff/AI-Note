use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use super::{Store, sections::decode_title};
use crate::{clock::now_ms, text::js_trim};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TaskStatus {
    Open,
    Work,
    Review,
    Done,
}

impl TaskStatus {
    pub(crate) const ALL: [Self; 4] = [Self::Open, Self::Work, Self::Review, Self::Done];

    pub(crate) fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|status| status.as_str() == value)
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Work => "work",
            Self::Review => "review",
            Self::Done => "done",
        }
    }

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Open => "Запланировано",
            Self::Work => "В работе",
            Self::Review => "На согласовании",
            Self::Done => "Готово",
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct TaskScope {
    pub tenant: Uuid,
    pub space: Uuid,
    pub account: Uuid,
}

pub(crate) struct Task {
    pub id: Uuid,
    pub number: i64,
    pub title: String,
    pub status: TaskStatus,
    pub source_page: Option<Uuid>,
    pub source_title: Option<String>,
    pub assignee: Option<Uuid>,
    pub assignee_name: Option<String>,
    pub created_at: i64,
    pub version: i64,
}

#[derive(FromRow)]
struct TaskRow {
    id: Uuid,
    number: i64,
    title: String,
    status: String,
    source_page_id: Option<Uuid>,
    source_title: Option<Vec<u8>>,
    assignee_account_id: Option<Uuid>,
    assignee_name: Option<String>,
    created_at: i64,
    version: i64,
}

impl TryFrom<TaskRow> for Task {
    type Error = sqlx::Error;

    fn try_from(row: TaskRow) -> Result<Self, Self::Error> {
        let status = TaskStatus::parse(&row.status).ok_or_else(|| {
            sqlx::Error::Protocol("Unrecognized task status in PostgreSQL".to_owned())
        })?;
        Ok(Self {
            id: row.id,
            number: row.number,
            title: row.title,
            status,
            source_page: row.source_page_id,
            source_title: row
                .source_title
                .map(|title| decode_title(title, "pages.title"))
                .transpose()?,
            assignee: row.assignee_account_id,
            assignee_name: row.assignee_name,
            created_at: row.created_at,
            version: row.version,
        })
    }
}

#[derive(FromRow)]
pub(crate) struct TaskMember {
    pub id: Uuid,
    pub name: String,
}

pub(crate) struct TaskSource {
    pub id: Uuid,
    pub title: String,
}

pub(crate) struct TaskArea {
    pub tenant_name: String,
    pub space_name: String,
    pub can_manage: bool,
}

#[derive(Clone, Copy, Default)]
pub(crate) struct TaskFilter<'a> {
    pub assignee: Option<Uuid>,
    pub stage: Option<TaskStatus>,
    pub search: &'a str,
    pub recent_first: bool,
}

impl TaskFilter<'_> {
    pub(crate) fn is_filtered(self) -> bool {
        self.assignee.is_some() || self.stage.is_some() || !self.search.is_empty()
    }
}

pub(crate) struct TaskBoard {
    pub tasks: Vec<Task>,
    pub members: Vec<TaskMember>,
    pub area: TaskArea,
}

pub(crate) struct TaskDetail {
    pub task: Task,
    pub members: Vec<TaskMember>,
    pub can_manage: bool,
    pub area: TaskArea,
}

pub(crate) struct TaskCreation {
    pub sources: Vec<TaskSource>,
    pub area: TaskArea,
}

pub(crate) struct CreateTask<'a> {
    pub operation: Uuid,
    pub title: &'a str,
    pub status: TaskStatus,
    pub source: Option<Uuid>,
}

#[derive(Clone, Copy)]
pub(crate) enum AssigneeChange {
    Keep,
    Set(Option<Uuid>),
}

#[derive(Clone, Copy)]
pub(crate) struct TaskChange {
    pub version: i64,
    pub status: TaskStatus,
    pub assignee: AssigneeChange,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TaskWrite {
    Saved(Uuid),
    Missing,
    Forbidden,
    Conflict,
    InvalidTitle,
    InvalidSource,
    InvalidAssignee,
}

// The page capability resolver is also the source of truth for space access.
// Its role from a particular page is relevant only for source-bound tasks.
pub(super) async fn area(
    connection: &mut PgConnection,
    scope: TaskScope,
) -> Result<Option<TaskArea>, sqlx::Error> {
    let row: Option<(String, Vec<u8>, bool)> = sqlx::query_as(
        "SELECT tn.name, sp.name, sp.owner_account_id = $3
         FROM spaces sp JOIN tenants tn ON tn.id = sp.tenant_id
         CROSS JOIN LATERAL page_capabilities($1, $2, $3, NULL::uuid) caps
         WHERE (sp.tenant_id,sp.id) = ($1,$2) AND caps.can_browse_space",
    )
    .bind(scope.tenant)
    .bind(scope.space)
    .bind(scope.account)
    .fetch_optional(connection)
    .await?;
    row.map(|(tenant_name, space_name, can_manage)| {
        Ok(TaskArea {
            tenant_name,
            space_name: decode_title(space_name, "spaces.name")?,
            can_manage,
        })
    })
    .transpose()
}

const TASK_ROWS: &str =
    "SELECT t.id, t.number, t.title, t.status, t.source_page_id, p.title AS source_title,
            t.assignee_account_id, a.name AS assignee_name, t.created_at, t.version
     FROM workspace_tasks t
     LEFT JOIN pages p ON (p.tenant_id,p.space_id,p.id) = (t.tenant_id,t.space_id,t.source_page_id)
     LEFT JOIN accounts a ON a.id = t.assignee_account_id AND EXISTS (
         SELECT 1 FROM page_capabilities(t.tenant_id,t.space_id,a.id,t.source_page_id) caps
         WHERE caps.can_browse_space AND (t.source_page_id IS NULL OR caps.can_read))
     WHERE (t.tenant_id,t.space_id) = ($1,$2)
       AND (t.source_page_id IS NULL OR EXISTS (
           SELECT 1 FROM page_permissions($1,$2,$3,t.source_page_id) caps WHERE caps.can_read))";

pub(super) async fn visible_tasks(
    connection: &mut PgConnection,
    scope: TaskScope,
    filter: TaskFilter<'_>,
) -> Result<Vec<Task>, sqlx::Error> {
    let order = if filter.recent_first {
        "t.updated_at DESC, t.number"
    } else {
        "t.number"
    };
    let query = format!(
        "{TASK_ROWS} AND ($4::uuid IS NULL OR a.id=$4)
         AND ($5::text IS NULL OR t.status=$5)
         AND ($6::text='' OR strpos(lower(normalize(t.title)),lower(normalize($6)))>0
              OR strpos('kb-' || lpad(t.number::text,greatest(3,length(t.number::text)),'0'),lower($6))>0)
         ORDER BY {order}"
    );
    sqlx::query_as::<_, TaskRow>(&query)
        .bind(scope.tenant)
        .bind(scope.space)
        .bind(scope.account)
        .bind(filter.assignee)
        .bind(filter.stage.map(TaskStatus::as_str))
        .bind(filter.search)
        .fetch_all(connection)
        .await?
        .into_iter()
        .map(Task::try_from)
        .collect()
}

async fn members(
    connection: &mut PgConnection,
    scope: TaskScope,
    source: Option<Uuid>,
) -> Result<Vec<TaskMember>, sqlx::Error> {
    sqlx::query_as(
        "SELECT a.id,a.name FROM tenant_memberships m JOIN accounts a ON a.id = m.account_id
         CROSS JOIN LATERAL page_capabilities($1,$2,m.account_id,$3) caps
         WHERE m.tenant_id = $1 AND m.status = 'active'
           AND caps.can_browse_space AND ($3::uuid IS NULL OR caps.can_read)
         ORDER BY a.name COLLATE \"C\", a.id",
    )
    .bind(scope.tenant)
    .bind(scope.space)
    .bind(source)
    .fetch_all(connection)
    .await
}

pub(super) async fn lock_task_area(
    connection: &mut PgConnection,
    scope: TaskScope,
    assignee: Option<Uuid>,
) -> Result<Option<TaskArea>, sqlx::Error> {
    sqlx::query("SET LOCAL statement_timeout = '3s'")
        .execute(&mut *connection)
        .await?;
    sqlx::query("SELECT id FROM tenants WHERE id=$1 FOR SHARE")
        .bind(scope.tenant)
        .fetch_optional(&mut *connection)
        .await?;
    let accounts = [scope.account, assignee.unwrap_or(scope.account)];
    sqlx::query("SELECT 1 FROM tenant_memberships WHERE tenant_id=$1 AND account_id=ANY($2) ORDER BY account_id FOR UPDATE")
        .bind(scope.tenant).bind(&accounts[..]).fetch_all(&mut *connection).await?;
    sqlx::query("SELECT id FROM spaces WHERE (tenant_id,id)=($1,$2) FOR NO KEY UPDATE")
        .bind(scope.tenant)
        .bind(scope.space)
        .fetch_optional(&mut *connection)
        .await?;
    area(connection, scope).await
}

async fn may_read_source(
    connection: &mut PgConnection,
    scope: TaskScope,
    source: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM page_permissions($1,$2,$3,$4) caps WHERE caps.can_read)",
    )
    .bind(scope.tenant)
    .bind(scope.space)
    .bind(scope.account)
    .bind(source)
    .fetch_one(connection)
    .await
}

impl Store {
    pub(crate) async fn tasks(
        &self,
        tenant: Uuid,
        space: Uuid,
        account: Uuid,
        filter: TaskFilter<'_>,
    ) -> Result<Option<TaskBoard>, sqlx::Error> {
        let scope = TaskScope {
            tenant,
            space,
            account,
        };
        let mut tx = self.0.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        let Some(area) = area(&mut tx, scope).await? else {
            return Ok(None);
        };
        let tasks = visible_tasks(&mut tx, scope, filter).await?;
        let members = members(&mut tx, scope, None).await?;
        tx.commit().await?;
        Ok(Some(TaskBoard {
            tasks,
            members,
            area,
        }))
    }

    pub(crate) async fn task(
        &self,
        tenant: Uuid,
        space: Uuid,
        account: Uuid,
        task: Uuid,
    ) -> Result<Option<TaskDetail>, sqlx::Error> {
        let scope = TaskScope {
            tenant,
            space,
            account,
        };
        let mut tx = self.0.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        let Some(area) = area(&mut tx, scope).await? else {
            return Ok(None);
        };
        let query = format!("{TASK_ROWS} AND t.id=$4");
        let Some(task) = sqlx::query_as::<_, TaskRow>(&query)
            .bind(tenant)
            .bind(space)
            .bind(account)
            .bind(task)
            .fetch_optional(&mut *tx)
            .await?
            .map(Task::try_from)
            .transpose()?
        else {
            return Ok(None);
        };
        let members = if area.can_manage {
            members(&mut tx, scope, task.source_page).await?
        } else {
            Vec::new()
        };
        tx.commit().await?;
        Ok(Some(TaskDetail {
            can_manage: area.can_manage,
            task,
            members,
            area,
        }))
    }

    pub(crate) async fn task_creation(
        &self,
        scope: TaskScope,
    ) -> Result<Option<TaskCreation>, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        let Some(area) = area(&mut tx, scope).await? else {
            return Ok(None);
        };
        let sources = if area.can_manage {
            sqlx::query_as::<_, (Uuid, Vec<u8>)>(
                "SELECT p.id,p.title FROM pages p
                 CROSS JOIN LATERAL page_permissions($1,$2,$3,p.id) caps
                 WHERE (p.tenant_id,p.space_id)=($1,$2) AND caps.can_read
                 ORDER BY p.title,p.id",
            )
            .bind(scope.tenant)
            .bind(scope.space)
            .bind(scope.account)
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(|(id, title)| {
                Ok(TaskSource {
                    id,
                    title: decode_title(title, "pages.title")?,
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?
        } else {
            Vec::new()
        };
        tx.commit().await?;
        Ok(Some(TaskCreation { sources, area }))
    }

    pub(crate) async fn create_task(
        &self,
        scope: TaskScope,
        input: &CreateTask<'_>,
    ) -> Result<TaskWrite, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        match lock_task_area(&mut tx, scope, None).await? {
            None => return Ok(TaskWrite::Missing),
            Some(area) if !area.can_manage => return Ok(TaskWrite::Forbidden),
            Some(_) => {}
        }
        let title = js_trim(input.title);
        if title.is_empty() || title.chars().count() > 100 || title.chars().any(char::is_control) {
            return Ok(TaskWrite::InvalidTitle);
        }
        if let Some(source) = input.source
            && !may_read_source(&mut tx, scope, source).await?
        {
            return Ok(TaskWrite::InvalidSource);
        }
        let mut fingerprint = Sha256::new();
        fingerprint.update(input.status.as_str());
        fingerprint.update([u8::from(input.source.is_some())]);
        fingerprint.update(input.source.unwrap_or_default().as_bytes());
        fingerprint.update(title.as_bytes());
        let fingerprint = format!("{:x}", fingerprint.finalize());
        let previous: Option<(Uuid, String)> = sqlx::query_as(
            "SELECT id,request_fingerprint FROM workspace_tasks
             WHERE (tenant_id,space_id,creator_account_id,operation_id)=($1,$2,$3,$4)",
        )
        .bind(scope.tenant)
        .bind(scope.space)
        .bind(scope.account)
        .bind(input.operation)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((id, saved)) = previous {
            return Ok(if saved == fingerprint {
                TaskWrite::Saved(id)
            } else {
                TaskWrite::Conflict
            });
        }
        let id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO workspace_tasks
             (tenant_id,space_id,id,number,creator_account_id,operation_id,request_fingerprint,title,status,
              source_page_id,assignee_account_id,created_at,updated_at)
             SELECT $1,$2,$3,coalesce(max(number),0)+1,$4,$5,$6,$7,$8,$9,$4,$10,$10
             FROM workspace_tasks WHERE (tenant_id,space_id)=($1,$2)")
            .bind(scope.tenant).bind(scope.space).bind(id).bind(scope.account).bind(input.operation).bind(&fingerprint)
            .bind(title).bind(input.status.as_str()).bind(input.source).bind(now_ms())
            .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(TaskWrite::Saved(id))
    }

    pub(crate) async fn change_task(
        &self,
        scope: TaskScope,
        task: Uuid,
        input: TaskChange,
    ) -> Result<TaskWrite, sqlx::Error> {
        let assigned = match input.assignee {
            AssigneeChange::Keep => None,
            AssigneeChange::Set(id) => id,
        };
        let mut tx = self.0.begin().await?;
        let Some(area) = lock_task_area(&mut tx, scope, assigned).await? else {
            return Ok(TaskWrite::Missing);
        };
        let previous: Option<(String, Option<Uuid>, Option<Uuid>, i64)> = sqlx::query_as(
            "SELECT status, assignee_account_id, source_page_id, version FROM workspace_tasks
             WHERE (tenant_id,space_id,id)=($1,$2,$3) FOR UPDATE",
        )
        .bind(scope.tenant)
        .bind(scope.space)
        .bind(task)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((status, assignee, source, version)) = previous else {
            return Ok(TaskWrite::Missing);
        };
        if let Some(source) = source
            && !may_read_source(&mut tx, scope, source).await?
        {
            return Ok(TaskWrite::Missing);
        }
        if !area.can_manage {
            return Ok(TaskWrite::Forbidden);
        }
        let desired_assignee = match input.assignee {
            AssigneeChange::Keep => assignee,
            AssigneeChange::Set(id) => id,
        };
        if let AssigneeChange::Set(Some(id)) = input.assignee {
            let visible: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM page_capabilities($1,$2,$3,$4) caps
                 WHERE caps.can_browse_space AND ($4::uuid IS NULL OR caps.can_read))",
            )
            .bind(scope.tenant)
            .bind(scope.space)
            .bind(id)
            .bind(source)
            .fetch_one(&mut *tx)
            .await?;
            if !visible {
                return Ok(TaskWrite::InvalidAssignee);
            }
        }
        if status == input.status.as_str() && assignee == desired_assignee {
            return Ok(TaskWrite::Saved(task));
        }
        if input.version != version {
            return Ok(TaskWrite::Conflict);
        }
        sqlx::query(
            "UPDATE workspace_tasks SET status=$4,assignee_account_id=$5,version=version+1,updated_at=greatest(updated_at,$6)
             WHERE (tenant_id,space_id,id)=($1,$2,$3)")
            .bind(scope.tenant).bind(scope.space).bind(task).bind(input.status.as_str()).bind(desired_assignee).bind(now_ms())
            .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(TaskWrite::Saved(task))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    #[test]
    fn only_prototype_statuses_can_be_persisted() {
        assert_eq!(
            TaskStatus::ALL.map(TaskStatus::label),
            ["Запланировано", "В работе", "На согласовании", "Готово",]
        );
        for status in TaskStatus::ALL {
            assert_eq!(TaskStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(TaskStatus::parse("closed"), None);
    }

    #[sqlx::test(migrator = "crate::db::MIGRATIONS")]
    #[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
    async fn owner_can_create_repeat_assign_and_change_status_without_cross_tenant_leaks(
        pool: PgPool,
    ) -> anyhow::Result<()> {
        let [tenant, other, space, owner, member, foreign, section, page] =
            std::array::from_fn(|_| Uuid::new_v4());
        sqlx::query("INSERT INTO tenants VALUES ($1,'Команда'), ($2,'Другая')")
            .bind(tenant)
            .bind(other)
            .execute(&pool)
            .await?;
        for (account, tenant, email) in [
            (owner, tenant, "owner@example.test"),
            (member, tenant, "member@example.test"),
            (foreign, other, "foreign@example.test"),
        ] {
            sqlx::query(
                "INSERT INTO accounts (id,email,name,salt,password_hash) VALUES ($1,$2,$2,$3,$4)",
            )
            .bind(account)
            .bind(email)
            .bind(vec![0_u8; 16])
            .bind(vec![0_u8; 32])
            .execute(&pool)
            .await?;
            sqlx::query(
                "INSERT INTO tenant_memberships VALUES ($1,$2,'member','active',false,false)",
            )
            .bind(tenant)
            .bind(account)
            .execute(&pool)
            .await?;
        }
        sqlx::query("INSERT INTO spaces (tenant_id,id,creator_account_id,owner_account_id,name,name_key) VALUES ($1,$2,$3,$3,'Задачи','tasks')")
            .bind(tenant).bind(space).bind(owner).execute(&pool).await?;
        sqlx::query("INSERT INTO space_grants VALUES ($1,$2,$3,'viewer')")
            .bind(tenant)
            .bind(space)
            .bind(member)
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO sections (tenant_id,space_id,id,creator_account_id,title,title_key,position) VALUES ($1,$2,$3,$4,'Раздел','sections',1)")
            .bind(tenant).bind(space).bind(section).bind(owner).execute(&pool).await?;
        sqlx::query("INSERT INTO pages (tenant_id,space_id,section_id,id,creator_account_id,title,title_key,position) VALUES ($1,$2,$3,$4,$5,'Источник','source',1)")
            .bind(tenant).bind(space).bind(section).bind(page).bind(owner).execute(&pool).await?;
        let store = Store::from_test_pool(pool.clone());
        let operation = Uuid::new_v4();
        let mut input = CreateTask {
            operation,
            title: "  Проверить доступ  ",
            status: TaskStatus::Open,
            source: Some(page),
        };
        let scope = TaskScope {
            tenant,
            space,
            account: owner,
        };
        let TaskWrite::Saved(id) = store.create_task(scope, &input).await? else {
            panic!("owner must be allowed to create a task");
        };
        assert_eq!(
            store.create_task(scope, &input).await?,
            TaskWrite::Saved(id)
        );
        input.title = "Другой заголовок";
        assert_eq!(store.create_task(scope, &input).await?, TaskWrite::Conflict);
        assert_eq!(
            store
                .create_task(
                    TaskScope {
                        account: member,
                        ..scope
                    },
                    &input
                )
                .await?,
            TaskWrite::Forbidden
        );
        assert!(
            store
                .tasks(tenant, space, foreign, TaskFilter::default())
                .await?
                .is_none()
        );
        assert!(store.task(other, space, foreign, id).await?.is_none());
        let read_task = store.task(tenant, space, member, id).await?.unwrap();
        assert_eq!(read_task.task.source_title.as_deref(), Some("Источник"));
        assert_eq!(read_task.task.title, "Проверить доступ");
        assert_eq!(read_task.task.number, 1);
        assert!(!read_task.can_manage);
        let assign = TaskChange {
            version: 1,
            status: TaskStatus::Work,
            assignee: AssigneeChange::Set(Some(member)),
        };
        assert_eq!(
            store.change_task(scope, id, assign).await?,
            TaskWrite::Saved(id)
        );
        assert_eq!(
            store.change_task(scope, id, assign).await?,
            TaskWrite::Saved(id)
        );
        assert_eq!(
            store
                .change_task(
                    scope,
                    id,
                    TaskChange {
                        status: TaskStatus::Review,
                        ..assign
                    }
                )
                .await?,
            TaskWrite::Conflict
        );
        let filtered = store
            .tasks(
                tenant,
                space,
                owner,
                TaskFilter {
                    assignee: Some(member),
                    ..Default::default()
                },
            )
            .await?
            .unwrap();
        assert_eq!(filtered.tasks.len(), 1);
        assert_eq!(filtered.tasks[0].status, TaskStatus::Work);
        assert_eq!(filtered.tasks[0].assignee, Some(member));
        let mut selection = TaskFilter {
            assignee: Some(member),
            stage: Some(TaskStatus::Work),
            search: "KB-001",
            recent_first: true,
        };
        for (query, expected) in [
            ("дОстУп", 1),
            ("KB-001", 1),
            ("%", 0),
            ("_", 0),
            ("KB-999", 0),
        ] {
            selection.search = query;
            let results = store.tasks(tenant, space, owner, selection).await?.unwrap();
            assert_eq!(results.tasks.len(), expected);
        }
        selection.search = "KB-001";
        selection.stage = Some(TaskStatus::Done);
        assert!(
            store
                .tasks(tenant, space, owner, selection)
                .await?
                .unwrap()
                .tasks
                .is_empty()
        );
        let invalid = TaskChange {
            version: 2,
            assignee: AssigneeChange::Set(Some(foreign)),
            ..assign
        };
        assert_eq!(
            store.change_task(scope, id, invalid).await?,
            TaskWrite::InvalidAssignee
        );
        sqlx::query(
            "DELETE FROM space_grants WHERE tenant_id=$1 AND space_id=$2 AND account_id=$3",
        )
        .bind(tenant)
        .bind(space)
        .bind(member)
        .execute(&pool)
        .await?;
        assert!(store.task(tenant, space, member, id).await?.is_none());
        assert!(
            store
                .tasks(
                    tenant,
                    space,
                    member,
                    TaskFilter {
                        assignee: Some(member),
                        ..Default::default()
                    }
                )
                .await?
                .is_none()
        );
        let task = store.task(tenant, space, owner, id).await?.unwrap();
        assert_eq!(task.task.assignee, Some(member));
        assert_eq!(task.task.assignee_name, None);
        assert_eq!(
            store
                .change_task(
                    scope,
                    id,
                    TaskChange {
                        version: 2,
                        ..assign
                    }
                )
                .await?,
            TaskWrite::InvalidAssignee
        );
        assert_eq!(
            store
                .change_task(
                    scope,
                    id,
                    TaskChange {
                        version: 2,
                        status: TaskStatus::Done,
                        assignee: AssigneeChange::Keep
                    }
                )
                .await?,
            TaskWrite::Saved(id)
        );
        Ok(())
    }
}
