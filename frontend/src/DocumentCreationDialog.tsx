import {useEffect, useLayoutEffect, useRef, useState, type FormEvent, type MouseEvent} from 'react';
import {Icon} from './Icon';
import type {DocumentFormModel} from './model';

const maxNameLength = 100;
const maxFormBytes = 4096;

const labels = {
  section: {
    heading: 'Новый раздел',
    placeholder: 'Например, Документация',
    help: 'От 1 до 100 символов. Названия разделов на одном уровне должны отличаться.',
    blank: 'Введите название раздела.',
    saving: 'Создаём раздел.',
  },
  page: {
    heading: 'Новая страница',
    placeholder: 'Например, Решение по доступу',
    help: 'От 1 до 100 символов. Название должно быть уникальным в разделе.',
    blank: 'Введите название страницы.',
    saving: 'Создаём страницу.',
  },
};

export function DocumentCreationDialog({kind, form: fields, parentName}: {
  kind: keyof typeof labels;
  form: DocumentFormModel;
  parentName: string;
}) {
  const copy = labels[kind];
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
      const encoded = new URLSearchParams();
      formData.forEach((entry, key) => { if (typeof entry === 'string') encoded.append(key, entry); });
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
  }, []);

  function onSubmit(event: FormEvent<HTMLFormElement>) {
    if (submitting.current) {
      event.preventDefault();
      return;
    }
    const name = input.current?.value.trim() ?? '';
    if (!name || Array.from(name).length > maxNameLength) {
      event.preventDefault();
      setClientError(name ? 'Введите не больше 100 символов.' : copy.blank);
      input.current?.focus();
      return;
    }
    submitting.current = true;
    setSaving(true);
  }

  function onCancelLink(event: MouseEvent<HTMLAnchorElement>) {
    if (submitting.current) event.preventDefault();
  }

  const helpId = `${kind}-help`;
  const clientErrorId = `${kind}-client-error`;
  const serverErrorId = `${kind}-error`;
  const nameId = `${kind}-name`;
  const headingId = `${kind}-dialog-title`;
  const description = clientError ? `${helpId} ${clientErrorId}` : error ? `${helpId} ${serverErrorId}` : helpId;
  const cancelMarker = {[`data-${kind}-cancel`]: ''};
  const statusMarker = {[`data-${kind}-status`]: ''};

  return (
    <dialog ref={dialog} id={`${kind}-dialog`} className={`space-dialog ${kind}-dialog`} aria-labelledby={headingId}
      onCancel={event => {
        event.preventDefault();
        if (!submitting.current) window.location.assign(cancelTo);
      }}>
      <header className="space-dialog-header">
        <div>
          <h2 id={headingId}>{copy.heading}</h2>
          <p className="space-dialog-tenant"><Icon name="folder" />{parentName}</p>
        </div>
        <a className="space-action space-close" href={cancelTo} aria-label="Закрыть" aria-disabled={saving || undefined}
          {...cancelMarker} onClick={onCancelLink}><Icon name="close" /></a>
      </header>
      <form ref={form} className="space-form" method="post" action={action} noValidate onSubmit={onSubmit} aria-busy={saving}>
        <input type="hidden" name="operation_id" value={operationId} />
        <label htmlFor={nameId}>Название</label>
        <input ref={input} id={nameId} name="name" defaultValue={title} placeholder={copy.placeholder}
          required readOnly={saving} aria-describedby={description} aria-invalid={Boolean(clientError) || invalidName || undefined}
          onInput={() => setClientError('')} />
        <p className="space-hint" id={helpId}>{copy.help}</p>
        <p className="form-error" id={clientErrorId} role="alert" hidden={!clientError}>{clientError}</p>
        {error && (
          <p className="form-error" id={serverErrorId} role="alert">
            {error}{retryTo && <> <a href={retryTo}>Открыть новую форму</a></>}
          </p>
        )}
        {kind === 'page' && fields.templates && (
          <fieldset className="page-template-picker">
            <legend>Начать с шаблона</legend>
            {[{key: '', title: 'Пустая страница', source: ''}, ...fields.templates].map(template => (
              <div className="page-template-option" key={template.key}>
                <label>
                  <input type="radio" name="template" value={template.key}
                    defaultChecked={template.key === (fields.selectedTemplate ?? '')} />
                  <span>{template.title}</span>
                </label>
                {template.source && <pre className="page-template-preview" aria-label="Содержимое шаблона" tabIndex={0}>{template.source}</pre>}
              </div>
            ))}
          </fieldset>
        )}
        <div className="space-dialog-actions">
          <a className="space-action space-secondary" href={cancelTo} aria-disabled={saving || undefined}
            {...cancelMarker} onClick={onCancelLink}>Отмена</a>
          <button className="primary-action space-action" type="submit" disabled={saving}>
            {saving ? 'Создаём…' : 'Создать'}
          </button>
        </div>
      </form>
      <p className="sr-only" role="status" {...statusMarker}>{saving ? copy.saving : ''}</p>
    </dialog>
  );
}
