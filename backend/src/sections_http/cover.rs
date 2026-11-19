use std::{error::Error, time::Duration};

use axum::{
    Json,
    body::to_bytes,
    extract::Request,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use http_body_util::LengthLimitError;
use serde::Serialize;
use tokio::{sync::Semaphore, time::timeout};
use uuid::Uuid;

use crate::{
    cover_image::{
        COVER_IMAGE_CONTENT_TYPE, MAX_COVER_BYTES, NormalizeCoverError, normalize_cover_image,
    },
    db::{PageAddress, PageCover, StagePageCover, Store},
    http::{self, plain},
    sections::CoverView,
};

const MISSING: &str = "Обложка не найдена.";
const DENIED: &str = "У вас нет права менять обложку этой страницы.";
const FORMATS: &str = "Выберите изображение PNG, JPEG или WebP.";
const UNAVAILABLE: &str = "Не удалось открыть изображение. Попробуйте ещё раз.";
const UPLOAD_UNAVAILABLE: &str = "Не удалось загрузить изображение. Попробуйте ещё раз.";
const BODY_TIMEOUT: Duration = Duration::from_secs(8);
static ACTIVE_UPLOADS: Semaphore = Semaphore::const_new(4);

#[derive(Serialize)]
struct UploadError {
    error: &'static str,
}

fn upload_error(status: StatusCode, error: &'static str) -> Response {
    (status, Json(UploadError { error })).into_response()
}

pub(crate) fn cover_unauthenticated(reading: bool) -> Response {
    if reading {
        plain(StatusCode::NOT_FOUND, MISSING)
    } else {
        upload_error(StatusCode::UNAUTHORIZED, "Войдите в «Север» ещё раз.")
    }
}

pub(crate) fn cover_unavailable(reading: bool) -> Response {
    if reading {
        plain(StatusCode::SERVICE_UNAVAILABLE, UNAVAILABLE)
    } else {
        upload_error(StatusCode::SERVICE_UNAVAILABLE, UPLOAD_UNAVAILABLE)
    }
}

fn accepts_upload(request: &Request) -> bool {
    http::single_header(request.headers(), header::CONTENT_TYPE)
        .and_then(|raw| raw.split(';').next())
        .map(str::trim)
        .is_some_and(|value| {
            [
                "image/png",
                "image/jpeg",
                "image/webp",
                "application/octet-stream",
            ]
            .iter()
            .any(|allowed| value.eq_ignore_ascii_case(allowed))
        })
}

#[derive(Debug)]
enum UploadReadError {
    TooLarge,
    Unreadable,
    Timeout,
}

impl UploadReadError {
    fn response(self) -> Response {
        let (status, message) = match self {
            Self::TooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                "Размер изображения должен быть до 5 МБ.",
            ),
            Self::Unreadable => (StatusCode::BAD_REQUEST, "Не удалось прочитать изображение."),
            Self::Timeout => (
                StatusCode::REQUEST_TIMEOUT,
                "Не удалось получить изображение вовремя. Попробуйте ещё раз.",
            ),
        };
        let mut response = upload_error(status, message);
        response
            .headers_mut()
            .insert(header::CONNECTION, HeaderValue::from_static("close"));
        response
    }
}

async fn upload_bytes(request: Request) -> Result<Vec<u8>, UploadReadError> {
    match timeout(BODY_TIMEOUT, to_bytes(request.into_body(), MAX_COVER_BYTES)).await {
        Ok(Ok(bytes)) => Ok(bytes.to_vec()),
        Ok(Err(error))
            if error
                .source()
                .is_some_and(|cause| cause.is::<LengthLimitError>()) =>
        {
            Err(UploadReadError::TooLarge)
        }
        Ok(Err(_)) => Err(UploadReadError::Unreadable),
        Err(_) => Err(UploadReadError::Timeout),
    }
}

pub(super) async fn upload(
    store: &Store,
    address: &PageAddress,
    origin: &str,
    request: Request,
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
    if !accepts_upload(&request) {
        return Ok(upload_error(StatusCode::UNSUPPORTED_MEDIA_TYPE, FORMATS));
    }
    let Ok(_slot) = ACTIVE_UPLOADS.try_acquire() else {
        return Ok(cover_unavailable(false));
    };
    let bytes = match upload_bytes(request).await {
        Ok(bytes) => bytes,
        Err(error) => return Ok(error.response()),
    };
    let image = match normalize_cover_image(bytes).await {
        Ok(image) => image,
        Err(NormalizeCoverError::Invalid) => {
            return Ok(upload_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Изображение повреждено или его размеры превышают допустимые.",
            ));
        }
        Err(NormalizeCoverError::Busy) => return Ok(cover_unavailable(false)),
    };
    match store.stage_page_cover(address, image).await? {
        StagePageCover::Saved(asset) => {
            eprintln!(
                "page.cover.staged account={} tenant={} space={} page={}",
                address.account, address.tenant, address.space, address.page
            );
            let cover = CoverView::new(
                address.tenant,
                address.space,
                address.page,
                PageCover::Image(asset),
            );
            Ok((StatusCode::CREATED, Json(cover)).into_response())
        }
        StagePageCover::Missing => Ok(upload_error(StatusCode::NOT_FOUND, MISSING)),
        StagePageCover::Forbidden => Ok(upload_error(StatusCode::FORBIDDEN, DENIED)),
    }
}

pub(super) async fn image(
    store: &Store,
    address: &PageAddress,
    asset: Uuid,
) -> Result<Response, sqlx::Error> {
    let Some(bytes) = store.page_cover_image(address, asset).await? else {
        return Ok(plain(StatusCode::NOT_FOUND, MISSING));
    };
    Ok(([(header::CONTENT_TYPE, COVER_IMAGE_CONTENT_TYPE)], bytes).into_response())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;

    use super::*;

    #[test]
    fn raster_upload_requires_one_supported_content_type() {
        for (content_type, accepted) in [
            ("image/png", true),
            ("image/JPEG", true),
            ("image/webp", true),
            ("image/svg+xml", false),
            ("application/octet-stream", true),
            ("text/plain", false),
        ] {
            let request = Request::builder()
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::empty())
                .unwrap();
            assert_eq!(accepts_upload(&request), accepted);
        }
        let duplicate = Request::builder()
            .header(header::CONTENT_TYPE, "image/png")
            .header(header::CONTENT_TYPE, "image/webp")
            .body(Body::empty())
            .unwrap();
        assert!(!accepts_upload(&duplicate));
    }

    #[tokio::test]
    async fn raw_cover_body_stops_at_five_mebibytes() {
        let exact = Request::new(Body::from(vec![0; MAX_COVER_BYTES]));
        assert_eq!(upload_bytes(exact).await.unwrap().len(), MAX_COVER_BYTES);
        let large = Request::new(Body::from(vec![0; MAX_COVER_BYTES + 1]));
        let error = upload_bytes(large).await.unwrap_err().response();
        assert_eq!(error.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(error.headers()[header::CONNECTION], "close");
    }
}
