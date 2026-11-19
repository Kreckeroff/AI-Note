import {escapeHtml as html, icon} from './ui.mjs';

const get = id => document.getElementById(id);
const imageFormat = /\.(?:png|jpe?g|webp)$/i;
const fileFormat = /\.(?:pdf|txt|md|markdown|docx|xlsx|pptx|png|jpe?g|webp)$/i;
const limit = 20 * 1024 * 1024;
const fileId = '[-a-f0-9]+';
const imageReference = new RegExp('^!\\[((?:\\\\.|[^\\]])*)\\]\\(attachment:(' + fileId + ')\\)$', 'u');
const seededImage = {id:'9b8c', name:'Меню команд редактора.png', size:252219, image:true, url:'../handoff/assets/sever-slash-menu.png'};
const seededText = new File(['Доступ к пространству\n\nЗапрос отправляет владелец страницы.\n'], 'Порядок доступа.md', {type:'text/markdown'});
const startingText = 'Добавьте в заявку название пространства и укажите, что человек должен делать: читать или редактировать.\n\nВладелец получит запрос и выберет нужное право.';
const view = new URLSearchParams(location.search).get('view');
let reader = view === 'reader';
let published = {
  revision:4,
  files:view === 'empty' ? [] : [{id:'a1', name:seededText.name, size:seededText.size, url:URL.createObjectURL(seededText)}, seededImage],
  source:startingText,
};
let draft = null;
let sourceMode = false;
let dialogAction;
let selection;
let opener;

get('tree-icon').innerHTML = icon('folder', 18);
const dialog = get('attachment-dialog');
const alert = get('dialog-error');
const submit = get('dialog-submit');
const form = get('attachment-form');
const visual = get('visual');
const source = get('source');
const attachmentLines = text => text.split('\n').filter(line => line !== '');
const inserted = (text, id) => attachmentLines(text).filter(line => imageReference.exec(line)?.[2] === id).length;
const writtenAlt = raw => raw.replace(/[\\\]]/gu, '\\$&');
const sizeLabel = size => size < 1024 ? size + ' Б' : size < 1024 * 1024
  ? Math.ceil(size / 1024) + ' КБ'
  : (size / (1024 * 1024)).toLocaleString('ru', {maximumFractionDigits:1}) + ' МБ';

function content(text, assets, editable = false) {
  return attachmentLines(text).map(line => {
    const match = imageReference.exec(line);
    const file = match && assets.find(asset => asset.id === match[2] && asset.image);
    if (!file) return '<p>' + html(line) + '</p>';
    const alt = match[1].replace(/\\(.)/gu, '$1');
    const locked = editable ? ' contenteditable="false" data-image-line="' + html(line) + '"' : '';
    return '<figure class="attachment-inline"' + locked + '><img src="' + html(file.url) + '" alt="' + html(alt) + '"></figure>';
  }).join('');
}

function currentSource() {
  if (sourceMode) return source.value;
  return [...visual.childNodes].map(node => node.dataset?.imageLine ?? (node.innerText ?? node.textContent)).join('\n\n');
}

function drawText() {
  get('reading').innerHTML = content(published.source, published.files);
  if (draft && !reader) {
    visual.innerHTML = content(draft.source, draft.files, true);
    source.value = draft.source;
    source.hidden = !sourceMode;
    visual.hidden = sourceMode;
    get('source-toggle').setAttribute('aria-pressed', String(sourceMode));
  }
}

function drawFiles() {
  const editing = draft && !reader;
  const files = editing ? draft.files : published.files;
  get('attachment-count').textContent = files.length ? String(files.length) : '';
  get('empty-files').hidden = files.length > 0;
  get('files').innerHTML = files.map(file => {
    const label = html(file.name);
    const pending = editing && !published.files.some(before => before.id === file.id);
    const actions = editing
      ? (file.image ? '<button class="btn ghost" type="button" data-insert="' + file.id + '">Вставить в текст</button>' : '')
        + '<button class="btn ghost" type="button" aria-label="Удалить ' + label + '" data-remove="' + file.id + '">Удалить</button>'
      : '<a class="btn ghost" aria-label="Скачать ' + label + '" download="' + label + '" href="' + html(file.url) + '">Скачать</a>';
    return '<li class="attachment-row"><span class="entity-icon">' + icon('doc') + '</span>'
      + '<span class="entity-text"><strong>' + label + '</strong><small>' + sizeLabel(file.size)
      + (pending ? ' · Появится после сохранения' : '') + '</small></span>'
      + '<div class="attachment-actions">' + actions + '</div></li>';
  }).join('');
}

