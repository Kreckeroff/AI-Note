export const stages = {
  idea: 'Идея', discovery: 'Аналитика', requirements_review: 'Согласование требований',
  design: 'Дизайн', design_review: 'Согласование макетов', technical_design: 'Технический дизайн',
  ready_for_implementation: 'Готова к старту', implementation: 'Разработка', qa: 'Независимый QA',
  rework: 'Исправления', ready_for_release: 'Готова к релизу', released: 'Выпущена', closed: 'Закрыта',
};
export const scopes = {must: 'Обязательно', should: 'Желательно', could: 'Опционально', later: 'Отложено'};
export const horizons = {now: 'Сейчас', next: 'Далее', later: 'Позже'};
export const priorities = {p0: 'P0', p1: 'P1', p2: 'P2', p3: 'P3'};
export const kinds = {
  spec: 'Спецификация', design: 'Макет', contract: 'Контракт / ADR', tests: 'План проверок',
  qa: 'Отчёт QA', release: 'Пакет релиза', other: 'Документ',
};
export const gates = {requirements: 'Требования', design: 'Макеты', start: 'Старт реализации', release: 'Релиз'};
export const lanes = [
  {name: 'Бэклог', status: ['idea'], color: 'slate'},
  {name: 'Проработка', status: ['discovery', 'design', 'technical_design'], color: 'violet'},
  {name: 'На согласовании', status: ['requirements_review', 'design_review', 'ready_for_implementation'], color: 'amber'},
  {name: 'Исполнение', status: ['implementation', 'qa', 'rework', 'ready_for_release'], color: 'blue'},
  {name: 'Завершено', status: ['released', 'closed'], color: 'green'},
];

const transitions = {
  idea: ['discovery'], discovery: ['requirements_review', 'idea'],
  requirements_review: ['design', 'technical_design', 'discovery'],
  design: ['design_review', 'requirements_review'], design_review: ['technical_design', 'design', 'requirements_review'],
  technical_design: ['ready_for_implementation', 'requirements_review', 'design_review'],
  ready_for_implementation: ['implementation', 'technical_design', 'requirements_review', 'design_review'],
  implementation: ['qa', 'technical_design', 'requirements_review', 'design_review'],
  qa: ['ready_for_release', 'rework', 'technical_design', 'requirements_review', 'design_review'],
  rework: ['qa', 'technical_design', 'requirements_review', 'design_review'],
  ready_for_release: ['released', 'rework', 'qa', 'technical_design', 'requirements_review', 'design_review'],
  released: [], closed: [],
};
const gateStages = {
  requirements: 'requirements_review', design: 'design_review',
  start: 'ready_for_implementation', release: 'ready_for_release',
};
const assert = (condition, message) => { if (!condition) throw new Error(message); };
const text = (value, max = 4000) => {
  assert(typeof value === 'string' && value.length <= max, 'Проверьте длину и формат полей.');
  return value.trim();
};
const choice = (value, options) => {
  assert(Object.hasOwn(options, value), 'Выберите значение из списка.');
  return value;
};
const flag = (value) => {
  assert(typeof value === 'boolean', 'Ожидалось значение да или нет.');
  return value;
};
const number = (value, min, max = 10000) => {
  assert(Number.isInteger(value) && value >= min && value <= max, 'Укажите целое число в допустимом диапазоне.');
  return value;
};
const day = (value) => {
  if (value === '') return value;
  assert(typeof value === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(value), 'Укажите корректную дату.');
  const date = new Date(value);
  assert(!Number.isNaN(+date) && date.toISOString().slice(0, 10) === value, 'Укажите корректную дату.');
  return value;
};
export const localDay = (value = new Date()) => new Intl.DateTimeFormat('en-CA', {
  timeZone: 'Europe/Moscow', year: 'numeric', month: '2-digit', day: '2-digit',
}).format(new Date(value));

