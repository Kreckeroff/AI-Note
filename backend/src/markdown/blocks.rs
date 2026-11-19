use std::fmt::Write as _;

use crate::text::{escape_html as html, js_trim};

use super::inline;

pub(super) fn render(source: &str) -> String {
    let lines: Vec<_> = source.split('\n').collect();
    let mut output = String::new();
    let mut index = 0;
    while let Some(&line) = lines.get(index) {
        if js_trim(line).is_empty() {
            index += 1;
            continue;
        }
        if let Some((marker, width, _)) = fence(line) {
            index = render_fence(&mut output, &lines, index, marker, width);
            continue;
        }
        if let Some(alignments) = table_divider(&lines, index) {
            index = render_table(&mut output, &lines, index, &alignments);
            continue;
        }
        if js_trim(line) == "---"
            && (index == 0 || js_trim(lines[index - 1]).is_empty())
            && lines
                .get(index + 1)
                .is_none_or(|line| js_trim(line).is_empty())
        {
            output.push_str("<hr>");
        } else if let Some(item) = list_item(line) {
            if item.indent == 0 {
                index = render_list(&mut output, &lines, index, item);
                continue;
            }
            let _ = write!(output, "<pre>{}</pre>", html(line));
        } else if let Some((level, body)) = heading(line) {
            let _ = write!(output, "<h{level}>{}</h{level}>", inline(body, false));
        } else if let Some(body) = line.strip_prefix('>') {
            let body = body.strip_prefix([' ', '\t']).unwrap_or(body);
            let _ = write!(output, "<blockquote>{}</blockquote>", inline(body, false));
        } else {
            let _ = write!(output, "<p>{}</p>", inline(line, false));
        }
        index += 1;
    }
    output
}

fn render_fence(
    output: &mut String,
    lines: &[&str],
    start: usize,
    marker: u8,
    width: usize,
) -> usize {
    output.push_str("<pre><code>");
    let mut index = start + 1;
    while let Some(&line) = lines.get(index) {
        if fence(line).is_some_and(|(closer, closing_width, rest)| {
            closer == marker && closing_width >= width && js_trim(rest).is_empty()
        }) {
            break;
        }
        if index > start + 1 {
            output.push('\n');
        }
        output.push_str(&html(line));
        index += 1;
    }
    output.push_str("</code></pre>");
    index + 1
}

fn fence(line: &str) -> Option<(u8, usize, &str)> {
    let original = line;
    let line = line.trim_start_matches([' ', '\t']);
    if original.len() - line.len() > 3 {
        return None;
    }
    let marker = *line.as_bytes().first()?;
    if !matches!(marker, b'`' | b'~') {
        return None;
    }
    let width = super::run(line.as_bytes(), marker);
    let rest = &line[width..];
    (width >= 3 && (marker != b'`' || !rest.contains('`'))).then_some((marker, width, rest))
}

fn heading(line: &str) -> Option<(usize, &str)> {
    let original = line;
    let line = line.trim_start_matches(' ');
    let width = super::run(line.as_bytes(), b'#');
    if original.len() - line.len() > 3 || !(1..=3).contains(&width) {
        return None;
    }
    Some((width + 1, after_separator(&line[width..])?))
}

fn after_separator(text: &str) -> Option<&str> {
    Some(
        text.strip_prefix([' ', '\t'])?
            .trim_start_matches([' ', '\t']),
    )
}

struct ListItem<'a> {
    indent: usize,
    number: Option<u32>,
    body: &'a str,
}

fn list_item(line: &str) -> Option<ListItem<'_>> {
    let body = line.trim_start_matches([' ', '\t']);
    let indent = line[..line.len() - body.len()]
        .bytes()
        .fold(0, |columns, ch| {
            columns + if ch == b'\t' { 4 - columns % 4 } else { 1 }
        });
    if let Some(rest) = body.strip_prefix(['-', '+', '*']) {
        return Some(ListItem {
            indent,
            number: None,
            body: after_separator(rest)?,
        });
    }
    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
    if !(1..=9).contains(&digits) {
        return None;
    }
    let rest = body[digits..].strip_prefix(['.', ')'])?;
    Some(ListItem {
        indent,
        number: Some(body[..digits].parse().ok()?),
        body: after_separator(rest)?,
    })
}

fn task_item(body: &str) -> Option<(bool, &str)> {
    let (checked, rest) = if let Some(rest) = body.strip_prefix("[ ]") {
        (false, rest)
    } else {
        (
            true,
            body.strip_prefix("[x]")
                .or_else(|| body.strip_prefix("[X]"))?,
        )
    };
    Some((
        checked,
        if rest.is_empty() {
            rest
        } else {
            after_separator(rest)?
        },
    ))
}

struct ListFrame {
    indent: usize,
    tag: &'static str,
}

