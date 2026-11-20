use sqlx::PgPool;
use uuid::Uuid;

use super::*;
use crate::{
    db::{CoverUpdate, CreatePage, CreatePageInput, PageUpdate, SavePageText},
    text::fold_name,
};

mod attachments;

struct Fixture {
    store: Store,
    owner: PageAddress,
    editor: PageAddress,
    viewer: PageAddress,
    admin: PageAddress,
    section: Uuid,
}

impl Fixture {
    async fn create_page(&self, title: &str) -> anyhow::Result<Uuid> {
        let answer = self
            .store
            .create_page(CreatePageInput {
                tenant_id: self.owner.tenant,
                space_id: self.owner.space,
                section_id: self.section,
                account_id: self.owner.account,
                operation_id: Uuid::new_v4(),
                title,
            })
            .await?;
        match answer {
            CreatePage::Saved(id) => Ok(id),
            _ => anyhow::bail!("fixture page did not save"),
        }
    }
}

async fn setup(pool: PgPool) -> anyhow::Result<Fixture> {
    let [tenant, space, owner, editor, viewer, admin, section] =
        std::array::from_fn(|_| Uuid::new_v4());
    sqlx::query("INSERT INTO tenants(id,name) VALUES($1,'Север')")
        .bind(tenant)
        .execute(&pool)
        .await?;
    for (id, email, name, role) in [
        (owner, "owner@example.test", "Владелец", "member"),
        (editor, "editor@example.test", "Редактор", "member"),
        (viewer, "viewer@example.test", "Читатель", "member"),
        (admin, "admin@example.test", "Администратор", "admin"),
    ] {
        sqlx::query(
            "INSERT INTO accounts(id,email,name,salt,password_hash) VALUES($1,$2,$3,$4,$5)",
        )
        .bind(id)
        .bind(email)
        .bind(name)
        .bind(vec![0_u8; 16])
        .bind(vec![0_u8; 32])
        .execute(&pool)
        .await?;
        sqlx::query("INSERT INTO tenant_memberships VALUES($1,$2,$3,'active',false,false)")
            .bind(tenant)
            .bind(id)
            .bind(role)
            .execute(&pool)
            .await?;
    }
    sqlx::query(
        "INSERT INTO spaces(tenant_id,id,creator_account_id,owner_account_id,name,name_key)
         VALUES($1,$2,$3,$3,'Документы','документы')",
    )
    .bind(tenant)
    .bind(space)
    .bind(owner)
    .execute(&pool)
    .await?;
    sqlx::query("INSERT INTO space_grants VALUES($1,$2,$3,'editor'),($1,$2,$4,'viewer')")
        .bind(tenant)
        .bind(space)
        .bind(editor)
        .bind(viewer)
        .execute(&pool)
        .await?;
    sqlx::query(
        "INSERT INTO sections(tenant_id,space_id,id,creator_account_id,title,title_key,position)
         VALUES($1,$2,$3,$4,'Раздел','раздел',1)",
    )
    .bind(tenant)
    .bind(space)
    .bind(section)
    .bind(owner)
    .execute(&pool)
    .await?;
    let address = |account| PageAddress {
        tenant,
        space,
        page: Uuid::nil(),
        account,
    };
    let mut fixture = Fixture {
        store: Store(pool),
        owner: address(owner),
        editor: address(editor),
        viewer: address(viewer),
        admin: address(admin),
        section,
    };
    let page = fixture.create_page("План").await?;
    for address in [
        &mut fixture.owner,
        &mut fixture.editor,
        &mut fixture.viewer,
        &mut fixture.admin,
    ] {
        address.page = page;
    }
    Ok(fixture)
}

async fn rename(fixture: &Fixture, title: &str) -> sqlx::Result<()> {
    sqlx::query("UPDATE pages SET title=$4, title_key=$5 WHERE (tenant_id,space_id,id)=($1,$2,$3)")
        .bind(fixture.owner.tenant)
        .bind(fixture.owner.space)
        .bind(fixture.owner.page)
        .bind(title.as_bytes())
        .bind(fold_name(title).into_bytes())
        .execute(&fixture.store.0)
        .await?;
    Ok(())
}

