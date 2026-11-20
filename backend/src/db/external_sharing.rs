use sqlx::{PgConnection, Postgres, Transaction};
use uuid::Uuid;

use super::{PageAddress, Store, page_access::lock_page_for_change};

mod guests;
mod public;

pub(crate) use guests::{GuestAcceptance, GuestPreview};
pub(crate) use public::{AssetKind, PublicPage};

#[derive(Debug)]
pub(crate) enum SharingFailure {
    Missing,
    Forbidden,
    Conflict,
    Database(sqlx::Error),
}

impl From<sqlx::Error> for SharingFailure {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

async fn manager_tx<'a>(
    store: &'a Store,
    address: &PageAddress,
) -> Result<Transaction<'a, Postgres>, SharingFailure> {
    let mut tx = store.0.begin().await?;
    let access = lock_page_for_change(&mut tx, address).await?;
    match access {
        Some(access) if access.can_manage => Ok(tx),
        Some(access) if access.can_read => Err(SharingFailure::Forbidden),
        _ => Err(SharingFailure::Missing),
    }
}

pub(crate) struct SharingOperation {
    pub(crate) id: Uuid,
    pub(crate) fingerprint: [u8; 32],
}

impl SharingOperation {
    async fn replay(
        &self,
        tx: &mut PgConnection,
        address: &PageAddress,
    ) -> Result<Option<Option<Uuid>>, SharingFailure> {
        let receipt = sqlx::query_as::<_, (Vec<u8>, Option<Uuid>)>(
            "SELECT fingerprint, result_id FROM page_sharing_receipts
             WHERE (tenant_id,space_id,page_id,account_id,operation_id) = ($1,$2,$3,$4,$5)",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(address.account)
        .bind(self.id)
        .fetch_optional(tx)
        .await?;
        match receipt {
            None => Ok(None),
            Some((digest, result)) if digest == self.fingerprint => Ok(Some(result)),
            _ => Err(SharingFailure::Conflict),
        }
    }

    async fn record(
        &self,
        tx: &mut PgConnection,
        address: &PageAddress,
        result: Option<Uuid>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO page_sharing_receipts
             (tenant_id,space_id,page_id,account_id,operation_id,fingerprint,result_id)
             VALUES ($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.page)
        .bind(address.account)
        .bind(self.id)
        .bind(&self.fingerprint[..])
        .bind(result)
        .execute(tx)
        .await?;
        Ok(())
    }
}

async fn tenant_allows(tx: &mut PgConnection, tenant: Uuid) -> Result<(bool, bool), sqlx::Error> {
    Ok(sqlx::query_as(
        "SELECT allow_guests, allow_public FROM tenant_sharing_policies WHERE tenant_id = $1",
    )
    .bind(tenant)
    .fetch_optional(tx)
    .await?
    .unwrap_or_default())
}
