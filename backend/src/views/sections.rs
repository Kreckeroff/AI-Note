use std::fmt::Write;

use crate::{
    db::{Page, Space},
    sections::{NameForm, SectionScreen},
    workspace::{WorkspaceContext, new_section_path, page_path, section_path, with_options},
};

use super::{PageAssets, html, icon, page};

pub(super) fn page_link(
    ctx: &WorkspaceContext,
    space: &Space,
    page: &Page,
    current: bool,
    class: &str,
) -> String {
    let href = with_options(
        &page_path(ctx.tenant.id, space.id, page.id),
        &ctx.catalog.options,
    );
    let current = if current {
        r#" aria-current="page""#
    } else {
        ""
    };
    format!(
        r#"<a class="{class}" data-page-id="{id}" href="{href}"{current}>{file}<span>{title}</span></a>"#,
        id = page.id,
        href = html(&href),
        file = icon("file"),
        title = html(&page.title),
    )
}

pub(super) fn tree_sidebar(
    ctx: &WorkspaceContext,
    space: &Space,
    tree: &SectionScreen,
    catalog_href: &str,
) -> String {
    let options = &ctx.catalog.options;
    let selected = tree.selected.as_ref().map(|section| section.id);
    let selected_page = tree.selected_page.as_ref().map(|page| page.id);
    let folder = icon("folder");
    let rows = if tree.sections.is_empty() {
        "<p>Пока нет разделов</p>".to_owned()
    } else {
        let mut rows = String::from(r#"<ul class="section-list">"#);
        for section in &tree.sections {
            let href = with_options(&section_path(ctx.tenant.id, space.id, section.id), options);
            let expanded = Some(section.id) == selected;
            let current = if expanded && selected_page.is_none() {
                r#" aria-current="page""#
            } else {
                ""
            };
            write!(
                rows,
                r#"<li><a data-section-id="{id}" href="{href}"{current}>{folder}<span>{title}</span></a>"#,
                id = section.id,
                href = html(&href),
                title = html(&section.title),
            )
            .unwrap();
            if expanded {
                let pages = tree
                    .pages
                    .iter()
                    .filter(|page| page.section_id == section.id);
                let links: String = pages
                    .map(|page| {
                        format!(
                            "<li>{}</li>",
                            page_link(ctx, space, page, Some(page.id) == selected_page, ""),
                        )
                    })
                    .collect();
                if !links.is_empty() {
                    write!(rows, r#"<ul class="section-pages">{links}</ul>"#).unwrap();
                }
            }
            rows.push_str("</li>");
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

pub(super) enum DocumentKind {
    Section,
    Page,
}

impl DocumentKind {
    fn dialog_labels(&self) -> (&'static str, &'static str, &'static str, &'static str) {
        match self {
            Self::Section => (
                "section",
                "Новый раздел",
                "Например, Документация",
                "От 1 до 100 символов. Название должно быть уникальным в пространстве.",
            ),
            Self::Page => (
                "page",
                "Новая страница",
                "Например, Решение по доступу",
                "От 1 до 100 символов. Название должно быть уникальным в разделе.",
            ),
        }
    }
}

pub(super) fn creation_dialog(
    form: &NameForm,
    parent: Option<&str>,
    document: DocumentKind,
) -> String {
    let (kind, heading, placeholder, hint) = document.dialog_labels();
    let cancel = html(&form.cancel_to);
    let (error_ref, error) = if form.error.is_empty() {
        (String::new(), String::new())
    } else {
        let retry = form.retry_to.as_ref().map_or_else(String::new, |href| {
            format!(r#" <a href="{}">Открыть новую форму</a>"#, html(href))
        });
        (
            format!(" {kind}-error"),
            format!(
                r#"<p class="form-error" id="{kind}-error" role="alert">{}{retry}</p>"#,
                html(&form.error),
            ),
        )
    };
    let invalid = if form.invalid_name {
        r#" aria-invalid="true""#
    } else {
        ""
    };
    let location = parent.map_or_else(String::new, |name| {
        format!(
            r#"<p class="space-dialog-tenant">{}{}</p>"#,
            icon("folder"),
            html(name),
        )
    });
    format!(
        r#"<dialog id="{kind}-dialog" class="space-dialog section-dialog" aria-labelledby="{kind}-dialog-title" open>
      <header class="space-dialog-header">
        <div><h2 id="{kind}-dialog-title">{heading}</h2>{location}</div>
        <a class="space-action space-close" href="{cancel}" data-{kind}-cancel aria-label="Закрыть">{close}</a>
      </header>
      <form class="space-form" method="post" action="{action}" novalidate>
        <input type="hidden" name="operation_id" value="{operation_id}">
        <label for="{kind}-name">Название</label>
        <input id="{kind}-name" name="name" value="{name}" placeholder="{placeholder}" autofocus required
          aria-describedby="{kind}-help{error_ref}"{invalid}>
        <p class="space-hint" id="{kind}-help">{hint}</p>
        <p class="form-error" id="{kind}-client-error" role="alert" hidden></p>
        {error}
        <div class="space-dialog-actions">
          <a class="space-action space-secondary" href="{cancel}" data-{kind}-cancel>Отмена</a>
          <button class="primary-action space-action" type="submit">Создать</button>
        </div>
      </form>
      <p class="sr-only" role="status" data-{kind}-status></p>
    </dialog>"#,
        close = icon("close"),
        action = html(&form.action),
        operation_id = form.operation_id,
        name = html(&form.title),
    )
}

fn retry(form: &NameForm, kind: DocumentKind) -> String {
    page(
        kind.dialog_labels().1,
        &creation_dialog(form, None, kind),
        PageAssets::Workspace {
            menu: false,
            form: true,
        },
        None,
    )
}

pub(crate) fn section_retry(form: &NameForm) -> String {
    retry(form, DocumentKind::Section)
}

pub(crate) fn page_retry(form: &NameForm) -> String {
    retry(form, DocumentKind::Page)
}
