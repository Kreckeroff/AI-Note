use serde::Serialize;
use sqlx::{PgConnection, Postgres, Transaction};
use uuid::Uuid;

use super::{SharingFailure, SharingOperation, manager_tx, tenant_allows};
use crate::{
    clock::now_ms,
    db::{PageAddress, Store, sections::decode_title},
    markdown::attachment_image_ids,
};

#[derive(Serialize)]
pub(crate) struct AffectedPage {
    id: Uuid,
    title: String,
}

#[derive(Serialize)]
pub(crate) struct PublicPolicy {
    pub(crate) allowed: bool,
    pub(crate) enabled: bool,
    pub(crate) version: i64,
    pub(crate) affected: Vec<AffectedPage>,
}

pub(crate) struct PublicPage {
    tenant_id: Uuid,
    space_id: Uuid,
    page_id: Uuid,
    pub(crate) title: String,
    pub(crate) source: String,
    pub(crate) markdown: bool,
    pub(crate) cover_theme: Option<String>,
    pub(crate) cover_id: Option<Uuid>,
    pub(crate) images: Vec<(Uuid, Uuid)>,
}

#[derive(Clone, Copy)]
pub(crate) enum AssetKind {
    Cover,
    Image,
}

#[derive(sqlx::FromRow)]
pub(crate) struct PublicAsset {
    pub(crate) media_type: String,
    pub(crate) bytes: Vec<u8>,
}

impl Store {
    pub(crate) async fn public_link_panel(
        &self,
        address: &PageAddress,
    ) -> Result<(Option<Uuid>, PublicPolicy), SharingFailure> {
        let mut tx = manager_tx(self, address).await?;
        let allowed = tenant_allows(&mut tx, address.tenant).await?.1;
        // Serialize snapshot version and link membership with all space-wide mutations.
        let (enabled, version): (bool, i64) = sqlx::query_as(
            "SELECT enabled, version FROM space_public_policies
             WHERE (tenant_id,space_id) = ($1,$2) FOR SHARE",
        )
        .bind(address.tenant)
        .bind(address.space)
        .fetch_optional(&mut *tx)
        .await?
        .unwrap_or_default();
        let enabled = allowed && enabled;
        let affected: Vec<AffectedPage> = if enabled {
            let rows: Vec<(Uuid, Vec<u8>)> = sqlx::query_as(
                "SELECT p.id, p.title
                 FROM public_page_links l JOIN pages p ON (p.tenant_id,p.space_id,p.id) = (l.tenant_id,l.space_id,l.page_id)
                 WHERE (l.tenant_id,l.space_id) = ($1,$2) AND l.revoked_at IS NULL ORDER BY p.title,p.id",
            )
            .bind(address.tenant).bind(address.space).fetch_all(&mut *tx).await?;
            rows.into_iter()
                .map(|(id, title)| {
                    Ok(AffectedPage {
                        id,
                        title: decode_title(title, "pages.title")?,
                    })
                })
                .collect::<Result<Vec<_>, sqlx::Error>>()?
        } else {
            Vec::new()
        };
        let link = if enabled {
            active_link(&mut tx, address).await?
        } else {
            None
        };
        tx.commit().await?;
        Ok((
            link,
            PublicPolicy {
                allowed,
                enabled,
                version,
                affected,
            },
        ))
    }

