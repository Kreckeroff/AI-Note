import {parseMarkdown, type SourceRange} from '../editor/markdownModel';

export type DraftFormat = 'literal' | 'markdown';
export type DraftText = {source: string; format: DraftFormat};
export type TextSplice = SourceRange & {text: string};

export function sameDraftText(a: DraftText, b: DraftText): boolean {
  return a.source === b.source && a.format === b.format;
}

function insideSurrogatePair(source: string, offset: number): boolean {
  const high = source.charCodeAt(offset - 1);
  const low = source.charCodeAt(offset);
  return high >= 0xd800 && high <= 0xdbff && low >= 0xdc00 && low <= 0xdfff;
}

export function draftSplice(previous: string, next: string): TextSplice {
  let start = 0;
  let end = previous.length;
  let nextEnd = next.length;
  while (start < end && start < nextEnd && previous[start] === next[start]) start++;
  if (insideSurrogatePair(previous, start) || insideSurrogatePair(next, start)) start--;

  while (end > start && nextEnd > start && previous[end - 1] === next[nextEnd - 1]) {
    end--;
    nextEnd--;
  }
  if (insideSurrogatePair(previous, end) || insideSurrogatePair(next, nextEnd)) {
    end++;
    nextEnd++;
  }
  return {start, end, text: next.slice(start, nextEnd)};
}

export function scalarOffset(source: string, offset: number): number {
  return Array.from(source.slice(0, offset)).length;
}

export function sourceOffset(source: string, scalar: number): number {
  let offset = 0;
  for (const character of source) {
    if (scalar-- === 0) break;
    offset += character.length;
  }
  return offset;
}

export function applySplice(source: string, splice: TextSplice): string {
  return source.slice(0, splice.start) + splice.text + source.slice(splice.end);
}

export function rebaseSplice(local: TextSplice, remote: TextSplice): TextSplice | undefined {
  if (remote.start === remote.end && !remote.text) return local;
  if (local.start >= remote.end) {
    const offset = remote.text.length - (remote.end - remote.start);
    return {...local, start: local.start + offset, end: local.end + offset};
  }
  if (local.end <= remote.start) return local;
}

export type ScalarSplice = {start: number; end: number; text: string};

export function orderedSplices(source: string, steps: readonly ScalarSplice[]): {source: string; splices: TextSplice[]} | undefined {
  const splices: TextSplice[] = [];
  for (const step of steps) {
    const start = sourceOffset(source,step.start);
    const end = sourceOffset(source,step.end);
    if (scalarOffset(source,end)!==step.end) return;
    const splice={start,end,text:step.text};
    splices.push(splice);
    source=applySplice(source,splice);
  }
  return {source,splices};
}

export function applySplices(source: string, splices: readonly TextSplice[]): string {
  return splices.reduce(applySplice,source);
}

function shiftSplice(splice: TextSplice, prior: TextSplice): TextSplice {
  const shift=prior.text.length-prior.end+prior.start;
  return {...splice,start:splice.start+shift,end:splice.end+shift};
}

// Carry the server operations into the optimistic view and the local
// operation into the server view. Applying either sequence converges.
export function carrySplice(local: TextSplice, steps: readonly TextSplice[]) {
  const carried: TextSplice[]=[];
  for (const step of steps) {
    if (step.end<=local.start) {
      carried.push(step);
      local=shiftSplice(local,step);
    } else if (local.end<=step.start) {
      carried.push(shiftSplice(step,local));
    } else return;
  }
  return {local,carried};
}

export function spliceAtSelection(previous: string, next: string, before: SourceRange, after: SourceRange): TextSplice {
  const {start,end}=before;
  const insertedEnd=next.length-(previous.length-end);
  if (start<=insertedEnd && !insideSurrogatePair(previous,start) && !insideSurrogatePair(previous,end) &&
      next.slice(0,start)===previous.slice(0,start) && next.slice(insertedEnd)===previous.slice(end) &&
      after.start===insertedEnd && after.end===insertedEnd) {
    return {start,end,text:next.slice(start,insertedEnd)};
  }
  return draftSplice(previous,next);
}

export function rebaseText(base: DraftText, local: DraftText, remote: DraftText): DraftText | undefined {
  if (sameDraftText(base, local)) return remote;
  if (sameDraftText(base, remote)) return local;
  if (base.format === local.format && base.format === remote.format) {
    const rebased = rebaseSplice(draftSplice(base.source, local.source), draftSplice(base.source, remote.source));
    if (rebased) return {source: applySplice(remote.source, rebased), format: remote.format};
  }
}

