mod invitations;

pub(crate) use invitations::{Invitation, InvitationAccess};

use sqlx::{FromRow, PgConnection};
use uuid::Uuid;

use super::{Store, sections::decode_title};

#[derive(FromRow)]
pub(crate) struct OrganizationSettings {
    pub has_logo: bool,
    pub allow_guests: bool,
    pub allow_public: bool,
}

#[derive(FromRow)]
pub(crate) struct OrganizationMember {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub role: String,
    pub can_create: bool,
    pub department: Option<String>,
}

#[derive(FromRow)]
pub(crate) struct OrganizationGroup {
    pub id: Uuid,
    pub name: String,
}

#[derive(FromRow)]
pub(crate) struct SpaceGrant {
    pub space: Uuid,
    pub space_name: Vec<u8>,
    pub account: Uuid,
    pub name: String,
    pub email: String,
    pub role: String,
    pub origins: Vec<String>,
}

impl SpaceGrant {
    pub(crate) fn space_name(&self) -> Result<String, sqlx::Error> {
        decode_title(self.space_name.clone(), "spaces.name")
    }
}

#[derive(Clone, Copy)]
pub(crate) enum MemberChange<'a> {
    CanCreate(bool),
    Administrator(bool),
    Department(&'a str),
}

pub(crate) enum Recipient<'a> {
    Person(Uuid),
    Group(Uuid),
    Department(&'a str),
}

pub(crate) async fn lock_tenant(tx: &mut PgConnection, tenant: Uuid) -> Result<(), sqlx::Error> {
    // Capability changes and publication must acquire the stable tenant row first.
    sqlx::query("SELECT id FROM tenants WHERE id = $1 FOR NO KEY UPDATE")
        .bind(tenant)
        .fetch_optional(tx)
        .await?;
    Ok(())
}

pub(crate) async fn member_role(
    tx: &mut PgConnection,
    tenant: Uuid,
    account: Uuid,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar("SELECT role FROM tenant_memberships WHERE tenant_id=$1 AND account_id=$2 AND status='active'")
        .bind(tenant).bind(account).fetch_optional(tx).await
}

pub(crate) fn admin(role: &str) -> bool {
    matches!(role, "admin" | "superadmin")
}

impl Store {
    pub(crate) async fn switch_organization(
        &self,
        account: Uuid,
        tenant: Uuid,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        // The same lock coordinates two switches and onboarding invitations.
        sqlx::query("SELECT id FROM accounts WHERE id=$1 FOR NO KEY UPDATE")
            .bind(account)
            .fetch_optional(&mut *tx)
            .await?;
        if member_role(&mut tx, tenant, account).await?.is_none() {
            return Ok(false);
        }
        sqlx::query("UPDATE tenant_memberships SET is_current=false WHERE account_id=$1 AND is_current AND tenant_id<>$2")
            .bind(account).bind(tenant).execute(&mut *tx).await?;
        let count = sqlx::query("UPDATE tenant_memberships SET is_current=true WHERE account_id=$1 AND tenant_id=$2 AND status='active'")
            .bind(account).bind(tenant).execute(&mut *tx).await?.rows_affected();
        if count == 0 {
            return Ok(false);
        }
        tx.commit().await?;
        Ok(true)
    }

    pub(crate) async fn organization_settings(
        &self,
        tenant: Uuid,
    ) -> Result<OrganizationSettings, sqlx::Error> {
        sqlx::query_as(
            "SELECT b.tenant_id IS NOT NULL AS has_logo,
                    coalesce(p.allow_guests,false) AS allow_guests,
                    coalesce(p.allow_public,false) AS allow_public
             FROM tenants t LEFT JOIN organization_brand b ON b.tenant_id=t.id
             LEFT JOIN tenant_sharing_policies p ON p.tenant_id=t.id WHERE t.id=$1",
        )
        .bind(tenant)
        .fetch_one(&self.0)
        .await
    }

