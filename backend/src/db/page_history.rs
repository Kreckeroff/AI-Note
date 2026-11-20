use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use super::{
    Attachment, PageAddress, PageContentFormat, PageCover, Store,
    coediting::record_publication,
    page_access::PAGE_ACCESS,
    page_attachments::{REVISION_ATTACHMENTS, decode_attachments, record_snapshot_attachments},
    page_watch::notify_page_publication,
    sections::decode_title,
};

mod restore;
#[cfg(test)]
mod tests;

const PAGE_SIZE: usize = 20;
// All read paths use the same effective edit permission and the same page scope.
fn scoped(statement: &str) -> String {
    format!(
        "WITH editable_page AS (
             SELECT p.content_revision FROM ({PAGE_ACCESS}) access
             JOIN pages p ON (p.tenant_id, p.space_id, p.id) = ($1, $2, $4)
             WHERE access.can_edit
         )
         {statement}"
    )
}

pub(crate) struct PageRevisionSummary {
    pub revision: i64,
    pub author: Option<String>,
    pub recorded_at: Option<String>,
}

pub(crate) struct PageHistory {
    pub current_revision: i64,
    pub items: Vec<PageRevisionSummary>,
    pub next_before: Option<i64>,
}

#[derive(FromRow)]
struct TimelineRow {
    content_revision: i64,
    revision: Option<i64>,
    author_name: Option<String>,
    recorded_at: Option<String>,
}

