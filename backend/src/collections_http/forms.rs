use axum::http::StatusCode;
use std::fmt::Write as _;

use super::{Form, Route, View, ViewTarget};
use crate::{
    db::collections::{FIELD_TYPES, Field},
    text::escape_html as h,
    workspace_chrome::icon,
};
use uuid::Uuid;

fn type_name(kind: &str) -> &str {
    FIELD_TYPES
        .iter()
        .find(|(k, _)| *k == kind)
        .map_or("", |(_, v)| *v)
}
fn title_field(value: &str) -> String {
    format!(
        r#"<label class="parity-field">Название<input name="title" required value="{}"></label>"#,
        h(value)
    )
}
fn paged(page: u32, has_more: bool, url: &str) -> String {
    if page == 1 && !has_more {
        return String::new();
    }
    let mut out = String::from(r#"<nav class="collection-pager" aria-label="Страницы">"#);
    if page > 1 {
        let _ = write!(out, r#"<a href="{url}?page={}">Назад</a>"#, page - 1);
    }
    let _ = write!(out, "<span>Страница {page}</span>");
    if has_more {
        let _ = write!(
            out,
            r#"<a href="{url}?page={}">Далее</a>"#,
            page.saturating_add(1)
        );
    }
    out.push_str("</nav>");
    out
}

pub(super) fn body(
    route: &Route,
    view: &View,
    page: u32,
    form: Option<&Form>,
    warning: Option<(StatusCode, &str)>,
) -> (String, String) {
    let home = route.home();
    let title = match route.target {
        ViewTarget::Catalog => "Реестры".into(),
        ViewTarget::Table(_) => view.collections[0].title.clone(),
        ViewTarget::NewRecord(_) => "Новая запись".into(),
        ViewTarget::Record(_, _) => view.records[0].title.clone(),
    };
    let crumb = match route.target {
        ViewTarget::Catalog => format!(
            r#"<a href="{}/tasks">Задачи</a>"#,
            crate::workspace::space_path(route.tenant, route.space)
        ),
        ViewTarget::Table(_) => format!(r#"<a href="{home}">Реестры</a>"#),
        ViewTarget::NewRecord(c) | ViewTarget::Record(c, _) => format!(
            r#"<a href="{home}">Реестры</a><span aria-hidden="true">›</span><a href="{home}/{c}">{}</a>"#,
            h(&view.collections[0].title)
        ),
    };
    let mut out = format!(
        r#"<section class="parity-area collection-area"><nav class="collection-crumbs" aria-label="Путь">{crumb}</nav><h1>{}</h1>"#,
        h(&title)
    );
    if let Some((_, error)) = warning {
        let _ = write!(
            out,
            r#"<p role="alert" class="parity-error">{}</p>"#,
            h(error)
        );
    }
    let opened = if warning.is_some() { " open" } else { "" };
    match route.target {
        ViewTarget::Catalog => {
            if view.editable {
                let _ = write!(
                    out,
                    r#"<details class="parity-surface"{opened}><summary>Создать реестр</summary><form method="post" action="{home}" class="collection-form"><input type="hidden" name="operation" value="{}">{}<button class="space-action primary-action">Создать реестр</button></form></details>"#,
                    Uuid::new_v4(),
                    title_field(form.map_or("", |f| f.get("title")))
                );
            }
            if view.collections.is_empty() {
                out.push_str("<p>Здесь пока нет реестров.</p>");
            } else {
                out.push_str(r#"<ul class="collection-list parity-surface">"#);
                for c in &view.collections {
                    let _ = write!(
                        out,
                        r#"<li><a href="{home}/{}">{}{}</a></li>"#,
                        c.id,
                        icon("table"),
                        h(&c.title)
                    );
                }
                out.push_str("</ul>");
            }
            out.push_str(&paged(page, view.has_more, &home));
        }
        ViewTarget::Table(c) => {
            if view.editable {
                let _ = write!(
                    out,
                    r#"<div class="collection-tools"><a class="space-action primary-action" href="{home}/{c}/records/new">{}Новая запись</a></div>"#,
                    icon("plus")
                );
                let mut opts = String::new();
                let kind = form.map_or("text", |f| f.get("kind"));
                for (key, name) in FIELD_TYPES {
                    let selected = if key == kind { " selected" } else { "" };
                    let _ = write!(opts, r#"<option value="{key}"{selected}>{name}</option>"#);
                }
                let _ = write!(
                    out,
                    r#"<details class="parity-surface"{opened}><summary>Добавить поле</summary><form action="{home}/{c}" method="post" class="collection-form"><input type="hidden" name="operation" value="{}">{}<label class="parity-field">Тип поля<select name="kind">{opts}</select></label><button class="space-action space-secondary">Добавить поле</button></form></details>"#,
                    Uuid::new_v4(),
                    title_field(form.map_or("", |f| f.get("title")))
                );
            }
            out.push_str(r#"<div class="collection-grid" tabindex="0" role="region" aria-label="Записи реестра"><table><thead><tr><th scope="col">Название</th>"#);
            for f in &view.fields {
                let _ = write!(
                    out,
                    r#"<th scope="col">{}<span>{}</span></th>"#,
                    h(&f.title),
                    type_name(&f.kind)
                );
            }
            out.push_str("</tr></thead><tbody>");
            for r in &view.records {
                let _ = write!(
                    out,
                    r#"<tr><th scope="row"><a href="{home}/{c}/records/{}">{}</a></th>"#,
                    r.id,
                    h(&r.title)
                );
                for f in &view.fields {
                    let _ = write!(
                        out,
                        "<td>{}</td>",
                        display(f, r.values.get(&f.id.to_string()).map(String::as_str))
                    );
                }
                out.push_str("</tr>");
            }
            out.push_str("</tbody></table></div>");
            if view.records.is_empty() {
                out.push_str(
                    r#"<p class="collection-muted">На этой странице пока нет записей.</p>"#,
                );
            }
            out.push_str(&paged(page, view.has_more, &route.current()));
        }
        ViewTarget::NewRecord(_) | ViewTarget::Record(_, _) => {
            let rebase = warning.is_some_and(|(status, _)| status == StatusCode::CONFLICT);
            record_form(&mut out, route, view, form, rebase)
        }
    }
    out.push_str("</section>");
    (title, out)
}
fn display(field: &Field, value: Option<&str>) -> String {
    if field.kind == "checkbox" {
        return if value == Some("true") {
            "Да"
        } else {
            "Нет"
        }
        .into();
    }
    value
        .filter(|v| !v.is_empty())
        .map_or_else(|| "—".into(), h)
}
fn record_form(
    out: &mut String,
    route: &Route,
    view: &View,
    form: Option<&Form>,
    show_current: bool,
) {
    let record = view.records.first();
    let current = route.current();
    if !view.editable {
        out.push_str(r#"<dl class="parity-surface collection-record-values">"#);
        for f in &view.fields {
            let v = record
                .and_then(|r| r.values.get(&f.id.to_string()))
                .map(String::as_str);
            let _ = write!(
                out,
                "<div><dt>{}</dt><dd>{}</dd></div>",
                h(&f.title),
                display(f, v)
            );
        }
        out.push_str("</dl>");
        return;
    }
    if show_current && record.is_some() {
        let _ = write!(
            out,
            r#"<a href="{current}" target="_blank" rel="noopener">Открыть сохранённую версию в новой вкладке</a>"#
        );
    }
    // Validation redisplays the author's snapshot. A conflict explicitly authorizes
    // the subsequent submit button to use the newly displayed baseline.
    let baseline = |name, current: i64| match form {
        Some(f) if !show_current => h(f.get(name)),
        _ => current.to_string(),
    };
    let _ = write!(
        out,
        r#"<form class="parity-surface collection-form" action="{current}" method="post"><input type="hidden" name="schema" value="{}">"#,
        baseline("schema", view.collections[0].schema_version)
    );
    if let Some(r) = record {
        let _ = write!(
            out,
            r#"<input type="hidden" name="version" value="{}">"#,
            baseline("version", r.version)
        );
    } else {
        let _ = write!(
            out,
            r#"<input type="hidden" name="operation" value="{}">"#,
            Uuid::new_v4()
        );
    }
    let name = form
        .map(|f| f.get("title"))
        .or_else(|| record.map(|r| r.title.as_str()))
        .unwrap_or("");
    out.push_str(&title_field(name));
    for field in &view.fields {
        let key = field.id.to_string();
        let value = form
            .map(|f| f.get(&key))
            .or_else(|| record.and_then(|r| r.values.get(&key).map(String::as_str)))
            .unwrap_or("");
        let name = h(&field.title);
        match field.kind.as_str() {
            "checkbox" => {
                let checked = if value == "true" { " checked" } else { "" };
                let _ = write!(
                    out,
                    r#"<label class="collection-check"><input name="{key}" type="checkbox" value="true"{checked}>{name}</label>"#
                );
            }
            "text" => {
                let _ = write!(
                    out,
                    r#"<label class="parity-field">{name}<textarea rows="2" name="{key}">
{}</textarea></label>"#,
                    h(&crate::text::normalize_line_endings(value))
                );
            }
            kind => {
                let attrs = if kind == "number" {
                    r#"type="number" step="any""#
                } else {
                    r#"type="date" min="0001-01-01" max="9999-12-31""#
                };
                let _ = write!(
                    out,
                    r#"<label class="parity-field">{name}<input name="{key}" {attrs} value="{}"></label>"#,
                    h(value)
                );
            }
        }
    }
    let action = if record.is_some() && show_current {
        "Сохранить мою версию"
    } else {
        "Сохранить запись"
    };
    let _ = write!(
        out,
        r#"<button class="space-action primary-action">{action}</button></form>"#
    );
}
