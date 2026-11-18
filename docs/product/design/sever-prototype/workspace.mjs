import {accessFor, accountForEmail, addTask, availableSpaces, canComment, canManageTask, canWrite, coverFor, coverStyles, createSpace, documentById, invitesEnabled,
  loadWorkspace, markdownLimit, memberRole, newItem, ownsSpace, pageTrail, persistSession, persistWorkspace, profileFor, roles, savePage,
  sharedPage, signUp, spaceInfo, taskAssignees, taskById, taskStatuses, workspaceStorageKey} from './model.mjs';
import {avatar, button, escapeHtml as html, icon} from './ui.mjs';
import {editorCover, markdown, renderDocuments, renderSharedPage} from './documents.mjs';
import {applyFormatting, connectEditor, editorMarkdown, toggleSource, validateEditorLength} from './editor.mjs';
import {renderBoard, visibleTasks} from './board.mjs';
import {dialogContent, searchResults} from './dialogs.mjs';
import {approachSource, getRunSequence, renderApproach, retrievalLabels} from './approach.mjs';
import {connectPresence, mergePageDraft, mergeWorkspaceChanges, withWorkspaceLock} from './collaboration.mjs';
import {coverImage, parseVideo, prepareCover} from './media.mjs';
import {playEmbeddedVideo} from './commands.mjs';

const dock = document.querySelector('#dock');
const host = document.querySelector('#workspace');
const panel = document.querySelector('#panel');
const notice = document.querySelector('#notice');
let data = loadWorkspace();
const entry = new URL(location.href);
if (entry.searchParams.has('collaborate')) {
  data.account = null;
  persistSession(data);
  entry.searchParams.delete('collaborate');
  history.replaceState(null,'',entry);
}
let baseline = structuredClone(data);
const compact = matchMedia('(max-width: 890px)');
const motion = matchMedia('(prefers-reduced-motion: reduce)');
const expanded = new Set(['onboarding','permissions','operations']);
const ui = {expanded,tree:false,context:data.settings.context && !compact.matches,contextTab:'about',editing:false,boardMode:'board',boardPerson:''};
const views = {doc:['Документы','docs'],graph:['Граф','graph'],approach:['AI-подход','flow'],board:['Задачи','board']};
const lastPages = new Map();
const visits = [];
const recent = new Map();
let visitCursor = -1;
let route = readRoute(), unmountGraph, modal, toastTimer, flowTimer;
let run = {running:false, steps:[], current:null, completed:false, error:''};
let draftBase, conflictBase, uploadedCover, savingPage = false;
const signalPresence = connectPresence(drawPresence);

