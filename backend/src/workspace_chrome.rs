use std::fmt::Write as _;

use axum::{
    http::{StatusCode, Uri},
    response::Response,
};
use serde_json::json;
use uuid::Uuid;

use crate::{
    db::{Principal, Store},
    external_sharing::{json as response_json, sharing_error},
    text::escape_html as html,
    workspace::{
        CatalogOptions, WorkspaceContext, canonical_uuid, page_path, space_path, spaces_path,
    },
    workspace_icons,
};

pub(crate) struct ChromeRoute {
    tenant: Uuid,
    recent: bool,
}

impl ChromeRoute {
    pub(crate) fn parse(path: &str) -> Option<Self> {
        let (tenant, action) = path.strip_prefix("/workspace/")?.split_once('/')?;
        Some(Self {
            tenant: canonical_uuid(tenant)?,
            recent: match action {
                "chrome" => false,
                "recent" => true,
                _ => return None,
            },
        })
    }

    pub(crate) async fn handle(
        self,
        store: &Store,
        principal: &Principal,
        uri: &Uri,
    ) -> Result<Response, sqlx::Error> {
        if self.recent {
            let ids: Vec<_> =
                url::form_urlencoded::parse(uri.query().unwrap_or_default().as_bytes())
                    .filter(|(key, _)| key == "id")
                    .take(12)
                    .filter_map(|(_, id)| canonical_uuid(&id))
                    .collect();
            let rows = store
                .readable_page_titles(principal.id, self.tenant, &ids)
                .await?;
            let items: Vec<_> = ids
                .iter()
                .filter_map(|id| {
                    let (_, space, title) = rows.iter().find(|row| row.0 == *id)?;
                    Some(json!({"path": page_path(self.tenant, *space, *id), "label": title}))
                })
                .collect();
            return Ok(response_json(json!({"items": items})));
        }

        let (context, _) = store.workspace(principal.id, Some(self.tenant)).await?;
        let Some(context) = context else {
            return Ok(sharing_error(
                StatusCode::NOT_FOUND,
                "Организация недоступна.",
            ));
        };
        let context = WorkspaceContext::build(context, principal, CatalogOptions::parse(""), true);
        let tenants: Vec<_> = context
            .tenants
            .iter()
            .map(|value| json!({"id": value.id, "name": value.name}))
            .collect();
        let spaces: Vec<_> = context.available_spaces.iter().map(|value| json!({
            "id": value.id, "name": value.name, "owner": value.owner_name,
            "isOwner": value.is_owner,
            "canCreateSection": value.is_owner || value.grant_role.as_deref() == Some("editor"),
            "canManageAccess": value.is_owner || context.tenant.role != "member"
        })).collect();
        Ok(response_json(json!({
            "accountId": principal.id, "tenants": tenants, "spaces": spaces
        })))
    }
}

