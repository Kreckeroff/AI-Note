SET LOCAL lock_timeout = '5s';

CREATE TABLE page_revision_covers (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    asset_id uuid NOT NULL,
    jpeg bytea NOT NULL CHECK (octet_length(jpeg) BETWEEN 1 AND 2097152),
    PRIMARY KEY (tenant_id, space_id, page_id, asset_id),
    FOREIGN KEY (tenant_id, space_id, page_id)
        REFERENCES pages(tenant_id, space_id, id) ON DELETE RESTRICT
);

CREATE TABLE page_revisions (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    revision bigint NOT NULL CHECK (revision > 0),
    title bytea NOT NULL CHECK (octet_length(title) BETWEEN 1 AND 400),
    title_key bytea NOT NULL CHECK (octet_length(title_key) > 0),
    markdown text NOT NULL,
    content_is_markdown boolean NOT NULL,
    cover_theme text CHECK (cover_theme IN ('orbit', 'aurora', 'grid')),
    cover_asset_id uuid,
    author_account_id uuid,
    author_name text,
    recorded_at timestamptz,
    PRIMARY KEY (tenant_id, space_id, page_id, revision),
    CHECK (cover_theme IS NULL OR cover_asset_id IS NULL),
    CHECK ((author_account_id IS NULL) = (author_name IS NULL)
       AND (author_account_id IS NULL) = (recorded_at IS NULL)),
    FOREIGN KEY (tenant_id, space_id, page_id)
        REFERENCES pages(tenant_id, space_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, space_id, page_id, cover_asset_id)
        REFERENCES page_revision_covers(tenant_id, space_id, page_id, asset_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, author_account_id)
        REFERENCES tenant_memberships(tenant_id, account_id) ON DELETE RESTRICT
);
CREATE INDEX page_revisions_by_cover
    ON page_revisions(tenant_id, space_id, page_id, cover_asset_id) WHERE cover_asset_id IS NOT NULL;
CREATE INDEX page_revisions_by_author
    ON page_revisions(tenant_id, author_account_id) WHERE author_account_id IS NOT NULL;

CREATE TABLE page_revision_restores (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    account_id uuid NOT NULL,
    base_revision bigint NOT NULL CHECK (base_revision > 0),
    source_revision bigint NOT NULL CHECK (source_revision > 0 AND source_revision < base_revision),
    restored_revision bigint NOT NULL CHECK (restored_revision = base_revision + 1),
    PRIMARY KEY (tenant_id, space_id, page_id, account_id, base_revision, source_revision),
    FOREIGN KEY (tenant_id, account_id)
        REFERENCES tenant_memberships(tenant_id, account_id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, space_id, page_id, source_revision)
        REFERENCES page_revisions(tenant_id, space_id, page_id, revision) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, space_id, page_id, restored_revision)
        REFERENCES page_revisions(tenant_id, space_id, page_id, revision) ON DELETE RESTRICT
);
CREATE INDEX page_revision_restores_by_actor ON page_revision_restores(tenant_id, account_id);
CREATE INDEX page_revision_restores_by_source ON page_revision_restores(tenant_id, space_id, page_id, source_revision);
CREATE UNIQUE INDEX page_revision_restores_by_result ON page_revision_restores(tenant_id, space_id, page_id, restored_revision);
