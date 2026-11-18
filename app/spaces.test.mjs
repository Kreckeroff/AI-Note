import {strict as assert} from 'node:assert';
import {spawn} from 'node:child_process';
import {createHash, randomUUID, scryptSync} from 'node:crypto';
import {once} from 'node:events';
import {mkdtemp, rm, writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {DatabaseSync} from 'node:sqlite';
import {test} from 'node:test';
import {fileURLToPath} from 'node:url';
import {createSeverServer} from './server.mjs';

const now = Date.UTC(2026, 10, 18, 9);
const studio = {id: 'a36ef841-c7f2-4df3-8fde-93dd23d54cca', name: 'Студия «Контур»'};
const other = {id: '24657081-c17d-4e4f-9ebc-24a4c0b570ab', name: 'Команда «Вектор»'};
const tenantPath = tenant => `/workspace/${tenant.id}/spaces`;
const anna = {email: 'anna@example.test', name: 'Анна Редактор', password: 'Тестовый пароль Анны 001'};
const boris = {email: 'boris@example.test', name: 'Борис Егоров', password: 'Тестовый пароль Бориса 002'};
const irina = {email: 'irina@example.test', name: 'Ирина Администратор', password: 'Тестовый пароль Ирины 003'};
const mikhail = {email: 'mikhail@example.test', name: 'Михаил Администратор', password: 'Тестовый пароль Михаила 004'};
const legacySession = 'A'.repeat(43);
const script = name => fileURLToPath(new URL(name, import.meta.url));

async function runCli(name, dataDir, args, input = '') {
  const child = spawn(process.execPath, [script(name), ...args], {
    env: {...process.env, SEVER_DATA_DIR: dataDir},
    stdio: ['pipe', 'ignore', 'pipe'],
  });
  let error = '';
  child.stderr.setEncoding('utf8').on('data', chunk => { error += chunk; });
  child.stdin.end(input);
  const [status] = await once(child, 'close');
  assert.equal(status, 0, error);
}

async function installSchemaOne(dataDir) {
  const filename = join(dataDir, 'sever.sqlite');
  await writeFile(filename, '', {mode: 0o600});
  const database = new DatabaseSync(filename, {enableForeignKeyConstraints: true});
  try {
    database.exec(`
      CREATE TABLE accounts (
        id TEXT PRIMARY KEY CHECK (length(id) = 36),
        email TEXT NOT NULL UNIQUE COLLATE NOCASE CHECK (length(email) BETWEEN 3 AND 254),
        name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 120),
        salt BLOB NOT NULL CHECK (length(salt) = 16),
        password_hash BLOB NOT NULL CHECK (length(password_hash) = 32)
      ) STRICT;
      CREATE TABLE sessions (
        token_digest BLOB PRIMARY KEY CHECK (length(token_digest) = 32),
        account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
        created_at INTEGER NOT NULL CHECK (created_at >= 0),
        expires_at INTEGER NOT NULL CHECK (expires_at > created_at)
      ) STRICT;
      CREATE INDEX sessions_by_expiry ON sessions(expires_at);
      CREATE INDEX sessions_by_account ON sessions(account_id);
      PRAGMA user_version = 1;
    `);
    const id = '546be3a7-b1a4-4ef6-9322-023cb69e1460';
    const salt = Buffer.alloc(16, 42);
    const hash = scryptSync(anna.password, salt, 32, {N: 2 ** 17, r: 8, p: 1, maxmem: 160 * 1024 * 1024});
    database.prepare('INSERT INTO accounts VALUES (?, ?, ?, ?, ?)').run(id, anna.email, anna.name, salt, hash);
    const digest = createHash('sha256').update(legacySession).digest();
    database.prepare('INSERT INTO sessions VALUES (?, ?, ?, ?)').run(digest, id, now - 1, now + 60_000);
  } finally {
    database.close();
  }
}

function membership(tenant, account, options = {}) {
  return {tenantId: tenant.id, email: account.email, role: 'member', status: 'active', canCreate: false, current: false, ...options};
}

async function fixture(t) {
  const dataDir = await mkdtemp(join(tmpdir(), 'sever-spaces-test-'));
  let server;
  let origin;
  let currentTime = now;
  const audit = [];
  const stop = async () => {
    if (server?.listening) await new Promise(resolve => server.close(resolve));
  };
  t.after(async () => {
    await stop();
    await rm(dataDir, {recursive: true, force: true});
  });
  return {
    dataDir, audit,
    async start() {
      await stop();
      server = createSeverServer({dataDir, clock: () => currentTime, record: (event, details) => audit.push({event, ...details})});
      server.listen(0, '127.0.0.1');
      await once(server, 'listening');
      origin = `http://127.0.0.1:${server.address().port}`;
    },
    schemaVersion() {
      const database = new DatabaseSync(join(dataDir, 'sever.sqlite'), {readOnly: true});
      try { return database.prepare('PRAGMA user_version').get().user_version; }
      finally { database.close(); }
    },
    advance: elapsed => { currentTime += elapsed; },
    async configure(memberships) {
      const file = join(dataDir, 'tenants.json');
      await writeFile(file, JSON.stringify({tenants: [studio, other], memberships}));
      await runCli('configure-tenants.mjs', dataDir, [file]);
    },
    async configureGrants(grants) {
      const file = join(dataDir, 'space-grants.json');
      await writeFile(file, JSON.stringify({grants}));
      await runCli('configure-space-grants.mjs', dataDir, [file]);
    },
    provision: account => runCli('create-account.mjs', dataDir, [account.email, account.name], account.password),
    get: (path, cookie) => fetch(`${origin}${path}`, {redirect: 'manual', headers: cookie ? {Cookie: cookie} : {}}),
    post: (path, body, cookie, requestOrigin) => fetch(`${origin}${path}`, {
      method: 'POST', body, redirect: 'manual', headers: {Origin: requestOrigin ?? origin, ...(cookie && {Cookie: cookie})},
    }),
  };
}

test('the prior schema keeps its accounts and sessions and only the current tenant grants the creation action', async t => {
  const app = await fixture(t);
  await installSchemaOne(app.dataDir);
  await app.start();
  assert.equal(app.schemaVersion(), 1);
  await app.provision(boris);
  assert.equal(app.schemaVersion(), 1);
  const cookie = `sever_session=${legacySession}`;
  const before = await app.get('/workspace', cookie);
  assert.equal(before.status, 200);
  const beforePage = await before.text();
  assert.match(beforePage, /Анна Редактор/u);
  assert.doesNotMatch(beforePage, /Создать пространство/u);

  const memberships = [membership(studio, anna, {canCreate: true, current: true}), membership(other, anna)];
  await app.configure(memberships);
  await app.configure(memberships);
  assert.equal(app.schemaVersion(), 2);
  await app.start();
  const workspace = await app.get('/workspace', cookie);
  const html = await workspace.text();
  assert.equal(workspace.status, 200);
  assert.match(html, /Студия «Контур»/u);
  assert.match(html, /Создать пространство/u);
  assert.ok(html.includes(`${tenantPath(studio)}/new`));
  assert.ok(html.includes(`href="${tenantPath(other)}"`), 'an active membership is available in the tenant selector');
  assert.ok(!html.includes(`${tenantPath(other)}/new`), 'the current catalog does not expose a creation link for another tenant');
  assert.equal((await app.get(`${tenantPath(studio)}/new`, cookie)).status, 200);
  assert.equal((await app.get(`${tenantPath(other)}/new`, cookie)).status, 403);

  await app.start();
  assert.equal((await app.get('/workspace', cookie)).status, 200);
  const login = await app.post('/login', new URLSearchParams({email: anna.email, password: anna.password}));
  assert.equal(login.status, 303);
  assert.equal(login.headers.get('location'), '/workspace');
});

function listedSpaces(html, attribute) {
  return Array.from(html.matchAll(new RegExp(`${attribute}="([0-9a-f-]+)"`, 'gu')), ([, id]) => id);
}

test('catalog, menu and Documents share one tenant-scoped view and honor grants on the next request', async t => {
  const app = await fixture(t);
  await installSchemaOne(app.dataDir);
  await app.provision(boris);
  await app.configure([
    membership(studio, anna, {current: true, canCreate: true}),
    membership(other, anna, {canCreate: true}),
    membership(studio, boris, {current: true, canCreate: true}),
  ]);
  await app.start();
  const ownerCookie = `sever_session=${legacySession}`;
  const create = async (tenant, name, cookie = ownerCookie) => (await createSpace(app, tenant, {
    name, operationId: await newOperation(app, tenant, cookie), cookie,
  })).headers.get('location');
  const mine = await create(studio, 'Секция Анны');
  const foreign = await create(other, 'Данные другой команды');
  const memberCookie = await signIn(app, boris);
  const granted = await create(studio, 'Секция Бориса', memberCookie);
  const hidden = await create(studio, 'Невидимая коллекция', memberCookie);

  assert.equal(app.schemaVersion(), 2);
  const before = await (await app.get('/workspace', ownerCookie)).text();
  assert.deepEqual(listedSpaces(before, 'data-catalog-space'), [mine.split('/').at(-1)]);
  assert.doesNotMatch(before, /Секция Бориса|Данные другой команды|Невидимая коллекция/u);
  assert.equal((await app.get(granted, ownerCookie)).status, 404);
  const grant = {tenantId: studio.id, spaceId: granted.split('/').at(-1), email: anna.email, role: 'editor'};
  await assert.rejects(app.configureGrants([grant, {...grant, tenantId: other.id}]), /пространств/ui);
  assert.equal(app.schemaVersion(), 2, 'an invalid grant rolls back both migration and assignments');
  await app.configureGrants([grant]);
  await app.configureGrants([grant]);
  assert.equal(app.schemaVersion(), 3);
  await app.start();

  const authorized = [mine, granted].map(path => path.split('/').at(-1)).sort();
  const full = await (await app.get('/workspace', ownerCookie)).text();
  const documents = await (await app.get(mine, ownerCookie)).text();
  assert.deepEqual(listedSpaces(full, 'data-catalog-space').sort(), authorized);
  assert.deepEqual(listedSpaces(full, 'data-picker-space').sort(), authorized);
  assert.deepEqual(listedSpaces(documents, 'data-picker-space').sort(), authorized);
  assert.match(documents, /Документы/u);
  assert.doesNotMatch(full + documents, /Невидимая коллекция|Данные другой команды/u);
  const assigned = await (await app.get('/workspace?scope=membership', ownerCookie)).text();
  assert.deepEqual(listedSpaces(assigned, 'data-catalog-space'), [grant.spaceId]);
  assert.match((await (await app.get(granted, ownerCookie)).text()), /Редактирование/u);
  assert.ok((await (await app.get(tenantPath(other), ownerCookie)).text()).includes('Данные другой команды'));
  assert.equal((await app.get(foreign, memberCookie)).status, 404);
  assert.equal((await app.get(hidden, ownerCookie)).status, 404);

  await app.configureGrants([{...grant, role: 'viewer'}]);
  const viewer = await (await app.get(granted, ownerCookie)).text();
  assert.match(viewer, />Просмотр<\/span>/u);
  assert.doesNotMatch(viewer, /Редактирование/u);

  await app.configureGrants([{...grant, role: null}]);
  assert.equal((await app.get(granted, ownerCookie)).status, 404);
  const revoked = await (await app.get(mine, ownerCookie)).text();
  assert.deepEqual(listedSpaces(revoked, 'data-picker-space'), [mine.split('/').at(-1)]);
});

test('catalog searches the complete visible set before pagination and distinguishes duplicate names by owner', async t => {
  const app = await fixture(t);
  await installSchemaOne(app.dataDir);
  await app.provision(boris);
  await app.configure([
    membership(studio, anna, {role: 'admin', current: true, canCreate: true}),
    membership(studio, boris, {current: true, canCreate: true}),
  ]);
  await app.start();
  const cookie = `sever_session=${legacySession}`;
  const firstOperation = randomUUID();
  for (let index = 101; index > 0; index--) {
    const name = `Архив ${String(index).padStart(3, '0')}`;
    const operationId = index === 101 ? firstOperation : randomUUID();
    assert.equal((await createSpace(app, studio, {name, operationId, cookie})).status, 303);
  }
  const title = 'Ящик Ёлки';
  const terminal = await createSpace(app, studio, {name: title, operationId: randomUUID(), cookie});
  const otherOwner = await createSpace(app, studio, {name: title, operationId: randomUUID(), cookie: await signIn(app, boris)});
  const root = await (await app.get('/workspace', cookie)).text();
  assert.equal(listedSpaces(root, 'data-catalog-space').length, 20);
  assert.equal(listedSpaces(root, 'data-picker-space').length, 103);
  assert.match(root, /Найдено: 103/u);
  assert.match(root, /1–20 из 103/u);
  const end = await (await app.get('/workspace?page=9999', cookie)).text();
  assert.equal(listedSpaces(end, 'data-catalog-space').length, 3);
  assert.match(end, /101–103 из 103/u);
  const query = new URLSearchParams({q: 'ящик е\u0308лки'});
  const searched = await (await app.get(`/workspace?${query}`, cookie)).text();
  assert.deepEqual(listedSpaces(searched, 'data-catalog-space'), [terminal, otherOwner].map(response => response.headers.get('location').split('/').at(-1)));
  assert.match(searched, /Найдено: 2/u);
  assert.match(searched, /Борис Егоров/u);
  const noResults = await (await app.get(`/workspace?q=неттакойзаписи`, cookie)).text();
  assert.match(noResults, /Пространства не найдены/u);
  assert.doesNotMatch(noResults, /Здесь пока нет пространств/u);
  const fromPageTwo = new URLSearchParams({q: 'архив', scope: 'mine', page: '2'});
  const conflict = await app.post(`${tenantPath(studio)}?${fromPageTwo}`, new URLSearchParams({name: 'Другое', operation_id: firstOperation}), cookie);
  assert.equal(conflict.status, 409);
  assert.ok((await conflict.text()).includes(`href="${tenantPath(studio)}/new?${String(fromPageTwo).replaceAll('&', '&amp;')}">Открыть новую форму`));
});

test('a deep link survives failed login and only recognized local paths can become the post-login destination', async t => {
  const app = await fixture(t);
  await installSchemaOne(app.dataDir);
  await app.configure([membership(studio, anna, {current: true, canCreate: true})]);
  await app.start();
  const cookie = `sever_session=${legacySession}`;
  const created = await createSpace(app, studio, {name: 'Документы команды', operationId: await newOperation(app, studio, cookie), cookie});
  const target = `${created.headers.get('location')}?q=%D0%B4%D0%BE%D0%BA&scope=mine&page=2`;
  const enter = await app.get(target);
  assert.equal(enter.status, 303);
  const loginUrl = enter.headers.get('location');
  assert.equal(new URLSearchParams(loginUrl.split('?')[1]).get('returnTo'), target);
  assert.match(await (await app.get(loginUrl)).text(), /name="returnTo"/u);
  const denied = await app.post('/login', new URLSearchParams({email: anna.email, password: 'incorrect password', returnTo: target}));
  assert.equal(denied.status, 422);
  assert.match(await denied.text(), /name="returnTo"/u);
  const allowed = await app.post('/login', new URLSearchParams({email: anna.email, password: anna.password, returnTo: target}));
  assert.equal(allowed.headers.get('location'), target);
  const docs = await (await app.get(target, cookie)).text();
  assert.ok(docs.includes(`${tenantPath(studio)}?q=%D0%B4%D0%BE%D0%BA&amp;scope=mine&amp;page=2`));
  const longQuery = '資料'.repeat(80);
  const longTarget = `${created.headers.get('location')}?${new URLSearchParams({q: longQuery, scope: 'mine', page: '2'})}`;
  const canonical = `${created.headers.get('location')}?${new URLSearchParams({q: '資料'.repeat(50), scope: 'mine', page: '2'})}`;
  const longEntry = await app.get(longTarget);
  assert.equal(longEntry.status, 303);
  assert.equal(new URLSearchParams(longEntry.headers.get('location').split('?')[1]).get('returnTo'), canonical);
  const longReturn = await app.post('/login', new URLSearchParams({email: anna.email, password: anna.password, returnTo: longTarget}));
  assert.equal(longReturn.headers.get('location'), canonical);
  app.advance(5 * 60_000);
  for (const returnTo of ['//example.test/', 'https://example.test/', '/workspace/../login', '/workspace/%5C%2Fevil']) {
    const fallback = await app.post('/login', new URLSearchParams({email: anna.email, password: anna.password, returnTo}));
    assert.equal(fallback.status, 303);
    assert.equal(fallback.headers.get('location'), '/workspace');
  }
});

test('ordinary auth setup and an unsuccessful explicit preparation leave the schema at version 1', async t => {
  const app = await fixture(t);
  await app.provision(anna);
  assert.equal(app.schemaVersion(), 1);
  await app.start();
  assert.equal(app.schemaVersion(), 1);
  await assert.rejects(app.configure([membership(studio, boris, {canCreate: true, current: true})]), /Сначала создайте/u);
  assert.equal(app.schemaVersion(), 1);
  assert.ok(await signIn(app, anna));
});

async function signIn(app, {email, password}) {
  const response = await app.post('/login', new URLSearchParams({email, password}));
  assert.equal(response.status, 303);
  return response.headers.get('set-cookie').split(';', 1)[0];
}

function operationFrom(html) {
  const operation = html.match(/name="operation_id" value="([a-f0-9-]+)"/u)?.[1];
  assert.ok(operation);
  return operation;
}

async function newOperation(app, tenant, cookie) {
  const response = await app.get(`${tenantPath(tenant)}/new`, cookie);
  assert.equal(response.status, 200);
  return operationFrom(await response.text());
}

function createSpace(app, tenant, {name, operationId, cookie, origin}) {
  return app.post(tenantPath(tenant), new URLSearchParams({name, operation_id: operationId}), cookie, origin);
}

test('creation rejects missing permission, a foreign origin and stale sessions before using the operation ID', async t => {
  const app = await fixture(t);
  await installSchemaOne(app.dataDir);
  await app.configure([membership(studio, anna, {canCreate: true, current: true}), membership(other, anna)]);
  await app.start();
  const cookie = `sever_session=${legacySession}`;
  const operationId = await newOperation(app, studio, cookie);
  const name = 'Закрытая запись';
  assert.equal((await createSpace(app, other, {name, operationId, cookie})).status, 403);
  assert.equal((await createSpace(app, studio, {name, operationId, cookie, origin: 'https://other.example.test'})).status, 403);
  const anonymous = await createSpace(app, studio, {name, operationId});
  assert.equal(anonymous.status, 303);
  assert.equal(anonymous.headers.get('location'), '/');
  app.advance(60_000);
  const expired = await createSpace(app, studio, {name, operationId, cookie});
  assert.equal(expired.status, 303);
  assert.equal(expired.headers.get('location'), '/');
  const permitted = await createSpace(app, studio, {name, operationId, cookie: await signIn(app, anna)});
  assert.equal(permitted.status, 303);
  assert.ok(permitted.headers.get('location').startsWith(`${tenantPath(studio)}/`));
});

test('creation preserves submitted values and enforces Unicode name boundaries and idempotency across restart', async t => {
  const app = await fixture(t);
  await app.provision(anna);
  await app.provision(boris);
  await app.configure([membership(studio, anna, {canCreate: true, current: true}), membership(studio, boris, {canCreate: true, current: true})]);
  await app.start();
  const cookie = await signIn(app, anna);
  const operationId = await newOperation(app, studio, cookie);
  const first = await createSpace(app, studio, {name: '  🦊  ', operationId, cookie});
  assert.equal(first.status, 303);
  const address = first.headers.get('location');
  assert.match(address, new RegExp(`^${tenantPath(studio)}/[a-f0-9-]{36}$`, 'u'));
  await app.start();
  const replay = await createSpace(app, studio, {name: '🦊', operationId, cookie});
  assert.equal(replay.status, 303);
  assert.equal(replay.headers.get('location'), address);
  const changed = await createSpace(app, studio, {name: 'Другое', operationId, cookie});
  assert.equal(changed.status, 409);
  const changedHtml = await changed.text();
  assert.match(changedHtml, /Откройте новую форму/u);
  assert.ok(changedHtml.includes(`href="${tenantPath(studio)}/new"`));
  for (const changedName of ['', '🦊'.repeat(101)]) {
    const invalidChange = await createSpace(app, studio, {name: changedName, operationId, cookie});
    assert.equal(invalidChange.status, 409);
    assert.match(await invalidChange.text(), /Откройте новую форму/u);
  }

  const nextOperation = await newOperation(app, studio, cookie);
  assert.equal((await createSpace(app, studio, {name: '  \t  ', operationId: nextOperation, cookie})).status, 422);
  const oversizedName = '🦊'.repeat(101);
  const oversized = await createSpace(app, studio, {name: oversizedName, operationId: nextOperation, cookie});
  assert.equal(oversized.status, 422);
  const errorPage = await oversized.text();
  assert.ok(errorPage.includes(`value="${oversizedName}"`));
  assert.equal(operationFrom(errorPage), nextOperation);
  assert.equal((await createSpace(app, studio, {name: '🦊'.repeat(100), operationId: nextOperation, cookie})).status, 303);
  const beyondTransport = await createSpace(app, studio, {name: '🦊'.repeat(400), operationId: nextOperation, cookie});
  assert.equal(beyondTransport.status, 413);

  const original = await createSpace(app, studio, {name: 'Планы Ёлки', operationId: await newOperation(app, studio, cookie), cookie});
  assert.equal(original.status, 303);
  const equivalent = 'планы е\u0308лки';
  const duplicate = await createSpace(app, studio, {name: equivalent, operationId: await newOperation(app, studio, cookie), cookie});
  assert.equal(duplicate.status, 409);
  assert.ok((await duplicate.text()).includes(`value="${equivalent}"`));
  const otherOwner = await signIn(app, boris);
  const otherSpace = await createSpace(app, studio, {name: equivalent, operationId: await newOperation(app, studio, otherOwner), cookie: otherOwner});
  assert.equal(otherSpace.status, 303);
  assert.notEqual(otherSpace.headers.get('location'), original.headers.get('location'));
});

test('a failed write rolls back the space and operation and parallel submissions produce one creation', async t => {
  const app = await fixture(t);
  await installSchemaOne(app.dataDir);
  await app.configure([membership(studio, anna, {canCreate: true, current: true})]);
  await app.start();
  const cookie = `sever_session=${legacySession}`;
  const operationId = await newOperation(app, studio, cookie);
  const database = new DatabaseSync(join(app.dataDir, 'sever.sqlite'));
  try {
    database.exec(`CREATE TRIGGER fail_creation BEFORE INSERT ON space_creations BEGIN SELECT RAISE(ABORT, 'test write failure'); END`);
    const unavailable = await createSpace(app, studio, {name: 'После сбоя', operationId, cookie});
    assert.equal(unavailable.status, 503);
    const failedPage = await unavailable.text();
    assert.ok(failedPage.includes('value="После сбоя"'));
    assert.equal(operationFrom(failedPage), operationId);
    assert.doesNotMatch(failedPage, /test write failure/u);
    database.exec('DROP TRIGGER fail_creation');
  } finally {
    database.close();
  }
  const retried = await createSpace(app, studio, {name: 'После сбоя', operationId, cookie});
  assert.equal(retried.status, 303, 'A partial write would leave a duplicate title after removing the failing trigger');
  const replayed = await createSpace(app, studio, {name: 'После сбоя', operationId, cookie});
  assert.equal(replayed.headers.get('location'), retried.headers.get('location'));

  const ids = await Promise.all([newOperation(app, studio, cookie), newOperation(app, studio, cookie)]);
  const raced = await Promise.all(ids.map(id => createSpace(app, studio, {name: 'Параллельная запись', operationId: id, cookie})));
  assert.deepEqual(raced.map(response => response.status).sort(), [303, 409]);
});

test('owners and their local tenant administrators can open a closed space while denials remain indistinguishable', async t => {
  const app = await fixture(t);
  await installSchemaOne(app.dataDir);
  for (const account of [boris, irina, mikhail]) await app.provision(account);
  await app.configure([
    membership(studio, anna, {canCreate: true, current: true}),
    membership(studio, boris, {current: true}),
    membership(studio, irina, {role: 'admin', current: true}),
    membership(other, mikhail, {role: 'superadmin', canCreate: true, current: true}),
  ]);
  await app.start();
  const cookie = `sever_session=${legacySession}`;
  const name = 'Календарь <команды>';
  const created = await createSpace(app, studio, {name, operationId: await newOperation(app, studio, cookie), cookie});
  assert.equal(created.status, 303);
  const address = created.headers.get('location');
  const anonymous = await app.get(address);
  assert.equal(anonymous.status, 303);
  assert.equal(anonymous.headers.get('location'), `/?${new URLSearchParams({returnTo: address})}`);

  await app.start();
  const owner = await app.get(address, await signIn(app, anna));
  assert.equal(owner.status, 200);
  assert.equal(owner.headers.get('cache-control'), 'no-store');
  const ownerHtml = await owner.text();
  assert.match(ownerHtml, /Календарь &lt;команды&gt;/u);
  assert.match(ownerHtml, /Студия «Контур»/u);
  assert.match(ownerHtml, /Закрытое/u);
  assert.match(ownerHtml, /Владелец/u);
  assert.match(ownerHtml, /В пространстве пока нет разделов/u);
  assert.ok(!ownerHtml.includes(name));
  const admin = await app.get(address, await signIn(app, irina));
  assert.equal(admin.status, 200);
  assert.match(await admin.text(), /Просмотр по роли администратора/u);

  const otherAdmin = await signIn(app, mikhail);
  const unknownSpace = `${tenantPath(studio)}/c48a61fb-074c-4ef5-bec6-3d821daf8f40`;
  const responses = await Promise.all([
    app.get(address, await signIn(app, boris)),
    app.get(address, otherAdmin),
    app.get(address.replace(studio.id, other.id), otherAdmin),
    app.get(unknownSpace, cookie),
  ]);
  const pages = [];
  for (const response of responses) {
    assert.equal(response.status, 404);
    const page = await response.text();
    assert.match(page, /Пространство не найдено/u);
    assert.doesNotMatch(page, /Календарь|Анна Редактор/u);
    pages.push(page);
  }
  assert.equal(new Set(pages).size, 1);
  const denials = app.audit.filter(({event, outcome}) => event === 'space.open' && outcome === 'not-found');
  assert.equal(denials.filter(({tenantId}) => tenantId === studio.id).length, 3);
  assert.equal(denials.filter(({tenantId}) => tenantId === other.id).length, 1);
});
