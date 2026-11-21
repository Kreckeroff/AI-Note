import {catalogFixtures, catalogOwners} from './catalog-fixtures.mjs';

export const roles = {read:'Читатель', comment:'Комментатор', edit:'Редактор', none:'Нет доступа'};
export const markdownLimit = 30000;
export const taskStatuses = {open:'Запланировано', work:'В работе', review:'На согласовании', done:'Готово'};
export const demoOrganizationId = 'sever-org';
export const demoOrganizationName = 'Север';
export const personalTenantName = 'Моя среда';
export const demoPersonalSpace = {id:'demo-personal',name:'Личные заметки',documentId:'demo-personal-notes'};
export const tenantDemoScope = '.tenant-demo';
export const workspaceNames = {
  north: {name:'Документация команды', description:'Рабочее пространство', glyph:'folder', owner:'anna'},
  lab: {name:'Лаборатория', description:'Исследования и черновики', glyph:'spark', owner:'anna'},
};
export const people = {
  anna: {name:'Анна Миронова', initials:'АМ', email:'anna@sever.example', tone:'agent'},
  ira: {name:'Ирина Соколова', initials:'ИС', email:'ira@sever.example', tone:'knowledge'},
  analyst: {name:'Аналитик A-02', initials:'A2', email:'agent@sever.example', tone:'document'},
};
export const coverStyles = {
  orbit:{name:'Орбита',description:'Лиловая сфера и тонкие орбиты',icon:'north'},
  aurora:{name:'Сияние',description:'Бирюзовые световые нити',icon:'spark'},
  grid:{name:'Картография',description:'Янтарное поле и линии рельефа',icon:'layers'},
  none:{name:'Без обложки',description:'Страница начинается с заголовка',icon:'doc'},
};
const folder = (id, parent, title, intro, space = 'north') => ({id,parent,title,intro,space,kind:'folder'});
const page = (id, parent, title, intro, body, revision = 1, space = 'north') => ({
  id, parent, title, intro, body, revision, space, kind:'page', author:'anna', graph: null,
});
const seed = {
  account:'anna', theme:'dark', space:'north',
  settings: {sso:true, invites:true, context:true},
  profiles: {},
  spaceDetails: {},
  memberships: {},
  coverImages: {},
  documents: [
    folder('onboarding', '', 'Онбординг', 'Материалы, с которыми команда начинает работу.'),
    folder('permissions', 'onboarding', 'Доступ и роли', 'Политика доступа и материалы для приглашения участников.'),
    {...page('access','permissions','Доступ к пространству','Как пригласить коллегу в «Север», выбрать роль и проверить доступ к первому разделу.',[
      '## Вход по приглашению',
      'Владелец выбирает раздел и роль до отправки ссылки. После входа участник увидит материалы в пределах выданного доступа.',
      '> Сначала выбери самый узкий раздел, которого достаточно для работы.',
      '## Роли участников',
      '- **Читатель** открывает опубликованные страницы и связанные источники.',
      '- **Комментатор** оставляет замечания на страницах.',
      '- **Редактор** меняет материалы в своём разделе.',
      '## Проверка доступа',
      '1. Открой настройки доступа к странице.',
      '2. Выбери участника и проверь его роль.',
      '3. Переключись на гостевой профиль и открой ссылку снова.',
    ].join('\n\n'),3),graph:'access'},
    {...page('scope','permissions','Граница раздела','Что входит в область приглашения и как её расширить.',[
      '## Область приглашения','Участник получает доступ к выбранному разделу и разрешённым вложенным материалам.',
      '## Повторная проверка','Используй поиск и прямую ссылку для проверки: закрытые страницы недоступны в обоих случаях.',
    ].join('\n\n'),2),graph:'scope'},
    {...page('template','permissions','Шаблон приглашения','Что указать в приглашении участнику.',[
      '## Перед отправкой','- Укажи получателя приглашения.\n- Выбери раздел и уровень доступа.\n- Добавь ссылку на первый документ.',
      '## Сообщение участнику',`Добро пожаловать в пространство «${workspaceNames.north.name}». Открой материалы раздела «Онбординг».`,
    ].join('\n\n')),graph:'template'},
    page('people','onboarding','Команда и роли','Участники команды и зоны ответственности.',
      '## Участники\n\nАнна согласует доступ. Ирина проверяет страницу с позиции приглашённого участника. Аналитик A-02 готовит предложение.'),
    folder('operations','','Процессы команды','Согласование и передача результатов.'),
    {...page('handoff','operations','Хэндофф аналитика','Предложение владельцу пространства перед согласованием.',
      '## Состав передачи\n\n- Редакция политики доступа.\n- Задача KB-014 и текущий исполнитель.\n\n## Ожидаемый результат\n\nВладелец подтверждает область и уровень доступа.',2),graph:'handoff'},
    page('workflow','operations','Приём участника','Работа аналитика, владельца и оркестратора.',
      '## Подготовка\n\nАналитик сравнивает доступные источники и выбирает предлагаемую роль.\n\n## Согласование\n\nВладелец подтверждает предложение и передаёт материал на доску.'),
    folder('knowledge-base','','База знаний','Проверенные правила и источники.'),
    {...page('knowledge','knowledge-base','Источники знаний','Как проследить знание до страницы и её редакции.',
      '## Подтверждение\n\nУ каждого знания есть редакция источника и автор решения.\n\n## Связи\n\nОткрой граф, чтобы увидеть связанные материалы и задачи.'),graph:'evidence'},
    page('welcome','','О пространстве','Открытые материалы и инструкции команды.',
      '## Как начать\n\nОткрой раздел «Онбординг» и проверь, какие материалы доступны.'),
    page('lab-notes','','Заметки лаборатории','Черновики личного пространства.',
      '## Черновики\n\nСоздай страницу для нового исследования.',1,'lab'),
  ],
  tasks: [
    {id:'KB-014', title:'Вход по приглашению', description:'Проверить, что первая страница открывается в пределах выданной роли, и записать итог в хэндофф.', sourceKey:'access', person:'anna', status:'review'},
    {id:'KB-015', title:'Собрать шаблон приглашения', description:'Подготовить текст со ссылкой на первый доступный раздел.', sourceKey:'template', person:'ira', status:'work'},
    {id:'KB-016', title:'Подготовить хэндофф', description:'Передать результаты проверки и редакцию политики.', sourceKey:'handoff', person:'analyst', status:'open'},
    {id:'KB-013', title:'Выбрать область доступа', description:'Определить раздел, достаточный для знакомства с проектом.', sourceKey:'scope', person:'anna', status:'done'},
    {id:'KB-012', title:'Собрать карту источников', description:'Связать проверенные документы с подтверждёнными фактами.', sourceKey:'knowledge', person:'analyst', status:'done'},
    {id:'KB-017', title:'Проверить отзыв доступа', description:'Убедиться, что после отзыва ссылка и поиск закрывают страницу.', sourceKey:'access', person:'ira', status:'open'},
  ],
  permissions: {access:{members:{ira:'read', analyst:'edit'}, link:false}, scope:{members:{analyst:'read'}, link:false}},
  comments: {access:[
    {person:'ira', text:'Проверила чтение раздела с гостевого профиля.', date:'18 ноября'},
  ]},
  revisions: {access:[
    {revision:2, title:'Доступ к пространству', body:'## Вход по приглашению\n\nВладелец определяет доступные материалы и выдаёт приглашение.\n\n## Проверка доступа\n\nОткрой страницу с учётной записи участника.'},
    {revision:1, title:'Доступ к пространству', body:'## Вход по приглашению\n\nПодготовить первое правило выдачи доступа участнику.'},
  ]},
  approach: {
    node:'instruction', source:'access', retrieval:'direct', version:1,
    instruction:'Проверь роль участника и область приглашения. Опирайся на доступные документы. Назови редакцию источника и передай краткое предложение владельцу пространства.',
  },
};

