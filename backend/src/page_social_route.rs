use axum::{
    body::{Body, to_bytes},
    extract::Request,
    http::{Method, StatusCode, header},
    response::Response,
};
use uuid::Uuid;

use crate::{
    coedit_http::{self, CoeditSurface},
    coedit_presence::PresenceTracker,
    db::{PageAddress, Store, external_sharing::AssetKind},
    external_sharing::{PublishedResource, SharingDelivery, sharing_error},
    http::{self, Form},
    identity::{self, Identity},
    page_discussion_http, page_sharing_http,
    workspace::{DocumentsRoute, DocumentsTarget, Route, canonical_uuid},
};

#[derive(Clone, Copy)]
pub(crate) enum SocialRoute<'a> {
    Sharing(DocumentsRoute, Uuid),
    Discussion(DocumentsRoute, Uuid),
    Coediting(DocumentsRoute, Uuid, CoeditSurface),
    Notifications,
    Invitation(Uuid, &'a str),
    Published(Uuid, &'a str, PublishedResource),
}

impl<'a> SocialRoute<'a> {
    pub(crate) fn parse(path: &'a str) -> Option<Self> {
        if path == "/workspace/notifications" {
            return Some(Self::Notifications);
        }
        if let Some(segments) = path.strip_prefix("/workspace/invitations/") {
            let (id, token) = segments.split_once('/')?;
            return Some(Self::Invitation(canonical_uuid(id)?, share_token(token)?));
        }
        if let Some(segments) = path.strip_prefix("/published/") {
            let mut parts = segments.split('/');
            let id = canonical_uuid(parts.next()?)?;
            let token = share_token(parts.next()?)?;
            let resource = match parts.next() {
                None => PublishedResource::Page,
                Some("cover") => {
                    PublishedResource::Asset(AssetKind::Cover, canonical_uuid(parts.next()?)?)
                }
                Some("images") => {
                    PublishedResource::Asset(AssetKind::Image, canonical_uuid(parts.next()?)?)
                }
                _ => return None,
            };
            return parts
                .next()
                .is_none()
                .then_some(Self::Published(id, token, resource));
        }
        let (document, surface) = path.rsplit_once('/')?;
        if !matches!(
            surface,
            "sharing" | "discussion" | "coedit" | "presence" | "watch"
        ) {
            return None;
        }
        let Route::Documents(
            route @ DocumentsRoute {
                target: DocumentsTarget::Page(page),
                ..
            },
        ) = Route::parse(document)?
        else {
            return None;
        };
        Some(match surface {
            "sharing" => Self::Sharing(route, page),
            "discussion" => Self::Discussion(route, page),
            "coedit" => Self::Coediting(route, page, CoeditSurface::Draft),
            "presence" => Self::Coediting(route, page, CoeditSurface::Presence),
            "watch" => Self::Coediting(route, page, CoeditSurface::Watch),
            _ => return None,
        })
    }

    pub(crate) fn is_api(&self) -> bool {
        matches!(
            self,
            Self::Sharing(..) | Self::Discussion(..) | Self::Coediting(..) | Self::Notifications
        )
    }
}

fn share_token(input: &str) -> Option<&str> {
    identity::valid_token(input).then_some(input)
}

pub(crate) async fn handle(
    store: &Store,
    identity: &Identity,
    delivery: &SharingDelivery,
    presence: &PresenceTracker,
    origin: &str,
    request: Request,
) -> Result<Response, sqlx::Error> {
    let path = request.uri().path().to_owned();
    let social = SocialRoute::parse(&path).expect("recognized social route");
    match social {
        SocialRoute::Invitation(id, token) => {
            return delivery
                .accept(store, identity, origin, id, token, request)
                .await;
        }
        SocialRoute::Published(id, token, resource) => {
            return delivery
                .published(store, request.method(), id, token, resource)
                .await;
        }
        _ => {}
    }
    let Some(principal) = identity::identify(store, request.headers()).await? else {
        return Ok(sharing_error(
            StatusCode::UNAUTHORIZED,
            "Войдите в «Север».",
        ));
    };
    let (route, page) = match social {
        SocialRoute::Notifications => {
            return page_discussion_http::notifications(store, principal.id, request).await;
        }
        SocialRoute::Sharing(route, page)
        | SocialRoute::Discussion(route, page)
        | SocialRoute::Coediting(route, page, _) => (route, page),
        SocialRoute::Invitation(..) | SocialRoute::Published(..) => unreachable!("already handled"),
    };
    let address = PageAddress {
        tenant: route.tenant,
        space: route.space,
        page,
        account: principal.id,
    };
    match social {
        SocialRoute::Discussion(..) => {
            page_discussion_http::handle(store, &address, origin, request).await
        }
        SocialRoute::Coediting(_, _, surface) => {
            coedit_http::handle(store, presence, &address, surface, origin, request).await
        }
        _ => sharing(store, delivery, &address, origin, request).await,
    }
}

async fn sharing(
    store: &Store,
    delivery: &SharingDelivery,
    address: &PageAddress,
    origin: &str,
    request: Request,
) -> Result<Response, sqlx::Error> {
    if !matches!(*request.method(), Method::GET | Method::HEAD | Method::POST) {
        let mut response = sharing_error(StatusCode::METHOD_NOT_ALLOWED, "Метод недоступен.");
        response.headers_mut().insert(
            header::ALLOW,
            axum::http::HeaderValue::from_static("GET, HEAD, POST"),
        );
        return Ok(response);
    }
    if matches!(*request.method(), Method::GET | Method::HEAD) {
        let query = Form::from_query(request.uri().query().unwrap_or_default());
        let tab = query.get("tab");
        if matches!(tab, "link" | "invites") {
            let offset = match query.field("offset") {
                Ok(None) => 0,
                Ok(Some(value)) => match value.parse::<i32>() {
                    Ok(offset) if offset >= 0 => i64::from(offset),
                    _ => {
                        return Ok(sharing_error(
                            StatusCode::BAD_REQUEST,
                            "Проверьте номер страницы списка.",
                        ));
                    }
                },
                Err(_) => {
                    return Ok(sharing_error(
                        StatusCode::BAD_REQUEST,
                        "Проверьте номер страницы списка.",
                    ));
                }
            };
            return delivery
                .sharing_tab(store, address, origin, tab, offset)
                .await;
        }
    }
    if request.method() == Method::POST
        && http::media_type(request.headers())
            .is_some_and(|value| value.eq_ignore_ascii_case("application/json"))
    {
        if !http::check_origin(request.headers(), origin) {
            return Ok(sharing_error(
                StatusCode::FORBIDDEN,
                "Откройте страницу в «Севере».",
            ));
        }
        let (parts, body) = request.into_parts();
        let Ok(bytes) = to_bytes(body, 8_192).await else {
            return Ok(sharing_error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "Запрос слишком большой.",
            ));
        };
        let Ok(input) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            return Ok(sharing_error(
                StatusCode::BAD_REQUEST,
                "Проверьте параметры операции.",
            ));
        };
        if matches!(
            input["action"].as_str(),
            Some("invite" | "cancelInvite" | "publish" | "unpublish" | "setPublicPolicy")
        ) {
            let response = delivery.change(store, address, origin, input).await?;
            eprintln!(
                "page.sharing.external account={} tenant={} page={} status={}",
                address.account,
                address.tenant,
                address.page,
                response.status().as_u16()
            );
            return Ok(response);
        }
        return page_sharing_http::handle(
            store,
            address,
            origin,
            Request::from_parts(parts, Body::from(bytes)),
        )
        .await;
    }
    page_sharing_http::handle(store, address, origin, request).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scoped_public_routes_validate_the_full_address() {
        let base = format!("/published/{}/{}", Uuid::new_v4(), "a".repeat(43));
        assert!(matches!(
            SocialRoute::parse(&base),
            Some(SocialRoute::Published(_, _, PublishedResource::Page))
        ));
        assert!(matches!(
            SocialRoute::parse(&format!("{base}/images/{}", Uuid::new_v4())),
            Some(SocialRoute::Published(
                _,
                _,
                PublishedResource::Asset(AssetKind::Image, _)
            ))
        ));
        assert!(SocialRoute::parse(&format!("{base}/attachments/{}", Uuid::new_v4())).is_none());
        assert!(SocialRoute::parse(&format!("{base}/")).is_none());
    }
}
