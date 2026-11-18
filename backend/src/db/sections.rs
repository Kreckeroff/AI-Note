use sha2::{Digest, Sha256};
use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use super::{Page, Store};
use crate::text::{fold_name, js_trim};

#[derive(Clone)]
pub(crate) struct Section {
    pub id: Uuid,
    pub title: String,
}

pub(crate) struct SectionTree {
    pub sections: Vec<Section>,
    pub pages: Vec<Page>,
    pub can_create: bool,
}

#[derive(FromRow)]
struct TreeRow {
    can_create: bool,
    section_id: Option<Uuid>,
    section_title: Option<Vec<u8>>,
    page_id: Option<Uuid>,
    page_title: Option<Vec<u8>>,
}

pub(crate) enum CreateSection {
    Saved(Uuid),
    MissingSpace,
    Forbidden,
    OperationConflict,
    EmptyName,
    LongName,
    Duplicate,
}

// A visible space has exactly one access row for this active membership.
pub(super) const SPACE_ACCESS: &str =
    "SELECT (s.owner_account_id = m.account_id OR g.role IS NOT DISTINCT FROM 'editor') AS can_create
     FROM tenant_memberships m
     JOIN spaces s ON s.tenant_id = m.tenant_id AND s.id = $2
     LEFT JOIN space_grants g ON g.tenant_id = s.tenant_id AND g.space_id = s.id AND g.account_id = m.account_id
     WHERE m.tenant_id = $1 AND m.account_id = $3 AND m.status = 'active'
       AND (s.owner_account_id = m.account_id OR m.role IN ('admin', 'superadmin') OR g.role IS NOT NULL)";

pub(super) async fn lock_active_member(
    tx: &mut PgConnection,
    tenant: Uuid,
    account: Uuid,
) -> Result<bool, sqlx::Error> {
    // Grant changes lock this membership. Check the grant after taking that lock.
    let member: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM tenant_memberships
         WHERE tenant_id = $1 AND account_id = $2 AND status = 'active' FOR UPDATE",
    )
    .bind(tenant)
    .bind(account)
    .fetch_optional(tx)
    .await?;
    Ok(member.is_some())
}

fn decode_title(title: Vec<u8>, column: &str) -> Result<String, sqlx::Error> {
    String::from_utf8(title).map_err(|source| sqlx::Error::ColumnDecode {
        index: column.to_owned(),
        source: Box::new(source),
    })
}

impl Store {
    pub(crate) async fn sections(
        &self,
        tenant: Uuid,
        space: Uuid,
        account: Uuid,
    ) -> Result<Option<SectionTree>, sqlx::Error> {
        let query = format!(
            "SELECT access.can_create, r.id AS section_id, r.title AS section_title,
                    p.id AS page_id, p.title AS page_title
             FROM ({SPACE_ACCESS}) access
             LEFT JOIN sections r ON r.tenant_id = $1 AND r.space_id = $2
             LEFT JOIN pages p ON p.tenant_id = r.tenant_id AND p.space_id = r.space_id AND p.section_id = r.id
             ORDER BY r.position, p.position",
        );
        let rows: Vec<TreeRow> = sqlx::query_as(&query)
            .bind(tenant)
            .bind(space)
            .bind(account)
            .fetch_all(&self.0)
            .await?;
        let Some(first) = rows.first() else {
            return Ok(None);
        };
        let mut tree = SectionTree {
            sections: Vec::new(),
            pages: Vec::new(),
            can_create: first.can_create,
        };
        for row in rows {
            if let Some((section_id, title)) = row.section_id.zip(row.section_title) {
                if tree
                    .sections
                    .last()
                    .is_none_or(|section| section.id != section_id)
                {
                    tree.sections.push(Section {
                        id: section_id,
                        title: decode_title(title, "sections.title")?,
                    });
                }
                if let Some((id, title)) = row.page_id.zip(row.page_title) {
                    tree.pages.push(Page {
                        id,
                        section_id,
                        title: decode_title(title, "pages.title")?,
                    });
                }
            }
        }
        Ok(Some(tree))
    }

    pub(crate) async fn create_section(
        &self,
        tenant: Uuid,
        space: Uuid,
        account: Uuid,
        operation: Uuid,
        entered: &str,
    ) -> Result<CreateSection, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        sqlx::query("SET LOCAL statement_timeout = '3s'")
            .execute(&mut *tx)
            .await?;
        if !lock_active_member(&mut tx, tenant, account).await? {
            return Ok(CreateSection::MissingSpace);
        }

