use std::collections::HashSet;

use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use crate::{
    markdown::{
        attachment_reference_ids, contains_attachment_reference, without_attachment_references,
    },
    text::{markdown_to_text, normalize_line_endings, text_to_markdown},
};

use super::{
    Attachment, AttachmentPatch, PageContentFormat, Store,
    coediting::{CoeditResult, current_coedit_draft, guard_coedit_publish, validate_coedit_lease},
    page_access::{PAGE_ACCESS, PageAddress, PageEditAccess, lock_page_for_edit},
    page_attachments::{
        CURRENT_ATTACHMENTS, apply_attachment_patch, decode_attachments, has_retired_attachments,
    },
    page_cover::{CoverUpdate, PageCover, update_page_cover},
    page_history::{ensure_baseline, record_page_publication},
};

pub(crate) struct PageText {
    pub source: String,
    pub format: PageContentFormat,
    pub content_revision: i64,
    pub can_edit: bool,
    pub can_manage: bool,
    pub can_browse_space: bool,
    pub effective_role: String,
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
    CoeditConflict(String, PageContentFormat),
    CoeditLeaseExpired,
    CoeditOperationConflict,
    InvalidCover,
    InvalidAttachments,
    AttachmentReferencesPresent,
    AttachmentRetired,
    Saved,
}

pub(crate) struct CoeditPublication {
    pub(crate) session: [u8; 32],
    pub(crate) lease: Uuid,
    pub(crate) version: i64,
    pub(crate) operation: Uuid,
    pub(crate) cover_base: Option<PageCover>,
}

struct CoeditPublishReceipt {
    operation: Uuid,
    version: i64,
    fingerprint: [u8; 32],
}

impl CoeditPublishReceipt {
    fn new(
        permit: &CoeditPublication,
        base_revision: i64,
        update: &PageUpdate<'_>,
        cover: CoverUpdate,
        attachments: Option<AttachmentPatch<'_>>,
    ) -> Self {
        let content = PageSaveReceipt::new(base_revision, update, cover, attachments);
        let mut hash = Sha256::new();
        hash.update(b"coedit-publish-v1");
        hash.update(base_revision.to_be_bytes());
        hash.update(permit.version.to_be_bytes());
        hash.update(permit.lease.as_bytes());
        hash.update(content.fingerprint);
        if let (CoverUpdate::Set(_), Some(base)) = (cover, permit.cover_base) {
            hash.update(base.value());
        }
        Self {
            operation: permit.operation,
            version: permit.version,
            fingerprint: hash.finalize().into(),
        }
    }

    async fn previous(
        &self,
        tx: &mut PgConnection,
        address: &PageAddress,
    ) -> Result<Option<bool>, sqlx::Error> {
        let row: Option<(String, Vec<u8>)> = sqlx::query_as(
            "SELECT outcome, fingerprint FROM page_coedit_receipts
             WHERE (tenant_id, space_id, page_id, account_id, operation_id) = ($1, $2, $3, $4, $5)",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(address.account)
        .bind(self.operation)
        .fetch_optional(tx)
        .await?;
        Ok(row.map(|(outcome, fingerprint)| {
            outcome == "published" && fingerprint == self.fingerprint
        }))
    }

    async fn record(self, tx: &mut PgConnection, address: &PageAddress) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO page_coedit_receipts
                 (tenant_id, space_id, page_id, account_id, operation_id, fingerprint, version, outcome)
             VALUES ($1, $2, $3, $4, $5, $6, $7, 'published')",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(address.account)
        .bind(self.operation)
        .bind(&self.fingerprint[..])
        .bind(self.version)
        .execute(tx)
        .await?;
        Ok(())
    }
}

