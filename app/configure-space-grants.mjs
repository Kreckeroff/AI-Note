import {readFileSync} from 'node:fs';
import {normalizeEmail} from './identity.mjs';
import {configureSpaceGrantsDatabase, openLocalDatabase} from './local-database.mjs';
import {isWorkspaceId} from './workspace-paths.mjs';

function readGrants(filename) {
  const {grants} = JSON.parse(readFileSync(filename, 'utf8')) ?? {};
  if (!Array.isArray(grants)) throw new Error('Укажите массив grants.');
  const unique = new Set();
  return grants.map(entry => {
    const {tenantId, spaceId, email, role} = entry ?? {};
    const normalized = typeof email === 'string' && normalizeEmail(email);
    if (!isWorkspaceId(tenantId) || !isWorkspaceId(spaceId) || !normalized || !['viewer', 'editor', null].includes(role)) {
      throw new Error('Проверьте UUID пространства и тенанта, адрес участника и роль.');
    }
    const key = `${tenantId}:${spaceId}:${normalized}`;
    if (unique.has(key)) throw new Error('Назначение для одного участника и пространства указывают один раз.');
    unique.add(key);
    return {tenantId, spaceId, email: normalized, role};
  });
}

function applyGrants(database, grants) {
  const findSpace = database.prepare('SELECT 1 FROM spaces WHERE tenant_id = ? AND id = ?');
  const findMember = database.prepare(`
    SELECT membership.account_id AS accountId, membership.status
    FROM accounts JOIN tenant_memberships AS membership ON membership.account_id = accounts.id
    WHERE membership.tenant_id = ? AND accounts.email = ?
  `);
  const grant = database.prepare(`
    INSERT INTO space_grants (tenant_id, space_id, account_id, role) VALUES (?, ?, ?, ?)
    ON CONFLICT(tenant_id, space_id, account_id) DO UPDATE SET role = excluded.role
  `);
  const revoke = database.prepare('DELETE FROM space_grants WHERE tenant_id = ? AND space_id = ? AND account_id = ?');
  for (const {tenantId, spaceId, email, role} of grants) {
    if (!findSpace.get(tenantId, spaceId)) throw new Error('Проверьте, что пространство существует в указанном тенанте.');
    const member = findMember.get(tenantId, email);
    if (!member || (role !== null && member.status !== 'active')) throw new Error('Назначение требует действующего членства в том же тенанте.');
    if (role === null) revoke.run(tenantId, spaceId, member.accountId);
    else grant.run(tenantId, spaceId, member.accountId, role);
  }
}

const [filename, ...other] = process.argv.slice(2);
if (!filename || other.length || !process.env.SEVER_DATA_DIR) {
  console.error('Использование: SEVER_DATA_DIR=<папка данных> node app/configure-space-grants.mjs <назначения.json>');
  process.exitCode = 1;
} else {
  let database;
  try {
    const grants = readGrants(filename);
    database = openLocalDatabase();
    configureSpaceGrantsDatabase(database, () => applyGrants(database, grants));
    console.log(`Настройки доступа сохранены. Обработано назначений: ${grants.length}.`);
  } catch (error) {
    console.error(error.code || error instanceof SyntaxError ? 'Не удалось применить доступы. Проверьте файл и папку данных.' : error.message);
    process.exitCode = 1;
  } finally {
    database?.close();
  }
}
