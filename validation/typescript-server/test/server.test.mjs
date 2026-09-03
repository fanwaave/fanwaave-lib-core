import assert from "node:assert/strict";
import test from "node:test";
import {
  InternalCommandSchema,
  ServerRequestContextSchema,
  TrustedActorSchema,
} from "../dist/server.js";

const validActor = {
  userId: "user-1",
  tenantId: "tenant-1",
  roles: ["reader"],
};

const validContext = {
  requestId: "req-1",
  traceId: "trace-1",
  actor: validActor,
  sourceIp: "127.0.0.1",
};

test("accepts a bounded trusted actor and request context", () => {
  assert.deepEqual(TrustedActorSchema.parse(validActor), validActor);
  assert.deepEqual(ServerRequestContextSchema.parse(validContext), validContext);
});

test("preserves exact trusted identity strings", () => {
  const actor = { userId: " user-1 ", roles: [] };
  assert.deepEqual(TrustedActorSchema.parse(actor), actor);
});

for (const [name, actor] of [
  ["empty user id", { userId: "", roles: [] }],
  ["oversized user id", { userId: "u".repeat(129), roles: [] }],
  ["empty role", { userId: "user-1", roles: [""] }],
  ["oversized role", { userId: "user-1", roles: ["r".repeat(129)] }],
  ["too many roles", { userId: "user-1", roles: Array.from({ length: 65 }, () => "reader") }],
  ["unknown actor field", { userId: "user-1", roles: [], token: "must-not-leak" }],
]) {
  test(`rejects trusted actor with ${name}`, () => {
    assert.equal(TrustedActorSchema.safeParse(actor).success, false);
  });
}

for (const [name, context] of [
  ["invalid source IP", { ...validContext, sourceIp: "not-an-ip" }],
  ["unknown public identity", { ...validContext, userId: "client-supplied" }],
  ["invalid nested request metadata", { ...validContext, requestId: "" }],
]) {
  test(`rejects server request context with ${name}`, () => {
    assert.equal(ServerRequestContextSchema.safeParse(context).success, false);
  });
}

test("accepts a complete internal command", () => {
  const command = {
    operationId: "alerts.create",
    idempotencyKey: "idem-1",
    context: validContext,
    payload: { message: "hello" },
  };
  assert.deepEqual(InternalCommandSchema.parse(command), command);
});

for (const [name, command] of [
  ["missing operation id", { context: validContext, payload: {} }],
  ["empty operation id", { operationId: "", context: validContext, payload: {} }],
  ["oversized operation id", { operationId: "o".repeat(257), context: validContext, payload: {} }],
  ["oversized idempotency key", { operationId: "alerts.create", idempotencyKey: "i".repeat(129), context: validContext, payload: {} }],
  ["unknown command field", { operationId: "alerts.create", context: validContext, payload: {}, credential: "secret" }],
]) {
  test(`rejects internal command with ${name}`, () => {
    assert.equal(InternalCommandSchema.safeParse(command).success, false);
  });
}
