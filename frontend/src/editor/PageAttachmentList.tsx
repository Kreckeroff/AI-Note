import {useId, type Ref} from 'react';
import {Icon} from '../Icon';
import type {PageAttachment} from './pageAttachments';

type Controls = {
  disabled: boolean;
  staged: readonly string[];
  onUpload(): void;
  onInsert(file: PageAttachment): void;
  onRemove(file: Pick<PageAttachment, 'id' | 'name'>): void;
  uploadRef: Ref<HTMLButtonElement>;
};

const number = new Intl.NumberFormat('ru', {maximumFractionDigits:1});
const kib = 1024;
const mib = kib * kib;

function sizeLabel(bytes: number): string {
  if (bytes < kib) return `${bytes} Б`;
  if (bytes < mib) return `${number.format(bytes / kib)} КБ`;
  return `${number.format(bytes / mib)} МБ`;
}

export function PageAttachmentList({files, controls}: {files: readonly PageAttachment[]; controls?: Controls}) {
  const title = useId();
  const unverified = controls?.staged.filter(id => !files.some(file => file.id === id)) ?? [];

  return <section className="page-attachments" aria-labelledby={title}>
    <header className="page-attachment-heading">
      <h2 id={title}>Вложения {files.length > 0 && <span>{files.length}</span>}</h2>
      {controls && <button type="button" className="space-action space-secondary" ref={controls.uploadRef}
        aria-haspopup="dialog" disabled={controls.disabled} onClick={controls.onUpload}>
        <Icon name="add" />Прикрепить файл
      </button>}
    </header>
    {files.length + unverified.length > 0 && <ul className="page-attachment-list">
      {files.map(file => <li className="page-attachment-row" key={file.id}>
        <span className="page-attachment-icon"><Icon name={file.imageTo ? 'image' : 'file'} /></span>
        <span className="page-attachment-meta">
          <strong>{file.name}</strong>
          <small>{sizeLabel(file.bytes)}{controls?.staged.includes(file.id) && ' · Появится после сохранения'}</small>
        </span>
        <div className="page-attachment-actions">
          {controls ? <>
            {file.imageTo && <button type="button" className="space-action space-secondary" aria-haspopup="dialog"
              disabled={controls.disabled} onClick={() => controls.onInsert(file)} aria-label={`Вставить в текст ${file.name}`}>
              Вставить в текст
            </button>}
            <button type="button" className="space-action space-secondary" disabled={controls.disabled} aria-label={`Удалить ${file.name}`}
              onClick={() => controls.onRemove(file)}>Удалить</button>
          </> : file.downloadTo && <a className="space-action space-secondary" href={file.downloadTo} aria-label={`Скачать ${file.name}`}>
            Скачать
          </a>}
        </div>
      </li>)}
      {controls && unverified.map((id, index) => {
        const name = `Вложение из черновика ${index + 1}`;
        return <li className="page-attachment-row" key={id}>
          <span className="page-attachment-meta"><strong>{name}</strong></span>
          <button className="space-action space-secondary" type="button" disabled={controls.disabled}
            aria-label={`Убрать вложение ${index + 1} из черновика`} onClick={() => controls.onRemove({id,name})}>
            Убрать
          </button>
        </li>;
      })}
    </ul>}
    {unverified.length > 0 && <p className="space-hint">Загруженные файлы будут повторно проверены при сохранении.</p>}
    {files.length + unverified.length === 0 && <p className="space-hint">У страницы пока нет вложений.</p>}
  </section>;
}
