use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{SharingFailure, SharingOperation, manager_tx, tenant_allows};
use crate::{
    clock::now_ms,
    db::{
        NewAccount, NewSession, PageAddress, Store, insert_account, insert_session,
        page_access::lock_page_for_change, page_sharing::SHARING_PAGE_SIZE,
    },
};

const INVITATION_LIFETIME: i64 = 7 * 24 * 60 * 60 * 1000;

#[derive(Serialize)]
pub(crate) struct GuestInvitation {
    id: Uuid,
    email: String,
    role: String,
    status: String,
    delivery: &'static str,
}

pub(crate) struct GuestPreview {
    pub(crate) email: String,
    pub(crate) account: Option<Uuid>,
}

pub(crate) enum GuestAcceptance<'a> {
    Existing(Uuid),
    New(&'a NewAccount, &'a NewSession),
}

#[derive(sqlx::FromRow)]
struct PendingGuest {
    tenant_id: Uuid,
    space_id: Uuid,
    page_id: Uuid,
    email: String,
    role: String,
    accepted_by: Option<Uuid>,
}

impl Store {
    pub(crate) async fn guest_invitations(
        &self,
        address: &PageAddress,
        offset: i64,
    ) -> Result<(bool, Vec<GuestInvitation>), SharingFailure> {
        let mut tx = manager_tx(self, address).await?;
        let allowed = tenant_allows(&mut tx, address.tenant).await?.0;
        let rows: Vec<(Uuid, String, String, String, bool)> = sqlx::query_as(
            "SELECT id,email,role,
              CASE WHEN accepted_at IS NOT NULL THEN 'accepted'
                   WHEN revoked_at < expires_at THEN 'revoked' WHEN expires_at <= $4 THEN 'expired' ELSE 'pending' END,
              delivered_at IS NOT NULL
             FROM page_guest_invitations WHERE (tenant_id,space_id,page_id) = ($1,$2,$3)
             ORDER BY (revoked_at IS NULL AND accepted_at IS NULL AND expires_at > $4) DESC,
                      issued_at DESC,id LIMIT $5 OFFSET $6",
        )
        .bind(address.tenant).bind(address.space).bind(address.page).bind(now_ms())
        .bind((SHARING_PAGE_SIZE + 1) as i64).bind(offset)
        .fetch_all(&mut *tx).await?;
        tx.commit().await?;
        Ok((
            allowed,
            rows.into_iter()
                .map(|(id, email, role, status, delivered)| GuestInvitation {
                    id,
                    email,
                    role,
                    status,
                    delivery: if delivered { "sent" } else { "pending" },
                })
                .collect(),
        ))
    }

