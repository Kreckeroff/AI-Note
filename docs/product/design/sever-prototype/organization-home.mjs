import {escapeHtml as html, icon} from './ui.mjs';
import {catalogFixtures} from './catalog-fixtures.mjs';

const people = {
  superadmin: ['Анна Миронова', 'АМ', 'Суперадминистратор'],
  admin: ['Мария Соколова', 'МС', 'Администратор'],
  member: ['Елена Ким', 'ЕК', 'Участник'],
};
const catalogSpaces = catalogFixtures('Документация команды');
const organizations = {
  studio: {
    name: 'Север',
    type: 'Организация',
    members: [
      ['Анна Миронова', 'АМ', 'Суперадминистратор'],
      ['Мария Соколова', 'МС', 'Администратор'],
      ['Елена Ким', 'ЕК', 'Участник'],
      ['Павел Антонов', 'ПА', 'Участник'],
    ],
    invitations: [
      {id: 1, email: 'olga@example.test', status: 'Ожидает'},
      {id: 2, email: 'pavel@example.test', status: 'Принято'},
    ],
    guests: false,
    logo: null,
    logoDraft: null,
    sso: 'Корпоративный каталог',
  },
  personal: {
    name: 'Моя среда',
    type: 'Личная среда',
    members: [],
    invitations: [],
    guests: false,
    logo: null,
    logoDraft: null,
    sso: '',
  },
};
const settings = {
  settings: ['Основные', 'settings'],
  members: ['Участники', 'user'],
  invites: ['Приглашения', 'message'],
  policies: ['Политики доступа', 'shield'],
};
const params = new URLSearchParams(location.search);
const state = {
  organization: params.get('organization') === 'personal' ? 'personal' : 'studio',
  role: Object.hasOwn(people, params.get('role')) ? params.get('role') : 'superadmin',
  notice: '',
  inviteeProfile: null,
};
const byId = id => document.getElementById(id);
const main = byId('main');
const dialog = byId('invitation-dialog');
const scenario = byId('scenario');
const pickers = [byId('organization-picker'), byId('space-picker')];
let nextInvitation = 3;
let dialogTrigger;

const organization = () => organizations[state.organization];
const isPersonal = () => state.organization === 'personal';
const profile = () => state.inviteeProfile ?? people[state.role];
const isPrivileged = () => isPersonal() || state.role === 'superadmin';
const canManage = () => isPersonal() || state.role !== 'member';

function currentView() {
  const hash = location.hash.slice(1);
  if (!canManage()) return 'home';
  if (hash === 'sso') return isPrivileged() ? 'sso' : 'members';
  return Object.hasOwn(settings, hash) ? hash : 'home';
}

function catalogLink(id) {
  const query = new URLSearchParams({theme: document.body.dataset.theme});
  if (isPersonal()) {
    query.set('demo', 'tenants');
    query.set('scenario', 'personal');
    if (id) {
      query.set('from-catalog', 'north');
      return `./?${query}#space/${id}`;
    }
  } else {
    query.set('scenario', canManage() ? 'admin' : 'member');
  }
  return `kb-spc-02.html?${query}#${id ? `space/${id}` : 'spaces'}`;
}

function availableSpaces() {
  if (isPersonal()) return [['demo-personal', 'Личные заметки', 'Владелец']];
  const shortcuts = canManage() ?
    [[0, 'Владелец'], [6, 'Редактирование'], [4, 'Просмотр по роли']] :
    [[0, 'Просмотр'], [5, 'Редактирование'], [10, 'Просмотр']];
  return shortcuts.map(([index, role]) => {
    const {id, name} = catalogSpaces[index];
    return [id, name, role];
  });
}

function renderIcons(container = document) {
  for (const element of container.querySelectorAll('[data-icon]')) {
    element.innerHTML = icon(element.dataset.icon, 16);
  }
}

function organizationMark(url, fallback, size = 16) {
  return url ? `<img class="org-logo" src="${html(url)}" alt="">` : icon(fallback, size);
}

