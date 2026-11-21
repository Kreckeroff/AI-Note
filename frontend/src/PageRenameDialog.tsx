import {useEffect, useId, useLayoutEffect, useRef, useState, type FormEvent} from 'react';
import {Icon} from './Icon';

export type PageName = {title: string; revision: string};

function isPageName(value: unknown): value is PageName {
  return typeof value === 'object' && value !== null && 'title' in value && 'revision' in value &&
    typeof value.title === 'string' && typeof value.revision === 'string' && /^[1-9]\d{0,18}$/u.test(value.revision);
}

const unavailable = 'Не удалось связаться с «Севером». Попробуйте ещё раз.';

async function responseError(response: Response): Promise<string> {
  if (response.redirected || response.status === 401) return 'Войдите в «Север» ещё раз.';
  if (response.status === 403) return 'У вас нет права переименовывать эту страницу.';
  if (response.status === 404) return 'Страница недоступна.';
  if (response.status === 422 && response.headers.get('content-type')?.startsWith('text/plain')) {
    return response.text();
  }
  return unavailable;
}

export function PageRenameDialog({pageTo, onRename, onDismiss}: {
  pageTo: string;
  onRename(name: PageName): void;
  onDismiss(): void;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const review = useRef<HTMLButtonElement>(null);
  const saving = useRef(false);
  const heading = useId();
  const nameId = useId();
  const helpId = useId();
  const [base, setBase] = useState<PageName>();
  const [conflict, setConflict] = useState<PageName>();
  const [title, setTitle] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const [loadAttempt, setLoadAttempt] = useState(0);
  const to = `${pageTo}/rename`;

  useLayoutEffect(() => { dialog.current?.showModal(); }, []);
  useEffect(() => {
    const request = new AbortController();
    async function load() {
      try {
        const response = await fetch(to, {signal: request.signal});
        if (!response.ok || response.redirected) {
          const message = await responseError(response);
          if (!request.signal.aborted) setError(message);
          return;
        }
        const name: unknown = await response.json();
        if (!isPageName(name)) throw new Error(unavailable);
        if (!request.signal.aborted) {
          setBase(name);
          setTitle(name.title);
        }
      } catch {
        if (!request.signal.aborted) setError(unavailable);
      }
    }
    void load();
    return () => request.abort();
  }, [to, loadAttempt]);

  const loaded = base !== undefined;
  useLayoutEffect(() => {
    if (loaded) {
      input.current?.focus();
      input.current?.select();
    }
  }, [loaded]);
  useLayoutEffect(() => { if (conflict) review.current?.focus(); }, [conflict]);

  async function save(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!base || conflict || saving.current) return;
    const trimmed = title.trim();
    if (!trimmed || Array.from(trimmed).length > 100) {
      setError('Введите название от 1 до 100 символов.');
      input.current?.focus();
      return;
    }
    saving.current = true;
    setBusy(true);
    setError('');
    try {
      const response = await fetch(to, {
        method: 'POST', body: new URLSearchParams({title: trimmed, base_revision: base.revision}),
      });
      if ((!response.ok && response.status !== 409) || response.redirected) {
        setError(await responseError(response));
        return;
      }
      const name: unknown = await response.json();
      if (!isPageName(name)) throw new Error(unavailable);
      if (response.status === 409) setConflict(name);
      else onRename(name);
    } catch {
      setError(unavailable);
    } finally {
      saving.current = false;
      setBusy(false);
    }
  }

  return <dialog ref={dialog} className="space-dialog page-rename-dialog" aria-labelledby={heading}
    onCancel={event => { event.preventDefault(); if (!saving.current) onDismiss(); }}>
    <header className="space-dialog-header">
      <h2 id={heading}>Переименовать страницу</h2>
      <button className="space-action space-close" type="button" aria-label="Закрыть" disabled={busy}
        onClick={onDismiss}><Icon name="close" /></button>
    </header>
    <form className="space-form" noValidate onSubmit={save} aria-busy={busy || (!base && !error)}>
      {base ? <>
        <label htmlFor={nameId}>Название</label>
        <input ref={input} id={nameId} name="title" value={title} required readOnly={busy}
          aria-describedby={helpId} onChange={event => { setTitle(event.currentTarget.value); setError(''); }} />
        <p className="space-hint" id={helpId}>От 1 до 100 символов. Название должно быть уникальным внутри раздела.</p>
      </> : !error && <p role="status">Загружаем название…</p>}
      {error && <p className="form-error" role="alert">{error}</p>}
      {conflict && <div className="page-rename-conflict">
        <p role="alert">Страница обновилась. Сейчас она называется «{conflict.title}».</p>
        <button ref={review} className="space-action space-secondary" type="button" onClick={() => {
          setBase(conflict);
          setConflict(undefined);
          input.current?.focus();
        }}>Продолжить с моей правкой</button>
      </div>}
      <div className="space-dialog-actions">
        <button className="space-action space-secondary" type="button" disabled={busy} onClick={onDismiss}>Отмена</button>
        {base ? <button className="space-action primary-action" type="submit" disabled={busy || Boolean(conflict)}>
          {busy ? 'Сохраняем…' : 'Сохранить название'}
        </button> : error && <button className="space-action primary-action" type="button" onClick={() => {
          setError('');
          setLoadAttempt(attempt => attempt + 1);
        }}>Повторить</button>}
      </div>
    </form>
  </dialog>;
}
