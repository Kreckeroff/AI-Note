import {accessFor, accountIds, availableTenants, availableTenantSpaces, coverFor, coverStyles, documentById, invitesEnabled, isCoverImage, ownsSpace, profileFor, roles,
  spaceInfo, taskAssignees, taskStatuses, tenantIdForSpace, tenantNameFor} from './model.mjs?v=tenant-catalog-2';
import {visibleTasks} from './board.mjs';
import {agentSnapshot} from './approach.mjs';
import {coverArt} from './documents.mjs?v=tenant-catalog-2';
import {coverImage, rasterAccept} from './media.mjs';
import {avatar, button, entity, escapeHtml as html, icon, selectOptions} from './ui.mjs';

const formName = (label,name,placeholder,options = '') => `<label class="dialog-field"><span>${label}</span><input class="field" name="${name}" placeholder="${placeholder}" maxlength="120" required ${options}></label>`;
const formSelect = (label,name,labels,selected) => `<label class="dialog-field"><span>${label}</span><select class="field" name="${name}">${selectOptions(labels,selected)}</select></label>`;
const adjust = (label,note,control) => `<label class="setting-line"><span><strong>${label}</strong><small class="muted">${note}</small></span>${control}</label>`;
const save = (label,glyph = 'check') => `<div class="dialog-buttons">${button('',label,glyph,{submit:true,className:'primary'})}</div>`;

export function searchResults(data,query) {
  const needle = query.trim().toLocaleLowerCase('ru');
  const pages = data.documents
    .filter(doc => doc.space === data.space && accessFor(data,doc) !== 'none' &&
      [doc.title,doc.intro,doc.body].some(value => value?.toLocaleLowerCase('ru').includes(needle)))
    .map(doc => entity('open-doc',doc.id,doc.kind === 'folder' ? 'folder' : 'doc',doc.title,doc.kind === 'folder' ? 'Раздел' : 'Страница'));
  const tasks = visibleTasks(data)
    .filter(task => [task.id,task.title].some(value => value.toLocaleLowerCase('ru').includes(needle)))
    .map(task => entity('task',task.id,'board',task.title,task.id+' · '+taskStatuses[task.status],'work'));
  return [...pages,...tasks].slice(0,12).join('') || '<p class="muted">Нет совпадений. Попробуйте другое название или номер задачи.</p>';
}

function share(data,doc) {
  const current = data.permissions[doc.id] || {members:{},link:false};
  const roleList = Object.fromEntries(Object.entries(roles).filter(([key]) => key !== 'none'));
  const ownerProfile = profileFor(data,spaceInfo(data,doc.space).owner);
  const owner = `<div class="person-row">${avatar(ownerProfile)}<span class="person-name"><strong>${html(ownerProfile.name)}</strong><small class="muted">${html(ownerProfile.email)}</small></span><span class="tag">Владелец</span></div>`;
  const members = Object.entries(current.members).map(([key,role]) => {
    const person = profileFor(data,key) || {name:key,email:key,initials:key.slice(0,2).toUpperCase()};
    return `<div class="person-row">${avatar(person)}<span class="person-name"><strong>${html(person.name)}</strong><small class="muted">${html(person.email)}</small></span>
    <select class="field" data-control="member-role" data-value="${html(key)}" aria-label="Роль для ${html(person.name)}">${selectOptions(roleList,role)}</select>
    ${button('remove-member','','close',{className:'ghost',title:'Отозвать доступ для '+person.name,value:key})}</div>`;
  }).join('');
  const inherited = accountIds(data).filter(key => key !== data.account && !Object.hasOwn(current.members,key)).map(key => {
    const role = accessFor({...data,account:key},doc), person = profileFor(data,key);
    if (role === 'none') return '';
    return `<div class="person-row" data-inherited-member="${html(key)}">${avatar(person)}<span class="person-name"><strong>${html(person.name)}</strong>
      <small class="muted">Унаследованный доступ</small></span><span class="tag">${roles[role]}</span></div>`;
  }).join('');
  const invite = invitesEnabled(data,doc.space) ? `<form class="permission-form" data-form="invite" style="margin:13px 0 0">
    <label class="dialog-field"><span>Кому выдать доступ</span><input class="field" name="email" type="email" placeholder="name@example.com" required maxlength="120"></label>
    ${formSelect('Роль','role',roleList,'read')}${button('','Пригласить','plus',{className:'primary',submit:true})}
    <p class="form-status" data-share-error role="status" hidden></p>
  </form>` : '<p class="office-note">Приглашения отключены в настройках.</p>';
  return `<p class="muted">Настройте доступ к «${html(doc.title)}».</p>
    ${owner}${members}${inherited}${invite}
    <div class="link-settings">${doc.kind === 'page' ? `<label class="dialog-field"><span>Доступ по ссылке</span><select class="field" data-control="share-link">
      ${selectOptions({private:'По приглашению', public:'Чтение для всех по ссылке'},current.link ? 'public' : 'private')}
    </select></label>` : '<span class="small muted">Доступ к разделу по приглашению</span>'}${button('copy-page-link','Скопировать ссылку','link')}</div>
    <label class="dialog-field" data-copy-fallback hidden><span>Ссылка на страницу</span><input class="field" data-copy-value readonly></label>`;
}

