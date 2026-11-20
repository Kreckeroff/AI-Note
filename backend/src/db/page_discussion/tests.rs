use axum::{body::Body, extract::Request, http::StatusCode};
use sqlx::PgPool;
use uuid::Uuid;

use super::{
    CommentCommand, DiscussionOutcome, DiscussionStatus, MentionLookup, ReplyPage, ThreadQuery,
    ThreadSelection,
};
use crate::db::{PageAddress, Store};

async fn fixture(pool: &PgPool) -> anyhow::Result<(PageAddress, Uuid, Uuid)> {
    let [tenant, space, section, page, owner, commenter, reader] =
        std::array::from_fn(|_| Uuid::new_v4());
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'Организация')")
        .bind(tenant)
        .execute(pool)
        .await?;
    for (id, email, name) in [
        (owner, "owner@example.test", "Владелец"),
        (commenter, "comment@example.test", "Комментатор"),
        (reader, "reader@example.test", "Читатель"),
    ] {
        sqlx::query(
            "INSERT INTO accounts (id, email, name, salt, password_hash)
                     VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id)
        .bind(email)
        .bind(name)
        .bind([0_u8; 16].as_slice())
        .bind([0_u8; 32].as_slice())
        .execute(pool)
        .await?;
        sqlx::query(
            "INSERT INTO tenant_memberships VALUES ($1, $2, 'member', 'active', false, false)",
        )
        .bind(tenant)
        .bind(id)
        .execute(pool)
        .await?;
    }
    sqlx::query(
        "INSERT INTO spaces (tenant_id, id, creator_account_id, owner_account_id, name, name_key)
                 VALUES ($1, $2, $3, $3, 'space', 'space')",
    )
    .bind(tenant)
    .bind(space)
    .bind(owner)
    .execute(pool)
    .await?;
    sqlx::query("INSERT INTO sections (tenant_id, space_id, id, creator_account_id, title, title_key, position)
                 VALUES ($1, $2, $3, $4, 'section', 'section', 1)")
        .bind(tenant).bind(space).bind(section).bind(owner).execute(pool).await?;
    sqlx::query(
        "INSERT INTO pages (tenant_id, space_id, section_id, id, creator_account_id, title,
                     title_key, position, markdown, content_is_markdown)
                 VALUES ($1, $2, $3, $4, $5, 'page', 'page', 1, 'Текст **цитаты**', true)",
    )
    .bind(tenant)
    .bind(space)
    .bind(section)
    .bind(page)
    .bind(owner)
    .execute(pool)
    .await?;
    sqlx::query(
        "INSERT INTO page_grants (tenant_id, space_id, page_id, account_id, role, guest)
                 VALUES ($1, $2, $3, $4, 'comment', false), ($1, $2, $3, $5, 'read', false)",
    )
    .bind(tenant)
    .bind(space)
    .bind(page)
    .bind(commenter)
    .bind(reader)
    .execute(pool)
    .await?;
    Ok((
        PageAddress {
            tenant,
            space,
            page,
            account: owner,
        },
        commenter,
        reader,
    ))
}

