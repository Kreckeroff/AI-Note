SET LOCAL lock_timeout = '5s';

CREATE TABLE page_coedit_drafts (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    version bigint NOT NULL DEFAULT 0 CHECK (version >= 0),
    source text NOT NULL,
    is_markdown boolean NOT NULL,
    published_revision bigint NOT NULL CHECK (published_revision > 0),
    published_source text NOT NULL,
    published_is_markdown boolean NOT NULL,
    conflict_start bigint,
    conflict_end bigint,
    conflict_mine text,
    CHECK (
        (conflict_start IS NULL AND conflict_end IS NULL AND conflict_mine IS NULL)
        OR (conflict_start IS NOT NULL AND conflict_end IS NOT NULL AND conflict_mine IS NOT NULL
            AND conflict_start >= 0 AND conflict_end >= conflict_start AND conflict_end <= char_length(source))
    ),
    PRIMARY KEY (tenant_id, space_id, page_id),
    FOREIGN KEY (tenant_id, space_id, page_id)
        REFERENCES pages(tenant_id, space_id, id) ON DELETE CASCADE
);

-- Journal entries record applied coordinates and both fragments. The inverse
-- reconstructs a retained base without writing a full snapshot on every keypress.
CREATE TABLE page_coedit_operations (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    version bigint NOT NULL CHECK (version > 0),
    start bigint NOT NULL CHECK (start >= 0),
    removed text NOT NULL,
    inserted text NOT NULL,
    before_markdown boolean NOT NULL,
    before_published_revision bigint NOT NULL CHECK (before_published_revision > 0),
    PRIMARY KEY (tenant_id, space_id, page_id, version),
    FOREIGN KEY (tenant_id, space_id, page_id)
        REFERENCES page_coedit_drafts(tenant_id, space_id, page_id) ON DELETE CASCADE
);

CREATE TABLE page_coedit_receipts (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    account_id uuid NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
    operation_id uuid NOT NULL,
    fingerprint bytea NOT NULL CHECK (octet_length(fingerprint) = 32),
    version bigint NOT NULL CHECK (version >= 0),
    outcome text NOT NULL CHECK (outcome IN ('applied', 'unchanged', 'conflict', 'published')),
    PRIMARY KEY (tenant_id, space_id, page_id, account_id, operation_id),
    FOREIGN KEY (tenant_id, space_id, page_id)
        REFERENCES page_coedit_drafts(tenant_id, space_id, page_id) ON DELETE CASCADE
);
CREATE INDEX page_coedit_receipts_account ON page_coedit_receipts(account_id);

-- A separate generation table retains tombstones after grant removal. A
-- delete-and-regrant must invalidate a prior lease even if no client observed
-- the intervening absence. Unrelated membership/profile fields do not bump it.
CREATE TABLE page_coedit_access_generations (
    scope text NOT NULL CHECK (scope IN ('member', 'space', 'owner', 'page', 'guest')),
    tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    space_key uuid NOT NULL,
    page_key uuid NOT NULL,
    account_key uuid NOT NULL,
    generation bigint NOT NULL DEFAULT 1 CHECK (generation > 0),
    PRIMARY KEY (scope, tenant_id, space_key, page_key, account_key)
);
CREATE INDEX page_coedit_access_generations_tenant ON page_coedit_access_generations(tenant_id);

CREATE FUNCTION page_coedit_bump_generation() RETURNS trigger
LANGUAGE plpgsql AS $$
DECLARE
    before_row jsonb;
    after_row jsonb;
    relevant_key text;
    changed boolean := false;
    row_data jsonb;
    scope_name text := TG_ARGV[0];
    zero uuid := '00000000-0000-0000-0000-000000000000';
