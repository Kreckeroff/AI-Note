import {useEffect, useLayoutEffect, useRef, useState, type FormEvent, type MouseEvent} from 'react';
import {Icon} from './Icon';
import type {SectionFormModel} from './model';

const maxNameLength = 100;
const maxFormBytes = 4096;

export function SectionDialog({form: fields, spaceName}: {form: SectionFormModel; spaceName: string}) {
  const {operationId, title, error, invalidName, cancelTo, action, retryTo} = fields;
  const dialog = useRef<HTMLDialogElement>(null);
  const form = useRef<HTMLFormElement>(null);
  const input = useRef<HTMLInputElement>(null);
  const submitting = useRef(false);
  const [saving, setSaving] = useState(false);
  const [clientError, setClientError] = useState('');

  useLayoutEffect(() => {
    const element = dialog.current;
    if (!element) return;
    if (typeof element.showModal === 'function') element.showModal();
    else element.open = true;
    input.current?.focus();
  }, []);

  useEffect(() => {
    const element = form.current;
    if (!element) return;

    function trimOversizedPadding({formData}: FormDataEvent) {
      const value = formData.get('name');
      if (typeof value !== 'string') return;
      const trimmed = value.trim();
      if (!trimmed || value === trimmed || Array.from(trimmed).length > maxNameLength) return;
      const encoded = new URLSearchParams([['operation_id', operationId], ['name', value]]);
      if (encoded.toString().length > maxFormBytes) formData.set('name', trimmed);
    }

    function resetSaving() {
      submitting.current = false;
      setSaving(false);
    }

    element.addEventListener('formdata', trimOversizedPadding);
    window.addEventListener('pageshow', resetSaving);
    return () => {
      element.removeEventListener('formdata', trimOversizedPadding);
      window.removeEventListener('pageshow', resetSaving);
    };
  }, [operationId]);

  function onSubmit(event: FormEvent<HTMLFormElement>) {
    if (submitting.current) {
      event.preventDefault();
      return;
    }
    const name = input.current?.value.trim() ?? '';
    if (!name || Array.from(name).length > maxNameLength) {
      event.preventDefault();
      setClientError(name ? 'Введите не больше 100 символов.' : 'Введите название раздела.');
      input.current?.focus();
      return;
    }
    submitting.current = true;
    setSaving(true);
  }

  function onCancelLink(event: MouseEvent<HTMLAnchorElement>) {
    if (submitting.current) event.preventDefault();
  }

  const description = clientError ? 'section-help section-client-error' :
    error ? 'section-help section-error' : 'section-help';

  return (
    <dialog ref={dialog} id="section-dialog" className="space-dialog section-dialog" aria-labelledby="section-dialog-title"
      onCancel={event => {
        event.preventDefault();
        if (!submitting.current) window.location.assign(cancelTo);
      }}>
      <header className="space-dialog-header">
        <div>
          <h2 id="section-dialog-title">Новый раздел</h2>
          <p className="space-dialog-tenant"><Icon name="folder" />{spaceName}</p>
        </div>
        <a className="space-action space-close" href={cancelTo} aria-label="Закрыть" aria-disabled={saving || undefined}
          data-section-cancel onClick={onCancelLink}><Icon name="close" /></a>
      </header>
      <form ref={form} className="space-form" method="post" action={action} noValidate onSubmit={onSubmit} aria-busy={saving}>
        <input type="hidden" name="operation_id" value={operationId} />
        <label htmlFor="section-name">Название</label>
        <input ref={input} id="section-name" name="name" defaultValue={title} placeholder="Например, Документация"
          required readOnly={saving} aria-describedby={description} aria-invalid={Boolean(clientError) || invalidName || undefined}
          onInput={() => setClientError('')} />
        <p className="space-hint" id="section-help">От 1 до 100 символов. Название должно быть уникальным в пространстве.</p>
        <p className="form-error" id="section-client-error" role="alert" hidden={!clientError}>{clientError}</p>
        {error && (
          <p className="form-error" id="section-error" role="alert">
            {error}{retryTo && <> <a href={retryTo}>Открыть новую форму</a></>}
          </p>
        )}
        <div className="space-dialog-actions">
          <a className="space-action space-secondary" href={cancelTo} aria-disabled={saving || undefined}
            data-section-cancel onClick={onCancelLink}>Отмена</a>
          <button className="primary-action space-action" type="submit" disabled={saving}>
            {saving ? 'Создаём…' : 'Создать'}
          </button>
        </div>
      </form>
      <p className="sr-only" role="status" data-section-status>{saving ? 'Создаём раздел.' : ''}</p>
    </dialog>
  );
}
