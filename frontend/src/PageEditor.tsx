import {useEffect, useLayoutEffect, useRef, useState, type FormEvent, type KeyboardEvent} from 'react';
import {flushSync} from 'react-dom';
import {Icon} from './Icon';
import {coverLabel, CoverConflictOptions, CoverGallery, PageCoverArt} from './editor/CoverGallery';
import {DraftDialogs, type DialogValue} from './editor/DraftDialogs';
import {createDraftHistory, type DraftEditKind, type DraftFrame, type DraftHistoryDirection} from './editor/draftHistory';
import {applyLink, pasteMarkdown, type EditorChange} from './editor/editorChanges';
import {ListStyleMenu} from './editor/ListStyleMenu';
import type {SourceRange} from './editor/markdownModel';
import {chooseCoverDraft, resolveCoverDraft, type CoverChoice, type CoverDraft, type CoverView} from './editor/pageCover';
import {
  applySourceCommand, decodeLegacyText, RichMarkdownEditor,
  type EditorCommand, type RichMarkdownEditorHandle,
} from './editor/RichMarkdownEditor';
import type {PageEditorForm} from './model';

const maxLiteralLength = 30_000;
const maxSourceLength = 60_000;
const tools = [
  {command: 'heading', label: 'Заголовок'},
  {command: 'bold', label: 'Жирный'},
  {command: 'italic', label: 'Курсив'},
  {command: 'strike', label: 'Зачеркнуть'},
  {command: 'list', label: 'Список'},
  {command: 'code', label: 'Код'},
] as const;

type DialogRequest = ({kind: 'link'; label: string} | {kind: 'paste'}) & {range: SourceRange};

function moveToolbarFocus(event: KeyboardEvent<HTMLDivElement>) {
  const direction = {ArrowLeft: -1, ArrowRight: 1, ArrowUp: -1, ArrowDown: 1};
  if (event.defaultPrevented || !(event.target instanceof HTMLButtonElement) || !event.target.hasAttribute('data-markdown-tool')) return;
  if (!(event.key in direction) && event.key !== 'Home' && event.key !== 'End') return;

  const buttons = [...event.currentTarget.querySelectorAll<HTMLButtonElement>('button[data-markdown-tool]:enabled')];
  if (!buttons.length) return;
  const current = buttons.indexOf(event.target as HTMLButtonElement);
  const offset = direction[event.key as keyof typeof direction];
  const next = event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 :
    (current + offset + buttons.length) % buttons.length;
  event.preventDefault();
  buttons[next]?.focus();
}

