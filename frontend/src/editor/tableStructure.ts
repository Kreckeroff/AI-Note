import {parseMarkdown, tableCells} from './markdownModel';
import type {EditorChange} from './editorChanges';

export type TablePosition = {tableStart: number; row: number; column: number};
export type TableOperation = 'add-row' | 'remove-row' | 'add-column' | 'remove-column';

function editColumn(line: string, column: number, add: boolean, separator: boolean): string {
  // The caller only supplies rows of a parsed, rectangular table.
  const cells = tableCells({text:line, start:0, end:line.length})!;
  const fields = cells.map(cell => line.slice(cell.rawStart, cell.end));
  let prefix = line.slice(0, cells[0].rawStart);
  let suffix = line.slice(cells[cells.length - 1].end);
  if (add) fields.splice(column + 1, 0, separator ? ' --- ' : '  ');
  else fields.splice(column, 1);
  if (fields.length === 1 && !prefix.includes('|') && !suffix.includes('|')) {
    // A bare single cell has no pipe and would be parsed as a paragraph.
    prefix += '|';
    suffix = '|' + suffix;
  }
  if (!fields[0].trim() && !prefix.includes('|')) prefix += '|';
  if (!fields[fields.length - 1].trim() && !suffix.includes('|')) suffix = '|' + suffix;
  return prefix + fields.join('|') + suffix;
}

export function changeTableStructure(source: string, at: TablePosition, operation: TableOperation): EditorChange | undefined {
  const block = parseMarkdown(source, false).blocks.find(item => item.start === at.tableStart);
  const table = block?.table;
  if (!block || !table?.rows[at.row]?.cells[at.column]) return;
  const lines = source.slice(block.start, block.end).split('\n');
  let row = at.row;
  let column = at.column;
  switch (operation) {
    case 'add-row':
      lines.splice(row + 2, 0, '|' + '  |'.repeat(table.alignments.length));
      row++;
      break;
    case 'remove-row':
      if (row === 0) return;
      lines.splice(row + 1, 1);
      row = Math.min(row, table.rows.length - 2);
      break;
    case 'add-column':
    case 'remove-column': {
      const add = operation === 'add-column';
      if (!add && table.alignments.length === 1) return;
      for (let i = 0; i < lines.length; i++) lines[i] = editColumn(lines[i], column, add, i === 1);
      column = add ? column + 1 : Math.min(column, table.alignments.length - 2);
      break;
    }
  }
  const replacement = lines.join('\n');
  const cell = parseMarkdown(replacement, false).blocks[0]?.table?.rows[row]?.cells[column];
  if (!cell) return;
  const start = block.start + (cell.runs.find(run => !run.locked)?.offsets[0] ?? cell.start);
  return {source:source.slice(0,block.start) + replacement + source.slice(block.end), legacy:false, start, end:start};
}
