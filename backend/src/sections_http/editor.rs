use axum::{extract::Request, http::StatusCode, response::Response};
use tokio::time::timeout;
use uuid::Uuid;

use super::{
    DocumentNavigation, DocumentsPage, FORM_TIMEOUT, RECOVERY_TIMEOUT, SAVE_TIMEOUT, load,
    not_found, record,
};
use crate::{
    db::{PageAddress, PageContentFormat, PageText, PageUpdate, Principal, SavePageText, Store},
    http::{self, Form, FormError, html, plain, redirect},
    markdown::render_markdown,
    sections::{PageEditConflict, PageEditorForm, PageEditorMode, PageReading},
    text::{markdown_to_text, normalize_line_endings, text_to_markdown},
    views,
    workspace::{DocumentsRoute, DocumentsTarget, page_edit_path, page_path, with_options},
};

const MAX_TEXT_CHARS: usize = 30_000;
const MAX_SOURCE_CHARS: usize = 60_000;
// A four-byte code point can take 12 bytes in the submitted form.
const MAX_FORM_BYTES: usize = 1024 * 1024;
const OPEN_FORM: &str = "Откройте редактор страницы в «Севере».";
const DENIED: &str = "У вас нет права сохранять текст этой страницы.";
const UNAVAILABLE: &str = "Не удалось сохранить текст. Попробуйте снова.";
const CONFLICT: &str = "Текст страницы изменился. Сравните его со своим черновиком.";

struct Submission {
    text: String,
    source: String,
    format: PageContentFormat,
    editor_mode: PageEditorMode,
    plain_post: bool,
    revision: String,
    version: Option<i64>,
    error: Option<&'static str>,
}

impl From<Form> for Submission {
    fn from(form: Form) -> Self {
        let submitted_text = form.field("text");
        let submitted_format = form.field("contentFormat");
        let submitted_mode = form.field("editorMode");
        let submitted_revision = form.field("revision");
        let explicit_base = form.field("baseRevision");
        let submitted = submitted_text.unwrap_or_else(Some).unwrap_or_default();
        let submitted = normalize_line_endings(submitted);
        let plain_post = matches!(submitted_format, Ok(None));
        let (format, valid_format) = match submitted_format {
            Ok(None | Some("literal")) => (PageContentFormat::Literal, true),
            Ok(Some("markdown")) => (PageContentFormat::Markdown, true),
            _ => (PageContentFormat::Literal, false),
        };
        let (editor_mode, valid_mode) = match submitted_mode {
            Ok(None | Some("visual")) => (PageEditorMode::Visual, true),
            Ok(Some("source")) => (PageEditorMode::Source, true),
            _ => (PageEditorMode::Visual, false),
        };
        let (source, text) = if plain_post {
            (text_to_markdown(&submitted), submitted.into_owned())
        } else {
            let text = match format {
                PageContentFormat::Literal => markdown_to_text(&submitted),
                PageContentFormat::Markdown => submitted.to_string(),
            };
            (submitted.into_owned(), text)
        };
        let original_revision = submitted_revision.unwrap_or_else(Some).unwrap_or_default();
        let original_version = submitted_revision.ok().flatten().and_then(parse_revision);
        let (revision, version) = match explicit_base {
            Ok(None) => (original_revision, original_version),
            Ok(Some(value)) if original_version.is_some() && parse_revision(value).is_some() => {
                (value, parse_revision(value))
            }
            _ => (original_revision, None),
        };
        let error = if version.is_none() {
            Some("Не удалось проверить версию. Скопируйте черновик и откройте редактор снова.")
        } else if !matches!(submitted_text, Ok(Some(_)))
            || source.contains('\0')
            || !valid_format
            || !valid_mode
        {
            Some("Не удалось прочитать текст страницы. Проверьте введённый текст.")
        } else if !plain_post && source.chars().count() > MAX_SOURCE_CHARS {
            Some("Исходник не должен превышать 60 000 символов.")
        } else if format == PageContentFormat::Literal && text.chars().count() > MAX_TEXT_CHARS {
            Some("Текст не должен превышать 30 000 символов.")
        } else {
            None
        };
        Self {
            text,
            source,
            format,
            editor_mode,
            plain_post,
            revision: revision.to_owned(),
            version,
            error,
        }
    }
}

