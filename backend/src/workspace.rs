use icu_collator::{
    Collator, CollatorPreferences,
    options::{CollatorOptions, Strength},
    preferences::CollationNumericOrdering,
};
use icu_locale_core::locale;
use uuid::Uuid;

use crate::{
    db::{Principal, Space, Tenant, TenantSpaces},
    text::{fold_name, js_trim},
};

pub(crate) const WORKSPACE_PATH: &str = "/workspace";
const PAGE_SIZE: usize = 20;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Scope {
    All,
    Mine,
    Membership,
}

impl Scope {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Mine => "mine",
            Self::Membership => "membership",
        }
    }
}

#[derive(Clone)]
pub(crate) struct CatalogOptions {
    pub q: String,
    pub scope: Scope,
    pub page: usize,
}

impl CatalogOptions {
    pub(crate) fn parse(query: &str) -> Self {
        let mut value = [None, None, None];
        for (key, input) in url::form_urlencoded::parse(query.as_bytes()) {
            let index = match key.as_ref() {
                "q" => 0,
                "scope" => 1,
                "page" => 2,
                _ => continue,
            };
            value[index].get_or_insert(input);
        }
        let [q, scope, page] = value.map(|part| part.unwrap_or_default());
        let scope = match scope.as_ref() {
            "mine" => Scope::Mine,
            "membership" => Scope::Membership,
            _ => Scope::All,
        };
        let requested_page = page.as_ref();
        let page =
            if requested_page.len() <= 15 && requested_page.bytes().all(|ch| ch.is_ascii_digit()) {
                requested_page.parse::<usize>().unwrap_or(1).max(1)
            } else {
                1
            };
        Self {
            q: js_trim(&q).chars().take(100).collect(),
            scope,
            page,
        }
    }
}

pub(crate) struct Catalog {
    pub options: CatalogOptions,
    pub rows: Vec<Space>,
    pub total: usize,
    pub pages: usize,
    pub start: usize,
    pub end: usize,
}

pub(crate) struct WorkspaceContext {
    pub tenant: Tenant,
    pub account_name: String,
    pub tenants: Vec<Tenant>,
    pub available_spaces: Vec<Space>,
    pub catalog: Catalog,
}

