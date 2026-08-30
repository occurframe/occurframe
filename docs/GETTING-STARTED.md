# Getting started

This walks from a downloaded archive to a JUnit report in CI. You need no Rust,
no Cargo, no Git and no Occurframe checkout — only the release archive and,
for the worked example below, a Python 3 interpreter.

Occurframe is a conformance oracle. It observes *your* engine; it computes
nothing itself. So the one piece you must supply is a **runner**: a small program
that speaks Occurframe's protocol on one side and calls your engine on the other.
The release ships a working example you can run before writing your own.

## 1. Verify the archive

Never skip this. The release publishes a checksum beside the archive, and every
file inside the archive is covered by its own `SHA256SUMS`.

```sh
# The digest published beside the archive.
sha256sum -c occurframe-0.1.0-rc2.tar.gz.sha256

tar -xzf occurframe-0.1.0-rc2.tar.gz
cd occurframe-0.1.0-rc2

# Every packaged file, at the digest the release recorded for it.
sha256sum -c SHA256SUMS
```

On macOS use `shasum -a 256 -c`. On Windows PowerShell:

```powershell
Get-FileHash occurframe-0.1.0-rc2.tar.gz -Algorithm SHA256
tar -xzf occurframe-0.1.0-rc2.tar.gz
```

`release-manifest.json` records what this artifact is: tooling version, the exact
commit and Rust toolchain that built it, every binary with its target triple and
digest, corpus identity, runner protocol version, and the certification identity
and evidence digests of the differential run it republishes. No build timestamp
is recorded, so two assemblies of the same inputs are identical.

## 2. Run the binary

The release contains both aliases for four targets. Pick the one for your
machine; they are the same program under two names.

```text
bin/occurframe-x86_64-unknown-linux-gnu   bin/oframe-x86_64-unknown-linux-gnu
bin/occurframe-x86_64-pc-windows-msvc.exe bin/oframe-x86_64-pc-windows-msvc.exe
bin/occurframe-aarch64-apple-darwin       bin/oframe-aarch64-apple-darwin
bin/occurframe-x86_64-apple-darwin        bin/oframe-x86_64-apple-darwin
```

```sh
./bin/occurframe-x86_64-unknown-linux-gnu --version
./bin/occurframe-x86_64-unknown-linux-gnu --help
```

Archive transports often drop the executable bit; `chmod +x bin/*` if needed. You
may rename the binary to `occurframe` (or `oframe`) and put it on `PATH`, or
symlink to it — Occurframe resolves its bundled corpus from the real location of
the executable, so a symlink from `/usr/local/bin` works. Move the whole
directory anywhere you like; nothing resolves relative to your working
directory.

## 3. Check the bundled corpus

The release carries the corpus it was certified against, so a test run needs no
network access.

```sh
cat VERSION
```

```text
Occurframe tooling: 0.1.0-rc2
Specification:      1.0.0-rc1
Corpus:             1.0.0-rc2
Runner protocol:    2.0
Certification:      rc2.1
Commit:             <exact released commit>
```

`--version` reports the same identities, because a conformance result cannot be
reproduced without knowing the specification it was scored against, the corpus it
came from and the protocol it was gathered over. All four version independently.

`corpus/manifest.json` records the corpus version, the canonical digest of the
authored vectors, and a per-file digest:

```sh
grep -o '"corpus_version": "[^"]*"' corpus/manifest.json
grep -o '"canonical_corpus_digest": "[^"]*"' corpus/manifest.json
```

Corpus `1.0.0-rc2` has canonical digest
`4804772d20fb36c7329b2c5f2f28e264d9bc00b11e407e76d9836fc38cd80470` and 184
vectors. Occurframe re-derives that digest from the vectors it actually loads and
refuses to run if it does not match, so a tampered or mismatched corpus is a
hard failure rather than a quiet difference in results.

## 4. Configure a runner

