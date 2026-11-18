use uuid::Uuid;

use crate::db::Section;

pub(crate) struct SectionScreen {
    pub sections: Vec<Section>,
    pub selected: Option<Section>,
    pub can_create_section: bool,
    pub form: Option<SectionForm>,
}

pub(crate) struct SectionForm {
    pub operation_id: Uuid,
    pub title: String,
    pub error: String,
    pub invalid_name: bool,
    pub cancel_to: String,
    pub action: String,
    pub retry_to: Option<String>,
}
