use std::fmt::Write as _;
use std::sync::LazyLock;
use std::{cell::RefCell, collections::BTreeSet, ops::Range};

use regex::Regex;
use serde::Serialize;
use url::Url;
use uuid::Uuid;

use crate::text::{escape_html as html, is_js_whitespace, markdown_to_text};

mod blocks;
mod embeds;

static LINK_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(!?)\[((?:\\[^\n]|[^\\\[\]\n])*)\]\(").expect("Markdown link prefix")
});

#[derive(Serialize)]
#[serde(transparent)]
pub(crate) struct RenderedMarkdown {
    html: String,
    #[serde(skip)]
    has_embeds: bool,
    #[serde(skip)]
    has_teamstream: bool,
}

impl RenderedMarkdown {
    pub(crate) fn as_str(&self) -> &str {
        &self.html
    }

    pub(crate) fn has_embeds(&self) -> bool {
        self.has_embeds
    }

    pub(crate) fn has_teamstream(&self) -> bool {
        self.has_teamstream
    }
}

#[cfg(test)]
pub(crate) fn render_markdown(source: &str) -> RenderedMarkdown {
    render_markdown_with_attachments(source, &|_, _| None)
}

type AttachmentUrl<'a> = dyn Fn(Uuid, bool) -> Option<&'a str> + 'a;
type AttachmentRender<'a> = dyn Fn(Uuid, bool, &str) -> Option<&'a str> + 'a;

pub(crate) fn render_markdown_with_attachments(
    source: &str,
    attachment_url: &AttachmentUrl<'_>,
) -> RenderedMarkdown {
    blocks::render(source, &|id, image, _| attachment_url(id, image), None)
}

pub(crate) fn render_document(
    source: &str,
    attachment_url: &AttachmentUrl<'_>,
    origin: &str,
) -> RenderedMarkdown {
    let origin = Url::parse(origin).ok();
    blocks::render(
        source,
        &|id, image, _| attachment_url(id, image),
        origin.as_ref().and_then(Url::host_str),
    )
}

pub(crate) fn embed_presence(source: &str, origin: &str) -> (bool, bool) {
    if !source.contains(embeds::LANGUAGE) {
        return (false, false);
    }
    let rendered = render_document(source, &|_, _| None, origin);
    (rendered.has_embeds(), rendered.has_teamstream())
}

pub(crate) fn contains_attachment_reference(source: &str, ids: &[Uuid]) -> bool {
    !ids.is_empty()
        && attachment_reference_ids(source)
            .iter()
            .any(|id| ids.contains(id))
}

pub(crate) fn attachment_reference_ids(source: &str) -> Vec<Uuid> {
    if !source.contains("attachment:") {
        return Vec::new();
    }
    let found = RefCell::new(BTreeSet::new());
    // Use the same grammar for storage and rendering, so code spans, fences
    // and escaped text never count as a live reference during publication.
    render_markdown_with_attachments(source, &|id, _| {
        found.borrow_mut().insert(id);
        None
    });
    found.into_inner().into_iter().collect()
}

pub(crate) fn attachment_image_ids(source: &str) -> Vec<Uuid> {
    let found = RefCell::new(BTreeSet::new());
    if source.contains("attachment:") {
        render_markdown_with_attachments(source, &|id, image| {
            if image {
                found.borrow_mut().insert(id);
            }
            None
        });
    }
    found.into_inner().into_iter().collect()
}

pub(crate) fn without_attachment_references<'a>(
    source: &'a str,
    ids: &[Uuid],
) -> std::borrow::Cow<'a, str> {
    if ids.is_empty() || !source.contains("attachment:") {
        return std::borrow::Cow::Borrowed(source);
    }
    let spans = RefCell::new(Vec::<Range<usize>>::new());
    blocks::render(
        source,
        &|id, _, markup| {
            if ids.contains(&id) {
                // Blocks hand the inline renderer slices of the original source.
                // Use their byte offsets to keep surrounding Markdown unchanged.
                let start = markup.as_ptr() as usize - source.as_ptr() as usize;
                spans.borrow_mut().push(start..start + markup.len());
            }
            None
        },
        None,
    );
    let mut spans = spans.into_inner();
    if spans.is_empty() {
        return std::borrow::Cow::Borrowed(source);
    }
    spans.sort_unstable_by_key(|span| span.start);
    let mut result = String::with_capacity(source.len());
    let mut from = 0;
    for span in spans {
        result.push_str(&source[from..span.start]);
        from = span.end;
    }
    result.push_str(&source[from..]);
    std::borrow::Cow::Owned(result)
}

