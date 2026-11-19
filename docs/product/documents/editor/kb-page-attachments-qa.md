# Вложения и изображения «Севера» · Проверка пакета 1.0

[Согласованный пакет](kb-page-attachments-start.md) · [Карточки вложений](../media/README.md)

## Кандидат `cb0d8c94a075`

Образ `sha256:8319171dc37c08179f486e80e367d34a46051c158f9eddcdc6bc8e5f7e366949` собран по lock-файлам из снимка 129 файлов. Миграция `0008` добавляет три таблицы. Публикация и восстановление обновляют Markdown, обложку, список файлов и снимок в одной транзакции. История ссылается на те же байты. Возвращённый после удаления файл получает новый текущий адрес.

В первом раунде ревью потребовалось отделить очистку просроченных загрузок от новых загрузок, выровнять распознавание ссылок в React и Rust, сохранить проверенные метаданные при сбое записи и защитить повтор после параллельного удаления файла. Свежее ревью одобрило второй раунд. Временные файлы доступны 24 часа. Сервер чистит истёкшие записи ограниченными пакетами при запуске и каждые 30 секунд. Фоновая задача сохраняет действующие и опубликованные файлы.

Проверка формата PDF ограничена заголовком и завершающей меткой. Office-файлы проходят проверку записей контейнера. Для встроенных PNG, JPEG и WebP проверяются декодирование, до 8192 px по стороне и до 16 Мп. Скачивание возвращает исходные байты.

## Независимый QA на `4131`

На итоговом образе прошли 63 теста Rust с PostgreSQL, 66 тестов редактора, TypeScript, rustfmt и строгий Clippy. Прошли 23 теста приложения и реестра и 37 браузерных сценариев Markdown. Отдельно перепроверены история и обложки. Дополнительный прогон подтвердил приём ровно 100 МБ, отказ при превышении на один байт, восстановление курсора после отмены и сохранение файла при удалении отдельной вставки. По окончании исходные данные всех 17 таблиц совпали. Журналы независимого QA находятся в `qa-bug-hunt/signals/attachments-1-0-final`.

| Карточка | Проверенный результат |
|---|---|
| KB-FIL-01 | Допустимые файлы видны в черновике и публикуются вместе с текстом. Проверены ограничения, приватность до публикации, повтор при потере ответа, публикация только файла и сохранение введённого при `503`. |
| KB-FIL-02 | Скачивание возвращает исходные байты и имя с кириллицей. Скопированный адрес закрывается после отзыва доступа, для другой страницы и другого тенанта. Читающий администратор сохраняет обычное право скачивания. |
| KB-FIL-03 | На двух сессиях проверены параллельные правки, отмена и удаление файла с несколькими вставками. Повтор принятого удаления сохраняет одну новую редакцию. Прежний текущий адрес закрывается. Доступный редактору файл открывается из старого снимка и возвращается восстановлением по новому адресу. |
| KB-FIL-04 | Вставка через меню `/` учитывает позицию курсора. Обычное и декоративное изображения работают в визуальном режиме, в исходнике, при Undo/Redo и при чтении без JavaScript. Если другая сессия удалила файл, попытка сохранить новую ссылку на него возвращает черновик с объяснением. |

## Показ на Docker-стенде `4125`

После миграции проверены повторный вход, прежние пространства, текст, обложка и история предыдущей демонстрационной страницы. Данные всех 17 исходных таблиц совпали до запуска приложения. На новой демонстрационной странице загружены файл и картинка. Проверены скачивание, удаление картинки вместе со вставкой, закрытие старого URL, восстановление в новой редакции и чтение на узком экране. [Открыть страницу](http://localhost:4125/workspace/25976b66-d4e1-47ae-8317-38f8a600d876/spaces/055284b0-387a-4702-8a27-43cc891e7c8a/pages/b3ab696f-f9ea-4f2e-be4b-1376df3ac4be) · [Открыть редактор](http://localhost:4125/workspace/25976b66-d4e1-47ae-8317-38f8a600d876/spaces/055284b0-387a-4702-8a27-43cc891e7c8a/pages/b3ab696f-f9ea-4f2e-be4b-1376df3ac4be/edit) · [Открыть снимок с файлами](http://localhost:4125/workspace/25976b66-d4e1-47ae-8317-38f8a600d876/spaces/055284b0-387a-4702-8a27-43cc891e7c8a/pages/b3ab696f-f9ea-4f2e-be4b-1376df3ac4be/revisions/2)

Резервная база до миграции — `sever_dev_before_attachments_20261119222953`. Предыдущий образ сохранён под тегом `sever-kb-attachments:before`. Для позднего отката нужны сохранение записей после переключения и согласованные версии приложения и базы.

## Изменённые файлы приложения

- `app/spaces.css`
- `backend/migrations/0008_page_attachments.sql`
- `backend/src/db.rs`, `backend/src/db/page_attachments.rs`, `backend/src/db/page_attachments/publication.rs`, `backend/src/db/page_history.rs`, `backend/src/db/page_history/restore.rs`, `backend/src/db/page_history/tests.rs`, `backend/src/db/page_history/tests/attachments.rs`, `backend/src/db/page_text.rs`, `backend/src/markdown.rs`, `backend/src/markdown/blocks.rs`, `backend/src/sections.rs`, `backend/src/sections_http.rs`, `backend/src/sections_http/attachments.rs`, `backend/src/sections_http/attachments/format.rs`, `backend/src/sections_http/editor.rs`, `backend/src/sections_http/history.rs`, `backend/src/server.rs`, `backend/src/views/editor.rs`, `backend/src/views/history.rs`, `backend/src/workspace.rs`
- `frontend/src/Icon.tsx`, `frontend/src/PageEditor.tsx`, `frontend/src/WorkspacePage.tsx`, `frontend/src/editor/EditorCommandMenu.tsx`, `frontend/src/editor/PageAttachmentDialog.tsx`, `frontend/src/editor/PageAttachmentList.tsx`, `frontend/src/editor/RichMarkdownBlocks.tsx`, `frontend/src/editor/RichMarkdownEditor.tsx`, `frontend/src/editor/draftHistory.test.mjs`, `frontend/src/editor/draftHistory.ts`, `frontend/src/editor/editorChanges.test.mjs`, `frontend/src/editor/editorChanges.ts`, `frontend/src/editor/markdownModel.ts`, `frontend/src/editor/pageAttachments.ts`, `frontend/src/model.ts`


Владелец принял кандидат `cb0d8c94a075` со стенда `4125` к публикации в `main`.