impl Submission {
    fn update(&self) -> PageUpdate<'_> {
        if self.plain_post {
            PageUpdate::PlainText(&self.text)
        } else {
            PageUpdate::Source(&self.source, self.format)
        }
    }

    fn editor_form(
        self,
        action: String,
        cancel_to: Option<String>,
        error: &'static str,
        conflict: Option<PageEditConflict>,
        can_save: bool,
    ) -> PageEditorForm {
        PageEditorForm {
            action,
            cancel_to,
            text: self.text,
            source: self.source,
            content_format: self.format,
            editor_mode: self.editor_mode,
            revision: self.revision,
            error: Some(error),
            conflict,
            can_save,
        }
    }
}

fn parse_revision(value: &str) -> Option<i64> {
    let revision = value.parse::<i64>().ok()?;
    (revision > 0).then_some(revision)
}

enum RequestError {
    Origin,
    Form(FormError),
    Timeout,
}

impl RequestError {
    fn response(self) -> Response {
        match self {
            Self::Origin => plain(StatusCode::FORBIDDEN, OPEN_FORM),
            Self::Form(error) => http::form_error(error, OPEN_FORM),
            Self::Timeout => plain(
                StatusCode::REQUEST_TIMEOUT,
                "Не удалось получить форму вовремя. Попробуйте ещё раз.",
            ),
        }
    }
}

async fn read_submission(origin: &str, request: Request) -> Result<Submission, RequestError> {
    if !http::check_origin(request.headers(), origin) {
        return Err(RequestError::Origin);
    }
    match timeout(
        FORM_TIMEOUT,
        http::read_form_bounded(request, MAX_FORM_BYTES),
    )
    .await
    {
        Ok(form) => form.map(Submission::from).map_err(RequestError::Form),
        Err(_) => Err(RequestError::Timeout),
    }
}

pub(crate) async fn retry_page_edit_before_authorization(
    route: DocumentsRoute,
    id: Uuid,
    origin: &str,
    request: Request,
) -> Response {
    let nav = DocumentNavigation::for_request(route, &request);
    match read_submission(origin, request).await {
        Ok(input) => retry(&nav, id, input),
        Err(error) => error.response(),
    }
}

fn address(nav: &DocumentNavigation, id: Uuid, principal: &Principal) -> PageAddress {
    PageAddress {
        tenant: nav.route.tenant,
        space: nav.route.space,
        page: id,
        account: principal.id,
    }
}

fn edit_to(nav: &DocumentNavigation, id: Uuid) -> String {
    with_options(
        &page_edit_path(nav.route.tenant, nav.route.space, id),
        &nav.options,
    )
}

fn return_to(nav: &DocumentNavigation, id: Uuid) -> String {
    let path = page_path(nav.route.tenant, nav.route.space, id);
    format!("{}#edit-page", with_options(&path, &nav.options))
}

pub(super) async fn open(
    store: &Store,
    nav: &DocumentNavigation,
    id: Uuid,
    principal: &Principal,
    page: DocumentsPage,
) -> Result<Response, sqlx::Error> {
    let route = nav.route;
    let Some(mut selected) = page.select_page(id) else {
        record(principal.id, route, "not-found");
        return Ok(not_found(route.target));
    };
    let Some(current) = store.page_text(&address(nav, id, principal)).await? else {
        record(principal.id, route, "not-found");
        return Ok(not_found(route.target));
    };
    if matches!(route.target, DocumentsTarget::EditPage(_)) {
        if !current.can_edit {
            record(principal.id, route, "forbidden");
            return Ok(plain(StatusCode::FORBIDDEN, DENIED));
        }
        let source = normalize_line_endings(&current.source).into_owned();
        let text = match current.format {
            PageContentFormat::Literal => markdown_to_text(&source),
            PageContentFormat::Markdown => source.clone(),
        };
        selected.editor = Some(PageEditorForm {
            action: edit_to(nav, id),
            cancel_to: Some(return_to(nav, id)),
            text,
            source,
            content_format: current.format,
            editor_mode: PageEditorMode::Visual,
            revision: current.content_revision.to_string(),
            error: None,
            conflict: None,
            can_save: true,
        });
    } else {
        let (text, rendered_html) = match current.format {
            PageContentFormat::Literal => (markdown_to_text(&current.source), None),
            PageContentFormat::Markdown => {
                let rendered = render_markdown(&current.source);
                (current.source, Some(rendered))
            }
        };
        selected.reading = Some(PageReading {
            text,
            rendered_html,
            edit_to: current.can_edit.then(|| edit_to(nav, id)),
        });
    }
    record(principal.id, route, "opened");
    Ok(page.render(StatusCode::OK, selected, None))
}

