# Runner architecture

## Boundary

`occurframe-runner` is a Rust process supervisor, not a recurrence engine. It
accepts vectors only after `occurframe-conformance` has validated the RC2 corpus,
launches one immutable external engine/configuration, normalizes the observation,
and calls the scorer owned by `occurframe-conformance`. It never parses cron or
RRULE, resolves civil time, chooses an expectation, or changes occurrence order.

The corpus owns the protocol schema. The orchestrator validates every inbound
non-empty stdout line against that schema and then against the Rust wire model.
The checked-in runner registry additionally pins runner identity, engine identity
and provenance, language/runtime name, capabilities, dialect IDs, semantic
profile claims, and allowable tzdb provenance form. A mismatched `hello` stops
that process as `runner_failure`.

## Lifecycle and transport

The transport is UTF-8 NDJSON over stdin/stdout:

```text
runner -> hello
Rust   -> case (validated vector, deterministic request ID, budget_ms=8000)
runner -> started
runner -> result
```

Stdout is protocol-only. Empty lines are ignored; every non-empty line must be
one valid protocol message. Logs belong on stderr. Rust retains only a bounded
64 KiB stderr tail as non-semantic execution metadata. That tail, process IDs,
request IDs, timestamps, machine paths, and timing measurements never enter a
normalized observation or semantic digest.

The 30-second infrastructure watchdog covers process startup, `hello`, case
delivery, and the wait for `started`. Expiry or process death in that interval is
`runner_failure`. The official 8,000 ms engine budget begins only after an
attributable `started`. Expiry then is the observed `timeout`; Rust kills the
whole process and makes no mathematical non-termination claim.

Malformed messages, stdout contamination, premature exit, an absent or wrong
request ID, and identity mismatch are infrastructure failures. An unexplained
exit after `started` is still `runner_failure`: the supervisor cannot infer an
engine cause. A native exception is `engine_error` only when the adapter catches
and safely emits that outcome.

After timeout or any runner failure, the process is terminated and discarded.
If independent cases remain, the next case starts a fresh process and repeats
`hello`. A successful result keeps the process for sequential cases. Exactly one
terminal engine outcome is allowed: `occurrences` (including `[]`), `accepted`,
`rejection`, `unsupported`, or `engine_error`. Warnings are orthogonal.

## Deterministic observations

Batch output is sorted by vector ID and configured build ID. JSON object keys are
canonical, records are LF-terminated, and occurrence arrays are copied without
sorting or deduplication. Transport metadata is kept separate. Scoring is a call
into `occurframe-conformance`; `unsupported`, timeout, and infrastructure failure
remain separate from engine disagreement.

## Certification boundary

Fast CI uses the deterministic Rust fake runner to inject every protocol and
process failure class. Manual/scheduled adapter certification restores pinned
real dependencies and runs the small RC2 fixture selection. Neither mode runs
the complete 184 × engine surface, publishes comparisons, ranks engines, or
creates an official differential matrix.
