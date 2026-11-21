use std::collections::BTreeMap;

use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use super::{
    Store,
    sections::{SPACE_ACCESS, decode_title, lock_active_member},
    tasks::TaskScope,
};
use crate::text::fold_name;

pub(crate) type Properties = BTreeMap<String, String>;
pub(crate) const PAGE_SIZE: i64 = 50;
pub(crate) const FIELD_TYPES: [(&str, &str); 4] = [
    ("text", "Текст"),
    ("number", "Число"),
    ("date", "Дата"),
    ("checkbox", "Флажок"),
];

#[derive(FromRow)]
pub(crate) struct Collection {
    pub id: Uuid,
    pub title: String,
    pub schema_version: i64,
}
#[derive(FromRow)]
pub(crate) struct Field {
    pub id: Uuid,
    pub title: String,
    pub kind: String,
}
pub(crate) struct Record {
    pub id: Uuid,
    pub title: String,
    pub version: i64,
    pub values: Properties,
}
pub(crate) struct View {
    pub tenant_name: String,
    pub space_name: String,
    pub editable: bool,
    pub collections: Vec<Collection>,
    pub fields: Vec<Field>,
    pub records: Vec<Record>,
    pub has_more: bool,
}
#[derive(Clone, Copy)]
pub(crate) enum ViewTarget {
    Catalog,
    Table(Uuid),
    NewRecord(Uuid),
    Record(Uuid, Uuid),
}
pub(crate) enum Change {
    Create {
        id: Uuid,
        title: String,
    },
    AddField {
        collection: Uuid,
        id: Uuid,
        title: String,
        kind: String,
    },
    Record {
        collection: Uuid,
        id: Uuid,
        revision: Option<i64>,
        schema: i64,
        title: String,
        values: Properties,
    },
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Saved,
    Missing,
    Forbidden,
    Duplicate,
    Invalid,
    Conflict,
    FieldLimit,
}

async fn area(
    connection: &mut PgConnection,
    s: TaskScope,
) -> Result<Option<(String, String, bool)>, sqlx::Error> {
    let sql = format!(
        "SELECT t.name,sp.name,access.can_create FROM tenants t JOIN spaces sp ON sp.tenant_id=t.id CROSS JOIN ({SPACE_ACCESS}) access WHERE (t.id,sp.id)=($1,$2)"
    );
    let row: Option<(String, Vec<u8>, bool)> = sqlx::query_as(&sql)
        .bind(s.tenant)
        .bind(s.space)
        .bind(s.account)
        .fetch_optional(connection)
        .await?;
    row.map(|(tenant, name, edit)| Ok((tenant, decode_title(name, "spaces.name")?, edit)))
        .transpose()
}
async fn fields(
    connection: &mut PgConnection,
    s: TaskScope,
    c: Uuid,
) -> Result<Vec<Field>, sqlx::Error> {
    sqlx::query_as("SELECT id,title,kind FROM collection_fields WHERE (tenant_id,space_id,collection_id)=($1,$2,$3) ORDER BY position").bind(s.tenant).bind(s.space).bind(c).fetch_all(connection).await
}

