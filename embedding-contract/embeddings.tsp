/**
 * Fanwaave embedding contract v2.
 *
 * Code-first consumers compile this TypeSpec. DB-first generation validates
 * the resulting shape against the product-owned SQL and JSON Schema artifacts.
 */
import "@typespec/json-schema";

using TypeSpec.JsonSchema;

@jsonSchema
@baseUri("https://github.com/fanwaave/fanwaave-lib-core/raw/main/embedding-contract/")
namespace Fanwaave.EmbeddingV2;

const storageDimensions = 4100;
const maximumSourceDimensions = 4096;

enum EmbeddingProvider {
  OpenAi: "openai",
  Google: "google",
  Voyage: "voyage",
  Qwen: "qwen",
  Nvidia: "nvidia",
  Baai: "baai",
  Custom: "custom",
}

enum GenerationProvider {
  OpenAi: "openai",
  Anthropic: "anthropic",
  Google: "google",
  Qwen: "qwen",
  Nvidia: "nvidia",
  Baai: "baai",
  Custom: "custom",
}

enum EmbeddingPurpose {
  MessageDeduplication: "message_deduplication",
  NotificationSuppression: "notification_suppression",
  ContentSearch: "content_search",
}

enum EmbeddingNormalization {
  Provider: "provider",
  L2: "l2",
  None: "none",
}

model EmbeddingInput {
  embeddingProvider: EmbeddingProvider;
  generationProvider?: GenerationProvider;
  @minLength(1)
  @maxLength(200)
  `model`: string;
  @minItems(1)
  @maxItems(4096)
  values: float32[];
}

model StoredEmbedding {
  @format("uuid")
  tenantId: string;
  @minLength(1)
  @maxLength(80)
  @pattern("^[a-z][a-z0-9_]*$")
  entityKind: string;
  @minLength(1)
  @maxLength(256)
  entityId: string;
  purpose: EmbeddingPurpose;
  embeddingProvider: EmbeddingProvider;
  generationProvider?: GenerationProvider;
  @minLength(1)
  @maxLength(200)
  `model`: string;
  @minValue(1)
  @maxValue(4096)
  originalDimensions: int32;
  @minLength(8)
  @maxLength(320)
  embeddingSpace: string;
  storageDimensions: 4100;
  @minItems(4100)
  @maxItems(4100)
  @extension("x-zero-pad-after", "originalDimensions")
  values: float32[];
  normalization: EmbeddingNormalization;
  searchText?: string;
  @minLength(64)
  @maxLength(64)
  @pattern("^[0-9a-f]{64}$")
  contentHash: string;
  metadata?: Record<unknown>;
}
