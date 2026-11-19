import type {SourceRange} from './markdownModel';
import type {AttachmentDraft} from './pageAttachments';

export type DraftFrame = {
  source: string;
  contentFormat: 'literal' | 'markdown';
  selection: SourceRange;
  attachments?: AttachmentDraft;
  textBase?: string | null;
};
export type DraftEditKind = 'typing' | 'atomic';
export type DraftHistoryDirection = 'undo' | 'redo';

const maxEntries = 100;
const typingWindow = 750;

function sameDraft(a: DraftFrame, b: DraftFrame): boolean {
  return a.source === b.source && a.contentFormat === b.contentFormat &&
    a.attachments === b.attachments && a.textBase === b.textBase;
}

export function createDraftHistory<Frame extends DraftFrame = DraftFrame>() {
  const entries: {before: Frame; after: Frame}[] = [];
  let cursor = 0;
  let typedAt: number | undefined;

  function record(before: Frame, after: Frame, kind: DraftEditKind = 'atomic', now = Date.now()) {
    if (sameDraft(before, after)) return;
    const previous = entries[cursor - 1];
    const selection = before.selection;
    if (kind === 'typing' && typedAt !== undefined && now - typedAt <= typingWindow && previous &&
        sameDraft(previous.after, before) && before.attachments === after.attachments && before.textBase === after.textBase &&
        selection.start === selection.end && previous.after.selection.start === selection.start &&
        previous.after.selection.end === selection.end) {
      previous.after = after;
    } else {
      entries.splice(cursor, entries.length - cursor, {before, after});
      if (entries.length > maxEntries) entries.shift();
      cursor = entries.length;
    }
    typedAt = kind === 'typing' ? now : undefined;
  }

  function step(direction: DraftHistoryDirection): Frame | undefined {
    typedAt = undefined;
    if (direction === 'undo') return cursor > 0 ? entries[--cursor].before : undefined;
    return cursor < entries.length ? entries[cursor++].after : undefined;
  }

  return {record, step, separate: () => { typedAt = undefined; }};
}
