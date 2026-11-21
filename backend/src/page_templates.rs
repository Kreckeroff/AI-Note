use serde::Serialize;

/// A published revision is immutable. Publish a new key when changing its Markdown.
#[derive(Serialize)]
pub(crate) struct PageTemplate {
    pub key: &'static str,
    pub revision: u16,
    pub title: &'static str,
    pub source: &'static str,
}

pub(crate) static EMPTY: PageTemplate = PageTemplate {
    key: "",
    revision: 0,
    title: "Пустая страница",
    source: "",
};

pub(crate) static TEMPLATES: &[PageTemplate] = &[
    PageTemplate {
        key: "meeting-v1",
        revision: 1,
        title: "Протокол встречи",
        source: "## Цель встречи\n\nКакой вопрос нужно решить?\n\n## Участники\n\n- Организатор\n- Участники обсуждения\n\n## Повестка\n\n1. Вопрос для обсуждения\n\n## Решения\n\nЧто решили и почему?\n\n## Следующие шаги\n\n- [ ] Действие — ответственный и срок",
    },
    PageTemplate {
        key: "decision-v1",
        revision: 1,
        title: "Проектное решение",
        source: "## Контекст\n\nКакую проблему решаем и какие ограничения учитываем?\n\n## Варианты\n\n- Вариант и его компромиссы\n- Альтернативный вариант\n\n## Решение\n\nКакой вариант выбран и почему?\n\n## Последствия\n\nКакие изменения потребуются и по каким признакам пересмотрим решение?",
    },
    PageTemplate {
        key: "spec-v1",
        revision: 1,
        title: "Спецификация",
        source: "## Цель\n\nКому и какую задачу помогает решить изменение?\n\n## Сценарий\n\n1. Участник начинает работу\n2. Система показывает результат\n\n## Требования\n\n- Ожидаемое поведение\n- Поведение при ошибке\n\n## Критерии приёмки\n\n- [ ] Основной сценарий проверен\n- [ ] Границы доступа проверены\n\n## Открытые вопросы\n\nЧто нужно уточнить до выпуска?",
    },
];

pub(crate) fn find(key: &str) -> Option<&'static PageTemplate> {
    if key.is_empty() {
        Some(&EMPTY)
    } else {
        TEMPLATES.iter().find(|template| template.key == key)
    }
}