impl Store {
    pub(crate) async fn collection_view(
        &self,
        scope: TaskScope,
        target: ViewTarget,
        offset: i64,
    ) -> Result<Option<View>, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let Some((tenant_name, space_name, editable)) = area(&mut tx, scope).await? else {
            return Ok(None);
        };
        let mut view = View {
            tenant_name,
            space_name,
            editable,
            collections: vec![],
            fields: vec![],
            records: vec![],
            has_more: false,
        };
        let collection = match target {
            ViewTarget::Catalog => None,
            ViewTarget::Table(id) | ViewTarget::NewRecord(id) | ViewTarget::Record(id, _) => {
                Some(id)
            }
        };
        view.collections=sqlx::query_as("SELECT id,title,schema_version FROM space_collections WHERE (tenant_id,space_id)=($1,$2) AND ($3::uuid IS NULL OR id=$3) ORDER BY title_key,id LIMIT $4 OFFSET $5")
            .bind(scope.tenant).bind(scope.space).bind(collection).bind(if collection.is_some(){1}else{PAGE_SIZE+1}).bind(if collection.is_some(){0}else{offset}).fetch_all(&mut *tx).await?;
        if let Some(c) = collection {
            if view.collections.is_empty() {
                return Ok(None);
            }
            view.fields = fields(&mut tx, scope, c).await?;
            if !matches!(target, ViewTarget::NewRecord(_)) {
                let item = if let ViewTarget::Record(_, r) = target {
                    Some(r)
                } else {
                    None
                };
                let rows:Vec<(Uuid,String,i64,String)>=sqlx::query_as("SELECT id,title,version,properties::text FROM collection_records WHERE (tenant_id,space_id,collection_id)=($1,$2,$3) AND ($4::uuid IS NULL OR id=$4) ORDER BY created_at,id LIMIT $5 OFFSET $6")
                    .bind(scope.tenant).bind(scope.space).bind(c).bind(item).bind(PAGE_SIZE+1).bind(if item.is_some(){0}else{offset}).fetch_all(&mut *tx).await?;
                if item.is_some() && rows.is_empty() {
                    return Ok(None);
                }
                view.has_more = rows.len() > PAGE_SIZE as usize;
                for (id, title, version, raw) in rows.into_iter().take(PAGE_SIZE as usize) {
                    let values =
                        serde_json::from_str(&raw).map_err(|e| sqlx::Error::Decode(e.into()))?;
                    view.records.push(Record {
                        id,
                        title,
                        version,
                        values,
                    });
                }
            }
        } else {
            view.has_more = view.collections.len() > PAGE_SIZE as usize;
            view.collections.truncate(PAGE_SIZE as usize);
        }
        tx.commit().await?;
        Ok(Some(view))
    }
    pub(crate) async fn change_collection(
        &self,
        scope: TaskScope,
        change: &Change,
    ) -> Result<Outcome, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        if !lock_active_member(&mut tx, scope.tenant, scope.account).await? {
            return Ok(Outcome::Missing);
        }
        sqlx::query("SELECT id FROM spaces WHERE (tenant_id,id)=($1,$2) FOR NO KEY UPDATE")
            .bind(scope.tenant)
            .bind(scope.space)
            .fetch_optional(&mut *tx)
            .await?;
        match area(&mut tx, scope).await? {
            None => return Ok(Outcome::Missing),
            Some((_, _, false)) => return Ok(Outcome::Forbidden),
            Some(_) => {}
        }
        let title = match change {
            Change::Create { title, .. }
            | Change::AddField { title, .. }
            | Change::Record { title, .. } => title.trim(),
        };
        if !(1..=100).contains(&title.chars().count()) || title.chars().any(char::is_control) {
            return Ok(Outcome::Invalid);
        }
        let result = match change {
            Change::Create { id, .. } => create_collection(&mut tx, scope, *id, title).await?,
            Change::AddField {
                collection,
                id,
                kind,
                ..
            } => add_field(&mut tx, scope, *collection, *id, title, kind).await?,
            Change::Record {
                collection,
                id,
                revision,
                schema,
                values,
                ..
            } => {
                save_record(
                    &mut tx,
                    scope,
                    *collection,
                    *id,
                    *revision,
                    *schema,
                    title,
                    values,
                )
                .await?
            }
        };
        tx.commit().await?;
        Ok(result)
    }
}

async fn create_collection(
    conn: &mut PgConnection,
    s: TaskScope,
    id: Uuid,
    title: &str,
) -> Result<Outcome, sqlx::Error> {
    let inserted=sqlx::query("INSERT INTO space_collections(tenant_id,space_id,id,creator,title,title_key) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING")
        .bind(s.tenant).bind(s.space).bind(id).bind(s.account).bind(title).bind(fold_name(title)).execute(&mut *conn).await?;
    if inserted.rows_affected() == 1 {
        return Ok(Outcome::Saved);
    }
    let previous: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT creator,title FROM space_collections WHERE (tenant_id,space_id,id)=($1,$2,$3)",
    )
    .bind(s.tenant)
    .bind(s.space)
    .bind(id)
    .fetch_optional(conn)
    .await?;
    Ok(match previous {
        Some((creator, prior)) if creator == s.account && prior == title => Outcome::Saved,
        Some(_) => Outcome::Conflict,
        None => Outcome::Duplicate,
    })
}

async fn schema_version(
    conn: &mut PgConnection,
    s: TaskScope,
    id: Uuid,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar("SELECT schema_version FROM space_collections WHERE (tenant_id,space_id,id)=($1,$2,$3) FOR UPDATE").bind(s.tenant).bind(s.space).bind(id).fetch_optional(conn).await
}

