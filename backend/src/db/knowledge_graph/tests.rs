use axum::{body, body::Body, extract::Request, http::StatusCode, response::Response};
use serde_json::Value;
use sqlx::PgPool;

use super::*;
use crate::{
    db::{
        CreatePage, CreatePageInput, Principal,
        tasks::{CreateTask, TaskStatus, TaskWrite},
    },
    knowledge_graph::{GraphRoute, handle},
    workspace::space_path,
};

#[sqlx::test(migrator = "crate::db::MIGRATIONS")]
#[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
async fn graph_projects_only_permitted_versions_and_published_results(
    pool: PgPool,
) -> anyhow::Result<()> {
    let [tenant, space, owner, reader, section] = std::array::from_fn(|_| Uuid::new_v4());
    sqlx::query("INSERT INTO tenants (id,name) VALUES ($1,'Команда')")
        .bind(tenant)
        .execute(&pool)
        .await?;
    for (account, email) in [
        (owner, "owner@example.test"),
        (reader, "reader@example.test"),
    ] {
        sqlx::query(
            "INSERT INTO accounts(id,email,name,salt,password_hash) VALUES ($1,$2,$2,$3,$4)",
        )
        .bind(account)
        .bind(email)
        .bind(vec![0_u8; 16])
        .bind(vec![0_u8; 32])
        .execute(&pool)
        .await?;
        sqlx::query("INSERT INTO tenant_memberships VALUES ($1,$2,'member','active',false,false)")
            .bind(tenant)
            .bind(account)
            .execute(&pool)
            .await?;
    }
    sqlx::query(
        "INSERT INTO spaces (tenant_id,id,creator_account_id,owner_account_id,name,name_key)
                 VALUES ($1,$2,$3,$3,'Документы','documents')",
    )
    .bind(tenant)
    .bind(space)
    .bind(owner)
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO sections (tenant_id,space_id,id,creator_account_id,title,title_key,position)
                 VALUES ($1,$2,$3,$4,'Раздел','section',1)",
    )
    .bind(tenant)
    .bind(space)
    .bind(section)
    .bind(owner)
    .execute(&pool)
    .await?;
    sqlx::query("INSERT INTO space_grants VALUES ($1,$2,$3,'viewer')")
        .bind(tenant)
        .bind(space)
        .bind(reader)
        .execute(&pool)
        .await?;
    let store = Store::from_test_pool(pool.clone());
    let mut pages = Vec::new();
    for title in ["Первое название", "Опубликованная спецификация"]
    {
        let CreatePage::Saved(id) = store
            .create_page(CreatePageInput {
                tenant_id: tenant,
                space_id: space,
                section_id: section,
                account_id: owner,
                operation_id: Uuid::new_v4(),
                title,
            })
            .await?
        else {
            panic!("failed to create graph fixture page")
        };
        pages.push(id);
    }
    let [page, result] = pages.try_into().expect("two fixture pages");
    sqlx::query(
        "UPDATE page_revisions SET recorded_at=CASE WHEN page_id=$1
                 THEN to_timestamp(1000) ELSE to_timestamp(3000) END
                 WHERE (tenant_id,space_id)=($2,$3)",
    )
    .bind(page)
    .bind(tenant)
    .bind(space)
    .execute(&pool)
    .await?;
    let title = "Второе название";
    sqlx::query("UPDATE pages SET content_revision=2,title=$2,title_key=$2 WHERE id=$1")
        .bind(page)
        .bind(title.as_bytes())
        .execute(&pool)
        .await?;
    sqlx::query(
        "INSERT INTO page_revisions (tenant_id,space_id,page_id,revision,title,title_key,
                 markdown,content_is_markdown,author_account_id,author_name,recorded_at)
                 VALUES ($1,$2,$3,2,$4,$4,'',true,$5,'Редактор',to_timestamp(2000))",
    )
    .bind(tenant)
    .bind(space)
    .bind(page)
    .bind(title.as_bytes())
    .bind(owner)
    .execute(&pool)
    .await?;
    let TaskWrite::Saved(task) = store
        .create_task(
            TaskScope {
                tenant,
                space,
                account: owner,
            },
            &CreateTask {
                operation: Uuid::new_v4(),
                title: "Подготовить спецификацию",
                status: TaskStatus::Open,
                source: Some(page),
            },
        )
        .await?
    else {
        panic!("failed to create graph fixture task");
    };
    sqlx::query("UPDATE workspace_tasks SET created_at=1500000 WHERE id=$1")
        .bind(task)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO workspace_approach_runs (tenant_id,space_id,id,task_id,actor_id,
                source_page_id,source_revision,spec_version,task_version,status,output,result_page_id,started_at,finished_at)
                VALUES ($1,$2,$3,$4,$5,$6,2,1,1,'accepted','Текст',$7,2200000,2400000)")
        .bind(tenant).bind(space).bind(Uuid::new_v4()).bind(task).bind(owner)
        .bind(page).bind(result).execute(&pool).await?;

    let route = GraphRoute::parse(&format!("{}/graph", space_path(tenant, space))).unwrap();
    let as_owner = graph_data(&store, route, owner).await?;
    let document = find_node(&as_owner, &page.to_string());
    assert_eq!(document["name"], "Второе название");
    assert_eq!(document["at"], 2000000);
    let first = find_node(&as_owner, &format!("{page}-r1"));
    assert_eq!(first["name"], "Первое название");
    assert_eq!(first["at"], 1000000);
    assert_eq!(first["document"], page.to_string());
    assert!(first["href"].as_str().unwrap().ends_with("/revisions/1"));
    assert!(as_owner["edges"].as_array().unwrap().iter().any(|edge| {
        edge["from"] == task.to_string()
            && edge["to"] == result.to_string()
            && edge["at"] == 3000000
    }));

    let as_reader = graph_data(&store, route, reader).await?;
    let reader_document = find_node(&as_reader, &page.to_string());
    assert_eq!(reader_document["at"], 2000000);
    assert!(!as_reader.to_string().contains("Первое название"));
    assert!(!as_reader.to_string().contains("/revisions/1"));
    assert!(
        as_reader["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|edge| edge["from"] == task.to_string() && edge["to"] == result.to_string())
    );
    sqlx::query("DELETE FROM space_grants WHERE (tenant_id,space_id,account_id)=($1,$2,$3)")
        .bind(tenant)
        .bind(space)
        .bind(reader)
        .execute(&pool)
        .await?;
    assert_eq!(
        graph_response(&store, route, reader).await?.status(),
        StatusCode::NOT_FOUND
    );
    Ok(())
}

async fn graph_response(
    store: &Store,
    route: GraphRoute,
    id: Uuid,
) -> Result<Response, sqlx::Error> {
    handle(
        store,
        &Principal {
            id,
            name: "Участник".to_owned(),
        },
        route,
        Request::new(Body::empty()),
    )
    .await
}

async fn graph_data(store: &Store, route: GraphRoute, id: Uuid) -> anyhow::Result<Value> {
    let response = graph_response(store, route, id).await?;
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = body::to_bytes(response.into_body(), 1 << 20).await?;
    let html = std::str::from_utf8(&bytes)?;
    let (_, tail) = html.split_once(r#"id="workspace-graph-data">"#).unwrap();
    Ok(serde_json::from_str(
        tail.split_once("</script>").unwrap().0,
    )?)
}

fn find_node<'a>(model: &'a Value, key: &str) -> &'a Value {
    model["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["key"] == key)
        .unwrap()
}
