use std::{path::Path, str::FromStr};

use anyhow::{Context, ensure};
use rusqlite::{Connection, OpenFlags, Row};
use sqlx::{Postgres, QueryBuilder};
use uuid::Uuid;

use crate::db::{NewAccount, NewSession, Store};

struct Membership {
    tenant: Uuid,
    account: Uuid,
    role: String,
    status: String,
    can_create: bool,
    is_current: bool,
}

struct Space {
    tenant: Uuid,
    id: Uuid,
    creator: Uuid,
    owner: Uuid,
    name: String,
    name_key: String,
}

struct Creation {
    tenant: Uuid,
    creator: Uuid,
    operation: Uuid,
    fingerprint: String,
    space: Uuid,
}

struct Grant {
    tenant: Uuid,
    space: Uuid,
    account: Uuid,
    role: String,
}

struct Snapshot {
    accounts: Vec<NewAccount>,
    sessions: Vec<(Uuid, NewSession)>,
    tenants: Vec<(Uuid, String)>,
    memberships: Vec<Membership>,
    spaces: Vec<Space>,
    creations: Vec<Creation>,
    grants: Vec<Grant>,
}

impl Snapshot {
    fn read(path: &Path) -> anyhow::Result<Self> {
        let mut source = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .context("Не удалось открыть исходную SQLite для чтения")?;
        let tx = source.transaction()?;
        let version: u8 = tx.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        ensure!(
            (1..=3).contains(&version),
            "Поддерживаются схемы SQLite 1, 2 и 3"
        );
        let integrity: String = tx.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        ensure!(
            integrity == "ok",
            "Исходная SQLite не прошла проверку целостности"
        );
        ensure!(
            !tx.prepare("PRAGMA foreign_key_check")?.exists([])?,
            "В исходной SQLite нарушены внешние ключи"
        );
        let accounts = read_rows(
            &tx,
            "SELECT id, email, name, salt, password_hash FROM accounts",
            |row| {
                Ok(NewAccount {
                    id: uuid(row, 0)?,
                    email: row.get(1)?,
                    name: row.get(2)?,
                    salt: row.get(3)?,
                    password_hash: row.get(4)?,
                })
            },
        )?;
        let sessions = read_rows(
            &tx,
            "SELECT account_id, token_digest, created_at, expires_at FROM sessions",
            |row| {
                Ok((
                    uuid(row, 0)?,
                    NewSession {
                        digest: row.get(1)?,
                        created_at: row.get(2)?,
                        expires_at: row.get(3)?,
                    },
                ))
            },
        )?;
        let mut data = Self {
            accounts,
            sessions,
            tenants: Vec::new(),
            memberships: Vec::new(),
            spaces: Vec::new(),
            creations: Vec::new(),
            grants: Vec::new(),
        };
        if version >= 2 {
            data.tenants = read_rows(&tx, "SELECT id, name FROM tenants", |row| {
                Ok((uuid(row, 0)?, row.get(1)?))
            })?;
            data.memberships = read_rows(
                &tx,
                "SELECT tenant_id, account_id, role, status, can_create, is_current FROM tenant_memberships",
                |row| {
                    Ok(Membership {
                        tenant: uuid(row, 0)?,
                        account: uuid(row, 1)?,
                        role: row.get(2)?,
                        status: row.get(3)?,
                        can_create: row.get(4)?,
                        is_current: row.get(5)?,
                    })
                },
            )?;
            data.spaces = read_rows(
                &tx,
                "SELECT tenant_id, id, creator_account_id, owner_account_id, name, name_key FROM spaces",
                |row| {
                    Ok(Space {
                        tenant: uuid(row, 0)?,
                        id: uuid(row, 1)?,
                        creator: uuid(row, 2)?,
                        owner: uuid(row, 3)?,
                        name: row.get(4)?,
                        name_key: row.get(5)?,
                    })
                },
            )?;
            data.creations = read_rows(
                &tx,
                "SELECT tenant_id, creator_account_id, operation_id, request_fingerprint, space_id FROM space_creations",
                |row| {
                    Ok(Creation {
                        tenant: uuid(row, 0)?,
                        creator: uuid(row, 1)?,
                        operation: uuid(row, 2)?,
                        fingerprint: row.get(3)?,
                        space: uuid(row, 4)?,
                    })
                },
            )?;
        }
        if version >= 3 {
            data.grants = read_rows(
                &tx,
                "SELECT tenant_id, space_id, account_id, role FROM space_grants",
                |row| {
                    Ok(Grant {
                        tenant: uuid(row, 0)?,
                        space: uuid(row, 1)?,
                        account: uuid(row, 2)?,
                        role: row.get(3)?,
                    })
                },
            )?;
        }
        tx.commit()?;
        Ok(data)
    }

