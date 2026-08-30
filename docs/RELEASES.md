# Releases

## `0.1.0-rc1`

This is the first public tooling/CLI release candidate. Its compatible corpus remains separately versioned as `1.0.0-rc2`; runner protocol remains `2.0`; the evidence profile is `rc2.1`. The release does not relabel the corpus as stable and does not imply that all four reserved commands are implemented.

The release bundle contains both executable aliases for Windows x86_64, Linux x86_64, macOS arm64, and macOS x86_64; the verified generated RC2 corpus distribution; the candidate public differential report and matrix; the certification manifest and environment; licenses; release metadata; and a SHA-256 manifest. Engine runtimes are separate prepared artifacts.

`release/evidence-lock.json` pins corpus and certification digests plus the exact required binary inventory. `xtask release-package` refuses a missing binary, changed corpus, changed certified matrix, bad certification checksum, or existing output directory. The GitHub workflow builds and smokes each native binary, assembles twice, checks byte-identical `SHA256SUMS`, and uploads the candidate artifact. It does not publish a GitHub Release or crates.io package.

Generated bundles belong under `dist/release/` or CI artifact storage and are not routine source-controlled content.
