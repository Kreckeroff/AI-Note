use std::collections::HashMap;

use serde::Serialize;
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use super::{
    DiscussionStatus,
    anchors::{AnchorView, StoredAnchor, published_text},
    can_change_thread,
};
use crate::{
    db::{PageAddress, Store, page_access::PagePermissions, sections::decode_title},
    text::js_trim,
};

const THREAD_PAGE_SIZE: i64 = 16;
const REPLY_PAGE_SIZE: i64 = 40;
const MENTION_PAGE_SIZE: i64 = 15;
const NOTIFICATION_PAGE_SIZE: i64 = 25;

pub(crate) enum ReplyPage {
    Start(i64),
    Target(Uuid),
}

pub(crate) struct ThreadSelection {
    pub(crate) id: Uuid,
    pub(crate) replies: ReplyPage,
}

pub(crate) enum ThreadQuery {
    Filter {
        status: DiscussionStatus,
        offset: i64,
    },
    One(ThreadSelection),
}

#[derive(FromRow)]
struct ThreadRow {
    id: Uuid,
    author: String,
    author_account_id: Uuid,
    body: String,
    created_at: i64,
    status: String,
    version: i32,
    changed_by: Option<String>,
    changed_at: Option<i64>,
    anchor_revision: Option<i64>,
    anchor_quote: Option<String>,
    anchor_start: Option<i32>,
    anchor_end: Option<i32>,
    anchor_before: Option<String>,
    anchor_after: Option<String>,
}

impl ThreadRow {
    fn take_anchor(&mut self) -> Option<StoredAnchor> {
        Some(StoredAnchor {
            revision: self.anchor_revision?,
            quote: self.anchor_quote.take()?,
            start: self.anchor_start?,
            end: self.anchor_end?,
            before: self.anchor_before.take()?,
            after: self.anchor_after.take()?,
        })
    }
}

#[derive(FromRow, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiscussionReply {
    #[serde(skip)]
    thread_id: Uuid,
    pub(crate) id: Uuid,
    body: String,
    author: String,
    created_at: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiscussionThread {
    pub(crate) id: Uuid,
    body: String,
    author: String,
    created_at: i64,
    pub(crate) status: DiscussionStatus,
    pub(crate) version: i32,
    can_change: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) changed_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    changed_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) anchor: Option<AnchorView>,
    pub(crate) replies: Vec<DiscussionReply>,
    replies_offset: i64,
    replies_next_offset: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiscussionPage {
    pub(crate) revision: String,
    pub(crate) can_comment: bool,
    next_offset: Option<i64>,
    pub(crate) threads: Vec<DiscussionThread>,
}

#[derive(FromRow, Serialize)]
pub(crate) struct MentionCandidate {
    pub(crate) id: Uuid,
    name: String,
}

#[derive(Serialize)]
pub(crate) struct MentionCandidates {
    pub(crate) people: Vec<MentionCandidate>,
}

pub(crate) enum MentionLookup {
    Missing,
    Denied,
    Found(MentionCandidates),
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum NotificationTarget {
    Mention { thread_id: Uuid, message_id: Uuid },
    Publication { title: String, revision: i64 },
}

pub(crate) struct DiscussionNotification {
    pub(crate) id: Uuid,
    pub(crate) author: String,
    pub(crate) created_at: i64,
    pub(crate) tenant_id: Uuid,
    pub(crate) space_id: Uuid,
    pub(crate) page_id: Uuid,
    pub(crate) target: NotificationTarget,
}

#[derive(FromRow)]
struct NotificationRow {
    id: Uuid,
    author: String,
    created_at: i64,
    tenant_id: Uuid,
    space_id: Uuid,
    page_id: Uuid,
    thread_id: Option<Uuid>,
    message_id: Option<Uuid>,
    title: Option<Vec<u8>>,
    revision: Option<i64>,
}

impl TryFrom<NotificationRow> for DiscussionNotification {
    type Error = sqlx::Error;

