use std::fmt::Write;

use crate::{
    db::{PageContentFormat, PageCover, PageHistory, PageRevisionSnapshot},
    sections::{CoverView, PageAttachmentView},
    workspace::{page_history_path, page_path},
};
use uuid::Uuid;

use super::{
    PageAssets,
    editor::{attachment_list, cover_art},
    html, page,
};

pub(crate) struct Links {
    pub preview: bool,
    current: String,
    history: String,
}

impl Links {
    pub(crate) fn new(tenant: Uuid, space: Uuid, page: Uuid, preview: bool) -> Self {
        Self {
            preview,
            current: page_path(tenant, space, page),
            history: page_history_path(tenant, space, page),
        }
    }

    fn with_query(&self, path: &str, query: &str) -> String {
        let query = match (query, self.preview) {
            ("", false) => return path.to_owned(),
            ("", true) => "preview=1".to_owned(),
            (query, false) => query.to_owned(),
            (query, true) => format!("{query}&preview=1"),
        };
        format!("{path}?{query}")
    }

    pub(crate) fn list(&self, before: Option<i64>) -> String {
        let query = before.map_or_else(String::new, |before| format!("before={before}"));
        self.with_query(&self.history, &query)
    }

    pub(crate) fn revision(&self, number: i64) -> String {
        self.with_query(&format!("{}/{number}", self.history), "")
    }

    fn confirmation(&self, number: i64) -> String {
        format!("{}/{number}?restore=1", self.history)
    }

    fn restore(&self, number: i64) -> String {
        format!("{}/{number}/restore", self.history)
    }

    fn cover(&self, number: i64) -> String {
        format!("{}/{number}/cover", self.history)
    }

    fn attachment(&self, number: i64, id: Uuid) -> String {
        format!("{}/{number}/attachments/{id}", self.history)
    }

    fn compare_path(&self) -> String {
        format!("{}/compare", self.history)
    }

    fn compare(&self, from: i64, to: i64) -> String {
        self.with_query(&self.compare_path(), &format!("from={from}&to={to}"))
    }
}

fn document(title: &str, header: &str, content: &str) -> String {
    let body = format!(
        r#"<main class="history-page space-shell">
    <header class="history-page-header">{header}</header>
    {content}
</main>"#
    );
    page(
        title,
        &body,
        PageAssets::Workspace {
            menu: false,
            form: false,
        },
        None,
    )
}

fn link(to: &str, label: &str) -> String {
    format!(
        r#"<a class="space-action space-secondary" href="{}">{}</a>"#,
        html(to),
        html(label)
    )
}

fn actions(links: &Links, current: i64, buttons: &str) -> String {
    let current_to = if links.preview {
        links.revision(current)
    } else {
        links.current.clone()
    };
    format!(
        r#"<nav class="history-page-actions" aria-label="Действия с редакциями">{}{buttons}</nav>"#,
        link(&current_to, "К текущей странице"),
    )
}

fn in_history(links: &Links) -> String {
    link(&links.list(None), "К истории")
}

