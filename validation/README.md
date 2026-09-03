# Isomorphic validation SDK

This directory is the runtime validation boundary for `fanwaave/fanwaave-lib-core`.

- Public definitions are authored in the matching `*-interfaces` repository and are safe for browser, mobile, desktop, CLI, and server consumers.
- Server definitions live only in the `*-lib-core` repository. They may extend public definitions but are never copied into `*-clients`.
- Route bindings use the stable `operationId` vocabulary from `ORESoftware/api-docs`; validators do not invent a second route namespace.
- TypeSpec and JSON Schema/OpenAPI remain peer, top-level authorities. A mismatch is a stop-and-evaluate condition, never an automatic winner selection.
- Every language emits the same `ores.validation.v1` contract version and the same public model names.

Runtime choices are Zod for TypeScript, Garde for Rust, `go-playground/validator/v10` for Go, and the official Gleam dynamic decoder API for Gleam.

The public packages contain `RequestMeta`, `PageQuery`, and `ProblemDetails`. The sibling server packages add `TrustedActor`, `ServerRequestContext`, and `InternalCommand` without exporting those types to clients.
