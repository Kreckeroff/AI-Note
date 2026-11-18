'use strict';
const app = document.querySelector('#workspace');
const dock = document.querySelector('#dock');
const dialog = document.querySelector('#detail');
const panel = document.querySelector('#dialogContent');
const icons = {
 book:'M3 4h6q3 0 3 3q0-3 3-3h6v15h-6q-3 0-3 2q0-2-3-2H3z M12 7v14', doc:'M5 3h10l4 4v14H5z M14 3v6h5 M8 13h8 M8 17h6',
 graph:'M5 7l8-3 6 12-13 3z M5 7l14 9 M6 19l7-15', board:'M3 4h18v16H3z M9 4v16 M15 4v16', flow:'M2 9h6v6H2z M16 2h6v6h-6z M16 16h6v6h-6z M8 12h4V5h4 M12 12v7h4',
 flag:'M4 21V4q5-3 9 0t7 0v10q-4 3-8 0t-8 0', arrow:'M4 12h15m-6-6 6 6-6 6', link:'M9 15l6-6 M10 7l2-2q4-3 7 0t0 7l-2 2 M14 17l-2 2q-4 3-7 0t0-7l2-2',
 layers:'M2 8l10-5 10 5-10 5z M2 12l10 5 10-5 M2 16l10 5 10-5', dot:'M12 3a9 9 0 1 0 .1 0 M12 9a3 3 0 1 0 .1 0', clock:'M12 3a9 9 0 1 0 .1 0 M12 7v6l4 2', settings:'M3 6h18 M3 12h18 M3 18h18 M8 3v6 M16 9v6 M9 15v6',
 check:'M4 12l5 5L20 6', x:'M5 5l14 14 M19 5L5 19', file:'M4 6h6l2 3h8v11H4z', edit:'M4 20l4-1 12-12-4-4L4 15z M13 6l4 4', person:'M12 4a4 4 0 1 0 .1 0 M4 21v-2q0-5 8-5t8 5v2', bot:'M4 8h16v12H4z M12 3v5 M8 12v3 M16 12v3', grid:'M3 3h7v7H3z M14 3h7v7h-7z M3 14h7v7H3z M14 14h7v7h-7z',
 orbit:'M2 12q4-11 14-9t4 10q-5 10-14 8t-4-9z M4 4q8-4 15 7t-1 10Q6 20 3 11t1-7z', menu:'M3 6h18 M3 12h18 M3 18h18'
};
function ic(key,size=18){return '<svg class="icon" aria-hidden="true" viewBox="0 0 24 24" width="'+size+'" height="'+size+'" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="'+icons[key]+'"/></svg>';}
function esc(str){return String(str).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));}
function button(action,inner,cls='',attrs=''){return '<button type="button" data-shell-action="'+action+'" class="'+cls+'" '+attrs+'>'+inner+'</button>';}
const labels={open:'Открыто',work:'В работе',review:'На согласовании',done:'Готово'};
const people=['Анна · владелец','Аналитик A-02','Оркестратор O-01','Дизайнер D-03'];
const tasks=[
 {id:'KB-014',name:'Вход по приглашению',status:'review',person:0,sourceKey:'access',time:'10:42'},
 {id:'KB-015',name:'Шаблон первой страницы',status:'work',person:1,sourceKey:'template',time:'10:36'},
 {id:'KB-016',name:'Подготовить хэндофф',status:'open',person:2,sourceKey:'workflow',time:'10:28'},
 {id:'KB-013',name:'Выбрать область доступа',status:'done',person:0,sourceKey:'scope',time:'09:55'},
 {id:'KB-012',name:'Обновить карту разделов',status:'work',person:3,sourceKey:'welcome',time:'09:31'}
];

