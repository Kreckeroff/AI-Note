import {escapeHtml as html, icon} from './ui.mjs';
import {catalogFixtures, catalogOwners} from './catalog-fixtures.mjs';
import {withWorkspaceLock} from './collaboration.mjs?v=tenant-switch-2';
import {availableTenantSpaces, createSpace, demoOrganizationId, demoOrganizationName, demoSpaceNameError, loadWorkspace,
  people as workspacePeople, persistWorkspace, personalTenantName, profileFor, spaceAccess, spaceInfo, tenantDemoScope, workspaceNames, workspaceStorageKey} from './model.mjs?v=tenant-catalog-2';
import {prepareTenantDemo, tenantDemoOptions, usesTenantDemo} from './tenant-demo.mjs?v=tenant-catalog-2';

const byId = id => document.getElementById(id);
const scenario = byId('scenario');
const search = byId('space-search');
const switcher = byId('space-selector');
const switcherSearch = byId('switcher-search');
const filters = [...document.querySelectorAll('[data-filter]')];
const themeButtons = [...document.querySelectorAll('.theme-toggle')];
const panel = byId('panel');
const tenantPanel = byId('tenant-panel');
let tenantControl = byId('dock-tenant');
const nameInput = byId('space-name');
const nameError = byId('name-error');
const pageSize = 20;
const sortNames = new Intl.Collator('ru', {sensitivity: 'base', numeric: true});
const organization = demoOrganizationName;
const preview = new URLSearchParams(location.search);
const markdownJourney = preview.get('journey') === 'markdown';
const tenantDemo = usesTenantDemo(preview);
const editorSpaceId = 'space-1';
const people = tenantDemo ? [[workspacePeople.anna.name,workspacePeople.anna.initials],...catalogOwners.slice(1)] : catalogOwners;
const spaces = catalogFixtures(workspaceNames.north.name).map(space => ({...space,environment:organization}));
const addedSpaces = [];
if (tenantDemo) await prepareTenantDemo();
let demoWorkspace = tenantDemo ? loadWorkspace(tenantDemoScope) : null;
const explicitAccess = {
  admin: new Map([[spaces[1].id, 'Просмотр'], [spaces[6].id, 'Редактирование']]),
  creator: new Map([[spaces[1].id, 'Просмотр'], [spaces[6].id, 'Редактирование'], [spaces[12].id, 'Просмотр']]),
  member: new Map([[spaces[0].id, 'Просмотр'], [spaces[2].id, 'Просмотр'], [spaces[5].id, 'Редактирование'], [spaces[10].id, 'Просмотр']]),
};
const profiles = {
  admin: {person: 0, environment: organization, canCreate: true, isAdmin: true},
  creator: {person: 0, environment: organization, canCreate: true},
  member: {person: 4, environment: organization, canCreate: false},
  personal: {person: 0, environment: personalTenantName, canCreate: true},
  empty: {person: 4, environment: organization, canCreate: false, empty: true},
};
let page = 1;
let filter = 'all';
let profile = profiles.admin;
let role = 'admin';
let selectedId = '';
let createdCount = 0;

const demoTenantId = () => role === 'personal' ? demoWorkspace.account : demoOrganizationId;

function routeId() {
  return /^#space\/([a-z0-9-]+)$/u.exec(location.hash)?.[1] ?? '';
}

function spaceLink(space) {
  if (!tenantDemo) return `#space/${space.id}`;
  const editor = new URL('./', location.href);
  editor.searchParams.set('from-catalog', 'north');
  editor.searchParams.set('demo','tenants');
  editor.searchParams.set('theme', document.body.dataset.theme);
  editor.searchParams.set('q', search.value);
  editor.searchParams.set('scope', filter);
  editor.searchParams.set('page', page);
  editor.searchParams.set('scenario', scenario.value);
  editor.hash = 'space/' + encodeURIComponent(space.id);
  return editor.pathname + editor.search + editor.hash;
}

function isPreviewOwner(space) {
  return space.owner === profile.person;
}

function previewAccess(space) {
  if (space.environment !== profile.environment || profile.empty) return '';
  if (isPreviewOwner(space)) return 'Владелец';
  return explicitAccess[role]?.get(space.id) || (profile.isAdmin ? 'Просмотр по роли' : '');
}