fn includes_confirmed_draft(
    update: &PageUpdate<'_>,
    draft: &str,
    format: PageContentFormat,
    attachments: Option<AttachmentPatch<'_>>,
) -> bool {
    match update {
        PageUpdate::Source(source, submitted_format) => {
            let staged = attachments.map_or(&[][..], |patch| patch.add);
            let stripped = if *submitted_format == PageContentFormat::Markdown {
                without_attachment_references(source, staged)
            } else {
                std::borrow::Cow::Borrowed(*source)
            };
            // Adding inline media can promote a legacy page to Markdown.
            // The escaped literal source stays byte-identical in that case.
            stripped == draft && (*submitted_format == format || stripped != *source)
        }
        PageUpdate::PlainText(text) => {
            format == PageContentFormat::Literal && text_to_markdown(text) == draft
        }
        PageUpdate::Keep => false,
    }
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
    can_manage: bool,
    can_browse_space: bool,
    effective_role: String,
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
                access.can_edit, access.can_manage, access.can_browse_space, access.effective_role,
                c.theme, c.asset_id, {CURRENT_ATTACHMENTS} AS attachments
         FROM ({PAGE_ACCESS}) access
         JOIN pages p ON (p.tenant_id, p.space_id, p.id) = ($1, $2, $4)
         LEFT JOIN page_covers c ON (c.tenant_id, c.space_id, c.page_id) = ($1, $2, $4)
         WHERE access.can_read"
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
            can_manage: row.can_manage,
            can_browse_space: row.can_browse_space,
            effective_role: row.effective_role,
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
        self.save_page_submission(address, base_revision, update, cover, attachments, None)
            .await
    }

    pub(crate) async fn publish_coedit(
        &self,
        address: &PageAddress,
        base_revision: i64,
        update: PageUpdate<'_>,
        cover: CoverUpdate,
        attachments: Option<AttachmentPatch<'_>>,
        publication: CoeditPublication,
    ) -> Result<SavePageText, sqlx::Error> {
        self.save_page_submission(
            address,
            base_revision,
            update,
            cover,
            attachments,
            Some(publication),
        )
        .await
    }

    async fn save_page_submission(
        &self,
        address: &PageAddress,
        base_revision: i64,
        update: PageUpdate<'_>,
        cover: CoverUpdate,
        attachments: Option<AttachmentPatch<'_>>,
        publication: Option<CoeditPublication>,
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
        let coedit_receipt = if let Some(permit) = &publication {
            let receipt =
                CoeditPublishReceipt::new(permit, base_revision, &update, cover, attachments);
            if let Some(matches) = receipt.previous(&mut tx, address).await? {
                if !validate_coedit_lease(&mut tx, address, &permit.session, permit.lease).await? {
                    return Ok(SavePageText::CoeditLeaseExpired);
                }
                return Ok(if matches {
                    SavePageText::Saved
                } else {
                    SavePageText::CoeditOperationConflict
                });
            }
            let draft = match guard_coedit_publish(
                &mut tx,
                address,
                &permit.session,
                permit.lease,
                permit.version,
            )
            .await?
            {
                CoeditResult::Saved(draft) => draft,
                CoeditResult::Conflict(conflict) => {
                    return Ok(SavePageText::CoeditConflict(
                        conflict.snapshot.source,
                        conflict.snapshot.format,
                    ));
                }
                CoeditResult::BaseExpired(draft) => {
                    return Ok(SavePageText::CoeditConflict(draft.source, draft.format));
                }
                CoeditResult::Missing => return Ok(SavePageText::Missing),
                CoeditResult::Forbidden => return Ok(SavePageText::Forbidden),
                CoeditResult::LeaseExpired => return Ok(SavePageText::CoeditLeaseExpired),
                CoeditResult::Invalid | CoeditResult::OperationConflict => {
                    return Ok(SavePageText::CoeditOperationConflict);
                }
            };
            if !includes_confirmed_draft(&update, &draft.source, draft.format, attachments) {
                return Ok(SavePageText::CoeditConflict(draft.source, draft.format));
            }
            Some(receipt)
        } else {
            None
        };

        let receipt = match (&update, cover, attachments) {
            _ if publication.is_some() => None,
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
        // Live text may advance the revision shown by an editor while that
        // editor's cover selection still refers to an earlier publication.
        if let (Some(permit), CoverUpdate::Set(_)) = (&publication, cover)
            && selected != current.cover
        {
            let Some(base) = permit.cover_base else {
                return Ok(SavePageText::InvalidCover);
            };
            if base != current.cover {
                return Ok(SavePageText::Conflict(current));
            }
        }
        // Legacy forms have always treated unchanged source as a no-op even
        // when their format field changed. A guarded draft controls both.
        let shared_format_changed = publication.is_some()
            && matches!(&update, PageUpdate::Source(_, format) if *format != current.format);
        let text_matches = !shared_format_changed && update.matches(&current);
        let attachment_change = attachments.filter(|patch| patch.changes(&current.attachments));
        if text_matches && selected == current.cover && attachment_change.is_none() {
            // A current-base save can acknowledge the exact request without
            // incrementing the revision. A stale no-op must not claim a write.
            if current.content_revision == base_revision {
                if let Some(receipt) = receipt {
                    receipt.record(&mut tx, address).await?;
                }
                if let Some(receipt) = coedit_receipt {
                    receipt.record(&mut tx, address).await?;
                }
                tx.commit().await?;
            } else if publication.is_some() {
                return Ok(SavePageText::Conflict(current));
            }
            return Ok(SavePageText::Saved);
        }
        if current.content_revision != base_revision {
            return Ok(SavePageText::Conflict(current));
        }
        let from_shared = if publication.is_some() {
            true
        } else if let Some((source, format, has_conflict)) =
            current_coedit_draft(&mut tx, address).await?
        {
            let matches = includes_confirmed_draft(&update, &source, format, attachments);
            if has_conflict || !matches && (source != current.source || format != current.format) {
                return Ok(SavePageText::CoeditConflict(source, format));
            }
            matches
        } else {
            false
        };
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
        record_page_publication(
            &mut tx,
            address,
            (selected == current.cover).then_some(base_revision),
            from_shared,
        )
        .await?;
        if let Some(receipt) = receipt {
            receipt.record(&mut tx, address).await?;
        }
        if let Some(receipt) = coedit_receipt {
            receipt.record(&mut tx, address).await?;
        }
        tx.commit().await?;
        Ok(SavePageText::Saved)
    }
}
