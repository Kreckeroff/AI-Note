import {randomUUID} from 'node:crypto';
import {writeTransaction} from './local-database.mjs';

export function prepareRegistrationStore(database, accounts) {
  const insertTenant = database.prepare('INSERT INTO tenants (id, name) VALUES (?, ?)');
  const insertMembership = database.prepare(`
    INSERT INTO tenant_memberships (tenant_id, account_id, role, status, can_create, is_current)
    VALUES (?, ?, ?, ?, ?, ?)
  `);

  return {
    hasAccount: email => Boolean(accounts.findCredentials(email)),
    createAccountWorkspaceAndSession({account, session, tenantName, membership}) {
      return writeTransaction(database, () => {
        if (!accounts.addAccount(account)) return undefined;
        const tenantId = randomUUID();
        insertTenant.run(tenantId, tenantName);
        const {role, status, canCreate, current} = membership;
        insertMembership.run(tenantId, account.id, role, status, Number(canCreate), Number(current));
        accounts.deleteExpiredSessions(session.createdAt);
        accounts.addSession({...session, accountId: account.id});
        return tenantId;
      });
    },
  };
}
