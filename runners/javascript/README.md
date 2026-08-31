# JavaScript protocol-v3 runner

The certification runtime is Bun 1.3.13. `npm ci` restores the exact package
artifacts and runtime from `package-lock.json`; Bun then executes offline from `node_modules`.
The declared engine commits are the Phase II RC1 provenance commits and the
package versions are unchanged.

Bun does not expose its tzdb release. The adapter therefore reports the
TZDB-001/002/003 fingerprint as bounded evidence, never as an exact release.
Each process is fixed to one `--engine` configuration.
