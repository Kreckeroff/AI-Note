use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgPool};
use tokio::time::{MissedTickBehavior, interval, timeout};
use uuid::Uuid;

use super::{
    Store,
    page_access::{PageAddress, PageEditAccess, lock_page_for_edit},
    sections::SPACE_ACCESS,
};

mod publication;

pub(super) use publication::{
    apply_attachment_patch, has_retired_attachments, record_snapshot_attachments,
    restore_snapshot_attachments,
};

const MAX_FILE_BYTES: i64 = 20 * 1024 * 1024;
const MAX_PAGE_BYTES: i64 = 100 * 1024 * 1024;
const MAX_PAGE_FILES: usize = 20;
const MAX_STAGED_FILES: i64 = 40;
const MAX_STAGED_BYTES: i64 = 200 * 1024 * 1024;
const MAX_EDITOR_STAGED_FILES: i64 = 3 * MAX_STAGED_FILES;
const MAX_EDITOR_STAGED_BYTES: i64 = 3 * MAX_STAGED_BYTES;

const VALID_DRAFT: &str = "asset.uploader_account_id = $3 AND asset.draft_id = $5";
const ACTIVE_STAGE: &str = "asset.published_at IS NULL
    AND asset.staged_at > statement_timestamp() - INTERVAL '24 hours'";

#[derive(Clone, Debug, Deserialize, FromRow, PartialEq, Eq)]
pub(crate) struct Attachment {
    pub id: Uuid,
    pub name: String,
    pub size: i64,
    pub image: bool,
    pub publication: Option<Uuid>,
}

#[derive(FromRow)]
pub(crate) struct AttachmentBody {
    pub name: String,
    pub bytes: Vec<u8>,
    pub image_type: Option<String>,
}

#[derive(Clone, Copy)]
pub(crate) enum AttachmentLookup {
    Published(Uuid),
    Draft { draft: Uuid, id: Uuid },
    Revision { revision: i64, id: Uuid },
}

#[derive(Clone, Copy)]
pub(crate) struct AttachmentPatch<'a> {
    pub draft: Uuid,
    pub add: &'a [Uuid],
    pub remove: &'a [Uuid],
}

impl AttachmentPatch<'_> {
    pub(super) fn is_valid(self) -> bool {
        self.add.len() <= MAX_PAGE_FILES
            && self.remove.len() <= MAX_PAGE_FILES
            && self
                .add
                .iter()
                .enumerate()
                .all(|(index, id)| !self.remove.contains(id) && !self.add[..index].contains(id))
            && self
                .remove
                .iter()
                .enumerate()
                .all(|(index, id)| !self.remove[..index].contains(id))
    }

    pub(super) fn is_empty(self) -> bool {
        self.add.is_empty() && self.remove.is_empty()
    }

    pub(super) fn changes(self, current: &[Attachment]) -> bool {
        self.add
            .iter()
            .any(|id| current.iter().all(|asset| asset.id != *id))
            || self
                .remove
                .iter()
                .any(|id| current.iter().any(|asset| asset.id == *id))
    }
}

pub(crate) enum StageAttachment {
    Saved(Attachment),
    Missing,
    Forbidden,
    Unavailable,
    Limit,
}

#[derive(FromRow)]
struct StagedUpload {
    id: Uuid,
    name: String,
    size: i64,
    image: bool,
    matches_request: bool,
}

#[derive(FromRow)]
struct StagedUsage {
    draft_files: i64,
    draft_bytes: i64,
    editor_files: i64,
    editor_bytes: i64,
}