    pub(crate) async fn issue_guest_invitation(
        &self,
        address: &PageAddress,
        operation: &SharingOperation,
        invite: (Uuid, &[u8; 32]),
        recipient: (&str, &str),
    ) -> Result<Uuid, SharingFailure> {
        let mut tx = manager_tx(self, address).await?;
        if !tenant_allows(&mut tx, address.tenant).await?.0 {
            return Err(SharingFailure::Forbidden);
        }
        if let Some(result) = operation.replay(&mut tx, address).await? {
            return result.ok_or(SharingFailure::Conflict);
        }
        let (id, digest) = invite;
        let (email, role) = recipient;
        let now = now_ms();
        sqlx::query(
            "UPDATE page_guest_invitations SET revoked_at = $5
             WHERE (tenant_id,space_id,page_id,email) = ($1,$2,$3,$4)
               AND expires_at <= $5 AND revoked_at IS NULL AND accepted_at IS NULL",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(email)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        let inserted = sqlx::query(
            "INSERT INTO page_guest_invitations
             (id,tenant_id,space_id,page_id,email,role,token_digest,issued_by,issued_at,expires_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
             ON CONFLICT DO NOTHING",
        )
        .bind(id)
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(email)
        .bind(role)
        .bind(&digest[..])
        .bind(address.account)
        .bind(now)
        .bind(now + INVITATION_LIFETIME)
        .execute(&mut *tx)
        .await?;
        let invitation = if inserted.rows_affected() == 1 {
            id
        } else {
            sqlx::query_scalar(
                "SELECT id FROM page_guest_invitations
                 WHERE (tenant_id,space_id,page_id,email,role) = ($1,$2,$3,$4,$5)
                   AND expires_at > $6 AND revoked_at IS NULL AND accepted_at IS NULL
                   AND delivered_at IS NULL",
            )
            .bind(address.tenant)
            .bind(address.space)
            .bind(address.page)
            .bind(email)
            .bind(role)
            .bind(now)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(SharingFailure::Conflict)?
        };
        operation.record(&mut tx, address, Some(invitation)).await?;
        tx.commit().await?;
        Ok(invitation)
    }

    pub(crate) async fn claim_guest_delivery(
        &self,
        id: Uuid,
    ) -> Result<Option<(String, i64)>, sqlx::Error> {
        let now = now_ms();
        sqlx::query_as(
            "UPDATE page_guest_invitations i SET send_lease = $3
             FROM tenant_sharing_policies t
             WHERE i.id = $1 AND i.delivered_at IS NULL AND i.accepted_at IS NULL
               AND i.revoked_at IS NULL AND i.expires_at > $2
               AND COALESCE(i.send_lease,0) <= $2 AND i.tenant_id = t.tenant_id AND t.allow_guests
             RETURNING i.email,i.send_lease",
        )
        .bind(id)
        .bind(now)
        .bind(now + 6_000)
        .fetch_optional(&self.0)
        .await
    }

    pub(crate) async fn finish_guest_delivery(
        &self,
        id: Uuid,
        lease: i64,
        delivered: bool,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE page_guest_invitations
             SET send_lease = NULL, delivered_at = CASE WHEN $3 THEN $4 ELSE NULL END
             WHERE id = $1 AND send_lease = $2",
        )
        .bind(id)
        .bind(lease)
        .bind(delivered)
        .bind(now_ms())
        .execute(&self.0)
        .await?;
        Ok(())
    }

