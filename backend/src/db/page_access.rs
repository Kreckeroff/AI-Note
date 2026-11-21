use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use super::{Page, Store, sections::decode_title};

pub(super) const PAGE_ACCESS: &str = "SELECT * FROM page_permissions($1, $2, $3, $4)";

#[derive(Clone, Copy)]
pub(crate) struct PageAddress {
    pub tenant: Uuid,
    pub space: Uuid,
    pub page: Uuid,
    pub account: Uuid,
}

#[derive(FromRow)]
pub(crate) struct PagePermissions {
    pub can_read: bool,
    pub can_comment: bool,
    pub can_edit: bool,
    pub can_manage: bool,
    pub can_request: bool,
    pub can_browse_space: bool,
}

pub(super) enum PageEditAccess {
    Missing,
    Forbidden,
    Allowed,
}

async fn permissions(
    connection: &mut PgConnection,
    address: &PageAddress,
) -> Result<Option<PagePermissions>, sqlx::Error> {
    sqlx::query_as(PAGE_ACCESS)
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.account)
        .bind(address.page)
        .fetch_optional(connection)
        .await
}

impl Store {
    pub(crate) async fn page_permissions(
        &self,
        address: &PageAddress,
    ) -> Result<Option<PagePermissions>, sqlx::Error> {
        permissions(&mut *self.0.acquire().await?, address).await
    }

    pub(crate) async fn standalone_page(
        &self,
        address: &PageAddress,
    ) -> Result<Option<Page>, sqlx::Error> {
        let query = format!(
            "SELECT p.title, access.can_edit FROM ({PAGE_ACCESS}) access
             JOIN pages p ON (p.tenant_id, p.space_id, p.id) = ($1, $2, $4)
             WHERE access.can_read"
        );
        let title: Option<(Vec<u8>, bool)> = sqlx::query_as(&query)
            .bind(address.tenant)
            .bind(address.space)
            .bind(address.account)
            .bind(address.page)
            .fetch_optional(&self.0)
            .await?;
        title
            .map(|(title, can_edit)| {
                Ok(Page {
                    id: address.page,
                    section_id: Uuid::nil(),
                    parent_id: None,
                    title: decode_title(title, "pages.title")?,
                    can_edit,
                })
            })
            .transpose()
    }

    pub(crate) async fn is_active_member(
        &self,
        tenant: Uuid,
        account: Uuid,
    ) -> Result<bool, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM tenant_memberships
             WHERE tenant_id = $1 AND account_id = $2 AND status = 'active')",
        )
        .bind(tenant)
        .bind(account)
        .fetch_one(&self.0)
        .await
    }
}

pub(crate) async fn lock_page_for_change(
    tx: &mut PgConnection,
    address: &PageAddress,
) -> Result<Option<PagePermissions>, sqlx::Error> {
    sqlx::query("SET LOCAL statement_timeout = '3s'")
        .execute(&mut *tx)
        .await?;
    // Tenant configuration updates the parent tenant before inserting a policy.
    // Lock that stable row too, so a missing policy cannot race its initial insert.
    sqlx::query("SELECT 1 FROM tenants WHERE id = $1 FOR SHARE")
        .bind(address.tenant)
        .fetch_optional(&mut *tx)
        .await?;
    sqlx::query("SELECT 1 FROM tenant_sharing_policies WHERE tenant_id = $1 FOR SHARE")
        .bind(address.tenant)
        .fetch_optional(&mut *tx)
        .await?;
    // Existing space-grant changes serialize on the member row. Unaffiliated
    // guests serialize through the page and policy locks.
    sqlx::query(
        "SELECT 1 FROM tenant_memberships WHERE tenant_id = $1 AND account_id = $2 FOR UPDATE",
    )
    .bind(address.tenant)
    .bind(address.account)
    .fetch_optional(&mut *tx)
    .await?;
    let page: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM pages WHERE (tenant_id, space_id, id) = ($1, $2, $3) FOR NO KEY UPDATE",
    )
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.page)
    .fetch_optional(&mut *tx)
    .await?;
    if page.is_none() {
        return Ok(None);
    }
    // Read once more after acquiring every lock so a queued write observes
    // the role of the last committed page mutation.
    permissions(tx, address).await
}

pub(super) async fn lock_page_for_edit(
    tx: &mut PgConnection,
    address: &PageAddress,
) -> Result<PageEditAccess, sqlx::Error> {
    Ok(match lock_page_for_change(tx, address).await? {
        Some(permission) if permission.can_edit => PageEditAccess::Allowed,
        Some(permission) if permission.can_read => PageEditAccess::Forbidden,
        _ => PageEditAccess::Missing,
    })
}