fn metadata(author: Option<&str>, recorded_at: Option<&str>) -> String {
    if author.is_none() && recorded_at.is_none() {
        return "<span>Исходный снимок</span>".to_owned();
    }
    let author = author.map_or_else(String::new, |author| {
        format!("<span>{}</span>", html(author))
    });
    let time = recorded_at.map_or_else(String::new, |timestamp| {
        let readable = timestamp.get(..16).map_or_else(
            || html(timestamp),
            |minutes| format!("{} UTC", html(&minutes.replace('T', " "))),
        );
        format!(r#"<time datetime="{}">{readable}</time>"#, html(timestamp))
    });
    format!("{author}{time}")
}

pub(crate) fn list(links: &Links, history: &PageHistory) -> String {
    let current = history.current_revision;
    let choose = link(
        &links.with_query(&links.compare_path(), ""),
        "Сравнить редакции",
    );
    let header = format!(
        "<h1>История страницы</h1><p>Текущая редакция — {current}</p>{}",
        actions(links, current, &choose),
    );
    document("История страницы", &header, &timeline_list(links, history))
}

fn timeline_list(links: &Links, history: &PageHistory) -> String {
    let mut content = String::new();
    if history.items.is_empty() {
        content.push_str("<p>Более ранних редакций нет.</p>");
    } else {
        content.push_str(r#"<ol class="history-page-list">"#);
        for entry in &history.items {
            let revision = entry.revision;
            let (mark, main) = if revision == history.current_revision {
                (r#" aria-current="true""#, "<span>Текущая редакция</span>")
            } else {
                ("", "")
            };
            let _ = write!(
                content,
                r#"<li class="history-page-row"><a href="{}"{mark}>Редакция {revision}</a><div class="history-page-meta">{main}{}</div></li>"#,
                html(&links.revision(revision)),
                metadata(entry.author.as_deref(), entry.recorded_at.as_deref()),
            );
        }
        content.push_str("</ol>");
    }
    if let Some(before) = history.next_before {
        content.push_str(&link(&links.list(Some(before)), "Более ранние редакции"));
    }
    content
}

fn with_timeline(links: &Links, history: &PageHistory, body: &str) -> String {
    format!(
        r#"<div class="history-page-layout">
    <div class="history-page-main">{body}</div>
    <aside class="history-page-sidebar" aria-labelledby="history-timeline">
      <h2 id="history-timeline">История</h2>
      {}
    </aside>
</div>"#,
        timeline_list(links, history)
    )
}

fn cover(links: &Links, snapshot: &PageRevisionSnapshot) -> String {
    if snapshot.cover == PageCover::None {
        return "<p>Без обложки</p>".to_owned();
    }
    let image_to =
        matches!(snapshot.cover, PageCover::Image(_)).then(|| links.cover(snapshot.revision));
    let cover = CoverView {
        value: snapshot.cover.value(),
        image_to,
    };
    format!(
        r#"<div class="page-cover-preview">{}</div>"#,
        cover_art(&cover)
    )
}

fn escape_source(source: &str) -> String {
    // The HTML parser normalizes literal carriage returns before decoding character references.
    html(source).replace('\r', "&#13;")
}

fn source_block(source: &str) -> String {
    // A sentinel newline is discarded by the HTML parser, preserving the original first byte.
    format!(
        "<pre class=\"history-source\" tabindex=\"0\">\n{}</pre>",
        escape_source(source)
    )
}

pub(crate) fn snapshot(
    links: &Links,
    snapshot: &PageRevisionSnapshot,
    history: &PageHistory,
    confirming: bool,
) -> String {
    let number = snapshot.revision;
    let current = snapshot.current_revision.max(history.current_revision);
    let heading = format!("Редакция {number}");
    let compare = link(&links.compare(number, current), "Сравнить с текущей");
    let history_to = in_history(links);
    let restore = if number == current || links.preview || confirming {
        String::new()
    } else {
        link(&links.confirmation(number), "Восстановить")
    };
    let header = format!(
        r#"<p>{heading}</p><h1>{}</h1><div class="history-page-meta">{}</div>{}"#,
        html(&snapshot.title),
        metadata(snapshot.author.as_deref(), snapshot.recorded_at.as_deref()),
        actions(links, current, &format!("{history_to}{compare}{restore}")),
    );
    let confirmation = if number == current {
        "<p>Это текущая редакция.</p>".to_owned()
    } else if links.preview {
        "<p>Чтобы восстановить редакцию, сохраните или отмените черновик и откройте историю из режима чтения.</p>".to_owned()
    } else if confirming {
        confirmation_form(links, number, current, None)
    } else {
        String::new()
    };
    let format = match snapshot.format {
        PageContentFormat::Literal => "Простой текст",
        PageContentFormat::Markdown => "Markdown",
    };
    let attachments = if snapshot.attachments.is_empty() {
        String::new()
    } else {
        let files: Vec<_> = snapshot
            .attachments
            .iter()
            .cloned()
            .map(|file| {
                let url = links.attachment(snapshot.revision, file.id);
                PageAttachmentView::at_url(file, url, true)
            })
            .collect();
        attachment_list(&files)
    };
    let content = format!(
        r#"{confirmation}
        <section aria-labelledby="history-cover"><h2 id="history-cover">Обложка</h2>{}</section>
        <section aria-labelledby="history-markdown"><h2 id="history-markdown">Исходный Markdown</h2><p>Формат — {format}</p>{}</section>
        {attachments}"#,
        cover(links, snapshot),
        source_block(&snapshot.source)
    );
    document(&heading, &header, &with_timeline(links, history, &content))
}

fn restore_form(links: &Links, number: i64, base: i64, label: &str) -> String {
    format!(
        r#"<form method="post" action="{}" class="history-page-actions">
    <input type="hidden" name="baseRevision" value="{base}">
    {}
    <button class="space-action primary-action" type="submit">{label}</button>
</form>"#,
        html(&links.restore(number)),
        link(&links.revision(number), "Отмена"),
    )
}

fn confirmation_form(links: &Links, number: i64, current: i64, error: Option<&str>) -> String {
    let error = error.map_or_else(String::new, |message| {
        format!(
            r#"<p class="form-error" role="alert">{}</p>"#,
            html(message)
        )
    });
    format!(
        r#"<section aria-labelledby="history-restore">
 <h2 id="history-restore">Восстановить редакцию {number}</h2>
 {error}
 <p>Текущая редакция — {current}. Название, текст, обложка и вложения редакции {number} будут опубликованы как новая редакция.</p>
 {}
</section>"#,
        restore_form(links, number, current, "Подтвердить восстановление"),
    )
}

pub(crate) fn restore_conflict(
    links: &Links,
    number: i64,
    history: &PageHistory,
    error: &str,
) -> String {
    let current = history.current_revision;
    let history_to = in_history(links);
    let compare = link(&links.compare(number, current), "Сравнить редакции");
    let header = format!(
        "<h1>Восстановление страницы</h1>{}",
        actions(links, current, &format!("{history_to}{compare}"))
    );
    document(
        "Восстановление страницы",
        &header,
        &with_timeline(
            links,
            history,
            &confirmation_form(links, number, current, Some(error)),
        ),
    )
}

pub(crate) fn restore_retry(links: &Links, number: i64, base: i64, error: &str) -> String {
    document(
        "Восстановление страницы",
        "<h1>Восстановление страницы</h1>",
        &format!(
            r#"<p class="form-error" role="alert">{}</p>{}"#,
            html(error),
            restore_form(links, number, base, "Повторить восстановление"),
        ),
    )
}

fn compare_form(links: &Links, from: Option<i64>, to: i64, history: &PageHistory) -> String {
    let preview = if links.preview {
        r#"<input type="hidden" name="preview" value="1">"#
    } else {
        ""
    };
    let mut revisions: Vec<_> = history.items.iter().map(|entry| entry.revision).collect();
    revisions.extend(from);
    revisions.push(to);
    revisions.sort_unstable_by(|a, b| b.cmp(a));
    revisions.dedup();
    let mut choices = String::new();
    for revision in revisions {
        let _ = write!(choices, r#"<option value="{revision}"></option>"#);
    }
    let from = from.map_or_else(String::new, |number| number.to_string());
    format!(
        r#"<form method="get" action="{}" class="history-compare-form">
     {preview}
     <label>Было · номер редакции <input name="from" inputmode="numeric" pattern="[1-9][0-9]*" maxlength="19" list="history-compare-choices" value="{from}" required></label>
     <label>Стало · номер редакции <input name="to" inputmode="numeric" pattern="[1-9][0-9]*" maxlength="19" list="history-compare-choices" value="{to}" required></label>
     <button class="space-action primary-action" type="submit">Сравнить</button>
     <datalist id="history-compare-choices">{choices}</datalist>
</form>"#,
        html(&links.compare_path()),
    )
}

pub(crate) fn choose_comparison(links: &Links, history: &PageHistory) -> String {
    let header = format!(
        "<h1>Сравнение редакций</h1>{}",
        actions(links, history.current_revision, &in_history(links))
    );
    document(
        "Сравнение редакций",
        &header,
        &with_timeline(
            links,
            history,
            &compare_form(links, None, history.current_revision, history),
        ),
    )
}

pub(crate) fn comparison(
    links: &Links,
    from: &PageRevisionSnapshot,
    to: &PageRevisionSnapshot,
    history: &PageHistory,
) -> String {
    let history_to = in_history(links);
    let from_link = link(
        &links.revision(from.revision),
        &format!("Было · редакция {}", from.revision),
    );
    let to_link = link(
        &links.revision(to.revision),
        &format!("Стало · редакция {}", to.revision),
    );
    let header = format!(
        "<h1>Сравнение редакций</h1>{}",
        actions(
            links,
            history.current_revision,
            &format!("{history_to}{from_link}{to_link}")
        )
    );
    let title = if from.title == to.title {
        "<p>Название не изменилось.</p>".to_owned()
    } else {
        format!(
            r#"<dl class="history-page-meta"><div><dt>Было</dt><dd>{}</dd></div><div><dt>Стало</dt><dd>{}</dd></div></dl>"#,
            html(&from.title),
            html(&to.title),
        )
    };
    let content = format!(
        r#"{}
        <section aria-labelledby="history-compare-title"><h2 id="history-compare-title">Название</h2>{title}</section>
        <section aria-labelledby="history-compare-source"><h2 id="history-compare-source">Исходный Markdown</h2>{}</section>"#,
        compare_form(links, Some(from.revision), to.revision, history),
        source_change(&from.source, &to.source),
    );
    document(
        "Сравнение редакций",
        &header,
        &with_timeline(links, history, &content),
    )
}

fn source_change(from: &str, to: &str) -> String {
    if from == to {
        return "<p>Исходный Markdown не изменился.</p>".to_owned();
    }
    let Some(lines) = line_diff(from, to) else {
        return format!(
            r#"<p>Исходники целиком</p><div class="history-diff-columns">
        <section><h3>Было</h3>{}</section><section><h3>Стало</h3>{}</section></div>"#,
            source_block(from),
            source_block(to),
        );
    };
    let mut output = String::from(
        r#"<p>− Было · + Стало</p><div class="history-diff" tabindex="0" aria-label="Построчные изменения исходника">"#,
    );
    for line in lines {
        let (change, meaning, mark, text) = match line {
            Line::Same(text) => ("same", "Без правок", " ", text),
            Line::Removed(text) => ("removed", "Было", "−", text),
            Line::Added(text) => ("added", "Стало", "+", text),
        };
        let _ = write!(
            output,
            r#"<div class="history-diff-line" data-change="{change}"><span aria-label="{meaning}">{mark}</span><code>{}</code></div>"#,
            escape_source(text),
        );
    }
    output.push_str("</div>");
    output
}

const MAX_DIFF_LINES: usize = 2_400;
const MAX_DIFF_CELLS: usize = 160_000;

#[derive(Debug, PartialEq, Eq)]
enum Line<'a> {
    Same(&'a str),
    Removed(&'a str),
    Added(&'a str),
}

fn line_diff<'a>(from: &'a str, to: &'a str) -> Option<Vec<Line<'a>>> {
    let lines = |source: &'a str| -> Vec<&'a str> {
        // Preserve trailing empty fragments while treating an empty document as zero lines.
        let limit = if source.is_empty() {
            0
        } else {
            MAX_DIFF_LINES + 1
        };
        source.split('\n').take(limit).collect()
    };
    let (from, to) = (lines(from), lines(to));
    let columns = to.len() + 1;
    let cells = (from.len() + 1) * columns;
    if from.len() + to.len() > MAX_DIFF_LINES || cells > MAX_DIFF_CELLS {
        return None;
    }
    let mut lengths = vec![0u16; cells];
    let cell = |row, column| row * columns + column;
    for (row, before) in from.iter().enumerate().rev() {
        for (column, after) in to.iter().enumerate().rev() {
            lengths[cell(row, column)] = if before == after {
                1 + lengths[cell(row + 1, column + 1)]
            } else {
                lengths[cell(row + 1, column)].max(lengths[cell(row, column + 1)])
            };
        }
    }

    let (mut row, mut column) = (0, 0);
    let mut lines = Vec::new();
    while let (Some(before), Some(after)) = (from.get(row), to.get(column)) {
        if before == after {
            lines.push(Line::Same(before));
            row += 1;
            column += 1;
        } else if lengths[cell(row + 1, column)] >= lengths[cell(row, column + 1)] {
            lines.push(Line::Removed(before));
            row += 1;
        } else {
            lines.push(Line::Added(after));
            column += 1;
        }
    }
    lines.extend(from[row..].iter().map(|line| Line::Removed(line)));
    lines.extend(to[column..].iter().map(|line| Line::Added(line)));
    Some(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_observes_empty_and_whitespace_only_lines() {
        assert_eq!(
            line_diff("\n# Title\n  ", "\n# Title\n"),
            Some(vec![
                Line::Same(""),
                Line::Same("# Title"),
                Line::Removed("  "),
                Line::Added("")
            ])
        );
        assert_eq!(
            line_diff("", "\n"),
            Some(vec![Line::Added(""), Line::Added("")])
        );
        assert_eq!(line_diff("", "text"), Some(vec![Line::Added("text")]));
        assert_eq!(
            line_diff("", "text\n"),
            Some(vec![Line::Added("text"), Line::Added("")])
        );
        assert_eq!(
            line_diff("text\n", ""),
            Some(vec![Line::Removed("text"), Line::Removed("")])
        );
        assert_eq!(line_diff("text", ""), Some(vec![Line::Removed("text")]));
    }

    #[test]
    fn large_diffs_use_both_full_exact_sources() {
        let from = "\n<before>\r\n".repeat(800);
        let to = "\n<after> \n".repeat(800);
        assert!(line_diff(&from, &to).is_none());
        let result = source_change(&from, &to);
        assert!(result.contains(&source_block(&from)));
        assert!(result.contains(&source_block(&to)));
        assert!(result.contains("<h3>Было</h3>"));
        assert!(result.contains("<h3>Стало</h3>"));
    }

    #[test]
    fn native_source_does_not_lose_its_first_or_last_line() {
        assert_eq!(
            source_block("\n<text>\r\n \n"),
            "<pre class=\"history-source\" tabindex=\"0\">\n\n&lt;text&gt;&#13;\n \n</pre>"
        );
    }

    #[test]
    fn preview_links_preserve_mode_when_switching_revisions() {
        let links = Links::new(Uuid::nil(), Uuid::nil(), Uuid::nil(), true);
        assert!(
            links
                .list(Some(4))
                .ends_with("/revisions?before=4&preview=1")
        );
        assert!(links.revision(3).ends_with("/revisions/3?preview=1"));
        assert!(
            links
                .compare(3, 5)
                .ends_with("/revisions/compare?from=3&to=5&preview=1")
        );
        let snapshot = PageRevisionSnapshot {
            revision: 3,
            current_revision: 5,
            author: None,
            recorded_at: None,
            title: String::from("<snapshot>"),
            source: String::from("  source"),
            format: PageContentFormat::Markdown,
            cover: PageCover::None,
            attachments: vec![crate::db::Attachment {
                id: Uuid::nil(),
                name: "Текст < 5.txt".to_owned(),
                size: 20,
                image: false,
                publication: None,
            }],
        };
        let history = PageHistory {
            current_revision: 5,
            items: Vec::new(),
            next_before: Some(3),
        };
        let preview = super::snapshot(&links, &snapshot, &history, true);
        assert!(!preview.contains("name=\"baseRevision\""));
        assert!(!preview.contains("/restore"));
        assert!(preview.contains("&lt;snapshot&gt;"));
        assert!(preview.contains("/revisions/5?preview=1\">К текущей странице"));
        assert!(preview.contains("history-page-sidebar"));
        assert!(preview.contains("Текст &lt; 5.txt"));
        assert!(preview.contains("/revisions/3/attachments/00000000-0000-0000-0000-000000000000"));

        let comparison = super::comparison(&links, &snapshot, &snapshot, &history);
        assert!(comparison.contains("id=\"history-compare-choices\""));
        assert!(comparison.contains("<option value=\"3\"></option>"));
        assert!(comparison.contains("history-page-sidebar"));
        assert!(comparison.contains("?before=3&amp;preview=1\""));

        let reading = Links::new(Uuid::nil(), Uuid::nil(), Uuid::nil(), false);
        let confirmation = super::snapshot(&reading, &snapshot, &history, true);
        assert!(confirmation.contains(&format!("{}\">К текущей странице", reading.current)));
        assert!(confirmation.contains("name=\"baseRevision\" value=\"5\""));
        assert!(confirmation.contains("history-page-sidebar"));
    }
}