async fn staged_cover(f: &Fixture) -> anyhow::Result<Uuid> {
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
        .encode_image(&image::RgbImage::new(1000, 340))?;
    let address = f.owner;
    let asset = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO page_cover_assets (tenant_id, space_id, page_id, id, uploader_account_id, jpeg)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .bind(asset)
    .bind(address.account)
    .bind(jpeg)
    .execute(&f.store.0)
    .await?;
    Ok(asset)
}

#[sqlx::test(migrator = "crate::db::MIGRATIONS")]
#[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
async fn page_history_keeps_exact_baseline_and_cursor_order(pool: PgPool) -> anyhow::Result<()> {
    let f = setup(pool).await?;
    let origin = f.store.page_revision(&f.owner, 1).await?.unwrap();
    assert_eq!(origin.author.as_deref(), Some("Владелец"));
    assert!(origin.recorded_at.as_deref().unwrap().ends_with('Z'));

    sqlx::query("DELETE FROM page_revisions WHERE (tenant_id,space_id,page_id)=($1,$2,$3)")
        .bind(f.owner.tenant)
        .bind(f.owner.space)
        .bind(f.owner.page)
        .execute(&f.store.0)
        .await?;
    let initial_source = "\r\n\\# До миграции  \r\n";
    sqlx::query(
        "UPDATE pages SET content_revision=7, markdown=$4 WHERE (tenant_id,space_id,id)=($1,$2,$3)",
    )
    .bind(f.owner.tenant)
    .bind(f.owner.space)
    .bind(f.owner.page)
    .bind(initial_source)
    .execute(&f.store.0)
    .await?;

    let initial = f.store.page_history(&f.editor, None).await?.unwrap();
    assert_eq!(initial.current_revision, 7);
    assert_eq!(initial.items.len(), 1);
    assert_eq!(initial.items[0].revision, 7);
    assert!(initial.items[0].author.is_none());
    assert!(initial.items[0].recorded_at.is_none());
    assert_eq!(
        f.store.page_revision(&f.owner, 7).await?.unwrap().source,
        initial_source
    );
    assert!(matches!(
        f.store
            .save_page(&f.editor, 7, PageUpdate::Keep, CoverUpdate::Keep)
            .await?,
        SavePageText::Saved
    ));
    let stored: i64 = sqlx::query_scalar("SELECT count(*) FROM page_revisions")
        .fetch_one(&f.store.0)
        .await?;
    assert_eq!(stored, 0);

    for based_on in 7..=27 {
        let markdown = format!("# Запись {}\n", based_on + 1);
        assert!(matches!(
            f.store
                .save_page(
                    &f.editor,
                    based_on,
                    PageUpdate::Source(&markdown, PageContentFormat::Markdown),
                    CoverUpdate::Keep,
                )
                .await?,
            SavePageText::Saved
        ));
    }
    let first = f.store.page_history(&f.owner, None).await?.unwrap();
    assert_eq!(first.current_revision, 28);
    assert_eq!(first.items.len(), 20);
    assert_eq!(first.items[0].revision, 28);
    assert_eq!(first.items[0].author.as_deref(), Some("Редактор"));
    assert_eq!(first.next_before, Some(9));
    let second = f
        .store
        .page_history(&f.owner, first.next_before)
        .await?
        .unwrap();
    assert_eq!(
        second.items.iter().map(|m| m.revision).collect::<Vec<_>>(),
        [8, 7]
    );
    assert!(second.next_before.is_none());
    assert!(
        f.store
            .page_history(&f.owner, Some(7))
            .await?
            .unwrap()
            .items
            .is_empty()
    );

    let (before, after) = f
        .store
        .compare_page_revisions(&f.owner, 7, 28)
        .await?
        .unwrap();
    assert_eq!(before.source, initial_source);
    assert_eq!(before.format, PageContentFormat::Literal);
    assert_eq!(after.source, "# Запись 28\n");
    assert_eq!(after.format, PageContentFormat::Markdown);
    assert!(
        f.store
            .compare_page_revisions(&f.owner, 28, 29)
            .await?
            .is_none()
    );

    for denied in [f.viewer, f.admin] {
        assert!(f.store.page_text(&denied).await?.is_some());
        assert!(f.store.page_history(&denied, None).await?.is_none());
        assert!(f.store.page_revision(&denied, 28).await?.is_none());
        assert!(
            f.store
                .compare_page_revisions(&denied, 7, 28)
                .await?
                .is_none()
        );
    }
    Ok(())
}

