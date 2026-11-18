import {readFileSync} from 'node:fs';
import {configureWorkspaceDatabase, openLocalDatabase} from './local-database.mjs';
import {isWorkspaceId} from './workspace-paths.mjs';

function readConfiguration(filename) {
  const configuration = JSON.parse(readFileSync(filename, 'utf8'));
  if (!Array.isArray(configuration?.tenants) || !Array.isArray(configuration.memberships)) {
    throw new Error('Укажите массивы tenants и memberships.');
  }
  const {tenants, memberships} = configuration;
  const tenantIds = new Set();
  for (const entry of tenants) {
    if (!entry || !isWorkspaceId(entry.id) || typeof entry.name !== 'string' || !entry.name.trim() ||
        Array.from(entry.name.trim()).length > 120 || /\p{Cc}/u.test(entry.name) || tenantIds.has(entry.id)) {
      throw new Error('Каждому тенанту нужны уникальный UUID и название длиной от 1 до 120 символов.');
    }
    tenantIds.add(entry.id);
  }

  const memberKeys = new Set();
  const currentAccounts = new Set();
  for (const entry of memberships) {
    if (!entry || !isWorkspaceId(entry.tenantId) || typeof entry.email !== 'string' || !entry.email.trim() ||
        !['member', 'admin', 'superadmin'].includes(entry.role) || !['active', 'inactive'].includes(entry.status) ||
        typeof entry.canCreate !== 'boolean' || typeof entry.current !== 'boolean' || (entry.current && entry.status !== 'active')) {
      throw new Error('Проверьте данные членства, роль, статус и флаги доступа.');
    }
    const email = entry.email.trim().toLowerCase();
    const key = `${entry.tenantId}:${email}`;
    if (memberKeys.has(key) || (entry.current && currentAccounts.has(email))) {
      throw new Error('Членство указывают один раз. У аккаунта может быть один текущий тенант.');
    }
    memberKeys.add(key);
    if (entry.current) currentAccounts.add(email);
  }
  return configuration;
}

function applyConfiguration(database, {tenants, memberships}) {
  const upsertTenant = database.prepare(`
    INSERT INTO tenants (id, name) VALUES (?, ?)
    ON CONFLICT(id) DO UPDATE SET name = excluded.name
  `);
  const accountByEmail = database.prepare('SELECT id FROM accounts WHERE email = ?');
  const clearCurrent = database.prepare('UPDATE tenant_memberships SET is_current = 0 WHERE account_id = ? AND is_current = 1');
  const upsertMembership = database.prepare(`
    INSERT INTO tenant_memberships (tenant_id, account_id, role, status, can_create, is_current) VALUES (?, ?, ?, ?, ?, ?)
    ON CONFLICT(tenant_id, account_id) DO UPDATE SET role = excluded.role, status = excluded.status,
      can_create = excluded.can_create, is_current = excluded.is_current
  `);
  for (const {id, name} of tenants) upsertTenant.run(id, name.trim());
  for (const {tenantId, email, role, status, canCreate, current} of memberships) {
    const account = accountByEmail.get(email.trim());
    if (!account) throw new Error('Сначала создайте все учётные записи, указанные в членствах.');
    if (current) clearCurrent.run(account.id);
    upsertMembership.run(tenantId, account.id, role, status, Number(canCreate), Number(current));
  }
}

const [filename, ...other] = process.argv.slice(2);
if (!filename || other.length || !process.env.SEVER_DATA_DIR) {
  console.error('Использование: SEVER_DATA_DIR=<папка данных> node app/configure-tenants.mjs <конфигурация.json>');
  process.exitCode = 1;
} else {
  let database;
  try {
    const configuration = readConfiguration(filename);
    database = openLocalDatabase();
    configureWorkspaceDatabase(database, () => applyConfiguration(database, configuration));
    console.log('Настройки тенантов и членств сохранены.');
  } catch (error) {
    console.error(error.code || error instanceof SyntaxError ? 'Не удалось применить настройки тенантов. Проверьте файл и папку данных.' : error.message);
    process.exitCode = 1;
  } finally {
    database?.close();
  }
}
