use std::fmt::Write as _;

use axum::{
    extract::Request,
    http::{Method, StatusCode},
    response::Response,
};
use uuid::Uuid;

use crate::{
    db::{
        Principal, Store,
        tasks::{
            AssigneeChange, CreateTask, Task, TaskArea, TaskBoard, TaskChange, TaskCreation,
            TaskDetail, TaskFilter, TaskMember, TaskScope, TaskStatus, TaskWrite,
        },
    },
    http::{self, Form, html, plain, redirect},
    text::{escape_html as escape, revision_number},
    workspace::{canonical_uuid, page_path, space_path},
    workspace_chrome::{self, icon},
};

pub(crate) const STYLES: &str = include_str!("tasks_http/tasks.css");
pub(crate) const SCRIPT: &str = include_str!("tasks_http/tasks.js");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Target {
    Board,
    New,
    Detail(Uuid),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TasksRoute {
    pub tenant: Uuid,
    pub space: Uuid,
    target: Target,
}

impl TasksRoute {
    pub(crate) fn parse(path: &str) -> Option<Self> {
        let mut parts = path.strip_prefix("/workspace/")?.split('/');
        let tenant = canonical_uuid(parts.next()?)?;
        if parts.next()? != "spaces" {
            return None;
        }
        let space = canonical_uuid(parts.next()?)?;
        if parts.next()? != "tasks" {
            return None;
        }
        let target = match parts.next() {
            None => Target::Board,
            Some("new") => Target::New,
            Some(id) => Target::Detail(canonical_uuid(id)?),
        };
        parts.next().is_none().then_some(Self {
            tenant,
            space,
            target,
        })
    }

    fn scope(self, principal: &Principal) -> TaskScope {
        TaskScope {
            tenant: self.tenant,
            space: self.space,
            account: principal.id,
        }
    }
}

pub(crate) fn tasks_path(tenant: Uuid, space: Uuid) -> String {
    format!("{}/tasks", space_path(tenant, space))
}

pub(crate) fn task_path(tenant: Uuid, space: Uuid, task: Uuid) -> String {
    format!("{}/{task}", tasks_path(tenant, space))
}

pub(crate) fn new_task_path(tenant: Uuid, space: Uuid) -> String {
    format!("{}/new", tasks_path(tenant, space))
}

#[derive(Clone, Copy)]
struct View<'a> {
    list: bool,
    filter: TaskFilter<'a>,
}

impl<'a> View<'a> {
    fn from_query(query: &'a Form) -> Self {
        let search = query.get("q").trim();
        let end = search
            .char_indices()
            .nth(100)
            .map_or(search.len(), |(index, _)| index);
        Self {
            list: query.get("view") == "list",
            filter: TaskFilter {
                assignee: canonical_uuid(query.get("assignee")),
                stage: TaskStatus::parse(query.get("stage")),
                search: &search[..end],
                recent_first: query.get("sort") == "recent",
            },
        }
    }

    fn reset(self) -> Self {
        Self {
            filter: TaskFilter {
                recent_first: self.filter.recent_first,
                ..Default::default()
            },
            ..self
        }
    }

    fn href(&self, path: &str, more: &[(&str, &str)]) -> String {
        let mut query = url::form_urlencoded::Serializer::new(String::new());
        if self.list {
            query.append_pair("view", "list");
        }
        if let Some(id) = self.filter.assignee {
            query.append_pair("assignee", &id.to_string());
        }
        if !self.filter.search.is_empty() {
            query.append_pair("q", self.filter.search);
        }
        if let Some(stage) = self.filter.stage {
            query.append_pair("stage", stage.as_str());
        }
        if self.filter.recent_first {
            query.append_pair("sort", "recent");
        }
        query.extend_pairs(more.iter().copied());
        let query = query.finish();
        if query.is_empty() {
            path.to_owned()
        } else {
            format!("{path}?{query}")
        }
    }
}

fn missing() -> Response {
    plain(StatusCode::NOT_FOUND, "Задача или пространство недоступны.")
}

fn forbidden() -> Response {
    plain(
        StatusCode::FORBIDDEN,
        "Изменять задачи может владелец пространства.",
    )
}

