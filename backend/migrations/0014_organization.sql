SET LOCAL lock_timeout = '5s';

CREATE TABLE organization_brand (
    tenant_id uuid PRIMARY KEY REFERENCES tenants(id) ON DELETE CASCADE,
    logo bytea NOT NULL CHECK (octet_length(logo) BETWEEN 1 AND 5242880)
);

CREATE TABLE organization_departments (
    tenant_id uuid NOT NULL,
    account_id uuid NOT NULL,
    department text NOT NULL CHECK (char_length(department) BETWEEN 1 AND 120),
    PRIMARY KEY (tenant_id, account_id),
    FOREIGN KEY (tenant_id, account_id) REFERENCES tenant_memberships(tenant_id, account_id) ON DELETE CASCADE
);
CREATE INDEX organization_departments_match ON organization_departments(tenant_id, department);

CREATE TABLE organization_groups (
    tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    id uuid NOT NULL,
    name text NOT NULL CHECK (char_length(name) BETWEEN 1 AND 120),
    PRIMARY KEY (tenant_id, id)
);
CREATE UNIQUE INDEX organization_groups_by_name ON organization_groups(tenant_id, lower(name));

CREATE TABLE organization_group_members (
    tenant_id uuid NOT NULL,
    group_id uuid NOT NULL,
    account_id uuid NOT NULL,
    PRIMARY KEY (tenant_id, group_id, account_id),
    FOREIGN KEY (tenant_id, group_id) REFERENCES organization_groups(tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, account_id) REFERENCES tenant_memberships(tenant_id, account_id) ON DELETE CASCADE
);
CREATE INDEX organization_groups_by_member ON organization_group_members(tenant_id, account_id);

CREATE TABLE space_group_grants (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    group_id uuid NOT NULL,
    role text NOT NULL CHECK (role IN ('viewer', 'editor')),
    PRIMARY KEY (tenant_id, space_id, group_id),
    FOREIGN KEY (tenant_id, space_id) REFERENCES spaces(tenant_id, id) ON DELETE CASCADE,
    FOREIGN KEY (tenant_id, group_id) REFERENCES organization_groups(tenant_id, id) ON DELETE CASCADE
);

CREATE TABLE space_department_grants (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    department text NOT NULL CHECK (char_length(department) BETWEEN 1 AND 120),
    role text NOT NULL CHECK (role IN ('viewer', 'editor')),
    PRIMARY KEY (tenant_id, space_id, department),
    FOREIGN KEY (tenant_id, space_id) REFERENCES spaces(tenant_id, id) ON DELETE CASCADE
);

CREATE VIEW effective_space_grants AS
    SELECT tenant_id, space_id, account_id, CASE WHEN bool_or(role = 'editor') THEN 'editor' ELSE 'viewer' END AS role
    FROM (
        SELECT tenant_id, space_id, account_id, role FROM space_grants
        UNION ALL
        SELECT g.tenant_id, g.space_id, m.account_id, g.role
        FROM space_group_grants g JOIN organization_group_members m USING (tenant_id, group_id)
        UNION ALL
        SELECT g.tenant_id, g.space_id, m.account_id, g.role
        FROM space_department_grants g JOIN organization_departments m USING (tenant_id, department)
    ) grants
    GROUP BY tenant_id, space_id, account_id;

CREATE TABLE organization_invitations (
    id uuid PRIMARY KEY,
    tenant_id uuid NOT NULL REFERENCES tenants(id) ON DELETE CASCADE,
    email text NOT NULL CHECK (email = lower(email) AND char_length(email) BETWEEN 3 AND 254),
    token_digest bytea NOT NULL UNIQUE CHECK (octet_length(token_digest) = 32),
    issued_by uuid NOT NULL REFERENCES accounts(id),
    issued_at bigint NOT NULL,
    expires_at bigint NOT NULL CHECK (expires_at > issued_at),
    revoked_at bigint,
    accepted_by uuid REFERENCES accounts(id),
    accepted_at bigint,
    delivered_at bigint,
    CHECK ((accepted_by IS NULL) = (accepted_at IS NULL)),
    CHECK (revoked_at IS NULL OR accepted_at IS NULL)
);
CREATE INDEX organization_invitations_recent ON organization_invitations(tenant_id, issued_at DESC, id);
CREATE UNIQUE INDEX organization_invitations_pending ON organization_invitations(tenant_id, email)
    WHERE revoked_at IS NULL AND accepted_at IS NULL;

CREATE OR REPLACE FUNCTION page_capabilities(uuid, uuid, uuid, uuid)
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
        LEFT JOIN effective_space_grants g ON (g.tenant_id, g.space_id, g.account_id) = ($1, $2, $3)
        LEFT JOIN page_grants d ON (d.tenant_id, d.space_id, d.page_id, d.account_id) = ($1, $2, $4, $3)
        LEFT JOIN tenant_sharing_policies policy ON policy.tenant_id = $1
        WHERE (s.tenant_id, s.id) = ($1, $2)
    ) roles
$$;

ALTER TABLE page_coedit_access_generations
    DROP CONSTRAINT page_coedit_access_generations_scope_check;
ALTER TABLE page_coedit_access_generations
    ADD CONSTRAINT page_coedit_access_generations_scope_check
    CHECK (scope IN ('member','space','owner','page','guest','organization-member','organization-space'));

CREATE TRIGGER page_coedit_department_generation
    AFTER INSERT OR UPDATE OF department OR DELETE ON organization_departments
    FOR EACH ROW EXECUTE FUNCTION page_coedit_bump_generation('organization-member', 'department');
CREATE TRIGGER page_coedit_group_membership_generation
    AFTER INSERT OR DELETE ON organization_group_members
    FOR EACH ROW EXECUTE FUNCTION page_coedit_bump_generation('organization-member');
CREATE TRIGGER page_coedit_group_grant_generation
    AFTER INSERT OR UPDATE OF role OR DELETE ON space_group_grants
    FOR EACH ROW EXECUTE FUNCTION page_coedit_bump_generation('organization-space', 'role');
CREATE TRIGGER page_coedit_department_grant_generation
    AFTER INSERT OR UPDATE OF role OR DELETE ON space_department_grants
    FOR EACH ROW EXECUTE FUNCTION page_coedit_bump_generation('organization-space', 'role');

CREATE OR REPLACE FUNCTION page_coedit_access_generation(uuid, uuid, uuid, uuid)
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
        ('organization-member', '00000000-0000-0000-0000-000000000000'::uuid,
                   '00000000-0000-0000-0000-000000000000'::uuid, $4, true),
        ('organization-space', $2, '00000000-0000-0000-0000-000000000000'::uuid,
                   '00000000-0000-0000-0000-000000000000'::uuid, true),
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

