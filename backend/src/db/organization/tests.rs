use sqlx::PgPool;

use super::{MemberChange, Recipient, Store};
use crate::db::PageAddress;

#[sqlx::test(migrator = "crate::db::MIGRATIONS")]
#[ignore = "requires isolated PostgreSQL"]
async fn effective_roles_and_membership_changes_are_tenant_bound(
    pool: PgPool,
) -> Result<(), sqlx::Error> {
    let [
        tenant,
        other,
        owner,
        member,
        outsider,
        space,
        section,
        page,
        group,
    ] = [(); 9].map(|_| uuid::Uuid::new_v4());
    let store = Store::from_test_pool(pool.clone());
    sqlx::query("INSERT INTO tenants VALUES ($1,'Организация'),($2,'Другая')")
        .bind(tenant)
        .bind(other)
        .execute(&pool)
        .await?;
    for (id, email) in [
        (owner, "owner@org.test"),
        (member, "member@org.test"),
        (outsider, "other@org.test"),
    ] {
        sqlx::query("INSERT INTO accounts VALUES ($1,$2,'Проверка',$3,$4)")
            .bind(id)
            .bind(email)
            .bind([0_u8; 16].as_slice())
            .bind([0_u8; 32].as_slice())
            .execute(&pool)
            .await?;
    }
    sqlx::query(
        "INSERT INTO tenant_memberships VALUES
         ($1,$3,'superadmin','active',true,true),($1,$4,'member','active',false,true),
         ($2,$5,'admin','active',true,true)",
    )
    .bind(tenant)
    .bind(other)
    .bind(owner)
    .bind(member)
    .bind(outsider)
    .execute(&pool)
    .await?;
    sqlx::query("INSERT INTO spaces VALUES ($1,$2,$3,$3,'space'::bytea,'space'::bytea)")
        .bind(tenant)
        .bind(space)
        .bind(owner)
        .execute(&pool)
        .await?;
    sqlx::query(
        "INSERT INTO sections (tenant_id,space_id,id,creator_account_id,title,title_key,position)
         VALUES ($1,$2,$3,$4,'section'::bytea,'section'::bytea,1)",
    )
    .bind(tenant)
    .bind(space)
    .bind(section)
    .bind(owner)
    .execute(&pool)
    .await?;
    sqlx::query("INSERT INTO pages (tenant_id,space_id,section_id,id,creator_account_id,title,title_key,position)
         VALUES ($1,$2,$3,$4,$5,'page'::bytea,'page'::bytea,1)")
        .bind(tenant).bind(space).bind(section).bind(page).bind(owner).execute(&pool).await?;
    let address = PageAddress {
        tenant,
        space,
        account: member,
        page,
    };
    assert!(!store.page_permissions(&address).await?.unwrap().can_read);
    assert!(
        !store
            .change_organization_member(tenant, outsider, member, MemberChange::CanCreate(true))
            .await?
    );
    assert!(
        store
            .change_organization_member(tenant, owner, member, MemberChange::CanCreate(true))
            .await?
    );
    assert!(
        store
            .workspace(member, Some(tenant))
            .await?
            .0
            .unwrap()
            .tenant
            .can_create
    );
    assert!(
        store
            .change_organization_member(tenant, owner, member, MemberChange::Department("Склад"))
            .await?
    );
    assert!(
        store
            .change_space_grant(
                tenant,
                space,
                owner,
                Recipient::Department("Склад"),
                Some("viewer")
            )
            .await?
    );
    let rights = store.page_permissions(&address).await?.unwrap();
    assert!(rights.can_read && !rights.can_edit);

    sqlx::query("INSERT INTO organization_groups VALUES($1,$2,'Группа')")
        .bind(tenant)
        .bind(group)
        .execute(&pool)
        .await?;
    assert!(
        store
            .change_organization_group_member(tenant, owner, group, member, true)
            .await?
    );
    assert!(
        !store
            .change_space_grant(
                tenant,
                space,
                outsider,
                Recipient::Group(group),
                Some("editor")
            )
            .await?
    );
    assert!(
        store
            .change_space_grant(
                tenant,
                space,
                owner,
                Recipient::Group(group),
                Some("editor")
            )
            .await?
    );
    assert!(store.page_permissions(&address).await?.unwrap().can_edit);
    let grants = store
        .organization_space_grants(tenant, Some(space), "member@org.test", "editor", "group", 0)
        .await?;
    assert_eq!(grants.len(), 1);
    assert!(
        grants[0]
            .origins
            .iter()
            .any(|origin| origin == "department")
    );

    assert!(
        store
            .change_organization_group_member(tenant, owner, group, member, false)
            .await?
    );
    let rights = store.page_permissions(&address).await?.unwrap();
    assert!(rights.can_read && !rights.can_edit);
    assert!(!store.switch_organization(member, other).await?);
    assert_eq!(
        store.workspace(member, None).await?.0.unwrap().tenant.id,
        tenant
    );
    Ok(())
}

#[sqlx::test(migrator = "crate::db::MIGRATIONS")]
#[ignore = "requires isolated PostgreSQL"]
async fn invitation_creates_one_membership_and_revocation_prevents_entry(
    pool: PgPool,
) -> Result<(), sqlx::Error> {
    let [tenant, owner, invitee, outsider] = [(); 4].map(|_| uuid::Uuid::new_v4());
    let store = Store::from_test_pool(pool.clone());
    sqlx::query("INSERT INTO tenants(id,name) VALUES ($1,'Организация')")
        .bind(tenant)
        .execute(&pool)
        .await?;
    for (id, email) in [(owner, "owner@org.test"), (outsider, "outsider@org.test")] {
        sqlx::query("INSERT INTO accounts VALUES ($1,$2,'Проверка',$3,$4)")
            .bind(id)
            .bind(email)
            .bind([0_u8; 16].as_slice())
            .bind([0_u8; 32].as_slice())
            .execute(&pool)
            .await?;
    }
    sqlx::query("INSERT INTO tenant_memberships VALUES ($1,$2,'superadmin','active',true,true)")
        .bind(tenant)
        .bind(owner)
        .execute(&pool)
        .await?;

    let email = "invitee@org.test";
    let now = crate::clock::now_ms();
    let first = [7_u8; 32];
    let revoked = [9_u8; 32];
    assert!(
        store
            .issue_organization_invitation(tenant, outsider, email, &first, now)
            .await?
            .is_none()
    );
    let id = store
        .issue_organization_invitation(tenant, owner, email, &revoked, now)
        .await?
        .unwrap();
    assert!(
        store
            .revoke_organization_invitation(tenant, owner, id, now)
            .await?
    );
    assert!(
        store
            .organization_invitation_access(&revoked, now, None)
            .await?
            .is_none()
    );
    let mail = store
        .issue_organization_invitation(tenant, owner, email, &first, now)
        .await?
        .unwrap();
    store
        .finish_organization_invitation_delivery(tenant, mail, true, now)
        .await?;
    assert!(
        store
            .issue_organization_invitation(tenant, owner, email, &revoked, now)
            .await?
            .is_none()
    );
    let invitation = store
        .organization_invitation_access(&first, now, None)
        .await?
        .unwrap();
    assert_eq!(invitation.tenant_id, tenant);
    assert!(!invitation.account_exists);
    assert!(
        !store
            .join_organization_invitation(tenant, &first, now, outsider, None)
            .await?
    );
    let account = crate::db::NewAccount {
        id: invitee,
        email: email.to_owned(),
        name: "Участник".to_owned(),
        salt: vec![0; 16],
        password_hash: vec![0; 32],
    };
    let session = crate::db::NewSession {
        digest: vec![5; 32],
        created_at: now,
        expires_at: now + 1000,
    };
    assert!(
        store
            .join_organization_invitation(tenant, &first, now, invitee, Some((&account, &session)))
            .await?
    );
    assert!(
        store
            .join_organization_invitation(tenant, &first, now, invitee, None)
            .await?
    );
    assert!(
        store
            .organization_invitation_access(&first, now, None)
            .await?
            .is_none()
    );
    assert!(
        !store
            .revoke_organization_invitation(tenant, owner, mail, now)
            .await?
    );
    let workspace = store.workspace(invitee, None).await?.0.unwrap();
    assert_eq!(workspace.tenant.role, "member");
    assert!(!workspace.tenant.can_create);
    Ok(())
}