#[sqlx::test(migrator = "crate::db::MIGRATIONS")]
#[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
async fn restore_keeps_archived_bytes_and_old_addresses_closed(pool: PgPool) -> anyhow::Result<()> {
    let f = setup(pool).await?;
    let original_asset = staged_cover(&f).await?;
    let original_source = "## Оригинал  \n\n**Markdown**";
    assert!(matches!(
        f.store
            .save_page(
                &f.owner,
                1,
                PageUpdate::Source(original_source, PageContentFormat::Markdown),
                CoverUpdate::Set(PageCover::Image(original_asset)),
            )
            .await?,
        SavePageText::Saved
    ));
    let original = f
        .store
        .page_cover_image(&f.viewer, original_asset)
        .await?
        .unwrap();
    assert_eq!(
        f.store.page_revision_cover(&f.owner, 2).await?.unwrap(),
        original
    );

    rename(&f, "Следующий план").await?;
    assert!(matches!(
        f.store
            .save_page(
                &f.editor,
                2,
                PageUpdate::PlainText("Новый текст"),
                CoverUpdate::Set(PageCover::Grid)
            )
            .await?,
        SavePageText::Saved
    ));
    assert!(
        f.store
            .page_cover_image(&f.owner, original_asset)
            .await?
            .is_none()
    );
    assert_eq!(
        f.store.page_revision_cover(&f.editor, 2).await?.unwrap(),
        original
    );
    assert!(f.store.page_revision_cover(&f.viewer, 2).await?.is_none());
    assert!(f.store.page_revision_cover(&f.admin, 2).await?.is_none());
    let wrong_page = PageAddress {
        page: f.create_page("План").await?,
        ..f.owner
    };
    assert!(f.store.page_revision_cover(&wrong_page, 2).await?.is_none());
    assert_eq!(
        f.store.restore_page_revision(&f.owner, 2, 2).await?,
        RestorePageRevision::Conflict(3)
    );
    assert_eq!(
        f.store.restore_page_revision(&f.owner, 2, 3).await?,
        RestorePageRevision::NameTaken
    );
    let after_collision = f.store.page_history(&f.owner, None).await?.unwrap();
    assert_eq!(after_collision.current_revision, 3);
    assert_eq!(after_collision.items.len(), 3);
    assert_eq!(
        f.store.page_revision(&f.owner, 3).await?.unwrap().title,
        "Следующий план"
    );
    assert_eq!(
        f.store.restore_page_revision(&f.viewer, 2, 3).await?,
        RestorePageRevision::Missing
    );
    sqlx::query("UPDATE pages SET title='Другая',title_key='другая' WHERE (tenant_id,space_id,id)=($1,$2,$3)")
        .bind(wrong_page.tenant).bind(wrong_page.space).bind(wrong_page.page)
        .execute(&f.store.0).await?;
    assert_eq!(
        f.store.restore_page_revision(&f.owner, 2, 3).await?,
        RestorePageRevision::Saved(4)
    );
    assert_eq!(
        f.store.restore_page_revision(&f.owner, 2, 3).await?,
        RestorePageRevision::Saved(4)
    );
    let restarted = f.store.page_revision(&f.owner, 4).await?.unwrap();
    assert_eq!(restarted.title, "План");
    assert_eq!(restarted.source, original_source);
    assert_eq!(restarted.format, PageContentFormat::Markdown);
    assert_eq!(restarted.cover, PageCover::Image(original_asset));
    let PageCover::Image(new_asset) = f.store.page_text(&f.owner).await?.unwrap().cover else {
        anyhow::bail!("restored revision lost its cover");
    };
    assert_ne!(new_asset, original_asset);
    assert_eq!(
        f.store
            .page_cover_image(&f.viewer, new_asset)
            .await?
            .unwrap(),
        original
    );
    assert!(
        f.store
            .page_cover_image(&f.owner, original_asset)
            .await?
            .is_none()
    );
    assert_eq!(
        f.store.page_revision_cover(&f.owner, 4).await?.unwrap(),
        original
    );
    assert!(f.store.page_revision(&f.owner, 3).await?.is_some());

    // The identical older payload is still a distinct restore requested on a new base.
    assert_eq!(
        f.store.restore_page_revision(&f.owner, 2, 4).await?,
        RestorePageRevision::Saved(5)
    );
    assert_eq!(
        f.store.restore_page_revision(&f.owner, 2, 3).await?,
        RestorePageRevision::Saved(4)
    );
    assert_eq!(
        f.store
            .page_history(&f.owner, None)
            .await?
            .unwrap()
            .current_revision,
        5
    );
    assert!(
        f.store
            .page_cover_image(&f.owner, new_asset)
            .await?
            .is_none()
    );
    assert!(matches!(
        f.store
            .save_page(
                &f.owner,
                5,
                PageUpdate::PlainText("Текст после восстановления"),
                CoverUpdate::Keep
            )
            .await?,
        SavePageText::Saved
    ));
    assert_eq!(
        f.store.page_revision_cover(&f.owner, 6).await?.unwrap(),
        original
    );
    let archived_covers: i64 = sqlx::query_scalar("SELECT count(*) FROM page_revision_covers")
        .fetch_one(&f.store.0)
        .await?;
    assert_eq!(archived_covers, 1);

    let (owner_restore, editor_restore) = tokio::join!(
        f.store.restore_page_revision(&f.owner, 2, 6),
        f.store.restore_page_revision(&f.editor, 2, 6),
    );
    match (owner_restore?, editor_restore?) {
        (RestorePageRevision::Saved(7), RestorePageRevision::Conflict(7))
        | (RestorePageRevision::Conflict(7), RestorePageRevision::Saved(7)) => {}
        results => anyhow::bail!("expected one publication and one conflict, got {results:?}"),
    }

    sqlx::query("DELETE FROM space_grants WHERE (tenant_id,space_id,account_id)=($1,$2,$3)")
        .bind(f.editor.tenant)
        .bind(f.editor.space)
        .bind(f.editor.account)
        .execute(&f.store.0)
        .await?;
    assert!(f.store.page_revision_cover(&f.editor, 2).await?.is_none());
    assert_eq!(
        f.store.restore_page_revision(&f.editor, 2, 7).await?,
        RestorePageRevision::Missing
    );
    Ok(())
}

