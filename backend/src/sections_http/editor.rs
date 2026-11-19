use axum::{extract::Request, http::StatusCode, response::Response};
use sha2::{Digest, Sha256};
use tokio::time::timeout;
use uuid::Uuid;

use super::{
    DocumentNavigation, DocumentsPage, FORM_TIMEOUT, RECOVERY_TIMEOUT, SAVE_TIMEOUT, load,
    not_found, record,
};
use crate::{
    db::{
        CoverUpdate, PageContentFormat, PageCover, PageText, PageUpdate, Principal, SavePageText,
        Store,
    },
    http::{self, Form, FormError, html, plain, redirect},
    markdown::render_markdown,
    sections::{CoverView, PageEditConflict, PageEditorForm, PageEditorMode, PageReading},
    text::{markdown_to_text, normalize_line_endings, text_to_markdown},
    views,
    workspace::{
        DocumentsRoute, DocumentsTarget, page_cover_upload_path, page_edit_path, page_path,
        with_options,
    },
};

const MAX_TEXT_CHARS: usize = 30_000;
const MAX_SOURCE_CHARS: usize = 60_000;
// A four-byte code point can take 12 bytes in the submitted form.
const MAX_FORM_BYTES: usize = 1024 * 1024;
const OPEN_FORM: &str = "Откройте редактор страницы в «Севере».";
const DENIED: &str = "У вас нет права сохранять текст этой страницы.";
const UNAVAILABLE: &str = "Не удалось сохранить текст. Попробуйте снова.";
const CONFLICT: &str = "Текст страницы изменился. Сравните его со своим черновиком.";
const COVER_CONFLICT: &str =
    "Страница изменилась. Сравните опубликованный вариант со своим черновиком.";
const INVALID_COVER: &str = "Не удалось выбрать обложку. Выберите её снова.";

struct Submission {
    text: String,
    source: String,
    format: PageContentFormat,
    text_base: Option<String>,
    text_unchanged: bool,
    editor_mode: PageEditorMode,
    plain_post: bool,
    revision: String,
    version: Option<i64>,
    error: Option<&'static str>,
    cover: Option<PageCover>,
    cover_base: Option<PageCover>,
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
        let text_base = form
            .field("textBase")
            .ok()
            .flatten()
            .filter(|token| {
                !plain_post
                    && token.len() == 64
                    && token
                        .bytes()
                        .all(|ch| matches!(ch, b'0'..=b'9' | b'a'..=b'f'))
            })
            .map(str::to_owned);
        let text_unchanged = text_base
            .as_ref()
            .is_some_and(|token| *token == text_fingerprint(format, &source));
        let original_revision = submitted_revision.unwrap_or_else(Some).unwrap_or_default();
        let original_version = submitted_revision.ok().flatten().and_then(parse_revision);
        let (revision, version) = match explicit_base {
            Ok(None) => (original_revision, original_version),
            Ok(Some(value)) if original_version.is_some() && parse_revision(value).is_some() => {
                (value, parse_revision(value))
            }
            _ => (original_revision, None),
        };
        let cover = cover_field(&form, "cover");
        let cover_base = cover_field(&form, "coverBase");
        let cover = match form.field("coverChoice") {
            Ok(None) => cover,
            Ok(Some("published")) if matches!(explicit_base, Ok(Some(_))) => cover.map(|_| None),
            Ok(Some("draft"))
                if matches!(explicit_base, Ok(Some(_))) && matches!(cover, Ok(Some(_))) =>
            {
                cover
            }
            _ => Err(()),
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
        } else if cover.is_err() || cover_base.is_err() {
            Some(INVALID_COVER)
        } else {
            None
        };
        Self {
            text,
            source,
            format,
            text_base,
            text_unchanged,
            editor_mode,
            plain_post,
            revision: revision.to_owned(),
            version,
            error,
            cover: cover.unwrap_or_default(),
            cover_base: cover_base.unwrap_or_default(),
        }
    }
}

