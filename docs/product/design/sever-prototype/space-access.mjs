import {escapeHtml as h, icon} from './ui.mjs';

const givenNames = [['Анна', 'anna'], ['Дарья', 'daria'], ['Кира', 'kira'], ['Лидия', 'lidia'], ['Нина', 'nina'], ['Ольга', 'olga'], ['София', 'sofia'], ['Татьяна', 'tatiana']];
const surnames = [['Абрамова', 'abramova'], ['Ветрова', 'vetrova'], ['Данилова', 'danilova'], ['Иванова', 'ivanova'], ['Крылова', 'krylova'], ['Леонова', 'leonova'], ['Никитина', 'nikitina'], ['Павлова', 'pavlova'], ['Романова', 'romanova'], ['Савина', 'savina'], ['Фролова', 'frolova'], ['Яшина', 'yashina']];
const employeeDepartments = ['Продукт', 'Дизайн', 'Операции', 'Разработка', 'Поддержка'];
const people = [
  {id: 'maria', name: 'Мария Ким', initials: 'МК', email: 'maria.kim@kontur.example', department: 'Продукт'},
  {id: 'alex', name: 'Алексей Орлов', initials: 'АО', email: 'alexey.orlov@kontur.example', department: 'Дизайн'},
  {id: 'elena', name: 'Елена Белова', initials: 'ЕБ', email: 'elena.belova@kontur.example', department: 'Операции'},
  {id: 'irina', name: 'Ирина Волкова', initials: 'ИВ', email: 'irina.volkova@kontur.example', department: 'Операции', admin: true},
  ...surnames.flatMap(([surname, familySlug], familyIndex) => givenNames.map(([first, firstSlug], nameIndex) => ({
    id: `${firstSlug}-${familySlug}`,
    name: `${first} ${surname}`,
    initials: `${first[0]}${surname[0]}`,
    email: `${firstSlug}.${familySlug}@kontur.example`,
    department: employeeDepartments[(familyIndex + nameIndex) % employeeDepartments.length],
  }))),
].sort((a, b) => a.name.localeCompare(b.name, 'ru'));
const groups = [{id: 'documentation', name: 'Команда документации', members: ['alex', 'elena']}];
const roles = {
  view: {name: 'Просмотр', role: 'Читатель', rank: 1},
  edit: {name: 'Редактор', role: 'Редактор пространства', rank: 2},
  owner: {name: 'Владелец', role: 'Владелец', rank: 3},
};
const operations = [
  ['view', 'Просматривать'], ['create', 'Создавать'], ['edit', 'Редактировать'],
  ['move', 'Перемещать'], ['delete', 'Удалять'],
];
const resources = [['section', 'Разделы'], ['page', 'Страницы']];
const defaults = new Set(['section:view', 'section:edit', 'page:view', 'page:create', 'page:edit']);
const accountByActor = {admin: 'irina', creator: 'maria', member: 'alex'};
const departments = [...new Set(people.map((person) => person.department))].sort((a, b) => a.localeCompare(b, 'ru'));
const topics = ['База знаний', 'Процессы', 'Планирование', 'Исследования', 'Стандарты', 'Проекты', 'Материалы', 'Справочник', 'Встречи', 'Отчёты'];
const teams = ['поддержки', 'маркетинга', 'разработки', 'контента', 'операций', 'продукта', 'архитектуры', 'аналитики', 'обучения', 'коммуникаций'];
const spaces = [
  {id: 'team-docs', name: 'Документы команды', owner: 'maria', sections: [{id: 'overview', name: 'Обзор проекта'}, {id: 'decisions', name: 'Решения команды'}], grants: [{id: 'design-staff', type: 'rule', target: 'Дизайн', access: 'view'}]},
  {id: 'engineering', name: 'Инженерная база', owner: 'alex', grants: []},
  {id: 'onboarding', name: 'База адаптации', owner: 'elena', grants: []},
  ...teams.flatMap((team, teamIndex) => topics.map((topic, topicIndex) => ({
    id: `sample-${teamIndex}-${topicIndex}`,
    name: `${topic} ${team}`,
    owner: 'elena',
    grants: [],
  }))).slice(0, 97),
];
const edition = {
  actor: 'admin', page: 'access', activeSpace: 'team-docs', employeeSpace: null, inside: false, allSpaces: false, creation: new Set(['maria']),
};
const initialLists = {
  catalog: {query: '', role: '', source: '', page: 1},
  access: {query: '', role: '', source: '', page: 1},
  creation: {query: '', department: '', right: '', page: 1},
  recipient: {query: '', kind: '', department: '', page: 1},
};
const lists = structuredClone(initialLists);
let pickedRecipient = '';

