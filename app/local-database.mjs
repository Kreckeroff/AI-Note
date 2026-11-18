import {closeSync, lstatSync, mkdirSync, openSync} from 'node:fs';
import {resolve} from 'node:path';
import {DatabaseSync} from 'node:sqlite';
import {fileURLToPath} from 'node:url';

const localDataDir = fileURLToPath(new URL('.local/', import.meta.url));
const authSchemaVersion = 1;
const workspaceSchemaVersion = 2;
const grantsSchemaVersion = 3;

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

export function writeTransaction(database, write) {
  database.exec('BEGIN IMMEDIATE');
  try {
    const result = write();
    database.exec('COMMIT');
    return result;
  } catch (error) {
    if (database.isTransaction) database.exec('ROLLBACK');
    throw error;
  }
}

const currentSchemaVersion = database => database.prepare('PRAGMA user_version').get().user_version;

function initializeAuth(database) {
  database.exec('PRAGMA busy_timeout = 5000');
  const current = currentSchemaVersion(database);
  if (![0, authSchemaVersion, workspaceSchemaVersion, grantsSchemaVersion].includes(current)) throw new Error('Эта версия «Севера» не поддерживает схему хранилища.');
  if (current !== 0) return;
  writeTransaction(database, () => {
    if (currentSchemaVersion(database) !== 0) return;
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
        PRAGMA user_version = ${authSchemaVersion};
    `);
  });
}

export function hasWorkspaceSchema(database) {
  return currentSchemaVersion(database) >= workspaceSchemaVersion;
}

export function hasSpaceGrantsSchema(database) {
  return currentSchemaVersion(database) === grantsSchemaVersion;
}

export function configureSpaceGrantsDatabase(database, configure) {
  return writeTransaction(database, () => {
    if (!hasWorkspaceSchema(database)) throw new Error('Сначала настройте тенанты и членства.');
    if (!hasSpaceGrantsSchema(database)) {
      database.exec(`
        CREATE TABLE space_grants (
          tenant_id TEXT NOT NULL,
          space_id TEXT NOT NULL,
          account_id TEXT NOT NULL,
          role TEXT NOT NULL CHECK (role IN ('viewer', 'editor')),
          PRIMARY KEY (tenant_id, space_id, account_id),
          FOREIGN KEY (tenant_id, space_id) REFERENCES spaces(tenant_id, id) ON DELETE RESTRICT,
          FOREIGN KEY (tenant_id, account_id) REFERENCES tenant_memberships(tenant_id, account_id) ON DELETE RESTRICT
        ) STRICT;
        CREATE INDEX space_grants_by_member ON space_grants(tenant_id, account_id, space_id);
        PRAGMA user_version = ${grantsSchemaVersion};
      `);
    }
    return configure();
  });
}

export function configureWorkspaceDatabase(database, configure) {
  return writeTransaction(database, () => {
    if (!hasWorkspaceSchema(database)) {
      database.exec(`
        CREATE TABLE tenants (
          id TEXT PRIMARY KEY CHECK (length(id) = 36),
          name TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 120)
        ) STRICT;
        CREATE TABLE tenant_memberships (
          tenant_id TEXT NOT NULL REFERENCES tenants(id) ON DELETE RESTRICT,
          account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
          role TEXT NOT NULL CHECK (role IN ('member', 'admin', 'superadmin')),
          status TEXT NOT NULL CHECK (status IN ('active', 'inactive')),
          can_create INTEGER NOT NULL CHECK (can_create IN (0, 1)),
          is_current INTEGER NOT NULL CHECK (is_current IN (0, 1) AND (is_current = 0 OR status = 'active')),
          PRIMARY KEY (tenant_id, account_id)
        ) STRICT;
        CREATE INDEX tenant_memberships_by_account ON tenant_memberships(account_id);
        CREATE UNIQUE INDEX tenant_memberships_current ON tenant_memberships(account_id) WHERE is_current = 1;
        CREATE TABLE spaces (
          tenant_id TEXT NOT NULL,
          id TEXT NOT NULL CHECK (length(id) = 36),
          creator_account_id TEXT NOT NULL,
          owner_account_id TEXT NOT NULL,
          name TEXT NOT NULL CHECK (length(CAST(name AS BLOB)) BETWEEN 1 AND 400),
          name_key TEXT NOT NULL,
          PRIMARY KEY (tenant_id, id),
          UNIQUE (tenant_id, owner_account_id, name_key),
          FOREIGN KEY (tenant_id, creator_account_id) REFERENCES tenant_memberships(tenant_id, account_id) ON DELETE RESTRICT,
          FOREIGN KEY (tenant_id, owner_account_id) REFERENCES tenant_memberships(tenant_id, account_id) ON DELETE RESTRICT
        ) STRICT;
        CREATE INDEX spaces_by_creator ON spaces(tenant_id, creator_account_id);
        CREATE TABLE space_creations (
          tenant_id TEXT NOT NULL,
          creator_account_id TEXT NOT NULL,
          operation_id TEXT NOT NULL CHECK (length(operation_id) = 36),
          request_fingerprint TEXT NOT NULL CHECK (length(request_fingerprint) = 64),
          space_id TEXT NOT NULL,
          PRIMARY KEY (tenant_id, creator_account_id, operation_id),
          FOREIGN KEY (tenant_id, creator_account_id) REFERENCES tenant_memberships(tenant_id, account_id) ON DELETE RESTRICT,
          FOREIGN KEY (tenant_id, space_id) REFERENCES spaces(tenant_id, id) ON DELETE RESTRICT
        ) STRICT;
        CREATE INDEX space_creations_by_space ON space_creations(tenant_id, space_id);
        PRAGMA user_version = ${workspaceSchemaVersion};
      `);
    }
    return configure();
  });
}

export function openLocalDatabase(dataDir = process.env.SEVER_DATA_DIR || localDataDir) {
  const database = new DatabaseSync(databaseFile(dataDir), {enableForeignKeyConstraints: true});
  try {
    initializeAuth(database);
    return database;
  } catch (error) {
    database.close();
    throw error;
  }
}
