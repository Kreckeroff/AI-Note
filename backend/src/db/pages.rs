use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::{
    PageAddress, Store,
    page_history::record_current_snapshot,
    sections::{SPACE_ACCESS, lock_active_member},
};
use crate::text::{fold_name, js_trim};

#[derive(Clone)]
pub(crate) struct Page {
    pub id: Uuid,
    pub section_id: Uuid,
    pub title: String,
}

pub(crate) enum CreatePage {
    Saved(Uuid),
    MissingSection,
    Forbidden,
    OperationConflict,
    EmptyName,
    LongName,
    Duplicate,
}

#[derive(Clone, Copy)]
pub(crate) struct CreatePageInput<'a> {
    pub tenant_id: Uuid,
    pub space_id: Uuid,
    pub section_id: Uuid,
    pub account_id: Uuid,
    pub operation_id: Uuid,
    pub title: &'a str,
}

impl Store {
    pub(crate) async fn create_page(
        &self,
        input: CreatePageInput<'_>,
    ) -> Result<CreatePage, sqlx::Error> {
        let CreatePageInput {
            tenant_id: tenant,
            space_id: space,
            section_id: section,
            account_id: account,
            operation_id: operation,
            title: entered,
        } = input;
        let mut tx = self.0.begin().await?;
        sqlx::query("SET LOCAL statement_timeout = '3s'")
            .execute(&mut *tx)
            .await?;
        if !lock_active_member(&mut tx, tenant, account).await? {
            return Ok(CreatePage::MissingSection);
        }

        // Serialize appends and operation replays within the visible parent.
        let access = format!(
            "SELECT access.can_create FROM ({SPACE_ACCESS}) access
             JOIN sections r ON r.tenant_id = $1 AND r.space_id = $2 AND r.id = $4
             FOR NO KEY UPDATE OF r"
        );
        let can_create: Option<bool> = sqlx::query_scalar(&access)
            .bind(tenant)
            .bind(space)
            .bind(account)
            .bind(section)
            .fetch_optional(&mut *tx)
            .await?;
        match can_create {
            None => return Ok(CreatePage::MissingSection),
            Some(false) => return Ok(CreatePage::Forbidden),
            Some(true) => {}
        }

        let title = js_trim(entered);
        let fingerprint = format!("{:x}", Sha256::digest(title.as_bytes()));
        let existing: Option<(String, Uuid)> = sqlx::query_as(
            "SELECT request_fingerprint, page_id FROM page_creations
             WHERE tenant_id = $1 AND space_id = $2 AND section_id = $3
               AND creator_account_id = $4 AND operation_id = $5",
        )
        .bind(tenant)
        .bind(space)
        .bind(section)
        .bind(account)
        .bind(operation)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some((previous, id)) = existing {
            return Ok(if previous == fingerprint {
                CreatePage::Saved(id)
            } else {
                CreatePage::OperationConflict
            });
        }
        if title.is_empty() {
            return Ok(CreatePage::EmptyName);
        }
        if title.chars().count() > 100 {
            return Ok(CreatePage::LongName);
        }

        let id = Uuid::new_v4();
        let added = sqlx::query(
            "INSERT INTO pages (tenant_id, space_id, section_id, id, creator_account_id, title, title_key, position)
             SELECT $1, $2, $3, $4, $5, $6, $7, coalesce(max(position), 0) + 1
             FROM pages WHERE tenant_id = $1 AND space_id = $2 AND section_id = $3
             ON CONFLICT (tenant_id, space_id, section_id, title_key) DO NOTHING",
        )
        .bind(tenant)
        .bind(space)
        .bind(section)
        .bind(id)
        .bind(account)
        .bind(title.as_bytes())
        .bind(fold_name(title).into_bytes())
        .execute(&mut *tx)
        .await?;
        if added.rows_affected() == 0 {
            return Ok(CreatePage::Duplicate);
        }
        let address = PageAddress {
            tenant,
            space,
            page: id,
            account,
        };
        record_current_snapshot(&mut tx, &address, Some(account), None).await?;

        sqlx::query(
            "INSERT INTO page_creations
                 (tenant_id, space_id, section_id, creator_account_id, operation_id, request_fingerprint, page_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(tenant)
        .bind(space)
        .bind(section)
        .bind(account)
        .bind(operation)
        .bind(fingerprint)
        .bind(id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(CreatePage::Saved(id))
    }
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    use super::*;
    use crate::db::CreateSection;

    #[sqlx::test(migrator = "crate::db::MIGRATIONS")]
    #[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
    async fn page_permissions_names_and_operation_replay(pool: PgPool) -> anyhow::Result<()> {
        let [tenant, space, owner, editor, viewer, admin, stranger] =
            std::array::from_fn(|_| Uuid::new_v4());
        sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'Север')")
            .bind(tenant)
            .execute(&pool)
            .await?;
        for (id, email, role) in [
            (owner, "owner@example.test", "member"),
            (editor, "editor@example.test", "member"),
            (viewer, "viewer@example.test", "member"),
            (admin, "admin@example.test", "admin"),
            (stranger, "stranger@example.test", "member"),
        ] {
            sqlx::query("INSERT INTO accounts (id, email, name, salt, password_hash) VALUES ($1, $2, $2, $3, $4)")
                .bind(id).bind(email).bind(vec![0_u8; 16]).bind(vec![0_u8; 32]).execute(&pool).await?;
            sqlx::query(
                "INSERT INTO tenant_memberships VALUES ($1, $2, $3, 'active', false, false)",
            )
            .bind(tenant)
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
        .bind(viewer)
        .execute(&pool)
        .await?;
        let store = Store(pool);
        let CreateSection::Saved(section) = store
            .create_section(tenant, space, owner, Uuid::new_v4(), "Инструкции")
            .await?
        else {
            panic!("create parent section");
        };
        let CreateSection::Saved(other) = store
            .create_section(tenant, space, owner, Uuid::new_v4(), "Решения")
            .await?
        else {
            panic!("create another section");
        };

        let input = CreatePageInput {
            tenant_id: tenant,
            space_id: space,
            section_id: section,
            account_id: editor,
            operation_id: Uuid::new_v4(),
            title: "  Café 🙂 ",
        };
        for account in [viewer, admin] {
            assert!(matches!(
                store
                    .create_page(CreatePageInput {
                        account_id: account,
                        ..input
                    })
                    .await?,
                CreatePage::Forbidden,
            ));
        }
        for (tenant, parent, account) in [
            (tenant, section, stranger),
            (tenant, Uuid::new_v4(), admin),
            (Uuid::new_v4(), section, owner),
        ] {
            assert!(matches!(
                store
                    .create_page(CreatePageInput {
                        tenant_id: tenant,
                        section_id: parent,
                        account_id: account,
                        ..input
                    })
                    .await?,
                CreatePage::MissingSection,
            ));
        }
        for (title, must_be_empty) in [("  ".to_owned(), true), ("🙂".repeat(101), false)] {
            let result = store
                .create_page(CreatePageInput {
                    title: &title,
                    ..input
                })
                .await?;
            assert!(matches!(
                (result, must_be_empty),
                (CreatePage::EmptyName, true) | (CreatePage::LongName, false)
            ));
        }

        let (first, replay) = tokio::join!(
            store.create_page(input),
            store.create_page(CreatePageInput {
                title: "Café 🙂",
                ..input
            }),
        );
        let (CreatePage::Saved(id), CreatePage::Saved(replayed)) = (first?, replay?) else {
            panic!("parallel replays should identify the same page");
        };
        assert_eq!(id, replayed);
        assert!(matches!(
            store
                .create_page(CreatePageInput {
                    title: "Другое",
                    ..input
                })
                .await?,
            CreatePage::OperationConflict,
        ));
        let duplicate = CreatePageInput {
            account_id: owner,
            operation_id: Uuid::new_v4(),
            title: "CAFE\u{301} 🙂",
            ..input
        };
        assert!(matches!(
            store.create_page(duplicate).await?,
            CreatePage::Duplicate
        ));
        assert!(matches!(
            store
                .create_page(CreatePageInput {
                    section_id: other,
                    ..duplicate
                })
                .await?,
            CreatePage::Saved(_),
        ));

        let hundred = "🙂".repeat(100);
        let (left, right) = tokio::join!(
            store.create_page(CreatePageInput {
                account_id: owner,
                operation_id: Uuid::new_v4(),
                title: &hundred,
                ..input
            }),
            store.create_page(CreatePageInput {
                operation_id: Uuid::new_v4(),
                title: "Параллельная страница",
                ..input
            }),
        );
        let (CreatePage::Saved(left), CreatePage::Saved(right)) = (left?, right?) else {
            panic!("parallel pages with different titles should both persist");
        };
        for reader in [viewer, admin] {
            let tree = store.sections(tenant, space, reader).await?.unwrap();
            assert!(!tree.can_create);
            let rows: Vec<_> = tree
                .pages
                .iter()
                .filter(|page| page.section_id == section)
                .collect();
            assert_eq!(rows.len(), 3);
            assert_eq!((rows[0].id, rows[0].title.as_str()), (id, "Café 🙂"));
            assert!(rows[1..].iter().any(|page| page.id == left));
            assert!(rows[1..].iter().any(|page| page.id == right));
        }
        let markdown: String = sqlx::query_scalar(
            "SELECT markdown FROM pages WHERE tenant_id=$1 AND space_id=$2 AND id=$3",
        )
        .bind(tenant)
        .bind(space)
        .bind(id)
        .fetch_one(&store.0)
        .await?;
        assert!(markdown.is_empty());

        sqlx::query(
            "UPDATE tenant_memberships SET status='inactive' WHERE tenant_id=$1 AND account_id=$2",
        )
        .bind(tenant)
        .bind(editor)
        .execute(&store.0)
        .await?;
        assert!(matches!(
            store.create_page(input).await?,
            CreatePage::MissingSection,
        ));
        Ok(())
    }
}
