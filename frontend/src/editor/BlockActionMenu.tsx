import {useId, useLayoutEffect, useMemo, useRef, useState, type RefObject} from 'react';
import {Icon} from '../Icon';
import {changeBlock, type BlockAction} from './blockActions';
import {blockUnits, type MarkdownDocument} from './markdownModel';
import type {EditorChange} from './editorChanges';

const choices = [
  ['up','Переместить выше','down'],
  ['down','Переместить ниже','down'],
  ['duplicate','Дублировать блок','clipboard'],
  ['remove','Удалить блок','close'],
] as const;

type Props = {
  editor: RefObject<HTMLDivElement | null>;
  model: MarkdownDocument;
  source: string;
  legacy: boolean;
  onChange(change: EditorChange): void;
  onRefocus(position: number): void;
};

export function BlockActionMenu({editor,model,source,legacy,onChange,onRefocus}: Props) {
  const units = useMemo(() => blockUnits(model,source,legacy),[model,source,legacy]);
  const [anchor,setAnchor] = useState<{index:number; top:number}>();
  const [opened,setOpened] = useState<{model: MarkdownDocument; index: number}>();
  const trigger = useRef<HTMLButtonElement>(null);
  const panel = useRef<HTMLDivElement>(null);
  const remembered = useRef<{model: MarkdownDocument; index: number}>(undefined);
  const [feedback,setFeedback] = useState('');
  const id = useId();
  const open = opened?.model === model;
  const index = open ? opened.index : anchor?.index ?? -1;
  const unit = units[index];

  useLayoutEffect(() => {
    const root = editor.current;
    if (!root) return;
    let active = units[remembered.current?.model === model ? remembered.current.index : 0];
    function position() {
      const first = active && root!.querySelector<HTMLElement>(`[data-editor-block="${active.start}"]`);
      if (!first) { setAnchor(undefined); return; }
      const top = Math.round(first.getBoundingClientRect().top - root!.parentElement!.getBoundingClientRect().top);
      const index = units.indexOf(active);
      remembered.current = {model,index};
      setAnchor(old => old?.index === index && old.top === top ? old : {index,top});
    }
    function track(node: EventTarget | Node | null) {
      const element = node instanceof Element ? node : node instanceof Node ? node.parentElement : undefined;
      const block = element?.closest<HTMLElement>('[data-editor-block]');
      if (!block || !root!.contains(block)) return;
      const offset = Number(block.dataset.editorBlock);
      const next = units.find(item => item.start <= offset && offset < item.end);
      if (next && next !== active) { active = next; position(); }
    }
    function selection() {
      if (root!.ownerDocument.activeElement === root) track(root!.ownerDocument.getSelection()?.focusNode ?? null);
    }
    function pointer(event: PointerEvent) { if (!open) track(event.target); }
    function shortcut(event: KeyboardEvent) {
      if (event.key !== 'F10' || !event.altKey || event.isComposing) return;
      event.preventDefault(); selection(); trigger.current?.focus();
    }
    if (open && opened) active = units[opened.index];
    else selection();
    position();
    const resize = new ResizeObserver(position);
    resize.observe(root);
    root.addEventListener('pointerdown',pointer);
    root.addEventListener('keydown',shortcut);
    root.ownerDocument.addEventListener('selectionchange',selection);
    return () => {
      resize.disconnect();
      root.removeEventListener('pointerdown',pointer);
      root.removeEventListener('keydown',shortcut);
      root.ownerDocument.removeEventListener('selectionchange',selection);
    };
  },[editor,units,model,open,opened]);

  useLayoutEffect(() => {
    if (!open) return;
    (panel.current?.querySelector<HTMLElement>('button:not(:disabled)') ?? panel.current)?.focus({preventScroll:true});
    function outside(event: PointerEvent) {
      if (event.target instanceof Node && !trigger.current?.parentElement?.contains(event.target)) setOpened(undefined);
    }
    document.addEventListener('pointerdown',outside);
    return () => document.removeEventListener('pointerdown',outside);
  },[open]);

  if (!unit || !anchor) return null;
  function run(action: BlockAction) {
    if (!open) return;
    const result = changeBlock(source,legacy,units,index,action);
    if (!result) {
      setFeedback('Команда изменит структуру соседних блоков. Используйте «Исходник».');
      return;
    }
    setOpened(undefined);
    onChange(result);
  }
  return <div className="block-actions" style={{top:anchor.top}}
    onBlur={event => {if (!event.currentTarget.contains(event.relatedTarget)) setOpened(undefined);}}
    onKeyDown={event => {
      if (event.key !== 'Escape') return;
      event.preventDefault();
      event.stopPropagation();
      if (open) {
        setOpened(undefined);
        trigger.current?.focus();
      } else onRefocus(unit.caret);
    }}>
    <button type="button" ref={trigger} className="block-actions-trigger" aria-label="Управление блоком" title="Управление блоком · Alt+F10"
      aria-keyshortcuts="Alt+F10" aria-expanded={open} aria-controls={open ? id : undefined}
      onClick={() => {
        setFeedback('');
        const selected = remembered.current?.model === model ? remembered.current.index : index;
        setOpened(open ? undefined : {model,index:selected});
      }}>
      <Icon name="grip" />
    </button>
    {open && <div ref={panel} className="block-actions-panel" id={id} role="group" aria-label="Команды блока" tabIndex={-1}>
      {unit.safe ? choices.map(([action,label,icon]) => <button type="button" key={action} data-block-action={action}
        disabled={(action === 'up' && !units[index-1]?.safe) || (action === 'down' && !units[index+1]?.safe)}
        onClick={() => run(action)}><Icon name={icon}/>{label}</button>) :
        <span>У этого фрагмента сложная разметка. Измените порядок в «Исходнике».</span>}
      {feedback && <span role="status">{feedback}</span>}
    </div>}
  </div>;
}
