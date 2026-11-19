use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::text::{markdown_to_text, normalize_line_endings, text_to_markdown};

use super::{
    PageContentFormat, Store,
    page_access::{PageAddress, PageEditAccess, lock_page_for_edit},
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
    Saved,
}

struct PageSaveReceipt {
    base_revision: i64,
    fingerprint: [u8; 32],
}

impl PageSaveReceipt {
    fn new(base_revision: i64, update: &PageUpdate<'_>, cover: CoverUpdate) -> Self {
        let mut hash = Sha256::new();
        hash.update(b"page-cover-save-v1\0");
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

async fn read(
    connection: &mut PgConnection,
    address: &PageAddress,
) -> Result<Option<PageText>, sqlx::Error> {
    let query = format!(
        "SELECT p.markdown, p.content_is_markdown, p.content_revision, access.can_create AS can_edit,
                c.theme, c.asset_id
         FROM ({SPACE_ACCESS}) access
         JOIN pages p ON (p.tenant_id, p.space_id, p.id) = ($1, $2, $4)
         LEFT JOIN page_covers c ON (c.tenant_id, c.space_id, c.page_id) = ($1, $2, $4)"
    );
    type PageTextRow = (String, bool, i64, bool, Option<String>, Option<Uuid>);
    let row: Option<PageTextRow> = sqlx::query_as(&query)
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.account)
        .bind(address.page)
        .fetch_optional(connection)
        .await?;
    Ok(row.map(
        |(source, is_markdown, content_revision, can_edit, theme, asset_id)| PageText {
            source,
            format: PageContentFormat::from_markdown(is_markdown),
            content_revision,
            can_edit,
            cover: PageCover::from_storage(theme.as_deref(), asset_id),
        },
    ))
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
        let mut tx = self.0.begin().await?;
        match lock_page_for_edit(&mut tx, address).await? {
            PageEditAccess::Missing => return Ok(SavePageText::Missing),
            PageEditAccess::Forbidden => return Ok(SavePageText::Forbidden),
            PageEditAccess::Allowed => {}
        }
        let receipt = match (&update, cover) {
            (PageUpdate::PlainText(_), CoverUpdate::Keep) => None,
            _ => Some(PageSaveReceipt::new(base_revision, &update, cover)),
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
        if text_matches && selected == current.cover {
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
        ensure_baseline(&mut tx, address).await?;
        if selected != current.cover
            && !update_page_cover(&mut tx, address, current.cover, selected).await?
        {
            return Ok(SavePageText::InvalidCover);
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