    pub(crate) async fn cancel_guest_invitation(
        &self,
        address: &PageAddress,
        operation: &SharingOperation,
        id: Uuid,
    ) -> Result<(), SharingFailure> {
        let mut tx = manager_tx(self, address).await?;
        if operation.replay(&mut tx, address).await?.is_some() {
            return Ok(());
        }
        let result = sqlx::query(
            "UPDATE page_guest_invitations SET revoked_at = COALESCE(revoked_at,$5)
             WHERE (tenant_id,space_id,page_id,id) = ($1,$2,$3,$4) AND accepted_at IS NULL",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(id)
        .bind(now_ms())
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() == 0 {
            return Err(SharingFailure::Missing);
        }
        operation.record(&mut tx, address, None).await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn preview_guest(
        &self,
        id: Uuid,
        digest: &[u8; 32],
    ) -> Result<Option<GuestPreview>, sqlx::Error> {
        let row: Option<(String, Option<Uuid>)> = sqlx::query_as(
            "SELECT i.email,a.id FROM page_guest_invitations i
             JOIN tenant_sharing_policies t ON t.tenant_id = i.tenant_id
             LEFT JOIN accounts a ON a.email = i.email
             WHERE i.id = $1 AND i.token_digest = $2 AND i.revoked_at IS NULL
               AND i.expires_at > $3 AND t.allow_guests",
        )
        .bind(id)
        .bind(&digest[..])
        .bind(now_ms())
        .fetch_optional(&self.0)
        .await?;
        Ok(row.map(|(email, account)| GuestPreview { email, account }))
    }

    pub(crate) async fn accept_guest_invitation(
        &self,
        id: Uuid,
        digest: &[u8; 32],
        recipient: GuestAcceptance<'_>,
    ) -> Result<PageAddress, SharingFailure> {
        let account = match recipient {
            GuestAcceptance::Existing(id) => id,
            GuestAcceptance::New(account, _) => account.id,
        };
        let mut tx = self.0.begin().await?;
        let Some(invite) = load_invitation(&mut tx, id, digest, false).await? else {
            return Err(SharingFailure::Missing);
        };
        let address = PageAddress {
            tenant: invite.tenant_id,
            space: invite.space_id,
            page: invite.page_id,
            account,
        };
        // The invited account need not be a member. The page lock still serializes accept, revoke and cancel.
        let access = lock_page_for_change(&mut tx, &address).await?;
        if !tenant_allows(&mut tx, address.tenant).await?.0 {
            return Err(SharingFailure::Forbidden);
        }
        let Some(invite) = load_invitation(&mut tx, id, digest, true).await? else {
            return Err(SharingFailure::Missing);
        };
        if let Some(accepted) = invite.accepted_by {
            return if accepted == account && access.is_some_and(|access| access.can_read) {
                Ok(address)
            } else {
                Err(SharingFailure::Missing)
            };
        }
        let suspended: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM tenant_memberships
             WHERE tenant_id = $1 AND account_id = $2 AND status = 'inactive')",
        )
        .bind(address.tenant)
        .bind(account)
        .fetch_one(&mut *tx)
        .await?;
        if suspended {
            return Err(SharingFailure::Forbidden);
        }
        match recipient {
            GuestAcceptance::Existing(_) => {
                let matches: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM accounts WHERE id = $1 AND email = $2)",
                )
                .bind(account)
                .bind(&invite.email)
                .fetch_one(&mut *tx)
                .await?;
                if !matches {
                    return Err(SharingFailure::Forbidden);
                }
            }
            GuestAcceptance::New(new, session) => {
                if new.email != invite.email || !insert_account(&mut tx, new).await? {
                    return Err(SharingFailure::Conflict);
                }
                insert_session(&mut tx, new.id, session).await?;
            }
        }
        sqlx::query(
            "INSERT INTO page_grants AS existing (tenant_id,space_id,page_id,account_id,role,guest)
             VALUES ($1,$2,$3,$4,$5,NOT EXISTS(SELECT 1 FROM tenant_memberships
                        WHERE tenant_id = $1 AND account_id = $4 AND status = 'active'))
             ON CONFLICT (tenant_id,space_id,page_id,account_id) DO UPDATE SET
             role = CASE WHEN existing.role='edit' OR EXCLUDED.role='edit' THEN 'edit'
                         WHEN existing.role='comment' OR EXCLUDED.role='comment' THEN 'comment' ELSE 'read' END,
             guest = existing.guest AND EXCLUDED.guest",
        )
        .bind(address.tenant).bind(address.space).bind(address.page).bind(account).bind(invite.role)
        .execute(&mut *tx).await?;
        sqlx::query(
            "UPDATE page_guest_invitations SET accepted_by = $2,accepted_at = $3 WHERE id = $1",
        )
        .bind(id)
        .bind(account)
        .bind(now_ms())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(address)
    }
}

async fn load_invitation(
    tx: &mut PgConnection,
    id: Uuid,
    digest: &[u8; 32],
    lock: bool,
) -> Result<Option<PendingGuest>, sqlx::Error> {
    let select =
        "SELECT tenant_id,space_id,page_id,email,role,accepted_by FROM page_guest_invitations
                  WHERE id = $1 AND token_digest = $2 AND revoked_at IS NULL AND expires_at > $3";
    let query = if lock {
        format!("{select} FOR UPDATE")
    } else {
        select.to_owned()
    };
    sqlx::query_as(&query)
        .bind(id)
        .bind(&digest[..])
        .bind(now_ms())
        .fetch_optional(tx)
        .await
}
