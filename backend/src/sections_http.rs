mod cover;
mod editor;
mod history;
mod pages;

pub(crate) use cover::{cover_unauthenticated, cover_unavailable};
pub(crate) use editor::retry_page_edit_before_authorization;
pub(crate) use history::{
    history_api_error, history_wants_json, retry_history_restore_before_authorization,
};
pub(crate) use pages::retry_before_authorization;

use std::time::Duration;

use axum::{
    extract::Request,
    http::{Method, StatusCode},
    response::Response,
};
use tokio::time::timeout;
use uuid::Uuid;

use crate::{
    db::{CreateSection, Page, PageAddress, Principal, Section, SectionTree, Space, Store},
    http::{self, html, plain, redirect},
    sections::{NameForm, PageEditorForm, PageReading, SectionScreen},
    views,
    workspace::{
        CatalogOptions, DocumentsRoute, DocumentsTarget, WorkspaceContext, canonical_uuid,
        new_section_path, page_path, section_form_path, section_origin, section_path,
        sections_path, space_path, with_options,
    },
};

const DENIED: &str = "У вас нет права создавать разделы в этом пространстве.";
const OPEN_FORM: &str = "Откройте форму создания раздела в «Севере».";
const UNAVAILABLE: &str = "Не удалось сохранить раздел. Попробуйте ещё раз.";

// Leave time inside the server-wide deadline to return the submitted form.
const FORM_TIMEOUT: Duration = Duration::from_secs(2);
const SAVE_TIMEOUT: Duration = Duration::from_secs(4);
const RECOVERY_TIMEOUT: Duration = Duration::from_secs(2);

struct DocumentNavigation {
    route: DocumentsRoute,
    options: CatalogOptions,
    from: Option<Uuid>,
}

impl DocumentNavigation {
    fn for_request(route: DocumentsRoute, request: &Request) -> Self {
        let query = request.uri().query().unwrap_or_default();
        Self {
            route,
            options: CatalogOptions::parse(query),
            from: section_origin(query),
        }
    }

    fn page_address(&self, page: Uuid, principal: &Principal) -> PageAddress {
        PageAddress {
            tenant: self.route.tenant,
            space: self.route.space,
            page,
            account: principal.id,
        }
    }

    fn destination(&self, section: Option<Uuid>) -> String {
        let DocumentsRoute { tenant, space, .. } = self.route;
        let path = match section {
            Some(id) => section_path(tenant, space, id),
            None => space_path(tenant, space),
        };
        with_options(&path, &self.options)
    }

    fn form(
        &self,
        from: Option<Uuid>,
        operation_id: Uuid,
        title: &str,
        failure: Option<Failure>,
    ) -> NameForm {
        let DocumentsRoute { tenant, space, .. } = self.route;
        let action = section_form_path(&sections_path(tenant, space), from, &self.options);
        let cancel_to = format!("{}#create-section", self.destination(from));
        NameForm {
            operation_id,
            title: title.to_owned(),
            error: failure.map_or("", |failure| failure.message).to_owned(),
            invalid_name: failure.is_some_and(|failure| failure.invalid_name),
            cancel_to,
            action,
            retry_to: failure
                .filter(|failure| failure.new_form)
                .map(|_| new_section_path(tenant, space, from, &self.options)),
        }
    }
}

struct DocumentsPage {
    context: WorkspaceContext,
    space: Space,
    tree: SectionTree,
}

#[derive(Default)]
struct Selection {
    section: Option<Section>,
    page: Option<Page>,
    reading: Option<PageReading>,
    editor: Option<PageEditorForm>,
}

enum CreationDialog {
    Section(NameForm),
    Page(NameForm),
}

impl DocumentsPage {
    fn section(&self, id: Option<Uuid>) -> Option<Section> {
        let id = id?;
        self.tree
            .sections
            .iter()
            .find(|section| section.id == id)
            .cloned()
    }

    fn select_page(&self, id: Uuid) -> Option<Selection> {
        let page = self.tree.pages.iter().find(|entry| entry.id == id)?.clone();
        Some(Selection {
            section: self.section(Some(page.section_id)),
            page: Some(page),
            ..Selection::default()
        })
    }

    fn render(
        self,
        status: StatusCode,
        selected: Selection,
        dialog: Option<CreationDialog>,
    ) -> Response {
        let (form, page_form) = match dialog {
            Some(CreationDialog::Section(form)) => (Some(form), None),
            Some(CreationDialog::Page(form)) => (None, Some(form)),
            None => (None, None),
        };
        let tree = SectionScreen {
            sections: self.tree.sections,
            pages: self.tree.pages,
            selected: selected.section,
            selected_page: selected.page,
            can_create_section: self.tree.can_create,
            form,
            page_form,
            reading: selected.reading,
            editor: selected.editor,
        };
        html(
            status,
            views::closed_space(&self.context, &self.space, &tree),
        )
    }
}

#[derive(Clone, Copy)]
struct Failure {
    status: StatusCode,
    message: &'static str,
    invalid_name: bool,
    new_form: bool,
}