function initialDoc() {
  const preferred = data.space === 'north' ? 'access' : data.space === 'lab' ? 'lab-notes' : '';
  if (accessFor(data,documentById(data,preferred)) !== 'none') return preferred;
  return data.documents.find(doc => doc.space === data.space && doc.kind === 'page' && accessFor(data,doc) !== 'none')?.id || preferred;
}
function requestedShare() { return new URL(location.href).searchParams.get('share'); }
function readRoute() {
  const [requested,value] = location.hash.slice(1).split('/');
  const view = Object.hasOwn(views,requested) ? requested : 'doc';
  let key;
  try { key = value ? decodeURIComponent(value) : ''; } catch { key = ''; }
  return {view,key:key || (view === 'doc' ? initialDoc() : view === 'graph' ? 'access' : '')};
}
function activeDoc() {
  const doc = documentById(data,route.view === 'doc' ? route.key : lastPages.get(data.space) || initialDoc());
  return doc?.space === data.space ? doc : undefined;
}
function href(view,key = '') { return '#'+view+(key ? '/'+encodeURIComponent(key) : ''); }
function show(view,key) {
  closePanel();
  const target = href(view,key);
  if (location.hash === target) routeChanged();
  else location.hash = target;
}
function recordVisit() {
  const last = visits[visitCursor];
  if (last?.view === route.view && last.key === route.key && last.space === data.space) return;
  visits.splice(visitCursor+1,visits.length,{...route,space:data.space});
  if (visits.length > 40) visits.shift();
  visitCursor = visits.length-1;
}
function moveWithinApp(offset) {
  const destination = visits[visitCursor+offset];
  if (!destination) return;
  visitCursor += offset;
  if (destination.space !== data.space && availableSpaces(data).includes(destination.space)) {
    data.space = destination.space;
    persistSession(data);
  }
  history.replaceState(null,'',href(destination.view,destination.key));
  routeChanged(false);
}
function recentPages() {
  return (recent.get(data.account) || []).map(id => documentById(data,id))
    .filter(doc => accessFor(data,doc) !== 'none').slice(0,8);
}
function rememberPage(doc) {
  if (doc.kind !== 'page' || accessFor(data,doc) === 'none' || requestedShare() !== null) return;
  const history = recent.get(data.account) || [];
  recent.set(data.account,[doc.id,...history.filter(id => id !== doc.id)].slice(0,40));
}
function routeChanged(record = true) {
  const next = readRoute();
  const changed = next.view !== route.view || next.key !== route.key;
  if (changed) {
    ui.editing = false;
    ui.contextTab = 'about';
    ui.tree = false;
    clearDraft();
  }
  closePanel();
  route = next;
  if (route.view === 'doc' && requestedShare() === null) {
    const target = documentById(data,route.key);
    if (target && target.space !== data.space && accessFor(data,target) !== 'none') {
      data.space = target.space;
      persistSession(data);
    }
  }
  const doc = activeDoc();
  if (route.view === 'doc' && doc) {
    lastPages.set(data.space,doc.id);
    for (const parent of pageTrail(data,doc)) expanded.add(parent.id);
    rememberPage(doc);
  }
  if (record && data.account && requestedShare() === null) recordVisit();
  render();
  const scroll = host.querySelector('.doc-scroll');
  if (changed && scroll) scroll.scrollTop = 0;
  if (route.view === 'board' && route.key) openTask(route.key);
}
function adopt(next) {
  data = next;
  baseline = structuredClone(next);
}
function save() {
  const merged = mergeWorkspaceChanges(baseline,data,loadWorkspace());
  if (persistWorkspace(merged)) adopt(merged);
  else notify('Хранилище прототипа недоступно. Изменения остались в этой вкладке.');
}
function notify(text) {
  clearTimeout(toastTimer);
  notice.textContent = text;
  notice.hidden = false;
  toastTimer = setTimeout(() => { notice.hidden = true; },2700);
}
function drawPresence(peers) {
  const target = host.querySelector('[data-presence]');
  if (!target) return;
  const doc = activeDoc();
  target.innerHTML = peers.filter(peer => accessFor({...data,account:peer.account},doc) !== 'none').map(peer => {
    const person = profileFor(data,peer.account);
    if (!person) return '';
    const verb = peer.mode === 'edit' ? 'редактирует' : 'читает';
    const label = html(person.name+' '+verb);
    return `<span class="presence-badge" data-presence-mode="${peer.mode === 'edit' ? 'edit' : 'read'}" data-presence-account="${html(peer.account)}" role="img" aria-label="${label}" title="${label}">${avatar(person,true)}<span class="presence-dot"></span></span>`;
  }).join('');
}
function syncPresence() {
  const doc = route.view === 'doc' && requestedShare() === null ? activeDoc() : undefined;
  const mode = ui.editing && canWrite(data,doc) ? 'edit' : 'read';
  signalPresence(doc?.kind === 'page' && accessFor(data,doc) !== 'none' ? {page:doc.id,account:data.account,mode} : null);
}
function showPanel(type,options = {}) {
  modal = {type,...options};
  renderPanel();
  if (!panel.open) panel.showModal();
  panel.querySelector('[autofocus]')?.focus();
}
function closePanel() {
  if (panel.open) panel.close();
  modal = null;
}
function renderPanel() {
  if (!modal) return;
  const {title, body, wide} = dialogContent(data,activeDoc(),modal);
  panel.dataset.wide = Boolean(wide);
  panel.innerHTML = `<header class="dialog-header"><h2 id="dialog-title">${html(title)}</h2>${button('close-panel','','close',{className:'ghost',title:'Закрыть'})}</header><div class="dialog-body">${body}</div>`;
}
function openTask(id) {
  const task = taskById(data,id);
  if (task && visibleTasks(data).includes(task)) showPanel('task',{task,snapshot:{status:task.status,person:task.person}});
}
function setTheme(theme) {
  data.theme = theme;
  document.body.dataset.theme = theme;
  try { localStorage.setItem('sever.graph.theme',theme); } catch {}
  const url = new URL(location.href);
  if (url.searchParams.has('theme')) {
    url.searchParams.set('theme',theme);
    history.replaceState(null,'',url);
  }
  persistSession(data);
}
function renderDock() {
  if (!data.account) { dock.innerHTML = ''; return; }
  const work = spaceInfo(data);
  const person = profileFor(data,data.account);
  const account = className => `<button type="button" class="dock-avatar ${className}" data-action="account" aria-label="Учётная запись: ${html(person.name)}">${avatar(person)}</button>`;
  dock.innerHTML = `<div class="dock-utility utility-start" role="group" aria-label="История переходов">
    ${button('history-back','','left',{title:'Назад в приложении',className:'ghost',disabled:visitCursor<1})}
    ${button('history-forward','','arrow',{title:'Вперёд в приложении',className:'ghost',disabled:visitCursor>=visits.length-1})}
    <span class="dock-rule"></span>${button('recent','Недавние','clock',{className:'ghost'})}
  </div><div class="dock-inner">
    <button type="button" class="dock-brand" data-action="spaces" aria-label="Сменить пространство"><span class="brand-mark">${icon(work.glyph,16)}</span><span class="brand-name">${html(work.name)}</span>${icon('down',12)}</button>
    <span class="dock-rule"></span>
    <nav aria-label="Рабочие области">${Object.entries(views).map(([key,[label,glyph]]) =>
      `<a href="${href(key,key === 'doc' ? activeDoc()?.id || initialDoc() : key === 'graph' ? activeDoc()?.graph || 'access' : '')}" class="dock-link" ${key === route.view ? 'aria-current="page"' : ''}>${icon(glyph,15)}<span>${label}</span></a>`
    ).join('')}</nav>
    <span class="dock-rule"></span>
    <div class="dock-actions">
      <button type="button" class="btn ghost dock-search" data-action="search" title="Поиск" aria-label="Поиск">${icon('search',15)}<span class="dock-key">⌘ K</span></button>
      ${canCreateMaterial() ? button('create','','plus',{title:'Создать материал',className:'primary'}) : ''}
      ${account('compact-account')}
    </div></div><div class="dock-utility utility-end" role="group" aria-label="Параметры рабочего пространства">
      ${button('toggle-theme','',data.theme === 'dark' ? 'sun' : 'moon',{title:data.theme === 'dark' ? 'Светлая тема' : 'Тёмная тема',className:'ghost'})}
      ${button('settings','','sliders',{title:'Настройки пространства',className:'ghost'})}
      <span class="dock-rule"></span>${account('desktop-account')}
    </div>`;
}

