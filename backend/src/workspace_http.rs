use axum::{
    extract::Request,
    http::{Method, StatusCode},
    response::Response,
};
use uuid::Uuid;

use crate::{
    db::{CreateSpace, Principal, Store},
    http::{self, html, plain, redirect},
    sections_http, views,
    workspace::{
        CatalogOptions, Route, WorkspaceContext, canonical_uuid, space_path, with_options,
    },
};

const CREATION_DENIED: &str = "У вас нет права создавать пространство в этой среде.";

pub(crate) async fn handle(
    store: &Store,
    route: Route,
    principal: &Principal,
    origin: &str,
    request: Request,
) -> Result<Response, sqlx::Error> {
    if let Route::Documents(documents) = route {
        return sections_http::handle(store, documents, principal, origin, request).await;
    }
    let (data, tenants) = store.workspace(principal.id, route.tenant()).await?;
    let Some(data) = data else {
        if matches!(route, Route::Current) {
            return Ok(html(
                StatusCode::OK,
                views::no_tenant(&principal.name, &tenants),
            ));
        }
        record("space.open", principal.id, route.tenant(), "not-found");
        return Ok(not_found());
    };
    let options = CatalogOptions::parse(request.uri().query().unwrap_or_default());
    let creating = request.method() == Method::POST;
    let ctx = WorkspaceContext::build(data, principal, options.clone(), true);
    let response = match route {
        Route::Current | Route::Collection(_) if !creating => {
            html(StatusCode::OK, views::catalog(&ctx))
        }
        _ if !ctx.tenant.can_create => {
            record(
                if creating {
                    "space.create"
                } else {
                    "space.open"
                },
                principal.id,
                Some(ctx.tenant.id),
                "forbidden",
            );
            plain(StatusCode::FORBIDDEN, CREATION_DENIED)
        }
        _ if creating => submit(store, principal, origin, ctx.tenant.id, &options, request).await,
        _ => html(
            StatusCode::OK,
            views::create_space(&ctx, Uuid::new_v4(), "", "", false, false),
        ),
    };
    Ok(response)
}

async fn submit(
    store: &Store,
    principal: &Principal,
    origin: &str,
    tenant: Uuid,
    options: &CatalogOptions,
    request: Request,
) -> Response {
    if !http::check_origin(request.headers(), origin) {
        return plain(
            StatusCode::FORBIDDEN,
            "Откройте форму создания пространства в «Севере».",
        );
    }
    let form = match http::read_form(request).await {
        Ok(form) => form,
        Err(error) => {
            return http::form_error(error, "Откройте форму создания пространства в «Севере».");
        }
    };
    let account = principal.id;
    let name = form.get("name");
    let Some(operation) = canonical_uuid(form.get("operation_id")) else {
        let error = CreationError {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            message: "Не удалось подтвердить отправку. Откройте новую форму.",
            invalid_name: false,
            new_form: true,
        };
        return render_error(
            store,
            principal,
            tenant,
            options,
            Uuid::new_v4(),
            name,
            error,
        )
        .await;
    };
    let result = store.create_space(tenant, account, operation, name).await;
    let (status, message, invalid_name, new_form, outcome) = match result {
        Ok(CreateSpace::Saved(id)) => {
            record("space.create", account, Some(tenant), "saved");
            return redirect(&with_options(&space_path(tenant, id), options));
        }
        Ok(CreateSpace::Forbidden) => {
            record("space.create", account, Some(tenant), "forbidden");
            return plain(StatusCode::FORBIDDEN, CREATION_DENIED);
        }
        Ok(CreateSpace::MissingMembership) => {
            record("space.create", account, Some(tenant), "not-found");
            return not_found();
        }
        Ok(CreateSpace::Duplicate) => (
            StatusCode::CONFLICT,
            "У вас уже есть пространство с таким названием.",
            true,
            false,
            "duplicate",
        ),
        Ok(CreateSpace::EmptyName) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "Введите название пространства.",
            true,
            false,
            "empty-name",
        ),
        Ok(CreateSpace::LongName) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "Введите не больше 100 символов.",
            true,
            false,
            "long-name",
        ),
        Ok(CreateSpace::OperationConflict) => (
            StatusCode::CONFLICT,
            "Эта отправка уже использована. Откройте новую форму.",
            false,
            true,
            "operation-conflict",
        ),
        Err(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "Не удалось сохранить пространство. Попробуйте ещё раз.",
            false,
            false,
            "unavailable",
        ),
    };
    record("space.create", account, Some(tenant), outcome);
    render_error(
        store,
        principal,
        tenant,
        options,
        operation,
        name,
        CreationError {
            status,
            message,
            invalid_name,
            new_form,
        },
    )
    .await
}

struct CreationError {
    status: StatusCode,
    message: &'static str,
    invalid_name: bool,
    new_form: bool,
}

async fn render_error(
    store: &Store,
    principal: &Principal,
    tenant: Uuid,
    options: &CatalogOptions,
    operation: Uuid,
    name: &str,
    error: CreationError,
) -> Response {
    let ctx = match store.workspace(principal.id, Some(tenant)).await {
        Ok((Some(data), _)) if data.tenant.can_create => {
            WorkspaceContext::build(data, principal, options.clone(), true)
        }
        Ok((Some(_), _)) => return plain(StatusCode::FORBIDDEN, CREATION_DENIED),
        Ok((None, _)) => return not_found(),
        Err(_) => {
            return plain(
                StatusCode::SERVICE_UNAVAILABLE,
                "Не удалось открыть форму создания пространства.",
            );
        }
    };
    html(
        error.status,
        views::create_space(
            &ctx,
            operation,
            name,
            error.message,
            error.invalid_name,
            error.new_form,
        ),
    )
}

fn not_found() -> Response {
    html(StatusCode::NOT_FOUND, views::space_not_found())
}

fn record(event: &str, account: Uuid, tenant: Option<Uuid>, outcome: &str) {
    eprintln!(
        "{event} account={account} tenant={} outcome={outcome}",
        tenant.unwrap_or(Uuid::nil())
    );
}
