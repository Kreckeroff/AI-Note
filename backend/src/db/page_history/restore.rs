use sqlx::PgConnection;
use uuid::Uuid;

use super::{
    PageAddress, PageCover, RestorePageRevision, Store, ensure_baseline, record_current_snapshot,
};
use crate::db::{
    page_access::{PageEditAccess, lock_page_for_edit},
    page_attachments::restore_snapshot_attachments,
    page_cover::update_page_cover,
};

impl Store {
    pub(crate) async fn restore_page_revision(
        &self,
        address: &PageAddress,
        source_revision: i64,
        base_revision: i64,
    ) -> Result<RestorePageRevision, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        if !matches!(
            lock_page_for_edit(&mut tx, address).await?,
            PageEditAccess::Allowed
        ) {
            return Ok(RestorePageRevision::Missing);
        }
        let replay: Option<i64> = sqlx::query_scalar(
            "SELECT restored_revision FROM page_revision_restores
             WHERE (tenant_id, space_id, page_id, account_id, base_revision, source_revision)
                 = ($1, $2, $3, $4, $5, $6)",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(address.account)
        .bind(base_revision)
        .bind(source_revision)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(restored) = replay {
            return Ok(RestorePageRevision::Saved(restored));
        }

        type RestoreState = (
            i64,
            Option<String>,
            Option<Uuid>,
            Option<String>,
            Option<Uuid>,
        );
        let state: Option<RestoreState> = sqlx::query_as(
            "SELECT p.content_revision, c.theme, c.asset_id, r.cover_theme, r.cover_asset_id
             FROM pages p
             JOIN page_revisions r
                 ON (r.tenant_id, r.space_id, r.page_id, r.revision) = ($1, $2, $3, $4)
                 AND r.revision < p.content_revision
             LEFT JOIN page_covers c ON (c.tenant_id, c.space_id, c.page_id) = ($1, $2, $3)
             WHERE (p.tenant_id, p.space_id, p.id) = ($1, $2, $3)",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(source_revision)
        .fetch_optional(&mut *tx)
        .await?;
        let Some((current, current_theme, current_asset, archived_theme, archived_asset)) = state
        else {
            return Ok(RestorePageRevision::Missing);
        };
        if current != base_revision {
            return Ok(RestorePageRevision::Conflict(current));
        }

        ensure_baseline(&mut tx, address).await?;
        let restored = sqlx::query_scalar(
            "UPDATE pages p
             SET title = r.title, title_key = r.title_key, markdown = r.markdown,
                 content_is_markdown = r.content_is_markdown, content_revision = p.content_revision + 1
             FROM page_revisions r
             WHERE (p.tenant_id, p.space_id, p.id) = ($1, $2, $3)
               AND (r.tenant_id, r.space_id, r.page_id, r.revision) = ($1, $2, $3, $4)
             RETURNING p.content_revision",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(source_revision)
        .fetch_one(&mut *tx)
        .await;
        let revision: i64 = match restored {
            Err(error)
                if error
                    .as_database_error()
                    .is_some_and(|error| error.is_unique_violation()) =>
            {
                return Ok(RestorePageRevision::NameTaken);
            }
            result => result?,
        };

        let previous = PageCover::from_storage(current_theme.as_deref(), current_asset);
        let restored_cover = match archived_asset {
            Some(asset) => PageCover::Image(clone_historical_cover(&mut tx, address, asset).await?),
            None => PageCover::from_storage(archived_theme.as_deref(), None),
        };
        if previous != restored_cover
            && !update_page_cover(&mut tx, address, previous, restored_cover).await?
        {
            return Err(sqlx::Error::RowNotFound);
        }
        restore_snapshot_attachments(&mut tx, address, source_revision).await?;
        record_current_snapshot(
            &mut tx,
            address,
            Some(address.account),
            Some(source_revision),
        )
        .await?;
        sqlx::query(
            "INSERT INTO page_revision_restores
                 (tenant_id, space_id, page_id, account_id, base_revision, source_revision, restored_revision)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(address.account)
        .bind(base_revision)
        .bind(source_revision)
        .bind(revision)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(RestorePageRevision::Saved(revision))
    }
}

async fn clone_historical_cover(
    tx: &mut PgConnection,
    address: &PageAddress,
    archived_asset: Uuid,
) -> Result<Uuid, sqlx::Error> {
    // Copy the normalized JPEG under a new live ID. A previously retired URL
    // must not start resolving even when the original image is restored.
    sqlx::query_scalar(
        "INSERT INTO page_cover_assets (tenant_id, space_id, page_id, id, uploader_account_id, jpeg)
         SELECT tenant_id, space_id, page_id, $5, $6, jpeg
         FROM page_revision_covers
         WHERE (tenant_id, space_id, page_id, asset_id) = ($1, $2, $3, $4)
         RETURNING id",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(archived_asset)
    .bind(Uuid::new_v4())
    .bind(address.account)
    .fetch_one(tx)
    .await
}
