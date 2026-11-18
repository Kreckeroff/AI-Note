import {readFileSync} from 'node:fs';
import {createServer} from 'node:http';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {openAccountStore} from './account-store.mjs';
import {createIdentity, emailMaxLength, sessionLifetime} from './identity.mjs';
import {signInPage, workspacePage} from './views.mjs';

const loopback = '127.0.0.1';
const port = 4121;
const cookieName = 'sever_session';
const workspacePath = '/workspace';
const maxFormBytes = 4_096;
const assets = new Map([
  ['/auth.css', {type: 'text/css; charset=utf-8', file: 'auth.css'}],
  ['/sign-in.mjs', {type: 'text/javascript; charset=utf-8', file: 'sign-in.mjs'}],
]);
const securityHeaders = {
  'Cache-Control': 'no-store',
  'Content-Security-Policy': "default-src 'none'; script-src 'self'; style-src 'self'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'",
  'Referrer-Policy': 'same-origin',
  'X-Content-Type-Options': 'nosniff',
};

function send(response, status, body, headers = {}) {
  if (response.destroyed) return;
  response.writeHead(status, {
    ...securityHeaders,
    'Content-Type': 'text/html; charset=utf-8',
    ...headers,
  });
  response.end(body);
}

function redirect(response, location, headers = {}) {
  send(response, 303, '', {Location: location, ...headers});
}

function plain(response, status, message, headers = {}) {
  send(response, status, message, {'Content-Type': 'text/plain; charset=utf-8', ...headers});
}

async function readSignInForm(request) {
  if (Number(request.headers['content-length']) > maxFormBytes) return undefined;
  const chunks = [];
  let length = 0;
  for await (const chunk of request.iterator({destroyOnReturn: false})) {
    length += chunk.length;
    if (length > maxFormBytes) return undefined;
    chunks.push(chunk);
  }
  const fields = new URLSearchParams(Buffer.concat(chunks).toString('utf8'));
  const email = fields.getAll('email');
  const password = fields.getAll('password');
  return {
    email: email.length === 1 ? email[0] : '',
    password: password.length === 1 ? password[0] : '',
  };
}

function sessionCookie(token) {
  return `${cookieName}=${token}; Path=${workspacePath}; HttpOnly; SameSite=Strict; Max-Age=${sessionLifetime / 1000}`;
}

function sessionToken(request) {
  const entries = (request.headers.cookie || '').split(';')
    .map(cookie => cookie.trim())
    .filter(cookie => cookie.startsWith(`${cookieName}=`));
  return entries.length === 1 ? entries[0].slice(cookieName.length + 1) : undefined;
}

async function handleSignIn(request, response, origin, identity, record) {
  if (request.headers.origin !== origin) return plain(response, 403, 'Откройте форму входа в «Север».');
  const mediaType = request.headers['content-type']?.split(';', 1)[0].trim().toLowerCase();
  if (mediaType !== 'application/x-www-form-urlencoded') {
    return plain(response, 415, 'Откройте форму входа в «Север».');
  }
  const form = await readSignInForm(request);
  if (!form) return plain(response, 413, 'Форма входа слишком большая.', {Connection: 'close'});

  const result = await identity.signIn(form);
  const email = form.email.slice(0, emailMaxLength);
  if (result.outcome === 'accepted') {
    record('sign-in.accepted', {accountId: result.accountId});
    return redirect(response, workspacePath, {'Set-Cookie': sessionCookie(result.token)});
  }
  if (result.outcome === 'limited') {
    record('sign-in.limited');
    const error = 'Слишком много попыток входа. Подождите немного и попробуйте снова.';
    return send(response, 429, signInPage({email, error}), {'Retry-After': String(result.retryAfter)});
  }
  record('sign-in.rejected');
  return send(response, 422, signInPage({email, error: 'Неверная почта или пароль.'}));
}

export function createSeverServer({dataDir, clock = Date.now, record = () => {}} = {}) {
  const contents = new Map(Array.from(assets, ([path, asset]) => [path, {
    type: asset.type,
    body: readFileSync(new URL(asset.file, import.meta.url)),
  }]));
  const store = openAccountStore(dataDir);
  const identity = createIdentity(store, clock);

  async function handle(request, response) {
    const host = `${loopback}:${request.socket.localPort}`;
    if (request.socket.remoteAddress !== loopback || request.headers.host !== host) {
      return plain(response, 403, 'Откройте «Север» по локальному адресу.');
    }
    const path = request.url.split('?', 1)[0];
    const isRead = request.method === 'GET' || request.method === 'HEAD';

    if (path === '/login') {
      if (request.method !== 'POST') return plain(response, 405, 'Метод недоступен.', {Allow: 'POST'});
      return handleSignIn(request, response, `http://${host}`, identity, record);
    }
    const asset = contents.get(path);
    if (path !== '/' && path !== workspacePath && !asset) return plain(response, 404, 'Страница не найдена.');
    if (!isRead) return plain(response, 405, 'Метод недоступен.', {Allow: 'GET, HEAD'});
    if (asset) return send(response, 200, asset.body, {'Content-Type': asset.type});
    if (path === '/') return send(response, 200, signInPage());

    const account = identity.identify(sessionToken(request));
    if (!account) return redirect(response, '/');
    return send(response, 200, workspacePage(account.name));
  }

  const server = createServer({requestTimeout: 15_000, headersTimeout: 10_000}, (request, response) => {
    handle(request, response).catch(() => {
      if (response.destroyed) return;
      record('request.failed');
      send(response, 503, signInPage({error: 'Не удалось открыть «Север». Попробуйте ещё раз.'}));
    });
  });
  server.on('close', () => store.close());
  return server;
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  try {
    const record = (event, details) => console.info(JSON.stringify({at: new Date().toISOString(), event, ...details}));
    const server = createSeverServer({record});
    server.on('error', () => {
      console.error('Локальный адрес «Севера» недоступен. Проверьте, свободен ли порт 4121.');
      process.exitCode = 1;
    });
    server.listen(port, loopback, () => console.log(`Север · http://${loopback}:${port}/`));
  } catch (error) {
    console.error(error.code ? 'Не удалось открыть локальное хранилище «Севера».' : error.message);
    process.exitCode = 1;
  }
}
