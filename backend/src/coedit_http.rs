use std::time::{Duration, Instant};

use axum::{
    Json,
    body::to_bytes,
    extract::Request,
    http::{HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use uuid::Uuid;

use crate::{
    coedit_presence::PresenceTracker,
    db::{
        PageAddress, Store,
        coediting::{CoeditResult, DraftCommand, DraftSnapshot},
    },
    http::{self, Form},
    identity::session_digest,
    workspace::canonical_uuid,
};

const FORM_LIMIT: usize = 1_000_000;

#[derive(Clone, Copy)]
pub(crate) enum CoeditSurface {
    Draft,
    Presence,
    Watch,
}

pub(crate) async fn handle(
    store: &Store,
    presence: &PresenceTracker,
    address: &PageAddress,
    surface: CoeditSurface,
    origin: &str,
    request: Request,
) -> Result<Response, sqlx::Error> {
    match (surface, request.method().clone()) {
        (CoeditSurface::Draft, Method::GET | Method::HEAD) => {
            draft_state(store, address, request).await
        }
        (CoeditSurface::Draft, Method::POST) => edit_draft(store, address, origin, request).await,
        (CoeditSurface::Presence, Method::POST) => {
            heartbeat(store, presence, address, origin, request).await
        }
        (CoeditSurface::Watch, Method::GET | Method::HEAD) => watch_state(store, address).await,
        (CoeditSurface::Watch, Method::POST) => change_watch(store, address, origin, request).await,
        (CoeditSurface::Presence, _) => Ok(method_not_allowed("POST")),
        _ => Ok(method_not_allowed("GET, HEAD, POST")),
    }
}

async fn draft_state(
    store: &Store,
    address: &PageAddress,
    request: Request,
) -> Result<Response, sqlx::Error> {
    let Some(session) = session_digest(request.headers()) else {
        return Ok(sign_in());
    };
    let query = Form::from_query(request.uri().query().unwrap_or_default());
    let Some(lease) = canonical_uuid(query.get("lease")) else {
        return Ok(bad_input());
    };
    let after = match query.field("after") {
        Ok(None) => None,
        Ok(Some(value)) => {
            let Some(version) = draft_version(value) else {
                return Ok(bad_input());
            };
            Some(version)
        }
        Err(_) => return Ok(bad_input()),
    };
    let result = store
        .coedit_state_after(address, &session, lease, after)
        .await?;
    Ok(draft_response(result, after))
}

fn draft_version(value: &str) -> Option<i64> {
    value
        .parse::<i64>()
        .ok()
        .filter(|number| *number >= 0 && number.to_string() == value)
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
enum DraftAction {
    Open,
    Apply(DraftCommand),
}

async fn edit_draft(
    store: &Store,
    address: &PageAddress,
    origin: &str,
    request: Request,
) -> Result<Response, sqlx::Error> {
    let Some(session) = session_digest(request.headers()) else {
        return Ok(sign_in());
    };
    let command = match command::<DraftAction>(origin, request).await {
        Ok(command) => command,
        Err(rejection) => return Ok(rejection.response()),
    };
    let (result, operation) = match command {
        DraftAction::Open => (store.open_coedit(address, &session).await?, "open"),
        DraftAction::Apply(input) => (store.apply_coedit(address, &session, input).await?, "apply"),
    };
    let response = draft_response(result, None);
    if operation == "open" || !response.status().is_success() {
        eprintln!(
            "page.coedit operation={operation} tenant={} space={} page={} account={} status={}",
            address.tenant,
            address.space,
            address.page,
            address.account,
            response.status().as_u16()
        );
    }
    Ok(response)
}

fn draft_response(result: CoeditResult<DraftSnapshot>, after: Option<i64>) -> Response {
    match result {
        CoeditResult::Saved(snapshot) if Some(snapshot.version) == after => {
            StatusCode::NO_CONTENT.into_response()
        }
        CoeditResult::Saved(snapshot) => Json(snapshot).into_response(),
        CoeditResult::Missing => missing(),
        CoeditResult::Forbidden => error(
            StatusCode::FORBIDDEN,
            "readOnly",
            "Доступ к редактированию изменился.",
        ),
        CoeditResult::LeaseExpired => error(
            StatusCode::CONFLICT,
            "leaseExpired",
            "Откройте черновик заново и проверьте ожидающие правки.",
        ),
        CoeditResult::Invalid => error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalidOperation",
            "Проверьте изменение и доступность вложений.",
        ),
        CoeditResult::Conflict(conflict) => (
            StatusCode::CONFLICT,
            Json(ConflictBody {
                code: "conflict",
                error: "Нужно совместить правки.",
                data: conflict,
            }),
        )
            .into_response(),
        CoeditResult::BaseExpired(snapshot) => (
            StatusCode::CONFLICT,
            Json(ConflictBody {
                code: "baseExpired",
                error: "Начальная версия правки устарела. Сверьте её с общим черновиком.",
                data: snapshot,
            }),
        )
            .into_response(),
        CoeditResult::OperationConflict => error(
            StatusCode::CONFLICT,
            "operationConflict",
            "Этот ключ уже использован для другой правки.",
        ),
    }
}

#[derive(Serialize)]
struct ConflictBody<T> {
    code: &'static str,
    error: &'static str,
    #[serde(flatten)]
    data: T,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Heartbeat {
    tab_id: Uuid,
    mode: TabMode,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
enum TabMode {
    Read,
    Edit,
    Leave,
}

async fn heartbeat(
    store: &Store,
    presence: &PresenceTracker,
    address: &PageAddress,
    origin: &str,
    request: Request,
) -> Result<Response, sqlx::Error> {
    let received = Instant::now();
    let Some(session) = session_digest(request.headers()) else {
        return Ok(sign_in());
    };
    let heartbeat = match command::<Heartbeat>(origin, request).await {
        Ok(heartbeat) => heartbeat,
        Err(rejection) => return Ok(rejection.response()),
    };
    let peers = presence.active(address, received);
    let Some(view) = store.page_presence(address, &session, &peers).await? else {
        return Ok(missing());
    };
    let editing = match heartbeat.mode {
        TabMode::Leave => None,
        TabMode::Edit => Some(view.can_edit),
        TabMode::Read => Some(false),
    };
    // Use the initial time so an older completion cannot resurrect that tab.
    presence.record(address, session, heartbeat.tab_id, editing, received);
    Ok(Json(view).into_response())
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Watching {
    following: bool,
}

async fn watch_state(store: &Store, address: &PageAddress) -> Result<Response, sqlx::Error> {
    let watching = store.page_watch(address).await?;
    Ok(watching.map_or_else(missing, |following| {
        Json(Watching { following }).into_response()
    }))
}

async fn change_watch(
    store: &Store,
    address: &PageAddress,
    origin: &str,
    request: Request,
) -> Result<Response, sqlx::Error> {
    let input = match command::<Watching>(origin, request).await {
        Ok(input) => input,
        Err(rejection) => return Ok(rejection.response()),
    };
    let following = store.set_page_watch(address, input.following).await?;
    Ok(following.map_or_else(missing, |following| {
        Json(Watching { following }).into_response()
    }))
}

enum CommandRejection {
    Origin,
    Media,
    Size,
    Timeout,
    Invalid,
}
impl CommandRejection {
    fn response(self) -> Response {
        match self {
            Self::Origin => error(
                StatusCode::FORBIDDEN,
                "origin",
                "Откройте страницу в «Севере».",
            ),
            Self::Media => error(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "mediaType",
                "Требуется JSON.",
            ),
            Self::Size => error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "bodySize",
                "Запрос слишком большой.",
            ),
            Self::Timeout => error(
                StatusCode::REQUEST_TIMEOUT,
                "timeout",
                "Запрос не удалось получить вовремя.",
            ),
            Self::Invalid => bad_input(),
        }
    }
}

async fn command<T: DeserializeOwned>(
    origin: &str,
    request: Request,
) -> Result<T, CommandRejection> {
    if !http::check_origin(request.headers(), origin) {
        return Err(CommandRejection::Origin);
    }
    if !http::media_type(request.headers())
        .is_some_and(|media| media.eq_ignore_ascii_case("application/json"))
    {
        return Err(CommandRejection::Media);
    }
    let bytes = tokio::time::timeout(
        Duration::from_secs(5),
        to_bytes(request.into_body(), FORM_LIMIT),
    )
    .await
    .map_err(|_| CommandRejection::Timeout)?
    .map_err(|failure| {
        if std::error::Error::source(&failure)
            .is_some_and(|cause| cause.is::<http_body_util::LengthLimitError>())
        {
            CommandRejection::Size
        } else {
            CommandRejection::Invalid
        }
    })?;
    serde_json::from_slice(&bytes).map_err(|_| CommandRejection::Invalid)
}

fn error(status: StatusCode, code: &'static str, message: &'static str) -> Response {
    (
        status,
        Json(serde_json::json!({"code":code,"error":message})),
    )
        .into_response()
}
fn sign_in() -> Response {
    error(StatusCode::UNAUTHORIZED, "signIn", "Войдите в «Север».")
}
fn missing() -> Response {
    error(
        StatusCode::NOT_FOUND,
        "pageUnavailable",
        "Страница недоступна.",
    )
}
fn bad_input() -> Response {
    error(
        StatusCode::BAD_REQUEST,
        "badInput",
        "Проверьте поля запроса.",
    )
}
fn method_not_allowed(allow: &'static str) -> Response {
    let mut response = error(
        StatusCode::METHOD_NOT_ALLOWED,
        "method",
        "Метод недоступен.",
    );
    response
        .headers_mut()
        .insert(header::ALLOW, HeaderValue::from_static(allow));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_draft_version_includes_zero() {
        assert_eq!(draft_version("0"), Some(0));
        assert_eq!(draft_version("1"), Some(1));
        assert_eq!(draft_version("01"), None);
        assert_eq!(draft_version("-1"), None);
    }
}
