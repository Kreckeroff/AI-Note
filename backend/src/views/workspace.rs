use std::fmt::Write;

use uuid::Uuid;

use crate::db::{Space, Tenant};
use crate::sections::SectionScreen;
use crate::workspace::{
    CatalogOptions, Scope, WORKSPACE_PATH, WorkspaceContext, new_page_path, new_space_path,
    section_path, space_path, spaces_path, with_options,
};

use super::{ClientPage, NORTH_ICON, PageAssets, html, icon, page, sections};
use sections::DocumentKind;

fn is_admin(role: &str) -> bool {
    matches!(role, "admin" | "superadmin")
}

fn access_label(space: &Space) -> &'static str {
    if space.is_owner {
        "Владелец"
    } else if space.grant_role.as_deref() == Some("editor") {
        "Редактирование"
    } else if is_admin(&space.tenant_role) {
        "Просмотр по роли администратора"
    } else {
        "Просмотр"
    }
}

fn tenant_control(tenant: &Tenant, tenants: &[Tenant]) -> String {
    let name = html(&tenant.name);
    let label = format!(r#"{NORTH_ICON}<span>{name}</span>"#);
    if tenants.len() < 2 {
        return format!(r#"<span class="dock-tenant">{label}</span>"#);
    }
    let mut rows = String::new();
    for item in tenants {
        let name = html(&item.name);
        if item.id == tenant.id {
            write!(rows, r#"<li><span class="choice-current" aria-current="true">{name} <small>Текущая среда</small></span></li>"#).unwrap();
        } else {
            write!(
                rows,
                r#"<li><a href="{href}">{name}{next}</a></li>"#,
                href = html(&spaces_path(item.id)),
                next = icon("next"),
            )
            .unwrap();
        }
    }
    format!(
        r#"<details class="dock-choice" data-workspace-choice>
    <summary class="dock-tenant" aria-label="Сменить среду. Текущая: {name}">{label}{down}</summary>
    <div class="dock-panel"><strong>Выбрать среду</strong><ul>{rows}</ul></div>
  </details>"#,
        down = icon("down")
    )
}

fn space_control(ctx: &WorkspaceContext, space: Option<&Space>) -> String {
    let options = &ctx.catalog.options;
    let label = html(space.map_or("Выбрать пространство", |space| {
        &space.name
    }));
    let accessible_label = if space.is_some() {
        format!("Сменить пространство. Текущее: {label}")
    } else {
        label.clone()
    };
    let catalog_href = html(&with_options(&spaces_path(ctx.tenant.id), options));
    let folder = icon("folder");
    let next = icon("next");
    let down = icon("down");
    let layers = icon("layers");
    let group = |title, is_owner| {
        let mut links = String::new();
        for row in ctx
            .available_spaces
            .iter()
            .filter(|row| row.is_owner == is_owner)
        {
            let href = html(&with_options(&space_path(ctx.tenant.id, row.id), options));
            let current = if space.is_some_and(|selected| selected.id == row.id) {
                r#" aria-current="page""#
            } else {
                ""
            };
            write!(links, r#"<li><a data-picker-space="{id}" href="{href}"{current}>
      {folder}<span><strong data-picker-label>{name}</strong><small>{owner_name}</small></span>{next}
    </a></li>"#, id = row.id, name = html(&row.name), owner_name = html(&row.owner_name)).unwrap();
        }
        if links.is_empty() {
            links
        } else {
            format!(r#"<li data-picker-group><h3>{title}</h3><ul>{links}</ul></li>"#)
        }
    };
    let (search, empty_attrs, empty_text) = if ctx.available_spaces.is_empty() {
        ("", "", "Доступных пространств пока нет")
    } else {
        (
            r#"<div class="picker-search" data-picker-search hidden><label for="picker-query" class="sr-only">Поиск пространства в меню</label><input id="picker-query" type="search" placeholder="Найти пространство" maxlength="200"></div>"#,
            "hidden",
            "Пространства не найдены",
        )
    };
    format!(
        r#"<details class="dock-choice" data-workspace-choice>
    <summary class="dock-space" aria-label="{accessible_label}">{folder}<span>{label}</span>{down}</summary>
    <div class="dock-panel dock-space-panel">{search}
      <ul class="picker-spaces">{mine}{available}</ul>
      <p class="picker-no-results" role="status" data-picker-empty {empty_attrs}>{empty_text}</p>
      <a class="picker-all" href="{catalog_href}">{layers}Все пространства{next}</a>
    </div>
  </details>"#,
        mine = group("Мои пространства", true),
        available = group("Доступные пространства", false)
    )
}

fn workspace_frame(
    title: &str,
    context: Option<&WorkspaceContext>,
    space: Option<&Space>,
    content: &str,
    dialog: &str,
    model: ClientPage<'_>,
) -> String {
    let (chrome, account) = if let Some(ctx) = context {
        let options = &ctx.catalog.options;
        let catalog_href = html(&with_options(&spaces_path(ctx.tenant.id), options));
        let (catalog_current, documents) = if space.is_some() {
            ("", r#"<span aria-current="page">Документы</span>"#)
        } else {
            (r#" aria-current="page""#, "")
        };
        let chrome = format!(
            r#"<div class="workspace-dock">
    <div class="dock-identity">{tenant_control}<span class="dock-divider">{divider}</span>{space_control}</div>
    <nav class="dock-nav" aria-label="Рабочие разделы"><a href="{catalog_href}"{catalog_current}>{layers}Пространства</a>{documents}</nav>
  </div>"#,
            tenant_control = tenant_control(&ctx.tenant, &ctx.tenants),
            divider = icon("next"),
            space_control = space_control(ctx, space),
            layers = icon("layers")
        );
        let account = if ctx.account_name.is_empty() {
            String::new()
        } else {
            let name = html(&ctx.account_name);
            format!(r#"<span class="space-account" title="{name}">{name}</span>"#)
        };
        (chrome, account)
    } else {
        (
            format!(
                r#"<div class="workspace-dock"><a class="dock-tenant" href="{WORKSPACE_PATH}">{NORTH_ICON}Север</a></div>"#
            ),
            String::new(),
        )
    };
    let shell = if space.is_some() {
        "documents-shell"
    } else {
        "space-shell"
    };
    page(
        title,
        &format!(
            r#"
    <header class="workspace-topbar">{chrome}{account}</header>
    <main class="{shell}">{content}</main>
    {dialog}"#
        ),
        PageAssets::Workspace {
            menu: context.is_some(),
            form: !dialog.is_empty(),
        },
        model,
    )
}

fn catalog_rows(ctx: &WorkspaceContext) -> String {
    let catalog = &ctx.catalog;
    let options = &catalog.options;
    let folder = icon("folder");
    let lock = icon("lock");
    let next = icon("next");
    let mut row_links = String::new();
    for row in &catalog.rows {
        let href = html(&with_options(&space_path(ctx.tenant.id, row.id), options));
        write!(row_links, r#"<li><a class="catalog-row" data-catalog-space="{id}" href="{href}">
      <span class="catalog-name"><span class="catalog-icon">{folder}</span><span><strong>{name}</strong><small>{lock}Закрытое</small></span></span>
      <span class="catalog-owner"><span class="sr-only">Владелец: </span>{owner_name}</span><span class="catalog-access"><span class="sr-only">Мой доступ: </span>{access}</span>{next}
    </a></li>"#, id = row.id, name = html(&row.name), owner_name = html(&row.owner_name), access = access_label(row)).unwrap();
    }
    let mut pagination = String::new();
    if catalog.pages > 1 {
        let base = spaces_path(ctx.tenant.id);
        let page_link = |page, label| {
            let href = html(&with_options(
                &base,
                &CatalogOptions {
                    q: options.q.clone(),
                    scope: options.scope,
                    page,
                },
            ));
            format!(
                r#"<a class="space-action space-secondary" href="{href}" data-catalog-nav>{label}</a>"#
            )
        };
        let previous = if options.page > 1 {
            page_link(options.page - 1, "Назад")
        } else {
            String::new()
        };
        let next = if options.page < catalog.pages {
            page_link(options.page + 1, "Дальше")
        } else {
            String::new()
        };
        pagination = format!(
            r#"<nav class="catalog-pagination" aria-label="Страницы каталога"><span>{start}–{end} из {total}</span><div>
      {previous}<span>{page} из {pages}</span>{next}
    </div></nav>"#,
            start = catalog.start,
            end = catalog.end,
            total = catalog.total,
            page = options.page,
            pages = catalog.pages
        );
    }
    format!(
        r#"<div class="catalog-table" aria-label="Список пространств">
    <div class="catalog-row catalog-columns" aria-hidden="true"><span>Название</span><span>Владелец</span><span>Мой доступ</span><span></span></div>
    <ul>{row_links}</ul>
    {pagination}
  </div>"#
    )
}

fn catalog_content(ctx: &WorkspaceContext) -> String {
    let options = &ctx.catalog.options;
    let path = spaces_path(ctx.tenant.id);
    let all = if is_admin(&ctx.tenant.role) {
        "Все пространства"
    } else {
        "Все доступные"
    };
    let mut filter_links = String::new();
    for (scope, label) in [
        (Scope::All, all),
        (Scope::Mine, "Мои"),
        (Scope::Membership, "По членству"),
    ] {
        let href = html(&with_options(
            &path,
            &CatalogOptions {
                q: options.q.clone(),
                scope,
                page: 1,
            },
        ));
        let current = if options.scope == scope {
            r#" aria-current="true""#
        } else {
            ""
        };
        write!(
            filter_links,
            r#"<a href="{href}"{current} data-catalog-nav>{label}</a>"#
        )
        .unwrap();
    }
    let path = html(&path);
    let results = if ctx.catalog.total > 0 {
        catalog_rows(ctx)
    } else {
        let filtered = !options.q.is_empty() || options.scope != Scope::All;
        let (glyph, heading, reset) = if filtered {
            (
                "search",
                "Пространства не найдены",
                format!(
                    r#"<a class="space-action space-secondary" href="{path}" data-catalog-nav>Сбросить поиск и фильтр</a>"#
                ),
            )
        } else {
            ("layers", "Здесь пока нет пространств", String::new())
        };
        format!(
            r#"<section class="space-empty catalog-empty" aria-label="Результат поиска"><span class="space-icon">{glyph}</span><h2>{heading}</h2>
      {reset}</section>"#,
            glyph = icon(glyph)
        )
    };
    let create = if ctx.tenant.can_create {
        let href = html(&with_options(&new_space_path(ctx.tenant.id), options));
        format!(
            r#"<a class="primary-action space-action" id="create-space" href="{href}">{}Создать пространство</a>"#,
            icon("add")
        )
    } else {
        String::new()
    };
    let scope_field = if options.scope == Scope::All {
        String::new()
    } else {
        format!(
            r#"<input type="hidden" name="scope" value="{}">"#,
            options.scope.as_str()
        )
    };
    format!(
        r#"<section class="catalog" aria-labelledby="page-title" data-catalog>
    <div class="space-overview"><div><h1 id="page-title">Пространства</h1><p class="space-summary">{tenant_name}</p></div>
      {create}</div>
    <div class="catalog-tools"><form method="get" action="{path}" data-catalog-search>
      <label class="sr-only" for="catalog-query">Поиск по названию пространства</label>
      <input id="catalog-query" type="search" name="q" value="{query}" placeholder="Поиск по названию" maxlength="200">
      {scope_field}
      <button class="space-action space-secondary" type="submit" aria-label="Искать">{search}</button>
    </form><nav class="catalog-filters" aria-label="Фильтр пространств">{filter_links}</nav></div>
    <p class="catalog-count" role="status" data-catalog-count>Найдено: {total}</p>{results}</section>"#,
        tenant_name = html(&ctx.tenant.name),
        query = html(&options.q),
        search = icon("search"),
        total = ctx.catalog.total
    )
}

pub(crate) fn catalog(ctx: &WorkspaceContext) -> String {
    workspace_frame(
        "Пространства",
        Some(ctx),
        None,
        &catalog_content(ctx),
        "",
        ClientPage::Catalog { workspace: ctx },
    )
}

pub(crate) fn catalog_unavailable(retry_to: &str) -> String {
    workspace_unavailable(
        "Пространства",
        "Не удалось загрузить пространства",
        retry_to,
        ClientPage::CatalogUnavailable { retry_to },
    )
}

pub(crate) fn documents_unavailable(retry_to: &str) -> String {
    workspace_unavailable(
        "Документы",
        "Не удалось открыть документы",
        retry_to,
        ClientPage::DocumentsUnavailable { retry_to },
    )
}

fn workspace_unavailable(
    title: &str,
    heading: &str,
    retry_to: &str,
    client: ClientPage<'_>,
) -> String {
    workspace_frame(
        title,
        None,
        None,
        &format!(
            r#"<section class="space-empty"><h1>{}</h1><a class="space-action space-secondary" href="{}">Повторить попытку</a></section>"#,
            html(heading),
            html(retry_to),
        ),
        "",
        client,
    )
}

pub(crate) fn closed_space(ctx: &WorkspaceContext, space: &Space, tree: &SectionScreen) -> String {
    let options = &ctx.catalog.options;
    let catalog_href = with_options(&spaces_path(ctx.tenant.id), options);
    let name = html(&space.name);
    let next = icon("next");
    let (title, kind, crumb, contents) = match (&tree.selected, &tree.selected_page) {
        (Some(section), Some(page)) => {
            let space_href = html(&with_options(&space_path(ctx.tenant.id, space.id), options));
            let section_href = html(&with_options(
                &section_path(ctx.tenant.id, space.id, section.id),
                options,
            ));
            (
                page.title.as_str(),
                "file",
                format!(
                    r#"<a href="{space_href}">{name}</a>{next}<a href="{section_href}">{section}</a>{next}<span aria-current="page">{page}</span>"#,
                    section = html(&section.title),
                    page = html(&page.title),
                ),
                super::editor::page_contents(tree),
            )
        }
        (Some(section), None) => {
            let space_href = html(&with_options(&space_path(ctx.tenant.id, space.id), options));
            (
                section.title.as_str(),
                "folder",
                format!(
                    r#"<a href="{space_href}">{name}</a>{next}<span aria-current="page">{}</span>"#,
                    html(&section.title),
                ),
                section_contents(ctx, space, tree, section.id),
            )
        }
        (None, _) => {
            let message = if tree.sections.is_empty() {
                "В пространстве пока нет разделов"
            } else if tree.pages.is_empty() {
                "Нет доступных страниц"
            } else {
                "Выберите раздел в дереве"
            };
            (
                space.name.as_str(),
                "folder",
                format!("<span>{name}</span>"),
                empty_documents(message),
            )
        }
    };
    let content = format!(
        r#"
    {sidebar}
    <div class="documents-content">
      <nav class="documents-path" aria-label="Путь"><a href="{catalog_href}">Пространства</a>{next}{crumb}</nav>
      <article class="documents-article">
        <header class="space-heading"><span class="space-icon">{document_icon}</span>
          <div><h1>{title}</h1><div class="space-badges"><span>{lock}Закрытое</span><span>{access}</span></div></div>
        </header>
        {contents}
      </article>
    </div>"#,
        sidebar = sections::tree_sidebar(ctx, space, tree, &catalog_href),
        catalog_href = html(&catalog_href),
        title = html(title),
        document_icon = icon(kind),
        lock = icon("lock"),
        access = access_label(space),
    );
    let dialog = if let Some(form) = tree.page_form.as_ref() {
        sections::creation_dialog(
            form,
            tree.selected.as_ref().map(|section| section.title.as_str()),
            DocumentKind::Page,
        )
    } else {
        tree.form.as_ref().map_or_else(String::new, |form| {
            sections::creation_dialog(form, Some(&space.name), DocumentKind::Section)
        })
    };
    workspace_frame(
        title,
        Some(ctx),
        Some(space),
        &content,
        &dialog,
        ClientPage::ClosedSpace {
            workspace: ctx,
            space,
            tree,
        },
    )
}

fn empty_documents(message: &str) -> String {
    format!(
        r#"<section class="space-empty"><span class="space-icon">{}</span><h2>{}</h2></section>"#,
        icon("layers"),
        html(message),
    )
}

fn section_contents(
    ctx: &WorkspaceContext,
    space: &Space,
    tree: &SectionScreen,
    section: Uuid,
) -> String {
    let pages: String = tree
        .pages
        .iter()
        .filter(|page| page.section_id == section)
        .map(|page| {
            format!(
                "<li>{}</li>",
                sections::page_link(ctx, space, page, false, "space-action space-secondary")
            )
        })
        .collect();
    let mut content = if pages.is_empty() {
        empty_documents("Этот раздел пока пуст")
    } else {
        format!(r#"<ul class="section-pages-list">{pages}</ul>"#)
    };
    if tree.can_create_section {
        let form_href = with_options(
            &new_page_path(ctx.tenant.id, space.id, section),
            &ctx.catalog.options,
        );
        write!(
            content,
            r#"<a id="create-page" class="space-action primary-action page-create-link" href="{}">{}Добавить страницу</a>"#,
            html(&form_href),
            icon("add"),
        )
        .unwrap();
    }
    content
}

pub(crate) fn space_not_found() -> String {
    not_found("Пространство не найдено", ClientPage::SpaceNotFound)
}

pub(crate) fn section_not_found() -> String {
    not_found("Раздел не найден", ClientPage::SectionNotFound)
}

pub(crate) fn page_not_found() -> String {
    not_found("Страница не найдена", ClientPage::PageNotFound)
}

fn not_found(title: &str, model: ClientPage<'_>) -> String {
    workspace_frame(
        title,
        None,
        None,
        &format!(
            r#"<section class="space-denied"><span class="space-icon">{lock}</span>
      <h1>{title}</h1><p class="space-summary">Проверьте адрес или вернитесь в рабочую область.</p>
      <a class="space-action space-secondary" href="{WORKSPACE_PATH}">{back}К рабочей области</a>
    </section>"#,
            title = html(title),
            lock = icon("lock"),
            back = icon("back")
        ),
        "",
        model,
    )
}

pub(crate) fn create_space(
    ctx: &WorkspaceContext,
    operation_id: Uuid,
    name: &str,
    error: &str,
    invalid_name: bool,
    new_form: bool,
) -> String {
    let options = &ctx.catalog.options;
    let catalog_href = html(&with_options(&spaces_path(ctx.tenant.id), options));
    let (error_ref, message) = if error.is_empty() {
        ("", String::new())
    } else {
        let extra = if new_form {
            let href = html(&with_options(&new_space_path(ctx.tenant.id), options));
            format!(r#" <a href="{href}">Открыть новую форму</a>"#)
        } else {
            String::new()
        };
        (
            " space-error",
            format!(
                r#"<p class="form-error" id="space-error" role="alert">{}{extra}</p>"#,
                html(error)
            ),
        )
    };
    let invalid = if invalid_name {
        r#" aria-invalid="true""#
    } else {
        ""
    };
    let dialog = format!(
        r#"
    <dialog id="space-dialog" class="space-dialog" aria-labelledby="space-dialog-title" open>
      <header class="space-dialog-header">
        <div><h2 id="space-dialog-title">Новое пространство</h2><p class="space-dialog-tenant">{home}{tenant_name}</p></div>
        <a class="space-action space-close" href="{catalog_href}#create-space" data-space-cancel aria-label="Закрыть">{close}</a>
      </header>
      <form class="space-form" method="post" action="{catalog_href}" novalidate>
        <input type="hidden" name="operation_id" value="{operation_id}">
        <label for="space-name">Название</label>
        <input id="space-name" name="name" value="{name}" placeholder="Например, Команда продукта" autofocus required
          aria-describedby="space-help{error_ref}"{invalid}>
        <p class="space-hint" id="space-help">От 1 до 100 символов. Выберите уникальное имя среди своих пространств в этой среде.</p>
        <p class="form-error" id="space-client-error" role="alert" hidden>Введите не больше 100 символов.</p>
        {message}
        <div class="space-note">{lock}<p><strong>Закрытое пространство</strong><br>Вы станете владельцем. Администраторы этой среды смогут просматривать содержимое.</p></div>
        <div class="space-dialog-actions">
          <a class="space-action space-secondary" href="{catalog_href}#create-space" data-space-cancel>Отмена</a>
          <button class="primary-action space-action" type="submit">Создать</button>
        </div>
      </form>
      <p class="sr-only" role="status" data-space-status></p>
    </dialog>"#,
        home = icon("home"),
        tenant_name = html(&ctx.tenant.name),
        close = icon("close"),
        name = html(name),
        lock = icon("lock")
    );
    workspace_frame(
        "Новое пространство",
        Some(ctx),
        None,
        &catalog_content(ctx),
        &dialog,
        ClientPage::CreateSpace {
            workspace: ctx,
            operation_id,
            name,
            error,
            invalid_name,
            new_form,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::{Section, Tenant},
        sections::{NameForm, SectionScreen},
        workspace::{Catalog, CatalogOptions, Scope},
    };

    #[test]
    fn section_screen_preserves_both_navigation_and_invalid_form_input() {
        let tenant_id = Uuid::from_u128(1);
        let space_id = Uuid::from_u128(2);
        let section_id = Uuid::from_u128(3);
        let operation_id = Uuid::from_u128(4);
        let options = CatalogOptions {
            q: "alpha".to_owned(),
            scope: Scope::Mine,
            page: 2,
        };
        let space = Space {
            id: space_id,
            name: "Команда".to_owned(),
            owner_name: "Владелец".to_owned(),
            is_owner: true,
            tenant_role: "member".to_owned(),
            grant_role: None,
        };
        let workspace = WorkspaceContext {
            tenant: Tenant {
                id: tenant_id,
                name: "Север".to_owned(),
                role: "member".to_owned(),
                can_create: true,
            },
            account_name: space.owner_name.clone(),
            tenants: vec![],
            available_spaces: vec![space.clone()],
            catalog: Catalog {
                options,
                rows: vec![space.clone()],
                total: 1,
                pages: 1,
                start: 1,
                end: 1,
            },
        };
        let selected = Section {
            id: section_id,
            title: "Решения".to_owned(),
        };
        let section_href = format!(
            "/workspace/{tenant_id}/spaces/{space_id}/sections/{section_id}?q=alpha&scope=mine&page=2"
        );
        let cancel_to = format!("{section_href}#create-section");
        let mut tree = SectionScreen {
            sections: vec![Section {
                id: section_id,
                title: selected.title.clone(),
            }],
            pages: Vec::new(),
            selected: Some(selected),
            selected_page: None,
            can_create_section: false,
            form: None,
            page_form: None,
            reading: None,
            editor: None,
        };
        let reader = closed_space(&workspace, &space, &tree);
        assert!(reader.contains("Этот раздел пока пуст"));
        assert!(reader.contains(&format!(
            r#"href="{}" aria-current="page""#,
            html(&section_href)
        )));
        assert!(!reader.contains(r#"id="create-section""#));

        tree.can_create_section = true;
        tree.form = Some(NameForm {
            operation_id,
            title: "Решения < архив".to_owned(),
            error: "Уже есть раздел с таким названием.".to_owned(),
            invalid_name: true,
            cancel_to: cancel_to.clone(),
            action: format!("/workspace/{tenant_id}/spaces/{space_id}/sections"),
            retry_to: None,
        });
        let editor = closed_space(&workspace, &space, &tree);
        assert!(editor.contains(r#"id="section-dialog""#));
        assert!(editor.contains(r#"name="name" value="Решения &lt; архив""#));
        assert!(
            editor.contains(r#"aria-describedby="section-help section-error" aria-invalid="true""#)
        );
        assert!(editor.contains(&format!(
            r#"href="{}" data-section-cancel"#,
            html(&cancel_to)
        )));
        assert!(editor.contains(&format!(r#"name="operation_id" value="{operation_id}""#)));
        assert_eq!(
            editor.contains(r#"src="/space-form.mjs""#),
            super::super::UI_BUNDLE.is_empty()
        );

        let form = tree.form.as_ref().unwrap();
        let model = serde_json::to_value(ClientPage::ClosedSpace {
            workspace: &workspace,
            space: &space,
            tree: &tree,
        })
        .unwrap();
        assert_eq!(
            model["tree"],
            serde_json::json!({
                "sections": [{"id": section_id, "title": "Решения"}],
                "selected": {"id": section_id, "title": "Решения"},
                "canCreateSection": true,
                "canCreatePage": true,
                "pages": [],
                "selectedPage": null,
                "pageForm": null,
                "reading": null,
                "editor": null,
                "form": {
                    "operationId": operation_id,
                    "title": form.title,
                    "error": form.error,
                    "invalidName": true,
                    "cancelTo": cancel_to,
                    "action": form.action,
                    "retryTo": null,
                },
            })
        );

        let retry = sections::section_retry(form);
        assert!(retry.contains(r#"name="name" value="Решения &lt; архив""#));
        assert!(retry.contains(r#"src="/space-form.mjs""#));
        assert!(!retry.contains("sever-ui-data"));
        assert!(!retry.contains("Дерево пространства"));
    }
}