impl Submission {
    fn update(&self) -> PageUpdate<'_> {
        if self.text_unchanged {
            PageUpdate::Keep
        } else if self.plain_post {
            PageUpdate::PlainText(&self.text)
        } else {
            PageUpdate::Source(&self.source, self.format)
        }
    }

    fn editor_form(
        self,
        nav: &DocumentNavigation,
        page: Uuid,
        cancel_to: Option<String>,
        error: &'static str,
        conflict: Option<PageEditConflict>,
        can_save: bool,
    ) -> PageEditorForm {
        let route = nav.route;
        // A replaced custom image is no longer accessible, while a preset can still preview the starting state.
        let prior_cover = self
            .cover_base
            .filter(|cover| conflict.is_none() || !matches!(cover, PageCover::Image(_)));
        let cover = self
            .cover
            .or(prior_cover)
            .map(|value| cover_view(route, page, value))
            .or_else(|| conflict.as_ref().map(|current| current.cover.clone()));
        PageEditorForm {
            action: edit_to(nav, page),
            cancel_to,
            text: self.text,
            source: self.source,
            content_format: self.format,
            text_base: self.text_base,
            text_unchanged: self.text_unchanged,
            editor_mode: self.editor_mode,
            revision: self.revision,
            error: Some(error),
            conflict,
            can_save,
            cover,
            cover_base: self.cover_base.map(PageCover::value),
            cover_changed: self.cover.is_some(),
            cover_upload_to: can_save
                .then(|| page_cover_upload_path(route.tenant, route.space, page)),
        }
    }
}

fn text_fingerprint(format: PageContentFormat, source: &str) -> String {
    let mut fingerprint = Sha256::new();
    fingerprint.update(format.as_str());
    fingerprint.update([0]);
    fingerprint.update(source);
    format!("{:x}", fingerprint.finalize())
}

fn cover_field(form: &Form, name: &str) -> Result<Option<PageCover>, ()> {
    match form.field(name) {
        Ok(None) => Ok(None),
        Ok(Some(raw)) => PageCover::parse(raw).map(Some).ok_or(()),
        Err(_) => Err(()),
    }
}

