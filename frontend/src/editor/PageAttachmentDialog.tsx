import {useId, useLayoutEffect, useRef, useState, type FormEvent} from 'react';
import {Icon} from '../Icon';
import {
  maxAttachmentCount, maxAttachmentSize, maxAttachmentTotal, uploadedAttachment, type PageAttachment,
} from './pageAttachments';

export type AttachmentDialogRequest =
  | {kind: 'upload'}
  | {kind: 'image'; select?: string}
  | {kind: 'remove'; file: Pick<PageAttachment, 'id' | 'name'>; references: number};

export type AttachmentCompletion = {kind: 'add'; file: PageAttachment} | {kind: 'image'; file: PageAttachment; alt: string} | {kind: 'remove'; id: string};
type PendingUpload = {id: string; file: File};

const imageAccept = '.png,.jpg,.jpeg,.webp';
const fileAccept = `.pdf,.txt,.md,.markdown,.docx,.xlsx,.pptx,${imageAccept}`;
const uploadFailed = 'Не удалось загрузить файл. Попробуйте ещё раз.';
const plural = new Intl.PluralRules('ru');

function referenceCount(count: number): string {
  const form = plural.select(count);
  return `${count} ${form === 'one' ? 'вставка' : form === 'few' ? 'вставки' : 'вставок'}`;
}

