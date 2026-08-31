-- Executable privilege assertions for Supabase and ordinary PostgreSQL test
-- clusters that define the anon/authenticated/service_role roles.
DO $$
BEGIN
  IF has_schema_privilege('anon', 'fanwaave', 'USAGE') THEN
    RAISE EXCEPTION 'anon must not have USAGE on fanwaave';
  END IF;
  IF has_schema_privilege('authenticated', 'fanwaave', 'USAGE') THEN
    RAISE EXCEPTION 'authenticated must not have USAGE on fanwaave';
  END IF;
  IF has_table_privilege('anon', 'fanwaave.semantic_embeddings', 'SELECT') THEN
    RAISE EXCEPTION 'anon must not read exact embeddings';
  END IF;
  IF has_table_privilege('authenticated', 'fanwaave.semantic_embedding_index', 'SELECT') THEN
    RAISE EXCEPTION 'authenticated must not read ANN projections';
  END IF;
  IF NOT has_table_privilege('service_role', 'fanwaave.semantic_embeddings', 'SELECT') THEN
    RAISE EXCEPTION 'service_role must read exact embeddings';
  END IF;
  IF NOT has_table_privilege('service_role', 'fanwaave.semantic_embedding_index', 'SELECT') THEN
    RAISE EXCEPTION 'service_role must read ANN projections';
  END IF;
END
$$;
