import {escapeHtml as html, icon} from './ui.mjs';

const get = id => document.getElementById(id);
const narrow = matchMedia('(max-width: 940px)');
const title = 'Доступ к пространству';
const roles = {read:'Просмотр', comment:'Комментирование', edit:'Редактирование', owner:'Владелец'};
const order = ['read','comment','edit','owner'];
const people = [
  {id:'owner', name:'Мария Орлова', email:'m.orlova@sever.example', inherited:'owner'},
  {id:'daria', name:'Дарья Волкова', email:'d.volkova@sever.example', direct:'comment'},
  {id:'alex', name:'Алексей Миронов', email:'a.mironov@sever.example', direct:'read', inherited:'read'},
  {id:'anton', name:'Антон Громов', email:'a.gromov@sever.example', inherited:'read', admin:true},
  {id:'nikita', name:'Никита Ларин', email:'n.larin@sever.example', direct:'edit'},
  {id:'olga', name:'Ольга Котова', email:'o.kotova@sever.example', inherited:'edit'},
  {id:'none', name:'Сергей Белов', email:'s.belov@sever.example'},
];
const guest = {id:'guest',name:'Елена Смирнова',email:'elena@partner.example'};
people.push(guest);
const invitations = [{id:'guest',email:guest.email,role:'comment',status:'pending'}];
const paragraphs = [
  'Чтобы открыть командные материалы, укажите в заявке название пространства и нужное право.',
  'Для чтения одного документа достаточно адресного доступа к странице. Владелец видит выданные права в настройках документа.',
  'Когда задача завершена, проверьте список людей и закройте доступ, который больше не нужен.',
];
const threads = [
  {id:'scope',author:'daria',time:'Сегодня, 10:24',text:'Уточним, какой доступ нужен подрядчику для проверки инструкции?',quote:'Для чтения одного документа достаточно адресного доступа к странице.',replies:[
    {id:'scope-reply',author:'owner',time:'Сегодня, 10:32',text:'Дадим комментирование только этой страницы.'},
  ]},
  {id:'discussion',author:'alex',time:'Сегодня, 09:10',text:'Добавить пример заявки для нового сотрудника.',replies:[]},
  {id:'previous',author:'owner',time:'Вчера, 17:08',text:'Обновить старое название команды.',orphan:true,resolved:'owner',resolvedAt:'Вчера, 17:40',replies:[]},
];
let notifications = [];
let requests = [{id:'none',status:'pending'}];
const params = new URLSearchParams(location.search);
const entry = params.get('view');
let viewer = ['owner','daria','alex','guest','public','none'].includes(entry) ? entry : 'owner';
let previewGuest = 'guest';
if(entry==='none') requests=[];
let panel = entry === 'comments' || entry === 'daria';
let tab = panel ? 'comments' : 'about';
let filter = 'active';
let dialogMode;
let shareTab = 'people';
let search = '';
let personFilter = 'all';
let pageIndex = 0;
let inviteByEmail = false;
let policy = entry === 'public';
let publicLink = policy ? params.get('token') || 'preview-1' : null;
let publicSequence = 1;
let editing = false;
let revision = 4;
let replyTo;
let quote = '';
let selectedText = '';
let savedRange;
let noticeTimer;
let closeFocus;
let hideMobile = false;
const drafts = new Map();
const mentions = new Map();

const person = id => people.find(item => item.id === id);
const viewerId = () => viewer==='guest' ? previewGuest : viewer;
const roleOf = item => item && order[Math.max(order.indexOf(item.direct),order.indexOf(item.inherited))];
const authRole = () => roleOf(person(viewerId()));
const canDiscuss = () => ['owner','edit','comment'].includes(authRole());
const canEdit = () => ['owner','edit'].includes(authRole());
const isOwner = () => viewer === 'owner';
const isPublic = () => viewer === 'public';
const permitted = () => isPublic() ? Boolean(publicLink) : Boolean(authRole());
const canManageThread = thread => canDiscuss() && (canEdit() || thread.author === viewerId());
const thread = id => threads.find(item => item.id === id);
const draftsKey = id => viewerId() + ':' + (id || 'page');
const activeTargets = () => people.filter(item => item.id !== viewerId() && roleOf(item));
const initials = name => name.split(' ').map(part => part[0]).slice(0,2).join('');
const now = () => 'Сегодня, ' + new Intl.DateTimeFormat('ru',{hour:'2-digit',minute:'2-digit'}).format(new Date());
const mediaPath = '../handoff/assets/sever-slash-menu.png';
const btn = (action,label,{value='',kind='ghost',disabled=false,attributes=''}={}) =>
  '<button type="button" class="btn ' + kind + '" data-action="' + action + '" data-value="' + html(value) + '"' + (disabled?' disabled':'') + ' ' + attributes + '>' + html(label) + '</button>';

