use std::env;

use anyhow::{Context, ensure};
use axum::{
    Json,
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::{
    db::{
        PageAddress, Store,
        external_sharing::{SharingFailure, SharingOperation},
        page_sharing::{PageRole, SHARING_PAGE_SIZE},
    },
    identity::{normalize_email, token_digest},
    registration::MailDelivery,
};

mod guest;
mod published;

pub(crate) use published::PublishedResource;

pub(crate) struct SharingDelivery {
    key: Option<[u8; 32]>,
    mail: Option<MailDelivery>,
    guest_setup: Semaphore,
}

impl SharingDelivery {
    pub(crate) fn from_env() -> anyhow::Result<Self> {
        let encoded = env::var("SEVER_SHARING_SECRET").unwrap_or_default();
        let key =
            if encoded.is_empty() {
                None
            } else {
                let decoded = URL_SAFE_NO_PAD
                    .decode(encoded)
                    .context("Неверный формат SEVER_SHARING_SECRET")?;
                Some(decoded.try_into().map_err(|_| {
                    anyhow::anyhow!("SEVER_SHARING_SECRET должен кодировать 32 байта")
                })?)
            };
        let mail = if env::var_os("SEVER_SHARING_SENDMAIL").is_some_and(|value| !value.is_empty()) {
            ensure!(
                key.is_some(),
                "Для почты приглашений укажите SEVER_SHARING_SECRET."
            );
            Some(MailDelivery::configured(
                "SEVER_SHARING_SENDMAIL",
                "SEVER_SHARING_FROM",
            )?)
        } else {
            None
        };
        Ok(Self {
            key,
            mail,
            guest_setup: Semaphore::new(2),
        })
    }

    #[cfg(test)]
    pub(crate) fn disabled() -> Self {
        Self {
            key: None,
            mail: None,
            guest_setup: Semaphore::new(2),
        }
    }

    fn token(&self, purpose: &'static [u8], id: Uuid) -> Option<String> {
        let mut signer =
            Hmac::<Sha256>::new_from_slice(self.key.as_ref()?).expect("HMAC accepts a 32-byte key");
        signer.update(purpose);
        signer.update(id.as_bytes());
        Some(URL_SAFE_NO_PAD.encode(signer.finalize().into_bytes()))
    }

    pub(crate) async fn sharing_tab(
        &self,
        store: &Store,
        address: &PageAddress,
        origin: &str,
        tab: &str,
        offset: i64,
    ) -> Result<Response, sqlx::Error> {
        let response = match tab {
            "invites" => store.guest_invitations(address,offset).await.map(|(allowed,mut invites)| {
                let next = (invites.len() > SHARING_PAGE_SIZE).then_some(offset + SHARING_PAGE_SIZE as i64);
                invites.truncate(SHARING_PAGE_SIZE);
                json(serde_json::json!({"allowed":allowed,"deliveryConfigured":self.mail.is_some(),"invites":invites,"nextOffset":next}))
            }),
            "link" => store.public_link_panel(address,origin).await.map(|(id,policy,(contains_embed, contains_teamstream))| {
                let link = id.filter(|_| policy.enabled).map(|id| {
                    let href = self.token(PUBLIC,id).map(|token| format!("{origin}/published/{id}/{token}"));
                    serde_json::json!({"id":id,"href":href})
                });
                json(serde_json::json!({"link":link,"policy":policy,"sharingConfigured":self.key.is_some(),"containsEmbed":contains_embed,"containsTeamStream":contains_teamstream}))
            }),
            _ => return Ok(sharing_error(StatusCode::BAD_REQUEST,"Неизвестная вкладка.")),
        };
        failure_response(response)
    }

    pub(crate) async fn change(
        &self,
        store: &Store,
        address: &PageAddress,
        origin: &str,
        input: serde_json::Value,
    ) -> Result<Response, sqlx::Error> {
        let Ok(command) = serde_json::from_value::<ExternalCommand>(input) else {
            return Ok(sharing_error(
                StatusCode::BAD_REQUEST,
                "Проверьте параметры операции.",
            ));
        };
        let operation = command.operation();
        let result = match command {
            ExternalCommand::Invite { email, role, .. } => {
                let Some(email) = normalize_email(&email) else {
                    return Ok(sharing_error(
                        StatusCode::UNPROCESSABLE_ENTITY,
                        "Укажите действительный адрес почты.",
                    ));
                };
                if self.mail.is_none() {
                    return Ok(sharing_error(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "Доставка приглашений пока недоступна.",
                    ));
                }
                self.issue_invitation(store, address, origin, &operation, (&email, role))
                    .await
            }
            ExternalCommand::CancelInvite { invite_id, .. } => store
                .cancel_guest_invitation(address, &operation, invite_id)
                .await
                .map(|()| json(serde_json::json!({"ok":true}))),
            ExternalCommand::Publish { .. } => {
                let id = Uuid::new_v4();
                let Some(token) = self.token(PUBLIC, id) else {
                    return Ok(sharing_error(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "Публичные ссылки пока недоступны.",
                    ));
                };
                store
                    .publish_link(address, &operation, id, &token_digest(&token))
                    .await
                    .map(|()| json(serde_json::json!({"ok":true})))
            }
            ExternalCommand::Unpublish { link_id, .. } => store
                .unpublish_link(address, &operation, link_id)
                .await
                .map(|()| json(serde_json::json!({"ok":true}))),
            ExternalCommand::SetPublicPolicy {
                enabled, version, ..
            } => {
                if enabled && self.key.is_none() {
                    return Ok(sharing_error(
                        StatusCode::SERVICE_UNAVAILABLE,
                        "Публичные ссылки пока недоступны.",
                    ));
                }
                store
                    .set_space_link_policy(address, &operation, enabled, version)
                    .await
                    .map(|()| json(serde_json::json!({"ok":true})))
            }
        };
        failure_response(result)
    }

    async fn issue_invitation(
        &self,
        store: &Store,
        address: &PageAddress,
        origin: &str,
        operation: &SharingOperation,
        recipient: (&str, PageRole),
    ) -> Result<Response, SharingFailure> {
        let (email, role) = recipient;
        let mail = self
            .mail
            .as_ref()
            .expect("invitation delivery is configured");
        let id = Uuid::new_v4();
        let token = self
            .token(INVITATION, id)
            .expect("mail delivery requires a key");
        let invitation = store
            .issue_guest_invitation(
                address,
                operation,
                (id, &token_digest(&token)),
                (email, role.as_str()),
            )
            .await?;
        if let Some((email, lease)) = store.claim_guest_delivery(invitation).await? {
            let token = self
                .token(INVITATION, invitation)
                .expect("mail delivery requires a key");
            let href = format!("{origin}/workspace/invitations/{invitation}/{token}");
            let body = format!(
                "Вас пригласили к документу в «Севере».\nОткройте ссылку, чтобы подтвердить почту и принять приглашение.\n\n{href}\n\nПриглашение действует семь дней.\n"
            );
            let delivered = mail
                .send_message(
                    &email,
                    "Приглашение к документу в «Севере»",
                    &body,
                    Some(invitation),
                )
                .await
                .is_ok();
            store
                .finish_guest_delivery(invitation, lease, delivered)
                .await?;
            if !delivered {
                eprintln!("page.invitation.delivery_failed invite={invitation}");
                return Ok(sharing_error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "Не удалось отправить письмо. Повторите отправку.",
                ));
            }
        }
        Ok(json(serde_json::json!({"ok":true,"id":invitation})))
    }
}

