import {useId, useLayoutEffect, useRef, useState, type FormEvent} from 'react';
import {httpDestination} from './markdownModel';

type LinkDialog = {kind: 'link'; label: string};
type PasteDialog = {kind: 'paste'};
export type DialogValue = {kind: 'link'; label: string; address: string} | {kind: 'paste'; text: string};

type Props = {
  request: LinkDialog | PasteDialog;
  onCancel(): void;
  onApply(value: DialogValue): boolean;
};

export function DraftDialogs({request, onCancel, onApply}: Props) {
  const heading = useId();
  const errorId = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const valueField = useRef<HTMLInputElement | HTMLTextAreaElement>(null);
  const [label, setLabel] = useState(request.kind === 'link' ? request.label : '');
  const [value, setValue] = useState('');
  const [error, setError] = useState('');
  const isLink = request.kind === 'link';
  const title = isLink ? 'Ссылка' : 'Вставить как Markdown';

  useLayoutEffect(() => {
    const element = dialog.current;
    if (element && !element.open) element.showModal();
    valueField.current?.focus();
  }, []);

  function dismiss() {
    dialog.current?.close();
    onCancel();
  }

  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (isLink && !label.trim()) {
      setError('Введите текст ссылки.');
      event.currentTarget.querySelector<HTMLInputElement>('input[name="label"]')?.focus();
      return;
    }
    if (isLink ? !httpDestination(value.trim()) : !value.trim()) {
      setError(isLink ? 'Укажите полный адрес с http:// или https://.' : 'Вставьте текст Markdown.');
      valueField.current?.focus();
      return;
    }
    const input: DialogValue = isLink ? {kind: 'link', label, address: value.trim()} : {kind: 'paste', text: value};
    if (!onApply(input)) setError('Здесь это изменение можно сделать в «Исходнике».');
  }

  return <dialog ref={dialog} className="space-dialog page-draft-dialog" aria-labelledby={heading}
    onCancel={event => { event.preventDefault(); dismiss(); }}>
    <header className="space-dialog-header"><h2 id={heading}>{title}</h2></header>
    <form className="space-form" onSubmit={submit} noValidate>
      {isLink && <label className="page-draft-field">Текст ссылки
        <input name="label" type="text" value={label} onChange={event => { setLabel(event.currentTarget.value); setError(''); }} />
      </label>}
      {isLink ? <label className="page-draft-field">Адрес ссылки
        <input ref={node => { valueField.current = node; }} type="url" value={value} aria-describedby={error ? errorId : undefined}
          onChange={event => { setValue(event.currentTarget.value); setError(''); }} />
      </label> : <label className="page-draft-field">Текст Markdown
        <textarea ref={node => { valueField.current = node; }} value={value} rows={7} spellCheck={false} aria-describedby={error ? errorId : undefined}
          onChange={event => { setValue(event.currentTarget.value); setError(''); }} />
      </label>}
      {error && <p id={errorId} className="form-error" role="alert">{error}</p>}
      <div className="space-dialog-actions">
        <button type="button" className="space-action space-secondary" onClick={dismiss}>Отменить</button>
        <button type="submit" className="space-action primary-action">{isLink ? 'Применить' : 'Вставить'}</button>
      </div>
    </form>
  </dialog>;
}
