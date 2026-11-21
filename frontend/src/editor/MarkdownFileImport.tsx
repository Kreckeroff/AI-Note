import {useEffect, useId, useLayoutEffect, useRef, useState} from 'react';
import {createPortal} from 'react-dom';
import {Icon} from '../Icon';
import {readMarkdownFile} from './markdownFile';

type DraftText = {source: string; contentFormat: 'markdown' | 'literal'};
type Props = {
  draft: DraftText;
  disabled: boolean;
  onReplace(source: string, expected: DraftText): 'applied' | 'changed' | 'blocked';
};

function ImportDialog({draft, disabled, onReplace, onDismiss}: Props & {onDismiss(): void}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const heading = useId();
  const [file, setFile] = useState<File>();
  const [expected, setExpected] = useState<DraftText>(draft);
  const [preview, setPreview] = useState<{file: File; source: string}>();
  const [error, setError] = useState('');
  const changed = draft.source !== expected.source || draft.contentFormat !== expected.contentFormat;
  const ready = preview && preview.file === file;

  useLayoutEffect(() => { dialog.current?.showModal(); }, []);
  useEffect(() => {
    if (!file) return;
    let active = true;
    readMarkdownFile(file).then(source => {
      if (active) setPreview({file, source});
    }, reason => {
      if (active) setError(reason.message);
    });
    return () => { active = false; };
  }, [file]);

  function close() {
    dialog.current?.close();
    onDismiss();
  }

  function apply() {
    if (!ready || disabled) return;
    if (changed) {
      setExpected(draft);
      setError('');
      return;
    }
    const outcome = onReplace(preview.source, expected);
    if (outcome === 'applied') close();
    else setError(outcome === 'changed' ? 'Текст черновика обновился. Сверьте замену ещё раз.'
      : 'Сейчас импорт недоступен. Вернитесь в редактор и проверьте состояние черновика.');
  }

  return <dialog ref={dialog} className="space-dialog page-draft-dialog" aria-labelledby={heading}
    onKeyDown={event => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 's') {
        event.preventDefault();
        event.stopPropagation();
      }
    }} onCancel={event => { event.preventDefault(); close(); }}>
    <header className="space-dialog-header"><h2 id={heading}>Импорт Markdown</h2></header>
    <div className="space-form">
      <label className="page-draft-field">Файл Markdown
        <input type="file" accept=".md,.markdown" onChange={event => {
          setFile(event.currentTarget.files?.[0]);
          setPreview(undefined);
          setError('');
          setExpected(draft);
        }} />
      </label>
      <p className="space-hint">UTF-8, до 60 000 символов. Название, обложка и вложения страницы сохранятся. Читатели увидят текст после публикации.</p>
      {ready && <label className="page-draft-field">Текст из файла
        <textarea readOnly rows={5} value={preview.source} placeholder="Пустой файл" />
      </label>}
      {changed && ready && <p className="space-hint" role="status">Пока файл был открыт, черновик изменился. Проверьте его текст перед заменой.</p>}
      {ready && draft.source && <details open={changed || undefined}>
        <summary>Текст черновика, который будет заменён</summary>
        <textarea className="page-source-field" aria-label="Текст черновика до замены" value={draft.source} readOnly rows={4} />
      </details>}
      {error && <p className="form-error" role="alert">{error}</p>}
      <div className="space-dialog-actions">
        <button type="button" className="space-action space-secondary" onClick={close}>Отменить</button>
        <button type="button" className="space-action primary-action" onClick={apply} disabled={disabled || !ready}>
          {changed && ready ? 'Подтвердить актуальную версию' : draft.source ? 'Заменить текст черновика' : 'Импортировать в черновик'}
        </button>
      </div>
    </div>
  </dialog>;
}

export function MarkdownFileImport(props: Props) {
  const [open, setOpen] = useState(false);
  return <div>
    <button type="button" className="space-action space-secondary" disabled={props.disabled} aria-haspopup="dialog"
      onClick={() => setOpen(true)}><Icon name="upload" />Импорт Markdown</button>
    {open && createPortal(<ImportDialog {...props} onDismiss={() => setOpen(false)} />, document.body)}
  </div>;
}