function demoRows() {
  return availableTenantSpaces(demoWorkspace,demoTenantId()).map(id => {
    const {name,owner} = spaceInfo(demoWorkspace,id);
    const access = spaceAccess(demoWorkspace,id);
    const isOwned = access?.source === 'owner';
    const byMembership = access?.source === 'membership';
    const roleLabel = isOwned ? 'Владелец' : access?.source === 'administrator' ? 'Просмотр по роли'
      : access?.role === 'edit' ? 'Редактирование' : 'Просмотр';
    return {id,name,ownerName:profileFor(demoWorkspace,owner)?.name || '',isOwned,byMembership,access:roleLabel};
  });
}

function visibleSpaces() {
  const rows = tenantDemo ? demoRows() : [...spaces,...addedSpaces].flatMap(space => {
    const access = previewAccess(space);
    return access ? [{...space,access,isOwned:isPreviewOwner(space),ownerName:people[space.owner][0],byMembership:explicitAccess[role]?.has(space.id)}] : [];
  });
  return rows.sort((a,b) => sortNames.compare(a.name,b.name) || sortNames.compare(a.ownerName,b.ownerName));
}

function renderTheme(theme) {
  document.body.dataset.theme = theme;
  const next = theme === 'dark' ? 'Светлая тема' : 'Тёмная тема';
  for (const button of themeButtons) {
    button.innerHTML = icon(theme === 'dark' ? 'sun' : 'moon');
    button.setAttribute('aria-label', next);
    button.title = next;
  }
  byId('editor-reference').href = `./?theme=${theme}${tenantDemo ? '&demo=tenants' : ''}#doc/access`;
}

function renderIdentity(space) {
  const [name, initials] = people[profile.person];
  byId('actor-avatar').textContent = initials;
  byId('actor-name').textContent = name;
  byId('actor-avatar').parentElement.title = name;
  byId('dock-tenant-name').textContent = profile.environment;
  tenantControl.title = profile.environment;
  if (tenantDemo) tenantControl.setAttribute('aria-label',`Сменить среду. Текущая: ${profile.environment}`);
  byId('dock-name').textContent = space?.name ?? 'Выбрать пространство';
  byId('dock-context').setAttribute('aria-label', space ? `Сменить пространство. Текущее: ${space.name}` : 'Выбрать пространство');
  byId('dock-context').title = space?.name ?? 'Выбрать пространство';
  byId('catalog-scope').textContent = profile.isAdmin
    ? `Все пространства организации «${profile.environment}»`
    : profile.environment;
  byId('all-filter').textContent = profile.isAdmin ? 'Все пространства' : 'Все доступные';
}