export function safeLink(value) {
  if (typeof value !== 'string' || /[\s\\]/.test(value)) return false;
  if (/^\/docs\/(?:[a-z0-9_-]+\/)*[a-z0-9_-]+\.md$/i.test(value)) return true;
  try {
    const url = new URL(value);
    return ['http:', 'https:'].includes(url.protocol) && !url.username && !url.password;
  } catch { return false; }
}
const link = (value) => {
  value = text(value, 1500);
  assert(safeLink(value), 'Укажите ссылку http(s) или путь /docs/… к Markdown-документу.');
  return value;
};
const artifact = (value) => {
  assert(value && typeof value === 'object' && !Array.isArray(value), 'Укажите данные артефакта.');
  const label = text(value.label, 120);
  const revision = text(value.revision, 100);
  assert(label && revision, 'Укажите название и редакцию артефакта.');
  return {kind: choice(value.kind, kinds), label, href: link(value.href), revision};
};
function artifactKeys(feature, types) {
  return feature.artifacts.filter((a) => types.includes(a.kind))
    .map((a) => JSON.stringify([a.kind, a.href, a.revision])).sort();
}
function gateKey(feature, gate) {
  const requirements = [feature.title, feature.description, feature.criteria, feature.ux, artifactKeys(feature, ['spec'])];
  const design = [requirements, artifactKeys(feature, ['design'])];
  const start = [design, feature.deps.slice().sort(), artifactKeys(feature, ['contract', 'tests'])];
  if (gate === 'requirements') return JSON.stringify(requirements);
  if (gate === 'design') return JSON.stringify(design);
  if (gate === 'start') return JSON.stringify(start);
  return JSON.stringify([start, feature.qa.at(-1) || null, artifactKeys(feature, ['release'])]);
}
export function approvals(feature) {
  return Object.fromEntries(Object.keys(gates).map((gate) => {
    const saved = feature.approvals[gate];
    const status = gate === 'design' && !feature.ux ? 'skip'
      : !saved ? 'pending' : saved.key === gateKey(feature, gate) ? 'approved' : 'stale';
    return [gate, status];
  }));
}
export function qaPassed(feature) {
  const report = feature.qa.at(-1);
  return Boolean(report && report.cycle === feature.qaCycle && report.startKey === gateKey(feature, 'start')
    && report.passed === report.required && report.open === 0 && !report.blockers);
}
export function isFinished(feature) {
  return feature.status === 'released' || feature.status === 'closed';
}
export function waitingFor(feature, features) {
  return feature.deps.map((id) => features.find((candidate) => candidate.id === id))
    .filter((candidate) => candidate && !isFinished(candidate));
}
export function nextStages(feature) {
  return (transitions[feature.status] || []).filter((s) =>
    (s !== 'design' && s !== 'design_review') || feature.ux).filter((s) =>
    feature.status !== 'requirements_review' || s !== 'technical_design' || !feature.ux);
}
function move(feature, target, all, at, evidence) {
  assert(nextStages(feature).includes(target), 'Этот переход недоступен из текущего этапа.');
  const valid = approvals(feature);
  if (['design', 'technical_design', 'ready_for_implementation'].includes(target) &&
      ['requirements_review', 'design_review', 'technical_design', 'ready_for_implementation'].includes(feature.status)) {
    assert(valid.requirements === 'approved', 'Согласуйте текущую редакцию требований.');
    if (feature.ux && ['technical_design', 'ready_for_implementation'].includes(target)) {
      assert(valid.design === 'approved', 'Согласуйте текущую редакцию макетов.');
    }
  }
  const enteringQA = target === 'qa';
  if (target === 'implementation' || enteringQA) {
    assert(valid.start === 'approved' && valid.requirements === 'approved'
      && ['approved', 'skip'].includes(valid.design), 'Согласуйте текущий пакет старта реализации.');
    assert(!feature.blocked && waitingFor(feature, all).length === 0, 'Снимите блокировки и завершите зависимости.');
  }
  if (target === 'ready_for_release') {
    assert(!feature.blocked && !waitingFor(feature, all).length, 'Снимите блокировки и завершите зависимости.');
    assert(qaPassed(feature), 'QA должен пройти все обязательные проверки без открытых ошибок и блокировок.');
  }
  if (target === 'released') {
    assert(valid.release === 'approved' && valid.start === 'approved' && qaPassed(feature), 'Согласуйте релиз для текущей версии с успешным QA.');
    assert(!feature.blocked && !waitingFor(feature, all).length, 'Снимите блокировки и завершите зависимости.');
    feature.releaseEvidence = text(evidence ?? '', 1000);
    assert(feature.releaseEvidence, 'Запишите результат или ссылку на фактический выпуск.');
    feature.releasedOn = localDay(at);
  }
  if (enteringQA) feature.qaCycle += 1;
  feature.status = target;
}
function approve(feature, gate, note, author, at) {
  const valid = approvals(feature);
  const has = (kind) => feature.artifacts.some((item) => item.kind === kind);
  assert(feature.status === gateStages[gate], 'Согласование доступно на соответствующем этапе.');
  if (gate === 'requirements') assert(feature.description && feature.criteria && has('spec'), 'Добавьте описание, критерии и версию спецификации.');
  if (gate === 'design') assert(feature.ux && valid.requirements === 'approved' && has('design'), 'Нужны согласованные требования и версия макета.');
  if (gate === 'start') {
    assert(valid.requirements === 'approved' && ['approved', 'skip'].includes(valid.design), 'Согласуйте требования и макеты.');
    assert(has('contract') && has('tests'), 'Добавьте версии технического решения и плана проверок.');
  }
  if (gate === 'release') assert(valid.start === 'approved' && qaPassed(feature) && has('release'), 'Нужны актуальный допуск, успешный QA и версия пакета релиза.');
  note = text(note, 1200);
  assert(note, 'Запишите решение владельца или ссылку на подтверждение.');
  feature.approvals[gate] = {key: gateKey(feature, gate), note, author, at};
}

