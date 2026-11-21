use anyhow::{Context, ensure};
use sqlx::PgConnection;
use uuid::Uuid;

use super::Store;
use crate::settings::{GrantSettings, TenantSettings};

impl Store {
    pub(crate) async fn configure_tenants(&self, config: TenantSettings) -> anyhow::Result<()> {
        let mut tx = self.0.begin().await?;
        lock_settings(&mut tx).await?;
        let tenant_ids: Vec<_> = config
            .tenants
            .iter()
            .map(|tenant| tenant.id)
            .chain(config.memberships.iter().map(|member| member.tenant_id))
            .collect();
        lock_permission_tenants(&mut tx, &tenant_ids).await?;
        for tenant in &config.tenants {
            sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, $2) ON CONFLICT (id) DO UPDATE SET name = EXCLUDED.name")
            .bind(tenant.id).bind(&tenant.name).execute(&mut *tx).await?;
            if tenant.allow_guests.is_some() || tenant.allow_public_links.is_some() {
                configure_sharing(
                    &mut tx,
                    tenant.id,
                    tenant.allow_guests,
                    tenant.allow_public_links,
                )
                .await?;
            }
        }
        for member in &config.memberships {
            let account = account_for_update(&mut tx, &member.email)
                .await?
                .context("Сначала создайте все учётные записи, указанные в членствах.")?;
            if member.current {
                sqlx::query("UPDATE tenant_memberships SET is_current = false WHERE account_id = $1 AND is_current")
                .bind(account).execute(&mut *tx).await?;
            }
            sqlx::query(
            "INSERT INTO tenant_memberships (tenant_id, account_id, role, status, can_create, is_current)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (tenant_id, account_id) DO UPDATE SET role = EXCLUDED.role, status = EXCLUDED.status,
                 can_create = EXCLUDED.can_create, is_current = EXCLUDED.is_current",
        ).bind(member.tenant_id).bind(account).bind(member.role.as_str()).bind(member.status.as_str())
            .bind(member.can_create).bind(member.current).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn configure_grants(&self, config: GrantSettings) -> anyhow::Result<()> {
        let mut tx = self.0.begin().await?;
        lock_settings(&mut tx).await?;
        let tenant_ids: Vec<_> = config.grants.iter().map(|grant| grant.tenant_id).collect();
        lock_permission_tenants(&mut tx, &tenant_ids).await?;
        for grant in &config.grants {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM spaces WHERE tenant_id = $1 AND id = $2)",
            )
            .bind(grant.tenant_id)
            .bind(grant.space_id)
            .fetch_one(&mut *tx)
            .await?;
            ensure!(
                exists,
                "Проверьте, что пространство существует в указанном тенанте."
            );
            let (account, active): (Uuid, bool) = sqlx::query_as(
            "SELECT m.account_id, m.status = 'active' FROM tenant_memberships m JOIN accounts a ON a.id = m.account_id
             WHERE m.tenant_id = $1 AND a.email = $2 FOR UPDATE OF m",
        ).bind(grant.tenant_id).bind(&grant.email).fetch_optional(&mut *tx).await?
            .context("Назначение требует членства в том же тенанте.")?;
            if let Some(role) = &grant.role {
                ensure!(
                    active,
                    "Назначение требует действующего членства в том же тенанте."
                );
                sqlx::query(
                "INSERT INTO space_grants (tenant_id, space_id, account_id, role) VALUES ($1, $2, $3, $4)
                 ON CONFLICT (tenant_id, space_id, account_id) DO UPDATE SET role = EXCLUDED.role",
            ).bind(grant.tenant_id).bind(grant.space_id).bind(account).bind(role.as_str()).execute(&mut *tx).await?;
            } else {
                sqlx::query("DELETE FROM space_grants WHERE tenant_id = $1 AND space_id = $2 AND account_id = $3")
                .bind(grant.tenant_id).bind(grant.space_id).bind(account).execute(&mut *tx).await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }
}

async fn lock_settings(conn: &mut PgConnection) -> Result<(), sqlx::Error> {
    // Different operator batches may touch the same rows in different orders.
    sqlx::query("SELECT pg_advisory_xact_lock(48744680, 1)")
        .execute(conn)
        .await?;
    Ok(())
}

async fn lock_permission_tenants(
    conn: &mut PgConnection,
    tenant_ids: &[Uuid],
) -> Result<(), sqlx::Error> {
    // Page mutations take a share lock before reading authorization and
    // publishing to watchers. Permission changes take the same parent first.
    sqlx::query("SELECT id FROM tenants WHERE id = ANY($1) ORDER BY id FOR NO KEY UPDATE")
        .bind(tenant_ids)
        .execute(conn)
        .await?;
    Ok(())
}

async fn account_for_update(
    conn: &mut PgConnection,
    email: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar("SELECT id FROM accounts WHERE email = $1 FOR NO KEY UPDATE")
        .bind(email)
        .fetch_optional(conn)
        .await
}

pub(super) async fn configure_sharing(
    conn: &mut PgConnection,
    tenant: Uuid,
    allow_guests: Option<bool>,
    allow_public: Option<bool>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO tenant_sharing_policies (tenant_id,allow_guests,allow_public)
         VALUES ($1,COALESCE($2,false),COALESCE($3,false))
         ON CONFLICT (tenant_id) DO UPDATE SET
         allow_guests = COALESCE($2,tenant_sharing_policies.allow_guests),
         allow_public = COALESCE($3,tenant_sharing_policies.allow_public)",
    )
    .bind(tenant)
    .bind(allow_guests)
    .bind(allow_public)
    .execute(&mut *conn)
    .await?;
    let now = crate::clock::now_ms();
    if allow_guests == Some(false) {
        sqlx::query(
            "UPDATE page_guest_invitations SET revoked_at = $2
             WHERE tenant_id = $1 AND revoked_at IS NULL AND accepted_at IS NULL",
        )
        .bind(tenant)
        .bind(now)
        .execute(&mut *conn)
        .await?;
    }
    if allow_public == Some(false) {
        sqlx::query(
            "UPDATE space_public_policies SET enabled = false, version = version + 1
             WHERE tenant_id = $1 AND enabled",
        )
        .bind(tenant)
        .execute(&mut *conn)
        .await?;
        sqlx::query(
            "UPDATE public_page_links SET revoked_at = $2
             WHERE tenant_id = $1 AND revoked_at IS NULL",
        )
        .bind(tenant)
        .bind(now)
        .execute(conn)
        .await?;
    }
    Ok(())
}
