use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use super::{
    CoeditResult, DraftConflict, DraftSnapshot, PageAddress, PageContentFormat, invalid_journal,
    patch::{Conflict, Splice, fragment},
};
use crate::text::normalize_line_endings;

const RETAINED_OPERATIONS: i64 = 256;

#[derive(FromRow)]
pub(super) struct DraftRow {
    pub(super) version: i64,
    pub(super) source: String,
    is_markdown: bool,
    pub(super) published_revision: i64,
    pub(super) published_source: String,
    published_is_markdown: bool,
    pub(super) conflict_start: Option<i64>,
    conflict_end: Option<i64>,
    conflict_mine: Option<String>,
}

impl DraftRow {
    pub(super) fn format(&self) -> PageContentFormat {
        PageContentFormat::from_markdown(self.is_markdown)
    }

    pub(super) fn published_format(&self) -> PageContentFormat {
        PageContentFormat::from_markdown(self.published_is_markdown)
    }

    pub(super) fn conflict(&self) -> Result<Option<Conflict>, sqlx::Error> {
        match (
            self.conflict_start,
            self.conflict_end,
            self.conflict_mine.as_ref(),
        ) {
            (Some(start), Some(end), Some(mine)) if start >= 0 && end >= start => {
                Ok(Some(Conflict {
                    range: start as usize..end as usize,
                    mine: mine.clone(),
                }))
            }
            (None, None, None) => Ok(None),
            _ => Err(invalid_journal()),
        }
    }

    pub(super) fn snapshot(&self, lease: Uuid) -> DraftSnapshot {
        DraftSnapshot {
            source: self.source.clone(),
            format: self.format(),
            version: self.version,
            published_revision: self.published_revision,
            lease,
            applied: None,
            from_version: None,
            steps: None,
            acknowledged_version: None,
            restore_conflict: None,
        }
    }

    pub(super) fn state(
        &self,
        snapshot: DraftSnapshot,
    ) -> Result<CoeditResult<DraftSnapshot>, sqlx::Error> {
        if let Some(conflict) = self.conflict()? {
            self.conflict_state(snapshot, conflict, true)
        } else {
            Ok(CoeditResult::Saved(snapshot))
        }
    }

    pub(super) fn recovery(
        &self,
        snapshot: DraftSnapshot,
    ) -> Result<CoeditResult<DraftSnapshot>, sqlx::Error> {
        if let Some(conflict) = self.conflict()? {
            self.conflict_state(snapshot, conflict, true)
        } else {
            Ok(CoeditResult::BaseExpired(snapshot))
        }
    }

    pub(super) fn conflict_state(
        &self,
        snapshot: DraftSnapshot,
        conflict: Conflict,
        persistent: bool,
    ) -> Result<CoeditResult<DraftSnapshot>, sqlx::Error> {
        let theirs = fragment(&self.source, &conflict.range).ok_or_else(invalid_journal)?;
        Ok(CoeditResult::Conflict(DraftConflict {
            snapshot,
            start: conflict.range.start,
            end: conflict.range.end,
            mine: conflict.mine,
            theirs: theirs.to_owned(),
            persistent,
        }))
    }
}

pub(super) async fn load_draft(
    tx: &mut PgConnection,
    address: &PageAddress,
) -> Result<Option<DraftRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT version,source,is_markdown,published_revision,published_source,published_is_markdown,
                conflict_start,conflict_end,conflict_mine
         FROM page_coedit_drafts
         WHERE (tenant_id,space_id,page_id)=($1,$2,$3)",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .fetch_optional(tx)
    .await
}

