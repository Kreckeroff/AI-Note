# Шеринг страниц и комментарии 1.0 · проверка кандидата

Кандидат `4d645eead8f8` собран из зафиксированного снимка. [Требования и макет](kb-page-sharing-comments-start.md) · [Контракт и критерии](kb-page-sharing-comments-sa.md)

| Проверка | Результат |
|---|---|
| Rust и PostgreSQL 16 | 75 тестов, включая интеграционные сценарии авторизации, редактора и обсуждений. Clippy и форматирование прошли. |
| React и Markdown | Vite собрал клиент. Прошли 66 тестов Markdown-редактора. |
| Миграция на отдельной копии 4125 | Схема обновилась с 0008 до 0011. Совпали данные 20 существовавших таблиц, включая 3 пространства и 9 страниц, учётные записи, сессии, редакции, обложки и вложения. |
| Доступ по API | Проверены адресные и унаследованные права, поиск, запрос просмотра с дедупликацией, отзыв, изоляция соседних страниц и другого тенанта, отказ по роли и проверка методов HTTP. |
| Гости и публичные ссылки | При выключенной саморегистрации приглашение принимает новая или существующая учётная запись. Проверены подтверждение почты, отмена, истечение и отзыв. Публичная ссылка открывает опубликованный документ с обложкой и встроенным изображением. Обычное скачивание, старый адрес изображения после отзыва и выдача при запрете политики закрыты. |
| Обсуждения | Проверены темы и ответы, повторы, статусы, упоминания и переход к уведомлению. После удаления фрагмента читатель видит статус привязки, редактор может открыть исходную цитату в истории. |
| Chrome | Проверены панель и диалоги на широком и узком экранах, возврат фокуса, вкладки и сохранение черновика, нативная отправка формы гостя и переход к конкретному ответу после входа. |

Первый раунд архитектурного ревью выявил блокировку отправки формы гостя в Chrome, раскрытие удалённой цитаты читателю и замечания к повтору операций, ссылкам и состоянию диалога. Второй раунд ревью подтвердил исправления. Независимая проверка проводилась на изолированном Docker-кандидате с синтетическими аккаунтами и локальной почтой.

## Показ на Docker-стенде

