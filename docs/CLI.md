# Conformance CLI

## `test`

Occurframe `0.1.0-rc3` implements one public operation:

```text
occurframe test --engine <configured-build-id>
oframe    test --engine <configured-build-id>
```

The two executable names are aliases for one shared `occurframe_cli::run()`
implementation. Options, output bytes and exit codes are identical; nothing
depends on which name was invoked.

A test loads and validates the pinned RC2 corpus, resolves one immutable engine
build from the runner registry, validates the protocol `hello` against that
build's configured identity, executes every selected vector, preserves the
normalized observations, delegates scoring to `occurframe-conformance`, and
renders the result. The CLI never parses or evaluates a recurrence expression.

## Options

```text
--engine <id>          exact stable runner-build ID (required)
--corpus <path>        authored checkout or packed corpus distribution
--family <family>      select a corpus family; repeatable
--tzdb <requirement>   any, exact:<release>, bounded, unknown, known, source:<name>
--format <format>      text, json, junit
--no-color             accepted for forward compatibility (see note below)
-h, --help             print help
```

With no family selection every vector is attempted. `--family` takes authored
corpus family IDs and never silently removes an unsupported cell: a case the
engine cannot express is reported as unsupported, not omitted. An unknown family
is a usage error and the message lists the available families.

`--tzdb` turns a provenance expectation into an enforced precondition. If the
observed provenance does not satisfy it the run fails as an environment failure
(exit `4`) rather than producing evidence you cannot reproduce.

An explicit `--corpus` must still match the locked RC2 version, canonical digest
and 184-vector population. The lock is compiled into the binary, so pointing at a
different corpus is a hard failure, not a silent change of meaning.

**Note on `--no-color`.** The current text renderer emits no ANSI colour at all,
so `--no-color` and the `NO_COLOR` environment variable are accepted and have no
observable effect. They are honoured now so that a future coloured renderer
cannot break existing invocations. JSON and JUnit are never coloured.

## Output formats

An explicit `--format` always wins. Otherwise a terminal gets concise text and
redirected stdout gets one deterministic JSON object.

- **text** — provenance and aggregate counts first, then only actionable rows.
- **json** — a single object with canonical key ordering. Identical inputs
  produce byte-identical output, so it diffs cleanly and can be committed as a
  baseline. It retains every verdict and outcome distinction, per-vector
  classification, execution status, native engine outcome, matched policy or
  dialect case, warnings, and full corpus and engine identity.
- **junit** — conformant and admissible map to success, unsupported and
  open/unscored to skipped, semantic mismatch to failure, and engine error,
  timeout and runner failure to error.

## Exit codes

```text
0  every scorable selected vector conforms; unsupported/open cells stay explicit
1  semantic non-conformance, engine error, or engine timeout
3  usage error, including an unknown command, option, engine ID or family
4  corpus, registry, runtime, identity, provenance, or runner infrastructure failure
```

These four are the active v1 contract and are never renumbered. Exit `4` takes
precedence over `1` when both are present: a run whose evidence is untrustworthy
is not a statement about the engine.

Codes `2` (schedule rejection) and `5` (truncation) stay frozen in the
specification and inactive here. Both presuppose evaluating a caller's own
schedule, which only the deferred engine-gated commands would do, so `test` can
never produce them. They are reserved rather than reused.

## Discovery

Nothing is resolved relative to the current working directory. An extracted
release works from any directory, and a registry may live anywhere on the
filesystem.

**Corpus**, first match wins:

1. `--corpus <path>`
2. `OCCURFRAME_CORPUS`
3. `<directory containing the executable>/../corpus` — the bundled corpus of an
   installed or extracted release. The executable path is canonicalized first, so
   a symlink from `/usr/local/bin` still resolves to the real bundle.
4. `./corpus`, then `../corpus` — source-checkout convenience only.

**Runner registry**, first match wins:

1. `OCCURFRAME_RUNNER_REGISTRY`
2. `<executable directory>/../adapters/runner-builds.json` — the release's
   adapter identity registry.
