# Writing a runner

A **runner** is a small program you write that sits between Occurframe and your
recurrence engine. Occurframe never links your engine, loads a plugin, or binds
to your language: it launches a process and exchanges newline-delimited JSON with
it. That is the whole integration surface, and this document is enough to
implement it without reading any Rust.

`examples/minimal-runner/runner.py` is a complete working implementation in about
150 lines. It is a protocol example only — not a recurrence engine and not a
conformance reference — but it is the fastest way to see the traffic.

## The shape of a session

```text
Occurframe                                    your runner
     |                                             |
     |<-------------------- hello ----------------- |   once, at startup
     |                                             |
     | ------------------- case ------------------> |   one per vector
     |<------------------ started ----------------- |   acknowledgement
     |<------------------ result ------------------ |   exactly one terminal
     |                                             |
     | ------------------- case ------------------> |   ... repeated
```

Rules that are enforced, not merely expected:

- Every message is **one line** of UTF-8 JSON on stdout, flushed immediately.
- **stdout carries protocol traffic and nothing else.** Send logging, warnings
  and stack traces to stderr. A stray `print` on stdout is a protocol violation
  and fails the run.
- The runner writes `hello` **before reading anything**.
- For each `case` the runner writes exactly one `started`, then exactly one
  `result`, both echoing the same `request_id`.
- One process serves many cases. Keep reading stdin until EOF.
- Every inbound and outbound message is validated against
  `corpus/schemas/runner-protocol-v3.schema.json`, shipped in the release. A
  message that does not validate is an infrastructure failure.
- A protocol line above 8 MiB is refused.

## `hello`

Sent once, immediately, before reading stdin.

```json
{
  "message": "hello",
  "protocol_version": "3.0",
  "runner":  {"name": "my-runner", "version": "3.0.0", "provenance": "source:runners/mine.py"},
  "engine":  {"name": "my-engine", "version": "3.2.1", "provenance": "git myorg/my-engine@<sha>"},
  "runtime": {"language": "Python", "runtime": "CPython", "version": "3.11.9"},
  "capabilities": ["cron.next", "cron.parse"],
  "dialect_ids": ["cron.vixie@1"],
  "semantic_profile_claims": {"cron.start_inclusivity": "exclusive"},
  "tzdb_provenance": {"source": "python tzdata package", "release_kind": "exact", "release": "2026c"}
}
```

This is an identity assertion, and Occurframe checks it against the registry
entry that launched you. Every one of these must match exactly, **including list
order**, or the run fails as an environment failure with no observation recorded:

| `hello` field | Registry field |
| --- | --- |
| `protocol_version` | `protocol_version` |
| `runner` | `runner` |
| `engine` | `engine` |
| `runtime.language` / `runtime.runtime` / `runtime.version` | `language` / `runtime_name` / `runtime_requirement` |
| `capabilities` | `supported_operations` |
| `dialect_ids` | `dialect_ids` |
| `semantic_profile_claims` | `semantic_profile_claims` |
| `tzdb_provenance.source` | `tzdb_source` or `additional_tzdb_sources` |
| `tzdb_provenance.release_kind` | must appear in `allowed_tzdb_release_kinds` |

The runtime version is identity, not decoration. A configuration pinning CPython
3.11.15 is not satisfied by CPython 3.13.5. Recording an observed version without
enforcing it would let an unpinned interpreter produce evidence attributed to a
pinned one, so Occurframe treats a difference as a failure of the environment
rather than a fact about your engine.

### Declaring your semantics honestly

`dialect_ids` and `semantic_profile_claims` are how you tell Occurframe which
expectation to score you against. For a vector whose expectation is
`per_policy` or `per_dialect`, the scorer selects the single case whose `when`
clause your declared claims satisfy. Declare what your engine *does*, not what
you wish it did: declaring `cron.start_inclusivity: inclusive` while behaving
exclusively does not make you conformant, it makes you non-conformant against the
inclusive case. If your declared profile selects zero cases or more than one, the
vector is non-conformant with an `unresolved_declared_profile` diagnostic — the
scorer will not guess.

Registered dialect IDs and semantic axes live in the corpus, in
`registry/dialects.json` and `registry/semantic-axes.json`.

### Timezone-database provenance

`release_kind` is one of:

- `exact` — you know the release, for example `{"release_kind": "exact", "release": "2026c"}`.
  Use this only when the engine or its data package genuinely reports it.
