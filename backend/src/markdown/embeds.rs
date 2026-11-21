use std::fmt::Write as _;

use url::Url;

use crate::text::escape_html as html;

pub(super) const LANGUAGE: &str = "sever-embed";

pub(super) fn address(body: &[&str], own_host: &str) -> Option<Url> {
    let [line] = body else { return None };
    let value = line
        .strip_suffix('\r')
        .unwrap_or(line)
        .trim_matches([' ', '\t']);
    if !value.get(..8)?.eq_ignore_ascii_case("https://")
        || value
            .chars()
            .any(|ch| ch.is_ascii_control() || matches!(ch, ' ' | '\\'))
    {
        return None;
    }
    let authority = value[8..].split(['/', '?', '#']).next()?;
    if authority.is_empty() || authority.contains('@') {
        return None;
    }
    let url = Url::parse(value).ok()?;
    let host = url.host_str()?;
    if !url.username().is_empty()
        || url.password().is_some()
        || host
            .trim_end_matches('.')
            .eq_ignore_ascii_case(own_host.trim_end_matches('.'))
    {
        return None;
    }
    Some(url)
}

pub(super) fn render(output: &mut String, address: &Url, teamstream: bool) {
    let host = html(address.host_str().expect("validated HTTPS address"));
    let href = html(address.as_str());
    // Keep the caption and labels stable when the browser opens or hides the
    // frame so text offsets for discussion anchors remain unchanged.
    if teamstream {
        let _ = write!(
            output,
            concat!(
                r#"<figure class="page-embed" data-page-embed data-page-embed-static data-page-embed-kind="teamstream">"#,
                r#"<strong class="page-embed-host" data-label="TeamStream"><span hidden>{host}</span></strong>"#,
                r#"<div class="page-embed-placeholder" data-page-embed-placeholder>"#,
                r#"<p hidden>Внешнее содержимое загрузится по вашему запросу.</p>"#,
                r#"<button type="button" class="space-action space-secondary" data-page-embed-load aria-label="Открыть плеер"><span hidden>Загрузить</span></button></div>"#,
                r#"<div class="page-embed-frame" data-page-embed-frame hidden></div><div class="page-embed-actions">"#,
                r#"<a data-page-embed-source href="{href}" target="_blank" rel="noopener noreferrer" aria-label="Открыть в TeamStream"><span hidden>Открыть источник</span></a>"#,
                r#"<button type="button" class="space-action space-secondary" data-page-embed-hide hidden>Скрыть</button>"#,
                r#"<span data-page-embed-help data-label="Если видео не открывается, проверьте доступ в TeamStream." hidden><span hidden>Если блок не открывается, откройте источник.</span></span></div></figure>"#,
            ),
            host = host,
            href = href,
        );
        return;
    }
    let _ = write!(
        output,
        concat!(
            r#"<figure class="page-embed" data-page-embed><strong class="page-embed-host">{host}</strong>"#,
            r#"<div class="page-embed-placeholder" data-page-embed-placeholder>"#,
            r#"<p>Внешнее содержимое загрузится по вашему запросу.</p><button type="button" class="space-action space-secondary" data-page-embed-load>Загрузить</button></div>"#,
            r#"<div class="page-embed-frame" data-page-embed-frame hidden></div><div class="page-embed-actions">"#,
            r#"<a data-page-embed-source href="{href}" target="_blank" rel="noopener noreferrer">Открыть источник</a>"#,
            r#"<button type="button" class="space-action space-secondary" data-page-embed-hide hidden>Скрыть</button>"#,
            r#"<span data-page-embed-help hidden>Если блок не открывается, откройте источник.</span></div></figure>"#,
        ),
        host = host,
        href = href
    );
}

#[cfg(test)]
mod tests {
    use crate::markdown::{embed_presence, render_document};

    #[test]
    fn embeds_render_only_an_escaped_placeholder_and_explicit_source_link() {
        let source = "До\n\n```sever-embed\nhttps://video.example.test/embed/demo?a=1&b=2#clip\n```\n\nПосле";
        let rendered = render_document(source, &|_, _| None, "http://localhost:4125");
        let html = rendered.as_str();
        assert!(rendered.has_embeds());
        assert_eq!(
            embed_presence(source, "http://localhost:4125"),
            (true, false)
        );
        assert!(html.starts_with("<p>До</p><figure"));
        assert!(html.ends_with("</figure><p>После</p>"));
        assert!(html.contains(r#"href="https://video.example.test/embed/demo?a=1&amp;b=2#clip" target="_blank" rel="noopener noreferrer""#));
        assert!(html.contains("data-page-embed-load>Загрузить"));
        assert!(!html.contains("<iframe"));
        assert!(!html.contains(" src="));
    }

    #[test]
    fn teamstream_uses_specialized_controls_without_changing_discussion_offsets() {
        let source = "before\n\n```sever-embed\nhttps://workspace.teamstream.mws.ru/embed/abcd1234-0000-4000-8000-000000000001?t=30\n```\n\nafter";
        let rendered = render_document(source, &|_, _| None, "http://localhost:4125");
        let html = rendered.as_str();
        assert!(rendered.has_teamstream());
        assert!(html.contains(r#"data-page-embed-kind="teamstream""#));
        assert!(html.contains(r#"aria-label="Открыть плеер""#));
        assert!(html.contains(r#"aria-label="Открыть в TeamStream""#));
        assert!(html.contains("<span hidden>workspace.teamstream.mws.ru</span>"));
        assert!(html.contains("<span hidden>Загрузить</span>"));
        assert!(!html.contains("<iframe"));

        let ordinary = source.replace(
            "workspace.teamstream.mws.ru/",
            "workspace.teamstream.mws.ru.example/",
        );
        let ordinary = render_document(&ordinary, &|_, _| None, "http://localhost:4125");
        assert!(ordinary.has_embeds());
        assert!(!ordinary.has_teamstream());
        assert!(!ordinary.as_str().contains("data-page-embed-kind"));
    }

    #[test]
    fn other_fences_and_rejected_addresses_remain_escaped_code() {
        let origin = "http://sever.example.test:4125";
        for value in [
            "http://video.example.test/embed/demo",
            "https:///video.example.test/embed/demo",
            "https://@video.example.test/",
            "https://user:password@video.example.test/",
            "https://SEVER.EXAMPLE.TEST.:443/workspace",
            "https://video.example.test/a\\b",
            "https://video.example.test/\nhttps://video.example.test/again",
        ] {
            let markdown = format!("```sever-embed\n{value}\n```");
            let rendered = render_document(&markdown, &|_, _| None, origin);
            assert!(!rendered.has_embeds(), "{value}");
            assert!(rendered.as_str().starts_with("<pre><code>"), "{value}");
        }
        for markdown in [
            "````text\n```sever-embed\nhttps://example.test\n```\n````",
            "```sever-embed\nhttps://example.test",
            "```sever-embed-extra\nhttps://example.test\n```",
        ] {
            assert_eq!(embed_presence(markdown, origin), (false, false));
        }
        assert_eq!(
            embed_presence(
                "~~~ sever-embed \r\nHTTPS://video.example.test/\r\n~~~\r\n",
                origin
            ),
            (true, false)
        );
    }
}
