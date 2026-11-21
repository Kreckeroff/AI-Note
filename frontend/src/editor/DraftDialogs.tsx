import {useEffect, useId, useLayoutEffect, useRef, useState, type FormEvent} from 'react';
import {collaborationError} from '../collaboration/api';
import {editorLinkDestination} from './markdownModel';
import {canLinkToPage, PageLinkPicker, type PageLink} from './PageLinkPicker';

type LinkDialog = {kind: 'link'; label: string};
type PasteDialog = {kind: 'paste'};
export type DialogValue = {kind: 'link'; label: string; address: string} | {kind: 'paste'; text: string};

type Props = {
  request: LinkDialog | PasteDialog;
  pageLinkSearchTo?: string;
  onCancel(): void;
  onApply(value: DialogValue): boolean;
};

export function DraftDialogs({request, pageLinkSearchTo, onCancel, onApply}: Props) {
  const heading = useId();
  const errorId = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const valueField = useRef<HTMLInputElement | HTMLTextAreaElement>(null);
  const validation = useRef<AbortController>(null);
  const [label, setLabel] = useState(request.kind === 'link' ? request.label : '');
  const [value, setValue] = useState('');
  const [error, setError] = useState('');
  const isLink = request.kind === 'link';
  const searchTo = isLink ? pageLinkSearchTo : undefined;
  const [kind, setKind] = useState(searchTo && request.kind === 'link' && !request.label.trim() ? 'page' : 'address');
  const [selection, setSelection] = useState<PageLink | null>(null);
  const [checking, setChecking] = useState(false);
  const [searchRevision, setSearchRevision] = useState(0);
  const searchPages = kind === 'page' ? searchTo : undefined;
  const title = isLink ? 'Ссылка' : 'Вставить как Markdown';

  useLayoutEffect(() => {
    const element = dialog.current;
    if (element && !element.open) element.showModal();
    valueField.current?.focus();
  }, [searchPages]);

  useEffect(() => () => validation.current?.abort(), []);

  function dismiss() {
    validation.current?.abort();
    dialog.current?.close();
    onCancel();
  }

  function selectPage(page: PageLink | null) {
    const defaultLabel = request.kind === 'link' ? request.label : '';
    setLabel(current => !current.trim() || current === selection?.title ? page?.title ?? defaultLabel : current);
    setSelection(page);
    setError('');
  }

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (validation.current) return;
    const address = searchPages ? selection?.path : value.trim();
    if (searchPages && !address) {
      setError('Выберите страницу из результатов поиска.');
      valueField.current?.focus();
      return;
    }
    if (isLink && !label.trim()) {
      setError('Введите текст ссылки.');
      event.currentTarget.querySelector<HTMLInputElement>('input[name="label"]')?.focus();
      return;
    }
    const input: DialogValue | null = isLink
      ? address && editorLinkDestination(address) ? {kind: 'link', label, address} : null
      : value.trim() ? {kind: 'paste', text: value} : null;
    if (!input) {
      setError(isLink ? 'Укажите адрес страницы «Севера» или ссылку с http:// либо https://.' : 'Вставьте текст Markdown.');
      valueField.current?.focus();
      return;
    }
    if (searchPages && selection) {
      const controller = new AbortController();
      validation.current = controller;
      setChecking(true);
      try {
        const accessible = await canLinkToPage(searchPages, selection, controller.signal);
        if (controller.signal.aborted) return;
        if (!accessible) {
          selectPage(null);
          setSearchRevision(revision => revision + 1);
          setError('Эта страница больше недоступна. Найдите другую страницу.');
          return;
        }
      } catch (failure) {
        if (!controller.signal.aborted) {
          selectPage(null);
          setSearchRevision(revision => revision + 1);
          setError(collaborationError(failure));
        }
        return;
      } finally {
        validation.current = null;
        if (!controller.signal.aborted) setChecking(false);
      }
    }
    if (!onApply(input)) setError('Здесь это изменение можно сделать в «Исходнике».');
  }

  return <dialog ref={dialog} className="space-dialog page-draft-dialog" aria-labelledby={heading}
    onKeyDown={event => {
      if (event.key === 'Escape' && !event.nativeEvent.isComposing) {
        event.preventDefault();
        dismiss();
      }
    }}
    onCancel={event => { event.preventDefault(); dismiss(); }}>
    <header className="space-dialog-header"><h2 id={heading}>{title}</h2></header>
    <form className="space-form" onSubmit={submit} noValidate>
      {searchTo && <div className="page-link-kind" role="group" aria-label="Тип ссылки">
        <button type="button" className="space-action space-secondary" aria-pressed={Boolean(searchPages)}
          disabled={checking} onClick={() => { setKind('page'); setError(''); }}>Страница пространства</button>
        <button type="button" className="space-action space-secondary" aria-pressed={!searchPages}
          disabled={checking} onClick={() => { setKind('address'); selectPage(null); }}>По адресу</button>
      </div>}
      {searchPages && <PageLinkPicker searchTo={searchPages} selection={selection} onSelect={selectPage}
        refresh={searchRevision} inputRef={node => { valueField.current = node; }}
        disabled={checking} errorId={error ? errorId : undefined} />}
      {isLink && <label className="page-draft-field">Текст ссылки
        <input name="label" type="text" value={label} disabled={checking}
          onChange={event => { setLabel(event.currentTarget.value); setError(''); }} />
      </label>}
      {isLink && !searchPages && <label className="page-draft-field">Адрес ссылки
        <input ref={node => { valueField.current = node; }} type="text" inputMode="url" value={value} aria-describedby={error ? errorId : undefined}
          onChange={event => { setValue(event.currentTarget.value); setError(''); }} />
      </label>}
      {!isLink && <label className="page-draft-field">Текст Markdown
        <textarea ref={node => { valueField.current = node; }} value={value} rows={7} spellCheck={false} aria-describedby={error ? errorId : undefined}
          onChange={event => { setValue(event.currentTarget.value); setError(''); }} />
      </label>}
      {error && <p id={errorId} className="form-error" role="alert">{error}</p>}
      <div className="space-dialog-actions">
        <button type="button" className="space-action space-secondary" onClick={dismiss}>Отменить</button>
        <button type="submit" className="space-action primary-action" disabled={checking}>
          {checking ? 'Проверяем доступ…' : isLink ? 'Применить' : 'Вставить'}
        </button>
      </div>
    </form>
  </dialog>;
}