pub(crate) fn icon(name: &str) -> String {
    workspace_icons::path(name).map_or_else(String::new, |path| {
        format!(r#"<svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round"><path d="{path}"/></svg>"#)
    })
}

fn nav_link(out: &mut String, href: &str, key: &str, label: &str, glyph: &str, active: &str) {
    let current = if key == active {
        r#" aria-current="page""#
    } else {
        ""
    };
    let _ = write!(
        out,
        r#"<a href="{}"{current}>{}<span>{label}</span></a>"#,
        html(href),
        icon(glyph)
    );
}

fn dock(tenant: Option<(Uuid, &str)>, space: Option<(Uuid, &str)>, active: &str) -> String {
    let mut nav = String::new();
    let context = tenant.map_or_else(
        || format!(r#"<a class="dock-tenant" href="/workspace">{}<span>Север</span></a>"#, icon("north")),
        |(id, name)| {
            nav_link(&mut nav, &spaces_path(id), "spaces", "Пространства", "layers", active);
            if let Some((space, _)) = space {
                let base = space_path(id, space);
                for (suffix, key, label, glyph) in [
                    ("", "documents", "Документы", "docs"),
                    ("/graph", "graph", "Граф", "graph"),
                    ("/approach", "approach", "AI-подход", "flow"),
                    ("/tasks", "tasks", "Задачи", "board"),
                ] {
                    nav_link(&mut nav, &format!("{base}{suffix}"), key, label, glyph, active);
                }
            }
            let current = space.map_or_else(
                || "Выбрать пространство".to_owned(),
                |(_, name)| html(name),
            );
            format!(r#"<details class="dock-choice" data-native-picker="tenants"><summary class="dock-tenant" aria-label="Сменить среду. Текущая: {}"><span class="parity-brand"><img src="/workspace/{id}/organization/logo" alt=""></span><span>{}</span>{}</summary><div class="dock-panel"><strong>Выбрать среду</strong><ul></ul><a href="/workspace/{id}/organization">Главная организации</a></div></details>
<span class="dock-divider">{}</span>
<details class="dock-choice" data-native-picker="spaces"><summary class="dock-space" aria-label="Сменить пространство. Текущее: {current}">{}<span>{current}</span>{}</summary><div class="dock-panel"><label class="picker-search"><span class="sr-only">Поиск пространства в меню</span><input type="search" placeholder="Найти пространство" maxlength="200"></label><ul></ul><a class="picker-all" href="{}">Все пространства</a></div></details>"#, html(name), html(name), icon("down"), icon("chevron"), icon("folder"), icon("down"), spaces_path(id))
        },
    );
    let search = tenant.map_or_else(String::new, |(id, _)| format!(
        r#"<a class="parity-icon" data-workspace-search href="/workspace/{id}/search" aria-label="Поиск" title="Поиск · ⌘K">{}</a>"#, icon("search")
    ));
    let identity = tenant.map_or_else(String::new, |(id, _)| {
        format!(r#" data-workspace-tenant="{id}""#)
    });
    let current_space = space.map_or_else(String::new, |(id, _)| {
        format!(r#" data-workspace-space="{id}""#)
    });
    let quick_create = space.map_or_else(String::new, |_| format!(
        r#"<details class="dock-choice" data-native-picker="create"><summary class="parity-icon parity-create" aria-label="Создать" title="Создать">{}</summary><div class="dock-panel"><strong>Создать</strong><ul></ul></div></details>"#, icon("plus")
    ));
    let account_tools = tenant.map_or_else(String::new, |_| format!(
        r#"<a hidden class="parity-icon" data-native-space-access aria-label="Настроить доступ" title="Настроить доступ">{}</a>
<details class="page-notifications" data-native-notifications><summary class="parity-icon" aria-label="Уведомления" title="Уведомления">{}</summary><div class="page-notifications-panel"><h2>Уведомления</h2><ul></ul><div class="collaboration-pager"></div></div></details>
<a class="parity-profile" href="/workspace/profile" title="Учётная запись" aria-label="Учётная запись">{}</a>"#, icon("sliders"), icon("bell"), icon("user")
    ));
    format!(
        r#"<header class="workspace-topbar parity-topbar"{identity}{current_space}>
<div class="parity-history">
<button type="button" class="parity-icon" data-workspace-history="back" aria-label="Назад" title="Назад">{}</button>
<button type="button" class="parity-icon parity-forward" data-workspace-history="forward" aria-label="Вперёд" title="Вперёд">{}</button>
<details class="dock-choice parity-recent" data-workspace-recent><summary class="parity-icon" aria-label="Недавние документы" title="Недавние документы">{}</summary><div class="dock-panel"><strong>Недавние документы</strong><ul></ul></div></details>
</div>
<div class="workspace-dock"><div class="dock-identity">{context}</div>
<nav class="dock-nav" aria-label="Рабочие разделы">{nav}</nav><div class="parity-quick">{search}{quick_create}</div></div>
<button type="button" hidden class="parity-icon workspace-account-toggle" popovertarget="workspace-account-menu" aria-label="Меню учётной записи" title="Меню учётной записи">{}</button>
<div class="workspace-account-tools" id="workspace-account-menu"><button type="button" class="parity-icon" data-workspace-theme aria-label="Переключить тему" title="Переключить тему">{}</button>{account_tools}</div>
</header>"#,
        icon("left"),
        icon("arrow"),
        icon("clock"),
        icon("user"),
        icon("sun")
    )
}

pub(crate) fn page(
    title: &str,
    tenant: Option<(Uuid, &str)>,
    space: Option<(Uuid, &str)>,
    active: &str,
    body: &str,
) -> String {
    let optional_assets = match active {
        "collections" => r#"<link rel="stylesheet" href="/collections.css">"#,
        "organization" => {
            r#"<link rel="stylesheet" href="/organization.css"><script type="module" src="/organization.mjs"></script>"#
        }
        "tasks" => {
            r#"<link rel="stylesheet" href="/tasks.css"><script src="/tasks.js" defer></script>"#
        }
        "graph" => r#"<script src="/workspace-graph.js" defer></script>"#,
        _ => "",
    };
    format!(
        r##"<!doctype html><html lang="ru"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1"><title>{} · Север</title>
<link rel="stylesheet" href="/auth.css"><link rel="stylesheet" href="/spaces.css"><link rel="stylesheet" href="/parity.css">
<script src="/workspace-chrome.js" defer></script>{optional_assets}</head>
<body class="parity-workspace"><a class="sr-only parity-skip" href="#workspace-body">К рабочей области</a>{}<main id="workspace-body">{body}</main></body></html>"##,
        html(title),
        dock(tenant, space, active)
    )
}
