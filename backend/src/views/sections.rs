use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
};

use crate::{
    db::{Page, Section, Space},
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

struct DocumentTree<'a> {
    ctx: &'a WorkspaceContext,
    space: &'a Space,
    sections: HashMap<Option<uuid::Uuid>, Vec<&'a Section>>,
    pages: HashMap<Option<uuid::Uuid>, Vec<&'a Page>>,
    roots: HashMap<uuid::Uuid, Vec<&'a Page>>,
    selected_section: Option<uuid::Uuid>,
    selected_page: Option<uuid::Uuid>,
    expanded: HashSet<uuid::Uuid>,
}

fn tree_branch<T>(
    path: Vec<T>,
    children: &mut HashSet<uuid::Uuid>,
    item: impl Fn(T) -> uuid::Uuid,
) {
    children.extend(path.into_iter().map(item));
}

pub(super) fn section_trail<'a>(tree: &'a SectionScreen, current: &'a Section) -> Vec<&'a Section> {
    ancestry(&tree.sections, current.parent_id, |item| {
        (item.id, item.parent_id)
    })
}

pub(super) fn page_trail<'a>(tree: &'a SectionScreen, current: &'a Page) -> Vec<&'a Page> {
    ancestry(&tree.pages, current.parent_id, |item| {
        (item.id, item.parent_id)
    })
}

fn ancestry<T>(
    items: &[T],
    mut parent: Option<uuid::Uuid>,
    identity: impl Fn(&T) -> (uuid::Uuid, Option<uuid::Uuid>),
) -> Vec<&T> {
    let ids: HashMap<_, _> = items.iter().map(|item| (identity(item).0, item)).collect();
    let mut seen = HashSet::new();
    let mut trail = Vec::new();
    while let Some(id) = parent.filter(|id| seen.insert(*id)) {
        let Some(item) = ids.get(&id) else { break };
        trail.push(*item);
        parent = identity(item).1;
    }
    trail.reverse();
    trail
}

impl<'a> DocumentTree<'a> {
    fn new(ctx: &'a WorkspaceContext, space: &'a Space, tree: &'a SectionScreen) -> Self {
        let mut group = Self {
            ctx,
            space,
            sections: HashMap::new(),
            pages: HashMap::new(),
            roots: HashMap::new(),
            selected_section: tree.selected.as_ref().map(|item| item.id),
            selected_page: tree.selected_page.as_ref().map(|item| item.id),
            expanded: HashSet::new(),
        };
        for item in &tree.sections {
            group.sections.entry(item.parent_id).or_default().push(item);
        }
        for item in &tree.pages {
            match item.parent_id {
                Some(parent) => group.pages.entry(Some(parent)).or_default().push(item),
                None => group.roots.entry(item.section_id).or_default().push(item),
            }
        }
        if let Some(section) = tree.selected.as_ref() {
            tree_branch(section_trail(tree, section), &mut group.expanded, |item| {
                item.id
            });
            group.expanded.insert(section.id);
        }
        if let Some(page) = tree.selected_page.as_ref() {
            tree_branch(page_trail(tree, page), &mut group.expanded, |item| item.id);
            group.expanded.insert(page.id);
        }
        group
    }

    fn section(&self, item: &Section, rows: &mut String) {
        let href = with_options(
            &section_path(self.ctx.tenant.id, self.space.id, item.id),
            &self.ctx.catalog.options,
        );
        let current = if Some(item.id) == self.selected_section && self.selected_page.is_none() {
            r#" aria-current="page""#
        } else {
            ""
        };
        let link = format!(
            r#"<a data-section-id="{}" href="{}"{current}>{}<span>{}</span></a>"#,
            item.id,
            html(&href),
            icon("folder"),
            html(&item.title)
        );
        let sub = self.sections.get(&Some(item.id));
        let pages = self.roots.get(&item.id);
        rows.push_str("<li>");
        rows.push_str(&link);
        if sub.is_some() || pages.is_some() {
            self.open_children(rows, item.id, &item.title);
            if let Some(children) = sub {
                for child in children {
                    self.section(child, rows);
                }
            }
            if let Some(children) = pages {
                for child in children {
                    self.page(child, rows);
                }
            }
            rows.push_str("</ul></details>");
        }
        rows.push_str("</li>");
    }