    fn try_from(row: NotificationRow) -> Result<Self, Self::Error> {
        let target = match (row.thread_id, row.message_id, row.title, row.revision) {
            (Some(thread_id), Some(message_id), None, None) => NotificationTarget::Mention {
                thread_id,
                message_id,
            },
            (None, None, Some(title), Some(revision)) => NotificationTarget::Publication {
                title: decode_title(title, "page_revisions.title")?,
                revision,
            },
            _ => return Err(sqlx::Error::Protocol("invalid page notification".into())),
        };
        Ok(Self {
            id: row.id,
            author: row.author,
            created_at: row.created_at,
            tenant_id: row.tenant_id,
            space_id: row.space_id,
            page_id: row.page_id,
            target,
        })
    }
}

pub(crate) struct DiscussionNotifications {
    pub(crate) next_offset: Option<i64>,
    pub(crate) notifications: Vec<DiscussionNotification>,
}

#[derive(FromRow)]
pub(crate) struct DiscussionQuote {
    pub(crate) before: String,
    pub(crate) quote: String,
    pub(crate) after: String,
}

const SELECT_THREADS: &str =
    "SELECT t.id, t.created_at, t.status, t.version, t.changed_by_name AS changed_by, t.changed_at,
            t.anchor_revision, t.anchor_quote, t.anchor_start, t.anchor_end, t.anchor_before, t.anchor_after,
            m.body, m.author_account_id, m.author_name AS author
     FROM page_comment_threads t
     JOIN page_comment_messages m ON (m.tenant_id, m.space_id, m.page_id, m.thread_id, m.id)
         = (t.tenant_id, t.space_id, t.page_id, t.id, t.id)
     WHERE (t.tenant_id, t.space_id, t.page_id) = ($1, $2, $3)";

impl Store {
    pub(crate) async fn discussion_threads(
        &self,
        address: &PageAddress,
        origin: &str,
        query: ThreadQuery,
    ) -> Result<Option<DiscussionPage>, sqlx::Error> {
        // Every list projection, including replies and anchor relocation,
        // observes the same current permission and published revision.
        let mut tx = self.0.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        sqlx::query("SET LOCAL statement_timeout = '3s'")
            .execute(&mut *tx)
            .await?;
        let Some((permission, revision)) = readable_revision(&mut tx, address).await? else {
            return Ok(None);
        };
        let (mut rows, next_offset, replies_offset) = match query {
            ThreadQuery::Filter { status, offset } => {
                let query = format!(
                    "{SELECT_THREADS} AND t.status = $4
                                     ORDER BY t.created_at DESC, t.id DESC LIMIT $5 OFFSET $6"
                );
                let mut rows: Vec<ThreadRow> = sqlx::query_as(&query)
                    .bind(address.tenant)
                    .bind(address.space)
                    .bind(address.page)
                    .bind(status.as_str())
                    .bind(THREAD_PAGE_SIZE + 1)
                    .bind(offset)
                    .fetch_all(&mut *tx)
                    .await?;
                let next_offset = trim_page(&mut rows, offset, THREAD_PAGE_SIZE);
                (rows, next_offset, 0)
            }
            ThreadQuery::One(selected) => {
                let query = format!("{SELECT_THREADS} AND t.id = $4");
                let Some(row) = sqlx::query_as(&query)
                    .bind(address.tenant)
                    .bind(address.space)
                    .bind(address.page)
                    .bind(selected.id)
                    .fetch_optional(&mut *tx)
                    .await?
                else {
                    return Ok(None);
                };
                let offset = match selected.replies {
                    ReplyPage::Start(offset) => offset,
                    ReplyPage::Target(reply) if reply == selected.id => 0,
                    ReplyPage::Target(reply) => {
                        let Some(offset) =
                            reply_offset(&mut tx, address, selected.id, reply).await?
                        else {
                            return Ok(None);
                        };
                        offset
                    }
                };
                (vec![row], None, offset)
            }
        };
        let publication = if rows
            .iter()
            .any(|row| row.anchor_revision.is_some_and(|origin| origin != revision))
        {
            Some(published_text(&mut tx, address, origin).await?)
        } else {
            None
        };
        let ids: Vec<_> = rows.iter().map(|row| row.id).collect();
        let mut replies = thread_replies(&mut tx, address, &ids, replies_offset).await?;
        tx.commit().await?;

        let threads = rows
            .drain(..)
            .map(|mut row| {
                let mut thread_replies = replies.remove(&row.id).unwrap_or_default();
                let replies_next_offset =
                    trim_page(&mut thread_replies, replies_offset, REPLY_PAGE_SIZE);
                let anchor = row
                    .take_anchor()
                    .map(|anchor| anchor.view(revision, publication.as_ref(), permission.can_edit));
                DiscussionThread {
                    id: row.id,
                    body: row.body,
                    author: row.author,
                    created_at: row.created_at,
                    status: DiscussionStatus::from_storage(&row.status),
                    version: row.version,
                    changed_by: row.changed_by,
                    changed_at: row.changed_at,
                    can_change: can_change_thread(
                        &permission,
                        address.account,
                        row.author_account_id,
                    ),
                    anchor,
                    replies: thread_replies,
                    replies_offset,
                    replies_next_offset,
                }
            })
            .collect();
        Ok(Some(DiscussionPage {
            revision: revision.to_string(),
            can_comment: permission.can_comment,
            next_offset,
            threads,
        }))
    }