pub(crate) async fn handle(
    store: &Store,
    route: DocumentsRoute,
    principal: &Principal,
    origin: &str,
    request: Request,
) -> Result<Response, sqlx::Error> {
    let nav = DocumentNavigation::for_request(route, &request);
    match route.target {
        DocumentsTarget::Sections => {
            return Ok(submit(store, &nav, principal, origin, request).await);
        }
        DocumentsTarget::Pages(section) => {
            return Ok(pages::submit(store, &nav, section, principal, origin, request).await);
        }
        DocumentsTarget::EditPage(id) if request.method() == Method::POST => {
            return Ok(editor::submit(store, &nav, id, principal, origin, request).await);
        }
        DocumentsTarget::UploadCover(page) => {
            return cover::upload(store, &nav.page_address(page, principal), origin, request).await;
        }
        DocumentsTarget::CoverImage { page, asset } => {
            return cover::image(store, &nav.page_address(page, principal), asset).await;
        }
        DocumentsTarget::History { page, target } => {
            let response = history::handle(
                store,
                &nav.page_address(page, principal),
                target,
                origin,
                request,
            )
            .await?;
            let outcome = match response.status() {
                StatusCode::OK => "opened",
                StatusCode::SEE_OTHER => "restored",
                StatusCode::NOT_FOUND => "not-found",
                StatusCode::CONFLICT => "conflict",
                StatusCode::SERVICE_UNAVAILABLE => "unavailable",
                _ => "rejected",
            };
            record(principal.id, route, outcome);
            return Ok(response);
        }
        _ => {}
    }
    let Some(page) = load(store, &nav, principal).await? else {
        record(principal.id, route, "not-found");
        return Ok(not_found(route.target));
    };

    let response = match route.target {
        DocumentsTarget::Space => match page.tree.pages.first() {
            Some(first) => redirect(&with_options(
                &page_path(route.tenant, route.space, first.id),
                &nav.options,
            )),
            None => page.render(StatusCode::OK, Selection::default(), None),
        },
        DocumentsTarget::Section(id) => match page.section(Some(id)) {
            Some(section) => page.render(
                StatusCode::OK,
                Selection {
                    section: Some(section),
                    ..Selection::default()
                },
                None,
            ),
            None => {
                record(principal.id, route, "not-found");
                return Ok(not_found(route.target));
            }
        },
        DocumentsTarget::NewSection if !page.tree.can_create => {
            record(principal.id, route, "forbidden");
            return Ok(plain(StatusCode::FORBIDDEN, DENIED));
        }
        DocumentsTarget::NewSection => {
            let selected = page.section(nav.from);
            let form = nav.form(
                selected.as_ref().map(|section| section.id),
                Uuid::new_v4(),
                "",
                None,
            );
            page.render(
                StatusCode::OK,
                Selection {
                    section: selected,
                    ..Selection::default()
                },
                Some(CreationDialog::Section(form)),
            )
        }
        DocumentsTarget::Page(id) | DocumentsTarget::EditPage(id) => {
            return editor::open(store, &nav, id, principal, page).await;
        }
        DocumentsTarget::NewPage(section_id) => {
            let Some(section) = page.section(Some(section_id)) else {
                record(principal.id, route, "not-found");
                return Ok(not_found(route.target));
            };
            if !page.tree.can_create {
                record(principal.id, route, "forbidden");
                return Ok(plain(StatusCode::FORBIDDEN, pages::DENIED));
            }
            let form = pages::creation_form(&nav, section.id, Uuid::new_v4(), "", None);
            page.render(
                StatusCode::OK,
                Selection {
                    section: Some(section),
                    ..Selection::default()
                },
                Some(CreationDialog::Page(form)),
            )
        }
        DocumentsTarget::Sections
        | DocumentsTarget::Pages(_)
        | DocumentsTarget::UploadCover(_)
        | DocumentsTarget::CoverImage { .. }
        | DocumentsTarget::History { .. } => {
            unreachable!("document writes were handled before loading the page")
        }
    };
    record(principal.id, route, "opened");
    Ok(response)
}

async fn load(
    store: &Store,
    nav: &DocumentNavigation,
    principal: &Principal,
) -> Result<Option<DocumentsPage>, sqlx::Error> {
    let (data, _) = store
        .workspace(principal.id, Some(nav.route.tenant))
        .await?;
    let Some(data) = data else { return Ok(None) };
    let Some(space) = data
        .spaces
        .iter()
        .find(|space| space.id == nav.route.space)
        .cloned()
    else {
        return Ok(None);
    };
    let Some(tree) = store
        .sections(nav.route.tenant, nav.route.space, principal.id)
        .await?
    else {
        return Ok(None);
    };
    let context = WorkspaceContext::build(data, principal, nav.options.clone(), false);
    Ok(Some(DocumentsPage {
        context,
        space,
        tree,
    }))
}

