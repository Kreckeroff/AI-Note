import {strict as assert} from 'node:assert';
import {spawn} from 'node:child_process';
import {once} from 'node:events';
import {mkdtemp, rm, stat} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {DatabaseSync} from 'node:sqlite';
import {test} from 'node:test';
import {fileURLToPath} from 'node:url';
import {createSeverServer} from './server.mjs';

const accounts = {
  anna: {email: 'anna@example.test', name: 'Анна <Редактор>', password: 'Тестовый пароль Анны 001'},
  irina: {email: 'irina@example.test', name: 'Ирина Смирнова', password: 'Тестовый пароль Ирины 002'},
};
const createAccount = fileURLToPath(new URL('create-account.mjs', import.meta.url));

async function provision(dataDir, account) {
  const child = spawn(process.execPath, [createAccount, account.email, account.name], {
    env: {...process.env, SEVER_DATA_DIR: dataDir},
    stdio: ['pipe', 'ignore', 'pipe'],
  });
  let error = '';
  child.stderr.setEncoding('utf8').on('data', chunk => { error += chunk });
  child.stdin.end(`${account.password}\n`);
  const [code] = await once(child, 'close');
  return {code, error};
}

async function fixture(t, seed = []) {
  const dataDir = await mkdtemp(join(tmpdir(), 'sever-auth-test-'));
  let server;
  let now = Date.UTC(2026, 10, 18, 9);
  t.after(async () => {
    if (server?.listening) await new Promise(resolve => server.close(resolve));
    await rm(dataDir, {recursive: true, force: true});
  });
  for (const account of seed) assert.equal((await provision(dataDir, account)).code, 0);
  const fixture = {dataDir};
  fixture.start = async () => {
    if (server?.listening) await new Promise(resolve => server.close(resolve));
    server = createSeverServer({dataDir, clock: () => now});
    server.listen(0, '127.0.0.1');
    await once(server, 'listening');
    fixture.origin = `http://127.0.0.1:${server.address().port}`;
  };
  fixture.advance = elapsed => { now += elapsed };
  fixture.get = (path, cookie) => fetch(`${fixture.origin}${path}`, {
    redirect: 'manual',
    headers: cookie ? {Cookie: cookie} : {},
  });
  fixture.login = (account, origin = fixture.origin) => fetch(`${fixture.origin}/login`, {
    method: 'POST',
    body: new URLSearchParams({email: account.email, password: account.password}),
    headers: {Origin: origin},
    redirect: 'manual',
  });
  await fixture.start();
  return fixture;
}

function cookieFrom(response) {
  return response.headers.get('set-cookie')?.split(';', 1)[0];
}

function messageFrom(page) {
  return page.match(/role="alert"[^>]*>([^<]+)</u)?.[1];
}

test('form, durable sessions, two isolated identities and expiry', async t => {
  const app = await fixture(t, [accounts.anna, accounts.irina]);
  const signIn = await app.get('/');
  assert.match(await signIn.text(), /<form method="post" action="\/login"/u);
  assert.equal(signIn.headers.get('cache-control'), 'no-store');
  const anonymous = await app.get('/workspace');
  assert.equal(anonymous.status, 303);
  assert.equal(anonymous.headers.get('location'), '/');

  const first = await app.login({...accounts.anna, email: ' ANNA@EXAMPLE.TEST '});
  const second = await app.login(accounts.irina);
  for (const response of [first, second]) {
    assert.equal(response.status, 303);
    assert.equal(response.headers.get('location'), '/workspace');
    assert.match(response.headers.get('set-cookie'), /Path=\/workspace; HttpOnly; SameSite=Strict/u);
  }
  const anna = cookieFrom(first);
  const irina = cookieFrom(second);
  assert.notEqual(anna, irina);

  for (const reload of [false, true]) {
    if (reload) await app.start();
    const firstPage = await app.get('/workspace', anna);
    assert.equal(firstPage.status, 200);
    assert.equal(firstPage.headers.get('cache-control'), 'no-store');
    assert.match(await firstPage.text(), /Анна &lt;Редактор&gt;/u);
    const secondPage = await app.get('/workspace', irina);
    assert.equal(secondPage.status, 200);
    assert.match(await secondPage.text(), /Ирина Смирнова/u);
  }

  app.advance(12 * 60 * 60 * 1_000);
  const expired = await app.get('/workspace', anna);
  assert.equal(expired.status, 303);
  assert.equal(expired.headers.get('location'), '/');
  assert.equal((await app.login(accounts.anna)).status, 303);
});