export function PageAttachmentDialog({request, files, pendingCount, uploadTo, onComplete, onDismiss}: {
  request: AttachmentDialogRequest;
  files: readonly PageAttachment[];
  pendingCount: number;
  uploadTo?: string | null;
  onComplete(completion: AttachmentCompletion): boolean;
  onDismiss(): void;
}) {
  const heading = useId();
  const uploadHelp = useId();
  const errorId = useId();
  const modal = useRef<HTMLDialogElement>(null);
  const first = useRef<HTMLButtonElement>(null);
  const ongoing = useRef<AbortController | null>(null);
  const [selected, setSelected] = useState(request.kind === 'image' ? request.select ?? '' : '');
  const [upload, setUpload] = useState<PendingUpload>();
  const [alt, setAlt] = useState('');
  const [decorative, setDecorative] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [retry, setRetry] = useState(false);
  const image = request.kind === 'image';
  const remove = request.kind === 'remove';
  const images = files.filter(file => file.imageTo);
  const labels = image ? {title:'Вставить изображение', action:'Вставить'} : remove
    ? {title:'Удалить вложение?', action:'Удалить файл и вставки'} : {title:'Прикрепить файл', action:'Прикрепить'};

  useLayoutEffect(() => {
    modal.current?.showModal();
    first.current?.focus();
    return () => ongoing.current?.abort();
  }, []);

  function dismiss() {
    ongoing.current?.abort();
    modal.current?.close();
    onDismiss();
  }

  function complete(proposed: AttachmentCompletion) {
    if (onComplete(proposed)) dismiss();
    else setError('Выберите место для изображения в тексте вне кода или ссылки.');
  }

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (ongoing.current) return;
    if (request.kind === 'remove') {
      complete({kind:'remove', id:request.file.id});
      return;
    }
    const description = decorative ? '' : alt.trim();
    if (image && !decorative && !description) {
      setError('Добавьте описание или отметьте изображение как декоративное.');
      return;
    }
    const existing = image && images.find(file => file.id === selected);
    if (!upload && existing) {
      complete({kind:'image', file:existing, alt:description});
      return;
    }
    if (!upload || !uploadTo) {
      setError(image ? 'Выберите или загрузите изображение.' : 'Выберите файл.');
      return;
    }
    if (upload.file.size <= 0 || upload.file.size > maxAttachmentSize) {
      setError(upload.file.size <= 0 ? 'Файл пуст. Выберите другой файл.' : 'Размер файла превышает 20 МБ.');
      return;
    }
    if (files.length + pendingCount >= maxAttachmentCount ||
        files.reduce((sum, file) => sum + file.bytes, upload.file.size) > maxAttachmentTotal) {
      setError('На странице может быть до 20 вложений общим размером до 100 МБ.');
      return;
    }

    const controller = new AbortController();
    ongoing.current = controller;
    setError('');
    setBusy(true);
    try {
      const response = await fetch(`${uploadTo}/${upload.id}`, {
        method:'POST', body:upload.file, credentials:'same-origin', redirect:'error', signal:controller.signal,
        headers:{
          'Content-Type':upload.file.type || 'application/octet-stream',
          'X-Attachment-Name':encodeURIComponent(upload.file.name),
        },
      });
      const body: unknown = await response.json().catch(() => null);
      if (controller.signal.aborted) return;
      const file = response.ok ? uploadedAttachment(body, uploadTo, upload.id) : undefined;
      if (!file) {
        setRetry(true);
        setError(body && typeof body === 'object' && 'error' in body && typeof body.error === 'string' && body.error.trim()
          ? body.error : uploadFailed);
        return;
      }
      if (image && !file.imageTo) {
        setError('Выберите изображение PNG, JPEG или WebP.');
        return;
      }
      complete(image ? {kind:'image', file, alt:description} : {kind:'add', file});
    } catch {
      if (!controller.signal.aborted) {
        setRetry(true);
        setError(uploadFailed);
      }
    } finally {
      if (!controller.signal.aborted) {
        ongoing.current = null;
        setBusy(false);
      }
    }
  }

  return <dialog ref={modal} className="space-dialog page-attachment-dialog" aria-labelledby={heading}
    onCancel={event => { event.preventDefault(); dismiss(); }}>
    <header className="space-dialog-header">
      <h2 id={heading}>{labels.title}</h2>
      <button type="button" className="space-action space-close" ref={first} onClick={dismiss} aria-label="Закрыть">
        <Icon name="close" />
      </button>
    </header>
    <form className="page-attachment-dialog-body" onSubmit={submit} noValidate>
      {remove ? <>
        <p><strong>{request.file.name}</strong> · {referenceCount(request.references)}</p>
        <p className="space-hint">Файл и связанные вставки исчезнут с текущей страницы после сохранения.</p>
      </> : <fieldset disabled={busy} className="page-attachment-fields">
        {image && images.length > 0 && <label>Вложение
          <select value={upload ? '' : selected} onChange={event => {
            setSelected(event.currentTarget.value);
            setUpload(undefined);
            setError('');
            setRetry(false);
          }}>
            <option value="">Выберите изображение</option>
            {images.map(file => <option key={file.id} value={file.id}>{file.name}</option>)}
          </select>
        </label>}
        {uploadTo && <label>{image && images.length ? 'Или загрузить новое' : 'Файл'}
          <input type="file" accept={image ? imageAccept : fileAccept} aria-label={image && images.length ? 'Или загрузить новое' : 'Файл'}
            aria-describedby={uploadHelp + (error ? ` ${errorId}` : '')} onChange={event => {
              const file = event.currentTarget.files?.[0];
              if (!file) return;
              event.currentTarget.value = '';
              setUpload({file, id:crypto.randomUUID()});
              setError('');
              setRetry(false);
            }} />
          <small id={uploadHelp}>{image ? 'PNG, JPEG или WebP' : 'PDF, TXT, Markdown, DOCX, XLSX, PPTX, PNG, JPEG, WebP'} до 20 МБ.</small>
        </label>}
        {upload && <span className="space-hint page-attachment-selected">Выбран файл «{upload.file.name}».</span>}
        {image && <>
          <label>Описание изображения
            <input type="text" value={alt} maxLength={250} disabled={decorative}
              onChange={event => setAlt(event.currentTarget.value)} placeholder="Что показано на изображении?" />
          </label>
          <label className="page-attachment-check"><input type="checkbox" checked={decorative}
            onChange={event => setDecorative(event.currentTarget.checked)} />Декоративное изображение</label>
        </>}
      </fieldset>}
      {error && <p id={errorId} role="alert" className="form-error">{error}</p>}
      <p role="status" className="page-attachment-upload-status">{busy ? 'Загружаем файл…' : ''}</p>
      <div className="space-dialog-actions">
        <button type="button" className="space-action space-secondary" onClick={dismiss}>Отмена</button>
        <button className="space-action primary-action" type="submit" disabled={busy}>
          {retry ? 'Повторить' : labels.action}
        </button>
      </div>
    </form>
  </dialog>;
}
