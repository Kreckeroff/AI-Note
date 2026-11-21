mod format;

use std::{error::Error, time::Duration};

use axum::{
    Json,
    body::{Bytes, to_bytes},
    extract::Request,
    http::{HeaderName, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use http_body_util::LengthLimitError;
use serde::Serialize;
use tokio::{sync::Semaphore, time::timeout};
use uuid::Uuid;

use crate::{
    db::{Attachment, AttachmentBody, AttachmentLookup, PageAddress, StageAttachment, Store},
    http::{self, content_disposition, plain},
    sections::PageAttachmentView,
    workspace::{AttachmentTarget, DocumentsRoute, attachment_draft_path, page_path},
};

use format::{FileFormat, decode_name};

const MISSING: &str = "Вложение не найдено.";
const DENIED: &str = "У вас нет права прикреплять файлы к этой странице.";
const MAX_FILE_BYTES: usize = 20 * 1024 * 1024;
const BODY_TIMEOUT: Duration = Duration::from_secs(8);
static ACTIVE_UPLOADS: Semaphore = Semaphore::const_new(4);

#[derive(Serialize)]
struct UploadError {
    error: &'static str,
}

fn upload_error(status: StatusCode, error: &'static str) -> Response {
    (status, Json(UploadError { error })).into_response()
}

pub(crate) fn unauthenticated(reading: bool) -> Response {
    if reading {
        plain(StatusCode::NOT_FOUND, MISSING)
    } else {
        upload_error(StatusCode::UNAUTHORIZED, "Войдите в «Север» ещё раз.")
    }
}

pub(crate) fn unavailable(reading: bool) -> Response {
    if reading {
        plain(
            StatusCode::SERVICE_UNAVAILABLE,
            "Не удалось открыть вложение. Попробуйте ещё раз.",
        )
    } else {
        upload_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Не удалось загрузить файл. Попробуйте ещё раз.",
        )
    }
}

pub(super) fn published(
    route: DocumentsRoute,
    page: Uuid,
    attachments: Vec<Attachment>,
) -> Vec<PageAttachmentView> {
    let base = page_path(route.tenant, route.space, page);
    attachments
        .into_iter()
        .filter_map(|attachment| {
            let token = attachment.publication?;
            Some(PageAttachmentView::at_url(
                attachment,
                format!("{base}/attachments/{token}"),
                true,
            ))
        })
        .collect()
}

pub(super) fn staged(base: &str, attachment: Attachment) -> PageAttachmentView {
    let path = format!("{base}/{}", attachment.id);
    PageAttachmentView::at_url(attachment, path, false)
}

pub(super) async fn handle(
    store: &Store,
    address: &PageAddress,
    target: AttachmentTarget,
    origin: &str,
    request: Request,
) -> Result<Response, sqlx::Error> {
    match target {
        AttachmentTarget::Upload { draft, id } => {
            upload(store, address, origin, request, draft, id).await
        }
        AttachmentTarget::DraftImage { draft, id } => {
            download(store, address, AttachmentLookup::Draft { draft, id }, true).await
        }
        AttachmentTarget::Published { token, inline } => {
            download(store, address, AttachmentLookup::Published(token), inline).await
        }
    }
}

async fn upload_bytes(request: Request) -> Result<Bytes, (StatusCode, &'static str)> {
    match timeout(BODY_TIMEOUT, to_bytes(request.into_body(), MAX_FILE_BYTES)).await {
        Ok(Ok(bytes)) => Ok(bytes),
        Ok(Err(error))
            if error
                .source()
                .is_some_and(|cause| cause.is::<LengthLimitError>()) =>
        {
            Err((
                StatusCode::PAYLOAD_TOO_LARGE,
                "Размер файла должен быть до 20 МБ.",
            ))
        }
        Ok(Err(_)) => Err((StatusCode::BAD_REQUEST, "Не удалось прочитать файл.")),
        Err(_) => Err((
            StatusCode::REQUEST_TIMEOUT,
            "Не удалось получить файл вовремя. Попробуйте ещё раз.",
        )),
    }
}

async fn upload(
    store: &Store,
    address: &PageAddress,
    origin: &str,
    request: Request,
    draft: Uuid,
    id: Uuid,
) -> Result<Response, sqlx::Error> {
    if !http::check_origin(request.headers(), origin) {
        return Ok(upload_error(
            StatusCode::FORBIDDEN,
            "Откройте редактор страницы в «Севере».",
        ));
    }
    let Some(page) = store.page_text(address).await? else {
        return Ok(upload_error(StatusCode::NOT_FOUND, MISSING));
    };
    if !page.can_edit {
        return Ok(upload_error(StatusCode::FORBIDDEN, DENIED));
    }
    let name = http::single_header(
        request.headers(),
        HeaderName::from_static("x-attachment-name"),
    )
    .and_then(decode_name);
    let Some(name) = name else {
        return Ok(upload_error(
            StatusCode::BAD_REQUEST,
            "Укажите корректное имя файла.",
        ));
    };
    let content_type = http::single_header(request.headers(), header::CONTENT_TYPE);
    let Some(format) = FileFormat::from_name(&name)
        .filter(|format| content_type.is_some_and(|value| format.accepts_content_type(value)))
    else {
        return Ok(upload_error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Выберите файл PDF, TXT, Markdown, DOCX, XLSX, PPTX, PNG, JPEG или WebP.",
        ));
    };

    let Ok(slot) = ACTIVE_UPLOADS.try_acquire() else {
        return Ok(unavailable(false));
    };
    let bytes = match upload_bytes(request).await {
        Ok(bytes) if bytes.is_empty() => {
            return Ok(upload_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Файл пуст. Выберите другой файл.",
            ));
        }
        Ok(bytes) => bytes,
        Err((status, message)) => {
            let mut response = upload_error(status, message);
            response
                .headers_mut()
                .insert(header::CONNECTION, HeaderValue::from_static("close"));
            return Ok(response);
        }
    };
    let validated = tokio::task::spawn_blocking(move || {
        // The permit stays with the CPU work if the caller disconnects.
        let image_type = format.verify(&bytes);
        (image_type, bytes, slot)
    })
    .await;
    let Ok((image_type, bytes, _slot)) = validated else {
        return Ok(unavailable(false));
    };
    let Some(image_type) = image_type else {
        return Ok(upload_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Файл повреждён или его формат и размеры не поддерживаются.",
        ));
    };

    let response = match store
        .stage_attachment(address, draft, id, &name, image_type, &bytes)
        .await?
    {
        StageAttachment::Saved(attachment) => {
            let base = attachment_draft_path(address.tenant, address.space, address.page, draft);
            eprintln!(
                "page.attachment.staged account={} tenant={} space={} page={}",
                address.account, address.tenant, address.space, address.page
            );
            (StatusCode::CREATED, Json(staged(&base, attachment))).into_response()
        }
        StageAttachment::Missing => upload_error(StatusCode::NOT_FOUND, MISSING),
        StageAttachment::Forbidden => upload_error(StatusCode::FORBIDDEN, DENIED),
        StageAttachment::Unavailable => upload_error(
            StatusCode::CONFLICT,
            "Не удалось повторить загрузку этого файла. Выберите его снова.",
        ),
        StageAttachment::Limit => upload_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "В этом черновике больше нет места. Сохраните страницу и откройте редактор снова.",
        ),
    };
    Ok(response)
}

