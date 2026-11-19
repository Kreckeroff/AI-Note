use super::*;
use crate::db::{AttachmentLookup, AttachmentPatch, StageAttachment};

#[sqlx::test(migrator = "crate::db::MIGRATIONS")]
#[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
async fn attachment_publication_restore_preserves_bytes_and_retires_current_url(
    pool: PgPool,
) -> anyhow::Result<()> {
    let f = setup(pool).await?;
    let draft = Uuid::new_v4();
    let id = Uuid::new_v4();
    let name = "Страница.md";
    let bytes = b"# Document\n";
    let draft_lookup = AttachmentLookup::Draft { draft, id };
    assert!(matches!(
        f.store
            .stage_attachment(&f.viewer, draft, id, name, None, bytes)
            .await?,
        StageAttachment::Forbidden
    ));
    for _ in 0..2 {
        assert!(matches!(
            f.store.stage_attachment(&f.owner, draft, id, name, None, bytes).await?,
            StageAttachment::Saved(file) if file.id == id && file.publication.is_none()
        ));
    }
    assert!(matches!(
        f.store
            .stage_attachment(&f.owner, draft, id, name, None, b"different")
            .await?,
        StageAttachment::Unavailable
    ));
    assert!(matches!(
        f.store
            .stage_attachment(&f.editor, draft, id, name, None, bytes)
            .await?,
        StageAttachment::Unavailable
    ));
    for denied in [
        f.editor,
        f.viewer,
        PageAddress {
            page: f.create_page("Другая").await?,
            ..f.owner
        },
        PageAddress {
            tenant: Uuid::new_v4(),
            ..f.owner
        },
    ] {
        assert!(
            f.store
                .attachment_body(&denied, draft_lookup)
                .await?
                .is_none()
        );
    }
    assert!(
        f.store
            .attachment_body(
                &f.owner,
                AttachmentLookup::Draft {
                    draft: Uuid::new_v4(),
                    id,
                }
            )
            .await?
            .is_none()
    );
    let own = f
        .store
        .attachment_body(&f.owner, draft_lookup)
        .await?
        .unwrap();
    assert_eq!(own.name, name);
    assert_eq!(own.bytes, bytes);
    assert!(
        f.store
            .page_text(&f.viewer)
            .await?
            .unwrap()
            .attachments
            .is_empty()
    );

    let add = AttachmentPatch {
        draft,
        add: &[id],
        remove: &[],
    };
    for _ in 0..2 {
        assert!(matches!(
            f.store
                .save_page_with_attachments(
                    &f.owner,
                    1,
                    PageUpdate::Keep,
                    CoverUpdate::Keep,
                    Some(add),
                )
                .await?,
            SavePageText::Saved
        ));
    }
    let current = f.store.page_text(&f.viewer).await?.unwrap();
    assert_eq!(current.content_revision, 2);
    let first_publication = current.attachments[0].publication.unwrap();
    let published = AttachmentLookup::Published(first_publication);
    for viewer in [f.viewer, f.admin] {
        let found = f.store.attachment_body(&viewer, published).await?.unwrap();
        assert_eq!(found.bytes, bytes);
        assert_eq!(found.name, name);
        assert!(
            f.store
                .attachment_body(&viewer, AttachmentLookup::Revision { revision: 2, id })
                .await?
                .is_none()
        );
    }
    assert!(
        f.store
            .attachment_body(&f.owner, draft_lookup)
            .await?
            .is_none()
    );
    assert!(
        f.store
            .page_revision(&f.editor, 1)
            .await?
            .unwrap()
            .attachments
            .is_empty()
    );
    assert_eq!(
        f.store
            .page_revision(&f.editor, 2)
            .await?
            .unwrap()
            .attachments[0]
            .id,
        id
    );

    let source = "Сохранённый Markdown";
    assert!(matches!(
        f.store
            .save_page(
                &f.editor,
                2,
                PageUpdate::Source(source, PageContentFormat::Markdown),
                CoverUpdate::Keep
            )
            .await?,
        SavePageText::Saved
    ));
    assert_eq!(
        f.store.page_text(&f.viewer).await?.unwrap().attachments[0].publication,
        Some(first_publication)
    );
    assert!(matches!(
        f.store
            .save_page_with_attachments(
                &f.editor,
                3,
                PageUpdate::Keep,
                CoverUpdate::Keep,
                Some(AttachmentPatch {
                    draft: Uuid::new_v4(),
                    add: &[],
                    remove: &[id]
                })
            )
            .await?,
        SavePageText::Saved
    ));
    let after_remove = f.store.page_text(&f.owner).await?.unwrap();
    assert!(after_remove.attachments.is_empty());
    assert_eq!(after_remove.source, source);
    assert!(
        f.store
            .attachment_body(&f.owner, published)
            .await?
            .is_none()
    );
    assert!(
        f.store
            .attachment_body(&f.owner, AttachmentLookup::Revision { revision: 4, id })
            .await?
            .is_none()
    );
    assert_eq!(
        f.store
            .attachment_body(&f.editor, AttachmentLookup::Revision { revision: 3, id })
            .await?
            .unwrap()
            .bytes,
        bytes
    );

    for _ in 0..2 {
        assert_eq!(
            f.store.restore_page_revision(&f.owner, 3, 4).await?,
            RestorePageRevision::Saved(5)
        );
    }
    assert!(matches!(
        f.store
            .save_page_with_attachments(&f.owner, 1, PageUpdate::Keep, CoverUpdate::Keep, Some(add))
            .await?,
        SavePageText::Saved
    ));
    let restored = f.store.page_text(&f.viewer).await?.unwrap();
    assert_eq!(restored.source, source);
    assert_eq!(restored.content_revision, 5);
    let restored_file = &restored.attachments[0];
    assert_eq!(restored_file.id, id);
    let restored_publication = restored_file.publication.unwrap();
    assert_ne!(restored_publication, first_publication);
    assert!(
        f.store
            .attachment_body(&f.owner, published)
            .await?
            .is_none()
    );
    assert_eq!(
        f.store
            .attachment_body(&f.viewer, AttachmentLookup::Published(restored_publication))
            .await?
            .unwrap()
            .bytes,
        bytes
    );
    let stored_bytes: i64 = sqlx::query_scalar("SELECT count(*) FROM page_attachment_assets")
        .fetch_one(&f.store.0)
        .await?;
    assert_eq!(stored_bytes, 1);

    sqlx::query("DELETE FROM space_grants WHERE (tenant_id, space_id, account_id) = ($1, $2, $3)")
        .bind(f.editor.tenant)
        .bind(f.editor.space)
        .bind(f.editor.account)
        .execute(&f.store.0)
        .await?;
    assert!(
        f.store
            .attachment_body(&f.editor, AttachmentLookup::Published(restored_publication))
            .await?
            .is_none()
    );
    assert!(
        f.store
            .attachment_body(&f.editor, AttachmentLookup::Revision { revision: 3, id })
            .await?
            .is_none()
    );
    Ok(())
}

