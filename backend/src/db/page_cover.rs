use std::time::Duration;

use sqlx::{PgConnection, PgPool};
use tokio::time::timeout;
use uuid::Uuid;

use crate::cover_image::CoverImage;

use super::{
    Store,
    page_access::{PAGE_ACCESS, PageAddress, PageEditAccess, lock_page_for_edit},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PageCover {
    None,
    Orbit,
    Aurora,
    Grid,
    Image(Uuid),
}

impl PageCover {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "none" => Some(Self::None),
            "orbit" => Some(Self::Orbit),
            "aurora" => Some(Self::Aurora),
            "grid" => Some(Self::Grid),
            _ => {
                let raw = value.strip_prefix("image:")?;
                let id = Uuid::parse_str(raw).ok()?;
                (id.to_string() == raw).then_some(Self::Image(id))
            }
        }
    }

    pub(crate) fn value(self) -> String {
        match self {
            Self::Image(id) => format!("image:{id}"),
            preset => preset.theme().unwrap_or("none").to_owned(),
        }
    }

    fn theme(self) -> Option<&'static str> {
        match self {
            Self::Orbit => Some("orbit"),
            Self::Aurora => Some("aurora"),
            Self::Grid => Some("grid"),
            Self::None | Self::Image(_) => None,
        }
    }

    fn asset_id(self) -> Option<Uuid> {
        match self {
            Self::Image(id) => Some(id),
            _ => None,
        }
    }

    pub(super) fn from_storage(theme: Option<&str>, asset_id: Option<Uuid>) -> Self {
        if let Some(id) = asset_id {
            return Self::Image(id);
        }
        match theme {
            None => Self::None,
            Some("orbit") => Self::Orbit,
            Some("aurora") => Self::Aurora,
            Some("grid") => Self::Grid,
            _ => unreachable!("page_covers.theme is constrained to the three preset values"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CoverUpdate {
    Keep,
    Set(PageCover),
}

pub(crate) enum StagePageCover {
    Missing,
    Forbidden,
    Saved(Uuid),
}

impl Store {
    pub(crate) async fn stage_page_cover(
        &self,
        address: &PageAddress,
        image: CoverImage,
    ) -> Result<StagePageCover, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        match lock_page_for_edit(&mut tx, address).await? {
            PageEditAccess::Missing => return Ok(StagePageCover::Missing),
            PageEditAccess::Forbidden => return Ok(StagePageCover::Forbidden),
            PageEditAccess::Allowed => {}
        }
        let asset: Uuid = sqlx::query_scalar(
            "INSERT INTO page_cover_assets
                 (tenant_id, space_id, page_id, id, uploader_account_id, jpeg)
             VALUES ($1, $2, $3, $4, $5, $6)
             RETURNING id",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(Uuid::new_v4())
        .bind(address.account)
        .bind(image.into_bytes())
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        match timeout(Duration::from_millis(250), prune_expired_covers(&self.0)).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => eprintln!("page.cover.cleanup outcome=database-error"),
            Err(_) => eprintln!("page.cover.cleanup outcome=timed-out"),
        }
        Ok(StagePageCover::Saved(asset))
    }

    pub(crate) async fn page_cover_image(
        &self,
        address: &PageAddress,
        asset: Uuid,
    ) -> Result<Option<Vec<u8>>, sqlx::Error> {
        let query = format!(
            "SELECT a.jpeg
             FROM ({PAGE_ACCESS}) access
             JOIN page_cover_assets a
               ON (a.tenant_id, a.space_id, a.page_id, a.id) = ($1, $2, $4, $5)
             LEFT JOIN page_covers c
               ON (c.tenant_id, c.space_id, c.page_id) = ($1, $2, $4)
             WHERE access.can_read AND ((c.asset_id = a.id AND a.published_at IS NOT NULL) OR (
                 a.published_at IS NULL AND a.uploader_account_id = $3 AND access.can_edit
                 AND a.staged_at > CURRENT_TIMESTAMP - INTERVAL '24 hours'
             ))"
        );
        sqlx::query_scalar(&query)
            .bind(address.tenant)
            .bind(address.space)
            .bind(address.account)
            .bind(address.page)
            .bind(asset)
            .fetch_optional(&self.0)
            .await
    }
}

async fn prune_expired_covers(pool: &PgPool) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SET LOCAL statement_timeout = '150ms'")
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "WITH expired AS (
            SELECT tenant_id, space_id, page_id, id FROM page_cover_assets
            WHERE published_at IS NULL AND staged_at <= CURRENT_TIMESTAMP - INTERVAL '24 hours'
            ORDER BY staged_at LIMIT 32 FOR UPDATE SKIP LOCKED
        )
        DELETE FROM page_cover_assets a USING expired e
        WHERE (a.tenant_id, a.space_id, a.page_id, a.id) = (e.tenant_id, e.space_id, e.page_id, e.id)",
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await
}

