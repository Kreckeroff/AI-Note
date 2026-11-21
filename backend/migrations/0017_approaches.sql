SET LOCAL lock_timeout = '5s';

CREATE TABLE workspace_approaches (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    instruction text NOT NULL CHECK (length(instruction) BETWEEN 1 AND 1500),
    source_page_id uuid NOT NULL,
    require_source boolean NOT NULL,
    version bigint NOT NULL CHECK (version > 0),
    PRIMARY KEY (tenant_id,space_id),
    FOREIGN KEY (tenant_id,space_id,source_page_id) REFERENCES pages(tenant_id,space_id,id)
);

CREATE TABLE workspace_approach_runs (
    tenant_id uuid NOT NULL,
    space_id uuid NOT NULL,
    id uuid NOT NULL,
    task_id uuid NOT NULL,
    actor_id uuid NOT NULL,
    source_page_id uuid NOT NULL,
    source_revision bigint NOT NULL CHECK (source_revision > 0),
    spec_version bigint NOT NULL CHECK (spec_version > 0),
    task_version bigint NOT NULL CHECK (task_version > 0),
    status text NOT NULL CHECK (status IN ('running','review','accepted','returned','failed')),
    output text,
    result_page_id uuid,
    started_at bigint NOT NULL,
    finished_at bigint,
    PRIMARY KEY (tenant_id,space_id,id),
    CHECK ((status = 'running') = (finished_at IS NULL)),
    CHECK ((status IN ('review','accepted','returned')) = (output IS NOT NULL)),
    CHECK ((status = 'accepted') = (result_page_id IS NOT NULL)),
    FOREIGN KEY (tenant_id,space_id,task_id) REFERENCES workspace_tasks(tenant_id,space_id,id),
    FOREIGN KEY (tenant_id,space_id,source_page_id) REFERENCES pages(tenant_id,space_id,id),
    FOREIGN KEY (tenant_id,space_id,result_page_id) REFERENCES pages(tenant_id,space_id,id),
    FOREIGN KEY (tenant_id,actor_id) REFERENCES tenant_memberships(tenant_id,account_id)
);
CREATE UNIQUE INDEX approach_active_per_task ON workspace_approach_runs(tenant_id,space_id,task_id)
    WHERE status IN ('running','review');
CREATE INDEX approach_runs_by_task ON workspace_approach_runs(tenant_id,space_id,task_id,started_at DESC);