function renderDock(view) {
  const [name, initials] = profile();
  byId('profile-name').textContent = name;
  byId('profile-avatar').textContent = initials;
  byId('current-organization').textContent = organization().name;
  byId('organization-mark').innerHTML = organizationMark(organization().logo, 'north');
  byId('catalog-link').href = catalogLink();
  byId('organizations').innerHTML = Object.entries(organizations).map(([id, team]) => {
    const current = id === state.organization ? ' aria-current="true"' : '';
    return `<button type="button" data-organization="${id}"${current}><span class="brand-mark">${organizationMark(team.logo, 'north')}</span><span>${html(team.name)}</span></button>`;
  }).join('');
  byId('spaces').innerHTML = availableSpaces().map(([id, title]) =>
    `<a href="${html(catalogLink(id))}">${icon('folder')}<span>${html(title)}</span></a>`
  ).join('') + `<a class="org-popup-catalog" href="${html(catalogLink())}">${icon('layers')}Все пространства</a>`;
  for (const anchor of document.querySelectorAll('.org-nav [data-view]')) {
    const active = anchor.dataset.view === (view === 'home' ? 'home' : 'settings');
    if (active) anchor.setAttribute('aria-current', 'page');
    else anchor.removeAttribute('aria-current');
  }
  for (const element of document.querySelectorAll('[data-management]')) element.hidden = !canManage();
}

function rowLink(href, glyph, title, caption, role = '') {
  return `<li><a class="org-row" href="${html(href)}">
    <span class="org-row-icon">${icon(glyph)}</span><span class="org-row-text"><strong>${html(title)}</strong><span class="org-row-note">${html(caption)}</span></span>
    <span class="org-role">${html(role)}</span><span class="org-row-tail">${icon('chevron',14)}</span>
  </a></li>`;
}

const inviteButton = label => `<button class="btn primary" type="button" data-invite>${icon('plus')}${label}</button>`;

function homeView() {
  const team = organization();
  const count = team.invitations.filter(invitation => invitation.status === 'Ожидает').length;
  const spaces = availableSpaces().map(([id, title, role]) => rowLink(catalogLink(id), 'folder', title, 'Закрытое пространство', role)).join('');
  const management = canManage() ? `<section class="org-panel" aria-labelledby="management-heading">
    <header class="org-panel-header"><h2 id="management-heading">Управление организацией</h2></header>
    <ul class="org-rows">
      ${rowLink('#settings', 'settings', 'Основные настройки', 'Название организации')}
      ${rowLink('#members', 'user', 'Участники', 'Состав и роли')}
      ${rowLink('#invites', 'message', 'Приглашения', count ? `Ожидают ответа · ${count}` : 'Ожидающих приглашений пока нет')}
      ${rowLink('#policies', 'shield', 'Политики доступа', 'Вход, гости и создание пространств')}
    </ul>
  </section>` : '';
  return `<header class="org-hero">
    <div class="org-hero-title">
      <span class="org-emblem">${organizationMark(team.logo, 'north', 34)}</span>
      <div><p>${team.type}</p><h1 tabindex="-1">${html(team.name)}</h1></div>
    </div>
    ${canManage() ? inviteButton('Пригласить участника') : ''}
  </header>
  ${state.notice ? `<p class="org-feedback org-dialog-copy">${html(state.notice)}</p>` : ''}
  <div class="org-grid" data-management="${canManage()}">
    <section class="org-panel" aria-labelledby="spaces-heading">
      <header class="org-panel-header">
        <h2 id="spaces-heading">Пространства</h2>
        <a class="btn" href="${html(catalogLink())}">Все пространства${icon('arrow',14)}</a>
      </header>
      <ul class="org-rows">${spaces}</ul>
    </section>
    ${management}
  </div>`;
}

function settingsPanel(title, body, action = '') {
  return `<section class="org-panel" aria-labelledby="settings-heading">
    <header class="org-panel-header"><h2 id="settings-heading">${title}</h2>${action}</header>
    ${state.notice ? `<p class="org-settings-row org-feedback">${html(state.notice)}</p>` : ''}
    ${body}
  </section>`;
}

