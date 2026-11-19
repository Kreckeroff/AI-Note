import assert from 'node:assert/strict';
import test from 'node:test';
import {createDraftHistory} from './draftHistory.ts';

const frame = (source, start = source.length, end = start, contentFormat = 'literal') =>
  ({source, contentFormat, selection: {start, end}});

test('undo and redo restore exact Markdown, format and selections across actions', () => {
  const history = createDraftHistory();
  const initial = frame('важно', 0, 5);
  const bold = frame('**важно**', 2, 7, 'markdown');
  const inserted = frame('**важно**\n\n- дело', 17, 17, 'markdown');
  history.record(initial, bold);
  history.record(bold, inserted);
  assert.deepEqual(history.step('undo'), bold);
  assert.deepEqual(history.step('undo'), initial);
  assert.equal(history.step('undo'), undefined);
  assert.deepEqual(history.step('redo'), bold);
  assert.deepEqual(history.step('redo'), inserted);
});

test('ordinary contiguous typing coalesces, atomic operations and a moved caret split it', () => {
  const history = createDraftHistory();
  history.record(frame(''), frame('а'), 'typing', 100);
  history.record(frame('а'), frame('аб'), 'typing', 110);
  history.record(frame('аб', 0), frame('ваб', 1), 'typing', 120);
  history.record(frame('ваб'), frame('вабг'));
  assert.deepEqual(history.step('undo'), frame('ваб'));
  assert.deepEqual(history.step('undo'), frame('аб', 0));
  assert.deepEqual(history.step('undo'), frame(''));
});

test('unchanged operations preserve the redo branch but a new edit replaces it', () => {
  const history = createDraftHistory();
  history.record(frame(''), frame('а'));
  history.record(frame('а'), frame('аб'));
  history.step('undo');
  history.record(frame('а'), frame('а', 0));
  assert.deepEqual(history.step('redo'), frame('аб'));
  history.step('undo');
  history.record(frame('а'), frame('ав'));
  assert.equal(history.step('redo'), undefined);
  assert.deepEqual(history.step('undo'), frame('а'));
});

test('attachment-only operations and image insertion preserve the complete draft through undo and redo', () => {
  const history = createDraftHistory();
  const before = {...frame('Текст'), attachments: {files: [], added: [], removed: []}};
  const withFile = {...before, attachments: {files: [{id:'image'}], added:['image'], removed:[]}};
  const withImage = {...frame('Текст и картинка'), attachments: withFile.attachments};
  history.record(before, withFile);
  history.record(withFile, withImage);
  assert.deepEqual(history.step('undo'), withFile);
  assert.deepEqual(history.step('undo'), before);
  assert.deepEqual(history.step('redo'), withFile);
  assert.deepEqual(history.step('redo'), withImage);
});
