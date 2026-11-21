use axum::{extract::Request, http::StatusCode, response::Response};
use tokio::time::timeout;
use uuid::Uuid;

use super::{
    CreationDialog, DocumentNavigation, FORM_TIMEOUT, Failure, RECOVERY_TIMEOUT, SAVE_TIMEOUT,
    Selection, load, not_found, record,
};
use crate::{
    db::{CreatePage, CreatePageInput, Principal, Store},
    http::{self, Form, FormError, html, plain, redirect},
    page_templates::{self, PageTemplate},
    sections::{NameForm, with_parent},
    views,
    workspace::{DocumentsRoute, canonical_uuid, page_path, pages_path, with_options},
};

pub(super) const DENIED: &str = "У вас нет права создавать страницы в этом разделе.";
const OPEN_FORM: &str = "Откройте форму создания страницы в «Севере».";
const INVALID_TEMPLATE: &str = "Выберите один доступный шаблон.";
const UNAVAILABLE: Failure = Failure {
    status: StatusCode::SERVICE_UNAVAILABLE,
    message: "Не удалось сохранить страницу. Попробуйте ещё раз.",
    invalid_name: false,
    new_form: false,
};

enum CreationFormError {
    InvalidOrigin,
    Malformed(FormError),
    Timeout,
}

impl CreationFormError {
    fn response(self) -> Response {
        match self {
            Self::InvalidOrigin => plain(StatusCode::FORBIDDEN, OPEN_FORM),
            Self::Malformed(error) => http::form_error(error, OPEN_FORM),
            Self::Timeout => plain(
                StatusCode::REQUEST_TIMEOUT,
                "Не удалось получить форму вовремя. Попробуйте ещё раз.",
            ),
        }
    }
}

async fn read_creation_form(origin: &str, request: Request) -> Result<Form, CreationFormError> {
    if !http::check_origin(request.headers(), origin) {
        return Err(CreationFormError::InvalidOrigin);
    }
    match timeout(FORM_TIMEOUT, http::read_form(request)).await {
        Ok(form) => form.map_err(CreationFormError::Malformed),
        Err(_) => Err(CreationFormError::Timeout),
    }
}

fn submitted_template(form: &Form) -> Option<&'static PageTemplate> {
    let key = form.field("template").ok()?;
    page_templates::find(key.unwrap_or(""))
}

pub(crate) async fn retry_before_authorization(
    route: DocumentsRoute,
    section: Uuid,
    origin: &str,
    request: Request,
) -> Response {
    let nav = DocumentNavigation::for_request(route, &request);
    match read_creation_form(origin, request).await {
        Ok(input) => match submitted_template(&input) {
            Some(template) => retry(
                &nav,
                section,
                canonical_uuid(input.get("operation_id")),
                input.get("name"),
                template,
            ),
            None => plain(StatusCode::UNPROCESSABLE_ENTITY, INVALID_TEMPLATE),
        },
        Err(error) => error.response(),
    }
}

pub(super) fn creation_form(
    nav: &DocumentNavigation,
    section: Uuid,
    operation_id: Uuid,
    title: &str,
    template: &'static PageTemplate,
    failure: Option<Failure>,
) -> NameForm {
    let parent = nav.parent.ok().flatten();
    let cancel_to = parent.map_or_else(
        || nav.destination(Some(section)),
        |id| {
            with_options(
                &page_path(nav.route.tenant, nav.route.space, id),
                &nav.options,
            )
        },
    );
    NameForm {
        template: Some(template),
        operation_id,
        title: title.to_owned(),
        error: failure.map_or("", |value| value.message).to_owned(),
        invalid_name: failure.is_some_and(|value| value.invalid_name),
        cancel_to: format!(
            "{cancel_to}#{}",
            if parent.is_some() {
                "create-child-page"
            } else {
                "create-page"
            }
        ),
        action: with_parent(
            &with_options(
                &pages_path(nav.route.tenant, nav.route.space, section),
                &nav.options,
            ),
            parent,
        ),
        retry_to: None,
    }
}

