#!/usr/bin/env node
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import Ajv2020 from "ajv/dist/2020.js";
import addFormats from "ajv-formats";

const read = (file) => readFile(new URL(`../${file}`, import.meta.url), "utf8");
const readJson = async (file) => JSON.parse(await read(file));
const emitted = (file) => `.typespec-output/@typespec/json-schema/${file}.json`;

const config = await readJson("embedding-contract/generation.json");
const schema = await readJson(config.codeFirst.jsonSchema);
const emittedStored = await readJson(emitted("StoredEmbedding"));
const emittedInput = await readJson(emitted("EmbeddingInput"));
const emittedEmbeddingProviders = await readJson(emitted("EmbeddingProvider"));
const emittedGenerationProviders = await readJson(emitted("GenerationProvider"));
const emittedPurposes = await readJson(emitted("EmbeddingPurpose"));
const emittedIndexProjection = await readJson(emitted("IndexedEmbeddingProjection"));

assert.equal(config.authority.repository, "fanwaave/fanwaave-lib-core");
assert.equal(config.authority.sqlAndCodeGenerationLocal, true);
assert.equal(config.authority.sharedDefinitionsRole, "inventory-pins-and-conformance-only");
assert.deepEqual(config.externalSqlAuthorities, []);
assert.equal(config.dimensions.storage, 4100);
assert.equal(config.dimensions.exactStorage, 4100);
assert.equal(config.dimensions.indexedProjection, 4000);
assert.equal(config.dimensions.maximumSource, 4096);
assert.equal(config.dimensions.padding, "trailing-zero");
assert.equal(config.migrationPlanning.tool, "declarative-migrations/declarative-postgres-migrate.rs");
assert.equal(config.migrationPlanning.runtimeMayApply, false);

// TypeSpec is compiled with Microsoft's JSON Schema emitter before this script.
// Compare its emitted wire constraints to the committed, runtime-facing bundle.
assert.equal(emittedStored.properties.storageDimensions.const, 4100);
assert.equal(emittedStored.properties.values.minItems, 4100);
assert.equal(emittedStored.properties.values.maxItems, 4100);
assert.equal(emittedStored.properties.values["x-zero-pad-after"], "originalDimensions");
assert.equal(emittedInput.properties.values.maxItems, 4096);
assert.deepEqual(emittedEmbeddingProviders.enum, schema.properties.embeddingProvider.enum);
assert.deepEqual(emittedGenerationProviders.enum, schema.properties.generationProvider.enum.filter(Boolean));
assert.deepEqual(emittedPurposes.enum, schema.properties.purpose.enum);
assert.equal(emittedIndexProjection.properties.indexedDimensions.const, 4000);
assert.equal(emittedIndexProjection.properties.values.minItems, 4000);
assert.equal(emittedIndexProjection.properties.values.maxItems, 4000);
assert.equal(schema.$defs.IndexedEmbeddingProjection.properties.indexedDimensions.const, 4000);
assert.equal(schema.$defs.IndexedEmbeddingProjection.properties.values.maxItems, 4000);
assert.equal(schema.properties.storageDimensions.const, 4100);
assert.equal(schema.properties.values["x-zero-pad-after"], "originalDimensions");
assert(schema.required.includes("embeddingSpace"));
assert(!schema.properties.embeddingProvider.enum.includes("anthropic"));
assert(schema.properties.generationProvider.enum.includes("anthropic"));

// Draft 2020-12 validation is executable, including the cross-field zero-tail
// keyword that neither array length constraints nor TypeSpec can express alone.
const ajv = new Ajv2020({ allErrors: true, strict: true });
addFormats(ajv);
ajv.addKeyword({
  keyword: "x-zero-pad-after",
  schemaType: "string",
  errors: false,
  validate(propertyName, values, _parentSchema, dataContext) {
    const originalDimensions = dataContext?.parentData?.[propertyName];
    return Number.isInteger(originalDimensions)
      && originalDimensions >= 1
      && originalDimensions <= values.length
      && values.slice(originalDimensions).every((value) => value === 0);
  },
});
const validateStored = ajv.compile(schema);
const values = Array.from({ length: 4100 }, (_, index) => index === 0 ? 1 : 0);
const validRecord = {
  tenantId: "00000000-0000-4000-8000-000000000001",
  entityKind: "message",
  entityId: "message-1",
  purpose: "message_deduplication",
  embeddingProvider: "openai",
  generationProvider: "anthropic",
  model: "text-embedding-3-small",
  originalDimensions: 1536,
  embeddingSpace: "openai:text-embedding-3-small:1536:provider",
  storageDimensions: 4100,
  values,
  normalization: "provider",
  contentHash: "0".repeat(64),
};
assert.equal(validateStored(validRecord), true, JSON.stringify(validateStored.errors));
const invalidTail = structuredClone(validRecord);
invalidTail.values[1536] = 0.25;
assert.equal(validateStored(invalidTail), false, "non-zero padding must fail JSON Schema validation");
const invalidProvider = structuredClone(validRecord);
invalidProvider.embeddingProvider = "anthropic";
assert.equal(validateStored(invalidProvider), false, "Anthropic is generation provenance, not an embedding provider");

