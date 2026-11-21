use super::*;
use crate::db::tasks::{AssigneeChange, CreateTask, TaskChange, TaskStatus, TaskWrite};
use sqlx::PgPool;

#[sqlx::test(migrator = "crate::db::MIGRATIONS")]
#[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
async fn launch_checks_context_and_allows_one_review_at_a_time(pool: PgPool) -> anyhow::Result<()> {
    let [tenant, space, account, section, source, other] = std::array::from_fn(|_| Uuid::new_v4());
    sqlx::query("INSERT INTO tenants VALUES ($1,'Команда')")
        .bind(tenant)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO accounts (id,email,name,salt,password_hash) VALUES ($1,'owner@example.test','Владелец',$2,$3)")
        .bind(account).bind(vec![0_u8; 16]).bind(vec![0_u8; 32]).execute(&pool).await?;
    sqlx::query("INSERT INTO tenant_memberships VALUES ($1,$2,'member','active',false,false)")
        .bind(tenant)
        .bind(account)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO spaces (tenant_id,id,creator_account_id,owner_account_id,name,name_key) VALUES ($1,$2,$3,$3,'Подходы','approaches')")
        .bind(tenant).bind(space).bind(account).execute(&pool).await?;
    sqlx::query("INSERT INTO sections (tenant_id,space_id,id,creator_account_id,title,title_key,position) VALUES ($1,$2,$3,$4,'Раздел','section',1)")
        .bind(tenant).bind(space).bind(section).bind(account).execute(&pool).await?;
    sqlx::query("INSERT INTO pages (tenant_id,space_id,section_id,id,creator_account_id,title,title_key,position,markdown) VALUES
        ($1,$2,$3,$4,$6,'Источник','source',1,repeat('я',2300)), ($1,$2,$3,$5,$6,'Другой','other',2,'Другой текст')")
        .bind(tenant).bind(space).bind(section).bind(source).bind(other).bind(account).execute(&pool).await?;

    let store = Store::from_test_pool(pool.clone());
    let scope = TaskScope {
        tenant,
        space,
        account,
    };
    let mut create = CreateTask {
        operation: Uuid::new_v4(),
        title: "Уточнить критерии",
        status: TaskStatus::Open,
        source: Some(other),
    };
    let TaskWrite::Saved(wrong_task) = store.create_task(scope, &create).await? else {
        panic!("expected the task with a different source");
    };
    create.operation = Uuid::new_v4();
    create.source = Some(source);
    let TaskWrite::Saved(task) = store.create_task(scope, &create).await? else {
        panic!("expected the task with the configured source");
    };
    assert!(
        store
            .set_approach(scope, 0, "Проверь критерии", source, true)
            .await?
    );
    let config = store.approach(scope).await?.unwrap();
    assert_eq!(
        config.source_excerpt,
        "я".repeat(SOURCE_EXCERPT_LIMIT as usize)
    );
    let mut versions = ApproachVersions {
        spec: config.version,
        source: config.source_revision,
        task: 1,
    };
    assert!(matches!(
        store
            .start_approach(scope, wrong_task, Uuid::new_v4(), versions)
            .await?,
        StartApproach::SourceRequired
    ));
    assert!(matches!(
        store
            .start_approach(
                scope,
                task,
                Uuid::new_v4(),
                ApproachVersions {
                    source: versions.source + 1,
                    ..versions
                }
            )
            .await?,
        StartApproach::PreviewChanged
    ));

    let abandoned = Uuid::new_v4();
    let StartApproach::Started(input) = store
        .start_approach(scope, task, abandoned, versions)
        .await?
    else {
        panic!("expected a launch for the authorized source");
    };
    assert_eq!(input.source, config.source_excerpt);
    assert_eq!(input.instruction, config.instruction);
    assert_eq!(input.task, create.title);
    assert!(matches!(
        store
            .start_approach(scope, task, abandoned, versions)
            .await?,
        StartApproach::AlreadyStarted
    ));
    sqlx::query("UPDATE workspace_approach_runs SET started_at=started_at-120001 WHERE id=$1")
        .bind(abandoned)
        .execute(&pool)
        .await?;
    assert_eq!(store.approach_runs(scope, task).await?[0].status, "expired");

    let (first, second) = (Uuid::new_v4(), Uuid::new_v4());
    let (a, b) = tokio::try_join!(
        store.start_approach(scope, task, first, versions),
        store.start_approach(scope, task, second, versions)
    )?;
    let run = match (a, b) {
        (StartApproach::Started(_), StartApproach::ActiveRun) => first,
        (StartApproach::ActiveRun, StartApproach::Started(_)) => second,
        _ => panic!("exactly one concurrent request must start"),
    };
    store
        .finish_approach(scope, abandoned, Some("Просроченный черновик".into()))
        .await?;
    store
        .finish_approach(scope, run, Some("Готовый черновик".into()))
        .await?;
    versions.task = 2;
    assert!(matches!(
        store
            .start_approach(scope, task, Uuid::new_v4(), versions)
            .await?,
        StartApproach::ActiveRun
    ));
    let task_state = TaskChange {
        version: 2,
        status: TaskStatus::Done,
        assignee: AssigneeChange::Keep,
    };
    assert_eq!(
        store.change_task(scope, task, task_state).await?,
        TaskWrite::Saved(task)
    );
    assert!(!store.review_approach(scope, task, run, false).await?);
    assert!(!store.review_approach(scope, task, run, true).await?);
    assert_eq!(
        store
            .task(tenant, space, account, task)
            .await?
            .unwrap()
            .task
            .status,
        TaskStatus::Done
    );
    assert!(
        store
            .approach_runs(scope, task)
            .await?
            .iter()
            .any(|entry| entry.id == run && entry.status == "superseded" && entry.output.is_none())
    );
    // The board can reopen the task, but only a fresh generation can be approved.
    assert_eq!(
        store
            .change_task(
                scope,
                task,
                TaskChange {
                    version: 3,
                    status: TaskStatus::Work,
                    ..task_state
                }
            )
            .await?,
        TaskWrite::Saved(task)
    );
    assert!(!store.review_approach(scope, task, run, true).await?);
    let retry = Uuid::new_v4();
    assert!(matches!(
        store
            .start_approach(
                scope,
                task,
                retry,
                ApproachVersions {
                    task: 4,
                    ..versions
                }
            )
            .await?,
        StartApproach::Started(_)
    ));
    store
        .finish_approach(
            scope,
            retry,
            Some("# Спецификация\n\nКритерии приёмки".into()),
        )
        .await?;
    assert!(store.review_approach(scope, task, retry, true).await?);
    assert!(!store.review_approach(scope, task, retry, true).await?);
    let readback = store.approach_runs(scope, task).await?;
    assert!(readback.iter().any(|entry| entry.id == retry
        && entry.status == "accepted"
        && entry.result_page_id == Some(retry)));
    Ok(())
}
