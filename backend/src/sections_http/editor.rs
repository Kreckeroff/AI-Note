use axum::{
    extract::Request,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};
use tokio::time::timeout;
use uuid::Uuid;

use super::{
    DocumentNavigation, DocumentsPage, FORM_TIMEOUT, RECOVERY_TIMEOUT, SAVE_TIMEOUT, attachments,
    not_found, record,
};
use crate::{
    db::{
        Attachment, AttachmentPatch, CoeditPublication, CoverUpdate, PageAddress,
        PageContentFormat, PageCover, PageUpdate, Principal, SavePageText, Store,
        document_finding::BacklinkCandidate, tasks::TaskStatus,
    },
    http::{self, Form, FormError, html, html_with_external_frames, plain, redirect},
    identity::session_digest,
    markdown::render_document,
    sections::{
        CoeditRetry, CoverView, PageAttachmentView, PageEditConflict, PageEditorForm,
        PageEditorMode, PageReading, RelatedDocument,
    },
    tasks_http::task_path,
    text::{markdown_to_text, normalize_line_endings, text_to_markdown},
    views,
    workspace::{
        DocumentsRoute, DocumentsTarget, attachment_draft_path, canonical_uuid,
        page_cover_upload_path, page_edit_path, page_history_path, page_path, with_options,
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
const COEDIT_CONFLICT: &str =
    "Общий черновик изменился. Сверьте свой текст с текущим черновиком перед публикацией.";
const COEDIT_LEASE: &str =
    "Допуск к общему черновику обновился. Проверьте правки в новом сеансе редактирования.";
const COEDIT_OPERATION: &str =
    "Эта попытка сохранения уже связана с другой версией. Проверьте текущий черновик.";
const COVER_CONFLICT: &str =
    "Страница изменилась. Сравните опубликованный вариант со своим черновиком.";
const INVALID_COVER: &str = "Не удалось выбрать обложку. Выберите её снова.";
const INVALID_ATTACHMENTS: &str =
    "Проверьте вложения и их размер. Можно сохранить до 20 файлов общим размером до 100 МБ.";
const ATTACHMENT_REFERENCES: &str = "У удаляемого файла остались ссылки или вставки. Уберите их из текста и сохраните страницу снова.";
const ATTACHMENT_RETIRED: &str =
    "Один из файлов удалён в другой редакции. Уберите ссылки на него или прикрепите файл снова.";
const FILE_RECOVERY_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(700);

struct AttachmentChanges {
    draft: Uuid,
    add: Vec<Uuid>,
    remove: Vec<Uuid>,
}

impl AttachmentChanges {
    fn parse(form: &Form) -> Result<Option<Self>, ()> {
        let draft = form.field("attachmentDraft").map_err(|_| ())?;
        let add = form.field("attachmentAdd").map_err(|_| ())?;
        let remove = form.field("attachmentRemove").map_err(|_| ())?;
        if draft.is_none() && add.is_none() && remove.is_none() {
            return Ok(None);
        }
        let draft = draft.and_then(canonical_uuid).ok_or(())?;
        let add = attachment_ids(add.unwrap_or_default())?;
        let remove = attachment_ids(remove.unwrap_or_default())?;
        if add.iter().any(|id| remove.contains(id)) {
            return Err(());
        }
        Ok(Some(Self { draft, add, remove }))
    }

    fn patch(&self) -> Option<AttachmentPatch<'_>> {
        (!self.add.is_empty() || !self.remove.is_empty()).then_some(AttachmentPatch {
            draft: self.draft,
            add: &self.add,
            remove: &self.remove,
        })
    }
}

fn attachment_ids(value: &str) -> Result<Vec<Uuid>, ()> {
    if value.is_empty() {
        return Ok(Vec::new());
    }
    let mut result = Vec::new();
    for value in value.split(',') {
        let id = canonical_uuid(value).ok_or(())?;
        if result.len() == 20 || result.contains(&id) {
            return Err(());
        }
        result.push(id);
    }
    Ok(result)
}

fn coedit_fields(form: &Form) -> Result<Option<CoeditRetry>, ()> {
    let lease = form.field("coedit_lease").map_err(|_| ())?;
    let version = form.field("coedit_version").map_err(|_| ())?;
    let operation = form.field("coedit_operation").map_err(|_| ())?;
    match (lease, version, operation) {
        (None, None, None) => Ok(None),
        (Some(lease), Some(version), Some(operation)) => {
            let parsed = version.parse::<i64>().map_err(|_| ())?;
            if parsed < 0 || parsed.to_string() != version {
                return Err(());
            }
            Ok(Some(CoeditRetry {
                lease: canonical_uuid(lease).ok_or(())?,
                version: version.to_owned(),
                operation: canonical_uuid(operation).ok_or(())?,
            }))
        }
        _ => Err(()),
    }
}

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
    attachments: Option<AttachmentChanges>,
    coedit: Option<CoeditRetry>,
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
        let attachments = AttachmentChanges::parse(&form);
        let coedit = coedit_fields(&form);
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
        } else if attachments.is_err() {
            Some(INVALID_ATTACHMENTS)
        } else if coedit.is_err() || plain_post && coedit.as_ref().is_ok_and(Option::is_some) {
            Some(COEDIT_LEASE)
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
            attachments: attachments.unwrap_or_default(),
            coedit: coedit.unwrap_or_default(),
        }
    }
}

