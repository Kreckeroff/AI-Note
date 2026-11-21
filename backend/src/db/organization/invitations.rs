use sqlx::FromRow;
use uuid::Uuid;

use super::{Store, admin, lock_tenant, member_role};
use crate::db::{NewAccount, NewSession, insert_account, insert_session};

const VALID_FOR_MS: i64 = 7 * 24 * 60 * 60 * 1000;

#[derive(FromRow)]
pub(crate) struct Invitation {
    pub id: Uuid,
    pub email: String,
    pub expires_at: i64,
    pub revoked_at: Option<i64>,
    pub accepted_at: Option<i64>,
    pub delivered_at: Option<i64>,
}

#[derive(FromRow)]
pub(crate) struct InvitationAccess {
    pub tenant_id: Uuid,
    pub name: String,
    pub email: String,
    pub account_exists: bool,
}

impl Store {
    pub(crate) async fn organization_invitations(
        &self,
        tenant: Uuid,
        query: &str,
        offset: i64,
    ) -> Result<Vec<Invitation>, sqlx::Error> {
        sqlx::query_as("SELECT id,email,expires_at,revoked_at,accepted_at,delivered_at
             FROM organization_invitations WHERE tenant_id=$1 AND ($2='' OR strpos(email,lower($2))>0)
             ORDER BY issued_at DESC,id LIMIT 21 OFFSET $3",
        ).bind(tenant).bind(query).bind(offset).fetch_all(&self.0).await
    }