const views = {
  doc: ['Документы', 'book'], graph: ['Граф', 'graph'], flow: ['Процессы', 'flow'],
  board: ['Задачи', 'board'], modules: ['Модули', 'settings']
};
const modes = {
  folio: ['Фолио', 'doc'], atlas: ['Орбита', 'graph'], terminal: ['Диспетчер', 'board']
};
const modules = {doc: window.KBLibrary, graph: window.KBGraph, flow: window.KBWorkflow};
let mode = 'atlas', view = 'graph', currentKey = '', filter = 'all', tableMode = 'table', openId = null, unmount = null;
const flags = {boards:true, sso:true, signup:false};
const taskEvents = new EventTarget();
const findTask = id => tasks.find(task => task.id===id);
const sourceTitle = key => window.KBLibrary.metadata(key)?.title || 'Документ недоступен';
function status(t) {return '<span class="pill '+t.status+'">'+esc(labels[t.status])+'</span>';}
function boardView(){
 const rows=tasks.filter(t=>filter==='all'||t.status===filter);
 const switches=(mode==='terminal'?[['table','Реестр'],['kanban','Kanban']]:[['kanban','Kanban'],['table','Реестр']]).map(([k,v])=>button('layout:'+k,v,tableMode===k?'on':'','aria-pressed="'+(tableMode===k)+'"')).join('');
 const controls='<div class="board-controls"><div class="segments">'+switches+'</div><label>Статус <select id="statusFilter">'+[['all','Все статусы'],...Object.entries(labels)].map(([k,v])=>'<option value="'+k+'" '+(k===filter?'selected':'')+'>'+v+'</option>').join('')+'</select></label></div>';
 const table='<div class="table-frame"><table><thead><tr><th>Статус</th><th>Задача</th><th>Исполнитель</th><th>Источник</th><th>Обновлено</th></tr></thead><tbody>'+rows.map(t=>'<tr class="'+(t.status==='review'?'attention':'')+'"><td>'+status(t)+'</td><td>'+button('task:'+t.id,esc(t.name),'table-link')+'<small>'+t.id+'</small></td><td>'+esc(people[t.person])+'</td><td>'+esc(sourceTitle(t.sourceKey))+'</td><td><time>'+t.time+'</time></td></tr>').join('')+'</tbody></table></div>';
 const kanban='<div class="kanban">'+Object.entries(labels).filter(([state])=>filter==='all'||filter===state).map(([k,v])=>'<section><h3>'+v+' <small>'+rows.filter(t=>t.status===k).length+'</small></h3>'+rows.filter(t=>t.status===k).map(t=>button('task:'+t.id,'<small>'+t.id+'</small><b>'+t.name+'</b><span>'+esc(people[t.person])+'</span>','ticket')).join('')+'</section>').join('')+'</div>';
 return '<div class="screen-heading"><div><span class="kicker">Работа команды</span><h1>Очередь задач</h1></div></div>'+controls+(tableMode==='table'?table:kanban)+(rows.length===0?'<p>В этом статусе пока нет задач.</p>':'')+'<div class="board-summary"><div>'+ic('flow',22)+' <b>Приём участника</b><span>Действующий процесс · v2</span></div>'+button('view:flow','Открыть процесс '+ic('arrow',16),'quiet')+'</div>';
}
const features={sso:['Корпоративный SSO','Вход через поставщика идентификации организации.'],signup:['Самостоятельная регистрация','Создание аккаунта пользователем без приглашения.'],boards:['Доски и задачи','Очередь работы, Kanban и карточки связанных задач.']};
function modulesView(){return '<div class="screen-heading"><div><span class="kicker">Настройки установки</span><h1>Возможности</h1></div></div><div class="settings-panel"><h2>Вход и модули</h2>'+Object.entries(features).map(([k,[name,hint]])=>'<div class="feature-row"><div><b>'+name+'</b><p>'+hint+'</p></div>'+button('flag:'+k,'<span></span>',flags[k]?'switch enabled':'switch','role="switch" aria-checked="'+flags[k]+'" aria-label="'+name+'"')+'</div>').join('')+'<div class="module-result" role="status"><b>Доступные способы входа</b><p>Существующий локальный аккаунт'+(flags.sso?', корпоративный SSO':'')+'. '+(flags.signup?'Самостоятельная регистрация открыта.':'Регистрация закрыта.')+'</p></div></div>';}
const glyphs = {
  search:'M15 15l5 5 M16 10a6 6 0 1 1-12 0 6 6 0 0 1 12 0',
  plus:'M12 5v14 M5 12h14', chevron:'m8 10 4 4 4-4',
  graph:'M7 7l9 3-4 8-5-11 M6 4a2.5 2.5 0 1 0 .1 0 M18 8a2.5 2.5 0 1 0 .1 0 M12 16a2.5 2.5 0 1 0 .1 0'
};
Object.assign(icons,glyphs);
const mark='<svg class="northmark" width="24" height="24" viewBox="0 0 24 24" aria-hidden="true"><path fill="currentColor" d="m12 1 1.9 8.2L22 6l-6.6 6 6.6 6-8.1-3.2L12 23l-1.9-8.2L2 18l6.6-6L2 6l8.1 3.2z"/></svg>';
const lastKey = {graph:'access',doc:'access'};
function toolButton(action,icon,label) {
  return button(action,ic(icon,18),'dock-tool','aria-label="'+label+'" title="'+label+'"');
}
function renderDock() {
  const links=Object.entries(views).filter(([id])=>id!=='modules' && (id!=='board'||flags.boards)).map(([id,[label,icon]])=>
    '<a href="#'+mode+'/'+id+'" class="dock-link" '+(view===id?'aria-current="page"':'')+'>'+ic(icon,17)+'<span>'+label+'</span></a>').join('');
  dock.innerHTML='<div class="dock-inner">'+button('workspace',mark+'<span>Север</span>'+ic('chevron',13),'workspace-switch','aria-label="Меню пространства Север" aria-haspopup="dialog"')+
    '<span class="dock-rule"></span><nav aria-label="Рабочие области">'+links+'</nav><span class="dock-rule"></span>'+
    toolButton('search','search','Поиск по пространству')+button('create',ic('plus',18),'dock-create','aria-label="Создать" title="Создать"')+'</div>';
}
function urlFor(target,key) {
  return '#'+mode+'/'+target+(key?'/'+encodeURIComponent(key):'');
}
function closeDialog(){if(dialog.open)dialog.close();}
function go(target,key) {
  if(!Object.hasOwn(views,target))return;
  const next=urlFor(target,key);
  closeDialog();
  if(location.hash===next){route();return;}
  location.hash=next;
}
function navigateKey(key) {
  currentKey=key;
  if(Object.hasOwn(lastKey,view))lastKey[view]=key;
  history.replaceState(null,'',urlFor(view,key));
}
function taskInfo(id) {
  const t=findTask(id);
  return t && {id:t.id,title:t.name,status:labels[t.status],assignee:people[t.person],sourceKey:t.sourceKey};
}
function tasksForPage(key) {
  return flags.boards ? tasks.filter(t=>t.sourceKey===key).map(t=>taskInfo(t.id)) : [];
}
function onTaskChange(callback,signal) {
  taskEvents.addEventListener('change',event=>callback(event.detail),{signal});
}
function refreshTask(task) {
  if(view==='board')app.querySelector('.shell-page').innerHTML=boardView();
  taskEvents.dispatchEvent(new CustomEvent('change',{detail:taskInfo(task.id)}));
}
function render() {
  if(unmount){unmount();unmount=null;}
  app.replaceChildren();
  document.body.dataset.mode=mode;
  document.body.dataset.view=view;
  renderDock();
  document.title=views[view][0]+' · '+modes[mode][0]+' · Север';
  if(modules[view]){
    unmount=modules[view].mount(app,{mode,key:currentKey,boards:flags.boards,go,navigateKey,openTask:showTask,taskInfo,tasksForPage,onTaskChange,sourceTitle,search:showSearch})||null;
  }else{
    const content=view==='board'?boardView():modulesView();
    app.innerHTML='<div class="shell-scroll"><div class="shell-page">'+content+'</div></div>';
  }
}
function route() {
  const [m,v,k]=location.hash.slice(1).split('/');
  const next=Object.hasOwn(modes,m)?m:'atlas';
  if(next!==mode){filter='all';tableMode=next==='terminal'?'table':'kanban';}
  mode=next;
  view=Object.hasOwn(views,v)?v:modes[mode][1];
  if(view==='board'&&!flags.boards)view='modules';
  try{currentKey=decodeURIComponent(k||'')||lastKey[view]||'';}catch{currentKey=lastKey[view]||'';}
  if(view==='graph'&&!flags.boards&&currentKey==='invitation')currentKey='access';
  if(Object.hasOwn(lastKey,view) && !currentKey.startsWith('new-'))lastKey[view]=currentKey;
  closeDialog();
  render();
}
function openPanel(title,content) {
  panel.innerHTML='<div class="dialog-head"><h2 id="dialogTitle">'+esc(title)+'</h2>'+toolButton('close','x','Закрыть')+'</div>'+content;
  if(!dialog.open)dialog.showModal();
}
function menuItem(action,icon,title,note='') {
  return button(action,ic(icon,19)+'<span><b>'+esc(title)+'</b>'+(note?'<small>'+esc(note)+'</small>':'')+'</span>'+ic('arrow',15),'menu-item');
}
function showWorkspace() {
  const directions=Object.entries(modes).map(([m,[label,home]])=>menuItem('mode:'+m,views[home][1],label,m===mode?'Текущий вариант':views[home][0])).join('');
  openPanel('Пространство Север','<div class="menu-label">Вариант макета</div>'+directions+'<div class="menu-label">Управление</div>'+menuItem('view:modules','settings','Модули и вход')+'<p class="local-note">Демонстрационное пространство</p>');
}
let matches=[];
function searchResults(query) {
  const needle=query.trim().toLocaleLowerCase('ru');
  const docs=window.KBLibrary.search(query).map(d=>({target:'doc',key:d.key,title:d.title,detail:d.path,icon:d.kind==='folder'?'file':'doc'}));
  const sections=Object.entries(views).filter(([v])=>v!=='board'||flags.boards).map(([v,[title,icon]])=>({target:v,title,icon,detail:'Рабочая область'}));
  const work=flags.boards?tasks.map(t=>({task:t.id,title:t.name,detail:t.id+' · '+labels[t.status],icon:'board'})):[];
  matches=[...sections.filter(s=>s.title.toLocaleLowerCase('ru').includes(needle)),...docs,...work.filter(t=>(t.title+' '+t.detail).toLocaleLowerCase('ru').includes(needle))];
  return matches.map((m,i)=>menuItem('match:'+i,m.icon,m.title,m.detail)).join('')||'<p role="status">Совпадений нет. Попробуй название раздела или страницы.</p>';
}
function showSearch() {
  openPanel('Поиск по пространству','<input id="globalQuery" type="search" placeholder="Страница, раздел, задача…" aria-label="Поисковый запрос"><div id="globalResults">'+searchResults('')+'</div>');
  document.querySelector('#globalQuery').focus();
}
function showTask(id) {
  const t=findTask(id);
  if(!flags.boards||!t)return;
  openId=t.id;
  const options=people.map((name,k)=>'<option value="'+k+'" '+(k===t.person?'selected':'')+'>'+esc(name)+'</option>').join('');
  openPanel(t.name,'<div class="task-byline">'+esc(t.id)+' '+status(t)+'</div><label class="field">Исполнитель<select id="assignee">'+options+'</select></label>'+
    menuItem('taskdoc','doc',sourceTitle(t.sourceKey),'Исходный документ')+menuItem('view:flow','flow','Приём участника','Связанный процесс')+
    (t.status==='review'?button('approve','Согласовать в демо','primary'):''));
}
function handleAction(action){
  const [a,b]=action.split(':');
  switch(a){
    case 'workspace':showWorkspace();break;
    case 'create':openPanel('Создать в пространстве',menuItem('new:page','doc','Страницу')+menuItem('new:folder','file','Вложенный раздел'));break;
    case 'new':go('doc','new-'+b);break;
    case 'search':showSearch();break;
    case 'match':{
      const result=matches[Number(b)];
      if(!result)break;
      if(result.task!==undefined)showTask(result.task);else go(result.target,result.key);
      break;
    }
    case 'mode':closeDialog();if(location.hash==='#'+b)route();else location.hash=b;break;
    case 'view':go(b);break;
    case 'close':closeDialog();break;
    case 'layout':tableMode=b;render();break;
    case 'flag':if(Object.hasOwn(flags,b)){flags[b]=!flags[b];render();dockFocus('flag:'+b);}break;
    case 'task':showTask(b);break;
    case 'taskdoc':{const task=findTask(openId);if(task&&flags.boards)go('doc',task.sourceKey);break;}
    case 'approve':{
      const task=findTask(openId);
      if(!task||!flags.boards||task.status!=='review')break;
      task.status='done';refreshTask(task);showTask(task.id);break;
    }
  }
}
function dockFocus(action){document.querySelector('[data-shell-action="'+action+'"]')?.focus();}
document.addEventListener('click',e=>{
  const item=e.target.closest('[data-shell-action]');
  if(item)handleAction(item.dataset.shellAction);
  else if(e.target===dialog)closeDialog();
});
document.querySelector('.skip').addEventListener('click',e=>{e.preventDefault();app.focus();});
document.addEventListener('change',e=>{
  if(e.target.id==='assignee'){
    const task=findTask(openId),person=Number(e.target.value);
    if(task&&flags.boards&&Number.isInteger(person)&&person>=0&&person<people.length){
      task.person=person;refreshTask(task);
    }
  }
  if(e.target.id==='statusFilter'){filter=e.target.value;render();document.querySelector('#statusFilter').focus();}
});
document.addEventListener('input',e=>{
  if(e.target.id==='globalQuery')document.querySelector('#globalResults').innerHTML=searchResults(e.target.value);
});
document.addEventListener('keydown',e=>{
  if((e.ctrlKey||e.metaKey)&&e.key.toLowerCase()==='k'){
    if(document.querySelector('dialog[open]:not(#detail)'))return;
    e.preventDefault();showSearch();
  }
});
window.addEventListener('hashchange',route);
route();