    async fn import(self, store: &Store) -> anyhow::Result<()> {
        let tables = [
            ("accounts", self.accounts.len()),
            ("sessions", self.sessions.len()),
            ("tenants", self.tenants.len()),
            ("tenant_memberships", self.memberships.len()),
            ("spaces", self.spaces.len()),
            ("space_creations", self.creations.len()),
            ("space_grants", self.grants.len()),
        ];
        let mut tx = store.0.begin().await?;
        sqlx::query("SET LOCAL lock_timeout = '5s'")
            .execute(&mut *tx)
            .await?;
        sqlx::query(
            "LOCK TABLE accounts, sessions, tenants, tenant_memberships, spaces, space_creations, space_grants
             IN SHARE ROW EXCLUSIVE MODE",
        ).execute(&mut *tx).await?;
        for (table, _) in tables {
            let has_data: bool =
                sqlx::query_scalar(&format!("SELECT EXISTS (SELECT 1 FROM {table})"))
                    .fetch_one(&mut *tx)
                    .await?;
            ensure!(
                !has_data,
                "Для импорта нужна пустая база PostgreSQL (таблица {table} содержит данные)"
            );
        }

        // Bounded multi-row inserts keep an arbitrary snapshot below PostgreSQL's parameter limit.
        const BATCH: usize = 500;
        for rows in self.accounts.chunks(BATCH) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO accounts (id, email, name, salt, password_hash) ",
            );
            query
                .push_values(rows, |mut values, row| {
                    values
                        .push_bind(row.id)
                        .push_bind(&row.email)
                        .push_bind(&row.name)
                        .push_bind(&row.salt)
                        .push_bind(&row.password_hash);
                })
                .build()
                .execute(&mut *tx)
                .await?;
        }
        for rows in self.sessions.chunks(BATCH) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO sessions (account_id, token_digest, created_at, expires_at) ",
            );
            query
                .push_values(rows, |mut values, (account, row)| {
                    values
                        .push_bind(*account)
                        .push_bind(&row.digest)
                        .push_bind(row.created_at)
                        .push_bind(row.expires_at);
                })
                .build()
                .execute(&mut *tx)
                .await?;
        }
        for rows in self.tenants.chunks(BATCH) {
            let mut query = QueryBuilder::<Postgres>::new("INSERT INTO tenants (id, name) ");
            query
                .push_values(rows, |mut values, (id, name)| {
                    values.push_bind(*id).push_bind(name);
                })
                .build()
                .execute(&mut *tx)
                .await?;
        }
        for rows in self.memberships.chunks(BATCH) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO tenant_memberships (tenant_id, account_id, role, status, can_create, is_current) ",
            );
            query
                .push_values(rows, |mut values, row| {
                    values
                        .push_bind(row.tenant)
                        .push_bind(row.account)
                        .push_bind(&row.role)
                        .push_bind(&row.status)
                        .push_bind(row.can_create)
                        .push_bind(row.is_current);
                })
                .build()
                .execute(&mut *tx)
                .await?;
        }
        for rows in self.spaces.chunks(BATCH) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO spaces (tenant_id, id, creator_account_id, owner_account_id, name, name_key) ",
            );
            query
                .push_values(rows, |mut values, row| {
                    values
                        .push_bind(row.tenant)
                        .push_bind(row.id)
                        .push_bind(row.creator)
                        .push_bind(row.owner)
                        .push_bind(row.name.as_bytes())
                        .push_bind(row.name_key.as_bytes());
                })
                .build()
                .execute(&mut *tx)
                .await?;
        }
        for rows in self.creations.chunks(BATCH) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO space_creations (tenant_id, creator_account_id, operation_id, request_fingerprint, space_id) ",
            );
            query
                .push_values(rows, |mut values, row| {
                    values
                        .push_bind(row.tenant)
                        .push_bind(row.creator)
                        .push_bind(row.operation)
                        .push_bind(&row.fingerprint)
                        .push_bind(row.space);
                })
                .build()
                .execute(&mut *tx)
                .await?;
        }
        for rows in self.grants.chunks(BATCH) {
            let mut query = QueryBuilder::<Postgres>::new(
                "INSERT INTO space_grants (tenant_id, space_id, account_id, role) ",
            );
            query
                .push_values(rows, |mut values, row| {
                    values
                        .push_bind(row.tenant)
                        .push_bind(row.space)
                        .push_bind(row.account)
                        .push_bind(&row.role);
                })
                .build()
                .execute(&mut *tx)
                .await?;
        }
        for (table, expected) in tables {
            let actual: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {table}"))
                .fetch_one(&mut *tx)
                .await?;
            ensure!(
                actual as usize == expected,
                "Количество записей после импорта не совпало: {table}"
            );
        }
        tx.commit().await?;
        for (table, count) in tables {
            println!("{table} {count}");
        }
        Ok(())
    }
}

fn read_rows<T>(
    connection: &Connection,
    query: &str,
    map: impl FnMut(&Row<'_>) -> rusqlite::Result<T>,
) -> anyhow::Result<Vec<T>> {
    let mut statement = connection.prepare(query)?;
    Ok(statement
        .query_map([], map)?
        .collect::<rusqlite::Result<_>>()?)
}

fn uuid(row: &Row<'_>, column: usize) -> rusqlite::Result<Uuid> {
    let text: String = row.get(column)?;
    Uuid::from_str(&text).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            column,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

pub(crate) async fn sqlite_to_postgres(store: &Store, source: &Path) -> anyhow::Result<()> {
    let source = source.to_path_buf();
    let snapshot = tokio::task::spawn_blocking(move || Snapshot::read(&source)).await??;
    snapshot.import(store).await
}