fn run(text: &[u8], marker: u8) -> usize {
    text.iter().take_while(|&&ch| ch == marker).count()
}

struct MarkdownLink<'a> {
    image: bool,
    label: &'a str,
    destination: &'a str,
    end: usize,
}

fn link_at(text: &str) -> Option<MarkdownLink<'_>> {
    let captures = LINK_PREFIX.captures(text)?;
    let prefix = captures.get(0)?.end();
    let mut nested = 0;
    let mut cursor = prefix;
    let bytes = text.as_bytes();
    while let Some(&byte) = bytes.get(cursor) {
        match byte {
            b'\\' if bytes.get(cursor + 1).is_some_and(u8::is_ascii_punctuation) => cursor += 1,
            b'(' if nested == 32 => return None,
            b'(' => nested += 1,
            b')' if nested == 0 => {
                return Some(MarkdownLink {
                    image: !captures.get(1)?.as_str().is_empty(),
                    label: captures.get(2)?.as_str(),
                    destination: &text[prefix..cursor],
                    end: cursor + 1,
                });
            }
            b')' => nested -= 1,
            _ => {}
        }
        cursor += 1;
    }
    None
}

fn internal_page_destination(value: &str) -> Option<&str> {
    use crate::workspace::{DocumentsTarget, Route};

    match Route::parse(value) {
        Some(Route::Documents(route)) if matches!(route.target, DocumentsTarget::Page(_)) => {
            Some(value)
        }
        _ => None,
    }
}

