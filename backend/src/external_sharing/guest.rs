use axum::{
    extract::Request,
    http::{HeaderValue, Method, StatusCode, header},
    response::Response,
};
use uuid::Uuid;

use super::{
    INVITATION, SharingDelivery, private_address,
    published::{public_document, same_token},
};
use crate::{
    db::{
        PageAddress, Store,
        external_sharing::{GuestAcceptance, GuestPreview, SharingFailure},
    },
    http::{self, html, method_not_allowed, plain, redirect},
    identity::{self, Identity, LoginOutcome, now_ms, token_digest},
    text::escape_html,
    workspace::page_path,
};

impl SharingDelivery {
    pub(crate) async fn accept(
        &self,
        store: &Store,
        identity: &Identity,
        origin: &str,
        id: Uuid,
        token: &str,
        request: Request,
    ) -> Result<Response, sqlx::Error> {
        let missing = || invitation_response(html(StatusCode::NOT_FOUND, invitation_unavailable()));
        let Some(secret) = self.token(INVITATION, id) else {
            return Ok(missing());
        };
        if !same_token(&secret, token) {
            return Ok(missing());
        }
        let digest = token_digest(token);
        let Some(invite) = store.preview_guest(id, &digest).await? else {
            return Ok(missing());
        };
        let path = format!("/workspace/invitations/{id}/{token}");
        let principal = identity::identify(store, request.headers()).await?;
        let matching_session = principal.is_some_and(|user| Some(user.id) == invite.account);
        if matches!(*request.method(), Method::GET | Method::HEAD) {
            return Ok(invitation_response(html(
                StatusCode::OK,
                invitation_form(&invite, &path, matching_session, "", ""),
            )));
        }
        if request.method() != Method::POST {
            return Ok(invitation_response(method_not_allowed("GET, HEAD, POST")));
        }
        if !http::check_origin(request.headers(), origin) {
            return Ok(invitation_response(plain(
                StatusCode::FORBIDDEN,
                "Откройте приглашение в «Севере».",
            )));
        }
        let Ok(input) = http::read_form(request).await else {
            return Ok(invitation_response(plain(
                StatusCode::BAD_REQUEST,
                "Не удалось прочитать форму.",
            )));
        };
        let render_error = |status, message: &str| {
            invitation_response(html(
                status,
                invitation_form(&invite, &path, matching_session, input.get("name"), message),
            ))
        };
        let result = if let Some(account) = invite.account {
            let session = if matching_session {
                None
            } else {
                match identity
                    .sign_in(store, &invite.email, input.get("password"))
                    .await?
                {
                    LoginOutcome::Accepted { token, account_id } if account_id == account => {
                        Some(token)
                    }
                    LoginOutcome::Limited(_) => {
                        return Ok(render_error(
                            StatusCode::TOO_MANY_REQUESTS,
                            "Слишком много попыток. Подождите и повторите.",
                        ));
                    }
                    _ => {
                        return Ok(render_error(
                            StatusCode::UNPROCESSABLE_ENTITY,
                            "Пароль не подходит к приглашённому адресу.",
                        ));
                    }
                }
            };
            store
                .accept_guest_invitation(id, &digest, GuestAcceptance::Existing(account))
                .await
                .map(|page| (page, session))
        } else {
            let Ok(permit) = self.guest_setup.try_acquire() else {
                return Ok(render_error(
                    StatusCode::TOO_MANY_REQUESTS,
                    "Подождите немного и повторите.",
                ));
            };
            let fields = (
                invite.email.clone(),
                input.get("name").to_owned(),
                input.get("password").to_owned(),
            );
            let prepared = tokio::task::spawn_blocking(move || {
                identity::prepare_account(&fields.0, &fields.1, &fields.2)
            })
            .await
            .map_err(|_| sqlx::Error::Protocol("guest account preparation failed".to_owned()))?;
            let account = match prepared {
                Ok(account) => account,
                Err(message) => return Ok(render_error(StatusCode::UNPROCESSABLE_ENTITY, message)),
            };
            let (cookie, session) = identity::prepare_session(now_ms());
            let result = store
                .accept_guest_invitation(id, &digest, GuestAcceptance::New(&account, &session))
                .await;
            drop(permit);
            result.map(|page| (page, Some(cookie)))
        };
        let response = match result {
            Ok((page, session)) => {
                eprintln!(
                    "page.invitation.accepted invite={id} account={} tenant={} page={}",
                    page.account, page.tenant, page.page
                );
                accept_response(&page, session.as_deref())
            }
            Err(SharingFailure::Database(error)) => return Err(error),
            Err(SharingFailure::Conflict) => {
                return Ok(render_error(
                    StatusCode::CONFLICT,
                    "Состояние учётной записи изменилось. Откройте приглашение ещё раз.",
                ));
            }
            Err(SharingFailure::Missing | SharingFailure::Forbidden) => return Ok(missing()),
        };
        Ok(invitation_response(response))
    }
}

fn invitation_response(response: Response) -> Response {
    let mut response = private_address(response);
    // Native POSTs preserve Origin, while Referer contains only the origin.
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("strict-origin"),
    );
    response
}

fn accept_response(page: &PageAddress, cookie: Option<&str>) -> Response {
    let mut response = redirect(&page_path(page.tenant, page.space, page.page));
    if let Some(cookie) = cookie {
        response.headers_mut().insert(
            header::SET_COOKIE,
            HeaderValue::from_str(&identity::session_cookie(cookie))
                .expect("session token is URL-safe"),
        );
    }
    response
}

fn invitation_form(
    invite: &GuestPreview,
    path: &str,
    matching_session: bool,
    name: &str,
    error: &str,
) -> String {
    let email = escape_html(&invite.email);
    let name = escape_html(name);
    let error = escape_html(error);
    let credentials = if matching_session {
        String::new()
    } else if invite.account.is_some() {
        r#"<label for="guest-password">Пароль</label><input id="guest-password" name="password" type="password" autocomplete="current-password" required>"#.to_owned()
    } else {
        format!(
            r#"<label for="guest-name">Имя</label><input id="guest-name" name="name" value="{name}" maxlength="120" autocomplete="name" required>
<label for="guest-password">Пароль</label><input id="guest-password" name="password" type="password" autocomplete="new-password" minlength="15" maxlength="128" required>"#
        )
    };
    let alert = if error.is_empty() {
        String::new()
    } else {
        format!(r#"<p class="form-error" role="alert">{error}</p>"#)
    };
    public_document(
        "Приглашение к странице",
        &format!(
            r#"
<main class="auth-screen"><article class="auth-card"><h1>Приглашение к странице</h1>
<p>Доступ будет выдан на адрес <strong>{email}</strong>.</p>
{alert}<form method="post" action="{path}">{credentials}<button class="primary-action" type="submit">Принять приглашение</button></form>
</article></main>"#
        ),
    )
}

fn invitation_unavailable() -> String {
    public_document(
        "Приглашение недоступно",
        r#"<main class="auth-screen"><article class="auth-card">
<h1>Приглашение недоступно</h1><p>Попросите владельца страницы отправить новое приглашение.</p>
</article></main>"#,
    )
}
