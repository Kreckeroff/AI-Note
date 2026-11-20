use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use sqlx::PgConnection;
use uuid::Uuid;

use crate::{markdown::attachment_reference_ids, text::markdown_to_text};

use super::{
    PageAddress, PageContentFormat, Store,
    page_access::{PAGE_ACCESS, PagePermissions, lock_page_for_change},
};

mod lease;
mod patch;
mod publication;
mod storage;

#[cfg(test)]
mod tests;

use lease::{issue_lease, lock_session};
use patch::{Conflict, Splice, disjoint_conflict_range, fragment, project, project_conflict};
use storage::{
    DraftRow, JournalRow, load_draft, load_journal, load_receipt, record_change, record_receipt,
};

pub(super) use lease::validate_coedit_lease;
pub(super) use publication::{current_coedit_draft, guard_coedit_publish, record_publication};

const MAX_SHARED_SOURCE: usize = 60_000;
const MAX_LITERAL_TEXT: usize = 30_000;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DraftSnapshot {
    pub(crate) source: String,
    pub(crate) format: PageContentFormat,
    #[serde(serialize_with = "serialize_version")]
    pub(crate) version: i64,
    #[serde(serialize_with = "serialize_version")]
    pub(crate) published_revision: i64,
    pub(crate) lease: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) applied: Option<DraftSplice>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_optional_version"
    )]
    pub(crate) from_version: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) steps: Option<Vec<DraftSplice>>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "serialize_optional_version"
    )]
    pub(crate) acknowledged_version: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) restore_conflict: Option<RestoreConflict>,
}

impl DraftSnapshot {
    fn attach_restore_conflict(&mut self, conflict: &Conflict) -> Result<(), sqlx::Error> {
        self.restore_conflict = Some(RestoreConflict {
            start: conflict.range.start,
            end: conflict.range.end,
            mine: conflict.mine.clone(),
            theirs: fragment(&self.source, &conflict.range)
                .ok_or_else(invalid_journal)?
                .to_owned(),
        });
        Ok(())
    }

    fn attach_steps<'a>(
        &mut self,
        from: i64,
        journal: impl Iterator<Item = &'a JournalRow>,
    ) -> Result<(), sqlx::Error> {
        let steps = journal
            .map(|row| row.splice().map(Into::into))
            .collect::<Result<Vec<_>, _>>()?;
        self.from_version = Some(from);
        self.steps = Some(steps);
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct RestoreConflict {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) mine: String,
    pub(crate) theirs: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct DraftSplice {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) text: String,
}

impl From<Splice> for DraftSplice {
    fn from(value: Splice) -> Self {
        Self {
            start: value.range.start,
            end: value.range.end,
            text: value.text,
        }
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct DraftConflict {
    #[serde(flatten)]
    pub(crate) snapshot: DraftSnapshot,
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) mine: String,
    pub(crate) theirs: String,
    pub(crate) persistent: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DraftCommand {
    lease: Uuid,
    operation_id: Uuid,
    #[serde(deserialize_with = "deserialize_version")]
    version: i64,
    start: usize,
    end: usize,
    text: String,
    #[serde(deserialize_with = "deserialize_format")]
    format: PageContentFormat,
    #[serde(default)]
    resolve: bool,
}

impl DraftCommand {
    fn fingerprint(&self) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(b"page-coedit-splice-v1\0");
        hash.update(self.version.to_be_bytes());
        hash.update((self.start as u64).to_be_bytes());
        hash.update((self.end as u64).to_be_bytes());
        hash.update([
            self.resolve as u8,
            (self.format == PageContentFormat::Markdown) as u8,
        ]);
        hash.update(self.text.as_bytes());
        hash.finalize().into()
    }

    fn valid(&self) -> bool {
        self.start <= self.end
            && self.text.chars().count() <= MAX_SHARED_SOURCE
            && !self.text.contains(['\0', '\r'])
    }

    fn into_splice(self) -> Splice {
        Splice {
            range: self.start..self.end,
            text: self.text,
        }
    }
}

#[derive(Debug)]
pub(crate) enum CoeditResult<T> {
    Saved(T),
    Missing,
    Forbidden,
    LeaseExpired,
    Invalid,
    Conflict(DraftConflict),
    BaseExpired(DraftSnapshot),
    OperationConflict,
}

fn serialize_version<S: Serializer>(version: &i64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.collect_str(version)
}

fn serialize_optional_version<S: Serializer>(
    version: &Option<i64>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    version.map(|value| value.to_string()).serialize(serializer)
}

fn deserialize_version<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
    let raw = String::deserialize(deserializer)?;
    let result = if raw == "0" {
        Some(0)
    } else {
        crate::text::revision_number(&raw)
    };
    result.ok_or_else(|| serde::de::Error::custom("invalid draft version"))
}

