use std::time::Duration;

use axum::{extract::Request, http::StatusCode, response::Response};
use tokio::time::timeout;
use uuid::Uuid;

use crate::{
    db::{CreateSection, Principal, Section, SectionTree, Space, Store},
    http::{self, html, plain, redirect},
    sections::{SectionForm, SectionScreen},
    views,
    workspace::{
        CatalogOptions, DocumentsRoute, DocumentsTarget, WorkspaceContext, canonical_uuid,
        new_section_path, section_form_path, section_origin, section_path, sections_path,
        space_path, with_options,
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
    ) -> SectionForm {
        let DocumentsRoute { tenant, space, .. } = self.route;
        let action = section_form_path(&sections_path(tenant, space), from, &self.options);
        let cancel_to = format!("{}#create-section", self.destination(from));
        SectionForm {
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

impl DocumentsPage {
    fn section(&self, id: Option<Uuid>) -> Option<Section> {
        let id = id?;
        self.tree
            .sections
            .iter()
            .find(|section| section.id == id)
            .cloned()
    }

    fn render(
        self,
        status: StatusCode,
        selected: Option<Section>,
        form: Option<SectionForm>,
    ) -> Response {
        let tree = SectionScreen {
            sections: self.tree.sections,
            selected,
            can_create_section: self.tree.can_create,
            form,
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
    if route.target == DocumentsTarget::Sections {
        return Ok(submit(store, &nav, principal, origin, request).await);
    }
    let Some(page) = load(store, &nav, principal).await? else {
        record(principal.id, route, "not-found");
        return Ok(not_found(route.target));
    };

    let response = match route.target {
        DocumentsTarget::Space => page.render(StatusCode::OK, None, None),
        DocumentsTarget::Section(id) => match page.section(Some(id)) {
            Some(selected) => page.render(StatusCode::OK, Some(selected), None),
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
            page.render(StatusCode::OK, selected, Some(form))
        }
        DocumentsTarget::Sections => {
            unreachable!("section writes were handled before loading the page")
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
            page.render(failure.status, selected, Some(form))
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
    let page = if target == DocumentsTarget::Space {
        views::space_not_found()
    } else {
        views::section_not_found()
    };
    html(StatusCode::NOT_FOUND, page)
}

fn record(account: Uuid, route: DocumentsRoute, outcome: &str) {
    let event = match route.target {
        DocumentsTarget::Space => "space.open",
        DocumentsTarget::Sections => "section.create",
        _ => "section.open",
    };
    eprintln!(
        "{event} account={account} tenant={} space={} outcome={outcome}",
        route.tenant, route.space
    );
}