function logoSettings() {
  const team = organization();
  const draft = isPrivileged() && team.logoDraft;
  const actions = isPrivileged() ? `<div class="org-brand-actions">
    <input type="file" id="logo-file" accept="image/png,image/jpeg,image/webp" hidden>
    <button class="btn" type="button" id="choose-logo" data-logo="choose" aria-describedby="logo-hint">${team.logo || draft ? 'Выбрать другой файл' : 'Загрузить логотип'}</button>
    ${draft ? '<button class="btn primary" id="apply-logo" type="button" data-logo="apply">Сохранить логотип</button><button class="btn ghost" type="button" data-logo="cancel">Отменить выбор</button>' : ''}
    ${team.logo && !draft ? '<button class="btn ghost" type="button" data-logo="reset">Вернуть эмблему Севера</button>' : ''}
  </div>
  <p class="small name-error" id="logo-error" role="alert" hidden></p>` : '';
  return `<section class="org-brand-settings" aria-labelledby="logo-heading">
    <div class="org-brand-preview">
      <span class="org-emblem" aria-hidden="true">${organizationMark(draft || team.logo, 'north', 34)}</span>
      <div><h3 id="logo-heading">Логотип организации</h3>
      <p class="small muted" id="logo-hint">${draft ? 'Предпросмотр перед сохранением' : isPrivileged() ? 'PNG, JPEG или WebP до 5 МБ' : 'Логотип меняет суперадминистратор.'}</p></div>
    </div>
    ${actions}
  </section>`;
}

function basicSettings() {
  const name = html(organization().name);
  const body = isPrivileged() ? `<form class="org-form" id="name-form">
    <label for="organization-name">Название организации</label>
    <input class="field" id="organization-name" name="name" value="${name}" required aria-describedby="name-hint">
    <p class="small muted" id="name-hint">От 1 до 120 символов. Название видно в шапке и переключателе.</p>
    <p class="small name-error" id="organization-name-error" role="alert" hidden></p>
    <button class="btn primary" type="submit">Сохранить название</button>
  </form>` : `<div class="org-settings-row"><div><strong>Название организации</strong><p>${name}</p><p class="org-row-note">Название меняет суперадминистратор.</p></div></div>`;
  return settingsPanel('Основные настройки', body + logoSettings());
}

function ssoStatus(provider) {
  return provider ? `<span class="tag" data-tone="knowledge">Настроено</span><span class="org-row-note">${html(provider)}</span>` :
    '<span class="tag">Не настроено</span>';
}

const ssoAction = () => isPrivileged() ? `<a class="btn" href="#sso">${organization().sso ? 'Изменить настройки SSO' : 'Настроить SSO'}</a>` : '';

function membersView() {
  const team = organization();
  const members = isPersonal() ? [[profile()[0], profile()[1], 'Суперадминистратор'], ...team.members] : team.members;
  const sso = `<div class="org-settings-row org-sso-summary">
    <span class="org-row-icon">${icon('key')}</span>
    <div><strong>Корпоративный вход (SSO)</strong><p class="org-status-line">${ssoStatus(team.sso)}</p></div>
    ${ssoAction()}
  </div>`;
  return settingsPanel('Участники', sso + members.map(([name, initials, role]) =>
    `<div class="org-settings-row"><span class="avatar" aria-hidden="true">${html(initials)}</span><div><strong>${html(name)}</strong></div><span class="tag">${role}</span></div>`
  ).join(''), inviteButton('Пригласить участника'));
}

function invitationsView() {
  const rows = organization().invitations.map(({id, email, status}) => `<li class="org-settings-row">
    <div><strong class="org-email">${html(email)}</strong><p class="org-row-note">Участник организации</p></div>
    <span class="tag" ${status === 'Принято' ? 'data-tone="knowledge"' : ''}>${status}</span>
    ${status === 'Ожидает' ? `<div class="org-invitation-actions">
      <button type="button" class="btn" data-accept="${id}" aria-label="Посмотреть приглашение для ${html(email)}">Посмотреть</button>
      <button type="button" class="btn ghost danger" data-revoke="${id}" aria-label="Отозвать приглашение для ${html(email)}">Отозвать</button>
    </div>` : ''}
  </li>`).join('');
  return settingsPanel('Приглашения в организацию', rows ? `<ul class="org-rows">${rows}</ul>` :
    '<p class="org-empty">Приглашений пока нет.</p>', inviteButton('Пригласить участника'));
}