pub(super) async fn open_draft(
    tx: &mut PgConnection,
    address: &PageAddress,
) -> Result<DraftRow, sqlx::Error> {
    if let Some(row) = load_draft(tx, address).await? {
        return Ok(row);
    }
    let (source, is_markdown, revision): (String, bool, i64) = sqlx::query_as(
        "SELECT markdown,content_is_markdown,content_revision FROM pages
         WHERE (tenant_id,space_id,id)=($1,$2,$3)",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .fetch_one(&mut *tx)
    .await?;
    let source = normalize_line_endings(&source);
    sqlx::query_as(
        "INSERT INTO page_coedit_drafts
            (tenant_id,space_id,page_id,source,is_markdown,published_revision,published_source,published_is_markdown)
         VALUES ($1,$2,$3,$4,$5,$6,$4,$5)
         RETURNING version,source,is_markdown,published_revision,published_source,published_is_markdown,
                   conflict_start,conflict_end,conflict_mine",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(source.as_ref())
    .bind(is_markdown)
    .bind(revision)
    .fetch_one(tx)
    .await
}

#[derive(FromRow)]
pub(super) struct JournalRow {
    pub(super) version: i64,
    start: i64,
    pub(super) removed: String,
    pub(super) inserted: String,
    before_markdown: bool,
    before_published_revision: i64,
}

impl JournalRow {
    fn splice_with(&self, length: usize, text: String) -> Result<Splice, sqlx::Error> {
        let start = usize::try_from(self.start).map_err(|_| invalid_journal())?;
        let end = start.checked_add(length).ok_or_else(invalid_journal)?;
        Ok(Splice {
            range: start..end,
            text,
        })
    }

    pub(super) fn splice(&self) -> Result<Splice, sqlx::Error> {
        self.splice_with(self.removed.chars().count(), self.inserted.clone())
    }

    fn inverse(&self) -> Result<Splice, sqlx::Error> {
        self.splice_with(self.inserted.chars().count(), self.removed.clone())
    }
}

pub(super) struct ReplayPoint {
    pub(super) source: String,
    pub(super) format: PageContentFormat,
    pub(super) published_revision: i64,
}

pub(super) fn restore_point<'a>(
    draft: &DraftRow,
    journal: impl DoubleEndedIterator<Item = &'a JournalRow>,
) -> Result<ReplayPoint, sqlx::Error> {
    let mut point = ReplayPoint {
        source: draft.source.clone(),
        format: draft.format(),
        published_revision: draft.published_revision,
    };
    for row in journal.rev() {
        if row.removed != row.inserted {
            point.source = row
                .inverse()?
                .apply(&point.source)
                .ok_or_else(invalid_journal)?;
        }
        point.format = PageContentFormat::from_markdown(row.before_markdown);
        point.published_revision = row.before_published_revision;
    }
    Ok(point)
}

pub(super) async fn load_journal(
    tx: &mut PgConnection,
    address: &PageAddress,
    after: i64,
) -> Result<Vec<JournalRow>, sqlx::Error> {
    sqlx::query_as(
        "SELECT version,start,removed,inserted,before_markdown,before_published_revision
         FROM page_coedit_operations
         WHERE (tenant_id,space_id,page_id)=($1,$2,$3) AND version>$4
         ORDER BY version",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(after)
    .fetch_all(tx)
    .await
}

#[derive(FromRow)]
pub(super) struct Receipt {
    pub(super) version: i64,
    pub(super) fingerprint: Vec<u8>,
    pub(super) outcome: String,
}

pub(super) async fn load_receipt(
    tx: &mut PgConnection,
    address: &PageAddress,
    operation: Uuid,
) -> Result<Option<Receipt>, sqlx::Error> {
    sqlx::query_as(
        "SELECT version,fingerprint,outcome FROM page_coedit_receipts
         WHERE (tenant_id,space_id,page_id,account_id,operation_id)=($1,$2,$3,$4,$5)",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(address.account)
    .bind(operation)
    .fetch_optional(tx)
    .await
}

pub(super) async fn record_receipt(
    tx: &mut PgConnection,
    address: &PageAddress,
    operation: Uuid,
    fingerprint: &[u8],
    version: i64,
    outcome: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO page_coedit_receipts
            (tenant_id,space_id,page_id,account_id,operation_id,fingerprint,version,outcome)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(address.account)
    .bind(operation)
    .bind(fingerprint)
    .bind(version)
    .bind(outcome)
    .execute(tx)
    .await?;
    Ok(())
}

pub(super) struct DraftWrite<'a> {
    pub(super) source: &'a str,
    pub(super) format: PageContentFormat,
    pub(super) published_revision: i64,
    pub(super) published_source: &'a str,
    pub(super) published_format: PageContentFormat,
    pub(super) conflict: Option<&'a Conflict>,
}

pub(super) async fn record_change(
    tx: &mut PgConnection,
    address: &PageAddress,
    before: &DraftRow,
    after: &DraftWrite<'_>,
    applied: &Splice,
) -> Result<i64, sqlx::Error> {
    let version = before.version.checked_add(1).ok_or_else(invalid_journal)?;
    let removed = fragment(&before.source, &applied.range).ok_or_else(invalid_journal)?;
    sqlx::query(
        "INSERT INTO page_coedit_operations
            (tenant_id,space_id,page_id,version,start,removed,inserted,before_markdown,before_published_revision)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(version)
    .bind(applied.range.start as i64)
    .bind(removed)
    .bind(&applied.text)
    .bind(before.is_markdown)
    .bind(before.published_revision)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE page_coedit_drafts
         SET version=$4,source=$5,is_markdown=$6,published_revision=$7,published_source=$8,
             published_is_markdown=$9,conflict_start=$10,conflict_end=$11,conflict_mine=$12
         WHERE (tenant_id,space_id,page_id)=($1,$2,$3)",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(version)
    .bind(after.source)
    .bind(after.format == PageContentFormat::Markdown)
    .bind(after.published_revision)
    .bind(after.published_source)
    .bind(after.published_format == PageContentFormat::Markdown)
    .bind(after.conflict.map(|conflict| conflict.range.start as i64))
    .bind(after.conflict.map(|conflict| conflict.range.end as i64))
    .bind(after.conflict.map(|conflict| conflict.mine.as_str()))
    .execute(&mut *tx)
    .await?;
    if version > RETAINED_OPERATIONS {
        sqlx::query(
            "DELETE FROM page_coedit_operations
             WHERE (tenant_id,space_id,page_id)=($1,$2,$3) AND version<=$4",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(version - RETAINED_OPERATIONS)
        .execute(tx)
        .await?;
    }
    Ok(version)
}
