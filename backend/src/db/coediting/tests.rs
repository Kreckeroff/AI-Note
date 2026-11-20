use sqlx::PgPool;

use super::*;

struct Fixture {
    store: Store,
    owner: PageAddress,
    guest: PageAddress,
    reader: PageAddress,
}

const OWNER_SESSION: [u8; 32] = [1; 32];
const GUEST_SESSION: [u8; 32] = [2; 32];
const READER_SESSION: [u8; 32] = [3; 32];

impl Fixture {
    async fn seed(pool: &PgPool) -> anyhow::Result<Self> {
        let [tenant, space, section, page, owner, guest, reader] =
            std::array::from_fn(|_| Uuid::new_v4());
        sqlx::query("INSERT INTO tenants VALUES ($1,'Coedit test')")
            .bind(tenant)
            .execute(pool)
            .await?;
        for (account, name, session) in [
            (owner, "Owner", OWNER_SESSION),
            (guest, "Guest", GUEST_SESSION),
            (reader, "Reader", READER_SESSION),
        ] {
            sqlx::query("INSERT INTO accounts VALUES ($1,$2,$3,$4,$5)")
                .bind(account)
                .bind(format!("{account}@example.test"))
                .bind(name)
                .bind(vec![0_u8; 16])
                .bind(vec![0_u8; 32])
                .execute(pool)
                .await?;
            sqlx::query("INSERT INTO sessions VALUES ($1,$2,0,$3)")
                .bind(&session[..])
                .bind(account)
                .bind(crate::clock::now_ms() + 60_000)
                .execute(pool)
                .await?;
        }
        sqlx::query(
            "INSERT INTO tenant_memberships VALUES
                ($1,$2,'member','active',false,false),
                ($1,$3,'admin','active',false,false)",
        )
        .bind(tenant)
        .bind(owner)
        .bind(reader)
        .execute(pool)
        .await?;
        sqlx::query("INSERT INTO spaces VALUES ($1,$2,$3,$3,$4,$4)")
            .bind(tenant)
            .bind(space)
            .bind(owner)
            .bind(b"space".as_slice())
            .execute(pool)
            .await?;
        sqlx::query("INSERT INTO sections VALUES ($1,$2,$3,$4,$5,$5,1)")
            .bind(tenant)
            .bind(space)
            .bind(section)
            .bind(owner)
            .bind(b"section".as_slice())
            .execute(pool)
            .await?;
        sqlx::query(
            "INSERT INTO pages
               (tenant_id,space_id,section_id,id,creator_account_id,title,title_key,position,markdown,content_is_markdown)
             VALUES ($1,$2,$3,$4,$5,$6,$6,1,'abc123xyz',true)",
        )
        .bind(tenant).bind(space).bind(section).bind(page).bind(owner).bind(b"page".as_slice())
        .execute(pool).await?;
        sqlx::query("INSERT INTO tenant_sharing_policies VALUES ($1,true,false)")
            .bind(tenant)
            .execute(pool)
            .await?;
        sqlx::query("INSERT INTO page_grants VALUES ($1,$2,$3,$4,'edit',true)")
            .bind(tenant)
            .bind(space)
            .bind(page)
            .bind(guest)
            .execute(pool)
            .await?;
        let address = |account| PageAddress {
            tenant,
            space,
            page,
            account,
        };
        Ok(Self {
            store: Store::from_test_pool(pool.clone()),
            owner: address(owner),
            guest: address(guest),
            reader: address(reader),
        })
    }
}

fn saved(result: CoeditResult<DraftSnapshot>) -> DraftSnapshot {
    match result {
        CoeditResult::Saved(snapshot) => snapshot,
        result => panic!("expected confirmation, got {result:?}"),
    }
}