#[sqlx::test(migrator = "crate::db::MIGRATIONS")]
#[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
async fn legacy_cover_is_archived_before_the_first_replacement(pool: PgPool) -> anyhow::Result<()> {
    let f = setup(pool).await?;
    let asset = staged_cover(&f).await?;
    assert!(matches!(
        f.store
            .save_page(
                &f.owner,
                1,
                PageUpdate::Keep,
                CoverUpdate::Set(PageCover::Image(asset))
            )
            .await?,
        SavePageText::Saved
    ));
    let jpeg = f.store.page_cover_image(&f.viewer, asset).await?.unwrap();
    sqlx::query("DELETE FROM page_revisions WHERE (tenant_id,space_id,page_id)=($1,$2,$3)")
        .bind(f.owner.tenant)
        .bind(f.owner.space)
        .bind(f.owner.page)
        .execute(&f.store.0)
        .await?;
    sqlx::query("DELETE FROM page_revision_covers WHERE (tenant_id,space_id,page_id)=($1,$2,$3)")
        .bind(f.owner.tenant)
        .bind(f.owner.space)
        .bind(f.owner.page)
        .execute(&f.store.0)
        .await?;
    assert_eq!(
        f.store.page_revision_cover(&f.owner, 2).await?.unwrap(),
        jpeg
    );
    assert!(f.store.page_revision_cover(&f.viewer, 2).await?.is_none());

    assert!(matches!(
        f.store
            .save_page(
                &f.owner,
                2,
                PageUpdate::Keep,
                CoverUpdate::Set(PageCover::None)
            )
            .await?,
        SavePageText::Saved
    ));
    assert!(f.store.page_cover_image(&f.owner, asset).await?.is_none());
    assert_eq!(
        f.store.page_revision_cover(&f.owner, 2).await?.unwrap(),
        jpeg
    );
    let baseline = f.store.page_revision(&f.owner, 2).await?.unwrap();
    assert_eq!(baseline.cover, PageCover::Image(asset));
    assert!(baseline.author.is_none());
    assert!(baseline.recorded_at.is_none());
    Ok(())
}