const fields = {
  title: (v) => text(v, 140), description: (v) => text(v), criteria: (v) => text(v),
  owner: (v) => text(v, 100), role: (v) => text(v, 100), priority: (v) => choice(v, priorities),
  scope: (v) => choice(v, scopes), horizon: (v) => choice(v, horizons),
  critical: flag, ux: flag, blocked: (v) => text(v, 600), startOn: day, dueOn: day,
  deadlineNote: (v) => text(v, 300), duration: (v) => v === null ? null : number(v, 1),
  deps: (v) => {
    assert(Array.isArray(v) && v.length <= 100, 'Выберите зависимости из реестра.');
    return [...new Set(v.map((id) => text(id, 30)))];
  },
  artifacts: (v) => {
    assert(Array.isArray(v) && v.length <= 50, 'В карточке может быть до 50 артефактов.');
    return v.map(artifact);
  },
};
function updateFields(feature, data) {
  assert(data && typeof data === 'object' && !Array.isArray(data), 'Укажите поля карточки.');
  const changes = [];
  for (const [key, value] of Object.entries(data)) {
    assert(Object.hasOwn(fields, key), 'Это поле нельзя изменить здесь.');
    const validated = fields[key](value);
    if (JSON.stringify(feature[key]) !== JSON.stringify(validated)) changes.push(key);
    feature[key] = validated;
  }
  assert(feature.title, 'Введите название фичи.');
  assert(!feature.startOn || !feature.dueOn || feature.startOn <= feature.dueOn, 'Дата завершения должна быть не раньше даты начала.');
  return changes;
}
function qaReport(feature, value, author, at) {
  assert(feature.status === 'qa' && value && typeof value === 'object', 'Добавляйте отчёт на этапе QA.');
  const version = text(value.version, 100);
  const notes = text(value.notes, 3000);
  assert(version && notes, 'Укажите проверенную версию и итог проверки.');
  const required = number(value.required, 1);
  feature.qa.push({
    version, notes, href: link(value.href), required, passed: number(value.passed, 0, required),
    open: number(value.open, 0), blockers: flag(value.blockers), author, at,
    cycle: feature.qaCycle, startKey: gateKey(feature, 'start'),
  });
}

