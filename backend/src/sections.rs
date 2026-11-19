use serde::Serialize;
use uuid::Uuid;

use crate::{
    db::{Attachment, Page, PageContentFormat, PageCover, Section},
    markdown::RenderedMarkdown,
    workspace::page_cover_image_path,
};

pub(crate) struct SectionScreen {
    pub sections: Vec<Section>,
    pub pages: Vec<Page>,
    pub selected: Option<Section>,
    pub selected_page: Option<Page>,
    pub can_create_section: bool,
    pub form: Option<NameForm>,
    pub page_form: Option<NameForm>,
    pub reading: Option<PageReading>,
    pub editor: Option<PageEditorForm>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PageReading {
    pub text: String,
    pub rendered_html: Option<RenderedMarkdown>,
    pub edit_to: Option<String>,
    pub history_to: Option<String>,
    pub cover: CoverView,
    pub attachments: Vec<PageAttachmentView>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PageAttachmentView {
    pub id: Uuid,
    pub name: String,
    pub bytes: i64,
    pub download_to: Option<String>,
    pub image_to: Option<String>,
}

impl PageAttachmentView {
    pub(crate) fn at_url(attachment: Attachment, path: String, downloadable: bool) -> Self {
        Self {
            id: attachment.id,
            name: attachment.name,
            bytes: attachment.size,
            image_to: attachment.image.then(|| format!("{path}/image")),
            download_to: downloadable.then_some(path),
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CoverView {
    pub value: String,
    pub image_to: Option<String>,
}

impl CoverView {
    pub(crate) fn new(tenant: Uuid, space: Uuid, page: Uuid, cover: PageCover) -> Self {
        let image_to = match cover {
            PageCover::Image(asset) => Some(page_cover_image_path(tenant, space, page, asset)),
            _ => None,
        };
        Self {
            value: cover.value(),
            image_to,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PageEditorMode {
    Visual,
    Source,
}

impl PageEditorMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Visual => "visual",
            Self::Source => "source",
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PageEditorForm {
    pub action: String,
    pub cancel_to: Option<String>,
    pub history_to: Option<String>,
    pub text: String,
    pub source: String,
    pub content_format: PageContentFormat,
    pub text_base: Option<String>,
    pub text_unchanged: bool,
    pub editor_mode: PageEditorMode,
    pub revision: String,
    pub error: Option<&'static str>,
    pub conflict: Option<PageEditConflict>,
    pub can_save: bool,
    pub cover: Option<CoverView>,
    pub cover_base: Option<String>,
    pub cover_changed: bool,
    pub cover_upload_to: Option<String>,
    pub attachments: Vec<PageAttachmentView>,
    pub attachment_draft: Option<Uuid>,
    pub attachment_upload_to: Option<String>,
    pub attachment_added: Vec<Uuid>,
    pub attachment_removed: Vec<Uuid>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PageEditConflict {
    pub text: String,
    pub source: String,
    pub content_format: PageContentFormat,
    pub revision: String,
    pub cover: CoverView,
}

pub(crate) struct NameForm {
    pub operation_id: Uuid,
    pub title: String,
    pub error: String,
    pub invalid_name: bool,
    pub cancel_to: String,
    pub action: String,
    pub retry_to: Option<String>,
}
