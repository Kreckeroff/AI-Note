import {useEffect, useState, type Ref} from 'react';
import {collaborationJson, isPageAddress, queryTo, useCollaborationQuery} from '../collaboration/api';

export type PageLink = {title: string; section: string; path: string};
type PageLinkResults = {pages: PageLink[]; next: boolean};

function isPageLink(value: unknown, searchTo: string): value is PageLink {
  return Boolean(value && typeof value === 'object' && 'title' in value && typeof value.title === 'string' &&
    'section' in value && typeof value.section === 'string' && 'path' in value && isPageAddress(value.path) &&
    value.path.startsWith(searchTo.replace(/\/search$/, '/pages/')));
}

export async function canLinkToPage(searchTo: string, link: PageLink, signal: AbortSignal): Promise<boolean> {
  const target = link.path.slice(link.path.lastIndexOf('/') + 1);
  const result = await collaborationJson<PageLinkResults>(queryTo(searchTo, {kind: 'page-links', target}), {signal});
  return Array.isArray(result?.pages) && result.pages.some(page => isPageLink(page, searchTo) && page.path === link.path);
}

export function PageLinkPicker({searchTo, selection, onSelect, refresh, inputRef, disabled, errorId}: {
  searchTo: string; selection: PageLink | null; onSelect(page: PageLink | null): void;
  refresh: number; inputRef: Ref<HTMLInputElement>; disabled: boolean; errorId?: string;
}) {
  const [query, setQuery] = useState('');
  const [settled, setSettled] = useState('');
  const [page, setPage] = useState(1);
  const [revision, setRevision] = useState(0);
  const term = query.trim();
  useEffect(() => {
    const timer = window.setTimeout(() => setSettled(term), 180);
    return () => window.clearTimeout(timer);
  }, [term]);
  const href = term && term === settled ? queryTo(searchTo, {kind: 'page-links', q: term, page}) : null;
  const result = useCollaborationQuery<PageLinkResults>(href, revision + refresh);
  const pages = Array.isArray(result?.data?.pages) ? result.data.pages.filter(item => isPageLink(item, searchTo)) : [];
  const message = !term ? 'Начните вводить название страницы.' : !result ? 'Ищем страницы…' :
    result.error ? null : pages.length ? null : 'Страницы с таким названием не найдены.';

  function changePage(next: number) {
    onSelect(null);
    setPage(next);
  }

  return <div className="page-link-picker">
    <label className="page-draft-field">Название страницы
      <input type="search" ref={inputRef} value={query} maxLength={100} autoComplete="off" disabled={disabled}
        aria-describedby={errorId} onChange={event => {
          setQuery(event.currentTarget.value);
          setPage(1);
          setRevision(value => value + 1);
          onSelect(null);
        }} onKeyDown={event => {
          if (event.key === 'Enter' && !event.nativeEvent.isComposing) {
            event.preventDefault();
            event.currentTarget.closest('.page-link-picker')?.querySelector<HTMLInputElement>(
              'input[type="radio"]:checked, input[type="radio"]',
            )?.focus();
          }
        }} />
    </label>
    {message && <p className="space-hint" role="status">{message}</p>}
    {result?.error && <div className="page-link-pagination">
      <p className="form-error" role="alert">{result.error}</p>
      <button type="button" className="space-action space-secondary" disabled={disabled}
        onClick={() => setRevision(value => value + 1)}>Повторить поиск</button>
    </div>}
    {pages.length > 0 && <ul className="page-link-results" aria-label="Результаты поиска страниц">
      {pages.map(item => <li key={item.path}>
        <label className="page-link-option">
          <input type="radio" name="link-page" value={item.path} checked={selection?.path === item.path}
            onChange={() => onSelect(item)} disabled={disabled} />
          <span><strong>{item.title}</strong><small>{item.section}</small></span>
        </label>
      </li>)}
    </ul>}
    {result?.data && (page > 1 || result.data.next) && <div className="page-link-pagination">
      {page > 1 && <button type="button" className="space-action space-secondary" disabled={disabled}
        onClick={() => changePage(page - 1)}>Назад</button>}
      <span className="space-hint">Страница {page}</span>
      {result.data.next && <button type="button" className="space-action space-secondary" disabled={disabled}
        onClick={() => changePage(page + 1)}>Дальше</button>}
    </div>}
  </div>;
}
