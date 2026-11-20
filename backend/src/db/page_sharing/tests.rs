use sqlx::PgPool;

use super::*;
use crate::db::{
    AttachmentLookup, AttachmentPatch, CoverUpdate, PageCover, PageUpdate, SavePageText,
    StageAttachment,
};

struct Fixture {
    tenant: Uuid,
    space: Uuid,
    page: Uuid,
    neighbor: Uuid,
    owner: Uuid,
    admin: Uuid,
    member: Uuid,
    guest: Uuid,
}

impl Fixture {
    async fn seed(pool: &PgPool) -> anyhow::Result<Self> {
        let [
            tenant,
            space,
            section,
            page,
            neighbor,
            owner,
            admin,
            member,
            guest,
        ] = std::array::from_fn(|_| Uuid::new_v4());
        sqlx::query("INSERT INTO tenants VALUES ($1, 'Первая организация')")
            .bind(tenant)
            .execute(pool)
            .await?;
        for (account, name, role) in [
            (owner, "Владелец", Some("member")),
            (admin, "Администратор", Some("admin")),
            (member, "Комментатор", Some("member")),
            (guest, "Гость", None),
        ] {
            sqlx::query("INSERT INTO accounts VALUES ($1,$2,$3,$4,$5)")
                .bind(account)
                .bind(format!("{account}@example.test"))
                .bind(name)
                .bind(vec![0_u8; 16])
                .bind(vec![0_u8; 32])
                .execute(pool)
                .await?;
            if let Some(role) = role {
                sqlx::query(
                    "INSERT INTO tenant_memberships VALUES ($1,$2,$3,'active',false,false)",
                )
                .bind(tenant)
                .bind(account)
                .bind(role)
                .execute(pool)
                .await?;
            }
        }
        let title = "Пространство".as_bytes();
        sqlx::query("INSERT INTO spaces VALUES ($1,$2,$3,$3,$4,$4)")
            .bind(tenant)
            .bind(space)
            .bind(owner)
            .bind(title)
            .execute(pool)
            .await?;
        sqlx::query("INSERT INTO sections VALUES ($1,$2,$3,$4,$5,$5,1)")
            .bind(tenant)
            .bind(space)
            .bind(section)
            .bind(owner)
            .bind("Раздел".as_bytes())
            .execute(pool)
            .await?;
        for (id, title, position) in [(page, "Документ", 1_i64), (neighbor, "Другая", 2)]
        {
            sqlx::query(
                "INSERT INTO pages (tenant_id,space_id,section_id,id,creator_account_id,title,title_key,position)
                 VALUES ($1,$2,$3,$4,$5,$6,$6,$7)",
            )
            .bind(tenant)
            .bind(space)
            .bind(section)
            .bind(id)
            .bind(owner)
            .bind(title.as_bytes())
            .bind(position)
            .execute(pool)
            .await?;
        }
        Ok(Self {
            tenant,
            space,
            page,
            neighbor,
            owner,
            admin,
            member,
            guest,
        })
    }

    fn address(&self, account: Uuid) -> PageAddress {
        PageAddress {
            tenant: self.tenant,
            space: self.space,
            page: self.page,
            account,
        }
    }

