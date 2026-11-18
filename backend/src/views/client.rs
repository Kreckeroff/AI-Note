use serde::{Serialize, Serializer};
use uuid::Uuid;

use crate::{
    db::{Page, Section, Space, Tenant},
    sections::{NameForm, SectionScreen},
    workspace::{CatalogOptions, WorkspaceContext},
};

pub(crate) const BUNDLE_PATH: &str = "/sever-ui.js";
pub(crate) const BUNDLE: &str = include_str!(concat!(env!("OUT_DIR"), "/sever-ui.js"));

#[derive(Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(super) enum ClientPage<'a> {
    SignIn {
        email: &'a str,
        error: &'a str,
        registration_enabled: bool,
        return_to: &'a str,
    },
    RegistrationStart {
        email: &'a str,
        error: &'a str,
    },
    RegistrationRequested,
    RegistrationComplete {
        token: &'a str,
        name: &'a str,
        error: &'a str,
    },
    NoTenant {
        account_name: &'a str,
        #[serde(serialize_with = "serialize_tenant_links")]
        tenants: &'a [Tenant],
    },
    Catalog {
        #[serde(serialize_with = "serialize_workspace")]
        workspace: &'a WorkspaceContext,
    },
    CatalogUnavailable {
        retry_to: &'a str,
    },
    DocumentsUnavailable {
        retry_to: &'a str,
    },
    ClosedSpace {
        #[serde(serialize_with = "serialize_workspace")]
        workspace: &'a WorkspaceContext,
        #[serde(serialize_with = "serialize_space")]
        space: &'a Space,
        #[serde(serialize_with = "serialize_section_tree")]
        tree: &'a SectionScreen,
    },
    SpaceNotFound,
    SectionNotFound,
    PageNotFound,
    CreateSpace {
        #[serde(serialize_with = "serialize_workspace")]
        workspace: &'a WorkspaceContext,
        operation_id: Uuid,
        name: &'a str,
        error: &'a str,
        invalid_name: bool,
        new_form: bool,
    },
}

impl ClientPage<'_> {
    pub(super) fn script_json(&self) -> serde_json::Result<String> {
        // HTML's script parser handles markup delimiters even in application/json.
        serde_json::to_string(self).map(|json| json.replace('<', "\\u003c"))
    }
}

#[derive(Serialize)]
struct TenantLink<'a> {
    id: Uuid,
    name: &'a str,
}

impl<'a> From<&'a Tenant> for TenantLink<'a> {
    fn from(tenant: &'a Tenant) -> Self {
        Self {
            id: tenant.id,
            name: &tenant.name,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CurrentTenant<'a> {
    #[serde(flatten)]
    link: TenantLink<'a>,
    role: &'a str,
    can_create: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SpaceData<'a> {
    id: Uuid,
    name: &'a str,
    owner_name: &'a str,
    is_owner: bool,
    tenant_role: &'a str,
    grant_role: Option<&'a str>,
}

impl<'a> From<&'a Space> for SpaceData<'a> {
    fn from(space: &'a Space) -> Self {
        Self {
            id: space.id,
            name: &space.name,
            owner_name: &space.owner_name,
            is_owner: space.is_owner,
            tenant_role: &space.tenant_role,
            grant_role: space.grant_role.as_deref(),
        }
    }
}

#[derive(Serialize)]
struct OptionsData<'a> {
    q: &'a str,
    scope: &'a str,
    page: usize,
}

impl<'a> From<&'a CatalogOptions> for OptionsData<'a> {
    fn from(options: &'a CatalogOptions) -> Self {
        Self {
            q: &options.q,
            scope: options.scope.as_str(),
            page: options.page,
        }
    }
}

#[derive(Serialize)]
struct CatalogData<'a> {
    options: OptionsData<'a>,
    rows: Vec<SpaceData<'a>>,
    total: usize,
    pages: usize,
    start: usize,
    end: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorkspaceData<'a> {
    tenant: CurrentTenant<'a>,
    tenants: Vec<TenantLink<'a>>,
    account_name: &'a str,
    available_spaces: Vec<SpaceData<'a>>,
    catalog: CatalogData<'a>,
}

