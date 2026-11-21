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
        approaches::{ApproachVersions, SOURCE_EXCERPT_LIMIT, StartApproach},
        tasks::{TaskScope, TaskStatus},
    },
    http::{self, Form, html, plain, redirect},
    model_provider::{Model, SYSTEM_PROMPT, user_prompt},
    text::escape_html as h,
    workspace::{canonical_uuid, page_path, space_path},
    workspace_chrome::{icon, page},
};

#[derive(Clone, Copy)]
pub(crate) struct ApproachRoute {
    tenant: Uuid,
    space: Uuid,
}

impl ApproachRoute {
    pub(crate) fn parse(path: &str) -> Option<Self> {
        let mut path = path.strip_prefix("/workspace/")?.split('/');
        let tenant = canonical_uuid(path.next()?)?;
        if path.next()? != "spaces" {
            return None;
        }
        let space = canonical_uuid(path.next()?)?;
        (path.next()? == "approach" && path.next().is_none()).then_some(Self { tenant, space })
    }
}

type Step = (&'static str, &'static str, &'static str, &'static str);

const STEPS: [Step; 7] = [
    ("trigger", "Вход", "Задача из пространства", "board"),
    (
        "instruction",
        "Инструкция",
        "Правила для аналитика",
        "spark",
    ),
    ("source", "Контекст", "Источник и редакция", "book"),
    ("guard", "Условие", "Проверить источник", "shield"),
    (
        "action",
        "Инструмент",
        "Подготовить Markdown-черновик",
        "code",
    ),
    ("review", "Подтверждение", "Решение владельца", "user"),
    ("output", "Результат", "Спецификация и ссылка", "doc"),
];

fn flow_chart(
    url: &str,
    requested: &str,
    require_source: bool,
    task: Option<Uuid>,
) -> (String, Step) {
    let steps = STEPS
        .iter()
        .copied()
        .filter(|step| step.0 != "guard" || require_source);
    let active = steps
        .clone()
        .find(|step| step.0 == requested)
        .unwrap_or(STEPS[1]);
    let task_pair = task.map_or_else(String::new, |id| format!("&amp;task={id}"));
    let mut nodes = String::new();
    for (key, kind, label, glyph) in steps {
        let current = if key == active.0 {
            r#" aria-current="step""#
        } else {
            ""
        };
        let _ = write!(
            nodes,
            r#"<li><a class="parity-flow-node" id="flow-{key}" data-node="{key}" href="{url}?node={key}{task_pair}#flow-properties"{current}><span>{} {kind}</span><strong>{label}</strong></a></li>"#,
            icon(glyph)
        );
    }
    // The lower row ends at the right edge. Join it at its first occupied column.
    let bend = if require_source { 200 } else { 400 };
    let chart = format!(
        r#"<nav class="parity-flow-chart" aria-label="Схема AI-подхода" data-guard="{require_source}">
<svg class="parity-flow-wires" aria-hidden="true" viewBox="0 0 800 400" preserveAspectRatio="none"><path d="M100 100 H790 Q797 100 797 107 V193 Q797 200 790 200 H{bend} q-5 0 -5 7 V293 q0 7 7 7 H700"/></svg>
<ol class="parity-flow-sequence" role="list">{nodes}</ol></nav>"#
    );
    (chart, active)
}

const SOURCE_REQUIRED: &str = "Задача должна ссылаться на выбранный источник.";

fn launch_versions(form: &Form) -> Option<ApproachVersions> {
    let version = |name| form.get(name).parse::<i64>().ok().filter(|v| *v > 0);
    Some(ApproachVersions {
        spec: version("specVersion")?,
        source: version("sourceRevision")?,
        task: version("taskVersion")?,
    })
}

