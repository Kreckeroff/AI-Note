use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use super::{
    PageAddress, Store,
    page_access::{PAGE_ACCESS, lock_page_for_change},
};

pub(crate) const SHARING_PAGE_SIZE: usize = 20;

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PageRole {
    Read,
    Comment,
    Edit,
}

impl PageRole {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Comment => "comment",
            Self::Edit => "edit",
        }
    }
}

#[derive(Deserialize)]
#[serde(
    tag = "action",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum PageSharingAction {
    Grant {
        account_id: Uuid,
        role: PageRole,
        operation_id: Uuid,
    },
    Revoke {
        account_id: Uuid,
        operation_id: Uuid,
    },
    Request {
        operation_id: Uuid,
    },
    Approve {
        request_id: Uuid,
        operation_id: Uuid,
    },
    Deny {
        request_id: Uuid,
        operation_id: Uuid,
    },
}

impl PageSharingAction {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Grant { .. } => "grant",
            Self::Revoke { .. } => "revoke",
            Self::Request { .. } => "request",
            Self::Approve { .. } => "approve",
            Self::Deny { .. } => "deny",
        }
    }

    pub(crate) fn operation(&self) -> Uuid {
        match self {
            Self::Grant { operation_id, .. }
            | Self::Revoke { operation_id, .. }
            | Self::Request { operation_id }
            | Self::Approve { operation_id, .. }
            | Self::Deny { operation_id, .. } => *operation_id,
        }
    }

    fn fingerprint(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(self.name().as_bytes());
        hash.update([0]);
        match self {
            Self::Grant {
                account_id, role, ..
            } => {
                hash.update(account_id.as_bytes());
                hash.update(role.as_str().as_bytes());
            }
            Self::Revoke { account_id, .. } => hash.update(account_id.as_bytes()),
            Self::Approve { request_id, .. } | Self::Deny { request_id, .. } => {
                hash.update(request_id.as_bytes())
            }
            Self::Request { .. } => {}
        }
        hash.finalize().into()
    }

    fn target_account(&self) -> Option<Uuid> {
        match self {
            Self::Grant { account_id, .. } | Self::Revoke { account_id, .. } => Some(*account_id),
            _ => None,
        }
    }
}

#[derive(FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PagePerson {
    pub(crate) id: Uuid,
    name: String,
    email: String,
    direct: Option<String>,
    inherited: Option<String>,
    effective: Option<String>,
    source: Option<String>,
    guest: bool,
}

#[derive(FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PageAccessRequest {
    id: Uuid,
    account_id: Uuid,
    name: String,
    status: String,
}

pub(crate) enum PageSharingOutcome {
    Saved(Option<PagePerson>),
    Accepted,
    Hidden,
    Forbidden,
    InvalidTarget,
    Conflict,
}

impl Store {
    pub(crate) async fn page_people(
        &self,
        address: &PageAddress,
        search: &str,
        filter: &str,
        offset: i64,
    ) -> Result<Vec<PagePerson>, sqlx::Error> {
        let query = format!(
            r#"WITH manager AS ({PAGE_ACCESS}),
                candidates AS (
                    SELECT account_id FROM tenant_memberships WHERE tenant_id = $1 AND status = 'active'
                    UNION
                    SELECT account_id FROM page_grants WHERE (tenant_id, space_id, page_id) = ($1, $2, $4)
                )
                SELECT a.id, a.name, a.email, direct.role AS direct, target.inherited_role AS inherited,
                       target.effective_role AS effective, target.inheritance_source AS source,
                       coalesce(direct.guest, false) AS guest
                FROM candidates c
                JOIN accounts a ON a.id = c.account_id
                CROSS JOIN manager
                CROSS JOIN LATERAL page_permissions($1, $2, a.id, $4) target
                LEFT JOIN page_grants direct ON (direct.tenant_id, direct.space_id, direct.page_id, direct.account_id)
                    = ($1, $2, $4, a.id)
                WHERE manager.can_manage
                  AND ($5 = '' OR strpos(lower(a.name), lower($5)) > 0 OR strpos(a.email, lower($5)) > 0)
                  AND CASE $6
                      WHEN 'direct' THEN direct.role IS NOT NULL
                      WHEN 'inherited' THEN target.inherited_role IS NOT NULL
                      WHEN 'available' THEN direct.role IS NULL AND target.inherited_role IS NULL
                      ELSE true
                  END
                ORDER BY lower(a.name), a.id
                LIMIT $7 OFFSET $8"#,
        );
        sqlx::query_as(&query)
            .bind(address.tenant)
            .bind(address.space)
            .bind(address.account)
            .bind(address.page)
            .bind(search)
            .bind(filter)
            .bind(SHARING_PAGE_SIZE as i64 + 1)
            .bind(offset)
            .fetch_all(&self.0)
            .await
    }

