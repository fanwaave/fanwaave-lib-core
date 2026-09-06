-- PostgreSQL-family platform preflight, owned by Fanwaave infra.
--
-- DPM declares and verifies extension placement as part of the desired state.
-- Use this idempotent bootstrap only while provisioning a blank database, or
-- after `dpm apply` as a capability assertion. For an existing database, let
-- `dpm diff` review any extension creation or schema move before execution.
-- Supabase installs its current default version; version clauses are absent.
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
