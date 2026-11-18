import test from 'node:test';
import assert from 'node:assert/strict';
import {apply, approvals, plan, waitingFor} from './public/core.mjs';

function command(state, action, fields = {}) {
  return apply(state, {action, id: 'FC-001', author: 'Владелец', ...fields}, '2026-11-17T10:00:00Z');
}
const doc = (kind, revision = 'v1') => ({kind, label: kind, href: 'https://example.org/doc', revision});

test('implementation and release require current approvals and a passing QA round', () => {
  let s = {revision: 1, nextId: 1, features: []};
  s = command(s, 'create', {data: {
    title: 'Фича', description: 'Поведение', criteria: 'Критерий', ux: false,
    artifacts: [doc('spec'), doc('contract'), doc('tests'), doc('release')],
  }});
  const move = (status) => {s = command(s, 'move', {status});};
  const approve = (gate) => {s = command(s, 'approve', {gate, note: 'Подтверждено владельцем'});};
  move('discovery');
  move('requirements_review');
  assert.throws(() => move('technical_design'), /требовани/i);
  approve('requirements');
  move('technical_design');
  move('ready_for_implementation');
  assert.throws(() => move('implementation'), /старт/i);
  approve('start');
  move('implementation');
  move('qa');
  s = command(s, 'qa', {report: {version: '001', href: 'https://example.org/qa', notes: 'Ожидание', required: 2, passed: 1, open: 0, blockers: false}});
  assert.throws(() => move('ready_for_release'), /QA/);
  s = command(s, 'qa', {report: {version: '001', href: 'https://example.org/qa', notes: 'Полный прогон', required: 2, passed: 2, open: 0, blockers: false}});
  move('ready_for_release');
  approve('release');
  const approved = s;
  assert.throws(() => move('released'), /фактический выпуск/i);
  const released = command(s, 'move', {status: 'released', evidence: 'Выпуск подтверждён'});
  assert.equal(released.features[0].releasedOn, '2026-11-17');
  assert.equal(released.features[0].releaseEvidence, 'Выпуск подтверждён');
  s = approved;
  s = command(s, 'update', {data: {criteria: 'Новый критерий'}});
  assert.equal(approvals(s.features[0]).requirements, 'stale');
  assert.equal(approvals(s.features[0]).release, 'stale');
  assert.throws(() => move('released'), /релиз/i);
});

test('CPM includes prerequisites, finds tied critical paths, checks unknowns and cycles', () => {
  const task = (id, duration, deps = [], scope = 'must') => ({id, duration, deps, scope, status: 'idea'});
  const tasks = [task('A', 2, [], 'could'), task('B', 3, ['A']), task('C', 3, ['A']), task('D', 1, ['B', 'C']), task('E', null, [], 'could')];
  const result = plan(tasks);
  assert.equal(result.total, 6);
  assert.deepEqual([...result.critical].sort(), ['A', 'B', 'C', 'D']);
  assert.equal(plan([...tasks, task('F', null)]).kind, 'missing');
  assert.equal(plan([task('A', 1, ['B']), task('B', 1, ['A'])]).kind, 'cycle');
});

test('unknown dependencies and cycles are rejected with the original state intact', () => {
  let s = {revision: 1, nextId: 1, features: []};
  s = command(s, 'create', {data: {title: 'Первая'}});
  s = command(s, 'create', {data: {title: 'Вторая', deps: ['FC-001']}});
  assert.throws(() => command(s, 'update', {data: {deps: ['FC-002']}}), /цикл/i);
  assert.throws(() => command(s, 'update', {data: {deps: ['FC-999']}}), /зависим/i);
  assert.deepEqual(s.features[0].deps, []);
});

test('owner-directed closure preserves history and removes the card from active planning', () => {
  let s = {revision: 1, nextId: 1, features: []};
  s = command(s, 'create', {data: {title: 'Подготовка', scope: 'must'}});
  s = command(s, 'create', {data: {title: 'Контекст продукта', scope: 'must', duration: 2, deps: ['FC-001']}});
  assert.throws(() => command(s, 'close'), /причину закрытия/i);
  s = command(s, 'close', {note: 'Закрыто по решению владельца'});
  const [closed, active] = s.features;
  assert.equal(closed.status, 'closed');
  assert.equal(closed.closure.previousStatus, 'idea');
  assert.equal(closed.releasedOn, '');
  assert.equal(closed.history.length, 2);
  assert.deepEqual(waitingFor(active, s.features), []);
  assert.equal(plan(s.features).total, 2);
  assert.equal(command(s, 'close', {note: closed.closure.note}), s);
});