pub(crate) async fn handle(
    store: &Store,
    principal: &Principal,
    route: TasksRoute,
    origin: &str,
    request: Request,
) -> Result<Response, sqlx::Error> {
    let scope = route.scope(principal);
    let query = Form::from_query(request.uri().query().unwrap_or_default());
    if query.get("q").contains('\0') {
        return Ok(plain(
            StatusCode::BAD_REQUEST,
            "Недопустимый символ в поиске задач.",
        ));
    }
    let view = View::from_query(&query);
    match *request.method() {
        Method::GET | Method::HEAD => show(store, scope, route.target, view, &query).await,
        Method::POST if route.target != Target::New => {
            if !http::check_origin(request.headers(), origin) {
                return Ok(plain(
                    StatusCode::FORBIDDEN,
                    "Откройте форму задачи в «Севере».",
                ));
            }
            let form = match http::read_form(request).await {
                Ok(form) => form,
                Err(error) => {
                    return Ok(http::form_error(
                        error,
                        "Используйте форму задачи «Севера».",
                    ));
                }
            };
            match route.target {
                Target::Board => create(store, scope, view, &form).await,
                Target::Detail(task) => update(store, scope, view, task, &form).await,
                Target::New => unreachable!(),
            }
        }
        _ => Ok(http::method_not_allowed(if route.target == Target::New {
            "GET, HEAD"
        } else {
            "GET, HEAD, POST"
        })),
    }
}

async fn show(
    store: &Store,
    scope: TaskScope,
    target: Target,
    view: View<'_>,
    query: &Form,
) -> Result<Response, sqlx::Error> {
    match target {
        Target::Board => render_board(store, scope, view, StatusCode::OK, "").await,
        Target::New => {
            let form = CreationForm {
                operation: Uuid::new_v4(),
                title: "",
                status: TaskStatus::parse(query.get("status")).unwrap_or(TaskStatus::Open),
                source: canonical_uuid(query.get("source")),
            };
            render_creation(store, scope, view, &form, StatusCode::OK, "").await
        }
        Target::Detail(id) => render_detail(store, scope, view, id, StatusCode::OK, "").await,
    }
}

struct CreationForm<'a> {
    operation: Uuid,
    title: &'a str,
    status: TaskStatus,
    source: Option<Uuid>,
}

fn optional_uuid(form: &Form, field: &str) -> Result<Option<Uuid>, ()> {
    match form.field(field).map_err(|_| ())? {
        None | Some("") => Ok(None),
        Some(id) => canonical_uuid(id).map(Some).ok_or(()),
    }
}

fn parse_creation(form: &Form) -> Option<CreationForm<'_>> {
    Some(CreationForm {
        operation: canonical_uuid(form.get("operation_id"))?,
        title: form.field("title").ok().flatten()?,
        status: TaskStatus::parse(form.get("status"))?,
        source: optional_uuid(form, "source").ok()?,
    })
}

async fn create(
    store: &Store,
    scope: TaskScope,
    view: View<'_>,
    data: &Form,
) -> Result<Response, sqlx::Error> {
    let Some(mut form) = parse_creation(data) else {
        let retry = CreationForm {
            operation: canonical_uuid(data.get("operation_id")).unwrap_or_else(Uuid::new_v4),
            title: data.get("title"),
            status: TaskStatus::parse(data.get("status")).unwrap_or(TaskStatus::Open),
            source: optional_uuid(data, "source").ok().flatten(),
        };
        return render_creation(
            store,
            scope,
            view,
            &retry,
            StatusCode::UNPROCESSABLE_ENTITY,
            "Проверьте поля задачи.",
        )
        .await;
    };
    let write = store
        .create_task(
            scope,
            &CreateTask {
                operation: form.operation,
                title: form.title,
                status: form.status,
                source: form.source,
            },
        )
        .await?;
    let (status, message) = match write {
        TaskWrite::Saved(id) => {
            return Ok(redirect(
                &view.href(&task_path(scope.tenant, scope.space, id), &[]),
            ));
        }
        TaskWrite::Missing => return Ok(missing()),
        TaskWrite::Forbidden => return Ok(forbidden()),
        TaskWrite::Conflict => {
            form.operation = Uuid::new_v4();
            (
                StatusCode::CONFLICT,
                "Предыдущая отправка уже принята. Проверьте поля перед повторной отправкой.",
            )
        }
        TaskWrite::InvalidTitle => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "Введите название от 1 до 100 символов.",
        ),
        TaskWrite::InvalidSource => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "Выберите доступный источник.",
        ),
        TaskWrite::InvalidAssignee => unreachable!(),
    };
    render_creation(store, scope, view, &form, status, message).await
}