function alert(message) {
  clearTimeout(noticeTimer);
  const notice=get('notice');
  notice.textContent=message;
  notice.hidden=false;
  noticeTimer=setTimeout(()=>{notice.hidden=true;},2600);
}

function meta(id, time) {
  const name=person(id)?.name || 'Участник';
  return '<div class="meta"><span class="sharing-avatar" aria-hidden="true">' + html(initials(name)) + '</span><strong>' + html(name) + '</strong><small>' + html(time) + '</small></div>';
}

function renderDocument() {
  const seen = permitted();
  const publicView = isPublic();
  const contextAccess = seen && !publicView;
  const showTree = contextAccess && viewer !== 'guest';
  get('tree').hidden=!showTree;
  get('workspace').dataset.tree=String(showTree);
  get('dock-name').innerHTML=icon('north')+'<strong>Север</strong>'+(showTree?'<span>/</span><span>Документация команды</span>':'');
  get('tree').innerHTML='<div class="space-card"><span class="space-emblem">'+icon('folder',18)+'</span><strong>Документация команды</strong></div><div class="tree-item">Руководство</div><div class="tree-level"><span class="tree-item" aria-current="page">'+title+'</span></div>';
  const count=notifications.filter(item=>item.target===viewerId()&&seen&&!publicView).length;
  get('notification-control').textContent='Уведомления'+(count?' '+count:'');
  get('notification-control').hidden=!contextAccess;
  get('perspective').value=viewer;
  if (!seen) {
    const pending=invitations.find(item=>item.id===previewGuest && item.status==='pending');
    const guestCard=viewer==='guest'&&pending;
    const requested=requests.find(item=>item.id===viewer);
    const text=publicView?'Попросите владельца прислать действующую ссылку.'
      : guestCard?'Подтвердите адрес электронной почты, чтобы принять приглашение.'
      : viewer==='guest'?'Попросите владельца прислать новое приглашение.'
      : 'Откройте обычную ссылку под нужной учётной записью или запросите просмотр.';
    const heading=publicView?'Ссылка недоступна':guestCard?'Приглашение к странице':'Страница недоступна';
    const actions=guestCard ? btn('accept','Подтвердить адрес и принять',{kind:'primary'})
      : !publicView&&viewer!=='guest'&&!requested?btn('request','Запросить доступ',{kind:'primary'}):'';
    get('document').innerHTML='<article class="sharing-status-card"><h1>'+heading+'</h1><p class="muted">'+text+'</p>'+actions
      +(requested?.status==='pending'?'<p role="status">Запрос принят к обработке.</p>':'')+'</article>';
    return;
  }

  const crumbs=showTree?'<div class="breadcrumbs">Руководство <span aria-hidden="true">›</span> <strong>'+title+'</strong></div>'
    :'<div class="breadcrumbs">'+(publicView?'Чтение по публичной ссылке':'Доступ только к этой странице')+'</div>';
  const buttons=(canEdit()?btn('edit','Редактировать'):'')
    +(contextAccess?btn('panel','Контекст страницы',{attributes:'aria-expanded="'+panel+'"'}):'')
    +(isOwner()?btn('share','Поделиться',{kind:'primary'}):'');
  const content=editing&&canEdit()?'<form class="edit-surface"><label for="demo-page-text">Текст страницы</label><textarea id="demo-page-text" class="field">'+html(paragraphs.join('\n\n'))+'</textarea><div class="dialog-buttons">'+btn('cancel-edit','Отмена')+btn('save-text','Сохранить',{kind:'primary'})+'</div></form>'
    :'<div class="article-body" id="article-text">'+paragraphs.map((p,i)=>'<p id="paragraph-'+i+'" tabindex="-1">'+html(p)+'</p>').join('')+'</div>'
      +'<div class="sharing-selection" id="selection-action" hidden>'+btn('quote','Прокомментировать выделение',{kind:'primary'})+'</div>';
  get('document').innerHTML='<header class="doc-bar">'+crumbs+'<div class="bar-actions">'+buttons+'</div></header>'
    +'<article class="article"><div class="cover orbit" data-cover="orbit" aria-hidden="true"></div><h1>'+title+'</h1>'
    +'<div class="page-meta"><span class="tag">Редакция '+String(revision).padStart(2,'0')+'</span>'
    +(viewer==='guest'?'<span class="tag">'+roles[authRole()]+'</span>':'')+'</div>'+content
    +(!editing?'<figure class="sharing-illustration"><img src="'+mediaPath+'" alt="Меню команд редактора"></figure>':'')
    +(publicView?'':'<section class="example-attachments"><h2>Вложения</h2><a class="entity" href="'+mediaPath+'" download="Меню редактора.png"><span class="entity-icon">'+icon('doc')+'</span><span class="entity-text"><strong>Меню редактора.png</strong><small>Скачать вложение</small></span></a></section>')+'</article>';
}