#[sqlx::test(migrator = "crate::db::MIGRATIONS")]
#[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
async fn attachment_publish_survives_conflict_and_checks_owner_and_expiration(
    pool: PgPool,
) -> anyhow::Result<()> {
    let f = setup(pool).await?;
    let draft = Uuid::new_v4();
    let id = Uuid::new_v4();
    assert!(matches!(
        f.store
            .stage_attachment(&f.editor, draft, id, "Файл.txt", None, b"text")
            .await?,
        StageAttachment::Saved(_)
    ));
    assert!(matches!(
        f.store
            .save_page(
                &f.owner,
                1,
                PageUpdate::PlainText("differ"),
                CoverUpdate::Keep
            )
            .await?,
        SavePageText::Saved
    ));
    let patch = AttachmentPatch {
        draft,
        add: &[id],
        remove: &[],
    };
    assert!(matches!(
        f.store.save_page_with_attachments(&f.editor, 1, PageUpdate::Keep, CoverUpdate::Keep, Some(patch)).await?,
        SavePageText::Conflict(page) if page.content_revision == 2 && page.attachments.is_empty()
    ));
    assert_eq!(
        f.store
            .draft_attachments(&f.editor, draft, &[id])
            .await?
            .len(),
        1
    );
    assert!(
        f.store
            .draft_attachments(&f.owner, draft, &[id])
            .await?
            .is_empty()
    );
    for (actor, invalid) in [
        (f.owner, patch),
        (
            f.editor,
            AttachmentPatch {
                draft: Uuid::new_v4(),
                ..patch
            },
        ),
    ] {
        assert!(matches!(
            f.store
                .save_page_with_attachments(
                    &actor,
                    2,
                    PageUpdate::PlainText("must not write"),
                    CoverUpdate::Set(PageCover::Orbit),
                    Some(invalid),
                )
                .await?,
            SavePageText::InvalidAttachments
        ));
    }
    let current = f.store.page_text(&f.owner).await?.unwrap();
    assert_eq!(current.source, "differ");
    assert_eq!(current.cover, PageCover::None);
    assert_eq!(current.content_revision, 2);
    sqlx::query("UPDATE page_attachment_assets SET staged_at = CURRENT_TIMESTAMP - INTERVAL '25 hours' WHERE id=$1")
        .bind(id).execute(&f.store.0).await?;
    assert!(
        f.store
            .draft_attachments(&f.editor, draft, &[id])
            .await?
            .is_empty()
    );
    assert!(
        f.store
            .attachment_body(&f.editor, AttachmentLookup::Draft { draft, id })
            .await?
            .is_none()
    );
    assert!(matches!(
        f.store
            .save_page_with_attachments(
                &f.editor,
                2,
                PageUpdate::Keep,
                CoverUpdate::Keep,
                Some(patch)
            )
            .await?,
        SavePageText::InvalidAttachments
    ));
    Ok(())
}