    pub(crate) async fn organization_logo(
        &self,
        tenant: Uuid,
        actor: Uuid,
    ) -> Result<Option<Vec<u8>>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT b.logo FROM organization_brand b
             JOIN tenant_memberships m USING(tenant_id)
             WHERE b.tenant_id=$1 AND m.account_id=$2 AND m.status='active'",
        )
        .bind(tenant)
        .bind(actor)
        .fetch_optional(&self.0)
        .await
    }

    pub(crate) async fn rename_organization(
        &self,
        tenant: Uuid,
        actor: Uuid,
        name: &str,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        lock_tenant(&mut tx, tenant).await?;
        if member_role(&mut tx, tenant, actor).await?.as_deref() != Some("superadmin") {
            return Ok(false);
        }
        sqlx::query("UPDATE tenants SET name=$2 WHERE id=$1")
            .bind(tenant)
            .bind(name)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub(crate) async fn update_organization_logo(
        &self,
        tenant: Uuid,
        actor: Uuid,
        image: Option<Vec<u8>>,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        lock_tenant(&mut tx, tenant).await?;
        if member_role(&mut tx, tenant, actor).await?.as_deref() != Some("superadmin") {
            return Ok(false);
        }
        if let Some(image) = image {
            sqlx::query(
                "INSERT INTO organization_brand VALUES($1,$2)
                ON CONFLICT(tenant_id) DO UPDATE SET logo=excluded.logo",
            )
            .bind(tenant)
            .bind(image)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query("DELETE FROM organization_brand WHERE tenant_id=$1")
                .bind(tenant)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;
        Ok(true)
    }

    pub(crate) async fn update_organization_policies(
        &self,
        tenant: Uuid,
        actor: Uuid,
        guests: bool,
        public: bool,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        lock_tenant(&mut tx, tenant).await?;
        if member_role(&mut tx, tenant, actor).await?.as_deref() != Some("superadmin") {
            return Ok(false);
        }
        super::configuration::configure_sharing(&mut tx, tenant, Some(guests), Some(public))
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub(crate) async fn organization_members(
        &self,
        tenant: Uuid,
        query: &str,
        role: &str,
        department: &str,
        offset: i64,
    ) -> Result<Vec<OrganizationMember>, sqlx::Error> {
        sqlx::query_as(
            "SELECT a.id,a.name,a.email,m.role,m.can_create,d.department
             FROM tenant_memberships m JOIN accounts a ON a.id=m.account_id
             LEFT JOIN organization_departments d USING(tenant_id,account_id)
             WHERE m.tenant_id=$1 AND m.status='active'
               AND ($2='' OR strpos(lower(a.name),lower($2))>0 OR strpos(a.email,lower($2))>0)
               AND ($3='' OR m.role=$3) AND ($4='' OR d.department=$4)
             ORDER BY lower(a.name),a.id LIMIT 21 OFFSET $5",
        )
        .bind(tenant)
        .bind(query)
        .bind(role)
        .bind(department)
        .bind(offset)
        .fetch_all(&self.0)
        .await
    }

    pub(crate) async fn organization_groups(
        &self,
        tenant: Uuid,
    ) -> Result<Vec<OrganizationGroup>, sqlx::Error> {
        sqlx::query_as("SELECT id,name FROM organization_groups WHERE tenant_id=$1 ORDER BY lower(name),id LIMIT 200")
            .bind(tenant).fetch_all(&self.0).await
    }

    pub(crate) async fn organization_departments(
        &self,
        tenant: Uuid,
    ) -> Result<Vec<String>, sqlx::Error> {
        sqlx::query_scalar("SELECT DISTINCT department FROM organization_departments WHERE tenant_id=$1 ORDER BY department LIMIT 200")
            .bind(tenant).fetch_all(&self.0).await
    }

    pub(crate) async fn change_organization_member(
        &self,
        tenant: Uuid,
        actor: Uuid,
        account: Uuid,
        change: MemberChange<'_>,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        lock_tenant(&mut tx, tenant).await?;
        let Some(author) = member_role(&mut tx, tenant, actor).await? else {
            return Ok(false);
        };
        let Some(target) = member_role(&mut tx, tenant, account).await? else {
            return Ok(false);
        };
        if !admin(&author)
            || (target == "superadmin" && actor != account)
            || matches!(change, MemberChange::Administrator(_))
                && (author != "superadmin" || target == "superadmin")
        {
            return Ok(false);
        }
        match change {
            MemberChange::CanCreate(value) => {
                sqlx::query("UPDATE tenant_memberships SET can_create=$3 WHERE tenant_id=$1 AND account_id=$2")
                    .bind(tenant).bind(account).bind(value).execute(&mut *tx).await?;
            }
            MemberChange::Administrator(value) => {
                sqlx::query(
                    "UPDATE tenant_memberships SET role=$3 WHERE tenant_id=$1 AND account_id=$2",
                )
                .bind(tenant)
                .bind(account)
                .bind(if value { "admin" } else { "member" })
                .execute(&mut *tx)
                .await?;
            }
            MemberChange::Department("") => {
                sqlx::query(
                    "DELETE FROM organization_departments WHERE tenant_id=$1 AND account_id=$2",
                )
                .bind(tenant)
                .bind(account)
                .execute(&mut *tx)
                .await?;
            }
            MemberChange::Department(label) => {
                sqlx::query(
                    "INSERT INTO organization_departments VALUES($1,$2,$3)
                    ON CONFLICT(tenant_id,account_id) DO UPDATE SET department=excluded.department
                    WHERE organization_departments.department <> excluded.department",
                )
                .bind(tenant)
                .bind(account)
                .bind(label)
                .execute(&mut *tx)
                .await?;
            }
        }
        tx.commit().await?;
        Ok(true)
    }

    pub(crate) async fn add_organization_group(
        &self,
        tenant: Uuid,
        actor: Uuid,
        name: &str,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        lock_tenant(&mut tx, tenant).await?;
        if !member_role(&mut tx, tenant, actor)
            .await?
            .is_some_and(|role| admin(&role))
        {
            return Ok(false);
        }
        sqlx::query("INSERT INTO organization_groups VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
            .bind(tenant)
            .bind(Uuid::new_v4())
            .bind(name)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }

    pub(crate) async fn change_organization_group_member(
        &self,
        tenant: Uuid,
        actor: Uuid,
        group: Uuid,
        account: Uuid,
        active: bool,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        lock_tenant(&mut tx, tenant).await?;
        if !member_role(&mut tx, tenant, actor)
            .await?
            .is_some_and(|role| admin(&role))
        {
            return Ok(false);
        }
        let found: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM organization_groups WHERE tenant_id=$1 AND id=$2)",
        )
        .bind(tenant)
        .bind(group)
        .fetch_one(&mut *tx)
        .await?;
        if !found || member_role(&mut tx, tenant, account).await?.is_none() {
            return Ok(false);
        }
        if active {
            sqlx::query(
                "INSERT INTO organization_group_members VALUES($1,$2,$3) ON CONFLICT DO NOTHING",
            )
            .bind(tenant)
            .bind(group)
            .bind(account)
            .execute(&mut *tx)
            .await?;
        } else {
            sqlx::query("DELETE FROM organization_group_members WHERE tenant_id=$1 AND group_id=$2 AND account_id=$3")
                .bind(tenant).bind(group).bind(account).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(true)
    }

    pub(crate) async fn organization_space_grants(
        &self,
        tenant: Uuid,
        space: Option<Uuid>,
        query: &str,
        role: &str,
        source: &str,
        offset: i64,
    ) -> Result<Vec<SpaceGrant>, sqlx::Error> {
        sqlx::query_as(
            "WITH visible AS (
                SELECT s.id AS space,s.name AS space_name,a.id AS account,a.name,a.email,
                    CASE WHEN s.owner_account_id=m.account_id THEN 'owner'
                        WHEN g.role='editor' THEN 'editor' ELSE 'viewer' END AS role,
                    ARRAY_REMOVE(ARRAY[
                        CASE WHEN s.owner_account_id=m.account_id THEN 'owner' END,
                        CASE WHEN m.role IN ('admin','superadmin') THEN 'administrator' END,
                        CASE WHEN EXISTS (SELECT 1 FROM space_grants d WHERE (d.tenant_id,d.space_id,d.account_id)=(m.tenant_id,s.id,m.account_id)) THEN 'direct' END,
                        CASE WHEN EXISTS (SELECT 1 FROM organization_group_members gm
                            JOIN space_group_grants gg USING(tenant_id,group_id)
                            WHERE (gm.tenant_id,gg.space_id,gm.account_id)=(m.tenant_id,s.id,m.account_id)) THEN 'group' END,
                        CASE WHEN EXISTS (SELECT 1 FROM organization_departments d
                            JOIN space_department_grants g USING(tenant_id,department)
                            WHERE (d.tenant_id,g.space_id,d.account_id)=(m.tenant_id,s.id,m.account_id)) THEN 'department' END
                    ], NULL) AS origins
                FROM spaces s JOIN tenant_memberships m USING(tenant_id) JOIN accounts a ON a.id=m.account_id
                LEFT JOIN effective_space_grants g ON (g.tenant_id,g.space_id,g.account_id)=(s.tenant_id,s.id,m.account_id)
                WHERE s.tenant_id=$1 AND ($2::uuid IS NULL OR s.id=$2) AND m.status='active'
                  AND (s.owner_account_id=m.account_id OR m.role IN ('admin','superadmin') OR g.role IS NOT NULL)
                  AND ($3='' OR strpos(lower(a.name),lower($3))>0 OR strpos(a.email,lower($3))>0
                            OR position(convert_to(lower($3),'UTF8') IN s.name_key)>0)
             )
             SELECT * FROM visible WHERE ($4='' OR role=$4) AND ($5='' OR $5=ANY(origins))
             ORDER BY space_name,space,lower(name),account LIMIT 21 OFFSET $6",
        ).bind(tenant).bind(space).bind(query).bind(role).bind(source).bind(offset).fetch_all(&self.0).await
    }

    pub(crate) async fn change_space_grant(
        &self,
        tenant: Uuid,
        space: Uuid,
        actor: Uuid,
        recipient: Recipient<'_>,
        role: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        lock_tenant(&mut tx, tenant).await?;
        let Some(author) = member_role(&mut tx, tenant, actor).await? else {
            return Ok(false);
        };
        let Some(owner): Option<Uuid> =
            sqlx::query_scalar("SELECT owner_account_id FROM spaces WHERE tenant_id=$1 AND id=$2")
                .bind(tenant)
                .bind(space)
                .fetch_optional(&mut *tx)
                .await?
        else {
            return Ok(false);
        };
        if actor != owner && !admin(&author) {
            return Ok(false);
        }
        match recipient {
            Recipient::Person(account) => {
                if member_role(&mut tx, tenant, account).await?.is_none() {
                    return Ok(false);
                }
                if let Some(role) = role {
                    sqlx::query("INSERT INTO space_grants VALUES($1,$2,$3,$4) ON CONFLICT(tenant_id,space_id,account_id)
                        DO UPDATE SET role=excluded.role WHERE space_grants.role<>excluded.role")
                        .bind(tenant).bind(space).bind(account).bind(role).execute(&mut *tx).await?;
                } else {
                    sqlx::query("DELETE FROM space_grants WHERE tenant_id=$1 AND space_id=$2 AND account_id=$3")
                        .bind(tenant).bind(space).bind(account).execute(&mut *tx).await?;
                }
            }
            Recipient::Group(group) if admin(&author) => {
                let found: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM organization_groups WHERE tenant_id=$1 AND id=$2)",
                )
                .bind(tenant)
                .bind(group)
                .fetch_one(&mut *tx)
                .await?;
                if !found {
                    return Ok(false);
                }
                if let Some(role) = role {
                    sqlx::query("INSERT INTO space_group_grants VALUES($1,$2,$3,$4) ON CONFLICT(tenant_id,space_id,group_id)
                        DO UPDATE SET role=excluded.role WHERE space_group_grants.role<>excluded.role")
                        .bind(tenant).bind(space).bind(group).bind(role).execute(&mut *tx).await?;
                } else {
                    sqlx::query("DELETE FROM space_group_grants WHERE tenant_id=$1 AND space_id=$2 AND group_id=$3")
                        .bind(tenant).bind(space).bind(group).execute(&mut *tx).await?;
                }
            }
            Recipient::Department(department) if admin(&author) => {
                if let Some(role) = role {
                    sqlx::query("INSERT INTO space_department_grants VALUES($1,$2,$3,$4) ON CONFLICT(tenant_id,space_id,department)
                        DO UPDATE SET role=excluded.role WHERE space_department_grants.role<>excluded.role")
                        .bind(tenant).bind(space).bind(department).bind(role).execute(&mut *tx).await?;
                } else {
                    sqlx::query("DELETE FROM space_department_grants WHERE tenant_id=$1 AND space_id=$2 AND department=$3")
                        .bind(tenant).bind(space).bind(department).execute(&mut *tx).await?;
                }
            }
            Recipient::Group(_) | Recipient::Department(_) => return Ok(false),
        }
        tx.commit().await?;
        Ok(true)
    }
}

#[derive(FromRow)]
pub(crate) struct ProfileSettings {
    pub name: String,
    pub email: String,
}

impl Store {
    pub(crate) async fn profile_settings(
        &self,
        account: Uuid,
    ) -> Result<ProfileSettings, sqlx::Error> {
        sqlx::query_as("SELECT name,email FROM accounts WHERE id=$1")
            .bind(account)
            .fetch_one(&self.0)
            .await
    }

    pub(crate) async fn rename_profile(
        &self,
        account: Uuid,
        name: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("UPDATE accounts SET name=$2 WHERE id=$1")
            .bind(account)
            .bind(name)
            .execute(&self.0)
            .await?;
        Ok(())
    }

    pub(crate) async fn change_profile_password(
        &self,
        account: Uuid,
        old_hash: &[u8],
        replacement: &super::NewAccount,
        keep_session: &[u8],
    ) -> Result<bool, sqlx::Error> {
        let mut tx = self.0.begin().await?;
        let count = sqlx::query(
            "UPDATE accounts SET salt=$3,password_hash=$4 WHERE id=$1 AND password_hash=$2",
        )
        .bind(account)
        .bind(old_hash)
        .bind(&replacement.salt)
        .bind(&replacement.password_hash)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if count == 0 {
            return Ok(false);
        }
        sqlx::query("DELETE FROM sessions WHERE account_id=$1 AND token_digest<>$2")
            .bind(account)
            .bind(keep_session)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(true)
    }
}

#[derive(FromRow)]
pub(crate) struct SpaceRule {
    pub kind: String,
    pub target: String,
    pub label: String,
    pub role: String,
}

impl Store {
    pub(crate) async fn space_grant_rules(
        &self,
        tenant: Uuid,
        space: Uuid,
    ) -> Result<Vec<SpaceRule>, sqlx::Error> {
        sqlx::query_as(
            "SELECT 'group' AS kind,gg.group_id::text AS target,g.name AS label,gg.role
             FROM space_group_grants gg JOIN organization_groups g ON (g.tenant_id,g.id)=(gg.tenant_id,gg.group_id)
             WHERE gg.tenant_id=$1 AND gg.space_id=$2
             UNION ALL
             SELECT 'department',department,department,role FROM space_department_grants WHERE tenant_id=$1 AND space_id=$2
             ORDER BY kind,label LIMIT 200",
        ).bind(tenant).bind(space).fetch_all(&self.0).await
    }

    pub(crate) async fn member_group_ids(
        &self,
        tenant: Uuid,
        accounts: &[Uuid],
    ) -> Result<Vec<(Uuid, Uuid)>, sqlx::Error> {
        sqlx::query_as(
            "SELECT account_id,group_id FROM organization_group_members
            WHERE tenant_id=$1 AND account_id=ANY($2)",
        )
        .bind(tenant)
        .bind(accounts)
        .fetch_all(&self.0)
        .await
    }
}

#[cfg(test)]
mod tests;