export const workspaceStorageKey = 'sever.workspace.prototype.v1';
const sessionKey = 'sever.workspace.session.v1';
const legacyCatalogKey = 'sever.space-catalog' + tenantDemoScope;
const catalogOwnerId = index => index === 0 ? 'anna' : `catalog-owner-${index}`;

function seedTenantDemo(data) {
  // North keeps its editor ID; the remaining catalogue rows retain their original IDs.
  for (const {id,name,owner} of catalogFixtures(workspaceNames.north.name).slice(1)) {
    data.spaceDetails[id] = {name, glyph:'folder', owner:catalogOwnerId(owner), tenantId:demoOrganizationId};
  }
  for (const [index,[name]] of catalogOwners.entries()) {
    if (index > 0) data.profiles[catalogOwnerId(index)] = {name};
  }
  data.memberships['space-2'] = {anna:'read'};
  data.memberships['space-7'] = {anna:'edit'};
  data.spaceDetails[demoPersonalSpace.id] = {name:demoPersonalSpace.name,description:'Личные документы и черновики',glyph:'folder',owner:'anna',tenantId:'anna'};
  data.documents.push(page(demoPersonalSpace.documentId,'','Мои заметки','Личные записи Анны Мироновой.',
    '## Черновики\n\nЗдесь можно хранить планы и заметки для себя.',1,demoPersonalSpace.id));
}

