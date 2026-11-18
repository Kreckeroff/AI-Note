CREATE TABLE accounts (
    id uuid PRIMARY KEY,
    email text COLLATE "C" NOT NULL UNIQUE
        CHECK (length(email) BETWEEN 3 AND 254 AND email = lower(email)),
    name text NOT NULL CHECK (length(name) BETWEEN 1 AND 120),
    salt bytea NOT NULL CHECK (octet_length(salt) = 16),
    password_hash bytea NOT NULL CHECK (octet_length(password_hash) = 32)
);

CREATE TABLE sessions (
    token_digest bytea PRIMARY KEY CHECK (octet_length(token_digest) = 32),
    account_id uuid NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    created_at bigint NOT NULL CHECK (created_at >= 0),
    expires_at bigint NOT NULL CHECK (expires_at > created_at)
);
CREATE INDEX sessions_by_expiry ON sessions(expires_at);
CREATE INDEX sessions_by_account ON sessions(account_id);

CREATE TABLE tenants (
    id uuid PRIMARY KEY,
    name text NOT NULL CHECK (length(name) BETWEEN 1 AND 120)
);

CREATE TABLE tenant_memberships (
    tenant_id uuid NOT NULL REFERENCES tenants(id),
    account_id uuid NOT NULL REFERENCES accounts(id),
    role text NOT NULL CHECK (role IN ('member', 'admin', 'superadmin')),
    status text NOT NULL CHECK (status IN ('active', 'inactive')),
    can_create boolean NOT NULL,
    is_current boolean NOT NULL CHECK (NOT is_current OR status = 'active'),
    PRIMARY KEY (tenant_id, account_id)
);
CREATE INDEX tenant_memberships_by_account ON tenant_memberships(account_id);
CREATE UNIQUE INDEX tenant_memberships_current ON tenant_memberships(account_id) WHERE is_current;

CREATE TABLE spaces (
    tenant_id uuid NOT NULL,
    id uuid NOT NULL,
    creator_account_id uuid NOT NULL,
    owner_account_id uuid NOT NULL,
    -- The legacy SQLite format permits U+0000 in a name. Keep both UTF-8
    -- fields lossless and compare the canonical key byte-for-byte.
    name bytea NOT NULL CHECK (octet_length(name) BETWEEN 1 AND 400),
    name_key bytea NOT NULL,
    PRIMARY KEY (tenant_id, id),
    UNIQUE (tenant_id, owner_account_id, name_key),
    FOREIGN KEY (tenant_id, creator_account_id) REFERENCES tenant_memberships(tenant_id, account_id),
    FOREIGN KEY (tenant_id, owner_account_id) REFERENCES tenant_memberships(tenant_id, account_id)
);
CREATE INDEX spaces_by_creator ON spaces(tenant_id, creator_account_id);

CREATE TABLE space_creations (
    tenant_id uuid NOT NULL,
    creator_account_id uuid NOT NULL,
    operation_id uuid NOT NULL,
    request_fingerprint text COLLATE "C" NOT NULL CHECK (request_fingerprint ~ '^[0-9a-f]{64}$'),
    space_id uuid NOT NULL,
    PRIMARY KEY (tenant_id, creator_account_id, operation_id),
    FOREIGN KEY (tenant_id, creator_account_id) REFERENCES tenant_memberships(tenant_id, account_id),
    FOREIGN KEY (tenant_id, space_id) REFERENCES spaces(tenant_id, id)
);
CREATE INDEX space_creations_by_space ON space_creations(tenant_id, space_id);

CREATE TABLE space_grants (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    account_id uuid NOT NULL,
    role text NOT NULL CHECK (role IN ('viewer', 'editor')),
    PRIMARY KEY (tenant_id, space_id, account_id),
    FOREIGN KEY (tenant_id, space_id) REFERENCES spaces(tenant_id, id),
    FOREIGN KEY (tenant_id, account_id) REFERENCES tenant_memberships(tenant_id, account_id)
);
CREATE INDEX space_grants_by_member ON space_grants(tenant_id, account_id, space_id);
