use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

use crate::text::{markdown_to_text, normalize_line_endings, text_to_markdown};

use super::{
    Store,
    sections::{SPACE_ACCESS, lock_active_member},
};

pub(crate) struct PageAddress {
    pub tenant: Uuid,
    pub space: Uuid,
    pub page: Uuid,
    pub account: Uuid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PageContentFormat {
    Literal,
    Markdown,
}

impl PageContentFormat {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Literal => "literal",
            Self::Markdown => "markdown",
        }
    }
}

pub(crate) struct PageText {
    pub source: String,
    pub format: PageContentFormat,
    pub content_revision: i64,
    pub can_edit: bool,
}

pub(crate) enum PageUpdate<'a> {
    PlainText(&'a str),
    Source(&'a str, PageContentFormat),
}

impl PageUpdate<'_> {
    fn matches(&self, current: &PageText) -> bool {
        match self {
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
    Saved,
}

async fn read(
    connection: &mut PgConnection,
    address: &PageAddress,
    for_update: bool,
) -> Result<Option<PageText>, sqlx::Error> {
    let lock = if for_update {
        " FOR NO KEY UPDATE OF p"
    } else {
        ""
    };
    let query = format!(
        "SELECT p.markdown, p.content_is_markdown, p.content_revision, access.can_create AS can_edit
         FROM ({SPACE_ACCESS}) access
         JOIN pages p ON p.tenant_id = $1 AND p.space_id = $2 AND p.id = $4{lock}"
    );
    let row: Option<(String, bool, i64, bool)> = sqlx::query_as(&query)
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.account)
        .bind(address.page)
        .fetch_optional(connection)
        .await?;
    Ok(row.map(
        |(source, is_markdown, content_revision, can_edit)| PageText {
            source,
            format: if is_markdown {
                PageContentFormat::Markdown
            } else {
                PageContentFormat::Literal
            },
            content_revision,
            can_edit,
        },
    ))
}

impl Store {
    pub(crate) async fn page_text(
        &self,
        address: &PageAddress,
    ) -> Result<Option<PageText>, sqlx::Error> {
        read(&mut *self.0.acquire().await?, address, false).await
    }

    pub(crate) async fn save_page_text(
        &self,
        address: &PageAddress,
        base_revision: i64,
        update: PageUpdate<'_>,
    ) -> Result<SavePageText, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        sqlx::query("SET LOCAL statement_timeout = '3s'")
            .execute(&mut *tx)
            .await?;
        if !lock_active_member(&mut tx, address.tenant, address.account).await? {
            return Ok(SavePageText::Missing);
        }
        let Some(current) = read(&mut tx, address, true).await? else {
            return Ok(SavePageText::Missing);
        };
        if !current.can_edit {
            return Ok(SavePageText::Forbidden);
        }
        // A retry after a committed save and an unchanged legacy document both succeed.
        if update.matches(&current) {
            return Ok(SavePageText::Saved);
        }
        if current.content_revision != base_revision {
            return Ok(SavePageText::Conflict(current));
        }
        let (source, format) = match update {
            PageUpdate::PlainText(text) => (
                std::borrow::Cow::Owned(text_to_markdown(text)),
                PageContentFormat::Literal,
            ),
            PageUpdate::Source(source, format) => (std::borrow::Cow::Borrowed(source), format),
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
        tx.commit().await?;
        Ok(SavePageText::Saved)
    }
}
