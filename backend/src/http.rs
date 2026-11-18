use std::error::Error;

use axum::{
    body::{Body, to_bytes},
    extract::Request,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use http_body_util::LengthLimitError;

pub(crate) fn html(status: StatusCode, body: String) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body,
    )
        .into_response()
}

pub(crate) fn plain(status: StatusCode, message: &str) -> Response {
    (status, message.to_owned()).into_response()
}

pub(crate) fn redirect(location: &str) -> Response {
    (StatusCode::SEE_OTHER, [(header::LOCATION, location)]).into_response()
}

pub(crate) fn method_not_allowed(allow: &'static str) -> Response {
    let mut response = plain(StatusCode::METHOD_NOT_ALLOWED, "Метод недоступен.");
    response
        .headers_mut()
        .insert(header::ALLOW, HeaderValue::from_static(allow));
    response
}

pub(crate) fn finish(response: &mut Response, head: bool) {
    let headers = response.headers_mut();
    headers
        .entry(header::CACHE_CONTROL)
        .or_insert(HeaderValue::from_static("no-store"));
    headers.entry(header::CONTENT_SECURITY_POLICY).or_insert(HeaderValue::from_static(
        "default-src 'none'; script-src 'self'; style-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'",
    ));
    headers
        .entry(header::X_CONTENT_TYPE_OPTIONS)
        .or_insert(HeaderValue::from_static("nosniff"));
    headers
        .entry(header::REFERRER_POLICY)
        .or_insert(HeaderValue::from_static("same-origin"));
    if head {
        *response.body_mut() = Body::empty();
    }
}

pub(crate) fn check_origin(headers: &HeaderMap, origin: &str) -> bool {
    single_header(headers, header::ORIGIN) == Some(origin)
}

pub(crate) fn single_header(headers: &HeaderMap, name: header::HeaderName) -> Option<&str> {
    let mut values = headers.get_all(name).iter();
    let value = values.next()?.to_str().ok()?;
    values.next().is_none().then_some(value)
}

pub(crate) struct Form(Vec<(String, String)>);

impl Form {
    pub(crate) fn from_query(query: &str) -> Self {
        Self(
            url::form_urlencoded::parse(query.as_bytes())
                .into_owned()
                .collect(),
        )
    }

    pub(crate) fn get(&self, name: &str) -> &str {
        let mut values = self
            .0
            .iter()
            .filter(|(key, _)| key == name)
            .map(|(_, value)| value.as_str());
        let value = values.next().unwrap_or_default();
        if values.next().is_some() { "" } else { value }
    }
}

pub(crate) enum FormError {
    TooLarge,
    WrongEncoding,
    Unreadable,
}

pub(crate) fn form_error(error: FormError, wrong_encoding: &str) -> Response {
    match error {
        FormError::TooLarge => {
            let mut response = plain(StatusCode::PAYLOAD_TOO_LARGE, "Форма слишком большая.");
            response
                .headers_mut()
                .insert(header::CONNECTION, HeaderValue::from_static("close"));
            response
        }
        FormError::WrongEncoding => plain(StatusCode::UNSUPPORTED_MEDIA_TYPE, wrong_encoding),
        FormError::Unreadable => plain(StatusCode::BAD_REQUEST, "Не удалось прочитать форму."),
    }
}

pub(crate) async fn read_form(request: Request) -> Result<Form, FormError> {
    const MAX_FORM_BYTES: usize = 4096;
    let encoding = single_header(request.headers(), header::CONTENT_TYPE)
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .is_some_and(|value| value.eq_ignore_ascii_case("application/x-www-form-urlencoded"));
    if !encoding {
        return Err(FormError::WrongEncoding);
    }
    let bytes = to_bytes(request.into_body(), MAX_FORM_BYTES)
        .await
        .map_err(|error| {
            if error
                .source()
                .is_some_and(|source| source.is::<LengthLimitError>())
            {
                FormError::TooLarge
            } else {
                FormError::Unreadable
            }
        })?;
    Ok(Form::from_query(&String::from_utf8_lossy(&bytes)))
}
