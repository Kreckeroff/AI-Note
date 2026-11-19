use crate::sections::{PageEditorForm, SectionScreen};

use super::{PageAssets, client::ClientPage, html, page};

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

fn page_editor(form: &PageEditorForm) -> String {
    let error = form.error.map_or_else(String::new, |message| {
        format!(
            r#"<p id="page-editor-error" class="form-error" role="alert">{}</p>"#,
            html(message)
        )
    });
    let (conflict_columns, current, rebase) = form.conflict.as_ref().map_or_else(
        || ("", String::new(), String::new()),
        |conflict| {
            let current = if conflict.text.is_empty() { "Пока здесь ничего нет." } else { &conflict.text };
            (" page-editor-columns-conflict", format!(
                r#"<section class="page-editor-current" aria-labelledby="page-editor-current-heading">
        <h2 id="page-editor-current-heading">Актуальный текст</h2>
        <div class="page-editor-current-text" tabindex="0" aria-labelledby="page-editor-current-heading">{}</div>
      </section>"#,
                html(current),
            ), format!(
                r#"<label class="page-editor-rebase"><input type="checkbox" name="baseRevision" value="{}" required> Использовать эту версию как основу</label>"#,
                html(&conflict.revision),
            ),)
        },
    );
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
      {error}
      <div class="page-editor-columns{conflict_columns}">
        <div class="page-editor-field">
          <label for="page-text">Исходный Markdown</label>
          <textarea class="page-source-field" id="page-text" name="text" rows="12" autofocus{readonly}{described_by}>
{text}</textarea>
        </div>
        {current}
      </div>
      {rebase}
      <div class="page-editor-actions"><span class="page-editor-status" role="status"></span>{cancel}{save}</div>
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
