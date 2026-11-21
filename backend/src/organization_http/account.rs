use axum::{
    extract::Request,
    http::{
        HeaderValue, Method, StatusCode,
        header::{REFERRER_POLICY, SET_COOKIE},
    },
    response::Response,
};
use tokio::sync::Semaphore;

use super::{
    Panel, forbidden, form, invalid, not_found, organization_path, panel_path, short, views,
};
use crate::{
    clock::now_ms,
    db::{Principal, Store},
    external_sharing::private_address,
    http::{Form, html, redirect},
    identity::{
        normalize_email, prepare_account, prepare_session, random_token, session_cookie,
        session_digest, token_digest,
    },
    registration::MailDelivery,
    text::escape_html as esc,
    workspace::WorkspaceContext,
    workspace_chrome::page,
};

static PASSWORDS: Semaphore = Semaphore::const_new(2);
const INVALID_INVITE: &str = "Ссылка недействительна или не подходит для этой учётной записи.";
const MAIL: &str = "Почтовая доставка приглашений не настроена.";

pub(super) async fn invitations(
    store: &Store,
    ctx: &WorkspaceContext,
    actor: &Principal,
    origin: &str,
    request: Request,
) -> Result<Response, sqlx::Error> {
    let tenant = ctx.tenant.id;
    let path = panel_path(tenant, Panel::Invitations);
    let mail = MailDelivery::configured("SEVER_SHARING_SENDMAIL", "SEVER_SHARING_FROM").ok();
    if request.method() == Method::POST {
        let fields = match form(request).await {
            Ok(fields) => fields,
            Err(response) => return Ok(response),
        };
        if fields.get("action") == "revoke" {
            let Some(id) = crate::workspace::canonical_uuid(fields.get("invitation")) else {
                return Ok(invalid("Выберите приглашение."));
            };
            if store
                .revoke_organization_invitation(tenant, actor.id, id, now_ms())
                .await?
            {
                return Ok(redirect(&path));
            }
            return Ok(not_found());
        }
        if fields.get("action") != "invite" {
            return Ok(invalid("Неизвестное действие."));
        }
        let Some(mail) = mail else {
            return Ok(invalid(MAIL));
        };
        let Some(email) = normalize_email(fields.get("email")) else {
            return Ok(invalid("Проверьте адрес электронной почты."));
        };
        let token = random_token();
        let Some(invitation) = store
            .issue_organization_invitation(
                tenant,
                actor.id,
                &email,
                &token_digest(&token),
                now_ms(),
            )
            .await?
        else {
            return Ok(invalid(
                "По этому адресу уже есть участник или действующее приглашение.",
            ));
        };
        let title = &ctx.tenant.name;
        let body = format!(
            "Вас пригласили в «{title}». Откройте ссылку, чтобы принять приглашение.\n{origin}/workspace/organization-invite/{token}\nСсылка действительна 7 дней.\n"
        );
        let sent = mail
            .send_message(&email, "Приглашение в «Север»", &body, Some(invitation))
            .await
            .is_ok();
        store
            .finish_organization_invitation_delivery(tenant, invitation, sent, now_ms())
            .await?;
        return Ok(if sent {
            redirect(&path)
        } else {
            invalid("Не удалось доставить письмо. Попробуйте отправить приглашение снова.")
        });
    }
    let query = Form::from_query(request.uri().query().unwrap_or_default());
    let filter = views::ListFilter::new(&query);
    let rows = store
        .organization_invitations(tenant, &filter.query, filter.offset())
        .await?;
    Ok(html(
        StatusCode::OK,
        views::invitations(ctx, mail.is_some(), &filter, rows),
    ))
}

pub(super) async fn accept_invitation(
    store: &Store,
    principal: Option<&Principal>,
    ticket: &str,
    request: Request,
) -> Result<Response, sqlx::Error> {
    let digest = token_digest(ticket);
    let Some(invite) = store
        .organization_invitation_access(&digest, now_ms(), principal.map(|actor| actor.id))
        .await?
    else {
        return Ok(private_address(not_found()));
    };
    let invite_path = format!("/workspace/organization-invite/{ticket}");
    let target = organization_path(invite.tenant_id);
    let result = if request.method() == Method::POST {
        let fields = match form(request).await {
            Ok(fields) => fields,
            Err(response) => return Ok(private_address(response)),
        };
        if let Some(actor) = principal {
            if store
                .join_organization_invitation(invite.tenant_id, &digest, now_ms(), actor.id, None)
                .await?
            {
                redirect(&target)
            } else {
                invalid(INVALID_INVITE)
            }
        } else if invite.account_exists {
            invalid("Войдите в приглашённую учётную запись.")
        } else {
            let Ok(permit) = PASSWORDS.try_acquire() else {
                return Ok(private_address(invalid(
                    "Регистрация занята. Повторите позже.",
                )));
            };
            let name = fields.get("name").to_owned();
            let password = fields.get("password").to_owned();
            let email = invite.email.clone();
            let prepared = tokio::task::spawn_blocking(move || {
                let _permit = permit;
                prepare_account(&email, &name, &password)
            })
            .await;
            let account = match prepared {
                Ok(Ok(account)) => account,
                Ok(Err(error)) => {
                    return Ok(invitation_response(invite_form(
                        &invite,
                        &invite_path,
                        None,
                        Some(error),
                    )));
                }
                Err(_) => {
                    return Ok(private_address(invalid(
                        "Не удалось подготовить учётную запись.",
                    )));
                }
            };
            let (token, session) = prepare_session(now_ms());
            if store
                .join_organization_invitation(
                    invite.tenant_id,
                    &digest,
                    now_ms(),
                    account.id,
                    Some((&account, &session)),
                )
                .await?
            {
                let mut response = redirect(&target);
                response.headers_mut().insert(
                    SET_COOKIE,
                    session_cookie(&token).parse().expect("cookie syntax"),
                );
                response
            } else {
                invalid(INVALID_INVITE)
            }
        }
    } else {
        invite_form(&invite, &invite_path, principal, None)
    };
    Ok(invitation_response(result))
}