pub(crate) struct PageRevisionSnapshot {
    pub current_revision: i64,
    pub revision: i64,
    pub author: Option<String>,
    pub recorded_at: Option<String>,
    pub title: String,
    pub source: String,
    pub format: PageContentFormat,
    pub cover: PageCover,
    pub attachments: Vec<Attachment>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RestorePageRevision {
    Missing,
    Conflict(i64),
    NameTaken,
    Saved(i64),
}

impl Store {
    pub(crate) async fn page_history(
        &self,
        address: &PageAddress,
        before: Option<i64>,
    ) -> Result<Option<PageHistory>, sqlx::Error> {
        let query = scoped(
            r#"SELECT p.content_revision, entry.revision, entry.author_name,
                      to_char(entry.recorded_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS recorded_at
               FROM editable_page p
               LEFT JOIN LATERAL (
                   (SELECT r.revision, r.author_name, r.recorded_at
                    FROM page_revisions r
                    WHERE (r.tenant_id, r.space_id, r.page_id) = ($1, $2, $4)
                      AND r.revision <= CASE
                          WHEN $5::bigint IS NULL THEN p.content_revision
                          WHEN $5 > 0 THEN $5 - 1
                          ELSE 0
                      END
                    ORDER BY r.revision DESC LIMIT $6)
                   UNION ALL
                   SELECT p.content_revision, NULL, NULL
                   WHERE ($5::bigint IS NULL OR p.content_revision < $5)
                     AND NOT EXISTS (
                        SELECT 1 FROM page_revisions r
                        WHERE (r.tenant_id, r.space_id, r.page_id, r.revision)
                            = ($1, $2, $4, p.content_revision)
                     )
                   ORDER BY revision DESC LIMIT $6
               ) AS entry ON true
               ORDER BY entry.revision DESC"#,
        );
        let rows: Vec<TimelineRow> = sqlx::query_as(&query)
            .bind(address.tenant)
            .bind(address.space)
            .bind(address.account)
            .bind(address.page)
            .bind(before)
            .bind((PAGE_SIZE + 1) as i64)
            .fetch_all(&self.0)
            .await?;
        let Some(current_revision) = rows.first().map(|row| row.content_revision) else {
            return Ok(None);
        };
        let mut items: Vec<_> = rows
            .into_iter()
            .filter_map(|row| {
                Some(PageRevisionSummary {
                    revision: row.revision?,
                    author: row.author_name,
                    recorded_at: row.recorded_at,
                })
            })
            .collect();
        let has_more = items.len() > PAGE_SIZE;
        items.truncate(PAGE_SIZE);
        let next_before = if has_more {
            items.last().map(|row| row.revision)
        } else {
            None
        };
        Ok(Some(PageHistory {
            current_revision,
            items,
            next_before,
        }))
    }

    pub(crate) async fn page_revision(
        &self,
        address: &PageAddress,
        revision: i64,
    ) -> Result<Option<PageRevisionSnapshot>, sqlx::Error> {
        let mut rows = read_snapshots(&self.0, address, &[revision]).await?;
        Ok(rows.pop())
    }

    pub(crate) async fn compare_page_revisions(
        &self,
        address: &PageAddress,
        from: i64,
        to: i64,
    ) -> Result<Option<(PageRevisionSnapshot, PageRevisionSnapshot)>, sqlx::Error> {
        let mut rows = read_snapshots(&self.0, address, &[from, to])
            .await?
            .into_iter();
        Ok(rows.next().zip(rows.next()))
    }

    pub(crate) async fn page_revision_cover(
        &self,
        address: &PageAddress,
        revision: i64,
    ) -> Result<Option<Vec<u8>>, sqlx::Error> {
        let query = scoped(
            "SELECT coalesce(archived.jpeg, live.jpeg)
             FROM editable_page p
             LEFT JOIN page_revisions r
                 ON (r.tenant_id, r.space_id, r.page_id, r.revision) = ($1, $2, $4, $5)
             LEFT JOIN page_revision_covers archived
                 ON (archived.tenant_id, archived.space_id, archived.page_id, archived.asset_id)
                     = ($1, $2, $4, r.cover_asset_id)
             LEFT JOIN page_covers c
                 ON (c.tenant_id, c.space_id, c.page_id) = ($1, $2, $4)
                 AND p.content_revision = $5 AND r.revision IS NULL
             LEFT JOIN page_cover_assets live
                 ON (live.tenant_id, live.space_id, live.page_id, live.id)
                     = ($1, $2, $4, c.asset_id) AND live.published_at IS NOT NULL",
        );
        let row: Option<Option<Vec<u8>>> = sqlx::query_scalar(&query)
            .bind(address.tenant)
            .bind(address.space)
            .bind(address.account)
            .bind(address.page)
            .bind(revision)
            .fetch_optional(&self.0)
            .await?;
        Ok(row.flatten())
    }
}

#[derive(FromRow)]
struct SnapshotRow {
    current_revision: i64,
    revision: i64,
    author: Option<String>,
    recorded_at: Option<String>,
    title: Vec<u8>,
    source: String,
    is_markdown: bool,
    cover_theme: Option<String>,
    cover_asset_id: Option<Uuid>,
    attachments: String,
}

async fn read_snapshots(
    pool: &sqlx::PgPool,
    address: &PageAddress,
    requested: &[i64],
) -> Result<Vec<PageRevisionSnapshot>, sqlx::Error> {
    // Both sides of a comparison come from the same statement snapshot, including
    // when the existing live page has not materialized its baseline yet.
    let query = scoped(&format!(
        r#"SELECT p.content_revision AS current_revision, request.revision,
                  r.author_name AS author,
                  to_char(r.recorded_at AT TIME ZONE 'UTC', 'YYYY-MM-DD"T"HH24:MI:SS.MS"Z"') AS recorded_at,
                  coalesce(r.title, live_page.title) AS title,
                  coalesce(r.markdown, live_page.markdown) AS source,
                  coalesce(r.content_is_markdown, live_page.content_is_markdown) AS is_markdown,
                  CASE WHEN r.revision IS NULL THEN c.theme ELSE r.cover_theme END AS cover_theme,
                  CASE WHEN r.revision IS NULL THEN c.asset_id ELSE r.cover_asset_id END AS cover_asset_id,
                  {REVISION_ATTACHMENTS} AS attachments
           FROM editable_page p
           JOIN pages live_page ON (live_page.tenant_id, live_page.space_id, live_page.id) = ($1, $2, $4)
           CROSS JOIN unnest($5::bigint[]) WITH ORDINALITY AS request(revision, ord)
           LEFT JOIN page_revisions r
               ON (r.tenant_id, r.space_id, r.page_id, r.revision) = ($1, $2, $4, request.revision)
           LEFT JOIN page_covers c
               ON (c.tenant_id, c.space_id, c.page_id) = ($1, $2, $4)
               AND p.content_revision = request.revision AND r.revision IS NULL
           WHERE r.revision IS NOT NULL OR request.revision = p.content_revision
           ORDER BY request.ord"#,
    ));
    let rows: Vec<SnapshotRow> = sqlx::query_as(&query)
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.account)
        .bind(address.page)
        .bind(requested)
        .fetch_all(pool)
        .await?;
    rows.into_iter()
        .map(|row| {
            Ok(PageRevisionSnapshot {
                revision: row.revision,
                current_revision: row.current_revision,
                author: row.author,
                recorded_at: row.recorded_at,
                title: decode_title(row.title, "page_revisions.title")?,
                source: row.source,
                format: PageContentFormat::from_markdown(row.is_markdown),
                cover: PageCover::from_storage(row.cover_theme.as_deref(), row.cover_asset_id),
                attachments: decode_attachments(&row.attachments)?,
            })
        })
        .collect()
}

// Callers already hold the page mutation lock or have just inserted the page
// on this transaction. Keep the snapshot in the same transaction as publication.
pub(super) async fn ensure_baseline(
    tx: &mut PgConnection,
    address: &PageAddress,
) -> Result<(), sqlx::Error> {
    let has_snapshot: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM pages p JOIN page_revisions r
                 ON (r.tenant_id, r.space_id, r.page_id, r.revision)
                     = (p.tenant_id, p.space_id, p.id, p.content_revision)
             WHERE (p.tenant_id, p.space_id, p.id) = ($1, $2, $3)
         )",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .fetch_one(&mut *tx)
    .await?;
    if !has_snapshot {
        record_current_snapshot(tx, address, None, None).await?;
    }
    Ok(())
}

