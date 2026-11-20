SET LOCAL lock_timeout = '5s';

CREATE TABLE space_public_policies (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    enabled boolean NOT NULL DEFAULT false,
    version bigint NOT NULL DEFAULT 0 CHECK (version >= 0),
    PRIMARY KEY (tenant_id, space_id),
    FOREIGN KEY (tenant_id, space_id) REFERENCES spaces(tenant_id, id) ON DELETE RESTRICT
);

CREATE TABLE public_page_links (
    id uuid PRIMARY KEY,
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    token_digest bytea NOT NULL UNIQUE CHECK (octet_length(token_digest) = 32),
    issued_by uuid NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
    issued_at bigint NOT NULL,
    revoked_at bigint,
    FOREIGN KEY (tenant_id, space_id, page_id)
        REFERENCES pages(tenant_id, space_id, id) ON DELETE RESTRICT
);
CREATE UNIQUE INDEX public_page_links_live ON public_page_links(tenant_id, space_id, page_id)
    WHERE revoked_at IS NULL;
CREATE INDEX public_page_links_page ON public_page_links(tenant_id, space_id, page_id);

CREATE TABLE page_guest_invitations (
    id uuid PRIMARY KEY,
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    email text NOT NULL CHECK (email = lower(email) AND char_length(email) BETWEEN 3 AND 254),
    role text NOT NULL CHECK (role IN ('read', 'comment', 'edit')),
    token_digest bytea NOT NULL UNIQUE CHECK (octet_length(token_digest) = 32),
    issued_by uuid NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
    issued_at bigint NOT NULL,
    expires_at bigint NOT NULL CHECK (expires_at > issued_at),
    revoked_at bigint,
    accepted_by uuid REFERENCES accounts(id) ON DELETE RESTRICT,
    accepted_at bigint,
    delivered_at bigint,
    send_lease bigint,
    CHECK ((accepted_by IS NULL) = (accepted_at IS NULL)),
    CHECK (revoked_at IS NULL OR accepted_at IS NULL),
    FOREIGN KEY (tenant_id, space_id, page_id)
        REFERENCES pages(tenant_id, space_id, id) ON DELETE RESTRICT
);
CREATE INDEX page_guest_invitations_page ON page_guest_invitations(tenant_id, space_id, page_id, issued_at DESC, id);
CREATE UNIQUE INDEX page_guest_invitations_pending ON page_guest_invitations(tenant_id, space_id, page_id, email)
    WHERE revoked_at IS NULL AND accepted_at IS NULL;

ALTER TABLE page_sharing_receipts ADD COLUMN result_id uuid;
