use sqlx::PgConnection;

use super::{PageAddress, Store, page_access::lock_page_for_change};

impl Store {
    pub(crate) async fn page_watch(
        &self,
        address: &PageAddress,
    ) -> Result<Option<bool>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM page_watchers
                    WHERE (tenant_id, space_id, page_id, account_id) = ($1, $2, $4, $3))
             FROM page_permissions($1, $2, $3, $4) access
             WHERE access.can_read",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.account)
        .bind(address.page)
        .fetch_optional(&self.0)
        .await
    }

    pub(crate) async fn set_page_watch(
        &self,
        address: &PageAddress,
        following: bool,
    ) -> Result<Option<bool>, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        if !lock_page_for_change(&mut tx, address)
            .await?
            .is_some_and(|access| access.can_read)
        {
            return Ok(None);
        }
        let query = if following {
            "INSERT INTO page_watchers (tenant_id, space_id, page_id, account_id) VALUES ($1, $2, $3, $4)
             ON CONFLICT DO NOTHING"
        } else {
            "DELETE FROM page_watchers
             WHERE (tenant_id, space_id, page_id, account_id) = ($1, $2, $3, $4)"
        };
        sqlx::query(query)
            .bind(address.tenant)
            .bind(address.space)
            .bind(address.page)
            .bind(address.account)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(Some(following))
    }
}

// Called with the page locked, after the new revision was written on the same
// transaction. A historical baseline and request replay never reach this call.
pub(super) async fn notify_page_publication(
    tx: &mut PgConnection,
    author: &PageAddress,
    revision: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO page_publication_notices
             (tenant_id, space_id, page_id, revision, recipient_account_id, created_at)
         SELECT w.tenant_id, w.space_id, w.page_id, $5, w.account_id, $6
         FROM page_watchers w
         CROSS JOIN LATERAL page_permissions(w.tenant_id, w.space_id, w.account_id, w.page_id) access
         WHERE (w.tenant_id, w.space_id, w.page_id) = ($1, $2, $3)
           AND w.account_id <> $4 AND access.can_read
         ON CONFLICT DO NOTHING",
    )
    .bind(author.tenant)
    .bind(author.space)
    .bind(author.page)
    .bind(author.account)
    .bind(revision)
    .bind(crate::clock::now_ms())
    .execute(tx)
    .await?;
    Ok(())
}