fn serialize_workspace<S: Serializer>(
    workspace: &WorkspaceContext,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    let catalog = &workspace.catalog;
    WorkspaceData {
        tenant: CurrentTenant {
            link: TenantLink::from(&workspace.tenant),
            role: &workspace.tenant.role,
            can_create: workspace.tenant.can_create,
        },
        tenants: workspace.tenants.iter().map(TenantLink::from).collect(),
        account_name: &workspace.account_name,
        available_spaces: workspace
            .available_spaces
            .iter()
            .map(SpaceData::from)
            .collect(),
        catalog: CatalogData {
            options: OptionsData::from(&catalog.options),
            rows: catalog.rows.iter().map(SpaceData::from).collect(),
            total: catalog.total,
            pages: catalog.pages,
            start: catalog.start,
            end: catalog.end,
        },
    }
    .serialize(serializer)
}

fn serialize_tenant_links<S: Serializer>(
    tenants: &[Tenant],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    tenants
        .iter()
        .map(TenantLink::from)
        .collect::<Vec<_>>()
        .serialize(serializer)
}

fn serialize_space<S: Serializer>(space: &Space, serializer: S) -> Result<S::Ok, S::Error> {
    SpaceData::from(space).serialize(serializer)
}

#[derive(Serialize)]
struct SectionData<'a> {
    id: Uuid,
    title: &'a str,
}

impl<'a> From<&'a Section> for SectionData<'a> {
    fn from(section: &'a Section) -> Self {
        Self {
            id: section.id,
            title: &section.title,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NameFormData<'a> {
    operation_id: Uuid,
    title: &'a str,
    error: &'a str,
    invalid_name: bool,
    cancel_to: &'a str,
    action: &'a str,
    retry_to: Option<&'a str>,
}

impl<'a> From<&'a NameForm> for NameFormData<'a> {
    fn from(form: &'a NameForm) -> Self {
        Self {
            operation_id: form.operation_id,
            title: &form.title,
            error: &form.error,
            invalid_name: form.invalid_name,
            cancel_to: &form.cancel_to,
            action: &form.action,
            retry_to: form.retry_to.as_deref(),
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PageData<'a> {
    id: Uuid,
    section_id: Uuid,
    title: &'a str,
}

impl<'a> From<&'a Page> for PageData<'a> {
    fn from(page: &'a Page) -> Self {
        Self {
            id: page.id,
            section_id: page.section_id,
            title: &page.title,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SectionTreeData<'a> {
    sections: Vec<SectionData<'a>>,
    selected: Option<SectionData<'a>>,
    can_create_section: bool,
    form: Option<NameFormData<'a>>,
    pages: Vec<PageData<'a>>,
    selected_page: Option<PageData<'a>>,
    can_create_page: bool,
    page_form: Option<NameFormData<'a>>,
}

fn serialize_section_tree<S: Serializer>(
    tree: &SectionScreen,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    SectionTreeData {
        sections: tree.sections.iter().map(SectionData::from).collect(),
        selected: tree.selected.as_ref().map(SectionData::from),
        can_create_section: tree.can_create_section,
        form: tree.form.as_ref().map(NameFormData::from),
        pages: tree.pages.iter().map(PageData::from).collect(),
        selected_page: tree.selected_page.as_ref().map(PageData::from),
        can_create_page: tree.can_create_section,
        page_form: tree.page_form.as_ref().map(NameFormData::from),
    }
    .serialize(serializer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tenant_switcher_serializes_links_without_permission_fields() {
        let tenant = Tenant {
            id: Uuid::nil(),
            name: "Вторая среда".to_owned(),
            role: "admin".to_owned(),
            can_create: true,
        };
        let data = serde_json::to_value(ClientPage::NoTenant {
            account_name: "Читатель",
            tenants: &[tenant],
        })
        .unwrap();
        assert_eq!(
            data["tenants"][0],
            serde_json::json!({ "id": Uuid::nil(), "name": "Вторая среда" })
        );
    }

    #[test]
    fn script_json_preserves_values_without_html_delimiters() {
        let name = "Команда < Север & \"Архив\"";
        let json = ClientPage::NoTenant {
            account_name: name,
            tenants: &[],
        }
        .script_json()
        .unwrap();
        assert!(!json.contains('<'));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&json).unwrap(),
            serde_json::json!({"kind": "noTenant", "accountName": name, "tenants": []})
        );
    }
}
