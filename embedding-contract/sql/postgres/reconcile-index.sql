-- Idempotent data reconciliation for the derived ANN projection.
--
-- declarative-postgres-migrate proves schema convergence; it intentionally
-- does not invent data transformations. Run this product-owned step after the
-- reviewed schema plan, using the dedicated migrator role. row_security=off
-- makes an under-privileged execution fail instead of silently seeing a
-- tenant-filtered subset.
BEGIN;
SET LOCAL row_security = off;

-- DPM is schema-only, so the product-owned model registry is reconciled here
-- rather than relying on INSERT statements used to materialize desired.sql.
INSERT INTO fanwaave.embedding_model_profiles
  (embedding_provider, model, minimum_dimensions, default_dimensions, maximum_dimensions, supports_mrl, provider_returns_normalized)
VALUES
  ('openai', 'text-embedding-ada-002', 1536, 1536, 1536, false, true),
  ('openai', 'text-embedding-3-small', 1, 1536, 1536, true, true),
  ('openai', 'text-embedding-3-large', 1, 3072, 3072, true, true),
  ('google', 'gemini-embedding-001', 128, 3072, 3072, true, false),
  ('qwen', 'Qwen/Qwen3-Embedding-8B', 32, 4096, 4096, true, true),
  ('nvidia', 'nvidia/NV-Embed-v2', 4096, 4096, 4096, false, true),
  ('baai', 'BAAI/bge-en-icl', 4096, 4096, 4096, false, true),
  ('voyage', 'voyage-4-large', 256, 1024, 2048, true, true),
  ('voyage', 'voyage-4', 256, 1024, 2048, true, true),
  ('voyage', 'voyage-4-lite', 256, 1024, 2048, true, true)
ON CONFLICT (embedding_provider, model) DO UPDATE SET
  minimum_dimensions = EXCLUDED.minimum_dimensions,
  default_dimensions = EXCLUDED.default_dimensions,
  maximum_dimensions = EXCLUDED.maximum_dimensions,
  supports_mrl = EXCLUDED.supports_mrl,
  provider_returns_normalized = EXCLUDED.provider_returns_normalized;

INSERT INTO fanwaave.semantic_embedding_index (
  embedding_id,
  tenant_id,
  embedding_space,
  purpose,
  indexed_dimensions,
  indexed_embedding,
  embedded_at
)
SELECT
  exact.embedding_id,
  exact.tenant_id,
  exact.embedding_space,
  exact.purpose,
  4000,
  extensions.subvector(exact.embedding, 1, 4000)::extensions.halfvec(4000),
  exact.embedded_at
FROM fanwaave.semantic_embeddings AS exact
ON CONFLICT (embedding_id) DO UPDATE SET
  tenant_id = EXCLUDED.tenant_id,
  embedding_space = EXCLUDED.embedding_space,
  purpose = EXCLUDED.purpose,
  indexed_dimensions = EXCLUDED.indexed_dimensions,
  indexed_embedding = EXCLUDED.indexed_embedding,
  embedded_at = EXCLUDED.embedded_at;

DELETE FROM fanwaave.semantic_embedding_index AS candidate
WHERE NOT EXISTS (
  SELECT 1
  FROM fanwaave.semantic_embeddings AS exact
  WHERE exact.embedding_id = candidate.embedding_id
);

DO $$
DECLARE
  exact_count BIGINT;
  candidate_count BIGINT;
BEGIN
  SELECT count(*) INTO exact_count FROM fanwaave.semantic_embeddings;
  SELECT count(*) INTO candidate_count FROM fanwaave.semantic_embedding_index;
  IF exact_count <> candidate_count THEN
    RAISE EXCEPTION
      'semantic embedding index reconciliation failed: exact %, indexed %',
      exact_count,
      candidate_count;
  END IF;
END
$$;

COMMIT;
ANALYZE fanwaave.semantic_embedding_index;
