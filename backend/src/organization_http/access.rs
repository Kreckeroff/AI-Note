use axum::{
    extract::Request,
    http::{Method, StatusCode},
    response::Response,
};
use uuid::Uuid;

use super::{
    Panel, forbidden, form, invalid, not_found, organization_context, panel_path, short, views,
};
use crate::{
    db::{
        Principal, Store,
        organization::{MemberChange, Recipient, admin},
    },
    http::{Form, html, redirect},
    workspace::{WorkspaceContext, canonical_uuid, space_path},
};

pub(super) async fn members(
    store: &Store,
    ctx: &WorkspaceContext,
    query: Form,
) -> Result<String, sqlx::Error> {
    let filter = views::ListFilter::new(&query);
    let tenant = ctx.tenant.id;
    let rows = store
        .organization_members(
            tenant,
            &filter.query,
            &filter.role,
            &filter.department,
            filter.offset(),
        )
        .await?;
    let groups = store.organization_groups(tenant).await?;
    let departments = store.organization_departments(tenant).await?;
    let memberships = store
        .member_group_ids(tenant, &rows.iter().map(|row| row.id).collect::<Vec<_>>())
        .await?;
    Ok(views::members(
        ctx,
        &filter,
        rows,
        &groups,
        &departments,
        &memberships,
    ))
}

pub(super) async fn change_member(
    store: &Store,
    ctx: &WorkspaceContext,
    actor: &Principal,
    fields: Form,
) -> Result<Response, sqlx::Error> {
    let tenant = ctx.tenant.id;
    let path = panel_path(tenant, Panel::Members);
    if fields.get("action") == "group" {
        let Some(name) = short(fields.get("name"), 1) else {
            return Ok(invalid("Введите имя группы."));
        };
        return Ok(
            if store.add_organization_group(tenant, actor.id, name).await? {
                redirect(&path)
            } else {
                forbidden()
            },
        );
    }
    let Some(member) = canonical_uuid(fields.get("account")) else {
        return Ok(invalid("Выберите участника."));
    };
    let saved = match fields.get("action") {
        "create" => {
            store
                .change_organization_member(
                    tenant,
                    actor.id,
                    member,
                    MemberChange::CanCreate(fields.get("value") == "yes"),
                )
                .await?
        }
        "administrator" => {
            store
                .change_organization_member(
                    tenant,
                    actor.id,
                    member,
                    MemberChange::Administrator(fields.get("value") == "yes"),
                )
                .await?
        }
        "department" => {
            let Some(department) = short(fields.get("value"), 0) else {
                return Ok(invalid("Проверьте название отдела."));
            };
            store
                .change_organization_member(
                    tenant,
                    actor.id,
                    member,
                    MemberChange::Department(department),
                )
                .await?
        }
        "group-member" => {
            let Some(group) = canonical_uuid(fields.get("group")) else {
                return Ok(invalid("Выберите группу."));
            };
            store
                .change_organization_group_member(
                    tenant,
                    actor.id,
                    group,
                    member,
                    fields.get("value") == "yes",
                )
                .await?
        }
        _ => return Ok(invalid("Неизвестное действие.")),
    };
    Ok(if saved { redirect(&path) } else { forbidden() })
}

pub(super) async fn manage(
    store: &Store,
    actor: &Principal,
    tenant: Uuid,
    space: Option<Uuid>,
    request: Request,
) -> Result<Response, sqlx::Error> {
    let Some(ctx) = organization_context(store, tenant, actor).await? else {
        return Ok(not_found());
    };
    let managed = space.and_then(|space| ctx.available_spaces.iter().find(|row| row.id == space));
    let is_admin = admin(&ctx.tenant.role);
    if match space {
        Some(_) => !managed.is_some_and(|row| is_admin || row.is_owner),
        None => !is_admin,
    } {
        return Ok(not_found());
    }
    let path = space.map_or_else(
        || panel_path(tenant, Panel::Access),
        |space| format!("{}/access", space_path(tenant, space)),
    );
    if request.method() == Method::POST {
        let Some(space) = space else {
            return Ok(forbidden());
        };
        let fields = match form(request).await {
            Ok(fields) => fields,
            Err(response) => return Ok(response),
        };
        let recipient = match fields.get("kind") {
            "person" => canonical_uuid(fields.get("recipient")).map(Recipient::Person),
            "group" => canonical_uuid(fields.get("recipient")).map(Recipient::Group),
            "department" => short(fields.get("recipient"), 1).map(Recipient::Department),
            _ => None,
        };
        let Some(recipient) = recipient else {
            return Ok(invalid("Выберите получателя."));
        };
        let role = match fields.get("role") {
            "editor" => Some("editor"),
            "viewer" => Some("viewer"),
            "remove" => None,
            _ => return Ok(invalid("Выберите роль.")),
        };
        if store
            .change_space_grant(tenant, space, actor.id, recipient, role)
            .await?
        {
            return Ok(redirect(&path));
        }
        return Ok(forbidden());
    }
    let filter =
        views::ListFilter::new(&Form::from_query(request.uri().query().unwrap_or_default()));
    let grants = store
        .organization_space_grants(
            tenant,
            space,
            &filter.query,
            &filter.role,
            &filter.source,
            filter.offset(),
        )
        .await?;
    let people = if space.is_some() {
        store
            .organization_members(tenant, &filter.person, "", "", 0)
            .await?
    } else {
        Vec::new()
    };
    let groups = if space.is_some() && is_admin {
        store.organization_groups(tenant).await?
    } else {
        Vec::new()
    };
    let departments = if space.is_some() && is_admin {
        store.organization_departments(tenant).await?
    } else {
        Vec::new()
    };
    let rules = match space {
        Some(space) => store.space_grant_rules(tenant, space).await?,
        None => Vec::new(),
    };
    Ok(html(
        StatusCode::OK,
        views::space_access(
            &ctx,
            managed,
            &path,
            &filter,
            grants,
            &people,
            &groups,
            &departments,
            &rules,
        )?,
    ))
}
