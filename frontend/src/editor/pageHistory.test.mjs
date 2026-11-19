import assert from 'node:assert/strict';
import test from 'node:test';
import {historySnapshotHref, isPageHistoryPath, readPageHistory} from './pageHistory.ts';

const tenantId = '11111111-1111-1111-1111-111111111111';
const spaceId = '22222222-2222-2222-2222-222222222222';
const pageId = '33333333-3333-3333-3333-333333333333';
const endpoint = `/workspace/${tenantId}/spaces/${spaceId}/pages/${pageId}/revisions`;
const currentRevision = '9223372036854775807';
const lastRevision = '9223372036854775806';
const row = (revision, author = 'Анна', recordedAt = '2026-11-19T10:15:00Z') => ({
  revision, author, recordedAt, href: `${endpoint}/${revision}`,
});

test('history accepts exact local routes and keeps revision numbers beyond the JavaScript number range', () => {
  const response = {
    currentRevision,
    items: [row(currentRevision), row(lastRevision, null, null)],
    nextTo: `${endpoint}?before=${lastRevision}`,
  };
  assert.equal(isPageHistoryPath(endpoint), true);
  assert.equal(isPageHistoryPath(`${endpoint}?preview=1`), false);
  assert.equal(isPageHistoryPath(`https://example.test${endpoint}`), false);
  assert.deepEqual(readPageHistory(response, endpoint), response);
});

test('history rejects malformed metadata and links outside the active document', () => {
  const page = {currentRevision: '4', items: [row('4'), row('3')], nextTo: null};
  const wrongPage = endpoint.replace(`/pages/${pageId}`, `/pages/${spaceId}`);
  for (const value of [
    {...page, items: [{...row('4'), href: `${wrongPage}/4`}]},
    {...page, items: [{...row('4'), href: `${endpoint}/3`}]},
    {...page, items: [{...row('4'), href: `//example.test${endpoint}/4`}]},
    {...page, items: [row('4', 'Анна', 'это не дата')]},
    {...page, items: [row('4'), row('4')]},
    {...page, items: [row('3'), row('4')]},
    {...page, items: [row('04')]},
    {...page, items: [row('5')]},
    {...page, currentRevision: '9223372036854775808'},
    {...page, nextTo: `${wrongPage}?before=3`},
    {...page, nextTo: `${endpoint}?before=4`},
    {...page, nextTo: `${endpoint}?before=3&preview=1`},
  ]) {
    assert.equal(readPageHistory(value, endpoint), undefined);
  }
});

test('reading opens the stable snapshot while editing uses the read-only preview', () => {
  const href = row('4').href;
  assert.equal(historySnapshotHref(href, false), href);
  assert.equal(historySnapshotHref(href, true), `${href}?preview=1`);
});
