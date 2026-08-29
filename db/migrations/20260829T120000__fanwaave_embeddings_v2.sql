-- GENERATED FILE - do not edit by hand.
-- Forward migration to embedding contract 2026.08.29 for fanwaave.message_embeddings.
-- Regenerate:  node scripts/embeddings/generate.mjs
-- CI gate:     node scripts/embeddings/generate.mjs --check
--
-- Contract:    ores-embedding-contract 2026.08.29
-- Contract sha256: 39b8e1599d227a97f2362fdb6d3495dd38853b3345d7b05e90b64088884f9250
-- Manifest sha256: c4dc92e735028690aa8b3b3b73db05bb87cbe612345830654cf4393bf0fd4f03
-- Owning GitHub org: fanwaave
--
-- This schema is owned by this repository. It is NOT assembled from
-- ORESoftware/k8s-libs-and-shared-defs; the org that owns the data owns the
-- DDL and the migrations that produce it. dpm is the only tool that applies
-- either, and apply requires human review.

-- Migration version: 20260829T120000
-- Applied by:        dpm  (declarative-migrations/declarative-postgres-migrate.rs)
-- Apply requires human review. Services never run DDL at boot.
--
-- What this migration does:
--   * widens the stored embedding to the canonical 4100 slot, zero-padded
--   * adds the model registry and points every row at a registry entry
--   * replaces the ANN index - which cannot exist on a 4100-wide vector column -
--     with the bit and prefix candidate surfaces plus exact rerank
--   * adds weighted tsvector full-text alongside the vector, so retrieval is
--     hybrid rather than vector-only

begin;

set local search_path = fanwaave, extensions, public;

create schema if not exists fanwaave;
create schema if not exists extensions;
create extension if not exists vector with schema extensions;
create extension if not exists pgcrypto with schema extensions;

-- fanwaave.message_embeddings does not exist yet in any deployed environment; this migration is the
-- create. It is byte-identical in effect to db/schema/postgres/0100_fanwaave_embeddings.sql,
-- which remains the declarative source of truth that dpm diffs against.

-- ---------------------------------------------------------------------
-- Re-apply the declarative schema. Everything below is idempotent and
-- matches db/schema/postgres/0100_fanwaave_embeddings.sql exactly; dpm diff
-- against that file must come back empty once this has run.
-- ---------------------------------------------------------------------
\i db/schema/postgres/0100_fanwaave_embeddings.sql

commit;
