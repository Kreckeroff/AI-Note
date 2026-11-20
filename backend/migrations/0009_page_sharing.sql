SET LOCAL lock_timeout = '5s';

CREATE TABLE tenant_sharing_policies (
    tenant_id uuid PRIMARY KEY REFERENCES tenants(id) ON DELETE RESTRICT,
    allow_guests boolean NOT NULL DEFAULT false,
    allow_public boolean NOT NULL DEFAULT false
);

CREATE TABLE page_grants (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    account_id uuid NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
    role text NOT NULL CHECK (role IN ('read', 'comment', 'edit')),
    guest boolean NOT NULL DEFAULT false,
    PRIMARY KEY (tenant_id, space_id, page_id, account_id),
    FOREIGN KEY (tenant_id, space_id, page_id)
        REFERENCES pages(tenant_id, space_id, id) ON DELETE RESTRICT
);
CREATE INDEX page_grants_by_account ON page_grants(account_id, tenant_id, space_id, page_id);

CREATE TABLE page_access_requests (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    id uuid NOT NULL,
    account_id uuid NOT NULL,
    status text NOT NULL CHECK (status IN ('pending', 'approved', 'denied')),
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    decided_at timestamptz,
    decided_by uuid REFERENCES accounts(id) ON DELETE RESTRICT,
    CHECK ((status = 'pending') = (decided_at IS NULL) AND (decided_at IS NULL) = (decided_by IS NULL)),
    PRIMARY KEY (tenant_id, space_id, page_id, id),
    FOREIGN KEY (tenant_id, space_id, page_id)
        REFERENCES pages(tenant_id, space_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, account_id)
        REFERENCES tenant_memberships(tenant_id, account_id) ON DELETE RESTRICT
);
CREATE UNIQUE INDEX page_access_requests_pending
    ON page_access_requests(tenant_id, space_id, page_id, account_id) WHERE status = 'pending';
CREATE INDEX page_access_requests_page_order
    ON page_access_requests(tenant_id, space_id, page_id, created_at DESC, id);
CREATE INDEX page_access_requests_account ON page_access_requests(tenant_id, account_id);
CREATE INDEX page_access_requests_decider ON page_access_requests(decided_by) WHERE decided_by IS NOT NULL;

CREATE TABLE page_sharing_receipts (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    account_id uuid NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
    operation_id uuid NOT NULL,
    fingerprint bytea NOT NULL CHECK (octet_length(fingerprint) = 32),
    PRIMARY KEY (tenant_id, space_id, page_id, account_id, operation_id),
    FOREIGN KEY (tenant_id, space_id, page_id)
        REFERENCES pages(tenant_id, space_id, id) ON DELETE RESTRICT
);
CREATE INDEX page_sharing_receipts_account ON page_sharing_receipts(account_id);

-- Existing rows already reference accounts through membership. The page keys
-- remain intact, and historical actor attribution survives removal of access.
ALTER TABLE page_cover_assets
    ADD CONSTRAINT page_cover_assets_uploader_fkey
    FOREIGN KEY (uploader_account_id) REFERENCES accounts(id) ON DELETE RESTRICT;
ALTER TABLE page_cover_assets DROP CONSTRAINT page_cover_assets_tenant_id_uploader_account_id_fkey;
ALTER TABLE page_cover_save_receipts
    ADD CONSTRAINT page_cover_save_receipts_actor_fkey
    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE RESTRICT;
ALTER TABLE page_cover_save_receipts DROP CONSTRAINT page_cover_save_receipts_tenant_id_account_id_fkey;
ALTER TABLE page_revisions
    ADD CONSTRAINT page_revisions_author_fkey
    FOREIGN KEY (author_account_id) REFERENCES accounts(id) ON DELETE RESTRICT;
ALTER TABLE page_revisions DROP CONSTRAINT page_revisions_tenant_id_author_account_id_fkey;
ALTER TABLE page_revision_restores
    ADD CONSTRAINT page_revision_restores_actor_fkey
    FOREIGN KEY (account_id) REFERENCES accounts(id) ON DELETE RESTRICT;