function policiesView() {
  const team = organization();
  return settingsPanel('Политики доступа', `<div class="org-settings-row">
    <div><strong>Способы входа</strong><p class="org-row-note">Доступные способы для этой организации</p></div>
    <span class="tag">${team.sso ? 'Почта и ИССО' : 'Почта'}</span>
    ${ssoAction()}
  </div><div class="org-settings-row">
    <div><strong>Гостевой доступ</strong><p class="org-row-note">Позволяет приглашать к отдельным материалам без членства в организации.</p></div>
    ${isPrivileged() ? `<label><input id="guests" type="checkbox" ${team.guests ? 'checked' : ''}>Разрешить</label>` :
    `<span class="tag">${team.guests ? 'Разрешён' : 'Выключен'}</span>`}
  </div><div class="org-settings-row">
    <div><strong>Создание пространств</strong><p class="org-row-note">Право назначается участникам.</p></div>
    <a class="btn" href="space-access.html#creation">Настроить права</a>
  </div><div class="org-settings-row">
    <div><strong>Доступ к пространствам</strong><p class="org-row-note">Роли настраиваются для каждого пространства.</p></div>
    <a class="btn" href="space-access.html#admin">Открыть настройки</a>
  </div>`);
}

function ssoSettings() {
  const provider = organization().sso;
  return settingsPanel('Корпоративный вход (SSO)', `<div class="org-settings-row">
    <div><strong>Действующее подключение</strong><p class="org-status-line">${ssoStatus(provider)}</p></div>
  </div>
  <form class="org-form" id="sso-form">
    <label for="provider-name">Название провайдера</label>
    <input class="field" id="provider-name" name="provider" value="${html(provider)}" placeholder="Например, корпоративный каталог" maxlength="120" required>
    <label for="demo-connection">Тестовое подключение в макете</label>
    <select class="field" id="demo-connection" name="connection">
      <option value="working">Провайдер отвечает</option>
      <option value="unavailable">Провайдер недоступен</option>
    </select>
    <p class="small" id="sso-result" role="status" hidden></p>
    <div class="org-brand-actions">
      <button class="btn" type="submit" value="check">Проверить подключение</button>
      <button class="btn primary" type="submit" value="apply" disabled>Применить настройки</button>
    </div>
  </form>`);
}

const panels = {settings:basicSettings, members:membersView, invites:invitationsView, policies:policiesView, sso:ssoSettings};

function render({focus = false} = {}) {
  const view = currentView();
  renderDock(view);
  if (view === 'home') main.innerHTML = homeView();
  else {
    const navigation = Object.entries(settings).map(([id, [title, glyph]]) =>
      `<a href="#${id}" ${id === (view === 'sso' ? 'members' : view) ? 'aria-current="page"' : ''}>${icon(glyph)}${title}</a>`
    ).join('');
    main.innerHTML = `<nav class="org-breadcrumbs" aria-label="Путь"><a href="#home">${html(organization().name)}</a>${icon('chevron',14)}<span aria-current="page">Настройки</span></nav>
      <h1 class="org-settings-title" tabindex="-1">Настройки организации</h1>
      <div class="org-settings-layout"><nav class="org-panel org-settings-nav" aria-label="Настройки организации">${navigation}</nav>
      ${panels[view]()}</div>`;
  }
  if (focus) main.querySelector('h1').focus({preventScroll:true});
}

function navigate(view, notice = '') {
  state.notice = notice;
  if (location.hash === `#${view}`) render({focus:true});
  else location.hash = view;
  if (notice) byId('feedback').textContent = notice;
}

function applyTheme(theme) {
  document.body.dataset.theme = theme;
  byId('theme').textContent = theme === 'light' ? 'Тёмная тема' : 'Светлая тема';
}

function showDialog(title, body, trigger) {
  dialogTrigger = trigger;
  dialog.innerHTML = `<div class="dialog-header">
    <h2 id="invitation-title">${html(title)}</h2><button class="btn ghost icon" type="button" data-close aria-label="Закрыть">${icon('close')}</button>
  </div>${body}`;
  dialog.showModal();
}

const dialogActions = button => `<div class="dialog-buttons">
  <button class="btn" type="button" data-close>Отмена</button><button class="btn primary" type="submit">${button}</button>
</div>`;

function openInvitation(trigger) {
  showDialog('Пригласить в организацию', `<form class="dialog-body" data-form="invite">
    <label class="dialog-field" for="invitation-email">Электронная почта</label>
    <input class="field" id="invitation-email" name="email" type="email" placeholder="name@example.com" maxlength="254" required autofocus aria-describedby="invite-help invite-error">
    <p class="org-dialog-copy"><strong>Организация</strong> · ${html(organization().name)}</p>
    <p class="org-dialog-copy"><strong>Роль</strong> · Участник</p>
    <p class="small muted" id="invite-help">Получатель подтвердит адрес и вступит в организацию.</p>
    <p class="small name-error" id="invite-error" role="alert" hidden></p>
    ${dialogActions('Отправить приглашение')}
  </form>`, trigger);
}

