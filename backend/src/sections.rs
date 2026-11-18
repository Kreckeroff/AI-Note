use serde::Serialize;
use uuid::Uuid;

use crate::db::{Page, Section};

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
    pub edit_to: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PageEditorForm {
    pub action: String,
    pub cancel_to: Option<String>,
    pub text: String,
    pub revision: String,
    pub error: Option<&'static str>,
    pub conflict: Option<PageEditConflict>,
    pub can_save: bool,
}

#[derive(Serialize)]
pub(crate) struct PageEditConflict {
    pub text: String,
    pub revision: String,
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
