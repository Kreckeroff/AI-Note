import {useId, useLayoutEffect, useRef, useState, type KeyboardEvent} from 'react';
import {Icon} from '../Icon';
import {availableCommands, EditorCommandMenu, type MenuAction} from './EditorCommandMenu';
import type {SourceRange} from './markdownModel';

type Props = {
  source: string;
  disabled: boolean;
  attachments: boolean;
  active: boolean;
  onFocus(): void;
  selection(): SourceRange;
  onChoose(command: MenuAction, range: SourceRange): void;
};

export function BlockInsertButton({source, disabled, attachments, active, onFocus, selection, onChoose}: Props) {
  const id = useId();
  const button = useRef<HTMLButtonElement>(null);
  const container = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState<{source:string; range:SourceRange}>();
  const [query, setQuery] = useState('');
  const [current, setCurrent] = useState(0);
  const commands = availableCommands(query, attachments);

  useLayoutEffect(() => {
    if (!open) return;
    if (disabled || source !== open.source) {
      setOpen(undefined);
      if (container.current?.contains(document.activeElement)) button.current?.focus({preventScroll:true});
      return;
    }
    function outside(event: PointerEvent) {
      if (event.target instanceof Node && !container.current?.contains(event.target)) setOpen(undefined);
    }
    document.addEventListener('pointerdown', outside);
    return () => document.removeEventListener('pointerdown', outside);
  }, [open, source, disabled]);

  function close() {
    setOpen(undefined);
    button.current?.focus({preventScroll:true});
  }

  function choose(command: MenuAction) {
    if (!open || disabled || open.source !== source) return;
    close();
    onChoose(command, open.range);
  }

  function filterKey(event: KeyboardEvent<HTMLInputElement>) {
    if (event.nativeEvent.isComposing) return;
    if (event.key === 'Tab') {
      close();
      return;
    }
    if (event.key === 'Escape') close();
    else if (event.key === 'Enter') {
      if (commands[current]) choose(commands[current].id);
    } else if (commands.length && (event.key === 'ArrowUp' || event.key === 'ArrowDown')) {
      const delta = event.key === 'ArrowDown' ? 1 : -1;
      setCurrent((current + delta + commands.length) % commands.length);
    } else return;
    event.preventDefault();
    event.stopPropagation();
  }

  function anchor() {
    const caret = button.current?.getBoundingClientRect();
    const editor = container.current?.closest('.page-editor-toolbar')?.getBoundingClientRect();
    return caret && editor ? {caret, editor} : undefined;
  }

  return <div ref={container} onBlur={event => {
    if (event.relatedTarget && !event.currentTarget.contains(event.relatedTarget)) setOpen(undefined);
  }}>
    <button ref={button} data-markdown-tool="" type="button" disabled={disabled} aria-label="Вставить блок"
      title="Вставить блок" aria-haspopup="listbox" aria-expanded={Boolean(open)} aria-controls={open ? id : undefined}
      tabIndex={active ? 0 : -1} onFocus={onFocus} onClick={() => {
        if (open) close();
        else {
          setQuery('');
          setCurrent(0);
          setOpen({source, range:selection()});
        }
      }}>
      <Icon name="add" /><span className="page-tool-label">Блок</span>
    </button>
    {open && open.source === source && !disabled && <EditorCommandMenu id={id} entries={commands} current={current}
      anchor={anchor} onSelect={choose} filter={{query, change:value => {setCurrent(0); setQuery(value);}, onKeyDown:filterKey}} />}
  </div>;
}