pub(super) async fn download(
    store: &Store,
    address: &PageAddress,
    lookup: AttachmentLookup,
    inline: bool,
) -> Result<Response, sqlx::Error> {
    let Some(file) = store.attachment_body(address, lookup).await? else {
        return Ok(plain(StatusCode::NOT_FOUND, MISSING));
    };
    Ok(attachment_response(file, inline))
}

fn attachment_response(file: AttachmentBody, inline: bool) -> Response {
    let content_type = if inline {
        match file.image_type.as_deref() {
            Some(kind @ ("image/png" | "image/jpeg" | "image/webp")) => kind,
            _ => return plain(StatusCode::NOT_FOUND, MISSING),
        }
    } else {
        "application/octet-stream"
    };
    let disposition = content_disposition(&file.name, inline);
    (
        [
            (header::CONTENT_TYPE, content_type),
            (header::CONTENT_DISPOSITION, disposition.as_str()),
        ],
        file.bytes,
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use axum::body::Body;

    use super::*;

    #[test]
    fn inline_is_available_only_for_verified_rasters() {
        for image_type in [None, Some("image/svg+xml".to_owned())] {
            let file = AttachmentBody {
                name: "a.txt".to_owned(),
                bytes: vec![7, 8],
                image_type,
            };
            let response = attachment_response(file, true);
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            assert!(!response.headers().contains_key(header::CONTENT_DISPOSITION));
        }
        let file = AttachmentBody {
            name: "Картинка.png".to_owned(),
            bytes: vec![7, 8],
            image_type: Some("image/png".to_owned()),
        };
        let response = attachment_response(file, false);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "application/octet-stream"
        );
        assert!(
            response.headers()[header::CONTENT_DISPOSITION]
                .to_str()
                .unwrap()
                .starts_with("attachment;")
        );
    }

    #[tokio::test]
    async fn raw_upload_rejects_a_body_over_the_file_limit() {
        let request = Request::new(Body::from(vec![8; MAX_FILE_BYTES + 1]));
        assert_eq!(
            upload_bytes(request).await.unwrap_err().0,
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }
}