fn task_change(data: &Form, drop_only: bool) -> Option<TaskChange> {
    Some(TaskChange {
        version: revision_number(data.get("version")).filter(|version| *version > 0)?,
        status: TaskStatus::parse(data.get("status"))?,
        assignee: if drop_only || data.get("person") == "keep" {
            AssigneeChange::Keep
        } else if data.get("assignee_choice") == "set" {
            AssigneeChange::Set(optional_uuid(data, "person").ok()?)
        } else {
            return None;
        },
    })
}

async fn update(
    store: &Store,
    scope: TaskScope,
    view: View<'_>,
    id: Uuid,
    data: &Form,
) -> Result<Response, sqlx::Error> {
    let drop_only = data.get("mode") == "drop";
    let Some(input) = task_change(data, drop_only) else {
        return render_detail(
            store,
            scope,
            view,
            id,
            StatusCode::UNPROCESSABLE_ENTITY,
            "Выберите допустимый этап и исполнителя.",
        )
        .await;
    };
    let write = store.change_task(scope, id, input).await?;
    let (status, message) = match write {
        TaskWrite::Saved(_) => {
            let path = if drop_only {
                tasks_path(scope.tenant, scope.space)
            } else {
                task_path(scope.tenant, scope.space, id)
            };
            return Ok(redirect(&view.href(&path, &[])));
        }
        TaskWrite::Missing => return Ok(missing()),
        TaskWrite::Forbidden => return Ok(forbidden()),
        TaskWrite::Conflict => (
            StatusCode::CONFLICT,
            "Задачу уже изменили. Проверьте текущие данные.",
        ),
        TaskWrite::InvalidAssignee => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "Исполнитель потерял доступ. Выберите другого участника.",
        ),
        TaskWrite::InvalidSource | TaskWrite::InvalidTitle => unreachable!(),
    };
    if drop_only {
        render_board(store, scope, view, status, message).await
    } else {
        render_detail(store, scope, view, id, status, message).await
    }
}

async fn render_board(
    store: &Store,
    scope: TaskScope,
    view: View<'_>,
    status: StatusCode,
    message: &str,
) -> Result<Response, sqlx::Error> {
    Ok(
        match store
            .tasks(scope.tenant, scope.space, scope.account, view.filter)
            .await?
        {
            Some(board) => render(
                &board.area,
                scope,
                status,
                &board_body(&board, scope, view, message),
                "",
            ),
            None => missing(),
        },
    )
}

async fn render_creation(
    store: &Store,
    scope: TaskScope,
    view: View<'_>,
    form: &CreationForm<'_>,
    status: StatusCode,
    message: &str,
) -> Result<Response, sqlx::Error> {
    let Some(creation) = store.task_creation(scope).await? else {
        return Ok(missing());
    };
    if !creation.area.can_manage {
        return Ok(forbidden());
    }
    let Some(board) = store
        .tasks(scope.tenant, scope.space, scope.account, view.filter)
        .await?
    else {
        return Ok(missing());
    };
    if !board.area.can_manage {
        return Ok(forbidden());
    }
    let dialog = creation_panel(&creation, scope, view, form, message);
    Ok(render(
        &board.area,
        scope,
        status,
        &board_body(&board, scope, view, ""),
        &dialog,
    ))
}

