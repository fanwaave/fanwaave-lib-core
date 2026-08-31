-- PostgreSQL-family platform preflight, owned by Fanwaave infra.
--
-- DPM intentionally diffs the product schema (`fanwaave`) rather than the
-- managed extension schema. Apply this idempotent preflight with the migrator
-- role before `dpm diff`, `dpm verify`, or `dpm apply`. Supabase installs its
-- current default extension version; version clauses are deliberately absent.
CREATE SCHEMA IF NOT EXISTS extensions;
CREATE EXTENSION IF NOT EXISTS vector WITH SCHEMA extensions;
CREATE EXTENSION IF NOT EXISTS pgcrypto;

DO $$
DECLARE
  installed_version TEXT;
BEGIN
  SELECT extversion INTO installed_version
  FROM pg_extension
  WHERE extname = 'vector';

  IF installed_version IS NULL THEN
    RAISE EXCEPTION 'pgvector extension is required';
  END IF;
  IF NOT EXISTS (
    SELECT 1
    FROM pg_type
    WHERE typnamespace = 'extensions'::regnamespace
      AND typname = 'halfvec'
  ) THEN
    RAISE EXCEPTION 'pgvector % does not provide extensions.halfvec', installed_version;
  END IF;
END
$$;