BEGIN
    IF TG_OP = 'UPDATE' THEN
        before_row := to_jsonb(OLD);
        after_row := to_jsonb(NEW);
        FOREACH relevant_key IN ARRAY TG_ARGV[1:] LOOP
            changed := changed OR before_row -> relevant_key IS DISTINCT FROM after_row -> relevant_key;
        END LOOP;
        IF NOT changed THEN
            RETURN NULL;
        END IF;
    END IF;

    row_data := CASE WHEN TG_OP = 'DELETE' THEN to_jsonb(OLD) ELSE to_jsonb(NEW) END;
    INSERT INTO page_coedit_access_generations (scope, tenant_id, space_key, page_key, account_key)
    VALUES (
        scope_name,
        (row_data ->> 'tenant_id')::uuid,
        coalesce((row_data ->> 'space_id')::uuid,
                 CASE WHEN scope_name = 'owner' THEN (row_data ->> 'id')::uuid END,
                 zero),
        coalesce((row_data ->> 'page_id')::uuid, zero),
        coalesce((row_data ->> 'account_id')::uuid, zero)
    )
    ON CONFLICT (scope, tenant_id, space_key, page_key, account_key)
    DO UPDATE SET generation = page_coedit_access_generations.generation + 1;
    RETURN NULL;
END
$$;

CREATE TRIGGER page_coedit_member_generation
    AFTER INSERT OR UPDATE OF role, status OR DELETE ON tenant_memberships
    FOR EACH ROW EXECUTE FUNCTION page_coedit_bump_generation('member', 'role', 'status');
CREATE TRIGGER page_coedit_space_generation
    AFTER INSERT OR UPDATE OF role OR DELETE ON space_grants
    FOR EACH ROW EXECUTE FUNCTION page_coedit_bump_generation('space', 'role');
CREATE TRIGGER page_coedit_owner_generation
    AFTER UPDATE OF owner_account_id ON spaces
    FOR EACH ROW EXECUTE FUNCTION page_coedit_bump_generation('owner', 'owner_account_id');
CREATE TRIGGER page_coedit_direct_generation
    AFTER INSERT OR UPDATE OF role, guest OR DELETE ON page_grants
    FOR EACH ROW EXECUTE FUNCTION page_coedit_bump_generation('page', 'role', 'guest');
CREATE TRIGGER page_coedit_guest_generation
    AFTER INSERT OR UPDATE OF allow_guests OR DELETE ON tenant_sharing_policies
    FOR EACH ROW EXECUTE FUNCTION page_coedit_bump_generation('guest', 'allow_guests');

CREATE FUNCTION page_coedit_access_generation(uuid, uuid, uuid, uuid)
RETURNS text
LANGUAGE sql STABLE AS $$
    SELECT coalesce(string_agg(g.scope || ':' || g.generation::text, ',' ORDER BY g.scope), '')
    FROM (VALUES
        ('member', '00000000-0000-0000-0000-000000000000'::uuid,
                   '00000000-0000-0000-0000-000000000000'::uuid, $4, true),
        ('space', $2, '00000000-0000-0000-0000-000000000000'::uuid, $4, true),
        ('owner', $2, '00000000-0000-0000-0000-000000000000'::uuid,
                      '00000000-0000-0000-0000-000000000000'::uuid, true),
        ('page', $2, $3, $4, true),
        ('guest', '00000000-0000-0000-0000-000000000000'::uuid,
                  '00000000-0000-0000-0000-000000000000'::uuid,
                  '00000000-0000-0000-0000-000000000000'::uuid, EXISTS (
            SELECT 1 FROM page_grants
            WHERE (tenant_id, space_id, page_id, account_id) = ($1, $2, $3, $4) AND guest
        ))
    ) keys(scope, space_key, page_key, account_key, applicable)
    JOIN page_coedit_access_generations g
      ON (g.scope, g.tenant_id, g.space_key, g.page_key, g.account_key)
         = (keys.scope, $1, keys.space_key, keys.page_key, keys.account_key)
    WHERE keys.applicable
$$;

CREATE TABLE page_coedit_leases (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    session_digest bytea NOT NULL REFERENCES sessions(token_digest) ON DELETE CASCADE,
    lease uuid NOT NULL,
    generation text NOT NULL,
    PRIMARY KEY (tenant_id, space_id, page_id, session_digest),
    FOREIGN KEY (tenant_id, space_id, page_id)
        REFERENCES page_coedit_drafts(tenant_id, space_id, page_id) ON DELETE CASCADE
);
CREATE INDEX page_coedit_leases_session ON page_coedit_leases(session_digest);