pub(crate) async fn handle(
    store: &Store,
    principal: &Principal,
    route: ApproachRoute,
    origin: &str,
    request: Request,
) -> Result<Response, sqlx::Error> {
    let scope = TaskScope {
        tenant: route.tenant,
        space: route.space,
        account: principal.id,
    };
    let base = space_path(route.tenant, route.space);
    let url = format!("{base}/approach");
    let query = Form::from_query(request.uri().query().unwrap_or_default());
    let task_query = canonical_uuid(query.get("task"));
    let read = matches!(*request.method(), Method::GET | Method::HEAD);
    if !read && request.method() != Method::POST {
        let mut response = plain(StatusCode::METHOD_NOT_ALLOWED, "Откройте форму AI-подхода.");
        response.headers_mut().insert(
            axum::http::header::ALLOW,
            axum::http::HeaderValue::from_static("GET, HEAD, POST"),
        );
        return Ok(response);
    }
    let Some(board) = store
        .tasks(route.tenant, route.space, principal.id, Default::default())
        .await?
    else {
        return Ok(plain(StatusCode::NOT_FOUND, "Пространство недоступно."));
    };
    if !read {
        if !http::check_origin(request.headers(), origin) || !board.area.can_manage {
            return Ok(plain(
                StatusCode::FORBIDDEN,
                "Для действия требуется доступ владельца пространства.",
            ));
        }
        let form = match http::read_form_bounded(request, 16_384).await {
            Ok(form) => form,
            Err(error) => return Ok(http::form_error(error, "Откройте форму AI-подхода.")),
        };
        let task = canonical_uuid(form.get("task"));
        let target = task.map_or_else(|| url.clone(), |task| format!("{url}?task={task}"));
        match form.get("action") {
            "configure" => {
                let instruction = form.get("instruction").trim();
                let Some(source) = canonical_uuid(form.get("source")) else {
                    return Ok(plain(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "Выберите доступный источник.",
                    ));
                };
                let Some(version) = form
                    .get("version")
                    .parse::<i64>()
                    .ok()
                    .filter(|number| *number >= 0)
                else {
                    return Ok(plain(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "Перезагрузите настройки.",
                    ));
                };
                if instruction.is_empty()
                    || instruction.chars().count() > 1500
                    || instruction.contains('\0')
                {
                    return Ok(plain(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "Инструкция должна содержать от 1 до 1500 символов.",
                    ));
                }
                if !store
                    .set_approach(
                        scope,
                        version,
                        instruction,
                        source,
                        form.get("requireSource") == "1",
                    )
                    .await?
                {
                    return Ok(plain(
                        StatusCode::CONFLICT,
                        "Настройки или права изменились. Откройте подход снова.",
                    ));
                }
            }
            "run" => {
                let Some(model) = Model::configured() else {
                    return Ok(plain(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "Локальная модель не настроена.",
                    ));
                };
                let Some((task, run)) = task.zip(canonical_uuid(form.get("operation"))) else {
                    return Ok(plain(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "Выберите задачу и откройте форму заново.",
                    ));
                };
                let Some(versions) = launch_versions(&form) else {
                    return Ok(plain(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "Откройте форму запуска заново.",
                    ));
                };
                let input = match store.start_approach(scope, task, run, versions).await? {
                    StartApproach::Started(input) => input,
                    StartApproach::AlreadyStarted => return Ok(redirect(&target)),
                    StartApproach::Unavailable => {
                        return Ok(plain(
                            StatusCode::NOT_FOUND,
                            "Подход или задача недоступны.",
                        ));
                    }
                    StartApproach::TaskDone => {
                        return Ok(plain(StatusCode::CONFLICT, "Задача уже завершена."));
                    }
                    StartApproach::SourceRequired => {
                        return Ok(plain(StatusCode::UNPROCESSABLE_ENTITY, SOURCE_REQUIRED));
                    }
                    StartApproach::PreviewChanged => {
                        return Ok(plain(
                            StatusCode::CONFLICT,
                            "Контекст изменился. Откройте подход снова.",
                        ));
                    }
                    StartApproach::ActiveRun => {
                        return Ok(plain(
                            StatusCode::CONFLICT,
                            "По задаче уже есть активный запуск или черновик на согласовании.",
                        ));
                    }
                };
                let store = store.clone();
                let prompt = user_prompt(&input.instruction, &input.source, &input.task);
                tokio::spawn(async move {
                    let output = model.generate(&prompt).await;
                    if output.is_err() {
                        eprintln!("approach.generation.failed run={run}");
                    }
                    if store
                        .finish_approach(scope, run, output.ok())
                        .await
                        .is_err()
                    {
                        eprintln!("approach.finish.failed run={run}");
                    }
                });
            }
            action @ ("accept" | "return") => {
                let Some((task, run)) = task.zip(canonical_uuid(form.get("run"))) else {
                    return Ok(plain(StatusCode::NOT_FOUND, "Запуск недоступен."));
                };
                if !store
                    .review_approach(scope, task, run, action == "accept")
                    .await?
                {
                    return Ok(plain(
                        StatusCode::CONFLICT,
                        "Решение уже принято или права изменились.",
                    ));
                }
            }
            _ => {
                return Ok(plain(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "Выберите действие.",
                ));
            }
        }
        return Ok(redirect(&target));
    }

    let setup = store.approach(scope).await?;
    let selected = task_query.and_then(|id| board.tasks.iter().find(|task| task.id == id));
    if task_query.is_some() && selected.is_none() {
        return Ok(plain(StatusCode::NOT_FOUND, "Задача недоступна."));
    }
    let runs = if let Some(task) = selected {
        store.approach_runs(scope, task.id).await?
    } else {
        Vec::new()
    };
    let (nodes, node) = flow_chart(
        &url,
        query.get("node"),
        setup.as_ref().is_none_or(|config| config.require_source),
        task_query,
    );
    let mut settings = match node.0 {
        "trigger" => selected.map_or_else(
            || String::from("<p>Выберите задачу над схемой.</p>"),
            |task| {
                format!(
                    r#"<p><a href="{}">{}</a> · {}</p>"#,
                    crate::tasks_http::task_path(route.tenant, route.space, task.id),
                    h(&task.title),
                    task.status.label()
                )
            },
        ),
        "action" => String::from(
            "<p>Модель получает выбранный документ и текст задачи. Черновик появится в журнале выполнения.</p>",
        ),
        "review" => String::from(
            r##"<p>Владелец пространства может принять черновик или вернуть его на доработку.</p><a href="#flow-runs">К журналу выполнения</a>"##,
        ),
        "output" => String::from(
            r##"<p>После согласования в разделе исходного документа появится страница со спецификацией.</p><a href="#flow-runs">К результатам запусков</a>"##,
        ),
        _ => String::new(),
    };
    if board.area.can_manage && matches!(node.0, "instruction" | "source" | "guard") {
        let pages = store
            .task_creation(scope)
            .await?
            .map(|creation| creation.sources)
            .unwrap_or_default();
        let options = pages
            .iter()
            .map(|source| {
                let selected = if setup
                    .as_ref()
                    .is_some_and(|config| config.source_page_id == source.id)
                {
                    " selected"
                } else {
                    ""
                };
                format!(
                    r#"<option value="{}"{selected}>{}</option>"#,
                    source.id,
                    h(&source.title)
                )
            })
            .collect::<String>();
        if pages.is_empty() {
            settings.push_str(
                "<p>Создайте документ, который агент сможет использовать как источник.</p>",
            );
        } else {
            let checked = if setup.as_ref().is_none_or(|config| config.require_source) {
                " checked"
            } else {
                ""
            };
            let instruction = setup.as_ref().map_or(
                "Проверь назначение, правила доступа и критерии приёмки.",
                |config| config.instruction.as_str(),
            );
            let task = selected.map_or_else(String::new, |task| task.id.to_string());
            let _ = write!(
                settings,
                r#"<form method="post" action="{url}" class="parity-approach-settings"><input type="hidden" name="action" value="configure"><input type="hidden" name="task" value="{task}"><input type="hidden" name="version" value="{}"><label class="parity-field">Инструкция<textarea name="instruction" rows="4" required maxlength="1500">{}</textarea></label><label class="parity-field">Источник<select name="source" required><option value="">Выберите документ</option>{options}</select></label><label class="parity-flow-check"><input type="checkbox" name="requireSource" value="1"{checked}> Требовать ссылку на выбранный источник в задаче</label><button class="space-action primary-action">Сохранить настройки</button></form>"#,
                setup.as_ref().map_or(0, |config| config.version),
                h(instruction)
            );
        }
    }
    let options = board
        .tasks
        .iter()
        .map(|task| {
            let mark = if selected.is_some_and(|current| current.id == task.id) {
                " selected"
            } else {
                ""
            };
            format!(
                r#"<option value="{}"{mark}>{}</option>"#,
                task.id,
                h(&task.title)
            )
        })
        .collect::<String>();
    let choose = format!(
        r#"<form action="{url}" class="parity-actions"><label>Задача <select class="parity-select" name="task" required><option value="">Выберите задачу</option>{options}</select></label><button class="space-action space-secondary">Открыть</button></form>"#
    );
    let mut log = String::new();
    let mut launch = String::new();
    let mut snapshot = String::new();
    if let Some(config) = &setup
        && matches!(node.0, "instruction" | "source" | "guard" | "action")
    {
        let source = page_path(route.tenant, route.space, config.source_page_id);
        let context = match selected {
            Some(task) if config.permits_task_source(task.source_page) => format!(
                r#"<p>Системная инструкция</p><pre class="parity-model-input">{}</pre><p>Запрос модели</p><pre class="parity-model-input">{}</pre>"#,
                h(SYSTEM_PROMPT),
                h(&user_prompt(
                    &config.instruction,
                    &config.source_excerpt,
                    &task.title
                )),
            ),
            Some(_) => format!("<p>{SOURCE_REQUIRED}</p>"),
            None => format!(
                r#"<p>Выберите задачу, чтобы увидеть полный запрос модели.</p><p>Инструкция</p><pre class="parity-model-input">{}</pre><p>Данные источника</p><pre class="parity-model-input">{}</pre>"#,
                h(&config.instruction),
                h(&config.source_excerpt),
            ),
        };
        snapshot = format!(
            r#"<details><summary>Глазами агента · версия {}</summary><p><a href="{source}">Источник, редакция {}</a> · до {SOURCE_EXCERPT_LIMIT} символов</p>{context}<p>Разрешено подготовить черновик и передать его на согласование.</p></details>"#,
            config.version, config.source_revision,
        );
    }
    if let Some(task) = selected {
        if board.area.can_manage && Model::configured().is_none() {
            launch.push_str("<p>Локальная модель не настроена.</p>");
        } else if board.area.can_manage
            && task.status != TaskStatus::Done
            && !runs
                .iter()
                .any(|run| matches!(run.status.as_str(), "running" | "review"))
            && let Some(config) = &setup
        {
            if config.permits_task_source(task.source_page) {
                let _ = write!(
                    launch,
                    r#"<form method="post" action="{url}"><input type="hidden" name="action" value="run"><input type="hidden" name="task" value="{}"><input type="hidden" name="operation" value="{}"><input type="hidden" name="taskVersion" value="{}"><input type="hidden" name="specVersion" value="{}"><input type="hidden" name="sourceRevision" value="{}"><button class="space-action primary-action">Запустить подход</button></form>"#,
                    task.id,
                    Uuid::new_v4(),
                    task.version,
                    config.version,
                    config.source_revision,
                );
            } else {
                let _ = write!(launch, "<p>{SOURCE_REQUIRED}</p>");
            }
        }
        for run in runs {
            let label = match run.status.as_str() {
                "running" => "Модель готовит черновик",
                "review" => "На согласовании",
                "accepted" => "Принято",
                "returned" => "Возвращено на доработку",
                "expired" => "Время запуска истекло. Подход можно запустить снова.",
                "superseded" => "Задача изменилась после запуска.",
                _ => "Не удалось подготовить черновик",
            };
            let output = run.output.map_or_else(String::new, |output| {
                format!(r#"<pre class="parity-run-output">{}</pre>"#, h(&output))
            });
            let source = page_path(route.tenant, route.space, run.source_page_id);
            let result = run.result_page_id.map_or_else(String::new, |id| {
                format!(
                    r#"<a href="{}">Открыть спецификацию</a>"#,
                    page_path(route.tenant, route.space, id)
                )
            });
            let review = if board.area.can_manage && run.status == "review" {
                format!(
                    r#"<form action="{url}" method="post" class="parity-actions"><input type="hidden" name="task" value="{}"><input type="hidden" name="run" value="{}"><button class="space-action primary-action" name="action" value="accept">Принять и создать страницу</button><button class="space-action space-secondary" name="action" value="return">Вернуть на доработку</button></form>"#,
                    task.id, run.id
                )
            } else {
                String::new()
            };
            let refresh = if run.status == "running" {
                format!(r#"<a href="{url}?task={}">Обновить журнал</a>"#, task.id)
            } else {
                String::new()
            };
            let _ = write!(
                log,
                r#"<article class="parity-surface parity-run"><h3>{label}</h3><p>Правила версии {} · <a href="{source}">Источник, редакция {}</a></p>{output}{review}{result}{refresh}</article>"#,
                run.spec_version, run.source_revision
            );
        }
    }
    if log.is_empty() {
        log.push_str("<p>Запусков по выбранной задаче пока нет.</p>");
    }
    let body = format!(
        r##"<section class="parity-area parity-approach"><p class="parity-flow-kicker">{} AI-подход</p><h1>Подготовка спецификации</h1>{choose}
<div class="parity-flow-layout">{nodes}
<section class="parity-surface parity-flow-settings" id="flow-properties" tabindex="-1" aria-labelledby="flow-step-title"><span class="parity-flow-kind" data-node="{}">{} {}</span><h2 id="flow-step-title">{}</h2>{settings}{snapshot}<a class="parity-flow-return" href="#flow-{}">К выбранному шагу</a></section></div>
<section class="parity-flow-log" id="flow-runs"><h2>Журнал выполнения</h2>{launch}{log}</section></section>"##,
        icon("flow"),
        node.0,
        icon(node.3),
        node.1,
        node.2,
        node.0
    );
    Ok(html(
        StatusCode::OK,
        page(
            "AI-подход",
            Some((route.tenant, &board.area.tenant_name)),
            Some((route.space, &board.area.space_name)),
            "approach",
            &body,
        ),
    ))
}

#[cfg(test)]
mod flow_tests {
    use super::*;

    #[test]
    fn flow_orders_the_optional_check_before_generation_and_preserves_the_task() {
        let task = Uuid::from_u128(19);
        for require_source in [true, false] {
            let (html, active) = flow_chart("/approach", "guard", require_source, Some(task));
            let steps: Vec<_> = html
                .split("data-node=\"")
                .skip(1)
                .map(|part| part.split('"').next().unwrap())
                .collect();
            let mut expected = vec!["trigger", "instruction", "source"];
            if require_source {
                expected.push("guard");
            }
            expected.extend(["action", "review", "output"]);
            assert_eq!(steps, expected);
            assert_eq!(
                active.0,
                if require_source {
                    "guard"
                } else {
                    "instruction"
                }
            );
            assert_eq!(html.matches("aria-current=\"step\"").count(), 1);
            assert_eq!(
                html.matches(&format!("task={task}#flow-properties"))
                    .count(),
                steps.len()
            );
        }
    }
}