    fn page(&self, item: &Page, rows: &mut String) {
        rows.push_str("<li>");
        rows.push_str(&page_link(
            self.ctx,
            self.space,
            item,
            Some(item.id) == self.selected_page,
            "",
        ));
        if let Some(children) = self.pages.get(&Some(item.id)) {
            self.open_children(rows, item.id, &item.title);
            for child in children {
                self.page(child, rows);
            }
            rows.push_str("</ul></details>");
        }
        rows.push_str("</li>");
    }

    fn open_children(&self, rows: &mut String, id: uuid::Uuid, title: &str) {
        let open = if self.expanded.contains(&id) {
            " open"
        } else {
            ""
        };
        let _ = write!(
            rows,
            r#"<details{open}><summary aria-label="Вложенные материалы: {}">Вложенные материалы</summary><ul class="section-pages">"#,
            html(title)
        );
    }
}

pub(super) fn tree_sidebar(ctx: &WorkspaceContext, space: &Space, tree: &SectionScreen) -> String {
    let options = &ctx.catalog.options;
    let selected = tree.selected.as_ref().map(|section| section.id);
    let folder = icon("folder");
    let group = DocumentTree::new(ctx, space, tree);
    let rows = match group.sections.get(&None) {
        None => "<p>Пока нет разделов</p>".to_owned(),
        Some(roots) => {
            let mut rows = String::from(r#"<ul class="section-list">"#);
            for item in roots {
                group.section(item, &mut rows);
            }
            rows.push_str("</ul>");
            rows
        }
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
    let base = crate::workspace::space_path(ctx.tenant.id, space.id);
    let settings = if space.is_owner || ctx.tenant.role != "member" {
        format!(
            r#"<a href="{base}/access">{}Настройки пространства</a>"#,
            crate::workspace_chrome::icon("settings")
        )
    } else {
        String::new()
    };
    format!(
        r#"<aside class="documents-tree" aria-label="Дерево пространства">
      <span class="documents-tree-title">{folder}{name}</span>
      <a href="{base}/search">{search}Найти в пространстве</a>
      <a href="/workspace/{tenant}/organization">{home}Главная</a>
      <details open><summary>Документы</summary>{rows}</details>
      {create}
      <div class="document-tree-footer"><a href="/workspace/profile">{profile}Учётная запись</a>{settings}</div>
    </aside>"#,
        name = html(&space.name),
        search = icon("search"),
        home = icon("home"),
        tenant = ctx.tenant.id,
        profile = crate::workspace_chrome::icon("user"),
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
                "От 1 до 100 символов. Названия разделов на одном уровне должны отличаться.",
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

fn template_picker(selected: &crate::page_templates::PageTemplate) -> String {
    use crate::page_templates::{EMPTY, TEMPLATES};

    let mut picker = String::from(r#"<fieldset class="page-template-picker"><legend>Начать с шаблона</legend>"#);
    for template in std::iter::once(&EMPTY).chain(TEMPLATES) {
        let checked = if selected.key == template.key { " checked" } else { "" };
        let _ = write!(picker, r#"<div class="page-template-option"><label><input type="radio" name="template" value="{}"{checked}><span>{}</span></label>"#, html(template.key), html(template.title));
        if !template.source.is_empty() {
            let _ = write!(picker, r#"<pre class="page-template-preview" aria-label="Содержимое шаблона" tabindex="0">{}</pre>"#, html(template.source));
        }
        picker.push_str("</div>");
    }
    picker.push_str("</fieldset>");
    picker
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
        {templates}
        <div class="space-dialog-actions">
          <a class="space-action space-secondary" href="{cancel}" data-{kind}-cancel>Отмена</a>
          <button class="primary-action space-action" type="submit">Создать</button>
        </div>
      </form>
      <p class="sr-only" role="status" data-{kind}-status></p>
    </dialog>"#,
        templates = form.template.map(template_picker).unwrap_or_default(),
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