    fn grant(&self, account_id: Uuid, role: PageRole) -> PageSharingAction {
        PageSharingAction::Grant {
            account_id,
            role,
            operation_id: Uuid::new_v4(),
        }
    }
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
async fn grants_add_page_capabilities_without_granting_navigation(
    pool: PgPool,
) -> anyhow::Result<()> {
    let fixture = Fixture::seed(&pool).await?;
    let store = Store(pool.clone());
    let owner = fixture.address(fixture.owner);
    let admin = fixture.address(fixture.admin);
    let member = fixture.address(fixture.member);
    let guest = fixture.address(fixture.guest);
    let outsider = PageAddress {
        tenant: Uuid::new_v4(),
        ..owner
    };

    assert!(store.page_permissions(&owner).await?.unwrap().can_manage);
    let inherited = store.page_permissions(&admin).await?.unwrap();
    assert!(inherited.can_read && inherited.can_browse_space && !inherited.can_comment);
    assert!(store.page_permissions(&outsider).await?.is_none());
    assert!(store.page_text(&member).await?.is_none());

    let comment = fixture.grant(fixture.member, PageRole::Comment);
    let PageSharingOutcome::Saved(Some(person)) =
        store.change_page_sharing(&owner, &comment).await?
    else {
        anyhow::bail!("commenter missing from sharing");
    };
    assert_eq!(person.direct.as_deref(), Some("comment"));
    let caps = store.page_permissions(&member).await?.unwrap();
    assert!(caps.can_read && caps.can_comment && !caps.can_edit && !caps.can_browse_space);
    assert!(store.standalone_page(&member).await?.is_some());
    assert!(
        store
            .page_text(&PageAddress {
                page: fixture.neighbor,
                ..member
            })
            .await?
            .is_none()
    );
    assert!(store.page_history(&member, None).await?.is_none());
    assert!(matches!(
        store
            .save_page(&member, 1, PageUpdate::PlainText("сбой"), CoverUpdate::Keep)
            .await?,
        SavePageText::Forbidden
    ));

    // A lower page assignment does not downscope an inherited editor.
    sqlx::query("INSERT INTO space_grants VALUES ($1,$2,$3,'editor')")
        .bind(fixture.tenant)
        .bind(fixture.space)
        .bind(fixture.admin)
        .execute(&pool)
        .await?;
    let grant = fixture.grant(fixture.admin, PageRole::Read);
    let PageSharingOutcome::Saved(Some(person)) = store.change_page_sharing(&owner, &grant).await?
    else {
        anyhow::bail!("editor missing from sharing");
    };
    assert_eq!(person.effective.as_deref(), Some("edit"));
    let revoke = PageSharingAction::Revoke {
        account_id: fixture.admin,
        operation_id: Uuid::new_v4(),
    };
    let PageSharingOutcome::Saved(Some(person)) =
        store.change_page_sharing(&owner, &revoke).await?
    else {
        anyhow::bail!("inherited editor missing after revoke");
    };
    assert_eq!(person.inherited.as_deref(), Some("edit"));

    let guest_role = fixture.grant(fixture.guest, PageRole::Edit);
    assert!(matches!(
        store.change_page_sharing(&owner, &guest_role).await?,
        PageSharingOutcome::InvalidTarget
    ));
    sqlx::query("INSERT INTO tenant_sharing_policies VALUES ($1,true,false)")
        .bind(fixture.tenant)
        .execute(&pool)
        .await?;
    sqlx::query("INSERT INTO page_grants VALUES ($1,$2,$3,$4,'edit',true)")
        .bind(fixture.tenant)
        .bind(fixture.space)
        .bind(fixture.page)
        .bind(fixture.guest)
        .execute(&pool)
        .await?;
    assert_eq!(
        store.page_people(&owner, "гость", "direct", 0).await?.len(),
        1
    );
    let draft = Uuid::new_v4();
    let file = Uuid::new_v4();
    let bytes = b"guest attachment";
    assert!(matches!(
        store
            .stage_attachment(&guest, draft, file, "note.txt", None, bytes)
            .await?,
        StageAttachment::Saved(_)
    ));
    let patch = AttachmentPatch {
        draft,
        add: &[file],
        remove: &[],
    };
    assert!(matches!(
        store
            .save_page_with_attachments(
                &guest,
                1,
                PageUpdate::PlainText("Правка гостя"),
                CoverUpdate::Set(PageCover::Grid),
                Some(patch)
            )
            .await?,
        SavePageText::Saved
    ));
    assert_eq!(
        store
            .page_revision(&guest, 2)
            .await?
            .unwrap()
            .author
            .as_deref(),
        Some("Гость")
    );
    assert!(
        store
            .sections(fixture.tenant, fixture.space, fixture.guest)
            .await?
            .is_none()
    );
    let attachment = store
        .page_text(&guest)
        .await?
        .unwrap()
        .attachments
        .remove(0);
    let lookup = AttachmentLookup::Published(attachment.publication.unwrap());
    assert_eq!(
        store.attachment_body(&member, lookup).await?.unwrap().bytes,
        bytes
    );
    assert!(store.attachment_body(&outsider, lookup).await?.is_none());
    let downgrade = fixture.grant(fixture.guest, PageRole::Comment);
    assert!(matches!(
        store.change_page_sharing(&owner, &downgrade).await?,
        PageSharingOutcome::Saved(_)
    ));
    assert!(!store.page_permissions(&guest).await?.unwrap().can_edit);
    assert!(store.page_permissions(&guest).await?.unwrap().can_comment);
    sqlx::query("UPDATE tenant_sharing_policies SET allow_guests = false WHERE tenant_id = $1")
        .bind(fixture.tenant)
        .execute(&pool)
        .await?;
    assert!(store.page_text(&guest).await?.is_none());
    assert!(store.page_revision(&guest, 2).await?.is_none());
    assert!(store.attachment_body(&guest, lookup).await?.is_none());
    assert_eq!(
        store
            .page_revision(&owner, 2)
            .await?
            .unwrap()
            .author
            .as_deref(),
        Some("Гость")
    );
    Ok(())
}

#[sqlx::test]
#[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
async fn requests_and_revocations_stay_idempotent_across_later_changes(
    pool: PgPool,
) -> anyhow::Result<()> {
    let fixture = Fixture::seed(&pool).await?;
    let store = Store(pool);
    let owner = fixture.address(fixture.owner);
    let account = fixture.address(fixture.member);
    let missing = PageAddress {
        page: Uuid::new_v4(),
        ..account
    };
    let first = PageSharingAction::Request {
        operation_id: Uuid::new_v4(),
    };
    let repeated = PageSharingAction::Request {
        operation_id: Uuid::new_v4(),
    };

    let (a, b) = tokio::join!(
        store.change_page_sharing(&account, &first),
        store.change_page_sharing(&account, &repeated)
    );
    assert!(matches!(a?, PageSharingOutcome::Accepted));
    assert!(matches!(b?, PageSharingOutcome::Accepted));
    assert!(matches!(
        store.change_page_sharing(&missing, &first).await?,
        PageSharingOutcome::Accepted
    ));
    let requests = store.page_access_requests(&owner, 0).await?;
    assert_eq!(requests.len(), 1);
    let approve = PageSharingAction::Approve {
        request_id: requests[0].id,
        operation_id: Uuid::new_v4(),
    };
    let deny = PageSharingAction::Deny {
        request_id: requests[0].id,
        operation_id: Uuid::new_v4(),
    };
    assert!(matches!(
        store.change_page_sharing(&owner, &approve).await?,
        PageSharingOutcome::Saved(None)
    ));
    assert!(matches!(
        store.change_page_sharing(&owner, &deny).await?,
        PageSharingOutcome::InvalidTarget
    ));
    assert!(store.page_text(&account).await?.is_some());

    let revoke = PageSharingAction::Revoke {
        account_id: fixture.member,
        operation_id: Uuid::new_v4(),
    };
    assert!(matches!(
        store.change_page_sharing(&owner, &revoke).await?,
        PageSharingOutcome::Saved(_)
    ));
    assert!(store.page_text(&account).await?.is_none());
    assert!(matches!(
        store.change_page_sharing(&account, &repeated).await?,
        PageSharingOutcome::Accepted
    ));
    assert_eq!(store.page_access_requests(&owner, 0).await?.len(), 1);

    let edit = fixture.grant(fixture.member, PageRole::Edit);
    assert!(matches!(
        store.change_page_sharing(&owner, &edit).await?,
        PageSharingOutcome::Saved(_)
    ));
    let reused_id = PageSharingAction::Grant {
        account_id: fixture.member,
        role: PageRole::Read,
        operation_id: revoke.operation(),
    };
    assert!(matches!(
        store.change_page_sharing(&owner, &reused_id).await?,
        PageSharingOutcome::Conflict
    ));
    assert!(matches!(
        store.change_page_sharing(&owner, &revoke).await?,
        PageSharingOutcome::Saved(_)
    ));
    assert!(store.page_permissions(&account).await?.unwrap().can_edit);
    Ok(())
}