const view = document.querySelector('#view');
const panel = document.querySelector('#panel');
const adminNav = document.querySelector('#admin-nav');
const accountName = document.querySelector('#account-name');
const accountAvatar = document.querySelector('#account-avatar');
const status = document.querySelector('#review-status');
const personById = (id) => people.find((person) => person.id === id);
const spaceById = (id) => spaces.find((space) => space.id === id);
const activeSpace = () => spaceById(edition.activeSpace);
const actor = () => personById(accountByActor[edition.actor]);
const managedSpace = () => {
  if (actor().admin) return edition.page === 'access' && !edition.allSpaces ? activeSpace() : undefined;
  const space = spaceById(edition.employeeSpace);
  return edition.inside && space?.owner === actor().id ? space : undefined;
};
const normalized = (value) => value.trim().normalize('NFC').toLocaleLowerCase('ru');
const roleOption = ([value, role], selected) => `<option value="${value}"${value === selected ? ' selected' : ''}>${role.role}</option>`;
const roleOptions = (selected) => Object.entries(roles).filter(([key]) => key !== 'owner').map((entry) => roleOption(entry, selected)).join('');
const editorRights = (space) => space.editorRights ?? defaults;
const allowed = (space, sources, resource, operation) => sources.some(({access}) =>
  access === 'owner' || (access === 'view' && operation === 'view') ||
  (access === 'edit' && editorRights(space).has(`${resource}:${operation}`)));

document.querySelectorAll('[data-icon]').forEach((node) => { node.innerHTML = icon(node.dataset.icon); });

const matches = (value, query) => normalized(value).includes(query);
const matchesPerson = (person, query) => matches(person.name, query) || matches(person.email, query);
const departmentOptions = departments.map((department) => [department, department]);
const roleFilters = [['view', 'Просмотр'], ['edit', 'Редактор'], ['owner', 'Владелец']];
const sourceFilters = [['person', 'Назначено человеку'], ['group', 'Роль группы'], ['rule', 'Правило по атрибуту'], ['system', 'Системный доступ']];

function searchField(list, label, placeholder) {
  return `<label class="list-search"><span>${label}</span><input type="search" class="field" data-list="${list}" data-filter="query" placeholder="${placeholder}" value="${h(lists[list].query)}"${list === 'recipient' ? ' autofocus' : ''}></label>`;
}

function filterField(list, key, label, first, options) {
  return `<label><span>${label}</span><select class="field" data-list="${list}" data-filter="${key}">${[['', first], ...options].map(([value, title]) =>
    `<option value="${h(value)}"${value === lists[list][key] ? ' selected' : ''}>${h(title)}</option>`).join('')}</select></label>`;
}

function accessFilters(list) {
  return filterField(list, 'role', 'Итоговая роль', 'Все роли', roleFilters) +
    filterField(list, 'source', 'Основание', 'Все основания', sourceFilters);
}

function pageSlice(items, list, size) {
  const filter = lists[list];
  const pages = Math.max(1, Math.ceil(items.length / size));
  filter.page = Math.max(1, Math.min(pages, filter.page));
  const start = (filter.page - 1) * size;
  return {rows: items.slice(start, start + size), count: items.length, page: filter.page, pages, start};
}

function pager(list, {rows, count, page, pages, start}) {
  if (!count) return '';
  return `<nav class="pager" aria-label="Страницы результатов"><span role="status">${start + 1}–${start + rows.length} из ${count}</span>
    <div><button type="button" class="btn icon" data-paginate="${list}" data-step="-1" aria-label="Предыдущая страница"${page === 1 ? ' disabled' : ''}>${icon('left')}</button>
    <span aria-label="Страница ${page} из ${pages}">${page} / ${pages}</span>
    <button type="button" class="btn icon" data-paginate="${list}" data-step="1" aria-label="Следующая страница"${page === pages ? ' disabled' : ''}>${icon('arrow')}</button></div></nav>`;
}

const emptyResults = (list) => `<div class="empty-results"><p>Совпадений нет.</p><button type="button" class="btn" data-reset="${list}">Сбросить фильтры</button></div>`;

function grantMatches(grant, person) {
  if (grant.type === 'rule') return person.department === grant.target;
  if (grant.type === 'group') return groups.find((group) => group.id === grant.target)?.members.includes(person.id);
  return person.id === grant.target;
}

