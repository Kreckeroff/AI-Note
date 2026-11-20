use sqlx::PgConnection;
use uuid::Uuid;

use crate::text::normalize_line_endings;

use super::{
    CoeditResult, DraftSnapshot, PageAddress, PageContentFormat, invalid_journal,
    lease::{lock_session, validate_coedit_lease},
    patch::{Conflict, difference, project, project_conflict},
    storage::{DraftWrite, load_draft, record_change},
};

pub(in crate::db) async fn current_coedit_draft(
    tx: &mut PgConnection,
    address: &PageAddress,
) -> Result<Option<(String, PageContentFormat, bool)>, sqlx::Error> {
    Ok(load_draft(tx, address).await?.map(|draft| {
        let format = draft.format();
        (draft.source, format, draft.conflict_start.is_some())
    }))
}

// Publication and legacy readers call these helpers under the same page lock
// as the normal revision and attachment writes.
pub(in crate::db) async fn guard_coedit_publish(
    tx: &mut PgConnection,
    address: &PageAddress,
    session: &[u8],
    lease: Uuid,
    version: i64,
) -> Result<CoeditResult<DraftSnapshot>, sqlx::Error> {
    if !lock_session(tx, address, session).await?
        || !validate_coedit_lease(tx, address, session, lease).await?
    {
        return Ok(CoeditResult::LeaseExpired);
    }
    let Some(draft) = load_draft(tx, address).await? else {
        return Ok(CoeditResult::LeaseExpired);
    };
    if let Some(conflict) = draft.conflict()? {
        return draft.conflict_state(draft.snapshot(lease), conflict, true);
    }
    if draft.version != version {
        let conflict = Conflict {
            range: 0..draft.source.chars().count(),
            mine: draft.source.clone(),
        };
        return draft.conflict_state(draft.snapshot(lease), conflict, false);
    }
    Ok(CoeditResult::Saved(draft.snapshot(lease)))
}

pub(in crate::db) async fn record_publication(
    tx: &mut PgConnection,
    address: &PageAddress,
    source: &str,
    format: PageContentFormat,
    published_revision: i64,
    from_coedit: bool,
) -> Result<(), sqlx::Error> {
    let Some(draft) = load_draft(tx, address).await? else {
        return Ok(());
    };
    if draft.published_revision == published_revision {
        return Ok(());
    }
    let published = normalize_line_endings(source);
    let source_matches = published == draft.source && format == draft.format();
    let dirty =
        draft.source != draft.published_source || draft.format() != draft.published_format();
    let (source, format_in_draft, conflict) = if from_coedit || !dirty || source_matches {
        (published.to_string(), format, None)
    } else {
        let draft_change = difference(&draft.published_source, &draft.source);
        let publication_change = difference(&draft.published_source, &published);
        if publication_change.range.is_empty() && publication_change.text.is_empty() {
            let next_format = if draft.format() == draft.published_format() {
                format
            } else {
                draft.format()
            };
            (draft.source.clone(), next_format, draft.conflict()?)
        } else {
            match project(publication_change, &draft_change, &draft.published_source) {
                Ok(rebased) => {
                    let rebased_source =
                        rebased.apply(&draft.source).ok_or_else(invalid_journal)?;
                    let conflict = draft
                        .conflict()?
                        .map(|conflict| project_conflict(conflict, &rebased, &draft.source));
                    let next_format = if draft.format() == draft.published_format() {
                        format
                    } else {
                        draft.format()
                    };
                    (rebased_source, next_format, conflict)
                }
                Err(conflict) => (draft.source.clone(), draft.format(), Some(conflict)),
            }
        }
    };
    let applied = difference(&draft.source, &source);
    let next = DraftWrite {
        source: &source,
        format: format_in_draft,
        published_revision,
        published_source: &published,
        published_format: format,
        conflict: conflict.as_ref(),
    };
    record_change(tx, address, &draft, &next, &applied).await?;
    Ok(())
}
