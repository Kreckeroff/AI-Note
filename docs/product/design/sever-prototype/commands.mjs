import {escapeHtml as html, icon} from './ui.mjs';
import {parseVideo} from './media.mjs';

const commands = [
  {id:'heading',label:'Заголовок',description:'Название раздела страницы',icon:'doc',markup:'<h3>Заголовок</h3>'},
  {id:'list',label:'Список',description:'Маркированные пункты',icon:'list',markup:'<ul><li>Пункт списка</li></ul>'},
  {id:'quote',label:'Цитата',description:'Отдельная выдержка из текста',icon:'message',markup:'<blockquote>Цитата</blockquote>'},
  {id:'code',label:'Блок кода',description:'Моноширинный блок',icon:'code',markup:'<pre><code>код</code></pre>'},
  {id:'video',label:'Видео',description:'Вставка по ссылке · встроенное расширение',icon:'play'},
];

export function videoBlock(input,{editing=false} = {}) {
  const video = parseVideo(input);
  if (!video) return '';
  const player = video.direct ? `<video controls preload="none" src="${html(video.source)}" aria-label="Видео"></video>` :
    `<div class="video-poster"><span class="video-icon">${icon('play',22)}</span><strong>${html(video.label)}</strong>
      ${editing ? '<small>Проигрыватель доступен при чтении</small>' : '<button type="button" class="btn" data-action="play-video">Воспроизвести</button>'}</div>`;
  return `<figure class="video-block" data-video="${html(video.source)}" contenteditable="false">${player}
    <figcaption>${html(video.label)} <a href="${html(video.source)}" target="_blank" rel="noopener noreferrer">Открыть источник</a></figcaption></figure>`;
}

export function playEmbeddedVideo(figure) {
  const video = parseVideo(figure?.dataset.video);
  if (!video?.embed) return;
  const frame = document.createElement('iframe');
  frame.src = video.embed;
  frame.title = video.label;
  frame.allowFullscreen = true;
  frame.referrerPolicy = 'strict-origin';
  frame.sandbox = 'allow-scripts allow-same-origin allow-presentation';
  figure.querySelector('.video-poster')?.replaceWith(frame);
}

export function connectCommands(form,{onVideo,onChange}) {
  const editor = form.querySelector('[data-rich-editor]');
  const menu = form.querySelector('[data-command-menu]');
  let match, visible = [], active = 0;
  function close() {
    menu.hidden = true;
    match = undefined;
    editor.removeAttribute('aria-activedescendant');
  }
  function draw() {
    menu.innerHTML = visible.length ? visible.map((command,index) =>
      `<button type="button" id="editor-command-${command.id}" role="option" aria-selected="${active === index}" data-command="${command.id}">
      <span class="entity-icon">${icon(command.icon,15)}</span><span><strong>${command.label}</strong><small>${command.description}</small></span></button>`
    ).join('') : '<p class="muted small">Команды не найдены</p>';
    if (visible[active]) {
      editor.setAttribute('aria-activedescendant','editor-command-'+visible[active].id);
      const selected = menu.children[active];
      if (selected.offsetTop < menu.scrollTop) menu.scrollTop = selected.offsetTop;
      if (selected.offsetTop+selected.offsetHeight > menu.scrollTop+menu.clientHeight) {
        menu.scrollTop = selected.offsetTop+selected.offsetHeight-menu.clientHeight;
      }
    }
    else editor.removeAttribute('aria-activedescendant');
  }
  function showCommands() {
    const selection = getSelection(), node = selection?.anchorNode;
    if (!selection?.isCollapsed || node?.nodeType !== Node.TEXT_NODE || !editor.contains(node) ||
       node.parentElement.closest('pre,code,[data-video]')) { close(); return; }
    const prefix = node.textContent.slice(0,selection.anchorOffset);
    const found = /(?:^|\s)(\/[^/\s]{0,28})$/.exec(prefix);
    if (!found) { close(); return; }
    const query = found[1].slice(1).toLocaleLowerCase('ru');
    if (match?.query !== query) active = 0;
    const range = document.createRange();
    range.setStart(node,prefix.length-found[1].length);
    range.setEnd(node,selection.anchorOffset);
    match = {range,query};
    visible = commands.filter(command => (command.label+' '+command.id).toLocaleLowerCase('ru').includes(query));
    if (active >= visible.length) active = 0;
    const at = range.getBoundingClientRect(), box = form.getBoundingClientRect();
    const scrollBox = editor.closest('.doc-scroll').getBoundingClientRect();
    const lowerEdge = Math.min(scrollBox.bottom,window.visualViewport?.height || innerHeight);
    const below = Math.max(0,lowerEdge-at.bottom-10);
    const above = Math.max(0,at.top-scrollBox.top-10);
    menu.style.setProperty('--command-left', Math.max(0,Math.min(at.left-box.left,form.clientWidth-258))+'px');
    menu.style.maxHeight = '';
    menu.hidden = false;
    draw();
    const flipped = menu.offsetHeight > below && above > below;
    menu.style.maxHeight = Math.max(36,flipped ? above : below)+'px';
    menu.style.setProperty('--command-top',(flipped ? at.top-box.top-menu.offsetHeight-8 : at.bottom-box.top+8)+'px');
  }
  function choose(command) {
    if (!command || !match) return;
    const range = match.range.cloneRange();
    close();
    const insert = markup => {
      if (!editor.contains(range.startContainer)) return;
      let block = range.startContainer;
      while (block.parentNode !== editor && block !== editor) block = block.parentNode;
      range.deleteContents();
      const template = document.createElement('template');
      template.innerHTML = markup;
      const last = template.content.lastElementChild;
      if (block === editor) editor.append(template.content);
      else if (block.nodeName === 'P' && !block.textContent.trim()) block.replaceWith(template.content);
      else block.after(template.content);
      editor.focus();
      const selection = getSelection();
      const caret = document.createRange();
      caret.selectNodeContents(last.querySelector('li,code') || last);
      caret.collapse(false);
      selection.removeAllRanges();
      selection.addRange(caret);
      onChange();
    };
    if (command.id === 'video') onVideo(url => insert(videoBlock(url,{editing:true})+'<p><br></p>'));
    else insert(command.markup);
  }
  editor.addEventListener('input',event => { if (!event.isComposing) showCommands(); });
  editor.addEventListener('focusout',event => { if (!menu.contains(event.relatedTarget)) close(); });
  editor.addEventListener('keydown',event => {
    if (menu.hidden) return;
    if (event.key === 'Escape') { event.preventDefault(); close(); return; }
    if (event.key === 'Enter' && visible[active]) { event.preventDefault(); choose(visible[active]); return; }
    if (!visible.length || !['ArrowDown','ArrowUp'].includes(event.key)) return;
    event.preventDefault();
    active = (active+(event.key === 'ArrowDown' ? 1 : -1)+visible.length)%visible.length;
    draw();
  });
  menu.addEventListener('mousedown',event => event.preventDefault());
  menu.addEventListener('click',event => {
    choose(commands.find(command => command.id === event.target.closest('[data-command]')?.dataset.command));
  });
}