A runner is any executable that speaks the protocol on stdin/stdout. It is
described to Occurframe by a **registry** entry that pins how to launch it and
exactly what identity it must report.

Start with the bundled example, which needs only `python3`:

```sh
export OCCURFRAME_RUNNER_REGISTRY="$PWD/examples/minimal-runner/runner-builds.example.json"
```

Relative launch paths in a registry are resolved against the registry's own
directory, so the registry may live anywhere — inside the release, beside your
engine, or in a scratch directory. Set `OCCURFRAME_RUNNER_ROOT` only when you
want them resolved against some other base.

The example is a **protocol example only — not a recurrence engine and not a
conformance reference**. It replays two fixed answers so you can watch the
handshake work. When you are ready to test something real, read
[Writing a runner](WRITING-A-RUNNER.md).

## 5. Run `test`

```sh
./bin/occurframe-x86_64-unknown-linux-gnu test \
    --engine example.minimal \
    --family cron.anchoring --family cron.invalid
```

```text
Occurframe conformance result
engine: occurframe-example-fixture
engine version: 0.0.0-example
corpus version: 1.0.0-rc2
corpus digest: 4804772d20fb36c7329b2c5f2f28e264d9bc00b11e407e76d9836fc38cd80470
...
selected vectors: 16
conformant: 2
unsupported: 14
```

Drop `--family` to attempt every vector. `--family` selects authored corpus
family IDs and never silently drops a cell: a vector your engine cannot handle
is reported as unsupported, not omitted.

Exit codes are the machine-readable summary:

```text
0  every scorable selected vector conformed
1  semantic non-conformance, engine error, or engine timeout
3  usage error (unknown command, option, engine ID or family)
4  environment failure: corpus, registry, identity/provenance, or runner infrastructure
```

Exit `4` takes precedence over `1`. That distinction matters: `4` means the run
did not produce trustworthy evidence, so it is not a statement about your engine
at all.

`test` is the whole semantic surface of Occurframe v1. `explain`, `classify` and
`occurrences` appear in the specification but are deferred behind the engine
gate — each needs Occurframe to compute occurrences, which it does not do — so
typing one gives an ordinary unknown-command usage error. See
[CLI](CLI.md#deferred-commands--engine-gated-not-part-of-v1).

## 6. Consume JSON or JUnit in CI

Text is the default on a terminal and JSON when stdout is redirected; an explicit
`--format` always wins.

```sh
occurframe test --engine my.engine --format json  > occurframe.json
occurframe test --engine my.engine --format junit > occurframe-junit.xml
```

JSON is one deterministic object with canonical key ordering — identical inputs
give byte-identical output, so it diffs cleanly between runs and can be committed
as a baseline. It keeps every distinction: per-vector classification, execution
status, native engine outcome, verdict, matched policy or dialect case, and
warnings, plus the full engine and corpus identity.

JUnit maps onto what ordinary CI reporting understands:

| Occurframe result | JUnit |
| --- | --- |
| conformant, conformant admissible | success |
| non-conformant | failure |
| unsupported, open/unscored | skipped |
| engine error, timeout, runner failure | error |

A GitHub Actions step:

```yaml
- name: Occurframe conformance
  run: |
    ./bin/occurframe-x86_64-unknown-linux-gnu test \
      --engine my.engine --format junit > occurframe-junit.xml
- uses: actions/upload-artifact@v4
  if: always()
  with:
    name: occurframe-junit
    path: occurframe-junit.xml
```

Because `error` and `failure` are distinct, an infrastructure problem in your CI
image shows up as an error rather than as a false report that your engine
regressed. Treat a nonzero exit `4` as "fix the environment", not "fix the
engine".

## Where to go next

- [Writing a runner](WRITING-A-RUNNER.md) — the protocol, in full.
- [Interpreting results](INTERPRETING-RESULTS.md) — what a verdict means, and
  what it does not license you to claim.
- [CLI](CLI.md) — every option, discovery rule and environment variable.
