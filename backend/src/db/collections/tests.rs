use super::*;

#[test]
fn validates_field_types_without_losing_precision() {
    assert!(valid_value("text", &"🧭".repeat(1000)));
    assert!(!valid_value("text", &"🧭".repeat(1001)));
    assert!(valid_value("number", "9007199254740993.15"));
    for value in ["NaN", "3 млн", "  ", "inf"] {
        assert!(!valid_value("number", value));
    }
    assert!(valid_value("date", "2024-02-29"));
    for value in ["2025-02-29", "2026-13-01", "2026-11-31", "0000-01-01"] {
        assert!(!valid_value("date", value));
    }
    assert!(!valid_value("checkbox", "yes"));
}

#[sqlx::test(migrator = "crate::db::MIGRATIONS")]
#[ignore = "requires an isolated PostgreSQL DATABASE_URL"]
async fn scoped_collection_schema_records_and_conflicting_edits(
    pool: sqlx::PgPool,
) -> anyhow::Result<()> {
    let [tenant, other, space, owner, editor, reader, foreign] =
        std::array::from_fn(|_| Uuid::new_v4());
    sqlx::query("INSERT INTO tenants VALUES ($1,'Главная'),($2,'Другая')")
        .bind(tenant)
        .bind(other)
        .execute(&pool)
        .await?;
    for (user, t) in [
        (owner, tenant),
        (editor, tenant),
        (reader, tenant),
        (foreign, other),
    ] {
        sqlx::query("INSERT INTO accounts (id,email,name,salt,password_hash) VALUES ($1,$2,'Участник',$3,$4)")
            .bind(user).bind(format!("{user}@example.test")).bind(vec![0u8;16]).bind(vec![0u8;32]).execute(&pool).await?;
        sqlx::query("INSERT INTO tenant_memberships VALUES ($1,$2,'member','active',false,false)")
            .bind(t)
            .bind(user)
            .execute(&pool)
            .await?;
    }
    sqlx::query("INSERT INTO spaces(tenant_id,id,creator_account_id,owner_account_id,name,name_key) VALUES ($1,$2,$3,$3,'Север','north')").bind(tenant).bind(space).bind(owner).execute(&pool).await?;
    sqlx::query("INSERT INTO space_grants VALUES ($1,$2,$3,'editor'),($1,$2,$4,'viewer')")
        .bind(tenant)
        .bind(space)
        .bind(editor)
        .bind(reader)
        .execute(&pool)
        .await?;
    let store = Store::from_test_pool(pool.clone());
    let scope = TaskScope {
        tenant,
        space,
        account: editor,
    };
    let c = Uuid::new_v4();
    let f = Uuid::new_v4();
    let r = Uuid::new_v4();
    let create = Change::Create {
        id: c,
        title: "Интеграции".into(),
    };
    assert_eq!(
        store.change_collection(scope, &create).await?,
        Outcome::Saved
    );
    assert_eq!(
        store.change_collection(scope, &create).await?,
        Outcome::Saved
    );
    assert_eq!(
        store
            .change_collection(
                scope,
                &Change::Create {
                    id: Uuid::new_v4(),
                    title: "ИНТЕГРАЦИИ".into()
                }
            )
            .await?,
        Outcome::Duplicate
    );
    let add = Change::AddField {
        collection: c,
        id: f,
        title: "Дата".into(),
        kind: "date".into(),
    };
    assert_eq!(store.change_collection(scope, &add).await?, Outcome::Saved);
    assert_eq!(store.change_collection(scope, &add).await?, Outcome::Saved);
    let build = |revision, title: &str, value: &str| Change::Record {
        collection: c,
        id: r,
        revision,
        schema: 2,
        title: title.into(),
        values: [(f.to_string(), value.into())].into(),
    };
    let original = build(None, "TeamStream", "2026-11-21");
    assert_eq!(
        store.change_collection(scope, &original).await?,
        Outcome::Saved
    );
    assert_eq!(
        store.change_collection(scope, &original).await?,
        Outcome::Saved
    );
    assert_eq!(
        store
            .change_collection(scope, &build(Some(1), "Новое", "2026-02-29"))
            .await?,
        Outcome::Invalid
    );
    assert_eq!(
        store
            .change_collection(scope, &build(Some(1), "Готово", "2026-11-22"))
            .await?,
        Outcome::Saved
    );
    assert_eq!(
        store
            .change_collection(scope, &build(Some(1), "Старое", "2026-11-20"))
            .await?,
        Outcome::Conflict
    );
    let reader_scope = TaskScope {
        account: reader,
        ..scope
    };
    let page = store
        .collection_view(reader_scope, ViewTarget::Record(c, r), 0)
        .await?
        .unwrap();
    assert!(!page.editable);
    assert_eq!(page.records[0].title, "Готово");
    assert_eq!(page.records[0].values[&f.to_string()], "2026-11-22");
    assert_eq!(
        store.change_collection(reader_scope, &create).await?,
        Outcome::Forbidden
    );
    let alien = TaskScope {
        tenant: other,
        account: foreign,
        ..scope
    };
    assert!(
        store
            .collection_view(alien, ViewTarget::Record(c, r), 0)
            .await?
            .is_none()
    );
    sqlx::query("DELETE FROM space_grants WHERE tenant_id=$1 AND space_id=$2 AND account_id=$3")
        .bind(tenant)
        .bind(space)
        .bind(editor)
        .execute(&pool)
        .await?;
    assert_eq!(
        store.change_collection(scope, &original).await?,
        Outcome::Missing
    );
    Ok(())
}
