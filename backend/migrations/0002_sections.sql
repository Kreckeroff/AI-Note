SET LOCAL lock_timeout = '5s';

CREATE TABLE sections (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    id uuid NOT NULL,
    creator_account_id uuid NOT NULL,
    title bytea NOT NULL CHECK (octet_length(title) BETWEEN 1 AND 400),
    title_key bytea NOT NULL CHECK (octet_length(title_key) > 0),
    position bigint NOT NULL CHECK (position > 0),
    PRIMARY KEY (tenant_id, space_id, id),
    UNIQUE (tenant_id, space_id, title_key),
    UNIQUE (tenant_id, space_id, position),
    FOREIGN KEY (tenant_id, space_id) REFERENCES spaces(tenant_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, creator_account_id)
        REFERENCES tenant_memberships(tenant_id, account_id) ON DELETE RESTRICT
);
CREATE INDEX sections_by_creator ON sections(tenant_id, creator_account_id);

CREATE TABLE section_creations (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    creator_account_id uuid NOT NULL,
    operation_id uuid NOT NULL,
    request_fingerprint text COLLATE "C" NOT NULL CHECK (request_fingerprint ~ '^[0-9a-f]{64}$'),
    section_id uuid NOT NULL,
    PRIMARY KEY (tenant_id, space_id, creator_account_id, operation_id),
    FOREIGN KEY (tenant_id, space_id, section_id)
        REFERENCES sections(tenant_id, space_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, creator_account_id)
        REFERENCES tenant_memberships(tenant_id, account_id) ON DELETE RESTRICT
);
CREATE INDEX section_creations_by_creator ON section_creations(tenant_id, creator_account_id);
CREATE INDEX section_creations_by_section ON section_creations(tenant_id, space_id, section_id);