function composer(id) {
  const reply=Boolean(id), key=draftsKey(id);
  const label=reply?'Ответ в ветке':'Новый комментарий';
  return '<form class="sharing-compose" data-form="comment" data-thread="'+html(id||'')+'">'
    +(!reply&&quote?'<p class="sharing-quote" aria-label="Выбранный фрагмент">'+html(quote)+'</p>'+btn('clear-quote','Убрать привязку'):'')
    +'<label for="share-message">'+label+'</label><textarea class="field" id="share-message" name="message" rows="3" maxlength="500" required placeholder="'+(reply?'Ответить по теме':'Напишите комментарий. @ — упомянуть участника')+'">'+html(drafts.get(key)||'')+'</textarea>'
    +'<div class="sharing-mentions" id="mention-list" aria-label="Упомянуть участника"></div>'
    +'<div class="form-foot"><small id="count-message">'+(drafts.get(key)||'').length+' / 500</small>'
    +'<button class="btn primary" type="submit">Отправить</button></div></form>';
}

function drawThread(item) {
  const canReply=canDiscuss()&&!item.resolved;
  const resolution=item.resolved?'<p class="muted">Решено · '+html(person(item.resolved)?.name||'Участник')+' · '+html(item.resolvedAt)+'</p>':'';
  const excerpt=item.orphan?'<p class="muted">Фрагмент изменён или удалён</p>'
    : item.quote?btn('jump',item.quote,{value:item.id,kind:'ghost sharing-quote',attributes:'aria-label="К фрагменту"'}):'';
  const actions=(canReply?btn('reply','Ответить',{value:item.id}):'')
    +(canManageThread(item)?btn(item.resolved?'reopen':'resolve', item.resolved?'Открыть снова':'Решить',{value:item.id}):'')
    +btn('copy-thread','Ссылка на ветку',{value:item.id});
  return '<article id="thread-'+html(item.id)+'" tabindex="-1" class="sharing-thread">'+meta(item.author,item.time)+excerpt
    +'<p>'+html(item.text)+'</p>'
    +item.replies.map(message=>'<div class="reply" id="message-'+html(message.id)+'" tabindex="-1">'+meta(message.author,message.time)+'<p>'+html(message.text)+'</p></div>').join('')
    +resolution+'<div class="thread-actions">'+actions+'</div>'
    +(replyTo===item.id&&canReply?composer(item.id):'')+'</article>';
}

function renderPanel() {
  const show=panel&&permitted()&&!isPublic();
  const dock=get('inspector'),mobile=get('mobile-inspector');
  dock.hidden=!show||narrow.matches;
  get('workspace').dataset.panel=String(show&&!narrow.matches);
  if(!show||!narrow.matches) {
    if(mobile.open) {hideMobile=true;mobile.close();}
    mobile.innerHTML='';
  }
  if(!show) {dock.innerHTML='';return;}
  const tabs=[['about','Контекст'],['comments','Комментарии'],...(canEdit()?[['history','История']]:[])];
  if(!tabs.some(([id])=>id===tab)) tab='about';
  const toggles=tabs.map(([id,label])=>btn('tab',label,{value:id,attributes:'role="tab" aria-selected="'+(tab===id)+'" id="inspector-tab-'+id+'" tabindex="'+(tab===id?0:-1)+'"'})).join('');
  let body;
  if(tab==='comments'){
    const rows=threads.filter(item=>Boolean(item.resolved)===(filter==='resolved'));
    body='<section class="context-section"><h3>Обсуждение</h3><div class="thread-actions">'
      +btn('thread-filter','Активные',{value:'active',attributes:'aria-pressed="'+(filter==='active')+'"'})
      +btn('thread-filter','Решённые',{value:'resolved',attributes:'aria-pressed="'+(filter==='resolved')+'"'})+'</div></section>'
      +(rows.length?rows.map(drawThread).join(''):'<p class="muted">Здесь пока нет веток.</p>')
      +(canDiscuss()&&filter==='active'&&!replyTo?composer():'');
  } else if(tab==='history'){
    body='<section class="context-section"><h3>Редакции страницы</h3><div class="sharing-thread"><strong>Редакция '+revision+' · Текущая</strong><p>Мария Орлова · Сегодня, 10:20</p></div><div class="sharing-thread"><strong>Редакция '+(revision-1)+'</strong><p>Мария Орлова · Вчера, 15:15</p></div></section>';
  } else {
    body='<section class="context-section"><h3>Доступ</h3><p class="sharing-status">'+html(roles[authRole()])+'</p>'
      +(viewer==='guest'?'<p class="sharing-status">Назначено только на эту страницу</p>':'')+'</section>'
      +'<section class="context-section"><h3>О странице</h3><p class="sharing-status">Мария Орлова · Редакция '+revision+'</p></section>';
  }
  const contents='<div id="inspector-body"><div class="context-head"><h2>Страница</h2>'+btn('close-panel','Закрыть')+'</div>'
    +'<div class="context-tabs" role="tablist" aria-label="Контекст страницы">'+toggles+'</div>'
    +'<div role="tabpanel" aria-labelledby="inspector-tab-'+tab+'">'+body+'</div></div>';
  if(narrow.matches) {
    dock.innerHTML='';
    mobile.innerHTML=contents;
    if(!mobile.open) mobile.showModal();
  } else dock.innerHTML=contents;
  drawMentions();
}