    pub(crate) async fn page_access_requests(
        &self,
        address: &PageAddress,
        offset: i64,
    ) -> Result<Vec<PageAccessRequest>, sqlx::Error> {
        let query = format!(
            "SELECT r.id, r.account_id, a.name, r.status FROM ({PAGE_ACCESS}) manager
             JOIN page_access_requests r ON (r.tenant_id, r.space_id, r.page_id) = ($1, $2, $4)
             JOIN accounts a ON a.id = r.account_id
             WHERE manager.can_manage
             ORDER BY (r.status = 'pending') DESC, r.created_at DESC, r.id
             LIMIT $5 OFFSET $6",
        );
        sqlx::query_as(&query)
            .bind(address.tenant)
            .bind(address.space)
            .bind(address.account)
            .bind(address.page)
            .bind(SHARING_PAGE_SIZE as i64 + 1)
            .bind(offset)
            .fetch_all(&self.0)
            .await
    }

    pub(crate) async fn change_page_sharing(
        &self,
        address: &PageAddress,
        action: &PageSharingAction,
    ) -> Result<PageSharingOutcome, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        let request = matches!(action, PageSharingAction::Request { .. });
        let Some(permissions) = lock_page_for_change(&mut tx, address).await? else {
            return Ok(if request {
                PageSharingOutcome::Accepted
            } else {
                PageSharingOutcome::Hidden
            });
        };
        if request {
            if !permissions.can_request || permissions.can_read {
                return Ok(PageSharingOutcome::Accepted);
            }
        } else if !permissions.can_manage {
            return Ok(if permissions.can_read {
                PageSharingOutcome::Forbidden
            } else {
                PageSharingOutcome::Hidden
            });
        }

        let fingerprint = action.fingerprint();
        let previous = receipt(&mut tx, address, action.operation()).await?;
        if previous
            .as_deref()
            .is_some_and(|value| value != fingerprint)
        {
            return Ok(PageSharingOutcome::Conflict);
        }
        if previous.is_none() {
            if !apply(&mut tx, address, action).await? {
                return Ok(PageSharingOutcome::InvalidTarget);
            }
            record_receipt(&mut tx, address, action.operation(), &fingerprint).await?;
        }
        let person = match action.target_account() {
            Some(account) => page_person(&mut tx, address, account).await?,
            None => None,
        };
        tx.commit().await?;
        Ok(if request {
            PageSharingOutcome::Accepted
        } else {
            PageSharingOutcome::Saved(person)
        })
    }
}

async fn apply(
    tx: &mut PgConnection,
    address: &PageAddress,
    action: &PageSharingAction,
) -> Result<bool, sqlx::Error> {
    match *action {
        PageSharingAction::Grant {
            account_id, role, ..
        } => {
            if account_id == address.account {
                return Ok(false);
            }
            if grant_member(tx, address, account_id, role, false).await? {
                return Ok(true);
            }
            change_guest_role(tx, address, account_id, role).await
        }
        PageSharingAction::Revoke { account_id, .. } => {
            sqlx::query(
                "DELETE FROM page_grants WHERE (tenant_id, space_id, page_id, account_id) = ($1, $2, $3, $4)",
            )
            .bind(address.tenant)
            .bind(address.space)
            .bind(address.page)
            .bind(account_id)
            .execute(tx)
            .await?;
            Ok(true)
        }
        PageSharingAction::Request { .. } => {
            sqlx::query(
                "INSERT INTO page_access_requests (tenant_id, space_id, page_id, id, account_id, status)
                 VALUES ($1, $2, $3, $4, $5, 'pending') ON CONFLICT DO NOTHING",
            )
            .bind(address.tenant)
            .bind(address.space)
            .bind(address.page)
            .bind(Uuid::new_v4())
            .bind(address.account)
            .execute(tx)
            .await?;
            Ok(true)
        }
        PageSharingAction::Approve { request_id, .. } => {
            decide(tx, address, request_id, true).await
        }
        PageSharingAction::Deny { request_id, .. } => decide(tx, address, request_id, false).await,
    }
}

