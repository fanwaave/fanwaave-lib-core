// Conformance lint for the model-space v3 desired state.
//
// Two defects were found by executing the shipped desired-v3.sql against a real
// PostgreSQL rather than by reading it. Both are invisible to review and to any
// check that only reads the registry JSON, and both would be reintroduced the
// next time an org's SQL is written, because there is no shared generator - each
// org's desired state is authored per org.
//
// This lint therefore lands in every org, including the ones whose SQL does not
// exist yet, so that the defects cannot come back silently.
//
//   1. UNQUALIFIED PGVECTOR CALLS IN A SQL FUNCTION BODY
//      pg_restore sets search_path to the empty string before loading data. A
//      SQL function body resolves names at CALL time, so a CHECK constraint that
//      calls a function whose body says vector_dims(...) instead of
//      public.vector_dims(...) raises during the restore. Every COPY row is
//      rejected, the table restores EMPTY, and pg_restore reports a warning and
//      exits 0 - so a routine restore looks successful and silently produces a
//      corpus with no embeddings.
//
//   2. BARE CREATE POLICY
//      PostgreSQL has no CREATE POLICY IF NOT EXISTS. The rest of the desired
//      state is additive, so a re-apply gets all the way to the first policy and
//      then aborts. "Forward-only replayable" is not true unless each policy is
//      guarded.
//
// Exit code 0 when clean or when there is no desired SQL yet; 1 with a report
// otherwise. No dependencies.

import { readdirSync, readFileSync, statSync } from 'node:fs';
import { join } from 'node:path';

const ROOTS = ['embedding-contract/sql', 'sql', 'db/schema'];

// pgvector functions whose resolution depends on search_path, plus the
// pg_catalog helpers the padding function uses. Operators are not listed: they
// resolve through the operator search path the same way, but the desired state
// already writes them as OPERATOR(public.<=>) where it uses them in a function.
const SEARCH_PATH_SENSITIVE = [
  'vector_dims', 'vector_norm', 'l2_normalize', 'binary_quantize', 'subvector',
  'cosine_distance', 'inner_product', 'l2_distance', 'l1_distance',
  'array_fill', 'cardinality',
];

// Every .sql path under dir, in directory order; an unreadable dir contributes nothing.
const findSql = (dir) => {
  const entries = (() => { try { return readdirSync(dir); } catch { return []; } })();
  return entries.flatMap((e) => {
    const p = join(dir, e);
    if (statSync(p).isDirectory()) return findSql(p);
    return e.endsWith('.sql') ? [p] : [];
  });
};