const modal = get('sharing-dialog');
const pageSize = 5;
const roleOptions = (selected='read') => order.slice(0,3).map(role =>
  '<option value="'+role+'"'+(role===selected?' selected':'')+'>'+roles[role]+'</option>').join('');
const urlFor = (view, fragment) => {
  const url = new URL(location.pathname,location.origin);
  if(view) url.searchParams.set('view',view);
  if(view==='public'&&publicLink) url.searchParams.set('token',publicLink);
  if(fragment) url.hash=fragment;
  return url.href;
};
const matchingPeople = () => people.filter(item => roleOf(item)
  && (personFilter==='all'||personFilter==='direct'&&item.direct||personFilter==='inherited'&&item.inherited)
  && (item.name+' '+item.email).toLocaleLowerCase('ru').includes(search.trim().toLocaleLowerCase('ru')));

function drawPeople() {
  const list = get('people-list');
  if(!list) return;
  const filtered=matchingPeople();
  const total=Math.max(1,Math.ceil(filtered.length/pageSize));
  pageIndex=Math.min(pageIndex,total-1);
  list.innerHTML=filtered.slice(pageIndex*pageSize,(pageIndex+1)*pageSize).map(item=>{
    const inherited=item.inherited?(item.admin?'Через роль администратора':'Через пространство'):null;
    const direct=item.direct?'Назначено на страницу':null;
    const level=roleOf(item);
    return '<li><span aria-hidden="true" class="sharing-avatar">'+html(initials(item.name))+'</span><div class="person"><strong>'+html(item.name)
      +'</strong><small>'+html(item.email)+'</small><small>'+[direct,inherited].filter(Boolean).join(' · ')+'</small></div>'
      +(item.direct?'<select class="field" data-role="'+html(item.id)+'" aria-label="Прямой доступ · '+html(item.name)+'">'+roleOptions(item.direct)+'</select>'
        +btn('revoke','Отозвать',{value:item.id,attributes:'aria-label="Отозвать прямой доступ · '+html(item.name)+'"'})
        :'<span class="tag">'+roles[level]+'</span>')
      +(item.direct&&item.inherited?'<small class="sharing-status">Итог: '+roles[level]+'</small>':'')+'</li>';
  }).join('')||'<li class="sharing-status">Совпадений нет.</li>';
  get('people-pager').innerHTML=total>1?btn('people-page','Назад',{value:'-1',disabled:pageIndex===0})
    +'<span class="sharing-status">Страница '+(pageIndex+1)+' из '+total+'</span>'
    +btn('people-page','Дальше',{value:'1',disabled:pageIndex===total-1}):'';
}

