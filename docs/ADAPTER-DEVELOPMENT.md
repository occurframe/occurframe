# Adapter development

> Integrating your own engine? Read [Writing a runner](WRITING-A-RUNNER.md)
> instead. That is the public, normative description of protocol `2.0`, and it
> assumes no knowledge of this repository. This page covers the in-repository
> adapters used for Occurframe's own certified differential evidence.

An adapter is intentionally small:

```text
expectation-blind case -> native engine call -> native result -> protocol-v3 result
```

It does not read expectations, score itself, repair ordering, choose the dialect
whose answer looks best, or implement missing recurrence behavior. Add one build
entry to `runners/registry/runner-builds.json` for every engine/configuration.
Dialect IDs are permanent corpus registry IDs; a bare `cron-compatible` claim is
invalid. One running process must never switch engine identity.

## Implementing protocol 3.0

On startup, write exactly one `hello` line and flush it. It contains:

```json
{"message":"hello","protocol_version":"3.0","runner":{"name":"example-runner","version":"3.0.0","provenance":"source pin"},"engine":{"name":"example-engine","version":"1.2.3","provenance":"tag/commit/hash"},"runtime":{"language":"Example","runtime":"ExampleVM","version":"4.5.6"},"capabilities":["cron.next"],"dialect_ids":["cron.vixie@1"],"semantic_profile_claims":{},"tzdb_provenance":{"source":"system zoneinfo","release_kind":"unknown"}}
```

Read one `case` line at a time. It is a typed, corpus-validated question projection
that contains `vector_id`, `family`, `operation`, `input`, and
`semantic_context`; it never contains expectations or authority metadata. Immediately
before the native engine operation, write and flush:

```json
{"message":"started","protocol_version":"3.0","request_id":"the-case-id"}
```

Then emit exactly one `result` with the same request ID. Preserve the native
occurrence sequence, including duplicates and non-monotonic output. Use
`rejection` only for a deliberate parser/validator rejection, `unsupported` only
for a capability boundary, and `engine_error` only for an unexpected native
exception safely caught by the adapter. Never translate an unexplained process
death. `accepted` is for validation-only success; `occurrences` may contain `[]`.
Warnings use the independent `warnings` array and do not redefine success.

Send all logging to stderr. Do not include duration, process identifiers,
timestamps, or absolute paths in protocol messages. Do not implement an internal
soft timeout as the authority: Rust owns hard containment after `started`.

For `cron.parse` and `rrule.parse`, successful validation is reported as
`accepted`; an adapter must not expose a subsequently generated occurrence
array as the result of a parse operation. Deliberate parser failures remain
`rejection`. This is especially important for syntax such as Jenkins `H`, where
a parser may internally choose a process-random hash value even though the only
observable question in a parse case is acceptance.

## Provenance and reproducibility

Pin the exact engine version/tag/commit, runner version, expected runtime, package
lock/source hashes, supported operations, dialect/profile claims, and tzdb
acquisition method. Dependency setup may be online, but a prepared runner must
execute offline. Exact tzdb release claims require exact evidence. A fingerprint
range remains `bounded`; absent proof remains `unknown`.

If an RC1 build cannot be restored, mark that registry entry `unreproducible` and
state precisely what provenance is missing. Do not substitute a newer engine or
dependency. The current Ruby entries demonstrate this rule because RC1 omitted
the concurrent-ruby commit.

## Verification

Run `cargo run -p xtask -- runner-contract` first. Then use `runner-smoke` with
the build ID and corpus protocol schema. The representative IDs are existing RC3
vectors selected in `runners/fixtures/representative-vectors.json`; this file is
adapter test configuration, not another corpus. Successful plumbing does not
assert engine quality, and a semantic disagreement is not an adapter defect
without evidence that translation changed native behavior.
