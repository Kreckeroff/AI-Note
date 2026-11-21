import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import test from 'node:test';
import {externalEmbedAddress, httpDestination, isProtected, parseMarkdown, selectionFormatting} from './markdownModel.ts';

const fixtures = JSON.parse(readFileSync(
  new URL('../../../backend/src/markdown-extended-fixtures.json', import.meta.url),
  'utf8',
));

test('toolbar reports all, some or none of the selected text carrying each mark', () => {
  const source = '**жирный** и *курсив*';
  const model = parseMarkdown(source,false);
  assert.deepEqual(selectionFormatting(model,{start:2,end:4}),{bold:true,italic:false,strike:false,code:false});
  assert.deepEqual(selectionFormatting(model,{start:3,end:source.length-1}),{bold:'mixed',italic:'mixed',strike:false,code:false});
  assert.equal(selectionFormatting(model,{start:3,end:3}).bold,true);
});

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

test('external embed addresses preserve URL components and reject ambiguous or local destinations', () => {
  assert.deepEqual(externalEmbedAddress(' HTTPS://Video.example.test:443/embed?a=1&b=2#t=1\t', 'sever.test'), {
    host: 'video.example.test', url: 'https://video.example.test/embed?a=1&b=2#t=1',
  });
  for (const value of [
    'http://video.example.test', 'https:///video.example.test', '//video.example.test',
    'https://a:b@video.example.test', 'https://@video.example.test', 'https://video.example.test/a\\b',
    'https://video.example.test/a\nb', 'https://video.example.test/a b', 'https://Sever.test.:4125/page', 'https://sever.test../page',
  ]) assert.equal(externalEmbedAddress(value, 'sever.test'), undefined, value);
});

test('only a closed single-line sever-embed fence becomes an atomic block', () => {
  const url = 'https://video.example.test/watch';
  const valid = `~~~ sever-embed\t\n${url}\n~~~`;
  const model = parseMarkdown(valid, false);
  assert.equal(model.blocks[0].embed.url, url);
  assert.ok(isProtected(model, {start:valid.indexOf(url),end:valid.indexOf(url)}));
  assert.equal(model.blocks.at(-1).runs[0].offsets[0], valid.length);
  assert.ok(!isProtected(model, {start:valid.length,end:valid.length}));
  for (const value of [valid.replace('sever-embed','markdown'), valid.replace(url,url+'\n'+url), valid.slice(0,-3)]) {
    const [block] = parseMarkdown(value, false).blocks;
    assert.equal(block.kind,'fence');
    assert.equal(block.embed,undefined);
    assert.equal(block.runs[0].text.includes(url),true);
  }
  assert.ok(!parseMarkdown(valid,true).blocks.some(block=>block.embed));
});

test('only the official TeamStream player enables the specialized video block', () => {
  const host = 'https://workspace.teamstream.mws.ru';
  const id = 'a1b2c3d4-0000-4000-8000-000000000001';
  const player = host + '/embed/' + id;
  for (const value of [player, player + '?t=0', player + '?t=00030', player + '?t=4294967295']) {
    assert.equal(externalEmbedAddress(value)?.kind, 'teamstream', value);
    assert.equal(externalEmbedAddress(value)?.url, value);
  }
  assert.equal(externalEmbedAddress(host + '/embed/' + id.toUpperCase())?.kind, 'teamstream');
  for (const value of [
    player + '?', player + '#', player + '#clip', player + '/',
    player + '?t=1&t=2', player + '?t=-1', player + '?t=1.5', player + '?%74=1',
    player + '?t=%31', player + '?t=4294967296', player + '?t=1&next=%2F',
    host + '/watch/' + id, host + '/EMBED/' + id, host + ':444/embed/' + id, host + './embed/' + id,
    'https://workspace.teamstream.mws.ru.example.test/embed/' + id,
  ]) assert.notEqual(externalEmbedAddress(value)?.kind, 'teamstream', value);
  const [block] = parseMarkdown('~~~ sever-embed\n' + player + '?t=30\n~~~').blocks;
  assert.equal(block.embed?.kind, 'teamstream');
  assert.equal(block.embed?.url, player + '?t=30');
});
