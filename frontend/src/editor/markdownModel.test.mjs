import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import {httpDestination, isProtected, parseMarkdown} from './markdownModel.ts';

const fixtures = JSON.parse(readFileSync(
  new URL('../../../backend/src/markdown-extended-fixtures.json', import.meta.url),
  'utf8',
));

for (const {name, source, kinds, runs, href} of fixtures) {
  test(name, () => {
    const model = parseMarkdown(source, false);
    assert.deepEqual(model.blocks.filter(block => block.start < block.end).map(block => block.kind), kinds);
    if (runs) {
      const visible = model.runs.filter(run => run.text).map(run => [run.text, run.marks.map(mark => mark.kind)]);
      assert.deepEqual(visible, runs);
    }
    if (href) assert.equal(model.runs.flatMap(run => run.marks).find(mark => mark.kind === 'link')?.href, href);
  });
}

test('editable metadata selects only the intended source characters', () => {
  const {source} = fixtures[1];
  const [table, , code] = parseMarkdown(source, false).blocks;
  assert.equal(table.table.rows.length, 3);
  const [first, second] = table.table.rows[1].cells;
  assert.equal(source.slice(first.start, first.runs[0].offsets.at(-1)), 'A \\| B');
  assert.equal(first.runs[0].text, 'A | B');
  assert.equal(first.runs[0].offsets[3], first.start + 4);
  assert.ok(!isProtected({blocks: [table], runs: table.runs}, first));
  assert.ok(isProtected({blocks: [table], runs: table.runs}, {start: first.start, end: second.end}));
  assert.equal(table.table.rows[2].cells[0].start, table.table.rows[2].cells[0].end);
  const link = second.runs.flatMap(run => run.marks).find(mark => mark.kind === 'link');
  assert.equal(source.slice(link.openEnd, link.closeStart), 'официальный **источник**');
  assert.equal(link.href, 'https://example.test/about?q=1&v=2');
  assert.equal(source.slice(code.fence.infoStart, code.fence.infoEnd), 'sh');
  assert.equal(source.slice(code.bodyStart, code.fence.bodyEnd), '---  \n\tне менять');
  assert.ok(!isProtected({blocks: [code], runs: code.runs}, {start: code.bodyStart, end: code.fence.bodyEnd}));
});

test('the visual caret can stay after a space just typed at the end of a table cell', () => {
  for (const input of ['|  A |  |', '| A  | B |', 'A  | B']) {
    const source = '| A | B |\n| --- | --- |\n' + input;
    const [cell] = parseMarkdown(source, false).blocks[0].table.rows[1].cells;
    const afterSpace = source.lastIndexOf('A') + 2;
    assert.ok(cell.runs.some(run => run.offsets.includes(afterSpace)), input);
    assert.ok(cell.end >= afterSpace, input);
    assert.equal(cell.runs.map(run => run.text).join('').trim(), 'A');
  }
});

test('nested numbering and checkbox source positions are retained', () => {
  const {source} = fixtures[0];
  const blocks = parseMarkdown(source, false).blocks.slice(0, 5);
  assert.deepEqual(blocks.map(({indent, number}) => [indent, number]), [
    [0, 7], [3, undefined], [3, undefined], [5, 3], [0, 1],
  ]);
  assert.equal(source.slice(blocks[1].task.start, blocks[1].task.end), 'x');
  assert.equal(source.slice(blocks[2].task.start, blocks[2].task.end), ' ');
  assert.equal(source[blocks[2].markerStart], '-');
  assert.deepEqual(parseMarkdown(source, true).blocks.map(block => block.kind), source.split('\n').map(() => 'paragraph'));
});

test('absolute link destination requires a supported scheme and a host', () => {
  assert.equal(httpDestination('HTTP://EXAMPLE.test'), 'http://example.test/');
  assert.equal(httpDestination('https://'), null);
  assert.equal(httpDestination('/page'), null);
  assert.equal(httpDestination('mailto:user@example.test'), null);
  assert.equal(httpDestination('https://example.test/a b'), null);
});