pub(super) async fn record_current_snapshot(
    tx: &mut PgConnection,
    address: &PageAddress,
    author: Option<Uuid>,
    reuse_cover_from: Option<i64>,
) -> Result<(), sqlx::Error> {
    record_snapshot(tx, address, author, reuse_cover_from).await?;
    Ok(())
}

pub(super) async fn record_page_publication(
    tx: &mut PgConnection,
    address: &PageAddress,
    reuse_cover_from: Option<i64>,
    from_shared: bool,
) -> Result<(), sqlx::Error> {
    let (revision, source, is_markdown) =
        record_snapshot(tx, address, Some(address.account), reuse_cover_from).await?;
    let format = PageContentFormat::from_markdown(is_markdown);
    record_publication(tx, address, &source, format, revision, from_shared).await?;
    notify_page_publication(tx, address, revision).await
}

async fn record_snapshot(
    tx: &mut PgConnection,
    address: &PageAddress,
    author: Option<Uuid>,
    reuse_cover_from: Option<i64>,
) -> Result<(i64, String, bool), sqlx::Error> {
    if reuse_cover_from.is_none() {
        sqlx::query(
            "INSERT INTO page_revision_covers (tenant_id, space_id, page_id, asset_id, jpeg)
             SELECT a.tenant_id, a.space_id, a.page_id, a.id, a.jpeg
             FROM page_covers c JOIN page_cover_assets a
                 ON (a.tenant_id, a.space_id, a.page_id, a.id)
                     = (c.tenant_id, c.space_id, c.page_id, c.asset_id)
             WHERE (c.tenant_id, c.space_id, c.page_id) = ($1, $2, $3)
               AND a.published_at IS NOT NULL
             ON CONFLICT (tenant_id, space_id, page_id, asset_id) DO NOTHING",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .execute(&mut *tx)
        .await?;
    }
    let snapshot = sqlx::query_as(
        "INSERT INTO page_revisions
             (tenant_id, space_id, page_id, revision, title, title_key, markdown,
              content_is_markdown, cover_theme, cover_asset_id, author_account_id, author_name, recorded_at)
         SELECT p.tenant_id, p.space_id, p.id, p.content_revision, p.title, p.title_key,
                p.markdown, p.content_is_markdown, c.theme,
                CASE WHEN $5::bigint IS NULL THEN c.asset_id ELSE previous.cover_asset_id END,
                a.id, a.name, CASE WHEN a.id IS NOT NULL THEN clock_timestamp() END
         FROM pages p
         LEFT JOIN page_covers c ON (c.tenant_id, c.space_id, c.page_id) = ($1, $2, $3)
         LEFT JOIN page_revisions previous
            ON (previous.tenant_id, previous.space_id, previous.page_id, previous.revision) = ($1, $2, $3, $5)
         LEFT JOIN accounts a ON a.id = $4
         WHERE (p.tenant_id, p.space_id, p.id) = ($1, $2, $3)
           AND ($4::uuid IS NULL OR a.id IS NOT NULL)
           AND ($5 IS NULL OR previous.revision IS NOT NULL)
         RETURNING revision, markdown, content_is_markdown",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(author)
    .bind(reuse_cover_from)
    .fetch_one(&mut *tx)
    .await?;
    record_snapshot_attachments(tx, address).await?;
    Ok(snapshot)
}
