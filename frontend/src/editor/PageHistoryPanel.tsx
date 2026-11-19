import {useEffect, useId, useLayoutEffect, useRef, useState, useSyncExternalStore} from 'react';
import {Icon} from '../Icon';
import {historySnapshotHref, readPageHistory, type PageHistory} from './pageHistory';

const narrowScreen = '(max-width: 890px)';
const historyDate = new Intl.DateTimeFormat('ru-RU', {dateStyle: 'medium', timeStyle: 'short'});

function subscribeToScreen(onChange: () => void) {
  const screen = window.matchMedia(narrowScreen);
  screen.addEventListener('change', onChange);
  return () => screen.removeEventListener('change', onChange);
}

type ListState = {
  phase: 'loading' | 'ready' | 'unavailable' | 'error';
  data: PageHistory | null;
};

type ListRequest = {href: string; previous: PageHistory | null};

export function PageHistoryPanel({id, historyTo, editing, onDismiss}: {
  id: string;
  historyTo: string;
  editing: boolean;
  onDismiss(): void;
}) {
  const titleId = useId();
  const overlay = useSyncExternalStore(subscribeToScreen, () => window.matchMedia(narrowScreen).matches, () => false);
  const dialog = useRef<HTMLDialogElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const listElement = useRef<HTMLUListElement>(null);
  const loadButton = useRef<HTMLButtonElement>(null);
  const loadFocusIndex = useRef<number | null>(null);
  const [request, setRequest] = useState<ListRequest>({href: historyTo, previous: null});
  const [list, setList] = useState<ListState>({phase: 'loading', data: null});

  useLayoutEffect(() => {
    const panel = dialog.current;
    panel?.showModal();
    closeButton.current?.focus({preventScroll: true});
    return () => panel?.close();
  }, [overlay]);

  useLayoutEffect(() => {
    if (list.phase === 'loading' || loadFocusIndex.current === null) return;
    const replacement = listElement.current?.querySelectorAll('a')[loadFocusIndex.current];
    const active = document.activeElement;
    if (list.phase === 'ready' && (active === loadButton.current || active === document.body)) {
      if (replacement) replacement.focus();
      else if (!loadButton.current) closeButton.current?.focus({preventScroll: true});
    }
    loadFocusIndex.current = null;
  }, [list]);

  useEffect(() => {
    const controller = new AbortController();
    const {href, previous} = request;
    const {signal} = controller;

    async function fetchHistory() {
      try {
        const response = await fetch(href, {
          headers: {Accept: 'application/json'}, redirect: 'manual', cache: 'no-store', signal,
        });
        if (signal.aborted) return;
        if (response.type === 'opaqueredirect' || [401, 403, 404].includes(response.status)) {
          setList({phase: 'unavailable', data: null});
          return;
        }
        if (!response.ok) throw new Error('Page history request failed');

        const page = readPageHistory(await response.json(), historyTo);
        if (signal.aborted) return;
        if (!page) throw new Error('Invalid page history response');

        // When another editor publishes, reopen the first batch to move the current marker.
        if (previous && previous.currentRevision !== page.currentRevision) {
          if (loadFocusIndex.current !== null) loadFocusIndex.current = 0;
          setList({phase: 'loading', data: null});
          setRequest({href: historyTo, previous: null});
          return;
        }
        const cursor = previous?.items.at(-1)?.revision;
        if (cursor && page.items.some(item => BigInt(item.revision) >= BigInt(cursor))) {
          throw new Error('Page history cursor did not advance');
        }
        setList({phase: 'ready', data: {
          ...page, items: previous ? [...previous.items, ...page.items] : page.items,
        }});
      } catch {
        if (!signal.aborted) setList({phase: 'error', data: previous});
      }
    }

    void fetchHistory();
    return () => controller.abort();
  }, [historyTo, request]);

  function more() {
    if (!list.data?.nextTo || list.phase !== 'ready') return;
    if (document.activeElement === loadButton.current) loadFocusIndex.current = list.data.items.length;
    setRequest({href: list.data.nextTo, previous: list.data});
    setList({phase: 'loading', data: list.data});
  }

  function retry() {
    const nextRequest = list.phase === 'error' ? {...request} : {href: historyTo, previous: null};
    if (document.activeElement === loadButton.current) loadFocusIndex.current = nextRequest.previous?.items.length ?? 0;
    setRequest(nextRequest);
    setList({phase: 'loading', data: nextRequest.previous});
  }

  const failed = list.phase === 'error' || list.phase === 'unavailable';
  const empty = list.phase === 'ready' && list.data?.items.length === 0;

  const contents = <>
    <header className="document-history-header">
      <h2 id={titleId}>История</h2>
      <button ref={closeButton} type="button" className="space-action space-close" aria-label="Закрыть историю" onClick={onDismiss}>
        <Icon name="close" />
      </button>
    </header>
    <div className="document-history-body">
      {editing && <p className="document-history-note">Редакции открываются в новой вкладке.</p>}
      {list.data && list.data.items.length > 0 && (
        <ul ref={listElement} className="document-history-list">
          {list.data.items.map(item => {
            const current = item.revision === list.data?.currentRevision;
            return <li key={item.revision}>
              <a className="document-history-entry" href={historySnapshotHref(item.href, editing)}
                aria-current={current ? 'true' : undefined} target={editing ? '_blank' : undefined}
                rel={editing ? 'noopener' : undefined}>
                <span className="document-history-glyph"><Icon name="history" /></span>
                <span>
                  <strong>Редакция {item.revision}</strong>
                  <small>
                    {item.author || item.recordedAt ? <>
                      {item.author}{item.author && item.recordedAt ? ' · ' : ''}
                      {item.recordedAt && <time dateTime={item.recordedAt}>{historyDate.format(new Date(item.recordedAt))}</time>}
                    </> : 'Исходный снимок'}
                  </small>
                </span>
                {current && <span className="document-history-current">Текущая</span>}
              </a>
            </li>;
          })}
        </ul>
      )}
      <p className="document-history-note document-history-status" role="status">
        {list.phase === 'loading' ? 'Загружаем историю…' : empty ? 'Пока нет записанных редакций.' : ''}
      </p>
      {failed && (
        <p className="document-history-note" role="alert">
          {list.phase === 'error' ? 'Не удалось загрузить историю.' : 'История недоступна.'}
        </p>
      )}
      {(failed || list.data?.nextTo) && (
        <button ref={loadButton} type="button" className="space-action space-secondary"
          aria-disabled={list.phase === 'loading' || undefined} onClick={failed ? retry : more}>
          {failed ? 'Повторить попытку' : 'Показать ещё'}
        </button>
      )}
    </div>
  </>;

  return overlay ? (
    <dialog ref={dialog} id={id} className="document-history-overlay" aria-labelledby={titleId}
      onCancel={event => { event.preventDefault(); onDismiss(); }}>
      {contents}
    </dialog>
  ) : (
    <aside id={id} className="document-history-dock" aria-labelledby={titleId} onKeyDown={event => {
      if (event.key === 'Escape') { event.preventDefault(); onDismiss(); }
    }}>
      {contents}
    </aside>
  );
}