const PUBLIC: &[u8] = b"sever/public-page/v1\0";
const INVITATION: &[u8] = b"sever/guest-page/v1\0";

#[derive(Deserialize, Serialize)]
#[serde(
    tag = "action",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum ExternalCommand {
    Invite {
        email: String,
        role: PageRole,
        operation_id: Uuid,
    },
    CancelInvite {
        invite_id: Uuid,
        operation_id: Uuid,
    },
    Publish {
        operation_id: Uuid,
    },
    Unpublish {
        link_id: Uuid,
        operation_id: Uuid,
    },
    SetPublicPolicy {
        enabled: bool,
        version: i64,
        operation_id: Uuid,
    },
}

impl ExternalCommand {
    fn operation(&self) -> SharingOperation {
        let id = match *self {
            Self::Invite { operation_id, .. }
            | Self::CancelInvite { operation_id, .. }
            | Self::Publish { operation_id }
            | Self::Unpublish { operation_id, .. }
            | Self::SetPublicPolicy { operation_id, .. } => operation_id,
        };
        let command = serde_json::to_vec(self).expect("external sharing command is serializable");
        SharingOperation {
            id,
            fingerprint: Sha256::digest(command).into(),
        }
    }
}

pub(crate) fn json(value: serde_json::Value) -> Response {
    Json(value).into_response()
}

pub(crate) fn sharing_error(status: StatusCode, message: &'static str) -> Response {
    (status, Json(serde_json::json!({"error":message}))).into_response()
}

fn failure_response(result: Result<Response, SharingFailure>) -> Result<Response, sqlx::Error> {
    match result {
        Ok(response) => Ok(response),
        Err(SharingFailure::Database(error)) => Err(error),
        Err(SharingFailure::Missing) => {
            Ok(sharing_error(StatusCode::NOT_FOUND, "Действие недоступно."))
        }
        Err(SharingFailure::Forbidden) => Ok(sharing_error(
            StatusCode::FORBIDDEN,
            "Проверьте права и политику организации.",
        )),
        Err(SharingFailure::Conflict) => Ok(sharing_error(
            StatusCode::CONFLICT,
            "Данные изменились. Обновите список и повторите действие.",
        )),
    }
}

pub(crate) fn private_address(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response.headers_mut().insert(
        header::HeaderName::from_static("x-robots-tag"),
        HeaderValue::from_static("noindex, nofollow, noarchive"),
    );
    response
}