function drawShare() {
  const tabs=[['people','Люди'],['link','Ссылка'],['requests','Запросы']];
  const pendingCount=requests.filter(item=>item.status==='pending').length;
  let content='<div class="dialog-tabs" role="tablist" aria-label="Управление доступом">'
    +tabs.map(([id,label])=>btn('share-tab',label+(id==='requests'&&pendingCount?' '+pendingCount:''),{value:id,attributes:'role="tab" aria-selected="'+(shareTab===id)+'" id="share-tab-'+id+'" tabindex="'+(shareTab===id?0:-1)+'"'})).join('')+'</div>';
  if(shareTab==='people') {
    const activeInvitations=invitations.filter(item=>item.status!=='accepted');
    content+='<form class="sharing-grant" data-form="access"><label>'+ (inviteByEmail?'Почта гостя':'Почта сотрудника')
      +'<input class="field" type="email" name="email" placeholder="name@example.com" required'+(!inviteByEmail?' list="directory"':'')+'>'
      +'</label><label>Уровень доступа<select class="field" name="role">'+roleOptions()+'</select></label>'
      +'<button class="btn primary" type="submit">'+(inviteByEmail?'Пригласить':'Выдать доступ')+'</button>'
      +'<datalist id="directory">'+people.filter(item=>item.id!=='owner'&&item.id!=='guest').map(item=>'<option value="'+html(item.email)+'">'+html(item.name)+'</option>').join('')+'</datalist></form>'
      +btn('invite-mode',inviteByEmail?'Выбрать сотрудника':'Пригласить внешнего гостя')
      +'<p class="sharing-status">Доступ только к этой странице. Права из пространства сохраняются.</p>'
      +'<div class="sharing-tools"><input type="search" class="field" id="people-search" aria-label="Поиск по имени или почте" placeholder="Поиск по имени или почте" value="'+html(search)+'"><select class="field compact" id="people-filter" aria-label="Источник доступа">'
      +[['all','Все люди'],['direct','Прямой доступ'],['inherited','Из пространства']].map(([id,label])=>'<option value="'+id+'"'+(id===personFilter?' selected':'')+'>'+label+'</option>').join('')
      +'</select></div><ul class="sharing-people" id="people-list"></ul><div class="sharing-pager" id="people-pager"></div>'
      +(activeInvitations.length?'<h3>Приглашения</h3><ul class="sharing-people">'+activeInvitations.map(item=>'<li><div class="person"><strong>'+html(item.email)+'</strong><small>'+roles[item.role]+' · '+(item.status==='pending'?'Ожидает · 7 дней':'Отменено')+'</small></div>'
        +(item.status==='pending'?btn('preview-invite','Открыть приглашение',{value:item.id})+btn('cancel-invite','Отменить приглашение',{value:item.id}):btn('renew-invite','Пригласить снова',{value:item.id}))+'</li>').join('')+'</ul>':'');
  } else if(shareTab==='link') {
    content+='<section class="sharing-link-box"><h3>Обычная ссылка</h3><p class="sharing-status">Открывается у людей с действующим доступом.</p>'
      +'<input class="field" aria-label="Обычная ссылка" readonly value="'+html(urlFor('comments'))+'">'
      +'<div class="sharing-link-actions">'+btn('copy','Скопировать обычную ссылку',{value:'internal'})+'</div></section>'
      +'<section class="sharing-link-box"><h3>Публичная ссылка</h3><p class="sharing-status">'
      +(publicLink?'Активна. По ссылке доступно чтение текущей страницы.':policy?'Не создана. Разрешено политикой пространства.':'Запрещена политикой пространства.')+'</p>'
      +(publicLink?'<input class="field" aria-label="Публичная ссылка" readonly value="'+html(urlFor('public'))+'"><div class="sharing-link-actions">'
        +btn('copy','Скопировать публичную ссылку',{value:'public'})+btn('preview-public','Посмотреть по ссылке')+btn('disable','Отключить публичную ссылку')+'</div>'
        :'<div class="sharing-link-actions">'+btn('create-public','Создать публичную ссылку',{disabled:!policy,kind:'primary'})+'</div>')+'</section>'
      +'<details class="sharing-policy"><summary>Политика пространства</summary><p class="sharing-status">'+(policy?'Разрешено создавать публичные ссылки.':'Создание публичных ссылок запрещено.')+'</p>'
      +btn('policy',policy?'Запретить публичные ссылки':'Разрешить публичные ссылки')+'</details>';
  } else {
    const rows=requests.map(item=>'<li><div class="person"><strong>'+html(person(item.id)?.name||'Сотрудник')+'</strong><small>'
      +(item.status==='pending'?'Запрошен просмотр':item.status==='approved'?'Просмотр выдан':'Отклонено')+'</small></div>'
      +(item.status==='pending'?btn('grant-request','Дать просмотр',{value:item.id,kind:'primary'})+btn('deny-request','Отклонить',{value:item.id}):'')+'</li>');
    content+='<ul class="sharing-people">'+(rows.join('')||'<li class="sharing-status">Запросов пока нет.</li>')+'</ul>';
  }
  return content;
}

function renderDialog() {
  if(!dialogMode) return;
  const {type,id}=dialogMode;
  let heading, content;
  if(type==='share') {
    heading='Управление доступом';
    content='<p class="sharing-status">'+title+'</p>'+drawShare();
  } else if(type==='notifications') {
    heading='Уведомления';
    const mine=notifications.filter(item=>item.target===viewerId() && permitted());
    content=mine.length?mine.map(item=>'<article class="sharing-thread"><p>'+html(person(item.author)?.name||'Участник')+' упоминает вас в комментарии.</p>'
      +btn('open-notification','Открыть комментарий',{value:item.thread})+'</article>').join(''):'<p class="muted">Пока нет уведомлений.</p>';
  } else {
    const revoke=type==='revoke';
    const disabling=type==='disable';
    heading=revoke?'Отозвать прямой доступ?':disabling?'Отключить публичную ссылку?':'Запретить публичные ссылки?';
    const row=person(id);
    const text=revoke?(row.name+'. '+(row.inherited?'Останется «'+roles[row.inherited]+'» через пространство или роль организации.':'Адресный доступ будет закрыт.')+(publicLink?' Публичная ссылка останется активной.':''))
      :disabling?'Старая ссылка и её адреса изображений перестанут открываться. Адресные права сохранятся.'
      :publicLink?'Будет закрыта 1 активная ссылка. Затронута страница «'+title+'».':'Сейчас в пространстве нет активных публичных ссылок.';
    content='<p>'+html(text)+'</p><div class="dialog-buttons">'+btn('back-share','Отмена')
      +btn('apply-confirm',revoke?'Отозвать':disabling?'Отключить':'Запретить',{kind:'primary'})+'</div>';
  }
  modal.innerHTML='<div class="dialog-header"><h2 id="sharing-title">'+heading+'</h2>'+btn('close-dialog','Закрыть')+'</div><div class="dialog-body">'+content+'</div>';
  if(type==='share'&&shareTab==='people') drawPeople();
}