// Function bodies are dollar-quoted. Capture the tag so a body containing a
// different tag does not terminate it early.
function sqlFunctionBodies(text) {
  const re = /CREATE\s+(?:OR\s+REPLACE\s+)?FUNCTION\s+([\w".]+)\s*\(([\s\S]*?)\)\s*RETURNS([\s\S]*?)AS\s+(\$[A-Za-z_]*\$)/gi;
  return [...text.matchAll(re)].flatMap((m) => {
    const [full, name, , preamble, tag] = m;
    const bodyStart = m.index + full.length;
    const end = text.indexOf(tag, bodyStart);
    if (end === -1) return [];
    return [{
      name,
      preamble,
      body: text.slice(bodyStart, end),
      language: /LANGUAGE\s+(\w+)/i.exec(preamble)?.[1]?.toLowerCase() ?? '',
      hasSetSearchPath: /\bSET\s+search_path\s*=/i.test(preamble),
      line: text.slice(0, m.index).split('\n').length,
    }];
  });
}

// True when the identifier at this position is already schema-qualified: the
// nearest non-whitespace character before it is a dot.
const isQualified = (text, index) => /\.\s*$/.test(text.slice(0, index));

// Every unqualified call of callee inside one function body.
const unqualifiedCalls = (file, fn, callee) =>
  [...fn.body.matchAll(new RegExp(`\\b${callee}\\s*\\(`, 'g'))]
    .filter((m) => !isQualified(fn.body, m.index))
    .map((m) => ({
      file,
      line: fn.line + fn.body.slice(0, m.index).split('\n').length - 1,
      fn: fn.name,
      callee,
    }));

function checkUnqualifiedCalls(file, text) {
  // A plpgsql body resolves at execution with the caller's path too, but the
  // restore path only evaluates CHECK constraints, which must be LANGUAGE SQL
  // or plpgsql; both are covered. A SET search_path clause makes it safe.
  return sqlFunctionBodies(text)
    .filter((fn) => !fn.hasSetSearchPath)
    .flatMap((fn) => SEARCH_PATH_SENSITIVE.flatMap((callee) => unqualifiedCalls(file, fn, callee)));
}

// A CREATE POLICY is guarded when the nearest preceding statement boundary is a
// DO block opener rather than a semicolon.
function checkBarePolicies(file, text) {
  return [...text.matchAll(/CREATE\s+POLICY\s+([\w"]+)/gi)].flatMap((m) => {
    const before = text.slice(0, m.index);
    const lastSemi = before.lastIndexOf(';');
    const lastDo = before.search(/DO\s+\$[A-Za-z_]*\$\s*BEGIN[^;]*$/i);
    const guarded = lastDo !== -1 && lastDo > lastSemi;
    return guarded ? [] : [{ file, line: before.split('\n').length, policy: m[1] }];
  });
}

const files = ROOTS.flatMap((r) => findSql(r));
if (files.length === 0) {
  console.log(
    'verify-desired-sql-v3: no desired-state SQL in this repository yet; ' +
      'the lint is in place for when there is.',
  );
  process.exit(0);
}

// CockroachDB has no pgvector and no pg_restore; these checks are about
// PostgreSQL restore and replay semantics.
const sources = files
  .filter((f) => !/cockroach/i.test(f))
  .map((f) => ({ file: f, text: readFileSync(f, 'utf8') }));
const unqualified = sources.flatMap(({ file, text }) => checkUnqualifiedCalls(file, text));
const bare = sources.flatMap(({ file, text }) => checkBarePolicies(file, text));

if (unqualified.length === 0 && bare.length === 0) {
  console.log(`verify-desired-sql-v3: ${files.length} SQL file(s) conform.`);
  process.exit(0);
}

if (unqualified.length) {
  console.error(
    '\nUnqualified search_path-sensitive calls inside a SQL function body.\n' +
      'pg_restore loads data with search_path set to the empty string. A CHECK\n' +
      'that calls one of these functions will raise during the restore, every\n' +
      'COPY row will be rejected, and the table will restore EMPTY while\n' +
      'pg_restore exits 0. Schema-qualify them (public.vector_dims(...)), or add\n' +
      'SET search_path to the function - qualification is preferred, because a\n' +
      'SET clause makes the function non-inlinable and this one runs per row.\n',
  );
  for (const p of unqualified) {
    console.error(`  ${p.file}:${p.line}  ${p.fn} calls ${p.callee}() unqualified`);
  }
}

if (bare.length) {
  console.error(
    '\nUnguarded CREATE POLICY. PostgreSQL has no CREATE POLICY IF NOT EXISTS,\n' +
      'so re-applying the desired state aborts here. Wrap each policy:\n\n' +
      '  DO $policy$ BEGIN\n' +
      '    CREATE POLICY ... ;\n' +
      '  EXCEPTION WHEN duplicate_object THEN NULL; END $policy$;\n\n' +
      'DROP POLICY IF EXISTS then CREATE also works but leaves the table briefly\n' +
      'unprotected; the DO block never does.\n',
  );
  for (const p of bare) {
    console.error(`  ${p.file}:${p.line}  policy ${p.policy} is not replay-guarded`);
  }
}

console.error(
  `\nverify-desired-sql-v3: ${unqualified.length} unqualified call(s), ` +
    `${bare.length} unguarded policy(ies).`,
);
process.exit(1);
