import {stages, scopes, horizons, priorities, kinds, gates, lanes, approvals, nextStages,
  plan, topology, waitingFor, localDay, safeLink, qaPassed, isFinished} from './core.mjs';

const $ = (selector) => document.querySelector(selector);
const esc = (value) => String(value ?? '').replace(/[&<>"']/g, (c) =>
  ({'&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;'}[c]));
const options = (values, selected = '') => Object.entries(values).map(([value, name]) =>
  `<option value="${esc(value)}"${selected === value ? ' selected' : ''}>${esc(name)}</option>`).join('');
const chip = (label, className = '') => `<span class="chip ${className}">${esc(label)}</span>`;
const day = localDay();
const dateFmt = new Intl.DateTimeFormat('ru-RU', {day: 'numeric', month: 'short', timeZone: 'Europe/Moscow'});
const timeFmt = new Intl.DateTimeFormat('ru-RU', {day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit', timeZone: 'Europe/Moscow'});
const date = (value) => value ? dateFmt.format(new Date(value + 'T00:00:00Z')) : '—';
const prettyTime = (value) => timeFmt.format(new Date(value));
const byId = (id) => state.features.find((feature) => feature.id === id);
const nav = {
  board: ['Доска фич', 'Статусы, согласования и следующая работа.'],
  backlog: ['Бэклог', 'Все фичи и задачи в одном реестре.'],
  roadmap: ['Роадмапа', 'Горизонты работ, плановые сроки и выпуски.'],
  deps: ['Зависимости', 'Что блокирует работу и определяет срок обязательного объёма.'],
};
const intervals = {now: 'В фокусе', next: 'Следом', later: 'В резерве'};
const approvalLabels = {pending: 'Ожидает', approved: 'Согласовано', stale: 'Устарело', skip: 'Не требуется'};
const stageGates = {requirements_review: 'requirements', design_review: 'design', ready_for_implementation: 'start', ready_for_release: 'release'};
const filterIds = ['status-filter', 'scope-filter', 'priority-filter', 'owner-filter', 'due-filter', 'block-filter'];
const drawer = $('#drawer');
let state, view = Object.hasOwn(nav, location.hash.slice(1)) ? location.hash.slice(1) : 'board';
let activeId, activeTab, editorRevision, toastTimer, author = 'Владелец';

$('#today').dateTime = day;
$('#today').textContent = new Intl.DateTimeFormat('ru-RU', {
  weekday: 'short', day: 'numeric', month: 'long', timeZone: 'Europe/Moscow',
}).format(new Date());
$('#status-filter').innerHTML = options({'': 'Все этапы', ...stages});
$('#scope-filter').innerHTML = options({'': 'Вся обязательность', ...scopes});
$('#priority-filter').innerHTML = options({'': 'Все приоритеты', ...priorities});
try { author = localStorage.getItem('feature-center-author') || author; } catch {}

function showToast(message) {
  const toast = $('#toast');
  toast.textContent = message;
  toast.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { toast.hidden = true; }, 3500);
}
async function load() {
  try {
    const response = await fetch('/api/features');
    if (!response.ok) throw new Error();
    state = await response.json();
    $('#connection').classList.remove('offline');
    $('#connection').innerHTML = '<i></i> Локальный реестр';
    render();
  } catch {
    $('#connection').classList.add('offline');
    $('#connection').innerHTML = '<i></i> Реестр недоступен';
    if (!state) $('#content').innerHTML = '<div class="empty">Реестр недоступен. Запустите локальный сервер и <button type="button" class="text-button" data-reload>повторите</button>.</div>';
    else showToast('Реестр недоступен. На экране последняя загруженная версия.');
  }
}
function isBlocked(f) { return !isFinished(f) && Boolean(f.blocked || waitingFor(f, state.features).length); }
function needsDecision(f) {
  const gate = stageGates[f.status];
  return Boolean(gate && ['pending', 'stale'].includes(approvals(f)[gate]));
}
function visibleFeatures() {
  const search = $('#query').value.trim().toLocaleLowerCase('ru');
  const [status, scope, priority, owner, due, blocked] = filterIds.map((id) => $('#' + id).value);
  const compare = new Intl.Collator('ru');
  const sort = $('#sort').value;
  return state.features.filter((f) => {
    if (search && ![f.title, f.id, f.description].some((field) => field.toLocaleLowerCase('ru').includes(search))) return false;
    if ((status && f.status !== status) || (scope && f.scope !== scope) || (priority && f.priority !== priority)) return false;
    if (owner && (f.owner || 'Без владельца') !== owner) return false;
    if (blocked && isBlocked(f) !== (blocked === 'blocked')) return false;
    if (due === 'today' && f.dueOn !== day) return false;
    if (due === 'overdue' && (!f.dueOn || f.dueOn >= day || isFinished(f))) return false;
    if (due === 'scheduled' && !f.dueOn) return false;
    if (due === 'unscheduled' && f.dueOn) return false;
    return true;
  }).sort((a, b) => {
    if (sort === 'updated') return b.updatedAt.localeCompare(a.updatedAt);
    if (sort === 'date') return (a.dueOn || '9999').localeCompare(b.dueOn || '9999');
    if (sort === 'title') return compare.compare(a.title, b.title);
    return a.priority.localeCompare(b.priority) || compare.compare(a.id, b.id);
  });
}
function metrics() {
  const features = state.features;
  const pending = features.filter(needsDecision).length;
  const stats = [
    ['Все карточки', features.length, 'slate', ''],
    ['Нужно решение', pending, 'amber', 'review'],
    ['В исполнении', features.filter((f) => ['implementation', 'qa', 'rework'].includes(f.status)).length, 'blue', 'active'],
    ['Обязательный объём', features.filter((f) => f.scope === 'must' && !isFinished(f)).length, 'violet', 'must'],
  ];
  return stats.map(([title, count, color, kind]) =>
    `<button type="button" class="metric" data-metric="${kind}" title="Открыть соответствующий список"><span><i class="dot ${color}"></i>${title}</span><strong>${count}</strong></button>`).join('');
}
function card(f) {
  const stale = Object.values(approvals(f)).includes('stale');
  return `<button type="button" class="feature" data-open="${esc(f.id)}">
    <span class="feature-top"><span class="feature-id">${esc(f.id)}</span>${chip(priorities[f.priority], f.priority)}</span>
    <strong>${esc(f.title)}</strong><span class="feature-desc">${esc(f.description || stages[f.status])}</span>
    <span class="chips">${chip(scopes[f.scope], 'scope-' + f.scope)}${isBlocked(f) ? chip('Блокировка', 'warn') : ''}${stale ? chip('Согласование устарело', 'warn') : ''}</span>
    <span class="feature-foot"><span><i class="person" aria-hidden="true"></i>${esc(f.owner || 'Без владельца')}</span><span>${f.dueOn ? date(f.dueOn) : esc(stages[f.status])}</span></span>
  </button>`;
}
function board(features) {
  return `<section class="board" aria-label="Этапы фич">${lanes.map((lane) => {
    const list = features.filter((f) => lane.status.includes(f.status));
    return `<section class="lane"><div class="lane-head"><i class="dot ${lane.color}"></i><h2>${lane.name}</h2><span class="count">${list.length}</span></div>
      <div class="lane-body">${list.map(card).join('') || '<p class="empty-lane">Нет карточек</p>'}</div></section>`;
  }).join('')}</section>`;
}
function backlog(features) {
  return `<div class="table-wrap"><table><thead><tr><th>Фича</th><th>Этап</th><th>Приоритет</th><th>Объём</th><th>Владелец</th><th>Срок</th></tr></thead>
    <tbody>${features.map((f) => `<tr><td><button type="button" class="row-title" data-open="${esc(f.id)}"><small>${esc(f.id)}</small>${esc(f.title)}</button></td>
      <td>${esc(stages[f.status])} ${isBlocked(f) ? chip('Блокировка', 'warn') : ''}</td><td>${chip(priorities[f.priority], f.priority)}</td>
      <td>${chip(scopes[f.scope], 'scope-' + f.scope)}</td><td>${esc(f.owner || '—')}</td><td>${date(f.dueOn)}</td></tr>`).join('')}</tbody></table></div>`;
}
function roadmap(features) {
  const dueToday = features.filter((f) => !isFinished(f) && [f.startOn, f.dueOn].includes(day));
  return `<div class="today-line"><b>Сегодня</b><span>${dueToday.length ? dueToday.map((f) =>
    `<button type="button" class="text-button" data-open="${esc(f.id)}">${esc(f.title)}</button>`).join(' ') : 'Запланированных на сегодня стартов и сроков нет.'}</span></div>
    <section class="roadmap">${Object.entries(horizons).map(([key, label]) => {
      const list = features.filter((f) => f.horizon === key && f.status !== 'closed');
      return `<section class="road-lane"><div class="road-head"><div><span>${intervals[key]}</span><h2>${label}</h2></div><span class="count">${list.length}</span></div>
      ${list.map((f) => `<button type="button" class="road-item" data-open="${esc(f.id)}">
        <span class="chips">${chip(scopes[f.scope], 'scope-' + f.scope)}${chip(priorities[f.priority], f.priority)}${f.critical ? chip('Бизнес-критично', 'warn') : ''}</span>
        <strong>${esc(f.title)}</strong><span>${esc(stages[f.status])}</span>
        <small>${f.releasedOn ? 'Выпущена ' + date(f.releasedOn) : f.startOn || f.dueOn ? date(f.startOn) + ' → ' + date(f.dueOn) : 'Даты не назначены'}${f.duration ? ' / ' + f.duration + ' дн.' : ''}</small></button>`).join('') || '<p class="empty-lane">Нет карточек</p>'}</section>`;
    }).join('')}</section>`;
}
function dependencies() {
  const features = state.features;
  const graph = topology(features);
  const schedule = plan(features);
  let summary;
  if (schedule.kind === 'missing') {
    summary = `<h2>Критический путь не рассчитан</h2><p>Добавьте оценки оставшейся длительности для обязательных фич и их предпосылок.</p><div class="chips">${schedule.missing.map((f) =>
      `<button type="button" class="tag-button" data-open="${esc(f.id)}">${esc(f.id)} ${esc(f.title)}</button>`).join('')}</div>`;
  } else if (schedule.kind === 'ready') {
    const rows = [...schedule.timings].sort(([, a], [, b]) => a.start - b.start);
    summary = `<h2>Критический путь · ${schedule.total} дн.</h2><p>Обязательные фичи и их незавершённые зависимости. Расчёт в относительных днях при доступной параллельной работе, без учёта загрузки, выходных и календарных ограничений.</p>
      <div class="timeline">${rows.map(([id, timing]) => `<div class="timeline-row"><button type="button" data-open="${esc(id)}" class="text-button">${esc(byId(id).title)}</button>
      <div class="track"><span class="${schedule.critical.has(id) ? 'critical' : ''}" style="left:${timing.start / schedule.total * 100}%;width:${(timing.end - timing.start) / schedule.total * 100}%"></span></div>
      <span>${timing.start}–${timing.end} дн.${schedule.critical.has(id) ? ' · критично' : ' · резерв ' + timing.slack}</span></div>`).join('')}</div>`;
  } else if (schedule.kind === 'empty') {
    summary = '<h2>Обязательный объём завершён или ещё не задан</h2><p>Отметьте карточки как обязательные, чтобы включить их в расчёт.</p>';
  } else {
    summary = '<h2>Проверьте зависимости</h2><p>В реестре есть цикл или недоступная ссылка. Расчёт приостановлен.</p>';
  }
  const depths = new Map();
  if (graph.kind === 'ok') for (const f of graph.order) {
    const depth = Math.max(0, ...f.deps.map((id) => 1 + (depths.get(id) || 0)));
    depths.set(f.id, depth);
  }
  const steps = [...new Set(depths.values())].sort((a, b) => a - b);
  return `<section class="path-panel">${summary}</section><h2 class="section-heading">Граф выполнения</h2>
    <div class="graph">${steps.map((step) => `<section class="graph-step"><h3>${step ? 'После зависимостей' : 'Без предпосылок'}</h3>
      ${features.filter((f) => depths.get(f.id) === step).map((f) => `<article>
      <button type="button" class="text-button" data-open="${esc(f.id)}"><small>${esc(f.id)}</small>${esc(f.title)}</button>
      ${f.deps.length ? `<p>Требует ${f.deps.map((id) => `<button class="tag-button" type="button" data-open="${esc(id)}">${esc(id)}</button>`).join(' ')}</p>` : ''}
      ${isBlocked(f) ? chip('Блокировка', 'warn') : ''}</article>`).join('')}</section>`).join('')}</div>`;
}
let metricFilter = '';
function render() {
  const [title, subtitle] = nav[view];
  $('#heading').textContent = title;
  $('#subtitle').textContent = subtitle;
  document.querySelectorAll('[data-view]').forEach((button) => {
    if (button.dataset.view === view) button.setAttribute('aria-current', 'page');
    else button.removeAttribute('aria-current');
  });
  $('#tools').hidden = view === 'deps';
  $('#advanced').hidden = view === 'deps' || $('#more').getAttribute('aria-expanded') === 'false';
  const owner = $('#owner-filter').value;
  $('#owner-filter').innerHTML = options(Object.fromEntries([['', 'Все владельцы'], ...Array.from(new Set(state.features.map((f) => f.owner || 'Без владельца'))).sort().map((o) => [o, o])]), owner);
  $('#clear').hidden = !($('#query').value || filterIds.some((id) => $('#' + id).value) || metricFilter);
  $('#metrics').innerHTML = metrics();
  if (view === 'deps') { $('#content').innerHTML = dependencies(); return; }
  let list = visibleFeatures();
  if (metricFilter === 'review') list = list.filter(needsDecision);
  if (metricFilter === 'active') list = list.filter((f) => ['implementation', 'qa', 'rework'].includes(f.status));
  if (metricFilter === 'must') list = list.filter((f) => f.scope === 'must' && !isFinished(f));
  const hint = metricFilter ? `<p class="result-hint">Подборка · ${{review: 'Нужно решение', active: 'В исполнении', must: 'Обязательный объём'}[metricFilter]}</p>` : '';
  $('#content').innerHTML = hint + (list.length ? {board, backlog, roadmap}[view](list) :
    '<div class="empty"><strong>Фичи не найдены</strong><p>Измените запрос или сбросьте фильтры.</p><button class="secondary" type="button" data-clear>Сбросить фильтры</button></div>');
}
function clearFilters() {
  filterIds.forEach((id) => { $('#' + id).value = ''; });
  $('#query').value = '';
  metricFilter = '';
  render();
}
function changeView(next) {
  view = next;
  location.hash = next;
  render();
}

const drawers = {summary: 'Карточка', artifacts: 'Артефакты', flow: 'Процесс и QA', history: 'История'};
function label(title, control) { return `<label class="field"><span>${title}</span>${control}</label>`; }
function input(name, value = '', attrs = '') {
  return `<input name="${name}" value="${esc(value)}" ${attrs}>`;
}
function textarea(name, value = '', attrs = '') {
  return `<textarea name="${name}" rows="3" ${attrs}>${esc(value)}</textarea>`;
}
function select(name, values, current, attrs = '') {
  return `<select name="${name}" ${attrs}>${options(values, current)}</select>`;
}
function authorInput() { return label('Автор записи', input('author', author, 'required maxlength="100"')); }
function submit(label, formName) {
  return `<div class="form-note" role="alert" hidden></div><div class="form-actions"><button class="primary" type="submit" name="command" value="${formName}">${label}</button></div>`;
}
function overview(f) {
  const existing = Boolean(f.id);
  const deps = state.features.filter((other) => other.id !== f.id);
  return `${existing ? `<p class="current-stage">${chip(stages[f.status])}${f.critical ? chip('Бизнес-критично', 'warn') : ''}${isBlocked(f) ? chip('Блокировка', 'warn') : ''}</p>` : ''}
  <form data-form="${existing ? 'update' : 'create'}">
    ${label('Название', input('title', f.title, 'required maxlength="140"'))}
    ${label('Проблема и ожидаемое поведение', textarea('description', f.description, 'maxlength="4000"'))}
    ${label('Критерии приёмки', textarea('criteria', f.criteria, 'maxlength="4000"'))}
    <div class="field-grid">
      ${label('Приоритет', select('priority', priorities, f.priority || 'p2'))}
      ${label('Обязательность', select('scope', scopes, f.scope || 'should'))}
      ${label('Горизонт', select('horizon', horizons, f.horizon || 'next'))}
      ${label('Владелец', input('owner', f.owner, 'maxlength="100"'))}
      ${label('Назначенная роль', input('role', f.role, 'maxlength="100"'))}
      ${label('Осталось дней', input('duration', f.duration, 'type="number" min="1" max="10000"'))}
      ${label('Плановый старт', input('startOn', f.startOn, 'type="date"'))}
      ${label('Плановый срок', input('dueOn', f.dueOn, 'type="date"'))}
    </div>
    <div class="check-row"><label><input type="checkbox" name="ux" ${f.ux !== false ? 'checked' : ''}> Нужны макеты</label>
    <label><input type="checkbox" name="critical" ${f.critical ? 'checked' : ''}> Бизнес-критично</label></div>
    ${label('Жёсткий срок и основание', input('deadlineNote', f.deadlineNote, 'maxlength="300"'))}
    ${label('Причина блокировки', input('blocked', f.blocked, 'maxlength="600"'))}
    ${deps.length ? `<fieldset class="dep-picker"><legend>Зависит от</legend>${deps.map((other) =>
      `<label><input type="checkbox" name="deps" value="${esc(other.id)}" ${f.deps?.includes(other.id) ? 'checked' : ''}><span>${esc(other.id)} · ${esc(other.title)}</span></label>`).join('')}</fieldset>` : ''}
    ${authorInput()}${submit(existing ? 'Сохранить карточку' : 'Создать фичу', 'save')}
  </form>`;
}
function hrefLink(url, name) {
  return safeLink(url) ? `<a href="${esc(url)}" target="_blank" rel="noopener noreferrer">${esc(name)} <span aria-hidden="true">↗</span></a>` : esc(name);
}
function artifacts(f) {
  return `<div class="artifact-list">${f.artifacts.map((a, index) => `<article class="record">
    <div><small>${esc(kinds[a.kind])} · ${esc(a.revision)}</small><strong>${hrefLink(a.href, a.label)}</strong></div>
    <button type="button" class="secondary" data-edit-artifact="${index}">Изменить</button></article>`).join('') || '<p class="muted">Прикрепите спецификацию, макет, контракт или другие материалы.</p>'}</div>
    <div id="artifact-editor"></div>`;
}
function artifactForm(f, index) {
  const a = f.artifacts[index] || {};
  return `<h3>${Number.isInteger(index) ? 'Изменить артефакт' : 'Добавить артефакт'}</h3><form data-form="artifact" data-index="${index ?? ''}">
    <div class="field-grid">${label('Тип', select('kind', kinds, a.kind || 'spec'))}
    ${label('Редакция', input('revision', a.revision, 'required maxlength="100"'))}</div>
    ${label('Название', input('label', a.label, 'required maxlength="120"'))}
    ${label('Ссылка', input('href', a.href, 'required maxlength="1500" placeholder="https://… или /docs/…md"'))}
    ${authorInput()}${submit(Number.isInteger(index) ? 'Сохранить артефакт' : 'Добавить артефакт', 'artifact')}
  </form>`;
}
function flow(f) {
  const states = approvals(f);
  const stageGate = stageGates[f.status];
  const blocked = waitingFor(f, state.features);
  const next = nextStages(f);
  return `<p class="current-stage">${chip(stages[f.status])}${f.releasedOn ? chip('Выпущена ' + date(f.releasedOn), 'good') : ''}</p>
    ${f.closure ? `<p class="notice">${esc(f.closure.note)} <small>${esc(f.closure.author)} · ${prettyTime(f.closure.at)}</small></p>` : ''}
    ${f.releaseEvidence ? `<p class="notice">${hrefLink(f.releaseEvidence, f.releaseEvidence)}</p>` : ''}
    ${f.blocked ? `<p class="notice">${esc(f.blocked)}</p>` : ''}
    ${blocked.length && !isFinished(f) ? `<p class="notice">Ожидает завершения ${blocked.map((other) => `<button type="button" class="text-button" data-open="${esc(other.id)}">${esc(other.id)} ${esc(other.title)}</button>`).join(', ')}</p>` : ''}
    <h3>Согласования владельца</h3>
    <div class="gate-list">${Object.entries(gates).map(([id, name]) => {
      const sign = f.approvals[id];
      return `<article class="record"><div><strong>${name}</strong>${sign ? `<small>${esc(sign.author)} · ${prettyTime(sign.at)}</small><p>${esc(sign.note)}</p>` : ''}</div>
        ${chip(approvalLabels[states[id]], states[id] === 'approved' ? 'good' : states[id] === 'stale' ? 'warn' : '')}</article>`;
    }).join('')}</div>
    ${stageGate ? `<details class="action"><summary>Записать согласование · ${gates[stageGate]}</summary><form data-form="approve" data-gate="${stageGate}">
      ${label('Решение владельца или ссылка на подтверждение', textarea('note', '', 'required maxlength="1200"'))}
      ${authorInput()}${submit('Записать согласование', 'approve')}</form></details>` : ''}
    ${next.length ? `<details class="action" open><summary>Сменить этап</summary><form data-form="move">
      ${label('Следующий этап', select('status', Object.fromEntries(next.map((s) => [s, stages[s]])), next[0]))}
      <div id="release-evidence" hidden>${label('Ссылка или запись о фактическом выпуске', input('evidence', '', 'maxlength="1000"'))}</div>
      ${authorInput()}${submit('Перевести фичу', 'move')}</form></details>` : ''}
    <h3>Раунды QA ${f.qa.length ? chip(String(f.qa.length)) : ''}</h3>
    <div class="qa-list">${f.qa.slice().reverse().map((q) => `<article class="record"><div><strong>${hrefLink(q.href, q.version)}</strong>
      <small>Раунд ${q.cycle} · ${esc(q.author)} · ${prettyTime(q.at)}</small><p>${esc(q.notes)}</p>
      <small>Пройдено ${q.passed}/${q.required} · открытых ошибок ${q.open}${q.blockers ? ' · есть блокировки' : ''}</small></div></article>`).join('') || '<p class="muted">Отчётов пока нет.</p>'}</div>
    ${f.status === 'qa' ? `<p class="muted">${qaPassed(f) ? 'Последний отчёт подтверждает допуск QA.' : 'Для допуска нужны все проверки, ноль открытых ошибок и отсутствие блокировок.'}</p>
      <details class="action"><summary>Добавить отчёт QA</summary><form data-form="qa">
      ${label('Версия кандидата', input('version', '', 'required maxlength="100"'))}
      ${label('Ссылка на отчёт', input('href', '', 'required maxlength="1500"'))}
      ${label('Покрытие и результат', textarea('notes', '', 'required maxlength="3000"'))}
      <div class="field-grid three">${label('Обязательных', input('required', '', 'type="number" required min="1" max="10000"'))}
      ${label('Пройдено', input('passed', '', 'type="number" required min="0" max="10000"'))}
      ${label('Открытых дефектов', input('open', '', 'type="number" required min="0" max="10000"'))}</div>
      <label class="check-row"><input type="checkbox" name="blockers"> Есть недоступные проверки или нерешённые подозрения</label>
      ${authorInput()}${submit('Сохранить отчёт', 'qa')}</form></details>` : ''}`;
}
function history(f) {
  return `<ol class="history">${f.history.slice().reverse().map((e) => `<li><strong>${esc(e.event)}</strong>
    <span>${esc(e.author)} · ${prettyTime(e.at)}</span></li>`).join('')}</ol>`;
}
function renderDrawer(focus = false) {
  const f = activeId ? byId(activeId) : {};
  if (!f) { drawer.close(); return; }
  $('#drawer-id').textContent = f.id || 'Бэклог';
  $('#drawer-title').textContent = f.title || 'Новая фича';
  $('#tabs').hidden = !activeId;
  $('#tabs').innerHTML = Object.entries(drawers).map(([id, name]) =>
    `<button type="button" data-tab="${id}" aria-pressed="${id === activeTab}">${name}</button>`).join('');
  $('#drawer-body').innerHTML = {summary: overview, artifacts, flow, history}[activeTab](f);
  if (activeTab === 'artifacts') $('#artifact-editor').innerHTML = artifactForm(f);
  if (activeTab === 'flow') {
    const change = $('#drawer-body').querySelector('select[name="status"]');
    if (change) {
      const reveal = () => {
        const visible = change.value === 'released';
        $('#release-evidence').hidden = !visible;
        $('#release-evidence input').required = visible;
      };
      change.addEventListener('change', reveal);
      reveal();
    }
  }
  if (focus) $('#drawer-body').querySelector('[name="title"]')?.focus();
}
function openDrawer(id = null, tab = 'summary') {
  activeId = id;
  activeTab = tab;
  editorRevision = state.revision;
  if (!drawer.open) drawer.showModal();
  renderDrawer(true);
}

function makeCommand(form) {
  const fields = new FormData(form);
  const get = (name) => String(fields.get(name) ?? '');
  const action = form.dataset.form;
  const f = activeId ? byId(activeId) : null;
  const command = {action, id: activeId, author: get('author'), revision: editorRevision};
  if (action === 'create' || action === 'update') {
    const entries = ['title', 'description', 'criteria', 'priority', 'scope', 'horizon', 'owner', 'role',
      'startOn', 'dueOn', 'deadlineNote', 'blocked'];
    command.data = Object.fromEntries(entries.map((name) => [name, get(name)]));
    Object.assign(command.data, {duration: fields.get('duration') ? Number(fields.get('duration')) : null,
      ux: fields.has('ux'), critical: fields.has('critical'), deps: fields.getAll('deps')});
  }
  if (action === 'artifact') {
    const updated = {kind: get('kind'), label: get('label'), href: get('href'), revision: get('revision')};
    const list = f.artifacts.slice();
    if (form.dataset.index === '') list.push(updated);
    else list[Number(form.dataset.index)] = updated;
    Object.assign(command, {action: 'update', data: {artifacts: list}});
  }
  if (action === 'move') Object.assign(command, {status: get('status'), evidence: get('evidence')});
  if (action === 'approve') Object.assign(command, {gate: form.dataset.gate, note: get('note')});
  if (action === 'qa') command.report = {
    version: get('version'), href: get('href'), notes: get('notes'),
    required: Number(get('required')), passed: Number(get('passed')), open: Number(get('open')), blockers: fields.has('blockers'),
  };
  return command;
}
async function save(form) {
  const error = form.querySelector('.form-note');
  const button = form.querySelector('button[type="submit"]');
  const command = makeCommand(form);
  const openedId = activeId;
  error.hidden = true;
  button.disabled = true;
  try {
    const response = await fetch('/api/features', {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(command)});
    const result = await response.json();
    if (response.status === 409) {
      state = result.state;
      render();
      throw new Error('Реестр изменился. Введённый текст сохранён в форме. Скопируйте нужные поля, закройте карточку и откройте актуальную редакцию.');
    }
    if (!response.ok) throw new Error(result.error || 'Запись не сохранена.');
    state = result;
    author = command.author.trim();
    try { localStorage.setItem('feature-center-author', author); } catch {}
    render();
    if (drawer.open && activeId === openedId) {
      editorRevision = state.revision;
      if (command.action === 'create') {
        activeId = state.features.at(-1).id;
        activeTab = 'summary';
      }
      renderDrawer();
    }
    showToast(command.action === 'create' ? 'Фича создана' : 'Изменения сохранены');
  } catch (err) {
    error.textContent = err instanceof TypeError ? 'Реестр недоступен. Проверьте сервер и повторите сохранение.' : err.message;
    error.hidden = false;
    error.scrollIntoView({block: 'nearest'});
  } finally { button.disabled = false; }
}

document.addEventListener('click', (event) => {
  const target = event.target.closest('button');
  if (!target) return;
  if (target.hasAttribute('data-reload')) load();
  if (!state) return;
  if (target.dataset.view) { metricFilter = ''; changeView(target.dataset.view); }
  if (target.dataset.open) openDrawer(target.dataset.open);
  if (target.hasAttribute('data-metric')) {
    clearFilters();
    metricFilter = target.dataset.metric;
    changeView('backlog');
  }
  if (target.id === 'new') openDrawer();
  if (target.id === 'close') drawer.close();
  if (target.id === 'clear' || target.hasAttribute('data-clear')) clearFilters();
  if (target.id === 'more') {
    const next = target.getAttribute('aria-expanded') === 'false';
    target.setAttribute('aria-expanded', String(next));
    $('#advanced').hidden = !next;
  }
  if (target.dataset.tab) { activeTab = target.dataset.tab; renderDrawer(); }
  if (target.hasAttribute('data-edit-artifact')) {
    $('#artifact-editor').innerHTML = artifactForm(byId(activeId), Number(target.dataset.editArtifact));
    $('#artifact-editor').querySelector('[name="kind"]').focus();
  }
});
document.addEventListener('submit', (event) => {
  if (!event.target.matches('form[data-form]')) return;
  event.preventDefault();
  save(event.target);
});
$('#query').addEventListener('input', () => { if (state) render(); });
[...filterIds, 'sort'].forEach((id) => $('#' + id).addEventListener('change', () => { if (state) render(); }));
window.addEventListener('hashchange', () => {
  const next = location.hash.slice(1);
  if (state && Object.hasOwn(nav, next) && next !== view) { metricFilter = ''; view = next; render(); }
});
window.addEventListener('focus', () => { if (!drawer.open) load(); });
load();
