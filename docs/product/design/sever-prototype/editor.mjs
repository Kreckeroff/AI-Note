import {escapeHtml} from './ui.mjs';
import {markdownLimit} from './model.mjs';
import {connectCommands} from './commands.mjs';
import {parseVideo} from './media.mjs';

const marks = [
  {pattern:/(?<!\*)\*\*\*([^*\n]+)\*\*\*$/, open:'<strong><em>', close:'</em></strong>'},
  {pattern:/(?<!\*)\*\*([^*\n]+)\*\*$/, open:'<strong>', close:'</strong>'},
  {pattern:/(?<!\*)\*([^*\n]+)\*$/, open:'<em>', close:'</em>'},
  {pattern:/`([^`\n]+)`$/, open:'<code>', close:'</code>'},
];
const boundary = '\u200b';
const blockTags = new Set(['P','DIV','H1','H2','H3','H4','BLOCKQUOTE','PRE','UL','OL','FIGURE']);

function markedValue(value,marker) {
  const [, before, text, after] = /^(\s*)([\s\S]*?)(\s*)$/.exec(value);
  return before+marker+text+marker+after;
}

function inlineValue(node) {
  if (node.nodeType === Node.TEXT_NODE) return node.textContent.replaceAll(boundary,'').replaceAll('\u00a0',' ');
  if (node.nodeType !== Node.ELEMENT_NODE) return '';
  if (node.tagName === 'BR') return '\n';
  const value = [...node.childNodes].map(inlineValue).join('');
  if (!value.trim()) return value;
  if (node.matches('strong,b')) return markedValue(value,'**');
  if (node.matches('em,i')) return markedValue(value,'*');
  if (node.matches('code')) return '`'+value.replaceAll('`','')+'`';
  if (node.matches('a') && /^https?:\/\//.test(node.getAttribute('href') || '')) return '['+value+']('+node.getAttribute('href')+')';
  return value;
}

function blockValue(block) {
  if (block.matches('figure[data-video]')) {
    const video = parseVideo(block.dataset.video);
    return video ? '![Видео]('+video.source+')' : '';
  }
  if (block.matches('ul,ol')) {
    const start = Number.parseInt(block.getAttribute('start') ?? '1',10);
    return [...block.children].filter(item => item.matches('li')).map((item,index) =>
      (block.tagName === 'UL' ? '- ' : ((Number.isFinite(start) ? start : 1)+index)+'. ')+inlineValue(item).trim()
    ).join('\n');
  }
  if (block.matches('pre')) {
    const fence = '`'.repeat(Number(block.dataset.mdFence) || 3);
    return fence+(block.dataset.mdInfo || '')+'\n'+block.textContent.replaceAll(boundary,'')+'\n'+fence;
  }
  const value = inlineValue(block).trim();
  if (!value) return '';
  if (block.matches('blockquote')) return '> '+value;
  if (block.matches('h1,h2,h3,h4')) {
    const level = Number(block.dataset.mdHeading) || Math.max(1,Number(block.tagName[1])-1);
    return '#'.repeat(level)+' '+value;
  }
  return value;
}

export function editorMarkdown(editor) {
  const paragraphs = [];
  let inline = '';
  for (const node of editor.childNodes) {
    if (!blockTags.has(node.tagName)) {
      inline += inlineValue(node);
      continue;
    }
    if (inline.trim()) paragraphs.push(inline.trim());
    inline = '';
    paragraphs.push(blockValue(node));
  }
  if (inline.trim()) paragraphs.push(inline.trim());
  return paragraphs.filter(Boolean).join('\n\n');
}

function syncSource(form) {
  form.elements.body.value = editorMarkdown(form.querySelector('[data-rich-editor]'));
  validateEditorLength(form);
}

export function validateEditorLength(form) {
  const count = form.elements.body.value.length;
  const valid = count <= markdownLimit;
  const status = form.querySelector('#editor-status');
  const submit = form.querySelector('button[type="submit"]');
  status.textContent = valid ? '' : 'Сократите Markdown до '+markdownLimit.toLocaleString('ru-RU')+' символов. Сейчас '+count.toLocaleString('ru-RU')+'.';
  status.hidden = valid;
  submit.disabled = !valid;
  return valid;
}

function markAsTyped(editor,event) {
  if (event.isComposing || event.inputType !== 'insertText' || !['*','`'].includes(event.data?.at(-1))) return;
  const selection = getSelection();
  const node = selection?.anchorNode;
  if (!selection?.isCollapsed || node?.nodeType !== Node.TEXT_NODE || !editor.contains(node) ||
      node.parentElement.closest('code,pre,strong,em')) return;
  const end = selection.anchorOffset;
  const prefix = node.textContent.slice(0,end);
  for (const {pattern,open,close} of marks) {
    const match = pattern.exec(prefix);
    if (!match || !match[1].trim() || match[1] !== match[1].trim()) continue;
    const range = document.createRange();
    range.setStart(node,match.index);
    range.setEnd(node,end);
    selection.removeAllRanges();
    selection.addRange(range);
    document.execCommand('insertHTML',false,open+escapeHtml(match[1])+close+boundary);
    break;
  }
}

export function connectEditor(form,{onVideo}) {
  const editor = form.querySelector('[data-rich-editor]');
  editor.addEventListener('input',event => {
    markAsTyped(editor,event);
    syncSource(form);
  });
  editor.addEventListener('paste',event => {
    event.preventDefault();
    const text = event.clipboardData?.getData('text/plain') || '';
    if (text) {
      document.execCommand('insertText',false,text);
      syncSource(form);
    }
  });
  form.elements.body.addEventListener('input',() => validateEditorLength(form));
  editor.addEventListener('drop',event => event.preventDefault());
  connectCommands(form,{onVideo,onChange:() => syncSource(form)});
  form.querySelector('.editor-toolbar').addEventListener('mousedown',event => {
    if (event.target.closest('[data-action="format"]') && !editor.hidden) event.preventDefault();
  });
  validateEditorLength(form);
}

export function applyFormatting(form,command) {
  const editor = form.querySelector('[data-rich-editor]');
  const source = form.elements.body;
  if (editor.hidden) {
    const rawMarks = {heading:['## ',''],bold:['**','**'],italic:['*','*'],list:['- ',''],code:['`','`']};
    const [left,right] = rawMarks[command];
    const start = source.selectionStart, end = source.selectionEnd;
    source.setRangeText(left+source.value.slice(start,end)+right,start,end,'end');
    source.focus();
    source.setSelectionRange(start+left.length,end+left.length);
    validateEditorLength(form);
    return;
  }
  editor.focus();
  const selection = getSelection();
  if (!selection?.anchorNode || !editor.contains(selection.anchorNode)) return;
  if (command === 'code') {
    const text = selection.toString() || 'код';
    document.execCommand('insertHTML',false,'<code>'+escapeHtml(text)+'</code>'+boundary);
  } else {
    const native = {heading:'formatBlock',bold:'bold',italic:'italic',list:'insertUnorderedList'};
    document.execCommand(native[command],false,command === 'heading' ? 'h3' : null);
  }
  syncSource(form);
}

export function toggleSource(form,renderMarkdown) {
  const editor = form.querySelector('[data-rich-editor]');
  const source = form.elements.body;
  const control = form.querySelector('[data-action="source-mode"]');
  source.hidden = !source.hidden;
  editor.hidden = !source.hidden;
  control.setAttribute('aria-pressed',String(editor.hidden));
  if (editor.hidden) source.focus();
  else {
    editor.innerHTML = renderMarkdown(source.value,{editing:true}).markup || '<p><br></p>';
    editor.focus();
  }
}
