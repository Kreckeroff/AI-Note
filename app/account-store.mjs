import {openLocalDatabase} from './local-database.mjs';

export function prepareAccountStore(database) {
  const insertAccount = database.prepare(`
    INSERT INTO accounts (id, email, name, salt, password_hash) VALUES (?, ?, ?, ?, ?)
    ON CONFLICT(email) DO NOTHING
  `);
  const selectCredentials = database.prepare('SELECT id, salt, password_hash AS passwordHash FROM accounts WHERE email = ?');
  const insertSession = database.prepare('INSERT INTO sessions (token_digest, account_id, created_at, expires_at) VALUES (?, ?, ?, ?)');
  const selectPrincipal = database.prepare(`
    SELECT accounts.id, accounts.name FROM sessions
    JOIN accounts ON accounts.id = sessions.account_id
    WHERE sessions.token_digest = ? AND sessions.expires_at > ?
  `);
  const deleteExpired = database.prepare('DELETE FROM sessions WHERE expires_at <= ?');
  return {
    addAccount: ({id, email, name, salt, passwordHash}) => insertAccount.run(id, email, name, salt, passwordHash).changes === 1,
    findCredentials: email => selectCredentials.get(email),
    addSession: ({tokenDigest, accountId, createdAt, expiresAt}) => insertSession.run(tokenDigest, accountId, createdAt, expiresAt),
    findPrincipal(tokenDigest, now) {
      const row = selectPrincipal.get(tokenDigest, now);
      if (row) return {id: row.id, name: row.name};
    },
    deleteExpiredSessions: now => deleteExpired.run(now),
  };
}

export function openAccountStore(dataDir) {
  const database = openLocalDatabase(dataDir);
  try {
    return {...prepareAccountStore(database), close: () => database.close()};
  } catch (error) {
    database.close();
    throw error;
  }
}
