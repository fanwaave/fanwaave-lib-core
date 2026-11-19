# Fanwaave authenticated, read-only contact lookup (stage 1)

Source authorities:
- fanwaave-interfaces/contracts/contact-delivery.tsp (ContactJobView)
- migrations/0002_contact_delivery.sql (contact_jobs table)
- fanwaave-api-server.rs/src/routes/contact.rs (GET contact job by UUID)

This draft implements **only** one existing operation, reading a job by ID.
The exported ReadContext is a real SQLx PostgreSQL pool and a parameterized
SELECT returning the canonical ContactJobView. It is not a mock, SQL
string-builder, arbitrary SQL API, or a migration engine.

## Execution boundary

ReadContext::connect accepts a CoreConfig with read_only=true and a dedicated
secret of 32+ characters. Credentials must be injected by the owning service's
approved configuration, never committed. At connection open, SQLx configures
every pooled connection with default_transaction_read_only=on. The real
PostgreSQL role must have SELECT privilege on public.contact_jobs and
**no** INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES or TRIGGER privilege
on that table; otherwise connection setup fails closed. A service's
dedicated database role must additionally lack write/DDL privileges on
every other object. These role grants cannot be enforced by app code alone.

get_contact_job(id, authorization) checks the exact Bearer credential in
constant time BEFORE querying. A missing record is None; unknown persisted
enum strings or invalid counters are typed InvalidData errors. Debug and
Display for ReadContext/errors never include DSNs, bearer credentials or
raw database driver errors.

This follows the existing *service-level* token shape in the API, but
does **not** imply per-user, tenant-scoped authorization. Do not give this
secret to clients. An externally accessible route requires independent
identity, policy, tenant/resource authorization and review.

The first stage supports **PostgreSQL only**. CockroachDB must be separately
tested for read-only transaction behavior and role privilege introspection
before enabling it. The schema check at open validates the selected columns
and the existing fanwaave-0002 shape; it does NOT certify a migration history
table or an exact DB revision, because the current canonical migration
defines no such table.

## What actually runs in CI

A source-repository GitHub Actions job brings up PostgreSQL 16, applies the
canonical migration, inserts a real fixture, and grants a separate SELECT-only
role. Rust tests use a real SQLx connection to verify:
- get_contact_job returns the persisted fixture, not a stub
- unknown UUID returns None
- bad/empty bearer credentials are rejected
- the read-only role cannot UPDATE or INSERT even on a separate connection
- a PostgreSQL owner/admin role is refused for its excess privileges
- disabled read-only config, weak credential and unverified Cockroach flavor
  fail before networking
- formatted output does not leak credentials or DSNs

The job separately compiles without default features, compiles and tests the
read-only feature, and runs strict Clippy. A skipped job or failed fetch does
not prove Rust compilation. No alternate-org mirror is needed when source
Actions can execute; private dependencies may only be admitted by owners.

## Downstream gateway integration not yet approved

This draft is **not** a completed ores_transport::DirectReader integration:
- An explicit service-owned Operation enum and per-resource authorization
  contract are still required for the four-avenue Gateway.
- The web server is pinned to a different immutable lib-core SHA; do not
  change its pin or enable readiness until this candidate is fully reviewed.
- Full four transport implementation, TLS connectors and mTLS handshakes are
  unrelated release gates tracked in web-server issue #19.
- Private Cargo admission issue #18 and ores-cli release #457 remain separate.

Never promote a test-only SELECT role or credential into production.

## Integration-test feature admission

The real PostgreSQL integration tests are gated by the non-default
`db-integration-tests` Cargo feature, which implies `read-only`. The dedicated
source CI selects both features and supplies actual PostgreSQL 16, role grants,
and test DSNs. Other repository workflows that compile or test the default
library never run DB-dependent tests without database infrastructure. This is
an explicit opt-in to testing infrastructure, **not** a skip within the
admitted database test job. That job runs the four real DB tests and fails
if its DSNs, migration, or role are unavailable.

## Scoped grant verification and expanded database privilege audit

The former shared, unscoped service bearer has been removed. The read context
now accepts a bounded allowlist of verification keys. Each authorization
request must be a signed HS256 JWT using a known JOSE kid, with the exact
issuer, audience, nonempty principal, contact_job.read scope, canonical
contact-job UUID matching the requested resource, and required issue,
not-before, expiry, and request ID fields. The maximum grant lifetime is
five minutes; unknown signing keys, altered payloads, expired grants and
grants for a different job fail before any SQL lookup. A rotation window may
accept two configured keys. Removing a key from a newly created context
immediately rejects grants issued with that key. The principal-to-job policy
is enforced by the independently trusted issuer, NOT by reading an unsigned
claim or by this database's tenant schema (there is no tenant_id).

The verification keys MUST come from a trusted secret manager or approved
runtime configuration and MUST NOT be sent to public clients. This library
**never signs or issues** grants; the CI-only Rust test helper issues fixture
JWTs. Bearer tokens are replayable until expiry, so short TTL, TLS and
upstream issuer security remain mandatory. No jti-replay cache exists yet.

Database admission also rejects PostgreSQL roles with elevated role flags,
CREATE rights on the database or application schemas, and write permissions
on any application table (not merely contact_jobs). The source CI creates an
additional SELECT-capable principal with UPDATE on an unrelated table and
asserts it cannot open the reader. Application checks DO NOT replace database
GRANT policy or prove the safety of arbitrary SECURITY DEFINER functions.
The reader also checks PostgreSQL's actual server version after connecting,
so hostname-based config classification alone cannot admit CockroachDB.

This draft pins fanwaave-interfaces commit
9fe7e98120a4fade4092fc5cf2eb555041fb26ef (draft contract PR #24).
After independent review and integration, update the pin to the merged,
immutable approved interface revision. The web server and private transport
source have not been repinned, and nothing is production-deployed.
