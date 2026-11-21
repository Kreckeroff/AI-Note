const get = id => document.getElementById(id);
const roles = ['editor', 'reader', 'guest', 'public'];
const params = new URLSearchParams(location.search);
const requested = params.get('view');
const sample = 'https://example.com/';
const restricted = params.get('case') === 'restricted';
const role = get('role');
role.value = roles.includes(requested) ? requested : 'editor';
const template = get('block-template');
const dialog = get('editor');
const field = get('embed-input');
const status = get('status');
const actions = get('commands');
const undo = [];
const loaded = new Set();
let lastId = 1;
let publicOn = params.get('case') !== 'revoked';
let published = role.value === 'editor' && !restricted ? [] : [{id: lastId, url: sample}];
let draft = [...published];
let sourceVisible = false;
let editingId;
let dialogOpener;

function announce(message, visible = false) {
  status.classList.toggle('sr-only', !visible);
  status.textContent = message;
}

function readAddress(raw) {
  let value = raw.trim();
  if (!value) throw new Error('Вставьте HTTPS-адрес или код iframe.');
  if (value.startsWith('<')) {
    const invalidFrame = 'Вставьте один iframe с адресом в атрибуте src.';
    if (!/^<iframe\b[\s\S]*<\/iframe\s*>$/iu.test(value)) throw new Error(invalidFrame);
    const fragment = document.createElement('template');
    fragment.innerHTML = value;
    const elements = [...fragment.content.childNodes].filter(node => node.nodeType !== Node.TEXT_NODE || node.textContent.trim());
    const frame = elements[0];
    if (elements.length !== 1 || frame.localName !== 'iframe' || frame.hasAttribute('srcdoc') || frame.textContent.trim()) {
      throw new Error(invalidFrame);
    }
    value = frame.getAttribute('src')?.trim() ?? '';
  }
  let url;
  try { url = new URL(value); }
  catch { throw new Error('Введите полный HTTPS-адрес источника.'); }
  if (url.protocol !== 'https:' || url.username || url.password || url.hostname === location.hostname) {
    throw new Error('Нужен внешний HTTPS-адрес без логина и пароля.');
  }
  return url.href;
}

function showError(message = '') {
  const error = get('input-error');
  error.textContent = message;
  error.hidden = !message;
  if (message) {
    field.setAttribute('aria-invalid', 'true');
    field.focus();
  } else field.removeAttribute('aria-invalid');
}

function setCommands(open, fromCaret = false) {
  const selection = window.getSelection();
  const toolbar = get('toolbar');
  const at = fromCaret && selection?.rangeCount ? selection.getRangeAt(0).getBoundingClientRect() : null;
  if (at?.bottom) {
    const origin = toolbar.getBoundingClientRect();
    actions.style.left = `${Math.max(0, Math.min(at.left - origin.left, origin.width - 120))}px`;
    actions.style.top = `${at.bottom - origin.top + 5}px`;
  } else {
    actions.style.removeProperty('left');
    actions.style.removeProperty('top');
  }
  actions.hidden = !open;
  get('insert-command').setAttribute('aria-expanded', String(open));
  if (open) get('embed-command').focus();
}

function openEditor(block, opener) {
  editingId = block?.id;
  dialogOpener = opener;
  setCommands(false);
  get('editor-title').textContent = block ? 'Изменить адрес блока' : 'Вставить внешний блок';
  get('apply').textContent = block ? 'Применить' : 'Вставить';
  field.value = block?.url ?? '';
  showError();
  dialog.showModal();
  field.focus();
}

function changeDraft(next) {
  undo.push(draft);
  draft = next;
  render();
}

function renderBlock(block, editor) {
  const item = template.content.firstElementChild.cloneNode(true);
  const part = name => item.querySelector(`[data-${name}]`);
  const key = block.id + ' ' + block.url;
  const active = loaded.has(key);
  part('host').textContent = new URL(block.url).hostname;
  part('editor-only').hidden = !editor;
  part('external').href = block.url;
  part('placeholder').hidden = active;
  part('frame').hidden = !active;
  part('hide').hidden = !active;
  part('help').hidden = !active;
  part('frame-content').textContent = restricted ? 'Источник ограничил встраивание · демонстрация' : 'Внешний документ · демонстрация';
  const load = part('load');
  const hide = part('hide');
  load.onclick = () => {
    loaded.add(key);
    part('placeholder').hidden = true;
    part('frame').hidden = hide.hidden = part('help').hidden = false;
    hide.focus();
    announce('Область внешнего содержимого открыта.');
  };
  hide.onclick = () => {
    loaded.delete(key);
    part('frame').hidden = hide.hidden = part('help').hidden = true;
    part('placeholder').hidden = false;
    load.focus();
    announce('Внешнее содержимое скрыто.');
  };
  part('edit').onclick = () => {
    const menu = part('editor-only');
    menu.open = false;
    openEditor(block, menu.querySelector('summary'));
  };
  part('remove').onclick = () => {
    loaded.delete(key);
    changeDraft(draft.filter(item => item.id !== block.id));
    get('last-paragraph').focus();
    announce('Блок удалён из черновика.');
  };
  return item;
}