function render() {
  const editing = draft && !reader;
  get('edit').hidden = reader || Boolean(editing);
  get('reading').hidden = Boolean(editing);
  get('metadata').hidden = Boolean(editing);
  get('editing').hidden = !editing;
  get('attach').hidden = !editing;
  get('publication').hidden = !editing;
  get('revision').textContent = 'Редакция ' + String(published.revision).padStart(2, '0');
  get('role-link').textContent = reader ? 'Режим редактора' : 'Режим читателя';
  get('role-link').href = reader ? '?view=owner' : '?view=reader';
  drawText();
  drawFiles();
}

function showDialog(title, label, markup, action) {
  dialogAction = action;
  opener = document.activeElement;
  get('commands').hidden = true;
  get('dialog-title').textContent = title;
  get('dialog-fields').innerHTML = markup;
  alert.hidden = true;
  submit.textContent = label;
  dialog.showModal();
  (form.querySelector('input:not([hidden]),select') ?? get('dialog-cancel')).focus();
}

function invalid(message) {
  alert.textContent = message;
  alert.hidden = false;
}

function uploadField(onlyImages, optional = false) {
  const accept = onlyImages ? '.png,.jpg,.jpeg,.webp' : '.pdf,.txt,.md,.markdown,.docx,.xlsx,.pptx,.png,.jpg,.jpeg,.webp';
  return '<label>' + (optional ? 'Или загрузить новое' : 'Файл') + '<input class="field" type="file" id="upload" accept="' + accept + '"></label>'
    + '<small>' + (onlyImages ? 'PNG, JPEG или WebP' : 'PDF, TXT, Markdown, DOCX, XLSX, PPTX, PNG, JPEG, WebP') + ' до 20 МБ.</small>';
}

async function createFile(file, pictureOnly) {
  if (!file) throw new Error('Выберите файл.');
  if (!file.size) throw new Error('Файл пуст. Выберите другой файл.');
  if (file.size > limit) throw new Error('Размер файла превышает 20 МБ.');
  if (!(pictureOnly ? imageFormat : fileFormat).test(file.name)) throw new Error('Этот формат не поддерживается.');
  if (draft.files.length >= 20) throw new Error('На странице уже 20 вложений.');
  if (draft.files.reduce((total, item) => total + item.size, file.size) > 5 * limit) {
    throw new Error('Вложения страницы суммарно превышают 100 МБ.');
  }
  if (imageFormat.test(file.name)) {
    try { (await createImageBitmap(file)).close(); }
    catch { throw new Error('Не удалось прочитать изображение. Выберите другой файл.'); }
  }
  return {id:crypto.randomUUID(), name:file.name, size:file.size, image:imageFormat.test(file.name), url:URL.createObjectURL(file)};
}

function openUpload() {
  showDialog('Прикрепить файл', 'Прикрепить', '<div class="attachment-fields">' + uploadField(false) + '</div>', async () => {
    draft.files.push(await createFile(get('upload').files[0], false));
    drawFiles();
    get('status').textContent = 'Файл появится на странице после сохранения.';
  });
}

function openImage(preselected = '', fromSlash = false) {
  const selected = preselected || '';
  const choices = draft.files.filter(file => file.image);
  const options = choices.map(file => '<option value="' + file.id + '"' + (file.id === selected ? ' selected' : '') + '>' + html(file.name) + '</option>').join('');
  const fields = '<div class="attachment-fields">'
    + (choices.length ? '<label>Вложение<select class="field" id="existing-image"><option value="">Выберите изображение</option>' + options + '</select></label>' : '')
    + uploadField(true, choices.length > 0)
    + '<label>Описание изображения<input class="field" id="image-alt" maxlength="250" placeholder="Что показано на изображении?"></label>'
    + '<label><span><input type="checkbox" id="decorative"> Декоративное изображение</span></label></div>';
  showDialog('Вставить изображение', 'Вставить', fields, async () => {
    const alt = get('decorative').checked ? '' : get('image-alt').value.trim();
    if (!alt && !get('decorative').checked) throw new Error('Добавьте описание или отметьте изображение как декоративное.');
    const upload = get('upload').files[0];
    const file = upload ? await createFile(upload, true) : choices.find(choice => choice.id === get('existing-image')?.value);
    if (!file) throw new Error('Выберите или загрузите изображение.');
    if (upload) draft.files.push(file);
    const before = currentSource();
    const reference = '![' + writtenAlt(alt) + '](attachment:' + file.id + ')';
    const pos = selection ?? before.length;
    const left = before.slice(0, pos);
    const right = before.slice(pos);
    draft.source = [
      fromSlash ? left.replace(/\/[\p{L}]*$/u, '').trimEnd() : left.trimEnd(),
      reference, right.trimStart(),
    ].filter(Boolean).join('\n\n');
    opener = sourceMode ? source : visual;
    drawText();
    drawFiles();
    get('status').textContent = 'Изображение вставлено в черновик.';
  });
  get('decorative').addEventListener('change', event => { get('image-alt').disabled = event.target.checked; });
}