function renderCatalog() {
  const loading = scenario.value === 'loading';
  const failed = scenario.value === 'error';
  byId('create-space').hidden = !profile.canCreate || loading || failed;
  byId('catalog-toolbar').hidden = loading || failed;
  byId('dock-search').hidden = loading || failed;
  const available = visibleSpaces();
  const phrase = search.value.trim().toLocaleLowerCase('ru');
  const matching = available.filter(space => {
    if (filter === 'mine' && !space.isOwned) return false;
    if (filter === 'membership' && !space.byMembership) return false;
    return space.name.toLocaleLowerCase('ru').includes(phrase);
  });
  const count = byId('result-count');
  const table = byId('catalog-table');
  const state = byId('catalog-state');
  const hasRows = !loading && !failed && matching.length > 0;
  table.hidden = !hasRows;
  state.hidden = hasRows;
  count.textContent = hasRows ? `Найдено: ${matching.length}` : '';
  if (!hasRows) {
    let glyph = 'layers';
    let title = 'Здесь пока нет пространств';
    let message = profile.canCreate ? 'Создайте первое пространство в этой среде.' : 'Здесь появятся пространства, когда вам откроют доступ.';
    let action = '';
    if (loading) {
      glyph = 'clock';
      title = 'Загружаем пространства…';
      message = '';
    } else if (failed) {
      glyph = 'clock';
      title = 'Не удалось загрузить пространства';
      message = 'Попробуйте открыть каталог ещё раз.';
      action = '<button class="btn" type="button" data-state-action="retry">Повторить</button>';
    } else if (phrase || (available.length > 0 && filter !== 'all')) {
      glyph = 'search';
      title = 'Пространства не найдены';
      message = 'Измените название в поиске или выберите другой фильтр.';
      action = '<button class="btn" type="button" data-state-action="clear">Очистить поиск и фильтры</button>';
    }
    state.innerHTML = `<span class="state-icon">${icon(glyph, 22)}</span><h2>${title}</h2>${message ? `<p class="muted">${message}</p>` : ''}${action}`;
    if (loading || failed) count.textContent = title;
    return;
  }
  const maxPage = Math.ceil(matching.length / pageSize);
  page = Math.min(page, maxPage);
  const start = (page - 1) * pageSize;
  const end = Math.min(page * pageSize, matching.length);
  byId('space-list').innerHTML = matching.slice(start, end).map(space => `
    <li><a class="space-row" href="${html(spaceLink(space))}" data-space-id="${html(space.id)}">
      <span class="space-row-title">
        <span class="row-emblem">${icon('folder', 17)}</span>
        <span class="row-label"><strong>${html(space.name)}</strong><span class="lock-label">${icon('lock', 12)}Закрытое</span></span>
      </span>
      <span class="row-owner"><span class="sr-only">Владелец: </span>${html(space.ownerName)}</span>
      <span class="tag row-access"><span class="sr-only">Мой доступ: </span>${html(space.access)}</span>
      ${icon('chevron', 15)}
    </a></li>`).join('');
  byId('pagination').hidden = maxPage < 2;
  byId('page-range').textContent = `${start + 1}–${end} из ${matching.length}`;
  byId('page-number').textContent = `${page} из ${maxPage}`;
  byId('previous-page').disabled = page <= 1;
  byId('next-page').disabled = page >= maxPage;
}

function renderSwitcher() {
  const phrase = switcherSearch.value.trim().toLocaleLowerCase('ru');
  const matching = visibleSpaces().filter(space => space.name.toLocaleLowerCase('ru').includes(phrase));
  const groups = [
    ['Мои пространства', matching.filter(space => space.isOwned)],
    ['Доступные пространства', matching.filter(space => !space.isOwned)],
  ];
  byId('switcher-results').innerHTML = groups.filter(([, entries]) => entries.length).map(([title, entries]) => {
    const rows = (phrase ? entries : entries.slice(0, 4)).map(space => `
      <a class="switcher-row" href="${html(spaceLink(space))}" ${selectedId === space.id ? 'aria-current="page"' : ''}>
        ${icon('folder', 15)}<span>${html(space.name)}</span><span class="small muted">${html(space.ownerName)}</span>
      </a>`).join('');
    return `<h2>${title}</h2>${rows}`;
  }).join('') || `<p class="small muted" role="status">${phrase ? 'Совпадений нет' : 'Доступных пространств пока нет'}</p>`;
}

function setFilter(value) {
  filter = value;
  page = 1;
  for (const button of filters) button.setAttribute('aria-pressed', String(button.dataset.filter === filter));
  renderCatalog();
}

function setTree(open) {
  byId('space-workbench').dataset.tree = String(open);
  byId('tree-toggle').setAttribute('aria-expanded', String(open));
}

function renderRoute({moveFocus = false} = {}) {
  const id = routeId();
  const previousId = selectedId;
  selectedId = id;
  const lookupId = tenantDemo && id === editorSpaceId ? 'north' : id;
  const space = lookupId && visibleSpaces().find(entry => entry.id === lookupId);
  const isCatalog = !id;
  byId('catalog').hidden = !isCatalog;
  byId('space-workbench').hidden = !space;
  byId('space-denied').hidden = isCatalog || Boolean(space);
  byId('back-utility').hidden = isCatalog;
  byId('documents-nav').hidden = !space;
  if (isCatalog) byId('spaces-nav').setAttribute('aria-current', 'page');
  else byId('spaces-nav').removeAttribute('aria-current');
  renderIdentity(space);
  setTree(false);
  if (space) {
    for (const key of ['space-title', 'tree-space', 'crumb-space']) byId(key).textContent = space.name;
    byId('tree-environment').textContent = profile.environment;
    byId('space-access').textContent = space.access === 'Просмотр по роли' ? 'Просмотр по роли администратора' : space.access;
    if (moveFocus) byId('space-title').focus();
  } else if (isCatalog) {
    renderCatalog();
    if (moveFocus) {
      const previousRow = previousId && document.querySelector(`[data-space-id="${previousId}"]`);
      (previousRow || byId('catalog-heading')).focus();
    }
  } else if (moveFocus) {
    byId('space-denied').querySelector('h1').focus();
  }
}

