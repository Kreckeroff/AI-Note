import {blockUnits, parseMarkdown, type BlockUnit} from './markdownModel';
import type {EditorChange} from './editorChanges';

export type BlockAction = 'up' | 'down' | 'duplicate' | 'remove';
export function changeBlock(source: string, legacy: boolean, units: readonly BlockUnit[], index: number,
  action: BlockAction): EditorChange | undefined {
  const block = units[index];
  if (!block?.safe) return;
  const before = units[index - 1];
  const after = units[index + 1];
  const raw = source.slice(block.start,block.end);
  const order = [...units];
  let next: string;
  let destination = index;
  switch (action) {
    case 'duplicate': {
      const gap = legacy ? '\n' : '\n\n';
      next = source.slice(0,block.end) + gap + raw + source.slice(block.end);
      order.splice(index + 1,0,block);
      destination++;
      break;
    }
    case 'remove': {
      const cutStart = !after && before ? before.end : block.start;
      next = source.slice(0,cutStart) + source.slice(after?.start ?? block.end);
      order.splice(index,1);
      destination = Math.min(index,order.length - 1);
      break;
    }
    case 'up':
    case 'down': {
      const other = action === 'up' ? before : after;
      if (!other?.safe) return;
      destination = index + (action === 'up' ? -1 : 1);
      const [left,right] = block.start < other.start ? [block,other] : [other,block];
      next = source.slice(0,left.start) + source.slice(right.start,right.end) + source.slice(left.end,right.start) +
        source.slice(left.start,left.end) + source.slice(right.end);
      [order[index],order[destination]] = [other,block];
      break;
    }
  }
  // A new neighbour can turn plain text into a table, a list continuation or
  // a different heading. Only commit the splice when every unit round-trips.
  function check(text: string) {
    const result = blockUnits(parseMarkdown(text,legacy),text,legacy);
    return result.length === order.length && result.every((unit,i) => {
      const expected = order[i];
      return unit.kind === expected.kind && unit.safe === expected.safe &&
        text.slice(unit.start,unit.end) === source.slice(expected.start,expected.end);
    }) ? result : undefined;
  }
  let parsed = check(next);
  if (!parsed && !legacy) {
    const original = new Map(units.map((unit,i) => [unit,i]));
    const pieces = [source.slice(0,units[0].start)];
    for (const [i,unit] of order.entries()) {
      const prior = order[i - 1];
      if (prior) pieces.push(original.get(unit)! === original.get(prior)! + 1
        ? source.slice(prior.end,unit.start) : '\n\n');
      pieces.push(source.slice(unit.start,unit.end));
    }
    next = pieces.join('') + source.slice(units[units.length - 1].end);
    parsed = check(next);
  }
  if (!parsed) return;
  const caret = parsed[destination]?.caret ?? 0;
  return {source:next,legacy,start:caret,end:caret};
}
