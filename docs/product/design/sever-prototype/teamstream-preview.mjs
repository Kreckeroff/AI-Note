const get = id => document.getElementById(id);
const mode = get('perspective');
const theme = get('theme');
const dialog = get('embed-dialog');
const address = get('address');
const share = get('share-dialog');
const sample = 'https://workspace.teamstream.mws.ru/embed/a1b2c3d4-0000-4000-8000-000000000001?t=30';
const view = new URLSearchParams(location.search).get('view');
if (['editor', 'reader', 'public', 'restricted'].includes(view)) mode.value = view;

let draft = sample;
let published = sample;
let publicOn = true;
let editing = false;
let interval;
let second = 0;
const history = [];

function status(message) {
  get('status').textContent = message;
}

function parse(value) {
  let source = value.trim();
  if (!source) return null;
  if (source.startsWith('<')) {
    const frame = /^<iframe\s+[^>]*\bsrc\s*=\s*(["'])([^"']+)\1[^>]*>\s*<\/iframe>$/i.exec(source);
    if (!frame || /\bsrcdoc\s*=/i.test(source)) return null;
    source = frame[2];
  }
  try {
    const url = new URL(source);
    return url.protocol === 'https:' && !url.username && !url.password ? url : null;
  } catch {
    return null;
  }
}

function isTeamStream(url) {
  return url?.origin === 'https://workspace.teamstream.mws.ru' &&
    /^\/embed\/[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}$/i.test(url.pathname) &&
    !url.hash && /^(|\?t=\d+)$/.test(url.search);
}

function paintTime() {
  get('time').textContent = '0:' + String(second).padStart(2, '0');
  get('progress').value = second;
}

function pause() {
  clearInterval(interval);
  interval = undefined;
  get('play').textContent = 'Воспроизвести';
}

function collapse() {
  pause();
  get('player').hidden = get('hide-player').hidden = get('fallback').hidden = true;
  get('open-player').hidden = false;
}

function render() {
  collapse();
  const editor = mode.value === 'editor';
  const isPublic = ['public', 'restricted'].includes(mode.value);
  const denied = isPublic && !publicOn;
  const url = parse(editor ? draft : published);
  const teamstream = isTeamStream(url);
  const base = 'Север · Видео TeamStream';
  document.title = isPublic ? base + ' · Публичный просмотр' : base;
  document.body.classList.toggle('demo-public', isPublic);
  for (const id of ['tree', 'dock-space', 'dock-divider', 'breadcrumbs']) get(id).hidden = isPublic;
  get('public-label').hidden = !isPublic || denied;
  get('denied').hidden = !denied;
  get('document').hidden = denied;
  for (const id of ['toolbar', 'insert-area', 'footer', 'media-menu', 'share']) get(id).hidden = !editor;
  get('save-note').hidden = !editor || !publicOn || !teamstream;
  get('undo').disabled = !history.length;
  get('body').classList.toggle('editor-visual', editor);
  get('media').hidden = !url;
  get('provider').textContent = teamstream ? 'TeamStream' : url?.hostname ?? '';
  get('open-player').lastChild.textContent = teamstream ? 'Открыть плеер' : 'Загрузить';
  get('external').textContent = teamstream ? 'Открыть в TeamStream' : 'Открыть источник';
  get('external').href = teamstream ? 'https://teamstream.mws.ru/' : url?.href ?? '';
  get('markdown').textContent = url ? ['```sever-embed', url.href, '```'].join('\n') : '';
  if (!editor && !get('markdown').hidden) get('source').click();
}

function toggleCommands() {
  const menu = get('commands');
  menu.hidden = !menu.hidden;
  get('insert').setAttribute('aria-expanded', String(!menu.hidden));
  if (!menu.hidden) get('embed').focus();
}

function openDialog(change) {
  editing = change;
  address.value = change ? draft ?? '' : '';
  get('dialog-title').textContent = change ? 'Изменить адрес' : 'Встроить внешний блок';
  get('apply').textContent = change ? 'Применить' : 'Вставить';
  get('error').hidden = true;
  address.dispatchEvent(new Event('input'));
  dialog.showModal();
  address.focus();
}

get('theme').onchange = () => { document.body.dataset.theme = theme.value; };
mode.onchange = render;
get('insert').onclick = toggleCommands;
get('embed').onclick = () => { toggleCommands(); openDialog(false); };
document.addEventListener('keydown', event => {
  if (event.key !== '/' || mode.value !== 'editor' || dialog.open || share.open ||
    event.target instanceof HTMLInputElement || event.target instanceof HTMLTextAreaElement) return;
  event.preventDefault();
  if (get('commands').hidden) toggleCommands();
});
address.oninput = () => {
  get('error').hidden = true;
  get('recognized').textContent = isTeamStream(parse(address.value)) ? 'Распознано видео TeamStream' : '';
};
dialog.onclose = () => {
  if (editing) get('media-menu').querySelector('summary').focus();
  else get('insert').focus();
};
get('example').onclick = () => {
  address.value = sample;
  address.dispatchEvent(new Event('input'));
  address.focus();
};
get('cancel').onclick = () => dialog.close();
get('embed-form').onsubmit = event => {
  event.preventDefault();
  const url = parse(address.value);
  if (!url) {
    const error = get('error');
    error.textContent = 'Вставьте HTTPS-адрес или код iframe с адресом в атрибуте src.';
    error.hidden = false;
    address.focus();
    return;
  }
  history.push(draft);
  draft = url.href;
  dialog.close();
  render();
  status('Блок добавлен в черновик.');
};
get('change').onclick = () => { get('media-menu').open = false; openDialog(true); };
get('remove').onclick = () => {
  history.push(draft);
  draft = null;
  render();
  get('insert').focus();
};
get('undo').onclick = () => { draft = history.pop(); render(); };
get('save').onclick = () => {
  published = draft;
  mode.value = 'reader';
  render();
  status('Страница сохранена.');
};
get('source').onclick = () => {
  get('markdown').hidden = !get('markdown').hidden;
  get('body').hidden = !get('markdown').hidden;
  get('source').setAttribute('aria-pressed', String(!get('markdown').hidden));
};
get('share').onclick = () => {
  get('publish').textContent = publicOn ? 'Отозвать публичную ссылку' : 'Создать публичную ссылку';
  share.showModal();
};
get('close-share').onclick = () => share.close();
get('publish').onclick = () => {
  publicOn = !publicOn;
  share.close();
  render();
  status(publicOn ? 'Публичная ссылка создана.' : 'Публичная ссылка отозвана.');
};
get('open-player').onclick = () => {
  const restricted = mode.value === 'restricted';
  const url = parse(mode.value === 'editor' ? draft : published);
  const video = isTeamStream(url);
  second = video ? Math.min(59, Number(url.searchParams.get('t')) || 0) : 0;
  paintTime();
  get('open-player').hidden = true;
  get('player').hidden = get('hide-player').hidden = get('fallback').hidden = false;
  get('player-controls').hidden = restricted || !video;
  get('player-title').textContent = restricted ? 'Видео недоступно в TeamStream' : video ? 'Демонстрация плеера' : 'Внешний блок';
  get('fallback').textContent = restricted ? 'Откройте ролик в TeamStream и проверьте доступ.' : 'Если видео не открывается, проверьте доступ в TeamStream.';
  get(restricted || !video ? 'hide-player' : 'play').focus();
};
get('hide-player').onclick = () => { collapse(); get('open-player').focus(); };
get('play').onclick = () => {
  if (interval) { pause(); return; }
  if (second >= 59) second = 0;
  get('play').textContent = 'Пауза';
  interval = setInterval(() => {
    second++;
    paintTime();
    if (second === 59) pause();
  }, 1000);
};

render();
