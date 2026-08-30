# Occurframe 0.1.0-rc1

Occurframe is an executable conformance oracle for recurring-time semantics. It is not a scheduling engine.

Run a prepared protocol-v2 adapter against the bundled RC2 corpus:

```text
occurframe test --engine <configured-build-id>
oframe test --engine <configured-build-id>
```

The two executable names are aliases for one implementation. The standard corpus in `corpus/` is version `1.0.0-rc2`; its source authority is the separate `occurframe/corpus` repository. The adapter registry is included for identity discovery, but third-party engine runtimes are deliberately not bundled. Use a prepared adapter checkout through `OCCURFRAME_RUNNER_ROOT`, or provide a protocol-v2 registry through `OCCURFRAME_RUNNER_REGISTRY`.

`reports/differential-report.md` explains the candidate public evidence. `reports/matrix.json` retains the cell-level derived matrix. `certification/` records the immutable certification identity and observed environment.

Only `test` is implemented. `explain`, `classify`, and `occurrences` remain reserved because their frozen semantics conflict with the no-engine boundary. This is a prerelease, not stable v1.