function showDialog(type, id) {
  if(!modal.open) closeFocus=document.activeElement;
  dialogMode={type,id};
  renderDialog();
  if(!modal.open) modal.showModal();
  modal.querySelector('[data-action="close-dialog"]')?.focus();
}

function openShareTab(id) {
  shareTab=id;
  renderDialog();
  get('share-tab-'+id)?.focus({preventScroll:true});
}

function renderAll() {
  renderDocument();
  renderPanel();
}

function chooseViewer(value) {
  if(modal.open) modal.close();
  viewer=value;
  selectedText='';
  savedRange=undefined;
  quote='';
  replyTo=undefined;
  editing=false;
  filter='active';
  panel=false;
  tab='about';
  renderAll();
}

async function copy(address) {
  try {await navigator.clipboard.writeText(address);alert('Ссылка скопирована.');}
  catch {alert('Выделите адрес в поле и скопируйте его.');}
}

function navigateThread(id) {
  const item=thread(id);
  if(!item||!permitted()||isPublic()) return;
  if(modal.open) modal.close();
  tab='comments';
  panel=true;
  filter=item.resolved?'resolved':'active';
  renderPanel();
  get('thread-'+id)?.focus();
}

function drawMentions() {
  const input=get('share-message'),menu=get('mention-list');
  if(!input||!menu||!canDiscuss()) return;
  const before=input.value.slice(0,input.selectionStart);
  const match=/(?:^|\s)@([^\s@]{0,28})$/u.exec(before);
  menu.innerHTML=match?activeTargets().filter(target=>target.name.toLocaleLowerCase('ru').includes(match[1].toLocaleLowerCase('ru')))
    .slice(0,5).map(target=>btn('mention',target.name,{value:target.id})).join(''):'';
}

function addMention(id) {
  const input=get('share-message'),target=person(id);
  if(!input||!target||!roleOf(target)) return;
  const before=input.value.slice(0,input.selectionStart);
  const marker=before.lastIndexOf('@');
  if(marker<0) return;
  const key=draftsKey(input.form.dataset.thread);
  mentions.set(key,new Set([...(mentions.get(key)||[]),id]));
  input.setRangeText('@'+target.name+' ',marker,input.selectionEnd,'end');
  input.dispatchEvent(new Event('input',{bubbles:true}));
  input.focus();
}

function focusPanelTrigger() {
  get('document').querySelector('[data-action="panel"]')?.focus({preventScroll:true});
}

function openTab(id) {
  tab=id;
  panel=true;
  renderPanel();
  get('document').querySelector('[data-action="panel"]')?.setAttribute('aria-expanded','true');
  get('inspector-tab-'+id)?.focus({preventScroll:true});
}

function panelOff() {
  panel=false;
  renderPanel();
  get('document').querySelector('[data-action="panel"]')?.setAttribute('aria-expanded','false');
  focusPanelTrigger();
}

function getSelected() {
  const range=window.getSelection();
  const content=get('article-text');
  if(!content||!range?.rangeCount||!content.contains(range.anchorNode)||!content.contains(range.focusNode)) return;
  const text=range.toString().trim();
  if(text) return {text,range:range.getRangeAt(0).cloneRange()};
}

function syncSelection() {
  const action=get('selection-action');
  if(!action||!canDiscuss()) return;
  const selected=getSelected();
  if(selected) {selectedText=selected.text;savedRange=selected.range;}
  action.hidden=!selected;
}

function jumpTo(id) {
  const excerpt=thread(id)?.quote;
  if(!excerpt) return;
  const hits=[...get('article-text').querySelectorAll('p')].filter(row=>row.textContent.includes(excerpt));
  if(hits.length!==1) return;
  get('article-text').querySelector('.sharing-highlight')?.classList.remove('sharing-highlight');
  if(narrow.matches) panelOff();
  hits[0].classList.add('sharing-highlight');
  hits[0].scrollIntoView({block:'center'});
  hits[0].focus({preventScroll:true});
}

function changeState(id, resolving) {
  const item=thread(id);
  if(!item||!canManageThread(item)) return;
  if(resolving) {
    item.resolved=viewerId();
    item.resolvedAt=now();
  } else {
    delete item.resolved;
    delete item.resolvedAt;
  }
  filter=resolving?'resolved':'active';
  replyTo=undefined;
  renderPanel();
  get('thread-'+id)?.focus();
  alert(resolving?'Ветка отмечена решённой.':'Ветка снова активна.');
}