#[sqlx::test(migrator = "crate::db::MIGRATIONS")]
#[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
async fn publication_notices_follow_real_revisions_and_current_access(
    pool: PgPool,
) -> anyhow::Result<()> {
    use crate::db::page_discussion::NotificationTarget;

    let f = setup(pool).await?;
    let store = &f.store;
    assert_eq!(store.page_watch(&f.viewer).await?, Some(false));
    assert_eq!(store.set_page_watch(&f.viewer, true).await?, Some(true));
    assert_eq!(store.set_page_watch(&f.viewer, true).await?, Some(true));
    assert_eq!(store.set_page_watch(&f.owner, true).await?, Some(true));
    assert!(
        store
            .discussion_notifications(f.viewer.account, 0)
            .await?
            .notifications
            .is_empty()
    );

    for _ in 0..2 {
        assert!(matches!(
            store
                .save_page(
                    &f.owner,
                    1,
                    PageUpdate::Source("Пилот", PageContentFormat::Markdown),
                    CoverUpdate::Keep
                )
                .await?,
            SavePageText::Saved
        ));
    }
    let notices = store
        .discussion_notifications(f.viewer.account, 0)
        .await?
        .notifications;
    assert_eq!(notices.len(), 1);
    assert_eq!(
        notices[0].target,
        NotificationTarget::Publication {
            title: "План".into(),
            revision: 2
        }
    );
    assert!(
        store
            .discussion_notifications(f.owner.account, 0)
            .await?
            .notifications
            .is_empty()
    );

    sqlx::query("DELETE FROM space_grants WHERE (tenant_id,space_id,account_id)=($1,$2,$3)")
        .bind(f.viewer.tenant)
        .bind(f.viewer.space)
        .bind(f.viewer.account)
        .execute(&store.0)
        .await?;
    assert_eq!(store.page_watch(&f.viewer).await?, None);
    assert_eq!(store.set_page_watch(&f.viewer, true).await?, None);
    assert!(
        store
            .discussion_notifications(f.viewer.account, 0)
            .await?
            .notifications
            .is_empty()
    );
    assert!(matches!(
        store
            .save_page(
                &f.owner,
                2,
                PageUpdate::Source("Пилот 2", PageContentFormat::Markdown),
                CoverUpdate::Keep
            )
            .await?,
        SavePageText::Saved
    ));
    sqlx::query("INSERT INTO space_grants VALUES ($1,$2,$3,'viewer')")
        .bind(f.viewer.tenant)
        .bind(f.viewer.space)
        .bind(f.viewer.account)
        .execute(&store.0)
        .await?;

    for _ in 0..2 {
        assert!(matches!(
            store.restore_page_revision(&f.editor, 2, 3).await?,
            RestorePageRevision::Saved(4)
        ));
    }
    let mut unsubscribe = store.0.begin().await?;
    assert!(
        crate::db::page_access::lock_page_for_change(&mut unsubscribe, &f.viewer)
            .await?
            .is_some_and(|access| access.can_read)
    );
    sqlx::query(
        "DELETE FROM page_watchers WHERE (tenant_id,space_id,page_id,account_id)=($1,$2,$3,$4)",
    )
    .bind(f.viewer.tenant)
    .bind(f.viewer.space)
    .bind(f.viewer.page)
    .bind(f.viewer.account)
    .execute(&mut *unsubscribe)
    .await?;
    let (save, committed) = tokio::join!(
        store.save_page(
            &f.owner,
            4,
            PageUpdate::Source("Пилот 3", PageContentFormat::Markdown),
            CoverUpdate::Keep
        ),
        unsubscribe.commit(),
    );
    committed?;
    assert!(matches!(save?, SavePageText::Saved));
    assert_eq!(store.page_watch(&f.viewer).await?, Some(false));
    let notices = store
        .discussion_notifications(f.viewer.account, 0)
        .await?
        .notifications;
    let target = |revision| NotificationTarget::Publication {
        title: "План".into(),
        revision,
    };
    assert_eq!(notices.len(), 2);
    for revision in [2, 4] {
        assert!(
            notices
                .iter()
                .any(|notice| notice.target == target(revision))
        );
    }
    let own_notices = store
        .discussion_notifications(f.owner.account, 0)
        .await?
        .notifications;
    assert_eq!(own_notices.len(), 1);
    assert_eq!(own_notices[0].target, target(4));
    Ok(())
}

