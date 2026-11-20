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

type DraftSpan = SourceRange & {text:string};
type Entry<Frame extends DraftFrame> = {before:Frame;after:Frame;span?:DraftSpan;blocked?:boolean};

const maxEntries = 100;
const typingWindow = 750;

function sameDraft(a: DraftFrame, b: DraftFrame): boolean {
  return a.source === b.source && a.contentFormat === b.contentFormat &&
    a.attachments === b.attachments && a.textBase === b.textBase;
}

export function createDraftHistory<Frame extends DraftFrame = DraftFrame>() {
  const entries: Entry<Frame>[] = [];
  let cursor = 0;
  let typedAt: number | undefined;

  function record(before: Frame, after: Frame, kind: DraftEditKind = 'atomic', now = Date.now(), span?: DraftSpan) {
    if (sameDraft(before, after)) return;
    const previous = entries[cursor - 1];
    const selection = before.selection;
    if (kind === 'typing' && typedAt !== undefined && now - typedAt <= typingWindow && previous && !previous.blocked &&
        sameDraft(previous.after, before) && before.attachments === after.attachments && before.textBase === after.textBase &&
        selection.start === selection.end && previous.after.selection.start === selection.start &&
        previous.after.selection.end === selection.end) {
      previous.after = after;
      previous.span = undefined;
    } else {
      entries.splice(cursor, entries.length - cursor, {before, after, span});
      if (entries.length > maxEntries) entries.shift();
      cursor = entries.length;
    }
    typedAt = kind === 'typing' ? now : undefined;
  }

  function step(direction: DraftHistoryDirection, adapt?: (from: Frame, to: Frame, span?: DraftSpan, blocked?: boolean) => Frame | undefined): Frame | undefined {
    typedAt = undefined;
    const undo = direction === 'undo';
    const entry = entries[undo ? cursor - 1 : cursor];
    if (!entry) return;
    const from = undo ? entry.after : entry.before;
    const to = undo ? entry.before : entry.after;
    const authored=entry.span;
    const span=undo && authored ? {
      start:authored.start,end:authored.start+authored.text.length,
      text:entry.before.source.slice(authored.start,authored.end),
    } : authored;
    const frame = adapt ? adapt(from, to, span, entry.blocked) : entry.blocked ? undefined : to;
    if (frame) cursor += undo ? -1 : 1;
    return frame;
  }

  function walkBack(adapt: (entry: Entry<Frame>) => Entry<Frame> | undefined) {
    for (let index=cursor-1;index>=0;index--) {
      const current=entries[index];
      if (current.blocked) break;
      const mapped=adapt(current);
      if (!mapped) {
        current.blocked=true;
        break;
      }
      entries[index]=mapped;
    }
  }

  function walkForward(adapt: (entry: Entry<Frame>) => Entry<Frame> | undefined) {
    for (let index=cursor;index<entries.length;index++) {
      const current=entries[index];
      if (current.blocked) break;
      const mapped=adapt(current);
      if (!mapped) {
        current.blocked=true;
        break;
      }
      entries[index]=mapped;
    }
  }

  return {record, step, walkBack, walkForward, separate: () => { typedAt = undefined; }};
}