    pub(crate) async fn publish_link(
        &self,
        address: &PageAddress,
        operation: &SharingOperation,
        id: Uuid,
        digest: &[u8; 32],
    ) -> Result<(), SharingFailure> {
        let mut tx = manager_tx(self, address).await?;
        if !tenant_allows(&mut tx, address.tenant).await?.1
            || !lock_policy(&mut tx, address).await?.0
        {
            return Err(SharingFailure::Forbidden);
        }
        if operation.replay(&mut tx, address).await?.is_some() {
            return Ok(());
        }
        let result = if let Some(prior) = active_link(&mut tx, address).await? {
            prior
        } else {
            sqlx::query(
                "INSERT INTO public_page_links(id,tenant_id,space_id,page_id,token_digest,issued_by,issued_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(id).bind(address.tenant).bind(address.space).bind(address.page).bind(&digest[..])
            .bind(address.account).bind(now_ms()).execute(&mut *tx).await?;
            increment_policy(&mut tx, address).await?;
            id
        };
        operation.record(&mut tx, address, Some(result)).await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn unpublish_link(
        &self,
        address: &PageAddress,
        operation: &SharingOperation,
        id: Uuid,
    ) -> Result<(), SharingFailure> {
        let mut tx = manager_tx(self, address).await?;
        lock_policy(&mut tx, address).await?;
        if operation.replay(&mut tx, address).await?.is_some() {
            return Ok(());
        }
        let existing: Option<bool> = sqlx::query_scalar(
            "SELECT revoked_at IS NOT NULL FROM public_page_links
             WHERE (id,tenant_id,space_id,page_id) = ($1,$2,$3,$4)",
        )
        .bind(id)
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .fetch_optional(&mut *tx)
        .await?;
        if existing == Some(false) {
            sqlx::query("UPDATE public_page_links SET revoked_at = $2 WHERE id = $1")
                .bind(id)
                .bind(now_ms())
                .execute(&mut *tx)
                .await?;
            increment_policy(&mut tx, address).await?;
        } else if existing.is_none() {
            return Err(SharingFailure::Missing);
        }
        operation.record(&mut tx, address, None).await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn set_space_link_policy(
        &self,
        address: &PageAddress,
        operation: &SharingOperation,
        enabled: bool,
        version: i64,
    ) -> Result<(), SharingFailure> {
        let mut tx = manager_tx(self, address).await?;
        if operation.replay(&mut tx, address).await?.is_some() {
            return Ok(());
        }
        if enabled && !tenant_allows(&mut tx, address.tenant).await?.1 {
            return Err(SharingFailure::Forbidden);
        }
        let (current, actual) = lock_policy(&mut tx, address).await?;
        if actual != version {
            return Err(SharingFailure::Conflict);
        }
        if enabled != current {
            sqlx::query(
                "UPDATE space_public_policies SET enabled = $3, version = version + 1
                 WHERE (tenant_id,space_id) = ($1,$2)",
            )
            .bind(address.tenant)
            .bind(address.space)
            .bind(enabled)
            .execute(&mut *tx)
            .await?;
            if !enabled {
                sqlx::query(
                    "UPDATE public_page_links SET revoked_at = $3
                     WHERE (tenant_id,space_id) = ($1,$2) AND revoked_at IS NULL",
                )
                .bind(address.tenant)
                .bind(address.space)
                .bind(now_ms())
                .execute(&mut *tx)
                .await?;
            }
        }
        operation.record(&mut tx, address, None).await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn load_public_page(
        &self,
        id: Uuid,
        digest: &[u8; 32],
    ) -> Result<Option<PublicPage>, sqlx::Error> {
        let mut tx = self.public_snapshot().await?;
        let Some(mut page) = current_public_page(&mut tx, id, digest).await? else {
            return Ok(None);
        };
        if page.markdown && page.source.contains("attachment:") {
            page.images = sqlx::query_as(
                "SELECT x.asset_id, x.publication FROM page_attachments x
                 JOIN page_attachment_assets a ON (a.tenant_id,a.space_id,a.page_id,a.id) = (x.tenant_id,x.space_id,x.page_id,x.asset_id)
                 WHERE (x.tenant_id,x.space_id,x.page_id) = ($1,$2,$3) AND a.image_type IS NOT NULL",
            )
            .bind(page.tenant_id).bind(page.space_id).bind(page.page_id).fetch_all(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(Some(page))
    }

    pub(crate) async fn load_public_asset(
        &self,
        id: Uuid,
        digest: &[u8; 32],
        kind: AssetKind,
        asset: Uuid,
    ) -> Result<Option<PublicAsset>, sqlx::Error> {
        let mut tx = self.public_snapshot().await?;
        let Some(page) = current_public_page(&mut tx, id, digest).await? else {
            return Ok(None);
        };
        let content = match kind {
            AssetKind::Cover if page.cover_id == Some(asset) => {
                sqlx::query_as(
                    "SELECT 'image/jpeg' AS media_type,jpeg AS bytes FROM page_cover_assets
                     WHERE (tenant_id,space_id,page_id,id) = ($1,$2,$3,$4) AND published_at IS NOT NULL",
                )
                .bind(page.tenant_id).bind(page.space_id).bind(page.page_id).bind(asset)
                .fetch_optional(&mut *tx).await?
            }
            AssetKind::Image if page.markdown => {
                let images = attachment_image_ids(&page.source);
                if images.is_empty() {
                    return Ok(None);
                }
                sqlx::query_as(
                    "SELECT a.image_type AS media_type,a.bytes FROM page_attachments x
                     JOIN page_attachment_assets a ON (a.tenant_id,a.space_id,a.page_id,a.id) = (x.tenant_id,x.space_id,x.page_id,x.asset_id)
                     WHERE (x.tenant_id,x.space_id,x.page_id,x.publication) = ($1,$2,$3,$4)
                       AND x.asset_id = ANY($5) AND a.published_at IS NOT NULL AND a.image_type IS NOT NULL",
                )
                .bind(page.tenant_id).bind(page.space_id).bind(page.page_id).bind(asset).bind(images)
                .fetch_optional(&mut *tx).await?
            }
            _ => None,
        };
        tx.commit().await?;
        Ok(content)
    }

    async fn public_snapshot(&self) -> Result<Transaction<'_, Postgres>, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ READ ONLY")
            .execute(&mut *tx)
            .await?;
        Ok(tx)
    }
}

async fn current_public_page(
    tx: &mut PgConnection,
    id: Uuid,
    digest: &[u8; 32],
) -> Result<Option<PublicPage>, sqlx::Error> {
    #[derive(sqlx::FromRow)]
    struct PageRow {
        tenant_id: Uuid,
        space_id: Uuid,
        page_id: Uuid,
        title: Vec<u8>,
        source: String,
        markdown: bool,
        cover_theme: Option<String>,
        cover_id: Option<Uuid>,
    }
    let row: Option<PageRow> = sqlx::query_as(
        "SELECT p.tenant_id,p.space_id,p.id AS page_id,p.title,
                p.markdown AS source,p.content_is_markdown AS markdown,c.theme AS cover_theme,c.asset_id AS cover_id
         FROM public_page_links l
         JOIN pages p ON (p.tenant_id,p.space_id,p.id) = (l.tenant_id,l.space_id,l.page_id)
         JOIN tenant_sharing_policies t ON t.tenant_id = l.tenant_id
         JOIN space_public_policies s ON (s.tenant_id,s.space_id) = (l.tenant_id,l.space_id)
         LEFT JOIN page_covers c ON (c.tenant_id,c.space_id,c.page_id) = (p.tenant_id,p.space_id,p.id)
         WHERE l.id = $1 AND l.token_digest = $2 AND l.revoked_at IS NULL AND t.allow_public AND s.enabled",
    )
    .bind(id).bind(&digest[..]).fetch_optional(tx).await?;
    row.map(|row| {
        Ok(PublicPage {
            tenant_id: row.tenant_id,
            space_id: row.space_id,
            page_id: row.page_id,
            title: decode_title(row.title, "pages.title")?,
            source: row.source,
            markdown: row.markdown,
            cover_theme: row.cover_theme,
            cover_id: row.cover_id,
            images: Vec::new(),
        })
    })
    .transpose()
}

async fn lock_policy(
    tx: &mut PgConnection,
    address: &PageAddress,
) -> Result<(bool, i64), sqlx::Error> {
    sqlx::query(
        "INSERT INTO space_public_policies(tenant_id,space_id) VALUES ($1,$2)
         ON CONFLICT (tenant_id,space_id) DO NOTHING",
    )
    .bind(address.tenant)
    .bind(address.space)
    .execute(&mut *tx)
    .await?;
    sqlx::query_as(
        "SELECT enabled,version FROM space_public_policies
         WHERE (tenant_id,space_id) = ($1,$2) FOR UPDATE",
    )
    .bind(address.tenant)
    .bind(address.space)
    .fetch_one(tx)
    .await
}

async fn active_link(
    tx: &mut PgConnection,
    address: &PageAddress,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT id FROM public_page_links
         WHERE (tenant_id,space_id,page_id) = ($1,$2,$3) AND revoked_at IS NULL",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .fetch_optional(tx)
    .await
}

async fn increment_policy(tx: &mut PgConnection, address: &PageAddress) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE space_public_policies SET version = version + 1 WHERE (tenant_id,space_id) = ($1,$2)")
        .bind(address.tenant).bind(address.space).execute(tx).await?;
    Ok(())
}
