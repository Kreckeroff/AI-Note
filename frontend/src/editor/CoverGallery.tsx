import {useId, useLayoutEffect, useRef, useState} from 'react';
import {Icon} from '../Icon';
import {coverOptions, uploadedCover, type CoverChoice, type CoverView} from './pageCover';

const maxCoverSize = 5 * 1024 * 1024;
const imageTypes = ['image/png', 'image/jpeg', 'image/webp'];
const uploadFailure = 'Не удалось загрузить обложку. Попробуйте снова.';

export function coverLabel(cover: CoverView | null | undefined): string {
  if (cover?.imageTo) return 'Своя обложка';
  return coverOptions.find(option => option.value === cover?.value)?.label ?? 'Без обложки';
}

export function PageCoverArt({cover}: {cover: CoverView | null | undefined}) {
  if (!cover || cover.value === 'none') return null;
  return <span className="page-cover-art" data-cover-theme={cover.imageTo ? 'custom' : cover.value} aria-hidden="true">
    {cover.imageTo && <img src={cover.imageTo} alt="" />}
  </span>;
}

function CoverThumbnail({cover}: {cover: CoverView | null | undefined}) {
  return <span className="page-cover-thumbnail" aria-hidden="true">
    {!cover || cover.value === 'none' ? <Icon name="file" /> : <PageCoverArt cover={cover} />}
  </span>;
}

export function CoverConflictOptions({draft, published, choice, disabled, onChoice}: {
  draft: CoverView | null;
  published: CoverView;
  choice: CoverChoice | null;
  disabled: boolean;
  onChoice(choice: CoverChoice): void;
}) {
  return <fieldset className="page-cover-conflict" disabled={disabled}>
    <legend>Какую обложку оставить?</legend>
    <div className="page-cover-conflict-options">
      {([
        {value: 'published', label: 'Опубликованная обложка', cover: published},
        {value: 'draft', label: 'Обложка из черновика', cover: draft},
      ] as const).map(option => <label className="page-cover-conflict-option" key={option.value}>
        <CoverThumbnail cover={option.cover} />
        <span className="page-cover-choice-copy">
          <input type="radio" name="coverChoice" value={option.value} checked={choice === option.value}
            onChange={() => onChoice(option.value)} />
          <span><strong>{option.label}</strong><small>{coverLabel(option.cover)}</small></span>
        </span>
      </label>)}
    </div>
  </fieldset>;
}

function uploadError(value: unknown): string {
  return value && typeof value === 'object' && 'error' in value && typeof value.error === 'string' && value.error.trim()
    ? value.error : uploadFailure;
}

export function CoverGallery({cover, uploadTo, onPick, onDismiss}: {
  cover: CoverView | null;
  uploadTo?: string | null;
  onPick(cover: CoverView): void;
  onDismiss(): void;
}) {
  const title = useId();
  const uploadHelp = useId();
  const uploadErrorId = useId();
  const dialog = useRef<HTMLDialogElement>(null);
  const closeButton = useRef<HTMLButtonElement>(null);
  const uploadRequest = useRef<AbortController | null>(null);
  const [uploading, setUploading] = useState(false);
  const [error, setError] = useState('');
  const current = cover?.value ?? 'none';

  useLayoutEffect(() => {
    dialog.current?.showModal();
    closeButton.current?.focus();
    return () => uploadRequest.current?.abort();
  }, []);

  function dismiss() {
    uploadRequest.current?.abort();
    dialog.current?.close();
    onDismiss();
  }

  function pick(value: CoverView) {
    onPick(value);
    dismiss();
  }

  async function upload(file: File) {
    if (!uploadTo) return;
    uploadRequest.current?.abort();
    setError('');
    if (file.size > maxCoverSize) {
      setUploading(false);
      setError('Размер изображения должен быть до 5 МБ.');
      return;
    }

    const controller = new AbortController();
    const contentType = imageTypes.includes(file.type) ? file.type : 'application/octet-stream';
    uploadRequest.current = controller;
    setUploading(true);
    try {
      const response = await fetch(uploadTo, {
        method: 'POST', headers: {'Content-Type': contentType}, body: file, signal: controller.signal,
        credentials: 'same-origin', redirect: 'error',
      });
      const data: unknown = await response.json().catch(() => null);
      if (controller.signal.aborted) return;
      const selected = response.ok ? uploadedCover(data, uploadTo) : undefined;
      if (selected) pick(selected);
      else setError(uploadError(data));
    } catch {
      if (!controller.signal.aborted) setError(uploadFailure);
    } finally {
      if (!controller.signal.aborted) {
        uploadRequest.current = null;
        setUploading(false);
      }
    }
  }

  function option(value: CoverView, label: string, description?: string) {
    const help = description ? `${title}-${value.value}` : undefined;
    return <button className="page-cover-choice" key={value.value} type="button" aria-label={label} aria-describedby={help}
      aria-pressed={current === value.value}
      onClick={() => pick(value)}>
      <CoverThumbnail cover={value} />
      <span className="page-cover-choice-copy">
        <span><strong>{label}</strong>{description && <small id={help}>{description}</small>}</span>
        {current === value.value && <Icon name="check" />}
      </span>
    </button>;
  }

  return <dialog ref={dialog} className="space-dialog page-cover-dialog" aria-labelledby={title}
    onCancel={event => { event.preventDefault(); dismiss(); }}>
    <header className="space-dialog-header">
      <h2 id={title}>Обложка страницы</h2>
      <button ref={closeButton} className="space-action space-close" type="button" aria-label="Закрыть" onClick={dismiss}>
        <Icon name="close" />
      </button>
    </header>
    <div className="page-cover-dialog-body">
      {uploadTo && <>
        <label className="page-cover-upload">
          <Icon name="upload" />
          <span><strong>Загрузить изображение</strong><small id={uploadHelp}>PNG, JPEG или WebP до 5 МБ</small></span>
          <input type="file" accept={imageTypes.join(',')} aria-label="Загрузить свою обложку"
            aria-describedby={error ? `${uploadHelp} ${uploadErrorId}` : uploadHelp} aria-invalid={Boolean(error) || undefined}
            onChange={event => {
              const file = event.currentTarget.files?.[0];
              event.currentTarget.value = '';
              if (file) void upload(file);
            }} />
        </label>
        <p className="page-cover-upload-status" role="status">{uploading ? 'Загружаем обложку…' : ''}</p>
        {error && <p id={uploadErrorId} className="form-error" role="alert">{error}</p>}
      </>}
      <div className="page-cover-gallery">
        {cover?.imageTo && option(cover, 'Своя обложка')}
        {coverOptions.map(theme => option({value: theme.value, imageTo: null}, theme.label, theme.description))}
      </div>
    </div>
  </dialog>;
}