fn cover_view(route: DocumentsRoute, page: Uuid, value: PageCover) -> CoverView {
    CoverView::new(route.tenant, route.space, page, value)
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
    let Some(current) = store.page_text(&nav.page_address(id, principal)).await? else {
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
        let text_base = text_fingerprint(current.format, &source);
        selected.editor = Some(PageEditorForm {
            action: edit_to(nav, id),
            cancel_to: Some(return_to(nav, id)),
            text,
            source,
            content_format: current.format,
            text_base: Some(text_base),
            text_unchanged: true,
            editor_mode: PageEditorMode::Visual,
            revision: current.content_revision.to_string(),
            error: None,
            conflict: None,
            can_save: true,
            cover: Some(cover_view(route, id, current.cover)),
            cover_base: Some(current.cover.value()),
            cover_changed: false,
            cover_upload_to: Some(page_cover_upload_path(route.tenant, route.space, id)),
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
            cover: cover_view(route, id, current.cover),
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
        let address = nav.page_address(id, principal);
        let cover = input.cover.map_or(CoverUpdate::Keep, CoverUpdate::Set);
        match timeout(
            SAVE_TIMEOUT,
            store.save_page(&address, revision, input.update(), cover),
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
            Ok(Ok(SavePageText::InvalidCover)) => SaveFailure::Invalid(INVALID_COVER),
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
            if input.cover.is_some() || input.cover_base.is_some_and(|base| base != current.cover) {
                COVER_CONFLICT
            } else {
                CONFLICT
            },
            Some(PageEditConflict {
                text: normalize_line_endings(&current.source).into_owned(),
                revision: current.content_revision.to_string(),
                cover: cover_view(route, id, current.cover),
            }),
        ),
    };
    let can_save = status != StatusCode::FORBIDDEN && input.version.is_some();
    selected.editor =
        Some(input.editor_form(nav, id, Some(return_to(nav, id)), error, conflict, can_save));
    record(principal.id, route, status.as_str());
    page.render(status, selected, None)
}

fn retry(nav: &DocumentNavigation, id: Uuid, input: Submission) -> Response {
    let error = if input.cover.is_some() {
        "Не удалось сохранить страницу. Попробуйте снова."
    } else {
        UNAVAILABLE
    };
    let form = input.editor_form(nav, id, None, error, None, true);
    html(
        StatusCode::SERVICE_UNAVAILABLE,
        views::page_edit_retry(&form),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_untouched_modern_draft_keeps_the_actual_text_across_rebase() {
        let base = text_fingerprint(PageContentFormat::Markdown, "# Начало\n\nДокумент  \n");
        let post = |text: &str, format: Option<&str>, token: &str| {
            let mut form = url::form_urlencoded::Serializer::new(String::new());
            form.append_pair("text", text)
                .append_pair("revision", "1")
                .append_pair("baseRevision", "2")
                .append_pair("textBase", token)
                .append_pair("cover", "orbit");
            if let Some(format) = format {
                form.append_pair("contentFormat", format);
            }
            Submission::from(Form::from_query(&form.finish()))
        };

        let same_source = "# Начало\r\n\r\nДокумент  \r\n";
        let untouched = post(same_source, Some("markdown"), &base);
        assert!(untouched.text_unchanged);
        assert_eq!(untouched.text_base.as_deref(), Some(base.as_str()));
        assert!(matches!(untouched.update(), PageUpdate::Keep));

        for changed in [
            post("# Начало\n\nДокумент\n", Some("markdown"), &base),
            post(same_source, Some("literal"), &base),
            post(same_source, Some("markdown"), &base.to_uppercase()),
            post(same_source, None, &base),
        ] {
            assert!(!changed.text_unchanged);
            assert!(!matches!(changed.update(), PageUpdate::Keep));
        }
    }

    #[test]
    fn an_untouched_cover_conflict_keeps_the_prior_preset_or_uses_a_reachable_image() {
        let page = Uuid::new_v4();
        let route = DocumentsRoute {
            tenant: Uuid::new_v4(),
            space: Uuid::new_v4(),
            target: DocumentsTarget::EditPage(page),
        };
        let nav = DocumentNavigation::for_request(route, &Request::new(axum::body::Body::empty()));
        let conflict = || PageEditConflict {
            text: String::new(),
            revision: "2".into(),
            cover: cover_view(route, page, PageCover::Grid),
        };
        for (base, expected) in [
            ("orbit".to_owned(), "orbit"),
            ("none".to_owned(), "none"),
            (PageCover::Image(Uuid::new_v4()).value(), "grid"),
        ] {
            let input = Submission::from(Form::from_query(&format!(
                "revision=1&text=a&coverBase={base}"
            )));
            let form = input.editor_form(&nav, page, None, COVER_CONFLICT, Some(conflict()), true);
            assert_eq!(form.cover.unwrap().value, expected);
            assert_eq!(form.conflict.unwrap().cover.value, "grid");
            assert!(!form.cover_changed);
        }
    }

    #[test]
    fn cover_submission_distinguishes_unmodified_and_removed() {
        let unchanged = Submission::from(Form::from_query("revision=1&text=a&coverBase=orbit"));
        assert_eq!(unchanged.cover, None);
        assert_eq!(unchanged.cover_base, Some(PageCover::Orbit));
        assert!(unchanged.error.is_none());

        let removed = Submission::from(Form::from_query(
            "revision=1&text=a&coverBase=orbit&cover=none",
        ));
        assert_eq!(removed.cover, Some(PageCover::None));
        assert_eq!(removed.cover_base, Some(PageCover::Orbit));
        assert!(removed.error.is_none());
    }

    #[test]
    fn native_rebase_can_explicitly_preserve_the_published_cover() {
        let form = Submission::from(Form::from_query(
            "revision=1&baseRevision=2&text=a&coverBase=aurora&cover=none&coverChoice=published",
        ));
        assert!(form.error.is_none());
        assert_eq!(form.version, Some(2));
        assert_eq!(form.cover, None);
        assert_eq!(form.cover_base, Some(PageCover::Aurora));
    }

    #[test]
    fn duplicate_or_invalid_cover_fields_never_drop_the_submitted_source() {
        for query in [
            "revision=1&text=a&cover=orbit&cover=none",
            "revision=1&text=a&coverBase=invalid",
            "revision=1&text=a&coverChoice=published",
        ] {
            let form = Submission::from(Form::from_query(query));
            assert_eq!(form.error, Some(INVALID_COVER));
            assert_eq!(form.source, "a");
        }
    }
}