fn http_destination(value: &str) -> Option<Url> {
    let (scheme, authority) = value.split_once("://")?;
    if !(scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
        || !authority
            .as_bytes()
            .first()
            .is_some_and(|byte| !matches!(byte, b'/' | b'?' | b'#'))
        || value
            .bytes()
            .any(|byte| matches!(byte, 0..=32 | 127 | b'\\'))
    {
        return None;
    }
    let parsed = Url::parse(value).ok()?;
    parsed.host().is_some().then_some(parsed)
}

fn append_literal(output: &mut String, text: &str, in_table: bool) {
    let text = if in_table && text.contains(r"\|") {
        std::borrow::Cow::Owned(text.replace(r"\|", "|"))
    } else {
        std::borrow::Cow::Borrowed(text)
    };
    output.push_str(&html(&text));
}

struct Emphasis {
    marker: u8,
    width: usize,
    html: String,
}

impl Emphasis {
    fn new(marker: u8, width: usize) -> Self {
        Self {
            marker,
            width,
            html: String::new(),
        }
    }
}

fn attachment_id(link: &MarkdownLink<'_>) -> Option<Uuid> {
    if !link.image && link.label.is_empty() {
        return None;
    }
    let token = link.destination.strip_prefix("attachment:")?;
    let id = Uuid::parse_str(token).ok()?;
    (token == id.to_string()).then_some(id)
}

fn inline(text: &str, in_table: bool, attachment_url: &AttachmentRender<'_>) -> String {
    inline_text(text, in_table, true, attachment_url)
}

fn inline_text(
    text: &str,
    in_table: bool,
    links: bool,
    attachment_url: &AttachmentRender<'_>,
) -> String {
    let mut stack = vec![Emphasis::new(0, 0)];
    let bytes = text.as_bytes();
    let mut cursor = 0;
    while cursor < text.len() {
        let plain_len = bytes[cursor..]
            .iter()
            .position(|&byte| {
                matches!(byte, b'\\' | b'`' | b'*' | b'_' | b'~' | b'[' | b'!' | b'<')
            })
            .unwrap_or(text.len() - cursor);
        let end = cursor + plain_len;
        let output = &mut stack.last_mut().unwrap().html;
        output.push_str(&html(&text[cursor..end]));
        cursor = end;
        if links
            && (bytes.get(cursor) == Some(&b'[') || bytes[cursor..].starts_with(b"!["))
            && let Some(link) = link_at(&text[cursor..])
        {
            if let Some(path) = attachment_id(&link)
                .and_then(|id| attachment_url(id, link.image, &text[cursor..cursor + link.end]))
            {
                if link.image {
                    let _ = write!(
                        output,
                        "<img class=\"page-inline-image\" src=\"{}\" alt=\"{}\" loading=\"lazy\" decoding=\"async\">",
                        html(path),
                        html(&markdown_to_text(link.label))
                    );
                } else {
                    let _ = write!(
                        output,
                        "<a href=\"{}\">{}</a>",
                        html(path),
                        inline_text(link.label, in_table, false, attachment_url)
                    );
                }
            } else if !link.image
                && !link.label.is_empty()
                && let Some(destination) =
                    internal_page_destination(&markdown_to_text(link.destination))
            {
                let _ = write!(
                    output,
                    "<a href=\"{}\">{}</a>",
                    html(destination),
                    inline_text(link.label, in_table, false, attachment_url)
                );
            } else if !link.image
                && !link.label.is_empty()
                && let Some(destination) = http_destination(&markdown_to_text(link.destination))
            {
                let _ = write!(
                    output,
                    "<a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">{}</a>",
                    html(destination.as_str()),
                    inline_text(link.label, in_table, false, attachment_url)
                );
            } else {
                append_literal(output, &text[cursor..cursor + link.end], in_table);
            }
            cursor += link.end;
            continue;
        }
        match bytes.get(cursor) {
            Some(b'\\') if bytes.get(cursor + 1).is_some_and(u8::is_ascii_punctuation) => {
                output.push_str(&html(&text[cursor + 1..cursor + 2]));
                cursor += 2;
            }
            Some(b'`') => {
                let width = run(&bytes[cursor..], b'`');
                let start = cursor + width;
                if let Some(close) = closing_code_span(text, start, width) {
                    output.push_str("<code>");
                    let code = &text[start..close];
                    let code = match code
                        .strip_prefix(' ')
                        .and_then(|code| code.strip_suffix(' '))
                    {
                        Some(padded) if !padded.bytes().all(|byte| byte == b' ') => padded,
                        _ => code,
                    };
                    append_literal(output, code, in_table);
                    output.push_str("</code>");
                    cursor = close + width;
                } else {
                    output.push_str(&text[cursor..start]);
                    cursor = start;
                }
            }
            Some(&marker @ (b'*' | b'_' | b'~')) => {
                let width = run(&bytes[cursor..], marker);
                let close = cursor + width;
                let before = text[..cursor].chars().next_back();
                let after = text[close..].chars().next();
                let may_open = after.is_some_and(|ch| !is_js_whitespace(ch))
                    && !(marker == b'_' && before.is_some_and(char::is_alphanumeric));
                let may_close = before.is_some_and(|ch| !is_js_whitespace(ch))
                    && !(marker == b'_' && after.is_some_and(char::is_alphanumeric));
                if if marker == b'~' {
                    width != 2
                } else {
                    width > 3
                } {
                    output.push_str(&text[cursor..close]);
                } else {
                    delimit(&mut stack, marker, width, may_open, may_close);
                }
                cursor = close;
            }
            Some(b'<') => {
                let after = &text[cursor + 1..];
                let end = after
                    .find(['<', '>'])
                    .filter(|&at| after.as_bytes()[at] == b'>');
                if let Some(end) = end {
                    let next = cursor + end + 2;
                    append_literal(output, &text[cursor..next], in_table);
                    cursor = next;
                } else {
                    output.push_str("&lt;");
                    cursor += 1;
                }
            }
            Some(b'\\' | b'[' | b'!') => {
                output.push(char::from(bytes[cursor]));
                cursor += 1;
            }
            None => break,
            _ => unreachable!("the text scanner stops only on punctuation"),
        }
    }
    while stack.len() > 1 {
        let frame = stack.pop().unwrap();
        let parent = &mut stack.last_mut().unwrap().html;
        parent.extend(std::iter::repeat_n(char::from(frame.marker), frame.width));
        parent.push_str(&frame.html);
    }
    stack.pop().unwrap().html
}

fn closing_code_span(text: &str, from: usize, width: usize) -> Option<usize> {
    let mut cursor = from;
    while let Some(index) = text[cursor..].find('`') {
        let start = cursor + index;
        let length = run(&text.as_bytes()[start..], b'`');
        if length == width {
            return Some(start);
        }
        cursor = start + length;
    }
    None
}

fn delimit(
    stack: &mut Vec<Emphasis>,
    marker: u8,
    mut width: usize,
    may_open: bool,
    may_close: bool,
) {
    while may_close && width > 0 && stack.len() > 1 {
        let frame = stack.last_mut().unwrap();
        if frame.marker != marker {
            break;
        }
        if frame.width == 3 && width < 3 {
            frame.html = wrapped(marker, width, &frame.html);
            frame.width -= width;
            return;
        }
        if frame.width > width {
            break;
        }
        let frame = stack.pop().unwrap();
        stack
            .last_mut()
            .unwrap()
            .html
            .push_str(&wrapped(marker, frame.width, &frame.html));
        width -= frame.width;
    }
    // A fixed depth keeps rendering predictably bounded for malformed input.
    if may_open && width > 0 && stack.len() < 33 {
        stack.push(Emphasis::new(marker, width));
    } else {
        stack
            .last_mut()
            .unwrap()
            .html
            .extend(std::iter::repeat_n(char::from(marker), width));
    }
}

fn wrapped(marker: u8, width: usize, body: &str) -> String {
    let (left, right) = match (marker, width) {
        (b'~', 2) => ("<del>", "</del>"),
        (_, 1) => ("<em>", "</em>"),
        (_, 2) => ("<strong>", "</strong>"),
        (_, 3) => ("<strong><em>", "</em></strong>"),
        _ => unreachable!("only single, double and triple emphasis are supported"),
    };
    format!("{left}{body}{right}")
}

#[cfg(test)]
mod tests {
    use super::{
        contains_attachment_reference, render_markdown, render_markdown_with_attachments,
        without_attachment_references,
    };

    #[test]
    fn stripping_private_attachment_spans_preserves_inert_markup_and_exact_neighbors() {
        let id = uuid::Uuid::new_v4();
        let attachment = format!("![этот файл](attachment:{id})");
        let source = format!(
            "α  {attachment}  β\n\n`{attachment}`\n\n```\n{attachment}\n```\n\n> {attachment} конец"
        );
        let expected = format!("α    β\n\n`{attachment}`\n\n```\n{attachment}\n```\n\n>  конец");
        assert_eq!(without_attachment_references(&source, &[id]), expected);
    }

    #[test]
    fn detects_live_attachment_references_without_treating_code_as_links() {
        let id = uuid::Uuid::new_v4();
        let token = format!("attachment:{id}");
        let reference = format!("![alt]({token})");
        for live in [
            reference.clone(),
            format!("- [Файл]({token})"),
            format!("![]({token})"),
        ] {
            assert!(contains_attachment_reference(&live, &[id]));
        }
        for inert in [
            format!("`{reference}`"),
            format!("~~~\n{reference}\n~~~"),
            format!(r"\[пример]({token})"),
            format!("[]({token})"),
        ] {
            assert!(!contains_attachment_reference(&inert, &[id]), "{inert}");
        }
        assert!(!contains_attachment_reference(
            &reference,
            &[uuid::Uuid::nil()]
        ));
    }

    #[test]
    fn page_markup_resolves_only_current_page_files_and_preserves_alt_text() {
        let id = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        let source = format!(
            r#"![«A» \[B\] <tag>]({0}:{id})
![missing](attachment:bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb)
![external](https://example.test/cat.png)
`![code]({0}:{id})`
[Файл]({0}:{id})"#,
            "attachment"
        );
        let rendered = render_markdown_with_attachments(&source, &|lookup, inline| {
            (lookup.to_string() == id).then_some(if inline { "/a/image" } else { "/a" })
        });
        assert_eq!(rendered.as_str().matches("<img ").count(), 1);
        assert!(
            rendered
                .as_str()
                .contains(r#"src="/a/image" alt="«A» [B] &lt;tag&gt;""#)
        );
        assert!(
            rendered
                .as_str()
                .contains("[missing](attachment:bbbbbbbb-bbbb-bbbb-bbbb-bbbbbbbbbbbb)")
        );
        assert!(!rendered.as_str().contains(r#"src="https://example.test"#));
        assert!(rendered.as_str().contains(r#"<a href="/a">Файл</a>"#));
        assert!(!render_markdown(&source).as_str().contains("<img "));
    }

    #[derive(serde::Deserialize)]
    struct ExtendedFixture {
        name: String,
        source: String,
        html: String,
    }

    #[test]
    fn matches_the_shared_extended_markdown_fixtures() {
        let fixtures: Vec<ExtendedFixture> =
            serde_json::from_str(include_str!("markdown-extended-fixtures.json")).unwrap();
        for fixture in fixtures {
            assert_eq!(
                render_markdown(&fixture.source).as_str(),
                fixture.html,
                "{}",
                fixture.name
            );
        }
    }

    #[test]
    fn toolbar_markdown_uses_only_allowed_html() {
        let source = "# Первый\n## Второй\n### Третий\n#### Обычный\n\nДо **важно** и _термин_. ***оба***.\n\n- Первый\n- Второй\n\n2. A\n3. B\n\n> Цитата\n\nЯзык: `` `**x**` ``\n\n```rust\n<b>не тег</b>\n```";
        assert_eq!(
            render_markdown(source).as_str(),
            concat!(
                "<h2>Первый</h2><h3>Второй</h3><h4>Третий</h4><p>#### Обычный</p>",
                "<p>До <strong>важно</strong> и <em>термин</em>. <strong><em>оба</em></strong>.</p>",
                "<ul><li>Первый</li><li>Второй</li></ul><ol start=\"2\"><li>A</li><li>B</li></ol>",
                "<blockquote>Цитата</blockquote><p>Язык: <code>`**x**`</code></p>",
                "<pre><code>&lt;b&gt;не тег&lt;/b&gt;</code></pre>",
            )
        );
    }

    #[test]
    fn escapes_literal_markup_and_keeps_delimiter_rules() {
        assert_eq!(
            render_markdown(r#"<img src="https://example.test/asset"> \*слово\* snake_case **a *b*** `<b>&</b>`"#).as_str(),
            "<p>&lt;img src=&quot;https://example.test/asset&quot;&gt; *слово* snake_case <strong>a <em>b</em></strong> <code>&lt;b&gt;&amp;&lt;/b&gt;</code></p>",
        );
    }

    #[test]
    fn fence_closer_accepts_up_to_three_horizontal_indent_characters() {
        assert_eq!(
            render_markdown("~~~lang\nкод\n    ~~~\n**после**").as_str(),
            "<pre><code>код\n    ~~~\n**после**</code></pre>",
        );
        assert_eq!(
            render_markdown("~~~lang\nкод\n \t ~~~ \t\n**после**").as_str(),
            "<pre><code>код</code></pre><p><strong>после</strong></p>",
        );
    }

    #[test]
    fn renders_the_editor_block_grammar_and_leaves_protected_ranges_literal() {
        let source = concat!(
            "   ##\tЗаголовок  \n+\tПервый\n* Второй\n- Третий\n",
            "3)\tТретий\n4. Последний\n>\tЦитата\n>Ещё\n",
            "  \t~~~lang\n<literal>\n ~~~~~ \t\n",
            "  - nested **literal**\n| A | B |\n| --- | :---: |\n| *mark* | <x> |\n",
            "[**link**](https://example.test) <tag data='**literal**'> _visible_  \nЕщё строка",
        );
        assert_eq!(
            render_markdown(source).as_str(),
            concat!(
                "<h3>Заголовок  </h3><ul><li>Первый</li><li>Второй</li><li>Третий</li></ul>",
                "<ol start=\"3\"><li>Третий</li><li>Последний</li></ol><blockquote>Цитата</blockquote><blockquote>Ещё</blockquote>",
                "<pre><code>&lt;literal&gt;</code></pre><pre>  - nested **literal**</pre>",
                "<div class=\"markdown-table-scroll\"><table><thead><tr><th scope=\"col\">A</th>",
                "<th scope=\"col\" class=\"markdown-cell-center\">B</th></tr></thead>",
                "<tbody><tr><td><em>mark</em></td><td class=\"markdown-cell-center\">&lt;x&gt;</td></tr></tbody></table></div>",
                "<p><a href=\"https://example.test/\" target=\"_blank\" rel=\"noopener noreferrer\"><strong>link</strong></a>",
                " &lt;tag data=&#39;**literal**&#39;&gt; <em>visible</em>  </p><p>Ещё строка</p>",
            )
        );
    }
}
