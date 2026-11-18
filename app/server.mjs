import {readFileSync} from 'node:fs';
import {createServer} from 'node:http';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {prepareAccountStore} from './account-store.mjs';
import {formValue, hasFormEncoding, plain, readForm, redirect, send} from './http.mjs';
import {createIdentity, emailMaxLength, sessionLifetime} from './identity.mjs';
import {hasWorkspaceSchema, openLocalDatabase} from './local-database.mjs';
import {createRegistration} from './registration.mjs';
import {createRegistrationHttp} from './registration-http.mjs';
import {createRegistrationDelivery} from './registration-mail.mjs';
import {isRegistrationPath} from './registration-paths.mjs';
import {prepareRegistrationStore} from './registration-store.mjs';
import {signInPage, workspacePage} from './views.mjs';
import {createSpaceHttp} from './workspace-http.mjs';
import {catalogOptions, localWorkspaceTarget, parseSpacePath, workspacePath} from './workspace-paths.mjs';
import {prepareWorkspaceStore} from './workspace-store.mjs';
import {catalogUnavailablePage, spaceNotFoundPage} from './workspace-views.mjs';

const loopback = '127.0.0.1';
const port = 4121;
const cookieName = 'sever_session';
const assets = new Map([
  ['/auth.css', {type: 'text/css; charset=utf-8', file: 'auth.css'}],
  ['/sign-in.mjs', {type: 'text/javascript; charset=utf-8', file: 'sign-in.mjs'}],
  ['/spaces.css', {type: 'text/css; charset=utf-8', file: 'spaces.css'}],
  ['/space-form.mjs', {type: 'text/javascript; charset=utf-8', file: 'space-form.mjs'}],
  ['/workspace-menu.mjs', {type: 'text/javascript; charset=utf-8', file: 'workspace-menu.mjs'}],
]);
function sessionCookie(token) {
  return `${cookieName}=${token}; Path=${workspacePath}; HttpOnly; SameSite=Strict; Max-Age=${sessionLifetime / 1000}`;
}

function sessionToken(request) {
  const entries = (request.headers.cookie || '').split(';')
    .map(cookie => cookie.trim())
    .filter(cookie => cookie.startsWith(`${cookieName}=`));
  return entries.length === 1 ? entries[0].slice(cookieName.length + 1) : undefined;
}

async function handleSignIn(request, response, origin, identity, record, registrationEnabled) {
  if (request.headers.origin !== origin) return plain(response, 403, 'Откройте форму входа в «Север».');
  if (!hasFormEncoding(request)) {
    return plain(response, 415, 'Откройте форму входа в «Север».');
  }
  const form = await readForm(request);
  if (!form) return plain(response, 413, 'Форма входа слишком большая.', {Connection: 'close'});

  const enteredEmail = formValue(form, 'email');
  const returnTo = localWorkspaceTarget(formValue(form, 'returnTo'));
  const result = await identity.signIn({email: enteredEmail, password: formValue(form, 'password')});
  const email = enteredEmail.slice(0, emailMaxLength);
  if (result.outcome === 'accepted') {
    record('sign-in.accepted', {accountId: result.accountId});
    return redirect(response, returnTo || workspacePath, {'Set-Cookie': sessionCookie(result.token)});
  }
  if (result.outcome === 'limited') {
    record('sign-in.limited');
    const error = 'Слишком много попыток входа. Подождите немного и попробуйте снова.';
    return send(response, 429, signInPage({email, error, registrationEnabled, returnTo}), {'Retry-After': String(result.retryAfter)});
  }
  record('sign-in.rejected');
  return send(response, 422, signInPage({email, error: 'Неверная почта или пароль.', registrationEnabled, returnTo}));
}