async fn render_detail(
    store: &Store,
    scope: TaskScope,
    view: View<'_>,
    id: Uuid,
    status: StatusCode,
    message: &str,
) -> Result<Response, sqlx::Error> {
    let Some(detail) = store
        .task(scope.tenant, scope.space, scope.account, id)
        .await?
    else {
        return Ok(missing());
    };
    let Some(board) = store
        .tasks(scope.tenant, scope.space, scope.account, view.filter)
        .await?
    else {
        return Ok(missing());
    };
    let dialog = detail_panel(&detail, scope, view, message);
    Ok(render(
        &detail.area,
        scope,
        status,
        &board_body(&board, scope, view, ""),
        &dialog,
    ))
}

fn render(
    area: &TaskArea,
    scope: TaskScope,
    status: StatusCode,
    board: &str,
    panel: &str,
) -> Response {
    let content = format!("{board}{panel}");
    let document = workspace_chrome::page(
        "Задачи команды",
        Some((scope.tenant, &area.tenant_name)),
        Some((scope.space, &area.space_name)),
        "tasks",
        &content,
    );
    let assets =
        r#"<link rel="stylesheet" href="/tasks.css"><script src="/tasks.js" defer></script>"#;
    let document = document.replacen("</head>", &format!("{assets}</head>"), 1);
    html(status, document)
}

fn notice(message: &str) -> String {
    if message.is_empty() {
        String::new()
    } else {
        format!(
            r#"<p class="tasks-error" role="alert">{}</p>"#,
            escape(message)
        )
    }
}

fn status_options(selected: TaskStatus) -> String {
    TaskStatus::ALL
        .into_iter()
        .map(|state| option(state.as_str(), state.label(), state == selected))
        .collect()
}

fn option(value: &str, label: &str, selected: bool) -> String {
    let selection = if selected { " selected" } else { "" };
    format!(
        r#"<option value="{}"{selection}>{}</option>"#,
        escape(value),
        escape(label)
    )
}

fn filter_bar(board: &TaskBoard, view: View<'_>, home: &str) -> String {
    let mut options = option("", "Все исполнители", view.filter.assignee.is_none());
    for person in &board.members {
        options.push_str(&option(
            &person.id.to_string(),
            &person.name,
            Some(person.id) == view.filter.assignee,
        ));
    }
    if let Some(id) = view.filter.assignee
        && !board.members.iter().any(|member| member.id == id)
    {
        options.push_str(&option(&id.to_string(), "Недоступный исполнитель", true));
    }
    let list_input = if view.list {
        r#"<input type="hidden" name="view" value="list">"#
    } else {
        ""
    };
    let mut stages = option("", "Все этапы", view.filter.stage.is_none());
    for status in TaskStatus::ALL {
        stages.push_str(&option(
            status.as_str(),
            status.label(),
            view.filter.stage == Some(status),
        ));
    }
    let reset = if view.filter.is_filtered() {
        format!(
            r#"<a class="tasks-filter-reset" href="{}">Сбросить фильтры</a>"#,
            escape(&view.reset().href(home, &[]))
        )
    } else {
        String::new()
    };
    let search = escape(view.filter.search);
    let order = [
        option("", "По номеру", !view.filter.recent_first),
        option("recent", "Недавно изменённые", view.filter.recent_first),
    ]
    .concat();
    format!(
        r#"<form class="tasks-filters" aria-label="Фильтры задач" action="{home}" method="get">
{list_input}<label class="tasks-search">{}<span class="tasks-visually-hidden">Поиск задач по названию или номеру</span><input type="search" name="q" maxlength="100" placeholder="Название или KB-номер" value="{search}"></label>
<label><span class="tasks-visually-hidden">Этап задачи</span><select name="stage">{stages}</select></label>
<label><span class="tasks-visually-hidden">Исполнитель</span><select name="assignee">{options}</select></label>
<label><span class="tasks-visually-hidden">Сортировка</span><select name="sort">{order}</select></label>
<button class="tasks-button tasks-filter-submit" type="submit">Найти</button>{reset}</form>"#,
        icon("search")
    )
}

