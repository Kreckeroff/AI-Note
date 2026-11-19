import {useLayoutEffect, useRef} from 'react';
import {Icon} from '../Icon';
import type {EditorCommand} from './markdownModel';

export type MenuAction = EditorCommand | 'paste-markdown' | 'attachment-image';
type MenuEntry = {id: MenuAction; label: string; icon: Parameters<typeof Icon>[0]['name']};
const entries: MenuEntry[] = [
  {id: 'heading', label: 'Заголовок', icon: 'file'},
  {id: 'list', label: 'Список', icon: 'list'},
  {id: 'quote', label: 'Цитата', icon: 'quote'},
  {id: 'code-block', label: 'Блок кода', icon: 'code'},
  {id: 'ordered', label: 'Нумерованный список', icon: 'ordered'},
  {id: 'task', label: 'Чекбокс', icon: 'check'},
  {id: 'rule', label: 'Разделитель', icon: 'rule'},
  {id: 'table', label: 'Таблица', icon: 'table'},
  {id: 'attachment-image', label: 'Изображение', icon: 'image'},
  {id: 'paste-markdown', label: 'Вставить как Markdown', icon: 'clipboard'},
];

export function availableCommands(query: string, attachments: boolean): MenuEntry[] {
  return entries.filter(entry => (entry.id !== 'attachment-image' || attachments) &&
    `${entry.label} ${entry.id}`.toLocaleLowerCase('ru').includes(query));
}

type MenuProps = {
  id: string;
  entries: MenuEntry[];
  current: number;
  anchor: () => {caret: DOMRect; editor: DOMRect} | undefined;
  onSelect(id: MenuAction): void;
};

export function EditorCommandMenu({id, entries, current, anchor, onSelect}: MenuProps) {
  const menu = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const node = menu.current;
    if (!node) return;
    const panel = node;
    function position() {
      const measured = anchor();
      if (!measured) return;
      const {caret, editor} = measured;
      const edge = 8;
      const viewport = window.visualViewport;
      const left = viewport?.offsetLeft ?? 0;
      const right = left + (viewport?.width ?? window.innerWidth);
      const top = viewport?.offsetTop ?? 0;
      const bottom = top + (viewport?.height ?? window.innerHeight);
      const minLeft = Math.max(left + edge, editor.left);
      const maxRight = Math.min(right - edge, editor.right);
      const width = Math.min(258, Math.max(0, maxRight - minLeft));
      panel.style.width = `${width}px`;
      panel.style.left = `${Math.max(minLeft, Math.min(caret.left, maxRight - width))}px`;
      const above = Math.max(0, caret.top - top - edge);
      const below = Math.max(0, bottom - caret.bottom - edge);
      const flip = panel.scrollHeight > below && above > below;
      panel.style.maxHeight = `${Math.max(36, flip ? above : below)}px`;
      panel.style.top = `${flip ? Math.max(top, caret.top - panel.offsetHeight - edge) : caret.bottom + edge}px`;
      const selected = panel.querySelector('[aria-selected="true"]');
      if (selected instanceof HTMLElement) {
        if (selected.offsetTop < panel.scrollTop) panel.scrollTop = selected.offsetTop;
        else if (selected.offsetTop + selected.offsetHeight > panel.scrollTop + panel.clientHeight) {
          panel.scrollTop = selected.offsetTop + selected.offsetHeight - panel.clientHeight;
        }
      }
    }
    position();
    window.addEventListener('scroll', position, {capture: true, passive: true});
    window.addEventListener('resize', position);
    window.visualViewport?.addEventListener('resize', position);
    window.visualViewport?.addEventListener('scroll', position);
    return () => {
      window.removeEventListener('scroll', position, true);
      window.removeEventListener('resize', position);
      window.visualViewport?.removeEventListener('resize', position);
      window.visualViewport?.removeEventListener('scroll', position);
    };
  }, [anchor, current, entries]);

  return <div ref={menu} id={id} role="listbox" aria-label="Вставить блок" className="page-command-menu"
    onMouseDown={event => { if (event.button === 0) event.preventDefault(); }}>
    {entries.length ? entries.map((entry, index) => <button key={entry.id} type="button" role="option"
      id={`${id}-${entry.id}`} aria-selected={index === current} tabIndex={-1} onClick={() => onSelect(entry.id)}>
      <Icon name={entry.icon} />{entry.label}
    </button>) : <span role="status">Команды не найдены</span>}
  </div>;
}