#[sqlx::test(migrator = "crate::db::MIGRATIONS")]
#[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
async fn attachment_sweeper_prunes_without_upload_and_keeps_active_and_published(
    pool: PgPool,
) -> anyhow::Result<()> {
    use std::time::Duration;

    let f = setup(pool).await?;
    let draft = Uuid::new_v4();
    let [startup_expired, next_expired, active, published] =
        std::array::from_fn(|_| Uuid::new_v4());
    for id in [startup_expired, next_expired, active, published] {
        assert!(matches!(
            f.store
                .stage_attachment(&f.owner, draft, id, "Вложение.txt", None, b"bytes")
                .await?,
            StageAttachment::Saved(_)
        ));
    }
    assert!(matches!(
        f.store
            .save_page_with_attachments(
                &f.owner,
                1,
                PageUpdate::Keep,
                CoverUpdate::Keep,
                Some(AttachmentPatch {
                    draft,
                    add: &[published],
                    remove: &[]
                }),
            )
            .await?,
        SavePageText::Saved
    ));
    let expire = "UPDATE page_attachment_assets
        SET staged_at = CURRENT_TIMESTAMP - INTERVAL '25 hours' WHERE id = ANY($1)";
    sqlx::query(expire)
        .bind(&[startup_expired, published][..])
        .execute(&f.store.0)
        .await?;

    let pool = &f.store.0;
    let wait_until_pruned = |id| async move {
        let present = "SELECT EXISTS(SELECT 1 FROM page_attachment_assets WHERE id=$1)";
        while sqlx::query_scalar::<_, bool>(present)
            .bind(id)
            .fetch_one(pool)
            .await?
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok::<_, sqlx::Error>(())
    };
    let check = async {
        wait_until_pruned(startup_expired).await?;
        sqlx::query(expire)
            .bind(&[next_expired][..])
            .execute(&f.store.0)
            .await?;
        wait_until_pruned(next_expired).await?;
        Ok::<_, sqlx::Error>(())
    };
    tokio::select! {
        _ = f.store.sweep_attachment_drafts(Duration::from_millis(10)) => unreachable!(),
        result = tokio::time::timeout(Duration::from_secs(3), check) => result??,
    }
    assert_eq!(
        f.store
            .draft_attachments(&f.owner, draft, &[active])
            .await?
            .len(),
        1
    );
    let stored = f.store.page_text(&f.viewer).await?.unwrap().attachments;
    assert_eq!(stored[0].id, published);
    assert_eq!(
        f.store
            .attachment_body(
                &f.owner,
                AttachmentLookup::Revision {
                    revision: 2,
                    id: published
                }
            )
            .await?
            .unwrap()
            .bytes,
        b"bytes",
    );
    Ok(())
}