fn command(snapshot: &DraftSnapshot, start: usize, end: usize, text: &str) -> DraftCommand {
    DraftCommand {
        lease: snapshot.lease,
        operation_id: Uuid::new_v4(),
        version: snapshot.version,
        start,
        end,
        text: text.to_owned(),
        format: snapshot.format,
        resolve: false,
    }
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn independent_editors_converge_and_idempotently_replay(pool: PgPool) -> anyhow::Result<()> {
    let f = Fixture::seed(&pool).await?;
    let owner = saved(f.store.open_coedit(&f.owner, &OWNER_SESSION).await?);
    let guest = saved(f.store.open_coedit(&f.guest, &GUEST_SESSION).await?);
    assert_eq!(owner.version, 0);
    assert_eq!(owner.source, guest.source);
    assert_ne!(owner.lease, guest.lease);
    assert!(matches!(
        f.store.open_coedit(&f.reader, &READER_SESSION).await?,
        CoeditResult::Forbidden
    ));
    assert!(matches!(
        f.store
            .coedit_state(&f.guest, &GUEST_SESSION, owner.lease)
            .await?,
        CoeditResult::LeaseExpired
    ));

    let mut a = command(&owner, 3, 3, "🐈");
    let a_id = a.operation_id;
    let b = command(&guest, 3, 3, "Ж");
    let (result_a, result_b) = tokio::join!(
        f.store.apply_coedit(&f.owner, &OWNER_SESSION, a),
        f.store.apply_coedit(&f.guest, &GUEST_SESSION, b)
    );
    let a_result = saved(result_a?);
    let b_result = saved(result_b?);
    assert_ne!(a_result.version, b_result.version);
    let (latest, order) = if a_result.version > b_result.version {
        (&a_result, "Ж🐈")
    } else {
        (&b_result, "🐈Ж")
    };
    assert_eq!(latest.source, format!("abc{order}123xyz"));
    let owner_view = saved(
        f.store
            .coedit_state(&f.owner, &OWNER_SESSION, owner.lease)
            .await?,
    );
    assert_eq!(owner_view.source, latest.source);
    assert_eq!(owner_view.version, 2);
    assert_eq!(
        f.store.page_text(&f.reader).await?.unwrap().source,
        "abc123xyz"
    );

    a = command(&owner, 3, 3, "🐈");
    a.operation_id = a_id;
    let replayed = saved(f.store.apply_coedit(&f.owner, &OWNER_SESSION, a).await?);
    assert_eq!(
        (replayed.version, replayed.source),
        (a_result.version, a_result.source)
    );
    assert_eq!(
        replayed
            .applied
            .map(|value| (value.start, value.end, value.text)),
        a_result
            .applied
            .map(|value| (value.start, value.end, value.text))
    );
    let mut mismatch = command(&owner, 3, 3, "wrong");
    mismatch.operation_id = a_id;
    assert!(matches!(
        f.store
            .apply_coedit(&f.owner, &OWNER_SESSION, mismatch)
            .await?,
        CoeditResult::OperationConflict
    ));
    let unpublished = format!("![local](attachment:{})", Uuid::new_v4());
    assert!(matches!(
        f.store
            .apply_coedit(
                &f.owner,
                &OWNER_SESSION,
                command(&owner_view, 0, 0, &unpublished)
            )
            .await?,
        CoeditResult::Invalid
    ));
    let after = saved(
        f.store
            .coedit_state(&f.owner, &OWNER_SESSION, owner.lease)
            .await?,
    );
    assert_eq!(after.version, 2);
    Ok(())
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn old_lease_fails_even_when_the_right_has_returned(pool: PgPool) -> anyhow::Result<()> {
    let f = Fixture::seed(&pool).await?;
    let guest = saved(f.store.open_coedit(&f.guest, &GUEST_SESSION).await?);
    let change_role = |role: &'static str| {
        sqlx::query(
            "UPDATE page_grants SET role=$5 WHERE (tenant_id,space_id,page_id,account_id)=($1,$2,$3,$4)",
        )
        .bind(f.guest.tenant).bind(f.guest.space).bind(f.guest.page).bind(f.guest.account)
        .bind(role).execute(&pool)
    };
    change_role("comment").await?;
    assert!(matches!(
        f.store
            .coedit_state(&f.guest, &GUEST_SESSION, guest.lease)
            .await?,
        CoeditResult::Forbidden
    ));
    change_role("edit").await?;
    assert!(matches!(
        f.store
            .apply_coedit(&f.guest, &GUEST_SESSION, command(&guest, 0, 0, "late"))
            .await?,
        CoeditResult::LeaseExpired
    ));
    let fresh = saved(f.store.open_coedit(&f.guest, &GUEST_SESSION).await?);
    assert_ne!(guest.lease, fresh.lease);
    assert_eq!(fresh.source, guest.source);
    let value = saved(
        f.store
            .apply_coedit(&f.guest, &GUEST_SESSION, command(&fresh, 0, 0, "new"))
            .await?,
    );
    assert_eq!(value.source, "newabc123xyz");
    Ok(())
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn restoration_keeps_overlapping_draft_for_explicit_resolution(
    pool: PgPool,
) -> anyhow::Result<()> {
    let f = Fixture::seed(&pool).await?;
    let first = saved(f.store.open_coedit(&f.owner, &OWNER_SESSION).await?);
    let edited = saved(
        f.store
            .apply_coedit(&f.owner, &OWNER_SESSION, command(&first, 3, 5, "Q"))
            .await?,
    );
    let mut tx = pool.begin().await?;
    assert!(
        lock_page_for_change(&mut tx, &f.owner)
            .await?
            .unwrap()
            .can_edit
    );
    record_publication(
        &mut tx,
        &f.owner,
        "abc1Rxyz",
        PageContentFormat::Markdown,
        2,
        false,
    )
    .await?;
    tx.commit().await?;
    let restored = f
        .store
        .coedit_state(&f.owner, &OWNER_SESSION, first.lease)
        .await?;
    let CoeditResult::Conflict(conflict) = restored else {
        anyhow::bail!("missing restoration conflict");
    };
    assert!(conflict.persistent);
    assert_eq!(conflict.snapshot.version, edited.version + 1);
    assert_eq!(conflict.snapshot.published_revision, 2);
    assert_eq!(conflict.snapshot.source, "abcQ3xyz");
    assert_eq!(
        (conflict.mine.as_str(), conflict.theirs.as_str()),
        ("1R", "Q3")
    );
    let overlapping = command(&conflict.snapshot, conflict.start, conflict.end, "blocked");
    let overlapping_id = overlapping.operation_id;
    let CoeditResult::Conflict(rejected) = f
        .store
        .apply_coedit(&f.owner, &OWNER_SESSION, overlapping)
        .await?
    else {
        anyhow::bail!("ordinary edit cannot replace the unresolved range");
    };
    assert!(rejected.persistent);
    assert!(rejected.snapshot.acknowledged_version.is_none());
    assert_eq!(rejected.snapshot.source, conflict.snapshot.source);
    let mut rejected_tx = pool.begin().await?;
    let receipt = load_receipt(&mut rejected_tx, &f.owner, overlapping_id)
        .await?
        .unwrap();
    assert_eq!(receipt.outcome, "conflict");
    rejected_tx.commit().await?;

    let mut same_overlap = command(&conflict.snapshot, conflict.start, conflict.end, "blocked");
    same_overlap.operation_id = overlapping_id;
    let CoeditResult::Conflict(repeated) = f
        .store
        .apply_coedit(&f.owner, &OWNER_SESSION, same_overlap)
        .await?
    else {
        anyhow::bail!("the rejected command must keep the persistent marker on retry");
    };
    assert!(repeated.persistent);

    let CoeditResult::Conflict(new_editor) = f.store.open_coedit(&f.guest, &GUEST_SESSION).await?
    else {
        anyhow::bail!("new editors see the unresolved fragments");
    };
    let mut disjoint = command(&new_editor.snapshot, 0, 0, "👋");
    let disjoint_id = disjoint.operation_id;
    let accepted = saved(
        f.store
            .apply_coedit(&f.guest, &GUEST_SESSION, disjoint)
            .await?,
    );
    assert_eq!(accepted.source, "👋abcQ3xyz");
    assert_eq!(
        accepted.acknowledged_version,
        Some(new_editor.snapshot.version + 1)
    );
    assert_eq!(
        accepted.steps.as_ref().unwrap().last(),
        accepted.applied.as_ref()
    );
    let active = accepted.restore_conflict.as_ref().unwrap();
    assert_eq!(
        (active.start, active.end),
        (conflict.start + 1, conflict.end + 1)
    );
    assert_eq!(
        (&active.mine, &active.theirs),
        (&conflict.mine, &conflict.theirs)
    );
    let wire = serde_json::to_value(&accepted)?;
    assert_eq!(wire["restoreConflict"]["theirs"], "Q3");

    disjoint = command(&new_editor.snapshot, 0, 0, "👋");
    disjoint.operation_id = disjoint_id;
    let replay = saved(
        f.store
            .apply_coedit(&f.guest, &GUEST_SESSION, disjoint)
            .await?,
    );
    assert_eq!(replay.version, accepted.version);
    assert_eq!(
        replay.restore_conflict.as_ref().unwrap().start,
        active.start
    );
    assert_eq!(replay.source, accepted.source);

    let CoeditResult::Conflict(shifted) = f
        .store
        .coedit_state_after(
            &f.owner,
            &OWNER_SESSION,
            first.lease,
            Some(conflict.snapshot.version),
        )
        .await?
    else {
        anyhow::bail!("the other editor still sees the conflict");
    };
    assert!(shifted.persistent);
    assert_eq!((shifted.start, shifted.end), (active.start, active.end));
    assert_eq!(shifted.snapshot.steps, accepted.steps);
    let mut resolve = command(&shifted.snapshot, shifted.start, shifted.end, "Q+R");
    resolve.resolve = true;
    let resolved = saved(
        f.store
            .apply_coedit(&f.owner, &OWNER_SESSION, resolve)
            .await?,
    );
    assert_eq!(resolved.source, "👋abcQ+Rxyz");
    assert!(resolved.restore_conflict.is_none());
    assert_eq!(resolved.published_revision, 2);
    let mut stale = command(
        &conflict.snapshot,
        conflict.start,
        conflict.end,
        "late overwrite",
    );
    stale.resolve = true;
    let CoeditResult::BaseExpired(confirmation) = f
        .store
        .apply_coedit(&f.owner, &OWNER_SESSION, stale)
        .await?
    else {
        anyhow::bail!("stale resolution should require explicit review");
    };
    assert_eq!(confirmation.source, resolved.source);
    Ok(())
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn confirmations_and_polls_carry_exact_ordered_splices(pool: PgPool) -> anyhow::Result<()> {
    let f = Fixture::seed(&pool).await?;
    let own_start = saved(f.store.open_coedit(&f.owner, &OWNER_SESSION).await?);
    let peer_start = saved(f.store.open_coedit(&f.guest, &GUEST_SESSION).await?);
    let mut noop = command(&own_start, 0, 0, "");
    let noop_id = noop.operation_id;
    let unchanged = saved(f.store.apply_coedit(&f.owner, &OWNER_SESSION, noop).await?);
    assert_eq!(unchanged.acknowledged_version, Some(0));
    assert_eq!(unchanged.steps.as_deref(), Some([].as_slice()));
    assert!(unchanged.applied.is_none());
    let peer_start_changed = saved(
        f.store
            .apply_coedit(&f.guest, &GUEST_SESSION, command(&peer_start, 0, 0, "A"))
            .await?,
    );
    let tail = peer_start_changed.source.chars().count();
    let peer_end_changed = saved(
        f.store
            .apply_coedit(
                &f.guest,
                &GUEST_SESSION,
                command(&peer_start_changed, tail, tail, "E"),
            )
            .await?,
    );

    let mut local = command(&own_start, 3, 6, "M");
    let local_id = local.operation_id;
    let accepted = saved(
        f.store
            .apply_coedit(&f.owner, &OWNER_SESSION, local)
            .await?,
    );
    assert_eq!(accepted.source, "AabcMxyzE");
    assert_eq!(accepted.from_version, Some(own_start.version));
    assert_eq!(accepted.acknowledged_version, Some(accepted.version));
    let steps = accepted.steps.as_ref().expect("exact accepted steps");
    assert_eq!(steps.len(), 3);
    let positions: Vec<_> = steps
        .iter()
        .map(|step| (step.start, step.end, step.text.as_str()))
        .collect();
    assert_eq!(positions, [(0, 0, "A"), (10, 10, "E"), (4, 7, "M")]);
    assert_eq!(accepted.applied.as_ref(), steps.last());
    let wire = serde_json::to_value(&accepted)?;
    assert_eq!(wire["fromVersion"], "0");
    assert_eq!(wire["acknowledgedVersion"], "3");
    assert_eq!(wire["steps"][2], wire["applied"]);
    let poll = saved(
        f.store
            .coedit_state_after(
                &f.owner,
                &OWNER_SESSION,
                own_start.lease,
                Some(peer_end_changed.version),
            )
            .await?,
    );
    assert_eq!(poll.from_version, Some(peer_end_changed.version));
    assert_eq!(poll.steps.as_deref(), Some(&steps[2..]));
    assert!(poll.acknowledged_version.is_none());

    let overlap = f
        .store
        .apply_coedit(
            &f.owner,
            &OWNER_SESSION,
            command(&own_start, 4, 6, "overlap"),
        )
        .await?;
    let CoeditResult::Conflict(overlap) = overlap else {
        anyhow::bail!("expected overlap with the changed middle");
    };
    assert!(!overlap.persistent);
    assert_eq!(overlap.snapshot.steps.as_ref(), Some(steps));
    assert!(overlap.snapshot.acknowledged_version.is_none());

    let later = saved(
        f.store
            .apply_coedit(&f.owner, &OWNER_SESSION, command(&accepted, 0, 0, "!"))
            .await?,
    );
    local = command(&own_start, 3, 6, "M");
    local.operation_id = local_id;
    let replay = saved(
        f.store
            .apply_coedit(&f.owner, &OWNER_SESSION, local)
            .await?,
    );
    assert_eq!(replay.acknowledged_version, Some(accepted.version));
    assert_eq!(replay.source, accepted.source);
    assert_eq!(replay.steps.as_ref(), Some(steps));

    // Model an expired journal window without synthesizing hundreds of edits.
    sqlx::query(
        "DELETE FROM page_coedit_operations
         WHERE (tenant_id,space_id,page_id)=($1,$2,$3) AND version<=2",
    )
    .bind(f.owner.tenant)
    .bind(f.owner.space)
    .bind(f.owner.page)
    .execute(&pool)
    .await?;
    local = command(&own_start, 3, 6, "M");
    local.operation_id = local_id;
    let CoeditResult::BaseExpired(recovery) = f
        .store
        .apply_coedit(&f.owner, &OWNER_SESSION, local)
        .await?
    else {
        anyhow::bail!("the old receipt must acknowledge without reapplying");
    };
    assert_eq!(recovery.acknowledged_version, Some(accepted.version));
    assert_eq!(recovery.version, later.version);
    assert_eq!(recovery.source, "!AabcMxyzE");
    assert!(recovery.steps.is_none());
    noop = command(&own_start, 0, 0, "");
    noop.operation_id = noop_id;
    let CoeditResult::BaseExpired(noop_recovery) =
        f.store.apply_coedit(&f.owner, &OWNER_SESSION, noop).await?
    else {
        anyhow::bail!("the old unchanged command must still acknowledge");
    };
    assert_eq!(noop_recovery.acknowledged_version, Some(0));
    assert_eq!(noop_recovery.source, recovery.source);
    let CoeditResult::BaseExpired(poll_recovery) = f
        .store
        .coedit_state_after(&f.owner, &OWNER_SESSION, own_start.lease, Some(0))
        .await?
    else {
        anyhow::bail!("the poll must signal missing history");
    };
    assert!(poll_recovery.acknowledged_version.is_none());
    assert_eq!(poll_recovery.source, recovery.source);
    Ok(())
}