export function selectionAfterSplice(selection: SourceRange, change: TextSplice): SourceRange {
  const map = (offset: number) => {
    if (offset < change.start) return offset;
    if (offset >= change.end) return offset + change.text.length - (change.end - change.start);
    return change.start + change.text.length;
  };
  return {start: map(selection.start), end: map(selection.end)};
}

type PrivateAttachment = {anchor: number; source: string};
type MediaProjection = {text: DraftText; privateAttachments: PrivateAttachment[]};

export function projectPrivateMedia(draft: DraftText, added: readonly string[]): MediaProjection {
  if (!added.length || draft.format !== 'markdown' || !draft.source.includes('attachment:')) {
    return {text: draft, privateAttachments: []};
  }
  const ids = new Set(added);
  const privateAttachments: PrivateAttachment[] = [];
  let source = '';
  let previousEnd = 0;
  for (const run of parseMarkdown(draft.source, false).runs) {
    if (!run.attachment || !ids.has(run.attachment.id)) continue;
    const start = run.offsets[0];
    const end = run.offsets.at(-1)!;
    source += draft.source.slice(previousEnd, start);
    privateAttachments.push({anchor: source.length, source: draft.source.slice(start, end)});
    previousEnd = end;
  }
  source += draft.source.slice(previousEnd);
  return {text: {source, format: draft.format}, privateAttachments};
}

export function restorePrivateMedia(projection: MediaProjection, confirmed: DraftText): DraftText {
  if (!projection.privateAttachments.length) return confirmed;
  const change = draftSplice(projection.text.source, confirmed.source);
  let source = confirmed.source;
  // Preserve original order when multiple private references share a position.
  for (const asset of [...projection.privateAttachments].reverse()) {
    const {start} = selectionAfterSplice({start: asset.anchor, end: asset.anchor}, change);
    source = applySplice(source, {start, end: start, text: asset.source});
  }
  return {source, format: 'markdown'};
}

export function projectPrivateMediaSplice(projection: MediaProjection, next: DraftText, splice: TextSplice): TextSplice | undefined {
  const project=(position:number) => {
    let removed=0;
    let preceding=0;
    for (const asset of projection.privateAttachments) {
      const start=asset.anchor+preceding;
      removed+=Math.max(0,Math.min(asset.source.length,position-start));
      preceding+=asset.source.length;
    }
    return position-removed;
  };
  const shared={...splice,start:project(splice.start),end:project(splice.end)};
  if (applySplice(projection.text.source,shared)===next.source) return shared;
}

export function liftPrivateMediaWithSteps(
  projection: MediaProjection, confirmed: DraftText, ids: readonly string[], steps?: readonly TextSplice[],
): {text:DraftText;steps?:TextSplice[]} | undefined {
  if (!steps) {
    if (!projection.privateAttachments.length || sameDraftText(projection.text,confirmed)) {
      return {text:restorePrivateMedia(projection,confirmed)};
    }
    return;
  }
  const matches=applySplices(projection.text.source,steps)===confirmed.source;
  if (!projection.privateAttachments.length) return {text:confirmed,steps:matches ? [...steps] : undefined};
  if (!matches) return;
  if (confirmed.format!=='markdown') return;
  const assets=projection.privateAttachments.map(asset=>({...asset}));
  const lifted:TextSplice[]=[];
  for (const step of steps) {
    if (assets.some(asset=>step.start<asset.anchor && asset.anchor<step.end)) return;
    const replacing=step.start<step.end;
    const inlineSize=(point:number, includeBoundary=false)=>assets.reduce((length,asset)=>
      length+(asset.anchor<point || includeBoundary && asset.anchor===point ? asset.source.length : 0),0);
    lifted.push({start:step.start+inlineSize(step.start,replacing),end:step.end+inlineSize(step.end),text:step.text});
    for (const asset of assets) {
      // A replacement starting at the same boundary stays after the private
      // reference. An insertion in that gap stays before it.
      if (!replacing || asset.anchor!==step.start) {
        asset.anchor=selectionAfterSplice({start:asset.anchor,end:asset.anchor},step).start;
      }
    }
  }
  let source=confirmed.source;
  for (const asset of assets.reverse()) {
    source=applySplice(source,{start:asset.anchor,end:asset.anchor,text:asset.source});
  }
  const restored={source,format:confirmed.format};
  if (sameDraftText(projectPrivateMedia(restored,ids).text,confirmed)) return {text:restored,steps:lifted};
}