function resolvedAccess(space, person) {
  const sources = [];
  if (space.owner === person.id) sources.push({access: 'owner', type: 'system', label: 'Владелец пространства'});
  if (person.admin) sources.push({access: 'view', type: 'system', label: 'Роль администратора организации'});
  space.grants.filter((grant) => grantMatches(grant, person)).forEach((grant) => {
    const role = roles[grant.access].role;
    const label = grant.type === 'rule' ? `Правило «Отдел = ${grant.target}» · ${role}`
      : grant.type === 'group' ? `Группа «${groups.find((group) => group.id === grant.target).name}» · ${role}`
        : `Назначено напрямую · ${role}`;
    sources.push({access: grant.access, type: grant.type, label});
  });
  const access = sources.reduce((best, item) => roles[item.access].rank > (roles[best]?.rank ?? 0) ? item.access : best, null);
  return {access, sources};
}

function accessTag(access) {
  return `<span class="tag access-${access}">${roles[access].name}</span>`;
}

function matchAccess({access, sources}, {role, source}) {
  return (!role || role === access) && (!source || sources.some(({type}) => type === source));
}

function matchingSpaceAccess() {
  const filter = lists.catalog;
  const query = normalized(filter.query);
  return spaces.map((space) => ({space, subjects: accessRows(space).filter((subject) =>
    (matches(space.name, query) || matchesPerson(subject.person, query)) && matchAccess(subject, filter))})).filter(({subjects}) => subjects.length);
}

function renderCatalog() {
  const filtered = matchingSpaceAccess();
  const page = pageSlice(filtered, 'catalog', 10);
  document.querySelector('#catalog-list').innerHTML = page.rows.map(({space}) => `
    <button type="button" class="space-choice" data-space="${space.id}" aria-pressed="${space.id === edition.activeSpace}">
      <span class="folder-mark">${icon('folder', 18)}</span><strong>${h(space.name)}</strong>
    </button>`).join('');
  const outsideFilter = !filtered.some(({space}) => space.id === edition.activeSpace);
  document.querySelector('#catalog-page').innerHTML = (page.count ? pager('catalog', page) : emptyResults('catalog')) +
    (outsideFilter ? '<p class="catalog-context muted-hint">Выбранное пространство вне результатов.</p>' : '');
}

function accessRows(space) {
  return people.map((person) => ({person, ...resolvedAccess(space, person)})).filter(({access}) => access);
}

function accessTable(subjects, label) {
  return `<table class="permission-table" aria-label="${h(label)}">
    <thead><tr><th scope="col">Человек</th><th scope="col">Доступ</th><th scope="col">Основание</th></tr></thead>
    <tbody>${subjects.map(({person, access, sources}) => `<tr><td><div class="subject-name">${h(person.name)}</div><div class="subject-detail">${h(person.department)}</div></td><td>${accessTag(access)}</td><td><ul class="basis-list">${sources.map((source) => `<li>${h(source.label)}</li>`).join('')}</ul></td></tr>`).join('')}</tbody>
  </table>`;
}

function grantItem(grant) {
  const recipient = grant.type === 'rule' ? `Отдел = ${grant.target}`
    : grant.type === 'group' ? groups.find((group) => group.id === grant.target).name : personById(grant.target).name;
  const mode = grant.type === 'rule' ? 'По правилу · ABAC' : 'По роли · RBAC';
  return `<li class="grant-item">
    <div><strong>${h(recipient)} · ${roles[grant.access].role}</strong><span class="basis">${mode}</span></div>
    ${actor().admin || grant.type === 'person' ? `<button type="button" class="btn" data-remove="${grant.id}" aria-label="Отозвать ${h(recipient)}">Отозвать</button>` : ''}
  </li>`;
}

function renderInspector() {
  const space = managedSpace();
  if (!space) return;
  document.querySelector('#inspector').innerHTML = `
    <header class="space-head">
      <h2 tabindex="-1" id="access-space-heading">${actor().admin ? h(space.name) : 'Доступ к пространству'}</h2>
      ${actor().admin ? `<div class="byline"><span class="tag">${icon('lock', 13)}Закрытое</span><span>Владелец: ${h(personById(space.owner).name)}</span></div>` : ''}
      <div class="actions">
        <button type="button" class="btn primary" data-dialog="role">${icon('user')}Дать роль</button>
        ${actor().admin ? `<button type="button" class="btn" data-dialog="rule">${icon('sliders')}Добавить правило</button>` : ''}
        <button type="button" class="btn" data-dialog="matrix">${icon('settings')}Права редактора</button>
      </div>
    </header>
    <div class="inline-title"><h3 id="access-heading">У кого есть доступ</h3></div>
    <div class="list-controls">${searchField('access', 'Сотрудник', 'Имя или почта')}${accessFilters('access')}</div>
    <div id="space-access-results"></div>
    <h3 class="subheading">Назначения и правила</h3>
    ${space.grants.length ? `<ul class="grant-list">${space.grants.map(grantItem).join('')}</ul>` : '<p class="muted-hint">Роли сотрудникам и правила ещё не добавлены.</p>'}`;
  renderSpaceAccess();
}