export function apply(state, command, at = new Date().toISOString()) {
  const next = structuredClone(state);
  const author = text(command.author, 100);
  assert(author, 'Укажите автора записи.');
  const action = command.action;
  let feature = next.features.find((f) => f.id === command.id);
  let event;
  if (action === 'create') {
    feature = {
      id: 'FC-' + String(next.nextId++).padStart(3, '0'), status: 'idea', scope: 'should',
      priority: 'p2', horizon: 'next', critical: false, ux: true, owner: '', role: '',
      description: '', criteria: '', blocked: '', startOn: '', dueOn: '', releasedOn: '',
      deadlineNote: '', duration: null, deps: [], artifacts: [], approvals: {}, qa: [], qaCycle: 0, history: [],
      createdAt: at,
    };
    updateFields(feature, command.data);
    next.features.push(feature);
    event = 'Карточка создана';
  } else {
    assert(feature, 'Карточка не найдена.');
    if (action === 'update') {
      const changes = updateFields(feature, command.data);
      if (!changes.length) return state;
      event = changes.some((k) => k === 'artifacts') ? 'Обновлены артефакты' : 'Карточка обновлена';
    } else if (action === 'move') {
      move(feature, command.status, next.features, at, command.evidence);
      event = 'Этап изменён: ' + stages[command.status];
    } else if (action === 'approve') {
      assert(Object.hasOwn(gates, command.gate), 'Выберите согласование.');
      approve(feature, command.gate, command.note, author, at);
      event = 'Согласовано: ' + gates[command.gate];
    } else if (action === 'qa') {
      qaReport(feature, command.report, author, at);
      event = 'Записан отчёт QA: ' + feature.qa.at(-1).version;
    } else if (action === 'close') {
      if (feature.status === 'closed') return state;
      assert(!isFinished(feature), 'Эта карточка уже завершена.');
      const note = text(command.note ?? '', 1200);
      assert(note, 'Укажите причину закрытия.');
      feature.closure = {note, author, at, previousStatus: feature.status};
      feature.status = 'closed';
      event = 'Карточка закрыта: ' + note;
    } else {
      throw new Error('Неизвестное действие.');
    }
  }
  const graph = topology(next.features);
  assert(graph.kind !== 'unknown', 'Одна из зависимостей отсутствует в реестре.');
  assert(graph.kind !== 'cycle', 'Эта связь замыкает цикл зависимостей.');
  feature.updatedAt = at;
  feature.history.push({event, author, at});
  next.revision++;
  return next;
}
export function topology(features) {
  const byId = new Map(features.map((f) => [f.id, f]));
  const marks = new Map();
  const order = [];
  let problem = null;
  function visit(id) {
    const mark = marks.get(id);
    if (mark === 1) { problem = {kind: 'cycle', id}; return; }
    if (mark === 2 || problem) return;
    const task = byId.get(id);
    if (!task) { problem = {kind: 'unknown', id}; return; }
    marks.set(id, 1);
    task.deps.forEach(visit);
    marks.set(id, 2);
    order.push(task);
  }
  features.forEach((f) => visit(f.id));
  return problem || {kind: 'ok', order, byId};
}
export function plan(features) {
  const graph = topology(features);
  if (graph.kind !== 'ok') return graph;
  const included = new Set();
  function include(id) {
    const f = graph.byId.get(id);
    if (included.has(id) || isFinished(f)) return;
    included.add(id);
    f.deps.forEach(include);
  }
  features.filter((f) => f.scope === 'must').forEach((f) => include(f.id));
  const order = graph.order.filter((f) => included.has(f.id));
  if (!order.length) return {kind: 'empty'};
  const missing = order.filter((f) => !Number.isInteger(f.duration) || f.duration < 1);
  if (missing.length) return {kind: 'missing', missing};
  const earliest = new Map();
  const latest = new Map();
  let total = 0;
  for (const f of order) {
    const start = Math.max(0, ...f.deps.map((dep) => earliest.get(dep) || 0));
    const finish = start + f.duration;
    earliest.set(f.id, finish);
    total = Math.max(total, finish);
    latest.set(f.id, Infinity);
  }
  const critical = new Set();
  const timings = new Map();
  for (const f of [...order].reverse()) {
    const end = Math.min(latest.get(f.id), total);
    const start = end - f.duration;
    const early = earliest.get(f.id) - f.duration;
    if (start === early) critical.add(f.id);
    timings.set(f.id, {start: early, end: earliest.get(f.id), slack: start - early});
    for (const dep of f.deps) if (included.has(dep)) latest.set(dep, Math.min(latest.get(dep), start));
  }
  return {kind: 'ready', total, critical, timings};
}
