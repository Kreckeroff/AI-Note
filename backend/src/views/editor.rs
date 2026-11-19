use crate::sections::{CoverView, PageEditorForm, SectionScreen};

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
    content
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
      {text_base}{cover_base}{cover_input}
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
