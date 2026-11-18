import {createHash, randomUUID} from 'node:crypto';
import {hasSpaceGrantsSchema, writeTransaction} from './local-database.mjs';

function prepareReadableSpaces(database, withGrants) {
  const grantJoin = withGrants ? `LEFT JOIN space_grants AS access
    ON access.tenant_id = spaces.tenant_id AND access.space_id = spaces.id AND access.account_id = membership.account_id` : '';
  const grantRole = withGrants ? 'access.role' : 'NULL';
  const granted = withGrants ? 'OR access.role IS NOT NULL' : '';
  return database.prepare(`
    SELECT spaces.id, spaces.name, accounts.name AS ownerName, spaces.owner_account_id = membership.account_id AS isOwner,
      membership.role AS tenantRole, ${grantRole} AS grantRole
    FROM tenant_memberships AS membership
    JOIN spaces ON spaces.tenant_id = membership.tenant_id
    JOIN accounts ON accounts.id = spaces.owner_account_id
    ${grantJoin}
    WHERE membership.tenant_id = ? AND membership.account_id = ? AND membership.status = 'active'
      AND (spaces.owner_account_id = membership.account_id OR membership.role IN ('admin', 'superadmin') ${granted})
  `);
}

export function prepareWorkspaceStore(database) {
  const tenantForMembership = `
    SELECT tenants.id, tenants.name, membership.role, membership.can_create AS canCreate
    FROM tenant_memberships AS membership
    JOIN tenants ON tenants.id = membership.tenant_id
    WHERE membership.account_id = ? AND membership.status = 'active'
  `;
  const selectCurrentTenant = database.prepare(`${tenantForMembership} AND membership.is_current = 1`);
  const selectTenant = database.prepare(`${tenantForMembership} AND membership.tenant_id = ?`);
  const selectTenants = database.prepare(`${tenantForMembership} ORDER BY tenants.name, tenants.id`);
  const readableWithoutGrants = prepareReadableSpaces(database, false);
  let readableWithGrants;
  const selectOperation = database.prepare(`
    SELECT request_fingerprint AS fingerprint, space_id AS spaceId
    FROM space_creations WHERE tenant_id = ? AND creator_account_id = ? AND operation_id = ?
  `);
  const insertSpace = database.prepare(`
    INSERT INTO spaces (tenant_id, id, creator_account_id, owner_account_id, name, name_key) VALUES (?, ?, ?, ?, ?, ?)
    ON CONFLICT(tenant_id, owner_account_id, name_key) DO NOTHING
  `);
  const insertOperation = database.prepare(`
    INSERT INTO space_creations (tenant_id, creator_account_id, operation_id, request_fingerprint, space_id) VALUES (?, ?, ?, ?, ?)
  `);

  return {
    findCurrentTenant: accountId => selectCurrentTenant.get(accountId),
    findTenant: (accountId, tenantId) => selectTenant.get(accountId, tenantId),
    listTenants: accountId => selectTenants.all(accountId),
    listReadableSpaces(tenantId, accountId) {
      if (!readableWithGrants && hasSpaceGrantsSchema(database)) readableWithGrants = prepareReadableSpaces(database, true);
      return (readableWithGrants ?? readableWithoutGrants).all(tenantId, accountId);
    },
    createSpace({tenantId, accountId, operationId, enteredName}) {
      try {
        return writeTransaction(database, () => {
          const membership = selectTenant.get(accountId, tenantId);
          if (!membership) return {outcome: 'not-found'};
          if (!membership.canCreate) return {outcome: 'forbidden'};

          const name = enteredName.trim();
          const fingerprint = createHash('sha256').update(name, 'utf8').digest('hex');
          const existing = selectOperation.get(tenantId, accountId, operationId);
          if (existing) return existing.fingerprint === fingerprint
            ? {outcome: 'replayed', spaceId: existing.spaceId}
            : {outcome: 'operation-conflict'};

          if (!name) return {outcome: 'empty-name'};
          if (Array.from(name).length > 100) return {outcome: 'long-name'};
          const nameKey = name.normalize('NFC').toLowerCase().normalize('NFC');
          const spaceId = randomUUID();
          if (!insertSpace.run(tenantId, spaceId, accountId, accountId, name, nameKey).changes) {
            return {outcome: 'duplicate'};
          }
          insertOperation.run(tenantId, accountId, operationId, fingerprint, spaceId);
          return {outcome: 'created', spaceId};
        });
      } catch (error) {
        if (error.code !== 'ERR_SQLITE_ERROR') throw error;
        return {outcome: 'unavailable'};
      }
    },
  };
}