fn topic(operation_id: Uuid, mentions: Vec<Uuid>) -> CommentCommand {
    CommentCommand::Topic {
        body: "Замечание".to_owned(),
        operation_id,
        mentions,
        anchor: None,
    }
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
async fn targeted_comments_can_be_retried_and_mentions_follow_permission(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (owner, commenter, reader) = fixture(&pool).await?;
    let store = Store::from_test_pool(pool.clone());
    let commenter = PageAddress {
        account: commenter,
        ..owner
    };
    let reader = PageAddress {
        account: reader,
        ..owner
    };
    let MentionLookup::Found(candidates) = store.discussion_mentions(&commenter, "чит").await?
    else {
        panic!("a commenter can search reachable readers");
    };
    assert_eq!(candidates.people[0].id, reader.account);
    let mention_request = || {
        Request::builder()
            .uri("/discussion?mentions=editor")
            .body(Body::empty())
            .unwrap()
    };
    assert_eq!(
        crate::page_discussion_http::handle(&store, &reader, "http://localhost", mention_request())
            .await?
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        store
            .write_comment(&reader, topic(Uuid::new_v4(), vec![]))
            .await?,
        DiscussionOutcome::Denied
    );
    let key = Uuid::new_v4();
    let result = store
        .write_comment(&commenter, topic(key, vec![reader.account, reader.account]))
        .await?;
    let DiscussionOutcome::Applied(created) = result else {
        panic!("comment creation failed: {result:?}")
    };
    assert_eq!(
        created,
        match store
            .write_comment(&commenter, topic(key, vec![reader.account]))
            .await?
        {
            DiscussionOutcome::Applied(replay) => replay,
            other => panic!("comment replay failed: {other:?}"),
        }
    );
    assert_eq!(
        store.write_comment(&commenter, topic(key, vec![])).await?,
        DiscussionOutcome::OperationConflict
    );
    let visible = store
        .discussion_threads(
            &reader,
            ThreadQuery::Filter {
                status: DiscussionStatus::Active,
                offset: 0,
            },
        )
        .await?
        .unwrap();
    assert_eq!(visible.threads.len(), 1);
    assert_eq!(visible.revision, "1");
    assert_eq!(visible.threads[0].id, created.id);
    assert!(!visible.can_comment);
    let notifications = store.discussion_notifications(reader.account, 0).await?;
    assert_eq!(notifications.notifications.len(), 1);
    assert_eq!(
        notifications.notifications[0].target,
        super::NotificationTarget::Mention {
            thread_id: created.id,
            message_id: created.id
        },
    );
    sqlx::query("DELETE FROM page_grants WHERE (tenant_id, space_id, page_id, account_id) = ($1, $2, $3, $4)")
       .bind(reader.tenant).bind(reader.space).bind(reader.page).bind(reader.account).execute(&pool).await?;
    assert!(
        store
            .discussion_notifications(reader.account, 0)
            .await?
            .notifications
            .is_empty()
    );
    assert_eq!(
        crate::page_discussion_http::handle(&store, &reader, "http://localhost", mention_request())
            .await?
            .status(),
        StatusCode::NOT_FOUND
    );
    assert!(
        store
            .discussion_threads(
                &reader,
                ThreadQuery::Filter {
                    status: DiscussionStatus::Active,
                    offset: 0
                }
            )
            .await?
            .is_none()
    );
    assert_eq!(
        store
            .write_comment(&commenter, topic(Uuid::new_v4(), vec![reader.account]))
            .await?,
        DiscussionOutcome::MentionUnavailable
    );
    let other = PageAddress {
        tenant: Uuid::new_v4(),
        ..owner
    };
    assert!(
        store
            .discussion_threads(
                &other,
                ThreadQuery::Filter {
                    status: DiscussionStatus::Active,
                    offset: 0
                }
            )
            .await?
            .is_none()
    );
    Ok(())
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
async fn versioned_status_preserves_reopened_thread_and_rejects_unauthorized_transition(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (owner, commenter, reader) = fixture(&pool).await?;
    let store = Store::from_test_pool(pool.clone());
    let commenter = PageAddress {
        account: commenter,
        ..owner
    };
    let reader = PageAddress {
        account: reader,
        ..owner
    };
    let DiscussionOutcome::Applied(created) = store
        .write_comment(&commenter, topic(Uuid::new_v4(), vec![]))
        .await?
    else {
        panic!("author must create thread")
    };
    let resolve = CommentCommand::Resolve {
        thread_id: created.id,
        version: 1,
        operation_id: Uuid::new_v4(),
    };
    assert_eq!(
        store.write_comment(&reader, resolve.clone()).await?,
        DiscussionOutcome::Denied
    );
    let result = store.write_comment(&commenter, resolve.clone()).await?;
    assert!(matches!(result, DiscussionOutcome::Applied(_)));
    let reply = CommentCommand::Reply {
        thread_id: created.id,
        body: "Принято".to_owned(),
        mentions: vec![],
        operation_id: Uuid::new_v4(),
    };
    assert_eq!(
        store.write_comment(&commenter, reply.clone()).await?,
        DiscussionOutcome::ThreadConflict
    );
    let resolved = store
        .discussion_threads(
            &reader,
            ThreadQuery::One(ThreadSelection {
                id: created.id,
                replies: ReplyPage::Start(0),
            }),
        )
        .await?
        .unwrap();
    assert_eq!(
        resolved.threads[0].changed_by.as_deref(),
        Some("Комментатор")
    );
    assert_eq!(resolved.threads[0].status, DiscussionStatus::Resolved);
    let reopen = CommentCommand::Reopen {
        thread_id: created.id,
        version: 2,
        operation_id: Uuid::new_v4(),
    };
    assert!(matches!(
        store.write_comment(&owner, reopen).await?,
        DiscussionOutcome::Applied(_)
    ));
    assert_eq!(store.write_comment(&commenter, resolve).await?, result);
    let replied = store.write_comment(&commenter, reply).await?;
    let DiscussionOutcome::Applied(replied) = replied else {
        panic!("reply must succeed: {replied:?}")
    };
    let active = store
        .discussion_threads(
            &reader,
            ThreadQuery::One(ThreadSelection {
                id: created.id,
                replies: ReplyPage::Target(replied.id),
            }),
        )
        .await?
        .unwrap();
    assert_eq!(active.threads[0].version, 3);
    assert_eq!(active.threads[0].status, DiscussionStatus::Active);
    assert_eq!(active.threads[0].replies[0].id, replied.id);
    assert_eq!(active.revision, "1");
    Ok(())
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
async fn anchors_validate_published_revision_and_keep_history_private(
    pool: PgPool,
) -> anyhow::Result<()> {
    let (owner, commenter, reader) = fixture(&pool).await?;
    let store = Store::from_test_pool(pool.clone());
    let commenter = PageAddress {
        account: commenter,
        ..owner
    };
    let reader = PageAddress {
        account: reader,
        ..owner
    };
    let key = Uuid::new_v4();
    let anchored: CommentCommand = serde_json::from_value(serde_json::json!({
        "action": "topic",
        "body": "Цитата для уточнения",
        "operationId": key,
        "anchor": {"revision": "1", "quote": "Текст цитаты", "start": 0, "end": 12,
            "before": "", "after": ""}
    }))?;
    let DiscussionOutcome::Applied(created) =
        store.write_comment(&commenter, anchored.clone()).await?
    else {
        panic!("valid published quote must attach");
    };
    sqlx::query(
        "UPDATE pages SET content_revision = 2, markdown = 'Новый документ без фразы'
                 WHERE (tenant_id, space_id, id) = ($1, $2, $3)",
    )
    .bind(owner.tenant)
    .bind(owner.space)
    .bind(owner.page)
    .execute(&pool)
    .await?;
    assert_eq!(
        store.write_comment(&commenter, anchored.clone()).await?,
        DiscussionOutcome::Applied(created)
    );
    let mut stale = anchored;
    if let CommentCommand::Topic {
        ref mut operation_id,
        ..
    } = stale
    {
        *operation_id = Uuid::new_v4();
    }
    assert_eq!(
        store.write_comment(&commenter, stale).await?,
        DiscussionOutcome::AnchorConflict
    );
    let query = || {
        ThreadQuery::One(ThreadSelection {
            id: created.id,
            replies: ReplyPage::Start(0),
        })
    };
    let result = store.discussion_threads(&reader, query()).await?.unwrap();
    let thread = serde_json::to_value(&result.threads[0])?;
    assert_eq!(thread["body"], "Цитата для уточнения");
    assert_eq!(thread["anchor"]["state"], "missing");
    assert_eq!(thread["anchor"]["quote"], "");
    assert!(thread["anchor"].get("historyTo").is_none());
    let result = store.discussion_threads(&owner, query()).await?.unwrap();
    let original_anchor = result.threads[0].anchor.as_ref().expect("anchor remains");
    assert_eq!(original_anchor.historical_revision, Some(1));
    assert_eq!(
        serde_json::to_value(original_anchor)?["quote"],
        "Текст цитаты"
    );
    assert!(
        store
            .discussion_history_quote(&reader, created.id, 1)
            .await?
            .is_none()
    );
    let original = store
        .discussion_history_quote(&owner, created.id, 1)
        .await?
        .unwrap();
    assert_eq!(original.quote, "Текст цитаты");
    Ok(())
}