export function migrateLegacyDemoCatalogue() {
  try {
    const previous = sessionStorage.getItem(legacyCatalogKey);
    if (previous === null) return;
    const records = JSON.parse(previous);
    if (!Array.isArray(records)) return;
    const data = loadWorkspace(tenantDemoScope);
    let changed = false;
    for (const record of records) {
      const {id,name,environment} = record ?? {};
      if (typeof id !== 'string' || !/^created-[a-f\d]{8}-[a-f\d]{4}-[a-f\d]{4}-[a-f\d]{4}-[a-f\d]{12}$/.test(id)) continue;
      if (typeof name !== 'string' || !name.trim() || [...name].length > 100 || Object.hasOwn(data.spaceDetails,id)) continue;
      if (environment !== demoOrganizationName && environment !== personalTenantName) continue;
      data.spaceDetails[id] = {name,glyph:'folder',owner:'anna',tenantId:environment === personalTenantName ? 'anna' : demoOrganizationId};
      changed = true;
    }
    if (!changed || persistWorkspace(data,tenantDemoScope)) sessionStorage.removeItem(legacyCatalogKey);
  } catch { /* A restricted storage provider must not prevent the demo from opening. */ }
}

export function loadWorkspace(scope = '') {
  let data = structuredClone(seed);
  const demo = scope === tenantDemoScope;
  if (demo) seedTenantDemo(data);
  try {
    const saved = JSON.parse(localStorage.getItem(workspaceStorageKey + scope) || 'null');
    if (saved && Array.isArray(saved.documents) && Array.isArray(saved.tasks)) {
      if (demo) for (const key of ['spaceDetails','profiles','memberships']) saved[key] = {...data[key],...saved[key]};
      data = {...data,...saved};
    }
  } catch { /* Preview still works when local storage is restricted. */ }
  try {
    const session = JSON.parse(sessionStorage.getItem(sessionKey + scope) || 'null');
    if (session) data = {...data,...session};
  } catch { /* Keep the demo defaults when tab storage is unavailable. */ }
  if (demo) {
    data.account = 'anna';
    data.tenantDemo = true;
  }
  return data;
}
export function persistWorkspace(data,scope = '') {
  const {account,space,theme,tenantDemo,...workspace} = data;
  try { localStorage.setItem(workspaceStorageKey + scope, JSON.stringify(workspace)); return true; }
  catch { return false; }
}
export function persistSession(data,scope = '') {
  const {account,space,theme} = data;
  try { sessionStorage.setItem(sessionKey + scope,JSON.stringify({account,space,theme})); }
  catch { /* Keep the current tab's identity in memory. */ }
}

