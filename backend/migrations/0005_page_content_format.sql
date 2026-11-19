SET LOCAL lock_timeout = '5s';

ALTER TABLE pages ADD COLUMN content_is_markdown boolean NOT NULL DEFAULT false;
