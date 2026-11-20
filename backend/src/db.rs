pub(crate) mod coedit_presence;
pub(crate) mod coediting;
mod configuration;
pub(crate) mod external_sharing;
pub(crate) mod import;
pub(crate) mod page_access;
mod page_attachments;
mod page_cover;
pub(crate) mod page_discussion;
mod page_format;
mod page_history;
pub(crate) mod page_sharing;
mod page_text;
pub(crate) mod page_watch;
mod pages;
mod sections;

pub(crate) use page_access::PageAddress;
pub(crate) use page_attachments::{
    Attachment, AttachmentBody, AttachmentLookup, AttachmentPatch, StageAttachment,
};
pub(crate) use page_cover::{CoverUpdate, PageCover, StagePageCover};
pub(crate) use page_format::PageContentFormat;
pub(crate) use page_history::{PageHistory, PageRevisionSnapshot, RestorePageRevision};
pub(crate) use page_text::{CoeditPublication, PageUpdate, SavePageText};
pub(crate) use pages::{CreatePage, CreatePageInput, Page};
pub(crate) use sections::{CreateSection, Section, SectionTree};

use std::time::Duration;

use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgConnection, PgPool, postgres::PgPoolOptions};
use uuid::Uuid;

use crate::text::{fold_name, js_trim};

#[derive(Clone)]
pub(crate) struct Store(PgPool);

static MIGRATIONS: sqlx::migrate::Migrator = sqlx::migrate!();

#[derive(FromRow)]
pub(crate) struct Principal {
    pub id: Uuid,
    pub name: String,
}

#[derive(FromRow)]
pub(crate) struct Credentials {
    pub id: Uuid,
    pub salt: Vec<u8>,
    pub password_hash: Vec<u8>,
}

pub(crate) struct NewAccount {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub salt: Vec<u8>,
    pub password_hash: Vec<u8>,
}

pub(crate) struct NewSession {
    pub digest: Vec<u8>,
    pub created_at: i64,
    pub expires_at: i64,
}

#[derive(Clone, FromRow)]
pub(crate) struct Tenant {
    pub id: Uuid,
    pub name: String,
    pub role: String,
    pub can_create: bool,
}

#[derive(Clone)]
pub(crate) struct Space {
    pub id: Uuid,
    pub name: String,
    pub owner_name: String,
    pub is_owner: bool,
    pub tenant_role: String,
    pub grant_role: Option<String>,
}

#[derive(FromRow)]
struct SpaceRow {
    id: Uuid,
    name: Vec<u8>,
    owner_name: String,
    is_owner: bool,
    tenant_role: String,
    grant_role: Option<String>,
}

impl TryFrom<SpaceRow> for Space {
    type Error = sqlx::Error;

    fn try_from(row: SpaceRow) -> Result<Self, Self::Error> {
        let name = String::from_utf8(row.name).map_err(|source| sqlx::Error::ColumnDecode {
            index: "spaces.name".to_owned(),
            source: Box::new(source),
        })?;
        Ok(Self {
            id: row.id,
            name,
            owner_name: row.owner_name,
            is_owner: row.is_owner,
            tenant_role: row.tenant_role,
            grant_role: row.grant_role,
        })
    }
}

pub(crate) struct TenantSpaces {
    pub tenant: Tenant,
    pub tenants: Vec<Tenant>,
    pub spaces: Vec<Space>,
}

pub(crate) enum CreateSpace {
    Saved(Uuid),
    MissingMembership,
    Forbidden,
    OperationConflict,
    EmptyName,
    LongName,
    Duplicate,
}

impl Store {
    #[cfg(test)]
    pub(crate) fn from_test_pool(pool: PgPool) -> Self {
        Self(pool)
    }