impl Store {
    pub(crate) async fn sweep_attachment_drafts(&self, period: Duration) -> ! {
        let mut ticks = interval(period);
        ticks.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            ticks.tick().await;
            match timeout(
                Duration::from_millis(250),
                prune_expired_attachments(&self.0),
            )
            .await
            {
                Ok(Ok(())) => {}
                Ok(Err(_)) => eprintln!("page.attachments.cleanup outcome=database-error"),
                Err(_) => eprintln!("page.attachments.cleanup outcome=timed-out"),
            }
        }
    }

    pub(crate) async fn stage_attachment(
        &self,
        address: &PageAddress,
        draft: Uuid,
        id: Uuid,
        name: &str,
        image_type: Option<&str>,
        bytes: &[u8],
    ) -> Result<StageAttachment, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        match lock_page_for_edit(&mut tx, address).await? {
            PageEditAccess::Missing => return Ok(StageAttachment::Missing),
            PageEditAccess::Forbidden => return Ok(StageAttachment::Forbidden),
            PageEditAccess::Allowed => {}
        }
        let size = bytes.len() as i64;
        if !(1..=MAX_FILE_BYTES).contains(&size) {
            return Ok(StageAttachment::Limit);
        }
        let fingerprint = Sha256::digest(bytes);
        let load_previous = format!(
            "SELECT id, name, byte_length AS size, image_type IS NOT NULL AS image,
                    (uploader_account_id = $3 AND draft_id = $5 AND name = $7
                     AND image_type IS NOT DISTINCT FROM $8 AND fingerprint = $9
                     AND (published_at IS NOT NULL OR ({ACTIVE_STAGE})))
                    AS matches_request
             FROM page_attachment_assets asset
             WHERE (tenant_id, space_id, page_id, id) = ($1, $2, $4, $6)",
        );
        let previous: Option<StagedUpload> = sqlx::query_as(&load_previous)
            .bind(address.tenant)
            .bind(address.space)
            .bind(address.account)
            .bind(address.page)
            .bind(draft)
            .bind(id)
            .bind(name)
            .bind(image_type)
            .bind(&fingerprint[..])
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(previous) = previous {
            return Ok(if previous.matches_request {
                StageAttachment::Saved(Attachment {
                    id: previous.id,
                    name: previous.name,
                    size: previous.size,
                    image: previous.image,
                    publication: None,
                })
            } else {
                StageAttachment::Unavailable
            });
        }
        let usage = format!(
            "SELECT count(*) FILTER (WHERE asset.draft_id = $5) AS draft_files,
                    coalesce(sum(asset.byte_length) FILTER (WHERE asset.draft_id = $5), 0)::bigint AS draft_bytes,
                    count(*) AS editor_files, coalesce(sum(asset.byte_length), 0)::bigint AS editor_bytes
             FROM page_attachment_assets asset
             WHERE (asset.tenant_id, asset.space_id, asset.page_id) = ($1, $2, $4)
               AND asset.uploader_account_id = $3 AND {ACTIVE_STAGE}",
        );
        let usage: StagedUsage = sqlx::query_as(&usage)
            .bind(address.tenant)
            .bind(address.space)
            .bind(address.account)
            .bind(address.page)
            .bind(draft)
            .fetch_one(&mut *tx)
            .await?;
        // Keep previously staged files available to Undo through the draft.
        // Publication separately applies the stricter limit to the final set.
        if usage.draft_files >= MAX_STAGED_FILES
            || usage.draft_bytes + size > MAX_STAGED_BYTES
            || usage.editor_files >= MAX_EDITOR_STAGED_FILES
            || usage.editor_bytes + size > MAX_EDITOR_STAGED_BYTES
        {
            return Ok(StageAttachment::Limit);
        }
        sqlx::query(
            "INSERT INTO page_attachment_assets
                (tenant_id, space_id, page_id, id, uploader_account_id, draft_id, name,
                 byte_length, image_type, fingerprint, bytes)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(id)
        .bind(address.account)
        .bind(draft)
        .bind(name)
        .bind(size)
        .bind(image_type)
        .bind(&fingerprint[..])
        .bind(bytes)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(StageAttachment::Saved(Attachment {
            id,
            name: name.to_owned(),
            size,
            image: image_type.is_some(),
            publication: None,
        }))
    }

    pub(crate) async fn attachment_body(
        &self,
        address: &PageAddress,
        lookup: AttachmentLookup,
    ) -> Result<Option<AttachmentBody>, sqlx::Error> {
        match lookup {
            AttachmentLookup::Published(publication) => {
                let query = format!(
                    "SELECT asset.name, asset.bytes, asset.image_type FROM ({SPACE_ACCESS}) access
                     JOIN page_attachments live
                        ON (live.tenant_id, live.space_id, live.page_id, live.publication) = ($1, $2, $4, $5)
                     JOIN page_attachment_assets asset
                        ON (asset.tenant_id, asset.space_id, asset.page_id, asset.id)
                         = (live.tenant_id, live.space_id, live.page_id, live.asset_id)",
                );
                sqlx::query_as(&query)
                    .bind(address.tenant)
                    .bind(address.space)
                    .bind(address.account)
                    .bind(address.page)
                    .bind(publication)
                    .fetch_optional(&self.0)
                    .await
            }
            AttachmentLookup::Draft { draft, id } => {
                let query = format!(
                    "SELECT asset.name, asset.bytes, asset.image_type FROM ({SPACE_ACCESS}) access
                     JOIN page_attachment_assets asset
                        ON (asset.tenant_id, asset.space_id, asset.page_id, asset.id) = ($1, $2, $4, $6)
                     WHERE access.can_create AND {VALID_DRAFT} AND {ACTIVE_STAGE}",
                );
                sqlx::query_as(&query)
                    .bind(address.tenant)
                    .bind(address.space)
                    .bind(address.account)
                    .bind(address.page)
                    .bind(draft)
                    .bind(id)
                    .fetch_optional(&self.0)
                    .await
            }
            AttachmentLookup::Revision { revision, id } => {
                let query = format!(
                    "SELECT asset.name, asset.bytes, asset.image_type FROM ({SPACE_ACCESS}) access
                     JOIN page_revision_attachments snapshot ON
                        (snapshot.tenant_id, snapshot.space_id, snapshot.page_id, snapshot.revision, snapshot.asset_id)
                         = ($1, $2, $4, $5, $6)
                     JOIN page_attachment_assets asset ON
                        (asset.tenant_id, asset.space_id, asset.page_id, asset.id)
                         = (snapshot.tenant_id, snapshot.space_id, snapshot.page_id, snapshot.asset_id)
                     WHERE access.can_create",
                );
                sqlx::query_as(&query)
                    .bind(address.tenant)
                    .bind(address.space)
                    .bind(address.account)
                    .bind(address.page)
                    .bind(revision)
                    .bind(id)
                    .fetch_optional(&self.0)
                    .await
            }
        }
    }

    pub(crate) async fn draft_attachments(
        &self,
        address: &PageAddress,
        draft: Uuid,
        ids: &[Uuid],
    ) -> Result<Vec<Attachment>, sqlx::Error> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let query = format!(
            "SELECT asset.id, asset.name, asset.byte_length AS size, asset.image_type IS NOT NULL AS image,
                    NULL::uuid AS publication FROM ({SPACE_ACCESS}) access
             JOIN page_attachment_assets asset ON
                (asset.tenant_id, asset.space_id, asset.page_id) = ($1, $2, $4)
             WHERE access.can_create AND {VALID_DRAFT} AND {ACTIVE_STAGE} AND asset.id = ANY($6)
             ORDER BY asset.staged_at, asset.id",
        );
        sqlx::query_as(&query)
            .bind(address.tenant)
            .bind(address.space)
            .bind(address.account)
            .bind(address.page)
            .bind(draft)
            .bind(ids)
            .fetch_all(&self.0)
            .await
    }
}