test('invalid credentials are indistinguishable; email is retained without the password', async t => {
  const app = await fixture(t, [accounts.anna]);
  const password = 'невозможная-пропускная-фраза';
  const wrong = await app.login({email: accounts.anna.email, password});
  const unknown = await app.login({email: 'absent@example.test', password});
  assert.equal(wrong.status, 422);
  assert.equal(unknown.status, 422);
  assert.equal(wrong.headers.get('cache-control'), 'no-store');
  assert.equal(cookieFrom(wrong), undefined);
  assert.equal(cookieFrom(unknown), undefined);
  const [wrongPage, unknownPage] = await Promise.all([wrong.text(), unknown.text()]);
  const alert = messageFrom(wrongPage);
  assert.ok(alert);
  assert.equal(alert, messageFrom(unknownPage));
  assert.ok(wrongPage.includes(accounts.anna.email));
  assert.ok(!wrongPage.includes(password));
  assert.equal(wrongPage.replace(accounts.anna.email, 'absent@example.test'), unknownPage);
});

test('login requires the product origin and applies a temporary attempt limit', async t => {
  const app = await fixture(t, [accounts.anna]);
  const foreign = await app.login(accounts.anna, 'http://127.0.0.1:4120');
  assert.equal(foreign.status, 403);
  assert.equal(cookieFrom(foreign), undefined);

  const wrong = {...accounts.anna, password: 'неверный пароль для лимита'};
  for (let attempt = 0; attempt < 6; attempt++) {
    assert.equal((await app.login(wrong)).status, 422);
  }
  const limited = await app.login(accounts.anna);
  assert.equal(limited.status, 429);
  assert.equal(limited.headers.get('retry-after'), '300');
  assert.ok(messageFrom(await limited.text()));
  app.advance(5 * 60 * 1_000);
  assert.equal((await app.login(accounts.anna)).status, 303);
});

test('account provisioning persists a salted hash and rejects a duplicate identity', async t => {
  const app = await fixture(t, [accounts.anna]);
  assert.equal((await provision(app.dataDir, {...accounts.anna, email: 'ANNA@EXAMPLE.TEST'})).code, 1);
  const filename = join(app.dataDir, 'sever.sqlite');
  assert.equal((await stat(filename)).mode & 0o077, 0);
  const database = new DatabaseSync(filename, {readOnly: true});
  t.after(() => database.close());
  const account = database.prepare('SELECT email, salt, password_hash FROM accounts').all();
  assert.equal(account.length, 1);
  assert.equal(account[0].email, accounts.anna.email);
  assert.equal(account[0].salt.length, 16);
  assert.equal(account[0].password_hash.length, 32);
  assert.notEqual(Buffer.from(account[0].password_hash).toString('utf8'), accounts.anna.password);
});

test('provisioning, the form, and sign-in agree on domain labels of 63 and 64 characters', async t => {
  const app = await fixture(t);
  const address = length => `editor@${'a'.repeat(length)}.test`;
  const valid = {...accounts.anna, email: address(63)};
  const invalid = {...accounts.anna, email: address(64)};
  assert.equal((await provision(app.dataDir, valid)).code, 0);
  assert.equal((await provision(app.dataDir, invalid)).code, 1);

  const markup = await (await app.get('/')).text();
  const input = markup.match(/<input id="email"[^>]+>/u)?.[0];
  assert.match(input, /maxlength="254"/u);
  const pattern = input.match(/pattern="([^"]+)"/u)?.[1];
  assert.ok(pattern, 'form exposes an email pattern');
  const validator = new RegExp(`^(?:${pattern.replaceAll('&amp;', '&').replaceAll('&#39;', "'")})$`, 'v');
  assert.ok(validator.test(valid.email));
  assert.ok(!validator.test(invalid.email));
  const longAddress = length => `${'x'.repeat(64)}@${'a'.repeat(63)}.${'b'.repeat(63)}.${'c'.repeat(length)}`;
  assert.ok(validator.test(longAddress(61)));
  assert.ok(!validator.test(longAddress(62)));
  assert.equal((await app.login(valid)).status, 303);
  assert.equal((await app.login(invalid)).status, 422);
  assert.equal((await provision(app.dataDir, {...accounts.anna, email: longAddress(61)})).code, 0);
  assert.equal((await provision(app.dataDir, {...accounts.anna, email: longAddress(62)})).code, 1);
});

test('an unambiguous email survives an absent or duplicate password field', async t => {
  const app = await fixture(t, [accounts.anna]);
  for (const passwords of [[], [accounts.anna.password, accounts.anna.password]]) {
    const fields = new URLSearchParams({email: accounts.anna.email});
    for (const password of passwords) fields.append('password', password);
    const response = await fetch(`${app.origin}/login`, {
      method: 'POST',
      body: fields,
      headers: {Origin: app.origin},
      redirect: 'manual',
    });
    assert.equal(response.status, 422);
    assert.equal(cookieFrom(response), undefined);
    const page = await response.text();
    assert.ok(page.includes(`value="${accounts.anna.email}"`));
    assert.ok(!page.includes(accounts.anna.password));
  }
});