#[sqlx::test(migrator = "crate::db::MIGRATIONS")]
#[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
async fn attachment_text_rebase_does_not_introduce_reference_to_retired_asset(
    pool: PgPool,
) -> anyhow::Result<()> {
    let f = setup(pool).await?;
    let draft = Uuid::new_v4();
    let id = Uuid::new_v4();
    assert!(matches!(
        f.store
            .stage_attachment(
                &f.owner,
                draft,
                id,
                "Схема.png",
                Some("image/png"),
                b"image"
            )
            .await?,
        StageAttachment::Saved(_)
    ));
    assert!(matches!(
        f.store
            .save_page_with_attachments(
                &f.owner,
                1,
                PageUpdate::Keep,
                CoverUpdate::Keep,
                Some(AttachmentPatch {
                    draft,
                    add: &[id],
                    remove: &[]
                }),
            )
            .await?,
        SavePageText::Saved
    ));
    assert!(matches!(
        f.store
            .save_page_with_attachments(
                &f.owner,
                2,
                PageUpdate::Keep,
                CoverUpdate::Keep,
                Some(AttachmentPatch {
                    draft,
                    add: &[],
                    remove: &[id]
                }),
            )
            .await?,
        SavePageText::Saved
    ));
    let draft_text = format!("Важный несохранённый текст\n\n![Схема](attachment:{id})");
    assert!(matches!(
        f.store.save_page(
            &f.editor, 2, PageUpdate::Source(&draft_text, PageContentFormat::Markdown), CoverUpdate::Keep,
        ).await?,
        SavePageText::Conflict(page) if page.content_revision == 3
    ));
    assert!(matches!(
        f.store
            .save_page(
                &f.editor,
                3,
                PageUpdate::Source(&draft_text, PageContentFormat::Markdown),
                CoverUpdate::Keep,
            )
            .await?,
        SavePageText::AttachmentRetired
    ));
    let unchanged = f.store.page_text(&f.viewer).await?.unwrap();
    assert_eq!(unchanged.content_revision, 3);
    assert!(unchanged.source.is_empty());
    assert!(unchanged.attachments.is_empty());

    let other = PageAddress {
        page: f.create_page("Другая страница").await?,
        ..f.owner
    };
    let other_id = Uuid::new_v4();
    assert!(matches!(
        f.store
            .stage_attachment(&other, draft, other_id, "Другое.txt", None, b"text")
            .await?,
        StageAttachment::Saved(_)
    ));
    assert!(matches!(
        f.store
            .save_page_with_attachments(
                &other,
                1,
                PageUpdate::Keep,
                CoverUpdate::Keep,
                Some(AttachmentPatch {
                    draft,
                    add: &[other_id],
                    remove: &[]
                }),
            )
            .await?,
        SavePageText::Saved
    ));
    let literal = format!(
        "История\n\n`![пример](attachment:{id})`\n\n[другая страница](attachment:{other_id})\n\n[старый текст](attachment:{})",
        Uuid::new_v4(),
    );
    assert!(matches!(
        f.store
            .save_page(
                &f.editor,
                3,
                PageUpdate::Source(&literal, PageContentFormat::Markdown),
                CoverUpdate::Keep,
            )
            .await?,
        SavePageText::Saved
    ));
    assert_eq!(f.store.page_text(&f.viewer).await?.unwrap().source, literal);
    Ok(())
}

