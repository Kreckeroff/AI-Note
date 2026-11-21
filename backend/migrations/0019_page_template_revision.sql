SET LOCAL lock_timeout = '5s';

-- Existing pages and pages created without a template keep a null origin.
ALTER TABLE pages ADD COLUMN template_revision text;