function changeScenario() {
  if (panel.open) panel.close();
  switcher.open = false;
  role = profiles[scenario.value] ? scenario.value : 'admin';
  profile = profiles[role];
  search.value = '';
  setFilter('all');
  if (routeId()) location.hash = 'spaces';
  renderRoute();
}

function showNameError(message = '') {
  nameError.textContent = message;
  nameError.hidden = !message;
  if (message) nameInput.setAttribute('aria-invalid', 'true');
  else nameInput.removeAttribute('aria-invalid');
}

for (const element of document.querySelectorAll('[data-icon]')) {
  element.insertAdjacentHTML('afterbegin', icon(element.dataset.icon));
}
if (tenantDemo) {
  byId('scenario-label').hidden = true;
  scenario.hidden = true;
  const button = document.createElement('button');
  button.type = 'button';
  button.id = tenantControl.id;
  button.className = tenantControl.className;
  button.setAttribute('aria-haspopup','dialog');
  button.append(...tenantControl.childNodes);
  button.insertAdjacentHTML('beforeend',icon('down',12));
  tenantControl.replaceWith(button);
  tenantControl = button;
  button.addEventListener('click', () => {
    switcher.open = false;
    byId('tenant-options').innerHTML = tenantDemoOptions(profile.environment === personalTenantName ? 'personal' : 'admin',
      document.body.dataset.theme,{organization:demoOrganizationName,personal:personalTenantName});
    tenantPanel.showModal();
  });
  byId('close-tenant-panel').addEventListener('click', () => tenantPanel.close());
}
for (const button of themeButtons) button.addEventListener('click', () => {
  renderTheme(document.body.dataset.theme === 'dark' ? 'light' : 'dark');
  if (tenantDemo) {
    renderCatalog();
    if (switcher.open) renderSwitcher();
  }
});
renderTheme(preview.get('theme') === 'light' ? 'light' : 'dark');
const initialScenario = preview.get('scenario');
if (!tenantDemo && Object.hasOwn(profiles, initialScenario)) {
  scenario.value = initialScenario;
  role = initialScenario;
  profile = profiles[role];
}
if (tenantDemo) {
  scenario.value = initialScenario === 'personal' ? 'personal' : 'admin';
  role = scenario.value;
  profile = profiles[role];
  const query = (preview.get('q') || '').slice(0, 100);
  // Existing 1.0 deep links used the demo space's old name. Restored searches are literal.
  const legacyLink = markdownJourney && preview.get('demo') !== 'tenants' && preview.get('restore') !== '1' && query === demoOrganizationName;
  search.value = legacyLink ? workspaceNames.north.name : query;
  const scope = preview.get('scope');
  const selectedFilter = filters.find(button => button.dataset.filter === scope);
  if (selectedFilter) setFilter(scope);
  const requestedPage = Number(preview.get('page'));
  if (Number.isSafeInteger(requestedPage) && requestedPage > 0) page = requestedPage;
}
scenario.addEventListener('change', changeScenario);
switcher.addEventListener('toggle', () => {
  if (!switcher.open) return;
  switcherSearch.value = '';
  renderSwitcher();
  switcherSearch.focus();
});
switcherSearch.addEventListener('input', renderSwitcher);
switcher.addEventListener('click', event => {
  const link = event.target.closest('a');
  if (!link) return;
  switcher.open = false;
  if (link.pathname === location.pathname && link.hash === location.hash) {
    (routeId() ? byId('space-title') : byId('catalog-heading')).focus();
  }
});
document.addEventListener('click', event => {
  if (switcher.open && !switcher.contains(event.target)) switcher.open = false;
});
document.addEventListener('keydown', event => {
  if (event.key === 'Escape' && switcher.open) {
    event.preventDefault();
    switcher.open = false;
    byId('dock-context').focus();
  }
});
search.addEventListener('input', () => { page = 1; renderCatalog() });
for (const button of filters) button.addEventListener('click', () => setFilter(button.dataset.filter));
byId('previous-page').addEventListener('click', () => { page--; renderCatalog(); byId('space-list').querySelector('a').focus() });
byId('next-page').addEventListener('click', () => { page++; renderCatalog(); byId('space-list').querySelector('a').focus() });
byId('catalog-state').addEventListener('click', event => {
  const action = event.target.closest('[data-state-action]')?.dataset.stateAction;
  if (action === 'clear') {
    search.value = '';
    setFilter('all');
    search.focus();
  } else if (action === 'retry') {
    scenario.value = 'loading';
    changeScenario();
    setTimeout(() => {
      if (scenario.value !== 'loading') return;
      scenario.value = 'admin';
      changeScenario();
      search.focus();
    }, 450);
  }
});
byId('dock-search').addEventListener('click', () => {
  if (routeId()) {
    window.addEventListener('hashchange', () => search.focus(), {once: true});
    location.hash = 'spaces';
  } else search.focus();
});
byId('tree-toggle').addEventListener('click', () => setTree(byId('space-workbench').dataset.tree === 'false'));
byId('space-workbench').addEventListener('keydown', event => {
  if (event.key === 'Escape' && byId('space-workbench').dataset.tree === 'true') {
    setTree(false);
    byId('tree-toggle').focus();
  }
});
const create = byId('create-space');
create.addEventListener('click', () => {
  nameInput.value = '';
  showNameError();
  byId('creation-environment').textContent = profile.environment;
  panel.showModal();
});
for (const id of ['close-dialog', 'cancel-dialog']) byId(id).addEventListener('click', () => panel.close());
panel.addEventListener('close', () => { if (!byId('catalog').hidden && !create.hidden) create.focus() });
nameInput.addEventListener('input', () => showNameError());
async function createDemoSpace(name) {
  const tenantId = demoTenantId();
  try {
    const result = await withWorkspaceLock(() => {
      const data = loadWorkspace(tenantDemoScope);
      const error = demoSpaceNameError(data,name,tenantId);
      if (error) return {error};
      const {id} = createSpace(data,name,'',tenantId,{starterPage:false});
      if (!persistWorkspace(data,tenantDemoScope)) return {error:'Не удалось сохранить пространство. Повторите попытку.'};
      return {id,data};
    },tenantDemoScope);
    if (result.error) return result;
    demoWorkspace = result.data;
    return result;
  } catch {
    return {error:'Не удалось сохранить пространство. Повторите попытку.'};
  }
}

