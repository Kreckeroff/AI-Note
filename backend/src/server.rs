use std::{
    net::{Ipv4Addr, SocketAddrV4},
    sync::Arc,
    time::Duration,
};

use axum::{
    Extension, Router,
    extract::{Request, State},
    http::{HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use tokio::net::TcpListener;

use crate::{
    approach_http, collections_http,
    coedit_presence::PresenceTracker,
    db::Store,
    document_finding,
    external_sharing::{SharingDelivery, private_address, sharing_error},
    http::{self, Form, FormError, html, plain, redirect},
    identity::{self, Identity, LoginOutcome},
    knowledge_graph, organization_http,
    page_social_route::{self, SocialRoute},
    registration::Registration,
    sections_http, tasks_http,
    teamstream::IsolatedOrigin,
    views,
    workspace::{
        DocumentsRoute, DocumentsTarget, HistoryTarget, Route, WORKSPACE_PATH, local_target,
    },
    workspace_chrome, workspace_http,
};

enum WorkspaceSurface {
    Organization(organization_http::OrganizationRoute),
    Finding(document_finding::FindingRoute),
    Collections(collections_http::Route),
    Tasks(tasks_http::TasksRoute),
    Graph(knowledge_graph::GraphRoute),
    Approach(approach_http::ApproachRoute),
    Chrome(workspace_chrome::ChromeRoute),
}

impl WorkspaceSurface {
    fn parse(path: &str) -> Option<Self> {
        organization_http::OrganizationRoute::parse(path)
            .map(Self::Organization)
            .or_else(|| document_finding::FindingRoute::parse(path).map(Self::Finding))
            .or_else(|| tasks_http::TasksRoute::parse(path).map(Self::Tasks))
            .or_else(|| collections_http::Route::parse(path).map(Self::Collections))
            .or_else(|| knowledge_graph::GraphRoute::parse(path).map(Self::Graph))
            .or_else(|| approach_http::ApproachRoute::parse(path).map(Self::Approach))
            .or_else(|| workspace_chrome::ChromeRoute::parse(path).map(Self::Chrome))
    }

    fn allows_anonymous(&self) -> bool {
        matches!(self, Self::Organization(route) if route.allows_anonymous())
    }

    fn allows_read(&self) -> bool {
        match self {
            Self::Organization(route) => route.allows_read(),
            _ => true,
        }
    }

    fn allows_post(&self) -> bool {
        match self {
            Self::Organization(route) => route.allows_post(),
            Self::Tasks(_) | Self::Approach(_) | Self::Collections(_) => true,
            Self::Finding(_) | Self::Graph(_) | Self::Chrome(_) => false,
        }
    }

    async fn handle(
        self,
        store: &Store,
        principal: Option<&crate::db::Principal>,
        origin: &str,
        request: Request,
    ) -> Result<Response, sqlx::Error> {
        if let Self::Organization(route) = self {
            return organization_http::handle(store, principal, route, origin, request).await;
        }
        let Some(principal) = principal else {
            return Ok(redirect("/"));
        };
        match self {
            Self::Finding(route) => {
                document_finding::handle(store, principal, route, origin, request).await
            }
            Self::Collections(route) => collections_http::handle(store, principal, route, origin, request).await,
            Self::Tasks(route) => {
                tasks_http::handle(store, principal, route, origin, request).await
            }
            Self::Graph(route) => knowledge_graph::handle(store, principal, route, request).await,
            Self::Approach(route) => {
                approach_http::handle(store, principal, route, origin, request).await
            }
            Self::Chrome(route) => route.handle(store, principal, request.uri()).await,
            Self::Organization(_) => unreachable!(),
        }
    }
}

fn login_target(target: &str) -> String {
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    if target.len() <= 1400
        && !target
            .chars()
            .any(|character| character.is_ascii_control() || character == '\\')
        && WorkspaceSurface::parse(path)
            .is_some_and(|route| !matches!(route, WorkspaceSurface::Chrome(_)) && route.allows_read())
    {
        let encoded = url::form_urlencoded::parse(query.as_bytes())
            .into_owned()
            .collect::<Vec<_>>();
        let clean = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(encoded)
            .finish();
        if clean.is_empty() {
            path.to_owned()
        } else {
            format!("{path}?{clean}")
        }
    } else {
        local_target(target)
    }
}

struct Server {
    store: Store,
    identity: Identity,
    registration: Option<Registration>,
    sharing: SharingDelivery,
    presence: PresenceTracker,
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
    let sharing = SharingDelivery::from_env()?;
    store.check_schema().await?;
    let listener = TcpListener::bind(SocketAddrV4::new(bind_addr, port)).await?;
    let host = format!("{public_host}:{}", listener.local_addr()?.port());
    let origin = format!("http://{host}");
    let teamstream = IsolatedOrigin::from_env(&origin)?;
    println!("Север · {origin}/");
    let cleanup_store = store.clone();
    let (stop_cleanup, cleanup_stopped) = tokio::sync::oneshot::channel();
    let state = Arc::new(Server {
        store,
        identity: Identity::new(),
        registration,
        sharing,
        presence: PresenceTracker::default(),
        host,
        origin,
    });
    let mut routes = Router::new().fallback(dispatch).with_state(state);
    if let Some(teamstream) = teamstream {
        routes = routes.layer(Extension(teamstream));
    }
    let app = axum::serve(listener, routes).with_graceful_shutdown(async move {
        shutdown().await;
        let _ = stop_cleanup.send(());
    });
    let cleanup = async move {
        tokio::select! {
            biased;
            _ = cleanup_stopped => {}
            _ = cleanup_store.sweep_attachment_drafts(Duration::from_secs(30)) => {}
        }
    };
    let (result, ()) = tokio::join!(async { app.await }, cleanup);
    result?;
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
    let is_read = head || request.method() == Method::GET;
    let mut response =
        if http::single_header(request.headers(), header::HOST) != Some(state.host.as_str()) {
            plain(
                StatusCode::FORBIDDEN,
                "Откройте «Север» по локальному адресу.",
            )
        } else {
            let target = login_target(
                request
                    .uri()
                    .path_and_query()
                    .map_or("", |part| part.as_str()),
            );
            let social = SocialRoute::parse(request.uri().path());
            let social_api = social.as_ref().is_some_and(SocialRoute::is_api);
            let private_route = social.is_some() && !social_api;
            let route = Route::parse(request.uri().path());
            let history_target = route.and_then(Route::history_target);
            let history_json = history_target == Some(HistoryTarget::List)
                && sections_http::history_wants_json(request.headers());
            let catalog = route.is_some_and(Route::is_catalog);
            let page_link_search = is_read
                && document_finding::is_page_link_request(request.uri().query())
                && document_finding::FindingRoute::parse(request.uri().path()).is_some();
            let documents_read =
                matches!(route, Some(Route::Documents(_))) && is_read && !target.is_empty();
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
                    if social_api {
                        sharing_error(
                            StatusCode::SERVICE_UNAVAILABLE,
                            "Не удалось выполнить действие. Попробуйте снова.",
                        )
                    } else if private_route {
                        private_address(plain(
                            StatusCode::SERVICE_UNAVAILABLE,
                            "Не удалось открыть ссылку. Попробуйте снова.",
                        ))
                    } else if route.is_some_and(Route::is_cover) {
                        sections_http::cover_unavailable(is_read)
                    } else if route.is_some_and(Route::is_attachment) {
                        sections_http::attachment_unavailable(is_read)
                    } else if history_json {
                        sections_http::history_api_error(StatusCode::SERVICE_UNAVAILABLE)
                    } else if page_link_search {
                        document_finding::page_link_search_error(
                            StatusCode::SERVICE_UNAVAILABLE,
                            "Не удалось загрузить страницы. Попробуйте снова.",
                        )
                    } else if catalog {
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

fn sign_in_redirect(request: &Request, is_read: bool) -> Response {
    let target = login_target(
        request
            .uri()
            .path_and_query()
            .map_or("", |part| part.as_str()),
    );
    if !is_read || target.is_empty() {
        return redirect("/");
    }
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("returnTo", &target)
        .finish();
    redirect(&format!("/?{query}"))
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
        if SocialRoute::parse(path).is_some() {
            return page_social_route::handle(
                &self.store,
                &self.identity,
                &self.sharing,
                &self.presence,
                &self.origin,
                request,
            )
            .await;
        }
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
        let surface = WorkspaceSurface::parse(path);
        let asset = asset(path);
        if path != "/" && route.is_none() && surface.is_none() && asset.is_none() {
            return Ok(plain(StatusCode::NOT_FOUND, "Страница не найдена."));
        }
        let is_read = matches!(*request.method(), Method::GET | Method::HEAD);
        let accepts_read = surface.as_ref().map_or_else(
            || route.is_none_or(Route::allows_read),
            WorkspaceSurface::allows_read,
        );
        let accepts_post = surface.as_ref().map_or_else(
            || route.is_some_and(Route::allows_post),
            WorkspaceSurface::allows_post,
        );
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
                    &login_target(query.get("returnTo")),
                ),
            ));
        }
        let identify = identity::identify(&self.store, request.headers());
        let preserve_submission = match route {
            Some(Route::Documents(
                documents @ DocumentsRoute {
                    target:
                        DocumentsTarget::Pages(_)
                        | DocumentsTarget::EditPage(_)
                        | DocumentsTarget::History {
                            target: HistoryTarget::Restore(_),
                            ..
                        },
                    ..
                },
            )) if request.method() == Method::POST => Some(documents),
            _ => None,
        };
        let principal = if let Some(documents) = preserve_submission {
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
                    return Ok(match documents.target {
                        DocumentsTarget::Pages(section) => {
                            sections_http::retry_before_authorization(
                                documents,
                                section,
                                &self.origin,
                                request,
                            )
                            .await
                        }
                        DocumentsTarget::EditPage(page) => {
                            sections_http::retry_page_edit_before_authorization(
                                documents,
                                page,
                                &self.origin,
                                request,
                            )
                            .await
                        }
                        DocumentsTarget::History {
                            page,
                            target: HistoryTarget::Restore(revision),
                        } => {
                            sections_http::retry_history_restore_before_authorization(
                                documents,
                                page,
                                revision,
                                &self.origin,
                                request,
                            )
                            .await
                        }
                        _ => unreachable!("only document form writes use early session timeout"),
                    });
                }
            }
        } else {
            identify.await?
        };
        if let Some(surface) = surface {
            if principal.is_some() || surface.allows_anonymous() {
                return surface
                    .handle(&self.store, principal.as_ref(), &self.origin, request)
                    .await;
            }
            if matches!(surface, WorkspaceSurface::Chrome(_)) {
                return Ok(sharing_error(
                    StatusCode::UNAUTHORIZED,
                    "Войдите в приложение.",
                ));
            }
            if matches!(surface, WorkspaceSurface::Finding(_))
                && document_finding::is_page_link_request(request.uri().query())
            {
                return Ok(document_finding::page_link_search_error(
                    StatusCode::UNAUTHORIZED,
                    "Войдите в приложение.",
                ));
            }
            return Ok(sign_in_redirect(&request, is_read));
        }
        let route = route.expect("workspace route checked");
        let Some(principal) = principal else {
            let history_target = route.history_target();
            if matches!(route, Route::Documents(DocumentsRoute { target: DocumentsTarget::RenamePage(_), .. })) {
                return Ok(plain(StatusCode::UNAUTHORIZED, "Войдите в приложение."));
            }
            if route.is_cover() {
                return Ok(sections_http::cover_unauthenticated(is_read));
            }
            if route.is_attachment() {
                return Ok(sections_http::attachment_unauthenticated(is_read));
            }
            if history_target == Some(HistoryTarget::List)
                && sections_http::history_wants_json(request.headers())
            {
                return Ok(sections_http::history_api_error(StatusCode::UNAUTHORIZED));
            }
            let target = login_target(
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
        if matches!(route, Route::Current)
            && is_read
            && request.uri().query().is_none()
            && let (Some(data), _) = self.store.workspace(principal.id, None).await?
        {
            return Ok(redirect(&organization_http::organization_path(
                data.tenant.id,
            )));
        }
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
        let return_to = login_target(form.get("returnTo"));
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
        "/parity.css" => (CSS, include_str!("../../app/parity.css")),
        "/workspace-chrome.js" => (JS, include_str!("../../app/workspace-chrome.js")),
        "/workspace-graph.js" => (JS, include_str!("../../app/workspace-graph.js")),
        "/collections.css" => (CSS, collections_http::STYLES),
        "/tasks.css" => (CSS, tasks_http::STYLES),
        "/tasks.js" => (JS, tasks_http::SCRIPT),
        "/sign-in.mjs" => (JS, include_str!("../../app/sign-in.mjs")),
        "/space-form.mjs" => (JS, include_str!("../../app/space-form.mjs")),
        "/page-editor.js" => (JS, include_str!("../../app/page-editor.js")),
        "/page-embed.js" => (JS, include_str!("../../app/page-embed.js")),
        "/workspace-menu.mjs" => (JS, include_str!("../../app/workspace-menu.mjs")),
        views::UI_BUNDLE_PATH if !views::UI_BUNDLE.is_empty() => (JS, views::UI_BUNDLE),
        _ => return organization_http::asset(path),
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
            sharing: SharingDelivery::disabled(),
            presence: PresenceTracker::default(),
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
        let post = |uri: String, form: String| {
            dispatch(
                State(server.clone()),
                Request::builder()
                    .method(Method::POST)
                    .uri(uri)
                    .header(header::HOST, &server.host)
                    .header(header::ORIGIN, &server.origin)
                    .header(header::COOKIE, format!("sever_session={token}"))
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(form))
                    .unwrap(),
            )
        };
        for fields in [
            "name=Example&template=meeting-v1&template=decision-v1",
            "name=Example&template=&template=",
            "name=One%E2%80%A8Two",
            "name=One%E2%80%A9Two",
        ] {
            let response = post(
                format!("{path}/sections/{}/pages", sections[0]),
                format!("operation_id={}&{fields}", Uuid::new_v4()),
            )
            .await;
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
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

        for title in ["One%E2%80%A8Two", "One%E2%80%A9Two"] {
            let response = post(
                format!("{path}/pages/{}/rename", pages[0]),
                format!("base_revision=1&title={title}"),
            )
            .await;
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
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

    #[sqlx::test]
    #[ignore = "requires an isolated PostgreSQL DATABASE_URL with database creation privileges"]
    async fn page_editor_preserves_text_on_conflict_and_rechecks_access(
        pool: PgPool,
    ) -> anyhow::Result<()> {
        let store = Store::from_test_pool(pool.clone());
        let tenant = Uuid::new_v4();
        sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'Север')")
            .bind(tenant)
            .execute(&pool)
            .await?;
        let mut sessions = Vec::new();
        for (email, role) in [
            ("writer@example.test", "member"),
            ("editor@example.test", "admin"),
        ] {
            let account = identity::prepare_account(email, email, "long test passphrase").unwrap();
            store.add_account(&account).await?;
            sqlx::query("INSERT INTO tenant_memberships VALUES ($1, $2, $3, 'active', true, true)")
                .bind(tenant)
                .bind(account.id)
                .bind(role)
                .execute(&pool)
                .await?;
            let (token, session) = identity::prepare_session(identity::now_ms());
            store.add_session(account.id, &session).await?;
            sessions.push((account.id, token));
        }
        let [(owner, owner_token), (editor, editor_token)] = &sessions[..] else {
            unreachable!()
        };
        let CreateSpace::Saved(space) = store
            .create_space(tenant, *owner, Uuid::new_v4(), "Документы")
            .await?
        else {
            unreachable!()
        };
        let CreateSection::Saved(section) = store
            .create_section(tenant, space, *owner, Uuid::new_v4(), "Тексты")
            .await?
        else {
            unreachable!()
        };
        let mut pages = Vec::new();
        for title in ["Рабочая страница", "Другая страница"] {
            let CreatePage::Saved(page) = store
                .create_page(CreatePageInput {
                    tenant_id: tenant,
                    space_id: space,
                    section_id: section,
                    account_id: *owner,
                    operation_id: Uuid::new_v4(),
                    title,
                })
                .await?
            else {
                unreachable!()
            };
            pages.push(page);
        }

        let server = Arc::new(Server {
            store,
            identity: Identity::new(),
            registration: None,
            sharing: SharingDelivery::disabled(),
            presence: PresenceTracker::default(),
            host: "localhost:4125".to_owned(),
            origin: "http://localhost:4125".to_owned(),
        });
        let send_encoded = |token: &str, uri: &str, form: Option<String>| {
            let mut request = Request::builder()
                .uri(uri)
                .header(header::HOST, &server.host)
                .header(header::COOKIE, format!("sever_session={token}"));
            let body = if let Some(form) = form {
                request = request
                    .method(Method::POST)
                    .header(header::ORIGIN, &server.origin)
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
                Body::from(form)
            } else {
                Body::empty()
            };
            dispatch(State(server.clone()), request.body(body).unwrap())
        };
        let send = |token: &str, uri: &str, form: Option<(&str, &str, Option<&str>)>| {
            send_encoded(
                token,
                uri,
                form.map(|(text, revision, rebase)| {
                    let mut form = url::form_urlencoded::Serializer::new(String::new());
                    form.append_pair("text", text)
                        .append_pair("revision", revision);
                    if let Some(rebase) = rebase {
                        form.append_pair("baseRevision", rebase);
                    }
                    form.finish()
                }),
            )
        };
        let send_source = |token: &str, uri: &str, source: &str, revision: &str, format: &str| {
            send_encoded(
                token,
                uri,
                Some(
                    url::form_urlencoded::Serializer::new(String::new())
                        .append_pair("text", source)
                        .append_pair("revision", revision)
                        .append_pair("contentFormat", format)
                        .append_pair("editorMode", "source")
                        .finish(),
                ),
            )
        };
        let page = pages[0];
        let path = format!("/workspace/{tenant}/spaces/{space}/pages/{page}");
        let query = "?q=docs&scope=mine&page=4";
        let read_to = format!("{path}{query}");
        let edit_to = format!("{path}/edit{query}");
        let other_to = format!("/workspace/{tenant}/spaces/{space}/pages/{}/edit", pages[1]);

        let empty = send(owner_token, &edit_to, None).await;
        assert_eq!(empty.status(), StatusCode::OK);
        let body = response_text(empty).await?;
        let script = body
            .find(r#"<script src="/page-editor.js"></script>"#)
            .unwrap();
        assert!(script < body.find("data-native-page-editor").unwrap());
        if let Some(bundle) = body.find(r#"src="/sever-ui.js""#) {
            assert!(script < bundle);
            assert!(body.contains(r#""textUnchanged":true"#));
        }
        assert!(body.contains("name=\"revision\" value=\"1\""));
        assert!(body.contains(r#"name="contentFormat" value="literal""#));
        assert!(body.contains(r#"name="editorMode" value="visual""#));
        assert!(body.contains(r#"name="textBase" value=""#));
        assert!(body.contains("data-native-source"));
        assert!(body.contains("autofocus>\n</textarea>"));
        let read_only = send(editor_token, &edit_to, None).await;
        assert_eq!(read_only.status(), StatusCode::FORBIDDEN);

        let seeded = "Текст из тестовых данных.";
        sqlx::query("UPDATE pages SET markdown = $1, content_revision = 7 WHERE tenant_id = $2 AND space_id = $3 AND id = $4")
            .bind(seeded).bind(tenant).bind(space).bind(pages[1]).execute(&pool).await?;
        let seeded_form = send(owner_token, &other_to, None).await;
        assert_eq!(seeded_form.status(), StatusCode::OK);
        assert!(response_text(seeded_form).await?.contains(seeded));
        let stale_same = send(owner_token, &other_to, Some((seeded, "6", None))).await;
        let current_same = send(owner_token, &other_to, Some((seeded, "7", None))).await;
        let stored: (i64, String) = sqlx::query_as(
            "SELECT content_revision, markdown FROM pages WHERE tenant_id = $1 AND space_id = $2 AND id = $3",
        ).bind(tenant).bind(space).bind(pages[1]).fetch_one(&pool).await?;
        assert_eq!(
            (stale_same.status(), current_same.status(), stored),
            (
                StatusCode::SEE_OTHER,
                StatusCode::SEE_OTHER,
                (7, seeded.to_owned())
            ),
        );
        let other = send(owner_token, &other_to, Some(("другой секрет", "7", None))).await;
        assert_eq!(other.status(), StatusCode::SEE_OTHER);
        let value = "\r\n  # Абзац\r\n\r\n<script>*буквально*</script> \\🙂  \r\n";
        for _ in 0..2 {
            let saved = send(owner_token, &edit_to, Some((value, "1", None))).await;
            assert_eq!(saved.status(), StatusCode::SEE_OTHER);
            assert_eq!(
                saved.headers()[header::LOCATION],
                format!("{read_to}#edit-page")
            );
        }
        let read = send(editor_token, &read_to, None).await;
        assert_eq!(read.status(), StatusCode::OK);
        let body = response_text(read).await?;
        let escaped = "\n  # Абзац\n\n&lt;script&gt;*буквально*&lt;/script&gt; \\🙂  \n";
        assert!(body.contains(escaped));
        assert!(!body.contains("другой секрет"));
        assert!(!body.contains("id=\"edit-page\""));

        let draft = "Мой <черновик>";
        let export_to = format!("{path}?download=markdown");
        let export = send(editor_token, &export_to, None).await;
        assert_eq!(export.status(), StatusCode::OK);
        assert_eq!(
            export.headers()[header::CONTENT_TYPE],
            "text/markdown; charset=utf-8"
        );
        assert!(
            export.headers()[header::CONTENT_DISPOSITION]
                .to_str()?
                .ends_with(".md")
        );
        assert_eq!(
            response_text(export).await?,
            crate::text::text_to_markdown(&value.replace("\r\n", "\n"))
        );
        let conflict = send(owner_token, &edit_to, Some((draft, "1", None))).await;
        assert_eq!(conflict.status(), StatusCode::CONFLICT);
        let body = response_text(conflict).await?;
        assert!(body.contains(r"\# Абзац"));
        assert!(body.contains(r"Мой \&lt;черновик\&gt;"));
        assert!(body.contains("name=\"baseRevision\" value=\"2\""));
        let rebased = send(owner_token, &edit_to, Some((draft, "1", Some("2")))).await;
        assert_eq!(rebased.status(), StatusCode::SEE_OTHER);

        sqlx::query("INSERT INTO space_grants VALUES ($1, $2, $3, 'editor')")
            .bind(tenant)
            .bind(space)
            .bind(editor)
            .execute(&pool)
            .await?;
        let editable = send(editor_token, &edit_to, None).await;
        assert_eq!(editable.status(), StatusCode::OK);
        let body = response_text(editable).await?;
        assert!(body.contains("name=\"revision\" value=\"3\""));

        let limit = "🙂".repeat(30_000);
        let too_long = send(
            editor_token,
            &edit_to,
            Some((&format!("{limit}🙂"), "3", None)),
        )
        .await;
        assert_eq!(too_long.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert!(
            response_text(too_long)
                .await?
                .contains(&format!("{limit}🙂"))
        );
        let saved = send(editor_token, &edit_to, Some((&limit, "3", None))).await;
        assert_eq!(saved.status(), StatusCode::SEE_OTHER);

        sqlx::query("UPDATE space_grants SET role = 'viewer' WHERE tenant_id = $1 AND space_id = $2 AND account_id = $3")
            .bind(tenant).bind(space).bind(editor).execute(&pool).await?;
        let forbidden = send(editor_token, &edit_to, Some((draft, "4", None))).await;
        assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
        let body = response_text(forbidden).await?;
        assert!(body.contains(r"Мой \&lt;черновик\&gt;"));
        assert!(body.contains("autofocus readonly"));
        assert!(!body.contains("/page-editor.js"));
        assert!(!body.contains(&limit));

        sqlx::query("UPDATE tenant_memberships SET status = 'inactive', is_current = false WHERE tenant_id = $1 AND account_id = $2")
            .bind(tenant).bind(editor).execute(&pool).await?;
        let denied = send(editor_token, &edit_to, Some((draft, "4", None))).await;
        let denied_export = send(editor_token, &export_to, None).await;
        assert!(
            !denied_export
                .headers()
                .contains_key(header::CONTENT_DISPOSITION)
        );
        assert!(!response_text(denied_export).await?.contains(&limit));
        let missing_to = format!(
            "/workspace/{tenant}/spaces/{space}/pages/{}/edit",
            Uuid::new_v4()
        );
        let missing = send(owner_token, &missing_to, Some((draft, "4", None))).await;
        assert_eq!(denied.status(), StatusCode::NOT_FOUND);
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);
        let denied = response_text(denied).await?;
        assert!(!denied.contains("Рабочая страница"));
        assert!(!denied.contains("черновик"));
        assert_eq!(denied, response_text(missing).await?);

        // The same source at a stale revision remains a no-op even when switching formats.
        let unchanged = crate::text::text_to_markdown("другой секрет");
        let unchanged_post = send_source(owner_token, &other_to, &unchanged, "7", "markdown").await;
        assert_eq!(unchanged_post.status(), StatusCode::SEE_OTHER);
        let stored = || {
            sqlx::query_as::<_, (i64, String, bool)>(
                "SELECT content_revision, markdown, content_is_markdown FROM pages
             WHERE tenant_id = $1 AND space_id = $2 AND id = $3",
            )
            .bind(tenant)
            .bind(space)
            .bind(pages[1])
            .fetch_one(&pool)
        };
        assert_eq!(stored().await?, (8, unchanged, false));

        let source = "\r\n## Заголовок\r\n\r\nДо **слово** и _термин_.\r\n\r\n- Первый\r\n- Второй\r\n\r\n`**x**`\r\n\r\n<img src=https://example.test/asset>  \r\n";
        let saved = send_source(owner_token, &other_to, source, "8", "markdown").await;
        assert_eq!(saved.status(), StatusCode::SEE_OTHER);
        let expected_source = source.replace("\r\n", "\n");
        assert_eq!(stored().await?, (9, expected_source.clone(), true));
        let repeated = send_source(owner_token, &other_to, source, "8", "markdown").await;
        assert_eq!(repeated.status(), StatusCode::SEE_OTHER);
        assert_eq!(stored().await?, (9, expected_source.clone(), true));

        let other_read = format!("/workspace/{tenant}/spaces/{space}/pages/{}", pages[1]);
        let exported = send(
            owner_token,
            &format!("{other_read}?download=markdown"),
            None,
        )
        .await;
        assert_eq!(response_text(exported).await?, expected_source);
        let body = response_text(send(owner_token, &other_read, None).await).await?;
        assert!(body.contains(r#"class="document-text document-markdown"><h3>Заголовок</h3>"#));
        assert!(body.contains("<strong>слово</strong> и <em>термин</em>"));
        assert!(body.contains("<ul><li>Первый</li><li>Второй</li></ul>"));
        assert!(body.contains("<code>**x**</code>"));
        assert!(body.contains("&lt;img src=https://example.test/asset&gt;"));
        assert!(!body.contains("<img src="));

        let whitespace_edit = format!("{expected_source} ");
        let conflict = send_source(owner_token, &other_to, &whitespace_edit, "8", "markdown").await;
        assert_eq!(conflict.status(), StatusCode::CONFLICT);
        let body = response_text(conflict).await?;
        assert!(body.contains("name=\"baseRevision\" value=\"9\""));
        assert!(body.contains("name=\"contentFormat\" value=\"markdown\""));
        assert!(body.contains("name=\"editorMode\" value=\"source\""));
        assert!(body.contains("&gt;  \n </textarea>"));

        let source_limit = "🙂".repeat(60_000);
        let excessive = format!("{source_limit}🙂");
        let invalid = send_source(owner_token, &other_to, &excessive, "9", "markdown").await;
        assert_eq!(invalid.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = response_text(invalid).await?;
        assert!(body.contains("Исходник не должен превышать 60 000 символов."));
        assert!(body.contains(&excessive));
        let saved = send_source(owner_token, &other_to, &source_limit, "9", "markdown").await;
        assert_eq!(saved.status(), StatusCode::SEE_OTHER);
        assert_eq!(stored().await?, (10, source_limit, true));

        let literal_limit = r"\*".repeat(30_000);
        let saved = send_source(owner_token, &edit_to, &literal_limit, "4", "literal").await;
        assert_eq!(saved.status(), StatusCode::SEE_OTHER);
        let edited = response_text(send(owner_token, &edit_to, None).await).await?;
        assert!(edited.contains(&literal_limit));
        let read = response_text(send(owner_token, &read_to, None).await).await?;
        assert!(read.contains(&format!(
            r#"<div class="document-text">{}</div>"#,
            "*".repeat(30_000)
        )));

        let draft = Uuid::new_v4();
        let file = Uuid::new_v4();
        let address = crate::db::PageAddress {
            tenant,
            space,
            page,
            account: *owner,
        };
        assert!(matches!(
            server
                .store
                .stage_attachment(
                    &address,
                    draft,
                    file,
                    "Ожидает.txt",
                    None,
                    b"draft contents"
                )
                .await?,
            crate::db::StageAttachment::Saved(_),
        ));
        let mut unavailable = pool.begin().await?;
        sqlx::query("LOCK TABLE pages IN ACCESS EXCLUSIVE MODE")
            .execute(&mut *unavailable)
            .await?;
        let form = format!(
            "revision=10&text=retry-body&contentFormat=markdown&attachmentDraft={draft}&attachmentAdd={file}"
        );
        let retry = send_encoded(owner_token, &edit_to, Some(form)).await;
        assert_eq!(retry.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = response_text(retry).await?;
        assert!(body.contains(&format!(r#"name="attachmentDraft" value="{draft}""#)));
        assert!(body.contains(&format!(r#"name="attachmentAdd" value="{file}""#)));
        assert!(body.contains("Ожидает.txt"));
        assert!(body.contains("retry-body"));
        assert!(!body.contains("draft contents"));
        unavailable.rollback().await?;
        Ok(())
    }

    async fn response_text(response: Response) -> anyhow::Result<String> {
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await?;
        Ok(std::str::from_utf8(&bytes)?.to_owned())
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
            sharing: SharingDelivery::disabled(),
            presence: PresenceTracker::default(),
            host: "localhost:4125".to_owned(),
            origin: "http://localhost:4125".to_owned(),
        });
        let request = Request::builder()
            .uri(format!("{PATH}?scope=mine&page=3&unexpected=ignored"))
            .header(header::HOST, &server.host)
            .header(header::COOKIE, format!("sever_session={}", "A".repeat(43)))
            .body(Body::empty())
            .unwrap();
        let response = dispatch(State(server.clone()), request).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let body = std::str::from_utf8(&body).unwrap();
        assert!(body.contains("Не удалось открыть документы"));
        assert!(body.contains(&format!(r#"href="{PATH}?scope=mine&amp;page=3""#)));

        let picker = Request::builder()
            .uri(format!("{}/search?kind=page-links", PATH.split("/pages/").next().unwrap()))
            .header(header::HOST, &server.host)
            .header(header::COOKIE, format!("sever_session={}", "A".repeat(43)))
            .body(Body::empty())
            .unwrap();
        let response = dispatch(State(server), picker).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        assert!(serde_json::from_slice::<serde_json::Value>(&body).unwrap()["error"].is_string());
    }

    #[tokio::test]
    async fn cover_routes_do_not_redirect_to_login_or_serve_public_bytes() {
        const PATH: &str = concat!(
            "/workspace/11111111-1111-1111-1111-111111111111",
            "/spaces/22222222-2222-2222-2222-222222222222",
            "/pages/33333333-3333-3333-3333-333333333333/cover",
        );
        let pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        pool.close().await;
        let server = Arc::new(Server {
            store: Store::from_test_pool(pool),
            identity: Identity::new(),
            registration: None,
            sharing: SharingDelivery::disabled(),
            presence: PresenceTracker::default(),
            host: "localhost:4125".to_owned(),
            origin: "http://localhost:4125".to_owned(),
        });
        let image = format!("{PATH}/44444444-4444-4444-4444-444444444444");
        for (method, path, status) in [
            (Method::GET, image.as_str(), StatusCode::NOT_FOUND),
            (Method::HEAD, image.as_str(), StatusCode::NOT_FOUND),
            (Method::POST, PATH, StatusCode::UNAUTHORIZED),
        ] {
            let request = Request::builder()
                .method(method)
                .uri(path)
                .header(header::HOST, &server.host)
                .body(Body::empty())
                .unwrap();
            let response = dispatch(State(server.clone()), request).await;
            assert_eq!(response.status(), status);
            assert!(!response.headers().contains_key(header::LOCATION));
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        }
    }

    #[tokio::test]
    async fn attachment_requests_do_not_redirect_or_expose_metadata_without_a_session() {
        const PATH: &str = concat!(
            "/workspace/11111111-1111-1111-1111-111111111111",
            "/spaces/22222222-2222-2222-2222-222222222222",
            "/pages/33333333-3333-3333-3333-333333333333",
        );
        const FILE: &str = "44444444-4444-4444-4444-444444444444";
        let pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        pool.close().await;
        let server = Arc::new(Server {
            store: Store::from_test_pool(pool),
            identity: Identity::new(),
            registration: None,
            sharing: SharingDelivery::disabled(),
            presence: PresenceTracker::default(),
            host: "localhost:4125".to_owned(),
            origin: "http://localhost:4125".to_owned(),
        });
        let draft = format!("{PATH}/attachments/drafts/{FILE}/{FILE}");
        let published = format!("{PATH}/attachments/{FILE}");
        let archived = format!("{PATH}/revisions/12/attachments/{FILE}/image");
        for (method, path, cookie, status) in [
            (Method::POST, draft.clone(), false, StatusCode::UNAUTHORIZED),
            (
                Method::GET,
                format!("{draft}/image"),
                false,
                StatusCode::NOT_FOUND,
            ),
            (
                Method::HEAD,
                published.clone(),
                false,
                StatusCode::NOT_FOUND,
            ),
            (Method::GET, archived, false, StatusCode::NOT_FOUND),
            (
                Method::GET,
                published,
                true,
                StatusCode::SERVICE_UNAVAILABLE,
            ),
        ] {
            let request = Request::builder()
                .method(method)
                .uri(path)
                .header(header::HOST, &server.host);
            let request = if cookie {
                request.header(header::COOKIE, format!("sever_session={}", "A".repeat(43)))
            } else {
                request
            };
            let response =
                dispatch(State(server.clone()), request.body(Body::empty()).unwrap()).await;
            assert_eq!(response.status(), status);
            assert!(!response.headers().contains_key(header::LOCATION));
            assert!(!response.headers().contains_key(header::CONTENT_DISPOSITION));
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        }
    }

    #[tokio::test]
    async fn private_history_keeps_binary_and_json_routes_off_the_login_redirect() {
        const PATH: &str = concat!(
            "/workspace/11111111-1111-1111-1111-111111111111",
            "/spaces/22222222-2222-2222-2222-222222222222",
            "/pages/33333333-3333-3333-3333-333333333333/revisions",
        );
        let pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        pool.close().await;
        let server = Arc::new(Server {
            store: Store::from_test_pool(pool),
            identity: Identity::new(),
            registration: None,
            sharing: SharingDelivery::disabled(),
            presence: PresenceTracker::default(),
            host: "localhost:4125".to_owned(),
            origin: "http://localhost:4125".to_owned(),
        });
        for (suffix, accept, status) in [
            ("/12/cover", "image/jpeg", StatusCode::NOT_FOUND),
            ("", "application/json", StatusCode::UNAUTHORIZED),
        ] {
            let request = Request::builder()
                .uri(format!("{PATH}{suffix}"))
                .header(header::HOST, &server.host)
                .header(header::ACCEPT, accept)
                .body(Body::empty())
                .unwrap();
            let response = dispatch(State(server.clone()), request).await;
            assert_eq!(response.status(), status);
            assert!(!response.headers().contains_key(header::LOCATION));
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        }
        let request = Request::builder()
            .uri(format!("{PATH}/12?preview=1"))
            .header(header::HOST, &server.host)
            .body(Body::empty())
            .unwrap();
        let response = dispatch(State(server), request).await;
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        let location = response.headers()[header::LOCATION].to_str().unwrap();
        let query = Form::from_query(location.strip_prefix("/?").unwrap());
        assert_eq!(query.get("returnTo"), format!("{PATH}/12?preview=1"));
    }

    #[tokio::test]
    async fn restore_retry_preserves_only_the_confirmed_intent_when_auth_is_unavailable() {
        const PATH: &str = concat!(
            "/workspace/11111111-1111-1111-1111-111111111111",
            "/spaces/22222222-2222-2222-2222-222222222222",
            "/pages/33333333-3333-3333-3333-333333333333/revisions/6/restore",
        );
        let pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        pool.close().await;
        let server = Arc::new(Server {
            store: Store::from_test_pool(pool),
            identity: Identity::new(),
            registration: None,
            sharing: SharingDelivery::disabled(),
            presence: PresenceTracker::default(),
            host: "localhost:4125".to_owned(),
            origin: "http://localhost:4125".to_owned(),
        });
        for (path, origin, form, status) in [
            (
                PATH,
                server.origin.as_str(),
                "baseRevision=8&error=private-payload",
                StatusCode::SERVICE_UNAVAILABLE,
            ),
            (
                PATH,
                server.origin.as_str(),
                "baseRevision=8&baseRevision=9",
                StatusCode::BAD_REQUEST,
            ),
            (
                PATH,
                "https://not-the-workspace.test",
                "baseRevision=8",
                StatusCode::FORBIDDEN,
            ),
            (
                &format!("{PATH}?preview=1"),
                server.origin.as_str(),
                "baseRevision=8",
                StatusCode::FORBIDDEN,
            ),
        ] {
            let request = Request::builder()
                .uri(path)
                .method(Method::POST)
                .header(header::HOST, &server.host)
                .header(header::ORIGIN, origin)
                .header(header::COOKIE, format!("sever_session={}", "A".repeat(43)))
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(form))
                .unwrap();
            let response = dispatch(State(server.clone()), request).await;
            assert_eq!(response.status(), status);
            assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
            let body = response_text(response).await.unwrap();
            if status == StatusCode::SERVICE_UNAVAILABLE {
                assert!(body.contains(&format!(r#"action="{PATH}""#)));
                assert!(body.contains(r#"name="baseRevision" value="8""#));
                assert!(!body.contains("private-payload"));
                assert!(!body.contains("Текущая редакция"));
            } else {
                assert!(!body.contains(r#"name="baseRevision""#));
            }
        }
    }

    #[tokio::test]
    async fn edit_post_preserves_draft_if_session_lookup_is_unavailable() {
        const PATH: &str = concat!(
            "/workspace/11111111-1111-1111-1111-111111111111",
            "/spaces/22222222-2222-2222-2222-222222222222",
            "/pages/33333333-3333-3333-3333-333333333333/edit",
        );
        const TEXT_BASE: &str = "ed2156d2825d29550727a3adf2be2ae2c609c5fca0844a2a978d626eae68c69b";
        let pool = PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new());
        pool.close().await;
        let server = Arc::new(Server {
            store: Store::from_test_pool(pool),
            identity: Identity::new(),
            registration: None,
            sharing: SharingDelivery::disabled(),
            presence: PresenceTracker::default(),
            host: "localhost:4125".to_owned(),
            origin: "http://localhost:4125".to_owned(),
        });
        let request = |origin: &str, source: Option<(&str, &str, &str)>, cover: Option<&str>| {
            let mut form = url::form_urlencoded::Serializer::new(String::new());
            form.append_pair("revision", "8");
            if let Some((source, format, mode)) = source {
                form.append_pair("text", source)
                    .append_pair("contentFormat", format)
                    .append_pair("editorMode", mode);
            } else {
                form.append_pair("text", "\nЧерновик <текста>\n\nПовтор");
            }
            if let Some(cover) = cover {
                form.append_pair("cover", cover)
                    .append_pair("coverBase", "grid")
                    .append_pair("textBase", TEXT_BASE);
            }
            Request::builder()
                .uri(format!("{PATH}?scope=mine"))
                .method(Method::POST)
                .header(header::HOST, &server.host)
                .header(header::ORIGIN, origin)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::COOKIE, format!("sever_session={}", "A".repeat(43)))
                .body(Body::from(form.finish()))
                .unwrap()
        };
        let response = dispatch(State(server.clone()), request(&server.origin, None, None)).await;
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let body = std::str::from_utf8(&body).unwrap();
        assert!(body.contains("Черновик \\&lt;текста\\&gt;\n\nПовтор"));
        assert!(body.contains(r#"name="revision" value="8""#));
        assert!(body.contains(r#"name="contentFormat" value="literal""#));
        assert!(body.contains(r#"name="editorMode" value="visual""#));
        assert!(!body.contains("data-native-source"));
        assert!(body.contains(&format!(r#"action="{PATH}?scope=mine""#)));
        assert!(!body.contains("Дерево пространства"));

        let markdown = "\n## Точный исходник\n\n**Черновик**  \n";
        let retry = dispatch(
            State(server.clone()),
            request(
                &server.origin,
                Some((markdown, "markdown", "source")),
                Some("image:44444444-4444-4444-4444-444444444444"),
            ),
        )
        .await;
        assert_eq!(retry.status(), StatusCode::SERVICE_UNAVAILABLE);
        let policy = retry.headers()[header::CONTENT_SECURITY_POLICY]
            .to_str()
            .unwrap();
        assert!(
            policy
                .split(';')
                .map(str::trim)
                .any(|part| part == "connect-src 'self'")
        );
        assert!(
            policy
                .split(';')
                .map(str::trim)
                .any(|part| part == "img-src 'self'")
        );
        let body = to_bytes(retry.into_body(), 64 * 1024).await.unwrap();
        let body = std::str::from_utf8(&body).unwrap();
        assert!(body.contains(markdown));
        assert!(body.contains(r#"name="revision" value="8""#));
        assert!(body.contains(r#"name="contentFormat" value="markdown""#));
        assert!(body.contains(r#"name="editorMode" value="source""#));
        assert!(
            body.contains(r#"name="cover" value="image:44444444-4444-4444-4444-444444444444""#)
        );
        assert!(body.contains(r#"name="coverBase" value="grid""#));
        assert!(body.contains("/cover/44444444-4444-4444-4444-444444444444"));
        assert!(body.contains(&format!(r#"name="textBase" value="{TEXT_BASE}""#)));
        if body.contains(r#"src="/sever-ui.js""#) {
            assert!(body.contains(r#""textUnchanged":true"#));
        }

        let literal = "\n## Прежний буквальный текст\n\n**Черновик**  \n";
        let retry = dispatch(
            State(server.clone()),
            request(&server.origin, Some((literal, "literal", "visual")), None),
        )
        .await;
        assert_eq!(retry.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = to_bytes(retry.into_body(), 64 * 1024).await.unwrap();
        let body = std::str::from_utf8(&body).unwrap();
        assert!(body.contains(literal));
        assert!(body.contains(r#"name="contentFormat" value="literal""#));
        assert!(body.contains(r#"name="editorMode" value="visual""#));
        assert!(!body.contains("data-native-source"));

        let draft = Uuid::new_v4();
        let file = Uuid::new_v4();
        let form = format!(
            "revision=8&text=pending&attachmentDraft={draft}&attachmentAdd={file}&name=client-supplied-secret.txt"
        );
        let pending = Request::builder()
            .uri(PATH)
            .method(Method::POST)
            .header(header::HOST, &server.host)
            .header(header::ORIGIN, &server.origin)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(header::COOKIE, format!("sever_session={}", "A".repeat(43)))
            .body(Body::from(form))
            .unwrap();
        let retry = dispatch(State(server.clone()), pending).await;
        assert_eq!(retry.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body = response_text(retry).await.unwrap();
        assert!(body.contains(&format!(r#"name="attachmentDraft" value="{draft}""#)));
        assert!(body.contains(&format!(r#"name="attachmentAdd" value="{file}""#)));
        assert!(!body.contains("client-supplied-secret.txt"));
        assert!(!body.contains("page-attachment-row"));

        let foreign = dispatch(
            State(server.clone()),
            request("http://foreign.test", None, None),
        )
        .await;
        assert_eq!(foreign.status(), StatusCode::FORBIDDEN);
        let body = to_bytes(foreign.into_body(), 4096).await.unwrap();
        assert!(!std::str::from_utf8(&body).unwrap().contains("Черновик"));
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
            sharing: SharingDelivery::disabled(),
            presence: PresenceTracker::default(),
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
