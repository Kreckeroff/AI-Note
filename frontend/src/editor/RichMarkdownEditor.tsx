import {useId, useImperativeHandle, useLayoutEffect, useMemo, useRef, useState, type KeyboardEvent, type Ref} from 'react';
import type {DraftEditKind, DraftHistoryDirection} from './draftHistory';
import {
  applyCommand, applySlashCommand, exitCodeBlock, insertVisual, moveListItem, removeVisual, toggleTask,
  updateCodeLanguage, type EditorChange,
} from './editorChanges';
import {availableCommands, EditorCommandMenu, type MenuAction} from './EditorCommandMenu';
import {
  escapeLiteralText, isProtected, parseMarkdown, type EditorBlock, type EditorCommand, type MarkdownDocument, type SourceRange,
} from './markdownModel';
import {caretPlaceholder, protectedEdit, RichMarkdownBlocks} from './RichMarkdownBlocks';
import {findSlashCommand, type SlashMatch} from './slashCommand';
import type {PageAttachment} from './pageAttachments';

export type {EditorCommand} from './markdownModel';
export {decodeLegacyText} from './markdownModel';
export {applySourceCommand} from './editorChanges';

export type RichMarkdownEditorHandle = {
  apply(command: EditorCommand): void;
  focus(): void;
  selection(): SourceRange;
  restoreSelection(range: SourceRange): void;
  selectedText(): string;
};

type EditorProps = {
  source: string;
  legacy: boolean;
  onChange(change: EditorChange, before: SourceRange, kind: DraftEditKind): void | boolean;
  onHistory(direction: DraftHistoryDirection): void;
  onPasteMarkdown(range: SourceRange): void;
  onInsertAttachmentImage?: (range: SourceRange) => void;
  attachments: readonly PageAttachment[];
  readOnly: boolean;
  describedBy?: string;
  id?: string;
  ref?: Ref<RichMarkdownEditorHandle>;
  onCompositionChange?(active: boolean): void;
};

const runSelector = '[data-markdown-run]';
const structuralEdit = 'Такое изменение можно сделать в «Исходнике».';

function belongsToEditor(root: HTMLElement | null, target: EventTarget | null): boolean {
  const element = target instanceof Element ? target : target instanceof Node ? target.parentElement : null;
  return Boolean(root && element?.closest('[contenteditable], input, textarea, select, button') === root);
}

function runElement(node: Node): HTMLElement | null {
  return (node instanceof Element ? node : node.parentElement)?.closest<HTMLElement>(runSelector) ?? null;
}

function runOffset(root: HTMLElement, model: MarkdownDocument, node: Node, offset: number): number | undefined {
  if (!root.contains(node)) return;
  const marked = runElement(node);
  if (marked && root.contains(marked)) {
    const run = model.runs[Number(marked.dataset.markdownRun)];
    if (!run) return;
    const inside = root.ownerDocument.createRange();
    inside.selectNodeContents(marked);
    inside.setEnd(node, offset);
    return run.offsets[Math.min(inside.toString().length, run.text.length)];
  }

  const point = root.ownerDocument.createRange();
  point.setStart(node, offset);
  point.collapse(true);
  let previous: number | undefined;
  for (const element of root.querySelectorAll<HTMLElement>(runSelector)) {
    const bounds = root.ownerDocument.createRange();
    bounds.selectNodeContents(element);
    const run = model.runs[Number(element.dataset.markdownRun)];
    if (!run) continue;
    if (point.compareBoundaryPoints(Range.START_TO_START, bounds) <= 0) {
      return previous ?? run.offsets[0];
    }
    previous = run.offsets.at(-1);
  }
  return previous;
}

function sourceRange(root: HTMLElement, model: MarkdownDocument, range: AbstractRange): SourceRange | undefined {
  const start = runOffset(root, model, range.startContainer, range.startOffset);
  const end = runOffset(root, model, range.endContainer, range.endOffset);
  return start !== undefined && end !== undefined ? {start, end} : undefined;
}

function selectedRange(root: HTMLElement, model: MarkdownDocument): SourceRange | undefined {
  const selection = root.ownerDocument.getSelection();
  return selection?.rangeCount ? sourceRange(root, model, selection.getRangeAt(0)) : undefined;
}