function render({persistGraphTheme = true} = {}) {
  if (unmountGraph) {
    const selectedTheme = document.body.dataset.graphTheme;
    if (selectedTheme && persistGraphTheme) setTheme(selectedTheme);
    unmountGraph();
    unmountGraph = undefined;
  }
  document.body.dataset.theme = data.theme;
  document.body.dataset.view = route.view;
  const share = requestedShare();
  if (share !== null) {
    dock.innerHTML = '';
    host.innerHTML = renderSharedPage(route.view === 'doc' && route.key === share ? sharedPage(data,share) : undefined,data.coverImages);
    syncPresence();
    return;
  }
  renderDock();
  const docScroll = host.querySelector('.doc-scroll')?.scrollTop || 0;
  const flowScroll = host.querySelector('.flow-surface')?.scrollLeft || 0;
  if (!data.account) {
    host.innerHTML = `<div class="login-wrap"><article class="login-card appearance"><span class="space-emblem">${icon('north',26)}</span><h1>Вход в «Север»</h1><p class="muted">Выберите локальный профиль для этой вкладки.</p><div class="view-actions">${button('account','Выбрать профиль','user',{className:'primary'})}${button('signup','Регистрация','plus')}</div></article></div>`;
    syncPresence();
    return;
  }
  if (route.view === 'doc') {
    host.innerHTML = renderDocuments(data,activeDoc(),ui);
    const form = host.querySelector('[data-form="page"]');
    if (form) connectEditor(form,{onVideo:insert => showPanel('video',{insert})});
    host.querySelector('.doc-scroll').scrollTop = docScroll;
  } else if (route.view === 'board') {
    host.innerHTML = renderBoard(data,ui);
  } else if (!ownsSpace(data,'north') || data.space !== 'north') {
    host.innerHTML = `<section class="board-view"><div class="empty-page"><h1>Область недоступна</h1><p class="muted">Откройте страницу, к которой у вас есть доступ.</p>${button('open-doc','К документам','docs',{value:initialDoc()})}</div></section>`;
  } else if (route.view === 'approach') {
    host.innerHTML = renderApproach(data,run);
    host.querySelector('.flow-surface').scrollLeft = flowScroll;
  } else if (route.view === 'graph') {
    host.innerHTML = '<div class="graph-host"></div>';
    unmountGraph = window.KBGraph.mount(host.firstElementChild,{
      key:route.key, boards:true,
      go:(view,key) => show(view,key),
      navigateKey:key => { route.key = key; history.replaceState(null,'',href('graph',key)); },
      onThemeChange:theme => { setTheme(theme); renderDock(); },
      taskInfo:id => {
        const task = taskById(data,id);
        return task && {sourceKey:task.sourceKey,title:task.title,assignee:profileFor(data,task.person)?.name,status:taskStatuses[task.status]};
      },
      openTask, sourceTitle:key => {
        const doc = documentById(data,key);
        return accessFor(data,doc) !== 'none' ? doc.title : 'Источник недоступен';
      },
    });
  }
  syncPresence();
}