- `bounded` — you know a range, with optional `min_inclusive` and
  `max_inclusive`. Typical for a runtime ICU whose data version you can bound but
  not pin.
- `unknown` — you cannot determine it. This is the honest answer for an arbitrary
  system zoneinfo, and it is respected rather than penalised.

An optional `fingerprint` may carry a stable identifier you derived yourself.
Claiming `exact` when you are guessing corrupts the evidence for everyone.

## `case`

Occurframe sends one per vector.

```json
{
  "message": "case",
  "protocol_version": "3.0",
  "request_id": "case-000001-CRON-ANCH-001",
  "vector_id": "CRON-ANCH-001",
  "family": "cron.anchoring",
  "operation": "cron.next",
  "input": {"kind":"cron","expr":"0 12 * * *","fields":5,"start":"2026-01-01T12:00:00","count":1,"inclusive":false,"zone":null},
  "semantic_context": {"dialect":"cron.vixie@1","policy":{},"requires":[],"tzdb_min":null,"tzdb_pin":null},
  "budget_ms": 8000
}
```

This is an expectation-blind projection, not the authored vector. The authority
retains the canonical vector and joins your observation to it after the runner
answers. The fields you act on:

- `operation` — what to do, for example `cron.next` or `cron.parse`. If it is not
  in your declared `capabilities`, answer `unsupported`.
- `input` — for cron: `expr`, `fields`, `start`, `count`, `inclusive`, `zone`.
- `semantic_context` — `dialect`, `policy`, `requires`, `tzdb_pin`, `tzdb_min`. If
  the context demands something you cannot honour, answer `unsupported` rather than
  approximating.
- `vector_id` and `family` — opaque diagnostic correlation only.

The case type has no field capable of carrying expectations, admissible answer
sets, classification, normative evidence, rationale, tags, or scoring policy.
Protocol 2.0 exposed the full vector and is intentionally not wire-compatible.

Occurrence values are civil-time strings and must be returned in the engine's
own order, unsorted and undeduplicated. Order and duplicates are part of the
observed behaviour.

## `started`

Emit this as soon as you have read the case and before you call the engine.

```json
{"message": "started", "protocol_version": "3.0", "request_id": "case-000001-CRON-ANCH-001"}
```

It is the attributable acknowledgement: until it arrives, a slow or hung process
is charged to the infrastructure watchdog (30 s by default, covering startup,
`hello` and the pre-`started` window). After it arrives, the per-case engine
budget of `budget_ms` (8,000 ms in certification) applies, and exceeding it is a
`timeout` recorded against the engine rather than an infrastructure failure.

## `result`

Exactly one per case, after `started`. `warnings` is required; send `[]` when
there are none.

```json
{
  "message": "result",
  "protocol_version": "3.0",
  "request_id": "case-000001-CRON-ANCH-001",
  "outcome": {"type": "occurrences", "occurrences": ["2026-01-02T12:00:00"]},
  "warnings": []
}
```

There are exactly five terminal outcomes, and choosing between them correctly is
the single most important thing a runner author does.

| Outcome | Meaning | Use when |
| --- | --- | --- |
| `occurrences` | The engine produced this sequence. | The operation returned occurrences. Preserve order and duplicates exactly. An empty list is a real answer, not an error. |
| `accepted` | The engine accepted the input but yields no occurrence list. | A `parse`-style operation succeeded and there is nothing further to report. |
| `rejection` | The engine deliberately refused the input. | Your engine raised its own documented "this expression is invalid" error. This is the *correct* answer for an invalid vector. |
| `unsupported` | The engine cannot express this case at all. | The operation, dialect, extension or context is outside what your engine implements. |
| `engine_error` | The engine failed unexpectedly. | An internal crash, an assertion, or a contract violation — a bug, not a refusal. |

`rejection` and `unsupported` are not interchangeable. An invalid-input vector
expects `rejection`; answering `unsupported` there is scored as unsupported and
tells nobody anything. Conversely, reporting `rejection` for a feature you simply
have not implemented misrepresents a gap as a deliberate validation decision.
`engine_error` is never a way to avoid deciding: if your engine legitimately
refuses input, that is `rejection`.

`rejection`, `unsupported` and `engine_error` each carry a `diagnostic`:

```json
{"code": "field_value_out_of_range", "message": "minute 60 is outside 0-59", "details": null}
```

`warnings` are orthogonal to the outcome. A warning never changes a verdict; use
it to record something a reader should know without altering what the engine
answered.

## Registry entry