function renderSpaceAccess() {
  const space = managedSpace();
  if (!space) return;
  const filter = lists.access;
  const query = normalized(filter.query);
  const subjects = accessRows(space).filter((subject) => matchesPerson(subject.person, query) && matchAccess(subject, filter));
  const page = pageSlice(subjects, 'access', 8);
  document.querySelector('#space-access-results').innerHTML = page.count ?
    accessTable(page.rows, 'У кого есть доступ') + pager('access', page) : emptyResults('access');
}

function renderAdminAccess() {
  const search = `<div class="catalog-head list-controls">${searchField('catalog', 'Пространство или сотрудник', 'Название, имя или почта')}${accessFilters('catalog')}</div>`;
  view.innerHTML = `
    <header class="page-heading"><div><h1 id="main-title" tabindex="-1">Доступ к пространствам</h1><p>Проверьте, кто может читать и редактировать материалы.</p></div>
      <button type="button" data-overview class="btn" aria-pressed="${edition.allSpaces}">${icon('list')}Права по всем пространствам</button></header>
    ${edition.allSpaces ? `<section class="surface" aria-label="Права по всем пространствам">${search}<div class="org-overview" id="overview-results"></div><div id="catalog-page"></div></section>` : `
      <div class="org-grid">
        <section class="surface" aria-label="Пространства организации">${search}<div class="catalog-list" id="catalog-list"></div><div id="catalog-page"></div></section>
        <section class="surface inspector" id="inspector" aria-labelledby="access-space-heading"></section>
      </div>`}`;
  if (edition.allSpaces) return renderOverview();
  renderCatalog();
  renderInspector();
}

function renderOverview() {
  const page = pageSlice(matchingSpaceAccess(), 'catalog', 6);
  document.querySelector('#overview-results').innerHTML = page.rows.map(({space, subjects}) => {
    return `<section class="org-overview-space"><div><h2>${h(space.name)}</h2><button type="button" class="btn" data-space="${space.id}" aria-label="Настроить доступ: ${h(space.name)}">Настроить доступ</button></div>${accessTable(subjects.slice(0, 6), `Доступ: ${space.name}`)}${subjects.length > 6 ? `<p class="muted-hint overview-more">Всего ${subjects.length} · <button type="button" class="btn" data-space="${space.id}">Показать всех</button></p>` : ''}</section>`;
  }).join('');
  document.querySelector('#catalog-page').innerHTML = page.count ? pager('catalog', page) : emptyResults('catalog');
}

function renderCreationRights() {
  view.innerHTML = `
    <header class="page-heading"><div><h1 id="main-title" tabindex="-1">Кто может создавать пространства</h1><p>Выдайте право сотруднику. После создания он станет владельцем своего пространства.</p></div></header>
    <section class="surface roster" aria-label="Право создавать пространства">
      <div class="list-controls">${searchField('creation', 'Поиск сотрудников', 'Имя или рабочая почта')}
        ${filterField('creation', 'department', 'Отдел', 'Все отделы', departmentOptions)}
        ${filterField('creation', 'right', 'Право создания', 'Любое состояние', [['granted', 'Может создавать'], ['missing', 'Право не выдано']])}</div>
      <div id="creation-results"></div></section>`;
  renderCreationRows();
}

function renderCreationRows() {
  const filter = lists.creation;
  const query = normalized(filter.query);
  const candidates = people.filter((person) => !person.admin && matchesPerson(person, query) &&
    (!filter.department || person.department === filter.department) &&
    (!filter.right || edition.creation.has(person.id) === (filter.right === 'granted')));
  const page = pageSlice(candidates, 'creation', 10);
  document.querySelector('#creation-results').innerHTML = page.count ?
    `<ul class="grant-list org-permissions">${page.rows.map((person) => {
        const permitted = edition.creation.has(person.id);
        return `<li class="person-item">
          <div class="row-identity"><span class="avatar" aria-hidden="true">${person.initials}</span><div><div class="subject-name">${h(person.name)}</div><div class="subject-detail">${h(person.email)} · ${h(person.department)}</div></div></div>
          <span class="tag state">${permitted ? 'Может создавать' : 'Право не выдано'}</span>
          <button type="button" class="btn ${permitted ? '' : 'primary'}" data-creation="${person.id}" aria-label="${permitted ? 'Отозвать' : 'Выдать'} право создания: ${h(person.name)}">${permitted ? 'Отозвать' : 'Выдать право'}</button>
        </li>`;
      }).join('')}</ul>${pager('creation', page)}` : emptyResults('creation');
}