function removeFile(id) {
  const file = draft.files.find(item => item.id === id);
  const before = currentSource();
  const count = inserted(before, id);
  const remove = () => {
    draft.files = draft.files.filter(item => item.id !== id);
    draft.source = before.split('\n').filter(line => imageReference.exec(line)?.[2] !== id).join('\n');
    drawText();
    drawFiles();
    opener = get('attach');
    get('status').textContent = 'Удаление применится после сохранения.';
  };
  if (!count) {
    remove();
    get('attach').focus();
    return;
  }
  const detail = count === 1 ? '1 вставка' : count + ' вставок';
  showDialog('Удалить вложение?', 'Удалить файл и вставки',
    '<p>' + html(file.name) + ' · ' + detail + '</p><p class="muted">Файл и его вставки исчезнут с текущей страницы после сохранения.</p>', remove);
}

get('edit').addEventListener('click', () => {
  draft = {...published, files:[...published.files]};
  sourceMode = false;
  selection = undefined;
  get('status').textContent = '';
  render();
  visual.focus();
});
get('role-link').addEventListener('click', event => {
  event.preventDefault();
  if (!reader && draft) draft.source = currentSource();
  reader = !reader;
  get('status').textContent = '';
  render();
});
get('source-toggle').addEventListener('click', () => {
  draft.source = currentSource();
  sourceMode = !sourceMode;
  selection = undefined;
  drawText();
  (sourceMode ? source : visual).focus();
});
get('attach').addEventListener('click', openUpload);
get('image').addEventListener('click', () => openImage());
get('command-image').addEventListener('click', () => openImage('', true));
source.addEventListener('select', () => { selection = source.selectionStart; });
source.addEventListener('keyup', () => { selection = source.selectionStart; });
source.addEventListener('pointerup', () => { selection = source.selectionStart; });
visual.addEventListener('keyup', () => {
  const active = getSelection();
  const block = active?.anchorNode?.parentElement?.closest('#visual > *');
  if (!block) return;
  const earlier = [...visual.children].slice(0, [...visual.children].indexOf(block)).map(child => child.dataset.imageLine ?? child.innerText);
  const length = earlier.length ? earlier.join('\n\n').length + 2 : 0;
  selection = length + (active.anchorNode.nodeType === Node.TEXT_NODE ? active.anchorOffset : block.innerText.length);
  get('commands').hidden = !/\/[\p{L}]*$/u.test(currentSource().slice(0, selection));
});
visual.addEventListener('keydown', event => {
  if (get('commands').hidden || !['Enter','Escape'].includes(event.key)) return;
  event.preventDefault();
  if (event.key === 'Enter') openImage('', true);
  else get('commands').hidden = true;
});
get('files').addEventListener('click', event => {
  const control = event.target.closest('button');
  if (control?.dataset.insert) openImage(control.dataset.insert);
  if (control?.dataset.remove) removeFile(control.dataset.remove);
});
get('dialog-close').addEventListener('click', () => dialog.close());
get('dialog-cancel').addEventListener('click', () => dialog.close());
dialog.addEventListener('close', () => { if (opener?.isConnected) opener.focus({preventScroll:true}); });
form.addEventListener('submit', async event => {
  event.preventDefault();
  if (submit.disabled) return;
  alert.hidden = true;
  submit.disabled = true;
  try {
    await dialogAction();
    dialog.close();
  } catch (error) {
    invalid(error.message);
  } finally {
    submit.disabled = false;
  }
});

function finish(save) {
  if (save) published = {...draft, revision:published.revision + 1, source:currentSource()};
  draft = null;
  get('commands').hidden = true;
  render();
  get('page-title').focus({preventScroll:true});
  get('status').textContent = save ? 'Страница сохранена.' : 'Черновик отменён.';
}
get('save').addEventListener('click', () => finish(true));
get('cancel').addEventListener('click', () => finish(false));
render();
if (view === 'edit') get('edit').click();
