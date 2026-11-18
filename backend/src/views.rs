mod account;
mod client;
mod sections;
mod workspace;

pub(crate) use account::{
    no_tenant, registration_complete, registration_requested, registration_start, sign_in,
};
pub(crate) use client::{BUNDLE as UI_BUNDLE, BUNDLE_PATH as UI_BUNDLE_PATH};
pub(crate) use sections::section_retry;
pub(crate) use workspace::{
    catalog, catalog_unavailable, closed_space, create_space, section_not_found, space_not_found,
};

use std::borrow::Cow;

use client::ClientPage;

const NORTH_ICON: &str = r#"<svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M12 2l2.8 7.2L22 12l-7.2 2.8L12 22l-2.8-7.2L2 12l7.2-2.8L12 2z M12 9v6m-3-3h6"/></svg>"#;

fn html(input: &str) -> String {
    let mut escaped = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn icon(name: &str) -> String {
    let path = match name {
        "add" => "M12 5v14 M5 12h14",
        "folder" => "M3 7V5a1 1 0 011-1h5l2 2h9a1 1 0 011 1v12H3z",
        "lock" => "M5 10h14v11H5z m3 0V7a4 4 0 118 0v3",
        "layers" => "m3 8 9-5 9 5-9 5-9-5zm0 5 9 5 9-5m-18 5 9 5 9-5",
        "back" => "M20 12H5m6 6-6-6 6-6",
        "close" => "m6 6 12 12M6 18 18 6",
        "home" => "m3 10 9-7 9 7v10H3zm6 10v-6h6v6",
        "down" => "m6 9 6 6 6-6",
        "next" => "m9 6 6 6-6 6",
        "search" => "M20 20l-4-4 M18 11a7 7 0 11-14 0 7 7 0 0114 0z",
        _ => unreachable!("unknown template icon"),
    };
    format!(
        r#"<svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="{path}"/></svg>"#
    )
}

enum PageAssets {
    Account { sign_in: bool },
    Workspace { menu: bool, form: bool },
}

fn page<'a>(
    title: &str,
    body: &str,
    assets: PageAssets,
    model: impl Into<Option<ClientPage<'a>>>,
) -> String {
    let title = html(title);
    let stylesheet = if matches!(assets, PageAssets::Workspace { .. }) {
        r#"<link rel="stylesheet" href="/spaces.css">"#
    } else {
        ""
    };
    let client_json = model
        .into()
        .filter(|_| !UI_BUNDLE.is_empty())
        .and_then(|model| match model.script_json() {
            Ok(json) => Some(json),
            Err(_) => {
                eprintln!("page.ui_data.failed");
                None
            }
        });
    let (scripts, body) = if let Some(json) = client_json {
        let script = format!(r#"<script type="module" src="{UI_BUNDLE_PATH}"></script>"#);
        let shell = format!(
            r#"<div id="sever-root">{body}</div>
    <script id="sever-ui-data" type="application/json">{json}</script>"#
        );
        (script, Cow::Owned(shell))
    } else {
        let sources = match assets {
            PageAssets::Account { sign_in } => [sign_in.then_some("/sign-in.mjs"), None],
            PageAssets::Workspace { menu, form } => [
                menu.then_some("/workspace-menu.mjs"),
                form.then_some("/space-form.mjs"),
            ],
        };
        let scripts = sources
            .into_iter()
            .flatten()
            .map(|source| format!(r#"<script type="module" src="{source}"></script>"#))
            .collect::<String>();
        (scripts, Cow::Borrowed(body))
    };
    format!(
        r#"<!doctype html>
<html lang="ru">
  <head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <title>{title} · Север</title>
    <link rel="stylesheet" href="/auth.css">
    {stylesheet}
    {scripts}
  </head>
  <body>{body}</body>
</html>"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_in_keeps_the_native_form_and_uses_one_client_script() {
        let page = sign_in("reader@example.test", "", true, "/workspace");
        assert!(page.contains(r#"<form method="post" action="/login""#));
        assert!(page.contains(r#"name="returnTo" value="/workspace""#));
        if UI_BUNDLE.is_empty() {
            assert!(page.contains(r#"src="/sign-in.mjs""#));
            assert!(!page.contains("sever-root"));
            assert!(!page.contains("sever-ui-data"));
        } else {
            assert!(page.contains(r#"src="/sever-ui.js""#));
            assert!(page.contains(r#"id="sever-root""#));
            assert!(!page.contains(r#"src="/sign-in.mjs""#));
            let data = page
                .split_once(r#"id="sever-ui-data" type="application/json">"#)
                .unwrap()
                .1
                .split_once("</script>")
                .unwrap()
                .0;
            assert_eq!(
                serde_json::from_str::<serde_json::Value>(data).unwrap(),
                serde_json::json!({
                    "kind": "signIn",
                    "email": "reader@example.test",
                    "error": "",
                    "registrationEnabled": true,
                    "returnTo": "/workspace"
                })
            );
        }
    }
}
