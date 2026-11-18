import {useEffect, useRef, useState, type FormEvent} from 'react';
import type {PageEditorForm} from './model';

const maxTextLength = 30_000;

export function PageEditor({form}: {form: PageEditorForm}) {
  const {action, cancelTo, text, revision, error, conflict, canSave} = form;
  const input = useRef<HTMLTextAreaElement>(null);
  const submitting = useRef(false);
  const [saving, setSaving] = useState(false);
  const [clientError, setClientError] = useState('');
  const [rebase, setRebase] = useState(false);
  const visibleError = clientError || error;

  useEffect(() => {
    function restoreFromHistory(event: PageTransitionEvent) {
      if (!event.persisted) return;
      submitting.current = false;
      setSaving(false);
    }
    window.addEventListener('pageshow', restoreFromHistory);
    return () => window.removeEventListener('pageshow', restoreFromHistory);
  }, []);

  function onSubmit(event: FormEvent<HTMLFormElement>) {
    if (!canSave || submitting.current || (conflict && !rebase)) {
      event.preventDefault();
      return;
    }
    const value = input.current?.value ?? '';
    if (Array.from(value).length > maxTextLength) {
      event.preventDefault();
      setClientError('Введите не больше 30 000 символов.');
      input.current?.focus();
      return;
    }
    submitting.current = true;
    setSaving(true);
  }

  return (
    <form className="page-editor" method="post" action={action} onSubmit={onSubmit}>
      <input type="hidden" name="revision" value={revision} />
      {visibleError && <p className="form-error" id="page-editor-error" role="alert">{visibleError}</p>}
      <div className={conflict ? 'page-editor-columns page-editor-columns-conflict' : 'page-editor-columns'}>
        <div className="page-editor-field">
          <label htmlFor="page-text">Текст страницы</label>
          <textarea id="page-text" name="text" rows={12} ref={input} defaultValue={text} autoFocus
            readOnly={!canSave || saving} aria-invalid={Boolean(clientError) || undefined}
            aria-describedby={visibleError ? 'page-editor-error' : undefined}
            onInput={() => setClientError('')} />
        </div>
        {conflict && (
          <section className="page-editor-current" aria-labelledby="page-editor-current-heading">
            <h2 id="page-editor-current-heading">Актуальный текст</h2>
            <div className="page-editor-current-text" tabIndex={0} aria-labelledby="page-editor-current-heading">
              {conflict.text || 'Пока здесь ничего нет.'}
            </div>
          </section>
        )}
      </div>
      {conflict && canSave && (
        <label className="page-editor-rebase">
          <input type="checkbox" name="baseRevision" value={conflict.revision} checked={rebase}
            onClick={event => { if (submitting.current) event.preventDefault(); }}
            onChange={event => setRebase(event.currentTarget.checked)} />
          Использовать эту версию как основу
        </label>
      )}
      <div className="page-editor-actions">
        <span className="page-editor-status" role="status">{saving ? 'Сохраняем…' : ''}</span>
        {cancelTo && (
          <a className="space-action space-secondary" href={cancelTo} aria-disabled={saving || undefined}
            onClick={event => { if (submitting.current) event.preventDefault(); }}>
            {canSave ? 'Отменить' : 'Вернуться к чтению'}
          </a>
        )}
        {canSave && (
          <button className="space-action primary-action" type="submit" disabled={saving || Boolean(conflict && !rebase)}>
            Сохранить
          </button>
        )}
      </div>
    </form>
  );
}