export function documentById(data, id) {
  return data.documents.find(item => item.id === id);
}
export function taskById(data, id) {
  return data.tasks.find(task => task.id === id);
}
export function taskSpace(data,task) {
  return task.space || documentById(data,task.sourceKey)?.space || 'north';
}
export function canSeeTask(data,task) {
  const space = taskSpace(data,task);
  if (space !== data.space) return false;
  const source = documentById(data,task.sourceKey);
  const role = memberRole(data,space);
  if (!task.sourceKey) return ownsSpace(data,space) || Boolean(role && role !== 'none');
  return source?.space === space && accessFor(data,source) !== 'none';
}
export function canManageTask(data,task) {
  return Boolean(task) && ownsSpace(data) && canSeeTask(data,task);
}
export function taskAssignees(data,task) {
  return accountIds(data).filter(account => canSeeTask({...data,account},task));
}
export function spaceInfo(data,id = data.space) {
  return {...workspaceNames[id],...data.spaceDetails[id]};
}
export function tenantIdForSpace(data,id = data.space) {
  const {tenantId,owner} = spaceInfo(data,id);
  if (tenantId) return tenantId;
  // Spaces from older previews have no tenant ID. Demo profiles share the organization.
  return owner && !Object.hasOwn(people,owner) ? owner : demoOrganizationId;
}
export function ownsSpace(data,id = data.space) {
  return Boolean(data.account) && spaceInfo(data,id).owner === data.account;
}
export function memberRole(data,id,account = data.account) {
  return data.memberships[id]?.[account];
}
export function spaceAccess(data,id) {
  if (!data.account || !spaceInfo(data,id).name) return null;
  if (ownsSpace(data,id)) return {role:'edit',source:'owner'};
  const assigned = memberRole(data,id);
  if (assigned && assigned !== 'none') return {role:assigned,source:'membership'};
  if (data.tenantDemo && data.account === 'anna' && tenantIdForSpace(data,id) === demoOrganizationId) {
    return {role:'read',source:'administrator'};
  }
  return null;
}
export function availableSpaces(data) {
  if (!data.account) return [];
  return [...new Set([...Object.keys(workspaceNames),...Object.keys(data.spaceDetails)])]
    .filter(id => spaceAccess(data,id) ||
      data.documents.some(doc => doc.space === id && accessFor(data,doc) !== 'none'));
}
export function availableTenantSpaces(data,tenantId = tenantIdForSpace(data)) {
  return availableSpaces(data).filter(id => tenantIdForSpace(data,id) === tenantId);
}
export function availableTenants(data) {
  return [...new Set(availableSpaces(data).map(id => tenantIdForSpace(data,id)))];
}
export function tenantNameFor(data,tenantId = tenantIdForSpace(data)) {
  if (tenantId === demoOrganizationId) return demoOrganizationName;
  if (tenantId === data.account) return personalTenantName;
  const owner = profileFor(data,tenantId)?.name;
  return owner ? `Личная среда · ${owner}` : 'Личная среда';
}
export function invitesEnabled(data,id = data.space) {
  return data.spaceDetails[id]?.invites ?? data.settings.invites;
}
export function profileFor(data,id) {
  const base = people[id], custom = data.profiles[id];
  if (!base && !custom) return undefined;
  const name = custom?.name || base.name;
  const initials = name.split(/\s+/).slice(0,2).map(part => Array.from(part)[0]).join('').toLocaleUpperCase('ru');
  const providers = custom?.providers || (id === 'anna' && data.settings.sso ? {sso:true} : {});
  return {...base,tone:base?.tone || 'agent',...custom,name,initials:custom?.name ? initials : base.initials,providers};
}
export function accountIds(data) {
  return [...new Set([...Object.keys(people),...Object.keys(data.profiles)])];
}
export function accountForEmail(data,email) {
  return accountIds(data).find(id => profileFor(data,id).email?.toLowerCase() === email.toLowerCase());
}
export function demoSpaceNameError(data,name,tenantId) {
  if (!name) return 'Введите название пространства.';
  if ([...name].length > 100) return 'Введите не больше 100 символов.';
  const normalized = value => value.normalize('NFC').toLocaleLowerCase('ru');
  if (availableTenantSpaces(data,tenantId).some(id => ownsSpace(data,id) && normalized(spaceInfo(data,id).name) === normalized(name))) {
    return 'У вас уже есть пространство с таким названием.';
  }
  return '';
}
export function createSpace(data,name,description = '',tenantId = tenantIdForSpace(data),{starterPage = true} = {}) {
  const id = crypto.randomUUID();
  data.spaceDetails[id] = {name,description,glyph:'folder',owner:data.account,tenantId};
  if (!starterPage) return {id};
  const home = {...page(crypto.randomUUID(),'','Добро пожаловать','', '## Начните со страницы\n\nДобавьте документ или пригласите участника через настройки пространства.',1,id),author:data.account};
  data.documents.push(home);
  return {id,home};
}
export function signUp(data,name,email,provider = 'local') {
  if (accountForEmail(data,email)) return undefined;
  const account = crypto.randomUUID();
  data.profiles[account] = {name,email,providers:provider === 'sso' ? {sso:true} : {}};
  data.account = account;
  return createSpace(data,name,'Личное пространство',account);
}
export function isCoverImage(cover) {
  return /^image:[\w-]{1,64}$/.test(cover || '');
}
export function validCover(cover) {
  return Object.hasOwn(coverStyles,cover) || isCoverImage(cover);
}
export function coverFor(doc) {
  return validCover(doc.cover) ? doc.cover : doc.id === 'access' ? 'orbit' : 'none';
}
export function sharedPage(data,id) {
  const page = documentById(data,id);
  return page?.kind === 'page' && data.permissions[id]?.link ? page : undefined;
}
export function accessFor(data, doc) {
  if (!doc || !data.account) return 'none';
  if (ownsSpace(data,doc.space)) return 'edit';
  for (let current = doc; current; current = documentById(data,current.parent)) {
    const permission = data.permissions[current.id];
    if (permission?.members[data.account]) return permission.members[data.account];
  }
  return spaceAccess(data,doc.space)?.role || 'none';
}
export function canWrite(data, doc) {
  return accessFor(data, doc) === 'edit';
}
export function canComment(data, doc) {
  return ['edit','comment'].includes(accessFor(data, doc));
}
export function canList(data, doc) {
  return accessFor(data,doc) !== 'none' ||
    (doc.kind === 'folder' && data.documents.some(item => item.parent === doc.id && canList(data,item)));
}
export function pageTrail(data, doc) {
  const trail = [];
  for (let parent = documentById(data,doc.parent); parent; parent = documentById(data,parent.parent)) trail.unshift(parent);
  return trail;
}
export function savePage(data, doc, title, body, cover = coverFor(doc)) {
  const previousCover = coverFor(doc);
  if (body.length > markdownLimit || !validCover(cover) || (title === doc.title && body === doc.body && cover === previousCover)) return false;
  data.revisions[doc.id] ??= [];
  data.revisions[doc.id].unshift({revision:doc.revision,title:doc.title,body:doc.body,cover:previousCover});
  Object.assign(doc, {title,body,cover,revision:doc.revision + 1});
  return true;
}
export function newItem(data, type, title, parent) {
  const id = crypto.randomUUID();
  const doc = type === 'folder'
    ? folder(id,parent,title,'',data.space)
    : {...page(id,parent,title,'','',1,data.space),author:data.account};
  data.documents.push(doc);
  return doc;
}
export function addTask(data, title, {status='open', sourceKey='', person=data.account, description=''} = {}) {
  const serial = Math.max(0,...data.tasks.map(task => Number(task.id.split('-').at(-1)))) + 1;
  const task = {id:'KB-'+String(serial).padStart(3,'0'),title,status,person,sourceKey,description,space:data.space};
  data.tasks.push(task);
  return task;
}
