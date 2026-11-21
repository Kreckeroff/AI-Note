use uuid::Uuid;

pub(crate) const WORKSPACE_PATH: &str = "/workspace";

pub(crate) fn spaces_path(tenant: Uuid) -> String {
    format!("{WORKSPACE_PATH}/{tenant}/spaces")
}

pub(crate) fn space_path(tenant: Uuid, space: Uuid) -> String {
    format!("{}/{space}", spaces_path(tenant))
}

pub(crate) fn page_path(tenant: Uuid, space: Uuid, page: Uuid) -> String {
    format!("{}/pages/{page}", space_path(tenant, space))
}
