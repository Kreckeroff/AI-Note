import {useEffect, useLayoutEffect, useMemo, useRef, useState, type FormEvent, type KeyboardEvent} from 'react';
import {flushSync} from 'react-dom';
import {Icon} from './Icon';
import {newOperationId} from './collaboration/api';
import {
  draftSplice, liftPrivateMediaWithSteps, projectPrivateMedia, projectPrivateMediaSplice,
  selectionAfterSplice, spliceAtSelection, type DraftText, type TextSplice,
} from './collaboration/draftSplice';
import {readPublishedPage} from './collaboration/publishedPage';
import {rebaseSharedHistory} from './collaboration/sharedDraftHistory';
import {sharedDraftStatus, SharedDraftRecovery} from './collaboration/SharedDraftStatus';
import {useSharedDraft} from './collaboration/useSharedDraft';
import {coverLabel, CoverConflictOptions, CoverGallery, PageCoverArt} from './editor/CoverGallery';
import {DraftDialogs, type DialogValue} from './editor/DraftDialogs';
import {createDraftHistory, type DraftEditKind, type DraftFrame, type DraftHistoryDirection} from './editor/draftHistory';
import {applyLink, attachmentRanges, insertAttachmentImage, insertExternalEmbed, pasteMarkdown, removeAttachmentReferences,
  updateExternalEmbed, type EditorChange} from './editor/editorChanges';
import {ListStyleMenu} from './editor/ListStyleMenu';
import {BlockInsertButton} from './editor/BlockInsertButton';
import {MarkdownFileImport} from './editor/MarkdownFileImport';
import {maxMarkdownSourceLength} from './editor/markdownFile';
import {parseMarkdown, type SelectionFormatting, type SourceRange} from './editor/markdownModel';
import {PageAttachmentDialog, type AttachmentCompletion, type AttachmentDialogRequest} from './editor/PageAttachmentDialog';
import {PageAttachmentList} from './editor/PageAttachmentList';
import {PageEmbedDialog} from './editor/PageEmbedDialog';
import {externalPageEmbedHint, teamStreamPublicHint} from './editor/pageEmbedInput';
import {addDraftAttachment, removeDraftAttachment, type AttachmentDraft, type PageAttachment} from './editor/pageAttachments';
import {chooseCoverDraft, resolveCoverDraft, type CoverChoice, type CoverDraft, type CoverView} from './editor/pageCover';
import {
  applySourceCommand, decodeLegacyText, RichMarkdownEditor,
  type EditorCommand, type RichMarkdownEditorHandle,
} from './editor/RichMarkdownEditor';
import type {PageEditorForm} from './model';

const maxLiteralLength = 30_000;
const tools = [
  {command: 'heading', label: 'Заголовок'},
  {command: 'bold', label: 'Жирный'},
  {command: 'italic', label: 'Курсив'},
  {command: 'strike', label: 'Зачеркнуть'},
  {command: 'list', label: 'Список'},
  {command: 'code', label: 'Код'},
] as const;

type DialogRequest = ({kind: 'link'; label: string} | {kind: 'paste'} |
  {kind: 'embed'; address?: string; opener?: HTMLElement}) & {range: SourceRange};
type EditorFrame = DraftFrame & {attachments: AttachmentDraft};
type EditorDraft = Omit<EditorFrame, 'selection'>;
type AttachmentRequest = AttachmentDialogRequest & {range: SourceRange; previousSelection: SourceRange};

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