function settings(data) {
  const owner = ownsSpace(data);
  const space = spaceInfo(data);
  const disabled = owner ? '' : ' disabled';
  const checkbox = (key,checked) => `<input type="checkbox" data-control="settings" data-value="${key}"${checked ? ' checked' : ''}${disabled}>`;
  return `<div class="settings-banner"><span class="brand-mark">${icon(space.glyph,18)}</span><div><strong>${html(space.name)}</strong><p>${html(space.description)}</p></div></div>
    <section class="settings-section"><h3>Общие</h3>
      ${owner ? `<form data-form="space-details"><label class="dialog-field"><span>Название пространства</span><input class="field" name="name" value="${html(space.name)}" required maxlength="42"></label>
      <label class="dialog-field"><span>Описание пространства</span><textarea class="field" name="description" rows="2" maxlength="150">${html(space.description)}</textarea></label>${save('Сохранить пространство')}</form>` :
      '<p class="small muted">Название и описание меняет владелец.</p>'}
    </section><section class="settings-section"><h3>Участники и доступ</h3>
      ${adjust('Приглашения','Разрешить выдачу доступа из окна шеринга в этом пространстве.',checkbox('invites',invitesEnabled(data)))}
      ${owner ? spaceMembers(data) : ''}
    </section><section class="settings-section"><h3>Интерфейс</h3>
      ${themeSetting(data)}
      ${adjust('Контекст страницы','Показывать правую панель на широком экране.',`<input type="checkbox" data-control="settings" data-value="context"${data.settings.context ? ' checked' : ''}>`)}
    </section>`;
}

function spaceMembers(data) {
  const memberRoles = {read:'Читатель',comment:'Комментатор',edit:'Редактор'};
  const current = data.memberships[data.space] || {};
  const members = Object.entries(current).map(([id,role]) => {
    const person = profileFor(data,id);
    return `<div class="person-row">${avatar(person,true)}<span class="person-name"><strong>${html(person.name)}</strong><small class="muted">${html(person.email)}</small></span>
      <select class="field" data-control="space-member-role" data-value="${html(id)}" aria-label="Роль участника ${html(person.name)}">${selectOptions(memberRoles,role)}</select>
      ${button('remove-space-member','','close',{className:'ghost',value:id,title:'Отозвать доступ к пространству'})}</div>`;
  }).join('');
  const candidates = Object.fromEntries(accountIds(data).filter(id => id !== data.account && !Object.hasOwn(current,id)).map(id => [id,profileFor(data,id).name]));
  const invite = invitesEnabled(data) && Object.keys(candidates).length ? `<form class="permission-form space-invite" data-form="space-invite">
    ${formSelect('Локальный профиль','member',candidates)}${formSelect('Роль','role',memberRoles,'read')}${button('','Пригласить','plus',{submit:true,className:'primary'})}
  </form>` : '';
  return `${members}${invite}`;
}

function themeSetting(data) {
  return adjust('Тема','Применяется к документам, графу и задачам.',`<select class="field" data-control="theme">${selectOptions({dark:'Тёмная',light:'Светлая'},data.theme)}</select>`);
}

