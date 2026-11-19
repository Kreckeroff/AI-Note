use std::collections::HashSet;

use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::{
    markdown::{attachment_reference_ids, contains_attachment_reference},
    text::{markdown_to_text, normalize_line_endings, text_to_markdown},
};

use super::{
    Attachment, AttachmentPatch, PageContentFormat, Store,
    page_access::{PageAddress, PageEditAccess, lock_page_for_edit},
    page_attachments::{
        CURRENT_ATTACHMENTS, apply_attachment_patch, decode_attachments, has_retired_attachments,
    },
    page_cover::{CoverUpdate, PageCover, update_page_cover},
    page_history::{ensure_baseline, record_current_snapshot},
    sections::SPACE_ACCESS,
};

pub(crate) struct PageText {
    pub source: String,
    pub format: PageContentFormat,
    pub content_revision: i64,
    pub can_edit: bool,
    pub cover: PageCover,
    pub attachments: Vec<Attachment>,
}

pub(crate) enum PageUpdate<'a> {
    Keep,
    PlainText(&'a str),
    Source(&'a str, PageContentFormat),
}

impl PageUpdate<'_> {
    fn matches(&self, current: &PageText) -> bool {
        match self {
            Self::Keep => true,
            Self::PlainText(text) => {
                current.format == PageContentFormat::Literal
                    && markdown_to_text(&current.source) == *text
            }
            Self::Source(source, _) => normalize_line_endings(&current.source) == *source,
        }
    }
}

pub(crate) enum SavePageText {
    Missing,
    Forbidden,
    Conflict(PageText),
    InvalidCover,
    InvalidAttachments,
    AttachmentReferencesPresent,
    AttachmentRetired,
    Saved,
}

struct PageSaveReceipt {
    base_revision: i64,
    fingerprint: [u8; 32],
}

impl PageSaveReceipt {
    fn new(
        base_revision: i64,
        update: &PageUpdate<'_>,
        cover: CoverUpdate,
        attachments: Option<AttachmentPatch<'_>>,
    ) -> Self {
        let mut hash = Sha256::new();
        if let Some(patch) = attachments {
            hash.update(b"page-attachments-save-v1\0");
            hash.update(patch.draft.as_bytes());
            for list in [patch.add, patch.remove] {
                let mut ids = list.to_vec();
                ids.sort_unstable();
                hash.update([ids.len() as u8]);
                for id in ids {
                    hash.update(id.as_bytes());
                }
            }
        } else {
            hash.update(b"page-cover-save-v1\0");
        }
        match cover {
            CoverUpdate::Keep => hash.update(b"keep"),
            CoverUpdate::Set(cover) => hash.update(cover.value()),
        }
        hash.update(b"\0");
        match update {
            PageUpdate::Keep => hash.update(b"keep"),
            PageUpdate::PlainText(text) => {
                hash.update(b"plain\0");
                hash.update(text);
            }
            PageUpdate::Source(source, format) => {
                hash.update(b"source\0");
                hash.update(format.as_str());
                hash.update(b"\0");
                hash.update(source);
            }
        }
        Self {
            base_revision,
            fingerprint: hash.finalize().into(),
        }
    }

    async fn exists(
        &self,
        tx: &mut PgConnection,
        address: &PageAddress,
    ) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM page_cover_save_receipts
             WHERE (tenant_id, space_id, page_id, account_id, base_revision, fingerprint)
                 = ($1, $2, $3, $4, $5, $6))",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(address.account)
        .bind(self.base_revision)
        .bind(&self.fingerprint[..])
        .fetch_one(tx)
        .await
    }

    async fn record(self, tx: &mut PgConnection, address: &PageAddress) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO page_cover_save_receipts
                 (tenant_id, space_id, page_id, account_id, base_revision, fingerprint)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(address.account)
        .bind(self.base_revision)
        .bind(&self.fingerprint[..])
        .execute(tx)
        .await?;
        Ok(())
    }
}

#[derive(FromRow)]
struct PageTextRow {
    source: String,
    is_markdown: bool,
    content_revision: i64,
    can_edit: bool,
    theme: Option<String>,
    asset_id: Option<Uuid>,
    attachments: String,
}

async fn read(
    connection: &mut PgConnection,
    address: &PageAddress,
) -> Result<Option<PageText>, sqlx::Error> {
    let query = format!(
        "SELECT p.markdown AS source, p.content_is_markdown AS is_markdown, p.content_revision,
                access.can_create AS can_edit, c.theme, c.asset_id, {CURRENT_ATTACHMENTS} AS attachments
         FROM ({SPACE_ACCESS}) access
         JOIN pages p ON (p.tenant_id, p.space_id, p.id) = ($1, $2, $4)
         LEFT JOIN page_covers c ON (c.tenant_id, c.space_id, c.page_id) = ($1, $2, $4)"
    );
    let row: Option<PageTextRow> = sqlx::query_as(&query)
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.account)
        .bind(address.page)
        .fetch_optional(connection)
        .await?;
    row.map(|row| {
        Ok(PageText {
            source: row.source,
            format: PageContentFormat::from_markdown(row.is_markdown),
            content_revision: row.content_revision,
            can_edit: row.can_edit,
            cover: PageCover::from_storage(row.theme.as_deref(), row.asset_id),
            attachments: decode_attachments(&row.attachments)?,
        })
    })
    .transpose()
}