export function PageEditor({form, pageTo, pageLinkSearchTo, onAccessEnded, renamedAtRevision, publicLinkActive = form.publicLinkActive}: {
  form: PageEditorForm; pageTo?: string; pageLinkSearchTo?: string; onAccessEnded?(): void; renamedAtRevision?: string; publicLinkActive?: boolean;
}) {
  const {action, cancelTo, revision, error, conflict, canSave} = form;
  const [draft, setDraft] = useState<EditorDraft>(() => ({
    source: form.source, contentFormat: form.contentFormat, textBase: form.textBase,
    attachments: {files:form.attachments ?? [], added:form.attachmentAdded ?? [], removed:form.attachmentRemoved ?? []},
  }));
  const [editorMode, setEditorMode] = useState(form.editorMode);
  const [formatting, setFormatting] = useState<SelectionFormatting>({bold:false, italic:false, strike:false, code:false});
  const [activeTool, setActiveTool] = useState<EditorCommand | 'source' | 'list-options' | 'link' | 'image' | 'insert'>('heading');
  const [saving, setSaving] = useState(false);
  const [clientError, setClientError] = useState('');
  const [rebase, setRebase] = useState(false);
  const [coverDraft, setCoverDraft] = useState<CoverDraft>(() => ({
    cover: form.cover ?? null,
    base: form.coverBase ?? (form.coverChanged ? null : form.cover?.value ?? null),
    changed: form.coverChanged ?? false,
  }));
  const [coverChoice, setCoverChoice] = useState<CoverChoice | null>(null);
  const [publishedMedia,setPublishedMedia] = useState<{revision:string;cover:CoverView}>();
  const [placement,setPlacement] = useState<{text:DraftText;previous:string}>();
  const [placementCopy,setPlacementCopy] = useState<string>();
  const [coverOpen, setCoverOpen] = useState(false);
  const coverButton = useRef<HTMLButtonElement>(null);
  const [dialog, setDialog] = useState<DialogRequest>();
  const [attachmentDialog, setAttachmentDialog] = useState<AttachmentRequest>();
  const attachmentTrigger = useRef<HTMLButtonElement>(null);
  const attachmentOpener = useRef<HTMLElement | null>(null);
  const [history] = useState(() => createDraftHistory<EditorFrame>());
  const input = useRef<HTMLTextAreaElement>(null);
  const rich = useRef<RichMarkdownEditorHandle>(null);
  const focusRange = useRef<SourceRange | undefined>(undefined);
  const rawBeforeInput = useRef<SourceRange | undefined>(undefined);
  const composing = useRef<EditorFrame | undefined>(undefined);
  const submitting = useRef(false);
  const currentDraft = useRef(draft);
  const richComposing = useRef(false);
  const sharedPath = canSave && !form.coeditRetry ? pageTo : undefined;
  const shared = useSharedDraft(sharedPath, form.error || conflict ? projectPrivateMedia({
    source:form.source, format:form.contentFormat,
  }, form.attachmentAdded ?? []).text : undefined, receiveSharedDraft, () => {
    if (onAccessEnded) onAccessEnded();
    else if (sharedPath) window.location.replace(sharedPath);
  });
  const sharedState = shared.state;
  const confirmed = sharedState?.snapshot;
  const waitingForRename = Boolean(sharedPath && renamedAtRevision &&
    BigInt(renamedAtRevision) > BigInt(confirmed?.publishedRevision ?? revision));
  const sharedReady = sharedState?.status === 'ready' && sharedState.pending === 0;
  const publishOperation = useMemo(() => newOperationId(), [confirmed?.version, coverDraft, coverChoice, draft.attachments]);
  const publishGuard = form.coeditRetry ?? (confirmed ? {
    version:confirmed.version,lease:confirmed.lease,operation:publishOperation,
  } : undefined);
  const sourceMode = editorMode === 'source';
  const visibleError = clientError || error;
  const describedBy = visibleError ? 'page-editor-error' : undefined;
  const readOnly = !canSave || saving || Boolean(form.coeditRetry || placement) || Boolean(sharedState &&
    (['opening','expired','recover','rejected'].includes(sharedState.status) ||
    !confirmed));
  const publishedCover = publishedMedia?.cover ?? conflict?.cover;
  const mediaReloading = Boolean(confirmed && confirmed.publishedRevision!==form.revision &&
    publishedMedia?.revision!==confirmed.publishedRevision);
  const cover = resolveCoverDraft(coverDraft,publishedCover,coverChoice,rebase || Boolean(publishedMedia));
  const hasCover = Boolean(cover.preview && cover.preview.value !== 'none');
  const showCover = hasCover || canSave && (form.cover !== undefined || form.coverUploadTo != null);
  const coverConflictPending = cover.requiresChoice && coverChoice === null;
  const supportsAttachments = Boolean(form.attachmentDraft && form.attachmentUploadTo);
  const attachments = draft.attachments;
  const changedAttachments = attachments.added.length + attachments.removed.length > 0;
  const textStillUnchanged = draft.textBase && form.textUnchanged && draft.source === form.source && draft.contentFormat === form.contentFormat;
  const embedNotice = useMemo(() => {
    if (!canSave || !publicLinkActive || draft.contentFormat !== 'markdown' || !draft.source.includes('sever-embed')) return null;
    const blocks = parseMarkdown(draft.source, false).blocks;
    if (blocks.some(block => block.embed?.kind === 'teamstream')) return `${externalPageEmbedHint} ${teamStreamPublicHint}`;
    return blocks.some(block => block.embed) ? externalPageEmbedHint : null;
  }, [canSave, publicLinkActive, draft.contentFormat, draft.source]);

  function replaceDraft(next: EditorDraft) {
    currentDraft.current = next;
    setDraft(next);
  }

  function mediaProjection(value: EditorDraft) {
    return projectPrivateMedia({source:value.source,format:value.contentFormat},value.attachments.added);
  }

  function scopedText(value: EditorDraft): DraftText {
    return mediaProjection(value).text;
  }

  function receiveSharedDraft(text: DraftText, steps?: TextSplice[]) {
    const current = currentDraft.current;
    const restored=liftPrivateMediaWithSteps(mediaProjection(current),text,current.attachments.added,steps);
    if (!restored) {
      shared.transport.current?.setPaused(true);
      setPlacement({text,previous:current.source});
      return;
    }
    const {text:ui,steps:exact}=restored;
    if (exact?.length) rebaseSharedHistory(history,current.source,exact);
    if (!exact?.length && ui.source===current.source && ui.format===current.contentFormat) return;
    if (document.activeElement?.id === 'page-text') {
      focusRange.current = (exact ?? [draftSplice(current.source,ui.source)]).reduce(selectionAfterSplice,selection());
    }
    replaceDraft({...current, source:ui.source,contentFormat:ui.format,textBase:null});
  }

  function updateSharedPause() {
    shared.transport.current?.setPaused(Boolean(composing.current || richComposing.current ||
      dialog || attachmentDialog || coverOpen || placement || submitting.current));
  }

  useEffect(() => {
    updateSharedPause();
  }, [dialog, attachmentDialog, coverOpen, placement, saving]);

  useEffect(() => {
    if (!sharedPath || !confirmed?.publishedRevision || confirmed.publishedRevision === form.revision) return;
    const request = new AbortController();
    const expected=confirmed.publishedRevision;
    let retry: number | undefined;
    async function updateMedia() {
      try {
        const publication=await readPublishedPage(sharedPath!,request.signal);
        if (request.signal.aborted) return;
        const latest=publication?.reading;
        if (latest?.revision===expected) {
          const current=currentDraft.current;
          const added=new Set(current.attachments.added);
          const removed=new Set(current.attachments.removed);
          const retained=(latest.attachments ?? []).filter(file => !removed.has(file.id) && !added.has(file.id));
          const staged=current.attachments.files.filter(file => added.has(file.id));
          replaceDraft({...current,attachments:{...current.attachments,files:[...retained,...staged]}});
          setPublishedMedia({revision:expected,cover:latest.cover ?? {value:'none',imageTo:null}});
          setCoverChoice(null);
          return;
        }
      } catch {
        // Keep staged media and the previous cover preview while retrying.
      }
      if (!request.signal.aborted) retry=window.setTimeout(updateMedia,2500);
    }
    void updateMedia();
    return () => {
      request.abort();
      if (retry !== undefined) window.clearTimeout(retry);
    };
  }, [sharedPath, confirmed?.publishedRevision, form.revision]);

  function restoreSelection(range?: SourceRange) {
    if (sourceMode) {
      const field = input.current;
      field?.focus({preventScroll: true});
      if (range) field?.setSelectionRange(range.start, range.end);
    } else if (range) rich.current?.restoreSelection(range);
    else rich.current?.focus();
  }

  useLayoutEffect(() => {
    if (dialog || attachmentDialog || !focusRange.current) return;
    restoreSelection(focusRange.current);
    focusRange.current = undefined;
  }, [dialog, attachmentDialog, sourceMode, draft]);

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
    if (readOnly || submitting.current) return false;
    const authored=spliceAtSelection(previous.source,next.source,before,after);
    const nextShared=scopedText(next);
    const exact=projectPrivateMediaSplice(mediaProjection(previous),nextShared,authored);
    if (shared.transport.current?.edit(nextShared,exact)===false) {
      replaceDraft(previous);
      setClientError('Совместите выделенный фрагмент перед его изменением.');
      if (input.current) {
        input.current.value=previous.source;
        input.current.setSelectionRange(before.start,before.end);
      }
      return false;
    }
    history.record({...previous, selection: before}, {...next, selection: after},
      sharedState ? 'atomic' : kind,Date.now(),sharedState ? authored : undefined);
    replaceDraft(next);
    setClientError('');
    return true;
  }

  function applyEdit(change: EditorChange, before: SourceRange, kind: DraftEditKind = 'atomic') {
    return record({...draft, source: change.source, contentFormat: change.legacy ? 'literal' : 'markdown'}, before,
      {start: change.start, end: change.end}, kind);
  }

  function sourceDraft(source: string): typeof draft {
    return source === draft.source ? draft : {...draft, source, contentFormat: sharedState?.conflict?.persistent ? draft.contentFormat : source === form.source ? form.contentFormat : 'markdown'};
  }

  function selection(): SourceRange {
    const field = input.current;
    return field ? {start: field.selectionStart, end: field.selectionEnd} : rich.current?.selection() ?? {start: 0, end: 0};
  }

  function stepHistory(direction: DraftHistoryDirection) {
    if (!canSave || submitting.current || composing.current) return;
    if (readOnly) return;
    const frame = history.step(direction, sharedState ? (from,to,span,blocked) => {
      const current=currentDraft.current;
      if (blocked || from.source!==current.source || from.contentFormat!==current.contentFormat) {
        setClientError('Коллега изменил этот фрагмент. Совместите правки перед отменой.');
        return;
      }
      const next={...to,attachments:from.attachments===to.attachments ? current.attachments : to.attachments};
      const nextShared=scopedText(next);
      const exact=span && projectPrivateMediaSplice(mediaProjection(current),nextShared,span);
      if (shared.transport.current?.edit(nextShared,exact)===false) {
        setClientError('Совместите выделенный фрагмент перед отменой.');
        return;
      }
      return next;
    } : undefined);
    if (!frame) return;
    flushSync(() => {
      replaceDraft({source:frame.source,contentFormat:frame.contentFormat,attachments:frame.attachments,textBase:frame.textBase});
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
    if (readOnly || submitting.current) return;
    const range = selection();
    const label = sourceMode ? draft.source.slice(range.start, range.end) : rich.current?.selectedText() || '';
    history.separate();
    shared.transport.current?.setPaused(true);
    setDialog({kind: 'link', range, label});
  }

  function closeDialog(range: SourceRange) {
    focusRange.current = range;
    setDialog(undefined);
  }

  function applyDialog(value: DialogValue) {
    if (!dialog || dialog.kind === 'embed' || submitting.current || readOnly) return false;
    const {source, contentFormat} = draft;
    const legacy = contentFormat === 'literal';
    const range = dialog.range;
    const change = value.kind === 'link' ? applyLink(source, legacy, range, value.label, value.address)
      : pasteMarkdown(source, legacy, range, value.text);
    if (change.source === source && change.legacy === legacy) return false;
    if (!applyEdit(change,dialog.kind === 'paste' ? {start:range.end,end:range.end} : range,'atomic')) return false;
    closeDialog({start: change.start, end: change.end});
    return true;
  }

  function openEmbed(range: SourceRange, address?: string, opener?: HTMLElement) {
    if (readOnly || submitting.current) return;
    history.separate();
    shared.transport.current?.setPaused(true);
    setDialog({kind: 'embed', range, address, opener});
  }

  function applyEmbed(address: string) {
    if (dialog?.kind !== 'embed' || readOnly || submitting.current) return false;
    const {source, contentFormat} = draft;
    const legacy = contentFormat === 'literal';
    if (dialog.address) {
      const update = updateExternalEmbed(source, legacy, dialog.range, dialog.address, address);
      if (!update || update.change.source !== source && !applyEdit(update.change, update.before)) return false;
      closeDialog(update.after);
      return true;
    }
    const result = insertExternalEmbed(source, legacy, dialog.range, address);
    if (result.source === source || !applyEdit(result, dialog.range)) return false;
    closeDialog({start: result.start, end: result.end});
    return true;
  }

  function removeEmbed(range: SourceRange, address: string) {
    if (readOnly || submitting.current) return;
    const update = updateExternalEmbed(draft.source, draft.contentFormat === 'literal', range, address);
    if (!update) return;
    history.separate();
    focusRange.current = update.after;
    if (!applyEdit(update.change, update.before)) focusRange.current = undefined;
  }

  function importMarkdown(source: string, expected: Pick<EditorDraft, 'source' | 'contentFormat'>) {
    const current = currentDraft.current;
    if (readOnly || submitting.current) return 'blocked';
    if (current.source !== expected.source || current.contentFormat !== expected.contentFormat) return 'changed';
    const caret = {start:0, end:0};
    return record({...current, source, contentFormat:'markdown'}, selection(), caret, 'atomic', current)
      ? 'applied' : 'blocked';
  }

  function chooseCover(selected: CoverView) {
    if (!publishedCover && selected.value === (cover.preview?.value ?? 'none')) return;
    setCoverDraft(chooseCoverDraft(coverDraft,selected,publishedCover));
    setCoverChoice(null);
  }

  function closeCover() {
    setCoverOpen(false);
    coverButton.current?.focus({preventScroll: true});
  }

  function openAttachment(request: AttachmentDialogRequest, range?: SourceRange) {
    if (readOnly || submitting.current) return;
    const previousSelection = selection();
    const active = document.activeElement;
    attachmentOpener.current = active instanceof HTMLElement ? active : null;
    history.separate();
    setAttachmentDialog({...request, previousSelection, range:range ?? previousSelection});
  }

  function dropAttachment(id: string, previousSelection: SourceRange) {
    const next = removeDraftAttachment(attachments, id);
    const change = removeAttachmentReferences(draft.source, draft.contentFormat === 'literal', previousSelection, id);
    record({...draft, source:change.source, contentFormat:change.legacy ? 'literal' : 'markdown', attachments:next},
      previousSelection, {start:change.start, end:change.end}, 'atomic');
  }

  function removeAttachment(file: Pick<PageAttachment, 'id' | 'name'>) {
    const references = attachmentRanges(draft.source, draft.contentFormat === 'literal', file.id).length;
    if (references) openAttachment({kind:'remove', file, references});
    else {
      dropAttachment(file.id, selection());
      attachmentTrigger.current?.focus({preventScroll:true});
    }
  }

  function completeAttachment(completion: AttachmentCompletion): boolean {
    if (!attachmentDialog || readOnly) return false;
    const {range, previousSelection} = attachmentDialog;
    if (completion.kind === 'remove') {
      dropAttachment(completion.id, previousSelection);
      attachmentOpener.current = attachmentTrigger.current;
      return true;
    }
    const next = addDraftAttachment(attachments, completion.file);
    if (completion.kind === 'add') {
      record({...draft, attachments:next}, previousSelection, previousSelection, 'atomic');
      return true;
    }
    const change = insertAttachmentImage(draft.source, draft.contentFormat === 'literal', range, completion.file.id, completion.alt);
    if (change.source === draft.source) return false;
    const after = {start:change.start, end:change.end};
    if (!record({...draft,source:change.source,contentFormat:'markdown',attachments:next},previousSelection,after,'atomic')) return false;
    focusRange.current = after;
    return true;
  }

  function dismissAttachment() {
    const opener = attachmentOpener.current;
    if (!focusRange.current) {
      if (opener?.closest('#page-text') && attachmentDialog) focusRange.current = attachmentDialog.previousSelection;
      else (opener?.isConnected ? opener : attachmentTrigger.current)?.focus({preventScroll:true});
    }
    setAttachmentDialog(undefined);
  }

  function chooseRebase(next: boolean) {
    setRebase(next);
    if (!next || !textStillUnchanged || !attachments.removed.length || conflict?.source === undefined || !conflict.contentFormat) return;
    const before = selection();
    const change = removeAttachmentReferences(conflict.source, conflict.contentFormat === 'literal', {start:0,end:0}, attachments.removed);
    if (change.source !== conflict.source) {
      // Dropping the old fingerprint makes this a deliberate write even when the cleanup reproduces the old text.
      record({...draft, source:change.source, contentFormat:'markdown', textBase:null}, before, change, 'atomic');
    }
  }

  function onSubmit(event: FormEvent<HTMLFormElement>) {
    if (!canSave || submitting.current || attachmentDialog || placement || waitingForRename || mediaReloading || coverConflictPending ||
        sharedState && !sharedReady || conflict && !rebase) {
      event.preventDefault();
      return;
    }
    let limitError = '';
    if (Array.from(draft.source).length > maxMarkdownSourceLength) {
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
    updateSharedPause();
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
      <input type="hidden" name="revision" value={confirmed?.publishedRevision ?? revision} />
      {publishGuard && <>
        <input type="hidden" name="coedit_version" value={publishGuard.version} />
        <input type="hidden" name="coedit_lease" value={publishGuard.lease} />
        <input type="hidden" name="coedit_operation" value={publishGuard.operation} />
      </>}
      <input type="hidden" name="text" value={draft.source} />
      <input type="hidden" name="contentFormat" value={draft.contentFormat} />
      <input type="hidden" name="editorMode" value={editorMode} />
      {draft.textBase && <input type="hidden" name="textBase" value={draft.textBase} />}
      {cover.base !== null && <input type="hidden" name="coverBase" value={cover.base} />}
      {cover.cover !== null && <input type="hidden" name="cover" value={cover.cover} />}
      {form.attachmentDraft && changedAttachments && <>
        <input type="hidden" name="attachmentDraft" value={form.attachmentDraft} />
        <input type="hidden" name="attachmentAdd" value={attachments.added.join(',')} />
        <input type="hidden" name="attachmentRemove" value={attachments.removed.join(',')} />
      </>}
      {visibleError && <p className="form-error" id="page-editor-error" role="alert">{visibleError}</p>}
      {showCover && <div className={`page-cover-slot${hasCover ? ' page-cover-slot-filled' : ''}`}>
        {hasCover && <div className="page-cover-preview">
          <PageCoverArt cover={cover.preview} />
          <span className="page-cover-caption">{coverLabel(cover.preview)}</span>
        </div>}
        {canSave && <button ref={coverButton} className="space-action space-secondary page-cover-change" type="button"
          aria-haspopup="dialog" disabled={readOnly} onClick={() => setCoverOpen(true)}>
          <Icon name={hasCover ? 'edit' : 'add'} />{hasCover ? 'Изменить обложку' : 'Добавить обложку'}
        </button>}
      </div>}
      <div className="page-editor-toolbar" role="toolbar" aria-label="Форматирование Markdown"
        onKeyDown={moveToolbarFocus} onMouseDown={event => {
          if (event.button === 0 && !(event.target instanceof HTMLInputElement)) event.preventDefault();
        }}>
        {!sourceMode && <BlockInsertButton source={draft.source} disabled={readOnly} attachments={supportsAttachments}
          active={activeTool === 'insert'} onFocus={() => setActiveTool('insert')} selection={selection}
          onChoose={(command, range) => rich.current?.insertBlock(command, range)} />}
        {tools.map(({command, label}) => {
          const tool = <button data-markdown-tool="" key={command} type="button" disabled={readOnly} title={label} aria-label={label}
            aria-pressed={!sourceMode && command !== 'list' && command !== 'heading' ? formatting[command] : undefined}
            tabIndex={canSave && activeTool === command ? 0 : -1}
            onFocus={() => setActiveTool(command)} onClick={() => apply(command)}>
            {command === 'list' || command === 'code' ? <Icon name={command} /> :
              <span className={`page-tool-glyph glyph-${command}`} aria-hidden="true">{{heading:'H₂', bold:'B', italic:'I', strike:'S'}[command]}</span>}
          </button>;
          return command === 'list' ? <div key={command} className="page-list-group">
            {tool}
            <ListStyleMenu disabled={readOnly} active={activeTool === 'list-options'}
              onFocus={() => setActiveTool('list-options')} onApply={apply} />
          </div> : tool;
        })}
        <button data-markdown-tool="" type="button" disabled={readOnly} aria-haspopup="dialog" tabIndex={activeTool === 'link' ? 0 : -1}
          title="Ссылка" aria-label="Ссылка" onFocus={() => setActiveTool('link')} onClick={openLink}><Icon name="link" /></button>
        {supportsAttachments && <button data-markdown-tool="" type="button" disabled={readOnly} aria-haspopup="dialog"
          aria-label="Изображение" title="Изображение"
          tabIndex={activeTool === 'image' ? 0 : -1} onFocus={() => setActiveTool('image')} onClick={() => openAttachment({kind:'image'})}>
          <Icon name="image" />
        </button>}
        <button data-markdown-tool="" className="page-source-toggle" type="button" aria-label="Исходник" title="Исходник Markdown"
          aria-pressed={sourceMode} aria-controls="page-text"
          tabIndex={!canSave || activeTool === 'source' ? 0 : -1} disabled={saving}
          onFocus={() => setActiveTool('source')}
          onClick={() => {
            if (submitting.current) return;
            focusRange.current = selection();
            history.separate();
            rawBeforeInput.current = undefined;
            setEditorMode(sourceMode ? 'visual' : 'source');
          }}>
          <Icon name="code" /><span className="page-source-label">Исходник</span><span className="page-source-short" aria-hidden="true">MD</span>
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
                onCompositionStart={() => {
                  composing.current = {...draft, selection: selection()};
                  updateSharedPause();
                }}
                onCompositionEnd={event => {
                  const original = composing.current;
                  composing.current = undefined;
                  rawBeforeInput.current = undefined;
                  if (original) record(sourceDraft(event.currentTarget.value), original.selection, selection(), 'atomic', original);
                  updateSharedPause();
                }}
                onChange={event => {
                  if (readOnly || submitting.current) return;
                  const after = selection();
                  const before = rawBeforeInput.current ?? after;
                  rawBeforeInput.current = undefined;
                  const next = sourceDraft(event.currentTarget.value);
                  if (composing.current) replaceDraft(next);
                  else {
                    const native = event.nativeEvent as InputEvent;
                    record(next, before, after, native.inputType === 'insertText' && !native.isComposing ? 'typing' : 'atomic');
                  }
                }} />
            </>
          ) : (
            <RichMarkdownEditor ref={rich} id="page-text" source={draft.source} legacy={draft.contentFormat === 'literal'}
              onChange={applyEdit} readOnly={readOnly} describedBy={describedBy} onHistory={stepHistory}
              onFormattingChange={setFormatting}
              onCompositionChange={active => { richComposing.current=active; updateSharedPause(); }}
              attachments={attachments.files} onInsertAttachmentImage={supportsAttachments ? range => openAttachment({kind:'image'}, range) : undefined}
              onInsertEmbed={range => openEmbed(range)}
              onEditEmbed={(block, opener) => { if (block.embed) openEmbed(block, block.embed.url, opener); }}
              onRemoveEmbed={block => { if (block.embed) removeEmbed(block, block.embed.url); }}
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
      {(form.attachments || supportsAttachments) && <PageAttachmentList files={attachments.files} controls={canSave && form.attachmentDraft ? {
        disabled:readOnly || !supportsAttachments, staged:attachments.added, uploadRef:attachmentTrigger,
        onUpload:() => openAttachment({kind:'upload'}), onInsert:file => openAttachment({kind:'image', select:file.id}),
        onRemove:removeAttachment,
      } : undefined} />}
      {canSave && <MarkdownFileImport draft={draft} disabled={readOnly} onReplace={importMarkdown} />}
      {cover.requiresChoice && publishedCover && <CoverConflictOptions draft={coverDraft.cover} published={publishedCover}
        choice={coverChoice} disabled={readOnly} onChoice={setCoverChoice} />}
      {conflict && textStillUnchanged && <p className="space-hint">Текст в черновике не менялся. После сохранения останется актуальный текст.</p>}
      {conflict && canSave && (
        <label className="page-editor-rebase">
          <input type="checkbox" name="baseRevision" value={conflict.revision} checked={rebase}
            onClick={event => { if (submitting.current) event.preventDefault(); }}
            onChange={event => chooseRebase(event.currentTarget.checked)} />
          Использовать эту версию как основу
        </label>
      )}
      {placement && <section className="shared-draft-recovery" aria-labelledby="private-media-placement">
        <h2 id="private-media-placement">Проверьте размещение вложений</h2>
        <p>Коллега изменил текст рядом с вашей вставкой. Файлы остались в черновике. Откройте актуальный текст и разместите их заново.</p>
        <button className="space-action space-secondary" type="button" onClick={() => {
          setPlacementCopy(placement.previous);
          replaceDraft({...currentDraft.current,source:placement.text.source,contentFormat:placement.text.format,textBase:null});
          setPlacement(undefined);
        }}>Открыть актуальный текст</button>
      </section>}
      {placementCopy !== undefined && <details className="shared-draft-recovery">
        <summary>Текст до совмещения вложений</summary>
        <pre tabIndex={0}>{placementCopy}</pre>
        <button className="space-action space-secondary" type="button" onClick={() => setPlacementCopy(undefined)}>Закрыть копию</button>
      </details>}
      {sharedState && <SharedDraftRecovery state={sharedState}
        onReopen={() => shared.transport.current?.reopen()}
        onUseCurrent={() => shared.transport.current?.useCurrent()}
        onDismissCopy={() => shared.transport.current?.dismissCopy()}
        onMergeInput={text => shared.transport.current?.changeMergeInput(text)}
        onResolve={text => shared.transport.current?.resolve(text)} />}
      {embedNotice && <p className="space-hint">{embedNotice}</p>}
      <div className="page-editor-actions">
        <span className="page-editor-status" role="status">{saving ? 'Сохраняем…' : waitingForRename ? 'Обновляем редакцию…' : mediaReloading ? 'Обновляем вложения и обложку…' : sharedDraftStatus(sharedState)}</span>
        {cancelTo && (
          <a className="space-action space-secondary page-editor-return" href={cancelTo} aria-disabled={saving || undefined}
            aria-label={canSave && !sharedState && !form.coeditRetry ? 'Отменить' : 'Вернуться к чтению'}
            title={canSave && !sharedState && !form.coeditRetry ? 'Отменить' : 'Вернуться к чтению'}
            onClick={event => { if (submitting.current) event.preventDefault(); }}>
            <Icon name="back" /><span>{canSave && !sharedState && !form.coeditRetry ? 'Отменить' : 'Вернуться к чтению'}</span>
          </a>
        )}
        {canSave && (
          <button className="space-action primary-action" type="submit" title="Сохранить и опубликовать редакцию" disabled={saving || Boolean(placement) || waitingForRename || mediaReloading || coverConflictPending ||
            Boolean(sharedState && !sharedReady) || Boolean(conflict && !rebase)}>
            {form.coeditRetry ? 'Повторить сохранение' : 'Сохранить'}
          </button>
        )}
      </div>
    </form>
    {coverOpen && <CoverGallery cover={cover.preview} uploadTo={form.coverUploadTo} onPick={chooseCover} onDismiss={closeCover} />}
    {attachmentDialog && <PageAttachmentDialog request={attachmentDialog} files={attachments.files} uploadTo={form.attachmentUploadTo}
      pendingCount={attachments.added.filter(id => !attachments.files.some(file => file.id === id)).length}
      onComplete={completeAttachment} onDismiss={dismissAttachment} />}
    {dialog?.kind === 'embed' && <PageEmbedDialog address={dialog.address} onApply={applyEmbed} onCancel={() => {
      if (dialog.opener?.isConnected) {
        setDialog(undefined);
        dialog.opener.focus({preventScroll:true});
      } else closeDialog({start:dialog.range.end,end:dialog.range.end});
    }} />}
    {dialog && dialog.kind !== 'embed' && <DraftDialogs request={dialog} pageLinkSearchTo={pageLinkSearchTo}
      onApply={applyDialog} onCancel={() => {
      const range = dialog.range;
      closeDialog(dialog.kind === 'paste' ? {start: range.end, end: range.end} : range);
    }} />}
  </>;
}