#[sqlx::test(migrator = "crate::db::MIGRATIONS")]
#[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
async fn guarded_publication_keeps_team_changes_and_replays_exactly_once(
    pool: PgPool,
) -> anyhow::Result<()> {
    use crate::db::{CoeditPublication, NewSession, coediting::CoeditResult};

    let f = setup(pool).await?;
    let store = &f.store;
    let session = [9_u8; 32];
    store
        .add_session(
            f.owner.account,
            &NewSession {
                digest: session.to_vec(),
                created_at: crate::clock::now_ms(),
                expires_at: crate::clock::now_ms() + 60_000,
            },
        )
        .await?;
    let CoeditResult::Saved(open) = store.open_coedit(&f.owner, &session).await? else {
        anyhow::bail!("coedit did not open");
    };
    let input = serde_json::from_value(serde_json::json!({
        "lease": open.lease,
        "operationId": Uuid::new_v4(),
        "version": open.version.to_string(),
        "start": 0,
        "end": 0,
        "text": "Общий черновик",
        "format": "markdown"
    }))?;
    let CoeditResult::Saved(draft) = store.apply_coedit(&f.owner, &session, input).await? else {
        anyhow::bail!("coedit did not apply");
    };
    let operation = Uuid::new_v4();
    let permit = |version| CoeditPublication {
        session,
        lease: open.lease,
        version,
        operation,
        cover_base: None,
    };
    let guarded_save = |version, source| {
        store.publish_coedit(
            &f.owner,
            open.published_revision,
            PageUpdate::Source(source, PageContentFormat::Markdown),
            CoverUpdate::Keep,
            None,
            permit(version),
        )
    };
    assert!(matches!(
        guarded_save(open.version, &draft.source).await?,
        SavePageText::CoeditConflict(..)
    ));
    assert!(matches!(
        guarded_save(draft.version, "Старый текст").await?,
        SavePageText::CoeditConflict(..)
    ));
    assert!(matches!(
        store
            .save_page(
                &f.editor,
                open.published_revision,
                PageUpdate::Source("Локальная правка", PageContentFormat::Markdown),
                CoverUpdate::Keep
            )
            .await?,
        SavePageText::CoeditConflict(..)
    ));
    assert_eq!(
        store.page_text(&f.viewer).await?.unwrap().source,
        open.source
    );
    for _ in 0..2 {
        assert!(matches!(
            guarded_save(draft.version, &draft.source).await?,
            SavePageText::Saved
        ));
    }
    assert!(matches!(
        guarded_save(draft.version, "Повтор с изменениями").await?,
        SavePageText::CoeditOperationConflict
    ));
    let published = store.page_text(&f.viewer).await?.unwrap();
    assert_eq!(published.source, draft.source);
    assert_eq!(published.content_revision, open.published_revision + 1);

    let file = Uuid::new_v4();
    let upload = Uuid::new_v4();
    assert!(matches!(
        store
            .stage_attachment(&f.owner, upload, file, "Черновик.md", None, b"# Note")
            .await?,
        crate::db::StageAttachment::Saved(_)
    ));
    let lookup = crate::db::AttachmentLookup::Draft {
        draft: upload,
        id: file,
    };
    assert!(store.attachment_body(&f.editor, lookup).await?.is_none());
    let CoeditResult::Saved(after) = store.coedit_state(&f.owner, &session, open.lease).await?
    else {
        anyhow::bail!("coedit did not advance to publication");
    };
    let marker = format!("[Заметка](attachment:{file})");
    let shared_attempt = serde_json::from_value(serde_json::json!({
        "lease": after.lease,
        "operationId": Uuid::new_v4(),
        "version": after.version.to_string(),
        "start": 0,
        "end": 0,
        "text": marker,
        "format": "markdown"
    }))?;
    assert!(matches!(
        store
            .apply_coedit(&f.owner, &session, shared_attempt)
            .await?,
        CoeditResult::Invalid
    ));
    let source = format!("{marker}{}", after.source);
    let operation = Uuid::new_v4();
    let patch = crate::db::AttachmentPatch {
        draft: upload,
        add: &[file],
        remove: &[],
    };
    for _ in 0..2 {
        assert!(matches!(
            store
                .publish_coedit(
                    &f.owner,
                    after.published_revision,
                    PageUpdate::Source(&source, PageContentFormat::Markdown),
                    CoverUpdate::Keep,
                    Some(patch),
                    CoeditPublication {
                        session,
                        lease: after.lease,
                        version: after.version,
                        operation,
                        cover_base: None,
                    },
                )
                .await?,
            SavePageText::Saved
        ));
    }
    let published_with_file = store.page_text(&f.viewer).await?.unwrap();
    assert_eq!(published_with_file.source, source);
    assert_eq!(
        published_with_file.content_revision,
        after.published_revision + 1
    );
    assert_eq!(published_with_file.attachments.len(), 1);
    assert_eq!(published_with_file.attachments[0].id, file);
    Ok(())
}