impl WorkspaceContext {
    pub(crate) fn build(
        data: TenantSpaces,
        principal: &Principal,
        mut options: CatalogOptions,
        clamp_page: bool,
    ) -> Self {
        let TenantSpaces {
            tenant,
            tenants,
            mut spaces,
        } = data;
        let mut preferences = CollatorPreferences::from(locale!("ru"));
        preferences.numeric_ordering = Some(CollationNumericOrdering::True);
        let mut collator_options = CollatorOptions::default();
        collator_options.strength = Some(Strength::Primary);
        let collator = Collator::try_new(preferences, collator_options)
            .expect("compiled ICU data contains Russian collation");
        spaces.sort_by(|left, right| {
            collator
                .compare(&left.name, &right.name)
                .then_with(|| collator.compare(&left.owner_name, &right.owner_name))
                .then_with(|| left.id.cmp(&right.id))
        });
        let search = fold_name(&options.q);
        let matches: Vec<&Space> = spaces
            .iter()
            .filter(|space| {
                (match options.scope {
                    Scope::All => true,
                    Scope::Mine => space.is_owner,
                    Scope::Membership => space.grant_role.is_some(),
                }) && (search.is_empty() || fold_name(&space.name).contains(&search))
            })
            .collect();
        let total = matches.len();
        let pages = total.div_ceil(PAGE_SIZE).max(1);
        let effective_page = options.page.min(pages);
        if clamp_page {
            options.page = effective_page;
        }
        let offset = (effective_page - 1) * PAGE_SIZE;
        let rows: Vec<Space> = matches
            .into_iter()
            .skip(offset)
            .take(PAGE_SIZE)
            .cloned()
            .collect();
        let end = offset + rows.len();
        let catalog = Catalog {
            options,
            rows,
            total,
            pages,
            start: offset + 1,
            end,
        };
        Self {
            tenant,
            account_name: principal.name.clone(),
            tenants,
            available_spaces: spaces,
            catalog,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum Route {
    Current,
    Collection(Uuid),
    New(Uuid),
    Documents(DocumentsRoute),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DocumentsTarget {
    Space,
    Section(Uuid),
    NewSection,
    Sections,
    Page(Uuid),
    EditPage(Uuid),
    NewPage(Uuid),
    Pages(Uuid),
}

#[derive(Clone, Copy)]
pub(crate) struct DocumentsRoute {
    pub tenant: Uuid,
    pub space: Uuid,
    pub target: DocumentsTarget,
}

impl Route {
    pub(crate) fn parse(path: &str) -> Option<Self> {
        if path == WORKSPACE_PATH {
            return Some(Self::Current);
        }
        let mut parts = path.strip_prefix("/workspace/")?.split('/');
        let tenant = canonical_uuid(parts.next()?)?;
        if parts.next()? != "spaces" {
            return None;
        }
        let route = match parts.next() {
            None => Self::Collection(tenant),
            Some("new") => Self::New(tenant),
            Some(id) => {
                let space = canonical_uuid(id)?;
                let target = match parts.next() {
                    None => DocumentsTarget::Space,
                    Some("sections") => match parts.next() {
                        None => DocumentsTarget::Sections,
                        Some("new") => DocumentsTarget::NewSection,
                        Some(id) => {
                            let section = canonical_uuid(id)?;
                            match parts.next() {
                                None => DocumentsTarget::Section(section),
                                Some("pages") => match parts.next() {
                                    None => DocumentsTarget::Pages(section),
                                    Some("new") => DocumentsTarget::NewPage(section),
                                    _ => return None,
                                },
                                _ => return None,
                            }
                        }
                    },
                    Some("pages") => {
                        let page = canonical_uuid(parts.next()?)?;
                        match parts.next() {
                            None => DocumentsTarget::Page(page),
                            Some("edit") => DocumentsTarget::EditPage(page),
                            _ => return None,
                        }
                    }
                    _ => return None,
                };
                Self::Documents(DocumentsRoute {
                    tenant,
                    space,
                    target,
                })
            }
        };
        parts.next().is_none().then_some(route)
    }

    pub(crate) fn tenant(self) -> Option<Uuid> {
        match self {
            Self::Current => None,
            Self::Collection(id) | Self::New(id) => Some(id),
            Self::Documents(route) => Some(route.tenant),
        }
    }

    pub(crate) fn is_catalog(self) -> bool {
        matches!(self, Self::Current | Self::Collection(_))
    }

    pub(crate) fn allows_read(self) -> bool {
        !matches!(
            self,
            Self::Documents(DocumentsRoute {
                target: DocumentsTarget::Sections | DocumentsTarget::Pages(_),
                ..
            })
        )
    }

    pub(crate) fn allows_post(self) -> bool {
        matches!(
            self,
            Self::Collection(_)
                | Self::Documents(DocumentsRoute {
                    target: DocumentsTarget::Sections
                        | DocumentsTarget::Pages(_)
                        | DocumentsTarget::EditPage(_),
                    ..
                })
        )
    }
}

pub(crate) fn canonical_uuid(value: &str) -> Option<Uuid> {
    let id = Uuid::parse_str(value).ok()?;
    (value == id.to_string()).then_some(id)
}

pub(crate) fn spaces_path(tenant: Uuid) -> String {
    format!("{WORKSPACE_PATH}/{tenant}/spaces")
}

pub(crate) fn new_space_path(tenant: Uuid) -> String {
    format!("{}/new", spaces_path(tenant))
}

pub(crate) fn space_path(tenant: Uuid, space: Uuid) -> String {
    format!("{}/{space}", spaces_path(tenant))
}

pub(crate) fn sections_path(tenant: Uuid, space: Uuid) -> String {
    format!("{}/sections", space_path(tenant, space))
}

pub(crate) fn section_path(tenant: Uuid, space: Uuid, section: Uuid) -> String {
    format!("{}/{section}", sections_path(tenant, space))
}

pub(crate) fn pages_path(tenant: Uuid, space: Uuid, section: Uuid) -> String {
    format!("{}/pages", section_path(tenant, space, section))
}

pub(crate) fn new_page_path(tenant: Uuid, space: Uuid, section: Uuid) -> String {
    format!("{}/new", pages_path(tenant, space, section))
}

pub(crate) fn page_path(tenant: Uuid, space: Uuid, page: Uuid) -> String {
    format!("{}/pages/{page}", space_path(tenant, space))
}

pub(crate) fn page_edit_path(tenant: Uuid, space: Uuid, page: Uuid) -> String {
    format!("{}/edit", page_path(tenant, space, page))
}

pub(crate) fn section_form_path(
    path: &str,
    from: Option<Uuid>,
    options: &CatalogOptions,
) -> String {
    let mut destination = with_options(path, options);
    if let Some(id) = from {
        let separator = if destination.contains('?') { '&' } else { '?' };
        destination.push_str(&format!("{separator}from={id}"));
    }
    destination
}

pub(crate) fn new_section_path(
    tenant: Uuid,
    space: Uuid,
    from: Option<Uuid>,
    options: &CatalogOptions,
) -> String {
    section_form_path(
        &format!("{}/new", sections_path(tenant, space)),
        from,
        options,
    )
}

pub(crate) fn section_origin(query: &str) -> Option<Uuid> {
    let mut values = url::form_urlencoded::parse(query.as_bytes())
        .filter_map(|(name, value)| (name == "from").then_some(value));
    let from = values.next()?;
    if values.next().is_some() {
        None
    } else {
        canonical_uuid(&from)
    }
}

pub(crate) fn with_options(path: &str, options: &CatalogOptions) -> String {
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    if !options.q.is_empty() {
        query.append_pair("q", &options.q);
    }
    if options.scope != Scope::All {
        query.append_pair("scope", options.scope.as_str());
    }
    if options.page > 1 {
        query.append_pair("page", &options.page.to_string());
    }
    let query = query.finish();
    if query.is_empty() {
        path.to_owned()
    } else {
        format!("{path}?{query}")
    }
}

pub(crate) fn local_target(target: &str) -> String {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let Some(route) = Route::parse(path).filter(|route| route.allows_read()) else {
        return String::new();
    };
    let options = CatalogOptions::parse(query);
    let destination = match route {
        Route::Documents(DocumentsRoute {
            target: DocumentsTarget::NewSection,
            ..
        }) => section_form_path(path, section_origin(query), &options),
        _ => with_options(path, &options),
    };
    if destination.len() > 1400 {
        String::new()
    } else {
        destination
    }
}

#[cfg(test)]
mod route_tests {
    use super::*;

    const SPACE: &str = "/workspace/11111111-1111-1111-1111-111111111111/spaces/22222222-2222-2222-2222-222222222222";
    const SECTION: &str = "33333333-3333-3333-3333-333333333333";

    #[test]
    fn section_links_restore_the_section_and_catalog_options_after_login() {
        let item = format!("{SPACE}/sections/{SECTION}?scope=mine&page=3");
        assert_eq!(local_target(&item), item);

        let form = format!("{SPACE}/sections/new?q=foo&from={SECTION}");
        assert_eq!(local_target(&form), form);
        assert_eq!(
            local_target(&format!("{SPACE}/sections/new?from=https://example.test")),
            format!("{SPACE}/sections/new")
        );
        assert_eq!(local_target(&format!("{SPACE}/sections")), "");
        assert!(Route::parse(&format!("{SPACE}/sections/{SECTION}/extra")).is_none());
    }

    #[test]
    fn page_routes_keep_the_parent_for_creation_and_a_stable_reader_address() {
        let page_id = "44444444-4444-4444-4444-444444444444";
        let collection = format!("{SPACE}/sections/{SECTION}/pages");
        let form = format!("{collection}/new?q=alpha&scope=mine&page=2");
        let reader = format!("{SPACE}/pages/{page_id}?scope=membership");
        let write = Route::parse(&collection).expect("page collection route");
        assert!(write.allows_post());
        assert!(!write.allows_read());
        assert_eq!(local_target(&collection), "");
        assert_eq!(local_target(&form), form);
        assert_eq!(local_target(&reader), reader);
        assert!(Route::parse(&format!("{collection}/{page_id}")).is_none());
        assert!(Route::parse(&format!("{SPACE}/pages/{page_id}/new")).is_none());
        for path in [form, reader] {
            let route = Route::parse(path.split('?').next().unwrap()).unwrap();
            assert!(route.allows_read());
            assert!(!route.allows_post());
        }
    }
}