function applyConfirm() {
  const {type,id}=dialogMode;
  if(type==='revoke') {
    delete person(id).direct;
    alert('Прямой доступ отозван.');
  } else if(type==='disable') {
    publicLink=null;
    alert('Публичная ссылка отключена.');
  } else {
    publicLink=null;
    policy=false;
    alert('Публичные ссылки запрещены.');
  }
  showDialog('share');
}

const actions = {
  panel:()=>panel?panelOff():openTab(tab),
  'close-panel':panelOff,
  'tab':openTab,
  share:()=>{ if(isOwner()) {shareTab='people';showDialog('share');} },
  'share-tab':openShareTab,
  'invite-mode':()=>{inviteByEmail=!inviteByEmail;renderDialog();modal.querySelector('[name=email]')?.focus();},
  'people-page':delta=>{pageIndex+=Number(delta);drawPeople();},
  revoke:id=>{if(isOwner()) showDialog('revoke',id);},
  'apply-confirm':()=>{if(isOwner()) applyConfirm();},
  'create-public':()=>{if(isOwner()&&policy){publicLink=String(++publicSequence);renderDialog();alert('Публичная ссылка создана.');}},
  'preview-public':()=>{if(publicLink) chooseViewer('public');},
  'preview-invite':id=>{previewGuest=id;chooseViewer('guest');},
  'cancel-invite':id=>{if(isOwner()){invitations.find(item=>item.id===id).status='revoked';renderDialog();alert('Приглашение отменено.');}},
  'renew-invite':id=>{if(isOwner()){invitations.find(item=>item.id===id).status='pending';renderDialog();alert('Создано новое приглашение.');}},
  disable:()=>showDialog('disable'),
  policy:()=>{if(policy) showDialog('policy');else {policy=true;renderDialog();alert('Публичные ссылки разрешены.');}},
  'back-share':()=>showDialog('share'),
  'close-dialog':()=>modal.close(),
  copy:id=>copy(urlFor(id==='public'?'public':'comments')),
  'copy-thread':id=>copy(urlFor('comments','thread-'+id)),
  'thread-filter':id=>{filter=id;replyTo=undefined;renderPanel();},
  reply:id=>{replyTo=replyTo===id?undefined:id;renderPanel();get('share-message')?.focus();},
  quote:()=>{
    if(!canDiscuss()||!selectedText) return;
    quote=selectedText;
    replyTo=undefined;
    filter='active';
    openTab('comments');
    get('share-message')?.focus();
  },
  'clear-quote':()=>{
    quote='';
    renderPanel();
    if(savedRange&&get('article-text')?.contains(savedRange.startContainer)){
      const selection=window.getSelection();
      selection.removeAllRanges();
      selection.addRange(savedRange);
    }
    get('share-message')?.focus();
  },
  jump:jumpTo,
  resolve:id=>changeState(id,true),
  reopen:id=>changeState(id,false),
  mention:addMention,
  notifications:()=>showDialog('notifications'),
  'open-notification':navigateThread,
  request:()=>{
    if(viewer==='none'&&!requests.some(item=>item.id==='none')){
      requests.push({id:'none',status:'pending'});
      renderDocument();
    }
  },
  'grant-request':id=>{
    const target=requests.find(item=>item.id===id);
    if(isOwner()&&target?.status==='pending'){target.status='approved';person(id).direct='read';renderDialog();}
  },
  'deny-request':id=>{
    const target=requests.find(item=>item.id===id);
    if(isOwner()&&target?.status==='pending'){target.status='denied';renderDialog();}
  },
  accept:()=>{
    const invite=invitations.find(item=>item.id===previewGuest&&item.status==='pending');
    if(viewer!=='guest'||!invite) return;
    invite.status='accepted';
    person(previewGuest).direct=invite.role;
    renderAll();
  },
  edit:()=>{if(canEdit()){editing=true;renderDocument();get('demo-page-text')?.focus();}},
  'cancel-edit':()=>{editing=false;renderDocument();},
  'save-text':()=>{
    if(!canEdit()||!editing) return;
    const updated=get('demo-page-text').value.trim();
    if(!updated) {alert('Введите текст страницы.');return;}
    paragraphs.splice(0,paragraphs.length,...updated.split(/\n\n+/u));
    for(const item of threads.filter(row=>row.quote)){
      item.orphan=paragraphs.filter(line=>line.includes(item.quote)).length!==1;
    }
    revision++;
    editing=false;
    renderAll();
    alert('Редакция сохранена.');
  },
};

