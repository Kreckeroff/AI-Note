# Проверенная матрица навыков

86 навыков. 77 проверены в каталоге 17 ноября, ещё 9 установлены для дизайна и проверены на диске 18 ноября 2026 года. Это выбранная библиотека для десяти ролей, а не набор, который нужно целиком загружать каждому агенту. Роль в таблице — основная; один навык может назначаться нескольким ролям по задаче.

Структурированный источник для будущего выбора и доски — [skill-registry.json](skill-registry.json). Абсолютные пути отражают текущее окружение; перед запуском проверить наличие и актуальность выбранного файла. Файл в кеше, отсутствующий в активном каталоге, не считается доступным навыком.

При назначении выбрать основной метод и дополнения по фактическому изменению. Передача имени не заменяет чтение SKILL.md. Для дубликатов выбран один источник. Проверенные ограничения ниже имеют значение при сочетании навыков.

## Оркестратор / продакт — 6

| Навык | Когда назначать | Условие |
|---|---|---|
| [product-management:roadmap-update](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/product-management/1.2.0/skills/roadmap-update/SKILL.md>) | Изменение приоритетов и дорожной карты | По соответствующей поверхности задачи |
| [product-management:sprint-planning](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/product-management/1.2.0/skills/sprint-planning/SKILL.md>) | Планирование с учётом ёмкости и зависимостей | По соответствующей поверхности задачи |
| [product-management:stakeholder-update](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/product-management/1.2.0/skills/stakeholder-update/SKILL.md>) | Сводка состояния и решений | По соответствующей поверхности задачи |
| [product-management:metrics-review](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/product-management/1.2.0/skills/metrics-review/SKILL.md>) | Проверка продуктового результата | По соответствующей поверхности задачи |
| [mattpocock-skills:writing-for-agents](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/mattpocock-skills/1.2.3/skills/productivity/writing-for-agents/SKILL.md>) | Агентские документы и пакеты контекста | По соответствующей поверхности задачи |
| [skill-creator](</Users/kreckeroff/.codex/skills/.system/skill-creator/SKILL.md>) | Создание связующего навыка под подтверждённый процесс | По соответствующей поверхности задачи |

## Аналитик — 10

| Навык | Когда назначать | Условие |
|---|---|---|
| [mattpocock-skills:grilling](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/mattpocock-skills/1.2.3/skills/productivity/grilling/SKILL.md>) | Уточнение решений и противоречий с пользователем | Решения согласует владелец; факты среды исследует агент |
| [mattpocock-skills:domain-modeling](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/mattpocock-skills/1.2.3/skills/engineering/domain-modeling/SKILL.md>) | Термины, сущности, состояния и бизнес-правила | По соответствующей поверхности задачи |
| [mattpocock-skills:research](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/mattpocock-skills/1.2.3/skills/engineering/research/SKILL.md>) | Проверка неизвестных фактов по первичным источникам | По соответствующей поверхности задачи |
| [mattpocock-skills:prototype](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/mattpocock-skills/1.2.3/skills/engineering/prototype/SKILL.md>) | Проверка гипотезы одноразовым прототипом | По соответствующей поверхности задачи |
| [product-management:write-spec](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/product-management/1.2.0/skills/write-spec/SKILL.md>) | PRD и критерии приёмки новой фичи | По соответствующей поверхности задачи |
| [product-management:product-brainstorming](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/product-management/1.2.0/skills/product-brainstorming/SKILL.md>) | Исследование задачи до фиксации решения | По соответствующей поверхности задачи |
| [product-management:synthesize-research](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/product-management/1.2.0/skills/synthesize-research/SKILL.md>) | Обобщение интервью и обратной связи | По соответствующей поверхности задачи |
| [product-management:competitive-brief](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/product-management/1.2.0/skills/competitive-brief/SKILL.md>) | Сравнение конкурентов | По соответствующей поверхности задачи |
| [design:user-research](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/design/1.2.0/skills/user-research/SKILL.md>) | План исследования пользователей | По соответствующей поверхности задачи |
| [design-skills:journey-mapping](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/design-skills/1.0.0/skills/journey-mapping/SKILL.md>) | Пользовательские пути | По соответствующей поверхности задачи |

## Архитектор — 10

