import {escapeHtml as html, icon, northIcon, page} from './page-view.mjs';
import {newSpacePath, spacePagePath, spacesPath, withCatalogOptions, workspacePath} from './workspace-paths.mjs';

const admin = role => role === 'admin' || role === 'superadmin';

function accessLabel(row) {
  if (row.isOwner) return 'Владелец';
  if (row.grantRole === 'editor') return 'Редактирование';
  return admin(row.tenantRole) ? 'Просмотр по роли администратора' : 'Просмотр';
}

function tenantControl(tenant, tenants) {
  const label = `${northIcon}<span>${html(tenant.name)}</span>`;
  if (tenants.length < 2) return `<span class="dock-tenant">${label}</span>`;
  const rows = tenants.map(item => `<li>${item.id === tenant.id
    ? `<span class="choice-current" aria-current="true">${html(item.name)} <small>Текущая среда</small></span>`
    : `<a href="${spacesPath(item.id)}">${html(item.name)}${icon('next')}</a>`}</li>`).join('');
  return `<details class="dock-choice" data-workspace-choice>
    <summary class="dock-tenant" aria-label="Сменить среду. Текущая: ${html(tenant.name)}">${label}${icon('down')}</summary>
    <div class="dock-panel"><strong>Выбрать среду</strong><ul>${rows}</ul></div>
  </details>`;
}

function spaceControl({tenant, availableSpaces, options, space}) {
  const label = html(space?.name ?? 'Выбрать пространство');
  const group = (title, isOwner) => {
    const items = availableSpaces.filter(row => Boolean(row.isOwner) === isOwner);
    if (!items.length) return '';
    const links = items.map(row => `<li><a data-picker-space="${row.id}" href="${html(withCatalogOptions(spacePagePath(tenant.id, row.id), options))}"${row.id === space?.id ? ' aria-current="page"' : ''}>
      ${icon('folder')}<span><strong data-picker-label>${html(row.name)}</strong><small>${html(row.ownerName)}</small></span>${icon('next')}
    </a></li>`).join('');
    return `<li data-picker-group><h3>${title}</h3><ul>${links}</ul></li>`;
  };
  const search = availableSpaces.length ? `<div class="picker-search" data-picker-search hidden><label for="picker-query" class="sr-only">Поиск пространства в меню</label><input id="picker-query" type="search" placeholder="Найти пространство" maxlength="200"></div>` : '';
  return `<details class="dock-choice" data-workspace-choice>
    <summary class="dock-space" aria-label="${space ? `Сменить пространство. Текущее: ${label}` : label}">${icon('folder')}<span>${label}</span>${icon('down')}</summary>
    <div class="dock-panel dock-space-panel">${search}
      <ul class="picker-spaces">${group('Мои пространства', true)}${group('Доступные пространства', false)}</ul>
      <p class="picker-no-results" role="status" data-picker-empty ${availableSpaces.length ? 'hidden' : ''}>${availableSpaces.length ? 'Пространства не найдены' : 'Доступных пространств пока нет'}</p>
      <a class="picker-all" href="${html(withCatalogOptions(spacesPath(tenant.id), options))}">${icon('layers')}Все пространства${icon('next')}</a>
    </div>
  </details>`;
}

export function workspaceFrame(title, context, content, dialog = '') {
  const {tenant, accountName, options, space, tenants} = context;
  const catalogHref = tenant && html(withCatalogOptions(spacesPath(tenant.id), options));
  const chrome = tenant ? `<div class="workspace-dock">
    <div class="dock-identity">${tenantControl(tenant, tenants)}<span class="dock-divider">${icon('next')}</span>${spaceControl(context)}</div>
    <nav class="dock-nav" aria-label="Рабочие разделы"><a href="${catalogHref}"${space ? '' : ' aria-current="page"'}>${icon('layers')}Пространства</a>${space ? '<span aria-current="page">Документы</span>' : ''}</nav>
  </div>` : `<div class="workspace-dock"><a class="dock-tenant" href="${workspacePath}">${northIcon}Север</a></div>`;
  return page(title, `
    <header class="workspace-topbar">${chrome}${accountName ? `<span class="space-account" title="${html(accountName)}">${html(accountName)}</span>` : ''}</header>
    <main class="${space ? 'documents-shell' : 'space-shell'}">${content}</main>
    ${dialog}`, {spaces: true, workspace: Boolean(tenant), script: dialog ? '/space-form.mjs' : undefined});
}

