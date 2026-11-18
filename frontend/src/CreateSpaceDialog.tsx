import {useEffect, useLayoutEffect, useRef, useState, type FormEvent, type MouseEvent} from 'react';
import {Icon} from './Icon';
import type {CreationPageModel} from './model';
import {newSpacePath, spacesPath, withOptions} from './routes';

const maxNameLength = 100;
const maxFormBytes = 4096;

function encodedSize(data: FormData): number {
  return new URLSearchParams(Array.from(data, ([key, value]) => [key, String(value)])).toString().length;
}

export function CreateSpaceDialog({page}: {page: CreationPageModel}) {
  const {operationId, name, error, invalidName, newForm, workspace: {tenant, catalog: {options}}} = page;
  const catalogHref = withOptions(spacesPath(tenant.id), options);
  const cancelHref = `${catalogHref}#create-space`;
  const input = useRef<HTMLInputElement>(null);
  const dialog = useRef<HTMLDialogElement>(null);
  const form = useRef<HTMLFormElement>(null);
  const submitting = useRef(false);
  const [saving, setSaving] = useState(false);
  const [clientError, setClientError] = useState('');

  useLayoutEffect(() => {
    const element = dialog.current;
    if (!element) return;
    if (typeof element.showModal === 'function') {
      element.showModal();
    } else {
      element.open = true;
    }
    input.current?.focus();
  }, []);

  useEffect(() => {
    const element = form.current;
    if (!element) return;

    function normalizeOversizedName({formData}: FormDataEvent) {
      const entered = formData.get('name');
      if (typeof entered !== 'string') return;
      const trimmed = entered.trim();
      if (trimmed && trimmed !== entered && Array.from(trimmed).length <= maxNameLength &&
          encodedSize(formData) > maxFormBytes) {
        formData.set('name', trimmed);
      }
    }
    function resetSaving() {
      submitting.current = false;
      setSaving(false);
    }

    element.addEventListener('formdata', normalizeOversizedName);
    window.addEventListener('pageshow', resetSaving);
    return () => {
      element.removeEventListener('formdata', normalizeOversizedName);
      window.removeEventListener('pageshow', resetSaving);
    };
  }, []);

  function onSubmit(event: FormEvent<HTMLFormElement>) {
    if (submitting.current) {
      event.preventDefault();
      return;
    }
    const entered = input.current?.value ?? '';
    const trimmed = entered.trim();
    const tooLong = Array.from(trimmed).length > maxNameLength;
    const oversizedBlank = !trimmed && entered.length > 0 &&
      encodedSize(new FormData(event.currentTarget)) > maxFormBytes;
    if (tooLong || oversizedBlank) {
      event.preventDefault();
      setClientError(tooLong ? 'Введите не больше 100 символов.' : 'Введите название пространства.');
      input.current?.focus();
      return;
    }
    submitting.current = true;
    setSaving(true);
  }

  function onCancelLink(event: MouseEvent<HTMLAnchorElement>) {
    if (submitting.current) event.preventDefault();
  }

  const description = clientError ? 'space-help space-client-error' : error ? 'space-help space-error' : 'space-help';

  return (
    <dialog ref={dialog} id="space-dialog" className="space-dialog" aria-labelledby="space-dialog-title"
      onCancel={event => {
        event.preventDefault();
        if (!submitting.current) window.location.assign(cancelHref);
      }}>
      <header className="space-dialog-header">
        <div>
          <h2 id="space-dialog-title">Новое пространство</h2>
          <p className="space-dialog-tenant"><Icon name="home" />{tenant.name}</p>
        </div>
        <a className="space-action space-close" href={cancelHref} data-space-cancel aria-label="Закрыть"
          aria-disabled={saving || undefined} onClick={onCancelLink}><Icon name="close" /></a>
      </header>
      <form ref={form} className="space-form" method="post" action={catalogHref} noValidate
        onSubmit={onSubmit} aria-busy={saving}>
        <input type="hidden" name="operation_id" value={operationId} />
        <label htmlFor="space-name">Название</label>
        <input ref={input} id="space-name" name="name" defaultValue={name} placeholder="Например, Команда продукта"
          required readOnly={saving} aria-describedby={description} aria-invalid={Boolean(clientError) || invalidName || undefined}
          onInput={() => setClientError('')} />
        <p className="space-hint" id="space-help">От 1 до 100 символов. Выберите уникальное имя среди своих пространств в этой среде.</p>
        {clientError && <p className="form-error" id="space-client-error" role="alert">{clientError}</p>}
        {error && (
          <p className="form-error" id="space-error" role="alert">
            {error}
            {newForm && <> <a href={withOptions(newSpacePath(tenant.id), options)}>Открыть новую форму</a></>}
          </p>
        )}
        <div className="space-note">
          <Icon name="lock" />
          <p><strong>Закрытое пространство</strong><br />Вы станете владельцем. Администраторы этой среды смогут просматривать содержимое.</p>
        </div>
        <div className="space-dialog-actions">
          <a className="space-action space-secondary" href={cancelHref} data-space-cancel
            aria-disabled={saving || undefined} onClick={onCancelLink}>Отмена</a>
          <button className="primary-action space-action" type="submit" disabled={saving}>
            {saving ? 'Создаём…' : 'Создать'}
          </button>
        </div>
      </form>
      <p className="sr-only" role="status" data-space-status>{saving ? 'Создаём пространство.' : ''}</p>
    </dialog>
  );
}
