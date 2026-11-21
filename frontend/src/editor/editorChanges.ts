import {
  blockUnits, decodeLegacyText, escapeLiteralText, externalEmbedAddress, editorLinkDestination, isProtected, normalizeEditorText, parseMarkdown,
  type EditorBlock, type EditorCommand, type InlineMark, type MarkdownDocument, type SourceRange, type TextRun,
} from './markdownModel';

export type EditorChange = SourceRange & {source: string; legacy: boolean};
type InsertKind = 'text' | 'typed' | 'paragraph' | 'line';
type EmphasisKind = 'bold' | 'italic' | 'strike';
type ListCommand = 'list' | 'bullet' | 'ordered' | 'task';
type BlockCommand = 'heading' | 'quote' | ListCommand;
type SpecialBlockCommand = 'code-block' | 'rule' | 'table';

const emphasisMarkers = {bold: '**', italic: '*', strike: '~~'};
const emphasisBits = {bold: 1, italic: 2, strike: 8};
const prefixes = {heading: '## ', list: '- ', bullet: '- ', ordered: '1. ', task: '- [ ] ', quote: '> '};

function change(source: string, legacy: boolean, range: SourceRange, insertion: string): EditorChange {
  const position = range.start + insertion.length;
  return {source: source.slice(0, range.start) + insertion + source.slice(range.end),
    legacy, start: position, end: position};
}

function unchanged(source: string, legacy: boolean, range: SourceRange): EditorChange {
  return {source, legacy, start: range.start, end: range.end};
}

function markBody(text: string, delimiter: string): {before: string; text: string; after: string} {
  const match = /^(\s*)(.*?)(\s*)$/s.exec(text)!;
  const [, left, body, right] = match;
  return body ? {before: left + delimiter, text: body, after: delimiter + right}
    : {before: '', text, after: ''};
}

