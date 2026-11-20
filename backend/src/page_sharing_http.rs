use axum::{
    Json,
    body::to_bytes,
    extract::Request,
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::json;

use crate::{
    db::{
        PageAddress, Store,
        page_sharing::{PageSharingAction, PageSharingOutcome, SHARING_PAGE_SIZE},
    },
    http::{self, Form, html},
    views,
    workspace::{canonical_uuid, page_path},
};

pub(crate) async fn handle(
    store: &Store,
    address: &PageAddress,
    origin: &str,
    request: Request,
) -> Result<Response, sqlx::Error> {
    if request.method() == Method::POST {
        submit(store, address, origin, request).await
    } else {
        list(
            store,
            address,
            Form::from_query(request.uri().query().unwrap_or_default()),
        )
        .await
    }
}

async fn list(store: &Store, address: &PageAddress, query: Form) -> Result<Response, sqlx::Error> {
    let permissions = store.page_permissions(address).await?;
    if !permissions
        .as_ref()
        .is_some_and(|permission| permission.can_manage)
    {
        return Ok(
            if permissions.is_some_and(|permission| permission.can_read) {
                error(StatusCode::FORBIDDEN, "У вас нет права управлять доступом.")
            } else {
                error(StatusCode::NOT_FOUND, "Страница не найдена.")
            },
        );
    }
    let offset = query
        .get("offset")
        .parse::<i64>()
        .unwrap_or(0)
        .clamp(0, 1_000_000);
    if query.get("tab") == "requests" {
        let mut requests = store.page_access_requests(address, offset).await?;
        let next = paginate(&mut requests, offset);
        return Ok(
            Json(json!({ "people": [], "requests": requests, "nextOffset": next })).into_response(),
        );
    }
    let entered = query.get("q").trim();
    let search: String = entered.chars().filter(|&c| c != '\0').take(254).collect();
    let mut people = store
        .page_people(address, &search, query.get("filter"), offset)
        .await?;
    let next = paginate(&mut people, offset);
    Ok(Json(json!({ "people": people, "requests": [], "nextOffset": next })).into_response())
}

async fn submit(
    store: &Store,
    address: &PageAddress,
    origin: &str,
    request: Request,
) -> Result<Response, sqlx::Error> {
    if !http::check_origin(request.headers(), origin) {
        return Ok(error(
            StatusCode::FORBIDDEN,
            "Откройте управление доступом в «Севере».",
        ));
    }
    let content_type = http::media_type(request.headers()).map(str::to_ascii_lowercase);
    let content_type = content_type.as_deref();
    let is_form = content_type == Some("application/x-www-form-urlencoded");
    let action = match content_type {
        Some("application/x-www-form-urlencoded") => http::read_form(request)
            .await
            .ok()
            .filter(|form| form.get("action") == "request")
            .and_then(|form| canonical_uuid(form.get("operation_id")))
            .map(|operation_id| PageSharingAction::Request { operation_id }),
        Some("application/json") => to_bytes(request.into_body(), 8192)
            .await
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok()),
        _ => {
            return Ok(error(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "Неверный формат операции.",
            ));
        }
    };
    let Some(action) = action else {
        return Ok(error(
            StatusCode::BAD_REQUEST,
            "Не удалось прочитать операцию.",
        ));
    };
    let outcome = store.change_page_sharing(address, &action).await?;
    let status = match &outcome {
        PageSharingOutcome::Saved(_) => "saved",
        PageSharingOutcome::Accepted => "accepted",
        PageSharingOutcome::Hidden => "not-found",
        PageSharingOutcome::Forbidden => "denied",
        PageSharingOutcome::InvalidTarget => "invalid-target",
        PageSharingOutcome::Conflict => "conflict",
    };
    eprintln!(
        "page.sharing actor={} tenant={} space={} page={} action={} operation={} outcome={status}",
        address.account,
        address.tenant,
        address.space,
        address.page,
        action.name(),
        action.operation(),
    );
    Ok(match outcome {
        PageSharingOutcome::Saved(person) => {
            Json(json!({ "status": "saved", "person": person })).into_response()
        }
        PageSharingOutcome::Accepted if is_form => {
            let target = format!(
                "{}/sharing",
                page_path(address.tenant, address.space, address.page)
            );
            html(StatusCode::OK, views::page_denied(Some(&target), true))
        }
        PageSharingOutcome::Accepted => Json(json!({ "status": "accepted" })).into_response(),
        PageSharingOutcome::Hidden => error(StatusCode::NOT_FOUND, "Страница не найдена."),
        PageSharingOutcome::Forbidden => {
            error(StatusCode::FORBIDDEN, "У вас нет права управлять доступом.")
        }
        PageSharingOutcome::InvalidTarget => error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Выбранный участник или запрос недоступен.",
        ),
        PageSharingOutcome::Conflict => error(
            StatusCode::CONFLICT,
            "Операция уже использована. Обновите данные.",
        ),
    })
}

fn error(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn paginate<T>(rows: &mut Vec<T>, offset: i64) -> Option<i64> {
    let next = (rows.len() > SHARING_PAGE_SIZE).then_some(offset + SHARING_PAGE_SIZE as i64);
    rows.truncate(SHARING_PAGE_SIZE);
    next
}
