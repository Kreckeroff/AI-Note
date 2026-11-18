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
    sections_http, views,
    workspace::{DocumentsRoute, DocumentsTarget, Route, WORKSPACE_PATH, local_target},
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
            let route = Route::parse(request.uri().path());
            let catalog = route.is_some_and(Route::is_catalog);
            let documents_read = matches!(route, Some(Route::Documents(_)))
                && matches!(*request.method(), Method::GET | Method::HEAD)
                && !target.is_empty();
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
                    } else if documents_read {
                        html(
                            StatusCode::SERVICE_UNAVAILABLE,
                            views::documents_unavailable(&target),
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
        let route = route.expect("workspace route checked");
        let identify = identity::identify(&self.store, request.headers());
        let principal = if let Route::Documents(
            documents @ DocumentsRoute {
                target: DocumentsTarget::Pages(section),
                ..
            },
        ) = route
        {
            // Reserve time to read the submitted form if the session lookup is unavailable.
            match tokio::time::timeout(Duration::from_secs(2), identify).await {
                Ok(Ok(principal)) => principal,
                failure => {
                    match failure {
                        Ok(Err(error)) => {
                            eprintln!("page.auth.failed category={}", database_error_kind(&error))
                        }
                        Err(_) => eprintln!("page.auth.failed category=timeout"),
                        Ok(Ok(_)) => unreachable!(),
                    }
                    return Ok(sections_http::retry_before_authorization(
                        documents,
                        section,
                        &self.origin,
                        request,
                    )
                    .await);
                }
            }
        } else {
            identify.await?
        };
        let Some(principal) = principal else {
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
        workspace_http::handle(&self.store, route, &principal, &self.origin, request).await
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

#[cfg(test)]
mod tests {
    use axum::body::{Body, to_bytes};
    use sqlx::{
        PgPool,
        postgres::{PgConnectOptions, PgPoolOptions},
    };
    use uuid::Uuid;

    use super::*;
    use crate::db::{CreatePage, CreatePageInput, CreateSection, CreateSpace};

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
    async fn opening_a_space_selects_the_first_page_in_tree_order(
        pool: PgPool,
    ) -> anyhow::Result<()> {
        let store = Store::from_test_pool(pool.clone());
        let tenant = Uuid::new_v4();
        let owner =
            identity::prepare_account("reader@example.test", "Читатель", "long test passphrase")
                .unwrap();
        store.add_account(&owner).await?;
        sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'Север')")
            .bind(tenant)
            .execute(&pool)
            .await?;
        sqlx::query(
            "INSERT INTO tenant_memberships VALUES ($1, $2, 'member', 'active', true, true)",
        )
        .bind(tenant)
        .bind(owner.id)
        .execute(&pool)
        .await?;
        let CreateSpace::Saved(space) = store
            .create_space(tenant, owner.id, Uuid::new_v4(), "Документы")
            .await?
        else {
            panic!("fixture space must be created");
        };
        let (token, session) = identity::prepare_session(identity::now_ms());
        store.add_session(owner.id, &session).await?;
        let server = Arc::new(Server {
            store: store.clone(),
            identity: Identity::new(),
            registration: None,
            host: "localhost:4125".to_owned(),
            origin: "http://localhost:4125".to_owned(),
        });
        let request = |method, uri: String| {
            dispatch(
                State(server.clone()),
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header(header::HOST, &server.host)
                    .header(header::COOKIE, format!("sever_session={token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
        };
        let path = format!("/workspace/{tenant}/spaces/{space}");
        let query = "?q=docs&scope=mine&page=4";
        let space_url = format!("{path}{query}");
        let empty = request(Method::GET, space_url.clone()).await;
        assert_eq!(empty.status(), StatusCode::OK);
        let body = to_bytes(empty.into_body(), 64 * 1024).await?;
        assert!(std::str::from_utf8(&body)?.contains("В пространстве пока нет разделов"));
        let mut sections = Vec::new();
        for title in ["Пустой", "Черновики", "Архив"] {
            let CreateSection::Saved(section) = store
                .create_section(tenant, space, owner.id, Uuid::new_v4(), title)
                .await?
            else {
                panic!("fixture section must be created");
            };
            sections.push(section);
        }
        let without_pages = request(Method::GET, space_url.clone()).await;
        assert_eq!(without_pages.status(), StatusCode::OK);
        let body = to_bytes(without_pages.into_body(), 64 * 1024).await?;
        assert!(std::str::from_utf8(&body)?.contains("Нет доступных страниц"));

        let mut pages = Vec::new();
        for (section, title) in [
            (sections[2], "Алфавит"),
            (sections[1], "Первая страница"),
            (sections[1], "Вторая страница"),
        ] {
            let CreatePage::Saved(page) = store
                .create_page(CreatePageInput {
                    tenant_id: tenant,
                    space_id: space,
                    section_id: section,
                    account_id: owner.id,
                    operation_id: Uuid::new_v4(),
                    title,
                })
                .await?
            else {
                panic!("fixture page must be created");
            };
            pages.push(page);
        }

        let first = format!("{path}/pages/{}{query}", pages[1]);
        for method in [Method::GET, Method::HEAD] {
            let response = request(method, space_url.clone()).await;
            assert_eq!(response.status(), StatusCode::SEE_OTHER);
            assert_eq!(response.headers()[header::LOCATION], first);
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        }

        let explicit = request(Method::GET, format!("{path}/pages/{}{query}", pages[2])).await;
        assert_eq!(explicit.status(), StatusCode::OK);
        let body = to_bytes(explicit.into_body(), 128 * 1024).await?;
        assert!(std::str::from_utf8(&body)?.contains("<h1>Вторая страница</h1>"));
        Ok(())
    }

    #[tokio::test]
    async fn unavailable_document_read_offers_a_local_retry() {
        const PATH: &str = concat!(
            "/workspace/11111111-1111-1111-1111-111111111111",
            "/spaces/22222222-2222-2222-2222-222222222222",
            "/pages/33333333-3333-3333-3333-333333333333",
        );
        let pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        pool.close().await;
        let server = Arc::new(Server {
            store: Store::from_test_pool(pool),
            identity: Identity::new(),
            registration: None,
            host: "localhost:4125".to_owned(),
            origin: "http://localhost:4125".to_owned(),
        });
        let request = Request::builder()
            .uri(format!("{PATH}?scope=mine&page=3&unexpected=ignored"))
            .header(header::HOST, &server.host)
            .header(header::COOKIE, format!("sever_session={}", "A".repeat(43)))
            .body(Body::empty())
            .unwrap();
        let response = dispatch(State(server), request).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let body = std::str::from_utf8(&body).unwrap();
        assert!(body.contains("Не удалось открыть документы"));
        assert!(body.contains(&format!(r#"href="{PATH}?scope=mine&amp;page=3""#)));
    }

    #[tokio::test]
    async fn page_post_preserves_the_form_if_session_lookup_is_unavailable() {
        const PATH: &str = concat!(
            "/workspace/11111111-1111-1111-1111-111111111111",
            "/spaces/22222222-2222-2222-2222-222222222222",
            "/sections/33333333-3333-3333-3333-333333333333/pages",
        );
        const OPERATION: &str = "44444444-4444-4444-4444-444444444444";
        let pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        pool.close().await;
        let server = Arc::new(Server {
            store: Store::from_test_pool(pool),
            identity: Identity::new(),
            registration: None,
            host: "localhost:4125".to_owned(),
            origin: "http://localhost:4125".to_owned(),
        });
        let request = |origin: &str, has_cookie: bool| {
            let mut request = Request::builder()
                .uri(format!("{PATH}?scope=mine"))
                .method(Method::POST)
                .header(header::HOST, &server.host)
                .header(header::ORIGIN, origin)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
            if has_cookie {
                request =
                    request.header(header::COOKIE, format!("sever_session={}", "A".repeat(43)));
            }
            let form = url::form_urlencoded::Serializer::new(String::new())
                .append_pair("operation_id", OPERATION)
                .append_pair("name", "План <на завтра>")
                .finish();
            request.body(Body::from(form)).unwrap()
        };
        let response = dispatch(State(server.clone()), request(&server.origin, true)).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let body = std::str::from_utf8(&body).unwrap();
        assert!(body.contains(r#"value="План &lt;на завтра&gt;""#));
        assert!(body.contains(&format!(r#"name="operation_id" value="{OPERATION}""#)));
        assert!(body.contains(&format!(r#"action="{PATH}?scope=mine""#)));
        assert!(!body.contains("Дерево пространства"));
        assert!(!body.contains("sever-ui-data"));

        let missing = dispatch(State(server.clone()), request(&server.origin, false)).await;
        assert_eq!(missing.status(), StatusCode::SEE_OTHER);
        assert_eq!(missing.headers()[header::LOCATION], "/");
        let foreign = dispatch(State(server.clone()), request("http://foreign.test", true)).await;
        assert_eq!(foreign.status(), StatusCode::FORBIDDEN);
        let body = to_bytes(foreign.into_body(), 4096).await.unwrap();
        assert!(!std::str::from_utf8(&body).unwrap().contains("План"));
    }
}