function employeeContent(space, access, sources) {
  const sections = space.sections ?? [];
  const basis = access === 'owner' ? '' : `<ul class="basis-list">${sources.map(({label}) => `<li>${h(label)}</li>`).join('')}</ul>`;
  const actions = resources.map(([resource, label]) => {
    const names = operations.filter(([operation]) => allowed(space, sources, resource, operation)).map(([, name]) => name.toLocaleLowerCase('ru'));
    return `<li><strong>${label}</strong><span>${names.length ? names.join(', ') : 'доступ закрыт'}</span></li>`;
  }).join('');
  const body = !allowed(space, sources, 'section', 'view') ? '<p>Вам недоступен просмотр разделов.</p>'
    : sections.length ? `<div class="section-preview"><h2>Разделы</h2><ul>${sections.map(({name}) => `<li><span>${icon('folder', 16)}${h(name)}</span><span class="tag">${allowed(space, sources, 'section', 'edit') ? 'Можно редактировать' : 'Только просмотр'}</span></li>`).join('')}</ul></div>`
      : `<div class="space-empty"><div><span class="folder-mark">${icon('layers')}</span><p>В пространстве пока нет разделов</p></div></div>`;
  return `${body}<div class="capabilities"><h2>Ваши права в пространстве</h2><ul class="action-list">${actions}</ul>${basis}</div>`;
}

function renderEmployee() {
  const person = actor();
  const accessible = spaces.filter((space) => resolvedAccess(space, person).access);
  const selected = accessible.find((space) => space.id === edition.employeeSpace);
  if (selected) {
    const {access, sources} = resolvedAccess(selected, person);
    view.innerHTML = `
      <div class="page-heading owner-nav"><button type="button" class="btn" data-back>${icon('left')}К пространствам</button>
        ${access === 'owner' ? `<button type="button" class="btn${edition.inside ? '' : ' primary'}" data-manage aria-pressed="${edition.inside}">${icon('shield')}${edition.inside ? 'К содержимому' : 'Настроить доступ'}</button>` : ''}</div>
      <section class="surface owner-space space-head" aria-labelledby="main-title">
        <div class="inline-title"><span class="folder-mark">${icon('folder', 18)}</span><h1 id="main-title" tabindex="-1">${h(selected.name)}</h1></div>
        <div class="byline"><span class="tag">${icon('lock', 13)}Закрытое</span>${accessTag(access)}<span>Создатель: ${h(personById(selected.owner).name)}</span></div>
        ${edition.inside && access === 'owner' ? '<section id="inspector" class="owner-inspector" aria-labelledby="access-space-heading"></section>' : employeeContent(selected, access, sources)}
      </section>`;
    if (edition.inside && access === 'owner') renderInspector();
    return;
  }
  view.innerHTML = `
    <header class="page-heading"><div><h1 id="main-title" tabindex="-1">Пространства</h1><p>Ваши пространства и материалы, которыми с вами поделились.</p></div>
      ${edition.creation.has(person.id) ? `<button type="button" class="btn primary" data-dialog="space">${icon('plus')}Создать пространство</button>` : ''}
    </header>
    <section class="surface employee-list" aria-label="Доступные пространства">
      ${accessible.length ? accessible.map((space) => `<button type="button" class="space-choice" data-space="${space.id}"><span class="folder-mark">${icon('folder', 18)}</span><strong>${h(space.name)}</strong>${accessTag(resolvedAccess(space, person).access)}</button>`).join('') : '<p class="catalog-note">Доступных пространств пока нет.</p>'}
    </section>`;
}

function render() {
  const person = actor();
  accountName.textContent = person.name;
  accountAvatar.textContent = person.initials;
  adminNav.hidden = !person.admin;
  document.querySelectorAll('[data-actor]').forEach((button) => button.setAttribute('aria-pressed', String(button.dataset.actor === edition.actor)));
  adminNav.querySelectorAll('[data-page]').forEach((button) => button.setAttribute('aria-pressed', String(button.dataset.page === edition.page)));
  if (person.admin) {
    if (edition.page === 'access') renderAdminAccess();
    else renderCreationRights();
  } else renderEmployee();
}

function route() {
  const name = location.hash.slice(1);
  if (name === 'editor' || name === 'creation') {
    edition.actor = 'admin';
    edition.page = name === 'creation' ? 'creation' : 'access';
    edition.allSpaces = false;
  } else if (name === 'owner') edition.actor = 'creator';
  else if (Object.hasOwn(accountByActor, name)) edition.actor = name;
  else if (name) return;
  edition.employeeSpace = name === 'owner' ? 'team-docs' : null;
  edition.inside = name === 'owner';
  lists.access = {...initialLists.access};
  render();
  if (name === 'editor') openDialog('matrix');
}

