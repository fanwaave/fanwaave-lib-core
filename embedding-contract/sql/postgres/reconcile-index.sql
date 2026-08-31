-- Idempotent data reconciliation for the derived ANN projection.
--
-- declarative-postgres-migrate proves schema convergence; it intentionally
-- does not invent data transformations. Run this product-owned step after the
-- reviewed schema plan, using the dedicated migrator role. row_security=off
-- makes an under-privileged execution fail instead of silently seeing a
-- tenant-filtered subset.
BEGIN;
SET LOCAL row_security = off;

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