| Навык | Когда назначать | Условие |
|---|---|---|
| [engineering:architecture](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/engineering/1.2.0/skills/architecture/SKILL.md>) | ADR при выборе решения | По соответствующей поверхности задачи |
| [engineering:system-design](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/engineering/1.2.0/skills/system-design/SKILL.md>) | Границы системы и ответственность | По соответствующей поверхности задачи |
| [mattpocock-skills:codebase-design](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/mattpocock-skills/1.2.3/skills/engineering/codebase-design/SKILL.md>) | Публичные интерфейсы и границы тестирования | По соответствующей поверхности задачи |
| [ai-integration-architect:ai-integration-architect](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/ai-integration-architect/1.0.0/skills/ai-integration-architect/SKILL.md>) | Интеграция ИИ, MCP, RAG и существующих систем | По соответствующей поверхности задачи |
| [arxitect:api-design-review](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/arxitect/1.1.1/skills/api-design-review/SKILL.md>) | Проверка API-контрактов | По соответствующей поверхности задачи |
| [arxitect:oo-design-review](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/arxitect/1.1.1/skills/oo-design-review/SKILL.md>) | Проверка объектной модели | По соответствующей поверхности задачи |
| [arxitect:clean-architecture-review](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/arxitect/1.1.1/skills/clean-architecture-review/SKILL.md>) | Проверка зависимостей компонентов | По соответствующей поверхности задачи |
| [clean-architecture:boundary](</Users/kreckeroff/.codex/plugins/cache/it-algo-local/clean-architecture/1.0.0/skills/boundary/SKILL.md>) | Архитектурная граница | По соответствующей поверхности задачи |
| [clean-architecture:components](</Users/kreckeroff/.codex/plugins/cache/it-algo-local/clean-architecture/1.0.0/skills/components/SKILL.md>) | Связность и связанность компонентов | По соответствующей поверхности задачи |
| [clean-architecture:diagram](</Users/kreckeroff/.codex/plugins/cache/it-algo-local/clean-architecture/1.0.0/skills/diagram/SKILL.md>) | Схема архитектуры по кодовой базе | По соответствующей поверхности задачи |

## Дизайнер — 22

| Навык | Когда назначать | Условие |
|---|---|---|
| [design:design-system](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/design/1.2.0/skills/design-system/SKILL.md>) | Расширение существующей дизайн-системы | По соответствующей поверхности задачи |
| [design:design-critique](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/design/1.2.0/skills/design-critique/SKILL.md>) | Оценка макетов и сценария | По соответствующей поверхности задачи |
| [design:design-handoff](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/design/1.2.0/skills/design-handoff/SKILL.md>) | Передача макета разработчикам | По соответствующей поверхности задачи |
| [design:ux-copy](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/design/1.2.0/skills/ux-copy/SKILL.md>) | Тексты, ошибки и пустые состояния | По соответствующей поверхности задачи |
| [design-skills:design-systems](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/design-skills/1.0.0/skills/design-systems/SKILL.md>) | Токены и компоненты | По соответствующей поверхности задачи |
| [design-skills:interaction-design](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/design-skills/1.0.0/skills/interaction-design/SKILL.md>) | Состояния интерактивных элементов | По соответствующей поверхности задачи |
| [design-skills:accessibility-audit](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/design-skills/1.0.0/skills/accessibility-audit/SKILL.md>) | Аудит WCAG 2.2 | По соответствующей поверхности задачи |
| [design-skills:ux-research](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/design-skills/1.0.0/skills/ux-research/SKILL.md>) | Проверка решений на пользователях | По соответствующей поверхности задачи |
| [figma:figma-use](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/figma/2.2.157/skills/figma-use/SKILL.md>) | Операции с содержимым Figma | Прочитать перед каждым соответствующим вызовом Figma |
| [figma:figma-create-new-file](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/figma/2.2.157/skills/figma-create-new-file/SKILL.md>) | Новый файл Figma/FigJam | По соответствующей поверхности задачи |
| [figma:figma-generate-design](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/figma/2.2.157/skills/figma-generate-design/SKILL.md>) | Экран в редактируемом дизайне | По соответствующей поверхности задачи |
| [figma:figma-spec](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/figma/2.2.157/skills/figma-spec/SKILL.md>) | Спецификация в Figma | По соответствующей поверхности задачи |
| [figma:figma-generate-diagram](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/figma/2.2.157/skills/figma-generate-diagram/SKILL.md>) | Диаграмма в FigJam | Прочитать перед каждым generate_diagram |
| [better-typography](</Users/kreckeroff/.codex/skills/better-typography/SKILL.md>) | Иерархия заголовков и длина строки | Установлен 18.11.2026 |
| [better-layout](</Users/kreckeroff/.codex/skills/better-layout/SKILL.md>) | Композиция панелей и адаптивность | Установлен 18.11.2026 |
| [interface-review](</Users/kreckeroff/.codex/skills/interface-review/SKILL.md>) | Ревью изменённых экранов по diff | Установлен 18.11.2026 |
| [better-interface](</Users/kreckeroff/.codex/skills/better-interface/SKILL.md>) | Комплексная оценка интерфейса | Установлен 18.11.2026 |
| [better-accessibility](</Users/kreckeroff/.codex/skills/better-accessibility/SKILL.md>) | Клавиатура, фокус и доступные состояния | Установлен 18.11.2026 |
| [better-ui](</Users/kreckeroff/.codex/skills/better-ui/SKILL.md>) | Детали компонентов и обратная связь | Установлен 18.11.2026 |
| [better-writing](</Users/kreckeroff/.codex/skills/better-writing/SKILL.md>) | Подписи действий и статусы | Установлен 18.11.2026 |
| [better-colors](</Users/kreckeroff/.codex/skills/better-colors/SKILL.md>) | Цветовые роли и контраст | Установлен 18.11.2026 |
| [shadcn](</Users/kreckeroff/.codex/skills/shadcn/SKILL.md>) | Подбор и композиция компонентов shadcn/ui | Установлен 18.11.2026 |

