use std::fmt::Write;

use crate::{
    db::Space,
    sections::{SectionForm, SectionScreen},
    workspace::{WorkspaceContext, new_section_path, section_path, with_options},
};

use super::{PageAssets, html, icon, page};

pub(super) fn tree_sidebar(
    ctx: &WorkspaceContext,
    space: &Space,
    tree: &SectionScreen,
    catalog_href: &str,
) -> String {
    let options = &ctx.catalog.options;
    let selected = tree.selected.as_ref().map(|section| section.id);
    let folder = icon("folder");
    let rows = if tree.sections.is_empty() {
        "<p>Пока нет разделов</p>".to_owned()
    } else {
        let mut rows = String::from(r#"<ul class="section-list">"#);
        for section in &tree.sections {
            let href = with_options(&section_path(ctx.tenant.id, space.id, section.id), options);
            let current = if Some(section.id) == selected {
                r#" aria-current="page""#
            } else {
                ""
            };
            write!(
                rows,
                r#"<li><a data-section-id="{id}" href="{href}"{current}>{folder}<span>{title}</span></a></li>"#,
                id = section.id,
                href = html(&href),
                title = html(&section.title),
            )
            .unwrap();
        }
        rows.push_str("</ul>");
        rows
    };
    let create = if tree.can_create_section {
        let href = new_section_path(ctx.tenant.id, space.id, selected, options);
        let action = if tree.sections.is_empty() {
            "primary-action"
        } else {
            "space-secondary"
        };
        format!(
            r#"<a id="create-section" class="space-action section-create-link {action}" href="{href}">{add}Создать раздел</a>"#,
            href = html(&href),
            add = icon("add"),
        )
    } else {
        String::new()
    };
    format!(
        r#"<aside class="documents-tree" aria-label="Дерево пространства">
      <span class="documents-tree-title">{folder}{name}</span>
      <a href="{catalog_href}">{layers}Все пространства</a>
      <details open><summary>Разделы</summary>{rows}</details>
      {create}
    </aside>"#,
        name = html(&space.name),
        catalog_href = html(catalog_href),
        layers = icon("layers"),
    )
}

pub(super) fn creation_dialog(form: &SectionForm, space_name: Option<&str>) -> String {
    let cancel = html(&form.cancel_to);
    let (error_ref, error) = if form.error.is_empty() {
        ("", String::new())
    } else {
        let retry = form.retry_to.as_ref().map_or_else(String::new, |href| {
            format!(r#" <a href="{}">Открыть новую форму</a>"#, html(href))
        });
        (
            " section-error",
            format!(
                r#"<p class="form-error" id="section-error" role="alert">{}{retry}</p>"#,
                html(&form.error),
            ),
        )
    };
    let invalid = if form.invalid_name {
        r#" aria-invalid="true""#
    } else {
        ""
    };
    let location = space_name.map_or_else(String::new, |name| {
        format!(
            r#"<p class="space-dialog-tenant">{}{}</p>"#,
            icon("folder"),
            html(name),
        )
    });
    format!(
        r#"<dialog id="section-dialog" class="space-dialog section-dialog" aria-labelledby="section-dialog-title" open>
      <header class="space-dialog-header">
        <div><h2 id="section-dialog-title">Новый раздел</h2>{location}</div>
        <a class="space-action space-close" href="{cancel}" data-section-cancel aria-label="Закрыть">{close}</a>
      </header>
      <form class="space-form" method="post" action="{action}" novalidate>
        <input type="hidden" name="operation_id" value="{operation_id}">
        <label for="section-name">Название</label>
        <input id="section-name" name="name" value="{name}" placeholder="Например, Документация" autofocus required
          aria-describedby="section-help{error_ref}"{invalid}>
        <p class="space-hint" id="section-help">От 1 до 100 символов. Название должно быть уникальным в пространстве.</p>
        <p class="form-error" id="section-client-error" role="alert" hidden></p>
        {error}
        <div class="space-dialog-actions">
          <a class="space-action space-secondary" href="{cancel}" data-section-cancel>Отмена</a>
          <button class="primary-action space-action" type="submit">Создать</button>
        </div>
      </form>
      <p class="sr-only" role="status" data-section-status></p>
    </dialog>"#,
        close = icon("close"),
        action = html(&form.action),
        operation_id = form.operation_id,
        name = html(&form.title),
    )
}

pub(crate) fn section_retry(form: &SectionForm) -> String {
    page(
        "Новый раздел",
        &creation_dialog(form, None),
        PageAssets::Workspace {
            menu: false,
            form: true,
        },
        None,
    )
}
