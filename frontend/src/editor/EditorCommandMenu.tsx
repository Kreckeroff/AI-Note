import {useLayoutEffect, useRef, type KeyboardEvent} from 'react';
import {Icon} from '../Icon';
import type {BlockMenuCommand} from './editorChanges';

export type MenuAction = BlockMenuCommand | 'paste-markdown' | 'attachment-image' | 'embed';
type MenuEntry = {id: MenuAction; label: string; detail: string; icon: Parameters<typeof Icon>[0]['name']};
const entries: MenuEntry[] = [
  {id: 'heading', label: 'Заголовок', detail: 'H2 · Раздел', icon: 'file'},
  {id: 'list', label: 'Список', detail: 'Маркированные пункты', icon: 'list'},
  {id: 'quote', label: 'Цитата', detail: 'Выдержка из текста', icon: 'quote'},
  {id: 'code-block', label: 'Блок кода', detail: 'Многострочный код', icon: 'code'},
  {id: 'ordered', label: 'Нумерованный список', detail: 'Последовательность шагов', icon: 'ordered'},
  {id: 'task', label: 'Чекбокс', detail: 'Отметка о выполнении', icon: 'checkBox'},
  {id: 'rule', label: 'Разделитель', detail: 'Горизонтальная линия', icon: 'rule'},
  {id: 'table', label: 'Таблица', detail: 'Строки и столбцы', icon: 'table'},
  {id: 'embed', label: 'Встроить', detail: 'Видео или внешний iframe', icon: 'context'},
  {id: 'attachment-image', label: 'Изображение', detail: 'Картинка из вложений', icon: 'image'},
  {id: 'paste-markdown', label: 'Вставить как Markdown', detail: 'Фрагмент с разметкой', icon: 'clipboard'},
  {id: 'heading-one', label: 'Заголовок 1', detail: 'H1 · Основной раздел', icon: 'file'},
  {id: 'heading-three', label: 'Заголовок 3', detail: 'H3 · Подраздел', icon: 'file'},
];

export function availableCommands(query: string, attachments: boolean): MenuEntry[] {
  return entries.filter(entry => (entry.id !== 'attachment-image' || attachments) &&
    `${entry.label} ${entry.detail} ${entry.id}`.toLocaleLowerCase('ru').includes(query.trim().toLocaleLowerCase('ru')));
}

type MenuProps = {
  id: string;
  entries: MenuEntry[];
  current: number;
  anchor: () => {caret: DOMRect; editor: DOMRect} | undefined;
  onSelect(id: MenuAction): void;
  filter?: {query:string; change(query:string):void; onKeyDown(event:KeyboardEvent<HTMLInputElement>):void};
};

export function EditorCommandMenu({id, entries, current, anchor, onSelect, filter}: MenuProps) {
  const menu = useRef<HTMLDivElement>(null);
  const search = useRef<HTMLInputElement>(null);
  useLayoutEffect(() => { search.current?.focus({preventScroll:true}); }, []);
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
      panel.style.maxHeight = `${Math.max(36, Math.min(370, flip ? above : below))}px`;
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

  return <div ref={menu} className="page-command-menu"
    onMouseDown={event => { if (event.button === 0 && event.target !== search.current) event.preventDefault(); }}>
    {filter && <input ref={search} role="combobox" aria-label="Поиск блока" placeholder="Найти блок…"
      aria-expanded="true" aria-autocomplete="list" aria-controls={id}
      aria-activedescendant={entries[current] ? `${id}-${entries[current].id}` : undefined}
      value={filter.query} onChange={event => filter.change(event.currentTarget.value)} onKeyDown={filter.onKeyDown} />}
    <div id={id} role="listbox" aria-label="Вставить блок">
      {entries.map((entry, index) => <button key={entry.id} type="button" role="option" aria-label={entry.label}
        id={`${id}-${entry.id}`} aria-selected={index === current} tabIndex={-1} onClick={() => onSelect(entry.id)}>
        <Icon name={entry.icon} /><span>{entry.label}<small>{entry.detail}</small></span>
      </button>)}
    </div>
    {!entries.length && <span role="status">Команды не найдены</span>}
  </div>;
}