#[sqlx::test(migrator = "crate::db::MIGRATIONS")]
#[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
async fn attachment_limits_allow_atomic_replacement_and_bound_draft_nonces(
    pool: PgPool,
) -> anyhow::Result<()> {
    let f = setup(pool).await?;
    let draft = Uuid::new_v4();
    let mut ids = Vec::new();
    for _ in 0..40 {
        let id = Uuid::new_v4();
        assert!(matches!(
            f.store
                .stage_attachment(&f.owner, draft, id, "Приложение.txt", None, b"x")
                .await?,
            StageAttachment::Saved(_)
        ));
        ids.push(id);
    }
    assert!(matches!(
        f.store
            .stage_attachment(&f.owner, draft, Uuid::new_v4(), "Лишний.txt", None, b"x")
            .await?,
        StageAttachment::Limit
    ));
    assert!(matches!(
        f.store
            .save_page_with_attachments(
                &f.owner,
                1,
                PageUpdate::Keep,
                CoverUpdate::Keep,
                Some(AttachmentPatch {
                    draft,
                    add: &ids[..20],
                    remove: &[],
                }),
            )
            .await?,
        SavePageText::Saved
    ));
    assert!(matches!(
        f.store
            .save_page_with_attachments(
                &f.owner,
                2,
                PageUpdate::Keep,
                CoverUpdate::Keep,
                Some(AttachmentPatch {
                    draft,
                    add: &ids[20..21],
                    remove: &[],
                }),
            )
            .await?,
        SavePageText::InvalidAttachments
    ));
    let current = f.store.page_text(&f.owner).await?.unwrap();
    assert_eq!(current.attachments.len(), 20);
    assert_eq!(current.content_revision, 2);
    assert!(matches!(
        f.store
            .save_page_with_attachments(
                &f.owner,
                2,
                PageUpdate::Keep,
                CoverUpdate::Keep,
                Some(AttachmentPatch {
                    draft,
                    add: &ids[20..21],
                    remove: &ids[..1],
                }),
            )
            .await?,
        SavePageText::Saved
    ));
    let replaced = f.store.page_text(&f.viewer).await?.unwrap();
    assert_eq!(replaced.attachments.len(), 20);
    assert_eq!(replaced.content_revision, 3);
    assert!(replaced.attachments.iter().any(|file| file.id == ids[20]));
    assert!(replaced.attachments.iter().all(|file| file.id != ids[0]));

    // 19 staged files remain from the original draft.
    for _ in 19..120 {
        assert!(matches!(
            f.store
                .stage_attachment(
                    &f.owner,
                    Uuid::new_v4(),
                    Uuid::new_v4(),
                    "Ещё один.txt",
                    None,
                    b"x",
                )
                .await?,
            StageAttachment::Saved(_)
        ));
    }
    assert!(matches!(
        f.store
            .stage_attachment(
                &f.owner,
                Uuid::new_v4(),
                Uuid::new_v4(),
                "Лишний.txt",
                None,
                b"x",
            )
            .await?,
        StageAttachment::Limit
    ));
    assert!(matches!(
        f.store
            .stage_attachment(
                &f.editor,
                Uuid::new_v4(),
                Uuid::new_v4(),
                "Файл коллеги.txt",
                None,
                b"x",
            )
            .await?,
        StageAttachment::Saved(_)
    ));
    Ok(())
}

#[sqlx::test(migrator = "crate::db::MIGRATIONS")]
#[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
async fn attachment_remove_rejects_new_reference_in_rebased_source(
    pool: PgPool,
) -> anyhow::Result<()> {
    let f = setup(pool).await?;
    let draft = Uuid::new_v4();
    let id = Uuid::new_v4();
    assert!(matches!(
        f.store
            .stage_attachment(&f.owner, draft, id, "Файл.txt", None, b"text")
            .await?,
        StageAttachment::Saved(_)
    ));
    let add = [id];
    assert!(matches!(
        f.store
            .save_page_with_attachments(
                &f.owner,
                1,
                PageUpdate::Keep,
                CoverUpdate::Keep,
                Some(AttachmentPatch {
                    draft,
                    add: &add,
                    remove: &[]
                }),
            )
            .await?,
        SavePageText::Saved
    ));
    let link = format!("[Приложение](attachment:{id})");
    assert!(matches!(
        f.store
            .save_page(
                &f.editor,
                2,
                PageUpdate::Source(&link, PageContentFormat::Markdown),
                CoverUpdate::Keep,
            )
            .await?,
        SavePageText::Saved
    ));
    let remove = [id];
    let patch = AttachmentPatch {
        draft,
        add: &[],
        remove: &remove,
    };
    assert!(matches!(
        f.store.save_page_with_attachments(
            &f.owner, 2, PageUpdate::Keep, CoverUpdate::Keep, Some(patch),
        ).await?,
        SavePageText::Conflict(page) if page.content_revision == 3
    ));
    assert!(matches!(
        f.store
            .save_page_with_attachments(
                &f.owner,
                3,
                PageUpdate::Keep,
                CoverUpdate::Keep,
                Some(patch),
            )
            .await?,
        SavePageText::AttachmentReferencesPresent
    ));
    let unchanged = f.store.page_text(&f.viewer).await?.unwrap();
    assert_eq!(unchanged.content_revision, 3);
    assert_eq!(unchanged.source, link);
    assert_eq!(unchanged.attachments.len(), 1);
    let fenced = format!("\x60\x60\x60md\n{link}\n\x60\x60\x60");
    assert!(matches!(
        f.store
            .save_page_with_attachments(
                &f.owner,
                3,
                PageUpdate::Source(&fenced, PageContentFormat::Markdown),
                CoverUpdate::Keep,
                Some(patch),
            )
            .await?,
        SavePageText::Saved
    ));
    let saved = f.store.page_text(&f.viewer).await?.unwrap();
    assert_eq!(saved.source, fenced);
    assert_eq!(saved.content_revision, 4);
    assert!(saved.attachments.is_empty());
    Ok(())
}
