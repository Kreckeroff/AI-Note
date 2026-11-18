import test from 'node:test';
import assert from 'node:assert/strict';
import {Readable} from 'node:stream';
import {mkdtempSync, readFileSync, rmSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {featureServer} from './server.mjs';

function request(server, method, url, data, headers = {}) {
  return new Promise((resolve) => {
    const req = Readable.from(data ? [JSON.stringify(data)] : []);
    Object.assign(req, {method, url, socket: {localPort: 4117}, headers: {
      host: '127.0.0.1:4117', 'content-type': 'application/json', ...headers,
    }});
    const res = {
      writeHead(code, responseHeaders) { this.code = code; this.headers = responseHeaders; this.headersSent = true; },
      end(body) { resolve({code: this.code, body: this.headers['Content-Type'].startsWith('application/json') ? JSON.parse(body) : body.toString()}); },
    };
    server.emit('request', req, res);
  });
}

test('save persists and stale writes cannot overwrite the file', async (t) => {
  const directory = mkdtempSync(join(tmpdir(), 'feature-center-test-'));
  t.after(() => rmSync(directory, {recursive: true, force: true}));
  const file = join(directory, 'features.json');
  const server = featureServer(file);
  const initial = await request(server, 'GET', '/api/features');
  assert.equal(initial.code, 200);
  const first = {action: 'create', author: 'Владелец', revision: initial.body.revision, data: {title: 'Сохранить карточку'}};
  const second = {...first, data: {title: 'Параллельная правка'}};
  const saved = await request(server, 'POST', '/api/features', first);
  assert.equal(saved.code, 200);
  const conflict = await request(server, 'POST', '/api/features', second);
  assert.equal(conflict.code, 409);
  assert.equal(conflict.body.state.revision, saved.body.revision);
  const restarted = await request(featureServer(file), 'GET', '/api/features');
  assert.deepEqual(restarted.body, JSON.parse(readFileSync(file, 'utf8')));
  assert.equal(restarted.body.features.at(-1).title, first.data.title);
  assert.equal(restarted.body.features.at(-1).history.at(-1).author, first.author);
  const rejected = await request(server, 'POST', '/api/features', {...second, revision: saved.body.revision},
    {origin: 'https://example.org'});
  assert.equal(rejected.code, 403);
  assert.equal((await request(server, 'GET', '/')).code, 200);
  assert.equal((await request(server, 'GET', '/docs/workflow/roles.md')).code, 200);
  assert.equal((await request(server, 'GET', '/features.json')).code, 404);
});
