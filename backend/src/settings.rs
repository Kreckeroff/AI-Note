use serde::{Deserialize, Deserializer};
use uuid::Uuid;

#[derive(Deserialize)]
pub(crate) struct TenantSettings {
    pub(crate) tenants: Vec<TenantSetting>,
    pub(crate) memberships: Vec<MembershipSetting>,
}

#[derive(Deserialize)]
pub(crate) struct TenantSetting {
    pub(crate) id: Uuid,
    pub(crate) name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MembershipSetting {
    pub(crate) tenant_id: Uuid,
    pub(crate) email: String,
    pub(crate) role: TenantRole,
    pub(crate) status: MembershipStatus,
    pub(crate) can_create: bool,
    pub(crate) current: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TenantRole {
    Member,
    Admin,
    Superadmin,
}

impl TenantRole {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Member => "member",
            Self::Admin => "admin",
            Self::Superadmin => "superadmin",
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum MembershipStatus {
    Active,
    Inactive,
}

impl MembershipStatus {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Inactive => "inactive",
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct GrantSettings {
    pub(crate) grants: Vec<GrantSetting>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GrantSetting {
    pub(crate) tenant_id: Uuid,
    pub(crate) space_id: Uuid,
    pub(crate) email: String,
    #[serde(deserialize_with = "explicit_role")]
    pub(crate) role: Option<GrantRole>,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum GrantRole {
    Viewer,
    Editor,
}

impl GrantRole {
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Editor => "editor",
        }
    }
}

fn explicit_role<'de, D: Deserializer<'de>>(value: D) -> Result<Option<GrantRole>, D::Error> {
    Option::deserialize(value)
}