function refreshDocument(action,value) {
  const editing = host.querySelector('[data-form="page"]');
  const doc = activeDoc();
  if (ui.editing && doc && editing?.dataset.pageId === doc.id && canWrite(data,doc)) {
    const layout = host.querySelector('.doc-layout');
    const next = document.createElement('template');
    next.innerHTML = renderDocuments(data,doc,ui);
    layout.dataset.context = next.content.firstElementChild.dataset.context;
    layout.dataset.tree = next.content.firstElementChild.dataset.tree;
    for (const selector of ['.sidebar','.doc-bar','.context']) {
      const old = layout.querySelector(selector), updated = next.content.querySelector(selector);
      if (old && updated) old.replaceWith(updated);
      else if (old) old.remove();
      else if (updated) layout.append(updated);
    }
    renderDock();
    syncPresence();
  } else render();
  const match = [...host.querySelectorAll('[data-action]')].find(el => el.dataset.action === action && (value === undefined || el.dataset.value === value));
  match?.focus({preventScroll:true});
}
function updateAppearance(theme) {
  setTheme(theme);
  if (unmountGraph) render({persistGraphTheme:false});
  else renderDock();
}
function focusHeading(id,button) {
  const target = document.getElementById(id);
  if (!target) return;
  host.querySelectorAll('.outline [aria-current]').forEach(el => el.removeAttribute('aria-current'));
  button.setAttribute('aria-current','true');
  if (compact.matches) {
    ui.context = false;
    refreshDocument('context-panel');
  }
  document.getElementById(id)?.scrollIntoView({behavior:motion.matches ? 'instant' : 'smooth',block:'start'});
}
function copyPageLink() {
  const doc = activeDoc();
  if (!doc || !ownsSpace(data,doc.space)) return;
  const url = new URL(location.href);
  url.hash = href('doc',doc.id);
  if (sharedPage(data,doc.id)) url.searchParams.set('share',doc.id);
  else url.searchParams.delete('share');
  const fallback = () => {
    const label = panel.querySelector('[data-copy-fallback]');
    const input = panel.querySelector('[data-copy-value]');
    if (!label || !input) return;
    label.hidden = false;
    input.value = url.href;
    input.select();
  };
  if (!navigator.clipboard) { fallback(); return; }
  navigator.clipboard.writeText(url.href).then(() => notify('Ссылка скопирована')).catch(fallback);
}
function leaveShare() {
  const url = new URL(location.href);
  url.searchParams.delete('share');
  url.hash = href('doc',initialDoc());
  history.replaceState(null,'',url);
  routeChanged();
}
function changeSpace(space) {
  if (!availableSpaces(data).includes(space)) return;
  data.space = space;
  persistSession(data);
  show('doc',initialDoc());
}
function changeAccount(account) {
  if (account && !profileFor(data,account)) return;
  data.account = account;
  const available = availableSpaces(data);
  if (account && !available.includes(data.space)) data.space = available[0] || 'north';
  ui.editing = false;
  clearDraft();
  stopRun();
  closePanel();
  persistSession(data);
  if (account) routeChanged();
  else render();
}
function canCreateMaterial() {
  return ownsSpace(data) || memberRole(data,data.space) === 'edit';
}
function permission() {
  return data.permissions[activeDoc().id] ??= {members:{},link:false};
}
async function writeLatest(change) {
  const account = data.account, space = data.space;
  try {
    const result = await withWorkspaceLock(() => {
      const newest = loadWorkspace();
      if (newest.account !== account || newest.space !== space) return {message:'Профиль или пространство изменились. Повторите действие.'};
      const outcome = change(newest);
      if (outcome.commit && !persistWorkspace(newest)) return {message:'Локальное хранилище недоступно.'};
      return {newest,...outcome};
    });
    if (result.newest) adopt(result.newest);
    if (result.message) notify(result.message);
    return result;
  } catch {
    notify('Сохранение недоступно.');
    return {};
  }
}
async function changePageAccess(docId,account,role,operation) {
  if (!docId || !account || (operation !== 'revoke' && !['read','comment','edit'].includes(role))) return;
  await writeLatest(newest => {
    const doc = documentById(newest,docId);
    if (!doc || doc.space !== newest.space || !ownsSpace(newest,doc.space)) return {message:'Управление доступом недоступно.'};
    const direct = newest.permissions[doc.id];
    if (operation !== 'grant' && !Object.hasOwn(direct?.members || {},account)) {
      return {message:'Прямой доступ уже отозван в другой вкладке.'};
    }
    if (operation === 'revoke') {
      delete direct.members[account];
      const inherited = accessFor({...newest,account},doc);
      const retained = inherited !== 'none' ? ' Унаследованная роль сохранена · '+roles[inherited]+'.'
        : direct.link ? ' Чтение по публичной ссылке доступно.' : '';
      return {commit:true,message:'Прямой доступ отозван.'+retained};
    }
    if (!profileFor(newest,account) || account === newest.account || (operation === 'grant' && !invitesEnabled(newest,doc.space))) {
      return {message:'Прямой доступ этому профилю недоступен.'};
    }
    (newest.permissions[doc.id] ??= {members:{},link:false}).members[account] = role;
    return {commit:true,message:operation === 'grant' ? 'Доступ выдан локальному профилю.' : 'Роль обновлена.'};
  });
  if (modal?.type === 'share' && activeDoc()?.id === docId) renderPanel();
}
function changeTask(id,patch) {
  return writeLatest(newest => {
    const task = taskById(newest,id);
    if (!canManageTask(newest,task)) return {message:'Задача недоступна в этом пространстве.'};
    if (patch.status !== undefined && !Object.hasOwn(taskStatuses,patch.status)) return {message:'Выберите доступный этап.'};
    if (patch.person !== undefined && patch.person !== task.person && !taskAssignees(newest,task).includes(patch.person)) {
      return {message:'Выберите участника с доступом к задаче.'};
    }
    Object.assign(task,patch);
    return {commit:true,id,message:'Задача обновлена.'};
  });
}
async function changeStatus(id,status) {
  const result = await changeTask(id,{status});
  if (result.id) render();
  return result;
}
async function createTask(form,title,status,sourceKey) {
  if (form.inert) return;
  form.inert = true;
  const origin = modal;
  const result = await writeLatest(newest => {
    const source = documentById(newest,sourceKey);
    if (!ownsSpace(newest) || !Object.hasOwn(taskStatuses,status) ||
      (sourceKey && (source?.space !== newest.space || source.kind !== 'page'))) return {message:'Выберите доступный источник и этап в этом пространстве.'};
    const task = addTask(newest,title,{status,sourceKey});
    return {commit:true,id:task.id,message:'Задача создана.'};
  });
  form.inert = false;
  if (result.id && modal === origin) show('board',result.id);
}
function pageVersion(doc) {
  return {revision:doc.revision,title:doc.title,body:doc.body,cover:coverFor(doc)};
}
function visualVersion(version) {
  // The visual editor canonicalizes legacy spacing between Markdown list items.
  const content = document.createElement('div');
  content.innerHTML = markdown(version.body,{editing:true}).markup;
  return {...version,body:editorMarkdown(content)};
}
function clearDraft() {
  draftBase = conflictBase = uploadedCover = undefined;
}
function beginEdit() {
  const doc = activeDoc();
  if (!canWrite(data,doc) || doc?.kind !== 'page') return;
  clearDraft();
  draftBase = pageVersion(doc);
  ui.editing = true;
  refreshDocument();
  host.querySelector('[data-rich-editor]')?.focus({preventScroll:true});
}
function showConflict(doc) {
  const target = host.querySelector('[data-editor-conflict]');
  if (!target) return;
  conflictBase = pageVersion(doc);
  target.hidden = false;
  target.innerHTML = `<strong>Изменения пересекаются</strong>
    <p class="small">Ваш черновик остался в редакторе. Сравните его с сохранённой редакцией и перенесите нужные изменения вручную.</p>
    <details><summary>Сохранённая редакция ${doc.revision} · ${html(doc.title)}</summary><pre class="agent-preview">${html(doc.body)}</pre></details>
    ${button('accept-saved-base','Использовать редакцию как основу','check')}`;
}
function showRemoteChange(doc) {
  const target = host.querySelector('[data-editor-remote]');
  if (!target || !draftBase) return;
  if (!canWrite(data,doc)) target.textContent = 'Права изменились. Черновик остался в редакторе, сохранение недоступно.';
  else if (doc.revision !== draftBase.revision) target.textContent = 'Сохранена редакция '+doc.revision+' в другой вкладке. Ваш черновик открыт.';
  else target.textContent = '';
  target.hidden = !target.textContent;
}
async function commitPage(form, doc, title, body, cover) {
  if (savingPage || !draftBase || !validateEditorLength(form)) return;
  savingPage = true;
  form.inert = true;
  form.setAttribute('aria-busy','true');
  const base = draftBase, asset = uploadedCover, account = data.account;
  try {
    const result = await withWorkspaceLock(() => {
      const newest = loadWorkspace();
      const stored = documentById(newest,doc.id);
      if (newest.account !== account || !canWrite(newest,stored)) return {kind:'denied',newest};
      const draft = {title,body,cover}, previous = pageVersion(stored);
      const merged = mergePageDraft(base,draft,previous) ||
        mergePageDraft(visualVersion(base),visualVersion(draft),visualVersion(previous));
      if (!merged) return {kind:'conflict',stored,newest};
      if (merged.body.length > markdownLimit) return {kind:'limit',newest};
      if (asset && asset.id === merged.cover) newest.coverImages[asset.id] = asset.source;
      savePage(newest,stored,merged.title,merged.body,merged.cover);
      if (!persistWorkspace(newest)) return {kind:'storage'};
      return {kind:'saved',newest};
    });
    if (result.newest) adopt(result.newest);
    if (!host.contains(form) || draftBase !== base) {
      if (host.querySelector('[data-form="page"]')) showRemoteChange(activeDoc());
      else render();
      return;
    }
    if (result.kind === 'denied') {showRemoteChange(activeDoc()); syncPresence(); return;}
    if (result.kind === 'conflict') {showConflict(result.stored); return;}
    if (result.kind === 'limit') {notify('Объединённый Markdown превышает лимит. Черновик сохранён в редакторе.'); return;}
    if (result.kind === 'storage') {notify('Недостаточно места в локальном хранилище. Черновик сохранён в редакторе.'); return;}
    ui.editing = false;
    clearDraft();
    refreshDocument('edit');
    notify('Редакция сохранена');
  } catch {
    notify('Сохранение недоступно. Черновик сохранён в редакторе.');
  } finally {
    savingPage = false;
    form.inert = false;
    form.removeAttribute('aria-busy');
  }
}
function selectCover(style) {
  const form = host.querySelector('[data-form="page"]');
  if (!form) return;
  const image = uploadedCover?.id === style ? uploadedCover.source : coverImage(data.coverImages,style);
  form.elements.cover.value = style;
  form.querySelector('.editor-cover-slot').innerHTML = editorCover(style,image);
  closePanel();
  form.querySelector('[data-action="covers"]')?.focus();
}
async function uploadCover(control) {
  const file = control.files[0];
  const form = host.querySelector('[data-form="page"]'), current = modal;
  if (!file || !form || modal?.type !== 'covers') return;
  try {
    control.disabled = true;
    const source = await prepareCover(file);
    if (!host.contains(form) || modal !== current) return;
    uploadedCover = {id:'image:'+crypto.randomUUID(),source};
    selectCover(uploadedCover.id);
  } catch (error) {
    if (modal !== current) return;
    const target = panel.querySelector('[data-cover-error]');
    target.textContent = error.message || 'Изображение не удалось прочитать.';
    target.hidden = false;
    control.value = '';
    control.disabled = false;
  }
}
function formError(form,selector,message) {
  const target = form.querySelector(selector);
  target.textContent = message;
  target.hidden = false;
}
function prepareRun() {
  if (run.running) return;
  run = {running:false,steps:[],current:null,completed:false,error:''};
  if (!approachSource(data) || !data.approach.instruction.trim()) {
    run.error = 'Добавьте инструкцию и доступный документ.';
    render();
    return;
  }
  const sequence = getRunSequence(data.approach);
  run.running = true;
  const advance = () => {
    if (run.current) run.steps.push(run.current);
    run.current = sequence[run.steps.length] || null;
    run.running = Boolean(run.current);
    run.completed = !run.running;
    if (route.view === 'approach') render();
    if (run.running) flowTimer = setTimeout(advance,motion.matches ? 100 : 410);
    else notify('Пробный запуск завершён');
  };
  advance();
}
function stopRun() {
  clearTimeout(flowTimer);
  run = {running:false,steps:[],current:null,completed:false,error:''};
}