async fn submit(
    store: &Store,
    nav: &DocumentNavigation,
    principal: &Principal,
    origin: &str,
    request: Request,
) -> Response {
    let route = nav.route;
    if !http::check_origin(request.headers(), origin) {
        record(principal.id, route, "invalid-origin");
        return plain(StatusCode::FORBIDDEN, OPEN_FORM);
    }
    let form = match timeout(FORM_TIMEOUT, http::read_form(request)).await {
        Ok(Ok(form)) => form,
        Ok(Err(error)) => return http::form_error(error, OPEN_FORM),
        Err(_) => {
            return plain(
                StatusCode::REQUEST_TIMEOUT,
                "Не удалось получить форму вовремя. Попробуйте ещё раз.",
            );
        }
    };
    let title = form.get("name");
    let operation = canonical_uuid(form.get("operation_id"));
    let failure = match operation {
        Some(id) => match timeout(
            SAVE_TIMEOUT,
            store.create_section(route.tenant, route.space, principal.id, id, title),
        )
        .await
        {
            Ok(Ok(CreateSection::Saved(id))) => {
                record(principal.id, route, "saved");
                return redirect(&nav.destination(Some(id)));
            }
            Ok(Ok(CreateSection::MissingSpace)) => {
                record(principal.id, route, "not-found");
                return not_found(route.target);
            }
            Ok(Ok(CreateSection::Forbidden)) => {
                record(principal.id, route, "forbidden");
                return plain(StatusCode::FORBIDDEN, DENIED);
            }
            Ok(Ok(CreateSection::Duplicate)) => Failure {
                status: StatusCode::CONFLICT,
                message: "На верхнем уровне уже есть раздел с таким названием.",
                invalid_name: true,
                new_form: false,
            },
            Ok(Ok(CreateSection::OperationConflict)) => Failure {
                status: StatusCode::CONFLICT,
                message: "Эта отправка уже использована. Откройте новую форму.",
                invalid_name: false,
                new_form: true,
            },
            Ok(Ok(CreateSection::EmptyName)) => Failure {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                message: "Введите название раздела.",
                invalid_name: true,
                new_form: false,
            },
            Ok(Ok(CreateSection::LongName)) => Failure {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                message: "Введите не больше 100 символов.",
                invalid_name: true,
                new_form: false,
            },
            Ok(Err(_)) | Err(_) => Failure {
                status: StatusCode::SERVICE_UNAVAILABLE,
                message: UNAVAILABLE,
                invalid_name: false,
                new_form: false,
            },
        },
        None => Failure {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: "Не удалось подтвердить отправку. Откройте новую форму.",
            invalid_name: false,
            new_form: true,
        },
    };
    let operation = operation.unwrap_or_else(Uuid::new_v4);
    record(principal.id, route, failure.status.as_str());
    match timeout(RECOVERY_TIMEOUT, load(store, nav, principal)).await {
        Ok(Ok(Some(page))) if page.tree.can_create => {
            let selected = page.section(nav.from);
            let form = nav.form(
                selected.as_ref().map(|section| section.id),
                operation,
                title,
                Some(failure),
            );
            page.render(
                failure.status,
                Selection {
                    section: selected,
                    ..Selection::default()
                },
                Some(CreationDialog::Section(form)),
            )
        }
        Ok(Ok(Some(_))) => plain(StatusCode::FORBIDDEN, DENIED),
        Ok(Ok(None)) => not_found(route.target),
        Ok(Err(_)) | Err(_) => {
            let unavailable = Failure {
                status: StatusCode::SERVICE_UNAVAILABLE,
                message: UNAVAILABLE,
                invalid_name: false,
                new_form: false,
            };
            // The origin is a UUID within the same scoped route. A later GET
            // validates access to it, even when the present read is unavailable.
            let form = nav.form(nav.from, operation, title, Some(unavailable));
            html(unavailable.status, views::section_retry(&form))
        }
    }
}

fn not_found(target: DocumentsTarget) -> Response {
    let page = match target {
        DocumentsTarget::Space => views::space_not_found(),
        DocumentsTarget::Page(_)
        | DocumentsTarget::EditPage(_)
        | DocumentsTarget::UploadCover(_)
        | DocumentsTarget::CoverImage { .. }
        | DocumentsTarget::History { .. } => views::page_not_found(),
        _ => views::section_not_found(),
    };
    html(StatusCode::NOT_FOUND, page)
}

fn record(account: Uuid, route: DocumentsRoute, outcome: &str) {
    let event = match route.target {
        DocumentsTarget::Space => "space.open",
        DocumentsTarget::Sections => "section.create",
        DocumentsTarget::Pages(_) => "page.create",
        DocumentsTarget::EditPage(_) => "page.edit",
        DocumentsTarget::Page(_) | DocumentsTarget::NewPage(_) => "page.open",
        DocumentsTarget::History { .. } => "page.history",
        _ => "section.open",
    };
    eprintln!(
        "{event} account={account} tenant={} space={} outcome={outcome}",
        route.tenant, route.space
    );
}
