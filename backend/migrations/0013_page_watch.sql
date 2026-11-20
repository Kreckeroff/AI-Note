SET LOCAL lock_timeout = '5s';

CREATE TABLE page_watchers (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    account_id uuid NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
    PRIMARY KEY (tenant_id, space_id, page_id, account_id),
    FOREIGN KEY (tenant_id, space_id, page_id)
        REFERENCES pages(tenant_id, space_id, id) ON DELETE RESTRICT
);
CREATE INDEX page_watchers_by_account ON page_watchers(account_id);

CREATE TABLE page_publication_notices (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    revision bigint NOT NULL,
    recipient_account_id uuid NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
    id uuid NOT NULL UNIQUE DEFAULT gen_random_uuid(),
    created_at bigint NOT NULL CHECK (created_at >= 0),
    PRIMARY KEY (tenant_id, space_id, page_id, revision, recipient_account_id),
    FOREIGN KEY (tenant_id, space_id, page_id, revision)
        REFERENCES page_revisions(tenant_id, space_id, page_id, revision) ON DELETE RESTRICT
);
CREATE INDEX page_publication_notices_by_recipient
    ON page_publication_notices(recipient_account_id, created_at DESC, id DESC);
