/* Библиотека рабочего пространства. Хранилище демо изолировано от реестра фич. */
(() => {
  'use strict';

  const storagePrefix = 'sever.design.library.v1.';
  const ROOT = 'north';
  const folder = (key, parent, title, lead) => ({key, parent, title, lead, kind: 'folder'});
  const page = (key, parent, title, lead, body, owner = 'Анна Миронова', revision = 1) =>
    ({key, parent, title, lead, body, owner, revision, kind: 'page'});

  const seed = [
    folder(ROOT, '', 'Север', 'Общие документы команды, правила работы, шаблоны и знания проекта.'),
    page('welcome', ROOT, 'О пространстве', 'С чего начать в командной библиотеке «Север».',
      '## Первый день\nОткройте раздел «Онбординг», прочитайте условия приглашения и уточните у владельца, к каким материалам нужен доступ.\n## Как устроена библиотека\n- В «Онбординге» лежат правила доступа и состав команды.\n- В «Процессах команды» — рабочие инструкции и хэндоффы.\n- В «Базе знаний» — знания с проверенными источниками.'),
    folder('onboarding', ROOT, 'Онбординг', 'Маршрут нового участника от приглашения до первой рабочей задачи.'),
    folder('permissions', 'onboarding', 'Доступ и роли', 'Правила выдачи приглашений, выбора роли и ограничения области доступа.'),
    page('access', 'permissions', 'Доступ к пространству',
      'Как пригласить коллегу в «Север», выбрать область доступа и передать ему первые материалы.',
      '## Вход по приглашению\nВладелец выбирает раздел и роль до отправки ссылки. После входа участник видит доступные страницы и может открыть задачу, с которой началось приглашение.\n## Роли участников\n- **Читатель** знакомится с опубликованными материалами.\n- **Редактор** создаёт и изменяет документы внутри своего раздела.\n- **Владелец** управляет участниками, приглашениями и границами доступа.\n## Проверка доступа\nОткройте выбранный раздел от лица нового участника. Проверьте, что он видит приглашение и документ с правилами, а закрытые соседние разделы скрыты из поиска и графа.\n## Источник решения\nОбласть действия зафиксирована в политике «Граница раздела». Изменения роли связываются с задачей KB-014 и сохраняются в её истории.', 'Анна Миронова', 3),
    page('scope', 'permissions', 'Граница раздела', 'Какие материалы открываются вместе с разделом и где заканчиваются права участника.',
      '## Область приглашения\nУчастник получает доступ к выбранному разделу и разрешённым вложенным материалам. Дополнительный доступ к соседнему разделу согласует его владелец.\n## Повторная проверка\nПроверяйте права при открытии страницы, в поисковой выдаче и в карточке источника на графе.\n## Пример\nПриглашение в «Доступ и роли» открывает политику доступа и шаблон приглашения. Раздел процессов требует собственного разрешения.', 'Аналитик A-02', 2),
    page('template', 'permissions', 'Шаблон приглашения', 'Поля приглашения и порядок подготовки сообщения для нового участника.',
      '## Перед отправкой\n- Укажите получателя приглашения.\n- Выберите раздел и уровень доступа.\n- Приложите ссылку на задачу и первую страницу.\n## Сообщение участнику\nДобро пожаловать в рабочее пространство «Север». Первые материалы собраны в разделе онбординга. За вопросы о доступе отвечает владелец пространства.\n## После отправки\nЗафиксируйте автора приглашения, выбранный раздел и связанный документ.', 'Аналитик A-02'),
    page('people', 'onboarding', 'Команда и роли', 'К кому обратиться с вопросом о документах и шагах процесса.',
      '## Участники\nАнна согласует права и итог задачи. Аналитик готовит спецификацию и исходные материалы. Оркестратор ведёт назначение и передачу.\n## Запрос помощи\nПриложите ссылку на страницу, выбранный раздел и задачу, где возник вопрос.'),
    folder('operations', ROOT, 'Процессы команды', 'Схемы работ, маршруты согласования и передача результатов между исполнителями.'),
    page('workflow', 'operations', 'Приём участника', 'Как аналитик, владелец и оркестратор проводят задачу от подготовки до передачи.',
      '## Подготовка\nАналитик собирает требования, указывает область доступа и привязывает редакцию политики к задаче.\n## Согласование\nВладелец проверяет область доступа и либо подтверждает её, либо возвращает работу аналитику.\n## Передача\nОркестратор сохраняет решение, назначает следующего исполнителя и прикладывает хэндофф.'),
    folder('contracts', 'operations', 'Передача работы', 'Документы, с которыми очередной исполнитель начинает свой шаг.'),
    page('handoff', 'contracts', 'Хэндофф аналитика', 'Что получает владелец перед согласованием доступа.',
      '## Состав передачи\n- Редакция политики и ссылка на источник.\n- Карточка KB-014 и текущий исполнитель.\n- Открытые вопросы с вариантами решений.\n## Ожидаемый результат\nВладелец принимает решение об уровне доступа и при необходимости возвращает документ с комментарием.', 'Аналитик A-02'),
    folder('knowledge-base', ROOT, 'База знаний', 'Правила работы с подтверждёнными фактами и их источниками.'),
    page('knowledge', 'knowledge-base', 'Источники знаний', 'Как проследить знание до редакции документа и задачи.',
      '## Подтверждение\nУ каждого подтверждённого факта есть область применения, редакция источника и автор решения.\n## Связи\nСоседние узлы показывают документы, задачи и навыки, использующие факт. Для проверки откройте исходную редакцию.'),
    page('registry', 'knowledge-base', 'Реестр артефактов', 'Как классифицировать рабочие материалы.',
      '## Типы материалов\n- Документ описывает правило и хранит редакции.\n- Навык задаёт воспроизводимый способ работы.\n- Хэндофф передаёт контекст исполнителю.\n## Ревизии\nУказывайте точную редакцию и источник при закреплении артефакта в задаче.', 'Оркестратор O-01')
  ];
  const content = new Map(seed.map(item => [item.key, item]));
  function validRecord(item, key) {
    return item && item.key === key && item.key !== ROOT && ['folder','page'].includes(item.kind) &&
      typeof item.title === 'string' && typeof item.parent === 'string' && typeof item.lead === 'string' &&
      (item.kind === 'folder' || typeof item.body === 'string');
  }
  try {
    for (const storageKey of Object.keys(localStorage)) {
      if (!storageKey.startsWith(storagePrefix)) continue;
      const key = storageKey.slice(storagePrefix.length);
      try {
        const item = JSON.parse(localStorage.getItem(storageKey));
        if (validRecord(item, key)) content.set(key, item);
      } catch { /* A damaged local draft must not block the other pages. */ }
    }
  } catch { /* A storage-restricted browser can still explore the demo. */ }

  function persist(item) {
    let stored = true;
    try { localStorage.setItem(storagePrefix + item.key, JSON.stringify(item)); }
    catch { stored = false; }
    content.set(item.key, item);
    return stored;
  }
  const esc = value => String(value ?? '').replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
  function ancestors(key) {
    const chain = [], seen = new Set();
    while (key && content.has(key) && !seen.has(key)) {
      seen.add(key);
      const item = content.get(key);
      chain.unshift(item);
      key = item.parent;
    }
    return chain;
  }
  const childrenOf = key => Array.from(content.values()).filter(item => item.parent === key);
  function search(query = '') {
    const words = query.trim().toLocaleLowerCase('ru').split(/\s+/).filter(Boolean);
    return Array.from(content.values()).filter(item => item.key !== ROOT).map(item => {
      const path = ancestors(item.parent).map(part => part.title).join(' / ');
      const haystack = [item.title, path, item.lead, item.body].join(' ').toLocaleLowerCase('ru');
      if (!words.every(word => haystack.includes(word))) return null;
      const score = words.filter(word => item.title.toLocaleLowerCase('ru').includes(word)).length;
      return {key:item.key, title:item.title, kind:item.kind, path, score};
    }).filter(Boolean).sort((a,b) => b.score - a.score).map(({score, ...entry}) => entry);
  }
  const iconPaths = {
    book:'<path d="M4 5q4-1 8 2 4-3 8-2v13q-4-1-8 2-4-3-8-2z"/><path d="M12 8v11"/>',
    folder:'<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>',
    page:'<path d="M6 3h8l4 4v14H6z"/><path d="M14 3v5h4M9 12h6m-6 4h5"/>',
    tree:'<rect x="3" y="4" width="18" height="16" rx="3"/><path d="M9 4v16"/>',
    search:'<circle cx="10" cy="10" r="6"/><path d="m15 15 5 5"/>',
    right:'<path d="m9 6 6 6-6 6"/>', down:'<path d="m6 9 6 6 6-6"/>',
    add:'<path d="M12 5v14M5 12h14"/>', edit:'<path d="m14 5 5 5M5 15 16 4q1-1 2 0l2 2q1 1 0 2L9 19l-5 1z"/>',
    outward:'<path d="M6 18 18 6M7 6h11v11"/>',
    graph:'<circle cx="5" cy="12" r="2"/><circle cx="17" cy="5" r="2"/><circle cx="18" cy="18" r="2"/><path d="m7 11 8-5M7 13l9 4m1-10 1 9"/>',
    check:'<path d="m5 12 4 4 10-10"/>', close:'<path d="m5 5 14 14m0-14L5 19"/>',
    flow:'<rect x="3" y="9" width="5" height="5" rx="1"/><rect x="16" y="3" width="5" height="5" rx="1"/><rect x="16" y="16" width="5" height="5" rx="1"/><path d="M8 11h4V5h4m-4 6v7h4"/>',
    task:'<rect x="4" y="4" width="16" height="16" rx="4"/><path d="m8 12 3 3 5-6"/>'
  };
  const icon = (name,size=17) => '<svg class="lib-icon" width="'+size+'" height="'+size+'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">'+iconPaths[name]+'</svg>';
  function control(action, text, cls='', value='', attrs='') {
    return '<button type="button" class="'+cls+'" data-lib="'+action+'" data-key="'+esc(value)+'" '+attrs+'>'+text+'</button>';
  }

  function parseBlocks(input) {
    const result = [], lines = input.split(/\r?\n/);
    let paragraph = [], list = [];
    const flushParagraph = () => { if (paragraph.length) {result.push({type:'p', text:paragraph.join(' ')}); paragraph = [];} };
    const flushList = () => { if (list.length) {result.push({type:'list', items:list}); list = [];} };
    for (const line of lines) {
      const title = /^#{1,6}\s+(.+)/.exec(line), bullet = /^[-*]\s+(.+)/.exec(line);
      if (title) {flushParagraph();flushList();result.push({type:'heading', text:title[1]});}
      else if (bullet) {flushParagraph();list.push(bullet[1]);}
      else if (!line.trim()) {flushParagraph();flushList();}
      else {flushList();paragraph.push(line.trim());}
    }
    flushParagraph();flushList();
    return result;
  }
  const inline = text => esc(text).replace(/\*\*([^*]+)\*\*/g,'<strong>$1</strong>').replace(/`([^`]+)`/g,'<code>$1</code>');
  function markdown(blocks) {
    let heading = 0;
    return blocks.map(b => {
      if (b.type === 'heading') return '<h2 id="lib-section-'+(heading++)+'">'+esc(b.text)+'</h2>';
      if (b.type === 'list') return '<ul>'+b.items.map(item => '<li>'+inline(item)+'</li>').join('')+'</ul>';
      return '<p>'+inline(b.text)+'</p>';
    }).join('');
  }
  function accessIllustration() {
    return '<figure class="lib-schematic"><figcaption><span>'+icon('tree',16)+' Область приглашения</span><strong>Раздел и его страницы</strong><small>Доступ наследуется внутри выбранной ветви.</small></figcaption>'+
      '<svg viewBox="0 0 270 119" role="img" aria-label="Одна ветвь пространства открыта, соседняя отделена границей"><defs><pattern id="lib-dots" width="8" height="8" patternUnits="userSpaceOnUse"><circle cx="1" cy="1" r=".7" fill="#9fbaae"/></pattern></defs><rect x="0" width="270" height="119" fill="url(#lib-dots)" opacity=".32"/>'+
      '<g fill="none" stroke-linejoin="round"><path d="M28 62 88 30l60 32-60 32z" fill="#b9c6b9" stroke="#4b6c5b"/><path d="m28 62 60 32v8L28 70z" fill="#8eaa94" stroke="#4b6c5b"/><path d="m88 94 60-32v8l-60 32z" fill="#6f8b76" stroke="#4b6c5b"/>'+
      '<path d="m55 52 33-18 33 18-33 18z" stroke="#3f6853" fill="#e0eadd"/><path d="m55 52 33 18v7L55 59z" fill="#a8bfa6" stroke="#3f6853"/><path d="m88 70 33-18v7L88 77z" fill="#8da98f" stroke="#3f6853"/>'+
      '<path d="m62 42 26-14 26 14-26 14z" fill="#eef3e7" stroke="#41634c"/><path d="m88 28 0-9m36 30 17-10h22" stroke="#476a52"/><circle cx="166" cy="39" r="3" fill="#476a52"/>'+
      '<path d="m158 76 35-19 35 19-35 19z M158 76v6l35 19 35-19v-6m-35 19v6" stroke="#a6b6a6" stroke-dasharray="3 3"/><path d="m144 97 29-16" stroke="#738879" stroke-dasharray="2 4"/></g></svg></figure>';
  }
  let activeCreate = null;
  let lastOpened = 'access';
  function mount(host, ctx) {
    const initialKey = ctx.key;
    let current = content.has(initialKey) ? initialKey : initialKey?.startsWith('new-') ? lastOpened : 'access';
    lastOpened = current;
    const expanded = new Set([ROOT,'onboarding','permissions', ...ancestors(current).map(item=>item.key)]);
    let drawer = false, collapsed = false, query = '', editing = false, notice = '';
    let formKind = null, formParent = null, editDraft = null;
    const abort = new AbortController(), listen = {signal:abort.signal};
    host.innerHTML = '<div class="lib-scene"><div class="lib-layout"></div><dialog class="lib-dialog" aria-labelledby="lib-dialog-title"></dialog></div>';
    const scene = host.querySelector('.lib-scene'), layout = scene.querySelector('.lib-layout'), dialog = scene.querySelector('dialog');
    function currentFolder() {const item = content.get(current);return item.kind==='folder'?item.key:item.parent;}
    function treeItems(parent, depth=0) {
      return '<ul class="lib-level">'+childrenOf(parent).map(it => {
        const isFolder = it.kind==='folder', open=expanded.has(it.key), isCurrent=it.key===current;
        const caret = isFolder ? control('expand',icon(open?'down':'right',12),'lib-caret',it.key,'aria-expanded="'+open+'" aria-label="'+(open?'Свернуть ':'Развернуть ')+esc(it.title)+'"') : '<span class="lib-caret lib-spacer"></span>';
        return '<li><div class="lib-row '+(isCurrent?'lib-selected':'')+'" style="--depth:'+depth+'">'+caret+
          control('open',icon(it.kind)+'<span>'+esc(it.title)+'</span>','lib-entry',it.key,isCurrent?'aria-current="page"':'')+'</div>'+
          (isFolder && open ? treeItems(it.key, depth+1) : '')+'</li>';
      }).join('')+'</ul>';
    }
    function searchRows() {
      const results = search(query);
      return results.length ? '<ul class="lib-search-results">'+results.map(it => '<li>'+control('open',icon(it.kind)+'<span>'+esc(it.title)+'<small>'+esc(it.path)+'</small></span>','lib-hit',it.key)+'</li>').join('')+'</ul>' : '<p class="lib-empty">По этому запросу страниц и разделов нет.</p>';
    }
    function sidebar() {
      return '<aside class="lib-sidebar" aria-label="Дерево пространства"><div class="lib-sidebar-header"><span class="lib-space-icon" aria-hidden="true">'+icon('book',20)+'</span><div><small>Пространство</small><strong>Север</strong></div>'+control('collapse',icon('tree',17),'lib-icon-button','','aria-label="Свернуть дерево" title="Свернуть дерево"')+'</div>'+
        '<label class="lib-search">'+icon('search',15)+'<input type="search" aria-label="Поиск в пространстве" placeholder="Найти страницу" value="'+esc(query)+'"></label>'+
        '<div class="lib-tree-title"><span>Содержание</span>'+control('create-page',icon('add',15),'lib-icon-button','','title="Создать страницу" aria-label="Создать страницу"')+'</div>'+
        '<nav class="lib-tree" aria-label="Страницы и разделы">'+(query.trim()?searchRows():treeItems(ROOT))+'</nav>'+
        '<div class="lib-sidebar-bottom">'+control('create-folder',icon('folder',15)+' Новый раздел','lib-soft-button')+'</div></aside>';
    }
    function toolbar(item) {
      const crumbs = ancestors(current);
      return '<div class="lib-toolbar">'+control('tree',icon('tree',17),'lib-icon-button lib-tree-switch','','aria-label="Открыть дерево" title="Открыть дерево"')+
        '<nav class="lib-breadcrumb" aria-label="Путь страницы">'+crumbs.map((part,i)=>i===crumbs.length-1?'<span aria-current="page">'+esc(part.title)+'</span>':control('open',esc(part.title),'lib-crumb',part.key)+icon('right',11)).join('')+'</nav>'+
        (item.kind==='page'?control('edit',icon('edit',15)+'<span>Изменить</span>','lib-edit'):'')+'</div>';
    }
    function folderView(item) {
      const kids = childrenOf(item.key);
      return '<div class="lib-folder-view"><span class="lib-section-stamp">'+icon('folder',24)+'</span><h1>'+esc(item.title)+'</h1><p class="lib-lead">'+esc(item.lead)+'</p><div class="lib-folder-actions">'+
        control('create-page',icon('add',15)+' Страница','lib-button')+control('create-folder',icon('folder',15)+' Раздел','lib-quiet-button')+'</div><h2>В этом разделе</h2>'+
        (kids.length?'<div class="lib-directory">'+kids.map(k=>control('open',icon(k.kind,20)+'<span><strong>'+esc(k.title)+'</strong><small>'+esc(k.lead)+'</small></span>'+icon('right',16),'lib-directory-entry',k.key)).join('')+'</div>':'<p>Здесь пока нет материалов.</p>')+'</div>';
    }
    function pageView(item) {
      const blocks = parseBlocks(item.body), headings = blocks.filter(b=>b.type==='heading');
      const related = childrenOf(item.parent).filter(it=>it.kind==='page'&&it.key!==item.key).slice(0,2);
      const graphKey = {access:'access',scope:'scope',template:'template',handoff:'handoff'}[item.key];
      const linkedTasks = ctx.tasksForPage(item.key);
      return '<div class="lib-reader"><article class="lib-paper"><div class="lib-kind">'+icon('page',15)+' Страница <span></span> Редакция '+esc(item.revision||1)+'</div><h1>'+esc(item.title)+'</h1><p class="lib-lead">'+esc(item.lead)+'</p>'+
        '<div class="lib-author"><span aria-hidden="true">'+esc((item.owner||'Автор').split(' ').map(s=>s[0]).slice(0,2).join(''))+'</span><b>'+esc(item.owner||'Автор')+'</b><i></i><small>'+esc(item.changed||'18 ноября 2026')+'</small></div>'+
        (item.key==='access'?accessIllustration():'')+'<div class="lib-prose">'+markdown(blocks)+'</div>'+
        (related.length?'<div class="lib-related"><h2>Рядом в разделе</h2>'+related.map(it=>control('open',icon('page',16)+' '+esc(it.title)+' '+icon('outward',13),'lib-related-link',it.key)).join('')+'</div>':'')+'</article>'+
        '<aside class="lib-context" aria-label="Контекст страницы">'+(headings.length?'<section><h2>На странице</h2><nav aria-label="Оглавление">'+headings.map((h,i)=>control('heading',esc(h.text),'lib-toc-link',String(i))).join('')+'</nav></section>':'')+
        (linkedTasks.length||graphKey||item.key==='workflow'?'<section><h2>Связано со страницей</h2>'+
          linkedTasks.map(task=>control('task',icon('task',16)+'<span><small>'+esc(task.id)+'</small>'+esc(task.title)+'<small data-lib-task-status>'+esc(task.status)+'</small></span>'+icon('outward',12),'lib-context-item',task.id)).join('')+
          (graphKey?control('graph',icon('graph',16)+'<span>Узел в графе</span>'+icon('outward',12),'lib-context-item',graphKey):'')+
          (item.key==='workflow'?control('flow',icon('flow',16)+'<span>Холст процесса</span>'+icon('outward',12),'lib-context-item'):'')+'</section>':'')+'</aside></div>';
    }
    function editor(item) {
      return '<form class="lib-editor"><div class="lib-editor-top"><h1>Редактирование</h1>'+control('cancel-edit','Отмена','lib-quiet-button')+'<button class="lib-button">Сохранить</button></div>'+
        '<label>Название<input name="title" required maxlength="140" value="'+esc(item.title)+'"></label><label>Краткое описание<textarea name="lead" rows="2">'+esc(item.lead)+'</textarea></label>'+
        '<label>Текст Markdown<textarea name="body" rows="15">'+esc(item.body)+'</textarea></label></form>';
    }
    function render() {
      const item = content.get(current);
      scene.classList.toggle('lib-drawer-open',drawer);scene.classList.toggle('lib-collapsed',collapsed);
      layout.innerHTML = sidebar()+control('dismiss-drawer','','lib-scrim','','aria-label="Закрыть дерево"')+
        '<div class="lib-main">'+toolbar(item)+(notice?'<div class="lib-notice" role="status">'+esc(notice)+'</div>':'')+
        '<div class="lib-scroll">'+(editing?editor(editDraft||item):item.kind==='folder'?folderView(item):pageView(item))+'</div></div>';
    }
    function open(key) {
      if (!content.has(key)) return;
      current=key;lastOpened=key;editing=false;editDraft=null;drawer=false;query='';
      ancestors(key).forEach(it=>expanded.add(it.key));
      ctx.navigateKey(key);render();
    }
    function create(kind='page', parentId=currentFolder()) {
      formKind = kind==='folder'?'folder':'page';
      const parent = content.get(parentId);
      formParent = parent?.kind==='folder'?parent.key:currentFolder();
      const title = formKind==='folder'?'Новый раздел':'Новая страница';
      const options = Array.from(content.values()).filter(it=>it.kind==='folder').map(it=>'<option value="'+esc(it.key)+'" '+(it.key===formParent?'selected':'')+'>'+esc(ancestors(it.key).map(p=>p.title).join(' / '))+'</option>').join('');
      dialog.innerHTML='<form class="lib-create-form"><div class="lib-dialog-head"><h2 id="lib-dialog-title">'+title+'</h2>'+control('cancel-create',icon('close',18),'lib-icon-button','','aria-label="Закрыть"')+'</div>'+
        '<label>Название<input name="title" placeholder="'+(formKind==='folder'?'Название раздела':'Название страницы')+'" required maxlength="140" autocomplete="off"></label><label>Родительский раздел<select name="parent">'+options+'</select></label><div class="lib-dialog-actions">'+control('cancel-create','Отмена','lib-quiet-button')+'<button class="lib-button">Создать</button></div></form>';
      if (!dialog.open) dialog.showModal();
      dialog.querySelector('input').focus();
    }
    function saveNotice(stored) {return stored?'':'Изменение доступно в этой вкладке. Браузер ограничил локальное сохранение.';}
    scene.addEventListener('click', e=>{
      const target = e.target.closest('[data-lib]');if(!target)return;
      const key = target.dataset.key, action=target.dataset.lib;
      switch(action){
        case 'expand': if(expanded.has(key))expanded.delete(key);else expanded.add(key); render();scene.querySelector('[data-lib="expand"][data-key="'+CSS.escape(key)+'"]')?.focus(); break;
        case 'open': notice='';open(key);break;
        case 'tree': if (matchMedia('(max-width: 860px)').matches) drawer=!drawer;else collapsed=!collapsed;render();break;
        case 'collapse': drawer=false;collapsed=true;render();break;
        case 'dismiss-drawer':drawer=false;render();break;
        case 'create-page':create('page');break;
        case 'create-folder':create('folder');break;
        case 'cancel-create':dialog.close();break;
        case 'edit':editDraft=null;editing=true;notice='';render();scene.querySelector('.lib-editor input')?.focus();break;
        case 'cancel-edit':editing=false;render();break;
        case 'heading':scene.querySelector('#lib-section-'+Number(key))?.scrollIntoView({behavior:matchMedia('(prefers-reduced-motion: reduce)').matches?'instant':'smooth',block:'start'});break;
        case 'task':ctx.openTask(key);break;
        case 'graph':ctx.go('graph',key);break;
        case 'flow':ctx.go('flow');break;
      }
    },listen);
    scene.addEventListener('input',e=>{
      const form=e.target.closest('.lib-editor');
      if (form) {
        const fields = new FormData(form);
        editDraft = {title:String(fields.get('title')||''),lead:String(fields.get('lead')||''),body:String(fields.get('body')||'')};
        return;
      }
      if (!e.target.matches('.lib-search input')) return;
      query=e.target.value;scene.querySelector('.lib-tree').innerHTML=query.trim()?searchRows():treeItems(ROOT);
    },listen);
    scene.addEventListener('submit',e=>{
      e.preventDefault();
      const form=e.target,data=new FormData(form);
      const title=String(data.get('title')||'').trim();
      if(!title){const input=form.elements.title;input.setCustomValidity('Введите название');input.reportValidity();input.addEventListener('input',()=>input.setCustomValidity(''),{once:true});return;}
      if(form.matches('.lib-editor')){
        const old=content.get(current);
        const item={...old,title,lead:String(data.get('lead')||'').trim(),body:String(data.get('body')||''),revision:(Number(old.revision)||1)+1,changed:new Date().toLocaleDateString('ru-RU',{day:'numeric',month:'long',year:'numeric'})};
        notice=saveNotice(persist(item));editing=false;render();return;
      }
      if(!form.matches('.lib-create-form'))return;
      const parent=String(data.get('parent'));if(content.get(parent)?.kind!=='folder')return;
      const key=(formKind==='folder'?'section-':'page-')+crypto.randomUUID();
      const item=formKind==='folder'?folder(key,parent,title,''):page(key,parent,title,'','');
      notice=saveNotice(persist(item));dialog.close();open(key);
      if(formKind==='page'){editing=true;render();scene.querySelector('[name="body"]')?.focus();}
    },listen);
    scene.addEventListener('keydown',e=>{if(e.key==='Escape'&&drawer&&!dialog.open){drawer=false;render();scene.querySelector('.lib-tree-switch')?.focus();}}, listen);
    ctx.onTaskChange(task=>{
      const status=scene.querySelector('[data-lib="task"][data-key="'+CSS.escape(task.id)+'"] [data-lib-task-status]');
      if(status)status.textContent=task.status;
    },abort.signal);
    activeCreate=create;render();
    if(initialKey==='new-page'||initialKey==='new-folder'){
      ctx.navigateKey(current);
      create(initialKey==='new-folder'?'folder':'page');
    }
    return ()=>{abort.abort();if(dialog.open)dialog.close();if(activeCreate===create)activeCreate=null;};
  }
  function metadata(key) {
    const item=content.get(key);
    return item && {key:item.key,title:item.title,kind:item.kind};
  }
  window.KBLibrary = {mount, search, metadata, create(kind,parentId){if(activeCreate)activeCreate(kind,parentId);}};
})();