async fn add_field(
    conn: &mut PgConnection,
    s: TaskScope,
    c: Uuid,
    id: Uuid,
    title: &str,
    kind: &str,
) -> Result<Outcome, sqlx::Error> {
    let Some(version) = schema_version(conn, s, c).await? else {
        return Ok(Outcome::Missing);
    };
    if !FIELD_TYPES.iter().any(|(k, _)| *k == kind) {
        return Ok(Outcome::Invalid);
    }
    let prior:Option<(Uuid,String,String)>=sqlx::query_as("SELECT creator,title,kind FROM collection_fields WHERE (tenant_id,space_id,collection_id,id)=($1,$2,$3,$4)").bind(s.tenant).bind(s.space).bind(c).bind(id).fetch_optional(&mut *conn).await?;
    if let Some((creator, t, k)) = prior {
        return Ok(if creator == s.account && t == title && k == kind {
            Outcome::Saved
        } else {
            Outcome::Conflict
        });
    }
    if fold_name(title) == "название" {
        return Ok(Outcome::Duplicate);
    }
    if version > 32 {
        return Ok(Outcome::FieldLimit);
    }
    let inserted=sqlx::query("INSERT INTO collection_fields(tenant_id,space_id,collection_id,id,creator,title,title_key,kind,position) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING")
        .bind(s.tenant).bind(s.space).bind(c).bind(id).bind(s.account).bind(title).bind(fold_name(title)).bind(kind).bind(version as i32).execute(&mut *conn).await?;
    if inserted.rows_affected() == 0 {
        return Ok(Outcome::Duplicate);
    }
    sqlx::query("UPDATE space_collections SET schema_version=schema_version+1 WHERE (tenant_id,space_id,id)=($1,$2,$3)").bind(s.tenant).bind(s.space).bind(c).execute(conn).await?;
    Ok(Outcome::Saved)
}

fn calendar_date(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return false;
    }
    let year: u32 = s[..4].parse().unwrap_or_default();
    let month: u32 = s[5..7].parse().unwrap_or_default();
    let day: u32 = s[8..].parse().unwrap_or_default();
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year.is_multiple_of(400) || (year.is_multiple_of(4) && !year.is_multiple_of(100)) => {
            29
        }
        2 => 28,
        _ => 0,
    };
    year > 0 && day > 0 && day <= days
}
fn valid_value(kind: &str, value: &str) -> bool {
    if value.is_empty() {
        return true;
    }
    match kind {
        "text" => value.chars().count() <= 1000 && !value.contains('\0'),
        "number" => value.len() <= 128 && value.parse::<f64>().is_ok_and(f64::is_finite),
        "date" => calendar_date(value),
        "checkbox" => value == "true",
        _ => false,
    }
}

async fn save_record(
    conn: &mut PgConnection,
    s: TaskScope,
    c: Uuid,
    id: Uuid,
    revision: Option<i64>,
    schema: i64,
    title: &str,
    values: &Properties,
) -> Result<Outcome, sqlx::Error> {
    let Some(current_schema) = schema_version(conn, s, c).await? else {
        return Ok(Outcome::Missing);
    };
    let payload = serde_json::to_string(values).expect("String maps are serializable");
    let fingerprint = format!(
        "{:x}",
        Sha256::digest(format!("{schema}\0{title}\0{payload}").as_bytes())
    );
    let existing:Option<(i64,Uuid,String)>=sqlx::query_as("SELECT version,creator,initial_fingerprint FROM collection_records WHERE (tenant_id,space_id,collection_id,id)=($1,$2,$3,$4)").bind(s.tenant).bind(s.space).bind(c).bind(id).fetch_optional(&mut *conn).await?;
    match (revision, existing) {
        (None, Some((_, creator, prior))) => {
            return Ok(if creator == s.account && prior == fingerprint {
                Outcome::Saved
            } else {
                Outcome::Conflict
            });
        }
        (Some(_), None) => return Ok(Outcome::Missing),
        (Some(v), Some((current, _, _))) if v != current => return Ok(Outcome::Conflict),
        _ => {}
    }
    if schema != current_schema {
        return Ok(Outcome::Conflict);
    }
    let fields = fields(conn, s, c).await?;
    if values.iter().any(|(key, v)| {
        !fields
            .iter()
            .any(|f| f.id.to_string() == *key && valid_value(&f.kind, v))
    }) {
        return Ok(Outcome::Invalid);
    }
    if revision.is_some() {
        sqlx::query("UPDATE collection_records SET title=$5,properties=$6::text::jsonb,version=version+1 WHERE (tenant_id,space_id,collection_id,id)=($1,$2,$3,$4)").bind(s.tenant).bind(s.space).bind(c).bind(id).bind(title).bind(payload).execute(conn).await?;
    } else {
        sqlx::query("INSERT INTO collection_records(tenant_id,space_id,collection_id,id,creator,title,properties,initial_fingerprint) VALUES($1,$2,$3,$4,$5,$6,$7::text::jsonb,$8)").bind(s.tenant).bind(s.space).bind(c).bind(id).bind(s.account).bind(title).bind(payload).bind(fingerprint).execute(conn).await?;
    }
    Ok(Outcome::Saved)
}

#[cfg(test)]
mod tests;
