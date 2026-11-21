use axum::{
    Json,
    extract::Request,
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Serialize;

use crate::{
    db::{
        PageAddress, Store,
        page_rename::{PageName, RenamePage},
    },
    http,
};

#[derive(Serialize)]
struct NameResponse {
    title: String,
    revision: String,
}

fn name_response(status: StatusCode, name: PageName) -> Response {
    (
        status,
        Json(NameResponse {
            title: name.title,
            revision: name.revision.to_string(),
        }),
    )
        .into_response()
}

pub(super) async fn handle(
    store: &Store,
    address: &PageAddress,
    origin: &str,
    request: Request,
) -> Result<Response, sqlx::Error> {
    let outcome = if request.method() == Method::POST {
        if !http::check_origin(request.headers(), origin) {
            return Ok(http::plain(
                StatusCode::FORBIDDEN,
                "Откройте форму переименования в «Севере».",
            ));
        }
        let form = match http::read_form(request).await {
            Ok(form) => form,
            Err(error) => {
                return Ok(http::form_error(
                    error,
                    "Отправьте название из формы в «Севере».",
                ));
            }
        };
        let revision = form.get("base_revision").parse::<i64>();
        let (Ok(revision), Ok(Some(title))) = (revision, form.field("title")) else {
            return Ok(http::plain(
                StatusCode::BAD_REQUEST,
                "Проверьте название и редакцию страницы.",
            ));
        };
        if revision <= 0 {
            return Ok(http::plain(
                StatusCode::BAD_REQUEST,
                "Укажите редакцию страницы.",
            ));
        }
        store.rename_page(address, revision, title).await?
    } else {
        store.page_name(address).await?
    };
    Ok(match outcome {
        RenamePage::Named(name) => name_response(StatusCode::OK, name),
        RenamePage::Stale(name) => name_response(StatusCode::CONFLICT, name),
        RenamePage::Missing => http::plain(StatusCode::NOT_FOUND, "Страница не найдена."),
        RenamePage::Forbidden => http::plain(
            StatusCode::FORBIDDEN,
            "У вас нет права переименовывать эту страницу.",
        ),
        RenamePage::InvalidName => http::plain(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Введите название от 1 до 100 символов в одну строку.",
        ),
        RenamePage::Duplicate => http::plain(
            StatusCode::UNPROCESSABLE_ENTITY,
            "Такое название уже есть в разделе.",
        ),
    })
}