export function createSeverServer({dataDir, clock = Date.now, record = () => {}, environment = process.env, deliverRegistration} = {}) {
  const deliver = createRegistrationDelivery(environment, deliverRegistration);
  const registrationEnabled = Boolean(deliver);
  const contents = new Map(Array.from(assets, ([path, asset]) => [path, {
    type: asset.type,
    body: readFileSync(new URL(asset.file, import.meta.url)),
  }]));
  const database = openLocalDatabase(dataDir);
  let identity;
  let workspaces;
  let handleRegistration;
  try {
    const accounts = prepareAccountStore(database);
    identity = createIdentity(accounts, clock);
    const hasWorkspace = hasWorkspaceSchema(database);
    if (hasWorkspace) workspaces = prepareWorkspaceStore(database);
    if (deliver) {
      if (!hasWorkspace) throw new Error('Подготовьте схему тенантов через app/configure-tenants.mjs для SEVER_SELF_REGISTRATION=true.');
      const store = prepareRegistrationStore(database, accounts);
      const registration = createRegistration({store, deliver, clock, record});
      handleRegistration = createRegistrationHttp(registration, {sessionCookie, record});
    }
  } catch (error) {
    database.close();
    throw error;
  }
  const handleSpace = workspaces && createSpaceHttp(workspaces, record);

  async function handle(request, response) {
    const host = `${loopback}:${request.socket.localPort}`;
    if (request.socket.remoteAddress !== loopback || request.headers.host !== host) {
      return plain(response, 403, 'Откройте «Север» по локальному адресу.');
    }
    const path = request.url.split('?', 1)[0];
    const query = new URLSearchParams(request.url.slice(path.length + 1));
    const isRead = request.method === 'GET' || request.method === 'HEAD';
    const origin = `http://${host}`;

    if (path === '/login') {
      if (request.method !== 'POST') return plain(response, 405, 'Метод недоступен.', {Allow: 'POST'});
      return handleSignIn(request, response, origin, identity, record, registrationEnabled);
    }
    if (isRegistrationPath(path)) {
      if (!handleRegistration) return plain(response, 404, 'Страница не найдена.');
      return handleRegistration(request, response, {path, origin});
    }
    const asset = contents.get(path);
    const workspaceRoute = parseSpacePath(path);
    if (path !== '/' && path !== workspacePath && !asset && !workspaceRoute) return plain(response, 404, 'Страница не найдена.');
    const isCollection = workspaceRoute && workspaceRoute[2] === undefined;
    if (!isRead && !(isCollection && request.method === 'POST')) {
      return plain(response, 405, 'Метод недоступен.', {Allow: isCollection ? 'GET, HEAD, POST' : 'GET, HEAD'});
    }
    if (asset) return send(response, 200, asset.body, {'Content-Type': asset.type});
    if (path === '/') return send(response, 200, signInPage({registrationEnabled, returnTo: localWorkspaceTarget(formValue(query, 'returnTo'))}));

    const account = identity.identify(sessionToken(request));
    if (!account) {
      const returnTo = isRead && localWorkspaceTarget(request.url);
      return redirect(response, returnTo && returnTo !== workspacePath ? `/?${new URLSearchParams({returnTo})}` : '/');
    }
    const options = catalogOptions(query);
    if (path === workspacePath) {
      return handleSpace ? handleSpace.current(response, account, options) : send(response, 200, workspacePage(account.name));
    }

    const [, tenantId, target] = workspaceRoute;
    if (!handleSpace) {
      record(request.method === 'POST' ? 'space.create' : 'space.open', {accountId: account.id, tenantId, outcome: 'not-found'});
      return send(response, 404, spaceNotFoundPage());
    }
    return handleSpace.handle(request, response, {origin, account, tenantId, target, options});
  }

  const server = createServer({requestTimeout: 15_000, headersTimeout: 10_000}, (request, response) => {
    handle(request, response).catch(() => {
      if (response.destroyed) return;
      record('request.failed');
      const [path] = request.url.split('?', 1);
      const route = parseSpacePath(path);
      if ((request.method === 'GET' || request.method === 'HEAD') && (path === workspacePath || route && route[2] === undefined)) {
        return send(response, 503, catalogUnavailablePage(localWorkspaceTarget(request.url) || workspacePath));
      }
      const error = 'Не удалось открыть «Север». Попробуйте ещё раз.';
      send(response, 503, signInPage({error, registrationEnabled}), {'Referrer-Policy': 'strict-origin'});
    });
  });
  server.on('close', () => database.close());
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