async fn grant_member(
    tx: &mut PgConnection,
    address: &PageAddress,
    account: Uuid,
    role: PageRole,
    keep_existing: bool,
) -> Result<bool, sqlx::Error> {
    let granted: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO page_grants (tenant_id, space_id, page_id, account_id, role)
         SELECT $1, $2, $3, account_id, $5 FROM tenant_memberships
         WHERE (tenant_id, account_id) = ($1, $4) AND status = 'active'
         ON CONFLICT (tenant_id, space_id, page_id, account_id) DO UPDATE
         SET role = CASE WHEN $6 THEN page_grants.role ELSE EXCLUDED.role END,
             guest = false
         RETURNING account_id",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(account)
    .bind(role.as_str())
    .bind(keep_existing)
    .fetch_optional(tx)
    .await?;
    Ok(granted.is_some())
}

async fn change_guest_role(
    tx: &mut PgConnection,
    address: &PageAddress,
    account: Uuid,
    role: PageRole,
) -> Result<bool, sqlx::Error> {
    // A role change may update an accepted guest's existing assignment. Guest
    // access can only be established by the verified invitation path.
    let updated = sqlx::query(
        "UPDATE page_grants SET role = $5
         WHERE (tenant_id, space_id, page_id, account_id) = ($1, $2, $3, $4) AND guest
           AND EXISTS (SELECT 1 FROM tenant_sharing_policies WHERE tenant_id = $1 AND allow_guests)
           AND NOT EXISTS (
               SELECT 1 FROM tenant_memberships
               WHERE (tenant_id, account_id) = ($1, $4) AND status = 'inactive'
           )",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(account)
    .bind(role.as_str())
    .execute(tx)
    .await?;
    Ok(updated.rows_affected() != 0)
}

async fn decide(
    tx: &mut PgConnection,
    address: &PageAddress,
    request: Uuid,
    approve: bool,
) -> Result<bool, sqlx::Error> {
    let target: Option<Uuid> = sqlx::query_scalar(
        "UPDATE page_access_requests SET status = $5, decided_at = clock_timestamp(), decided_by = $6
         WHERE (tenant_id, space_id, page_id, id) = ($1, $2, $3, $4) AND status = 'pending'
         RETURNING account_id",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(request)
    .bind(if approve { "approved" } else { "denied" })
    .bind(address.account)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(target) = target else {
        return Ok(false);
    };
    if approve {
        // Failure rolls back the decision and leaves the request pending.
        return grant_member(tx, address, target, PageRole::Read, true).await;
    }
    Ok(true)
}

async fn receipt(
    tx: &mut PgConnection,
    address: &PageAddress,
    operation: Uuid,
) -> Result<Option<Vec<u8>>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT fingerprint FROM page_sharing_receipts
         WHERE (tenant_id, space_id, page_id, account_id, operation_id) = ($1, $2, $3, $4, $5)",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(address.account)
    .bind(operation)
    .fetch_optional(tx)
    .await
}

async fn record_receipt(
    tx: &mut PgConnection,
    address: &PageAddress,
    operation: Uuid,
    fingerprint: &[u8],
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO page_sharing_receipts (tenant_id, space_id, page_id, account_id, operation_id, fingerprint)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(address.account)
    .bind(operation)
    .bind(fingerprint)
    .execute(tx)
    .await?;
    Ok(())
}

async fn page_person(
    tx: &mut PgConnection,
    address: &PageAddress,
    account: Uuid,
) -> Result<Option<PagePerson>, sqlx::Error> {
    sqlx::query_as(
        "SELECT a.id, a.name, a.email, direct.role AS direct, target.inherited_role AS inherited,
                target.effective_role AS effective, target.inheritance_source AS source,
                coalesce(direct.guest, false) AS guest
         FROM accounts a
         CROSS JOIN LATERAL page_permissions($1, $2, a.id, $4) target
         LEFT JOIN page_grants direct ON (direct.tenant_id, direct.space_id, direct.page_id, direct.account_id)
              = ($1, $2, $4, a.id)
         WHERE a.id = $3 AND (direct.account_id IS NOT NULL OR EXISTS (
             SELECT 1 FROM tenant_memberships m WHERE (m.tenant_id, m.account_id) = ($1, a.id) AND m.status = 'active'
         ))",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(account)
    .bind(address.page)
    .fetch_optional(tx)
    .await
}

#[cfg(test)]
mod tests;
