# Conformance CLI

## `test`

Occurframe `0.1.0-rc1` implements one public operation:

```text
occurframe test --engine <configured-build-id>
oframe test --engine <configured-build-id>
```

The aliases call one shared `occurframe_cli::run()` implementation. A test loads and validates the pinned RC2 corpus, resolves one immutable engine build from the runner registry, validates the protocol `hello`, executes every selected vector, preserves normalized observations, uses `occurframe-conformance` for scoring, and renders the result. The CLI never parses or evaluates recurrence expressions.

## Options

```text
--engine <id>          exact stable runner-build ID (required)
--corpus <path>        authored checkout or packed corpus distribution
--family <family>      select a corpus family; repeatable
--tzdb <requirement>   any, exact:<release>, bounded, unknown, known, source:<name>
--format <format>      text, json, junit
--no-color             disable text color (NO_COLOR is also honored)
```

With no family selection, every vector is attempted. A family filter uses authored corpus family IDs; it never silently removes unsupported cells. The shipped distribution is discovered beside the executable. An explicit corpus must still match the locked RC2 version, canonical digest, and 184-vector population.

Explicit `--format` always wins. Otherwise a terminal gets concise text and redirected stdout gets one deterministic JSON object. Text starts with provenance and aggregate counts, then prints only actionable rows. JSON retains every verdict/outcome distinction and an Occurframe version. JUnit maps conformant/admissible cases to success, unsupported and open/unscored cases to skipped, semantic mismatch to failure, and engine error/timeout/runner failure to error. JSON and JUnit never contain color.

## Exit codes

```text
0  every scorable selected vector conforms; unsupported/open cells stay explicit
1  semantic non-conformance, engine error, or engine timeout
3  usage error, including missing/unknown engine or family
4  corpus, registry, runtime, identity, provenance, or runner infrastructure failure
```

Exit `4` takes precedence over `1` if both classes are present. Codes `2` (schedule rejection) and `5` (truncation) remain reserved for the frozen future surface and are never manufactured by `test`.

## Engine discovery and third-party adapters

From a source checkout, engine IDs come from `runners/registry/runner-builds.json`. Unknown IDs are rejected with the configured ID inventory; the CLI never guesses a version or dialect. A release contains this identity registry but deliberately excludes third-party runtimes.

A third-party maintainer uses the same NDJSON protocol 2.0 and registry model—no plugin loader, dynamic library, or language binding exists. Point the CLI at a prepared registry/root with `OCCURFRAME_RUNNER_REGISTRY` and `OCCURFRAME_RUNNER_ROOT`; optionally set `OCCURFRAME_RUNNER_PROTOCOL_SCHEMA` when the schema is not beside the corpus. The orchestrator verifies runner, engine, runtime, capabilities, dialect/profile claims, and tzdb provenance against `hello` before trusting results.

## Reserved commands

`explain`, `classify`, and `occurrences` are recognized only as reserved/not-yet-available names. They are not redirected, faked, or evaluator-backed. This prerelease does not claim the full frozen v1 command surface.