3. `./runners/registry/runner-builds.json` — source-checkout convenience only.

**Runner root** — the base that relative `launch.program`, `launch.arguments` and
`launch.working_directory` entries are resolved against:

1. `OCCURFRAME_RUNNER_ROOT`, which may point anywhere, inside or outside the
   release.
2. `<root>/runners/registry/runner-builds.json` → `<root>` (source checkout).
3. `<bundle>/adapters/runner-builds.json` → `<bundle>` (extracted release).
4. Otherwise, the directory containing the registry file. This is what makes a
   third-party registry portable: put the registry beside your runner and its
   relative launch paths work wherever the directory is moved.

**Protocol schema**, first match wins:

1. `OCCURFRAME_RUNNER_PROTOCOL_SCHEMA`
2. `<corpus>/schemas/runner-protocol-v3.schema.json`
3. `<bundle>/corpus/schemas/runner-protocol-v3.schema.json`
4. `<runner root>/../corpus/schemas/runner-protocol-v3.schema.json`

## Environment variables

| Variable | Effect |
| --- | --- |
| `OCCURFRAME_CORPUS` | Corpus checkout or packed distribution to use. |
| `OCCURFRAME_RUNNER_REGISTRY` | Protocol-v3 registry file describing engine builds. |
| `OCCURFRAME_RUNNER_ROOT` | Base directory for relative launch paths. |
| `OCCURFRAME_RUNNER_PROTOCOL_SCHEMA` | Protocol schema, when not beside the corpus. |
| `NO_COLOR` | Accepted; currently has no observable effect (see above). |

## Engine discovery and third-party adapters

From a source checkout, engine IDs come from
`runners/registry/runner-builds.json`. A release contains the same identity
registry under `adapters/`, but deliberately excludes third-party engine
runtimes: the release records what an engine build *is*, not a copy of it.

Unknown IDs are rejected with the configured ID inventory; the CLI never guesses
a version or a dialect. A build whose registry entry is marked
`unreproducible_provenance` is configured but refused at run time, with the
recorded reason, because evidence that cannot be reproduced is not evidence.

A third-party maintainer uses the same NDJSON protocol `2.0` and registry model.
There is no plugin loader, dynamic library or language binding. Point the CLI at
a prepared registry with `OCCURFRAME_RUNNER_REGISTRY`, optionally override the
base with `OCCURFRAME_RUNNER_ROOT`, and see
[Writing a runner](WRITING-A-RUNNER.md). A complete worked example ships in
`examples/minimal-runner/`.

## Deferred commands — engine-gated, not part of v1

Occurframe v1 ships one semantic command. `explain`, `classify` and
`occurrences` are **not** part of the command tree: they are not recognized,
not reserved-with-a-message, and not present in `--help`. Typing one produces the
same ordinary usage error as any other unknown word, because a command that
cannot exist should not be advertised as if it merely had not arrived yet.

Research II froze all four names. Three of them require Occurframe to compute
occurrences rather than observe them — `occurrences` emits them outright,
`classify` needs a parser and evaluator per cron dialect, and `explain` must
decide what a schedule denotes and which policy axes are reachable — and the
ORACLE ONLY verdict does not authorise a production recurrence engine in any
language. The verdict governs the interface text derived from it. The full
reasoning is
[ERRATA-001](https://github.com/occurframe/corpus/blob/dev/spec/ERRATA.md) in the
corpus; their frozen semantics are preserved unchanged in the specification's
§6.7 so the engine gate can be walked without reopening research.

They were not reimplemented as corpus or report inspection commands to keep their
names, and nothing delegates to `cron_ref.py`, to one incumbent engine, or to an
arbitrary adapter to imitate them.

The conceptual scheduling API in the specification is likewise **specification
only**: it describes what a conforming implementation exposes, and Occurframe v1
does not ship it as a library. Three things are kept distinct throughout this
documentation — a *specified operation*, the *implemented oracle tooling*, and a
*future engine implementation*.