const actions = {
  search:() => showPanel('search'),
  'history-back':() => moveWithinApp(-1),
  'history-forward':() => moveWithinApp(1),
  recent:() => showPanel('recent',{pages:recentPages()}),
  'toggle-theme':() => updateAppearance(data.theme === 'dark' ? 'light' : 'dark'),
  spaces:() => showPanel('spaces'),
  settings:() => showPanel('settings'),
  account:() => showPanel('account'),
  signup:() => { if (!data.account) showPanel('signup'); },
  'create-space':() => { if (data.account) showPanel('new-space'); },
  'choose-account':value => changeAccount(value),
  'sign-out':() => changeAccount(null),
  'choose-space':value => changeSpace(value),
  'close-panel':() => closePanel(),
  'leave-share':leaveShare,
  'open-doc':value => show('doc',value),
  'open-graph':value => show('graph',value),
  'open-flow':() => show('approach'),
  'tree-folder':value => {
    if (expanded.has(value)) expanded.delete(value); else expanded.add(value);
    if (accessFor(data,documentById(data,value)) !== 'none' && value !== route.key) show('doc',value);
    else refreshDocument('tree-folder',value);
  },
  'tree-panel':() => { ui.tree = !ui.tree; if (ui.tree) ui.context = false; refreshDocument('tree-panel'); },
  'context-panel':() => { ui.context = !ui.context; if (ui.context) ui.tree = false; refreshDocument('context-panel'); },
  'context-tab':value => { ui.contextTab = value; refreshDocument('context-tab',value); },
  heading:focusHeading,
  share:() => { if (activeDoc() && ownsSpace(data,activeDoc().space)) showPanel('share'); },
  'remove-member':value => changePageAccess(activeDoc()?.id,value,null,'revoke'),
  'remove-space-member':value => {
    if (!ownsSpace(data) || !data.memberships[data.space]) return;
    delete data.memberships[data.space][value]; save(); renderPanel(); notify('Доступ к пространству отозван');
  },
  'copy-page-link':copyPageLink,
  create:() => { if (canCreateMaterial()) showPanel('create-menu'); },
  'choose-create':value => {
    if (!canCreateMaterial()) return;
    if (value === 'task' && ownsSpace(data)) showPanel('new-task',{fromChooser:true});
    if (value === 'page' || value === 'folder') showPanel('create',{kind:value,fromChooser:true});
  },
  'create-doc':() => { if (canCreateMaterial()) showPanel('create'); },
  'create-page':value => { if (canWrite(data,documentById(data,value))) showPanel('create',{kind:'page',value}); },
  'create-task':value => { if (ownsSpace(data)) showPanel('new-task',{value}); },
  edit:beginEdit,
  'cancel-edit':() => { ui.editing = false; clearDraft(); refreshDocument('edit'); },
  'accept-saved-base':() => {
    if (!conflictBase) return;
    draftBase = conflictBase;
    conflictBase = undefined;
    host.querySelector('[data-editor-conflict]').hidden = true;
    showRemoteChange(activeDoc());
    notify('Сохранённая редакция выбрана как основа. Объедините нужные изменения в черновике.');
  },
  'play-video':(_,control) => playEmbeddedVideo(control.closest('[data-video]')),
  format:value => { const form = host.querySelector('[data-form="page"]'); if (form) applyFormatting(form,value); },
  'source-mode':() => {
    const form = host.querySelector('[data-form="page"]');
    if (form) toggleSource(form,markdown);
  },
  covers:() => {
    const form = host.querySelector('[data-form="page"]');
    if (form) showPanel('covers',{value:form.elements.cover.value});
  },
  'choose-cover':value => {
    if (Object.hasOwn(coverStyles,value)) selectCover(value);
  },
  revision:value => showPanel('revision',{value}),
  task:openTask,
  'board-mode':value => { ui.boardMode = value; render(); host.querySelector('[data-action="board-mode"][aria-pressed=true]')?.focus(); },
  'flow-node':value => {
    data.approach.node = value; save(); render();
    [...host.querySelectorAll('[data-action="flow-node"]')].find(el => el.dataset.value === value)?.focus({preventScroll:true});
  },
  'flow-guard':() => {
    if (run.running) return;
    stopRun();
    data.approach.guard = !data.approach.guard; data.approach.node = data.approach.guard ? 'guard' : 'instruction'; data.approach.version++;
    save(); render();
  },
  'agent-preview':() => showPanel('agent'),
  'flow-run':prepareRun,
  'flow-stop':() => { stopRun(); render(); notify('Пробный запуск остановлен'); },
  'flow-handoff':async () => {
    if (run.completed && (await changeStatus('KB-014','review')).id) show('board','KB-014');
  },
};