function createPreviewSpace(name) {
  if (!name || [...name].length > 100) {
    return {error:!name ? 'Введите название пространства.' : 'Введите не больше 100 символов.'};
  }
  const normalized = value => value.normalize('NFC').toLocaleLowerCase('ru');
  if (visibleSpaces().some(space => space.isOwned && normalized(space.name) === normalized(name))) {
    return {error:'У вас уже есть пространство с таким названием.'};
  }
  const id = 'created-' + ++createdCount;
  addedSpaces.push({id,name,owner:profile.person,environment:profile.environment});
  return {id};
}

const createForm = byId('creation-form');
createForm.addEventListener('submit', async event => {
  event.preventDefault();
  const submit = createForm.querySelector('button[type="submit"]');
  if (!profile.canCreate || submit.disabled) return;
  submit.disabled = true;
  const name = nameInput.value.trim();
  const result = tenantDemo ? await createDemoSpace(name) : createPreviewSpace(name);
  submit.disabled = false;
  if (result.error) {
    showNameError(result.error);
    nameInput.focus();
    return;
  }
  panel.close();
  if (tenantDemo) location.assign(spaceLink({id:result.id}));
  else location.hash = `space/${result.id}`;
});
if (tenantDemo) window.addEventListener('storage', event => {
  if (event.key !== null && event.key !== workspaceStorageKey + tenantDemoScope) return;
  demoWorkspace = loadWorkspace(tenantDemoScope);
  renderRoute();
  if (switcher.open) renderSwitcher();
});
window.addEventListener('hashchange', () => {
  if (location.hash === '#main') return;
  switcher.open = false;
  renderRoute({moveFocus: true});
});
renderRoute();