fn deserialize_format<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<PageContentFormat, D::Error> {
    match String::deserialize(deserializer)?.as_str() {
        "literal" => Ok(PageContentFormat::Literal),
        "markdown" => Ok(PageContentFormat::Markdown),
        _ => Err(serde::de::Error::custom("invalid draft format")),
    }
}

fn access_error(permissions: Option<PagePermissions>) -> Option<CoeditResult<DraftSnapshot>> {
    match permissions {
        Some(permission) if permission.can_edit => None,
        Some(permission) if permission.can_read => Some(CoeditResult::Forbidden),
        _ => Some(CoeditResult::Missing),
    }
}

impl Store {
    pub(crate) async fn open_coedit(
        &self,
        address: &PageAddress,
        session: &[u8],
    ) -> Result<CoeditResult<DraftSnapshot>, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        if let Some(error) = access_error(lock_page_for_change(&mut tx, address).await?) {
            return Ok(error);
        }
        if !lock_session(&mut tx, address, session).await? {
            return Ok(CoeditResult::LeaseExpired);
        }
        let draft = storage::open_draft(&mut tx, address).await?;
        let lease = issue_lease(&mut tx, address, session).await?;
        let state = draft.state(draft.snapshot(lease))?;
        tx.commit().await?;
        Ok(state)
    }

    #[cfg(test)]
    pub(crate) async fn coedit_state(
        &self,
        address: &PageAddress,
        session: &[u8],
        lease: Uuid,
    ) -> Result<CoeditResult<DraftSnapshot>, sqlx::Error> {
        self.coedit_state_after(address, session, lease, None).await
    }

    pub(crate) async fn coedit_state_after(
        &self,
        address: &PageAddress,
        session: &[u8],
        lease: Uuid,
        after: Option<i64>,
    ) -> Result<CoeditResult<DraftSnapshot>, sqlx::Error> {
        // Read rights, lease generation and text from the same committed
        // snapshot without making frequent reads contend for the write lock.
        let mut tx = self.0.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        let permissions = sqlx::query_as(PAGE_ACCESS)
            .bind(address.tenant)
            .bind(address.space)
            .bind(address.account)
            .bind(address.page)
            .fetch_optional(&mut *tx)
            .await?;
        if let Some(error) = access_error(permissions) {
            return Ok(error);
        }
        if !validate_coedit_lease(&mut tx, address, session, lease).await? {
            return Ok(CoeditResult::LeaseExpired);
        }
        let draft = required_draft(&mut tx, address).await?;
        let mut snapshot = draft.snapshot(lease);
        let state = if let Some(after) = after {
            let journal = load_journal(&mut tx, address, after).await?;
            if covers(&journal, after, draft.version) {
                snapshot.attach_steps(after, journal.iter())?;
                draft.state(snapshot)?
            } else {
                draft.recovery(snapshot)?
            }
        } else {
            draft.state(snapshot)?
        };
        tx.commit().await?;
        Ok(state)
    }

    pub(crate) async fn apply_coedit(
        &self,
        address: &PageAddress,
        session: &[u8],
        input: DraftCommand,
    ) -> Result<CoeditResult<DraftSnapshot>, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        if let Some(error) = access_error(lock_page_for_change(&mut tx, address).await?) {
            return Ok(error);
        }
        if !lock_session(&mut tx, address, session).await?
            || !validate_coedit_lease(&mut tx, address, session, input.lease).await?
        {
            return Ok(CoeditResult::LeaseExpired);
        }
        let result = apply_locked(&mut tx, address, input).await?;
        tx.commit().await?;
        Ok(result)
    }
}