#[sqlx::test(migrator = "crate::db::MIGRATIONS")]
#[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
async fn guarded_publication_compares_cover_even_after_text_catches_up(
    pool: PgPool,
) -> anyhow::Result<()> {
    use crate::db::{CoeditPublication, NewSession, coediting::CoeditResult};

    let f = setup(pool).await?;
    let store = &f.store;
    let owner_session = [1_u8; 32];
    let editor_session = [2_u8; 32];
    let now = crate::clock::now_ms();
    for (address, session) in [(f.owner, owner_session), (f.editor, editor_session)] {
        store
            .add_session(
                address.account,
                &NewSession {
                    digest: session.to_vec(),
                    created_at: now,
                    expires_at: now + 60_000,
                },
            )
            .await?;
    }
    let CoeditResult::Saved(owner) = store.open_coedit(&f.owner, &owner_session).await? else {
        anyhow::bail!("owner cannot open coedit");
    };
    let CoeditResult::Saved(editor) = store.open_coedit(&f.editor, &editor_session).await? else {
        anyhow::bail!("editor cannot open coedit");
    };
    let original = store.page_text(&f.editor).await?.unwrap();
    assert!(matches!(
        store
            .publish_coedit(
                &f.owner,
                owner.published_revision,
                PageUpdate::Source(&owner.source, owner.format),
                CoverUpdate::Set(PageCover::Orbit),
                None,
                CoeditPublication {
                    session: owner_session,
                    lease: owner.lease,
                    version: owner.version,
                    operation: Uuid::new_v4(),
                    cover_base: Some(original.cover),
                },
            )
            .await?,
        SavePageText::Saved
    ));
    let CoeditResult::Saved(latest) = store
        .coedit_state(&f.editor, &editor_session, editor.lease)
        .await?
    else {
        anyhow::bail!("editor cannot catch up");
    };
    assert_eq!(latest.published_revision, owner.published_revision + 1);
    let save = |selected, cover_base| {
        store.publish_coedit(
            &f.editor,
            latest.published_revision,
            PageUpdate::Source(&latest.source, latest.format),
            CoverUpdate::Set(selected),
            None,
            CoeditPublication {
                session: editor_session,
                lease: latest.lease,
                version: latest.version,
                operation: Uuid::new_v4(),
                cover_base,
            },
        )
    };

    assert!(matches!(
        save(PageCover::Aurora, Some(original.cover)).await?,
        SavePageText::Conflict(current)
            if current.cover == PageCover::Orbit
                && current.content_revision == latest.published_revision
    ));
    assert!(matches!(
        save(PageCover::Aurora, None).await?,
        SavePageText::InvalidCover
    ));
    assert!(matches!(
        save(PageCover::Orbit, Some(original.cover)).await?,
        SavePageText::Saved
    ));
    let published = store.page_text(&f.viewer).await?.unwrap();
    assert_eq!(published.cover, PageCover::Orbit);
    assert_eq!(published.content_revision, latest.published_revision);

    assert!(matches!(
        save(PageCover::Aurora, Some(PageCover::Orbit)).await?,
        SavePageText::Saved
    ));
    let published = store.page_text(&f.viewer).await?.unwrap();
    assert_eq!(published.cover, PageCover::Aurora);
    assert_eq!(published.content_revision, latest.published_revision + 1);
    Ok(())
}
