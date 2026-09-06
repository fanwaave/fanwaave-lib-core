# Fanwaave embedding contract v3

This product-owned contract covers semantic message deduplication and redundant-notification suppression. It supports provider
outputs up to 4,096 dimensions and stores every vector in exactly 4,100 slots.
The padding operation only appends zeros; it never truncates a source vector or
pretends the padding contains learned information.

## Provider provenance

`embeddingProvider` records the service or model family that actually emitted
the vector. Anthropic is deliberately not an embedding-provider value because
Anthropic does not offer an embedding model. When Claude produced or transformed
the source material, record `generationProvider = anthropic` and record the
actual embedding provider separately (for example, Voyage, OpenAI, or Google).

OpenAI `text-embedding-3-large` defaults to 3,072 dimensions. A 1,536 value is
valid only when the caller explicitly requests the shortened output.

## Storage and indexing

- PostgreSQL, Neon, and Supabase keep the authoritative value in the unindexed
  `semantic_embeddings.embedding extensions.vector(4100)` column. A separate
  one-to-one `semantic_embedding_index` table holds the deterministic first
  4,000 values as `extensions.halfvec(4000)`, pgvector's HNSW ceiling for that
  type. Candidate generation uses this projection and always reranks against
  all 4,100 full-precision values. Lexical ranking uses a GIN-indexed
  `tsvector` on the exact table.
- A database trigger maintains new projections without an application-level
  dual-write race. The idempotent `sql/postgres/reconcile-index.sql` handles
  existing rows; DPM remains schema-only and verifies the resulting schema.
- The Supabase adapter keeps product vector tables outside the browser-facing
  Data API by revoking `anon` and `authenticated` privileges explicitly.
  Exposing a client API requires a separate reviewed ownership layer.
- CockroachDB stores `VECTOR(4100)`, uses a tenant-prefixed native vector
  index, and combines it with a GIN-indexed `TSVECTOR`.
- Every row has a deterministic embedding-space identity (provider, model,
  original dimension, and normalization). Search requires that identity, so
  padding never causes mathematically unrelated model spaces to be compared.
- Tenant predicates and forced row-level security are applied before candidate
  rows become visible. Alert-match rows carry a deduplication key so retries do
  not emit duplicate notifications.

## Code-first and DB-first agreement

This lib-core repository keeps Microsoft TypeSpec and JSON Schema beside the
Rust model and the Diesel/SeaORM projection. Product-owned desired-state SQL,
model-profile inputs, and generation configuration are authoritative here.
`fanwaave-orm-core` may package reviewed generated mirrors, but it does not own
or independently edit those artifacts.

Run `npm ci --ignore-scripts && npm run check`, then
`cargo check --all-targets --features orm-projections`. Migration jobs use
`dpm diff` and `dpm verify` against the local desired SQL for PostgreSQL, Neon,
and Supabase, require review of the generated plan, and never run from
application startup. The ORESoftware shared
definitions repository may inventory and pin this source revision; it is not a
fallback SQL or code-generation authority.
