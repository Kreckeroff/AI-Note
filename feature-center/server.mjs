import {createServer} from 'node:http';
import {readFileSync, writeFileSync, renameSync, existsSync, realpathSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {dirname, resolve, sep} from 'node:path';
import {apply} from './public/core.mjs';

const root = dirname(fileURLToPath(import.meta.url));
const docs = realpathSync(resolve(root, '../docs'));
const assets = new Map([
  ['/', ['index.html', 'text/html; charset=utf-8']],
  ['/style.css', ['style.css', 'text/css; charset=utf-8']],
  ['/app.mjs', ['app.mjs', 'text/javascript; charset=utf-8']],
  ['/core.mjs', ['core.mjs', 'text/javascript; charset=utf-8']],
]);
const titles = {
  workflow: 'Регламент работы агентов', board: 'Центр управления фичами',
  roles: 'Профили агентов и матрица навыков', qa: 'Независимая приёмка и QA-раунды',
  intake: 'Связующие навыки оркестратора', plugins: 'Подключения под проект',
  product: 'Уточнить контекст продукта',
};
function seed() {
  const source = (kind, label, name) => ({kind, label, href: '/docs/workflow/' + name + '.md', revision: '17.11.2026'});
  const items = [
    {title: titles.workflow, owner: 'Оркестратор', role: 'Оркестратор', scope: 'must', priority: 'p0', horizon: 'now', critical: true,
      description: 'Закрепить маршруты фич, роли и четыре решения владельца.', criteria: 'Маршруты и правила перехода проверены на сложных сценариях.',
      artifacts: [source('spec', 'Процесс и согласования', 'operating-model')], steps: ['discovery', 'requirements_review']},
    {title: titles.board, owner: 'Оркестратор', role: 'Фронтенд и бэкенд', scope: 'must', priority: 'p0', horizon: 'now',
      description: 'Собрать в одном месте бэклог, согласования, roadmap и критический путь.', criteria: 'Карточки и история сохраняются. Переходы учитывают доступность согласований.',
      artifacts: [source('spec', 'Требования к доске', 'roadmap-board')], steps: ['discovery', 'requirements_review']},
    {title: titles.roles, owner: 'Оркестратор', role: 'Архитектор', scope: 'must', priority: 'p1', horizon: 'now', ux: false,
      description: 'Описать обязанности, артефакты и условия запуска каждой роли.', deps: ['FC-001'],
      artifacts: [source('other', 'Карточки ролей', 'roles'), source('other', 'Каталог 77 навыков', 'skill-catalog')], steps: ['discovery']},
    {title: titles.qa, owner: 'Оркестратор', role: 'QA', scope: 'must', priority: 'p1', horizon: 'next', ux: false,
      description: 'Задать чистый контекст, проверяемый снимок и правила повторных проходов.', deps: ['FC-001', 'FC-003'],
      artifacts: [source('other', 'Контракты передачи', 'artifacts'), source('other', 'Сценарный разбор', 'validation')], steps: ['discovery']},
    {title: titles.intake, owner: 'Оркестратор', role: 'Оркестратор', scope: 'should', priority: 'p1', horizon: 'next', ux: false,
      description: 'Оформить маршрутизацию, согласования и хэндоффы в проектные навыки.', deps: ['FC-003'],
      artifacts: [source('other', 'Проект 12 навыков', 'custom-skills')]},
    {title: titles.plugins, scope: 'could', priority: 'p3', horizon: 'later', ux: false,
      description: 'Подобрать подключения после выбора продукта и рабочих сервисов.',
      artifacts: [source('other', 'План подключений', 'plugins')]},
    {title: titles.product, role: 'Аналитик', scope: 'should', priority: 'p1', horizon: 'now',
      description: 'Зафиксировать продукт, пользователей и первый рабочий сценарий.',
      artifacts: [{kind: 'other', label: 'Контекст проекта', href: '/docs/project-context.md', revision: '17.11.2026'}]},
  ];
  let state = {revision: 0, nextId: 1, features: []};
  for (const {steps = [], ...data} of items) {
    state = apply(state, {action: 'create', author: 'Оркестратор', data});
    const id = state.features.at(-1).id;
    for (const status of steps) state = apply(state, {action: 'move', author: 'Оркестратор', id, status});
  }
  return state;
}
function save(file, state) {
  const temporary = file + '.tmp';
  writeFileSync(temporary, JSON.stringify(state, null, 2) + '\n', {mode: 0o600});
  renameSync(temporary, file);
}
const headers = {
  'Cache-Control': 'no-store', 'X-Content-Type-Options': 'nosniff',
  'Referrer-Policy': 'no-referrer', 'X-Frame-Options': 'SAMEORIGIN',
  'Content-Security-Policy': "default-src 'self'; style-src 'self' 'unsafe-inline'; object-src 'none'; base-uri 'none'; form-action 'self'",
};
function respond(response, status, content, type = 'application/json; charset=utf-8') {
  response.writeHead(status, {...headers, 'Content-Type': type});
  response.end(Buffer.isBuffer(content) ? content : JSON.stringify(content));
}
async function readCommand(request) {
  let source = '';
  request.setEncoding('utf8');
  for await (const chunk of request) {
    source += chunk;
    if (source.length > 250_000) throw new Error('Запись слишком большая.');
  }
  const value = JSON.parse(source);
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Передайте объект команды.');
  return value;
}

export function featureServer(file = resolve(root, 'features.json')) {
  if (!existsSync(file)) save(file, seed());
  const read = () => JSON.parse(readFileSync(file, 'utf8'));
  return createServer(async (request, response) => {
    const port = request.socket.localPort;
    const allowedHosts = ['127.0.0.1:' + port, 'localhost:' + port];
    const host = request.headers.host;
    if (!allowedHosts.includes(host)) return respond(response, 403, {error: 'Откройте локальный адрес доски.'});
    const {pathname} = new URL(request.url, 'http://' + host);
    const method = request.method;
    try {
      if (pathname === '/api/features' && method === 'GET') {
        return respond(response, 200, read());
      }
      if (pathname === '/api/features' && method === 'POST') {
        const origin = request.headers.origin;
        const json = request.headers['content-type']?.split(';')[0] === 'application/json';
        if (!json || (origin && !allowedHosts.some((h) => origin === 'http://' + h))) {
          return respond(response, 403, {error: 'Изменения принимаются из локальной доски.'});
        }
        let command, next;
        try { command = await readCommand(request); }
        catch { return respond(response, 400, {error: 'Проверьте формат и размер записи.'}); }
        const state = read();
        if (state.revision !== command.revision) {
          return respond(response, 409, {error: 'Реестр уже изменился. Обновите данные перед сохранением.', state});
        }
        try { next = apply(state, command); }
        catch (error) { return respond(response, 422, {error: error.message}); }
        if (next !== state) save(file, next);
        return respond(response, 200, next);
      }
      if (method === 'GET' && assets.has(pathname)) {
        const [name, type] = assets.get(pathname);
        return respond(response, 200, readFileSync(resolve(root, 'public', name)), type);
      }
      if (method === 'GET' && /^\/docs\/(?:[a-z0-9_-]+\/)*[a-z0-9_-]+\.md$/i.test(pathname)) {
        const candidate = resolve(docs, pathname.slice(6));
        if (existsSync(candidate)) {
          const actual = realpathSync(candidate);
          if (actual.startsWith(docs + sep)) return respond(response, 200, readFileSync(actual), 'text/plain; charset=utf-8');
        }
      }
      respond(response, 404, {error: 'Страница не найдена.'});
    } catch (error) {
      console.error(method, pathname, error.message);
      if (!response.headersSent) respond(response, 500, {error: 'Не удалось открыть или сохранить локальный реестр.'});
    }
  });
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const port = 4117;
  featureServer().listen(port, '127.0.0.1', () => console.log('Центр фич: http://127.0.0.1:' + port));
}