function roleRecipientFields() {
  const admin = actor().admin;
  return `<div class="recipient-area">
    <div class="list-controls">${searchField('recipient', admin ? 'Сотрудник или группа' : 'Сотрудник', admin ? 'Имя, почта или группа' : 'Имя или рабочая почта')}
      ${admin ? filterField('recipient', 'kind', 'Получатель', 'Все получатели', [['person', 'Сотрудники'], ['group', 'Группы']]) : ''}
      ${filterField('recipient', 'department', 'Отдел', 'Все отделы', departmentOptions)}</div>
    <div id="recipient-results"></div><input type="hidden" name="recipient" value="${h(pickedRecipient)}">
    <p class="selected-recipient" id="role-selection" role="status"></p>
  </div>`;
}

function renderRoleRecipients() {
  const space = managedSpace();
  const filter = lists.recipient;
  const query = normalized(filter.query);
  const staff = filter.kind === 'group' ? [] : people.filter((person) => !person.admin && person.id !== space.owner &&
    matchesPerson(person, query) && (!filter.department || person.department === filter.department)).map((person) => ({
    key: `person:${person.id}`, title: person.name, detail: `${person.email} · ${person.department}`,
  }));
  const matchedGroups = actor().admin && filter.kind !== 'person' && !filter.department ? groups.filter(({name}) => matches(name, query)).map((group) => ({
    key: `group:${group.id}`, title: group.name, detail: `Группа · ${group.members.length} участника`,
  })) : [];
  const page = pageSlice([...matchedGroups, ...staff], 'recipient', 4);
  panel.querySelector('#recipient-results').innerHTML = page.count ?
    `<div class="recipient-list">${page.rows.map(({key, title, detail}) => `<button type="button" class="recipient-option" data-recipient="${key}" aria-pressed="${key === pickedRecipient}"><span><strong>${h(title)}</strong><span class="subject-detail">${h(detail)}</span></span>${key === pickedRecipient ? icon('check') : ''}</button>`).join('')}</div>${pager('recipient', page)}` : emptyResults('recipient');
  const [kind, id] = pickedRecipient.split(':');
  const name = kind === 'person' ? personById(id).name : groups.find((group) => group.id === id)?.name;
  panel.querySelector('[name="recipient"]').value = pickedRecipient;
  panel.querySelector('button[type="submit"]').disabled = !name;
  panel.querySelector('#role-selection').textContent = name ? `Выбрано: ${name}` : 'Выберите получателя.';
}

