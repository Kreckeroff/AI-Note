use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgConnection};

use crate::{
    db::{
        PageAddress,
        page_attachments::{CURRENT_ATTACHMENTS, decode_attachments},
    },
    markdown::render_document,
    text::{markdown_to_text, normalize_line_endings, revision_number},
};

const CONTEXT_LENGTH: usize = 32;
const PAGE_TEXT_LIMIT: usize = 60_000;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AnchorInput {
    revision: String,
    quote: String,
    start: usize,
    end: usize,
    before: String,
    after: String,
}

#[derive(FromRow)]
struct PageSource {
    markdown: String,
    content_is_markdown: bool,
    content_revision: i64,
    attachments: String,
}

pub(super) struct PublishedText {
    pub(super) revision: i64,
    text: String,
}

pub(super) async fn published_text(
    tx: &mut PgConnection,
    address: &PageAddress,
    origin: &str,
) -> Result<PublishedText, sqlx::Error> {
    let source: PageSource = sqlx::query_as(&format!(
        "SELECT p.markdown, p.content_is_markdown, p.content_revision, {CURRENT_ATTACHMENTS} AS attachments
         FROM page_permissions($1, $2, $3, $4) access
         JOIN pages p ON (p.tenant_id, p.space_id, p.id) = ($1, $2, $4)
         WHERE access.can_read"
    ))
    .bind(address.tenant)
    .bind(address.space)
    .bind(address.account)
    .bind(address.page)
    .fetch_one(tx)
    .await?;
    let text = if source.content_is_markdown {
        let attachments = decode_attachments(&source.attachments)?;
        let source = normalize_line_endings(&source.markdown);
        let html = render_document(
            &source,
            &|id, inline| {
                attachments
                    .iter()
                    .find(|file| {
                        file.id == id && file.publication.is_some() && (!inline || file.image)
                    })
                    // The path affects only HTML attributes, which are discarded below.
                    .map(|_| "/")
            },
            origin,
        );
        text_content(html.as_str())
    } else {
        markdown_to_text(&source.markdown)
    };
    Ok(PublishedText {
        revision: source.content_revision,
        text,
    })
}

