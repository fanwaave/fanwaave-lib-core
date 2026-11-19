# Contact-job read contract (draft; fanwaave-web-server.rs issue #19)

This is an opt-in real PostgreSQL read path for contact jobs already defined
by the canonical migration, ContactJobView, and fanwaave-interfaces contact
delivery JSON Schema. Enable postgres-read alongside read-only.

There is no dynamic SQL, no general-purpose DB query method, no mutation API
and no permissive authorizer. The caller must inject a ReadAuthorizer that
verifies identity AND per-record authorization for each ContactJob UUID.
Only the test harness contains a test-only token policy. Production tenancy
and identity rules require the domain/security owner's independent approval.

On connect, the actual PostgreSQL role must SELECT contact_jobs but cannot
INSERT, UPDATE, DELETE or TRUNCATE. A zero-row real statement validates the
table and named column projection; a config flag alone is never sufficient.
Each read authorizes the specific job before contacting the database, binds
the UUID parameter in SQLx, and uses an explicit READ ONLY transaction.

Errors are typed and deliberately avoid printing the DB URL, raw SQLx errors,
database row contents or credentials. The app must provision a role with
USAGE on public and SELECT only on public.contact_jobs.

This is a one-operation implementation. It does not validate the complete
schema revision, implement multi-tenant scoping in the database, add an API
envelope, wire TCP/HTTP/NATS, or prove TLS. The existing contact_jobs schema
contains no tenant identifier. Therefore do NOT ship a web gateway based
on this alone; the domain owner must ratify per-operation authorization.

Exact-head source CI runs a disposable PostgreSQL 16 service, applies the
canonical migration, creates an ephemeral restricted role, reads a real
persisted job, rejects wrong tokens and record IDs, demonstrates an actual
SQL UPDATE denial, rejects privileged roles and rejects write-enabled config.

The web server still pins an older lib-core SHA. After independent review,
updating that pin and supplying a production identity policy are separate
changes requiring their own real integration evidence.