pub(super) async fn update_page_cover(
    tx: &mut PgConnection,
    address: &PageAddress,
    previous: PageCover,
    selected: PageCover,
) -> Result<bool, sqlx::Error> {
    if let Some(asset) = selected.asset_id() {
        let claimed = sqlx::query(
            "UPDATE page_cover_assets SET published_at = CURRENT_TIMESTAMP
             WHERE (tenant_id, space_id, page_id, id, uploader_account_id) = ($1, $2, $3, $4, $5)
               AND published_at IS NULL AND staged_at > CURRENT_TIMESTAMP - INTERVAL '24 hours'",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(asset)
        .bind(address.account)
        .execute(&mut *tx)
        .await?;
        if claimed.rows_affected() == 0 {
            return Ok(false);
        }
    }
    if selected == PageCover::None {
        sqlx::query("DELETE FROM page_covers WHERE (tenant_id, space_id, page_id) = ($1, $2, $3)")
            .bind(address.tenant)
            .bind(address.space)
            .bind(address.page)
            .execute(&mut *tx)
            .await?;
    } else {
        sqlx::query(
            "INSERT INTO page_covers (tenant_id, space_id, page_id, theme, asset_id)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (tenant_id, space_id, page_id) DO UPDATE
             SET theme = EXCLUDED.theme, asset_id = EXCLUDED.asset_id",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(selected.theme())
        .bind(selected.asset_id())
        .execute(&mut *tx)
        .await?;
    }
    if let Some(old_asset) = previous.asset_id() {
        sqlx::query(
            "DELETE FROM page_cover_assets WHERE (tenant_id, space_id, page_id, id) = ($1, $2, $3, $4)",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(old_asset)
        .execute(tx)
        .await?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::{DynamicImage, ImageFormat, Rgb, RgbImage};
    use sqlx::PgPool;

    use super::*;
    use crate::{
        cover_image::normalize_cover_image,
        db::{CreatePage, CreatePageInput, CreateSection, PageUpdate, SavePageText},
    };

    struct CoverPageFixture {
        store: Store,
        owner: PageAddress,
        editor: PageAddress,
        viewer: PageAddress,
        other_page: PageAddress,
    }

    impl CoverPageFixture {
        async fn new(pool: &PgPool) -> anyhow::Result<Self> {
            let [tenant, owner, editor, viewer] = std::array::from_fn(|_| Uuid::new_v4());
            sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'Север')")
                .bind(tenant)
                .execute(pool)
                .await?;
            for (id, email) in [
                (owner, "owner@example.test"),
                (editor, "editor@example.test"),
                (viewer, "viewer@example.test"),
            ] {
                sqlx::query(
                    "INSERT INTO accounts (id, email, name, salt, password_hash)
                     VALUES ($1, $2, $2, $3, $4)",
                )
                .bind(id)
                .bind(email)
                .bind(vec![0_u8; 16])
                .bind(vec![0_u8; 32])
                .execute(pool)
                .await?;
                sqlx::query(
                    "INSERT INTO tenant_memberships VALUES ($1, $2, 'member', 'active', true, false)",
                )
                .bind(tenant)
                .bind(id)
                .execute(pool)
                .await?;
            }
            let store = Store::from_test_pool(pool.clone());
            let crate::db::CreateSpace::Saved(space) = store
                .create_space(tenant, owner, Uuid::new_v4(), "Документы")
                .await?
            else {
                panic!("space creation");
            };
            sqlx::query(
                "INSERT INTO space_grants VALUES ($1, $2, $3, 'editor'), ($1, $2, $4, 'viewer')",
            )
            .bind(tenant)
            .bind(space)
            .bind(editor)
            .bind(viewer)
            .execute(pool)
            .await?;
            let CreateSection::Saved(section) = store
                .create_section(tenant, space, owner, Uuid::new_v4(), "Раздел")
                .await?
            else {
                panic!("section creation");
            };
            let mut pages = Vec::new();
            for title in ["Первая", "Вторая"] {
                let CreatePage::Saved(page) = store
                    .create_page(CreatePageInput {
                        tenant_id: tenant,
                        space_id: space,
                        section_id: section,
                        account_id: owner,
                        operation_id: Uuid::new_v4(),
                        title,
                    })
                    .await?
                else {
                    panic!("page creation");
                };
                pages.push(page);
            }
            let owner = PageAddress {
                tenant,
                space,
                page: pages[0],
                account: owner,
            };
            Ok(Self {
                store,
                owner,
                editor: PageAddress {
                    account: editor,
                    ..owner
                },
                viewer: PageAddress {
                    account: viewer,
                    ..owner
                },
                other_page: PageAddress {
                    page: pages[1],
                    ..owner
                },
            })
        }
    }

    async fn sample_cover() -> CoverImage {
        let image = DynamicImage::ImageRgb8(RgbImage::from_pixel(4, 4, Rgb([40, 100, 160])));
        let mut encoded = Cursor::new(Vec::new());
        image.write_to(&mut encoded, ImageFormat::Png).unwrap();
        normalize_cover_image(encoded.into_inner()).await.unwrap()
    }

    #[sqlx::test(migrator = "crate::db::MIGRATIONS")]
    #[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
    async fn draft_and_published_images_follow_the_page_lifecycle(
        pool: PgPool,
    ) -> anyhow::Result<()> {
        let CoverPageFixture {
            store,
            owner,
            editor,
            viewer,
            other_page,
        } = CoverPageFixture::new(&pool).await?;
        let StagePageCover::Saved(asset) =
            store.stage_page_cover(&owner, sample_cover().await).await?
        else {
            panic!("stage a cover");
        };
        assert!(matches!(
            store.stage_page_cover(&owner, sample_cover().await).await?,
            StagePageCover::Saved(another_draft) if another_draft != asset
        ));
        assert!(matches!(
            store
                .stage_page_cover(&viewer, sample_cover().await)
                .await?,
            StagePageCover::Forbidden
        ));
        let bytes = store
            .page_cover_image(&owner, asset)
            .await?
            .expect("uploader's draft");
        for address in [&editor, &viewer, &other_page] {
            assert!(store.page_cover_image(address, asset).await?.is_none());
        }
        assert_eq!(store.page_text(&owner).await?.unwrap().content_revision, 1);
        for address in [&editor, &other_page] {
            assert!(matches!(
                store
                    .save_page(
                        address,
                        1,
                        PageUpdate::PlainText(""),
                        CoverUpdate::Set(PageCover::Image(asset))
                    )
                    .await?,
                SavePageText::InvalidCover
            ));
        }
        for _ in 0..2 {
            assert!(matches!(
                store
                    .save_page(
                        &owner,
                        1,
                        PageUpdate::PlainText(""),
                        CoverUpdate::Set(PageCover::Image(asset))
                    )
                    .await?,
                SavePageText::Saved
            ));
        }
        assert_eq!(store.page_text(&owner).await?.unwrap().content_revision, 2);
        assert_eq!(
            store.page_cover_image(&viewer, asset).await?.as_deref(),
            Some(bytes.as_slice())
        );
        assert!(store.page_cover_image(&other_page, asset).await?.is_none());
        let foreign_tenant = PageAddress {
            tenant: Uuid::new_v4(),
            ..owner
        };
        assert!(
            store
                .page_cover_image(&foreign_tenant, asset)
                .await?
                .is_none()
        );

        sqlx::query(
            "DELETE FROM space_grants WHERE (tenant_id, space_id, account_id) = ($1, $2, $3)",
        )
        .bind(viewer.tenant)
        .bind(viewer.space)
        .bind(viewer.account)
        .execute(&pool)
        .await?;
        assert!(store.page_cover_image(&viewer, asset).await?.is_none());
        assert!(matches!(
            store
                .save_page(
                    &editor,
                    2,
                    PageUpdate::PlainText("Текст с обложкой"),
                    CoverUpdate::Keep
                )
                .await?,
            SavePageText::Saved
        ));
        assert_eq!(
            store.page_text(&owner).await?.unwrap().cover,
            PageCover::Image(asset)
        );
        assert!(matches!(
            store
                .save_page(
                    &owner,
                    3,
                    PageUpdate::PlainText("Текст с обложкой"),
                    CoverUpdate::Set(PageCover::None)
                )
                .await?,
            SavePageText::Saved
        ));
        let page = store.page_text(&owner).await?.unwrap();
        assert_eq!(page.content_revision, 4);
        assert_eq!(page.source, "Текст с обложкой");
        assert_eq!(page.cover, PageCover::None);
        assert!(store.page_cover_image(&owner, asset).await?.is_none());
        assert!(matches!(
            store
                .save_page(
                    &owner,
                    4,
                    PageUpdate::PlainText("Текст с обложкой"),
                    CoverUpdate::Set(PageCover::Image(asset))
                )
                .await?,
            SavePageText::InvalidCover
        ));

        let StagePageCover::Saved(expired) =
            store.stage_page_cover(&owner, sample_cover().await).await?
        else {
            panic!("stage a replacement");
        };
        sqlx::query(
            "UPDATE page_cover_assets SET staged_at = CURRENT_TIMESTAMP - INTERVAL '25 hours'
             WHERE (tenant_id, space_id, page_id, id) = ($1, $2, $3, $4)",
        )
        .bind(owner.tenant)
        .bind(owner.space)
        .bind(owner.page)
        .bind(expired)
        .execute(&pool)
        .await?;
        assert!(store.page_cover_image(&owner, expired).await?.is_none());
        assert!(matches!(
            store
                .save_page(
                    &owner,
                    4,
                    PageUpdate::PlainText("Несохранённый текст"),
                    CoverUpdate::Set(PageCover::Image(expired))
                )
                .await?,
            SavePageText::InvalidCover
        ));
        let unchanged = store.page_text(&owner).await?.unwrap();
        assert_eq!(unchanged.source, "Текст с обложкой");
        assert_eq!(unchanged.content_revision, 4);

        sqlx::query(
            "CREATE FUNCTION reject_expired_cover_cleanup() RETURNS trigger
             LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'test cleanup failure'; END; $$",
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "CREATE TRIGGER fail_expired_cover_cleanup BEFORE DELETE ON page_cover_assets
             FOR EACH ROW EXECUTE FUNCTION reject_expired_cover_cleanup()",
        )
        .execute(&pool)
        .await?;
        let StagePageCover::Saved(next) =
            store.stage_page_cover(&owner, sample_cover().await).await?
        else {
            panic!("cleanup failure must not undo a staged image");
        };
        assert!(store.page_cover_image(&owner, next).await?.is_some());
        Ok(())
    }

    #[sqlx::test(migrator = "crate::db::MIGRATIONS")]
    #[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
    async fn concurrent_cover_changes_share_the_text_revision(pool: PgPool) -> anyhow::Result<()> {
        let CoverPageFixture {
            store,
            owner,
            editor,
            ..
        } = CoverPageFixture::new(&pool).await?;
        let (first, second) = tokio::join!(
            store.save_page(
                &owner,
                1,
                PageUpdate::PlainText(""),
                CoverUpdate::Set(PageCover::Orbit)
            ),
            store.save_page(
                &editor,
                1,
                PageUpdate::PlainText(""),
                CoverUpdate::Set(PageCover::Grid)
            )
        );
        let (saved_cover, current) = match (first?, second?) {
            (SavePageText::Saved, SavePageText::Conflict(current)) => (PageCover::Orbit, current),
            (SavePageText::Conflict(current), SavePageText::Saved) => (PageCover::Grid, current),
            _ => panic!("only one concurrent change should be published"),
        };
        assert_eq!(current.cover, saved_cover);
        assert_eq!(current.content_revision, 2);
        let reopened = store.page_text(&owner).await?.unwrap();
        assert_eq!(reopened.cover, current.cover);
        assert_eq!(reopened.content_revision, current.content_revision);
        assert!(reopened.source.is_empty());
        Ok(())
    }

    #[sqlx::test(migrator = "crate::db::MIGRATIONS")]
    #[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
    async fn cover_only_rebase_preserves_newer_markdown_and_replays_after_retirement(
        pool: PgPool,
    ) -> anyhow::Result<()> {
        use crate::db::PageContentFormat::Markdown;

        let CoverPageFixture {
            store,
            owner,
            editor,
            ..
        } = CoverPageFixture::new(&pool).await?;
        let StagePageCover::Saved(first) =
            store.stage_page_cover(&owner, sample_cover().await).await?
        else {
            panic!("stage first tab's image");
        };
        let StagePageCover::Saved(second) =
            store.stage_page_cover(&owner, sample_cover().await).await?
        else {
            panic!("stage second tab's image");
        };
        assert_ne!(first, second);

        let editor_source = "# Новая редакция\n\n**Точный Markdown**\n";
        assert!(matches!(
            store
                .save_page(
                    &editor,
                    1,
                    PageUpdate::Source(editor_source, Markdown),
                    CoverUpdate::Keep
                )
                .await?,
            SavePageText::Saved
        ));
        let first_cover = CoverUpdate::Set(PageCover::Image(first));
        let SavePageText::Conflict(current) = store
            .save_page(&owner, 1, PageUpdate::Keep, first_cover)
            .await?
        else {
            panic!("stale change must show the new editing base");
        };
        assert_eq!(current.content_revision, 2);
        assert_eq!(current.source, editor_source);
        assert_eq!(current.format, Markdown);
        assert!(matches!(
            store
                .save_page(&owner, 2, PageUpdate::Keep, first_cover)
                .await?,
            SavePageText::Saved
        ));
        let rebased = store.page_text(&owner).await?.unwrap();
        assert_eq!(rebased.source, editor_source);
        assert_eq!(rebased.format, Markdown);
        assert_eq!(rebased.cover, PageCover::Image(first));
        assert_eq!(rebased.content_revision, 3);

        let latest_source = "# Следующая редакция";
        assert!(matches!(
            store
                .save_page(
                    &editor,
                    3,
                    PageUpdate::Source(latest_source, Markdown),
                    CoverUpdate::Set(PageCover::None)
                )
                .await?,
            SavePageText::Saved
        ));
        assert!(store.page_cover_image(&owner, first).await?.is_none());
        assert!(store.page_cover_image(&owner, second).await?.is_some());
        assert!(matches!(
            store
                .save_page(&owner, 2, PageUpdate::Keep, first_cover)
                .await?,
            SavePageText::Saved
        ));
        assert!(matches!(
            store
                .save_page(&editor, 2, PageUpdate::Keep, first_cover)
                .await?,
            SavePageText::Conflict(_)
        ));
        assert!(matches!(
            store
                .save_page(
                    &owner,
                    2,
                    PageUpdate::Source("Другая правка", Markdown),
                    first_cover
                )
                .await?,
            SavePageText::Conflict(_)
        ));
        let replayed = store.page_text(&owner).await?.unwrap();
        assert_eq!(replayed.source, latest_source);
        assert_eq!(replayed.cover, PageCover::None);
        assert_eq!(replayed.content_revision, 4);

        assert!(matches!(
            store
                .save_page(
                    &owner,
                    4,
                    PageUpdate::Keep,
                    CoverUpdate::Set(PageCover::Image(second))
                )
                .await?,
            SavePageText::Saved
        ));
        assert_eq!(
            store.page_text(&owner).await?.unwrap().cover,
            PageCover::Image(second)
        );
        Ok(())
    }

    #[sqlx::test(migrator = "crate::db::MIGRATIONS")]
    #[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
    async fn modern_text_and_current_no_ops_replay_after_an_intervening_save(
        pool: PgPool,
    ) -> anyhow::Result<()> {
        use crate::db::PageContentFormat::Markdown;

        let CoverPageFixture {
            store,
            owner,
            editor,
            other_page,
            ..
        } = CoverPageFixture::new(&pool).await?;
        let orbit = CoverUpdate::Set(PageCover::Orbit);
        assert!(matches!(
            store.save_page(&owner, 1, PageUpdate::Keep, orbit).await?,
            SavePageText::Saved
        ));
        let accepted = "# Первый текст\n\nТочный источник";
        let newer = "# Актуальный текст";
        assert!(matches!(
            store
                .save_page(
                    &owner,
                    2,
                    PageUpdate::Source(accepted, Markdown),
                    CoverUpdate::Keep
                )
                .await?,
            SavePageText::Saved
        ));
        for (update, cover) in [
            (PageUpdate::Source(accepted, Markdown), CoverUpdate::Keep),
            (PageUpdate::Keep, orbit),
        ] {
            assert!(matches!(
                store.save_page(&owner, 3, update, cover).await?,
                SavePageText::Saved
            ));
        }
        assert!(matches!(
            store
                .save_page(
                    &editor,
                    2,
                    PageUpdate::Source(accepted, Markdown),
                    CoverUpdate::Keep
                )
                .await?,
            SavePageText::Saved
        ));
        assert_eq!(store.page_text(&owner).await?.unwrap().content_revision, 3);
        assert!(matches!(
            store
                .save_page(
                    &editor,
                    3,
                    PageUpdate::Source(newer, Markdown),
                    CoverUpdate::Set(PageCover::Grid)
                )
                .await?,
            SavePageText::Saved
        ));
        for (base, update, cover) in [
            (2, PageUpdate::Source(accepted, Markdown), CoverUpdate::Keep),
            (3, PageUpdate::Source(accepted, Markdown), CoverUpdate::Keep),
            (3, PageUpdate::Keep, orbit),
        ] {
            assert!(matches!(
                store.save_page(&owner, base, update, cover).await?,
                SavePageText::Saved
            ));
        }
        assert!(matches!(
            store
                .save_page(
                    &editor,
                    2,
                    PageUpdate::Source(accepted, Markdown),
                    CoverUpdate::Keep
                )
                .await?,
            SavePageText::Conflict(_)
        ));
        let final_page = store.page_text(&owner).await?.unwrap();
        assert_eq!(final_page.source, newer);
        assert_eq!(final_page.format, Markdown);
        assert_eq!(final_page.cover, PageCover::Grid);
        assert_eq!(final_page.content_revision, 4);

        assert!(matches!(
            store
                .save_page(&other_page, 1, PageUpdate::PlainText(""), CoverUpdate::Keep)
                .await?,
            SavePageText::Saved
        ));
        let legacy_receipts: i64 =
            sqlx::query_scalar("SELECT count(*) FROM page_cover_save_receipts WHERE page_id = $1")
                .bind(other_page.page)
                .fetch_one(&pool)
                .await?;
        assert_eq!(legacy_receipts, 0);
        assert_eq!(
            store
                .page_text(&other_page)
                .await?
                .unwrap()
                .content_revision,
            1
        );
        Ok(())
    }
}
