export type EditorCommand = 'heading' | 'bold' | 'italic' | 'strike' | 'list' | 'bullet' | 'ordered'
  | 'task' | 'quote' | 'code' | 'code-block' | 'rule' | 'table';
export type SourceRange = {start: number; end: number};
export const inlineFormats = ['bold', 'italic', 'strike', 'code'] as const;
export type SelectionFormatting = Record<typeof inlineFormats[number], boolean | 'mixed'>;

export function selectionFormatting(model: MarkdownDocument, range: SourceRange): SelectionFormatting {
  const caret = range.start === range.end;
  const runs = model.runs.filter(run => !run.locked && (caret
    ? run.offsets[0] <= range.start && range.start <= run.offsets.at(-1)!
    : run.offsets[0] < range.end && range.start < run.offsets.at(-1)! && run.text.length > 0));
  const states: SelectionFormatting = {bold:false, italic:false, strike:false, code:false};
  for (const format of inlineFormats) {
    const marked = runs.filter(run => run.marks.some(mark => mark.kind === format)).length;
    states[format] = marked > 0 && (marked === runs.length || 'mixed');
  }
  return states;
}

type MarkKind = 'bold' | 'italic' | 'strike' | 'code' | 'link';

export type InlineMark = {
  kind: MarkKind;
  openStart: number;
  openEnd: number;
  closeStart: number;
  closeEnd: number;
  href?: string;
};

export type TextRun = {
  text: string;
  // Each DOM UTF-16 boundary maps to the corresponding raw source boundary.
  offsets: number[];
  marks: InlineMark[];
  locked: boolean;
  attachment?: {id: string; alt: string; image: boolean};
};

export type TableCell = SourceRange & {runs: TextRun[]};
type TableAlignment = 'left' | 'center' | 'right' | null;

export type EditorBlock = {
  kind: 'paragraph' | 'heading' | 'bullet' | 'ordered' | 'quote' | 'fence' | 'rule' | 'table' | 'protected';
  start: number;
  end: number;
  bodyStart: number;
  level?: number;
  number?: number;
  indent?: number;
  markerStart?: number;
  task?: SourceRange & {checked: boolean};
  embed?: ExternalEmbedAddress;
  fence?: {marker: string; language: string; infoStart: number; infoEnd: number; bodyEnd: number; closingStart: number | null};
  table?: {rows: {cells: TableCell[]}[]; alignments: TableAlignment[]; separator: SourceRange};
  locked: boolean;
  runs: TextRun[];
};

export type ExternalEmbedAddress = {url: string; host: string; kind?: 'teamstream'};

const teamStreamOrigin = 'https://workspace.teamstream.mws.ru';
const teamStreamPath = /^\/embed\/[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}$/;

function isTeamStreamAddress(address: URL): boolean {
  if (address.origin !== teamStreamOrigin || !teamStreamPath.test(address.pathname)) return false;
  const suffix = address.href.slice(address.origin.length + address.pathname.length);
  const seconds = /^\?t=([0-9]+)$/.exec(suffix)?.[1];
  return suffix === '' || seconds !== undefined && Number(seconds) <= 0xffffffff;
}

