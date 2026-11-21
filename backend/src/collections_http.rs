use crate::{
    db::{
        Principal, Store,
        collections::{Change, Outcome, PAGE_SIZE, Properties, View, ViewTarget},
        tasks::TaskScope,
    },
    http::{self, Form, html, plain, redirect},
    workspace::{canonical_uuid, space_path},
    workspace_chrome,
};
use axum::{
    extract::Request,
    http::{Method, StatusCode},
    response::Response,
};
use uuid::Uuid;

mod forms;
pub(crate) const STYLES: &str = include_str!("collections_http/collections.css");

pub(crate) struct Route {
    tenant: Uuid,
    space: Uuid,
    target: ViewTarget,
}
impl Route {
    pub(crate) fn parse(path: &str) -> Option<Self> {
        let mut p = path.strip_prefix("/workspace/")?.split('/');
        let tenant = canonical_uuid(p.next()?)?;
        if p.next() != Some("spaces") {
            return None;
        }
        let space = canonical_uuid(p.next()?)?;
        if p.next() != Some("collections") {
            return None;
        }
        let target = match p.next() {
            None => ViewTarget::Catalog,
            Some(raw) => {
                let c = canonical_uuid(raw)?;
                match p.next() {
                    None => ViewTarget::Table(c),
                    Some("records") => match p.next()? {
                        "new" => ViewTarget::NewRecord(c),
                        id => ViewTarget::Record(c, canonical_uuid(id)?),
                    },
                    _ => return None,
                }
            }
        };
        p.next().is_none().then_some(Self {
            tenant,
            space,
            target,
        })
    }
    fn home(&self) -> String {
        format!("{}/collections", space_path(self.tenant, self.space))
    }
    fn current(&self) -> String {
        let home = self.home();
        match self.target {
            ViewTarget::Catalog => home,
            ViewTarget::Table(c) => format!("{home}/{c}"),
            ViewTarget::NewRecord(c) => format!("{home}/{c}/records/new"),
            ViewTarget::Record(c, r) => format!("{home}/{c}/records/{r}"),
        }
    }
}

fn missing() -> Response {
    plain(StatusCode::NOT_FOUND, "Реестр или запись недоступны.")
}
fn forbidden() -> Response {
    plain(
        StatusCode::FORBIDDEN,
        "Чтобы изменить реестр, нужны права редактора пространства.",
    )
}

pub(crate) async fn handle(
    store: &Store,
    principal: &Principal,
    route: Route,
    origin: &str,
    request: Request,
) -> Result<Response, sqlx::Error> {
    let scope = TaskScope {
        tenant: route.tenant,
        space: route.space,
        account: principal.id,
    };
    let query = Form::from_query(request.uri().query().unwrap_or_default());
    let page = query.get("page").parse::<u32>().unwrap_or(1).max(1);
    match *request.method() {
        Method::GET | Method::HEAD => {
            let Some(view) = store
                .collection_view(scope, route.target, i64::from(page - 1) * PAGE_SIZE)
                .await?
            else {
                return Ok(missing());
            };
            if matches!(route.target, ViewTarget::NewRecord(_)) && !view.editable {
                return Ok(forbidden());
            }
            Ok(render(&route, &view, page, None, None))
        }
        Method::POST => {
            if !http::check_origin(request.headers(), origin) {
                return Ok(plain(StatusCode::FORBIDDEN, "Откройте форму в «Севере»."));
            }
            let form = match http::read_form_bounded(request, 512 * 1024).await {
                Ok(f) => f,
                Err(e) => return Ok(http::form_error(e, "Используйте форму реестра.")),
            };
            let Some(view) = store.collection_view(scope, route.target, 0).await? else {
                return Ok(missing());
            };
            if !view.editable {
                return Ok(forbidden());
            }
            let Some(change) = parse_change(route.target, &view, &form) else {
                return Ok(render(
                    &route,
                    &view,
                    1,
                    Some(&form),
                    Some((
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "Проверьте поля формы и повторите сохранение.",
                    )),
                ));
            };
            let outcome = store.change_collection(scope, &change).await?;
            let warning = match outcome {
                Outcome::Saved => {
                    let destination = match change {
                        Change::Create { id, .. } => format!("{}/{id}", route.home()),
                        Change::AddField { .. } => route.current(),
                        Change::Record { collection, id, .. } => {
                            format!("{}/{collection}/records/{id}", route.home())
                        }
                    };
                    return Ok(redirect(&destination));
                }
                Outcome::Missing => return Ok(missing()),
                Outcome::Forbidden => return Ok(forbidden()),
                Outcome::Duplicate => (
                    StatusCode::CONFLICT,
                    "Это название уже используется. Введите другое.",
                ),
                Outcome::Invalid => (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "Проверьте длину названия и значения полей.",
                ),
                Outcome::FieldLimit => (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "В реестре уже есть 32 поля.",
                ),
                Outcome::Conflict => (
                    StatusCode::CONFLICT,
                    "Данные изменились. Ваш ввод сохранён в форме. Сверьте его с текущей версией перед повторным сохранением.",
                ),
            };
            let Some(current) = store.collection_view(scope, route.target, 0).await? else {
                return Ok(missing());
            };
            if !current.editable {
                return Ok(forbidden());
            }
            Ok(render(&route, &current, 1, Some(&form), Some(warning)))
        }
        _ => Ok(http::method_not_allowed("GET, HEAD, POST")),
    }
}

