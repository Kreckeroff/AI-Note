import type {SourceRange} from './markdownModel';

export type DraftFrame = {
  source: string;
  contentFormat: 'literal' | 'markdown';
  selection: SourceRange;
};
export type DraftEditKind = 'typing' | 'atomic';
export type DraftHistoryDirection = 'undo' | 'redo';

type Entry = {before: DraftFrame; after: DraftFrame};
const maxEntries = 100;
const typingWindow = 750;

export function createDraftHistory() {
  const entries: Entry[] = [];
  let cursor = 0;
  let typedAt: number | undefined;

  function record(before: DraftFrame, after: DraftFrame, kind: DraftEditKind = 'atomic', now = Date.now()) {
    if (before.source === after.source && before.contentFormat === after.contentFormat) return;
    const previous = entries[cursor - 1];
    const selection = before.selection;
    if (kind === 'typing' && typedAt !== undefined && now - typedAt <= typingWindow && previous &&
        previous.after.source === before.source && previous.after.contentFormat === before.contentFormat &&
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

  function step(direction: DraftHistoryDirection): DraftFrame | undefined {
    typedAt = undefined;
    if (direction === 'undo') return cursor > 0 ? entries[--cursor].before : undefined;
    return cursor < entries.length ? entries[cursor++].after : undefined;
  }

  return {record, step, separate: () => { typedAt = undefined; }};
}
