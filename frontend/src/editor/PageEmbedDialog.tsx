import {useId, useLayoutEffect, useMemo, useRef, useState, type FormEvent} from 'react';
import {externalEmbedAddress} from './markdownModel';
import {readEmbedInput} from './pageEmbedInput';

export function PageEmbedDialog({address, onApply, onCancel}: {
  address?: string; onApply(url: string): boolean; onCancel(): void;
}) {
  const heading = useId();
  const help = useId();
  const recognized = useId();
  const errorId = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const field = useRef<HTMLTextAreaElement>(null);
  const [value, setValue] = useState(address ?? '');
  const [error, setError] = useState('');
  const input = useMemo(() => readEmbedInput(value, document), [value]);
  const embed = 'address' in input ? externalEmbedAddress(input.address) : undefined;

  useLayoutEffect(() => {
    dialog.current?.showModal();
    field.current?.focus();
  }, []);

  function dismiss() {
    dialog.current?.close();
    onCancel();
  }

  function fail(message: string) {
    setError(message);
    field.current?.focus();
  }

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if ('error' in input) return fail(input.error);
    if (!embed) return fail('Нужен внешний HTTPS-адрес без логина и пароля.');
    if (!onApply(embed.url)) fail('Блок изменился или доступ к правке ограничен. Закройте окно и откройте его снова.');
  }

  return <dialog ref={dialog} className="space-dialog page-draft-dialog page-embed-dialog" aria-labelledby={heading}
    onCancel={event => { event.preventDefault(); dismiss(); }}>
    <header className="space-dialog-header"><h2 id={heading}>{address ? 'Изменить адрес блока' : 'Вставить внешний блок'}</h2></header>
    <form className="space-form" onSubmit={submit} noValidate>
      <label className="page-draft-field">Ссылка или код встраивания
        <textarea ref={field} rows={4} spellCheck={false} placeholder="https://…" value={value}
          aria-invalid={Boolean(error) || undefined} aria-describedby={error ? `${help} ${recognized} ${errorId}` : `${help} ${recognized}`}
          onChange={event => { setValue(event.currentTarget.value); setError(''); }} />
      </label>
      <p id={help} className="space-hint">Вставьте HTTPS-адрес или код iframe, который выдал сайт.</p>
      <p id={recognized} className="space-hint page-embed-recognition" role="status">
        {embed?.kind === 'teamstream' ? 'Распознано видео TeamStream' : ''}
      </p>
      {error && <p id={errorId} className="form-error" role="alert">{error}</p>}
      <div className="space-dialog-actions">
        <button type="button" className="space-action space-secondary" onClick={dismiss}>Отмена</button>
        <button type="submit" className="space-action primary-action">{address ? 'Применить' : 'Вставить'}</button>
      </div>
    </form>
  </dialog>;
}
