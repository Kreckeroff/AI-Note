import assert from 'node:assert/strict';
import test from 'node:test';
import {chooseCoverDraft, isCoverUploadPath, isCoverView, resolveCoverDraft, uploadedCover} from './pageCover.ts';

const orbit = {value: 'orbit', imageTo: null};
const aurora = {value: 'aurora', imageTo: null};
const grid = {value: 'grid', imageTo: null};
const none = {value: 'none', imageTo: null};
const id = '12345678-1234-1234-1234-123456789abc';
const otherId = 'fedcba98-7654-3210-4321-abcdef987654';
const uploadTo = `/workspace/${id}/spaces/${id}/pages/${id}/cover`;
const custom = {value: `image:${id}`, imageTo: `${uploadTo}/${id}`};

test('an unchanged cover follows the published page after the user rebases', () => {
  const draft = {cover: orbit, base: 'orbit', changed: false};
  assert.deepEqual(resolveCoverDraft(draft, aurora, null, false), {
    preview: orbit, cover: null, base: 'orbit', requiresChoice: false,
  });
  assert.deepEqual(resolveCoverDraft(draft, aurora, null, true), {
    preview: aurora, cover: null, base: 'aurora', requiresChoice: false,
  });
});

test('a local cover removal survives an unrelated text conflict', () => {
  assert.deepEqual(resolveCoverDraft({cover: none, base: 'orbit', changed: true}, orbit, null, true), {
    preview: none, cover: 'none', base: 'orbit', requiresChoice: false,
  });
});

test('different cover changes require an explicit choice and use the latest base', () => {
  const draft = {cover: grid, base: 'orbit', changed: true};
  assert.deepEqual(resolveCoverDraft(draft, aurora, null, true), {
    preview: grid, cover: 'grid', base: 'aurora', requiresChoice: true,
  });
  assert.deepEqual(resolveCoverDraft(draft, aurora, 'draft', true), {
    preview: grid, cover: 'grid', base: 'aurora', requiresChoice: true,
  });
  assert.deepEqual(resolveCoverDraft(draft, aurora, 'published', true), {
    preview: aurora, cover: null, base: 'aurora', requiresChoice: true,
  });
  assert.deepEqual(resolveCoverDraft(draft, grid, null, true), {
    preview: grid, cover: null, base: 'grid', requiresChoice: false,
  });
});

test('a deliberate gallery choice on the conflict screen is based on the published cover', () => {
  const original = {cover: orbit, base: 'orbit', changed: false};
  const selected = chooseCoverDraft(original, orbit, aurora);
  for (const rebase of [false, true]) {
    assert.deepEqual(resolveCoverDraft(selected, aurora, null, rebase), {
      preview: orbit, cover: 'orbit', base: 'aurora', requiresChoice: false,
    });
  }

  assert.deepEqual(resolveCoverDraft(chooseCoverDraft(original, aurora, aurora), aurora, null, true), {
    preview: aurora, cover: null, base: 'aurora', requiresChoice: false,
  });
});

test('covers accept only known presets and the exact local image path', () => {
  assert.equal(isCoverView(orbit), true);
  assert.equal(isCoverView(custom), true);
  assert.equal(isCoverView({value: 'orbit', imageTo: '/elsewhere'}), false);
  assert.equal(isCoverView({value: 'unknown', imageTo: null}), false);
  assert.equal(isCoverView({...custom, imageTo: `${uploadTo}/${otherId}`}), false);
  assert.equal(isCoverView({...custom, imageTo: `https://example.org${custom.imageTo}`}), false);
  assert.equal(isCoverUploadPath(uploadTo), true);
  assert.equal(isCoverUploadPath(`${uploadTo}?other=1`), false);
  assert.deepEqual(uploadedCover(custom, uploadTo), custom);
  assert.equal(uploadedCover(custom, uploadTo.replace(`/pages/${id}`, `/pages/${otherId}`)), undefined);
  assert.equal(uploadedCover(orbit, uploadTo), undefined);
});