impl Submission {
    fn update(&self) -> PageUpdate<'_> {
        if self.coedit.is_some() {
            PageUpdate::Source(&self.source, self.format)
        } else if self.text_unchanged {
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
        let attachment_draft = self
            .attachments
            .as_ref()
            .map(|change| change.draft)
            .or_else(|| (can_save && cancel_to.is_some()).then(Uuid::new_v4));
        let (attachment_added, attachment_removed) = self.attachments.map_or_else(
            || (Vec::new(), Vec::new()),
            |change| (change.add, change.remove),
        );
        PageEditorForm {
            action: edit_to(nav, page),
            history_to: (can_save && cancel_to.is_some())
                .then(|| page_history_path(route.tenant, route.space, page)),
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
            attachments: Vec::new(),
            attachment_draft,
            attachment_upload_to: attachment_draft
                .filter(|_| can_save)
                .map(|draft| attachment_draft_path(route.tenant, route.space, page, draft)),
            attachment_added,
            attachment_removed,
            public_link_active: None,
            coedit_retry: None,
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
    origin: &str,
) -> Result<Response, sqlx::Error> {
    let route = nav.route;
    let Some(mut selected) = page.select_page(id) else {
        record(principal.id, route, "not-found");
        return Ok(not_found(route.target));
    };
    let address = nav.page_address(id, principal);
    let Some(current) = store.page_text(&address).await? else {
        record(principal.id, route, "not-found");
        return super::denied_page(store, &address).await;
    };
    if nav.download_markdown {
        record(principal.id, route, "exported");
        return Ok(markdown_response(&current.title, current.source));
    }
    let page =
        page.with_visible_context(&address, principal, &mut selected, current.can_browse_space);
    selected.can_share = current.can_manage;
    selected.access_role = Some(if current.can_manage {
        "owner".into()
    } else {
        current.effective_role.clone()
    });
    let files = attachments::published(route, id, current.attachments);
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
        let draft = Uuid::new_v4();
        selected.editor = Some(PageEditorForm {
            action: edit_to(nav, id),
            cancel_to: Some(return_to(nav, id)),
            history_to: Some(page_history_path(route.tenant, route.space, id)),
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
            attachments: files,
            attachment_draft: Some(draft),
            attachment_upload_to: Some(attachment_draft_path(route.tenant, route.space, id, draft)),
            attachment_added: Vec::new(),
            attachment_removed: Vec::new(),
            public_link_active: Some(current.public_link_active),
            coedit_retry: None,
        });
    } else {
        let (text, rendered_html) = match current.format {
            PageContentFormat::Literal => (markdown_to_text(&current.source), None),
            PageContentFormat::Markdown => {
                let rendered = render_document(
                    &current.source,
                    &|id, inline| {
                        let file = files.iter().find(|file| file.id == id)?;
                        if inline {
                            file.image_to.as_deref()
                        } else {
                            file.download_to.as_deref()
                        }
                    },
                    origin,
                );
                (current.source, Some(rendered))
            }
        };
        let mut related = if page.page_only {
            Vec::new()
        } else {
            related_documents(&address, origin, store.backlink_candidates(&address).await?)
        };
        if !page.page_only {
            for (task, number, title, status) in store.tasks_related_to_document(&address).await? {
                let Some(status) = TaskStatus::parse(&status) else {
                    continue;
                };
                related.push(RelatedDocument {
                    title,
                    path: task_path(route.tenant, route.space, task),
                    task_label: Some(format!("KB-{number:03} · {}", status.label())),
                });
            }
        }
        selected.reading = Some(PageReading {
            template: current.template_revision.as_deref().and_then(crate::page_templates::find),
            related,
            revision: current.content_revision.to_string(),
            text,
            rendered_html,
            edit_to: current.can_edit.then(|| edit_to(nav, id)),
            history_to: current
                .can_edit
                .then(|| page_history_path(route.tenant, route.space, id)),
            cover: cover_view(route, id, current.cover),
            attachments: files,
        });
    }
    record(principal.id, route, "opened");
    Ok(page.render(StatusCode::OK, selected, None, nav.teamstream.as_ref()))
}

fn markdown_response(title: &str, source: String) -> Response {
    let sanitized = title.replace(
        |c: char| c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'),
        "_",
    );
    let stem = sanitized.trim_matches(|c: char| c == '.' || c.is_whitespace());
    let stem = if stem.is_empty() {
        "Страница"
    } else {
        stem
    };
    // Keep the extension even on file systems with a 255-byte filename limit.
    let end = stem.floor_char_boundary(240.min(stem.len()));
    let name = format!("{}.md", &stem[..end]);
    (
        [
            (header::CONTENT_TYPE, "text/markdown; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                http::content_disposition(&name, false).as_str(),
            ),
        ],
        source,
    )
        .into_response()
}

fn related_documents(
    current: &PageAddress,
    origin: &str,
    candidates: Vec<BacklinkCandidate>,
) -> Vec<RelatedDocument> {
    let target = page_path(current.tenant, current.space, current.page);
    let relative = format!(r#"href="{target}""#);
    let absolute = format!(r#"href="{origin}{target}""#);
    candidates
        .into_iter()
        .filter(|candidate| {
            // Apply the same grammar as the reader, including code and escaped HTML.
            let rendered = render_document(&candidate.markdown, &|_, _| None, origin);
            rendered.as_str().contains(&relative) || rendered.as_str().contains(&absolute)
        })
        .take(20)
        .map(|candidate| RelatedDocument {
            path: page_path(current.tenant, candidate.space_id, candidate.id),
            title: candidate.title,
            task_label: None,
        })
        .collect()
}

enum SaveFailure {
    Invalid(&'static str),
    AttachmentReferences,
    Forbidden,
    Conflict,
    CoeditConflict(String, PageContentFormat),
    CoeditRejected(&'static str),
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
    let session = session_digest(request.headers());
    let input = match read_submission(origin, request).await {
        Ok(input) => input,
        Err(error) => {
            if matches!(error, RequestError::Origin) {
                record(principal.id, route, "invalid-origin");
            }
            return error.response();
        }
    };
    let permit = input.coedit.as_ref().and_then(|retry| {
        Some(CoeditPublication {
            lease: retry.lease,
            session: session?,
            version: retry.version.parse().ok()?,
            operation: retry.operation,
            cover_base: input.cover.and(input.cover_base),
        })
    });
    let failure = if let Some(error) = input.error {
        SaveFailure::Invalid(error)
    } else if input.coedit.is_some() && permit.is_none() {
        SaveFailure::CoeditRejected(COEDIT_LEASE)
    } else {
        let revision = input
            .version
            .expect("validated submission includes a version");
        let address = nav.page_address(id, principal);
        let cover = input.cover.map_or(CoverUpdate::Keep, CoverUpdate::Set);
        let patch = input
            .attachments
            .as_ref()
            .and_then(AttachmentChanges::patch);
        let save = async {
            if let Some(permit) = permit {
                store
                    .publish_coedit(&address, revision, input.update(), cover, patch, permit)
                    .await
            } else if let Some(patch) = patch {
                store
                    .save_page_with_attachments(
                        &address,
                        revision,
                        input.update(),
                        cover,
                        Some(patch),
                    )
                    .await
            } else {
                store
                    .save_page(&address, revision, input.update(), cover)
                    .await
            }
        };
        match timeout(SAVE_TIMEOUT, save).await {
            Ok(Ok(SavePageText::Saved)) => {
                record(principal.id, route, "saved");
                return redirect(&return_to(nav, id));
            }
            Ok(Ok(SavePageText::Missing)) => {
                record(principal.id, route, "not-found");
                return unavailable_draft(nav, id, input);
            }
            Ok(Ok(SavePageText::Forbidden)) => SaveFailure::Forbidden,
            Ok(Ok(SavePageText::Conflict(current))) => {
                eprintln!(
                    "page.edit.conflict page={id} revision={}",
                    current.content_revision
                );
                SaveFailure::Conflict
            }
            Ok(Ok(SavePageText::CoeditConflict(source, format))) => {
                SaveFailure::CoeditConflict(source, format)
            }
            Ok(Ok(SavePageText::CoeditLeaseExpired)) => SaveFailure::CoeditRejected(COEDIT_LEASE),
            Ok(Ok(SavePageText::CoeditOperationConflict)) => {
                SaveFailure::CoeditRejected(COEDIT_OPERATION)
            }
            Ok(Ok(SavePageText::InvalidCover)) => SaveFailure::Invalid(INVALID_COVER),
            Ok(Ok(SavePageText::InvalidAttachments)) => SaveFailure::Invalid(INVALID_ATTACHMENTS),
            Ok(Ok(SavePageText::AttachmentReferencesPresent)) => SaveFailure::AttachmentReferences,
            Ok(Ok(SavePageText::AttachmentRetired)) => SaveFailure::Invalid(ATTACHMENT_RETIRED),
            Ok(Err(_)) | Err(_) => {
                record(principal.id, route, "unavailable");
                return retry_after_write_failure(store, nav, &address, input).await;
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
    mut input: Submission,
    mut failure: SaveFailure,
) -> Response {
    let route = nav.route;
    let workspace = match timeout(
        RECOVERY_TIMEOUT,
        super::load_page(store, nav, principal, id),
    )
    .await
    {
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
        return unavailable_draft(nav, id, input);
    };
    let address = nav.page_address(id, principal);
    let current = match timeout(RECOVERY_TIMEOUT, store.page_text(&address)).await {
        Ok(Ok(Some(current))) => current,
        Ok(Ok(None)) => return unavailable_draft(nav, id, input),
        Ok(Err(_)) | Err(_) => return retry(nav, id, input),
    };
    let page =
        page.with_visible_context(&address, principal, &mut selected, current.can_browse_space);
    selected.can_share = current.can_manage;
    selected.access_role = Some(if current.can_manage {
        "owner".into()
    } else {
        current.effective_role.clone()
    });
    if !current.can_edit {
        if input.coedit.is_some() {
            return redirect(&return_to(nav, id));
        }
        failure = SaveFailure::Forbidden;
    }
    let (status, error, conflict, files) = match failure {
        SaveFailure::Invalid(_) | SaveFailure::AttachmentReferences => {
            let error = match failure {
                SaveFailure::Invalid(error) => error,
                SaveFailure::AttachmentReferences => {
                    if input.text_unchanged {
                        input.source = normalize_line_endings(&current.source).into_owned();
                        input.format = current.format;
                        input.text = match current.format {
                            PageContentFormat::Literal => markdown_to_text(&input.source),
                            PageContentFormat::Markdown => input.source.clone(),
                        };
                        input.text_base = Some(text_fingerprint(current.format, &input.source));
                    }
                    ATTACHMENT_REFERENCES
                }
                _ => unreachable!(),
            };
            (
                StatusCode::UNPROCESSABLE_ENTITY,
                error,
                None,
                current.attachments,
            )
        }
        SaveFailure::Forbidden => (StatusCode::FORBIDDEN, DENIED, None, Vec::new()),
        SaveFailure::CoeditRejected(reason) => {
            (StatusCode::CONFLICT, reason, None, current.attachments)
        }
        SaveFailure::CoeditConflict(source, format) => (
            StatusCode::CONFLICT,
            COEDIT_CONFLICT,
            Some(PageEditConflict {
                text: match format {
                    PageContentFormat::Literal => markdown_to_text(&source),
                    PageContentFormat::Markdown => source.clone(),
                },
                source,
                content_format: format,
                revision: current.content_revision.to_string(),
                cover: cover_view(route, id, current.cover),
            }),
            current.attachments,
        ),
        SaveFailure::Conflict => (
            StatusCode::CONFLICT,
            if input.cover.is_some()
                || input.cover_base.is_some_and(|base| base != current.cover)
                || input
                    .attachments
                    .as_ref()
                    .is_some_and(|patch| patch.patch().is_some())
            {
                COVER_CONFLICT
            } else {
                CONFLICT
            },
            Some(PageEditConflict {
                text: normalize_line_endings(&current.source).into_owned(),
                source: normalize_line_endings(&current.source).into_owned(),
                content_format: current.format,
                revision: current.content_revision.to_string(),
                cover: cover_view(route, id, current.cover),
            }),
            current.attachments,
        ),
    };
    let can_save = status != StatusCode::FORBIDDEN && input.version.is_some();
    let changes = input.attachments.as_ref().filter(|_| can_save);
    let staged = match timeout(RECOVERY_TIMEOUT, staged_files(store, &address, changes)).await {
        Ok(Ok(files)) => files,
        Ok(Err(_)) | Err(_) => return retry(nav, id, input),
    };
    let files = attachment_projection(route, id, changes, files, staged);
    let mut form = input.editor_form(nav, id, Some(return_to(nav, id)), error, conflict, can_save);
    form.attachments = files;
    form.public_link_active = can_save.then_some(current.public_link_active);
    selected.editor = Some(form);
    record(principal.id, route, status.as_str());
    page.render(status, selected, None, nav.teamstream.as_ref())
}

fn unavailable_draft(nav: &DocumentNavigation, id: Uuid, input: Submission) -> Response {
    if input.plain_post || input.coedit.is_some() {
        return not_found(nav.route.target);
    }
    let form = input.editor_form(
        nav,
        id,
        Some(return_to(nav, id)),
        "Страница недоступна. Черновик можно скопировать.",
        None,
        false,
    );
    html(StatusCode::NOT_FOUND, views::page_edit_retry(&form))
}

fn retry(nav: &DocumentNavigation, id: Uuid, input: Submission) -> Response {
    retry_with_attachments(nav, id, input, Vec::new())
}

async fn staged_files(
    store: &Store,
    address: &PageAddress,
    changes: Option<&AttachmentChanges>,
) -> Result<Vec<Attachment>, sqlx::Error> {
    match changes.filter(|patch| !patch.add.is_empty()) {
        Some(patch) => {
            store
                .draft_attachments(address, patch.draft, &patch.add)
                .await
        }
        None => Ok(Vec::new()),
    }
}

fn attachment_projection(
    route: DocumentsRoute,
    id: Uuid,
    changes: Option<&AttachmentChanges>,
    published: Vec<Attachment>,
    staged: Vec<Attachment>,
) -> Vec<PageAttachmentView> {
    let mut files = attachments::published(route, id, published);
    if let Some(patch) = changes {
        files.retain(|file| !patch.remove.contains(&file.id));
        let base = attachment_draft_path(route.tenant, route.space, id, patch.draft);
        for file in staged {
            if !files.iter().any(|current| current.id == file.id) {
                files.push(attachments::staged(&base, file));
            }
        }
    }
    files
}

async fn retry_after_write_failure(
    store: &Store,
    nav: &DocumentNavigation,
    address: &PageAddress,
    input: Submission,
) -> Response {
    let changes = input.attachments.as_ref();
    let (live, staged) = tokio::join!(
        timeout(FILE_RECOVERY_TIMEOUT, store.page_text(address)),
        timeout(FILE_RECOVERY_TIMEOUT, staged_files(store, address, changes)),
    );
    // These checks run only after identity is known and both reads independently
    // recheck access. Staged metadata can be recovered even if pages is locked.
    let staged = staged.ok().and_then(Result::ok).unwrap_or_default();
    let files = match live {
        Ok(Ok(Some(page))) if page.can_edit => {
            attachment_projection(nav.route, address.page, changes, page.attachments, staged)
        }
        Ok(Ok(_)) => Vec::new(),
        _ => attachment_projection(nav.route, address.page, changes, Vec::new(), staged),
    };
    retry_with_attachments(nav, address.page, input, files)
}

fn retry_with_attachments(
    nav: &DocumentNavigation,
    id: Uuid,
    input: Submission,
    files: Vec<PageAttachmentView>,
) -> Response {
    let error = if input.cover.is_some()
        || input
            .attachments
            .as_ref()
            .is_some_and(|patch| patch.patch().is_some())
    {
        "Не удалось сохранить страницу. Попробуйте снова."
    } else {
        UNAVAILABLE
    };
    let permit = input.coedit.clone();
    let mut form = input.editor_form(nav, id, None, error, None, true);
    form.attachments = files;
    form.coedit_retry = permit;
    html_with_external_frames(
        StatusCode::SERVICE_UNAVAILABLE,
        views::page_edit_retry(&form),
        nav.teamstream.as_ref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn markdown_file_preserves_source_and_encodes_a_single_download_name() {
        let source = "# Заметка\r\n\n+ **Ввод** и <foo>  \n".to_owned();
        let response = markdown_response("../🧭/план\r\n", source.clone());
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "text/markdown; charset=utf-8"
        );
        let disposition = response.headers()[header::CONTENT_DISPOSITION]
            .to_str()
            .unwrap();
        assert!(disposition.starts_with("attachment;"));
        assert!(
            disposition.ends_with(".md")
                && !disposition.contains('/')
                && !disposition.contains("%0D")
        );
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        assert_eq!(bytes.as_ref(), source.as_bytes());
    }

    #[test]
    fn related_documents_accept_live_local_links_and_ignore_code() {
        let address = PageAddress {
            tenant: Uuid::new_v4(),
            space: Uuid::new_v4(),
            page: Uuid::new_v4(),
            account: Uuid::new_v4(),
        };
        let origin = "http://localhost:4125";
        let path = page_path(address.tenant, address.space, address.page);
        let candidate = |title: &str, markdown| BacklinkCandidate {
            id: Uuid::new_v4(),
            space_id: Uuid::new_v4(),
            title: title.into(),
            markdown,
        };
        let relative = candidate("Относительная", format!("[Исходник]({path})"));
        let relative_path = page_path(address.tenant, relative.space_id, relative.id);
        let links = related_documents(
            &address,
            origin,
            vec![
                candidate("Код", format!("`[Исходник]({path})`")),
                candidate("Блок кода", format!("```md\n[Исходник]({path})\n```")),
                candidate("HTML", format!(r#"<a href="{path}">Исходник</a>"#)),
                candidate(
                    "Внешняя",
                    format!("[Исходник](https://remote.example{path})"),
                ),
                relative,
                candidate("Абсолютная", format!("[Исходник]({origin}{path})")),
            ],
        );
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].title, "Относительная");
        assert_eq!(links[0].path, relative_path);
        assert_eq!(links[1].title, "Абсолютная");
    }

    #[test]
    fn coedit_form_preserves_complete_exact_permit_and_rejects_partial_inputs() {
        let lease = "11111111-1111-1111-1111-111111111111";
        let operation = "22222222-2222-2222-2222-222222222222";
        let body = format!(
            "revision=2&text=typed&contentFormat=markdown&coedit_lease={lease}&coedit_operation={operation}"
        );
        for input in [
            body.clone(),
            format!("{body}&coedit_version=01"),
            format!("{body}&coedit_version=0&coedit_version=1"),
        ] {
            let input = Submission::from(Form::from_query(&input));
            assert_eq!(input.error, Some(COEDIT_LEASE));
            assert_eq!(input.source, "typed");
        }
        let input = Submission::from(Form::from_query(&format!("{body}&coedit_version=0")));
        assert!(input.error.is_none());
        let permit = input.coedit.as_ref().unwrap();
        assert_eq!(permit.version, "0");
        assert_eq!(permit.lease.to_string(), lease);
        assert_eq!(permit.operation.to_string(), operation);
        assert!(matches!(
            input.update(),
            PageUpdate::Source("typed", PageContentFormat::Markdown)
        ));
    }

    #[test]
    fn attachment_patch_is_bounded_canonical_and_preserved_for_retry() {
        let draft = "11111111-1111-1111-1111-111111111111";
        let add = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        let remove = "bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb";
        for part in [
            format!("attachmentAdd={add}"),
            format!("attachmentDraft={draft}&attachmentAdd={add}&attachmentRemove={add}"),
            format!("attachmentDraft={draft}&attachmentAdd={add},{add}"),
            format!(
                "attachmentDraft={draft}&attachmentAdd={}",
                add.to_uppercase()
            ),
            format!("attachmentDraft={draft}&attachmentAdd={add}&attachmentAdd="),
        ] {
            let form = Submission::from(Form::from_query(&format!("revision=2&text=kept&{part}")));
            assert_eq!(form.error, Some(INVALID_ATTACHMENTS));
            assert!(form.attachments.is_none());
            assert_eq!(form.source, "kept");
        }
        let input = Submission::from(Form::from_query(&format!(
            "revision=2&text=preserved&attachmentDraft={draft}&attachmentAdd={add}&attachmentRemove={remove}"
        )));
        let id = Uuid::new_v4();
        let route = DocumentsRoute {
            tenant: Uuid::new_v4(),
            space: Uuid::new_v4(),
            target: DocumentsTarget::EditPage(id),
        };
        let navigation =
            DocumentNavigation::for_request(route, &Request::new(axum::body::Body::empty()));
        let retry = input.editor_form(&navigation, id, None, UNAVAILABLE, None, true);
        assert!(retry.attachments.is_empty());
        assert_eq!(retry.source, "preserved");
        assert_eq!(retry.attachment_draft, canonical_uuid(draft));
        assert_eq!(retry.attachment_added, vec![canonical_uuid(add).unwrap()]);
        assert_eq!(
            retry.attachment_removed,
            vec![canonical_uuid(remove).unwrap()]
        );
        assert!(
            retry
                .attachment_upload_to
                .unwrap()
                .ends_with(&format!("/attachments/drafts/{draft}"))
        );
    }

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
            source: String::new(),
            content_format: PageContentFormat::Markdown,
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