export function externalEmbedAddress(value: string, ownHost = globalThis.location?.hostname): ExternalEmbedAddress | undefined {
  const input = value.replace(/^[ \t]+|[ \t]+$/g, '');
  const authority = /^https:\/\/([^/?#]+)/i.exec(input)?.[1];
  if (!authority || authority.includes('@') || /[\x00-\x20\x7f\\]/.test(input)) return;
  try {
    const address = new URL(input);
    const host = address.hostname.replace(/\.+$/, '');
    if (!host || address.protocol !== 'https:' || address.username || address.password ||
      host === ownHost?.toLowerCase().replace(/\.+$/, '')) return;
    const embed: ExternalEmbedAddress = {url: address.href, host: address.hostname};
    if (isTeamStreamAddress(address)) embed.kind = 'teamstream';
    return embed;
  } catch {
    return;
  }
}

export type MarkdownDocument = {
  blocks: EditorBlock[];
  runs: TextRun[];
};

const punctuation = /[!-/:-@\[-`{-~]/;
const emphasis = /[*_~]/;
const whitespace = /\s/u;
const word = /[\p{Alphabetic}\p{Number}]/u;
const ticks = '\u0060';
const linkPrefix = /(!?)\[((?:\\[^\n]|[^\\[\]\n])*)\]\(/y;
const angleText = /<[^<>\n]*>/y;
const attachmentDestination = /^attachment:([0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12})$/u;

export function escapeLiteralText(value: string): string {
  return value.replace(/[!-/:-@\[-`{-~]/g, '\\$&');
}

export function decodeLegacyText(value: string): string {
  return value.replace(/\\([!-/:-@\[-`{-~])/g, '$1');
}

export function normalizeEditorText(value: string): string {
  return value.replace(/\r\n?/g, '\n');
}

const internalPagePath = /^\/workspace\/[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}\/spaces\/[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}\/pages\/[a-f0-9]{8}(?:-[a-f0-9]{4}){3}-[a-f0-9]{12}$/u;

export function editorLinkDestination(value: string): string | null {
  return internalPagePath.test(value) ? value : httpDestination(value);
}

export function httpDestination(value: string): string | null {
  if (!/^https?:\/\/[^/?#\\]/i.test(value) || /[\u0000-\u0020\u007f\\]/.test(value)) return null;
  try {
    const url = new URL(value);
    return url.hostname ? url.href : null;
  } catch {
    return null;
  }
}

function runWidth(source: string, start: number, char: string): number {
  let end = start;
  while (source[end] === char) end++;
  return end - start;
}

function makeRun(source: string, start: number, end: number, marks: InlineMark[], locked = false, literal = false, inTable = false): TextRun {
  const offsets = [start];
  let text = '';
  const escaped = !marks.some(mark => mark.kind === 'code') && !locked && !literal;
  for (let cursor = start; cursor < end; cursor++) {
    const next = source[cursor + 1];
    if (source[cursor] === '\\' && cursor + 1 < end
      && (escaped && punctuation.test(next) || inTable && next === '|')) {
      cursor++;
    }
    text += source[cursor];
    offsets.push(cursor + 1);
  }
  return {text, offsets, marks: [...marks], locked};
}

type EmphasisOpener = {char: string; start: number; width: number};

function addEmphasis(marks: InlineMark[], marker: string, open: number, close: number, width: number) {
  marks.push({kind: marker === '~' ? 'strike' : width === 2 ? 'bold' : 'italic', openStart: open,
    openEnd: open + width, closeStart: close, closeEnd: close + width});
}

function codeEnd(source: string, from: number, end: number, width: number): number | undefined {
  let at = source.indexOf(ticks, from);
  while (at >= 0 && at < end) {
    const length = runWidth(source, at, ticks);
    if (length === width) return at;
    at = source.indexOf(ticks, at + length);
  }
}

function linkAt(source: string, start: number, end: number) {
  linkPrefix.lastIndex = start;
  const prefix = linkPrefix.exec(source);
  if (!prefix || linkPrefix.lastIndex > end) return;
  const destination = linkPrefix.lastIndex;
  const label = start + prefix[1].length + 1;
  let nested = 0;
  for (let cursor = destination; cursor < end; cursor++) {
    const char = source[cursor];
    if (char === '\\' && cursor + 1 < end && punctuation.test(source[cursor + 1])) cursor++;
    else if (char === '(') {
      if (nested === 32) return;
      nested++;
    }
    else if (char === ')' && nested === 0) return {
      image: prefix[1] === '!', label, labelEnd: destination - 2, end: cursor + 1,
      destination: source.slice(destination, cursor),
    };
    else if (char === ')') nested--;
  }
}

function inlineTokens(source: string, start: number, end: number, links = true) {
  const marks: InlineMark[] = [];
  const literal: (SourceRange & {locked: boolean; attachment?: TextRun['attachment']})[] = [];
  const openers: EmphasisOpener[] = [];
  for (let position = start; position < end;) {
    const char = source[position];
    if (char === '\\' && position + 1 < end && punctuation.test(source[position + 1])) {
      position += 2;
      continue;
    }
    if (char === ticks) {
      const width = runWidth(source, position, char);
      const close = codeEnd(source, position + width, end, width);
      if (close !== undefined) {
        const body = source.slice(position + width, close);
        const pad = body.startsWith(' ') && body.endsWith(' ') && /[^ ]/.test(body) ? 1 : 0;
        marks.push({kind: 'code', openStart: position, openEnd: position + width + pad,
          closeStart: close - pad, closeEnd: close + width});
        position = close + width;
        continue;
      }
      position += width;
      continue;
    }
    if (links && (char === '[' || (char === '!' && source[position + 1] === '['))) {
      const link = linkAt(source, position, end);
      if (link) {
        const hasLabel = link.label < link.labelEnd;
        const href = !link.image && hasLabel ? editorLinkDestination(decodeLegacyText(link.destination)) : null;
        if (href) {
          const caption = inlineTokens(source, link.label, link.labelEnd, false);
          marks.push({kind: 'link', openStart: position, openEnd: link.label,
            closeStart: link.labelEnd, closeEnd: link.end, href}, ...caption.marks);
          literal.push(...caption.literal);
        } else {
          const id = link.image || hasLabel ? attachmentDestination.exec(link.destination)?.[1] : undefined;
          const attachment = id ? {id, alt: decodeLegacyText(source.slice(link.label, link.labelEnd)), image: link.image} : undefined;
          literal.push({start: position, end: link.end, locked: true, attachment});
        }
        position = link.end;
        continue;
      }
    }
    if (char === '<') {
      angleText.lastIndex = position;
      const angle = angleText.exec(source);
      if (angle && angleText.lastIndex <= end) {
        literal.push({start: position, end: angleText.lastIndex, locked: true});
        position = angleText.lastIndex;
        continue;
      }
    }
    if (!emphasis.test(char)) {
      position++;
      continue;
    }
    const width = runWidth(source, position, char);
    if (char === '~' ? width !== 2 : width > 3) {
      position += width;
      continue;
    }
    const previous = position > start ? source[position - 1] : '';
    const next = position + width < end ? source[position + width] : '';
    const canClose = previous && !whitespace.test(previous) && !(char === '_' && word.test(next));
    const canOpen = next && !whitespace.test(next) && !(char === '_' && word.test(previous));
    let consumed = 0;
    if (canClose) {
      let index = openers.length - 1;
      while (index >= 0 && consumed < width && openers[index].char === char) {
        const opener = openers[index];
        const closing = width - consumed;
        if (opener.width === 2 && closing === 1) break;
        const matched = Math.min(opener.width, closing);
        openers.length = index + 1;
        const open = opener.start + opener.width - matched;
        const close = position + consumed;
        if (matched === 3) {
          addEmphasis(marks, char, open, close + 1, 2);
          addEmphasis(marks, char, open + 2, close, 1);
        } else {
          addEmphasis(marks, char, open, close, matched);
        }
        consumed += matched;
        opener.width -= matched;
        if (!opener.width) {
          openers.pop();
          index--;
        }
      }
    }
    if (canOpen && consumed < width && openers.length < 32) {
      openers.push({char, width: width - consumed, start: position + consumed});
    }
    position += width;
  }
  return {marks, literal};
}

function inlineRuns(source: string, start: number, end: number, legacy: boolean, inTable = false): TextRun[] {
  const textRun = (from: number, to: number, active: InlineMark[], locked = false, literal = false) =>
    makeRun(source, from, to, active, locked, literal, inTable);
  if (legacy || start === end) return [textRun(start, end, [])];
  const {marks, literal} = inlineTokens(source, start, end);
  if (!marks.length && !literal.length) return [textRun(start, end, [])];

  const openings = new Map(marks.map(mark => [mark.openStart, mark]));
  const closings = new Map(marks.map(mark => [mark.closeStart, mark]));
  const literalAt = new Map(literal.map(range => [range.start, range]));
  const edges = [...new Set([...openings.keys(), ...closings.keys(), ...literalAt.keys(), end])].sort((a, b) => a - b);
  const active: InlineMark[] = [];
  const runs: TextRun[] = [];
  let offset = start;
  for (const edge of edges) {
    if (edge < offset) continue;
    if (edge > offset) runs.push(textRun(offset, edge, active));
    const opening = openings.get(edge);
    const closing = closings.get(edge);
    const literalRange = literalAt.get(edge);
    if (opening) {
      if (edge === start) runs.push(textRun(edge, edge, []));
      active.push(opening);
      offset = opening.openEnd;
    } else if (closing) {
      const index = active.lastIndexOf(closing);
      if (index >= 0) active.splice(index, 1);
      offset = closing.closeEnd;
      if (offset === end) runs.push(textRun(offset, offset, active));
    } else if (literalRange) {
      if (edge === start) runs.push(textRun(edge, edge, active));
      const run = textRun(edge, literalRange.end, active, literalRange.locked, true);
      if (literalRange.attachment) run.attachment = literalRange.attachment;
      runs.push(run);
      offset = literalRange.end;
      if (offset === end) runs.push(textRun(offset, offset, active));
    }
  }
  return runs.length ? runs : [textRun(start, end, [])];
}

type SourceLine = {text: string; start: number; end: number};

function sourceLines(source: string): SourceLine[] {
  let offset = 0;
  return source.split('\n').map(text => {
    const start = offset;
    offset += text.length + 1;
    return {text, start, end: offset - 1};
  });
}

const fenceOpening = /^[ \t]{0,3}(`{3,}|~{3,})(.*)$/s;
const blockName = /^(?: {0,3}(#{1,3})[ \t]+|(>)[ \t]?)/;
const listPrefix = /^([ \t]*)(?:([-+*])|(\d{1,9})[.)])[ \t]+/;
const taskPrefix = /^\[([ xX])\](?:[ \t]+|$)/;

function indentColumns(indent: string): number {
  let columns = 0;
  for (const char of indent) columns += char === '\t' ? 4 - columns % 4 : 1;
  return columns;
}

function protectedBlock(source: string, start: number, end: number): EditorBlock {
  return {kind: 'protected', start, end, bodyStart: start, locked: true,
    runs: [makeRun(source, start, end, [], true)]};
}

function makeBlock(source: string, line: SourceLine, legacy: boolean, hasListParent: boolean): EditorBlock {
  const {start, end, text} = line;
  const list = legacy ? null : listPrefix.exec(text);
  if (list) {
    const indent = indentColumns(list[1]);
    if (indent && !hasListParent) return protectedBlock(source, start, end);
    const markerStart = start + list[1].length;
    const itemStart = start + list[0].length;
    const match = list[2] ? taskPrefix.exec(source.slice(itemStart, end)) : null;
    const task = match ? {checked: match[1] !== ' ', start: itemStart + 1, end: itemStart + 2} : undefined;
    const bodyStart = itemStart + (match?.[0].length ?? 0);
    return {kind: list[2] ? 'bullet' : 'ordered', start, end, bodyStart, markerStart, indent, task,
      number: list[3] ? Number(list[3]) : undefined, locked: false,
      runs: inlineRuns(source, bodyStart, end, false)};
  }
  const prefix = legacy ? null : blockName.exec(text);
  const bodyStart = start + (prefix?.[0].length ?? 0);
  return {kind: prefix?.[1] ? 'heading' : prefix?.[2] ? 'quote' : 'paragraph', start, end, bodyStart,
    level: prefix?.[1]?.length, locked: false, runs: inlineRuns(source, bodyStart, end, legacy)};
}

function trimHorizontal(text: string, start: number, end: number): SourceRange {
  while (start < end && (text[start] === ' ' || text[start] === '\t')) start++;
  while (end > start && (text[end - 1] === ' ' || text[end - 1] === '\t')) end--;
  return {start, end};
}

export function tableCells(line: SourceLine): (SourceRange & {rawStart: number; contentEnd: number})[] | undefined {
  const {text} = line;
  const bounds = trimHorizontal(text, 0, text.length);
  const separators: number[] = [];
  let backslashes = 0;
  for (let index = bounds.start; index < bounds.end; index++) {
    if (text[index] === '|' && backslashes % 2 === 0) separators.push(index);
    backslashes = text[index] === '\\' ? backslashes + 1 : 0;
  }
  if (!separators.length) return;
  const leading = separators[0] === bounds.start;
  const trailing = separators.at(-1) === bounds.end - 1;
  let start = bounds.start + Number(leading);
  const end = trailing ? bounds.end - 1 : text.length;
  if (start > end) return;
  return [...separators.slice(Number(leading), separators.length - Number(trailing)), end].map(separator => {
    const rawStart = line.start + start;
    const cell = trimHorizontal(text, start, separator);
    start = separator + 1;
    return {rawStart, start: line.start + cell.start, end: line.start + separator, contentEnd: line.start + cell.end};
  });
}

function tableAt(source: string, lines: SourceLine[], first: number): {block: EditorBlock; last: number} | undefined {
  const header = lines[first];
  const divider = lines[first + 1];
  if (!header.text.includes('|') || !divider) return;
  const dividers = tableCells(divider);
  if (!dividers?.length) return;
  const separatorText = dividers.map(range => source.slice(range.start, range.contentEnd));
  if (!separatorText.every(text => /^:?-{3,}:?$/.test(text))) return;
  const alignments: TableAlignment[] = separatorText.map(text => text.startsWith(':')
    ? text.endsWith(':') ? 'center' : 'left'
    : text.endsWith(':') ? 'right' : null);
  let last = first + 1;
  while (lines[last + 1]?.text.includes('|')) last++;
  const rows = [header, ...lines.slice(first + 2, last + 1)].map(tableCells);
  const end = lines[last].end;
  if (!rows.every(row => row?.length === alignments.length)) {
    return {block: protectedBlock(source, header.start, end), last};
  }
  const parsedRows = rows.map(row => ({cells: row!.map(({start, end, contentEnd}) => {
    const runs = inlineRuns(source, start, contentEnd, false, true);
    // The caret must also be able to sit after a space typed before the separator.
    if (contentEnd < end) runs.push(makeRun(source, contentEnd, end, []));
    return {start, end, runs};
  })}));
  return {last, block: {kind: 'table', start: header.start, end, bodyStart: parsedRows[0].cells[0].start, locked: false,
    runs: parsedRows.flatMap(row => row.cells.flatMap(cell => cell.runs)),
    table: {rows: parsedRows, alignments, separator: {start: divider.start, end: divider.end}}}};
}

function fenceEnd(lines: SourceLine[], index: number, opener: string): number | undefined {
  for (let next = index + 1; next < lines.length; next++) {
    const closer = fenceOpening.exec(lines[next].text);
    if (closer && closer[1][0] === opener[0] && closer[1].length >= opener.length && !closer[2].trim()) return next;
  }
}

export function parseMarkdown(source: string, legacy: boolean): MarkdownDocument {
  const lines = sourceLines(source);
  const blocks: EditorBlock[] = [];
  for (let index = 0; index < lines.length; index++) {
    const line = lines[index];
    const fence = legacy ? null : fenceOpening.exec(line.text);
    const opening = fence && (fence[1][0] !== ticks || !fence[2].includes(ticks)) ? fence : null;
    if (opening) {
      const closing = fenceEnd(lines, index, opening[1]);
      const last = closing ?? lines.length - 1;
      const end = lines[last].end;
      const bodyStart = Math.min(line.end + 1, end);
      const bodyEnd = closing !== undefined ? Math.max(bodyStart, lines[last].start - 1) : end;
      const marker = opening[1];
      const info = trimHorizontal(source, line.end - opening[2].length, line.end);
      const language = source.slice(info.start, info.end);
      const embed = closing !== undefined && language === 'sever-embed'
        ? externalEmbedAddress(source.slice(bodyStart, bodyEnd)) : undefined;
      const locked = closing === undefined || Boolean(embed);
      blocks.push({kind: 'fence', start: line.start, end, bodyStart, locked, embed,
        runs: embed ? [] : [makeRun(source, bodyStart, bodyEnd, [], locked, true)],
        fence: {marker, language, infoStart: info.start, infoEnd: info.end,
          bodyEnd, closingStart: closing === undefined ? null : lines[closing].start}});
      index = last;
      continue;
    }
    const table = legacy ? undefined : tableAt(source, lines, index);
    if (table) {
      blocks.push(table.block);
      index = table.last;
      continue;
    }
    if (!legacy && line.text.trim() === '---'
      && !lines[index - 1]?.text.trim() && !lines[index + 1]?.text.trim()) {
      blocks.push({...protectedBlock(source, line.start, line.end), kind: 'rule'});
      continue;
    }
    const previous = blocks.at(-1);
    blocks.push(makeBlock(source, line, legacy, previous?.indent !== undefined));
  }
  if (blocks.at(-1)?.embed) {
    // Keep an editable caret after an atomic embed that reaches EOF.
    const end = source.length;
    blocks.push(makeBlock(source, {text: '', start: end, end}, legacy, false));
  }
  return {blocks, runs: blocks.flatMap(block => block.runs)};
}

export function isProtected(document: MarkdownDocument, range: SourceRange): boolean {
  const intersects = (start: number, end: number) => range.start === range.end
    ? start <= range.start && range.start < end
    : start < range.end && range.start < end;
  const blockEdge = range.start !== range.end ? 1 : 0;
  const within = (start: number, end: number) => range.start >= start && range.end <= end;
  return document.blocks.some(block => {
    if (!intersects(Math.max(0, block.start - blockEdge), block.end + blockEdge)) return false;
    if (block.locked) return true;
    if (block.fence) return !within(block.bodyStart, block.fence.bodyEnd);
    return block.table && !block.table.rows.some(row => row.cells.some(cell => within(cell.start, cell.end)));
  })
    || document.runs.some(run => run.locked && intersects(run.offsets[0], run.offsets.at(-1)!));
}

type BlockKind = EditorBlock['kind'] | 'list';

export type BlockUnit = {
  kind: BlockKind;
  start: number;
  end: number;
  caret: number;
  safe: boolean;
};

// These constructs can span multiple visually separate paragraphs. Their
// boundaries belong to the Markdown source until the editor models them.
const rawContainer = /^\s*(?:<[/!?a-zA-Z]|\$\$|:::{1,}|\[.{1,999}\]:|={2,}\s*$|(?:-{2,}|\*{3,}|_{3,})\s*$)/;

export function blockUnits(model: MarkdownDocument, source: string, legacy: boolean): BlockUnit[] {
  const units: BlockUnit[] = [];
  let ambiguous = false;
  for (const block of model.blocks) {
    const raw = source.slice(block.start,block.end);
    if (!raw.trim()) continue;
    const previous = units.at(-1);
    const kind = block.kind === 'bullet' || block.kind === 'ordered' ? 'list' : block.kind;
    const adjacent = previous && previous.end + 1 === block.start;
    const indented = /^[ \t]/.test(raw);
    if (!legacy && kind === 'paragraph' && (rawContainer.test(raw) ||
        (previous?.kind !== 'list' && /^( {4}|\t)/.test(raw)))) ambiguous = true;
    // A blank line can precede an indented child. Some of those children are
    // opaque to the visual renderer, so protect their parent as a whole.
    const listChild = previous?.kind === 'list' &&
      ((kind === 'paragraph' && adjacent) || indented);
    const lazyQuote = previous?.kind === 'quote' && kind === 'paragraph' && adjacent;
    const sameGroup = adjacent && previous.kind === kind &&
      (kind === 'paragraph' || kind === 'quote' || kind === 'list');
    if (!legacy && previous && (listChild || lazyQuote || sameGroup)) {
      previous.end = block.end;
      if (kind === 'protected' || (kind === 'fence' && block.fence?.closingStart === null)) previous.safe = false;
      continue;
    }
    const safe = kind !== 'protected' && !(kind === 'fence' && block.fence?.closingStart === null);
    units.push({kind,start:block.start,end:block.end,caret:block.runs.find(run => !run.locked)?.offsets[0] ?? block.bodyStart,safe});
  }
  if (ambiguous) for (const unit of units) unit.safe = false;
  return units;
}
