SET LOCAL lock_timeout = '5s';

CREATE TABLE page_comment_threads (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    id uuid NOT NULL,
    created_at bigint NOT NULL CHECK (created_at >= 0),
    status text NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'resolved')),
    version integer NOT NULL DEFAULT 1 CHECK (version > 0),
    changed_by uuid REFERENCES accounts(id) ON DELETE RESTRICT,
    changed_by_name text,
    changed_at bigint CHECK (changed_at >= 0),
    anchor_revision bigint,
    anchor_quote text,
    anchor_start integer,
    anchor_end integer,
    anchor_before text,
    anchor_after text,
    PRIMARY KEY (tenant_id, space_id, page_id, id),
    FOREIGN KEY (tenant_id, space_id, page_id)
        REFERENCES pages(tenant_id, space_id, id) ON DELETE RESTRICT,
    CHECK ((version = 1 AND status = 'active' AND changed_by IS NULL
            AND changed_by_name IS NULL AND changed_at IS NULL)
        OR (version > 1 AND changed_by IS NOT NULL AND changed_by_name IS NOT NULL AND changed_at IS NOT NULL)),
    CHECK ((anchor_revision IS NULL AND anchor_quote IS NULL AND anchor_start IS NULL
              AND anchor_end IS NULL AND anchor_before IS NULL AND anchor_after IS NULL)
        OR (anchor_revision IS NOT NULL AND anchor_revision > 0
            AND anchor_start IS NOT NULL AND anchor_start >= 0
            AND anchor_end IS NOT NULL AND anchor_end <= 60000 AND anchor_start < anchor_end
            AND anchor_quote IS NOT NULL AND char_length(anchor_quote) = anchor_end - anchor_start
            AND anchor_before IS NOT NULL AND char_length(anchor_before) <= 32
            AND anchor_after IS NOT NULL AND char_length(anchor_after) <= 32))
);
CREATE INDEX page_comment_threads_by_status
    ON page_comment_threads(tenant_id, space_id, page_id, status, created_at DESC, id DESC);
CREATE INDEX page_comment_threads_by_status_actor
    ON page_comment_threads(changed_by) WHERE changed_by IS NOT NULL;

CREATE TABLE page_comment_messages (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    thread_id uuid NOT NULL,
    id uuid NOT NULL,
    author_account_id uuid NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
    author_name text NOT NULL,
    body text NOT NULL CHECK (char_length(body) BETWEEN 1 AND 500),
    created_at bigint NOT NULL CHECK (created_at >= 0),
    PRIMARY KEY (tenant_id, space_id, page_id, id),
    FOREIGN KEY (tenant_id, space_id, page_id, thread_id)
        REFERENCES page_comment_threads(tenant_id, space_id, page_id, id) ON DELETE RESTRICT
);
CREATE INDEX page_comment_messages_by_thread
    ON page_comment_messages(tenant_id, space_id, page_id, thread_id, created_at, id);
CREATE INDEX page_comment_messages_by_author ON page_comment_messages(author_account_id);

CREATE TABLE page_comment_mentions (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    message_id uuid NOT NULL,
    recipient_account_id uuid NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
    id uuid NOT NULL UNIQUE,
    created_at bigint NOT NULL CHECK (created_at >= 0),
    PRIMARY KEY (tenant_id, space_id, page_id, message_id, recipient_account_id),
    FOREIGN KEY (tenant_id, space_id, page_id, message_id)
        REFERENCES page_comment_messages(tenant_id, space_id, page_id, id) ON DELETE RESTRICT
);
CREATE INDEX page_comment_mentions_by_recipient
    ON page_comment_mentions(recipient_account_id, created_at DESC, id DESC);

CREATE TABLE page_comment_operations (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    page_id uuid NOT NULL,
    actor_account_id uuid NOT NULL REFERENCES accounts(id) ON DELETE RESTRICT,
    operation_id uuid NOT NULL,
    fingerprint bytea NOT NULL CHECK (octet_length(fingerprint) = 32),
    result_id uuid NOT NULL,
    result_thread_id uuid NOT NULL,
    created_at bigint NOT NULL CHECK (created_at >= 0),
    PRIMARY KEY (tenant_id, space_id, page_id, actor_account_id, operation_id),
    FOREIGN KEY (tenant_id, space_id, page_id, result_id)
        REFERENCES page_comment_messages(tenant_id, space_id, page_id, id) ON DELETE RESTRICT,
    FOREIGN KEY (tenant_id, space_id, page_id, result_thread_id)
        REFERENCES page_comment_threads(tenant_id, space_id, page_id, id) ON DELETE RESTRICT
);
CREATE INDEX page_comment_operations_by_actor ON page_comment_operations(actor_account_id);
CREATE INDEX page_comment_operations_by_result
    ON page_comment_operations(tenant_id, space_id, page_id, result_id);
CREATE INDEX page_comment_operations_by_thread
    ON page_comment_operations(tenant_id, space_id, page_id, result_thread_id);