fn render_list<'a>(
    output: &mut String,
    lines: &[&'a str],
    mut index: usize,
    mut item: ListItem<'a>,
) -> usize {
    let mut frames: Vec<ListFrame> = Vec::new();
    loop {
        let tag = if item.number.is_some() { "ol" } else { "ul" };
        while frames.last().is_some_and(|frame| {
            frame.indent > item.indent || frame.indent == item.indent && frame.tag != tag
        }) {
            let frame = frames.pop().unwrap();
            let _ = write!(output, "</li></{}>", frame.tag);
        }
        if frames
            .last()
            .is_some_and(|frame| frame.indent == item.indent)
        {
            output.push_str("</li>");
        } else {
            if let Some(start) = item.number.filter(|&number| number != 1) {
                let _ = write!(output, "<ol start=\"{start}\">");
            } else {
                let _ = write!(output, "<{tag}>");
            }
            frames.push(ListFrame {
                indent: item.indent,
                tag,
            });
        }

        if item.number.is_none()
            && let Some((checked, body)) = task_item(item.body)
        {
            let checked = if checked { " checked" } else { "" };
            let label = if body.is_empty() {
                "aria-label=\"Чекбокс пункта\"".to_owned()
            } else {
                format!("aria-labelledby=\"markdown-task-{index}\"")
            };
            let _ = write!(
                output,
                "<li class=\"markdown-task\"><input type=\"checkbox\" disabled{checked} {label}>"
            );
            if !body.is_empty() {
                let _ = write!(
                    output,
                    " <span id=\"markdown-task-{index}\">{}</span>",
                    inline(body, false)
                );
            }
        } else {
            let _ = write!(output, "<li>{}", inline(item.body, false));
        }
        index += 1;
        let Some(next) = lines.get(index).and_then(|line| list_item(line)) else {
            break;
        };
        if table_divider(lines, index).is_some() {
            break;
        }
        item = next;
    }
    for frame in frames.into_iter().rev() {
        let _ = write!(output, "</li></{}>", frame.tag);
    }
    index
}

fn cells(line: &str) -> Option<Vec<&str>> {
    let line = line.trim_matches([' ', '\t']);
    let mut pipes = Vec::new();
    let mut escaped = false;
    for (index, byte) in line.bytes().enumerate() {
        if byte == b'|' && !escaped {
            pipes.push(index);
        }
        escaped = byte == b'\\' && !escaped;
    }
    let leading = usize::from(*pipes.first()? == 0);
    let trailing = usize::from(*pipes.last()? == line.len() - 1);
    let end = line.len() - trailing;
    if leading > end {
        return None;
    }
    let mut start = leading;
    Some(
        pipes[leading..pipes.len() - trailing]
            .iter()
            .copied()
            .chain([end])
            .map(|next| {
                let cell = line[start..next].trim_matches([' ', '\t']);
                start = next + 1;
                cell
            })
            .collect(),
    )
}

enum Alignment {
    Default,
    Left,
    Center,
    Right,
}

impl Alignment {
    fn from_divider(cell: &str) -> Option<Self> {
        let left = cell.starts_with(':');
        let right = cell.ends_with(':');
        let dashes = cell.trim_matches(':');
        if dashes.len() < 3
            || dashes.len() + usize::from(left) + usize::from(right) != cell.len()
            || !dashes.bytes().all(|byte| byte == b'-')
        {
            return None;
        }
        Some(match (left, right) {
            (true, true) => Self::Center,
            (true, false) => Self::Left,
            (false, true) => Self::Right,
            _ => Self::Default,
        })
    }

    fn attribute(&self) -> &'static str {
        match self {
            Self::Default => "",
            Self::Left => " class=\"markdown-cell-left\"",
            Self::Center => " class=\"markdown-cell-center\"",
            Self::Right => " class=\"markdown-cell-right\"",
        }
    }
}

fn table_divider(lines: &[&str], start: usize) -> Option<Vec<Alignment>> {
    if !lines[start].contains('|') {
        return None;
    }
    cells(lines.get(start + 1)?)?
        .into_iter()
        .map(Alignment::from_divider)
        .collect()
}

fn render_table(
    output: &mut String,
    lines: &[&str],
    start: usize,
    alignments: &[Alignment],
) -> usize {
    let last = start
        + 2
        + lines[start + 2..]
            .iter()
            .take_while(|line| line.contains('|'))
            .count();
    let rows: Option<Vec<_>> = std::iter::once(lines[start])
        .chain(lines[start + 2..last].iter().copied())
        .map(|line| cells(line).filter(|row| row.len() == alignments.len()))
        .collect();
    let Some(rows) = rows else {
        let _ = write!(
            output,
            "<pre>{}</pre>",
            html(&lines[start..last].join("\n"))
        );
        return last;
    };
    output.push_str("<div class=\"markdown-table-scroll\"><table><thead>");
    render_table_row(output, &rows[0], alignments, true);
    output.push_str("</thead>");
    if rows.len() > 1 {
        output.push_str("<tbody>");
        for row in &rows[1..] {
            render_table_row(output, row, alignments, false);
        }
        output.push_str("</tbody>");
    }
    output.push_str("</table></div>");
    last
}

fn render_table_row(output: &mut String, row: &[&str], alignments: &[Alignment], header: bool) {
    let (tag, scope) = if header {
        ("th", " scope=\"col\"")
    } else {
        ("td", "")
    };
    output.push_str("<tr>");
    for (cell, alignment) in row.iter().zip(alignments) {
        let _ = write!(
            output,
            "<{tag}{scope}{}>{}</{tag}>",
            alignment.attribute(),
            inline(cell, true)
        );
    }
    output.push_str("</tr>");
}
