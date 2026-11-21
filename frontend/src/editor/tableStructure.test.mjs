import assert from 'node:assert/strict';
import test from 'node:test';
import {registerHooks} from 'node:module';

const imports = registerHooks({
  resolve(specifier, context, next) {
    return next(specifier === './markdownModel' ? './markdownModel.ts' : specifier, context);
  },
});
const {parseMarkdown} = await import('./markdownModel.ts');
const {changeTableStructure} = await import('./tableStructure.ts');
imports.deregister();

const before = 'До\n\n';
const after = '\n\nПосле';
const source = before + 'Имя| **край** | Тег\n:----|:---:|-----:\nA \\| B |  10 |[цель](https://example.test)' + after;
const target = {tableStart:before.length, row:1, column:1};

function table(value) {
  return parseMarkdown(value, false).blocks.find(block => block.start === before.length)?.table;
}
function checkFocus(change, row, column) {
  const cell = table(change.source).rows[row].cells[column];
  assert.equal(change.start, cell.runs[0].offsets[0]);
  assert.equal(change.start, change.end);
}

test('adds and removes a row at the selected cell without changing adjacent Markdown or the header separator', () => {
  const added = changeTableStructure(source, target, 'add-row');
  assert.equal(added.source, source.replace(after, '\n|  |  |  |' + after));
  checkFocus(added, 2, 1);
  const removed = changeTableStructure(added.source, {...target, row:2}, 'remove-row');
  assert.equal(removed.source, source);
  checkFocus(removed, 1, 1);
  const afterHeader = changeTableStructure(source, {...target, row:0}, 'add-row');
  assert.match(afterHeader.source, /-----:\n\|  \|  \|  \|\nA/);
  checkFocus(afterHeader, 1, 1);
});

test('inserts a column to the right, keeps exact padding, escapes and alignment, and reverses the change', () => {
  const added = changeTableStructure(source, target, 'add-column');
  assert.equal(added.source, before + 'Имя| **край** |  | Тег\n:----|:---:| --- |-----:\nA \\| B |  10 |  |[цель](https://example.test)' + after);
  assert.deepEqual(table(added.source).alignments, ['left','center',null,'right']);
  checkFocus(added, 1, 2);
  const removed = changeTableStructure(added.source, {...target,column:2}, 'remove-column');
  assert.equal(removed.source, source);
  checkFocus(removed, 1, 2);
});

test('retains a valid single-column table when removing either column from an unframed table', () => {
  const unframed = before + 'a|b\n:---|---:\nx|y' + after;
  for (const column of [0, 1]) {
    const changed = changeTableStructure(unframed, {...target,column}, 'remove-column');
    const grid = table(changed.source);
    assert.deepEqual(grid.alignments, [column ? 'left' : 'right']);
    assert.equal(grid.rows[1].cells[0].runs[0].text, column ? 'x' : 'y');
    checkFocus(changed, 1, 0);
    assert.equal(changeTableStructure(changed.source, {...target,column:0}, 'remove-column'), undefined);
  }
});

test('allows header-only tables, rejects deletion of the header and ignores stale cell coordinates or protected syntax', () => {
  const compact = before + '|a|\n|---|\n|x|' + after;
  assert.equal(changeTableStructure(compact, {...target,row:0,column:0}, 'remove-row'), undefined);
  assert.equal(changeTableStructure(compact, target, 'add-row'), undefined);
  assert.equal(changeTableStructure(source, {...target,row:-1}, 'remove-column'), undefined);
  assert.equal(changeTableStructure(source, {...target,tableStart:2}, 'remove-row'), undefined);
  assert.equal(changeTableStructure('a|b\n---|---\na|b|c', {...target,tableStart:0,column:0}, 'remove-row'), undefined);
  const last = changeTableStructure(compact, {...target,column:0}, 'remove-row');
  assert.equal(last.source, before + '|a|\n|---|' + after);
  checkFocus(last, 0, 0);
  const restored = changeTableStructure(last.source, {...target,row:0,column:0}, 'add-row');
  checkFocus(restored, 1, 0);
});

test('inserts an empty last column on unframed and partially framed tables', () => {
  for (const body of ['a|b\n---|---\nx|y', '|a|b\n|---|---\n|x|y', 'a|b|\n---|---|\nx|y|']) {
    const changed = changeTableStructure(before + body + after, target, 'add-column');
    assert.equal(table(changed.source).alignments.length, 3);
    assert.equal(table(changed.source).rows[1].cells[2].runs[0].text.trim(), '');
    checkFocus(changed, 1, 2);
  }
});