ALTER TABLE page_revision_restores DROP CONSTRAINT page_revision_restores_tenant_id_account_id_fkey;
ALTER TABLE page_attachment_assets
    ADD CONSTRAINT page_attachment_assets_uploader_fkey
    FOREIGN KEY (uploader_account_id) REFERENCES accounts(id) ON DELETE RESTRICT;
ALTER TABLE page_attachment_assets DROP CONSTRAINT page_attachment_assets_tenant_id_uploader_account_id_fkey;

CREATE INDEX page_cover_assets_uploader_account ON page_cover_assets(uploader_account_id);
CREATE INDEX page_cover_save_receipts_actor_account ON page_cover_save_receipts(account_id);
CREATE INDEX page_revisions_author_account
    ON page_revisions(author_account_id) WHERE author_account_id IS NOT NULL;
CREATE INDEX page_revision_restores_actor_account ON page_revision_restores(account_id);
CREATE INDEX page_attachment_assets_uploader_account ON page_attachment_assets(uploader_account_id);

-- Capabilities address a page by its compound key. Callers that do not
-- already have a foreign-key-scoped row use page_permissions to require
-- the page's existence.
CREATE TYPE page_capability_row AS (
    can_read boolean,
    can_comment boolean,
    can_edit boolean,
    can_manage boolean,
    can_request boolean,
    can_browse_space boolean,
    direct_role text,
    inherited_role text,
    effective_role text,
    inheritance_source text
);

CREATE FUNCTION page_capabilities(uuid, uuid, uuid, uuid)
RETURNS SETOF page_capability_row
LANGUAGE sql STABLE AS $$
    SELECT greatest(direct_rank, inherited_rank) >= 1,
           greatest(direct_rank, inherited_rank) >= 2,
           greatest(direct_rank, inherited_rank) >= 3,
           coalesce(owner, false), coalesce(active, false), inherited_rank >= 1,
           (ARRAY['read', 'comment', 'edit'])[direct_rank],
           (ARRAY['read', 'comment', 'edit'])[inherited_rank],
           (ARRAY['read', 'comment', 'edit'])[greatest(direct_rank, inherited_rank)],
           inheritance
    FROM (
        SELECT m.status = 'active' AND s.owner_account_id = $3 AS owner,
               m.status = 'active' AS active,
               CASE
                   WHEN m.status = 'active' AND (s.owner_account_id = $3 OR g.role = 'editor') THEN 3
                   WHEN m.status = 'active' AND (g.role = 'viewer' OR m.role IN ('admin', 'superadmin')) THEN 1
                   ELSE 0
               END AS inherited_rank,
               CASE
                   WHEN m.status = 'active' AND s.owner_account_id = $3 THEN 'owner'
                   WHEN m.status = 'active' AND g.role IS NOT NULL THEN 'space'
                   WHEN m.status = 'active' AND m.role IN ('admin', 'superadmin') THEN 'tenant'
               END AS inheritance,
               CASE WHEN (NOT d.guest AND m.status = 'active')
                           OR (d.guest AND policy.allow_guests AND m.status IS DISTINCT FROM 'inactive')
                    THEN array_position(ARRAY['read', 'comment', 'edit'], d.role)
                    ELSE 0
               END AS direct_rank
        FROM spaces s
        LEFT JOIN tenant_memberships m ON (m.tenant_id, m.account_id) = (s.tenant_id, $3)
        LEFT JOIN space_grants g ON (g.tenant_id, g.space_id, g.account_id) = ($1, $2, $3)
        LEFT JOIN page_grants d ON (d.tenant_id, d.space_id, d.page_id, d.account_id) = ($1, $2, $4, $3)
        LEFT JOIN tenant_sharing_policies policy ON policy.tenant_id = $1
        WHERE (s.tenant_id, s.id) = ($1, $2)
    ) roles
$$;


CREATE FUNCTION page_permissions(uuid, uuid, uuid, uuid)
RETURNS SETOF page_capability_row
LANGUAGE sql STABLE AS $$
    SELECT caps.*
    FROM pages p
    CROSS JOIN LATERAL page_capabilities($1, $2, $3, $4) caps
    WHERE (p.tenant_id, p.space_id, p.id) = ($1, $2, $4)
$$;
