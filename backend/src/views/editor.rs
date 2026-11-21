use std::fmt::Write as _;

use crate::sections::{CoverView, PageAttachmentView, PageEditorForm, SectionScreen};

use super::{PageAssets, client::ClientPage, html, page};

pub(super) fn cover_art(cover: &CoverView) -> String {
    match cover.value.as_str() {
        theme @ ("orbit" | "aurora" | "grid") => format!(
            r#"<div class="page-cover-art" data-cover-theme="{theme}" aria-hidden="true"></div>"#,
        ),
        _ => cover.image_to.as_deref().map_or_else(String::new, |image| {
            format!(
                r#"<div class="page-cover-art" data-cover-theme="custom" aria-hidden="true"><img src="{}" alt=""></div>"#,
                html(image),
            )
        }),
    }
}

fn cover_label(cover: &CoverView) -> &'static str {
    match cover.value.as_str() {
        "orbit" => "Орбита",
        "aurora" => "Сияние",
        "grid" => "Картография",
        "none" => "Без обложки",
        _ => "Своя обложка",
    }
}

fn hidden_field(name: &str, value: &str) -> String {
    format!(
        r#"<input type="hidden" name="{name}" value="{}">"#,
        html(value)
    )
}

pub(super) fn page_contents(tree: &SectionScreen) -> String {
    if let Some(form) = &tree.editor {
        return page_editor(form);
    }
    let mut content = String::new();
    if let Some(edit_to) = tree
        .reading
        .as_ref()
        .and_then(|reading| reading.edit_to.as_ref())
    {
        content.push_str(&format!(
            r#"<a id="edit-page" class="space-action space-secondary document-edit-link" href="{}">Редактировать</a>"#,
            html(edit_to),
        ));
    }
    if let Some(history_to) = tree
        .reading
        .as_ref()
        .and_then(|reading| reading.history_to.as_ref())
    {
        content.push_str(&history_link(history_to, false));
    }
    let body = tree.reading.as_ref().and_then(|reading| {
        match reading.rendered_html.as_ref().map(|html| html.as_str()) {
            Some(rendered) if !rendered.is_empty() => Some(format!(
                r#"<div class="document-text document-markdown">{rendered}</div>"#,
            )),
            None if !reading.text.is_empty() => Some(format!(
                r#"<div class="document-text">{}</div>"#,
                html(&reading.text),
            )),
            _ => None,
        }
    });
    content.push_str(
        body.as_deref()
            .unwrap_or(r#"<p class="document-empty">Пока здесь ничего нет.</p>"#),
    );
    if let Some(reading) = &tree.reading {
        if let Some(template) = reading.template {
            let _ = write!(content, r#"<details class="page-template-origin" id="page-template"><summary>Исходный шаблон · {} · v{}</summary><pre>{}</pre></details>"#, html(template.title), template.revision, html(template.source));
        }
        content.push_str(&attachment_list(&reading.attachments));
        content.push_str(r#"<a class="space-action space-secondary page-create-link" href="?download=markdown">Скачать Markdown</a>"#);
    }
    content
}

pub(super) fn attachment_list(files: &[PageAttachmentView]) -> String {
    let mut result = String::from(
        r#"<section class="page-attachments" aria-labelledby="page-attachments-title">
<header class="page-attachment-heading"><h2 id="page-attachments-title">Вложения</h2></header>"#,
    );
    if files.is_empty() {
        result.push_str(r#"<p class="page-attachment-empty">У страницы пока нет вложений.</p>"#);
    } else {
        result.push_str(r#"<ul class="page-attachment-list">"#);
        for file in files {
            let name = html(&file.name);
            let _ = write!(
                result,
                r#"<li class="page-attachment-row"><span class="page-attachment-meta"><strong>{name}</strong><small>{}</small></span>"#,
                size_label(file.bytes),
            );
            if let Some(path) = &file.download_to {
                let _ = write!(
                    result,
                    r#"<a class="space-action space-secondary" href="{}" aria-label="Скачать {name}">Скачать</a>"#,
                    html(path),
                );
            }
            result.push_str("</li>");
        }
        result.push_str("</ul>");
    }
    result.push_str("</section>");
    result
}

fn size_label(bytes: i64) -> String {
    if bytes < 1024 {
        format!("{bytes} Б")
    } else if bytes < 1024 * 1024 {
        format!("{} КБ", (bytes + 1023) / 1024)
    } else {
        let mebibytes = format!("{:.1}", bytes as f64 / (1024.0 * 1024.0));
        format!("{} МБ", mebibytes.trim_end_matches(".0").replace('.', ","))
    }
}

fn history_link(to: &str, editing: bool) -> String {
    let to = html(to);
    let (suffix, attributes) = if editing {
        (
            "?preview=1",
            r#" target="_blank" rel="noopener" aria-label="История (в новой вкладке)""#,
        )
    } else {
        ("", "")
    };
    format!(
        r#"<a class="space-action space-secondary document-edit-link" href="{to}{suffix}"{attributes}>История</a>"#
    )
}

fn page_editor(form: &PageEditorForm) -> String {
    let history = form
        .history_to
        .as_deref()
        .map_or_else(String::new, |to| history_link(to, true));
    let error = form.error.map_or_else(String::new, |message| {
        format!(
            r#"<p id="page-editor-error" class="form-error" role="alert">{}</p>"#,
            html(message)
        )
    });
    let cover_input = form
        .cover
        .as_ref()
        .filter(|_| form.cover_changed)
        .map_or_else(String::new, |cover| hidden_field("cover", &cover.value));
    let cover_base = form
        .conflict
        .as_ref()
        .map(|conflict| conflict.cover.value.as_str())
        .or(form.cover_base.as_deref())
        .map_or_else(String::new, |cover| hidden_field("coverBase", cover));
    let text_base = form
        .text_base
        .as_deref()
        .map_or_else(String::new, |base| hidden_field("textBase", base));
    let attachment_inputs = form.attachment_draft.map_or_else(String::new, |draft| {
        let ids = |value: &[uuid::Uuid]| {
            value
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",")
        };
        format!(
            "{}{}{}",
            hidden_field("attachmentDraft", &draft.to_string()),
            hidden_field("attachmentAdd", &ids(&form.attachment_added)),
            hidden_field("attachmentRemove", &ids(&form.attachment_removed)),
        )
    });
    let coedit_fields = form
        .coedit_retry
        .as_ref()
        .map_or_else(String::new, |retry| {
            [
                hidden_field("coedit_lease", &retry.lease.to_string()),
                hidden_field("coedit_version", &retry.version),
                hidden_field("coedit_operation", &retry.operation.to_string()),
            ]
            .concat()
        });
    let attachments = if !form.attachments.is_empty() {
        attachment_list(&form.attachments)
    } else {
        String::new()
    };
    let cover_preview = form.cover.as_ref().map_or_else(String::new, |cover| {
        let art = cover_art(cover);
        if art.is_empty() {
            String::new()
        } else {
            format!(
                r#"<div class="page-cover-slot page-cover-slot-filled"><div class="page-cover-preview">{art}<span class="page-cover-caption">{label}</span></div></div>"#,
                label = cover_label(cover),
            )
        }
    });
    let (conflict_columns, current, rebase) = form.conflict.as_ref().map_or_else(
        || ("", String::new(), String::new()),
        |conflict| {
            let current = if conflict.text.is_empty() { "Пока здесь ничего нет." } else { &conflict.text };
            let current_cover = &conflict.cover;
            let current_art = cover_art(current_cover);
            let current_preview = if current_art.is_empty() { String::new() } else {
                format!(r#"<div class="page-cover-conflict-preview">{current_art}</div>"#)
            };
            (" page-editor-columns-conflict", format!(
                r#"<section class="page-editor-current" aria-labelledby="page-editor-current-heading">
        <h2 id="page-editor-current-heading">Актуальный текст</h2>
        <div class="page-editor-current-text" tabindex="0" aria-labelledby="page-editor-current-heading">{}</div>
        <div class="page-cover-current"><h3>Актуальная обложка · {cover_label}</h3>{current_preview}</div>
      </section>"#,
                html(current),
                cover_label = cover_label(current_cover),
            ), format!(
                r#"<label class="page-editor-rebase"><input type="checkbox" name="baseRevision" value="{}" required> Использовать эту версию как основу</label>"#,
                html(&conflict.revision),
            ),)
        },
    );
    let cover_choice = match (&form.cover, &form.conflict) {
        (Some(draft), Some(current))
            if form.can_save
                && form.cover_changed
                && draft.value != current.cover.value
                && form.cover_base.as_deref() != Some(current.cover.value.as_str()) =>
        {
            format!(
                r#"<fieldset class="page-cover-conflict"><legend>Какую обложку сохранить?</legend>
      <div class="page-cover-conflict-options">
        <label class="page-cover-conflict-option"><span class="page-cover-choice-copy"><input type="radio" name="coverChoice" value="published" required> Опубликованная · {}</span></label>
        <label class="page-cover-conflict-option"><span class="page-cover-choice-copy"><input type="radio" name="coverChoice" value="draft" required> Из черновика · {}</span></label>
      </div>
    </fieldset>"#,
                cover_label(&current.cover),
                cover_label(draft),
            )
        }
        _ => String::new(),
    };
    let text_note = if form.conflict.is_some() && form.text_unchanged {
        r#"<p class="page-editor-rebase-note">При сохранении обложки без правок текста останется актуальный текст.</p>"#
    } else {
        ""
    };
    let cancel = form.cancel_to.as_ref().map_or_else(String::new, |to| {
        let label = if form.can_save {
            "Отменить"
        } else {
            "Вернуться к чтению"
        };
        format!(
            r#"<a class="space-action space-secondary" href="{}">{label}</a>"#,
            html(to)
        )
    });
    let save = if form.can_save {
        r#"<button class="space-action primary-action" type="submit">Сохранить</button>"#
    } else {
        ""
    };
    // The HTML parser removes one leading newline from textarea contents.
    format!(
        r#"<form class="page-editor" method="post" action="{action}"{editable}{fresh}>
      <input type="hidden" name="revision" value="{revision}">
      <input type="hidden" name="contentFormat" value="{format}">
      <input type="hidden" name="editorMode" value="{mode}">
      {text_base}{cover_base}{cover_input}{attachment_inputs}{coedit_fields}
      {cover_preview}
      {error}
      <div class="page-editor-columns{conflict_columns}">
        <div class="page-editor-field">
          <label for="page-text">Исходный Markdown</label>
          <textarea class="page-source-field" id="page-text" name="text" rows="12" autofocus{readonly}{described_by}>
{text}</textarea>
        </div>
        {current}
      </div>
      {attachments}
      {cover_choice}
      {rebase}
      {text_note}
      <div class="page-editor-actions"><span class="page-editor-status" role="status"></span>{history}{cancel}{save}</div>
    </form>"#,
        action = html(&form.action),
        revision = html(&form.revision),
        format = form.content_format.as_str(),
        mode = form.editor_mode.as_str(),
        text = html(&form.source),
        editable = if form.can_save {
            " data-native-page-editor"
        } else {
            ""
        },
        fresh = if form.can_save && form.error.is_none() {
            " data-native-source"
        } else {
            ""
        },
        readonly = if form.can_save { "" } else { " readonly" },
        described_by = if form.error.is_some() {
            r#" aria-describedby="page-editor-error""#
        } else {
            ""
        },
    )
}

pub(crate) fn page_edit_retry(form: &PageEditorForm) -> String {
    let retry = format!(
        r#"<main class="space-shell"><h1>Редактирование страницы</h1>{}</main>"#,
        page_editor(form),
    );
    page(
        "Редактирование страницы",
        &retry,
        PageAssets::Workspace {
            menu: false,
            form: false,
        },
        ClientPage::PageEditRetry { editor: form },
    )
}
