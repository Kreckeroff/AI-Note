use std::{fmt::Write as _, io::Cursor};

use image::{ImageDecoder, ImageFormat, ImageReader, Limits, codecs::jpeg::JpegEncoder};
use tokio::sync::Semaphore;
use uuid::Uuid;

use super::{Panel, organization_logo_path, organization_path, panel_path};
use crate::{
    clock::now_ms,
    db::{
        Space,
        organization::{
            Invitation, OrganizationGroup, OrganizationMember, OrganizationSettings, SpaceGrant,
            SpaceRule, admin,
        },
    },
    http::Form,
    text::escape_html as esc,
    workspace::{WorkspaceContext, new_space_path, space_path, spaces_path},
    workspace_chrome::{icon, page},
};

const PAGE_SIZE: usize = 20;
static LOGO_DECODERS: Semaphore = Semaphore::const_new(2);

pub(super) struct ListFilter {
    pub query: String,
    pub role: String,
    pub department: String,
    pub source: String,
    pub person: String,
    page: i64,
}

impl ListFilter {
    pub fn new(query: &Form) -> Self {
        fn value(form: &Form, field: &str) -> String {
            form.get(field)
                .trim()
                .chars()
                .filter(|ch| !ch.is_control())
                .take(120)
                .collect()
        }
        Self {
            query: value(query, "q"),
            role: value(query, "role"),
            department: value(query, "department"),
            source: value(query, "source"),
            person: value(query, "person"),
            page: query
                .get("page")
                .parse::<i64>()
                .unwrap_or(1)
                .clamp(1, 10000),
        }
    }

    pub fn offset(&self) -> i64 {
        (self.page - 1) * PAGE_SIZE as i64
    }

    fn path(&self, base: &str, page: i64) -> String {
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        for (name, value) in [
            ("q", &self.query),
            ("role", &self.role),
            ("department", &self.department),
            ("source", &self.source),
            ("person", &self.person),
        ] {
            if !value.is_empty() {
                query.append_pair(name, value);
            }
        }
        query.append_pair("page", &page.to_string());
        format!("{base}?{}", query.finish())
    }

    fn navigation(&self, path: &str, count: usize) -> String {
        let mut controls = String::new();
        if self.page > 1 {
            let _ = write!(
                controls,
                r#"<a class="space-action space-secondary" href="{}">Назад</a>"#,
                esc(&self.path(path, self.page - 1))
            );
        }
        if count > PAGE_SIZE {
            let _ = write!(
                controls,
                r#"<a class="space-action space-secondary" href="{}">Далее</a>"#,
                esc(&self.path(path, self.page + 1))
            );
        }
        if controls.is_empty() {
            return controls;
        }
        format!(
            r#"<nav class="org-pagination" aria-label="Страницы результатов"><span>Страница {}</span>{controls}</nav>"#,
            self.page
        )
    }
}

pub(super) async fn normalize_logo(bytes: Vec<u8>) -> Result<Vec<u8>, &'static str> {
    let permit = LOGO_DECODERS
        .try_acquire()
        .map_err(|_| "Не удалось обработать файл. Повторите попытку.")?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        decode_logo(&bytes).ok_or("Выберите корректное изображение PNG, JPEG или WebP.")
    })
    .await
    .map_err(|_| "Не удалось обработать файл. Повторите попытку.")?
}