function openDialog(type) {
  const space = managedSpace();
  const titles = {role: 'Дать роль', rule: 'Добавить правило', matrix: 'Права редактора пространства', space: 'Новое пространство'};
  let fields;
  if (type === 'matrix') {
    fields = `<p class="matrix-scope"><strong>${h(space.name)}</strong><span>Матрица применяется ко всем редакторам этого пространства.</span></p>
      <table class="matrix" aria-label="Разрешения редактора пространства"><thead><tr><th scope="col">Действие</th>
      ${resources.map(([, label]) => `<th scope="col">${label}</th>`).join('')}</tr></thead>
      <tbody>${operations.map(([operation, label]) => `<tr><th scope="row">${label}</th>${resources.map(([resource, name]) => {
        const key = `${resource}:${operation}`;
        return `<td><input type="checkbox" name="right" value="${key}" aria-label="${name}: ${label.toLocaleLowerCase('ru')}"${editorRights(space).has(key) ? ' checked' : ''}></td>`;
      }).join('')}</tr>`).join('')}</tbody></table>
      <p class="dialog-preview">Для изменений нужен просмотр. Отключение просмотра разделов закрывает доступ к их страницам.</p>`;
  } else if (type === 'role') {
    lists.recipient = {...initialLists.recipient};
    pickedRecipient = '';
    fields = `${roleRecipientFields()}
      <label class="dialog-field">Роль<select name="access" class="field">${roleOptions('edit')}</select></label>
      <p class="dialog-preview">Пространство: ${h(space.name)}. <span id="role-preview"></span></p>`;
  } else if (type === 'rule') {
    fields = `<p class="muted-hint">Пространство: ${h(space.name)}. Отдел берётся из профиля сотрудника в организации.</p>
      <label class="dialog-field">Если отдел сотрудника<select name="department" class="field" autofocus>${departments.map((department) => `<option value="${h(department)}">${h(department)}</option>`).join('')}</select></label>
      <label class="dialog-field">То выдать роль<select name="access" class="field">${roleOptions('view')}</select></label>
      <p class="dialog-preview" id="rule-preview" role="status"></p>`;
  } else {
    if (!edition.creation.has(actor().id)) return;
    fields = `<label class="dialog-field">Название<input name="name" class="field" placeholder="Например, База знаний" aria-describedby="name-help dialog-error" autofocus></label>
      <p class="muted-hint" id="name-help">От 1 до 100 символов. Название уникально среди ваших пространств.</p>
      <p class="dialog-preview">${icon('lock', 14)} Закрытое пространство. Вы станете владельцем. Администратор организации сможет просматривать материалы.</p>`;
  }
  panel.innerHTML = `<header class="dialog-header"><h2 id="dialog-title">${titles[type]}</h2><button type="button" class="btn icon ghost" data-close aria-label="Закрыть">${icon('close')}</button></header>
    <form class="dialog-body" data-type="${type}" novalidate>${fields}<p id="dialog-error" class="dialog-error" role="alert" hidden></p>
    <div class="dialog-buttons"><button type="button" class="btn" data-close>Отмена</button><button type="submit" class="btn primary">${type === 'space' ? 'Создать' : 'Сохранить'}</button></div></form>`;
  if (type === 'role') {
    renderRoleRecipients();
    const select = panel.querySelector('[name="access"]');
    const preview = panel.querySelector('#role-preview');
    const update = () => {
      preview.textContent = select.value === 'edit' ? 'Действия редактора определяются матрицей этого пространства.' : 'Читатель может просматривать материалы.';
    };
    select.addEventListener('change', update);
    update();
  }
  if (type === 'rule') {
    const select = panel.querySelector('[name="department"]');
    const preview = panel.querySelector('#rule-preview');
    const update = () => {
      const matched = people.filter(({department}) => department === select.value);
      const names = matched.slice(0, 3).map(({name}) => name).join(', ');
      preview.textContent = `Совпадений: ${matched.length}. ${names}${matched.length > 3 ? ` и ещё ${matched.length - 3}` : ''}.`;
    };
    select.addEventListener('change', update);
    update();
  }
  if (type === 'matrix') {
    const permissions = new Map([...panel.querySelectorAll('[name="right"]')].map((checkbox) => [checkbox.value, checkbox]));
    const sync = () => {
      for (const [key, checkbox] of permissions) {
        const [resource, operation] = key.split(':');
        const requiresBranch = resource === 'page' && !permissions.get('section:view').checked;
        const requiresView = operation !== 'view' && !permissions.get(`${resource}:view`).checked;
        checkbox.disabled = requiresBranch || requiresView;
        if (checkbox.disabled) checkbox.checked = false;
      }
    };
    panel.querySelector('.matrix').addEventListener('change', sync);
    sync();
  }
  panel.showModal();
}

const listRenderers = {
  catalog: () => edition.allSpaces ? renderOverview() : renderCatalog(),
  access: renderSpaceAccess,
  creation: renderCreationRows,
  recipient: renderRoleRecipients,
};

function updateListFilter(event) {
  const {list, filter} = event.target.dataset;
  if (!list || !filter) return;
  lists[list][filter] = event.target.value;
  lists[list].page = 1;
  listRenderers[list]();
}

document.addEventListener('input', (event) => {
  if (event.target.tagName === 'INPUT') updateListFilter(event);
});
document.addEventListener('change', (event) => {
  if (event.target.tagName === 'SELECT') updateListFilter(event);
});