[Версия 4d645eead8f8 на 4125](http://localhost:4125/workspace) использует тот же образ, что кандидат на 4133. Перед обновлением стенда сохранён снимок базы. После обновления снова совпали данные 20 существовавших таблиц, включая три пространства и девять страниц. Саморегистрация на стенде выключена, приглашения обрабатывает локальный тестовый приёмник.

Дополнительный прогон на изолированной базе подтвердил запрет гостевого доступа политикой организации, конкурентный отзыв во время отправки комментария и пагинацию реальных списков участников и приглашений.

## Файлы пакета

- [app/spaces.css](../../../../app/spaces.css)
- [backend/Cargo.lock](../../../../backend/Cargo.lock)
- [backend/Cargo.toml](../../../../backend/Cargo.toml)
- [backend/README.md](../../../../backend/README.md)
- [backend/migrations/0009_page_sharing.sql](../../../../backend/migrations/0009_page_sharing.sql)
- [backend/migrations/0010_page_discussion.sql](../../../../backend/migrations/0010_page_discussion.sql)
- [backend/migrations/0011_page_external_sharing.sql](../../../../backend/migrations/0011_page_external_sharing.sql)
- [backend/src/clock.rs](../../../../backend/src/clock.rs)
- [backend/src/db.rs](../../../../backend/src/db.rs)
- [backend/src/db/configuration.rs](../../../../backend/src/db/configuration.rs)
- [backend/src/db/external_sharing.rs](../../../../backend/src/db/external_sharing.rs)
- [backend/src/db/external_sharing/guests.rs](../../../../backend/src/db/external_sharing/guests.rs)
- [backend/src/db/external_sharing/public.rs](../../../../backend/src/db/external_sharing/public.rs)
- [backend/src/db/page_access.rs](../../../../backend/src/db/page_access.rs)
- [backend/src/db/page_attachments.rs](../../../../backend/src/db/page_attachments.rs)
- [backend/src/db/page_cover.rs](../../../../backend/src/db/page_cover.rs)
- [backend/src/db/page_discussion.rs](../../../../backend/src/db/page_discussion.rs)
- [backend/src/db/page_discussion/anchors.rs](../../../../backend/src/db/page_discussion/anchors.rs)
- [backend/src/db/page_discussion/reads.rs](../../../../backend/src/db/page_discussion/reads.rs)
- [backend/src/db/page_discussion/tests.rs](../../../../backend/src/db/page_discussion/tests.rs)
- [backend/src/db/page_history.rs](../../../../backend/src/db/page_history.rs)
- [backend/src/db/page_sharing.rs](../../../../backend/src/db/page_sharing.rs)
- [backend/src/db/page_sharing/tests.rs](../../../../backend/src/db/page_sharing/tests.rs)
- [backend/src/db/page_text.rs](../../../../backend/src/db/page_text.rs)
- [backend/src/external_sharing.rs](../../../../backend/src/external_sharing.rs)
- [backend/src/external_sharing/guest.rs](../../../../backend/src/external_sharing/guest.rs)
- [backend/src/external_sharing/published.rs](../../../../backend/src/external_sharing/published.rs)
- [backend/src/http.rs](../../../../backend/src/http.rs)
- [backend/src/identity.rs](../../../../backend/src/identity.rs)
- [backend/src/main.rs](../../../../backend/src/main.rs)
- [backend/src/markdown.rs](../../../../backend/src/markdown.rs)
- [backend/src/page_discussion_http.rs](../../../../backend/src/page_discussion_http.rs)
- [backend/src/page_sharing_http.rs](../../../../backend/src/page_sharing_http.rs)
- [backend/src/page_social_route.rs](../../../../backend/src/page_social_route.rs)
- [backend/src/registration.rs](../../../../backend/src/registration.rs)
- [backend/src/sections.rs](../../../../backend/src/sections.rs)
- [backend/src/sections_http.rs](../../../../backend/src/sections_http.rs)
- [backend/src/sections_http/editor.rs](../../../../backend/src/sections_http/editor.rs)
- [backend/src/sections_http/history.rs](../../../../backend/src/sections_http/history.rs)
- [backend/src/server.rs](../../../../backend/src/server.rs)
- [backend/src/settings.rs](../../../../backend/src/settings.rs)
- [backend/src/text.rs](../../../../backend/src/text.rs)
- [backend/src/views.rs](../../../../backend/src/views.rs)
- [backend/src/views/client.rs](../../../../backend/src/views/client.rs)
- [backend/src/views/history.rs](../../../../backend/src/views/history.rs)
- [backend/src/views/workspace.rs](../../../../backend/src/views/workspace.rs)
- [backend/src/workspace.rs](../../../../backend/src/workspace.rs)
- [compose.yaml](../../../../compose.yaml)
- [frontend/src/Icon.tsx](../../../../frontend/src/Icon.tsx)
- [frontend/src/WorkspacePage.tsx](../../../../frontend/src/WorkspacePage.tsx)
- [frontend/src/collaboration/CollaborationTabs.tsx](../../../../frontend/src/collaboration/CollaborationTabs.tsx)
- [frontend/src/collaboration/CommentComposer.tsx](../../../../frontend/src/collaboration/CommentComposer.tsx)
- [frontend/src/collaboration/PageDiscussion.tsx](../../../../frontend/src/collaboration/PageDiscussion.tsx)
- [frontend/src/collaboration/PageInspector.tsx](../../../../frontend/src/collaboration/PageInspector.tsx)
- [frontend/src/collaboration/PageNotifications.tsx](../../../../frontend/src/collaboration/PageNotifications.tsx)
- [frontend/src/collaboration/PageSharingDialog.tsx](../../../../frontend/src/collaboration/PageSharingDialog.tsx)
- [frontend/src/collaboration/api.ts](../../../../frontend/src/collaboration/api.ts)
- [frontend/src/collaboration/pageAnchors.ts](../../../../frontend/src/collaboration/pageAnchors.ts)
- [frontend/src/editor/PageHistoryPanel.tsx](../../../../frontend/src/editor/PageHistoryPanel.tsx)
- [frontend/src/model.ts](../../../../frontend/src/model.ts)
