use axum::{
    extract::Request,
    http::{Method, StatusCode, header},
    response::{IntoResponse, Response},
};
use uuid::Uuid;

use super::{PUBLIC, SharingDelivery, private_address};
use crate::{
    db::{
        Store,
        external_sharing::{AssetKind, PublicPage},
    },
    http::{html, html_with_external_frames, method_not_allowed, plain},
    identity::token_digest,
    markdown::render_document,
    teamstream::IsolatedOrigin,
    text::{escape_html, markdown_to_text},
    views::PAGE_EMBED_SCRIPT,
};

#[derive(Clone, Copy)]
pub(crate) enum PublishedResource {
    Page,
    Asset(AssetKind, Uuid),
}

impl SharingDelivery {
    pub(crate) async fn published(
        &self,
        store: &Store,
        request: Request,
        id: Uuid,
        token: &str,
        resource: PublishedResource,
        origin: &str,
    ) -> Result<Response, sqlx::Error> {
        let missing = || private_address(plain(StatusCode::NOT_FOUND, "Страница недоступна."));
        if !matches!(*request.method(), Method::GET | Method::HEAD) {
            return Ok(private_address(method_not_allowed("GET, HEAD")));
        }
        let Some(current_token) = self.token(PUBLIC, id) else {
            return Ok(missing());
        };
        if !same_token(&current_token, token) {
            return Ok(missing());
        }
        let digest = token_digest(token);
        let response = match resource {
            PublishedResource::Page => store
                .load_public_page(id, &digest)
                .await?
                .map(|page| public_page(&page, id, token, origin, request.extensions().get())),
            PublishedResource::Asset(kind, asset) => store
                .load_public_asset(id, &digest, kind, asset)
                .await?
                .map(|body| {
                    ([(header::CONTENT_TYPE, body.media_type)], body.bytes).into_response()
                }),
        };
        Ok(response.map_or_else(missing, private_address))
    }
}

pub(super) fn same_token(expected: &str, supplied: &str) -> bool {
    use subtle::ConstantTimeEq;
    bool::from(expected.as_bytes().ct_eq(supplied.as_bytes()))
}

fn public_page(
    page: &PublicPage,
    id: Uuid,
    token: &str,
    origin: &str,
    teamstream: Option<&IsolatedOrigin>,
) -> Response {
    let base = format!("/published/{id}/{token}");
    let title = escape_html(&page.title);
    let cover = if let Some(asset) = page.cover_id {
        format!(
            r#"<div class="page-cover-art" data-cover-theme="custom" aria-hidden="true"><img src="{base}/cover/{asset}" alt=""></div>"#
        )
    } else {
        match page.cover_theme.as_deref() {
            Some(theme @ ("orbit" | "aurora" | "grid")) => format!(
                r#"<div class="page-cover-art" data-cover-theme="{theme}" aria-hidden="true"></div>"#
            ),
            _ => String::new(),
        }
    };
    let mut embeds = false;
    let content = if page.source.is_empty() {
        r#"<p class="document-empty">Пока здесь ничего нет.</p>"#.to_owned()
    } else if page.markdown {
        let images: std::collections::HashMap<_, _> = page
            .images
            .iter()
            .map(|(id, publication)| (*id, format!("{base}/images/{publication}")))
            .collect();
        let rendered = render_document(
            &page.source,
            &|asset, image| {
                if image {
                    images.get(&asset).map(String::as_str)
                } else {
                    None
                }
            },
            origin,
        );
        embeds = rendered.has_embeds();
        format!(
            r#"<div class="document-text document-markdown">{}</div>"#,
            rendered.as_str()
        )
    } else {
        format!(
            r#"<div class="document-text">{}</div>"#,
            escape_html(&markdown_to_text(&page.source))
        )
    };
    let heading_class = if cover.is_empty() {
        "space-heading"
    } else {
        "space-heading page-cover-heading"
    };
    let body = document(
        &title,
        &format!(
            r#"
        <main class="documents-content">
        <article class="documents-article">
        <header class="{heading_class}">{cover}<div><h1>{title}</h1>
        <div class="space-badges"><span>Публичный просмотр</span></div></div></header>
        {content}</article></main>"#
        ),
        if embeds { PAGE_EMBED_SCRIPT } else { "" },
    );
    if embeds {
        html_with_external_frames(StatusCode::OK, body, teamstream)
    } else {
        html(StatusCode::OK, body)
    }
}

pub(super) fn public_document(title: &str, contents: &str) -> String {
    document(title, contents, "")
}

fn document(title: &str, contents: &str, scripts: &str) -> String {
    format!(
        r#"<!doctype html>
<html lang="ru"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} · Север</title><link rel="stylesheet" href="/auth.css"><link rel="stylesheet" href="/spaces.css">{scripts}
</head><body>{contents}</body></html>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn digest_comparison_requires_the_complete_secret() {
        assert!(same_token("complete-secret", "complete-secret"));
        assert!(!same_token("complete-secret", "complete-secre"));
        assert!(!same_token("complete-secret", "complete-secreT"));
    }
}