function account(data,fixedProfile) {
  const person = profileFor(data,data.account);
  const banner = person ? `<div class="settings-banner">${avatar(person)}<div><strong>${html(person.name)}</strong><p>${html(person.email)}</p></div></div>` : '';
  if (fixedProfile && person) return banner + `<section class="settings-section"><h3>Интерфейс</h3>${themeSetting(data)}</section>`;
  const profiles = accountIds(data).filter(key => key !== 'analyst' && key !== data.account).map(key => {
    const person = profileFor(data,key);
    return (
      entity('choose-account',key,'user',person.name,person.email,person.tone)
    );
  }).join('');
  const signIn = !person ? accountIds(data).filter(id => id !== 'analyst').map(id => {
    const profile = profileFor(data,id);
    return [
      profile.providers.sso && entity('choose-account',id,'key','Войти через SSO',profile.name,'document'),
      profile.providers.google && entity('choose-account',id,'key','Войти через Google',profile.name,'document'),
    ].filter(Boolean).join('');
  }).join('') : '';
  return `${person ? `${banner}
    <section class="settings-section"><h3>Профиль</h3><form data-form="profile-details">
      <label class="dialog-field"><span>Отображаемое имя</span><input required maxlength="48" name="name" class="field" value="${html(person.name)}"></label>
      <label class="dialog-field"><span>Почта</span><input type="email" required maxlength="120" name="email" class="field" value="${html(person.email)}"></label>
      <p class="form-status" data-profile-error hidden></p>
      ${save('Сохранить профиль')}
    </form></section>
    <section class="settings-section"><h3>Вход и безопасность</h3><p class="small muted">Пароль и подключения проверяются локально в демо.</p>
      <form data-form="password">
        <label class="dialog-field"><span>Новый пароль</span><input class="field" name="password" type="password" minlength="8" maxlength="120" required autocomplete="new-password"></label>
        <label class="dialog-field"><span>Повторите пароль</span><input class="field" name="confirmation" type="password" minlength="8" maxlength="120" required autocomplete="new-password"></label>
        <p class="form-status" data-password-error hidden></p>${save('Проверить смену пароля','key')}
      </form>
      ${adjust('SSO организации','Имитация входа через организацию.',`<input type="checkbox" data-control="provider" data-value="sso"${person.providers.sso ? ' checked' : ''}>`)}
      ${adjust('Google','Локальная привязка демонстрационного профиля.',`<input type="checkbox" data-control="provider" data-value="google"${person.providers.google ? ' checked' : ''}>`)}
    </section>
    <section class="settings-section"><h3>Интерфейс</h3>${themeSetting(data)}</section>
    <section class="settings-section"><h3>Переключить профиль</h3><div class="search-results">${profiles}</div></section>
    <div class="dialog-buttons">${button('sign-out','Выйти','log',{className:'ghost'})}</div>` :
    `<p class="muted">Выберите профиль для этой вкладки. Доступ к страницам определяется ролью.</p><div class="search-results">${profiles}${signIn}</div>
    <div class="dialog-buttons">${button('signup','Создать профиль','plus',{className:'primary'})}</div>`}`;
}

function createMenu(data,doc) {
  const parent = doc?.kind === 'folder' ? doc : documentById(data,doc?.parent);
  return `<div class="chooser-location">${icon('folder',14)}<span>${html(spaceInfo(data).name)}${parent ? ' / '+html(parent.title) : ''}</span></div>
    <div class="create-choices">${[
      ['page','doc','Страница','Документ с Markdown, комментариями и доступом.','document'],
      ['folder','folder','Раздел','Место для страниц и вложенных разделов.','knowledge'],
      ['task','board','Задача','Карточка с исполнителем и этапом на доске.','work'],
    ].filter(([key]) => key !== 'task' || ownsSpace(data)).map(([key,glyph,title,description,tone]) => `<button class="create-choice" type="button" data-action="choose-create" data-value="${key}" data-tone="${tone}">
      <span class="entity-icon">${icon(glyph,20)}</span><strong>${title}</strong><span>${description}</span>${icon('arrow',14)}
    </button>`).join('')}</div>`;
}

function coverMenu(current) {
  return `<label class="cover-upload"><span class="entity-icon">${icon('upload',17)}</span><span><strong>Загрузить изображение</strong><small>PNG, JPEG или WebP до 5 МБ</small></span>
    <input type="file" data-cover-upload accept="${rasterAccept}" aria-label="Загрузить свою обложку"></label>
    <p class="form-status" data-cover-error role="status" hidden></p>
    <div class="cover-gallery">${Object.entries(coverStyles).map(([key,info]) => `<button class="cover-choice" type="button" data-action="choose-cover" data-value="${key}" aria-pressed="${key === current}">
    <span class="cover-thumb ${key === 'none' ? 'cover-thumb-empty' : 'cover'}" data-cover="${key}">${key === 'none' ? icon(info.icon,24) : coverArt(key)}</span>
    <span class="cover-choice-copy"><strong>${info.name}</strong><small>${info.description}</small></span>
    <span class="cover-choice-mark">${icon('check',15)}</span>
  </button>`).join('')}</div>`;
}