document.addEventListener('click',event => {
  const control = event.target.closest('[data-action]');
  if (!control || control.disabled) return;
  const action = actions[control.dataset.action];
  action?.(control.dataset.value,control);
});
panel.addEventListener('click',event => { if (event.target === panel) closePanel(); });
panel.addEventListener('cancel',() => { modal = null; });
document.addEventListener('input',event => {
  const target = event.target;
  if (target.matches('[data-search]')) {
    modal.query = target.value;
    panel.querySelector('[data-results]').innerHTML = searchResults(data,target.value);
  }
});
document.addEventListener('change',event => {
  const control = event.target;
  if (control.matches('[data-cover-upload]')) {uploadCover(control); return;}
  switch (control.dataset.control) {
    case 'member-role':
      changePageAccess(activeDoc()?.id,control.dataset.value,control.value,'replace'); return;
    case 'space-member-role':
      if (!ownsSpace(data) || !memberRole(data,data.space,control.dataset.value)) return;
      data.memberships[data.space][control.dataset.value] = control.value; save(); notify('Роль участника обновлена'); return;
    case 'share-link':
      if (activeDoc()?.kind !== 'page' || !ownsSpace(data,activeDoc().space)) return;
      permission().link = control.value === 'public'; save(); refreshDocument(); return;
    case 'theme':
      updateAppearance(control.value);
      return;
    case 'settings': {
      const setting = control.dataset.value;
      if (setting === 'invites' && ownsSpace(data)) {
        (data.spaceDetails[data.space] ??= {}).invites = control.checked;
      } else if (setting === 'context') {
        data.settings.context = control.checked;
        ui.context = control.checked && !compact.matches;
        refreshDocument();
      }
      save(); return;
    }
    case 'provider': {
      const key = control.dataset.value;
      if (!data.account || !['sso','google'].includes(key)) return;
      const providers = {...profileFor(data,data.account).providers,[key]:control.checked};
      (data.profiles[data.account] ??= {}).providers = providers;
      save(); notify(control.checked ? 'Демонстрационное подключение добавлено' : 'Демонстрационное подключение удалено'); return;
    }
    case 'board-person': ui.boardPerson = control.value; render(); host.querySelector('[data-control="board-person"]')?.focus(); return;
  }
  const property = control.dataset.approach;
  if (!property || run.running || data.approach[property] === control.value) return;
  if (property === 'retrieval' && !Object.hasOwn(retrievalLabels,control.value)) return;
  data.approach[property] = control.value;
  data.approach.version++;
  stopRun(); save();
  if (property === 'instruction') host.querySelector('.flow-toolbar .tag').lastChild.textContent = ' Редакция '+data.approach.version;
  else render();
});