    pub(crate) async fn discussion_mentions(
        &self,
        address: &PageAddress,
        search: &str,
    ) -> Result<MentionLookup, sqlx::Error> {
        // Only accounts with inherited or explicit page access can be
        // candidates. Return no identity or role information on refusal.
        let mut tx = self.0.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        sqlx::query("SET LOCAL statement_timeout = '3s'")
            .execute(&mut *tx)
            .await?;
        let allowed: Option<bool> = sqlx::query_scalar(
            "SELECT can_comment FROM page_permissions($1, $2, $3, $4) WHERE can_read",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.account)
        .bind(address.page)
        .fetch_optional(&mut *tx)
        .await?;
        match allowed {
            None => return Ok(MentionLookup::Missing),
            Some(false) => return Ok(MentionLookup::Denied),
            Some(true) => {}
        }
        let people = sqlx::query_as(
            "WITH candidates AS (
                 SELECT owner_account_id AS account_id FROM spaces WHERE (tenant_id, id) = ($1, $2)
                 UNION
                 SELECT account_id FROM tenant_memberships WHERE tenant_id = $1 AND role IN ('admin', 'superadmin') AND status = 'active'
                 UNION
                 SELECT account_id FROM effective_space_grants WHERE (tenant_id, space_id) = ($1, $2)
                 UNION
                 SELECT account_id FROM page_grants WHERE (tenant_id, space_id, page_id) = ($1, $2, $3)
             )
             SELECT a.id, a.name FROM candidates c
             JOIN accounts a ON a.id = c.account_id
             CROSS JOIN LATERAL page_permissions($1, $2, a.id, $3) access
             WHERE access.can_read AND position(lower($4) IN lower(a.name)) > 0
             ORDER BY lower(a.name), a.id LIMIT $5",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(js_trim(search))
        .bind(MENTION_PAGE_SIZE)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(MentionLookup::Found(MentionCandidates { people }))
    }

    pub(crate) async fn discussion_notifications(
        &self,
        account: Uuid,
        offset: i64,
    ) -> Result<DiscussionNotifications, sqlx::Error> {
        // Permissions and notification contents are evaluated in this one
        // statement, so revoked pages never reveal their notification data.
        let mut rows: Vec<NotificationRow> = sqlx::query_as(
            "(SELECT n.id, n.tenant_id, n.space_id, n.page_id, n.created_at,
                     m.thread_id, n.message_id, m.author_name AS author,
                     NULL::bytea AS title, NULL::bigint AS revision
              FROM page_comment_mentions n
              JOIN page_comment_messages m ON (m.tenant_id, m.space_id, m.page_id, m.id)
                 = (n.tenant_id, n.space_id, n.page_id, n.message_id)
              CROSS JOIN LATERAL page_permissions(n.tenant_id, n.space_id, n.recipient_account_id, n.page_id) access
              WHERE n.recipient_account_id = $1 AND access.can_read
              ORDER BY n.created_at DESC, n.id DESC LIMIT ($2::bigint + $3::bigint))
             UNION ALL
             (SELECT n.id, n.tenant_id, n.space_id, n.page_id, n.created_at,
                     NULL::uuid, NULL::uuid, r.author_name, r.title, r.revision
              FROM page_publication_notices n
              JOIN page_revisions r ON (r.tenant_id, r.space_id, r.page_id, r.revision)
                 = (n.tenant_id, n.space_id, n.page_id, n.revision)
              CROSS JOIN LATERAL page_permissions(n.tenant_id, n.space_id, n.recipient_account_id, n.page_id) access
              WHERE n.recipient_account_id = $1 AND access.can_read
              ORDER BY n.created_at DESC, n.id DESC LIMIT ($2::bigint + $3::bigint))
             ORDER BY created_at DESC, id DESC LIMIT $2 OFFSET $3",
        )
        .bind(account)
        .bind(NOTIFICATION_PAGE_SIZE + 1)
        .bind(offset)
        .fetch_all(&self.0)
        .await?;
        let next_offset = trim_page(&mut rows, offset, NOTIFICATION_PAGE_SIZE);
        Ok(DiscussionNotifications {
            next_offset,
            notifications: rows
                .into_iter()
                .map(TryInto::try_into)
                .collect::<Result<_, _>>()?,
        })
    }