function nearestPoint(root: HTMLElement, model: MarkdownDocument, source: number): {node: Node; offset: number} | undefined {
  let nearest: {node: Node; offset: number} | undefined;
  let distance = Infinity;
  for (const element of root.querySelectorAll<HTMLElement>(runSelector)) {
    const run = model.runs[Number(element.dataset.markdownRun)];
    const node = element.firstChild;
    if (!run || !node || run.locked) continue;
    for (const [offset, position] of run.offsets.entries()) {
      const difference = Math.abs(source - position);
      if (difference === 0) return {node, offset};
      if (difference < distance) {
        nearest = {node, offset};
        distance = difference;
      }
    }
  }
  return nearest;
}

function selectSource(root: HTMLElement, model: MarkdownDocument, range: SourceRange) {
  const start = nearestPoint(root, model, range.start);
  const end = nearestPoint(root, model, range.end);
  const selection = root.ownerDocument.getSelection();
  if (!start || !end || !selection) return;
  const rendered = root.ownerDocument.createRange();
  rendered.setStart(start.node, start.offset);
  rendered.setEnd(end.node, end.offset);
  selection.removeAllRanges();
  selection.addRange(rendered);
}

type InsertionKind = Parameters<typeof insertVisual>[4];

export function RichMarkdownEditor({source, legacy, onChange, onHistory, onPasteMarkdown, onInsertAttachmentImage,
  attachments, readOnly, describedBy, id, ref, onCompositionChange}: EditorProps) {
  const editor = useRef<HTMLDivElement>(null);
  const selection = useRef<SourceRange | undefined>(undefined);
  const pending = useRef<{change: EditorChange; refreshMenu: boolean} | undefined>(undefined);
  const composition = useRef<{range: SourceRange; source: string; legacy: boolean} | undefined>(undefined);
  const committedComposition = useRef<string | undefined>(undefined);
  const typingMarks = useRef<Map<'bold' | 'italic', boolean>>(new Map());
  const [repair, setRepair] = useState(0);
  const [notice, setNotice] = useState('');
  const [menu, setMenu] = useState<(SlashMatch & {active: number}) | undefined>(undefined);
  const menuId = useId();
  const model = useMemo(() => parseMarkdown(source, legacy), [source, legacy]);
  const commands = menu ? availableCommands(menu.query, Boolean(onInsertAttachmentImage)) : [];

  function currentSelection(): SourceRange {
    return editor.current && selectedRange(editor.current, model) || selection.current || {start: 0, end: 0};
  }

  function apply(change: EditorChange, before = currentSelection(), kind: DraftEditKind = 'atomic', refreshMenu = false) {
    setNotice('');
    if (!refreshMenu) setMenu(undefined);
    if (source === change.source && legacy === change.legacy) {
      const root = editor.current;
      if (root && root.ownerDocument.activeElement === root) selectSource(root, model, change);
      return;
    }
    selection.current = change;
    pending.current = {change, refreshMenu};
    if (onChange(change,before,kind)===false) {
      pending.current=undefined;
      selection.current=before;
      if (editor.current) selectSource(editor.current,model,before);
    }
  }

  function focus() {
    const root = editor.current;
    if (!root) return;
    root.focus();
    selectSource(root, model, selection.current ?? {start: 0, end: 0});
  }

  function restoreSelection(range: SourceRange) {
    pending.current = undefined;
    selection.current = range;
    typingMarks.current.clear();
    setMenu(undefined);
    setNotice('');
    focus();
  }

  function selectCommand(command: MenuAction) {
    if (!menu || readOnly) return;
    setMenu(undefined);
    if (command === 'paste-markdown') onPasteMarkdown(menu.range);
    else if (command === 'attachment-image') onInsertAttachmentImage?.(menu.range);
    else apply(applySlashCommand(source, legacy, menu.range, command), currentSelection());
  }

  function menuAnchor() {
    const root = editor.current;
    if (!root || !menu) return;
    const point = nearestPoint(root, model, menu.range.end);
    if (!point) return;
    const range = root.ownerDocument.createRange();
    range.setStart(point.node, point.offset);
    range.collapse(true);
    const caret = range.getBoundingClientRect();
    return {caret: caret.height ? caret : point.node.parentElement!.getBoundingClientRect(), editor: root.getBoundingClientRect()};
  }

  function moveTableSelection(direction: number): boolean {
    const root = editor.current;
    const anchor = root?.ownerDocument.getSelection()?.anchorNode;
    const cell = anchor && runElement(anchor)?.closest<HTMLElement>('[data-markdown-cell-start]');
    const cells = cell && Array.from(cell.closest('[data-markdown-table]')!.querySelectorAll<HTMLElement>('[data-markdown-cell-start]'));
    if (!cell || !cells) return false;
    const target = cells[cells.indexOf(cell) + direction];
    if (!target) return false;
    const position = Number(target.dataset.markdownCellStart);
    restoreSelection({start: position, end: position});
    target.scrollIntoView({block: 'nearest', inline: 'nearest'});
    return true;
  }

  function leaveCode(range: SourceRange) {
    const change = exitCodeBlock(source, legacy, range);
    if (change.source === source && change.start === range.start) return false;
    editor.current?.focus();
    apply(change, range);
    return true;
  }

  function handleKey(event: KeyboardEvent<HTMLDivElement>) {
    if (readOnly || event.nativeEvent.isComposing || !belongsToEditor(event.currentTarget, event.target)) return;
    if (menu && event.key === 'Escape') {
      setMenu(undefined);
      event.preventDefault();
    } else if (menu && commands.length && (event.key === 'ArrowDown' || event.key === 'ArrowUp')) {
      const offset = event.key === 'ArrowDown' ? 1 : -1;
      setMenu({...menu, active: (menu.active + offset + commands.length) % commands.length});
      event.preventDefault();
    } else if (menu && event.key === 'Enter' && commands[menu.active] && !event.shiftKey) {
      selectCommand(commands[menu.active].id);
      event.preventDefault();
    } else if (event.key === 'Tab') {
      setMenu(undefined);
      if (moveTableSelection(event.shiftKey ? -1 : 1)) {
        event.preventDefault();
        return;
      }
      const range = currentSelection();
      const change = moveListItem(source, legacy, range, event.shiftKey ? 'outdent' : 'indent');
      if (change.source !== source) {
        event.preventDefault();
        apply(change, range);
      }
    } else if (event.key === 'Enter' && (event.metaKey || event.ctrlKey) && leaveCode(currentSelection())) {
      event.preventDefault();
    }
  }

  function runCommand(command: EditorCommand) {
    if (readOnly) return;
    const range = currentSelection();
    if (isProtected(model, range)) {
      setNotice(protectedEdit);
      return;
    }
    if (range.start === range.end && (command === 'bold' || command === 'italic')) {
      const current = model.runs.find(run => run.offsets.includes(range.start));
      const active = current?.marks.find(mark => mark.kind === command);
      const enabled = !(typingMarks.current.get(command) ?? Boolean(active));
      typingMarks.current.set(command, enabled);
      if (!enabled && active && range.start === active.closeStart) {
        selection.current = {start: active.closeEnd, end: active.closeEnd};
      } else {
        selection.current = range;
      }
      focus();
      return;
    }
    typingMarks.current.clear();
    const result = applyCommand(source, legacy, range, command);
    editor.current?.focus();
    if (result.source === source && (command === 'bold' || command === 'italic' || command === 'code')) {
      const selectedRuns = model.runs.filter(run => run.offsets[0] < range.end && range.start < run.offsets.at(-1)!);
      if (selectedRuns.some(run => !run.marks.some(mark => mark.kind === 'code'))) setNotice(structuralEdit);
      if (editor.current) selectSource(editor.current, model, range);
      return;
    }
    apply(result, range);
  }

  useImperativeHandle(ref, () => ({
    apply: runCommand, focus, restoreSelection, selection: currentSelection,
    selectedText() {
      const root = editor.current;
      return root && selectedRange(root, model) ? root.ownerDocument.getSelection()?.toString().replaceAll(caretPlaceholder, '') ?? '' : '';
    },
  }));

  useLayoutEffect(() => {
    const root = editor.current;
    const operation = pending.current;
    if (root && operation && source === operation.change.source && legacy === operation.change.legacy) {
      pending.current = undefined;
      if (root.ownerDocument.activeElement !== root) return;
      const next = operation.change;
      selectSource(root, model, next);
      if (operation.refreshMenu && next.start === next.end) {
        const block = model.blocks.find(item => item.bodyStart <= next.end && next.end <= item.end);
        const match = block && !block.locked && block.kind !== 'fence' && block.kind !== 'table' &&
          block.kind !== 'rule' && findSlashCommand(block.runs, next.end);
        setMenu(match ? {...match, active: 0} : undefined);
      }
    }
  }, [source, legacy, model, repair]);

  useLayoutEffect(() => {
    const root = editor.current;
    if (!root || readOnly) return;

    function rememberSelection() {
      const current = selectedRange(root!, model);
      if (current && !composition.current && !pending.current && root!.ownerDocument.activeElement === root) {
        const previous = selection.current;
        if (previous && (previous.start !== current.start || previous.end !== current.end)) typingMarks.current.clear();
        if (menu && (current.start !== menu.range.end || current.end !== menu.range.end)) setMenu(undefined);
        selection.current = current;
      }
    }

    function insert(range: SourceRange, value: string, kind: InsertionKind) {
      if (isProtected(model, range)) {
        setNotice(protectedEdit);
        return;
      }
      let result = insertVisual(source, legacy, range, value, kind);
      if (result.source === source && value && (range.start === range.end || kind === 'paragraph' || kind === 'line')) {
        setNotice(structuralEdit);
        return;
      }
      if (typingMarks.current.size && /\S/u.test(value) && !value.includes('\n') && result.start > range.start) {
        let textRange = {start: result.start - escapeLiteralText(value).length, end: result.end};
        for (const [command, enabled] of typingMarks.current) {
          const next = parseMarkdown(result.source, result.legacy);
          const styled = next.runs.some(run => run.offsets[0] <= textRange.start && textRange.end <= run.offsets.at(-1)!
            && run.marks.some(mark => mark.kind === command));
          if (styled === enabled) continue;
          const formatted = applyCommand(result.source, result.legacy, textRange, command);
          if (formatted.source === result.source) continue;
          result = formatted;
          textRange = formatted;
        }
        result = {...result, start: result.end};
      }
      apply(result, range, kind === 'typed' ? 'typing' : 'atomic', kind === 'typed');
    }

    function beforeInput(event: InputEvent) {
      if (!belongsToEditor(root, event.target) || !event.cancelable || composition.current || event.isComposing) return;
      const type = event.inputType;
      const current = selectedRange(root!, model) ?? currentSelection();
      if (type === 'insertText' && committedComposition.current !== undefined && event.data === committedComposition.current) {
        event.preventDefault();
        committedComposition.current = undefined;
        return;
      }
      event.preventDefault();
      if (type === 'historyUndo' || type === 'historyRedo') {
        onHistory(type === 'historyUndo' ? 'undo' : 'redo');
      } else if (type.startsWith('delete')) {
        const direction = type.endsWith('Forward') ? 'forward' : 'backward';
        const useTarget = type !== 'deleteContentBackward' && type !== 'deleteContentForward';
        const target = useTarget ? event.getTargetRanges()[0] : undefined;
        const range = target ? sourceRange(root!, model, target) ?? current : current;
        if (isProtected(model, range)) setNotice(protectedEdit);
        else apply(removeVisual(source, legacy, range, direction), range, 'atomic', Boolean(menu));
      } else if (type === 'insertParagraph') {
        insert(current, '\n', 'paragraph');
      } else if (type === 'insertLineBreak') {
        insert(current, '\n', 'line');
      } else if (type === 'insertText' || type === 'insertReplacementText' || type === 'insertFromPaste' || type === 'insertFromDrop') {
        const target = type === 'insertReplacementText' ? event.getTargetRanges()[0] : undefined;
        const range = target ? sourceRange(root!, model, target) ?? current : current;
        insert(range, event.data ?? event.dataTransfer?.getData('text/plain') ?? '', type === 'insertText' ? 'typed' : 'text');
      } else if (type === 'formatBold' || type === 'formatItalic') {
        runCommand(type === 'formatBold' ? 'bold' : 'italic');
      }
    }

    function paste(event: ClipboardEvent) {
      if (!belongsToEditor(root, event.target)) return;
      event.preventDefault();
      insert(selectedRange(root!, model) ?? currentSelection(), event.clipboardData?.getData('text/plain') ?? '', 'text');
    }

    function cut(event: ClipboardEvent) {
      if (!belongsToEditor(root, event.target)) return;
      event.preventDefault();
      const range = selectedRange(root!, model);
      if (!range || range.start === range.end) return;
      const selected = root!.ownerDocument.getSelection()?.toString() ?? '';
      event.clipboardData?.setData('text/plain', selected.replaceAll(caretPlaceholder, ''));
      if (isProtected(model, range)) setNotice(protectedEdit);
      else apply(removeVisual(source, legacy, range, 'backward'));
    }

    function compositionStart(event: CompositionEvent) {
      if (!belongsToEditor(root, event.target)) return;
      setMenu(undefined);
      composition.current = {range: selectedRange(root!, model) ?? currentSelection(), source, legacy};
      onCompositionChange?.(true);
    }

    function compositionEnd(event: CompositionEvent) {
      if (!belongsToEditor(root, event.target)) return;
      const captured = composition.current;
      composition.current = undefined;
      if (!captured) return;
      if (event.data) {
        committedComposition.current = event.data;
        insert(captured.range, event.data, 'text');
      }
      setRepair(old => old + 1);
      onCompositionChange?.(false);
      window.setTimeout(() => { committedComposition.current = undefined; }, 0);
    }

    function drop(event: DragEvent) {
      if (belongsToEditor(root, event.target)) event.preventDefault();
    }

    root.addEventListener('beforeinput', beforeInput);
    root.addEventListener('paste', paste);
    root.addEventListener('cut', cut);
    root.addEventListener('drop', drop);
    root.addEventListener('compositionstart', compositionStart);
    root.addEventListener('compositionend', compositionEnd);
    root.ownerDocument.addEventListener('selectionchange', rememberSelection);
    return () => {
      root.removeEventListener('beforeinput', beforeInput);
      root.removeEventListener('paste', paste);
      root.removeEventListener('cut', cut);
      root.removeEventListener('drop', drop);
      root.removeEventListener('compositionstart', compositionStart);
      root.removeEventListener('compositionend', compositionEnd);
      root.ownerDocument.removeEventListener('selectionchange', rememberSelection);
    };
  });

  return <>
    <div
      ref={editor}
      id={id}
      className="page-rich-editor"
      contentEditable={!readOnly}
      suppressContentEditableWarning
      role="textbox"
      tabIndex={readOnly ? 0 : undefined}
      aria-label="Содержимое страницы"
      aria-multiline="true"
      aria-readonly={readOnly || undefined}
      aria-describedby={describedBy}
      aria-controls={menu ? menuId : undefined}
      aria-activedescendant={menu && commands[menu.active] ? `${menuId}-${commands[menu.active].id}` : undefined}
      spellCheck={!readOnly}
      onKeyDown={handleKey}
      onPointerDown={() => setMenu(undefined)}
      onBlur={event => { if (event.target === event.currentTarget) setMenu(undefined); }}
    ><div key={repair}>
      <RichMarkdownBlocks model={model} readOnly={readOnly} attachments={attachments}
        onRemoveImage={({start, end}) => {
          editor.current?.focus();
          apply({source:source.slice(0,start) + source.slice(end), legacy, start, end:start}, currentSelection());
        }}
        onTask={block => {
          const range = blockCaret(block);
          apply(toggleTask(source, legacy, range), range);
        }}
        onLanguage={(block, value) => {
          const range = blockCaret(block);
          apply(updateCodeLanguage(source, legacy, range, value), range);
        }}
        onExitCode={block => leaveCode(blockCaret(block))} />
    </div></div>
    {menu && <EditorCommandMenu id={menuId} entries={commands} current={menu.active} anchor={menuAnchor} onSelect={selectCommand} />}
    {notice && <p className="page-rich-notice" role="status">{notice}</p>}
  </>;
}

function blockCaret(block: EditorBlock): SourceRange {
  return {start: block.bodyStart, end: block.bodyStart};
}