document.addEventListener('click', (event) => {
  const button = event.target.closest('button');
  if (!button) return;
  if (button.hasAttribute('data-close')) return panel.close();
  const {actor: newActor, page, space, dialog, creation, remove, paginate, step, reset, recipient} = button.dataset;
  if (paginate) {
    lists[paginate].page += Number(step);
    listRenderers[paginate]();
    const next = document.querySelector(`[data-paginate="${paginate}"][data-step="${step}"]`);
    (next?.disabled ? document.querySelector(`[data-paginate="${paginate}"]:not([disabled])`) : next)?.focus();
    return;
  }
  if (reset) {
    lists[reset] = {...initialLists[reset]};
    document.querySelectorAll(`[data-list="${reset}"]`).forEach((control) => { control.value = lists[reset][control.dataset.filter]; });
    listRenderers[reset]();
    document.querySelector(`[data-list="${reset}"][data-filter="query"]`)?.focus();
    return;
  }
  if (recipient && managedSpace()) {
    pickedRecipient = recipient;
    renderRoleRecipients();
    panel.querySelector(`[data-recipient="${recipient}"]`)?.focus();
    return;
  }
  if (newActor) {
    if (location.hash !== '#' + newActor) location.hash = newActor;
    return;
  }
  if (page && actor().admin) {
    edition.page = page;
    return render();
  }
  if (button.hasAttribute('data-overview') && actor().admin) {
    edition.allSpaces = !edition.allSpaces;
    lists.catalog.page = 1;
    renderAdminAccess();
    view.querySelector('[data-overview]').focus();
    return;
  }
  if (space) {
    if (actor().admin) {
      edition.activeSpace = space;
      lists.access = {...initialLists.access};
      if (edition.allSpaces) {
        lists.access = {query: matches(activeSpace().name, normalized(lists.catalog.query)) ? '' : lists.catalog.query,
          role: lists.catalog.role, source: lists.catalog.source, page: 1};
        edition.allSpaces = false;
        renderAdminAccess();
      } else {
        renderCatalog();
        renderInspector();
      }
      document.querySelector('#access-space-heading').focus();
    } else {
      edition.employeeSpace = space;
      edition.inside = false;
      renderEmployee();
      document.querySelector('#main-title').focus();
    }
    return;
  }
  if (button.hasAttribute('data-back')) {
    edition.employeeSpace = null;
    edition.inside = false;
    renderEmployee();
    document.querySelector('#main-title').focus();
    return;
  }
  if (button.hasAttribute('data-manage') && spaceById(edition.employeeSpace)?.owner === actor().id) {
    edition.inside = !edition.inside;
    lists.access = {...initialLists.access};
    renderEmployee();
    document.querySelector(edition.inside ? '#access-space-heading' : '[data-manage]').focus();
    return;
  }
  if (dialog === 'space' || (dialog && managedSpace() && (dialog !== 'rule' || actor().admin))) return openDialog(dialog);
  if (creation && actor().admin) {
    const permitted = edition.creation.has(creation);
    if (permitted) edition.creation.delete(creation);
    else edition.creation.add(creation);
    status.textContent = permitted ? 'Право создания отозвано.' : 'Право создания выдано.';
    renderCreationRows();
    (view.querySelector(`[data-creation="${creation}"]`) ?? view.querySelector('[data-list="creation"][data-filter="query"]')).focus();
  }
  if (remove && managedSpace()) {
    const selected = managedSpace();
    const grant = selected.grants.find(({id}) => id === remove);
    if (!grant || (!actor().admin && grant.type !== 'person')) return;
    selected.grants = selected.grants.filter(({id}) => id !== remove);
    if (actor().admin) renderCatalog();
    renderInspector();
    status.textContent = 'Назначение отозвано. Список доступа обновлён.';
    document.querySelector('#access-space-heading').focus();
  }
});

panel.addEventListener('submit', (event) => {
  event.preventDefault();
  const type = event.target.dataset.type;
  const form = new FormData(event.target);
  if (type === 'space') {
    if (!edition.creation.has(actor().id)) return;
    const name = form.get('name').trim();
    const error = panel.querySelector('#dialog-error');
    const length = Array.from(name).length;
    const owner = actor().id;
    const duplicate = spaces.some((space) => space.owner === owner && normalized(space.name) === normalized(name));
    if (!length || length > 100 || duplicate) {
      error.hidden = false;
      error.textContent = !length ? 'Введите название.' : length > 100 ? 'Сократите название до 100 символов.' : 'У вас уже есть пространство с таким названием.';
      const input = panel.querySelector('[name="name"]');
      input.setAttribute('aria-invalid', 'true');
      input.focus();
      return;
    }
    const space = {id: crypto.randomUUID(), name, owner, grants: []};
    spaces.unshift(space);
    edition.employeeSpace = space.id;
    edition.activeSpace = space.id;
    lists.catalog = {...initialLists.catalog};
    edition.inside = false;
    edition.page = 'access';
    edition.allSpaces = false;
    panel.close();
    renderEmployee();
    document.querySelector('#main-title').focus();
    status.textContent = 'Пространство создано.';
  } else if (managedSpace()) {
    const space = managedSpace();
    if (type === 'matrix') {
      space.editorRights = new Set(form.getAll('right'));
      panel.close();
      renderInspector();
      document.querySelector('#access-space-heading').focus();
      status.textContent = 'Права редактора в этом пространстве сохранены.';
      return;
    }
    if ((type === 'rule' && !actor().admin) || (type === 'role' && !pickedRecipient)) return;
    const [grantType, target] = type === 'rule' ? ['rule', form.get('department')] : form.get('recipient').split(':');
    if (grantType !== 'person' && !actor().admin) return;
    const access = form.get('access');
    const existing = space.grants.find((grant) => grant.type === grantType && grant.target === target);
    if (existing) existing.access = access;
    else space.grants.push({id: crypto.randomUUID(), type: grantType, target, access});
    panel.close();
    if (actor().admin) renderCatalog();
    renderInspector();
    document.querySelector('#access-space-heading').focus();
    status.textContent = 'Настройка сохранена. Список доступа обновлён.';
  }
});

window.addEventListener('hashchange', route);
route();