export function spaceSwitcherItems(data,query = '') {
  const phrase = query.trim().toLocaleLowerCase('ru');
  const owner = space => profileFor(data,space.owner)?.name || '';
  let ids = availableTenantSpaces(data);
  if (data.tenantDemo) {
    ids = ids.filter(id => spaceInfo(data,id).name.toLocaleLowerCase('ru').includes(phrase)).sort((a,b) => {
      const left = spaceInfo(data,a), right = spaceInfo(data,b);
      return left.name.localeCompare(right.name,'ru') || owner(left).localeCompare(owner(right),'ru');
    });
    if (phrase && !ids.length) return '<p class="small muted" role="status">Совпадений нет.</p>';
  }
  const owned = ids.filter(id => ownsSpace(data,id));
  const joined = ids.filter(id => !ownsSpace(data,id));
  const group = (ids,title) => {
    const visible = data.tenantDemo && !phrase ? ids.filter((id,index) => index < 4 || id === data.space) : ids;
    const rows = visible.map(id => {
      const space = spaceInfo(data,id);
      const detail = data.tenantDemo ? owner(space) : space.description;
      const description = id === data.space ? ['Текущее пространство',data.tenantDemo && detail].filter(Boolean).join(' · ') : detail;
      return entity('choose-space',id,space.glyph,space.name,description);
    }).join('');
    return `<section class="switcher-group"><h3>${title}</h3>${rows || '<p class="small muted">Пока нет пространств.</p>'}</section>`;
  };
  return group(owned,'Мои пространства')+group(joined,'Доступные пространства');
}

function tenants(data) {
  const current = tenantIdForSpace(data);
  return availableTenants(data).map(id => entity('choose-tenant',id,'north',tenantNameFor(data,id),id === current ? 'Текущая среда' : '')).join('');
}

function createItem(data,doc,modal) {
  const folders = data.documents.filter(item => item.kind === 'folder' && item.space === data.space && accessFor(data,item) === 'edit');
  const preferred = modal.value || (doc?.kind === 'folder' ? doc.id : doc?.parent);
  const selected = folders.some(folder => folder.id === preferred) ? preferred : '';
  return `<form data-form="new-item">${modal.kind ? `<input type="hidden" name="kind" value="${modal.kind}">` : formSelect('Что создать','kind',{page:'Страница',folder:'Раздел'},'page')}
    ${formName('Название','title',modal.kind === 'folder' ? 'Например, «Документация»' : 'Например, «Решение по доступу»','autofocus')}${formSelect('Расположение','parent',{'':spaceInfo(data).name,...Object.fromEntries(folders.map(item => [item.id,item.title]))},selected)}
    ${creationActions(modal,'Создать')}</form>`;
}

function createTask(data,doc,modal) {
  const pages = data.documents.filter(item => item.kind === 'page' && item.space === data.space && accessFor(data,item) !== 'none');
  const selected = doc?.kind === 'page' ? doc.id : '';
  return `<form data-form="new-task">${formName('Название','title','Например, «Согласовать доступ»','autofocus')}
    ${formSelect('Этап','status',taskStatuses,modal.value || 'open')}
    ${formSelect('Источник','sourceKey',{'':'Без источника',...Object.fromEntries(pages.map(item=>[item.id,item.title]))},selected)}
    ${creationActions(modal,'Создать задачу')}</form>`;
}

function creationActions(modal,label) {
  return `<div class="dialog-buttons">${modal.fromChooser ? button('create','К выбору','chevron',{className:'ghost'}) : ''}${button('',label,'plus',{submit:true,className:'primary'})}</div>`;
}

function taskDetail(data,task) {
  const doc = documentById(data,task.sourceKey);
  if (!visibleTasks(data).includes(task)) return '<p class="muted">Задача недоступна.</p>';
  const personList = Object.fromEntries(taskAssignees(data,task).map(key=>[key,profileFor(data,key).name]));
  if (task.person && !Object.hasOwn(personList,task.person)) {
    personList[task.person] = (profileFor(data,task.person)?.name || task.person)+' · текущий исполнитель';
  }
  return `<div class="view-actions"><span class="tag" data-tone="work">${task.id}</span><span class="tag">${taskStatuses[task.status]}</span></div>
    ${task.description ? `<p class="muted" style="margin:11px 0">${html(task.description)}</p>` : ''}
    ${ownsSpace(data) ? `<form data-form="task">
      ${formSelect('Этап','status',taskStatuses,task.status)}${formSelect('Исполнитель','person',personList,task.person)}${save('Применить')}</form>` : ''}
    ${doc ? `<hr class="rule">${entity('open-doc',doc.id,'doc',doc.title,'Открыть источник')}` : ''}
    ${task.id === 'KB-014' ? `<hr class="rule">${entity('open-flow','','flow','Проверка приглашения','Открыть AI-подход','agent')}` : ''}`;
}

