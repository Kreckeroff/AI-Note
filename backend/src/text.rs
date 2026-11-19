use std::borrow::Cow;

use unicode_normalization::UnicodeNormalization;

pub(crate) fn escape_html(input: &str) -> String {
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

pub(crate) fn is_js_whitespace(ch: char) -> bool {
    matches!(ch,
        '\t'..='\r' | ' ' | '\u{00a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}'
        | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

pub(crate) fn js_trim(value: &str) -> &str {
    value.trim_matches(is_js_whitespace)
}

pub(crate) fn fold_name(value: &str) -> String {
    value
        .nfc()
        .collect::<String>()
        .to_lowercase()
        .nfc()
        .collect()
}

pub(crate) fn normalize_line_endings(text: &str) -> Cow<'_, str> {
    if text.contains('\r') {
        Cow::Owned(text.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(text)
    }
}

// The visual editor stores literal Markdown text. Escape each CommonMark
// backslash-escapable character and undo only those escapes when displaying it.
pub(crate) fn text_to_markdown(text: &str) -> String {
    let mut markdown = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_ascii_punctuation() {
            markdown.push('\\');
        }
        markdown.push(ch);
    }
    markdown
}

pub(crate) fn markdown_to_text(markdown: &str) -> String {
    let markdown = normalize_line_endings(markdown);
    let mut text = String::with_capacity(markdown.len());
    let mut chars = markdown.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' || !chars.peek().is_some_and(char::is_ascii_punctuation) {
            text.push(ch);
        } else if let Some(punctuation) = chars.next() {
            text.push(punctuation);
        }
    }
    text
}
