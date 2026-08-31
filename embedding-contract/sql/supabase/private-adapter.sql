-- Supabase adapter for the portable PostgreSQL desired state.
--
-- The fanwaave schema is intentionally NOT a browser-facing Data API schema.
-- Embeddings contain tenant data and are accessed by server-side named ORM
-- operations. This adapter makes the boundary explicit instead of relying on
-- historical Supabase default privileges. Do not add fanwaave to the Data API
-- exposed-schema list without a separate, reviewed auth.uid()-based API layer.

REVOKE ALL ON SCHEMA fanwaave FROM anon, authenticated;
REVOKE ALL ON ALL TABLES IN SCHEMA fanwaave FROM anon, authenticated;
REVOKE ALL ON ALL SEQUENCES IN SCHEMA fanwaave FROM anon, authenticated;
REVOKE EXECUTE ON ALL FUNCTIONS IN SCHEMA fanwaave FROM anon, authenticated;

ALTER DEFAULT PRIVILEGES IN SCHEMA fanwaave
  REVOKE ALL ON TABLES FROM anon, authenticated;
ALTER DEFAULT PRIVILEGES IN SCHEMA fanwaave
  REVOKE ALL ON SEQUENCES FROM anon, authenticated;
ALTER DEFAULT PRIVILEGES IN SCHEMA fanwaave
  REVOKE EXECUTE ON FUNCTIONS FROM anon, authenticated;

GRANT USAGE ON SCHEMA fanwaave TO service_role;
GRANT SELECT ON fanwaave.embedding_model_profiles TO service_role;
GRANT SELECT, INSERT, UPDATE, DELETE
  ON fanwaave.semantic_embeddings,
     fanwaave.semantic_embedding_index,
     fanwaave.semantic_alert_rules,
     fanwaave.semantic_match_events
  TO service_role;
GRANT EXECUTE ON FUNCTION fanwaave.pad_embedding_4100(REAL[]) TO service_role;

-- Trigger routines are never a public RPC surface.
REVOKE ALL ON FUNCTION fanwaave.sync_semantic_embedding_index_4000()
  FROM PUBLIC, anon, authenticated, service_role;
