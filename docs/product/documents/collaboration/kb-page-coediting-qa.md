# Совместное редактирование страниц 1.0 · проверка кандидата

20 ноября 2026 года · `e49a72c3c148` · [Утверждённый пакет](kb-page-coediting-1.0.md) · [Технический контракт](kb-page-coediting-sa.md)

Кандидат доступен для приёмки на Docker-стенде 4125. После выкладки совпали контрольные суммы 31 таблицы. На стенде открывается вход, база содержит 3 пространства и 10 страниц. [Открыть стенд](http://localhost:4125/workspace)

## Реализация

Редакторы обмениваются подтверждёнными изменениями Markdown. Непересекающиеся правки и Undo проходят через точные диапазоны операций. Участники с правом чтения видят опубликованную редакцию и имена присутствующих коллег. Одна чужая публикация или восстановление создаёт одно уведомление подписчику.

React опрашивает общий черновик по последней подтверждённой версии раз в секунду. Присутствие и номер публикации обновляются раз в четыре секунды. Сервер проверяет текущую сессию и права на каждом запросе. PostgreSQL хранит подтверждённый черновик, операции и подписки. После выхода версии из окна журнала клиент получает свежий снимок и удерживает неподтверждённый текст для восстановления. Список активных вкладок хранится в процессе и восстанавливается после очередного сигнала присутствия.

## Проверки

| Карточка | Доказательство |
|---|---|
| [KB-COL-07 · Присутствие](kb-col-07.md) | Сервер проверил свёртку вкладок аккаунта, другую страницу, другой тенант, гостя и отзыв прав. Chrome показал 4 аватара и счётчик на широком экране, 2 аватара на узком, список с клавиатуры и возврат фокуса. Просроченный список скрылся при зависшем запросе. Аварийно отключённая вкладка исчезла за 11,916 секунды при пределе 15 секунд. |
| [KB-COL-08 · Общий черновик](kb-col-08.md) | Chrome подтвердил порядок одновременных вставок, Undo своей вставки и приватного изображения, обмен между визуальным режимом и исходником, очередь после обрыва, независимый ввод при совмещении и отзыв допуска. Сервер подтвердил Unicode-диапазоны, повтор после перезапуска, приватность медиа, сохранение прежней формой и одну редакцию при конкурирующих сохранениях. |
| [KB-COL-09 · Подписка](kb-col-09.md) | Chrome подтвердил подписку и отписку в двух вкладках одного аккаунта, колокольчик и переход к опубликованному тексту. Серверные проверки охватили публикацию и восстановление, повтор, неизменный текст, собственное сохранение, гонку отписки и сохранения, отзыв чтения и пропущенный период. |

Сборка React и TypeScript, 85 проверок фронтенда, 90 Rust-тестов с PostgreSQL, Clippy, форматирование и проверка diff прошли. На отдельной копии Docker-базы миграции 12 и 13 сохранили контрольную сумму данных всех 31 ранее существовавших таблиц. Исходники проверяемого снимка закреплены манифестом SHA-256 из 178 файлов.

## Ревью и исправления

- Раунд 1 зафиксировал проблемы с правами, подтверждением операций и публикацией. В реализации повторно проверили актуальный допуск и привязали публикацию к версии общего черновика.
- Раунд 2 проверил журнал операций, работу очереди и совмещение после восстановления истории. Сервер начал отдавать точные последовательности правок и подтверждать применённые команды отдельным номером.
- Раунд 3 проверил истечение журнала, очистку присутствия и сохранность ввода ручного совмещения. Ревью и независимый Chrome-QA `c1dd45b4b6bc` выявили сбой Undo после одновременной вставки. В `e49a72c3c148` точный журнал переносит историю один раз. Новый тест покрывает конкурентную вставку, очередь из нескольких правок, одинаковые символы рядом с приватной картинкой и несвязанный ввод после ручного совмещения.

Независимый QA повторил сбой на предыдущем кандидате и трижды проверил воспроизведение на новом. Все три раза Undo сохранил чужую вставку. В отдельном браузерном сеансе два Undo отменили локальный ввод и локальное изображение, сохранив правку другого редактора.

Дополнительный прогон Chrome проверил строку из комбинируемого знака и эмодзи с модификатором и ZWJ. При вводе через IME коллега получил подтверждённое слово после завершения композиции. Устаревшее ручное совмещение сервер отклонил в проверке PostgreSQL.

## Выкладка и откат

Перед запуском снят полный PostgreSQL-снимок стенда и сохранён предыдущий образ. На стенд установлен тот же образ, который прошёл проверку на изолированной базе. Саморегистрация выключена действующей конфигурацией. При отказе запуска предыдущий образ может читать прежние таблицы с сохранённым томом. Восстановление базы из снимка требует отдельной сверки записей, созданных после выкладки.

<details>
<summary>Изменённые файлы продукта</summary>

```text
app/spaces.css
backend/migrations/0012_page_coediting.sql
backend/migrations/0013_page_watch.sql
backend/src/coedit_http.rs
backend/src/coedit_presence.rs
backend/src/db.rs
backend/src/db/coedit_presence.rs
backend/src/db/coediting.rs
backend/src/db/coediting/lease.rs
backend/src/db/coediting/patch.rs
backend/src/db/coediting/publication.rs
backend/src/db/coediting/storage.rs
backend/src/db/coediting/tests.rs
backend/src/db/configuration.rs
backend/src/db/page_discussion.rs
backend/src/db/page_discussion/reads.rs
backend/src/db/page_discussion/tests.rs
backend/src/db/page_history.rs
backend/src/db/page_history/restore.rs
backend/src/db/page_history/tests.rs
backend/src/db/page_text.rs
backend/src/db/page_watch.rs
backend/src/identity.rs
backend/src/main.rs
backend/src/markdown.rs
backend/src/markdown/blocks.rs
backend/src/page_discussion_http.rs
backend/src/page_social_route.rs
backend/src/sections.rs
backend/src/sections_http/editor.rs
backend/src/server.rs
backend/src/views/editor.rs
frontend/src/PageEditor.tsx
frontend/src/WorkspacePage.tsx
frontend/src/collaboration/PageInspector.tsx
frontend/src/collaboration/PageNotifications.tsx
frontend/src/collaboration/PagePresence.tsx
frontend/src/collaboration/PageWatch.tsx
frontend/src/collaboration/SharedDraftStatus.tsx
frontend/src/collaboration/coeditingApi.ts
frontend/src/collaboration/draftSplice.ts
frontend/src/collaboration/publishedPage.ts
frontend/src/collaboration/sharedDraft.test.mjs
frontend/src/collaboration/sharedDraft.ts
frontend/src/collaboration/sharedDraftHistory.ts
frontend/src/collaboration/useSharedDraft.ts
frontend/src/editor/RichMarkdownEditor.tsx
frontend/src/editor/draftHistory.test.mjs
frontend/src/editor/draftHistory.ts
frontend/src/model.ts
```

</details>