export function PageEditor({form}: {form: PageEditorForm}) {
  const {action, cancelTo, revision, error, conflict, canSave} = form;
  const [draft, setDraft] = useState({source: form.source, contentFormat: form.contentFormat});
  const [editorMode, setEditorMode] = useState(form.editorMode);
  const [activeTool, setActiveTool] = useState<EditorCommand | 'source' | 'list-options' | 'link'>('heading');
  const [saving, setSaving] = useState(false);
  const [clientError, setClientError] = useState('');
  const [rebase, setRebase] = useState(false);
  const [coverDraft, setCoverDraft] = useState<CoverDraft>(() => ({
    cover: form.cover ?? null,
    base: form.coverBase ?? (form.coverChanged ? null : form.cover?.value ?? null),
    changed: form.coverChanged ?? false,
  }));
  const [coverChoice, setCoverChoice] = useState<CoverChoice | null>(null);
  const [coverOpen, setCoverOpen] = useState(false);
  const coverButton = useRef<HTMLButtonElement>(null);
  const [dialog, setDialog] = useState<DialogRequest>();
  const [history] = useState(createDraftHistory);
  const input = useRef<HTMLTextAreaElement>(null);
  const rich = useRef<RichMarkdownEditorHandle>(null);
  const focusRange = useRef<SourceRange | undefined>(undefined);
  const rawBeforeInput = useRef<SourceRange | undefined>(undefined);
  const composing = useRef<DraftFrame | undefined>(undefined);
  const submitting = useRef(false);
  const sourceMode = editorMode === 'source';
  const visibleError = clientError || error;
  const describedBy = visibleError ? 'page-editor-error' : undefined;
  const readOnly = !canSave || saving;
  const cover = resolveCoverDraft(coverDraft, conflict?.cover, coverChoice, rebase);
  const hasCover = Boolean(cover.preview && cover.preview.value !== 'none');
  const showCover = hasCover || canSave && (form.cover !== undefined || form.coverUploadTo != null);
  const coverConflictPending = cover.requiresChoice && coverChoice === null;
  const textStillUnchanged = form.textBase && form.textUnchanged && draft.source === form.source && draft.contentFormat === form.contentFormat;

  function restoreSelection(range?: SourceRange) {
    if (sourceMode) {
      const field = input.current;
      field?.focus({preventScroll: true});
      if (range) field?.setSelectionRange(range.start, range.end);
    } else if (range) rich.current?.restoreSelection(range);
    else rich.current?.focus();
  }

  useLayoutEffect(() => {
    if (dialog || !focusRange.current) return;
    restoreSelection(focusRange.current);
    focusRange.current = undefined;
  }, [dialog, sourceMode]);

  useLayoutEffect(() => { restoreSelection(); }, []);

  useEffect(() => {
    function restoreFromHistory(event: PageTransitionEvent) {
      if (!event.persisted) return;
      submitting.current = false;
      setSaving(false);
    }
    window.addEventListener('pageshow', restoreFromHistory);
    return () => window.removeEventListener('pageshow', restoreFromHistory);
  }, []);

  function record(next: typeof draft, before: SourceRange, after: SourceRange, kind: DraftEditKind, previous = draft) {
    if (!canSave || submitting.current) return;
    history.record({...previous, selection: before}, {...next, selection: after}, kind);
    setDraft(next);
    setClientError('');
  }

  function applyEdit(change: EditorChange, before: SourceRange, kind: DraftEditKind = 'atomic') {
    record({source: change.source, contentFormat: change.legacy ? 'literal' : 'markdown'}, before,
      {start: change.start, end: change.end}, kind);
  }

  function sourceDraft(source: string): typeof draft {
    return source === draft.source ? draft : {source, contentFormat: source === form.source ? form.contentFormat : 'markdown'};
  }

  function selection(): SourceRange {
    const field = input.current;
    return field ? {start: field.selectionStart, end: field.selectionEnd} : rich.current?.selection() ?? {start: 0, end: 0};
  }

  function stepHistory(direction: DraftHistoryDirection) {
    if (!canSave || submitting.current || composing.current) return;
    const frame = history.step(direction);
    if (!frame) return;
    flushSync(() => {
      setDraft({source: frame.source, contentFormat: frame.contentFormat});
      setClientError('');
    });
    restoreSelection(frame.selection);
  }

  useLayoutEffect(() => {
    const field = input.current;
    if (!field) return;
    function rememberSource(event: InputEvent) {
      if (event.cancelable && (event.inputType === 'historyUndo' || event.inputType === 'historyRedo')) {
        event.preventDefault();
        stepHistory(event.inputType === 'historyUndo' ? 'undo' : 'redo');
      } else rawBeforeInput.current = selection();
    }
    field.addEventListener('beforeinput', rememberSource);
    return () => field.removeEventListener('beforeinput', rememberSource);
  });

  function apply(command: EditorCommand) {
    if (!canSave || submitting.current) return;
    if (!sourceMode) {
      rich.current?.apply(command);
      return;
    }
    const field = input.current;
    if (!field) return;
    const before = selection();
    const change = applySourceCommand(field.value, before.start, before.end, command);
    flushSync(() => record(sourceDraft(change.source), before, {start: change.start, end: change.end}, 'atomic'));
    restoreSelection(change);
  }

  function openLink() {
    const range = selection();
    const label = sourceMode ? draft.source.slice(range.start, range.end) : rich.current?.selectedText() || '';
    if (!canSave || submitting.current || !label.trim() || range.start === range.end) {
      restoreSelection(range);
      return;
    }
    history.separate();
    setDialog({kind: 'link', range, label});
  }

  function closeDialog(range: SourceRange) {
    focusRange.current = range;
    setDialog(undefined);
  }

  function applyDialog(value: DialogValue) {
    if (!dialog || submitting.current || !canSave) return false;
    const {source, contentFormat} = draft;
    const legacy = contentFormat === 'literal';
    const range = dialog.range;
    const change = value.kind === 'link' ? applyLink(source, legacy, range, value.label, value.address)
      : pasteMarkdown(source, legacy, range, value.text);
    if (change.source === source && change.legacy === legacy) return false;
    applyEdit(change, dialog.kind === 'paste' ? {start: range.end, end: range.end} : range, 'atomic');
    closeDialog({start: change.start, end: change.end});
    return true;
  }

  function chooseCover(selected: CoverView) {
    if (!conflict?.cover && selected.value === (cover.preview?.value ?? 'none')) return;
    setCoverDraft(chooseCoverDraft(coverDraft, selected, conflict?.cover));
    setCoverChoice(null);
  }

  function closeCover() {
    setCoverOpen(false);
    coverButton.current?.focus({preventScroll: true});
  }

  function onSubmit(event: FormEvent<HTMLFormElement>) {
    if (!canSave || submitting.current || conflict && (!rebase || coverConflictPending)) {
      event.preventDefault();
      return;
    }
    let limitError = '';
    if (Array.from(draft.source).length > maxSourceLength) {
      limitError = 'Введите не больше 60 000 символов исходника.';
    } else if (draft.contentFormat === 'literal' && Array.from(decodeLegacyText(draft.source)).length > maxLiteralLength) {
      limitError = 'Введите не больше 30 000 символов.';
    }
    if (limitError) {
      event.preventDefault();
      setClientError(limitError);
      if (sourceMode) input.current?.focus();
      else rich.current?.focus();
      return;
    }
    submitting.current = true;
    setSaving(true);
  }

  function handleShortcut(event: KeyboardEvent<HTMLFormElement>) {
    if (!(event.metaKey || event.ctrlKey) || event.altKey || event.nativeEvent.isComposing) return;
    const key = event.key.toLowerCase();
    if (!event.shiftKey && key === 's') {
      event.preventDefault();
      if (canSave) event.currentTarget.requestSubmit();
    } else if (event.target instanceof HTMLElement && event.target.closest('#page-text') &&
        (key === 'z' || key === 'y' && event.ctrlKey && !event.metaKey && !event.shiftKey)) {
      event.preventDefault();
      stepHistory(key === 'y' || event.shiftKey ? 'redo' : 'undo');
    }
  }

  return <>
    <form className="page-editor" method="post" action={action} onSubmit={onSubmit} onKeyDown={handleShortcut}>
      <input type="hidden" name="revision" value={revision} />
      <input type="hidden" name="text" value={draft.source} />
      <input type="hidden" name="contentFormat" value={draft.contentFormat} />
      <input type="hidden" name="editorMode" value={editorMode} />
      {form.textBase && <input type="hidden" name="textBase" value={form.textBase} />}
      {cover.base !== null && <input type="hidden" name="coverBase" value={cover.base} />}
      {cover.cover !== null && <input type="hidden" name="cover" value={cover.cover} />}
      {visibleError && <p className="form-error" id="page-editor-error" role="alert">{visibleError}</p>}
      {showCover && <div className={`page-cover-slot${hasCover ? ' page-cover-slot-filled' : ''}`}>
        {hasCover && <div className="page-cover-preview">
          <PageCoverArt cover={cover.preview} />
          <span className="page-cover-caption">{coverLabel(cover.preview)}</span>
        </div>}
        {canSave && <button ref={coverButton} className="space-action space-secondary page-cover-change" type="button"
          aria-haspopup="dialog" disabled={saving} onClick={() => setCoverOpen(true)}>
          <Icon name={hasCover ? 'edit' : 'add'} />{hasCover ? 'Изменить обложку' : 'Добавить обложку'}
        </button>}
      </div>}
      <div className="page-editor-toolbar" role="toolbar" aria-label="Форматирование Markdown"
        onKeyDown={moveToolbarFocus} onMouseDown={event => { if (event.button === 0) event.preventDefault(); }}>
        {tools.map(({command, label}) => {
          const tool = <button data-markdown-tool="" key={command} type="button" disabled={readOnly}
            tabIndex={canSave && activeTool === command ? 0 : -1}
            onFocus={() => setActiveTool(command)} onClick={() => apply(command)}>
            {command === 'list' && <Icon name="list" />}{label}
          </button>;
          return command === 'list' ? <div key={command} className="page-list-group">
            {tool}
            <ListStyleMenu disabled={readOnly} active={activeTool === 'list-options'}
              onFocus={() => setActiveTool('list-options')} onApply={apply} />
          </div> : tool;
        })}
        <button data-markdown-tool="" type="button" disabled={readOnly} tabIndex={activeTool === 'link' ? 0 : -1}
          onFocus={() => setActiveTool('link')} onClick={openLink}>Ссылка</button>
        <button data-markdown-tool="" className="page-source-toggle" type="button" aria-pressed={sourceMode} aria-controls="page-text"
          tabIndex={!canSave || activeTool === 'source' ? 0 : -1} disabled={saving}
          onFocus={() => setActiveTool('source')}
          onClick={() => {
            if (submitting.current) return;
            focusRange.current = selection();
            history.separate();
            rawBeforeInput.current = undefined;
            setEditorMode(sourceMode ? 'visual' : 'source');
          }}>
          <Icon name="code" />Исходник
        </button>
      </div>
      <div className={conflict ? 'page-editor-columns page-editor-columns-conflict' : 'page-editor-columns'}>
        <div className={sourceMode ? 'page-editor-field' : 'page-rich-field'}>
          {sourceMode ? (
            <>
              <label className="page-editor-label" htmlFor="page-text">Исходный Markdown</label>
              <textarea className="page-source-field" id="page-text" rows={12} ref={input} value={draft.source}
                readOnly={readOnly} spellCheck={false} aria-invalid={Boolean(clientError) || undefined}
                aria-describedby={describedBy}
                onCompositionStart={() => { composing.current = {...draft, selection: selection()}; }}
                onCompositionEnd={event => {
                  const original = composing.current;
                  composing.current = undefined;
                  rawBeforeInput.current = undefined;
                  if (original) record(sourceDraft(event.currentTarget.value), original.selection, selection(), 'atomic', original);
                }}
                onChange={event => {
                  if (readOnly || submitting.current) return;
                  const after = selection();
                  const before = rawBeforeInput.current ?? after;
                  rawBeforeInput.current = undefined;
                  const next = sourceDraft(event.currentTarget.value);
                  if (composing.current) setDraft(next);
                  else {
                    const native = event.nativeEvent as InputEvent;
                    record(next, before, after, native.inputType === 'insertText' && !native.isComposing ? 'typing' : 'atomic');
                  }
                }} />
            </>
          ) : (
            <RichMarkdownEditor ref={rich} id="page-text" source={draft.source} legacy={draft.contentFormat === 'literal'}
              onChange={applyEdit} readOnly={readOnly} describedBy={describedBy} onHistory={stepHistory}
              onPasteMarkdown={range => {
                history.separate();
                setDialog({kind: 'paste', range});
              }} />
          )}
        </div>
        {conflict && (
          <section className="page-editor-current" aria-labelledby="page-editor-current-heading">
            <h2 id="page-editor-current-heading">Актуальный текст</h2>
            <div className="page-editor-current-text" tabIndex={0} aria-labelledby="page-editor-current-heading">
              {conflict.text || 'Пока здесь ничего нет.'}
            </div>
            {!cover.requiresChoice && conflict.cover && <div className="page-cover-current">
              <h3>Опубликованная обложка</h3>
              {conflict.cover.value !== 'none' && <div className="page-cover-conflict-preview">
                <PageCoverArt cover={conflict.cover} />
              </div>}
              <p>{coverLabel(conflict.cover)}</p>
            </div>}
          </section>
        )}
      </div>
      {cover.requiresChoice && conflict?.cover && <CoverConflictOptions draft={coverDraft.cover} published={conflict.cover}
        choice={coverChoice} disabled={readOnly} onChoice={setCoverChoice} />}
      {conflict && textStillUnchanged && <p className="space-hint">Текст в черновике не менялся. После сохранения останется актуальный текст.</p>}
      {conflict && canSave && (
        <label className="page-editor-rebase">
          <input type="checkbox" name="baseRevision" value={conflict.revision} checked={rebase}
            onClick={event => { if (submitting.current) event.preventDefault(); }}
            onChange={event => setRebase(event.currentTarget.checked)} />
          Использовать эту версию как основу
        </label>
      )}
      <div className="page-editor-actions">
        <span className="page-editor-status" role="status">{saving ? 'Сохраняем…' : ''}</span>
        {cancelTo && (
          <a className="space-action space-secondary" href={cancelTo} aria-disabled={saving || undefined}
            onClick={event => { if (submitting.current) event.preventDefault(); }}>
            {canSave ? 'Отменить' : 'Вернуться к чтению'}
          </a>
        )}
        {canSave && (
          <button className="space-action primary-action" type="submit" disabled={saving || Boolean(conflict && (!rebase || coverConflictPending))}>
            Сохранить
          </button>
        )}
      </div>
    </form>
    {coverOpen && <CoverGallery cover={cover.preview} uploadTo={form.coverUploadTo} onPick={chooseCover} onDismiss={closeCover} />}
    {dialog && <DraftDialogs request={dialog} onApply={applyDialog} onCancel={() => {
      const range = dialog.range;
      closeDialog(dialog.kind === 'paste' ? {start: range.end, end: range.end} : range);
    }} />}
  </>;
}