    pub(crate) async fn connect(url: &str) -> anyhow::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(8)
            .acquire_timeout(Duration::from_secs(5))
            .connect(url)
            .await?;
        Ok(Self(pool))
    }

    pub(crate) async fn migrate(&self) -> anyhow::Result<()> {
        MIGRATIONS.run(&self.0).await?;
        Ok(())
    }

    pub(crate) async fn check_schema(&self) -> anyhow::Result<()> {
        let migrations: Vec<(i64, Vec<u8>, bool)> = sqlx::query_as(
            "SELECT version, checksum, success FROM _sqlx_migrations ORDER BY version",
        ).fetch_all(&self.0).await.map_err(|_| anyhow::anyhow!(
            "Не удалось проверить схему PostgreSQL. Выполните sever migrate и проверьте доступ к базе."
        ))?;
        let mut expected = MIGRATIONS.iter();
        let known = migrations.iter().all(|(version, checksum, success)| {
            *success
                && expected.next().is_some_and(|migration| {
                    migration.version == *version && migration.checksum.as_ref() == checksum
                })
        });
        anyhow::ensure!(
            known && expected.next().is_none(),
            "Схема PostgreSQL несовместима с этим выпуском"
        );
        Ok(())
    }

    pub(crate) async fn credentials(
        &self,
        email: &str,
    ) -> Result<Option<Credentials>, sqlx::Error> {
        sqlx::query_as("SELECT id, salt, password_hash FROM accounts WHERE email = $1")
            .bind(email)
            .fetch_optional(&self.0)
            .await
    }

    pub(crate) async fn add_account(&self, account: &NewAccount) -> Result<bool, sqlx::Error> {
        insert_account(&mut *self.0.acquire().await?, account).await
    }

    pub(crate) async fn add_session(
        &self,
        id: Uuid,
        session: &NewSession,
    ) -> Result<(), sqlx::Error> {
        let mut tx = self.0.begin().await?;
        insert_session(&mut tx, id, session).await?;
        tx.commit().await
    }

    pub(crate) async fn principal(
        &self,
        digest: &[u8],
        now: i64,
    ) -> Result<Option<Principal>, sqlx::Error> {
        sqlx::query_as(
            "SELECT a.id, a.name FROM sessions s JOIN accounts a ON a.id = s.account_id
             WHERE s.token_digest = $1 AND s.expires_at > $2",
        )
        .bind(digest)
        .bind(now)
        .fetch_optional(&self.0)
        .await
    }

    pub(crate) async fn register(
        &self,
        account: &NewAccount,
        session: &NewSession,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        if !insert_account(&mut tx, account).await? {
            return Ok(None);
        }
        let tenant_id = Uuid::new_v4();
        sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'Моя среда')")
            .bind(tenant_id)
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "INSERT INTO tenant_memberships (tenant_id, account_id, role, status, can_create, is_current)
             VALUES ($1, $2, 'superadmin', 'active', true, true)",
        ).bind(tenant_id).bind(account.id).execute(&mut *tx).await?;
        insert_session(&mut tx, account.id, session).await?;
        tx.commit().await?;
        Ok(Some(tenant_id))
    }

    pub(crate) async fn workspace(
        &self,
        account: Uuid,
        requested: Option<Uuid>,
    ) -> Result<(Option<TenantSpaces>, Vec<Tenant>), sqlx::Error> {
        let mut tx = self.0.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        let members: Vec<(Uuid, String, String, bool, bool)> = sqlx::query_as(
            "SELECT t.id, t.name, m.role, m.can_create, m.is_current
             FROM tenant_memberships m JOIN tenants t ON t.id = m.tenant_id
             WHERE m.account_id = $1 AND m.status = 'active'
             ORDER BY t.name COLLATE \"C\", t.id",
        )
        .bind(account)
        .fetch_all(&mut *tx)
        .await?;
        let selected = members
            .iter()
            .find(|row| requested.map_or(row.4, |id| row.0 == id))
            .map(|row| row.0);
        let tenants: Vec<Tenant> = members
            .into_iter()
            .map(|(id, name, role, can_create, _)| Tenant {
                id,
                name,
                role,
                can_create,
            })
            .collect();
        let Some(tenant) = tenants
            .iter()
            .find(|tenant| Some(tenant.id) == selected)
            .cloned()
        else {
            return Ok((None, tenants));
        };
        // The picker needs the full set. The same space set feeds the paginated catalog.
        let spaces = sqlx::query_as::<_, SpaceRow>(
            "SELECT s.id, s.name, owner.name AS owner_name,
                    s.owner_account_id = m.account_id AS is_owner, m.role AS tenant_role,
                    g.role AS grant_role
             FROM tenant_memberships m JOIN spaces s ON s.tenant_id = m.tenant_id
             JOIN accounts owner ON owner.id = s.owner_account_id
             LEFT JOIN space_grants g ON g.tenant_id = s.tenant_id
                  AND g.space_id = s.id AND g.account_id = m.account_id
             WHERE m.tenant_id = $1 AND m.account_id = $2 AND m.status = 'active'
               AND (s.owner_account_id = m.account_id OR m.role IN ('admin', 'superadmin') OR g.role IS NOT NULL)",
        ).bind(tenant.id).bind(account).fetch_all(&mut *tx).await?;
        let spaces = spaces
            .into_iter()
            .map(Space::try_from)
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit().await?;
        Ok((
            Some(TenantSpaces {
                tenant,
                tenants,
                spaces,
            }),
            Vec::new(),
        ))
    }

    pub(crate) async fn create_space(
        &self,
        tenant: Uuid,
        account: Uuid,
        operation: Uuid,
        entered: &str,
    ) -> Result<CreateSpace, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        // Serializes creates by this member and coordinates with permission changes.
        let can_create: Option<bool> = sqlx::query_scalar(
            "SELECT can_create FROM tenant_memberships WHERE tenant_id = $1 AND account_id = $2
             AND status = 'active' FOR UPDATE",
        )
        .bind(tenant)
        .bind(account)
        .fetch_optional(&mut *tx)
        .await?;
        match can_create {
            None => return Ok(CreateSpace::MissingMembership),
            Some(false) => return Ok(CreateSpace::Forbidden),
            Some(true) => {}
        }
        let name = js_trim(entered);
        let fingerprint = format!("{:x}", Sha256::digest(name.as_bytes()));
        let existing: Option<(String, Uuid)> = sqlx::query_as(
            "SELECT request_fingerprint, space_id FROM space_creations
             WHERE tenant_id = $1 AND creator_account_id = $2 AND operation_id = $3",
        )
        .bind(tenant)
        .bind(account)
        .bind(operation)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((previous, id)) = existing {
            return Ok(if previous == fingerprint {
                CreateSpace::Saved(id)
            } else {
                CreateSpace::OperationConflict
            });
        }
        if name.is_empty() {
            return Ok(CreateSpace::EmptyName);
        }
        if name.chars().count() > 100 {
            return Ok(CreateSpace::LongName);
        }
        let id = Uuid::new_v4();
        let inserted = sqlx::query(
            "INSERT INTO spaces (tenant_id, id, creator_account_id, owner_account_id, name, name_key)
             VALUES ($1, $2, $3, $3, $4, $5) ON CONFLICT (tenant_id, owner_account_id, name_key) DO NOTHING",
        ).bind(tenant).bind(id).bind(account).bind(name.as_bytes()).bind(fold_name(name).into_bytes()).execute(&mut *tx).await?;
        if inserted.rows_affected() == 0 {
            return Ok(CreateSpace::Duplicate);
        }
        sqlx::query(
            "INSERT INTO space_creations (tenant_id, creator_account_id, operation_id, request_fingerprint, space_id)
             VALUES ($1, $2, $3, $4, $5)",
        ).bind(tenant).bind(account).bind(operation).bind(fingerprint).bind(id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(CreateSpace::Saved(id))
    }
}

async fn insert_account(
    conn: &mut PgConnection,
    account: &NewAccount,
) -> Result<bool, sqlx::Error> {
    let inserted = sqlx::query(
        "INSERT INTO accounts (id, email, name, salt, password_hash)
         VALUES ($1, $2, $3, $4, $5) ON CONFLICT (email) DO NOTHING",
    )
    .bind(account.id)
    .bind(&account.email)
    .bind(&account.name)
    .bind(&account.salt)
    .bind(&account.password_hash)
    .execute(conn)
    .await?;
    Ok(inserted.rows_affected() == 1)
}

async fn insert_session(
    conn: &mut PgConnection,
    account: Uuid,
    session: &NewSession,
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM sessions WHERE expires_at <= $1")
        .bind(session.created_at)
        .execute(&mut *conn)
        .await?;
    sqlx::query("INSERT INTO sessions (token_digest, account_id, created_at, expires_at) VALUES ($1, $2, $3, $4)")
        .bind(&session.digest).bind(account).bind(session.created_at).bind(session.expires_at).execute(conn).await?;
    Ok(())
}