function render() {
  const editor = role.value === 'editor';
  const publicView = role.value === 'public';
  const denied = publicView && !publicOn;
  get('workspace').dataset.view = role.value;
  get('document').hidden = denied;
  get('denied').hidden = !denied;
  get('breadcrumb').hidden = publicView;
  get('toolbar').hidden = !editor;
  get('edit-footer').hidden = !editor;
  get('undo').disabled = !undo.length;
  get('markdown-toggle').setAttribute('aria-pressed', String(sourceVisible));
  const note = get('public-note');
  note.hidden = !editor || !(publicOn ? draft.length : published.length);
  note.textContent = publicOn
    ? 'Публичная ссылка включена. После сохранения адрес источника будет доступен всем, у кого есть эта ссылка.'
    : 'Если включить публичную ссылку, адрес источника будет доступен всем, у кого есть эта ссылка.';
  get('demo-actions').hidden = !editor;
  get('public-toggle').textContent = publicOn ? 'Отозвать публичную ссылку' : 'Включить публичную ссылку';
  const showSource = editor && sourceVisible && draft.length > 0;
  get('markdown').hidden = !showSource;
  const body = get('body-preview');
  body.classList.toggle('editor-visual', editor);
  body.contentEditable = editor ? 'plaintext-only' : 'false';
  body.hidden = showSource;
  get('markdown').textContent = [get('intro').textContent,
    ...draft.map(({url}) => `\`\`\`sever-embed\n${url}\n\`\`\``),
    get('last-paragraph').textContent].join('\n\n');
  get('blocks').replaceChildren(...(denied ? [] : (editor ? draft : published).map(block => renderBlock(block, editor))));
}

get('form').addEventListener('submit', event => {
  event.preventDefault();
  try {
    const url = readAddress(field.value);
    const existing = draft.find(block => block.id === editingId);
    if (existing) loaded.delete(existing.id + ' ' + existing.url);
    changeDraft(existing ? draft.map(block => block.id === editingId ? {...block, url} : block) : [...draft, {id: ++lastId, url}]);
    dialog.close('saved');
    announce(existing ? 'Адрес изменён в черновике.' : 'Блок добавлен в черновик.');
  } catch (error) {
    showError(error.message);
  }
});
field.addEventListener('input', () => showError());
dialog.addEventListener('close', () => {
  if (dialog.returnValue === 'saved') get('last-paragraph').focus();
  else dialogOpener?.focus();
  dialog.returnValue = '';
});
get('cancel').onclick = () => dialog.close();
get('example').onclick = () => {
  field.value = `<iframe src="${sample}" style="width:100%;aspect-ratio:16/9" allowfullscreen></iframe>`;
  showError();
  field.focus();
};
get('insert-command').onclick = () => setCommands(actions.hidden);
get('embed-command').onclick = () => openEditor(undefined, get('insert-command'));
get('markdown-toggle').onclick = () => {
  sourceVisible = !sourceVisible;
  render();
};
get('undo').onclick = () => {
  if (!undo.length) return;
  loaded.clear();
  draft = undo.pop();
  render();
  announce('Последняя правка блока отменена.');
};
get('publish').onclick = () => {
  published = [...draft];
  render();
  announce('Редакция сохранена. Переключитесь на читателя или публичный просмотр.', true);
};
get('public-toggle').onclick = () => {
  publicOn = !publicOn;
  render();
  announce(publicOn ? 'Публичная ссылка включена.' : 'Публичная ссылка отозвана.', true);
};
role.onchange = () => {
  loaded.clear();
  setCommands(false);
  announce('');
  render();
};
document.addEventListener('keydown', event => {
  if (event.key === 'Escape' && !actions.hidden) {
    setCommands(false);
    get('insert-command').focus();
  }
  if (role.value !== 'editor' || dialog.open || event.target.closest('textarea, input, select')) return;
  if (event.key === '/') {
    event.preventDefault();
    setCommands(true, get('body-preview').contains(event.target));
  } else if (event.key.toLowerCase() === 'z' && (event.metaKey || event.ctrlKey) && !event.shiftKey) {
    event.preventDefault();
    get('undo').click();
  }
});
render();