    pub(crate) async fn discussion_history_quote(
        &self,
        address: &PageAddress,
        thread: Uuid,
        revision: i64,
    ) -> Result<Option<DiscussionQuote>, sqlx::Error> {
        sqlx::query_as(
            "SELECT t.anchor_before AS before, t.anchor_quote AS quote, t.anchor_after AS after
             FROM page_permissions($1, $2, $3, $4) access
             JOIN page_comment_threads t ON (t.tenant_id, t.space_id, t.page_id, t.id) = ($1, $2, $4, $5)
             WHERE access.can_edit AND t.anchor_revision = $6",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.account)
        .bind(address.page)
        .bind(thread)
        .bind(revision)
        .fetch_optional(&self.0)
        .await
    }
}

#[derive(FromRow)]
struct PageRevisionAccess {
    #[sqlx(flatten)]
    permissions: PagePermissions,
    revision: i64,
}

async fn readable_revision(
    tx: &mut PgConnection,
    address: &PageAddress,
) -> Result<Option<(PagePermissions, i64)>, sqlx::Error> {
    let row: Option<PageRevisionAccess> = sqlx::query_as(
        "SELECT access.*, p.content_revision AS revision
         FROM page_permissions($1, $2, $3, $4) access
         JOIN pages p ON (p.tenant_id, p.space_id, p.id) = ($1, $2, $4)
         WHERE access.can_read",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.account)
    .bind(address.page)
    .fetch_optional(tx)
    .await?;
    Ok(row.map(|row| (row.permissions, row.revision)))
}

async fn reply_offset(
    tx: &mut PgConnection,
    address: &PageAddress,
    thread: Uuid,
    reply: Uuid,
) -> Result<Option<i64>, sqlx::Error> {
    let position: Option<i64> = sqlx::query_scalar(
        "SELECT (SELECT count(*) FROM page_comment_messages preceding
                 WHERE (preceding.tenant_id, preceding.space_id, preceding.page_id, preceding.thread_id) = ($1, $2, $3, $4)
                   AND preceding.id <> $4 AND (preceding.created_at, preceding.id) < (target.created_at, target.id))
         FROM page_comment_messages target
         WHERE (target.tenant_id, target.space_id, target.page_id, target.thread_id, target.id) = ($1, $2, $3, $4, $5)",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(thread)
    .bind(reply)
    .fetch_optional(tx)
    .await?;
    Ok(position.map(|position| position / REPLY_PAGE_SIZE * REPLY_PAGE_SIZE))
}

async fn thread_replies(
    tx: &mut PgConnection,
    address: &PageAddress,
    threads: &[Uuid],
    offset: i64,
) -> Result<HashMap<Uuid, Vec<DiscussionReply>>, sqlx::Error> {
    if threads.is_empty() {
        return Ok(HashMap::new());
    }
    let replies: Vec<DiscussionReply> = sqlx::query_as(
        "SELECT reply.* FROM unnest($4::uuid[]) thread(id)
         CROSS JOIN LATERAL (
             SELECT m.thread_id, m.id, m.author_name AS author, m.body, m.created_at
             FROM page_comment_messages m
             WHERE (m.tenant_id, m.space_id, m.page_id, m.thread_id) = ($1, $2, $3, thread.id)
               AND m.id <> m.thread_id
             ORDER BY m.created_at, m.id LIMIT $5 OFFSET $6
         ) reply ORDER BY reply.thread_id, reply.created_at, reply.id",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(threads)
    .bind(REPLY_PAGE_SIZE + 1)
    .bind(offset)
    .fetch_all(tx)
    .await?;
    let mut by_thread: HashMap<_, Vec<_>> = HashMap::new();
    for reply in replies {
        by_thread.entry(reply.thread_id).or_default().push(reply);
    }
    Ok(by_thread)
}

fn trim_page<T>(items: &mut Vec<T>, offset: i64, size: i64) -> Option<i64> {
    if items.len() > size as usize {
        items.truncate(size as usize);
        Some(offset.saturating_add(size))
    } else {
        None
    }
}