fn parse_change(target: ViewTarget, view: &View, f: &Form) -> Option<Change> {
    let title = f.field("title").ok()??.to_owned();
    match target {
        ViewTarget::Catalog => Some(Change::Create {
            id: canonical_uuid(f.get("operation"))?,
            title,
        }),
        ViewTarget::Table(collection) => Some(Change::AddField {
            collection,
            id: canonical_uuid(f.get("operation"))?,
            title,
            kind: f.get("kind").to_owned(),
        }),
        ViewTarget::NewRecord(collection) | ViewTarget::Record(collection, _) => {
            let (id, revision) = match target {
                ViewTarget::Record(_, id) => {
                    (id, Some(crate::text::revision_number(f.get("version"))?))
                }
                _ => (canonical_uuid(f.get("operation"))?, None),
            };
            let mut values = Properties::new();
            for field in &view.fields {
                let key = field.id.to_string();
                if let Some(value) = f.field(&key).ok()?.filter(|v| !v.is_empty()) {
                    let canonical = if field.kind == "text" {
                        crate::text::normalize_line_endings(value).into_owned()
                    } else {
                        value.to_owned()
                    };
                    values.insert(key, canonical);
                }
            }
            Some(Change::Record {
                collection,
                id,
                revision,
                schema: crate::text::revision_number(f.get("schema"))?,
                title,
                values,
            })
        }
    }
}

fn render(
    route: &Route,
    view: &View,
    page: u32,
    form: Option<&Form>,
    warning: Option<(StatusCode, &str)>,
) -> Response {
    let (title, body) = forms::body(route, view, page, form, warning);
    html(
        warning.map_or(StatusCode::OK, |(code, _)| code),
        workspace_chrome::page(
            &title,
            Some((route.tenant, &view.tenant_name)),
            Some((route.space, &view.space_name)),
            "collections",
            &body,
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::collections::{Collection, Field, Record};

    #[test]
    fn invalid_then_corrected_form_preserves_stale_baselines_and_multiline_text() {
        let id = Uuid::new_v4();
        let field = Uuid::new_v4();
        let collection = Uuid::new_v4();
        let view = View {
            tenant_name: "Среда".into(),
            space_name: "Команда".into(),
            editable: true,
            collections: vec![Collection {
                id: collection,
                title: "Реестр".into(),
                schema_version: 3,
            }],
            fields: vec![Field {
                id: field,
                title: "Описание".into(),
                kind: "text".into(),
            }],
            records: vec![Record {
                id,
                title: "Актуально".into(),
                version: 8,
                values: Properties::new(),
            }],
            has_more: false,
        };
        let route = Route {
            tenant: Uuid::new_v4(),
            space: Uuid::new_v4(),
            target: ViewTarget::Record(collection, id),
        };
        let submitted = Form::from_query(&format!(
            "title=&schema=2&version=7&{field}=%0D%0Aabc%0D%0Adef"
        ));
        let (_, invalid) = forms::body(
            &route,
            &view,
            1,
            Some(&submitted),
            Some((StatusCode::UNPROCESSABLE_ENTITY, "Проверьте название.")),
        );
        assert!(invalid.contains(r#"name="version" value="7""#));
        assert!(invalid.contains(r#"name="schema" value="2""#));
        assert!(!invalid.contains("Сохранить мою версию"));
        let corrected = Form::from_query(&format!(
            "title=OK&schema=2&version=7&{field}=%0D%0Aabc%0D%0Adef"
        ));
        let Some(Change::Record {
            revision,
            schema,
            values,
            ..
        }) = parse_change(route.target, &view, &corrected)
        else {
            panic!("expected a record")
        };
        assert_eq!(revision, Some(7));
        assert_eq!(schema, 2);
        assert_eq!(values[&field.to_string()], "\nabc\ndef");
        let (_, conflict) = forms::body(
            &route,
            &view,
            1,
            Some(&corrected),
            Some((StatusCode::CONFLICT, "Данные изменились.")),
        );
        assert!(conflict.contains(r#"name="version" value="8""#));
        assert!(conflict.contains(r#"name="schema" value="3""#));
        assert!(conflict.contains("Сохранить мою версию"));
        assert!(conflict.contains(&format!("name=\"{field}\">\n\nabc\ndef</textarea>")));
    }
}
