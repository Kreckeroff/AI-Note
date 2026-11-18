import {strict as assert} from 'node:assert';
import {once} from 'node:events';
import {mkdtemp, rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {test} from 'node:test';
import {openAccountStore} from './account-store.mjs';
import {provisionAccount} from './identity.mjs';
import {configureWorkspaceDatabase, openLocalDatabase} from './local-database.mjs';
import {createSeverServer} from './server.mjs';

const enabled = {SEVER_SELF_REGISTRATION: 'true'};
const registration = {email: 'new@example.test', name: 'Новая участница', password: 'Самостоятельно выбранный пароль'};

async function fixture(t) {
  const dataDir = await mkdtemp(join(tmpdir(), 'sever-registration-test-'));
  let server;
  let origin;
  let now = Date.UTC(2026, 10, 18, 9);
  const messages = [];
  const stop = async () => {
    if (server?.listening) await new Promise(resolve => server.close(resolve));
  };
  t.after(async () => {
    await stop();
    await rm(dataDir, {recursive: true, force: true});
  });
  const database = openLocalDatabase(dataDir);
  try {
    configureWorkspaceDatabase(database, () => {});
  } finally {
    database.close();
  }
  return {
    dataDir, messages,
    async start(environment) {
      await stop();
      server = createSeverServer({dataDir, environment, clock: () => now, deliverRegistration: message => messages.push(message)});
      server.listen(0, '127.0.0.1');
      await once(server, 'listening');
      origin = `http://127.0.0.1:${server.address().port}`;
    },
    advance(milliseconds) { now += milliseconds },
    get(path, headers) { return fetch(`${origin}${path}`, {headers, redirect: 'manual'}) },
    post(path, values, headers) {
      return fetch(`${origin}${path}`, {
        method: 'POST', body: new URLSearchParams(values), headers: {Origin: origin, ...headers}, redirect: 'manual',
      });
    },
  };
}

test('registration is off unless explicitly enabled and operator-issued accounts still sign in', async t => {
  const app = await fixture(t);
  const existing = {...registration, email: 'existing@example.test'};
  const accounts = openAccountStore(app.dataDir);
  try {
    await provisionAccount(accounts, existing);
  } finally {
    accounts.close();
  }
  for (const environment of [{}, {SEVER_SELF_REGISTRATION: 'false'}]) {
    await app.start(environment);
    assert.doesNotMatch(await (await app.get('/')).text(), /href="\/register"/u);
    for (const path of ['/register', '/register/confirm']) {
      assert.equal((await app.get(path)).status, 404);
      assert.equal((await app.post(path, registration)).status, 404);
    }
  }
  assert.equal((await app.post('/login', existing)).status, 303);
  assert.equal(app.messages.length, 0);
  assert.throws(() => createSeverServer({dataDir: app.dataDir, environment: {SEVER_SELF_REGISTRATION: 'yes'}}), /SEVER_SELF_REGISTRATION/u);
  assert.throws(() => createSeverServer({dataDir: app.dataDir, environment: enabled}), /SEVER_REGISTRATION_SENDMAIL/u);
});

test('email confirmation creates one personal workspace and signs its owner in', async t => {
  const app = await fixture(t);
  await app.start(enabled);
  assert.match(await (await app.get('/')).text(), /href="\/register"/u);
  assert.doesNotMatch(await (await app.get('/register')).text(), /name="password"/u);
  assert.equal((await app.post('/register', registration, {Origin: 'https://other.example.test'})).status, 403);
  const start = await app.post('/register', {...registration, email: ' NEW@EXAMPLE.TEST '});
  assert.equal(start.status, 202);
  const reply = await start.text();
  assert.equal((await app.post('/login', registration)).status, 422);
  assert.equal(app.messages.length, 1);
  assert.equal(app.messages[0].email, registration.email);
  assert.equal(app.messages[0].expiresInMinutes, 30);
  const confirmation = new URL(app.messages[0].link);
  const token = confirmation.searchParams.get('token');
  assert.ok(token);
  assert.ok(!reply.includes(token));
  const form = await app.get(`${confirmation.pathname}${confirmation.search}`);
  assert.equal(form.status, 200);
  assert.equal(form.headers.get('referrer-policy'), 'strict-origin');
  assert.match(await form.text(), /autocomplete="new-password"/u);
  assert.equal((await app.post(confirmation.pathname, {...registration, token}, {Origin: 'null'})).status, 403);
  const invalidFields = await app.post(confirmation.pathname, {...registration, token, password: 'short'});
  assert.equal(invalidFields.status, 422);
  assert.equal(invalidFields.headers.get('referrer-policy'), 'strict-origin');

  const attempts = await Promise.all([
    app.post(confirmation.pathname, {...registration, token}),
    app.post(confirmation.pathname, {...registration, token}),
  ]);
  assert.deepEqual(attempts.map(response => response.status).sort(), [303, 404]);
  const accepted = attempts.find(response => response.status === 303);
  assert.equal(accepted.headers.get('location'), '/workspace');
  const cookie = accepted.headers.get('set-cookie')?.split(';', 1)[0];
  assert.ok(cookie);
  const workspace = await app.get('/workspace', {Cookie: cookie});
  assert.equal(workspace.status, 200);
  const page = await workspace.text();
  assert.match(page, /Моя среда/u);
  const createPath = page.match(/id="create-space" href="([^"]+)"/u)?.[1];
  assert.ok(createPath);
  const spaceForm = await (await app.get(createPath, {Cookie: cookie})).text();
  const operationId = spaceForm.match(/name="operation_id" value="([^"]+)"/u)?.[1];
  assert.ok(operationId);
  const space = await app.post(createPath.slice(0, -4), {name: 'Рядом', operation_id: operationId}, {Cookie: cookie});
  assert.equal(space.status, 303);
  assert.match(await (await app.get(space.headers.get('location'), {Cookie: cookie})).text(), /В пространстве пока нет разделов/u);

  const sameEmail = await app.post('/register', registration);
  assert.equal(sameEmail.status, 202);
  assert.equal(await sameEmail.text(), reply);
  assert.equal(app.messages.length, 1);
  const database = openLocalDatabase(app.dataDir);
  try {
    assert.equal(database.prepare('SELECT COUNT(*) AS count FROM tenants').get().count, 1);
    assert.equal(database.prepare('SELECT role FROM tenant_memberships').get().role, 'superadmin');
  } finally {
    database.close();
  }
  await app.start({SEVER_SELF_REGISTRATION: 'false'});
  assert.equal((await app.get(`${confirmation.pathname}${confirmation.search}`)).status, 404);
  assert.equal((await app.post('/login', registration)).status, 303);
});

test('an expired link cannot create an account and the mailbox can request another link', async t => {
  const app = await fixture(t);
  await app.start(enabled);
  assert.equal((await app.post('/register', registration)).status, 202);
  const token = new URL(app.messages[0].link).searchParams.get('token');
  assert.equal((await app.post('/register', registration)).status, 202);
  assert.equal(app.messages.length, 1);
  app.advance(31 * 60 * 1000);
  assert.equal((await app.post('/register/confirm', {...registration, token})).status, 404);
  assert.equal((await app.post('/register', registration)).status, 202);
  assert.equal(app.messages.length, 2);
  const newToken = new URL(app.messages[1].link).searchParams.get('token');
  assert.notEqual(newToken, token);
  assert.equal((await app.get(`/register/confirm?token=${newToken}`)).status, 200);

  await app.start({SEVER_SELF_REGISTRATION: 'false'});
  assert.equal((await app.get(`/register/confirm?token=${newToken}`)).status, 404);
  assert.equal((await app.post('/register/confirm', {...registration, token: newToken})).status, 404);
  assert.equal((await app.post('/login', registration)).status, 422);
});
