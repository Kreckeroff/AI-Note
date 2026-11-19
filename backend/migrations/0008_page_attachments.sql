SET LOCAL lock_timeout = '5s';

CREATE TABLE page_attachment_assets (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    id uuid NOT NULL,
    uploader_account_id uuid NOT NULL,
    draft_id uuid NOT NULL,
    name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 255),
    byte_length bigint NOT NULL CHECK (byte_length BETWEEN 1 AND 20971520),
    image_type text CHECK (image_type IN ('image/png', 'image/jpeg', 'image/webp')),
    bytes bytea NOT NULL CHECK (octet_length(bytes) = byte_length),
    fingerprint bytea NOT NULL CHECK (octet_length(fingerprint) = 32),
    staged_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    published_at timestamptz,
    PRIMARY KEY (tenant_id, space_id, page_id, id),
    FOREIGN KEY (tenant_id, space_id, page_id)
        REFERENCES pages(tenant_id, space_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, uploader_account_id)
        REFERENCES tenant_memberships(tenant_id, account_id) ON DELETE RESTRICT
);
CREATE INDEX page_attachment_assets_by_uploader ON page_attachment_assets(tenant_id, uploader_account_id);
CREATE INDEX page_attachment_assets_by_draft
    ON page_attachment_assets(tenant_id, space_id, page_id, uploader_account_id, draft_id, staged_at)
    WHERE published_at IS NULL;
CREATE INDEX page_attachment_assets_by_expiry ON page_attachment_assets(staged_at)
    WHERE published_at IS NULL;

CREATE TABLE page_attachments (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    asset_id uuid NOT NULL,
    publication uuid NOT NULL DEFAULT gen_random_uuid(),
    PRIMARY KEY (tenant_id, space_id, page_id, asset_id),
    UNIQUE (tenant_id, space_id, page_id, publication),
    FOREIGN KEY (tenant_id, space_id, page_id, asset_id)
        REFERENCES page_attachment_assets(tenant_id, space_id, page_id, id) ON DELETE RESTRICT
);

CREATE TABLE page_revision_attachments (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    revision bigint NOT NULL,
    asset_id uuid NOT NULL,
    PRIMARY KEY (tenant_id, space_id, page_id, revision, asset_id),
    FOREIGN KEY (tenant_id, space_id, page_id, revision)
        REFERENCES page_revisions(tenant_id, space_id, page_id, revision) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, space_id, page_id, asset_id)
        REFERENCES page_attachment_assets(tenant_id, space_id, page_id, id) ON DELETE RESTRICT
);
CREATE INDEX page_revision_attachments_by_asset ON page_revision_attachments(tenant_id, space_id, page_id, asset_id);
