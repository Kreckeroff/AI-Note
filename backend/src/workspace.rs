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

pub(crate) use crate::document_path::{WORKSPACE_PATH, page_path, space_path, spaces_path};
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
    pub account_id: Uuid,
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
            account_id: principal.id,
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
    RenamePage(Uuid),
    UploadCover(Uuid),
    CoverImage {
        page: Uuid,
        asset: Uuid,
    },
    Attachment {
        page: Uuid,
        target: AttachmentTarget,
    },
    History {
        page: Uuid,
        target: HistoryTarget,
    },
    NewPage(Uuid),
    Pages(Uuid),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttachmentTarget {
    Upload { draft: Uuid, id: Uuid },
    DraftImage { draft: Uuid, id: Uuid },
    Published { token: Uuid, inline: bool },
}

impl AttachmentTarget {
    fn parse<'a>(parts: &mut impl Iterator<Item = &'a str>) -> Option<Self> {
        match parts.next()? {
            "drafts" => {
                let draft = canonical_uuid(parts.next()?)?;
                let id = canonical_uuid(parts.next()?)?;
                match parts.next() {
                    None => Some(Self::Upload { draft, id }),
                    Some("image") => Some(Self::DraftImage { draft, id }),
                    _ => None,
                }
            }
            token => Some(Self::Published {
                token: canonical_uuid(token)?,
                inline: attachment_inline(parts.next())?,
            }),
        }
    }
}

