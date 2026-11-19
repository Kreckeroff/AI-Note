use sqlx::PgConnection;
use uuid::Uuid;

use super::sections::{SPACE_ACCESS, lock_active_member};

#[derive(Clone, Copy)]
pub(crate) struct PageAddress {
    pub tenant: Uuid,
    pub space: Uuid,
    pub page: Uuid,
    pub account: Uuid,
}

pub(super) enum PageEditAccess {
    Missing,
    Forbidden,
    Allowed,
}

pub(super) async fn lock_page_for_edit(
    tx: &mut PgConnection,
    address: &PageAddress,
) -> Result<PageEditAccess, sqlx::Error> {
    sqlx::query("SET LOCAL statement_timeout = '3s'")
        .execute(&mut *tx)
        .await?;
    if !lock_active_member(tx, address.tenant, address.account).await? {
        return Ok(PageEditAccess::Missing);
    }
    let query = format!(
        "SELECT access.can_create FROM ({SPACE_ACCESS}) access
         JOIN pages p ON (p.tenant_id, p.space_id, p.id) = ($1, $2, $4)
         FOR NO KEY UPDATE OF p"
    );
    let permission = sqlx::query_scalar::<_, bool>(&query)
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.account)
        .bind(address.page)
        .fetch_optional(tx)
        .await?;
    Ok(match permission {
        None => PageEditAccess::Missing,
        Some(false) => PageEditAccess::Forbidden,
        Some(true) => PageEditAccess::Allowed,
    })
}