fn board_body(board: &TaskBoard, scope: TaskScope, view: View<'_>, message: &str) -> String {
    let home = tasks_path(scope.tenant, scope.space);
    let new_url = new_task_path(scope.tenant, scope.space);
    let new_button = if board.area.can_manage {
        format!(
            r#"<a class="tasks-button tasks-primary" href="{}">{}Новая задача</a>"#,
            escape(&view.href(&new_url, &[])),
            icon("plus")
        )
    } else {
        String::new()
    };
    let title = escape(&board.area.space_name);
    let board_view = View {
        list: false,
        ..view
    };
    let list_view = View { list: true, ..view };
    let filters = filter_bar(board, view, &home);
    let result = if view.filter.is_filtered() && board.tasks.is_empty() {
        r#"<div class="tasks-empty-selection"><p>По заданным условиям задач нет.</p></div>"#
            .to_owned()
    } else if view.list {
        let rows = board
            .tasks
            .iter()
            .map(|task| task_card(task, scope, view, false))
            .collect::<String>();
        let rows = if rows.is_empty() {
            r#"<p class="tasks-empty">Задач пока нет.</p>"#.to_owned()
        } else {
            rows
        };
        format!(r#"<div class="tasks-list">{rows}</div>"#)
    } else {
        let mut lanes = String::new();
        for state in TaskStatus::ALL {
            let items = board
                .tasks
                .iter()
                .filter(|task| task.status == state)
                .collect::<Vec<_>>();
            let add = if board.area.can_manage {
                format!(
                    r#"<a class="tasks-button tasks-icon" aria-label="Добавить задачу в «{}»" href="{}">{}</a>"#,
                    state.label(),
                    escape(&view.href(&new_url, &[("status", state.as_str())])),
                    icon("plus")
                )
            } else {
                String::new()
            };
            let cards = if items.is_empty() {
                r#"<p class="tasks-empty">Задач пока нет.</p>"#.to_owned()
            } else {
                items
                    .iter()
                    .map(|task| task_card(task, scope, view, board.area.can_manage))
                    .collect()
            };
            let _ = write!(
                lanes,
                r#"<section class="tasks-lane" data-task-lane="{}" aria-label="{}">
<header class="tasks-lane-head"><span class="tasks-status-dot" aria-hidden="true"></span><h2>{}</h2><span class="tasks-mono" aria-label="{} задач">{}</span>{add}</header>{cards}</section>"#,
                state.as_str(),
                state.label(),
                state.label(),
                items.len(),
                items.len()
            );
        }
        format!(r#"<div class="tasks-lanes">{lanes}</div>"#)
    };
    let error = notice(message);
    let collections = format!("{}/collections", space_path(scope.tenant,scope.space));
    format!(
        r#"<section class="tasks-board" aria-labelledby="tasks-title">
<header class="tasks-header"><div><p class="tasks-kicker">{}{title}</p><h1 id="tasks-title">Задачи команды</h1></div><div class="tasks-actions"><a class="tasks-button" href="{collections}">Реестры</a>{new_button}</div></header>
<div class="tasks-toolbar">
<nav class="tasks-actions" aria-label="Представление задач">
<a class="tasks-button" href="{}" {}>{}Kanban</a>
<a class="tasks-button" href="{}" {}>{}Список</a></nav>
{filters}
</div>{error}{result}
<form method="post" data-task-drop-form hidden><input type="hidden" name="mode" value="drop"><input type="hidden" name="version"><input type="hidden" name="status"></form>
</section>"#,
        icon("board"),
        escape(&board_view.href(&home, &[])),
        if view.list {
            ""
        } else {
            r#"aria-current="page""#
        },
        icon("board"),
        escape(&list_view.href(&home, &[])),
        if view.list {
            r#"aria-current="page""#
        } else {
            ""
        },
        icon("list")
    )
}

fn source_link(task: &Task, scope: TaskScope) -> String {
    match (task.source_page, task.source_title.as_deref()) {
        (Some(id), Some(title)) => format!(
            r#"<a class="tasks-source" href="{}">{}<span>{}</span></a>"#,
            page_path(scope.tenant, scope.space, id),
            icon("doc"),
            escape(title)
        ),
        _ => "<span>Без источника</span>".to_owned(),
    }
}

fn task_card(task: &Task, scope: TaskScope, view: View<'_>, draggable: bool) -> String {
    let href = escape(&view.href(&task_path(scope.tenant, scope.space, task.id), &[]));
    let data = if draggable {
        format!(
            r#" draggable="true" data-task-drag="{href}" data-task-version="{}" data-task-status="{}""#,
            task.version,
            task.status.as_str()
        )
    } else {
        String::new()
    };
    let code = format!("KB-{:03}", task.number);
    let title = escape(&task.title);
    let author = match (task.assignee, task.assignee_name.as_deref()) {
        (_, Some(name)) => format!(
            r#"<span class="tasks-avatar" title="{}" aria-label="Исполнитель: {}">{}</span>"#,
            escape(name),
            escape(name),
            escape(
                &name
                    .split_whitespace()
                    .take(2)
                    .filter_map(|word| word.chars().next())
                    .flat_map(char::to_uppercase)
                    .collect::<String>()
            )
        ),
        (Some(_), None) => "<span>Доступ исполнителя отозван</span>".to_owned(),
        (None, None) => "<span>Без исполнителя</span>".to_owned(),
    };
    let status = if view.list {
        format!(r#"<span class="tasks-tag">{}</span>"#, task.status.label())
    } else {
        String::new()
    };
    format!(
        r#"<article class="tasks-card"{data}><a class="tasks-open" href="{href}"><span class="tasks-mono">{code}</span><strong>{title}</strong></a>{status}
<div class="tasks-attributes">{}{author}</div></article>"#,
        source_link(task, scope)
    )
}

fn panel(title: &str, back: &str, content: &str) -> String {
    format!(
        r#"<section class="tasks-panel" data-task-panel aria-labelledby="task-panel-title" tabindex="-1">
<header class="tasks-panel-header"><h2 id="task-panel-title">{}</h2><a class="tasks-button tasks-icon" href="{}" aria-label="Закрыть">{}</a></header>
<div class="tasks-panel-body">{content}</div></section>"#,
        escape(title),
        escape(back),
        icon("close")
    )
}

fn creation_panel(
    creation: &TaskCreation,
    scope: TaskScope,
    view: View<'_>,
    form: &CreationForm<'_>,
    message: &str,
) -> String {
    let mut sources = option("", "Без источника", form.source.is_none());
    for source in &creation.sources {
        sources.push_str(&option(
            &source.id.to_string(),
            &source.title,
            Some(source.id) == form.source,
        ));
    }
    let back = view.href(&tasks_path(scope.tenant, scope.space), &[]);
    let fields = format!(
        r#"{}<form method="post" action="{}" class="tasks-form">
<input type="hidden" name="operation_id" value="{}">
<label>Название<input name="title" value="{}" maxlength="100" placeholder="Например, «Согласовать доступ»" required></label>
<label>Этап<select name="status">{}</select></label>
<label>Источник<select name="source">{sources}</select></label>
<div class="tasks-actions"><button class="tasks-button tasks-primary" type="submit">{}Создать задачу</button></div></form>"#,
        notice(message),
        escape(&back),
        form.operation,
        escape(form.title),
        status_options(form.status),
        icon("plus")
    );
    panel("Новая задача", &back, &fields)
}

fn detail_panel(detail: &TaskDetail, scope: TaskScope, view: View<'_>, message: &str) -> String {
    let task = &detail.task;
    let mut fields = format!(
        r#"<div class="tasks-actions"><span class="tasks-tag">KB-{:03}</span><span class="tasks-tag">{}</span></div>{}"#,
        task.number,
        task.status.label(),
        notice(message)
    );
    if detail.can_manage {
        let current = task.assignee;
        let invalid_assignee =
            current.is_some_and(|id| !detail.members.iter().any(|member| member.id == id));
        let mut people = option("", "Без исполнителя", current.is_none());
        if invalid_assignee {
            people.push_str(&option("keep", "Доступ прежнего исполнителя отозван", true));
        }
        for TaskMember { id, name } in &detail.members {
            people.push_str(&option(&id.to_string(), name, Some(*id) == current));
        }
        let _ = write!(
            fields,
            r#"<form method="post" class="tasks-form">
<input type="hidden" name="version" value="{}"><input type="hidden" name="assignee_choice" value="set">
<label>Этап<select name="status">{}</select></label><label>Исполнитель<select name="person">{people}</select></label>
<div class="tasks-actions"><button class="tasks-button tasks-primary" type="submit">{}Применить</button></div></form>"#,
            task.version,
            status_options(task.status),
            icon("check")
        );
    } else {
        let label = task.assignee_name.as_deref().unwrap_or("Без исполнителя");
        let _ = write!(
            fields,
            r#"<p class="tasks-muted">Исполнитель · {}</p>"#,
            escape(label)
        );
    }
    let _ = write!(
        fields,
        r#"<div class="tasks-detail-links">{}
<a class="tasks-button" href="{}/graph?node={}">{}Открыть на графе</a>
<a class="tasks-button" href="{}/approach?task={}">{}Открыть AI-подход</a></div>"#,
        source_link(task, scope),
        space_path(scope.tenant, scope.space),
        task.id,
        icon("graph"),
        space_path(scope.tenant, scope.space),
        task.id,
        icon("flow")
    );
    panel(
        &task.title,
        &view.href(&tasks_path(scope.tenant, scope.space), &[]),
        &fields,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_selection_survives_detail_create_and_reset_without_overriding_initial_status() {
        let query = Form::from_query("view=list&q=++KB-00++&stage=work&sort=recent");
        let view = View::from_query(&query);
        assert_eq!(
            view.href("/tasks/new", &[("status", "review")]),
            "/tasks/new?view=list&q=KB-00&stage=work&sort=recent&status=review"
        );
        assert_eq!(
            view.reset().href("/tasks", &[]),
            "/tasks?view=list&sort=recent"
        );
        let home = Form::from_query("q=%26%3C+%25&stage=unknown&sort=unknown");
        assert_eq!(
            View::from_query(&home).href("/tasks", &[]),
            "/tasks?q=%26%3C+%25"
        );
    }

    #[test]
    fn empty_selection_has_one_reset_and_no_readonly_manage_controls() {
        let scope = TaskScope {
            tenant: Uuid::new_v4(),
            space: Uuid::new_v4(),
            account: Uuid::new_v4(),
        };
        let board = TaskBoard {
            tasks: vec![],
            members: vec![],
            area: TaskArea {
                tenant_name: "Т".into(),
                space_name: "К".into(),
                can_manage: false,
            },
        };
        let form = Form::from_query("q=%22%3C&stage=done&view=list");
        let body = board_body(&board, scope, View::from_query(&form), "");
        assert!(body.contains("По заданным условиям задач нет"));
        assert_eq!(body.matches("Сбросить фильтры").count(), 1);
        assert!(body.contains("value=\"&quot;&lt;\""));
        assert!(!body.contains("Новая задача"));
        assert!(!body.contains("data-task-drag="));
        let defaults = Form::from_query("");
        let unfiltered = board_body(&board, scope, View::from_query(&defaults), "");
        assert_eq!(unfiltered.matches("data-task-lane=").count(), 4);
        assert!(!unfiltered.contains("Сбросить фильтры"));
    }

    #[test]
    fn task_routing_keeps_space_and_detail_ids_and_rejects_suffixes() {
        let tenant = Uuid::new_v4();
        let space = Uuid::new_v4();
        let task = Uuid::new_v4();
        assert_eq!(
            TasksRoute::parse(&task_path(tenant, space, task)),
            Some(TasksRoute {
                tenant,
                space,
                target: Target::Detail(task)
            })
        );
        assert!(TasksRoute::parse(&format!("{}/edit", task_path(tenant, space, task))).is_none());
        assert!(TasksRoute::parse(&format!("{}/other", tasks_path(tenant, space))).is_none());
    }

    #[test]
    fn task_filters_survive_switching_view_and_operation_scope() {
        let assignee = Uuid::new_v4();
        let query = Form::from_query(&format!("assignee={assignee}&view=list"));
        let view = View::from_query(&query);
        let target = view.href("/example", &[("status", "done")]);
        assert_eq!(
            target,
            format!("/example?view=list&assignee={assignee}&status=done")
        );
        assert!(
            task_change(
                &Form::from_query("version=1&status=work&assignee_choice=set&person=invalid"),
                false
            )
            .is_none()
        );
        assert!(task_change(&Form::from_query("version=1&status=work"), true).is_some());
    }
}