Occurframe learns about your build from a registry file. Point at it with
`OCCURFRAME_RUNNER_REGISTRY`; a full worked example is
`examples/minimal-runner/runner-builds.example.json`.

```json
{
  "schema_version": "1.0.0",
  "builds": [
    {
      "build_id": "myorg.my-engine.pinned",
      "protocol_version": "3.0",
      "runner":  {"name": "my-runner", "version": "3.0.0", "provenance": "source:runners/mine.py"},
      "engine":  {"name": "my-engine", "version": "3.2.1", "provenance": "git myorg/my-engine@<sha>"},
      "language": "Python",
      "runtime_name": "CPython",
      "runtime_requirement": "3.11.9",
      "launch": {
        "program": ".venv/bin/python",
        "arguments": ["mine.py"],
        "working_directory": ".",
        "environment": {}
      },
      "supported_operations": ["cron.next", "cron.parse"],
      "dialect_ids": ["cron.vixie@1"],
      "semantic_profile_claims": {"cron.start_inclusivity": "exclusive"},
      "tzdb_provenance_acquisition": "tzdata.IANA_VERSION from the pinned tzdata package",
      "tzdb_source": "python tzdata package",
      "allowed_tzdb_release_kinds": ["exact"],
      "fallback_tzdb_provenance": {"source": "python tzdata package", "release_kind": "exact", "release": "2026c"},
      "representative_vectors": ["CRON-ANCH-001", "CRON-INV-001"],
      "reproducibility": {"status": "reproducible", "setup": "CPython 3.11.9 virtualenv plus requirements.lock"},
      "legacy_source": "not applicable: new integration"
    }
  ]
}
```

Notes that save time:

- `build_id` is how you name the build on the command line, and it identifies one
  immutable engine *and configuration*. A different configuration that changes
  semantic claims is a different build ID, not a flag.
- `launch.program` with more than one path component and no leading root is
  resolved against the runner root; a bare name such as `python3` is resolved
  from the authority process's `PATH` before launch. The runner itself receives
  no inherited `PATH`.
- The runner root defaults to the registry file's own directory, so a registry can
  live anywhere. Override with `OCCURFRAME_RUNNER_ROOT`.
- `working_directory` is resolved against the runner root and becomes the
  process's working directory.
- The child environment is cleared. Occurframe grants fixed `TZ=UTC`, `LANG=C`,
  `LC_ALL=C`, the documented platform minimum, and then exactly the variables
  under `launch.environment`. Declare every host dependency explicitly; secret
  and CI variables are not inherited.
- `fallback_tzdb_provenance` is used only to give a deterministic identity to a
  process that died before it could send a trustworthy `hello`.
- `representative_vectors` is the smoke subset for the build and must be
  non-empty.
- `reproducibility.status` may be `unreproducible`, and then
  `reason_code: "unreproducible_provenance"` and a `reason` are required. Such a
  build is configured but refused at run time, because evidence that cannot be
  reproduced is not evidence. Two Ruby builds in the certified population are in
  exactly this state.

## Checking your work

```sh
export OCCURFRAME_RUNNER_REGISTRY=/path/to/your/runner-builds.json
occurframe test --engine myorg.my-engine.pinned --family cron.anchoring --format json
```

Failure modes and what they mean:

- **Exit 3** — usage. An unknown `build_id` prints the configured IDs.
- **Exit 4 with an identity message** — your `hello` disagrees with the registry.
  Compare field by field, including list order.
- **Exit 4 with `startup_failure`** — the process could not be launched; check
  `launch.program` and the runner root.
- **Exit 4 with `missing_started` or `invalid_terminal_message`** — you emitted
  something other than exactly one `started` then one `result`, or wrote
  non-protocol output to stdout.
- **Exit 4 with `hello_failure`** — nothing valid arrived before the
  infrastructure watchdog; are you flushing stdout?
- **Exit 1** — the protocol worked and your engine disagreed with the corpus.
  That is a real result. Read [Interpreting results](INTERPRETING-RESULTS.md)
  before concluding it is a bug.

A failed or timed-out process is always discarded and a fresh one launched for
the next case, so a single bad case cannot corrupt later ones.

## What Occurframe will never do

There is no plugin loader, no dynamic library interface, no language binding and
no in-process embedding, and none is planned. The process boundary is deliberate:
it is what makes an observation attributable to a named engine build, on a named
runtime, with named provenance — and what keeps Occurframe from having any
opinion about how your engine computes anything.