    pub(crate) async fn issue_organization_invitation(
        &self,
        tenant: Uuid,
        actor: Uuid,
        email: &str,
        digest: &[u8],
        now: i64,
    ) -> Result<Option<Uuid>, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        lock_tenant(&mut tx, tenant).await?;
        if !member_role(&mut tx, tenant, actor)
            .await?
            .is_some_and(|role| admin(&role))
        {
            return Ok(None);
        }
        let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM tenant_memberships m
            JOIN accounts a ON a.id=m.account_id WHERE m.tenant_id=$1 AND a.email=$2 AND m.status='active')")
            .bind(tenant).bind(email).fetch_one(&mut *tx).await?;
        if active {
            return Ok(None);
        }
        // Replace only expired/failed invitations automatically. A live sent link remains valid until explicitly revoked.
        sqlx::query(
            "UPDATE organization_invitations SET revoked_at=$3
             WHERE tenant_id=$1 AND email=$2 AND revoked_at IS NULL AND accepted_at IS NULL
             AND (expires_at<=$3 OR (delivered_at IS NULL AND issued_at<=$3-60000))",
        )
        .bind(tenant)
        .bind(email)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        let id = Uuid::new_v4();
        let inserted=sqlx::query("INSERT INTO organization_invitations
             (id,tenant_id,email,token_digest,issued_by,issued_at,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7)
             ON CONFLICT DO NOTHING")
            .bind(id).bind(tenant).bind(email).bind(digest).bind(actor).bind(now).bind(now+VALID_FOR_MS)
            .execute(&mut *tx).await?.rows_affected();
        tx.commit().await?;
        Ok((inserted > 0).then_some(id))
    }

    pub(crate) async fn finish_organization_invitation_delivery(
        &self,
        tenant: Uuid,
        invitation: Uuid,
        sent: bool,
        now: i64,
    ) -> Result<(), sqlx::Error> {
        // A just-delivered message may already have been accepted when the mail subprocess exits.
        sqlx::query(
            "UPDATE organization_invitations
            SET delivered_at=CASE WHEN $3 THEN $4 ELSE delivered_at END,
                revoked_at=CASE WHEN NOT $3 AND accepted_at IS NULL THEN $4 ELSE revoked_at END
            WHERE tenant_id=$1 AND id=$2 AND revoked_at IS NULL",
        )
        .bind(tenant)
        .bind(invitation)
        .bind(sent)
        .bind(now)
        .execute(&self.0)
        .await?;
        Ok(())
    }

    pub(crate) async fn revoke_organization_invitation(
        &self,
        tenant: Uuid,
        actor: Uuid,
        invitation: Uuid,
        now: i64,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        lock_tenant(&mut tx, tenant).await?;
        if !member_role(&mut tx, tenant, actor)
            .await?
            .is_some_and(|role| admin(&role))
        {
            return Ok(false);
        }
        let row = sqlx::query(
            "UPDATE organization_invitations SET revoked_at=$3
             WHERE tenant_id=$1 AND id=$2 AND accepted_at IS NULL AND revoked_at IS NULL",
        )
        .bind(tenant)
        .bind(invitation)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(row.rows_affected() > 0)
    }

    pub(crate) async fn organization_invitation_access(
        &self,
        digest: &[u8],
        now: i64,
        account: Option<Uuid>,
    ) -> Result<Option<InvitationAccess>, sqlx::Error> {
        sqlx::query_as("SELECT t.id AS tenant_id,t.name,i.email,
                EXISTS(SELECT 1 FROM accounts a WHERE a.email=i.email) AS account_exists
             FROM organization_invitations i JOIN tenants t ON t.id=i.tenant_id
             WHERE i.token_digest=$1 AND i.revoked_at IS NULL AND (
                 (i.expires_at>$2 AND i.accepted_at IS NULL) OR
                 (i.accepted_by=$3 AND EXISTS (SELECT FROM tenant_memberships m
                    WHERE (m.tenant_id,m.account_id)=(i.tenant_id,i.accepted_by) AND m.status='active'))
             )")
            .bind(digest).bind(now).bind(account).fetch_optional(&self.0).await
    }

    pub(crate) async fn join_organization_invitation(
        &self,
        tenant: Uuid,
        digest: &[u8],
        now: i64,
        account: Uuid,
        registration: Option<(&NewAccount, &NewSession)>,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        lock_tenant(&mut tx, tenant).await?;
        let invitation: Option<(Uuid, String, i64, Option<Uuid>)> = sqlx::query_as(
            "SELECT id,email,expires_at,accepted_by FROM organization_invitations
                WHERE tenant_id=$1 AND token_digest=$2 AND revoked_at IS NULL FOR UPDATE",
        )
        .bind(tenant)
        .bind(digest)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((id, address, expires_at, accepted_by)) = invitation else {
            return Ok(false);
        };
        if accepted_by.is_some() {
            return Ok(accepted_by == Some(account)
                && member_role(&mut tx, tenant, account).await?.is_some());
        }
        if expires_at <= now {
            return Ok(false);
        }
        if let Some((new_account, session)) = registration {
            if new_account.id != account
                || new_account.email != address
                || !insert_account(&mut tx, new_account).await?
            {
                return Ok(false);
            }
            insert_session(&mut tx, account, session).await?;
        } else {
            let email: Option<String> =
                sqlx::query_scalar("SELECT email FROM accounts WHERE id=$1 FOR NO KEY UPDATE")
                    .bind(account)
                    .fetch_optional(&mut *tx)
                    .await?;
            if email.as_deref() != Some(address.as_str()) {
                return Ok(false);
            }
        }
        sqlx::query("UPDATE tenant_memberships SET is_current=false WHERE account_id=$1 AND is_current AND tenant_id<>$2")
            .bind(account).bind(tenant).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO tenant_memberships(tenant_id,account_id,role,status,can_create,is_current)
            VALUES($1,$2,'member','active',false,true)
            ON CONFLICT(tenant_id,account_id) DO UPDATE
            SET status='active',is_current=true,role=CASE WHEN tenant_memberships.status='active' THEN tenant_memberships.role ELSE 'member' END,
                can_create=CASE WHEN tenant_memberships.status='active' THEN tenant_memberships.can_create ELSE false END")
            .bind(tenant).bind(account).execute(&mut *tx).await?;
        sqlx::query(
            "UPDATE organization_invitations SET accepted_by=$2,accepted_at=$3 WHERE id=$1",
        )
        .bind(id)
        .bind(account)
        .bind(now)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(true)
    }
}
