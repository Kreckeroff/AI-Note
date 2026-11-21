mod access;
mod account;
mod views;

use axum::{
    body::to_bytes,
    extract::Request,
    http::{HeaderValue, Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use uuid::Uuid;

use crate::{
    cover_image::MAX_COVER_BYTES,
    db::{Principal, Store, organization::admin},
    http::{self, Form, html, plain, redirect},
    identity::valid_token,
    text::js_trim,
    workspace::{CatalogOptions, WorkspaceContext, canonical_uuid},
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Panel {
    Home,
    Settings,
    Members,
    Invitations,
    Policies,
    Access,
    Logo,
    Switch,
}

pub(crate) enum OrganizationRoute {
    Tenant(Uuid, Panel),
    SpaceAccess { tenant: Uuid, space: Uuid },
    Invite(String),
    Account,
}

impl OrganizationRoute {
    pub(crate) fn parse(path: &str) -> Option<Self> {
        let parts = path
            .strip_prefix("/workspace/")?
            .split('/')
            .collect::<Vec<_>>();
        match parts.as_slice() {
            ["profile"] => Some(Self::Account),
            ["organization-invite", ticket] if valid_token(ticket) => {
                Some(Self::Invite((*ticket).to_owned()))
            }
            [tenant, "spaces", space, "access"] => Some(Self::SpaceAccess {
                tenant: canonical_uuid(tenant)?,
                space: canonical_uuid(space)?,
            }),
            [tenant, "organization"] => Some(Self::Tenant(canonical_uuid(tenant)?, Panel::Home)),
            [tenant, "organization", panel] => Some(Self::Tenant(
                canonical_uuid(tenant)?,
                match *panel {
                    "settings" => Panel::Settings,
                    "members" => Panel::Members,
                    "invitations" | "invites" => Panel::Invitations,
                    "policies" => Panel::Policies,
                    "access" => Panel::Access,
                    "logo" => Panel::Logo,
                    "switch" => Panel::Switch,
                    _ => return None,
                },
            )),
            _ => None,
        }
    }

    pub(crate) fn allows_anonymous(&self) -> bool {
        matches!(self, Self::Invite(_))
    }

    pub(crate) fn allows_read(&self) -> bool {
        !matches!(self, Self::Tenant(_, Panel::Switch))
    }

    pub(crate) fn allows_post(&self) -> bool {
        !matches!(self, Self::Tenant(_, Panel::Home))
    }
}

pub(crate) fn organization_path(tenant: Uuid) -> String {
    format!("/workspace/{tenant}/organization")
}

pub(crate) fn organization_logo_path(tenant: Uuid) -> String {
    format!("{}/logo", organization_path(tenant))
}

pub(crate) fn asset(path: &str) -> Option<(&'static str, &'static str)> {
    match path {
        "/organization.css" => Some((
            "text/css; charset=utf-8",
            include_str!("../../app/organization.css"),
        )),
        "/organization.mjs" => Some((
            "text/javascript; charset=utf-8",
            include_str!("../../app/organization.mjs"),
        )),
        _ => None,
    }
}

fn panel_path(tenant: Uuid, panel: Panel) -> String {
    let home = organization_path(tenant);
    match panel {
        Panel::Home => home,
        _ => format!(
            "{home}/{}",
            match panel {
                Panel::Settings => "settings",
                Panel::Members => "members",
                Panel::Invitations => "invitations",
                Panel::Policies => "policies",
                Panel::Access => "access",
                Panel::Logo => "logo",
                Panel::Switch => "switch",
                Panel::Home => unreachable!(),
            }
        ),
    }
}

fn not_found() -> Response {
    plain(StatusCode::NOT_FOUND, "Страница не найдена.")
}

fn forbidden() -> Response {
    plain(
        StatusCode::FORBIDDEN,
        "Для этого изменения нужны дополнительные права.",
    )
}

fn invalid(message: &'static str) -> Response {
    plain(StatusCode::UNPROCESSABLE_ENTITY, message)
}

fn short(value: &str, min: usize) -> Option<&str> {
    let value = js_trim(value);
    (value.chars().count() >= min
        && value.chars().count() <= 120
        && !value.chars().any(char::is_control))
    .then_some(value)
}

async fn form(request: Request) -> Result<Form, Response> {
    http::read_form_bounded(request, 8192)
        .await
        .map_err(|error| http::form_error(error, "Откройте форму в «Севере»."))
}

async fn organization_context(
    store: &Store,
    tenant: Uuid,
    actor: &Principal,
) -> Result<Option<WorkspaceContext>, sqlx::Error> {
    let (data, _) = store.workspace(actor.id, Some(tenant)).await?;
    Ok(data.map(|data| WorkspaceContext::build(data, actor, CatalogOptions::parse(""), true)))
}

pub(crate) async fn handle(
    store: &Store,
    principal: Option<&Principal>,
    route: OrganizationRoute,
    origin: &str,
    request: Request,
) -> Result<Response, sqlx::Error> {
    let is_post = request.method() == Method::POST;
    if !matches!(*request.method(), Method::GET | Method::HEAD | Method::POST)
        || is_post && !route.allows_post()
        || !is_post && !route.allows_read()
    {
        return Ok(http::method_not_allowed(
            match (route.allows_read(), route.allows_post()) {
                (true, true) => "GET, HEAD, POST",
                (true, false) => "GET, HEAD",
                _ => "POST",
            },
        ));
    }
    if is_post && !http::check_origin(request.headers(), origin) {
        return Ok(forbidden());
    }
    if let OrganizationRoute::Invite(ticket) = route {
        return account::accept_invitation(store, principal, &ticket, request).await;
    }
    let Some(actor) = principal else {
        return Ok(redirect("/"));
    };
    let (tenant, panel) = match route {
        OrganizationRoute::Account => return account::profile(store, actor, request).await,
        OrganizationRoute::SpaceAccess { tenant, space } => {
            return access::manage(store, actor, tenant, Some(space), request).await;
        }
        OrganizationRoute::Tenant(tenant, panel) => (tenant, panel),
        OrganizationRoute::Invite(_) => unreachable!(),
    };
    if panel == Panel::Switch {
        return if store.switch_organization(actor.id, tenant).await? {
            Ok(redirect(&organization_path(tenant)))
        } else {
            Ok(not_found())
        };
    }
    let Some(ctx) = organization_context(store, tenant, actor).await? else {
        return Ok(not_found());
    };
    if !matches!(panel, Panel::Home | Panel::Logo) && !admin(&ctx.tenant.role) {
        return Ok(not_found());
    }
    match panel {
        Panel::Access => return access::manage(store, actor, tenant, None, request).await,
        Panel::Invitations => {
            return account::invitations(store, &ctx, actor, origin, request).await;
        }
        Panel::Logo if !is_post => return logo(store, tenant, actor).await,
        Panel::Logo => return update_logo(store, tenant, actor, &ctx, request).await,
        _ => (),
    }
    if is_post {
        let values = match form(request).await {
            Ok(values) => values,
            Err(response) => return Ok(response),
        };
        let saved = match panel {
            Panel::Settings => {
                let Some(name) = short(values.get("name"), 1) else {
                    let settings = store.organization_settings(tenant).await?;
                    return Ok(html(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        views::settings(
                            &ctx,
                            &settings,
                            Some(values.get("name")),
                            Some("Введите название от 1 до 120 символов."),
                        ),
                    ));
                };
                store.rename_organization(tenant, actor.id, name).await?
            }
            Panel::Policies => {
                store
                    .update_organization_policies(
                        tenant,
                        actor.id,
                        values.get("guests") == "yes",
                        values.get("public") == "yes",
                    )
                    .await?
            }
            Panel::Members => return access::change_member(store, &ctx, actor, values).await,
            _ => false,
        };
        return Ok(if saved {
            redirect(&panel_path(tenant, panel))
        } else {
            forbidden()
        });
    }
    let query = Form::from_query(request.uri().query().unwrap_or_default());
    let settings = store.organization_settings(tenant).await?;
    let markup = match panel {
        Panel::Home => views::home(&ctx, &settings),
        Panel::Settings => views::settings(&ctx, &settings, None, None),
        Panel::Members => access::members(store, &ctx, query).await?,
        Panel::Policies => views::policies(&ctx, &settings),
        _ => return Ok(not_found()),
    };
    let mut response = html(StatusCode::OK, markup);
    if panel == Panel::Settings {
        // A logo is previewed from a local blob only on this settings page.
        response.headers_mut().insert(header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static("default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' blob:; connect-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'"));
    }
    Ok(response)
}

async fn logo(store: &Store, tenant: Uuid, actor: &Principal) -> Result<Response, sqlx::Error> {
    if let Some(image) = store.organization_logo(tenant, actor.id).await? {
        return Ok(([(header::CONTENT_TYPE, "image/jpeg")], image).into_response());
    }
    let icon = crate::workspace_chrome::icon("north");
    let icon = icon
        .replacen("<svg", "<svg xmlns=\"http://www.w3.org/2000/svg\"", 1)
        .replace("currentColor", "#9baaf9");
    Ok(([(header::CONTENT_TYPE, "image/svg+xml")], icon).into_response())
}

async fn update_logo(
    store: &Store,
    tenant: Uuid,
    actor: &Principal,
    ctx: &WorkspaceContext,
    request: Request,
) -> Result<Response, sqlx::Error> {
    if ctx.tenant.role != "superadmin" {
        return Ok(forbidden());
    }
    let error = match http::media_type(request.headers()) {
        Some("application/x-www-form-urlencoded") => {
            let values = match form(request).await {
                Ok(values) => values,
                Err(response) => return Ok(response),
            };
            if values.get("action") != "clear" {
                return Ok(invalid("Проверьте действие с логотипом."));
            }
            if store
                .update_organization_logo(tenant, actor.id, None)
                .await?
            {
                return Ok(redirect(&panel_path(tenant, Panel::Settings)));
            }
            return Ok(forbidden());
        }
        Some("image/png" | "image/jpeg" | "image/webp" | "application/octet-stream") => {
            match to_bytes(request.into_body(), MAX_COVER_BYTES).await {
                Ok(bytes) => match views::normalize_logo(bytes.to_vec()).await {
                    Ok(image) => {
                        if store
                            .update_organization_logo(tenant, actor.id, Some(image))
                            .await?
                        {
                            return Ok(redirect(&panel_path(tenant, Panel::Settings)));
                        }
                        return Ok(forbidden());
                    }
                    Err(message) => message,
                },
                Err(_) => "Размер файла не должен превышать 5 МБ.",
            }
        }
        _ => "Выберите PNG, JPEG или WebP.",
    };
    Ok(invalid(error))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_tenant_routes_keep_scope_and_methods() {
        let tenant = Uuid::new_v4();
        let space = Uuid::new_v4();
        let home = OrganizationRoute::parse(&organization_path(tenant)).unwrap();
        assert!(home.allows_read() && !home.allows_post() && !home.allows_anonymous());
        let switch = OrganizationRoute::parse(&panel_path(tenant, Panel::Switch)).unwrap();
        assert!(!switch.allows_read() && switch.allows_post());
        assert!(matches!(
            OrganizationRoute::parse(&format!("/workspace/{tenant}/spaces/{space}/access")),
            Some(OrganizationRoute::SpaceAccess {tenant: requested, space: selected}) if requested==tenant && selected==space
        ));
        assert!(
            OrganizationRoute::parse(&format!("/workspace/{tenant}/organization/unknown"))
                .is_none()
        );
        assert!(OrganizationRoute::parse("/workspace/organization-invite/invalid").is_none());
        assert!(
            OrganizationRoute::parse(&format!(
                "/workspace/organization-invite/{}",
                "a".repeat(43)
            ))
            .unwrap()
            .allows_anonymous()
        );
    }
}
