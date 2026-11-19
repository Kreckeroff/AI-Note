import assert from 'node:assert/strict';
import test from 'node:test';
import {findSlashCommand} from './slashCommand.ts';

const run = (text, offsets, extra = {}) => ({text, offsets, marks: [], locked: false, ...extra});

test('slash search maps displayed query back to its escaped source range', () => {
  const visible = run('До /ци', [10, 11, 12, 13, 15, 16, 17]);
  assert.deepEqual(findSlashCommand([visible], 17), {query: 'ци', range: {start: 13, end: 17}});
});

test('URL, code, locked text and text adjacent to another run do not trigger a menu', () => {
  const url = run('https://x', Array.from({length: 10}, (_, i) => i));
  assert.equal(findSlashCommand([url], 9), undefined);
  assert.equal(findSlashCommand([run('/a', [3, 4, 5], {marks: [{kind: 'code'}]})], 5), undefined);
  assert.equal(findSlashCommand([run('/a', [3, 4, 5], {locked: true})], 5), undefined);
  assert.equal(findSlashCommand([run('a', [0, 1]), run('/b', [3, 4, 5])], 5), undefined);
});
