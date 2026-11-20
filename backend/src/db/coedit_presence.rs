use serde::Serialize;
use sqlx::FromRow;
use uuid::Uuid;

use crate::clock::now_ms;

use super::{PageAddress, Store};

#[derive(FromRow)]
struct PresenceRow {
    published_revision: i64,
    can_edit: bool,
    people: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PresenceView {
    pub(crate) published_revision: String,
    pub(crate) can_edit: bool,
    pub(crate) people: serde_json::Value,
}

pub(crate) struct ActiveSession {
    pub(crate) account: Uuid,
    pub(crate) digest: [u8; 32],
    pub(crate) edit: bool,
}

impl Store {
    pub(crate) async fn page_presence(
        &self,
        address: &PageAddress,
        session: &[u8; 32],
        candidates: &[ActiveSession],
    ) -> Result<Option<PresenceView>, sqlx::Error> {
        let accounts: Vec<_> = candidates.iter().map(|entry| entry.account).collect();
        let digests: Vec<_> = candidates
            .iter()
            .map(|entry| entry.digest.to_vec())
            .collect();
        let editors: Vec<_> = candidates.iter().map(|entry| entry.edit).collect();
        let row = sqlx::query_as::<_, PresenceRow>(
            "WITH caller AS MATERIALIZED (
                 SELECT p.content_revision AS published_revision, access.can_edit
                 FROM page_permissions($1,$2,$3,$4) access
                 JOIN pages p ON (p.tenant_id,p.space_id,p.id)=($1,$2,$4)
                 JOIN sessions s ON s.account_id = $3 AND s.token_digest = $9 AND s.expires_at > $8
                 WHERE access.can_read
             ), people AS (
                 SELECT c.account_id AS id, a.name,
                        CASE WHEN bool_or(c.edit AND access.can_edit) THEN 'edit' ELSE 'read' END AS mode
                 FROM caller
                 CROSS JOIN unnest($5::uuid[],$6::bytea[],$7::boolean[]) c(account_id,digest,edit)
                 JOIN accounts a ON a.id=c.account_id
                 JOIN sessions s ON s.account_id=c.account_id AND s.token_digest=c.digest AND s.expires_at > $8
                 CROSS JOIN LATERAL page_permissions($1,$2,c.account_id,$4) access
                 WHERE access.can_read AND c.account_id <> $3
                 GROUP BY c.account_id,a.name
             )
             SELECT published_revision,can_edit,
                    COALESCE((SELECT jsonb_agg(to_jsonb(people) ORDER BY name,id) FROM people),'[]'::jsonb)::text AS people
             FROM caller",
        )
        .bind(address.tenant)
        .bind(address.space)
        .bind(address.account)
        .bind(address.page)
        .bind(accounts)
        .bind(digests)
        .bind(editors)
        .bind(now_ms())
        .bind(&session[..])
        .fetch_optional(&self.0)
        .await?;
        row.map(|row| {
            Ok(PresenceView {
                published_revision: row.published_revision.to_string(),
                can_edit: row.can_edit,
                people: serde_json::from_str(&row.people).map_err(|error| {
                    sqlx::Error::ColumnDecode {
                        index: "people".to_owned(),
                        source: Box::new(error),
                    }
                })?,
            })
        })
        .transpose()
    }
}
