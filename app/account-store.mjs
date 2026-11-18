import {closeSync, lstatSync, mkdirSync, openSync} from 'node:fs';
import {resolve} from 'node:path';
import {DatabaseSync} from 'node:sqlite';
import {fileURLToPath} from 'node:url';

const localDataDir = fileURLToPath(new URL('.local/', import.meta.url));
const schemaVersion = 1;

function ensurePrivateLocation(filename, isDirectory) {
  const stat = lstatSync(filename);
  if ((isDirectory ? !stat.isDirectory() : !stat.isFile()) ||
      (stat.mode & 0o077) !== 0 || (process.getuid && stat.uid !== process.getuid())) {
    throw new Error('Хранилище «Севера» должно принадлежать текущему пользователю и быть закрыто для других.');
  }
}

function databaseFile(dataDir) {
  dataDir = resolve(dataDir);
  mkdirSync(dataDir, {recursive: true, mode: 0o700});
  ensurePrivateLocation(dataDir, true);
  const filename = resolve(dataDir, 'sever.sqlite');
  try {
    closeSync(openSync(filename, 'wx', 0o600));
  } catch (error) {
    if (error.code !== 'EEXIST') throw error;
  }
  ensurePrivateLocation(filename, false);
  return filename;
}

function initialize(database) {
  database.exec('PRAGMA busy_timeout = 5000; BEGIN IMMEDIATE');
  try {
    const {user_version: current} = database.prepare('PRAGMA user_version').get();
    if (current === 0) {
      database.exec(`
        CREATE TABLE accounts (
          id TEXT PRIMARY KEY CHECK (length(id) = 36),
          email TEXT NOT NULL UNIQUE COLLATE NOCASE CHECK (length(email) BETWEEN 3 AND 254),
          name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 120),
          salt BLOB NOT NULL CHECK (length(salt) = 16),
          password_hash BLOB NOT NULL CHECK (length(password_hash) = 32)
        ) STRICT;
        CREATE TABLE sessions (
          token_digest BLOB PRIMARY KEY CHECK (length(token_digest) = 32),
          account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
          created_at INTEGER NOT NULL CHECK (created_at >= 0),
          expires_at INTEGER NOT NULL CHECK (expires_at > created_at)
        ) STRICT;
        CREATE INDEX sessions_by_expiry ON sessions(expires_at);
        CREATE INDEX sessions_by_account ON sessions(account_id);
        PRAGMA user_version = 1;
      `);
    } else if (current !== schemaVersion) {
      throw new Error('Эта версия «Севера» не поддерживает схему хранилища.');
    }
    database.exec('COMMIT');
  } catch (error) {
    database.exec('ROLLBACK');
    throw error;
  }
}

export function openAccountStore(dataDir = process.env.SEVER_DATA_DIR || localDataDir) {
  const database = new DatabaseSync(databaseFile(dataDir), {enableForeignKeyConstraints: true});
  try {
    initialize(database);
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
      addAccount: ({id, email, name, salt, passwordHash}) =>
        insertAccount.run(id, email, name, salt, passwordHash).changes === 1,
      findCredentials: email => selectCredentials.get(email),
      addSession: ({tokenDigest, accountId, createdAt, expiresAt}) =>
        insertSession.run(tokenDigest, accountId, createdAt, expiresAt),
      findPrincipal(tokenDigest, now) {
        const row = selectPrincipal.get(tokenDigest, now);
        if (row) return {id: row.id, name: row.name};
      },
      deleteExpiredSessions: now => deleteExpired.run(now),
      close: () => database.close(),
    };
  } catch (error) {
    database.close();
    throw error;
  }
}