// The application renderer escapes all text and emits attributes with escaped
// values. Read only the text fragments and undo precisely that escaping.
fn text_content(mut html: &str) -> String {
    let mut text = String::with_capacity(html.len());
    while let Some(index) = html.find(['<', '&']) {
        text.push_str(&html[..index]);
        html = &html[index..];
        if let Some(tag) = html.strip_prefix('<') {
            let Some((_, rest)) = tag.split_once('>') else {
                break;
            };
            html = rest;
        } else if let Some((entity, ch)) = [
            ("&amp;", '&'),
            ("&lt;", '<'),
            ("&gt;", '>'),
            ("&quot;", '"'),
            ("&#39;", '\''),
        ]
        .into_iter()
        .find(|(entity, _)| html.starts_with(entity))
        {
            text.push(ch);
            html = &html[entity.len()..];
        } else {
            text.push('&');
            html = &html[1..];
        }
    }
    text.push_str(html);
    text
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct StoredAnchor {
    pub(super) revision: i64,
    pub(super) quote: String,
    pub(super) start: i32,
    pub(super) end: i32,
    pub(super) before: String,
    pub(super) after: String,
}

impl AnchorInput {
    pub(super) fn validate(self, publication: PublishedText) -> Option<StoredAnchor> {
        if revision_number(&self.revision) != Some(publication.revision)
            || self.start >= self.end
            || self.end > PAGE_TEXT_LIMIT
        {
            return None;
        }
        let chars: Vec<_> = publication.text.chars().collect();
        let quote: String = chars.get(self.start..self.end)?.iter().collect();
        let before: String = chars[self.start.saturating_sub(CONTEXT_LENGTH)..self.start]
            .iter()
            .collect();
        let after: String = chars[self.end..(self.end + CONTEXT_LENGTH).min(chars.len())]
            .iter()
            .collect();
        if self.quote != quote || self.before != before || self.after != after {
            return None;
        }
        Some(StoredAnchor {
            revision: publication.revision,
            quote,
            start: self.start as i32,
            end: self.end as i32,
            before,
            after,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum AnchorState {
    Attached,
    Ambiguous,
    Missing,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AnchorView {
    revision: String,
    quote: String,
    start: Option<i32>,
    end: Option<i32>,
    state: AnchorState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) history_to: Option<String>,
    #[serde(skip)]
    pub(crate) historical_revision: Option<i64>,
}

impl StoredAnchor {
    pub(super) fn view(
        self,
        current_revision: i64,
        publication: Option<&PublishedText>,
        can_edit: bool,
    ) -> AnchorView {
        let location = if current_revision == self.revision {
            Location::Attached(self.start, self.end)
        } else if let Some(publication) = publication {
            self.locate(&publication.text)
        } else {
            Location::Missing
        };
        let (state, start, end) = match location {
            Location::Attached(start, end) => (AnchorState::Attached, Some(start), Some(end)),
            Location::Ambiguous => (AnchorState::Ambiguous, None, None),
            Location::Missing => (AnchorState::Missing, None, None),
        };
        // An absent quote belongs to an older revision and follows history rights.
        let quote = if state == AnchorState::Missing && !can_edit {
            String::new()
        } else {
            self.quote
        };
        AnchorView {
            revision: self.revision.to_string(),
            quote,
            start,
            end,
            state,
            history_to: None,
            historical_revision: (can_edit && current_revision != self.revision)
                .then_some(self.revision),
        }
    }

    fn locate(&self, document: &str) -> Location {
        let quote = self.quote.as_bytes();
        let prefix = prefix_lengths(quote);
        let mut matched = 0;
        let mut first = None;
        let mut multiple = false;
        let mut contextual_match = None;
        for (index, byte) in document.bytes().enumerate() {
            matched = advance_prefix(quote, &prefix, matched, byte);
            if matched != quote.len() {
                continue;
            }
            let end = index + 1;
            let start = end - quote.len();
            if first.is_none() {
                first = Some(start);
            } else {
                multiple = true;
            }
            if document[..start].ends_with(&self.before) && document[end..].starts_with(&self.after)
            {
                if contextual_match.is_some() {
                    return Location::Ambiguous;
                }
                contextual_match = Some(start);
            }
            // Continue with the longest matching prefix to count overlapping
            // matches without rescanning repeated sections of a long page.
            matched = prefix[matched - 1];
        }
        let start = match (first, multiple, contextual_match) {
            (None, _, _) => return Location::Missing,
            (Some(start), false, _) | (_, true, Some(start)) => start,
            _ => return Location::Ambiguous,
        };
        let start = document[..start].chars().count() as i32;
        Location::Attached(start, start + self.end - self.start)
    }
}

fn prefix_lengths(pattern: &[u8]) -> Vec<usize> {
    let mut lengths = vec![0; pattern.len()];
    for index in 1..pattern.len() {
        lengths[index] = advance_prefix(pattern, &lengths, lengths[index - 1], pattern[index]);
    }
    lengths
}

fn advance_prefix(pattern: &[u8], lengths: &[usize], mut matched: usize, byte: u8) -> usize {
    while matched > 0 && byte != pattern[matched] {
        matched = lengths[matched - 1];
    }
    matched + usize::from(byte == pattern[matched])
}

#[derive(Debug, PartialEq, Eq)]
enum Location {
    Attached(i32, i32),
    Ambiguous,
    Missing,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn published_markdown_projection_follows_dom_text_and_scalar_offsets() {
        let html = render_document(
            "# Привет **🚀**\n\n`&lt; < > \" '`",
            &|_, _| None,
            "http://localhost",
        );
        let text = text_content(html.as_str());
        assert_eq!(text, "Привет 🚀&lt; < > \" '");
        let input = AnchorInput {
            revision: "5".to_owned(),
            quote: "🚀&lt;".to_owned(),
            start: 7,
            end: 12,
            before: "Привет ".to_owned(),
            after: " < > \" '".to_owned(),
        };
        let publication = PublishedText { revision: 5, text };
        let anchor = input
            .validate(publication)
            .expect("published selection matches");
        assert_eq!(
            anchor.locate("Теперь 🚀&lt; и ещё"),
            Location::Attached(7, 12)
        );
        assert_eq!(
            anchor.locate("Цитата 🚀&lt; повтор 🚀&lt;"),
            Location::Ambiguous
        );
        assert_eq!(anchor.locate("Новый заголовок"), Location::Missing);
    }

    #[test]
    fn contextual_relocation_handles_non_unique_and_overlapping_occurrences() {
        let anchor = StoredAnchor {
            revision: 1,
            quote: "aa".to_owned(),
            start: 0,
            end: 2,
            before: "prefix ".to_owned(),
            after: " suffix".to_owned(),
        };
        assert_eq!(anchor.locate("aaa"), Location::Ambiguous);
        assert_eq!(
            anchor.locate("aaa prefix aa suffix and more aa"),
            Location::Attached(11, 13)
        );
        assert_eq!(
            anchor.locate("prefix aa suffix prefix aa suffix"),
            Location::Ambiguous
        );
    }
}