function openInvitationAction(trigger) {
  const action = trigger.hasAttribute('data-accept') ? 'accept' : 'revoke';
  const invitation = organization().invitations.find(item => item.id === Number(trigger.dataset[action]));
  if (invitation?.status !== 'Ожидает') return;
  const accepting = action === 'accept';
  const title = accepting ? 'Вступить в организацию' : 'Отозвать приглашение';
  showDialog(title, `<form class="dialog-body" data-form="${action}" data-id="${invitation.id}">
    <p><strong>${html(organization().name)}</strong></p>
    <p class="org-accept-address">${html(invitation.email)}</p>
    <p class="small muted">${accepting ? 'В макете почта получателя уже подтверждена.' : 'После отзыва по этой ссылке нельзя вступить в организацию.'}</p>
    ${dialogActions(accepting ? 'Принять приглашение' : 'Отозвать')}
  </form>`, trigger);
}

dialog.addEventListener('click', event => {
  if (event.target.closest('[data-close]')) dialog.close();
});
dialog.addEventListener('close', () => {
  if (dialogTrigger?.isConnected) dialogTrigger.focus();
});
dialog.addEventListener('submit', event => {
  event.preventDefault();
  if (!canManage()) return;
  const form = event.target;
  const team = organization();
  if (form.dataset.form === 'invite') {
    const email = form.elements.email.value.trim().toLowerCase();
    const existing = team.invitations.find(invitation => invitation.email === email && invitation.status !== 'Отозвано');
    if (existing) {
      const error = byId('invite-error');
      error.textContent = existing.status === 'Ожидает' ? 'Приглашение на этот адрес уже ожидает ответа.' : 'Этот человек уже вступил в организацию.';
      error.hidden = false;
      form.elements.email.focus();
      return;
    }
    team.invitations.unshift({id:nextInvitation++, email, status:'Ожидает'});
    dialog.close();
    navigate('invites', 'Приглашение добавлено в макете.');
    return;
  }
  const invitation = team.invitations.find(item => item.id === Number(form.dataset.id));
  if (invitation?.status !== 'Ожидает') return;
  dialog.close();
  if (form.dataset.form === 'accept') {
    invitation.status = 'Принято';
    team.members.push([invitation.email, invitation.email.slice(0, 2).toUpperCase(), 'Участник']);
    state.role = 'member';
    state.inviteeProfile = team.members.at(-1);
    scenario.value = state.role;
    navigate('home', 'Приглашение принято в макете.');
  } else {
    invitation.status = 'Отозвано';
    navigate('invites', 'Приглашение отозвано в макете.');
  }
});

function logoError(input, message) {
  const error = byId('logo-error');
  error.textContent = message;
  error.hidden = false;
  input.value = '';
  byId('choose-logo').focus();
}

async function previewLogo(input) {
  const file = input.files[0];
  if (!file) return;
  if (!['image/png', 'image/jpeg', 'image/webp'].includes(file.type) || file.size > 5 * 1024 * 1024) {
    logoError(input, 'Выберите изображение PNG, JPEG или WebP до 5 МБ.');
    return;
  }
  const team = organization();
  const url = URL.createObjectURL(file);
  const preview = new Image();
  preview.src = url;
  try {
    await preview.decode();
  } catch {
    URL.revokeObjectURL(url);
    if (input.isConnected && input.files[0] === file) logoError(input, 'Не удалось открыть изображение. Выберите другой файл.');
    return;
  }
  if (!input.isConnected || input.files[0] !== file || team !== organization()) {
    URL.revokeObjectURL(url);
    return;
  }
  if (team.logoDraft) URL.revokeObjectURL(team.logoDraft);
  team.logoDraft = url;
  state.notice = '';
  render();
  byId('apply-logo').focus();
}

function handleLogoAction(action) {
  if (!isPrivileged()) return;
  if (action === 'choose') {
    byId('logo-file').click();
    return;
  }
  const team = organization();
  if (action === 'apply' && team.logoDraft) {
    if (team.logo) URL.revokeObjectURL(team.logo);
    team.logo = team.logoDraft;
    team.logoDraft = null;
    state.notice = 'Логотип сохранён в макете.';
  } else if (action === 'cancel' && team.logoDraft) {
    URL.revokeObjectURL(team.logoDraft);
    team.logoDraft = null;
    state.notice = 'Выбор логотипа отменён.';
  } else if (action === 'reset' && team.logo) {
    URL.revokeObjectURL(team.logo);
    team.logo = null;
    state.notice = 'Восстановлена эмблема Севера.';
  } else return;
  byId('feedback').textContent = state.notice;
  render();
  byId('choose-logo').focus();
}