enum SaveFailure {
    Invalid(&'static str),
    Forbidden,
    Conflict(PageText),
}

pub(super) async fn submit(
    store: &Store,
    nav: &DocumentNavigation,
    id: Uuid,
    principal: &Principal,
    origin: &str,
    request: Request,
) -> Response {
    let route = nav.route;
    let input = match read_submission(origin, request).await {
        Ok(input) => input,
        Err(error) => {
            if matches!(error, RequestError::Origin) {
                record(principal.id, route, "invalid-origin");
            }
            return error.response();
        }
    };
    let failure = if let Some(error) = input.error {
        SaveFailure::Invalid(error)
    } else {
        let revision = input
            .version
            .expect("validated submission includes a version");
        let address = address(nav, id, principal);
        match timeout(
            SAVE_TIMEOUT,
            store.save_page_text(&address, revision, input.update()),
        )
        .await
        {
            Ok(Ok(SavePageText::Saved)) => {
                record(principal.id, route, "saved");
                return redirect(&return_to(nav, id));
            }
            Ok(Ok(SavePageText::Missing)) => {
                record(principal.id, route, "not-found");
                return not_found(route.target);
            }
            Ok(Ok(SavePageText::Forbidden)) => SaveFailure::Forbidden,
            Ok(Ok(SavePageText::Conflict(current))) => SaveFailure::Conflict(current),
            Ok(Err(_)) | Err(_) => {
                record(principal.id, route, "unavailable");
                return retry(nav, id, input);
            }
        }
    };
    render_failure(store, nav, id, principal, input, failure).await
}

async fn render_failure(
    store: &Store,
    nav: &DocumentNavigation,
    id: Uuid,
    principal: &Principal,
    input: Submission,
    mut failure: SaveFailure,
) -> Response {
    let route = nav.route;
    let workspace = match timeout(RECOVERY_TIMEOUT, load(store, nav, principal)).await {
        Ok(Ok(workspace)) => workspace,
        Ok(Err(_)) | Err(_) => {
            record(principal.id, route, "unavailable");
            return retry(nav, id, input);
        }
    };
    let Some((page, mut selected)) =
        workspace.and_then(|page| page.select_page(id).map(|selection| (page, selection)))
    else {
        record(principal.id, route, "not-found");
        return not_found(route.target);
    };
    if !page.tree.can_create {
        failure = SaveFailure::Forbidden;
    }
    let (status, error, conflict) = match failure {
        SaveFailure::Invalid(error) => (StatusCode::UNPROCESSABLE_ENTITY, error, None),
        SaveFailure::Forbidden => (StatusCode::FORBIDDEN, DENIED, None),
        SaveFailure::Conflict(current) => (
            StatusCode::CONFLICT,
            CONFLICT,
            Some(PageEditConflict {
                text: normalize_line_endings(&current.source).into_owned(),
                revision: current.content_revision.to_string(),
            }),
        ),
    };
    let can_save = status != StatusCode::FORBIDDEN && input.version.is_some();
    selected.editor = Some(input.editor_form(
        edit_to(nav, id),
        Some(return_to(nav, id)),
        error,
        conflict,
        can_save,
    ));
    record(principal.id, route, status.as_str());
    page.render(status, selected, None)
}

fn retry(nav: &DocumentNavigation, id: Uuid, input: Submission) -> Response {
    let form = input.editor_form(edit_to(nav, id), None, UNAVAILABLE, None, true);
    html(
        StatusCode::SERVICE_UNAVAILABLE,
        views::page_edit_retry(&form),
    )
}
