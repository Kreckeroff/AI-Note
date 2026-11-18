use std::{
    net::{Ipv4Addr, SocketAddrV4},
    sync::Arc,
    time::Duration,
};

use axum::{
    Router,
    extract::{Request, State},
    http::{HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use tokio::net::TcpListener;

use crate::{
    db::Store,
    http::{self, Form, FormError, html, plain, redirect},
    identity::{self, Identity, LoginOutcome},
    registration::Registration,
    views,
    workspace::{Route, WORKSPACE_PATH, local_target},
    workspace_http,
};

struct Server {
    store: Store,
    identity: Identity,
    registration: Option<Registration>,
    host: String,
    origin: String,
}

pub(crate) async fn run(
    store: Store,
    bind_addr: Ipv4Addr,
    port: u16,
    public_host: &str,
) -> anyhow::Result<()> {
    let registration = Registration::from_env()?;
    store.check_schema().await?;
    let listener = TcpListener::bind(SocketAddrV4::new(bind_addr, port)).await?;
    let host = format!("{public_host}:{}", listener.local_addr()?.port());
    let origin = format!("http://{host}");
    println!("Север · {origin}/");
    let state = Arc::new(Server {
        store,
        identity: Identity::new(),
        registration,
        host,
        origin,
    });
    axum::serve(listener, Router::new().fallback(dispatch).with_state(state))
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

async fn shutdown() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {}
                _ = terminate.recv() => {}
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

async fn dispatch(State(state): State<Arc<Server>>, request: Request) -> Response {
    let head = request.method() == Method::HEAD;
    let mut response =
        if http::single_header(request.headers(), header::HOST) != Some(state.host.as_str()) {
            plain(
                StatusCode::FORBIDDEN,
                "Откройте «Север» по локальному адресу.",
            )
        } else {
            let target = local_target(
                request
                    .uri()
                    .path_and_query()
                    .map_or("", |part| part.as_str()),
            );
            let catalog = Route::parse(request.uri().path()).is_some_and(Route::is_catalog);
            match tokio::time::timeout(Duration::from_secs(15), state.handle(request)).await {
                Ok(Ok(response)) => response,
                failure => {
                    match failure {
                        Ok(Err(error)) => {
                            eprintln!("request.failed category={}", database_error_kind(&error))
                        }
                        Err(_) => eprintln!("request.failed category=timeout"),
                        Ok(Ok(_)) => unreachable!(),
                    }
                    if catalog {
                        html(
                            StatusCode::SERVICE_UNAVAILABLE,
                            views::catalog_unavailable(if target.is_empty() {
                                WORKSPACE_PATH
                            } else {
                                &target
                            }),
                        )
                    } else {
                        let mut response = html(
                            StatusCode::SERVICE_UNAVAILABLE,
                            views::sign_in(
                                "",
                                "Не удалось открыть «Север». Попробуйте ещё раз.",
                                state.registration.is_some(),
                                "",
                            ),
                        );
                        response.headers_mut().insert(
                            header::REFERRER_POLICY,
                            HeaderValue::from_static("strict-origin"),
                        );
                        response
                    }
                }
            }
        };
    http::finish(&mut response, head);
    response
}

fn database_error_kind(error: &sqlx::Error) -> &'static str {
    match error {
        sqlx::Error::PoolTimedOut => "pool-timeout",
        sqlx::Error::Io(_) => "database-io",
        sqlx::Error::Database(_) => "database",
        _ => "backend",
    }
}

impl Server {
    async fn handle(&self, request: Request) -> Result<Response, sqlx::Error> {
        let path = request.uri().path();
        if matches!(path, "/register" | "/register/confirm") {
            return if let Some(registration) = &self.registration {
                registration
                    .handle(&self.store, &self.origin, request)
                    .await
            } else {
                Ok(plain(StatusCode::NOT_FOUND, "Страница не найдена."))
            };
        }
        if path == "/login" {
            return if request.method() == Method::POST {
                self.login(request).await
            } else {
                Ok(http::method_not_allowed("POST"))
            };
        }
        let route = Route::parse(path);
        let asset = asset(path);
        if path != "/" && route.is_none() && asset.is_none() {
            return Ok(plain(StatusCode::NOT_FOUND, "Страница не найдена."));
        }
        let is_read = matches!(*request.method(), Method::GET | Method::HEAD);
        let accepts_read = route.is_none_or(Route::allows_read);
        let accepts_post = route.is_some_and(Route::allows_post);
        if !(is_read && accepts_read || accepts_post && request.method() == Method::POST) {
            let methods = match (accepts_read, accepts_post) {
                (true, true) => "GET, HEAD, POST",
                (true, false) => "GET, HEAD",
                (false, true) => "POST",
                (false, false) => unreachable!("recognized routes have at least one method"),
            };
            return Ok(http::method_not_allowed(methods));
        }
        if let Some((media_type, body)) = asset {
            return Ok(([(header::CONTENT_TYPE, media_type)], body).into_response());
        }
        if path == "/" {
            let query = Form::from_query(request.uri().query().unwrap_or_default());
            return Ok(html(
                StatusCode::OK,
                views::sign_in(
                    "",
                    "",
                    self.registration.is_some(),
                    &local_target(query.get("returnTo")),
                ),
            ));
        }
        let Some(principal) = identity::identify(&self.store, request.headers()).await? else {
            let target = local_target(
                request
                    .uri()
                    .path_and_query()
                    .map_or("", |value| value.as_str()),
            );
            return Ok(
                if is_read && !target.is_empty() && target != WORKSPACE_PATH {
                    let query = url::form_urlencoded::Serializer::new(String::new())
                        .append_pair("returnTo", &target)
                        .finish();
                    redirect(&format!("/?{query}"))
                } else {
                    redirect("/")
                },
            );
        };
        workspace_http::handle(
            &self.store,
            route.expect("workspace route checked"),
            &principal,
            &self.origin,
            request,
        )
        .await
    }

    async fn login(&self, request: Request) -> Result<Response, sqlx::Error> {
        let open_form = "Откройте форму входа в «Север».";
        if !http::check_origin(request.headers(), &self.origin) {
            return Ok(plain(StatusCode::FORBIDDEN, open_form));
        }
        let form = match http::read_form(request).await {
            Ok(form) => form,
            Err(FormError::WrongEncoding) => {
                return Ok(plain(StatusCode::UNSUPPORTED_MEDIA_TYPE, open_form));
            }
            Err(FormError::TooLarge) => {
                let mut response = plain(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "Форма входа слишком большая.",
                );
                response
                    .headers_mut()
                    .insert(header::CONNECTION, HeaderValue::from_static("close"));
                return Ok(response);
            }
            Err(FormError::Unreadable) => {
                return Ok(plain(
                    StatusCode::BAD_REQUEST,
                    "Не удалось прочитать форму.",
                ));
            }
        };
        let return_to = local_target(form.get("returnTo"));
        let email = form.get("email");
        let value: String = email.chars().take(identity::EMAIL_MAX_LENGTH).collect();
        match self
            .identity
            .sign_in(&self.store, email, form.get("password"))
            .await?
        {
            LoginOutcome::Accepted { token, account_id } => {
                eprintln!("sign-in.accepted account={account_id}");
                let mut response = redirect(if return_to.is_empty() {
                    WORKSPACE_PATH
                } else {
                    &return_to
                });
                response.headers_mut().insert(
                    header::SET_COOKIE,
                    HeaderValue::from_str(&identity::session_cookie(&token))
                        .expect("session token uses URL-safe ASCII"),
                );
                Ok(response)
            }
            LoginOutcome::Invalid => {
                eprintln!("sign-in.rejected");
                Ok(html(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    views::sign_in(
                        &value,
                        "Неверная почта или пароль.",
                        self.registration.is_some(),
                        &return_to,
                    ),
                ))
            }
            LoginOutcome::Limited(wait) => {
                eprintln!("sign-in.limited");
                let mut response = html(
                    StatusCode::TOO_MANY_REQUESTS,
                    views::sign_in(
                        &value,
                        "Слишком много попыток входа. Подождите немного и попробуйте снова.",
                        self.registration.is_some(),
                        &return_to,
                    ),
                );
                response
                    .headers_mut()
                    .insert(header::RETRY_AFTER, HeaderValue::from(wait));
                Ok(response)
            }
        }
    }
}

fn asset(path: &str) -> Option<(&'static str, &'static str)> {
    const CSS: &str = "text/css; charset=utf-8";
    const JS: &str = "text/javascript; charset=utf-8";
    Some(match path {
        "/auth.css" => (CSS, include_str!("../../app/auth.css")),
        "/spaces.css" => (CSS, include_str!("../../app/spaces.css")),
        "/sign-in.mjs" => (JS, include_str!("../../app/sign-in.mjs")),
        "/space-form.mjs" => (JS, include_str!("../../app/space-form.mjs")),
        "/workspace-menu.mjs" => (JS, include_str!("../../app/workspace-menu.mjs")),
        views::UI_BUNDLE_PATH if !views::UI_BUNDLE.is_empty() => (JS, views::UI_BUNDLE),
        _ => return None,
    })
}