function submitSso(form, action) {
  const provider = form.elements.provider.value.trim();
  const working = form.elements.connection.value === 'working';
  const result = byId('sso-result');
  if (action === 'apply') {
    if (!working || form.dataset.checkedProvider !== provider) return;
    organization().sso = provider;
    navigate('members', 'Настройки SSO применены в макете.');
    return;
  }
  const checked = Boolean(provider) && working;
  form.dataset.checkedProvider = checked ? provider : '';
  form.querySelector('[value="apply"]').disabled = !checked;
  result.classList.toggle('name-error', !checked);
  result.classList.toggle('org-feedback', checked);
  result.textContent = !provider ? 'Введите название провайдера.' : checked ? 'Проверка в макете прошла. Настройки можно применить.' : 'Провайдер недоступен. Проверьте подключение и повторите попытку.';
  result.hidden = false;
}

main.addEventListener('click', event => {
  const action = event.target.closest('button');
  if (!action || !canManage()) return;
  if (action.hasAttribute('data-invite')) openInvitation(action);
  if (action.matches('[data-accept], [data-revoke]')) openInvitationAction(action);
  if (action.hasAttribute('data-logo')) handleLogoAction(action.dataset.logo);
});
main.addEventListener('submit', event => {
  if (!isPrivileged()) return;
  if (event.target.id === 'sso-form') {
    event.preventDefault();
    submitSso(event.target, event.submitter?.value);
    return;
  }
  if (event.target.id !== 'name-form') return;
  event.preventDefault();
  const field = event.target.elements.name;
  const name = field.value.trim();
  if (!name || Array.from(name).length > 120 || /\p{Cc}/u.test(name)) {
    const error = byId('organization-name-error');
    error.textContent = 'Введите от 1 до 120 символов без управляющих знаков.';
    error.hidden = false;
    field.focus();
    return;
  }
  organization().name = name;
  state.notice = 'Название сохранено в макете.';
  byId('feedback').textContent = state.notice;
  render();
  byId('organization-name').focus();
});
main.addEventListener('input', event => {
  const form = event.target.form;
  if (form?.id !== 'sso-form') return;
  delete form.dataset.checkedProvider;
  form.querySelector('[value="apply"]').disabled = true;
  byId('sso-result').hidden = true;
});
main.addEventListener('change', event => {
  if (event.target.id === 'logo-file' && isPrivileged()) void previewLogo(event.target);
  if (event.target.id !== 'guests' || !isPrivileged()) return;
  organization().guests = event.target.checked;
  byId('feedback').textContent = event.target.checked ? 'Гостевой доступ разрешён в макете.' : 'Гостевой доступ выключен в макете.';
});

byId('organizations').addEventListener('click', event => {
  const id = event.target.closest('[data-organization]')?.dataset.organization;
  if (!Object.hasOwn(organizations, id)) return;
  state.organization = id;
  pickers.forEach(picker => { picker.open = false; });
  navigate('home');
});
document.addEventListener('click', event => {
  for (const picker of pickers) if (picker.open && !picker.contains(event.target)) picker.open = false;
  const link = event.target.closest('a[href^="#"]');
  if (link?.classList.contains('skip-link')) {
    event.preventDefault();
    main.focus();
  } else if (link) state.notice = '';
});
document.addEventListener('keydown', event => {
  if (event.key !== 'Escape' || dialog.open) return;
  const picker = pickers.find(item => item.open);
  if (picker) {
    picker.open = false;
    picker.querySelector('summary').focus();
  }
});
scenario.addEventListener('change', () => {
  state.role = scenario.value;
  state.notice = '';
  state.inviteeProfile = null;
  if (!canManage()) navigate('home');
  else render();
});
byId('theme').addEventListener('click', () => {
  applyTheme(document.body.dataset.theme === 'light' ? 'dark' : 'light');
  render();
});
window.addEventListener('hashchange', () => render({focus:true}));
scenario.value = state.role;
applyTheme(params.get('theme') === 'light' ? 'light' : 'dark');
renderIcons();
render();