pub(super) async fn submit(
    store: &Store,
    nav: &DocumentNavigation,
    section_id: Uuid,
    principal: &Principal,
    origin: &str,
    request: Request,
) -> Response {
    let route = nav.route;
    let form = match read_creation_form(origin, request).await {
        Ok(form) => form,
        Err(error) => {
            if matches!(error, CreationFormError::InvalidOrigin) {
                record(principal.id, route, "invalid-origin");
            }
            return error.response();
        }
    };
    let title = form.get("name");
    let operation = canonical_uuid(form.get("operation_id"));
    let template = submitted_template(&form);
    let failure = match (operation, template) {
        (_, None) => Failure {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: INVALID_TEMPLATE,
            invalid_name: false,
            new_form: false,
        },
        (Some(id), Some(template)) => match timeout(
            SAVE_TIMEOUT,
            store.create_page_from_template(
                CreatePageInput {
                    tenant_id: route.tenant,
                    space_id: route.space,
                    section_id,
                    account_id: principal.id,
                    operation_id: id,
                    title,
                },
                nav.parent.ok().flatten(),
                template,
            ),
        )
        .await
        {
            Ok(Ok(CreatePage::Saved(id))) => {
                record(principal.id, route, "saved");
                return redirect(&with_options(
                    &page_path(route.tenant, route.space, id),
                    &nav.options,
                ));
            }
            Ok(Ok(CreatePage::MissingSection)) => {
                record(principal.id, route, "not-found");
                return not_found(route.target);
            }
            Ok(Ok(CreatePage::Forbidden)) => {
                record(principal.id, route, "forbidden");
                return plain(StatusCode::FORBIDDEN, DENIED);
            }
            Ok(Ok(CreatePage::Duplicate)) => Failure {
                status: StatusCode::CONFLICT,
                message: "В этом разделе уже есть страница с таким названием.",
                invalid_name: true,
                new_form: false,
            },
            Ok(Ok(CreatePage::OperationConflict)) => Failure {
                status: StatusCode::CONFLICT,
                message: "Эта отправка уже использована. Проверьте название и повторите.",
                invalid_name: false,
                new_form: true,
            },
            Ok(Ok(CreatePage::EmptyName)) => Failure {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                message: "Введите название страницы.",
                invalid_name: true,
                new_form: false,
            },
            Ok(Ok(CreatePage::TooDeep)) => Failure {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                message: "Достигнута максимальная вложенность. Выберите уровень выше.",
                invalid_name: false,
                new_form: false,
            },
            Ok(Ok(CreatePage::InvalidName)) => Failure {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                message: "Введите название в одну строку без управляющих символов.",
                invalid_name: true,
                new_form: false,
            },
            Ok(Ok(CreatePage::LongName)) => Failure {
                status: StatusCode::UNPROCESSABLE_ENTITY,
                message: "Введите не больше 100 символов.",
                invalid_name: true,
                new_form: false,
            },
            Ok(Err(_)) | Err(_) => UNAVAILABLE,
        },
        (None, Some(_)) => Failure {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: "Не удалось подтвердить отправку. Повторите с этой формой.",
            invalid_name: false,
            new_form: true,
        },
    };

    let template = template.unwrap_or(&page_templates::EMPTY);
    record(principal.id, route, failure.status.as_str());
    match timeout(RECOVERY_TIMEOUT, load(store, nav, principal)).await {
        Ok(Ok(Some(page))) => {
            let Some(section) = page.section(Some(section_id)) else {
                return not_found(route.target);
            };
            if !page.tree.can_create {
                return plain(StatusCode::FORBIDDEN, DENIED);
            }
            let operation = operation
                .filter(|_| !failure.new_form)
                .unwrap_or_else(Uuid::new_v4);
            let form = creation_form(nav, section_id, operation, title, template, Some(failure));
            let selected_page = nav.parent.ok().flatten().and_then(|parent| {
                page.tree
                    .pages
                    .iter()
                    .find(|item| item.id == parent)
                    .cloned()
            });
            page.render(
                failure.status,
                Selection {
                    section: Some(section),
                    page: selected_page,
                    ..Selection::default()
                },
                Some(CreationDialog::Page(form)),
                None,
            )
        }
        Ok(Ok(None)) => not_found(route.target),
        Ok(Err(_)) | Err(_) => retry(nav, section_id, operation, title, template),
    }
}

fn retry(
    nav: &DocumentNavigation,
    section_id: Uuid,
    operation: Option<Uuid>,
    title: &str,
    template: &'static PageTemplate,
) -> Response {
    let form = creation_form(
        nav,
        section_id,
        operation.unwrap_or_else(Uuid::new_v4),
        title,
        template,
        Some(UNAVAILABLE),
    );
    html(UNAVAILABLE.status, views::page_retry(&form))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_template_selection_is_single_valued_and_optional() {
        for (query, expected) in [
            ("", Some("")),
            ("template=", Some("")),
            ("template=spec-v1", Some("spec-v1")),
            ("template=unavailable", None),
            ("template=spec-v1&template=spec-v1", None),
            ("template=&template=spec-v1", None),
        ] {
            assert_eq!(submitted_template(&Form::from_query(query)).map(|item| item.key), expected);
        }
    }
}