document.addEventListener('click',event=>{
  const action=event.target.closest('button[data-action]');
  if(action) actions[action.dataset.action]?.(action.dataset.value);
});
document.addEventListener('change',event=>{
  if(event.target.id==='perspective') chooseViewer(event.target.value);
  else if(event.target.id==='people-filter'){personFilter=event.target.value;pageIndex=0;drawPeople();}
  else if(event.target.hasAttribute('data-role')&&isOwner()){
    person(event.target.dataset.role).direct=event.target.value;
    drawPeople();
    alert('Прямой доступ изменён.');
  }
});
document.addEventListener('input',event=>{
  if(event.target.id==='people-search'){
    search=event.target.value;
    pageIndex=0;
    drawPeople();
  } else if(event.target.id==='share-message'){
    drafts.set(draftsKey(event.target.form.dataset.thread),event.target.value);
    get('count-message').textContent=event.target.value.length+' / 500';
    drawMentions();
  }
});
document.addEventListener('submit',event=>{
  const form=event.target;
  if(!form.dataset.form) return;
  event.preventDefault();
  const values=new FormData(form);
  if(form.dataset.form==='access'&&isOwner()){
    const email=String(values.get('email')).trim().toLowerCase();
    const role=String(values.get('role'));
    if(!order.slice(0,3).includes(role)) return;
    let target=people.find(item=>item.email===email);
    if(inviteByEmail) {
      if(!target){target={id:crypto.randomUUID(),name:email.split('@')[0],email};people.push(target);}
      if(target.direct||target.inherited) {alert('У этого пользователя уже действует доступ.');return;}
      const existing=invitations.find(item=>item.id===target.id);
      if(existing&&existing.status==='pending') {alert('Приглашение уже ожидает подтверждения.');return;}
      if(existing){existing.role=role;existing.status='pending';}
      else invitations.push({id:target.id,email,role,status:'pending'});
      alert('Приглашение ожидает подтверждения.');
    } else {
      if(!target||target.id==='owner'||target.id==='guest') {alert('Выберите сотрудника организации или пригласите гостя.');return;}
      target.direct=role;
      alert('Доступ к странице выдан.');
    }
    search='';pageIndex=0;personFilter='all';
    renderDialog();
  }
  if(form.dataset.form==='comment'&&canDiscuss()){
    const value=String(values.get('message')).trim();
    const key=draftsKey(form.dataset.thread);
    if(!value||Array.from(value).length>500){alert('Введите от 1 до 500 символов.');return;}
    const mentionsInText=[...(mentions.get(key)||[])].filter(id=>value.includes('@'+person(id)?.name));
    if(mentionsInText.some(id=>!roleOf(person(id)))){alert('Уберите упоминание участника, у которого больше нет доступа.');return;}
    const message={id:crypto.randomUUID(),author:viewerId(),time:now(),text:value};
    const existing=thread(form.dataset.thread);
    if(form.dataset.thread){
      if(!existing||existing.resolved){alert('Откройте ветку снова перед отправкой.');return;}
      existing.replies.push(message);
    } else threads.unshift({...message,quote,replies:[]});
    for(const id of mentionsInText) notifications.push({thread:existing?.id||message.id,author:viewerId(),target:id});
    drafts.delete(key);
    mentions.delete(key);
    quote='';
    replyTo=undefined;
    renderAll();
    get('thread-'+(existing?.id||message.id))?.focus();
    alert('Комментарий отправлен.');
  }
});
modal.addEventListener('close',()=>{dialogMode=undefined;if(closeFocus?.isConnected) closeFocus.focus({preventScroll:true});});
get('mobile-inspector').addEventListener('cancel',event=>{event.preventDefault();panelOff();});
get('mobile-inspector').addEventListener('close',()=>{if(!hideMobile) panelOff();hideMobile=false;});
document.addEventListener('selectionchange',syncSelection);
document.addEventListener('pointerdown',event=>{
  if(event.target.closest('[data-action="quote"], [data-action="mention"]')) event.preventDefault();
});
document.addEventListener('keydown',event=>{
  if(event.key==='Escape'&&panel&&!modal.open&&!narrow.matches){event.preventDefault();panelOff();}
  const target=event.target.closest('[role=tablist] [role=tab]');
  if(!target||!['ArrowLeft','ArrowRight','Home','End'].includes(event.key)) return;
  const tabs=[...target.parentElement.querySelectorAll('[role=tab]')];
  const offset=tabs.indexOf(target);
  let next=event.key==='Home'?0:event.key==='End'?tabs.length-1:(offset+(event.key==='ArrowRight'?1:-1)+tabs.length)%tabs.length;
  event.preventDefault();
  if(target.dataset.action==='share-tab') openShareTab(tabs[next].dataset.value);
  else openTab(tabs[next].dataset.value);
});
narrow.addEventListener('change',renderPanel);
renderAll();
if(entry==='access') actions.share();
const threadId=location.hash.match(/^#thread-([a-f0-9-]+|scope|discussion|previous)$/u)?.[1];
if(threadId) navigateThread(threadId);
