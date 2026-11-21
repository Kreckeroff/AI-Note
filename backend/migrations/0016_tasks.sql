SET LOCAL lock_timeout = '5s';

CREATE TABLE workspace_tasks (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    id uuid NOT NULL,
    number bigint NOT NULL CHECK (number > 0),
    creator_account_id uuid NOT NULL,
    operation_id uuid NOT NULL,
    request_fingerprint text COLLATE "C" NOT NULL CHECK (request_fingerprint ~ '^[0-9a-f]{64}$'),
    title text NOT NULL CHECK (length(title) BETWEEN 1 AND 100),
    status text NOT NULL CHECK (status IN ('open', 'work', 'review', 'done')),
    source_page_id uuid,
    assignee_account_id uuid,
    created_at bigint NOT NULL,
    updated_at bigint NOT NULL,
    version bigint NOT NULL DEFAULT 1 CHECK (version > 0),
    CHECK (created_at >= 0 AND updated_at >= created_at),
    PRIMARY KEY (tenant_id, space_id, id),
    UNIQUE (tenant_id, space_id, number),
    UNIQUE (tenant_id, space_id, creator_account_id, operation_id),
    FOREIGN KEY (tenant_id, space_id) REFERENCES spaces (tenant_id, id),
    FOREIGN KEY (tenant_id, creator_account_id) REFERENCES tenant_memberships (tenant_id, account_id),
    FOREIGN KEY (tenant_id, space_id, source_page_id) REFERENCES pages (tenant_id, space_id, id),
    FOREIGN KEY (tenant_id, assignee_account_id) REFERENCES tenant_memberships (tenant_id, account_id)
);

CREATE INDEX workspace_tasks_by_source ON workspace_tasks (tenant_id, space_id, source_page_id)
    WHERE source_page_id IS NOT NULL;
CREATE INDEX workspace_tasks_by_assignee ON workspace_tasks (tenant_id, space_id, assignee_account_id)
    WHERE assignee_account_id IS NOT NULL;
