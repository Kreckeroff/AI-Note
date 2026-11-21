SET LOCAL lock_timeout = '5s';

ALTER TABLE sections
    ADD COLUMN parent_id uuid,
    ADD CONSTRAINT sections_parent_fkey FOREIGN KEY (tenant_id, space_id, parent_id)
        REFERENCES sections(tenant_id, space_id, id) ON DELETE RESTRICT,
    ADD CONSTRAINT sections_not_own_parent CHECK (parent_id IS DISTINCT FROM id),
    DROP CONSTRAINT sections_tenant_id_space_id_title_key_key,
    ADD CONSTRAINT sections_sibling_title_key UNIQUE NULLS NOT DISTINCT (tenant_id, space_id, parent_id, title_key);
CREATE INDEX sections_by_parent ON sections(tenant_id, space_id, parent_id, position);

ALTER TABLE pages
    ADD COLUMN parent_id uuid,
    ADD CONSTRAINT pages_section_identity UNIQUE (tenant_id, space_id, section_id, id),
    ADD CONSTRAINT pages_parent_fkey FOREIGN KEY (tenant_id, space_id, section_id, parent_id)
        REFERENCES pages(tenant_id, space_id, section_id, id) ON DELETE RESTRICT,
    ADD CONSTRAINT pages_not_own_parent CHECK (parent_id IS DISTINCT FROM id);
CREATE INDEX pages_by_parent ON pages(tenant_id, space_id, section_id, parent_id, position);