function catalogRows(tenant, catalog) {
  const {rows, options, total, pages, start, end} = catalog;
  const currentPage = options.page;
  const base = spacesPath(tenant.id);
  const rowLinks = rows.map(row => `<li><a class="catalog-row" data-catalog-space="${row.id}" href="${html(withCatalogOptions(spacePagePath(tenant.id, row.id), options))}">
      <span class="catalog-name"><span class="catalog-icon">${icon('folder')}</span><span><strong>${html(row.name)}</strong><small>${icon('lock')}Закрытое</small></span></span>
      <span class="catalog-owner"><span class="sr-only">Владелец: </span>${html(row.ownerName)}</span><span class="catalog-access"><span class="sr-only">Мой доступ: </span>${accessLabel(row)}</span>${icon('next')}
    </a></li>`).join('');
  const pageLink = (pageNumber, label) => `<a class="space-action space-secondary" href="${html(withCatalogOptions(base, {...options, page: pageNumber}))}" data-catalog-nav>${label}</a>`;
  return `<div class="catalog-table" aria-label="Список пространств">
    <div class="catalog-row catalog-columns" aria-hidden="true"><span>Название</span><span>Владелец</span><span>Мой доступ</span><span></span></div>
    <ul>${rowLinks}</ul>
    ${pages > 1 ? `<nav class="catalog-pagination" aria-label="Страницы каталога"><span>${start}–${end} из ${total}</span><div>
      ${currentPage > 1 ? pageLink(currentPage - 1, 'Назад') : ''}<span>${currentPage} из ${pages}</span>${currentPage < pages ? pageLink(currentPage + 1, 'Дальше') : ''}
    </div></nav>` : ''}
  </div>`;
}

export function catalogContent({tenant, catalog}) {
  const {options, total} = catalog;
  const {q, scope} = options;
  const path = spacesPath(tenant.id);
  const filters = {all: admin(tenant.role) ? 'Все пространства' : 'Все доступные', mine: 'Мои', membership: 'По членству'};
  const filterLinks = Object.entries(filters).map(([value, label]) =>
    `<a href="${html(withCatalogOptions(path, {q, scope: value}))}"${scope === value ? ' aria-current="true"' : ''} data-catalog-nav>${label}</a>`).join('');
  const filtered = q || scope !== 'all';
  const results = total ? catalogRows(tenant, catalog)
    : `<section class="space-empty catalog-empty" aria-label="Результат поиска"><span class="space-icon">${icon(filtered ? 'search' : 'layers')}</span><h2>${filtered ? 'Пространства не найдены' : 'Здесь пока нет пространств'}</h2>
      ${filtered ? `<a class="space-action space-secondary" href="${path}" data-catalog-nav>Сбросить поиск и фильтр</a>` : ''}</section>`;
  return `<section class="catalog" aria-labelledby="page-title" data-catalog>
    <div class="space-overview"><div><h1 id="page-title">Пространства</h1><p class="space-summary">${html(tenant.name)}</p></div>
      ${tenant.canCreate ? `<a class="primary-action space-action" id="create-space" href="${html(withCatalogOptions(newSpacePath(tenant.id), options))}">${icon('add')}Создать пространство</a>` : ''}</div>
    <div class="catalog-tools"><form method="get" action="${path}" data-catalog-search>
      <label class="sr-only" for="catalog-query">Поиск по названию пространства</label>
      <input id="catalog-query" type="search" name="q" value="${html(q)}" placeholder="Поиск по названию" maxlength="200">
      ${scope !== 'all' ? `<input type="hidden" name="scope" value="${scope}">` : ''}
      <button class="space-action space-secondary" type="submit" aria-label="Искать">${icon('search')}</button>
    </form><nav class="catalog-filters" aria-label="Фильтр пространств">${filterLinks}</nav></div>
    <p class="catalog-count" role="status" data-catalog-count>Найдено: ${total}</p>${results}</section>`;
}