## Фронтенд-разработчик — 6

| Навык | Когда назначать | Условие |
|---|---|---|
| [frontend-design:frontend-design](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/frontend-design/1.1.1/skills/frontend-design/SKILL.md>) | Создание или переработка интерфейса | По соответствующей поверхности задачи |
| [ui-styling](</Users/kreckeroff/.agents/skills/ui-styling/SKILL.md>) | UI на Radix/shadcn/Tailwind при таком стеке | По соответствующей поверхности задачи |
| [ui-ux-pro-max](</Users/kreckeroff/.agents/skills/ui-ux-pro-max/SKILL.md>) | Визуальные и UX-паттерны | По соответствующей поверхности задачи |
| [figma:figma-design-to-code](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/figma/2.2.157/skills/figma-design-to-code/SKILL.md>) | Реализация макета Figma | Прочитать до get_design_context |
| [figma:figma-code-connect](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/figma/2.2.157/skills/figma-code-connect/SKILL.md>) | Соответствие компонентов Figma и кода | По соответствующей поверхности задачи |
| [design:accessibility-review](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/design/1.2.0/skills/accessibility-review/SKILL.md>) | Аудит WCAG 2.1 AA | По соответствующей поверхности задачи |

## Бэкенд-разработчик — 14

| Навык | Когда назначать | Условие |
|---|---|---|
| [backend-design:think-before-coding](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/backend-design/0.2.0/skills/think-before-coding/SKILL.md>) | Подготовка backend-изменения | По соответствующей поверхности задачи |
| [backend-design:error-handling-as-design](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/backend-design/0.2.0/skills/error-handling-as-design/SKILL.md>) | Контракты ошибок до happy path | По соответствующей поверхности задачи |
| [backend-design:data-modeling-discipline](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/backend-design/0.2.0/skills/data-modeling-discipline/SKILL.md>) | Схема и жизненный цикл данных | По соответствующей поверхности задачи |
| [backend-design:query-discipline](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/backend-design/0.2.0/skills/query-discipline/SKILL.md>) | Запросы и доступ к данным | По соответствующей поверхности задачи |
| [backend-design:idempotency-and-side-effects](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/backend-design/0.2.0/skills/idempotency-and-side-effects/SKILL.md>) | Повторные запросы и побочные эффекты | По соответствующей поверхности задачи |
| [backend-design:performance-and-scaling](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/backend-design/0.2.0/skills/performance-and-scaling/SKILL.md>) | Производительность и масштабирование | По соответствующей поверхности задачи |
| [backend-design:boring-by-default](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/backend-design/0.2.0/skills/boring-by-default/SKILL.md>) | Выбор инфраструктуры | По соответствующей поверхности задачи |
| [backend-design:observability-by-default](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/backend-design/0.2.0/skills/observability-by-default/SKILL.md>) | Логи, метрики и диагностика | По соответствующей поверхности задачи |
| [backend-design:migration-safety](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/backend-design/0.2.0/skills/migration-safety/SKILL.md>) | Изменение схемы данных | По соответствующей поверхности задачи |
| [clean-architecture:entity](</Users/kreckeroff/.codex/plugins/cache/it-algo-local/clean-architecture/1.0.0/skills/entity/SKILL.md>) | Бизнес-правила сущности | По соответствующей поверхности задачи |
| [clean-architecture:usecase](</Users/kreckeroff/.codex/plugins/cache/it-algo-local/clean-architecture/1.0.0/skills/usecase/SKILL.md>) | Прикладной сценарий | По соответствующей поверхности задачи |
| [clean-architecture:gateway](</Users/kreckeroff/.codex/plugins/cache/it-algo-local/clean-architecture/1.0.0/skills/gateway/SKILL.md>) | Внешние сервисы и хранилища | По соответствующей поверхности задачи |
| [clean-architecture:controller](</Users/kreckeroff/.codex/plugins/cache/it-algo-local/clean-architecture/1.0.0/skills/controller/SKILL.md>) | Адаптация входного запроса | По соответствующей поверхности задачи |
| [mattpocock-skills:diagnosing-bugs](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/mattpocock-skills/1.2.3/skills/engineering/diagnosing-bugs/SKILL.md>) | Сложная или нестабильная ошибка | Назначать разработчику; включает инструментирование, исправление и постоянную регрессию |

