use axum::{
    Json,
    extract::Request,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use tokio::time::timeout;
use uuid::Uuid;

use crate::{
    cover_image::COVER_IMAGE_CONTENT_TYPE,
    db::{AttachmentLookup, PageAddress, PageHistory, RestorePageRevision, Store},
    http::{self, Form, FormError, html, plain, redirect},
    views::{self, history as view},
    workspace::{DocumentsRoute, HistoryTarget, revision_number},
};

use super::{FORM_TIMEOUT, RECOVERY_TIMEOUT, SAVE_TIMEOUT, attachments, cover_unauthenticated};

const OPEN_FORM: &str = "Откройте подтверждение восстановления в «Севере».";
const INVALID_REVISION: &str = "Укажите корректный номер редакции.";
const RESTORE_UNAVAILABLE: &str =
    "Не удалось подтвердить результат восстановления. Попробуйте ещё раз.";
const STALE_RESTORE: &str =
    "Страница изменилась. Сравните редакции и подтвердите восстановление ещё раз.";

pub(crate) fn history_wants_json(headers: &HeaderMap) -> bool {
    headers
        .get_all(header::ACCEPT)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .any(|range| {
            let mut parts = range.split(';').map(str::trim);
            parts
                .next()
                .is_some_and(|kind| kind.eq_ignore_ascii_case("application/json"))
                && !parts
                    .filter_map(|parameter| parameter.split_once('='))
                    .any(|(key, value)| {
                        key.trim().eq_ignore_ascii_case("q")
                            && value.trim().parse::<f32>() == Ok(0.0)
                    })
        })
}

#[derive(Serialize)]
struct HistoryError {
    error: &'static str,
}

pub(crate) fn history_api_error(status: StatusCode) -> Response {
    let error = match status {
        StatusCode::BAD_REQUEST => INVALID_REVISION,
        StatusCode::UNAUTHORIZED => "Войдите в «Север» ещё раз.",
        StatusCode::NOT_FOUND => "История страницы не найдена.",
        _ => "Не удалось открыть историю. Попробуйте ещё раз.",
    };
    (status, Json(HistoryError { error })).into_response()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoryPage {
    current_revision: String,
    items: Vec<HistoryItem>,
    next_to: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoryItem {
    revision: String,
    author: Option<String>,
    recorded_at: Option<String>,
    href: String,
}

fn list_json(history: PageHistory, links: &view::Links) -> Response {
    Json(HistoryPage {
        current_revision: history.current_revision.to_string(),
        items: history
            .items
            .into_iter()
            .map(|item| HistoryItem {
                revision: item.revision.to_string(),
                author: item.author,
                recorded_at: item.recorded_at,
                href: links.revision(item.revision),
            })
            .collect(),
        next_to: history.next_before.map(|before| links.list(Some(before))),
    })
    .into_response()
}

fn missing() -> Response {
    html(StatusCode::NOT_FOUND, views::page_not_found())
}

fn revision_field(form: &Form, field: &str) -> Result<Option<i64>, ()> {
    form.field(field)
        .map_err(|_| ())?
        .map(|raw| revision_number(raw).ok_or(()))
        .transpose()
}

pub(super) async fn handle(
    store: &Store,
    address: &PageAddress,
    target: HistoryTarget,
    origin: &str,
    request: Request,
) -> Result<Response, sqlx::Error> {
    let query = Form::from_query(request.uri().query().unwrap_or_default());
    let links = view::Links::new(
        address.tenant,
        address.space,
        address.page,
        query.get("preview") == "1",
    );
    match target {
        HistoryTarget::List => {
            let json = history_wants_json(request.headers());
            let mut response = list(store, address, &links, &query, json).await?;
            response
                .headers_mut()
                .insert(header::VARY, header::HeaderValue::from_static("Accept"));
            Ok(response)
        }
        HistoryTarget::Revision(revision) => {
            let Some(snapshot) = store.page_revision(address, revision).await? else {
                return Ok(missing());
            };
            let Some(history) = store.page_history(address, None).await? else {
                return Ok(missing());
            };
            Ok(html(
                StatusCode::OK,
                view::snapshot(&links, &snapshot, &history, query.get("restore") == "1"),
            ))
        }
        HistoryTarget::Cover(revision) => {
            let Some(bytes) = store.page_revision_cover(address, revision).await? else {
                return Ok(cover_unauthenticated(true));
            };
            Ok(([(header::CONTENT_TYPE, COVER_IMAGE_CONTENT_TYPE)], bytes).into_response())
        }
        HistoryTarget::Attachment {
            revision,
            id,
            inline,
        } => {
            attachments::download(
                store,
                address,
                AttachmentLookup::Revision { revision, id },
                inline,
            )
            .await
        }
        HistoryTarget::Compare => compare(store, address, &links, &query).await,
        HistoryTarget::Restore(revision) => {
            Ok(restore(store, address, &links, revision, origin, request).await)
        }
    }
}

async fn list(
    store: &Store,
    address: &PageAddress,
    links: &view::Links,
    query: &Form,
    json: bool,
) -> Result<Response, sqlx::Error> {
    let Ok(before) = revision_field(query, "before") else {
        return Ok(if json {
            history_api_error(StatusCode::BAD_REQUEST)
        } else {
            plain(StatusCode::BAD_REQUEST, INVALID_REVISION)
        });
    };
    let Some(history) = store.page_history(address, before).await? else {
        return Ok(if json {
            history_api_error(StatusCode::NOT_FOUND)
        } else {
            missing()
        });
    };
    Ok(if json {
        list_json(history, links)
    } else {
        html(StatusCode::OK, view::list(links, &history))
    })
}

async fn compare(
    store: &Store,
    address: &PageAddress,
    links: &view::Links,
    query: &Form,
) -> Result<Response, sqlx::Error> {
    match (revision_field(query, "from"), revision_field(query, "to")) {
        (Ok(Some(from)), Ok(Some(to))) => {
            let Some((from, to)) = store.compare_page_revisions(address, from, to).await? else {
                return Ok(missing());
            };
            let Some(history) = store.page_history(address, None).await? else {
                return Ok(missing());
            };
            Ok(html(
                StatusCode::OK,
                view::comparison(links, &from, &to, &history),
            ))
        }
        (Ok(None), Ok(None)) => {
            let Some(history) = store.page_history(address, None).await? else {
                return Ok(missing());
            };
            Ok(html(
                StatusCode::OK,
                view::choose_comparison(links, &history),
            ))
        }
        _ => Ok(plain(StatusCode::BAD_REQUEST, INVALID_REVISION)),
    }
}

enum RestoreFormError {
    Forbidden,
    Read(FormError),
    Timeout,
    Revision,
}

impl RestoreFormError {
    fn response(self) -> Response {
        match self {
            Self::Forbidden => plain(StatusCode::FORBIDDEN, OPEN_FORM),
            Self::Read(error) => http::form_error(error, OPEN_FORM),
            Self::Timeout => plain(
                StatusCode::REQUEST_TIMEOUT,
                "Не удалось получить форму вовремя. Попробуйте ещё раз.",
            ),
            Self::Revision => plain(StatusCode::BAD_REQUEST, INVALID_REVISION),
        }
    }
}

async fn restore_base(
    request: Request,
    links: &view::Links,
    origin: &str,
) -> Result<i64, RestoreFormError> {
    if links.preview || !http::check_origin(request.headers(), origin) {
        return Err(RestoreFormError::Forbidden);
    }
    let form = match timeout(FORM_TIMEOUT, http::read_form(request)).await {
        Ok(Ok(form)) => form,
        Ok(Err(error)) => return Err(RestoreFormError::Read(error)),
        Err(_) => return Err(RestoreFormError::Timeout),
    };
    revision_field(&form, "baseRevision")
        .ok()
        .flatten()
        .ok_or(RestoreFormError::Revision)
}

async fn restore(
    store: &Store,
    address: &PageAddress,
    links: &view::Links,
    revision: i64,
    origin: &str,
    request: Request,
) -> Response {
    let base = match restore_base(request, links, origin).await {
        Ok(base) => base,
        Err(error) => return error.response(),
    };
    let (observed, conflict) = match timeout(
        SAVE_TIMEOUT,
        store.restore_page_revision(address, revision, base),
    )
    .await
    {
        Ok(Ok(RestorePageRevision::Missing)) => return missing(),
        Ok(Ok(RestorePageRevision::Saved(saved))) => return redirect(&links.revision(saved)),
        Ok(Ok(RestorePageRevision::Conflict(current))) => (current, STALE_RESTORE),
        Ok(Ok(RestorePageRevision::NameTaken)) => (
            base,
            "В этом разделе уже есть страница с таким названием. Освободите название и повторите восстановление.",
        ),
        failure => {
            let outcome = if failure.is_err() {
                "timeout"
            } else {
                "database"
            };
            eprintln!("page.history_restore.failed category={outcome}");
            return retry_restore(links, revision, base);
        }
    };
    let history = match timeout(RECOVERY_TIMEOUT, store.page_history(address, None)).await {
        Ok(Ok(Some(history))) => history,
        Ok(Ok(None)) => return missing(),
        Ok(Err(_)) | Err(_) => return retry_restore(links, revision, base),
    };
    let conflict = if history.current_revision == observed {
        conflict
    } else {
        STALE_RESTORE
    };
    html(
        StatusCode::CONFLICT,
        view::restore_conflict(links, revision, &history, conflict),
    )
}

fn retry_restore(links: &view::Links, revision: i64, base: i64) -> Response {
    html(
        StatusCode::SERVICE_UNAVAILABLE,
        view::restore_retry(links, revision, base, RESTORE_UNAVAILABLE),
    )
}

pub(crate) async fn retry_history_restore_before_authorization(
    route: DocumentsRoute,
    page: Uuid,
    revision: i64,
    origin: &str,
    request: Request,
) -> Response {
    let query = Form::from_query(request.uri().query().unwrap_or_default());
    let links = view::Links::new(route.tenant, route.space, page, query.get("preview") == "1");
    match restore_base(request, &links, origin).await {
        Ok(base) => retry_restore(&links, revision, base),
        Err(error) => error.response(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_revisions_reject_duplicate_and_noncanonical_numbers() {
        assert_eq!(
            revision_field(&Form::from_query("before=42"), "before"),
            Ok(Some(42))
        );
        for query in [
            "before=042",
            "before=0",
            "before=42&before=42",
            "before=9223372036854775808",
        ] {
            assert_eq!(revision_field(&Form::from_query(query), "before"), Err(()));
        }
    }
}