function codeBody(text: string): {before: string; text: string; after: string} {
  const widths = Array.from(text.matchAll(/`+/g), part => part[0].length);
  const delimiter = '`'.repeat(Math.max(0, ...widths) + 1);
  const padded = text.startsWith('`') || text.endsWith('`')
    || (text.startsWith(' ') && text.endsWith(' ') && /[^ ]/.test(text));
  const boundary = padded ? delimiter + ' ' : delimiter;
  return {before: boundary, text, after: padded ? ' ' + delimiter : delimiter};
}

export function applySourceCommand(source: string, start: number, end: number, command: EditorCommand) {
  const selection = source.slice(start, end);
  if (command === 'code' && selection.includes('\n')) command = 'code-block';
  if (command === 'code-block' || command === 'rule' || command === 'table') {
    const edit = insertBlock(source, {start, end}, blockTemplate(command, selection));
    return {source: edit.source, start: edit.start, end: edit.end};
  }
  if (command === 'bullet' || command === 'ordered' || command === 'task') {
    const edit = applyBlockCommand(source, parseMarkdown(source, false), {start, end}, command);
    return {source: edit.source, start: edit.start, end: edit.end};
  }
  const wrap = command === 'code' ? codeBody(selection)
    : command === 'heading' || command === 'list' || command === 'quote'
      ? {before: prefixes[command], text: selection, after: ''}
      : {before: emphasisMarkers[command], text: selection, after: emphasisMarkers[command]};
  return {
    source: source.slice(0, start) + wrap.before + wrap.text + wrap.after + source.slice(end),
    start: start + wrap.before.length,
    end: start + wrap.before.length + wrap.text.length,
  };
}

function blockAt(document: MarkdownDocument, offset: number): EditorBlock | undefined {
  return document.blocks.find(block => block.start <= offset && offset <= block.end);
}

function runAt(document: MarkdownDocument, offset: number): TextRun | undefined {
  return blockAt(document, offset)?.runs.find(run => run.offsets[0] <= offset && offset <= run.offsets.at(-1)!);
}

function listItem(block: EditorBlock | undefined): block is EditorBlock & {kind: 'bullet' | 'ordered'} {
  return block?.kind === 'bullet' || block?.kind === 'ordered';
}

function structuralBlock(block: EditorBlock | undefined): boolean {
  return block?.kind === 'table' || block?.kind === 'fence' || block?.kind === 'rule';
}

function intersectedStructure(document: MarkdownDocument, range: SourceRange): EditorBlock | undefined {
  return document.blocks.find(block => structuralBlock(block) && block.start <= range.end && range.start <= block.end);
}

function tableCellAt(block: EditorBlock | undefined, range: SourceRange) {
  return block?.table?.rows.flatMap(row => row.cells).find(cell => cell.start <= range.start && range.end <= cell.end);
}

function closedFenceAt(block: EditorBlock, range: SourceRange) {
  const fence = block.fence;
  return fence && fence.closingStart !== null && block.bodyStart <= range.start && range.end <= fence.bodyEnd
    ? {...fence, closingStart: fence.closingStart} : undefined;
}

function fenceCollision(body: string, marker: string): number {
  let width = 0;
  for (const line of body.split('\n')) {
    const closer = /^[ \t]{0,3}(\x60{3,}|~{3,})(.*)$/s.exec(line);
    if (closer && closer[1].startsWith(marker) && !closer[2].trim()) width = Math.max(width, closer[1].length);
  }
  return width;
}

function editFence(source: string, range: SourceRange, input: string, block: EditorBlock): EditorChange {
  const fence = closedFenceAt(block, range);
  if (!fence) return unchanged(source, false, range);
  const body = source.slice(block.bodyStart, range.start) + input + source.slice(range.end, fence.bodyEnd);
  const collision = fenceCollision(body, fence.marker[0]);
  const frame = block.bodyStart === fence.closingStart && input ? '\n' : '';
  if (collision < fence.marker.length) {
    const edit = change(source, false, range, input + frame);
    return {...edit, start: edit.start - frame.length, end: edit.end - frame.length};
  }
  const opening = source.indexOf(fence.marker, block.start);
  const closing = source.indexOf(fence.marker, fence.closingStart);
  const closer = /^(\x60+|~+)/.exec(source.slice(closing))![0];
  const replacement = fence.marker[0].repeat(collision + 1);
  const cursor = range.start + input.length + replacement.length - fence.marker.length;
  const edited = applyPatches(source, range, [
    {start: opening, end: opening + fence.marker.length, value: replacement},
    { ...range, value: input + frame },
    {start: closing, end: closing + closer.length, value: replacement},
  ]);
  return {...edited, start: cursor, end: cursor};
}

function promotesSyntax(source: string): Array<number> {
  const parsed = parseMarkdown(source, false);
  const escapes = new Set<number>();
  for (const block of parsed.blocks) {
    if (block.kind === 'table' || block.kind === 'rule' || block.kind === 'fence' || block.kind === 'protected') {
      for (const match of source.slice(block.start, block.end).matchAll(/\\([!-/:-@\[-\x60{-~])|[!-/:-@\[-\x60{-~]/g)) {
        if (!match[1]) escapes.add(block.start + match.index);
      }
      continue;
    }
    if (block.kind !== 'paragraph') {
      const marker = source.slice(block.start, block.bodyStart).search(/[!-/:-@\[-`{-~]/);
      if (marker >= 0) escapes.add(block.start + marker);
    }
    for (const run of block.runs) {
      for (const mark of run.marks) {
        if (mark.kind === 'link') {
          // Quoting the brackets keeps the original label and destination literal.
          escapes.add(mark.openStart);
          escapes.add(mark.closeStart);
          continue;
        }
        for (let index = mark.openStart; index < mark.openEnd; index++) {
          if (source[index] !== ' ') escapes.add(index);
        }
        for (let index = mark.closeStart; index < mark.closeEnd; index++) {
          if (source[index] !== ' ') escapes.add(index);
        }
      }
      if (run.locked && !block.locked && decodeLegacyText(run.text) !== run.text) {
        // Literal spans already preserve unescaped old text. Leave their bytes
        // alone unless they contain an escape that MD-01 rendered differently.
        const start = run.offsets[0];
        for (let index = 0; index < run.text.length; index++) {
          const character = run.text[index];
          if (character === '\\' && escapeLiteralText(run.text[index + 1] ?? '').length === 2) {
            index++;
          } else if (escapeLiteralText(character).length === 2) {
            escapes.add(start + index);
          }
        }
      }
    }
  }
  return [...escapes].sort((a, b) => a - b);
}

function promote(source: string, legacy: boolean, range: SourceRange) {
  if (!legacy) return {source, range};
  // MD-01 escaped punctuation on write. Imports can still contain literal,
  // unescaped syntax, so quote those constructs before enabling markup.
  const offsets = promotesSyntax(source);
  let last = 0;
  let converted = '';
  for (const offset of offsets) {
    converted += source.slice(last, offset) + '\\';
    last = offset;
  }
  converted += source.slice(last);
  const move = (position: number) => position + offsets.filter(offset => offset < position).length;
  return {source: converted, range: {start: move(range.start), end: move(range.end)}};
}

function replaceCode(source: string, legacy: boolean, range: SourceRange, insertion: string, mark: InlineMark): EditorChange {
  const before = source.slice(mark.openEnd, range.start);
  const after = source.slice(range.end, mark.closeStart);
  const body = before + insertion + after;
  if (!body) return change(source, legacy, {start: mark.openStart, end: mark.closeEnd}, '');
  const opening = source.slice(mark.openStart, mark.openEnd);
  const closing = source.slice(mark.closeStart, mark.closeEnd);
  const candidate = 'x ' + opening + body + closing;
  const parsed = parseMarkdown(candidate, false).runs.find(run => run.marks.some(item => item.kind === 'code'));
  if (parsed?.text === body) return change(source, legacy, range, insertion);
  const wrap = codeBody(body);
  const edit = change(source, legacy, {start: mark.openStart, end: mark.closeEnd}, wrap.before + body + wrap.after);
  const position = mark.openStart + wrap.before.length + before.length + insertion.length;
  return {...edit, start: position, end: position};
}

function pendingTicks(text: string) {
  let pending = 0;
  for (const token of text.matchAll(/(?<![\\`])`+/g)) {
    if (pending === token[0].length) pending = 0;
    else if (!pending) pending = token[0].length;
  }
  return pending;
}

function typedFormat(source: string, legacy: boolean, document: MarkdownDocument, range: SourceRange, insertion: string): EditorChange | undefined {
  if (range.start !== range.end || (insertion !== '*' && insertion !== '`')) return;
  const run = runAt(document, range.start);
  if (!run || run.marks.length || run.locked) return;
  const cursor = run.offsets.indexOf(range.start);
  if (cursor < 0) return;
  const before = run.text.slice(0, cursor);
  const text = before + insertion;
  let start: number;
  let replacement: string;
  if (insertion === '`') {
    const match = /(?<![\\`])(`+)([^`\n][\s\S]*?)\1$/.exec(text);
    if (!match || !match[2].trim() || pendingTicks(text.slice(0, match.index))) return;
    const body = match[2];
    start = match.index;
    const wrap = codeBody(body);
    replacement = wrap.before + body + wrap.after;
  } else {
    if (pendingTicks(before)) return;
    const match = /(?<![\\*])(\*{1,3})([^*\n]+)\1$/.exec(text);
    if (!match || !match[2].trim() || match[2] !== match[2].trim()) return;
    start = match.index;
    replacement = match[1] + escapeLiteralText(match[2]) + match[1];
  }
  const promoted = promote(source, legacy, {start: run.offsets[start], end: range.end});
  return change(promoted.source, false, promoted.range, replacement);
}

function nextListPrefix(source: string, document: MarkdownDocument, block: EditorBlock): string {
  if (!block.task && block.kind !== 'ordered') return source.slice(block.start, block.bodyStart);
  const markerStart = block.markerStart ?? block.start;
  const indent = source.slice(block.start, markerStart);
  if (block.task) return indent + source.slice(markerStart, block.task.start) + ' '
    + source.slice(block.task.end, block.bodyStart);
  const depth = block.indent ?? 0;
  let first = block.number ?? 1;
  let count = 1;
  for (let index = document.blocks.indexOf(block) - 1; index >= 0; index--) {
    const previous = document.blocks[index];
    if (!listItem(previous) || (previous.indent ?? 0) < depth) break;
    if ((previous.indent ?? 0) > depth) continue;
    if (previous.kind !== 'ordered') break;
    first = previous.number ?? 1;
    count++;
  }
  const marker = /^\d{1,9}[.)]/.exec(source.slice(markerStart, block.bodyStart))![0];
  return indent + Math.min(first + count, 999999999) + marker.at(-1) + source.slice(markerStart + marker.length, block.bodyStart);
}

function paragraphBreak(source: string, legacy: boolean, range: SourceRange, document: MarkdownDocument): EditorChange {
  const block = blockAt(document, range.start);
  if (!listItem(block)) {
    if (block?.kind === 'quote' && range.start === block.bodyStart && range.end === block.end && block.bodyStart === block.end) {
      return change(source, legacy, {start:block.start, end:block.end}, '');
    }
    const ending = block?.kind === 'heading' || block?.kind === 'quote' ? '\n\n' : '\n';
    return separateInline(source, legacy, document, range, ending, 'right');
  }
  const index = document.blocks.indexOf(block);
  const last = document.blocks[subtreeEnd(document.blocks, index)];
  if (last !== block && range.end > block.end) return unchanged(source, legacy, range);
  const nextItem = document.blocks[index + 1];
  const continuesList = listItem(nextItem)
    && (nextItem.kind === block.kind || (block.indent ?? 0) > 0 || (nextItem.indent ?? 0) > 0);
  if (!continuesList && !source.slice(block.bodyStart, block.end).trim()) {
    const previous = source[block.start - 1] === '\n' && source[block.start - 2] !== '\n';
    const next = source[block.end] === '\n';
    const gap = previous || next ? '\n' : '';
    return change(source, false, {start: block.start, end: block.end}, gap);
  }
  const prefix = nextListPrefix(source, document, block);
  const edit = separateInline(source, legacy, document, range, '\n' + prefix, 'right');
  if (last === block || edit.source === source) return edit;
  // Move only the new sibling. Existing descendants stay under the first half.
  const split = edit.source.indexOf('\n', block.start);
  const siblingEnd = block.end + edit.source.length - source.length;
  const descendants = source.slice(block.end, last.end);
  const draft = edit.source.slice(0, split) + descendants + edit.source.slice(split, siblingEnd) + source.slice(last.end);
  const cursor = edit.start + descendants.length;
  return {...edit, source: draft, start: cursor, end: cursor};
}

function allMarks(document: MarkdownDocument): InlineMark[] {
  return [...new Set(document.runs.flatMap(run => run.marks))];
}

function expandRemovedMarks(document: MarkdownDocument, original: SourceRange, empty: boolean): SourceRange {
  let {start, end} = original;
  for (const mark of allMarks(document).sort((a, b) => b.openEnd - a.openEnd)) {
    if (start <= mark.openEnd && end >= mark.closeStart && (empty || start < mark.openEnd || end > mark.closeStart)) {
      start = Math.min(start, mark.openStart);
      end = Math.max(end, mark.closeEnd);
    }
  }
  return {start, end};
}

function visibleRunText(run: TextRun, range: SourceRange): string {
  const first = run.offsets.findIndex(offset => offset >= range.start);
  if (first < 0) return '';
  return run.text.slice(first, lastBefore(run.offsets, range.end));
}

type EmphasisChange = {range: SourceRange; kind: EmphasisKind; apply: boolean};

function styleSignature(document: MarkdownDocument, range: SourceRange, emphasis?: EmphasisChange, ignoreLinks = false): string {
  const signatures: string[] = [];
  for (const run of document.runs) {
    if (run.offsets[0] >= range.end || run.offsets.at(-1)! <= range.start) continue;
    const mask = new Set(run.marks.map(mark => mark.kind));
    const style = Number(mask.has('bold')) + 2 * Number(mask.has('italic')) + 4 * Number(mask.has('code'))
      + 8 * Number(mask.has('strike')) + 16 * Number(!ignoreLinks && mask.has('link'));
    let index = run.offsets.findIndex(offset => offset >= range.start);
    for (const char of visibleRunText(run, range)) {
      const position = run.offsets[index];
      let result = style;
      if (emphasis && !mask.has('code') && emphasis.range.start <= position && position < emphasis.range.end) {
        const flag = emphasisBits[emphasis.kind];
        result = emphasis.apply ? style | flag : style & ~flag;
      }
      if (/\S/u.test(char)) signatures.push(char + result);
      index += char.length;
    }
  }
  return signatures.join('');
}

function separateInline(
  source: string, legacy: boolean, document: MarkdownDocument, selected: SourceRange,
  insertion: string, cursor: 'right' | 'middle', codeText?: string,
): EditorChange {
  const range = selected.start === selected.end ? {...selected} : expandRemovedMarks(document, selected, true);
  const closing: InlineMark[] = [];
  const opening: InlineMark[] = [];
  for (const mark of allMarks(document)) {
    if (range.start <= mark.openStart && mark.closeEnd <= range.end) continue;
    const left = mark.openEnd <= selected.start && selected.start <= mark.closeStart;
    const right = mark.openEnd <= selected.end && selected.end <= mark.closeStart;
    const runs = document.runs.filter(run => run.text && run.marks.includes(mark));
    if (!runs.length) continue;
    if (left) {
      if (selected.start > runs[0].offsets[0]) closing.push(mark);
      else range.start = Math.min(range.start, mark.openStart);
    }
    if (right) {
      if (selected.end < runs.at(-1)!.offsets.at(-1)!) opening.push(mark);
      else range.end = Math.max(range.end, mark.closeEnd);
    }
  }

  closing.sort((a, b) => a.closeStart - b.closeStart);
  opening.sort((a, b) => a.openStart - b.openStart);
  const trimLeft = closing.length && !closing.some(mark => mark.kind === 'code');
  const trimRight = opening.length && !opening.some(mark => mark.kind === 'code');
  let leftSpace = '';
  let rightSpace = '';
  if (trimLeft) {
    leftSpace = /[ \t]*$/.exec(source.slice(Math.max(...closing.map(mark => mark.openEnd)), range.start))![0];
    range.start -= leftSpace.length;
  }
  if (trimRight) {
    rightSpace = /^[ \t]*/.exec(source.slice(range.end, Math.min(...opening.map(mark => mark.closeStart))))![0];
    range.end += rightSpace.length;
  }
  const before = closing.map(mark => source.slice(mark.closeStart, mark.closeEnd)).join('') + leftSpace;
  const after = rightSpace + opening.map(mark => source.slice(mark.openStart, mark.openEnd)).join('');
  const edit = change(source, legacy, range, before + insertion + after);
  if (closing.length || opening.length || codeText !== undefined) {
    const result = parseMarkdown(edit.source, legacy);
    const left = range.start + before.length;
    const right = left + insertion.length;
    if (styleSignature(document, {start: 0, end: selected.start}) !== styleSignature(result, {start: 0, end: left})
      || styleSignature(document, {start: selected.end, end: source.length}) !== styleSignature(result, {start: right, end: edit.source.length})) {
      return unchanged(source, legacy, selected);
    }
    if (codeText !== undefined) {
      const content = result.runs.filter(run => run.offsets[0] < right && left < run.offsets.at(-1)!);
      if (content.some(run => !run.marks.some(mark => mark.kind === 'code'))
          || content.map(run => visibleRunText(run, {start:left,end:right})).join('') !== codeText) {
        return unchanged(source, legacy, selected);
      }
    }
  }
  const position = cursor === 'middle' ? edit.start - after.length : edit.start;
  return {...edit, start: position, end: position};
}

function replaceAcrossMarks(source: string, legacy: boolean, document: MarkdownDocument, selected: SourceRange, input: string) {
  const range = expandRemovedMarks(document, selected, !input);
  const marks = allMarks(document);
  const closing = marks.filter(mark => mark.openEnd < range.start && range.start <= mark.closeStart && range.end > mark.closeStart)
    .sort((a, b) => a.closeStart - b.closeStart);
  const opening = marks.filter(mark => range.start < mark.openEnd && mark.openEnd < range.end && range.end < mark.closeStart)
    .sort((a, b) => a.openStart - b.openStart);
  if ((input.includes('\n') && closing.length) || [...closing, ...opening].some(mark => mark.kind === 'code')) {
    return unchanged(source, legacy, selected);
  }
  const tail = closing.map(mark => source.slice(mark.closeStart, mark.closeEnd)).join('')
    + opening.map(mark => source.slice(mark.openStart, mark.openEnd)).join('');
  const text = escapeLiteralText(input);
  const result = change(source, legacy, range, text + tail);
  const position = range.start + text.length;
  return {...result, start: position, end: position};
}

export function insertVisual(
  source: string, legacy: boolean, range: SourceRange, value: string, kind: InsertKind = 'text',
): EditorChange {
  const document = parseMarkdown(source, legacy);
  if (isProtected(document, range)) return unchanged(source, legacy, range);
  const input = normalizeEditorText(value);
  const structural = intersectedStructure(document, range);
  if (structural?.embed && range.start === source.length && range.start === range.end) {
    const text = kind === 'paragraph' || kind === 'line' ? '' : escapeLiteralText(input);
    return change(source, legacy, range, '\n\n' + text);
  }
  if (structural?.kind === 'fence') {
    return editFence(source, range, kind === 'paragraph' || kind === 'line' ? '\n' : input, structural);
  }
  if (structural && (!tableCellAt(structural, range) || input.includes('\n') || kind === 'paragraph' || kind === 'line')) {
    return unchanged(source, legacy, range);
  }
  if (kind === 'paragraph') return paragraphBreak(source, legacy, range, document);
  if (kind === 'line') return separateInline(source, legacy, document, range, '\n', 'right');
  if (kind === 'typed') {
    const formatted = typedFormat(source, legacy, document, range, input);
    if (formatted) return formatted;
  }
  const marks = runAt(document, range.start)?.marks ?? [];
  const code = marks.find(mark => mark.kind === 'code' && range.end <= mark.closeStart);
  if (code) {
    if (input.includes('\n')) return unchanged(source, legacy, range);
    return replaceCode(source, legacy, range, structural?.kind === 'table' ? input.replaceAll('|','\\|') : input, code);
  }
  if (marks.length && input.includes('\n')) return unchanged(source, legacy, range);
  const changesMarkBoundaries = range.start !== range.end && allMarks(document).some(mark =>
    range.start < mark.openEnd && mark.openEnd < range.end || range.start <= mark.closeStart && mark.closeStart < range.end);
  if (changesMarkBoundaries || !input && range.end > range.start) return replaceAcrossMarks(source, legacy, document, range, input);
  if (marks.length && input && /^\s|\s$/u.test(input)) {
    const blockRuns = blockAt(document, range.start)?.runs ?? [];
    const bound = (mark: InlineMark) => blockRuns.filter(run => run.text && run.marks.includes(mark));
    const openingStack = marks.filter(mark => bound(mark)[0]?.offsets[0] === range.start);
    const closingStack = marks.filter(mark => bound(mark).at(-1)?.offsets.at(-1) === range.end);
    const front = openingStack.length ? input.match(/^\s*/u)![0] : '';
    const back = closingStack.length ? input.slice(front.length).match(/\s*$/u)![0] : '';
    if (front || back) {
      const movable = input.slice(front.length, input.length - back.length);
      const left = front ? Math.min(...openingStack.map(mark => mark.openStart)) : range.start;
      const right = back ? Math.max(...closingStack.map(mark => mark.closeEnd)) : range.end;
      if (!movable && openingStack.some(mark => closingStack.includes(mark))) {
        return replaceAcrossMarks(source, legacy, document, expandRemovedMarks(document, range, true), front + back);
      }
      const prefix = front + source.slice(left, range.start);
      const suffix = source.slice(range.end, right) + back;
      return change(source, legacy, {start:left,end:right}, prefix + escapeLiteralText(movable) + suffix);
    }
  }
  return change(source, legacy, range, escapeLiteralText(input));
}

const graphemes = new Intl.Segmenter(undefined, {granularity: 'grapheme'});

function lastBefore(offsets: number[], limit: number) {
  let index = offsets.length - 1;
  while (index >= 0 && offsets[index] > limit) index--;
  return index;
}

function adjacentRange(document: MarkdownDocument, source: string, offset: number, direction: 'backward' | 'forward'): SourceRange {
  const block = blockAt(document, offset);
  if (!block) return {start: offset, end: offset};
  const structure = structuralBlock(block);
  if (!structure && direction === 'backward' && offset === block.bodyStart && block.bodyStart > block.start) {
    return {start: block.start, end: block.bodyStart};
  }
  const content = block.kind === 'table' ? tableCellAt(block, {start:offset,end:offset})?.runs ?? [] : block.runs;
  const runs = direction === 'backward' ? [...content].reverse() : content;
  const run = runs.find(candidate => direction === 'backward'
    ? candidate.offsets[0] < offset : candidate.offsets.at(-1)! > offset);
  if (run) {
    const index = direction === 'backward'
      ? lastBefore(run.offsets, offset - 1) : run.offsets.findIndex(raw => raw > offset);
    const boundary = direction === 'backward' ? Math.min(index + 1, run.text.length) : Math.max(0, index - 1);
    const segments = graphemes.segment(run.text);
    const part = segments.containing(direction === 'backward' ? boundary - 1 : boundary);
    if (part) return {start: run.offsets[part.index], end: run.offsets[part.index + part.segment.length]};
  }
  if (structure) return {start:offset,end:offset};
  if (direction === 'backward' && source[block.start - 1] === '\n') {
    if (structuralBlock(blockAt(document, block.start - 1))) return {start:offset,end:offset};
    return {start: block.start - 1, end: block.start};
  }
  if (direction === 'forward' && source[block.end] === '\n') {
    const following = blockAt(document, block.end + 1);
    if (following?.locked || structuralBlock(following)) return {start: offset, end: offset};
    return {start: block.end, end: following?.bodyStart ?? block.end + 1};
  }
  return {start: offset, end: offset};
}

export function removeVisual(source: string, legacy: boolean, range: SourceRange, direction: 'backward' | 'forward'): EditorChange {
  const document = parseMarkdown(source, legacy);
  const remove = range.start === range.end ? adjacentRange(document, source, range.start, direction) : range;
  if (remove.start === remove.end || isProtected(document, remove)) return unchanged(source, legacy, range);
  const edit = insertVisual(source, legacy, remove, '');
  return edit.source === source ? unchanged(source, legacy, range) : edit;
}

function applyBlockCommand(source: string, document: MarkdownDocument, range: SourceRange, command: BlockCommand) {
  const collapsed = range.start === range.end;
  const touched = document.blocks.filter(block => block.start <= range.end && block.end >= range.start);
  const blocks = command === 'heading' ? touched.slice(0, 1) : touched.filter(block => block.start < range.end || collapsed);
  if (blocks.some(block => block.locked || structuralBlock(block))) return unchanged(source, false, range);
  const targets = blocks.filter(block => {
    if (block.kind === 'quote') return false;
    if (block.kind === 'ordered' && (command === 'list' || command === 'heading' || command === 'quote')) return false;
    if (command === 'heading') return true;
    if (block.kind === 'heading') return false;
    return collapsed || listItem(block) || Boolean(source.slice(block.bodyStart, block.end).trim());
  });
  if (!targets.length) return unchanged(source, false, range);
  if (command === 'ordered' && targets.every(block => block.kind === 'ordered')) return unchanged(source, false, range);
  const removeList = command === 'list' && targets.every(block => block.kind === 'bullet' && !block.task);
  if (removeList) {
    const removed = new Set(targets);
    const strandsNestedItem = (block: EditorBlock) => {
      const next = document.blocks[document.blocks.indexOf(block) + 1];
      return listItem(next) && (next.indent ?? 0) > 0 && !removed.has(next);
    };
    if (targets.some(strandsNestedItem)) return unchanged(source, false, range);
  }
  const patches: SourcePatch[] = [];
  for (const [index, block] of targets.entries()) {
    if (command === 'task' && block.task) continue;
    if (command === 'bullet' && block.kind === 'bullet' && !block.task) continue;
    const prefix = removeList ? '' : command === 'ordered' ? (index + 1) + '. ' : prefixes[command];
    const markerOnly = listItem(block) && !removeList && command !== 'heading' && command !== 'quote';
    patches.push({start: markerOnly ? block.markerStart ?? block.start : block.start, end: block.bodyStart, value: prefix,
      inside: {start:prefix.length,end:prefix.length}});
  }
  if (command !== 'heading' && command !== 'quote' && !removeList) {
    for (let index = 1; index < targets.length; index++) {
      const start = targets[index - 1].end;
      const end = targets[index].start;
      const gap = source.slice(start, end);
      if (gap.length > 1 && !gap.trim()) patches.push({start,end,value:'\n'});
    }
  }
  return applyPatches(source, range, patches);
}

function stripMark(source: string, document: MarkdownDocument, mark: InlineMark, range: SourceRange): EditorChange {
  const textRuns = document.runs.filter(run => run.text && run.marks.includes(mark));
  if (range.start <= textRuns[0]?.offsets[0] && range.end >= textRuns.at(-1)!.offsets.at(-1)!) {
    const selectionStart = range.start - (mark.openEnd - mark.openStart);
    return {source: source.slice(0, mark.openStart) + source.slice(mark.openEnd, mark.closeStart) + source.slice(mark.closeEnd),
      legacy: false, start: selectionStart, end: selectionStart + range.end - range.start};
  }
  const before = source.slice(mark.openEnd, range.start);
  const selected = source.slice(range.start, range.end);
  const after = source.slice(range.end, mark.closeStart);
  const delimiter = source.slice(mark.openStart, mark.openEnd);
  // Underscore delimiters between letters do not open or close. Use stars at
  // the newly cut edge, leaving the surviving text and its emphasis intact.
  const alternate = '*'.repeat(delimiter.length);
  const wrapBefore = before ? markBody(before, delimiter.startsWith('_') && /^[\p{Alphabetic}\p{Number}]/u.test(selected) ? alternate : delimiter) : undefined;
  const wrapAfter = after ? markBody(after, delimiter.startsWith('_') && /[\p{Alphabetic}\p{Number}]$/u.test(selected) ? alternate : delimiter) : undefined;
  const left = wrapBefore ? wrapBefore.before + wrapBefore.text + wrapBefore.after : '';
  const right = wrapAfter ? wrapAfter.before + wrapAfter.text + wrapAfter.after : '';
  const edit = change(source, false, {start: mark.openStart, end: mark.closeEnd}, left + selected + right);
  const from = mark.openStart + left.length;
  return {...edit, start: from, end: from + selected.length};
}

function visualText(document: MarkdownDocument, range: SourceRange): string {
  return document.runs.filter(run => run.offsets[0] < range.end && run.offsets.at(-1)! > range.start)
    .map(run => visibleRunText(run, range)).join('');
}

function codeSelection(source: string, document: MarkdownDocument, selected: SourceRange): EditorChange {
  const code = allMarks(document).filter(mark => mark.kind === 'code');
  if (code.some(mark => mark.openEnd <= selected.start && selected.end <= mark.closeStart)) {
    return unchanged(source, false, selected);
  }
  const text = selected.start === selected.end ? 'код' : visualText(document, selected);
  function wrap(range: SourceRange, body: string) {
    const {before, after} = codeBody(body);
    return separateInline(source, false, document, range, before + body + after, 'middle', body);
  }
  const direct = wrap(selected, text);
  if (direct.source !== source) return direct;
  const left = code.find(mark => mark.closeEnd === selected.start);
  const right = code.find(mark => mark.openStart === selected.end);
  if (!left && !right) return direct;
  const previous = left ? source.slice(left.openEnd, left.closeStart) : '';
  const following = right ? source.slice(right.openEnd, right.closeStart) : '';
  const merged = wrap({start:left?.openStart ?? selected.start,end:right?.closeEnd ?? selected.end}, previous + text + following);
  if (merged.source === source) return direct;
  const parsed = parseMarkdown(merged.source, false);
  const run = parsed.runs.find(run => run.marks.some(mark => mark.kind === 'code' && mark.closeEnd === merged.start));
  if (!run) return direct;

  const boundary = previous.length + text.length;
  const sameLeft = !left || styleSignature(document, {start:left.openEnd,end:left.closeStart})
    === styleSignature(parsed, {start:run.offsets[0],end:run.offsets[previous.length]});
  const sameRight = !right || styleSignature(document, {start:right.openEnd,end:right.closeStart})
    === styleSignature(parsed, {start:run.offsets[boundary],end:run.offsets.at(-1)!});
  if (!sameLeft || !sameRight) return direct;
  return right ? {...merged,start:run.offsets[boundary],end:run.offsets[boundary]} : merged;
}

function wholeStyle(document: MarkdownDocument, source: string, emphasis?: EmphasisChange): string {
  return styleSignature(document, {start: 0, end: source.length}, emphasis);
}

type SourcePatch = SourceRange & {value: string; inside?: SourceRange};

type EmphasisPiece = {text: string; path: EmphasisKind[]; selected: boolean; whitespace?: boolean};

function partialMarkPieces(source: string, document: MarkdownDocument, mark: InlineMark, selection: SourceRange): EmphasisPiece[] {
  const order = (['bold', 'italic', 'strike'] as const).filter(kind => kind !== mark.kind);
  if (mark.kind === 'bold' || mark.kind === 'italic' || mark.kind === 'strike') order.push(mark.kind);
  const out: EmphasisPiece[] = [];
  for (const run of document.runs) {
    if (!run.text || !run.marks.includes(mark)) continue;
    const code = run.marks.find(item => item.kind === 'code');
    const start = code?.openStart ?? run.offsets[0];
    const end = code?.closeEnd ?? run.offsets.at(-1)!;
    const fragments = code ? [start, end]
      : [start, ...[selection.start, selection.end].filter(edge => start < edge && edge < end), end];
    for (let index = 1; index < fragments.length; index++) {
      const from = fragments[index - 1];
      const to = fragments[index];
      const selected = selection.start <= from && to <= selection.end;
      const path = order.filter(kind => run.marks.some(item => item.kind === kind && item.openStart >= mark.openStart
        && !(selected && item === mark && !code)));
      const [, left, text, right] = /^([ \t]*)(.*?)([ \t]*)$/s.exec(source.slice(from, to))!;
      if (left) out.push({text:left,path,selected,whitespace:true});
      if (text) out.push({text,path,selected});
      if (right) out.push({text:right,path,selected,whitespace:true});
    }
  }
  return out;
}

function commonPath(left: EmphasisKind[], right: EmphasisKind[]): EmphasisKind[] {
  let count = 0;
  while (count < left.length && left[count] === right[count]) count++;
  return left.slice(0, count);
}

function bracketPieces(range: SourceRange, pieces: EmphasisPiece[], markers: Record<EmphasisKind, string>): SourcePatch {
  const rightPaths = new Map<EmphasisPiece, EmphasisKind[]>();
  let right: EmphasisKind[] = [];
  for (const piece of [...pieces].reverse()) {
    if (piece.whitespace) rightPaths.set(piece, right);
    else right = piece.path;
  }
  let value = '';
  let left: EmphasisKind[] = [];
  let stack: EmphasisKind[] = [];
  let selectionStart: number | undefined;
  let selectionEnd = 0;
  for (const piece of pieces) {
    const path = piece.whitespace ? commonPath(left, rightPaths.get(piece)!) : piece.path;
    const common = commonPath(stack, path).length;
    for (let index = stack.length - 1; index >= common; index--) value += markers[stack[index]];
    for (const kind of path.slice(common)) value += markers[kind];
    if (piece.selected) selectionStart ??= value.length;
    value += piece.text;
    if (piece.selected) selectionEnd = value.length;
    stack = path;
    if (!piece.whitespace) left = piece.path;
  }
  for (const kind of [...stack].reverse()) value += markers[kind];
  return {...range, value, inside:{start:selectionStart ?? 0,end:selectionEnd}};
}

function applyPatches(source: string, selection: SourceRange, patches: SourcePatch[]): EditorChange {
  const sorted = [...patches].sort((a, b) => a.start - b.start);
  const output: string[] = [];
  let start = selection.start;
  let end = selection.end;
  let cursor = 0;
  let delta = 0;
  for (const patch of sorted) {
    if (patch.start < cursor) return unchanged(source, false, selection);
    output.push(source.slice(cursor, patch.start), patch.value);
    const adjustment = patch.value.length + patch.start - patch.end;
    if (selection.start > patch.end) start += adjustment;
    else if (selection.start >= patch.start) start = patch.start + delta + (patch.inside?.start ?? 0);
    if (selection.end > patch.end) end += adjustment;
    else if (selection.end >= patch.start) end = patch.start + delta + (patch.inside?.end ?? patch.value.length);
    delta += adjustment;
    cursor = patch.end;
  }
  output.push(source.slice(cursor));
  return {source: output.join(''), legacy: false, start, end};
}

function selectedPart(run: TextRun, selection: SourceRange): SourceRange {
  return {start: Math.max(run.offsets[0], selection.start), end: Math.min(run.offsets.at(-1)!, selection.end)};
}

function applyEmphasis(source: string, document: MarkdownDocument, selection: SourceRange, kind: EmphasisKind): EditorChange {
  const segments = document.runs.filter(run => !run.locked && !run.marks.some(mark => mark.kind === 'code')
    && run.offsets[0] < selection.end && run.offsets.at(-1)! > selection.start
    && /\S/u.test(visibleRunText(run, selection)));
  if (!segments.length) return unchanged(source, false, selection);
  const hasMark = (run: TextRun) => run.marks.find(mark => mark.kind === kind);
  const removing = segments.every(hasMark);
  const expected = wholeStyle(document, source, {range:selection, kind, apply: !removing});
  const matches = (candidate: string) => wholeStyle(parseMarkdown(candidate, false), candidate) === expected;
  if (removing) {
    const marks = new Map<InlineMark, SourceRange>();
    for (const run of document.runs) {
      const mark = hasMark(run);
      if (!mark || !run.text) continue;
      const start = marks.get(mark)?.start ?? run.offsets[0];
      marks.set(mark, {start, end: run.offsets.at(-1)!});
    }
    const patches: SourcePatch[] = [];
    const partial: {index:number; mark:InlineMark; range:SourceRange}[] = [];
    for (const mark of new Set(segments.map(run => hasMark(run)!))) {
      const bounds = marks.get(mark)!;
      if (selection.start <= bounds.start && bounds.end <= selection.end) {
        patches.push({start:mark.openStart,end:mark.openEnd,value:''}, {start:mark.closeStart,end:mark.closeEnd,value:''});
      } else {
        const range = {start: Math.max(selection.start, mark.openEnd), end: Math.min(selection.end, mark.closeStart)};
        const edit = stripMark(source, document, mark, range);
        const delta = edit.source.length - source.length;
        partial.push({index:patches.length,mark,range});
        patches.push({start:mark.openStart,end:mark.closeEnd,value:edit.source.slice(mark.openStart, mark.closeEnd + delta),
          inside:{start:edit.start - mark.openStart,end:edit.end - mark.openStart}});
      }
    }
    const edit = applyPatches(source, selection, patches);
    if (matches(edit.source)) return edit;
    // An outer mark can be interrupted inside an inner mark. Keep adjacent
    // untouched Markdown; rebracket only the crossing mark's contents.
    const pieces = partial.map(item => ({...item,pieces:partialMarkPieces(source,document,item.mark,item.range)}));
    for (const markers of [{...emphasisMarkers, italic:'_'}, {...emphasisMarkers, bold:'__'}, emphasisMarkers]) {
      const alternate = [...patches];
      for (const item of pieces) {
        alternate[item.index] = bracketPieces({start:item.mark.openStart,end:item.mark.closeEnd},item.pieces,markers);
      }
      const candidate = applyPatches(source, selection, alternate);
      if (matches(candidate.source)) return candidate;
    }
    return unchanged(source, false, selection);
  }

  const delimiter = emphasisMarkers[kind];
  const alternative = kind === 'strike' ? delimiter : '_'.repeat(delimiter.length);
  const unmarked = segments.filter(run => !hasMark(run));
  for (const marker of [delimiter, alternative]) {
    const patches = unmarked.map(run => {
      const range = selectedPart(run, selection);
      const pieces = markBody(source.slice(range.start, range.end), marker);
      return {...range,value:pieces.before + pieces.text + pieces.after,
        inside:{start:pieces.before.length,end:pieces.before.length + pieces.text.length}};
    });
    const edit = applyPatches(source, selection, patches);
    if (matches(edit.source)) return edit;
  }

  // Adjacent delimiters can need a different spelling. Keep the repair bounded
  // because each attempt checks the visible state of the whole draft.
  if (unmarked.length > 32) return unchanged(source, false, selection);
  let start = selection.start;
  let end = selection.end;
  let result = source;
    for (const run of [...unmarked].reverse()) {
      const range = selectedPart(run, selection);
      const text = result.slice(range.start, range.end);
      const current = parseMarkdown(result, false);
      const expected = wholeStyle(current, result, {range, kind, apply: true});
      const options = [delimiter, alternative].map(marker => {
        const pieces = markBody(text, marker);
        return {
          source: change(result, false, range, pieces.before + pieces.text + pieces.after).source,
          from: range.start + pieces.before.length,
          to: range.start + pieces.before.length + pieces.text.length,
        };
      });
      if (text === text.trim()) {
        for (const mark of allMarks(current).filter(mark => mark.kind === kind)) {
          if (mark.openStart === range.end && mark.openEnd < end) {
            const head = result.slice(mark.openStart, mark.openEnd);
            options.push({source: change(result, false, {start:range.start,end:mark.openEnd}, head + text).source,
              from: range.start + head.length,to:range.start + head.length + text.length});
          }
          if (mark.closeEnd === range.start && mark.closeStart > start) {
            const tail = result.slice(mark.closeStart, mark.closeEnd);
            options.push({source: change(result, false, {start:mark.closeStart,end:range.end}, text + tail).source,
              from: mark.closeStart,to:mark.closeStart + text.length});
          }
        }
      }
      const chosen = options.find(option => wholeStyle(parseMarkdown(option.source, false), option.source) === expected);
      if (!chosen) return unchanged(source, false, selection);
      if (range.start === selection.start) start = chosen.from;
      end = range.end === selection.end ? chosen.to : end + chosen.source.length - result.length;
      result = chosen.source;
    }
  return matches(result)
    ? {source: result, legacy: false, start, end}
    : unchanged(source, false, selection);
}

export function applyCommand(source: string, legacy: boolean, range: SourceRange, command: EditorCommand): EditorChange {
  const initial = parseMarkdown(source, legacy);
  if (isProtected(initial, range)) return unchanged(source, legacy, range);
  const block = blockAt(initial, range.start);
  const structural = intersectedStructure(initial, range);
  if (structural && !tableCellAt(structural, range)) return unchanged(source, legacy, range);
  if (command === 'code' && block !== blockAt(initial, range.end)) command = 'code-block';
  const inline = command === 'bold' || command === 'italic' || command === 'strike' || command === 'code';
  if (!inline && structural || inline && command !== 'code' && range.start === range.end) return unchanged(source, legacy, range);
  const promoted = promote(source, legacy, range);
  const document = parseMarkdown(promoted.source, false);
  let result: EditorChange;
  if (command === 'heading' || command === 'list' || command === 'bullet' || command === 'ordered' || command === 'task' || command === 'quote') {
    result = applyBlockCommand(promoted.source, document, promoted.range, command);
  } else if (command === 'rule' || command === 'table' || command === 'code-block') {
    const {source: draft, range: selected} = promoted;
    const text = command === 'code-block' ? visibleBlockText(document, selected) : '';
    result = insertFormattedBlock(draft, document, selected, blockTemplate(command, text));
  } else {
    result = command === 'code' ? codeSelection(promoted.source, document, promoted.range)
      : applyEmphasis(promoted.source, document, promoted.range, command);
  }
  return result.source === promoted.source ? unchanged(source, legacy, range) : result;
}

type BlockInsertion = {value: string; anchor: number; followingParagraph?: boolean};
export type BlockMenuCommand = BlockCommand | SpecialBlockCommand | 'heading-one' | 'heading-three';

function blockTemplate(command: BlockMenuCommand, body = ''): BlockInsertion {
  switch (command) {
    case 'heading-one':
    case 'heading-three': {
      const value = (command === 'heading-one' ? '# ' : '### ') + 'Заголовок';
      return {value, anchor:value.length};
    }
    case 'code-block': {
      const text = body || 'код';
      const fence = '\x60'.repeat(Math.max(3, fenceCollision(text, '\x60') + 1));
      return {value: fence + '\n' + text + '\n' + fence, anchor: fence.length + 1 + text.length};
    }
    case 'table':
      return {value: '| Столбец 1 | Столбец 2 |\n| --- | --- |\n|  |  |\n|  |  |', anchor: 11};
    case 'rule':
      return {value: '---', anchor: 4, followingParagraph: true};
    default: {
      const text = command === 'heading' ? 'Заголовок'
        : command === 'quote' ? 'Цитата' : 'Пункт списка';
      const value = prefixes[command] + text;
      return {value, anchor: value.length};
    }
  }
}

function insertBlock(source: string, range: SourceRange, block: BlockInsertion): EditorChange {
  const left = source.slice(0, range.start);
  const right = source.slice(range.end);
  const prefix = left ? '\n'.repeat(Math.max(0, 2 - (left.match(/\n*$/)![0].length + block.value.match(/^\n*/)![0].length))) : '';
  const suffix = right || block.followingParagraph
    ? '\n'.repeat(Math.max(0, 2 - (right.match(/^\n*/)![0].length + block.value.match(/\n*$/)![0].length)))
    : '';
  const edit = change(source, false, range, prefix + block.value + suffix);
  const cursor = range.start + prefix.length + block.anchor;
  return {...edit, start: cursor, end: cursor};
}

function visibleBlockText(document: MarkdownDocument, range: SourceRange): string {
  return document.blocks.filter(block => block.start <= range.end && range.start <= block.end)
    .map(block => block.runs.filter(run => run.offsets[0] < range.end && range.start < run.offsets.at(-1)!)
      .map(run => visibleRunText(run, range)).join(''))
    .join('\n');
}

function insertionGroupEnd(source: string, document: MarkdownDocument, block: EditorBlock): number {
  const group = blockUnits(document, source, false).find(unit => unit.start <= block.start && block.end <= unit.end);
  return group?.end ?? block.end;
}

function insertFormattedBlock(source: string, document: MarkdownDocument, range: SourceRange, raw: BlockInsertion): EditorChange {
  if (range.start === range.end) {
    const block = blockAt(document, range.start);
    if (!block) return insertBlock(source, range, raw);
    if (!source.slice(block.start, block.end).trim()) {
      return insertBlock(source, {start: block.start, end: block.end}, raw);
    }
    const end = insertionGroupEnd(source, document, block);
    return insertBlock(source, {start: end, end}, raw);
  }
  const rawEdit = insertBlock(source, range, raw);
  const after = rawEdit.source.length - (source.length - range.end);
  const inserted = rawEdit.source.slice(range.start, after);
  const closed = separateInline(source, false, document, range, inserted, 'middle');
  const cursor = closed.start - (after - rawEdit.start);
  return closed.source === source ? unchanged(source, false, range) : {...closed, start: cursor, end: cursor};
}

const slashToken = /^(?:\\)?\/[^\s/]*$/u;

function insertAfterSlash(source: string, document: MarkdownDocument, range: SourceRange, raw: BlockInsertion): EditorChange {
  const block = blockAt(document, range.start);
  if (!block || range.end > block.end || structuralBlock(block)) return unchanged(source, false, range);
  const removed = replaceAcrossMarks(source, false, document, range, '');
  const offset = removed.source.length - source.length;
  const blockEnd = block.end + offset;
  if (!removed.source.slice(block.start, blockEnd).trim()) {
    return insertBlock(removed.source, {start:block.start, end: blockEnd}, raw);
  }
  const end = insertionGroupEnd(source, document, block) + offset;
  return insertBlock(removed.source, {start: end, end}, raw);
}

function insertMenuTemplate(source: string, legacy: boolean, range: SourceRange, template: BlockInsertion): EditorChange {
  const atCaret = range.start === range.end;
  if ((!atCaret && !slashToken.test(source.slice(range.start, range.end))) ||
      isProtected(parseMarkdown(source, legacy), range)) return unchanged(source, legacy, range);
  const promoted = promote(source, legacy, range);
  const insert = atCaret ? insertFormattedBlock : insertAfterSlash;
  const result = insert(promoted.source, parseMarkdown(promoted.source, false), promoted.range, template);
  return result.source === promoted.source ? unchanged(source, legacy, range) : result;
}

export function insertMenuBlock(source: string, legacy: boolean, range: SourceRange, command: BlockMenuCommand): EditorChange {
  return insertMenuTemplate(source, legacy, range, blockTemplate(command));
}

export function insertExternalEmbed(source: string, legacy: boolean, range: SourceRange, address: string): EditorChange {
  const embed = externalEmbedAddress(address);
  if (!embed) return unchanged(source, legacy, range);
  const value = '```sever-embed\n' + embed.url + '\n```';
  return insertMenuTemplate(source, legacy, range, {value, anchor: value.length + 1, followingParagraph: true});
}

export function updateExternalEmbed(source: string, legacy: boolean, blockRange: SourceRange, original: string, address?: string) {
  const block = blockAt(parseMarkdown(source, legacy), blockRange.start);
  if (!block?.embed || !block.fence || block.start !== blockRange.start || block.end !== blockRange.end || block.embed.url !== original) return;
  const url = address === undefined ? undefined : externalEmbedAddress(address)?.url;
  if (address !== undefined && !url) return;
  const before = url ? {start: block.bodyStart, end: block.fence.bodyEnd} : blockRange;
  const changed = url === original ? unchanged(source, legacy, before) : change(source, legacy, before, url ?? '');
  const end = url ? block.end + changed.source.length - source.length + Number(source[block.end] === '\n') : changed.end;
  return {change: changed, before, after: {start: end, end}};
}

export function pasteMarkdown(source: string, legacy: boolean, range: SourceRange, input: string): EditorChange {
  const value = normalizeEditorText(input);
  const initial = parseMarkdown(source, legacy);
  if (!value.trim() || isProtected(initial, range) || intersectedStructure(initial, range)) return unchanged(source, legacy, range);
  const promoted = promote(source, legacy, range);
  const document = parseMarkdown(promoted.source, false);
  const raw = {value, anchor: value.length};
  let result: EditorChange;
  if (slashToken.test(source.slice(range.start, range.end))) {
    result = insertAfterSlash(promoted.source, document, promoted.range, raw);
  } else {
    result = insertFormattedBlock(promoted.source, document, promoted.range, raw);
  }
  return result.source === promoted.source ? unchanged(source, legacy, range) : result;
}

export function insertAttachmentImage(source: string, legacy: boolean, range: SourceRange, id: string, alt: string): EditorChange {
  const initial = parseMarkdown(source, legacy);
  const block = blockAt(initial, range.start);
  if (!block || block !== blockAt(initial, range.end) || range.start < block.bodyStart ||
      isProtected(initial, range) || structuralBlock(block) && !tableCellAt(block, range) ||
      /[\r\n]/u.test(alt)) return unchanged(source, legacy, range);
  const promoted = promote(source, legacy, range);
  const document = parseMarkdown(promoted.source, false);
  const selected = promoted.range;
  if (allMarks(document).some(mark => (mark.kind === 'code' || mark.kind === 'link') &&
      mark.openStart < selected.end && selected.start < mark.closeEnd)) return unchanged(source, legacy, range);

  const reference = `![${escapeLiteralText(alt)}](attachment:${id})`;
  const result = separateInline(promoted.source, false, document, selected, reference, 'middle');
  const rendered = parseMarkdown(result.source, false).runs.find(run => run.attachment?.id === id &&
    run.attachment.image && run.offsets.at(-1) === result.end);
  return rendered ? result : unchanged(source, legacy, range);
}

export function attachmentRanges(source: string, legacy: boolean, id: string): SourceRange[] {
  return attachmentSpans(source, legacy, new Set([id]));
}

function attachmentSpans(source: string, legacy: boolean, ids: Set<string>): SourceRange[] {
  return parseMarkdown(source, legacy).runs.filter(run => run.attachment && ids.has(run.attachment.id)).map(run => ({
    start: run.offsets[0], end: run.offsets.at(-1)!,
  }));
}

export function removeAttachmentReferences(source: string, legacy: boolean, selection: SourceRange, ids: string | readonly string[]): EditorChange {
  const ranges = attachmentSpans(source, legacy, new Set(typeof ids === 'string' ? [ids] : ids));
  if (!ranges.length) return unchanged(source, legacy, selection);
  return applyPatches(source, selection, ranges.map(range => ({...range, value: ''})));
}

export function toggleTask(source: string, legacy: boolean, range: SourceRange): EditorChange {
  const task = blockAt(parseMarkdown(source, legacy), range.start)?.task;
  if (!task) return unchanged(source, legacy, range);
  return {...change(source, legacy, task, task.checked ? ' ' : 'x'), start: range.start, end: range.end};
}

function subtreeEnd(blocks: EditorBlock[], from: number): number {
  const depth = blocks[from].indent ?? 0;
  let last = from;
  while (listItem(blocks[last + 1]) && (blocks[last + 1].indent ?? 0) > depth) last++;
  return last;
}

function precedingListPeer(blocks: EditorBlock[], from: number, depth: number, direction: 'indent' | 'outdent'): number {
  for (let index = from - 1; listItem(blocks[index]); index--) {
    const previousDepth = blocks[index].indent ?? 0;
    if (direction === 'indent' && previousDepth < depth) break;
    if (direction === 'indent' ? previousDepth === depth : previousDepth < depth) return index;
  }
  return -1;
}

export function moveListItem(
  source: string, legacy: boolean, range: SourceRange, direction: 'indent' | 'outdent',
): EditorChange {
  const document = parseMarkdown(source, legacy);
  const block = blockAt(document, range.start);
  if (!listItem(block) || range.start < block.bodyStart || range.end > block.end) return unchanged(source, legacy, range);
  const {blocks} = document;
  const index = blocks.indexOf(block);
  const depth = block.indent ?? 0;
  const peerIndex = precedingListPeer(blocks, index, depth, direction);
  if (peerIndex < 0) return unchanged(source, legacy, range);
  const peer = blocks[peerIndex];
  const peerDepth = peer.indent ?? 0;
  const child = blocks[peerIndex + 1];
  const marker = source.slice(peer.markerStart ?? peer.start, peer.bodyStart).match(/^(?:\d{1,9}[.)]|[-+*])[ \t]+/)![0];
  let markerEnd = peerDepth;
  for (const character of marker) markerEnd += character === '\t' ? 4 - markerEnd % 4 : 1;
  const target = direction === 'outdent' ? peerDepth
    : listItem(child) && (child.indent ?? 0) > depth ? child.indent!
    : markerEnd;
  const last = subtreeEnd(blocks, index);
  const oldEnd = blocks[last].end;
  const newEnd = direction === 'outdent' ? blocks[subtreeEnd(blocks, peerIndex)].end : oldEnd;
  const moved = blocks.slice(index, last + 1).map(item => {
    return ' '.repeat((item.indent ?? 0) + target - depth) + source.slice(item.markerStart ?? item.start, item.end);
  }).join('\n');
  const remaining = newEnd > oldEnd ? source.slice(oldEnd + 1, newEnd) + '\n' : '';
  const shift = remaining.length + target - ((block.markerStart ?? block.start) - block.start);
  const edit = change(source, legacy, {start: block.start, end: newEnd}, remaining + moved);
  return {...edit, start: range.start + shift, end: range.end + shift};
}

export function updateCodeLanguage(source: string, legacy: boolean, range: SourceRange, language: string): EditorChange {
  const fence = blockAt(parseMarkdown(source, legacy), range.start)?.fence;
  const value = language.trim();
  if (!fence || fence.closingStart === null || /[\r\n\u0000]/.test(language)
    || fence.marker[0] === '\x60' && value.includes('\x60') || value === fence.language) {
    return unchanged(source, legacy, range);
  }
  const needsSeparator = fence.infoStart === fence.infoEnd || fence.marker.startsWith('~') && value.startsWith('~');
  const leading = needsSeparator && !/[ \t]/.test(source[fence.infoStart - 1] ?? '') ? ' ' : '';
  return applyPatches(source, range, [{start: fence.infoStart, end: fence.infoEnd, value: leading + value}]);
}

export function exitCodeBlock(source: string, legacy: boolean, range: SourceRange): EditorChange {
  const block = blockAt(parseMarkdown(source, legacy), range.start);
  if (!block?.fence || block.fence.closingStart === null) return unchanged(source, legacy, range);
  const following = source.slice(block.end);
  const prefix = !following.startsWith('\n') || following.length > 1 && !following.startsWith('\n\n') ? '\n' : '';
  const cursor = block.end + 1;
  const edit = change(source, legacy, {start:block.end, end:block.end}, prefix);
  return {...edit, start: cursor, end: cursor};
}

function selectedLabel(document: MarkdownDocument, range: SourceRange): string {
  const pieces: EmphasisPiece[] = [];
  for (const run of document.runs) {
    if (run.offsets[0] >= range.end || range.start >= run.offsets.at(-1)!) continue;
    const text = visibleRunText(run, range);
    const path = (['bold', 'italic', 'strike'] as const).filter(kind => run.marks.some(mark => mark.kind === kind));
    if (run.marks.some(mark => mark.kind === 'code')) {
      const code = codeBody(text);
      pieces.push({text:code.before + code.text + code.after, path, selected:true});
      continue;
    }
    const [, before, body, after] = /^(\s*)(.*?)(\s*)$/su.exec(text)!;
    if (before) pieces.push({text:before, path, selected:true, whitespace:true});
    if (body) pieces.push({text:escapeLiteralText(body), path, selected:true});
    if (after) pieces.push({text:after, path, selected:true, whitespace:true});
  }
  return bracketPieces(range, pieces, {...emphasisMarkers, italic:'_'}).value;
}

export function applyLink(
  source: string, legacy: boolean, range: SourceRange, label: string, address: string,
): EditorChange {
  const href = editorLinkDestination(address.trim());
  const initial = parseMarkdown(source, legacy);
  const block = blockAt(initial, range.start);
  if (!href || !label.trim() || /[\r\n]/.test(label) || !block || block !== blockAt(initial, range.end)
    || isProtected(initial, range) || structuralBlock(block) && !tableCellAt(block, range)) return unchanged(source, legacy, range);
  const promoted = promote(source, legacy, range);
  const document = parseMarkdown(promoted.source, false);
  const selected = promoted.range;
  if (allMarks(document).some(mark => mark.kind === 'link' && mark.openStart < selected.end && selected.start < mark.closeEnd)) {
    return unchanged(source, legacy, range);
  }
  const destination = href.replace(/[()|]/g, '\\$&');
  const literal = escapeLiteralText(label);
  const expanded = expandRemovedMarks(document, selected, true);
  const raw = promoted.source.slice(expanded.start, expanded.end);
  const sameLabel = label === visualText(document, selected);
  const expectedStyle = sameLabel ? styleSignature(document, selected, undefined, true) : undefined;
  const labelOptions = sameLabel ? new Set([raw, selectedLabel(document, selected), literal]) : [literal];
  for (const content of labelOptions) {
    const insertion = '[' + content + '](' + destination + ')';
    const edit = separateInline(promoted.source, false, document, selected, insertion, 'middle');
    if (edit.source === promoted.source) continue;
    const result = parseMarkdown(edit.source, false);
    const mark = allMarks(result).find(mark => mark.kind === 'link' && mark.closeEnd === edit.start);
    if (mark?.href !== href) continue;
    const bounds = {start: mark.openEnd, end: mark.closeStart};
    if (visualText(result, bounds) !== label || expectedStyle !== undefined
      && styleSignature(result, bounds, undefined, true) !== expectedStyle) continue;
    return {...edit, ...bounds};
  }
  return unchanged(source, legacy, range);
}