fn decode_logo(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    if !matches!(
        reader.format(),
        Some(ImageFormat::Png | ImageFormat::Jpeg | ImageFormat::WebP)
    ) {
        return None;
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(4096);
    limits.max_image_height = Some(4096);
    limits.max_alloc = Some(64 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().ok()?;
    let (width, height) = decoder.dimensions();
    if width == 0
        || height == 0
        || u64::from(width) * u64::from(height) > 8_000_000
        || decoder.total_bytes() > 64 * 1024 * 1024
    {
        return None;
    }
    let orientation = decoder.orientation().ok()?;
    let mut image = image::DynamicImage::from_decoder(decoder).ok()?;
    image.apply_orientation(orientation);
    let resized = image.thumbnail(512, 512).into_rgb8();
    let mut result = Vec::new();
    JpegEncoder::new_with_quality(&mut result, 88)
        .encode_image(&resized)
        .ok()?;
    Some(result)
}

fn organization_page(
    title: &str,
    ctx: &WorkspaceContext,
    current: Panel,
    space: Option<&Space>,
    body: &str,
) -> String {
    let tenant = &ctx.tenant;
    let mut choices = String::new();
    if ctx.tenants.len() > 1 {
        for candidate in &ctx.tenants {
            let selected = if candidate.id == tenant.id {
                " aria-current=\"true\""
            } else {
                ""
            };
            let _ = write!(
                choices,
                r#"<form method="post" action="{}"><button class="space-action space-secondary"{selected}>{} {}</button></form>"#,
                panel_path(candidate.id, Panel::Switch),
                brand(candidate.id),
                esc(&candidate.name)
            );
        }
        choices = format!(
            r#"<details class="org-tenant-switch"><summary>{} Сменить организацию</summary><div>{choices}</div></details>"#,
            icon("down")
        );
    }
    let mut navigation = String::new();
    if admin(&tenant.role) {
        for (panel, label, glyph) in [
            (Panel::Home, "Главная", "home"),
            (Panel::Settings, "Основные", "settings"),
            (Panel::Members, "Участники", "user"),
            (Panel::Invitations, "Приглашения", "outward"),
            (Panel::Policies, "Политики доступа", "shield"),
            (Panel::Access, "Доступ к пространствам", "layers"),
        ] {
            let selected = if current == panel {
                " aria-current=\"page\""
            } else {
                ""
            };
            let _ = write!(
                navigation,
                r#"<a href="{}"{selected}>{} {label}</a>"#,
                panel_path(tenant.id, panel),
                icon(glyph)
            );
        }
    }
    let markup = format!(
        r#"<div class="org-main" id="main">
        <div class="org-breadcrumbs"><a href="{}">{}</a>{choices}<a class="org-profile-link" href="/workspace/profile">{} {}</a></div>
        <nav class="org-settings-nav" aria-label="Разделы организации">{navigation}</nav>{body}</div>"#,
        organization_path(tenant.id),
        esc(&tenant.name),
        icon("user"),
        esc(&ctx.account_name)
    );
    page(
        title,
        Some((tenant.id, &tenant.name)),
        space.map(|row| (row.id, row.name.as_str())),
        "organization",
        &markup,
    )
}

fn role_name(role: &str) -> &'static str {
    match role {
        "superadmin" => "Суперадминистратор",
        "admin" => "Администратор",
        "owner" => "Владелец",
        "editor" => "Редактор",
        "viewer" => "Читатель",
        _ => "Участник",
    }
}

fn brand(tenant: Uuid) -> String {
    format!(
        r#"<img class="org-picker-logo" src="{}" width="18" height="18" alt="">"#,
        organization_logo_path(tenant)
    )
}

pub(super) fn home(ctx: &WorkspaceContext, settings: &OrganizationSettings) -> String {
    let tenant = ctx.tenant.id;
    let mark = if settings.has_logo {
        format!(r#"<img alt="" src="{}">"#, organization_logo_path(tenant))
    } else {
        icon("north")
    };
    let titles = if ctx.available_spaces.is_empty() {
        r#"<li class="org-empty">Пока нет доступных пространств.</li>"#.to_owned()
    } else {
        ctx.available_spaces.iter().take(6).map(|space|
            format!(r#"<li><a class="org-row" href="{}"><span class="org-row-icon">{}</span><span class="org-row-text"><strong>{}</strong><small>{}</small></span><span>{}</span></a></li>"#,
            space_path(tenant,space.id),icon("folder"),esc(&space.name),
            role_name(if space.is_owner{"owner"}else{space.grant_role.as_deref().unwrap_or("viewer")}),icon("chevron"))
        ).collect()
    };
    let create = if ctx.tenant.can_create {
        format!(
            r#"<a class="space-action primary-action" href="{}">{} Создать пространство</a>"#,
            new_space_path(tenant),
            icon("plus")
        )
    } else {
        String::new()
    };
    let management = if admin(&ctx.tenant.role) {
        [
            (Panel::Settings,"Настройки организации","settings"),
            (Panel::Members,"Участники","user"),
            (Panel::Invitations,"Приглашения","outward"),
            (Panel::Policies,"Политики доступа","shield"),
            (Panel::Access,"Доступ к пространствам","layers"),
        ].iter().map(|(target,label,glyph)|format!(r#"<li><a class="org-row" href="{}"><span class="org-row-icon">{}</span><strong>{label}</strong>{}</a></li>"#,panel_path(tenant,*target),icon(glyph),icon("chevron"))).collect::<String>()
    } else {
        String::new()
    };
    let body = format!(
        r#"
       <section class="org-hero"><div class="org-hero-title"><span class="org-emblem">{mark}</span><div><h1>{}</h1><p>{}</p></div></div>{create}</section>
       <div class="org-grid" data-management="{}">
         <section class="org-panel"><header class="org-panel-header"><h2>Пространства</h2><a href="{}">Все пространства</a></header><ul class="org-rows">{titles}</ul></section>
         {}
       </div>"#,
        esc(&ctx.tenant.name),
        role_name(&ctx.tenant.role),
        admin(&ctx.tenant.role),
        spaces_path(tenant),
        if management.is_empty() {
            String::new()
        } else {
            format!(
                r#"<section class="org-panel"><header class="org-panel-header"><h2>Управление</h2></header><ul class="org-rows">{management}</ul></section>"#
            )
        }
    );
    organization_page(&ctx.tenant.name, ctx, Panel::Home, None, &body)
}

pub(super) fn settings(
    ctx: &WorkspaceContext,
    data: &OrganizationSettings,
    entered: Option<&str>,
    error: Option<&str>,
) -> String {
    let tenant = ctx.tenant.id;
    let title = esc(entered.unwrap_or(&ctx.tenant.name));
    let error = error
        .map(|message| format!(r#"<p role="alert" class="org-error">{}</p>"#, esc(message)))
        .unwrap_or_default();
    let logo = organization_logo_path(tenant);
    let inputs = if ctx.tenant.role == "superadmin" {
        let reset = if data.has_logo {
            format!(
                r#"<form method="post" action="{logo}"><input type="hidden" name="action" value="clear"><button class="space-action space-secondary">Вернуть эмблему Севера</button></form>"#
            )
        } else {
            String::new()
        };
        format!(
            r#"<form method="post" class="org-fields" action="{}"><label>Название<input name="name" value="{title}" maxlength="120" required></label>{error}<button class="space-action primary-action">Сохранить</button></form>
          <div class="org-logo-actions"><form class="org-fields" method="post" data-org-logo action="{logo}"><label>Выбрать логотип<input type="file" name="logo" accept="image/png,image/jpeg,image/webp" required></label>
          <p class="org-muted">PNG, JPEG или WebP до 5 МБ</p><button class="space-action primary-action">Сохранить логотип</button>
          <p role="status" data-org-logo-status></p></form>{reset}</div>"#,
            panel_path(tenant, Panel::Settings)
        )
    } else {
        format!("<p>{title}</p>")
    };
    organization_page(
        "Основные настройки",
        ctx,
        Panel::Settings,
        None,
        &format!(
            r#"
        <section class="org-panel org-panel-pad"><h1>Основные настройки</h1>
        <div class="org-emblem org-logo-preview"><img data-org-logo-preview alt="Логотип организации" src="{logo}"></div>{inputs}</section>"#
        ),
    )
}

pub(super) fn policies(ctx: &WorkspaceContext, data: &OrganizationSettings) -> String {
    let editable = ctx.tenant.role == "superadmin";
    let checked = |yes| if yes { " checked" } else { "" };
    let disabled = if editable { "" } else { " disabled" };
    let submit = if editable {
        r#"<button class="space-action primary-action">Сохранить</button>"#
    } else {
        ""
    };
    organization_page(
        "Политики доступа",
        ctx,
        Panel::Policies,
        None,
        &format!(
            r#"<section class="org-panel org-panel-pad">
        <h1>Политики доступа</h1><form class="org-fields" method="post" action="{}">
        <label class="org-check"><input name="guests" type="checkbox" value="yes"{}{disabled}>Разрешить гостей на страницах</label>
        <label class="org-check"><input name="public" type="checkbox" value="yes"{}{disabled}>Разрешить публичные ссылки</label>{submit}</form>
        </section><section class="org-panel org-panel-pad org-gap"><h2>Корпоративный вход</h2><p>SSO не настроен.</p></section>"#,
            panel_path(ctx.tenant.id, Panel::Policies),
            checked(data.allow_guests),
            checked(data.allow_public)
        ),
    )
}

fn filter_markup(path: &str, filter: &ListFilter, extra: &str) -> String {
    format!(
        r#"<form class="org-filter" method="get" action="{}"><label>Поиск<input name="q" value="{}" maxlength="120" type="search"></label>
        {extra}<button class="space-action space-secondary">Найти</button><a href="{}">Сбросить</a></form>"#,
        esc(path),
        esc(&filter.query),
        esc(path)
    )
}

fn options(current: &str, values: impl IntoIterator<Item = (String, String)>) -> String {
    values
        .into_iter()
        .map(|(value, label)| {
            let selected = if value == current { " selected" } else { "" };
            format!(
                r#"<option value="{}"{selected}>{}</option>"#,
                esc(&value),
                esc(&label)
            )
        })
        .collect()
}

fn simple_options(current: &str, values: &[(&str, &str)]) -> String {
    options(
        current,
        values
            .iter()
            .map(|(value, label)| (value.to_string(), label.to_string())),
    )
}

fn member_boolean_form(
    path: &str,
    account: Uuid,
    action: &str,
    enabled: bool,
    label: &str,
) -> String {
    let value = if enabled { "no" } else { "yes" };
    format!(
        r#"<form method="post" action="{path}"><input type="hidden" name="action" value="{action}"><input type="hidden" name="account" value="{account}"><input type="hidden" name="value" value="{value}"><button class="space-action space-secondary">{}</button></form>"#,
        esc(label)
    )
}

pub(super) fn members(
    ctx: &WorkspaceContext,
    filter: &ListFilter,
    rows: Vec<OrganizationMember>,
    groups: &[OrganizationGroup],
    departments: &[String],
    memberships: &[(Uuid, Uuid)],
) -> String {
    let tenant = ctx.tenant.id;
    let admin_role = ctx.tenant.role == "superadmin";
    let path = panel_path(tenant, Panel::Members);
    let role_opts = simple_options(
        &filter.role,
        &[
            ("", "Все роли"),
            ("superadmin", "Суперадминистраторы"),
            ("admin", "Администраторы"),
            ("member", "Участники"),
        ],
    );
    let department_opts = options(
        &filter.department,
        std::iter::once((String::new(), "Все отделы".to_owned()))
            .chain(departments.iter().map(|name| (name.clone(), name.clone()))),
    );
    let filters = filter_markup(
        &path,
        filter,
        &format!(
            r#"<label>Роль<select name="role">{role_opts}</select></label><label>Отдел<select name="department">{department_opts}</select></label>"#
        ),
    );
    let pagination = filter.navigation(&path, rows.len());
    let mut records = String::new();
    for member in rows.iter().take(PAGE_SIZE) {
        let id = member.id;
        let name = esc(&member.name);
        let department = esc(member.department.as_deref().unwrap_or_default());
        let creation = if member.role == "superadmin" {
            String::new()
        } else {
            member_boolean_form(
                &path,
                id,
                "create",
                member.can_create,
                if member.can_create {
                    "Отозвать право создания"
                } else {
                    "Разрешить создание"
                },
            )
        };
        let administrator = if admin_role && member.role != "superadmin" {
            member_boolean_form(
                &path,
                id,
                "administrator",
                member.role == "admin",
                if member.role == "admin" {
                    "Снять роль администратора"
                } else {
                    "Назначить администратором"
                },
            )
        } else {
            String::new()
        };
        let group_form = if groups.is_empty() {
            String::new()
        } else {
            let assigned: Vec<_> = groups
                .iter()
                .filter(|group| memberships.contains(&(id, group.id)))
                .collect();
            let assigned_labels = assigned
                .iter()
                .map(|group| esc(&group.name))
                .collect::<Vec<_>>()
                .join(", ");
            let group_options = options(
                "",
                groups
                    .iter()
                    .map(|group| (group.id.to_string(), group.name.clone())),
            );
            format!(
                r#"<form class="org-inline" method="post" action="{path}">
                 <input type="hidden" name="action" value="group-member"><input type="hidden" name="account" value="{id}">
                 <label>Группа<select name="group">{group_options}</select></label>
                 <button class="space-action space-secondary" name="value" value="yes">Добавить</button>
                 <button class="space-action space-secondary" name="value" value="no">Исключить</button>
               </form><p class="org-muted">{assigned_labels}</p>"#
            )
        };
        let _ = write!(
            records,
            r#"<li class="org-person"><div class="org-person-summary"><span class="org-row-icon">{}</span><div><strong>{name}</strong><small>{}</small></div><span class="org-badge">{}</span></div>
           <details><summary>Права и группы</summary><div class="org-inline">{creation}{administrator}</div>
            <form class="org-inline" method="post" action="{path}"><input type="hidden" name="action" value="department"><input type="hidden" name="account" value="{id}">
              <label>Отдел<input name="value" value="{department}" maxlength="120"></label><button class="space-action space-secondary">Сохранить отдел</button></form>
            {group_form}
           </details></li>"#,
            icon("user"),
            esc(&member.email),
            role_name(&member.role)
        );
    }
    if records.is_empty() {
        records = "<li class=\"org-empty\">Участники не найдены.</li>".to_owned();
    }
    organization_page(
        "Участники",
        ctx,
        Panel::Members,
        None,
        &format!(
            r#"<section class="org-panel">
        <header class="org-panel-header"><h1>Участники</h1><a class="space-action primary-action" href="{}">{} Пригласить участника</a></header>
        <div class="org-panel-pad">{filters}<p class="org-muted">SSO не настроен.</p></div>
        <ul class="org-rows">{records}</ul>{pagination}</section>
        <section class="org-panel org-panel-pad org-gap"><h2>Группы</h2><form class="org-inline" method="post" action="{path}">
          <input type="hidden" name="action" value="group"><label>Название группы<input name="name" maxlength="120" required></label>
          <button class="space-action primary-action">Создать группу</button></form></section>"#,
            panel_path(tenant, Panel::Invitations),
            icon("plus")
        ),
    )
}

pub(super) fn invitations(
    ctx: &WorkspaceContext,
    mail_enabled: bool,
    filter: &ListFilter,
    rows: Vec<Invitation>,
) -> String {
    let path = panel_path(ctx.tenant.id, Panel::Invitations);
    let filters = filter_markup(&path, filter, "");
    let pagination = filter.navigation(&path, rows.len());
    let send = if mail_enabled {
        format!(
            r#"<form class="org-inline" method="post" action="{path}"><input type="hidden" name="action" value="invite">
        <label>Электронная почта<input type="email" name="email" maxlength="254" required></label>
        <button class="space-action primary-action">Отправить приглашение</button></form>"#
        )
    } else {
        r#"<p class="org-muted">Почтовая доставка приглашений не настроена.</p>"#.to_owned()
    };
    let mut markup = String::new();
    for row in rows.iter().take(PAGE_SIZE) {
        let status = if row.accepted_at.is_some() {
            "Принято"
        } else if row.revoked_at.is_some() {
            "Отозвано"
        } else if row.expires_at <= now_ms() {
            "Просрочено"
        } else if row.delivered_at.is_none() {
            "Отправляется"
        } else {
            "Ожидает принятия"
        };
        let revoke = if row.accepted_at.is_none()
            && row.revoked_at.is_none()
            && row.expires_at > now_ms()
        {
            format!(
                r#"<form method="post" action="{path}"><input type="hidden" name="action" value="revoke"><input type="hidden" name="invitation" value="{}"><button class="space-action space-secondary">Отозвать</button></form>"#,
                row.id
            )
        } else {
            String::new()
        };
        let _ = write!(
            markup,
            r#"<li class="org-invitation-row"><strong>{}</strong><span class="org-badge">{status}</span>{revoke}</li>"#,
            esc(&row.email)
        );
    }
    if markup.is_empty() {
        markup = r#"<li class="org-empty">Приглашений пока нет.</li>"#.to_owned();
    }
    organization_page(
        "Приглашения",
        ctx,
        Panel::Invitations,
        None,
        &format!(
            r#"<section class="org-panel"><header class="org-panel-header"><h1>Приглашения</h1></header>
        <div class="org-panel-pad">{send}{filters}</div><ul class="org-rows">{markup}</ul>{pagination}</section>"#
        ),
    )
}

fn grant_form(path: &str, kind: &str, label: &str, choices: &str) -> String {
    if choices.is_empty() {
        return String::new();
    }
    format!(
        r#"<form class="org-inline" method="post" action="{path}"><input type="hidden" name="kind" value="{kind}">
      <label>{label}<select name="recipient">{choices}</select></label><label>Доступ<select name="role">
      <option value="viewer">Читатель</option><option value="editor">Редактор</option><option value="remove">Отозвать назначение</option></select></label>
      <button class="space-action primary-action">Применить</button></form>"#
    )
}

fn remove_grant(path: &str, kind: &str, target: &str) -> String {
    format!(
        r#"<form method="post" action="{path}"><input type="hidden" name="kind" value="{kind}">
       <input type="hidden" name="recipient" value="{}"><button class="space-action space-secondary" name="role" value="remove">Отозвать</button></form>"#,
        esc(target)
    )
}

fn origin_name(origin: &str) -> &'static str {
    match origin {
        "owner" => "Владелец",
        "administrator" => "Администратор организации",
        "direct" => "Личная роль",
        "group" => "Группа",
        "department" => "Правило отдела",
        _ => "Доступ",
    }
}

pub(super) fn space_access(
    ctx: &WorkspaceContext,
    space: Option<&Space>,
    path: &str,
    filter: &ListFilter,
    grants: Vec<SpaceGrant>,
    people: &[OrganizationMember],
    groups: &[OrganizationGroup],
    departments: &[String],
    rules: &[SpaceRule],
) -> Result<String, sqlx::Error> {
    let title = space.map_or("Доступ к пространствам", |space| {
        space.name.as_str()
    });
    let role_opts = simple_options(
        &filter.role,
        &[
            ("", "Все роли"),
            ("owner", "Владельцы"),
            ("editor", "Редакторы"),
            ("viewer", "Читатели"),
        ],
    );
    let source_opts = simple_options(
        &filter.source,
        &[
            ("", "Все основания"),
            ("owner", "Владелец"),
            ("administrator", "Администратор"),
            ("direct", "Личная роль"),
            ("group", "Группа"),
            ("department", "Правило отдела"),
        ],
    );
    let person_field = if space.is_some() {
        format!(
            r#"<label>Найти получателя<input name="person" value="{}" maxlength="120" type="search"></label>"#,
            esc(&filter.person)
        )
    } else {
        String::new()
    };
    let filters = filter_markup(
        path,
        filter,
        &format!(
            r#"<label>Роль<select name="role">{role_opts}</select></label>
      <label>Основание<select name="source">{source_opts}</select></label>{person_field}"#
        ),
    );
    let pagination = filter.navigation(path, grants.len());
    let mut rows = String::new();
    for grant in grants.iter().take(PAGE_SIZE) {
        let sources = grant
            .origins
            .iter()
            .map(|origin| origin_name(origin))
            .collect::<Vec<_>>()
            .join(", ");
        let action = if space.is_none() {
            format!(
                r#"<a href="{}/access">Настроить доступ</a>"#,
                space_path(ctx.tenant.id, grant.space)
            )
        } else if grant.origins.iter().any(|origin| origin == "direct") {
            remove_grant(path, "person", &grant.account.to_string())
        } else {
            String::new()
        };
        let _ = write!(
            rows,
            r#"<tr><td>{}</td><td><strong>{}</strong><small>{}</small></td><td>{}</td><td>{sources}</td><td>{action}</td></tr>"#,
            esc(&grant.space_name()?),
            esc(&grant.name),
            esc(&grant.email),
            role_name(&grant.role)
        );
    }
    let list = if rows.is_empty() {
        r#"<p class="org-empty">Совпадений нет.</p>"#.to_owned()
    } else {
        format!(
            r#"<div class="org-table-wrap" tabindex="0"><table class="org-table"><thead><tr><th scope="col">Пространство</th><th scope="col">Участник</th><th scope="col">Доступ</th><th scope="col">Основания</th><th scope="col"><span class="sr-only">Действия</span></th></tr></thead><tbody>{rows}</tbody></table></div>"#
        )
    };
    let forms = if space.is_some() {
        let person_opts = options(
            "",
            people
                .iter()
                .take(PAGE_SIZE)
                .map(|row| (row.id.to_string(), format!("{} · {}", row.name, row.email))),
        );
        let group_opts = options(
            "",
            groups
                .iter()
                .map(|row| (row.id.to_string(), row.name.clone())),
        );
        let department_opts = options(
            "",
            departments.iter().map(|name| (name.clone(), name.clone())),
        );
        let mut existing = String::new();
        for rule in rules {
            let kind = if rule.kind == "group" {
                "Группа"
            } else {
                "Отдел"
            };
            let remove = if admin(&ctx.tenant.role) {
                remove_grant(path, &rule.kind, &rule.target)
            } else {
                String::new()
            };
            let _ = write!(
                existing,
                r#"<li class="org-invitation-row"><strong>{kind} · {}</strong><span class="org-badge">{}</span>{remove}</li>"#,
                esc(&rule.label),
                role_name(&rule.role)
            );
        }
        format!(
            r#"<section class="org-panel org-panel-pad org-gap"><h2>Назначить доступ</h2>{}{}{}
          <ul class="org-rows">{existing}</ul></section>"#,
            grant_form(path, "person", "Участник", &person_opts),
            grant_form(path, "group", "Группа", &group_opts),
            grant_form(path, "department", "Отдел", &department_opts)
        )
    } else {
        String::new()
    };
    Ok(organization_page(
        title,
        ctx,
        Panel::Access,
        space,
        &format!(
            r#"<section class="org-panel"><header class="org-panel-header">
        <div><h1>{}</h1><p>Доступ к пространствам</p></div></header><div class="org-panel-pad">{filters}</div>{list}{pagination}</section>{forms}"#,
            esc(title)
        ),
    ))
}