fn attachment_inline(suffix: Option<&str>) -> Option<bool> {
    match suffix {
        None => Some(false),
        Some("image") => Some(true),
        _ => None,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum HistoryTarget {
    List,
    Revision(i64),
    Cover(i64),
    Attachment {
        revision: i64,
        id: Uuid,
        inline: bool,
    },
    Compare,
    Restore(i64),
}

impl HistoryTarget {
    fn parse<'a>(parts: &mut impl Iterator<Item = &'a str>) -> Option<Self> {
        match parts.next() {
            None => Some(Self::List),
            Some("compare") => Some(Self::Compare),
            Some(number) => {
                let revision = revision_number(number)?;
                match parts.next() {
                    None => Some(Self::Revision(revision)),
                    Some("cover") => Some(Self::Cover(revision)),
                    Some("attachments") => Some(Self::Attachment {
                        revision,
                        id: canonical_uuid(parts.next()?)?,
                        inline: attachment_inline(parts.next())?,
                    }),
                    Some("restore") => Some(Self::Restore(revision)),
                    _ => None,
                }
            }
        }
    }
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
                            Some("rename") => DocumentsTarget::RenamePage(page),
                            Some("cover") => match parts.next() {
                                None => DocumentsTarget::UploadCover(page),
                                Some(id) => DocumentsTarget::CoverImage {
                                    page,
                                    asset: canonical_uuid(id)?,
                                },
                            },
                            Some("attachments") => DocumentsTarget::Attachment {
                                page,
                                target: AttachmentTarget::parse(&mut parts)?,
                            },
                            Some("revisions") => DocumentsTarget::History {
                                page,
                                target: HistoryTarget::parse(&mut parts)?,
                            },
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

    pub(crate) fn is_cover(self) -> bool {
        matches!(
            self,
            Self::Documents(DocumentsRoute {
                target: DocumentsTarget::UploadCover(_)
                    | DocumentsTarget::CoverImage { .. }
                    | DocumentsTarget::History {
                        target: HistoryTarget::Cover(_),
                        ..
                    },
                ..
            })
        )
    }

    pub(crate) fn is_attachment(self) -> bool {
        matches!(
            self,
            Self::Documents(DocumentsRoute {
                target: DocumentsTarget::Attachment { .. }
                    | DocumentsTarget::History {
                        target: HistoryTarget::Attachment { .. },
                        ..
                    },
                ..
            })
        )
    }

    pub(crate) fn history_target(self) -> Option<HistoryTarget> {
        match self {
            Self::Documents(DocumentsRoute {
                target: DocumentsTarget::History { target, .. },
                ..
            }) => Some(target),
            _ => None,
        }
    }

    pub(crate) fn allows_read(self) -> bool {
        !matches!(
            self,
            Self::Documents(DocumentsRoute {
                target: DocumentsTarget::Sections
                    | DocumentsTarget::Pages(_)
                    | DocumentsTarget::UploadCover(_)
                    | DocumentsTarget::Attachment {
                        target: AttachmentTarget::Upload { .. },
                        ..
                    }
                    | DocumentsTarget::History {
                        target: HistoryTarget::Restore(_),
                        ..
                    },
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
                        | DocumentsTarget::EditPage(_)
                        | DocumentsTarget::RenamePage(_)
                        | DocumentsTarget::UploadCover(_)
                        | DocumentsTarget::Attachment {
                            target: AttachmentTarget::Upload { .. },
                            ..
                        }
                        | DocumentsTarget::History {
                            target: HistoryTarget::Restore(_),
                            ..
                        },
                    ..
                })
        )
    }
}

pub(crate) fn canonical_uuid(value: &str) -> Option<Uuid> {
    let id = Uuid::parse_str(value).ok()?;
    (value == id.to_string()).then_some(id)
}

pub(crate) use crate::text::revision_number;

pub(crate) fn new_space_path(tenant: Uuid) -> String {
    format!("{}/new", spaces_path(tenant))
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

pub(crate) fn page_edit_path(tenant: Uuid, space: Uuid, page: Uuid) -> String {
    format!("{}/edit", page_path(tenant, space, page))
}

pub(crate) fn page_history_path(tenant: Uuid, space: Uuid, page: Uuid) -> String {
    format!("{}/revisions", page_path(tenant, space, page))
}

pub(crate) fn page_cover_upload_path(tenant: Uuid, space: Uuid, page: Uuid) -> String {
    format!("{}/cover", page_path(tenant, space, page))
}

pub(crate) fn page_cover_image_path(tenant: Uuid, space: Uuid, page: Uuid, asset: Uuid) -> String {
    format!("{}/{asset}", page_cover_upload_path(tenant, space, page))
}

pub(crate) fn attachment_draft_path(tenant: Uuid, space: Uuid, page: Uuid, draft: Uuid) -> String {
    format!(
        "{}/attachments/drafts/{draft}",
        page_path(tenant, space, page)
    )
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
    let Some(route) = Route::parse(path)
        .filter(|route| route.allows_read() && !route.is_cover() && !route.is_attachment())
    else {
        return String::new();
    };
    let options = CatalogOptions::parse(query);
    let destination = match route {
        Route::Documents(DocumentsRoute {
            target: DocumentsTarget::NewSection,
            ..
        }) => section_form_path(path, section_origin(query), &options),
        Route::Documents(DocumentsRoute {
            target: DocumentsTarget::History { target, .. },
            ..
        }) => history_return_to(path, query, target),
        Route::Documents(DocumentsRoute {
            target: DocumentsTarget::Page(_),
            ..
        }) => {
            let params = crate::http::Form::from_query(query);
            let mut destination = with_options(path, &options);
            if let Some(thread) = canonical_uuid(params.get("thread")) {
                let separator = if destination.contains('?') { '&' } else { '?' };
                destination.push_str(&format!("{separator}thread={thread}"));
                if let Some(comment) = canonical_uuid(params.get("comment")) {
                    destination.push_str(&format!("&comment={comment}"));
                }
            }
            destination
        }
        _ => with_options(path, &options),
    };
    if destination.len() > 1400 {
        String::new()
    } else {
        destination
    }
}

fn history_return_to(path: &str, query: &str, target: HistoryTarget) -> String {
    let form = crate::http::Form::from_query(query);
    let mut query = url::form_urlencoded::Serializer::new(String::new());
    let number_fields: &[&str] = match target {
        HistoryTarget::List => &["before"],
        HistoryTarget::Compare => &["from", "to"],
        _ => &[],
    };
    for name in number_fields {
        let value = form.get(name);
        if revision_number(value).is_some() {
            query.append_pair(name, value);
        }
    }
    if form.get("preview") == "1" {
        query.append_pair("preview", "1");
    } else if matches!(target, HistoryTarget::Revision(_)) && form.get("restore") == "1" {
        query.append_pair("restore", "1");
    }
    if matches!(target, HistoryTarget::Revision(_))
        && let Some(comment) = canonical_uuid(form.get("comment"))
    {
        query.append_pair("comment", &comment.to_string());
    }
    let query = query.finish();
    if query.is_empty() {
        path.to_owned()
    } else {
        format!("{path}?{query}")
    }
}

#[cfg(test)]
mod route_tests {
    use super::*;

    const SPACE: &str = "/workspace/11111111-1111-1111-1111-111111111111/spaces/22222222-2222-2222-2222-222222222222";
    const SECTION: &str = "33333333-3333-3333-3333-333333333333";

    #[test]
    fn history_login_retains_canonical_read_state() {
        let root = format!("{SPACE}/pages/{SECTION}/revisions");
        for suffix in [
            "",
            "/5",
            "/compare?from=2&to=5&preview=1",
            "?before=4&preview=1",
        ] {
            let url = format!("{root}{suffix}");
            assert_eq!(local_target(&url), url);
        }
        assert_eq!(
            local_target(&format!("{root}/5?preview=1&restore=1")),
            format!("{root}/5?preview=1")
        );
        assert_eq!(local_target(&format!("{root}/5/cover")), "");
        assert_eq!(local_target(&format!("{root}/5/restore")), "");
        for suffix in [
            "/00",
            "/0",
            "/+1",
            "/01",
            "/9223372036854775808",
            "/5/extra",
        ] {
            assert!(Route::parse(&format!("{root}{suffix}")).is_none());
        }
    }

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

    #[test]
    fn cover_routes_require_canonical_ids_and_are_never_login_destinations() {
        let tenant = Uuid::parse_str("11111111-1111-1111-1111-111111111111").unwrap();
        let space = Uuid::parse_str("22222222-2222-2222-2222-222222222222").unwrap();
        let page = Uuid::parse_str("44444444-4444-4444-4444-444444444444").unwrap();
        let file = Uuid::parse_str("aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa").unwrap();
        let upload = page_cover_upload_path(tenant, space, page);
        let image = page_cover_image_path(tenant, space, page, file);

        let upload_route = Route::parse(&upload).expect("cover upload route");
        assert!(upload_route.allows_post());
        assert!(!upload_route.allows_read());
        let image_route = Route::parse(&image).expect("cover image route");
        assert!(image_route.allows_read());
        assert!(!image_route.allows_post());
        assert!(image_route.is_cover());
        assert_eq!(local_target(&image), "");
        assert!(Route::parse(&format!("{upload}/{}", file.to_string().to_uppercase())).is_none());
        assert!(Route::parse(&format!("{image}/extra")).is_none());
    }

    #[test]
    fn attachments_have_distinct_read_and_upload_routes() {
        let root = format!("{SPACE}/pages/{SECTION}");
        let asset = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        let base = format!("{root}/attachments");
        let stage = format!("{base}/drafts/{SECTION}/{asset}");
        let upload = Route::parse(&stage).unwrap();
        assert!(upload.allows_post());
        assert!(!upload.allows_read());
        for path in [
            format!("{base}/{asset}"),
            format!("{base}/{asset}/image"),
            format!("{stage}/image"),
            format!("{root}/revisions/1/attachments/{asset}"),
            format!("{root}/revisions/1/attachments/{asset}/image"),
        ] {
            let route = Route::parse(&path).unwrap();
            assert!(route.allows_read());
            assert!(!route.allows_post());
            assert!(local_target(&path).is_empty());
        }
        assert!(Route::parse(&format!("{stage}/download")).is_none());
        assert!(Route::parse(&format!("{base}/{}", asset.to_uppercase())).is_none());
        assert!(Route::parse(&format!("{root}/revisions/01/attachments/{asset}")).is_none());
    }
}