impl Store {
    pub(crate) async fn page_text(
        &self,
        address: &PageAddress,
    ) -> Result<Option<PageText>, sqlx::Error> {
        read(&mut *self.0.acquire().await?, address).await
    }

    pub(crate) async fn save_page(
        &self,
        address: &PageAddress,
        base_revision: i64,
        update: PageUpdate<'_>,
        cover: CoverUpdate,
    ) -> Result<SavePageText, sqlx::Error> {
        self.save_page_with_attachments(address, base_revision, update, cover, None)
            .await
    }

    pub(crate) async fn save_page_with_attachments(
        &self,
        address: &PageAddress,
        base_revision: i64,
        update: PageUpdate<'_>,
        cover: CoverUpdate,
        attachments: Option<AttachmentPatch<'_>>,
    ) -> Result<SavePageText, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        match lock_page_for_edit(&mut tx, address).await? {
            PageEditAccess::Missing => return Ok(SavePageText::Missing),
            PageEditAccess::Forbidden => return Ok(SavePageText::Forbidden),
            PageEditAccess::Allowed => {}
        }
        if attachments.is_some_and(|patch| !patch.is_valid()) {
            return Ok(SavePageText::InvalidAttachments);
        }
        let attachments = attachments.filter(|patch| !patch.is_empty());
        let receipt = match (&update, cover, attachments) {
            (PageUpdate::PlainText(_), CoverUpdate::Keep, None) => None,
            _ => Some(PageSaveReceipt::new(
                base_revision,
                &update,
                cover,
                attachments,
            )),
        };
        if let Some(receipt) = &receipt
            && receipt.exists(&mut tx, address).await?
        {
            // A later revision may have retired the original image. The
            // receipt confirms this actor's earlier commit without reusing it.
            return Ok(SavePageText::Saved);
        }
        // Read after acquiring the page lock so a waited-on save's companion
        // cover row and new revision come from the same statement snapshot.
        let Some(current) = read(&mut tx, address).await? else {
            return Ok(SavePageText::Missing);
        };
        let selected = match cover {
            CoverUpdate::Keep => current.cover,
            CoverUpdate::Set(cover) => cover,
        };
        let text_matches = update.matches(&current);
        let attachment_change = attachments.filter(|patch| patch.changes(&current.attachments));
        if text_matches && selected == current.cover && attachment_change.is_none() {
            // A current-base save can acknowledge the exact request without
            // incrementing the revision. A stale no-op must not claim a write.
            if let Some(receipt) = receipt
                && current.content_revision == base_revision
            {
                receipt.record(&mut tx, address).await?;
                tx.commit().await?;
            }
            return Ok(SavePageText::Saved);
        }
        if current.content_revision != base_revision {
            return Ok(SavePageText::Conflict(current));
        }
        let (source, format) = match update {
            PageUpdate::PlainText(text) if !text_matches => (
                std::borrow::Cow::Owned(text_to_markdown(text)),
                PageContentFormat::Literal,
            ),
            PageUpdate::Source(source, format) if !text_matches => {
                (std::borrow::Cow::Borrowed(source), format)
            }
            _ => (
                std::borrow::Cow::Borrowed(current.source.as_str()),
                current.format,
            ),
        };
        if let Some(patch) = attachment_change
            && format == PageContentFormat::Markdown
        {
            let removed: Vec<_> = patch
                .remove
                .iter()
                .copied()
                .filter(|id| current.attachments.iter().any(|asset| asset.id == *id))
                .collect();
            if contains_attachment_reference(&source, &removed) {
                return Ok(SavePageText::AttachmentReferencesPresent);
            }
        }
        if !text_matches && format == PageContentFormat::Markdown {
            let mut new_references = attachment_reference_ids(&source);
            new_references.retain(|id| !current.attachments.iter().any(|asset| asset.id == *id));
            if !new_references.is_empty() && current.format == PageContentFormat::Markdown {
                let previous: HashSet<_> = attachment_reference_ids(&current.source)
                    .into_iter()
                    .collect();
                new_references.retain(|id| !previous.contains(id));
            }
            if has_retired_attachments(&mut tx, address, &new_references).await? {
                return Ok(SavePageText::AttachmentRetired);
            }
        }
        ensure_baseline(&mut tx, address).await?;
        if let Some(patch) = attachment_change
            && !apply_attachment_patch(&mut tx, address, &current.attachments, patch).await?
        {
            return Ok(SavePageText::InvalidAttachments);
        }
        if selected != current.cover
            && !update_page_cover(&mut tx, address, current.cover, selected).await?
        {
            return Ok(SavePageText::InvalidCover);
        }
        sqlx::query(
            "UPDATE pages SET markdown = $1, content_is_markdown = $2, content_revision = content_revision + 1
             WHERE tenant_id = $3 AND space_id = $4 AND id = $5",
        )
        .bind(source.as_ref())
        .bind(format == PageContentFormat::Markdown)
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .execute(&mut *tx)
        .await?;
        record_current_snapshot(
            &mut tx,
            address,
            Some(address.account),
            (selected == current.cover).then_some(base_revision),
        )
        .await?;
        if let Some(receipt) = receipt {
            receipt.record(&mut tx, address).await?;
        }
        tx.commit().await?;
        Ok(SavePageText::Saved)
    }
}
