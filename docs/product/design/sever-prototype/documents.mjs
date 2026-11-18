import {escapeHtml as html, icon, button, avatar, entity} from './ui.mjs';
import {accessFor, canComment, canList, canWrite, coverFor, coverStyles, isCoverImage, memberRole, ownsSpace, pageTrail, profileFor, spaceInfo, taskStatuses} from './model.mjs';
import {coverImage} from './media.mjs';
import {videoBlock} from './commands.mjs';

function inline(text) {
  const parts = /(`([^`]+)`|\[([^\]]+)\]\((https?:\/\/[^ )]+)\)|\*+)/g;
  const stack = [{width:0,markup:''}];
  const append = markup => { stack.at(-1).markup += markup; };
  let cursor = 0;
  for (const match of text.matchAll(parts)) {
    append(html(text.slice(cursor,match.index)));
    const [, whole, code, label, url] = match;
    cursor = match.index + whole.length;
    if (code) append(`<code>${html(code)}</code>`);
    else if (url) append(`<a href="${html(url)}" target="_blank" rel="noopener noreferrer">${inline(label)}</a>`);
    else {
      const closes = match.index > 0 && !/\s/.test(text[match.index-1]);
      const opens = cursor < text.length && !/\s/.test(text[cursor]);
      let count = whole.length;
      while (closes && stack.length > 1 && count > 0) {
        const bracket = stack.at(-1);
        if (bracket.width === 3 && count < 3) {
          // A triple opener starts both marks. The first closing run identifies the inner mark.
          const tag = count === 2 ? 'strong' : 'em';
          bracket.markup = '<'+tag+'>'+bracket.markup+'</'+tag+'>';
          bracket.width -= count;
          count = 0;
          continue;
        }
        if (count < bracket.width) break;
        const {width,markup} = stack.pop(), tag = width === 2 ? 'strong' : 'em';
        append(width === 3 ? '<strong><em>'+markup+'</em></strong>' : '<'+tag+'>'+markup+'</'+tag+'>');
        count -= width;
      }
      while (opens && count > 0 && stack.length < 9) {
        const width = count === 3 ? 3 : Math.min(2,count);
        stack.push({width,markup:''});
        count -= width;
      }
      append('*'.repeat(count));
    }
  }
  append(html(text.slice(cursor)));
  while (stack.length > 1) {
    const {width,markup} = stack.pop();
    append('*'.repeat(width)+markup);
  }
  return stack[0].markup;
}

export function markdown(source,options) {
  const blocks = [], headings = [];
  let currentList = null, code = null;
  const finishList = () => {
    if (!currentList) return;
    const {kind,start,items} = currentList;
    blocks.push(`<${kind}${kind === 'ol' ? ' start="'+start+'"' : ''}>${items.join('')}</${kind}>`);
    currentList = null;
  };
  const finishCode = () => {
    blocks.push(`<pre data-md-fence="${code.width}" data-md-info="${html(code.info)}"><code>${html(code.lines.join('\n'))}</code></pre>`);
    code = null;
  };
  for (const line of source.split(/\r?\n/)) {
    const fence = /^(`{3,})([^`]*)$/.exec(line);
    if (code) {
      if (fence && fence[1].length >= code.width && !fence[2].trim()) finishCode();
      else code.lines.push(line);
      continue;
    }
    if (fence) {
      finishList();
      code = {width:fence[1].length,info:fence[2],lines:[]};
      continue;
    }
    if (!line.trim()) continue;
    const item = /^\s*(- |\d{1,9}\. )(.+)$/.exec(line);
    if (item) {
      const kind = item[1] === '- ' ? 'ul' : 'ol';
      if (currentList?.kind !== kind) { finishList(); currentList = {kind,start:Number.parseInt(item[1],10),items:[]}; }
      currentList.items.push(`<li>${inline(item[2])}</li>`);
      continue;
    }
    finishList();
    const heading = /^(#{1,3})\s+(.+)$/.exec(line);
    const video = /^!\[Видео\]\((.+)\)$/.exec(line);
    const embedded = video ? videoBlock(video[1],options) : '';
    if (heading) {
      const rank = heading[1].length + 1;
      const id = 'heading-' + headings.length;
      headings.push({id,text:heading[2],rank});
      blocks.push(`<h${rank} id="${id}" data-md-heading="${heading[1].length}">${inline(heading[2])}</h${rank}>`);
    } else if (embedded) {
      blocks.push(embedded);
    } else if (/^>\s?/.test(line)) {
      blocks.push(`<blockquote>${inline(line.replace(/^>\s?/,''))}</blockquote>`);
    } else {
      blocks.push(`<p>${inline(line)}</p>`);
    }
  }
  finishList();
  if (code) finishCode();
  return {markup:blocks.join(''), headings};
}

function sideAction(action, text, glyph, detail = '', value = '') {
  return `<button class="side-action" type="button" data-action="${action}" data-value="${html(value)}">${icon(glyph,15)}<span>${text}</span>${detail ? `<span class="push mono">${detail}</span>` : ''}</button>`;
}

function sidebar(data, doc, view) {
  const space = spaceInfo(data);
  function branch(parent = '') {
    return data.documents.filter(item => item.parent === parent && item.space === data.space && canList(data,item)).map(item => {
      const isFolder = item.kind === 'folder';
      const opened = view.expanded.has(item.id);
      const name = accessFor(data,item) === 'none' ? 'Доступные страницы' : item.title;
      const active = doc?.id === item.id;
      const pageIcon = item.id === 'access' ? 'shield' : 'doc';
      return `<button type="button" class="tree-item" data-action="${isFolder ? 'tree-folder' : 'open-doc'}" data-value="${html(item.id)}" ${isFolder ? `aria-expanded="${opened}"` : ''} ${active ? 'aria-current="page"' : ''}>
        <span class="tree-caret">${isFolder ? icon('chevron',12) : ''}</span>${icon(isFolder ? 'folder' : pageIcon,15)}<span>${html(name)}</span></button>
        ${isFolder && opened ? `<div class="tree-level">${branch(item.id)}</div>` : ''}`;
    }).join('');
  }
  return `<aside class="sidebar" aria-label="Дерево пространства">
    <button class="space-card" type="button" data-action="spaces"><span class="space-emblem">${icon(space.glyph,20)}</span><span class="grow"><strong>${html(space.name)}</strong><small class="muted">${html(space.description)}</small></span>${icon('down',14)}</button>
    <div class="tree-menu">${sideAction('search','Найти в пространстве','search','⌘ K')}${sideAction('open-doc','Главная','home','',data.space === 'north' ? 'welcome' : 'lab-notes')}</div>
    <div class="sidebar-heading"><span>Документы</span>${ownsSpace(data) || memberRole(data,data.space) === 'edit' ? button('create-doc','','plus',{className:'ghost',title:'Создать страницу или раздел'}) : ''}</div>
    <nav class="tree-scroll" aria-label="Разделы и страницы">${branch() || '<p class="muted small">Нет доступных страниц.</p>'}</nav>
    <div class="side-foot">${sideAction('account','Учётная запись','user')}${sideAction('settings','Настройки пространства','settings')}</div>
  </aside>`;
}

function pageBar(data, doc, view) {
  const trail = doc ? pageTrail(data,doc).filter(parent => accessFor(data,parent) !== 'none') : [];
  const breadcrumbs = trail.map(parent =>
    `<span class="bread-parent">/</span><button class="bread-parent" type="button" data-action="open-doc" data-value="${html(parent.id)}">${html(parent.title)}</button>`
  ).join('');
  const available = doc && accessFor(data,doc) !== 'none';
  const editor = available && doc.kind === 'page' && canWrite(data,doc) && !view.editing;
  const canShare = available && ownsSpace(data,doc.space);
  return `<div class="doc-bar">
    <div class="breadcrumbs">
      ${button('tree-panel','','side',{className:'ghost drawer-tree',title:'Показать дерево',pressed:view.tree})}
      <button type="button" data-action="spaces">${html(spaceInfo(data).name)}</button>${breadcrumbs}
      ${available ? `<span>/</span><strong>${html(doc.title)}</strong>` : ''}
    </div>
    <div class="bar-actions">
      ${available && doc.kind === 'page' ? `<div class="page-presence" data-presence aria-label="Участники на странице"></div>
        <a class="btn ghost icon collab-link" data-open-collaboration href="?collaborate=1#doc/${encodeURIComponent(doc.id)}" target="_blank" rel="noopener" title="Открыть эту страницу в другой вкладке с выбором профиля" aria-label="Открыть для совместной работы в другой вкладке">${icon('user',15)}</a>` : ''}
      ${editor ? '<button type="button" class="btn ghost" data-action="edit" aria-label="Редактировать">'+icon('pen')+'<span class="bar-label">Редактировать</span></button>' : ''}
      ${canShare ? '<button type="button" class="btn primary" data-action="share" aria-label="Поделиться">'+icon('share')+'<span class="bar-label">Поделиться</span></button>' : ''}
      ${available && doc.kind === 'page' ? button('context-panel','','panel',{className:'ghost',title:'Контекст страницы',pressed:view.context}) : ''}
    </div>
  </div>`;
}

export function coverArt(style,image = '') {
  if (isCoverImage(style) && image) return `<img class="cover-photo" src="${html(image)}" alt="" aria-hidden="true">`;
  if (style === 'orbit') return `<div class="cover-graphic" aria-hidden="true"><i class="cover-ring"></i><i class="cover-ring"></i><i class="cover-ring"></i><span class="cover-orb">${icon('lock',40)}</span><i class="satellite one"></i><i class="satellite two"></i><i class="satellite three"></i></div>`;
  if (style === 'aurora' || style === 'grid') return `<div class="cover-graphic cover-art" aria-hidden="true"><i></i><i></i><i></i></div>`;
  return '';
}

function cover(doc,images) {
  const style = coverFor(doc);
  const glyph = doc.id === 'access' ? 'shield' : 'doc';
  if (style === 'none') return `<header class="section-page"><span class="entity-icon">${icon(glyph,17)}</span><h1>${html(doc.title)}</h1>${doc.intro ? `<p class="muted">${html(doc.intro)}</p>` : ''}</header>`;
  return `<header class="cover" data-cover="${isCoverImage(style) ? 'custom' : style}">
    <div class="cover-copy"><span class="cover-icon">${icon(glyph,19)}</span><h1>${html(doc.title)}</h1>${doc.intro ? `<p>${html(doc.intro)}</p>` : ''}</div>
    ${coverArt(style,coverImage(images,style))}
  </header>`;
}

function links(data, doc) {
  const linked = data.tasks.filter(task => task.sourceKey === doc.id);
  return linked.map(task => entity('task',task.id,'board',task.title,task.id+' · '+taskStatuses[task.status],'work')).join('') +
    (doc.graph ? entity('open-graph',doc.graph,'graph','Узел в графе','Связи и источник','knowledge') : '') +
    (doc.id === 'access' || doc.id === 'workflow' ? entity('open-flow','','flow','Проверка приглашения','AI-подход','agent') : '');
}

function sectionContents(data, doc) {
  const children = data.documents.filter(item => item.parent === doc.id && canList(data,item));
  return `<header class="section-page"><span class="entity-icon">${icon('folder',17)}</span><h1>${html(doc.title)}</h1>${doc.intro ? `<p class="muted">${html(doc.intro)}</p>` : ''}</header>
    ${children.length ? `<div class="doc-links">${children.map(item => entity('open-doc',item.id,item.kind === 'folder' ? 'folder' : 'doc',accessFor(data,item) === 'none' ? 'Доступные страницы' : item.title,item.kind === 'folder' ? 'Раздел' : 'Страница')).join('')}</div>` : '<p class="empty-page muted">Этот раздел пока пуст.</p>'}
    <div class="empty-page">${canWrite(data,doc) ? button('create-page','Добавить страницу','plus',{className:'primary',value:doc.id}) : ''}</div>`;
}

export function editorCover(style,image) {
  const custom = isCoverImage(style);
  return style === 'none' ? button('covers','Добавить обложку','plus',{className:'ghost'}) :
    `<div class="cover editor-cover" data-cover="${custom ? 'custom' : style}">
      ${coverArt(style,image)}
      <span class="cover-caption">${custom ? 'Своя обложка' : coverStyles[style].name}</span>
      ${button('covers','Изменить обложку','pen')}
    </div>`;
}

function editor(doc,images) {
  const style = coverFor(doc);
  const rendered = markdown(doc.body,{editing:true});
  return `<form class="editor-form" data-form="page" data-page-id="${html(doc.id)}">
    <input type="hidden" name="cover" value="${style}">
    <div class="editor-cover-slot">${editorCover(style,coverImage(images,style))}</div>
    <label class="sr-only" for="editor-title">Название страницы</label>
    <input class="editor-title" id="editor-title" name="title" value="${html(doc.title)}" required maxlength="120">
    <div class="editor-toolbar" aria-label="Форматирование Markdown">
      ${button('format','Заголовок',null,{value:'heading',className:'ghost'})}
      ${button('format','Жирный',null,{value:'bold',className:'ghost'})}
      ${button('format','Курсив',null,{value:'italic',className:'ghost'})}
      ${button('format','Список','list',{value:'list',className:'ghost'})}
      ${button('format','Код','code',{value:'code',className:'ghost'})}
      <span class="spacer"></span>${button('source-mode','Исходник','code',{className:'ghost',pressed:false})}
    </div>
    <textarea class="editor-source" name="body" aria-label="Исходный Markdown" aria-describedby="editor-status" spellcheck="true" hidden>${html(doc.body)}</textarea>
    <div class="editor-visual article-body" data-rich-editor contenteditable="true" role="textbox" aria-multiline="true" aria-label="Содержимое страницы" aria-describedby="editor-status" spellcheck="true">${rendered.markup || '<p><br></p>'}</div>
    <div class="editor-commands" data-command-menu role="listbox" aria-label="Вставить блок" hidden></div>
    <p class="small muted" data-editor-remote role="status" hidden></p>
    <section class="editor-conflict" data-editor-conflict aria-label="Конфликт изменений" hidden></section>
    <p id="editor-status" class="form-status" role="status" hidden></p>
    <div class="edit-footer">${button('cancel-edit','Отменить',null,{className:'ghost'})}${button('','Сохранить','check',{className:'primary',submit:true})}</div>
  </form>`;
}

function comments(data,doc) {
  const notes = data.comments[doc.id] || [];
  return `<div class="context-section"><h2>Комментарии</h2>
    ${notes.length ? notes.map(note => {
      const author = profileFor(data,note.person);
      return `<div class="office-note"><span class="page-author">${avatar(author,true)}<strong>${html(author.name)}</strong><span>${html(note.date)}</span></span><p>${html(note.text)}</p></div>`;
    }).join('') : '<p class="small muted">Пока нет комментариев.</p>'}
    ${canComment(data,doc) ? `<form data-form="comment"><label class="dialog-field"><span>Новый комментарий</span><textarea class="field" name="comment" required maxlength="500"></textarea></label><div class="dialog-buttons">${button('','Отправить','arrow',{submit:true})}</div></form>` : ''}</div>`;
}

function contextPanel(data,doc,view,headings) {
  const tab = (key,label) => button('context-tab',label,null,{value:key,className:'ghost',pressed:view.contextTab === key});
  const outline = headings.length ? `<section class="context-section"><div class="context-caption"><h2>Оглавление</h2><span class="mono muted">${headings.length}</span></div><nav class="outline" aria-label="Оглавление">${headings.map((heading,index) => `<button type="button" data-action="heading" data-value="${heading.id}" ${index === 0 ? 'aria-current="true"' : ''}>${html(heading.text)}</button>`).join('')}</nav></section>` : '';
  const relations = links(data,doc);
  const about = `${outline}${relations ? `<section class="context-section"><h2>Связано со страницей</h2>${relations}</section>` : ''}
    <section class="context-section"><h2>Доступ</h2><div class="office-note">${icon('lock',13)} ${data.permissions[doc.id]?.link ? 'По ссылке доступно чтение' : 'Доступ по приглашению'}</div></section>`;
  const history = `<section class="context-section"><h2>Редакции страницы</h2>
    <div class="office-note">Редакция ${doc.revision} · Текущая</div>
    ${(data.revisions[doc.id] || []).map(revision => entity('revision',revision.revision,'clock','Редакция '+revision.revision,'Открыть Markdown')).join('')}</section>`;
  return `<aside class="context" aria-label="Контекст страницы">
    <div class="context-head"><strong>Страница</strong>${button('context-panel','','close',{title:'Скрыть контекст',className:'ghost'})}</div>
    <div class="context-tabs">${tab('about','Контекст')}${tab('comments','Комментарии')}${tab('history','История')}</div>
    ${view.contextTab === 'comments' ? comments(data,doc) : view.contextTab === 'history' ? history : about}
  </aside>`;
}

export function renderDocuments(data, doc, view) {
  const available = doc && accessFor(data,doc) !== 'none';
  const reading = available && doc.kind === 'page';
  const rendered = reading ? markdown(doc.body) : {markup:'',headings:[]};
  let content = '<div class="empty-page"><span class="entity-icon">'+icon('lock')+'</span><h1>Страница недоступна</h1><p class="muted">Попросите владельца выдать доступ к этому материалу.</p>'+button('account','Сменить профиль','user')+'</div>';
  if (reading) {
    const author = profileFor(data,doc.author);
    content = view.editing && canWrite(data,doc) ? editor(doc,data.coverImages) : `${cover(doc,data.coverImages)}
      <div class="page-meta"><span class="page-author">${avatar(author,true)} ${html(author.name)}</span>
      <span class="tag">Редакция ${String(doc.revision).padStart(2,'0')}</span><span class="saved">${icon('check',13)} Сохранено</span></div>
      <div class="article-body">${rendered.markup || '<p>Пока здесь ничего нет.</p>'}</div>
      ${!view.context ? `<div class="doc-links">${links(data,doc)}</div>` : ''}`;
  } else if (available) {
    content = sectionContents(data,doc);
  }
  return `<div class="doc-layout" data-context="${Boolean(reading && view.context)}" data-tree="${view.tree}">
    ${sidebar(data,doc,view)}
    <section class="doc-center">
      ${pageBar(data,doc,view)}
      <div class="doc-scroll"><article class="article">${content}</article></div>
    </section>
    ${reading && view.context ? contextPanel(data,doc,view,rendered.headings) : ''}
  </div>`;
}

export function renderSharedPage(doc,images) {
  return `<section class="shared-view appearance">
    <div class="shared-bar"><span class="dock-brand"><span class="brand-mark">${icon('north',16)}</span>Север</span>
    ${button('leave-share','Рабочее пространство','arrow',{className:'ghost'})}</div>
    <article class="article">
      ${doc ? `${cover(doc,images)}<div class="page-meta"><span class="tag">${icon('eye',13)} Чтение по ссылке</span><span>Редакция ${doc.revision}</span></div>
      <div class="article-body">${markdown(doc.body).markup || '<p>Пока здесь ничего нет.</p>'}</div>` :
      `<div class="empty-page"><span class="entity-icon">${icon('lock')}</span><h1>Ссылка недоступна</h1><p class="muted">Попросите владельца поделиться актуальной ссылкой.</p></div>`}
    </article>
  </section>`;
}

