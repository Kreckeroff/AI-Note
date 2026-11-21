use std::{error::Error, fmt::Write};

use axum::{
    body::{Body, to_bytes},
    extract::Request,
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use http_body_util::LengthLimitError;

use crate::{teamstream::IsolatedOrigin, text::escape_html};

const CONTENT_POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";

pub(crate) fn html(status: StatusCode, body: String) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        body,
    )
        .into_response()
}

pub(crate) fn html_with_external_frames(
    status: StatusCode,
    mut body: String,
    teamstream: Option<&IsolatedOrigin>,
) -> Response {
    let policy = if let Some((origin, head_end)) = teamstream.zip(body.find("</head>")) {
        // Frame-enabled routes use a server-owned template head. The wrapper
        // configuration travels with its CSP so the two cannot disagree.
        let meta = format!(
            r#"<meta name="sever-teamstream-origin" content="{}">"#,
            escape_html(origin.as_str())
        );
        body.insert_str(head_end, &meta);
        format!("{CONTENT_POLICY}; frame-src https: {}", origin.as_str())
    } else {
        format!("{CONTENT_POLICY}; frame-src https:")
    };
    let mut response = html(status, body);
    let policy = HeaderValue::from_str(&policy)
        .expect("fixed content security policy with validated player origin");
    response
        .headers_mut()
        .insert(header::CONTENT_SECURITY_POLICY, policy);
    response
}

pub(crate) fn plain(status: StatusCode, message: &str) -> Response {
    (status, message.to_owned()).into_response()
}

pub(crate) fn content_disposition(name: &str, inline: bool) -> String {
    let disposition = if inline { "inline" } else { "attachment" };
    let mut value = format!("{disposition}; filename=\"attachment\"; filename*=UTF-8''");
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            value.push(char::from(byte));
        } else {
            let _ = write!(value, "%{byte:02X}");
        }
    }
    value
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
    headers
        .entry(header::CONTENT_SECURITY_POLICY)
        .or_insert(HeaderValue::from_static(CONTENT_POLICY));
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

pub(crate) fn media_type(headers: &HeaderMap) -> Option<&str> {
    single_header(headers, header::CONTENT_TYPE)
        .and_then(|value| value.split(';').next())
        .map(str::trim)
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
        self.field(name).unwrap_or_default().unwrap_or_default()
    }

    pub(crate) fn field(&self, name: &str) -> Result<Option<&str>, &str> {
        let mut values = self
            .0
            .iter()
            .filter(|(key, _)| key == name)
            .map(|(_, value)| value.as_str());
        let Some(value) = values.next() else {
            return Ok(None);
        };
        if values.next().is_some() {
            Err(value)
        } else {
            Ok(Some(value))
        }
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
    read_form_bounded(request, 4096).await
}

pub(crate) async fn read_form_bounded(
    request: Request,
    max_bytes: usize,
) -> Result<Form, FormError> {
    let encoding = media_type(request.headers())
        .is_some_and(|value| value.eq_ignore_ascii_case("application/x-www-form-urlencoded"));
    if !encoding {
        return Err(FormError::WrongEncoding);
    }
    let bytes = to_bytes(request.into_body(), max_bytes)
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

#[cfg(test)]
mod frame_policy_tests {
    use super::*;

    #[test]
    fn frame_policy_is_explicit_and_keeps_ancestors_closed() {
        let mut ordinary = html(StatusCode::OK, "ordinary".into());
        let mut document = html_with_external_frames(StatusCode::OK, "document".into(), None);
        finish(&mut ordinary, false);
        finish(&mut document, false);
        let policy = |response: &Response| {
            response.headers()[header::CONTENT_SECURITY_POLICY]
                .to_str()
                .unwrap()
                .to_owned()
        };
        assert_eq!(policy(&ordinary), CONTENT_POLICY);
        assert_eq!(
            policy(&document),
            format!("{CONTENT_POLICY}; frame-src https:")
        );
        assert!(policy(&document).contains("frame-ancestors 'none'"));
    }

    #[tokio::test]
    async fn configured_documents_receive_only_the_exact_player_origin() {
        let origin = IsolatedOrigin::parse("http://127.0.0.1:4124").unwrap();
        let document = "<html><head><title>Page</title></head><body></body></html>";
        let response = html_with_external_frames(StatusCode::OK, document.into(), Some(&origin));
        let csp = response.headers()[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap();
        assert!(csp.contains("frame-ancestors 'none'"));
        assert!(csp.ends_with("frame-src https: http://127.0.0.1:4124"));
        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        assert!(std::str::from_utf8(&body).unwrap().contains(
            r#"<meta name="sever-teamstream-origin" content="http://127.0.0.1:4124"></head>"#
        ));
    }
}