document.addEventListener('submit',event => {
  const form = event.target;
  if (!form.dataset.form) return;
  event.preventDefault();
  const fields = new FormData(form);
  const value = name => String(fields.get(name) || '');
  const title = value('title').trim();
  const doc = activeDoc();
  switch (form.dataset.form) {
    case 'page':
      if (!doc || !canWrite(data,doc) || !title || !validateEditorLength(form)) return;
      commitPage(form,doc,title,value('body'),value('cover')); return;
    case 'video': {
      if (modal?.type !== 'video') return;
      const video = parseVideo(value('url').trim());
      if (!video) {formError(form,'[data-video-error]','Введите ссылку YouTube, Vimeo или HTTPS-ссылку на файл MP4 или WebM.'); return;}
      const insert = modal.insert;
      closePanel(); insert(video.source); return;
    }
    case 'signup': {
      if (data.account) return;
      const name = value('name').trim(), email = value('email').trim().toLowerCase();
      if (!name || !email) return;
      const personal = signUp(data,name,email,value('provider'));
      if (!personal) {formError(form,'[data-signup-error]','Эта почта уже используется локальным профилем.'); return;}
      data.space = personal.id;
      save(); persistSession(data); show('doc',personal.home.id); return;
    }
    case 'new-space': {
      const name = value('name').trim();
      if (!data.account || !name) return;
      const {id,home} = createSpace(data,name,value('description').trim());
      data.space = id;
      save(); persistSession(data); show('doc',home.id); notify('Пространство создано'); return;
    }
    case 'space-invite': {
      const id = value('member'), role = value('role');
      if (!ownsSpace(data) || !invitesEnabled(data) || id === data.account || !profileFor(data,id) || !['read','comment','edit'].includes(role)) return;
      (data.memberships[data.space] ??= {})[id] = role;
      save(); renderPanel(); notify('Доступ к пространству выдан локальному профилю'); return;
    }
    case 'space-details': {
      const name = value('name').trim();
      if (!ownsSpace(data) || !name) return;
      Object.assign(data.spaceDetails[data.space] ??= {},{name,description:value('description').trim()});
      save();
      if (route.view === 'doc') refreshDocument(); else render();
      renderPanel(); notify('Настройки пространства сохранены'); return;
    }
    case 'profile-details': {
      const name = value('name').trim(), email = value('email').trim().toLowerCase();
      if (!data.account || !name || !email) return;
      const existing = accountForEmail(data,email);
      if (existing && existing !== data.account) {formError(form,'[data-profile-error]','У другого профиля уже указана эта почта.'); return;}
      Object.assign(data.profiles[data.account] ??= {},{name,email});
      save();
      if (route.view === 'doc') refreshDocument(); else render();
      renderPanel(); notify('Профиль сохранён'); return;
    }
    case 'password':
      if (!data.account) return;
      if (value('password').length < 8 || value('password') !== value('confirmation')) {
        formError(form,'[data-password-error]','Введите одинаковые пароли длиной от восьми символов.'); return;
      }
      form.reset();
      form.querySelector('[data-password-error]').hidden = true;
      notify('Проверка смены пароля прошла. В демо пароль не сохраняется.'); return;
    case 'comment':
      if (!doc || !canComment(data,doc) || !value('comment').trim()) return;
      (data.comments[doc.id] ??= []).push({person:data.account,text:value('comment').trim(),date:new Intl.DateTimeFormat('ru-RU',{day:'numeric',month:'long'}).format(new Date())});
      save(); refreshDocument('context-tab','comments'); notify('Комментарий добавлен'); return;
    case 'invite': {
      if (!doc || !ownsSpace(data,doc.space) || !invitesEnabled(data,doc.space)) return;
      const email = value('email').trim().toLowerCase();
      const key = accountForEmail(loadWorkspace(),email);
      if (!key) {formError(form,'[data-share-error]','Сначала создайте локальный профиль для этого адреса.'); return;}
      if (key === data.account) {formError(form,'[data-share-error]','У владельца уже есть доступ.'); return;}
      changePageAccess(doc.id,key,value('role'),'grant'); return;
    }
    case 'new-item': {
      const parent = documentById(data,value('parent'));
      if (!title || (!parent && !canCreateMaterial()) || (parent && (parent.space !== data.space || !canWrite(data,parent)))) return;
      const item = newItem(data,value('kind'),title,parent?.id || '');
      if (parent) expanded.add(parent.id);
      if (item.kind === 'folder') expanded.add(item.id);
      save(); show('doc',item.id); notify(item.kind === 'folder' ? 'Раздел создан' : 'Страница создана'); return;
    }
    case 'new-task': {
      if (!title || !ownsSpace(data)) return;
      createTask(form,title,value('status'),value('sourceKey')); return;
    }
    case 'task': {
      const origin = modal;
      if (!origin?.task || !origin.snapshot || form.inert) return;
      const patch = Object.fromEntries(['status','person'].filter(key => value(key) !== origin.snapshot[key]).map(key => [key,value(key)]));
      if (!Object.keys(patch).length) {closePanel(); return;}
      form.inert = true;
      changeTask(origin.task.id,patch).then(result => {
        form.inert = false;
        if (!result.id) return;
        if (modal === origin) closePanel();
        render();
      });
      return;
    }
  }
});
document.addEventListener('keydown',event => {
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k' && data.account && requestedShare() === null) {
    event.preventDefault();
    showPanel('search');
  }
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 's' && ui.editing && !panel.open) {
    event.preventDefault();
    host.querySelector('[data-form="page"]')?.requestSubmit();
  }
  if (event.key === 'Enter' && event.target.matches('[data-search]')) {
    event.preventDefault();
    panel.querySelector('[data-results] button')?.click();
  }
});

