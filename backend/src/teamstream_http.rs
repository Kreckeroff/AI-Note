use std::{
    env,
    net::{Ipv4Addr, SocketAddrV4},
    sync::Arc,
};

use anyhow::Context;
use axum::{
    Router,
    extract::{Request, State},
    http::{HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use tokio::net::TcpListener;
use url::Position;

use crate::{
    http::{self, html, method_not_allowed, plain},
    teamstream::{IsolatedOrigin, PROVIDER, Video, bare_origin},
};

const PLAYER_CSS: &str = "html,body{height:100%;margin:0;overflow:hidden;background:#101626}iframe{display:block;width:100%;height:100%;border:0}";

struct Wrapper {
    host: String,
    parent: String,
}

impl Wrapper {
    fn response(&self, request: &Request) -> Response {
        let mut response =
            if http::single_header(request.headers(), header::HOST) != Some(self.host.as_str()) {
                plain(StatusCode::FORBIDDEN, "Неверный адрес плеера.")
            } else if !matches!(*request.method(), Method::GET | Method::HEAD) {
                method_not_allowed("GET, HEAD")
            } else {
                let query = request.uri().query();
                match request.uri().path() {
                    "/health" if query.is_none() => plain(StatusCode::OK, "ok"),
                    "/player.css" if query.is_none() => (
                        [(header::CONTENT_TYPE, "text/css; charset=utf-8")],
                        PLAYER_CSS,
                    )
                        .into_response(),
                    path => Video::from_target(path, query).map_or_else(
                        || plain(StatusCode::NOT_FOUND, "Видео не найдено."),
                        |video| self.player(&video),
                    ),
                }
            };
        response.headers_mut().insert(
            header::REFERRER_POLICY,
            HeaderValue::from_static("no-referrer"),
        );
        http::finish(&mut response, request.method() == Method::HEAD);
        response
    }

    fn player(&self, video: &Video) -> Response {
        let provider_url = video.provider_url();
        let body = format!(
            r#"<!doctype html>
<html lang="ru"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>Видео TeamStream</title><link rel="stylesheet" href="/player.css"></head>
<body><iframe src="{provider_url}" title="Видео TeamStream" sandbox="allow-scripts allow-same-origin" referrerpolicy="no-referrer" allow="fullscreen 'src'; autoplay 'none'; camera 'none'; microphone 'none'; geolocation 'none'"></iframe></body></html>"#
        );
        let policy = format!(
            "default-src 'none'; style-src 'self'; base-uri 'none'; form-action 'none'; frame-src {PROVIDER}; frame-ancestors {}",
            self.parent
        );
        let mut response = html(StatusCode::OK, body);
        response.headers_mut().insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_str(&policy).expect("normalized origins are valid CSP sources"),
        );
        response
    }
}

pub(crate) async fn run() -> anyhow::Result<()> {
    let parent = env::var("SEVER_TEAMSTREAM_PARENT_ORIGIN")
        .ok()
        .and_then(|value| bare_origin(&value))
        .context("Укажите HTTPS или локальный HTTP origin в SEVER_TEAMSTREAM_PARENT_ORIGIN")?;
    let parent = parent[..Position::BeforePath].to_owned();
    let address = IsolatedOrigin::from_env(&parent)?.context("Укажите SEVER_TEAMSTREAM_ORIGIN")?;
    let host = address.authority().to_owned();
    let port = env::var("SEVER_PORT")
        .unwrap_or_else(|_| "4124".to_owned())
        .parse()
        .context("SEVER_PORT должен быть номером локального порта")?;
    let bind_addr = env::var("SEVER_BIND_ADDR")
        .unwrap_or_else(|_| Ipv4Addr::LOCALHOST.to_string())
        .parse()
        .context("SEVER_BIND_ADDR должен быть адресом IPv4")?;
    let listener = TcpListener::bind(SocketAddrV4::new(bind_addr, port)).await?;
    let app = Router::new()
        .fallback(
            |State(state): State<Arc<Wrapper>>, request| async move { state.response(&request) },
        )
        .with_state(Arc::new(Wrapper { host, parent }));
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::body::Body;

    use super::*;

    const APP: &str = "http://localhost:4125";
    const ID: &str = "abcd1234-0000-4000-8000-000000000001";

    #[tokio::test]
    async fn wrapper_has_one_provider_no_other_app_routes_and_no_session() {
        let config = Wrapper {
            host: "127.0.0.1:4124".to_owned(),
            parent: APP.to_owned(),
        };
        let request = |method: &str, path: &str, host: &str| {
            Request::builder()
                .method(method)
                .uri(path)
                .header("Host", host)
                .body(Body::empty())
                .unwrap()
        };
        let path = format!("/embed/{ID}?t=4");
        let host = "127.0.0.1:4124";
        let response = config.response(&request("GET", &path, host));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::REFERRER_POLICY], "no-referrer");
        assert!(!response.headers().contains_key(header::SET_COOKIE));
        let policy = response.headers()[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap();
        assert_eq!(
            policy,
            format!(
                "default-src 'none'; style-src 'self'; base-uri 'none'; form-action 'none'; frame-src {PROVIDER}; frame-ancestors {APP}"
            )
        );
        let body = axum::body::to_bytes(response.into_body(), 8192)
            .await
            .unwrap();
        let body = std::str::from_utf8(&body).unwrap();
        assert!(body.contains(&format!(r#"src="{PROVIDER}/embed/{ID}?t=4""#)));
        assert!(body.contains(r#"sandbox="allow-scripts allow-same-origin""#));
        assert!(body.contains(r#"referrerpolicy="no-referrer""#));
        assert!(!body.contains("<script"));
        assert_eq!(
            config.response(&request("POST", &path, host)).status(),
            StatusCode::METHOD_NOT_ALLOWED
        );
        assert_eq!(
            config
                .response(&request("GET", &path, "localhost:4124"))
                .status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            config.response(&request("GET", "/health", host)).status(),
            StatusCode::OK
        );
        for bad in [
            "/workspace",
            "/published/test",
            "/embed/abc",
            &format!("{path}&t=1"),
        ] {
            assert_eq!(
                config.response(&request("GET", bad, host)).status(),
                StatusCode::NOT_FOUND
            );
        }
        let head = config.response(&request("HEAD", &path, host));
        assert_eq!(head.status(), StatusCode::OK);
        assert!(
            axum::body::to_bytes(head.into_body(), 8192)
                .await
                .unwrap()
                .is_empty()
        );
    }
}