fn invitation_response(response: Response) -> Response {
    let mut response = private_address(response);
    // The native POST needs its origin. Only the origin can be sent as a referrer.
    response
        .headers_mut()
        .insert(REFERRER_POLICY, HeaderValue::from_static("strict-origin"));
    response
}

fn invite_form(
    invite: &crate::db::organization::InvitationAccess,
    path: &str,
    principal: Option<&Principal>,
    error: Option<&str>,
) -> Response {
    let name = esc(&invite.name);
    let error_text = error
        .map(|value| format!(r#"<p class="org-error" role="alert">{}</p>"#, esc(value)))
        .unwrap_or_default();
    let action = if let Some(actor) = principal {
        format!(
            r#"<p>Вы вошли как {}.</p><form method="post" action="{path}"><button class="space-action primary-action">Принять приглашение</button></form>"#,
            esc(&actor.name)
        )
    } else if invite.account_exists {
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("returnTo", path)
            .finish();
        format!(
            r#"<a class="space-action primary-action" href="/?{query}">Войти и принять приглашение</a>"#
        )
    } else {
        format!(
            r#"<form class="org-fields" method="post" action="{path}">
          <label>Имя<input name="name" maxlength="120" autocomplete="name" required></label>
          <label>Пароль<input name="password" type="password" minlength="15" maxlength="128" autocomplete="new-password" required></label>
          <button class="space-action primary-action">Создать аккаунт и вступить</button></form>"#
        )
    };
    let body = format!(
        r#"<div id="main" class="org-main"><section class="org-panel org-panel-pad">
       <h1>Приглашение в {name}</h1><p>Приглашение для {}.</p>{error_text}{action}</section></div>"#,
        esc(&invite.email)
    );
    html(
        if error.is_some() {
            StatusCode::UNPROCESSABLE_ENTITY
        } else {
            StatusCode::OK
        },
        page("Приглашение", None, None, "organization", &body),
    )
}

pub(super) async fn profile(
    store: &Store,
    actor: &Principal,
    request: Request,
) -> Result<Response, sqlx::Error> {
    let account = store.profile_settings(actor.id).await?;
    let (workspace, _) = store.workspace(actor.id, None).await?;
    let tenant = workspace
        .as_ref()
        .map(|space| (space.tenant.id, space.tenant.name.as_str()));
    if request.method() == Method::POST {
        let session = session_digest(request.headers());
        let fields = match form(request).await {
            Ok(fields) => fields,
            Err(response) => return Ok(response),
        };
        match fields.get("action") {
            "rename" => {
                let Some(name) = short(fields.get("name"), 1) else {
                    return Ok(invalid("Введите имя от 1 до 120 символов."));
                };
                store.rename_profile(actor.id, name).await?;
            }
            "password" => {
                let Some(session) = session else {
                    return Ok(forbidden());
                };
                let Some(credentials) = store.credentials(&account.email).await? else {
                    return Ok(forbidden());
                };
                let Ok(permit) = PASSWORDS.try_acquire() else {
                    return Ok(invalid("Смена пароля занята. Повторите позже."));
                };
                let current = fields.get("current_password").to_owned();
                let next = fields.get("new_password").to_owned();
                let result = tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    if !crate::identity::password_matches(
                        &current,
                        &credentials.salt,
                        &credentials.password_hash,
                    ) {
                        return Err("Текущий пароль не подошёл.");
                    }
                    let replacement = prepare_account(&account.email, &account.name, &next)?;
                    Ok((credentials.password_hash, replacement))
                })
                .await;
                let (old, replacement) = match result {
                    Ok(Ok(values)) => values,
                    Ok(Err(error)) => return Ok(invalid(error)),
                    Err(_) => return Ok(invalid("Не удалось подготовить новый пароль.")),
                };
                if !store
                    .change_profile_password(actor.id, &old, &replacement, &session)
                    .await?
                {
                    return Ok(invalid("Пароль уже изменился. Повторите действие."));
                }
            }
            _ => return Ok(invalid("Неизвестное действие.")),
        }
        return Ok(redirect("/workspace/profile"));
    }
    let body = format!(
        r#"<div id="main" class="org-main">
      <h1>Учётная запись</h1><div class="org-grid">
      <section class="org-panel org-panel-pad"><h2>Профиль</h2><p class="org-muted">{}</p>
      <form class="org-fields" action="/workspace/profile" method="post"><input type="hidden" name="action" value="rename">
       <label>Имя<input name="name" value="{}" autocomplete="name" required maxlength="120"></label><button class="space-action primary-action">Сохранить</button>
      </form></section>
      <section class="org-panel org-panel-pad"><h2>Вход и безопасность</h2>
      <p class="org-muted">Вход по электронной почте</p>
      <form class="org-fields" action="/workspace/profile" method="post"><input type="hidden" name="action" value="password">
       <label>Текущий пароль<input name="current_password" type="password" autocomplete="current-password" required></label>
       <label>Новый пароль<input name="new_password" type="password" autocomplete="new-password" required minlength="15" maxlength="128"></label>
       <button class="space-action primary-action">Сменить пароль</button>
      </form></section></div></div>"#,
        esc(&account.email),
        esc(&account.name)
    );
    Ok(html(
        StatusCode::OK,
        page("Учётная запись", tenant, None, "organization", &body),
    ))
}
