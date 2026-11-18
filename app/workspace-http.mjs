import {randomUUID} from 'node:crypto';
import {formValue, hasFormEncoding, plain, readForm, redirect, send} from './http.mjs';
import {workspacePage} from './views.mjs';
import {catalogPage, sortSpaces} from './workspace-catalog.mjs';
import {isWorkspaceId, spacePagePath, withCatalogOptions} from './workspace-paths.mjs';
import {closedSpacePage, createSpacePage, spaceNotFoundPage, spacesCatalogPage} from './workspace-views.mjs';

const creationDenied = 'У вас нет права создавать пространство в этой среде.';
const denySpaceRead = response => send(response, 404, spaceNotFoundPage());

export function createSpaceHttp(workspaces, record) {
  function workspaceContext(tenant, account, options) {
    const availableSpaces = sortSpaces(workspaces.listReadableSpaces(tenant.id, account.id));
    return {tenant, accountName: account.name, availableSpaces, options, tenants: workspaces.listTenants(account.id)};
  }

  function withCatalog(context) {
    const catalog = catalogPage(context.availableSpaces, context.options);
    return {...context, catalog, options: catalog.options};
  }

  async function submitSpace(request, response, {origin, account, tenant, options, recordOutcome}) {
    if (request.headers.origin !== origin) {
      recordOutcome('foreign-origin');
      return plain(response, 403, 'Откройте форму создания пространства в «Севере».');
    }
    if (!hasFormEncoding(request)) {
      recordOutcome('invalid-encoding');
      return plain(response, 415, 'Откройте форму создания пространства в «Севере».');
    }
    const form = await readForm(request);
    if (!form) {
      recordOutcome('form-too-large');
      return plain(response, 413, 'Форма слишком большая.', {Connection: 'close'});
    }
    const enteredName = formValue(form, 'name');
    const operationId = formValue(form, 'operation_id');
    const renderForm = (status, error, flags = {}) => send(response, status, createSpacePage({
      ...withCatalog(workspaceContext(tenant, account, options)), operationId, name: enteredName, error, ...flags,
    }));
    if (!isWorkspaceId(operationId)) {
      recordOutcome('invalid-operation');
      return renderForm(422, 'Не удалось подтвердить отправку. Откройте новую форму.', {operationId: randomUUID(), newForm: true});
    }
    const result = workspaces.createSpace({tenantId: tenant.id, accountId: account.id, operationId, enteredName});
    recordOutcome(result.outcome);
    switch (result.outcome) {
      case 'created':
      case 'replayed':
        return redirect(response, withCatalogOptions(spacePagePath(tenant.id, result.spaceId), options));
      case 'duplicate':
        return renderForm(409, 'У вас уже есть пространство с таким названием.', {invalidName: true});
      case 'empty-name':
        return renderForm(422, 'Введите название пространства.', {invalidName: true});
      case 'long-name':
        return renderForm(422, 'Введите не больше 100 символов.', {invalidName: true});
      case 'operation-conflict':
        return renderForm(409, 'Эта отправка уже использована. Откройте новую форму.', {newForm: true});
      case 'forbidden':
        return plain(response, 403, creationDenied);
      case 'not-found':
        return denySpaceRead(response);
      case 'unavailable':
        return renderForm(503, 'Не удалось сохранить пространство. Попробуйте ещё раз.');
      default:
        throw new Error('Неизвестный результат создания пространства.');
    }
  }

  function current(response, account, options) {
    const tenant = workspaces.findCurrentTenant(account.id);
    if (!tenant) return send(response, 200, workspacePage(account.name, workspaces.listTenants(account.id)));
    return send(response, 200, spacesCatalogPage(withCatalog(workspaceContext(tenant, account, options))));
  }

  function handle(request, response, {origin, account, tenantId, target, options}) {
    const creating = request.method === 'POST';
    const tenant = workspaces.findTenant(account.id, tenantId);
    const recordOutcome = outcome => record(creating ? 'space.create' : 'space.open', {
      accountId: account.id, tenantId, outcome,
    });
    if (!tenant) {
      recordOutcome('not-found');
      return denySpaceRead(response);
    }
    if (!creating && target === undefined) {
      return send(response, 200, spacesCatalogPage(withCatalog(workspaceContext(tenant, account, options))));
    }
    if (!creating && target !== 'new') {
      const context = workspaceContext(tenant, account, options);
      const space = context.availableSpaces.find(row => row.id === target);
      if (!space) {
        recordOutcome('not-found');
        return denySpaceRead(response);
      }
      recordOutcome('opened');
      return send(response, 200, closedSpacePage({...context, space}));
    }
    if (!tenant.canCreate) {
      recordOutcome('forbidden');
      return plain(response, 403, creationDenied);
    }
    if (creating) return submitSpace(request, response, {origin, account, tenant, options, recordOutcome});
    recordOutcome('form-opened');
    return send(response, 200, createSpacePage({...withCatalog(workspaceContext(tenant, account, options)), operationId: randomUUID()}));
  }

  return {current, handle};
}