export function dialogContent(data,doc,modal) {
  switch (modal.type) {
    case 'search': return {title:'Поиск в пространстве', body:`<div class="search-field-wrap">${icon('search')}<input class="field" data-search type="search" placeholder="Страница, раздел или задача…" aria-label="Поиск" autofocus></div><div class="search-results" data-results>${searchResults(data,modal.query || '')}</div>`};
    case 'share': return {title:'Управление доступом',body:share(data,doc)};
    case 'settings': return {title:'Настройки пространства',body:settings(data)};
    case 'account': return {title:data.account ? 'Учётная запись' : 'Вход в «Север»',body:account(data,modal.fixedProfile)};
    case 'tenants': return {title:'Выбрать среду',body:tenants(data)};
    case 'spaces': return {title:'Рабочие пространства',body:data.tenantDemo
      ? `<div class="search-field-wrap">${icon('search')}<input class="field" type="search" data-space-search placeholder="Найти пространство" aria-label="Поиск по названию пространства" value="${html(modal.query || '')}" autofocus></div><div class="search-results" data-space-results>${spaceSwitcherItems(data,modal.query)}</div>`
      : spaceSwitcherItems(data)};
    case 'signup': return {title:'Создать профиль',body:`<p class="small muted">Создайте локальный профиль и личное пространство.</p>
      <form data-form="signup">${formName('Имя и фамилия','name','Алексей Иванов','autofocus')}
        <label class="dialog-field"><span>Почта</span><input class="field" type="email" required maxlength="120" name="email" placeholder="name@example.com"></label>
        ${formSelect('Способ входа','provider',{local:'Локальный профиль',sso:'SSO организации · демо'},'local')}
        <p class="form-status" data-signup-error hidden></p>${save('Создать профиль','plus')}
      </form>`};
    case 'new-space': return {title:'Новое пространство',body:`<form data-form="new-space">${formName('Название','name','Например, «Команда продукта»','autofocus')}
      ${data.tenantDemo ? '<p class="form-status" data-space-name-error role="alert" hidden></p>' : ''}
      <label class="dialog-field"><span>Описание</span><textarea class="field" name="description" rows="2" maxlength="150"></textarea></label>
      ${save('Создать пространство','plus')}</form>`};
    case 'video': return {title:'Вставить видео',body:`<p class="small muted">Поддерживаются YouTube, Vimeo и прямые HTTPS-ссылки на MP4 или WebM.</p>
      <form data-form="video"><label class="dialog-field"><span>Ссылка на видео</span><input class="field" name="url" type="url" required maxlength="2000" autofocus placeholder="https://…"></label>
      <p class="form-status" data-video-error hidden></p>${save('Вставить видео','play')}</form>`};
    case 'recent': return {title:'Недавние страницы',body:modal.pages.map(page => entity('open-doc',page.id,page.kind === 'folder' ? 'folder' : 'doc',page.title,spaceInfo(data,page.space).name)).join('') || '<p class="muted">История пока пуста.</p>'};
    case 'covers': return {title:'Обложка страницы',wide:true,body:coverMenu(modal.value)};
    case 'create-menu': return {title:'Создать',wide:true,body:createMenu(data,doc)};
    case 'create': return {title:modal.kind === 'page' ? 'Новая страница' : modal.kind === 'folder' ? 'Новый раздел' : 'Создать материал',body:createItem(data,doc,modal)};
    case 'new-task': return {title:'Новая задача',body:createTask(data,doc,modal)};
    case 'task': return {title:modal.task.title,body:taskDetail(data,modal.task)};
    case 'agent': return {title:'Контекст глазами агента',wide:true,body:`<pre class="agent-preview">${html(agentSnapshot(data))}</pre>`};
    case 'revision': {
      const revision = data.revisions[doc.id]?.find(item => item.revision === Number(modal.value));
      if (!revision) return {title:'Редакция недоступна',body:'<p class="muted">Редакция не сохранилась в прототипе.</p>'};
      const style = coverFor({id:doc.id,cover:revision.cover});
      const image = style === 'none' ? '' : `<div class="cover editor-cover" data-cover="${isCoverImage(style) ? 'custom' : style}">${coverArt(style,coverImage(data.coverImages,style))}</div>`;
      return {title:'Редакция '+revision.revision,wide:true,body:`${image}<h3 class="small">${html(revision.title)}</h3><hr class="rule"><pre class="agent-preview">${html(revision.body)}</pre>`};
    }
    default: return {title:'Север',body:''};
  }
}
