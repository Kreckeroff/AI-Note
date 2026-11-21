SET LOCAL lock_timeout = '5s';

CREATE TABLE space_collections (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    id uuid NOT NULL,
    creator uuid NOT NULL REFERENCES accounts(id),
    title text NOT NULL CHECK (length(title) BETWEEN 1 AND 100),
    title_key text COLLATE "C" NOT NULL,
    schema_version bigint NOT NULL DEFAULT 1 CHECK (schema_version > 0),
    PRIMARY KEY (tenant_id, space_id, id),
    UNIQUE (tenant_id, space_id, title_key),
    FOREIGN KEY (tenant_id, space_id) REFERENCES spaces(tenant_id, id)
);
CREATE TABLE collection_fields (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    collection_id uuid NOT NULL,
    id uuid NOT NULL,
    creator uuid NOT NULL REFERENCES accounts(id),
    title text NOT NULL CHECK (length(title) BETWEEN 1 AND 100),
    title_key text COLLATE "C" NOT NULL,
    kind text NOT NULL CHECK (kind IN ('text','number','date','checkbox')),
    position integer NOT NULL CHECK (position BETWEEN 1 AND 32),
    PRIMARY KEY (tenant_id, space_id, collection_id, id),
    UNIQUE (tenant_id, space_id, collection_id, title_key),
    UNIQUE (tenant_id, space_id, collection_id, position),
    FOREIGN KEY (tenant_id, space_id, collection_id) REFERENCES space_collections(tenant_id, space_id, id)
);
CREATE TABLE collection_records (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    collection_id uuid NOT NULL,
    id uuid NOT NULL,
    creator uuid NOT NULL REFERENCES accounts(id),
    initial_fingerprint text NOT NULL,
    title text NOT NULL CHECK (length(title) BETWEEN 1 AND 100),
    properties jsonb NOT NULL CHECK (jsonb_typeof(properties) = 'object'),
    version bigint NOT NULL DEFAULT 1 CHECK (version > 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tenant_id, space_id, collection_id, id),
    FOREIGN KEY (tenant_id, space_id, collection_id) REFERENCES space_collections(tenant_id, space_id, id)
);
CREATE INDEX collection_record_order ON collection_records(tenant_id, space_id, collection_id, created_at, id);