async fn required_draft(
    tx: &mut PgConnection,
    address: &PageAddress,
) -> Result<DraftRow, sqlx::Error> {
    load_draft(tx, address)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

async fn apply_locked(
    tx: &mut PgConnection,
    address: &PageAddress,
    input: DraftCommand,
) -> Result<CoeditResult<DraftSnapshot>, sqlx::Error> {
    let draft = required_draft(tx, address).await?;
    let fingerprint = input.fingerprint();
    let operation_id = input.operation_id;
    let prior = load_receipt(tx, address, operation_id).await?;
    if let Some(receipt) = prior.as_ref() {
        if receipt.fingerprint != fingerprint || receipt.outcome == "published" {
            return Ok(CoeditResult::OperationConflict);
        }
        if receipt.outcome != "conflict" {
            return replay_change(tx, address, &draft, input.lease, input.version, receipt).await;
        }
    }
    if !input.valid() {
        return Ok(CoeditResult::Invalid);
    }
    let in_range = input.start..input.end;
    if input.version > draft.version {
        return Ok(CoeditResult::Invalid);
    }
    let journal = load_journal(tx, address, input.version).await?;
    let mut snapshot = draft.snapshot(input.lease);
    if covers(&journal, input.version, draft.version) {
        snapshot.attach_steps(input.version, journal.iter())?;
    } else {
        return draft.recovery(snapshot);
    }
    let mut restore_conflict = draft.conflict()?;
    if input.resolve
        && let Some(conflict) = restore_conflict.as_ref()
        && (input.version != draft.version || in_range != conflict.range)
    {
        if prior.is_none() {
            record_receipt(
                tx,
                address,
                operation_id,
                &fingerprint,
                draft.version,
                "conflict",
            )
            .await?;
        }
        return draft.state(snapshot);
    }
    if input.resolve && input.version != draft.version {
        return Ok(CoeditResult::BaseExpired(snapshot));
    }

    let base = storage::restore_point(&draft, journal.iter())?;
    if fragment(&base.source, &in_range).is_none() {
        return Ok(CoeditResult::Invalid);
    }
    let format = if draft.format() == base.format {
        input.format
    } else {
        draft.format()
    };
    let clears_restore_conflict = input.resolve && restore_conflict.is_some();
    let authored = input.into_splice();
    let projected = rebase(authored.clone(), &journal, base.source)?;
    let intersects_restore = |range: &std::ops::Range<usize>| {
        restore_conflict
            .as_ref()
            .is_some_and(|active| disjoint_conflict_range(active, range, 0).is_none())
    };
    let applied = match projected {
        Ok(applied) if prior.is_none() => applied,
        // A previously declined command may only be applied by a new
        // explicit resolution command. Never reinterpret its receipt as a write.
        Ok(applied) => {
            if intersects_restore(&applied.range) {
                return draft.state(snapshot);
            }
            let conflict = Conflict {
                range: applied.range,
                mine: authored.text,
            };
            return draft.conflict_state(snapshot, conflict, false);
        }
        Err(conflict) => {
            if prior.is_none() {
                record_receipt(
                    tx,
                    address,
                    operation_id,
                    &fingerprint,
                    draft.version,
                    "conflict",
                )
                .await?;
            }
            if intersects_restore(&conflict.range) {
                return draft.state(snapshot);
            }
            return draft.conflict_state(snapshot, conflict, false);
        }
    };
    if clears_restore_conflict {
        restore_conflict = None;
    } else if let Some(conflict) = restore_conflict.as_mut() {
        let Some(range) =
            disjoint_conflict_range(conflict, &applied.range, applied.text.chars().count())
        else {
            record_receipt(
                tx,
                address,
                operation_id,
                &fingerprint,
                draft.version,
                "conflict",
            )
            .await?;
            return draft.state(snapshot);
        };
        conflict.range = range;
    }
    let source = applied.apply(&draft.source).ok_or_else(invalid_journal)?;
    if !valid_source(tx, address, &draft, &source, format).await? {
        return Ok(CoeditResult::Invalid);
    }
    if source != draft.source || format != draft.format() || clears_restore_conflict {
        let next = storage::DraftWrite {
            source: &source,
            format,
            published_revision: draft.published_revision,
            published_source: &draft.published_source,
            published_format: draft.published_format(),
            conflict: restore_conflict.as_ref(),
        };
        snapshot.version = record_change(tx, address, &draft, &next, &applied).await?;
        snapshot.source = source;
        snapshot.format = format;
        let own: DraftSplice = applied.into();
        snapshot
            .steps
            .get_or_insert_with(Vec::new)
            .push(own.clone());
        snapshot.applied = Some(own);
        record_receipt(
            tx,
            address,
            operation_id,
            &fingerprint,
            snapshot.version,
            "applied",
        )
        .await?;
    } else {
        record_receipt(
            tx,
            address,
            operation_id,
            &fingerprint,
            snapshot.version,
            "unchanged",
        )
        .await?;
    }
    if let Some(conflict) = restore_conflict.as_ref() {
        snapshot.attach_restore_conflict(conflict)?;
    }
    snapshot.acknowledged_version = Some(snapshot.version);
    Ok(CoeditResult::Saved(snapshot))
}

fn covers(journal: &[JournalRow], from: i64, through: i64) -> bool {
    from >= 0 && through >= from && journal.len() as i64 == through - from
}

async fn replay_change(
    tx: &mut PgConnection,
    address: &PageAddress,
    draft: &DraftRow,
    lease: Uuid,
    from: i64,
    receipt: &storage::Receipt,
) -> Result<CoeditResult<DraftSnapshot>, sqlx::Error> {
    let journal = load_journal(tx, address, from).await?;
    let mut result = draft.snapshot(lease);
    result.acknowledged_version = Some(receipt.version);
    if !covers(&journal, from, draft.version) {
        // The receipt still confirms the previous write. Without the exact
        // sequence the client must reconcile the rest of its queue explicitly.
        return draft.recovery(result);
    }
    let through_receipt = journal
        .iter()
        .take_while(|row| row.version <= receipt.version);
    result.attach_steps(from, through_receipt)?;
    let point = storage::restore_point(
        draft,
        journal.iter().filter(|row| row.version > receipt.version),
    )?;
    result.version = receipt.version;
    result.source = point.source;
    result.format = point.format;
    result.published_revision = point.published_revision;
    if receipt.outcome == "applied" {
        let row = journal.iter().find(|row| row.version == receipt.version);
        result.applied = row.map(|row| row.splice().map(Into::into)).transpose()?;
    }
    if result.version == draft.version
        && let Some(conflict) = draft.conflict()?
    {
        result.attach_restore_conflict(&conflict)?;
    }
    Ok(CoeditResult::Saved(result))
}

fn rebase(
    original: Splice,
    journal: &[JournalRow],
    mut source: String,
) -> Result<Result<Splice, Conflict>, sqlx::Error> {
    let mut result = Ok(original);
    for row in journal {
        if row.removed == row.inserted {
            continue;
        }
        let accepted = row.splice()?;
        result = match result {
            Ok(incoming) => project(incoming, &accepted, &source),
            Err(conflict) => Err(project_conflict(conflict, &accepted, &source)),
        };
        source = accepted.apply(&source).ok_or_else(invalid_journal)?;
    }
    Ok(result)
}

async fn valid_source(
    tx: &mut PgConnection,
    address: &PageAddress,
    before: &DraftRow,
    source: &str,
    format: PageContentFormat,
) -> Result<bool, sqlx::Error> {
    if source.chars().count() > MAX_SHARED_SOURCE {
        return Ok(false);
    }
    if format == PageContentFormat::Literal {
        return Ok(markdown_to_text(source).chars().count() <= MAX_LITERAL_TEXT);
    }
    let mut references = attachment_reference_ids(source);
    if !references.is_empty() && before.format() == PageContentFormat::Markdown {
        let previous = attachment_reference_ids(&before.source);
        references.retain(|id| !previous.contains(id));
    }
    if references.is_empty() {
        return Ok(true);
    }
    let published: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM page_attachments
         WHERE (tenant_id,space_id,page_id)=($1,$2,$3) AND asset_id=ANY($4)",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(&references)
    .fetch_one(tx)
    .await?;
    Ok(published == references.len() as i64)
}

fn invalid_journal() -> sqlx::Error {
    sqlx::Error::Protocol("invalid page coedit journal range".to_owned())
}