## Инженер тестирования — 4

| Навык | Когда назначать | Условие |
|---|---|---|
| [mattpocock-skills:tdd](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/mattpocock-skills/1.2.3/skills/engineering/tdd/SKILL.md>) | Поведенческий red-тест перед реализацией | Сначала согласовать публичные границы тестов; затем RED→GREEN по срезам |
| [engineering:testing-strategy](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/engineering/1.2.0/skills/testing-strategy/SKILL.md>) | Уровни тестирования и риски | По соответствующей поверхности задачи |
| [backend-design:testing-with-discernment](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/backend-design/0.2.0/skills/testing-with-discernment/SKILL.md>) | Отбор полезных backend-тестов | По соответствующей поверхности задачи |
| [clean-architecture:test](</Users/kreckeroff/.codex/plugins/cache/it-algo-local/clean-architecture/1.0.0/skills/test/SKILL.md>) | Тестирование архитектурных границ | По соответствующей поверхности задачи |

## Независимый QA — 3

| Навык | Когда назначать | Условие |
|---|---|---|
| [qa-bug-hunter:qa-bug-hunter](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/qa-bug-hunter/1.9.0/skills/qa-bug-hunter/SKILL.md>) | Независимый поиск и воспроизведение дефектов | Свежий контекст; изолированные repro, постоянные исправления другой ролью |
| [engineering:code-review](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/engineering/1.2.0/skills/code-review/SKILL.md>) | Проверка изменений | По соответствующей поверхности задачи |
| [mattpocock-skills:code-review](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/mattpocock-skills/1.2.3/skills/engineering/code-review/SKILL.md>) | Проверка изменений | Нужны спецификация, issue-tracker.md и корректная область diff |

## Проверяющий безопасность — 3

| Навык | Когда назначать | Условие |
|---|---|---|
| [backend-design:auth-and-authorization](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/backend-design/0.2.0/skills/auth-and-authorization/SKILL.md>) | Авторизация и сессии | По соответствующей поверхности задачи |
| [backend-design:security-discipline](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/backend-design/0.2.0/skills/security-discipline/SKILL.md>) | Входные данные, права и backend-diff | По соответствующей поверхности задачи |
| [review-security](</Users/kreckeroff/.agents/skills/review-security/SKILL.md>) | Специализированное ревью безопасности | Нужен специальный runtime; проверить возможность запуска |

## Релиз-инженер — 8

| Навык | Когда назначать | Условие |
|---|---|---|
| [engineering:deploy-checklist](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/engineering/1.2.0/skills/deploy-checklist/SKILL.md>) | Готовность перед выпуском | По соответствующей поверхности задачи |
| [engineering:incident-response](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/engineering/1.2.0/skills/incident-response/SKILL.md>) | Сбой и разбор инцидента | По соответствующей поверхности задачи |
| [engineering:documentation](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/engineering/1.2.0/skills/documentation/SKILL.md>) | Обновление документации | По соответствующей поверхности задачи |
| [mattpocock-skills:resolving-merge-conflicts](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/mattpocock-skills/1.2.3/skills/engineering/resolving-merge-conflicts/SKILL.md>) | Текущий merge/rebase-конфликт | По соответствующей поверхности задачи |
| [mattpocock-skills:pr](</Users/kreckeroff/.codex/plugins/cache/claude-cowork/mattpocock-skills/1.2.3/skills/in-progress/pr/SKILL.md>) | Описание итогового PR | По соответствующей поверхности задачи |
| [split-to-prs](</Users/kreckeroff/.agents/skills/split-to-prs/SKILL.md>) | Разделение работы на обозримые PR | По соответствующей поверхности задачи |
| [autopilot](</Users/kreckeroff/.agents/skills/autopilot/SKILL.md>) | Поддержание PR готовым к merge | Проверить разрешённые внешние действия и последствия push |
| [deploy-with-vercel](</Users/kreckeroff/.agents/skills/deploy-with-vercel/SKILL.md>) | Выпуск в Vercel после настройки и разрешения | Нужно действующее подключение Vercel |