pub(super) const CURRENT_ATTACHMENTS: &str =
    "(SELECT coalesce(jsonb_agg(jsonb_build_object(
                'id', asset.id, 'name', asset.name, 'size', asset.byte_length,
                'image', asset.image_type IS NOT NULL, 'publication', live.publication
            ) ORDER BY asset.staged_at, asset.id), '[]'::jsonb)::text
      FROM page_attachments live
      JOIN page_attachment_assets asset ON (asset.tenant_id, asset.space_id, asset.page_id, asset.id)
         = (live.tenant_id, live.space_id, live.page_id, live.asset_id)
      WHERE (live.tenant_id, live.space_id, live.page_id) = ($1, $2, $4))";

pub(super) const REVISION_ATTACHMENTS: &str =
    "(SELECT coalesce(jsonb_agg(jsonb_build_object(
                'id', asset.id, 'name', asset.name, 'size', asset.byte_length,
                'image', asset.image_type IS NOT NULL, 'publication', NULL::uuid
            ) ORDER BY asset.staged_at, asset.id), '[]'::jsonb)::text
      FROM page_revision_attachments snapshot
      JOIN page_attachment_assets asset ON (asset.tenant_id, asset.space_id, asset.page_id, asset.id)
         = (snapshot.tenant_id, snapshot.space_id, snapshot.page_id, snapshot.asset_id)
      WHERE (snapshot.tenant_id, snapshot.space_id, snapshot.page_id, snapshot.revision)
         = ($1, $2, $4, r.revision))";

pub(super) fn decode_attachments(json: &str) -> Result<Vec<Attachment>, sqlx::Error> {
    serde_json::from_str(json).map_err(|source| sqlx::Error::ColumnDecode {
        index: "attachments".to_owned(),
        source: Box::new(source),
    })
}

async fn prune_expired_attachments(pool: &PgPool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET LOCAL statement_timeout = '150ms'")
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "WITH expired AS (
            SELECT tenant_id, space_id, page_id, id FROM page_attachment_assets
            WHERE published_at IS NULL AND staged_at <= statement_timestamp() - INTERVAL '24 hours'
            ORDER BY staged_at LIMIT 32 FOR UPDATE SKIP LOCKED
         )
         DELETE FROM page_attachment_assets asset USING expired
         WHERE (asset.tenant_id, asset.space_id, asset.page_id, asset.id)
             = (expired.tenant_id, expired.space_id, expired.page_id, expired.id)",
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}
