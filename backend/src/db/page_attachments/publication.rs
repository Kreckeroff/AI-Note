use sqlx::PgConnection;
use uuid::Uuid;

use super::{
    ACTIVE_STAGE, Attachment, AttachmentPatch, MAX_PAGE_BYTES, MAX_PAGE_FILES, PageAddress,
    VALID_DRAFT,
};

pub(in crate::db) async fn has_retired_attachments(
    tx: &mut PgConnection,
    address: &PageAddress,
    ids: &[Uuid],
) -> Result<bool, sqlx::Error> {
    if ids.is_empty() {
        return Ok(false);
    }
    sqlx::query_scalar(
        "SELECT EXISTS(
            SELECT 1 FROM page_attachment_assets asset
            WHERE (asset.tenant_id, asset.space_id, asset.page_id) = ($1, $2, $3)
              AND asset.id = ANY($4) AND asset.published_at IS NOT NULL
              AND NOT EXISTS (
                  SELECT 1 FROM page_attachments live
                  WHERE (live.tenant_id, live.space_id, live.page_id, live.asset_id)
                      = (asset.tenant_id, asset.space_id, asset.page_id, asset.id)
              )
         )",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(ids)
    .fetch_one(tx)
    .await
}

// The caller holds the page writer lock. Lock claimed assets through the
// update so that expiry cleanup cannot remove a validated draft mid-save.
pub(in crate::db) async fn apply_attachment_patch(
    tx: &mut PgConnection,
    address: &PageAddress,
    current: &[Attachment],
    patch: AttachmentPatch<'_>,
) -> Result<bool, sqlx::Error> {
    let kept: Vec<_> = current
        .iter()
        .filter(|asset| !patch.remove.contains(&asset.id))
        .collect();
    let added: Vec<_> = patch
        .add
        .iter()
        .copied()
        .filter(|id| current.iter().all(|asset| asset.id != *id))
        .collect();
    let mut bytes = kept.iter().map(|asset| asset.size).sum::<i64>();
    if kept.len() + added.len() > MAX_PAGE_FILES {
        return Ok(false);
    }
    if !added.is_empty() {
        let lookup = format!(
            "SELECT asset.byte_length FROM page_attachment_assets asset
             WHERE (asset.tenant_id, asset.space_id, asset.page_id) = ($1, $2, $4)
               AND {VALID_DRAFT} AND {ACTIVE_STAGE} AND asset.id = ANY($6) FOR UPDATE",
        );
        let sizes: Vec<i64> = sqlx::query_scalar(&lookup)
            .bind(address.tenant)
            .bind(address.space)
            .bind(address.account)
            .bind(address.page)
            .bind(patch.draft)
            .bind(&added)
            .fetch_all(&mut *tx)
            .await?;
        if sizes.len() != added.len() {
            return Ok(false);
        }
        bytes += sizes.into_iter().sum::<i64>();
    }
    if bytes > MAX_PAGE_BYTES {
        return Ok(false);
    }

    if kept.len() < current.len() {
        sqlx::query(
            "DELETE FROM page_attachments
             WHERE (tenant_id, space_id, page_id) = ($1, $2, $3) AND asset_id = ANY($4)",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(patch.remove)
        .execute(&mut *tx)
        .await?;
    }
    if !added.is_empty() {
        sqlx::query(
            "WITH claimed AS (
                UPDATE page_attachment_assets SET published_at = clock_timestamp()
                WHERE (tenant_id, space_id, page_id) = ($1, $2, $3) AND id = ANY($4)
                RETURNING tenant_id, space_id, page_id, id
             )
             INSERT INTO page_attachments (tenant_id, space_id, page_id, asset_id)
             SELECT tenant_id, space_id, page_id, id FROM claimed",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(&added)
        .execute(&mut *tx)
        .await?;
    }
    Ok(true)
}

pub(in crate::db) async fn record_snapshot_attachments(
    tx: &mut PgConnection,
    address: &PageAddress,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO page_revision_attachments (tenant_id, space_id, page_id, revision, asset_id)
         SELECT p.tenant_id, p.space_id, p.id, p.content_revision, live.asset_id
         FROM pages p JOIN page_attachments live
             ON (live.tenant_id, live.space_id, live.page_id) = (p.tenant_id, p.space_id, p.id)
         WHERE (p.tenant_id, p.space_id, p.id) = ($1, $2, $3)",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .execute(tx)
    .await?;
    Ok(())
}

pub(in crate::db) async fn restore_snapshot_attachments(
    tx: &mut PgConnection,
    address: &PageAddress,
    revision: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE FROM page_attachments live
         WHERE (live.tenant_id, live.space_id, live.page_id) = ($1, $2, $3)
           AND NOT EXISTS (
                SELECT 1 FROM page_revision_attachments old
                WHERE (old.tenant_id, old.space_id, old.page_id, old.revision, old.asset_id)
                    = ($1, $2, $3, $4, live.asset_id)
           )",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(revision)
    .execute(&mut *tx)
    .await?;
    // Keeping an existing association preserves its still-current URL. An
    // association coming back after removal gets a fresh publication token.
    sqlx::query(
        "INSERT INTO page_attachments (tenant_id, space_id, page_id, asset_id)
         SELECT old.tenant_id, old.space_id, old.page_id, old.asset_id
         FROM page_revision_attachments old
         WHERE (old.tenant_id, old.space_id, old.page_id, old.revision) = ($1, $2, $3, $4)
         ON CONFLICT (tenant_id, space_id, page_id, asset_id) DO NOTHING",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(revision)
    .execute(tx)
    .await?;
    Ok(())
}
