-- Parameters: $1 tenant UUID, $2 padded extensions.vector(4100), $3 lexical text,
-- $4 purpose, $5 result limit, $6 embedding-space identity.
-- ANN runs only over the 4,000-dimension halfvec projection. The authoritative
-- unindexed 4,100-float vector is always used for the final exact rerank.
WITH halfvec_candidates AS MATERIALIZED (
  SELECT
    candidate.embedding_id,
    candidate.indexed_embedding OPERATOR(extensions.<=>)
      extensions.subvector($2::extensions.vector(4100), 1, 4000)::extensions.halfvec(4000)
      AS ann_distance
  FROM fanwaave.semantic_embedding_index AS candidate
  WHERE candidate.tenant_id = $1::UUID
    AND candidate.purpose = $4::TEXT
    AND candidate.embedding_space = $6::TEXT
  ORDER BY candidate.indexed_embedding OPERATOR(extensions.<=>)
    extensions.subvector($2::extensions.vector(4100), 1, 4000)::extensions.halfvec(4000)
  LIMIT LEAST(GREATEST($5::INT * 20, 100), 4000)
), exact_rerank AS (
  SELECT
    exact.embedding_id,
    exact.entity_kind,
    exact.entity_id,
    candidate.ann_distance,
    1 - (exact.embedding OPERATOR(extensions.<=>) $2::extensions.vector(4100)) AS semantic_score,
    CASE WHEN btrim($3::TEXT) = '' THEN 0
      ELSE ts_rank_cd(exact.search_document, websearch_to_tsquery('simple', $3::TEXT))
    END AS lexical_score
  FROM halfvec_candidates AS candidate
  JOIN fanwaave.semantic_embeddings AS exact
    ON exact.embedding_id = candidate.embedding_id
  WHERE exact.tenant_id = $1::UUID
    AND exact.purpose = $4::TEXT
    AND exact.embedding_space = $6::TEXT
    AND (exact.expires_at IS NULL OR exact.expires_at > now())
)
SELECT
  embedding_id,
  entity_kind,
  entity_id,
  semantic_score,
  lexical_score,
  (semantic_score * 0.75) + (least(lexical_score, 1) * 0.25) AS combined_score
FROM exact_rerank
ORDER BY combined_score DESC, ann_distance ASC, embedding_id ASC
LIMIT LEAST(GREATEST($5::INT, 1), 200);