        // All writers in the same space take this lock before allocating a position.
        let access = format!("{SPACE_ACCESS} FOR NO KEY UPDATE OF s");
        let can_create: Option<bool> = sqlx::query_scalar(&access)
            .bind(tenant)
            .bind(space)
            .bind(account)
            .fetch_optional(&mut *tx)
            .await?;
        match can_create {
            None => return Ok(CreateSection::MissingSpace),
            Some(false) => return Ok(CreateSection::Forbidden),
            Some(true) => {}
        }

        let title = js_trim(entered);
        let fingerprint = format!("{:x}", Sha256::digest(title.as_bytes()));
        let existing: Option<(String, Uuid)> = sqlx::query_as(
            "SELECT request_fingerprint, section_id FROM section_creations
             WHERE tenant_id = $1 AND space_id = $2 AND creator_account_id = $3 AND operation_id = $4",
        )
        .bind(tenant)
        .bind(space)
        .bind(account)
        .bind(operation)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((previous, id)) = existing {
            return Ok(if previous == fingerprint {
                CreateSection::Saved(id)
            } else {
                CreateSection::OperationConflict
            });
        }
        if title.is_empty() {
            return Ok(CreateSection::EmptyName);
        }
        if title.chars().count() > 100 {
            return Ok(CreateSection::LongName);
        }

        let id = Uuid::new_v4();
        let added = sqlx::query(
            "INSERT INTO sections (tenant_id, space_id, id, creator_account_id, title, title_key, position)
             SELECT $1, $2, $3, $4, $5, $6, coalesce(max(position), 0) + 1
             FROM sections WHERE tenant_id = $1 AND space_id = $2
             ON CONFLICT (tenant_id, space_id, title_key) DO NOTHING",
        )
        .bind(tenant)
        .bind(space)
        .bind(id)
        .bind(account)
        .bind(title.as_bytes())
        .bind(fold_name(title).into_bytes())
        .execute(&mut *tx)
        .await?;
        if added.rows_affected() == 0 {
            return Ok(CreateSection::Duplicate);
        }

        sqlx::query(
            "INSERT INTO section_creations (tenant_id, space_id, creator_account_id, operation_id, request_fingerprint, section_id)
             VALUES ($1, $2, $3, $4, $5, $6)",
        )
        .bind(tenant)
        .bind(space)
        .bind(account)
        .bind(operation)
        .bind(fingerprint)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(CreateSection::Saved(id))
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;

    #[sqlx::test(migrator = "crate::db::MIGRATIONS")]
    #[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
    async fn section_access_order_and_replay(pool: PgPool) -> anyhow::Result<()> {
        let tenant = Uuid::new_v4();
        let foreign_tenant = Uuid::new_v4();
        let space = Uuid::new_v4();
        let owner = Uuid::new_v4();
        let editor = Uuid::new_v4();
        let reader = Uuid::new_v4();
        let admin = Uuid::new_v4();
        let foreign_admin = Uuid::new_v4();
        sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'Среда'), ($2, 'Другая среда')")
            .bind(tenant)
            .bind(foreign_tenant)
            .execute(&pool)
            .await?;
        for (id, tenant_id, email, role) in [
            (owner, tenant, "owner@example.test", "member"),
            (editor, tenant, "editor@example.test", "member"),
            (reader, tenant, "reader@example.test", "member"),
            (admin, tenant, "admin@example.test", "admin"),
            (
                foreign_admin,
                foreign_tenant,
                "foreign@example.test",
                "admin",
            ),
        ] {
            sqlx::query("INSERT INTO accounts (id, email, name, salt, password_hash) VALUES ($1, $2, $2, $3, $4)")
                .bind(id).bind(email).bind(vec![0_u8; 16]).bind(vec![0_u8; 32]).execute(&pool).await?;
            sqlx::query(
                "INSERT INTO tenant_memberships VALUES ($1, $2, $3, 'active', false, false)",
            )
            .bind(tenant_id)
            .bind(id)
            .bind(role)
            .execute(&pool)
            .await?;
        }
        sqlx::query("INSERT INTO spaces (tenant_id, id, creator_account_id, owner_account_id, name, name_key) VALUES ($1, $2, $3, $3, 'space', 'space')")
            .bind(tenant).bind(space).bind(owner).execute(&pool).await?;
        sqlx::query(
            "INSERT INTO space_grants VALUES ($1, $2, $3, 'editor'), ($1, $2, $4, 'viewer')",
        )
        .bind(tenant)
        .bind(space)
        .bind(editor)
        .bind(reader)
        .execute(&pool)
        .await?;

