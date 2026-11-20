use sqlx::PgConnection;
use uuid::Uuid;

use crate::clock::now_ms;

use super::PageAddress;

pub(in crate::db) async fn coedit_access_generation(
    tx: &mut PgConnection,
    address: &PageAddress,
) -> Result<String, sqlx::Error> {
    sqlx::query_scalar("SELECT page_coedit_access_generation($1, $2, $3, $4)")
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(address.account)
        .fetch_one(tx)
        .await
}

pub(super) async fn lock_session(
    tx: &mut PgConnection,
    address: &PageAddress,
    session: &[u8],
) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM sessions WHERE token_digest=$1 AND account_id=$2 AND expires_at>$3 FOR SHARE",
    )
    .bind(session)
    .bind(address.account)
    .bind(now_ms())
    .fetch_optional(tx)
    .await?
    .is_some())
}

pub(in crate::db) async fn validate_coedit_lease(
    tx: &mut PgConnection,
    address: &PageAddress,
    session: &[u8],
    lease: Uuid,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM page_coedit_leases l
            JOIN sessions s ON s.token_digest=l.session_digest
            WHERE (l.tenant_id,l.space_id,l.page_id,l.session_digest,l.lease)=($1,$2,$3,$4,$5)
              AND s.account_id=$6 AND s.expires_at>$7
              AND l.generation=page_coedit_access_generation($1,$2,$3,$6)
        )",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(session)
    .bind(lease)
    .bind(address.account)
    .bind(now_ms())
    .fetch_one(tx)
    .await
}

pub(super) async fn issue_lease(
    tx: &mut PgConnection,
    address: &PageAddress,
    session: &[u8],
) -> Result<Uuid, sqlx::Error> {
    let generation = coedit_access_generation(tx, address).await?;
    sqlx::query_scalar(
        "INSERT INTO page_coedit_leases
             (tenant_id,space_id,page_id,session_digest,lease,generation)
         VALUES ($1,$2,$3,$4,$5,$6)
         ON CONFLICT (tenant_id,space_id,page_id,session_digest) DO UPDATE
         SET lease=CASE WHEN page_coedit_leases.generation=EXCLUDED.generation
                        THEN page_coedit_leases.lease ELSE EXCLUDED.lease END,
             generation=EXCLUDED.generation
         RETURNING lease",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(session)
    .bind(Uuid::new_v4())
    .bind(generation)
    .fetch_one(tx)
    .await
}
