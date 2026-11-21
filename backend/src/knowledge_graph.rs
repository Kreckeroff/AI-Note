use std::collections::HashSet;

use axum::{extract::Request, http::StatusCode, response::Response};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    db::{Principal, Store, knowledge_graph::GraphSpace, tasks::TaskScope},
    http::{Form, html, plain},
    text::escape_html,
    workspace::{canonical_uuid, page_history_path, page_path, space_path},
    workspace_chrome,
};

#[derive(Clone, Copy)]
pub(crate) struct GraphRoute {
    tenant: Uuid,
    space: Uuid,
}

impl GraphRoute {
    pub(crate) fn parse(path: &str) -> Option<Self> {
        let mut path = path.strip_prefix("/workspace/")?.split('/');
        let tenant = canonical_uuid(path.next()?)?;
        if path.next()? != "spaces" {
            return None;
        }
        let space = canonical_uuid(path.next()?)?;
        (path.next()? == "graph" && path.next().is_none()).then_some(Self { tenant, space })
    }
}

#[derive(Serialize)]
struct Node {
    key: String,
    name: String,
    family: &'static str,
    href: String,
    detail: String,
    at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    document: Option<Uuid>,
}

#[derive(Serialize)]
struct Edge {
    from: String,
    to: String,
    at: Option<i64>,
}

pub(crate) async fn handle(
    store: &Store,
    principal: &Principal,
    route: GraphRoute,
    request: Request,
) -> Result<Response, sqlx::Error> {
    let scope = TaskScope {
        tenant: route.tenant,
        space: route.space,
        account: principal.id,
    };
    let Some(GraphSpace {
        area,
        pages,
        history,
        tasks,
        results,
    }) = store.graph_space(scope).await?
    else {
        return Ok(plain(StatusCode::NOT_FOUND, "Пространство недоступно."));
    };
    let base = space_path(route.tenant, route.space);
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    for page in pages {
        let key = page.id.to_string();
        if let Some(parent) = page.parent_id {
            edges.push(Edge {
                from: parent.to_string(),
                to: key.clone(),
                at: None,
            });
        }
        nodes.push(Node {
            key,
            name: page.title,
            family: "document",
            href: page_path(route.tenant, route.space, page.id),
            detail: format!("Редакция {}", page.revision),
            at: page.at,
            document: None,
        });
    }
    for revision in history {
        let owner = revision.page.to_string();
        let key = format!("{owner}-r{}", revision.revision);
        edges.push(Edge {
            from: owner,
            to: key.clone(),
            at: Some(revision.at),
        });
        nodes.push(Node {
            key,
            name: revision.title,
            family: "record",
            href: format!(
                "{}/{}",
                page_history_path(route.tenant, route.space, revision.page),
                revision.revision
            ),
            detail: format!("Редакция {}", revision.revision),
            at: Some(revision.at),
            document: Some(revision.page),
        });
    }
    for task in tasks {
        let key = task.id.to_string();
        if let Some(source) = task.source_page {
            edges.push(Edge {
                from: source.to_string(),
                to: key.clone(),
                at: Some(task.created_at),
            });
        }
        nodes.push(Node {
            href: format!("{base}/tasks/{key}"),
            key,
            name: task.title,
            family: "work",
            detail: format!("Задача {}", task.number),
            at: Some(task.created_at),
            document: None,
        });
    }
    edges.extend(results.into_iter().map(|result| Edge {
        from: result.task.to_string(),
        to: result.page.to_string(),
        at: Some(result.at),
    }));
    let keys: HashSet<_> = nodes.iter().map(|node| node.key.as_str()).collect();
    edges.retain(|edge| keys.contains(edge.from.as_str()) && keys.contains(edge.to.as_str()));
    let query = Form::from_query(request.uri().query().unwrap_or_default());
    let key = nodes
        .iter()
        .find(|node| node.key == query.get("node"))
        .map(|node| &node.key);
    let model = serde_json::json!({
        "nodes": nodes, "edges": edges, "key": key,
        "tenant": area.tenant_name, "space": area.space_name
    });
    let data = model.to_string().replace('<', "\\u003c");
    let body = if nodes.is_empty() {
        format!(
            r#"<section class="parity-area"><h1>Граф знаний</h1><p class="area-subtitle">В пространстве пока нет доступных документов или задач.</p><a href="{base}">К документам</a></section>"#
        )
    } else {
        let fallback = nodes
            .iter()
            .take(25)
            .map(|node| {
                format!(
                    r#"<li><a href="{}">{}</a></li>"#,
                    escape_html(&node.href),
                    escape_html(&node.name)
                )
            })
            .collect::<String>();
        format!(
            r#"<div id="workspace-graph" class="parity-graph"><noscript><h1>Граф знаний</h1><ul>{fallback}</ul></noscript></div><script type="application/json" id="workspace-graph-data">{data}</script>"#
        )
    };
    Ok(html(
        StatusCode::OK,
        workspace_chrome::page(
            "Граф знаний",
            Some((route.tenant, &area.tenant_name)),
            Some((route.space, &area.space_name)),
            "graph",
            &body,
        ),
    ))
}