        let store = Store(pool);
        // Workspace-create permission is false for everyone in the fixture, regardless of section access.
        for account in [owner, editor] {
            let tree = store.sections(tenant, space, account).await?.unwrap();
            assert!(tree.can_create);
            assert!(tree.sections.is_empty());
        }
        for account in [reader, admin] {
            assert!(
                !store
                    .sections(tenant, space, account)
                    .await?
                    .unwrap()
                    .can_create
            );
            assert!(matches!(
                store
                    .create_section(tenant, space, account, Uuid::new_v4(), "Скрытая правка")
                    .await?,
                CreateSection::Forbidden
            ));
        }
        assert!(
            store
                .sections(tenant, space, foreign_admin)
                .await?
                .is_none()
        );
        assert!(
            store
                .sections(foreign_tenant, space, foreign_admin)
                .await?
                .is_none()
        );
        assert!(matches!(
            store
                .create_section(tenant, space, foreign_admin, Uuid::new_v4(), "Чужая среда")
                .await?,
            CreateSection::MissingSpace
        ));

        let operation = Uuid::new_v4();
        let CreateSection::Saved(first) = store
            .create_section(tenant, space, owner, operation, "  Café 🙂  ")
            .await?
        else {
            panic!("first section must be created");
        };
        assert!(matches!(
            store.create_section(tenant, space, owner, operation, "Café 🙂").await?,
            CreateSection::Saved(id) if id == first
        ));
        assert!(matches!(
            store
                .create_section(tenant, space, owner, operation, "Другое название")
                .await?,
            CreateSection::OperationConflict
        ));
        assert!(matches!(
            store
                .create_section(tenant, space, editor, Uuid::new_v4(), "CAFE\u{301} 🙂")
                .await?,
            CreateSection::Duplicate
        ));
        assert!(matches!(
            store
                .create_section(tenant, space, owner, Uuid::new_v4(), " \t ")
                .await?,
            CreateSection::EmptyName
        ));
        assert!(matches!(
            store
                .create_section(tenant, space, owner, Uuid::new_v4(), &"🙂".repeat(101))
                .await?,
            CreateSection::LongName
        ));

        let editor_operation = Uuid::new_v4();
        let owner_operation = Uuid::new_v4();
        let (edit_result, own_result) = tokio::join!(
            store.create_section(tenant, space, editor, editor_operation, "Раздел редактора"),
            store.create_section(tenant, space, owner, owner_operation, "Раздел владельца"),
        );
        let (CreateSection::Saved(second), CreateSection::Saved(third)) =
            (edit_result?, own_result?)
        else {
            panic!("parallel appends must persist independently");
        };
        let observed = store
            .sections(tenant, space, reader)
            .await?
            .unwrap()
            .sections;
        assert_eq!(observed.len(), 3);
        assert_eq!(
            (observed[0].id, observed[0].title.as_str()),
            (first, "Café 🙂")
        );
        assert!(observed[1..].iter().any(|row| row.id == second));
        assert!(observed[1..].iter().any(|row| row.id == third));
        assert_eq!(
            store
                .sections(tenant, space, admin)
                .await?
                .unwrap()
                .sections
                .into_iter()
                .map(|row| row.id)
                .collect::<Vec<_>>(),
            observed.iter().map(|row| row.id).collect::<Vec<_>>()
        );

        sqlx::query(
            "DELETE FROM space_grants WHERE tenant_id=$1 AND space_id=$2 AND account_id=$3",
        )
        .bind(tenant)
        .bind(space)
        .bind(editor)
        .execute(&store.0)
        .await?;
        assert!(matches!(
            store
                .create_section(tenant, space, editor, editor_operation, "Раздел редактора")
                .await?,
            CreateSection::MissingSpace
        ));
        Ok(())
    }
}
