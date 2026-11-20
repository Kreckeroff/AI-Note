mod anchors;
mod reads;
#[cfg(test)]
mod tests;

pub(crate) use reads::{DiscussionQuote, MentionLookup, ReplyPage, ThreadQuery, ThreadSelection};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use self::anchors::{AnchorInput, StoredAnchor, published_text};
use super::{
    PageAddress, Store,
    page_access::{PagePermissions, lock_page_for_change},
};
use crate::{clock::now_ms, text::js_trim};

const MAX_MENTIONS: usize = 40;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub(crate) enum DiscussionStatus {
    Active,
    Resolved,
}

impl DiscussionStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Resolved => "resolved",
        }
    }

    fn from_storage(status: &str) -> Self {
        if status == "active" {
            Self::Active
        } else {
            Self::Resolved
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "action",
    rename_all = "lowercase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum CommentCommand {
    Topic {
        body: String,
        operation_id: Uuid,
        #[serde(default)]
        mentions: Vec<Uuid>,
        anchor: Option<AnchorInput>,
    },
    Reply {
        thread_id: Uuid,
        body: String,
        operation_id: Uuid,
        #[serde(default)]
        mentions: Vec<Uuid>,
    },
    Resolve {
        thread_id: Uuid,
        version: i32,
        operation_id: Uuid,
    },
    Reopen {
        thread_id: Uuid,
        version: i32,
        operation_id: Uuid,
    },
}

impl CommentCommand {
    pub(crate) fn action(&self) -> &'static str {
        match self {
            Self::Topic { .. } => "topic",
            Self::Reply { .. } => "reply",
            Self::Resolve { .. } => "resolve",
            Self::Reopen { .. } => "reopen",
        }
    }

    fn operation_id(&self) -> Uuid {
        match self {
            Self::Topic { operation_id, .. }
            | Self::Reply { operation_id, .. }
            | Self::Resolve { operation_id, .. }
            | Self::Reopen { operation_id, .. } => *operation_id,
        }
    }

    fn thread_id(&self) -> Option<Uuid> {
        match self {
            Self::Topic { .. } => None,
            Self::Reply { thread_id, .. }
            | Self::Resolve { thread_id, .. }
            | Self::Reopen { thread_id, .. } => Some(*thread_id),
        }
    }

    fn normalize(&mut self) -> bool {
        match self {
            Self::Topic { body, mentions, .. } | Self::Reply { body, mentions, .. } => {
                *body = js_trim(body).to_owned();
                mentions.sort_unstable();
                mentions.dedup();
                !body.contains('\0')
                    && (1..=500).contains(&body.chars().count())
                    && mentions.len() <= MAX_MENTIONS
            }
            Self::Resolve { version, .. } | Self::Reopen { version, .. } => *version > 0,
        }
    }

    fn fingerprint(&self) -> Result<Vec<u8>, sqlx::Error> {
        let normalized =
            serde_json::to_vec(self).map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
        Ok(Sha256::digest(normalized).to_vec())
    }
}

#[derive(Debug, FromRow)]
struct ThreadMetadata {
    status: String,
    version: i32,
    author_account_id: Uuid,
}

