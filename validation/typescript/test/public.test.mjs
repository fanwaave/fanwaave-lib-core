import assert from "node:assert/strict";
import test from "node:test";
import { parsePublic, safeParsePublic } from "../dist/public.js";

const validProblem = {
  type: "https://example.test/problems/invalid",
  title: "Invalid request",
  status: 400,
  requestId: "req-1",
};

test("accepts and preserves exact request metadata", () => {
  const value = {
    requestId: " req-1 ",
    traceId: " trace-1 ",
    locale: "en",
  };
  assert.deepEqual(parsePublic("request-meta", value), value);
});

test("accepts request metadata at every length boundary", () => {
  assert.equal(safeParsePublic("request-meta", {
    requestId: "r".repeat(128),
    traceId: "t".repeat(128),
    locale: "l".repeat(64),
  }).success, true);
});

for (const [name, value] of [
  ["missing trace id", { requestId: "req-1" }],
  ["empty request id", { requestId: "", traceId: "trace-1" }],
  ["oversized request id", { requestId: "r".repeat(129), traceId: "trace-1" }],
  ["short locale", { requestId: "req-1", traceId: "trace-1", locale: "e" }],
  ["oversized locale", { requestId: "req-1", traceId: "trace-1", locale: "l".repeat(65) }],
  ["unknown browser identity", { requestId: "req-1", traceId: "trace-1", userId: "client-supplied" }],
]) {
  test(`rejects request metadata with ${name}`, () => {
    assert.equal(safeParsePublic("request-meta", value).success, false);
  });
}

test("accepts page-query bounds", () => {
  assert.deepEqual(parsePublic("page-query", { limit: 1 }), { limit: 1 });
  assert.deepEqual(parsePublic("page-query", { limit: 100, cursor: "c".repeat(512) }), {
    limit: 100,
    cursor: "c".repeat(512),
  });
});

for (const [name, value] of [
  ["missing required limit", {}],
  ["zero limit", { limit: 0 }],
  ["oversized limit", { limit: 101 }],
  ["fractional limit", { limit: 1.5 }],
  ["empty cursor", { limit: 50, cursor: "" }],
  ["oversized cursor", { limit: 50, cursor: "c".repeat(513) }],
  ["unknown field", { limit: 50, offset: 1 }],
]) {
  test(`rejects page query with ${name}`, () => {
    assert.equal(safeParsePublic("page-query", value).success, false);
  });
}

test("accepts problem-details status and detail boundaries", () => {
  assert.equal(safeParsePublic("problem-details", validProblem).success, true);
  assert.equal(safeParsePublic("problem-details", {
    ...validProblem,
    status: 599,
    detail: "d".repeat(4096),
  }).success, true);
});

for (const [name, value] of [
  ["status below range", { ...validProblem, status: 399 }],
  ["status above range", { ...validProblem, status: 600 }],
  ["fractional status", { ...validProblem, status: 400.5 }],
  ["empty type", { ...validProblem, type: "" }],
  ["empty title", { ...validProblem, title: "" }],
  ["oversized detail", { ...validProblem, detail: "d".repeat(4097) }],
  ["unknown server field", { ...validProblem, internalCode: "secret" }],
]) {
  test(`rejects problem details with ${name}`, () => {
    assert.equal(safeParsePublic("problem-details", value).success, false);
  });
}
