use super::{
    PageAddress, Store,
    page_access::{PAGE_ACCESS, PageEditAccess, lock_page_for_edit},
    page_history::{ensure_baseline, record_page_publication},
    sections::decode_title,
};
use crate::text::{fold_name, js_trim};

pub(crate) struct PageName {
    pub title: String,
    pub revision: i64,
}

pub(crate) enum RenamePage {
    Named(PageName),
    Stale(PageName),
    Missing,
    Forbidden,
    InvalidName,
    Duplicate,
}

impl Store {
    pub(crate) async fn page_name(&self, address: &PageAddress) -> Result<RenamePage, sqlx::Error> {
        let query = format!(
            "SELECT p.title, p.content_revision, access.can_edit FROM ({PAGE_ACCESS}) access
             JOIN pages p ON (p.tenant_id, p.space_id, p.id) = ($1, $2, $4)
             WHERE access.can_read"
        );
        let name = sqlx::query_as::<_, (Vec<u8>, i64, bool)>(&query)
            .bind(address.tenant)
            .bind(address.space)
            .bind(address.account)
            .bind(address.page)
            .fetch_optional(&self.0)
            .await?;
        Ok(match name {
            Some((title, revision, true)) => RenamePage::Named(PageName {
                title: decode_title(title, "pages.title")?,
                revision,
            }),
            Some(_) => RenamePage::Forbidden,
            None => RenamePage::Missing,
        })
    }

    pub(crate) async fn rename_page(
        &self,
        address: &PageAddress,
        base_revision: i64,
        entered: &str,
    ) -> Result<RenamePage, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        match lock_page_for_edit(&mut tx, address).await? {
            PageEditAccess::Missing => return Ok(RenamePage::Missing),
            PageEditAccess::Forbidden => return Ok(RenamePage::Forbidden),
            PageEditAccess::Allowed => {}
        }
        let title = js_trim(entered);

        let (current, revision): (Vec<u8>, i64) = sqlx::query_as(
            "SELECT title, content_revision FROM pages WHERE (tenant_id, space_id, id) = ($1, $2, $3)",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .fetch_one(&mut *tx)
        .await?;
        if current == title.as_bytes() {
            return Ok(RenamePage::Named(PageName {
                title: title.to_owned(),
                revision,
            }));
        }
        if super::pages::validate_title(title).is_err() {
            return Ok(RenamePage::InvalidName);
        }
        if revision != base_revision {
            return Ok(RenamePage::Stale(PageName {
                title: decode_title(current, "pages.title")?,
                revision,
            }));
        }

        ensure_baseline(&mut tx, address).await?;
        let updated = sqlx::query_scalar::<_, i64>(
            "UPDATE pages SET title = $4, title_key = $5, content_revision = content_revision + 1
             WHERE (tenant_id, space_id, id) = ($1, $2, $3) RETURNING content_revision",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(title.as_bytes())
        .bind(fold_name(title).as_bytes())
        .fetch_one(&mut *tx)
        .await;
        let revision_after = match updated {
            Ok(revision) => revision,
            Err(error)
                if error
                    .as_database_error()
                    .is_some_and(|error| error.is_unique_violation()) =>
            {
                return Ok(RenamePage::Duplicate);
            }
            Err(error) => return Err(error),
        };
        record_page_publication(&mut tx, address, Some(revision), false).await?;
        tx.commit().await?;
        Ok(RenamePage::Named(PageName {
            title: title.to_owned(),
            revision: revision_after,
        }))
    }
}
