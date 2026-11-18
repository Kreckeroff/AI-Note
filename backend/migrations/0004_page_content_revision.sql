SET LOCAL lock_timeout = '5s';

ALTER TABLE pages ADD COLUMN content_revision bigint NOT NULL DEFAULT 1;
