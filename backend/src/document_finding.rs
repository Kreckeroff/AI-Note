use std::fmt::Write as _;

use axum::{
    Json,
    extract::Request,
    http::{Method, StatusCode},
    response::{IntoResponse, Response},
};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    db::{
        Principal, Store,
        document_finding::{SEARCH_PAGE_SIZE, SearchResults},
    },
    http::{Form, html, method_not_allowed, plain},
    tasks_http::task_path,
    text::{escape_html, js_trim},
    workspace::{canonical_uuid, page_path, space_path},
    workspace_chrome,
};

const PAGE_LINK_KIND: &str = "page-links";

pub(crate) fn is_page_link_request(query: Option<&str>) -> bool {
    Form::from_query(query.unwrap_or_default()).get("kind") == PAGE_LINK_KIND
}

pub(crate) fn page_link_search_error(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

#[derive(Clone, Copy)]
pub(crate) struct FindingRoute {
    pub tenant: Uuid,
    pub space: Option<Uuid>,
}

impl FindingRoute {
    pub(crate) fn parse(path: &str) -> Option<Self> {
        let mut segments = path.strip_prefix("/workspace/")?.split('/');
        let tenant = canonical_uuid(segments.next()?)?;
        let space = match segments.next()? {
            "search" => None,
            "spaces" => {
                let id = canonical_uuid(segments.next()?)?;
                if segments.next()? != "search" {
                    return None;
                }
                Some(id)
            }
            _ => return None,
        };
        segments.next().is_none().then_some(Self { tenant, space })
    }

    fn location(self) -> String {
        match self.space {
            Some(space) => format!("{}/search", space_path(self.tenant, space)),
            None => format!("/workspace/{}/search", self.tenant),
        }
    }

    fn location_for(self, query: &str, page: i64) -> String {
        let mut params = url::form_urlencoded::Serializer::new(String::new());
        params.append_pair("q", query);
        if page > 1 {
            params.append_pair("page", &page.to_string());
        }
        format!("{}?{}", self.location(), params.finish())
    }
}

struct SearchInput {
    query: String,
    page: i64,
}

impl SearchInput {
    fn parse(raw: &str) -> Self {
        let mut query = None;
        let mut page = None;
        for (key, value) in url::form_urlencoded::parse(raw.as_bytes()) {
            if key == "q" && query.is_none() {
                query = Some(
                    js_trim(&value)
                        .chars()
                        .filter(|ch| !ch.is_control())
                        .take(100)
                        .collect(),
                );
            } else if key == "page" && page.is_none() {
                page = value
                    .parse::<i64>()
                    .ok()
                    .filter(|page| (1..=10_000).contains(page));
            }
        }
        Self {
            query: query.unwrap_or_default(),
            page: page.unwrap_or(1),
        }
    }
}

pub(crate) async fn handle(
    store: &Store,
    principal: &Principal,
    route: FindingRoute,
    _origin: &str,
    request: Request,
) -> Result<Response, sqlx::Error> {
    if !matches!(*request.method(), Method::GET | Method::HEAD) {
        return Ok(method_not_allowed("GET, HEAD"));
    }
    let raw_query = request.uri().query().unwrap_or_default();
    let input = SearchInput::parse(raw_query);
    let params = Form::from_query(raw_query);
    if params.get("kind") == PAGE_LINK_KIND {
        return page_link_search(store, principal, route, input, &params).await;
    }
    let Some(results) = store
        .search_documents(
            route.tenant,
            route.space,
            principal.id,
            &input.query,
            input.page,
        )
        .await?
    else {
        return Ok(plain(StatusCode::NOT_FOUND, "Поиск недоступен."));
    };
    let (workspace, _) = store.workspace(principal.id, Some(route.tenant)).await?;
    let Some(workspace) = workspace else {
        return Ok(plain(StatusCode::NOT_FOUND, "Поиск недоступен."));
    };
    let current_space = route
        .space
        .and_then(|id| workspace.spaces.iter().find(|space| space.id == id));
    if route.space.is_some() && current_space.is_none() {
        return Ok(plain(StatusCode::NOT_FOUND, "Поиск недоступен."));
    }
    let scope = current_space
        .map(|space| space.name.as_str())
        .unwrap_or("Все пространства");
    Ok(html(
        StatusCode::OK,
        render(
            route,
            &input,
            results,
            &workspace.tenant.name,
            current_space.map(|space| (space.id, scope)),
        ),
    ))
}

#[derive(Serialize)]
struct PageLink {
    title: String,
    section: String,
    path: String,
}

#[derive(Serialize)]
struct PageLinkResults {
    pages: Vec<PageLink>,
    next: bool,
}

async fn page_link_search(
    store: &Store,
    principal: &Principal,
    route: FindingRoute,
    input: SearchInput,
    params: &Form,
) -> Result<Response, sqlx::Error> {
    let unavailable = || page_link_search_error(StatusCode::NOT_FOUND, "Пространство недоступно.");
    let Some(space) = route.space else {
        return Ok(unavailable());
    };
    let target = match params.field("target") {
        Ok(None) => None,
        Ok(Some(value)) if let Some(id) = canonical_uuid(value) => Some(id),
        _ => {
            return Ok(page_link_search_error(
                StatusCode::BAD_REQUEST,
                "Некорректный адрес страницы.",
            ));
        }
    };
    let Some(matches) = store
        .search_page_links(
            route.tenant,
            space,
            principal.id,
            &input.query,
            target,
            input.page,
        )
        .await?
    else {
        return Ok(unavailable());
    };
    let next = matches.len() > SEARCH_PAGE_SIZE as usize;
    let pages = matches
        .into_iter()
        .take(SEARCH_PAGE_SIZE as usize)
        .map(|item| PageLink {
            path: page_path(route.tenant, space, item.id),
            title: item.title,
            section: item.section_title,
        })
        .collect();
    Ok(Json(PageLinkResults { pages, next }).into_response())
}

fn render(
    route: FindingRoute,
    input: &SearchInput,
    results: SearchResults,
    tenant_name: &str,
    space: Option<(Uuid, &str)>,
) -> String {
    let has_next = results.next;
    let mut links = String::new();
    for item in results.items {
        let path = match (item.space_title, item.section_title, item.task_number) {
            (Some(space), _, Some(number)) => format!(
                r#"<p class="space-summary">{} · KB-{number:03}</p>"#,
                escape_html(&space),
            ),
            (Some(space), Some(section), None) => format!(
                r#"<p class="space-summary">{} / {}</p>"#,
                escape_html(&space),
                escape_html(&section),
            ),
            _ => String::new(),
        };
        let snippet = if item.snippet.is_empty() {
            String::new()
        } else {
            format!(
                r#"<p class="space-summary">{}</p>"#,
                escape_html(&item.snippet)
            )
        };
        let href = if item.task_number.is_some() {
            task_path(route.tenant, item.space_id, item.id)
        } else {
            page_path(route.tenant, item.space_id, item.id)
        };
        let _ = write!(
            links,
            r#"<li><h2><a href="{href}">{}</a></h2>{path}{snippet}</li>"#,
            escape_html(&item.title)
        );
    }
    let results = if links.is_empty() {
        if input.query.is_empty() {
            r#"<p class="space-summary">Введите название, фрагмент текста или ID задачи.</p>"#
                .to_owned()
        } else {
            r#"<p class="space-summary">Ничего не найдено.</p>"#.to_owned()
        }
    } else {
        format!(r#"<ul class="finding-results">{links}</ul>"#)
    };
    let previous = (input.page > 1)
        .then(|| {
            format!(
                r#"<a class="space-action space-secondary" href="{}">Назад</a>"#,
                escape_html(&route.location_for(&input.query, input.page - 1))
            )
        })
        .unwrap_or_default();
    let next = if has_next {
        format!(
            r#"<a class="space-action space-secondary" href="{}">Дальше</a>"#,
            escape_html(&route.location_for(&input.query, input.page + 1))
        )
    } else {
        String::new()
    };
    let all = route
        .space
        .map(|_| {
            format!(
                r#"<a class="space-action space-secondary" href="{}">Во всех пространствах</a>"#,
                escape_html(
                    &FindingRoute {
                        space: None,
                        ..route
                    }
                    .location_for(&input.query, 1)
                )
            )
        })
        .unwrap_or_default();
    let scope = space.map_or("Все пространства", |(_, name)| name);
    let body = format!(
        r#"<section class="space-shell">
<header class="space-heading"><div><h1>Поиск в рабочем пространстве</h1><p class="space-summary">{scope}</p></div></header>
<form method="get" class="finding-form" action="{action}"><label for="document-query">Название, текст или ID задачи</label>
<input id="document-query" type="search" name="q" maxlength="100" value="{query}" autocomplete="off">
<button type="submit" class="space-action primary-action">Найти</button>{all}</form>
<section aria-label="Результаты">{results}</section>
<nav aria-label="Страницы поиска">{previous}{next}</nav>
</section>"#,
        scope = escape_html(scope),
        action = route.location(),
        query = escape_html(&input.query)
    );
    workspace_chrome::page(
        "Поиск",
        Some((route.tenant, tenant_name)),
        space,
        "search",
        &body,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_routes_and_bounded_query() {
        let tenant = Uuid::new_v4();
        let space = Uuid::new_v4();
        assert!(FindingRoute::parse(&format!("/workspace/{tenant}/search/extra")).is_none());
        assert!(
            FindingRoute::parse(&format!("/workspace/{tenant}/spaces/{space}/search")).is_some()
        );
        let input = SearchInput::parse("q=%20word%20&q=ignored&page=10001");
        assert_eq!(input.query, "word");
        assert_eq!(input.page, 1);
    }
}