## Совместимость и ограничения

### Grilling

Собирает решения по дереву вопросов и ожидает подтверждения общего понимания до реализации; факты окружения собирает агент, а не пользователь.

Использовать на этапе уточнения; ограничивать текущим фронтом решений, не повторять ответы из утверждённой спецификации.

### TDD

Требует заранее согласованных публичных границ тестирования; один red-тест затем соответствующая реализация. Не предписывает писать весь комплект тестов заранее. Рефакторинг вынесен в ревью.

Согласовать test-plan и seams вместе с разрешением на реализацию; тестировать поведение, состояния и сохранение через публичные интерфейсы.

### QA Bug Hunter

Поиск дефектов не изменяет application-код и suite. Допустимы repro-тесты и артефакты в qa-bug-hunt/, который gitignored.

Разработчик или test-engineer переносит repro в постоянную регрессионную suite и чинит; оркестратор сохраняет очищенную долговечную сводку отчёта вне raw-папки.

### QA freshness

QA Bug Hunter по умолчанию использует warm-start knowledge base/resource registry, что не соответствует буквальному требованию чистого контекста.

Пользовательское требование приоритетно: новый агент без истории диалога получает только утверждённый test oracle, snapshot, scope и запуск. Старые выводы и подсказки разработчиков не входят в независимый поиск.

### Arxitect architect

Этот навык реализует код и запускает три ревью; после трёх итераций сам принимает оставшиеся WARNING/SUGGESTION.

До согласования старта брать architecture/system-design и отдельные review-навыки. Внутри реализации проектный допуск с нулём открытых подтверждённых дефектов имеет приоритет над auto-approval.

### Nested reviews

Arxitect architecture-review и Matt code-review запускают несколько подагентов.

Оркестратор должен учитывать общий лимит четырёх агентов, включая себя, и освобождать слоты или выполнять обзоры очередью.

### Matt code-review

Нужны issue-tracker.md, исходная спецификация и фиксированная git-точка; команда fixed-point...HEAD сама по себе не охватывает незакоммиченные изменения.

Перед запуском явно зафиксировать проверяемый snapshot/diff; при неподходящей среде взять engineering:code-review, не считать невидимые изменения проверенными.

### Create-subagent

Локальный skill описывает .cursor/agents/ и markdown-frontmatter, а не проверенную схему всех Codex runtimes.

Документы роли и runtime-конфигурацию разделять; конфигурацию проверить по текущим инструментам. Назначение роли/скилла само по себе не создаёт техническое ограничение доступа.

### Review-security

Требует отдельный runtime subtype security-review, которого среди generic collaboration.spawn_agent параметров нет.

Считать capability-dependent. Без специального инструмента использовать явно описанную роль security с backend security навыками; не утверждать, что специализированный review был выполнен.

### Release

deploy-checklist создаёт чеклист, а не выполняет выпуск. deploy-with-vercel требует подключённый Vercel. autopilot может пушить исправления, но не делает merge/auto-merge/draft-ready.

Привязать разрешение к ревизии и окружению; учитывать, что push в связанную ветку может запустить production-deploy. Готовность PR и успешный выпуск фиксировать раздельно.

## Дополнительная проверка назначения

Diagnosing Bugs включает инструментирование, исправление и cleanup, поэтому назначается разработчику, а не независимому QA. При дефектах фронтенда этот навык также может применяться соответствующим разработчиком.

## Найденные, но не включённые варианты

В кеше встречаются grill-me и отдельные handoff/to-spec/to-tickets. Они отсутствуют в предоставленном активном каталоге; их наличие на диске не подтверждает регистрацию. Для текущего процесса используется доступный mattpocock-skills:grilling.

Arxitect Architect не включён в основное назначение подготовительного архитектора из-за встроенного цикла реализации. Create Subagent не используется как источник подтверждённой схемы runtime-конфигурации. Их можно оценить отдельно при соответствующей задаче.

Двенадцать предлагаемых связующих навыков описаны отдельно в [custom-skills.md](custom-skills.md). Они пока являются проектом, а не частью установленного набора.