host.addEventListener('dragstart',event => {
  const card = event.target.closest('[data-drag-task]');
  if (!card || !ownsSpace(data)) return;
  event.dataTransfer.setData('text/plain',card.dataset.dragTask);
  event.dataTransfer.effectAllowed = 'move';
});
host.addEventListener('dragover',event => {
  const lane = event.target.closest('.lane');
  if (!lane || !ownsSpace(data)) return;
  event.preventDefault();
  lane.classList.add('drag-over');
});
host.addEventListener('dragleave',event => {
  const lane = event.target.closest('.lane');
  if (lane && !lane.contains(event.relatedTarget)) lane.classList.remove('drag-over');
});
host.addEventListener('drop',event => {
  const lane = event.target.closest('.lane');
  if (!lane || !ownsSpace(data)) return;
  event.preventDefault();
  lane.classList.remove('drag-over');
  changeStatus(event.dataTransfer.getData('text/plain'),lane.dataset.status);
});
host.addEventListener('dragend',() => host.querySelectorAll('.drag-over').forEach(lane => lane.classList.remove('drag-over')));
window.addEventListener('hashchange',() => routeChanged());
window.addEventListener('storage',event => {
  if (event.key !== workspaceStorageKey && event.key !== null) return;
  const openShareId = modal?.type === 'share' ? activeDoc()?.id : undefined;
  const currentAccess = openShareId ? JSON.stringify([data.permissions[openShareId],data.memberships[data.space]]) : '';
  adopt(loadWorkspace());
  if (openShareId && currentAccess !== JSON.stringify([data.permissions[openShareId],data.memberships[data.space]])) {
    const invitation = panel.querySelector('[data-form="invite"]');
    const email = invitation?.elements.email.value, role = invitation?.elements.role.value;
    renderPanel();
    const next = panel.querySelector('[data-form="invite"]');
    if (invitation && next) {
      next.elements.email.value = email;
      next.elements.role.value = role;
    }
  }
  if (ui.editing && host.querySelector('[data-form="page"]')) {
    showRemoteChange(activeDoc());
    syncPresence();
  } else render();
});

const preferredTheme = new URL(location.href).searchParams.get('theme');
const initialTheme = preferredTheme === 'light' || preferredTheme === 'dark' ? preferredTheme : data.theme;
if (requestedShare() === null) setTheme(initialTheme);
else data.theme = initialTheme;
routeChanged();
