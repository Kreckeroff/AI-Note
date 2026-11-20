use std::{error::Error, time::Duration};

use axum::{
    Json,
    body::to_bytes,
    extract::Request,
    http::{HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use http_body_util::LengthLimitError;
use serde::Serialize;
use uuid::Uuid;

use crate::{
    db::{
        PageAddress, Store,
        page_discussion::{
            CommentCommand, DiscussionOutcome, DiscussionStatus, MentionLookup, ReplyPage,
            ThreadQuery, ThreadSelection,
        },
    },
    http::{self, Form},
    workspace::{canonical_uuid, page_history_path, page_path},
};

const MAX_JSON_BYTES: usize = 512 * 1024;
const MAX_OFFSET: i64 = i32::MAX as i64;
const BODY_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Serialize)]
struct DiscussionError {
    error: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
}

fn problem(status: StatusCode, message: &'static str, code: Option<&'static str>) -> Response {
    (
        status,
        Json(DiscussionError {
            error: message,
            code,
        }),
    )
        .into_response()
}

pub(crate) fn error(status: StatusCode) -> Response {
    let message = match status {
        StatusCode::BAD_REQUEST => "Проверьте поля запроса.",
        StatusCode::UNAUTHORIZED => "Войдите в «Север» ещё раз.",
        StatusCode::FORBIDDEN => "У вас нет права на это действие.",
        StatusCode::NOT_FOUND => "Страница или обсуждение недоступны.",
        _ => "Не удалось загрузить обсуждение. Попробуйте ещё раз.",
    };
    problem(status, message, None)
}

pub(crate) async fn handle(
    store: &Store,
    address: &PageAddress,
    origin: &str,
    request: Request,
) -> Result<Response, sqlx::Error> {
    match *request.method() {
        Method::GET | Method::HEAD => read(store, address, request).await,
        Method::POST => write(store, address, origin, request).await,
        _ => Ok(http::method_not_allowed("GET, HEAD, POST")),
    }
}

enum DiscussionQuery<'a> {
    Threads(ThreadQuery),
    Mentions(&'a str),
}

fn offset(value: Option<&str>) -> Result<i64, ()> {
    let Some(value) = value else { return Ok(0) };
    value
        .parse()
        .ok()
        .filter(|parsed| {
            (0..=MAX_OFFSET).contains(parsed) && value.bytes().all(|byte| byte.is_ascii_digit())
        })
        .ok_or(())
}

fn parse_query(form: &Form) -> Result<DiscussionQuery<'_>, ()> {
    let get = |key| form.field(key).map_err(|_| ());
    if let Some(search) = get("mentions")? {
        if search.contains('\0') || search.chars().count() > 120 {
            return Err(());
        }
        return Ok(DiscussionQuery::Mentions(search));
    }
    let selected = get("thread")?
        .map(|id| canonical_uuid(id).ok_or(()))
        .transpose()?;
    let threads = if let Some(id) = selected {
        let reply = get("reply")?
            .map(|id| canonical_uuid(id).ok_or(()))
            .transpose()?;
        let start = get("repliesOffset")?;
        let replies = match (reply, start) {
            (Some(reply), None) => ReplyPage::Target(reply),
            (None, start) => ReplyPage::Start(offset(start)?),
            (Some(_), Some(_)) => return Err(()),
        };
        ThreadQuery::One(ThreadSelection { id, replies })
    } else {
        let status = match get("status")? {
            None | Some("active") => DiscussionStatus::Active,
            Some("resolved") => DiscussionStatus::Resolved,
            _ => return Err(()),
        };
        ThreadQuery::Filter {
            status,
            offset: offset(get("offset")?)?,
        }
    };
    Ok(DiscussionQuery::Threads(threads))
}

async fn read(
    store: &Store,
    address: &PageAddress,
    request: Request,
) -> Result<Response, sqlx::Error> {
    let form = Form::from_query(request.uri().query().unwrap_or_default());
    let Ok(query) = parse_query(&form) else {
        return Ok(error(StatusCode::BAD_REQUEST));
    };
    match query {
        DiscussionQuery::Mentions(search) => {
            Ok(match store.discussion_mentions(address, search).await? {
                MentionLookup::Found(data) => Json(data).into_response(),
                MentionLookup::Missing => error(StatusCode::NOT_FOUND),
                MentionLookup::Denied => error(StatusCode::FORBIDDEN),
            })
        }
        DiscussionQuery::Threads(query) => {
            let Some(mut result) = store.discussion_threads(address, query).await? else {
                return Ok(error(StatusCode::NOT_FOUND));
            };
            let history_path = page_history_path(address.tenant, address.space, address.page);
            for thread in &mut result.threads {
                if let Some(anchor) = &mut thread.anchor {
                    anchor.history_to = anchor.historical_revision.map(|revision| {
                        format!(
                            "{history_path}/{revision}?comment={}#history-quote",
                            thread.id
                        )
                    });
                }
            }
            Ok(Json(result).into_response())
        }
    }
}