const postgres = await read(config.databaseFirst.postgres);
const cockroach = await read(config.databaseFirst.cockroachdb);
const postgresQuery = await read(config.databaseFirst.postgresQuery);
const cockroachQuery = await read(config.databaseFirst.cockroachdbQuery);
const reconcileIndex = await read(config.databaseFirst.postgresIndexReconciliation);
const postgresPreflight = await read(config.databaseFirst.postgresFamilyPreflight);
const supabaseAdapter = await read(config.databaseFirst.supabasePrivateAdapter);
const supabaseVerification = await read(config.databaseFirst.supabasePrivilegeVerification);
const rust = await read(config.codeFirst.rustModel);
const projection = await read(config.codeFirst.ormProjection);

for (const sql of [postgres, cockroach]) {
  assert.match(sql, /VECTOR\(4100\)/i);
  assert.match(sql, /TSVECTOR/i);
  assert.match(sql, /ROW LEVEL SECURITY/i);
  assert.match(sql, /notification_dedupe_key/i);
  assert.match(sql, /embedding_tail_is_zero_4100/i);
  assert.match(sql, /subvector\(input, source_dimensions \+ 1, 4100 - source_dimensions\)/i);
}
for (const profile of config.models) {
  assert(profile.maximum <= config.dimensions.maximumSource);
  const tuple = `'${profile.provider}', '${profile.model}', ${profile.minimum}, ${profile.default}, ${profile.maximum}`;
  assert(postgres.includes(tuple), `PostgreSQL is missing model profile ${profile.provider}/${profile.model}`);
  assert(cockroach.includes(tuple), `CockroachDB is missing model profile ${profile.provider}/${profile.model}`);
  assert(rust.includes(`"${profile.model}"`), `Rust is missing model profile ${profile.model}`);
}
for (const model of ["Qwen/Qwen3-Embedding-8B", "nvidia/NV-Embed-v2", "BAAI/bge-en-icl"]) {
  assert.equal(config.models.find((profile) => profile.model === model)?.maximum, 4096);
}

assert.match(postgres, /semantic_embeddings[\s\S]+extensions\.vector\(4100\)/i);
assert.match(postgres, /semantic_embedding_index[\s\S]+extensions\.halfvec\(4000\)/i);
assert.match(postgres, /USING hnsw \(indexed_embedding extensions\.halfvec_cosine_ops\)/i);
assert.doesNotMatch(postgres, /binary_quantize/i);
assert.doesNotMatch(postgres, /USING hnsw\s*\(embedding\s+extensions\.vector_/i);
assert.match(postgres, /SECURITY INVOKER[\s\S]+SET search_path = ''/i);
assert.doesNotMatch(postgres, /CREATE EXTENSION[^;]+VERSION/i);
assert.match(cockroach, /CREATE VECTOR INDEX/i);
assert.match(cockroach, /tenant_id, embedding_space, purpose, embedding/);
assert.match(postgresQuery, /exact_rerank/i);
assert.match(postgresQuery, /halfvec_candidates/i);
assert.match(postgresQuery, /extensions\.halfvec\(4000\)/i);
assert.match(postgresQuery, /extensions\.vector\(4100\)/i);
assert.match(postgresQuery, /embedding_space = \$6::TEXT/i);
assert.match(cockroachQuery, /tenant_id = \$1::UUID/i);
assert.match(cockroachQuery, /embedding_space = \$6::STRING/i);
assert.match(rust, /EMBEDDING_STORAGE_DIMENSIONS: usize = 4100/);
assert.match(rust, /EMBEDDING_INDEX_DIMENSIONS: usize = 4000/);
assert.match(rust, /MAXIMUM_SOURCE_DIMENSIONS: usize = 4096/);
assert.match(projection, /derive\(Debug, Clone, PartialEq, Queryable, Selectable, Identifiable\)/);
assert.match(projection, /derive\(Clone, Debug, PartialEq, DeriveEntityModel\)/);
assert.match(projection, /pgvector::\{HalfVector, Vector as PgVector\}/);
assert.doesNotMatch(projection, /DatabaseConnection|MigrationTrait|execute_unprepared/);
assert.match(reconcileIndex, /SET LOCAL row_security = off/);
assert.match(reconcileIndex, /ON CONFLICT \(embedding_id\) DO UPDATE SET/);
assert.match(postgresPreflight, /CREATE EXTENSION IF NOT EXISTS vector WITH SCHEMA extensions/);
assert.doesNotMatch(postgresPreflight, /VERSION\s+['"]/i);
assert.match(supabaseAdapter, /REVOKE ALL ON SCHEMA fanwaave FROM anon, authenticated/);
assert.match(supabaseAdapter, /GRANT[\s\S]+TO service_role/);
assert.doesNotMatch(supabaseAdapter, /GRANT[\s\S]+TO anon|GRANT[\s\S]+TO authenticated/);
assert.match(supabaseVerification, /has_table_privilege\('anon'/);

process.stdout.write("embedding contract verified: 4000-dimension halfvec ANN, exact 4100-vector rerank, PostgreSQL/Neon base, private Supabase adapter, Diesel + SeaORM projections, and product-owned DPM inputs\n");