export function spacesCatalogPage(context) {
  return workspaceFrame('Пространства', context, catalogContent(context));
}

export function catalogUnavailablePage(retryTo) {
  return workspaceFrame('Пространства', {}, `<section class="space-empty"><h1>Не удалось загрузить пространства</h1><a class="space-action space-secondary" href="${html(retryTo)}">Повторить попытку</a></section>`);
}

export function closedSpacePage(context) {
  const {tenant, space, options} = context;
  const catalogHref = html(withCatalogOptions(spacesPath(tenant.id), options));
  return workspaceFrame(space.name, context, `
    <aside class="documents-tree" aria-label="Дерево пространства">
      <span class="documents-tree-title">${icon('folder')}${html(space.name)}</span>
      <a href="${catalogHref}">${icon('layers')}Все пространства</a>
      <details open><summary>Разделы</summary><p>Пока нет разделов</p></details>
    </aside>
    <div class="documents-content">
      <nav class="documents-path" aria-label="Путь"><a href="${catalogHref}">Пространства</a>${icon('next')}<span>${html(space.name)}</span></nav>
      <article class="documents-article">
        <header class="space-heading"><span class="space-icon">${icon('folder')}</span>
          <div><h1>${html(space.name)}</h1><div class="space-badges"><span>${icon('lock')}Закрытое</span><span>${accessLabel(space)}</span></div></div>
        </header>
        <section class="space-empty"><span class="space-icon">${icon('layers')}</span><h2>В пространстве пока нет разделов</h2></section>
      </article>
    </div>`);
}

export function spaceNotFoundPage() {
  return workspaceFrame('Пространство не найдено', {}, `<section class="space-denied"><span class="space-icon">${icon('lock')}</span>
      <h1>Пространство не найдено</h1><p class="space-summary">Проверьте адрес или вернитесь в рабочую область.</p>
      <a class="space-action space-secondary" href="${workspacePath}">${icon('back')}К рабочей области</a>
    </section>`);
}

export function createSpacePage({operationId, name = '', error = '', invalidName = false, newForm = false, ...context}) {
  const {tenant, options} = context;
  const catalogHref = html(withCatalogOptions(spacesPath(tenant.id), options));
  const dialog = `
    <dialog id="space-dialog" class="space-dialog" aria-labelledby="space-dialog-title" open>
      <header class="space-dialog-header">
        <div><h2 id="space-dialog-title">Новое пространство</h2><p class="space-dialog-tenant">${icon('home')}${html(tenant.name)}</p></div>
        <a class="space-action space-close" href="${catalogHref}#create-space" data-space-cancel aria-label="Закрыть">${icon('close')}</a>
      </header>
      <form class="space-form" method="post" action="${catalogHref}" novalidate>
        <input type="hidden" name="operation_id" value="${html(operationId)}">
        <label for="space-name">Название</label>
        <input id="space-name" name="name" value="${html(name)}" placeholder="Например, Команда продукта" autofocus required
          aria-describedby="space-help${error ? ' space-error' : ''}"${invalidName ? ' aria-invalid="true"' : ''}>
        <p class="space-hint" id="space-help">От 1 до 100 символов. Выберите уникальное имя среди своих пространств в этой среде.</p>
        <p class="form-error" id="space-client-error" role="alert" hidden>Введите не больше 100 символов.</p>
        ${error ? `<p class="form-error" id="space-error" role="alert">${html(error)}${newForm ? ` <a href="${html(withCatalogOptions(newSpacePath(tenant.id), options))}">Открыть новую форму</a>` : ''}</p>` : ''}
        <div class="space-note">${icon('lock')}<p><strong>Закрытое пространство</strong><br>Вы станете владельцем. Администраторы этой среды смогут просматривать содержимое.</p></div>
        <div class="space-dialog-actions">
          <a class="space-action space-secondary" href="${catalogHref}#create-space" data-space-cancel>Отмена</a>
          <button class="primary-action space-action" type="submit">Создать</button>
        </div>
      </form>
      <p class="sr-only" role="status" data-space-status></p>
    </dialog>`;
  return workspaceFrame('Новое пространство', context, catalogContent(context), dialog);
}