async fn write(
    store: &Store,
    address: &PageAddress,
    origin: &str,
    request: Request,
) -> Result<Response, sqlx::Error> {
    if !http::check_origin(request.headers(), origin) {
        return Ok(error(StatusCode::FORBIDDEN));
    }
    let is_json = http::single_header(request.headers(), header::CONTENT_TYPE)
        .and_then(|content_type| content_type.split(';').next())
        .is_some_and(|media| media.trim().eq_ignore_ascii_case("application/json"));
    if !is_json {
        return Ok(problem(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "Ожидается JSON.",
            None,
        ));
    }
    let bytes =
        match tokio::time::timeout(BODY_TIMEOUT, to_bytes(request.into_body(), MAX_JSON_BYTES))
            .await
        {
            Ok(Ok(bytes)) => bytes,
            Ok(Err(error))
                if error
                    .source()
                    .is_some_and(|cause| cause.is::<LengthLimitError>()) =>
            {
                return Ok(problem(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "Запрос слишком большой.",
                    None,
                ));
            }
            Ok(Err(_)) => return Ok(error(StatusCode::BAD_REQUEST)),
            Err(_) => {
                return Ok(problem(
                    StatusCode::REQUEST_TIMEOUT,
                    "Не удалось получить запрос вовремя.",
                    None,
                ));
            }
        };
    let Ok(command) = serde_json::from_slice::<CommentCommand>(&bytes) else {
        return Ok(error(StatusCode::BAD_REQUEST));
    };
    let action = command.action();
    let request_id = Uuid::new_v4();
    let result = store.write_comment(address, command).await;
    let outcome = result
        .as_ref()
        .map_or("unavailable", DiscussionOutcome::outcome);
    eprintln!(
        "page.discussion action={action} actor={} tenant={} space={} page={} outcome={outcome} request={request_id}",
        address.account, address.tenant, address.space, address.page
    );
    let mut response = match result? {
        DiscussionOutcome::Applied(result) => Json(result).into_response(),
        DiscussionOutcome::Missing => error(StatusCode::NOT_FOUND),
        DiscussionOutcome::Denied => error(StatusCode::FORBIDDEN),
        DiscussionOutcome::Invalid => problem(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Укажите текст комментария длиной от 1 до 500 символов.",
            Some("invalidComment"),
        ),
        DiscussionOutcome::AnchorConflict => problem(
            StatusCode::CONFLICT,
            "Опубликованный текст изменился. Откройте актуальную страницу и выделите фрагмент снова.",
            Some("anchorConflict"),
        ),
        DiscussionOutcome::ThreadConflict => problem(
            StatusCode::CONFLICT,
            "Состояние ветки изменилось. Откройте актуальное обсуждение.",
            Some("threadConflict"),
        ),
        DiscussionOutcome::OperationConflict => problem(
            StatusCode::CONFLICT,
            "Этот ключ уже связан с другой операцией. Обновите обсуждение.",
            Some("operationConflict"),
        ),
        DiscussionOutcome::MentionUnavailable => problem(
            StatusCode::CONFLICT,
            "У одного из упомянутых читателей больше нет доступа. Измените упоминания.",
            Some("mentionUnavailable"),
        ),
    };
    if let Ok(value) = HeaderValue::from_str(&request_id.to_string()) {
        response.headers_mut().insert("x-request-id", value);
    }
    Ok(response)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NotificationItem {
    id: Uuid,
    author: String,
    href: String,
    created_at: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NotificationPage {
    notifications: Vec<NotificationItem>,
    next_offset: Option<i64>,
}

fn discussion_href(page: &str, thread: Uuid, comment: Uuid) -> String {
    if thread == comment {
        format!("{page}?thread={thread}#comment-{comment}")
    } else {
        format!("{page}?thread={thread}&comment={comment}#comment-{comment}")
    }
}

pub(crate) async fn notifications(
    store: &Store,
    account: Uuid,
    request: Request,
) -> Result<Response, sqlx::Error> {
    if !matches!(*request.method(), Method::GET | Method::HEAD) {
        return Ok(http::method_not_allowed("GET, HEAD"));
    }
    let query = Form::from_query(request.uri().query().unwrap_or_default());
    let Ok(offset) = query.field("offset").map_err(|_| ()).and_then(offset) else {
        return Ok(error(StatusCode::BAD_REQUEST));
    };
    let result = store.discussion_notifications(account, offset).await?;
    let notifications = result
        .notifications
        .into_iter()
        .map(|item| {
            let path = page_path(item.tenant_id, item.space_id, item.page_id);
            NotificationItem {
                id: item.id,
                author: item.author,
                href: discussion_href(&path, item.thread_id, item.message_id),
                created_at: item.created_at,
            }
        })
        .collect();
    Ok(Json(NotificationPage {
        notifications,
        next_offset: result.next_offset,
    })
    .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_rejects_malformed_anchors_and_duplicate_filters() {
        for query in [
            "offset=-1",
            "thread=wrong",
            "status=active&status=resolved",
            "mentions=%00",
        ] {
            assert!(parse_query(&Form::from_query(query)).is_err(), "{query}");
        }
    }

    #[test]
    fn reply_notification_keeps_the_target_in_the_request_url() {
        let thread = Uuid::new_v4();
        let reply = Uuid::new_v4();
        let page = "/workspace/page";
        assert_eq!(
            discussion_href(page, thread, reply),
            format!("{page}?thread={thread}&comment={reply}#comment-{reply}")
        );
        assert_eq!(
            discussion_href(page, thread, thread),
            format!("{page}?thread={thread}#comment-{thread}")
        );
    }

    #[test]
    fn wire_command_uses_page_scoped_stable_ids() {
        let thread_id = Uuid::new_v4();
        let operation_id = Uuid::new_v4();
        let input = serde_json::json!({
            "action": "resolve", "threadId": thread_id, "version": 2, "operationId": operation_id,
        });
        let command: CommentCommand = serde_json::from_value(input).unwrap();
        assert!(matches!(
            command,
            CommentCommand::Resolve { thread_id: id, operation_id: key, version: 2 }
                if id == thread_id && key == operation_id
        ));
    }
}
