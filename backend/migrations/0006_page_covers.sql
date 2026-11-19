SET LOCAL lock_timeout = '5s';

CREATE TABLE page_cover_assets (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    id uuid NOT NULL,
    uploader_account_id uuid NOT NULL,
    jpeg bytea NOT NULL CHECK (octet_length(jpeg) BETWEEN 1 AND 2097152),
    staged_at timestamptz NOT NULL DEFAULT CURRENT_TIMESTAMP,
    published_at timestamptz,
    PRIMARY KEY (tenant_id, space_id, page_id, id),
    FOREIGN KEY (tenant_id, space_id, page_id)
        REFERENCES pages(tenant_id, space_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, uploader_account_id)
        REFERENCES tenant_memberships(tenant_id, account_id) ON DELETE RESTRICT
);
CREATE INDEX page_cover_assets_by_uploader ON page_cover_assets(tenant_id, uploader_account_id);
CREATE INDEX page_cover_assets_by_expiry ON page_cover_assets(staged_at) WHERE published_at IS NULL;

CREATE TABLE page_covers (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    theme text CHECK (theme IN ('orbit', 'aurora', 'grid')),
    asset_id uuid,
    PRIMARY KEY (tenant_id, space_id, page_id),
    CHECK ((theme IS NULL) <> (asset_id IS NULL)),
    FOREIGN KEY (tenant_id, space_id, page_id)
        REFERENCES pages(tenant_id, space_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, space_id, page_id, asset_id)
        REFERENCES page_cover_assets(tenant_id, space_id, page_id, id) ON DELETE RESTRICT
);

CREATE TABLE page_cover_save_receipts (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    account_id uuid NOT NULL,
    base_revision bigint NOT NULL CHECK (base_revision > 0),
    fingerprint bytea NOT NULL CHECK (octet_length(fingerprint) = 32),
    PRIMARY KEY (tenant_id, space_id, page_id, account_id, base_revision, fingerprint),
    FOREIGN KEY (tenant_id, space_id, page_id)
        REFERENCES pages(tenant_id, space_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, account_id)
        REFERENCES tenant_memberships(tenant_id, account_id) ON DELETE RESTRICT
);
CREATE INDEX page_cover_save_receipts_by_actor
    ON page_cover_save_receipts(tenant_id, account_id);