fn can_change_thread(permission: &PagePermissions, actor: Uuid, author: Uuid) -> bool {
    permission.can_edit || (permission.can_comment && actor == author)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CommentResult {
    pub(crate) id: Uuid,
    pub(crate) thread_id: Uuid,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DiscussionOutcome {
    Applied(CommentResult),
    Missing,
    Denied,
    Invalid,
    AnchorConflict,
    ThreadConflict,
    OperationConflict,
    MentionUnavailable,
}

impl DiscussionOutcome {
    pub(crate) fn outcome(&self) -> &'static str {
        match self {
            Self::Applied(_) => "applied",
            Self::Missing => "missing",
            Self::Denied => "denied",
            Self::Invalid => "invalid",
            Self::AnchorConflict => "anchor-conflict",
            Self::ThreadConflict => "thread-conflict",
            Self::OperationConflict => "operation-conflict",
            Self::MentionUnavailable => "mention-unavailable",
        }
    }
}

#[derive(FromRow)]
struct Receipt {
    fingerprint: Vec<u8>,
    result_id: Uuid,
    result_thread_id: Uuid,
}

async fn find_receipt(
    tx: &mut PgConnection,
    address: &PageAddress,
    operation_id: Uuid,
) -> Result<Option<Receipt>, sqlx::Error> {
    sqlx::query_as(
        "SELECT fingerprint, result_id, result_thread_id FROM page_comment_operations
         WHERE (tenant_id, space_id, page_id, actor_account_id, operation_id) = ($1, $2, $3, $4, $5)",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(address.account)
    .bind(operation_id)
    .fetch_optional(tx)
    .await
}

impl Store {
    pub(crate) async fn write_comment(
        &self,
        address: &PageAddress,
        mut command: CommentCommand,
    ) -> Result<DiscussionOutcome, sqlx::Error> {
        if !command.normalize() {
            return Ok(DiscussionOutcome::Invalid);
        }
        let operation_id = command.operation_id();
        let fingerprint = command.fingerprint()?;
        let mut tx = self.0.begin().await?;
        let Some(permission) = lock_page_for_change(&mut tx, address)
            .await?
            .filter(|permission| permission.can_read)
        else {
            return Ok(DiscussionOutcome::Missing);
        };
        if !permission.can_comment {
            return Ok(DiscussionOutcome::Denied);
        }
        let saved = find_receipt(&mut tx, address, operation_id).await?;
        if saved
            .as_ref()
            .is_some_and(|receipt| receipt.fingerprint != fingerprint)
        {
            return Ok(DiscussionOutcome::OperationConflict);
        }
        let thread = match command.thread_id() {
            Some(id) => {
                let Some(thread) = thread_metadata(&mut tx, address, id).await? else {
                    return Ok(DiscussionOutcome::Missing);
                };
                if matches!(
                    &command,
                    CommentCommand::Resolve { .. } | CommentCommand::Reopen { .. }
                ) && !can_change_thread(&permission, address.account, thread.author_account_id)
                {
                    return Ok(DiscussionOutcome::Denied);
                }
                Some(thread)
            }
            None => None,
        };
        if let Some(saved) = saved {
            return Ok(DiscussionOutcome::Applied(CommentResult {
                id: saved.result_id,
                thread_id: saved.result_thread_id,
            }));
        }

        let actor: String = sqlx::query_scalar("SELECT name FROM accounts WHERE id = $1")
            .bind(address.account)
            .fetch_one(&mut *tx)
            .await?;
        let time = now_ms();
        let result = match command {
            CommentCommand::Topic {
                body,
                mentions,
                anchor,
                ..
            } => {
                let anchor = if let Some(input) = anchor {
                    let current = published_text(&mut tx, address).await?;
                    let Some(anchor) = input.validate(current) else {
                        return Ok(DiscussionOutcome::AnchorConflict);
                    };
                    Some(anchor)
                } else {
                    None
                };
                let id = Uuid::new_v4();
                insert_thread(&mut tx, address, id, time, anchor.as_ref()).await?;
                let message = NewMessage {
                    id,
                    thread_id: id,
                    author: &actor,
                    body: &body,
                    created_at: time,
                };
                if !persist_message(&mut tx, address, message, &mentions).await? {
                    return Ok(DiscussionOutcome::MentionUnavailable);
                }
                CommentResult { id, thread_id: id }
            }
            CommentCommand::Reply {
                thread_id,
                body,
                mentions,
                ..
            } => {
                if thread.is_none_or(|thread| thread.status != DiscussionStatus::Active.as_str()) {
                    return Ok(DiscussionOutcome::ThreadConflict);
                }
                let id = Uuid::new_v4();
                let message = NewMessage {
                    id,
                    thread_id,
                    author: &actor,
                    body: &body,
                    created_at: time,
                };
                if !persist_message(&mut tx, address, message, &mentions).await? {
                    return Ok(DiscussionOutcome::MentionUnavailable);
                }
                CommentResult { id, thread_id }
            }
            CommentCommand::Resolve {
                thread_id, version, ..
            }
            | CommentCommand::Reopen {
                thread_id, version, ..
            } => {
                let desired = if matches!(command, CommentCommand::Resolve { .. }) {
                    DiscussionStatus::Resolved
                } else {
                    DiscussionStatus::Active
                };
                if thread.is_none_or(|thread| {
                    thread.version != version || thread.status == desired.as_str()
                }) {
                    return Ok(DiscussionOutcome::ThreadConflict);
                }
                sqlx::query(
                    "UPDATE page_comment_threads
                     SET status = $5, version = version + 1, changed_by = $6, changed_by_name = $7, changed_at = $8
                     WHERE (tenant_id, space_id, page_id, id) = ($1, $2, $3, $4)",
                )
                .bind(address.tenant)
                .bind(address.space)
                .bind(address.page)
                .bind(thread_id)
                .bind(desired.as_str())
                .bind(address.account)
                .bind(&actor)
                .bind(time)
                .execute(&mut *tx)
                .await?;
                CommentResult {
                    id: thread_id,
                    thread_id,
                }
            }
        };
        sqlx::query(
            "INSERT INTO page_comment_operations (tenant_id, space_id, page_id, actor_account_id, operation_id,
                                                   fingerprint, result_id, result_thread_id, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(address.account)
        .bind(operation_id)
        .bind(fingerprint)
        .bind(result.id)
        .bind(result.thread_id)
        .bind(time)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(DiscussionOutcome::Applied(result))
    }
}

async fn thread_metadata(
    tx: &mut PgConnection,
    address: &PageAddress,
    id: Uuid,
) -> Result<Option<ThreadMetadata>, sqlx::Error> {
    sqlx::query_as(
        "SELECT t.status, t.version, m.author_account_id
         FROM page_comment_threads t JOIN page_comment_messages m
           ON (m.tenant_id, m.space_id, m.page_id, m.thread_id, m.id)
            = (t.tenant_id, t.space_id, t.page_id, t.id, t.id)
         WHERE (t.tenant_id, t.space_id, t.page_id, t.id) = ($1, $2, $3, $4)",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(id)
    .fetch_optional(tx)
    .await
}

async fn insert_thread(
    tx: &mut PgConnection,
    address: &PageAddress,
    id: Uuid,
    created_at: i64,
    anchor: Option<&StoredAnchor>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO page_comment_threads
           (tenant_id, space_id, page_id, id, created_at, anchor_revision, anchor_quote,
            anchor_start, anchor_end, anchor_before, anchor_after)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(id)
    .bind(created_at)
    .bind(anchor.map(|anchor| anchor.revision))
    .bind(anchor.map(|anchor| anchor.quote.as_str()))
    .bind(anchor.map(|anchor| anchor.start))
    .bind(anchor.map(|anchor| anchor.end))
    .bind(anchor.map(|anchor| anchor.before.as_str()))
    .bind(anchor.map(|anchor| anchor.after.as_str()))
    .execute(tx)
    .await?;
    Ok(())
}

struct NewMessage<'a> {
    id: Uuid,
    thread_id: Uuid,
    author: &'a str,
    body: &'a str,
    created_at: i64,
}

async fn persist_message(
    tx: &mut PgConnection,
    address: &PageAddress,
    message: NewMessage<'_>,
    recipients: &[Uuid],
) -> Result<bool, sqlx::Error> {
    sqlx::query(
        "INSERT INTO page_comment_messages
            (tenant_id, space_id, page_id, thread_id, id, author_account_id, author_name, body, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(message.thread_id)
    .bind(message.id)
    .bind(address.account)
    .bind(message.author)
    .bind(message.body)
    .bind(message.created_at)
    .execute(&mut *tx)
    .await?;
    if recipients.is_empty() {
        return Ok(true);
    }
    let mention_ids: Vec<_> = recipients.iter().map(|_| Uuid::new_v4()).collect();
    let inserted = sqlx::query(
        "INSERT INTO page_comment_mentions
           (tenant_id, space_id, page_id, message_id, recipient_account_id, id, created_at)
         SELECT $1, $2, $3, $4, recipient.account_id, recipient.id, $7
         FROM unnest($5::uuid[], $6::uuid[]) AS recipient(account_id, id)
         CROSS JOIN LATERAL page_permissions($1, $2, recipient.account_id, $3) access
         WHERE access.can_read",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(message.id)
    .bind(recipients)
    .bind(&mention_ids)
    .bind(message.created_at)
    .execute(&mut *tx)
    .await?;
    // A missing recipient rolls back the message and every notification.
    Ok(inserted.rows_affected() == recipients.len() as u64)
}
