SET LOCAL lock_timeout = '5s';

CREATE TABLE pages (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    section_id uuid NOT NULL,
    id uuid NOT NULL,
    creator_account_id uuid NOT NULL,
    title bytea NOT NULL CHECK (octet_length(title) BETWEEN 1 AND 400),
    title_key bytea NOT NULL CHECK (octet_length(title_key) > 0),
    position bigint NOT NULL CHECK (position > 0),
    markdown text NOT NULL DEFAULT '',
    PRIMARY KEY (tenant_id, space_id, id),
    UNIQUE (tenant_id, space_id, section_id, title_key),
    UNIQUE (tenant_id, space_id, section_id, position),
    FOREIGN KEY (tenant_id, space_id, section_id)
        REFERENCES sections(tenant_id, space_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, creator_account_id)
        REFERENCES tenant_memberships(tenant_id, account_id) ON DELETE RESTRICT
);
CREATE INDEX pages_by_creator ON pages(tenant_id, creator_account_id);

CREATE TABLE page_creations (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    section_id uuid NOT NULL,
    creator_account_id uuid NOT NULL,
    operation_id uuid NOT NULL,
    request_fingerprint text COLLATE "C" NOT NULL CHECK (request_fingerprint ~ '^[0-9a-f]{64}$'),
    page_id uuid NOT NULL,
    PRIMARY KEY (tenant_id, space_id, section_id, creator_account_id, operation_id),
    FOREIGN KEY (tenant_id, space_id, section_id)
        REFERENCES sections(tenant_id, space_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, space_id, page_id)
        REFERENCES pages(tenant_id, space_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, creator_account_id)
        REFERENCES tenant_memberships(tenant_id, account_id) ON DELETE RESTRICT
);
CREATE INDEX page_creations_by_creator ON page_creations(tenant_id, creator_account_id);
CREATE INDEX page_creations_by_page ON page_creations(tenant_id, space_id, page_id);
