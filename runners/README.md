# Migrated Phase II runners

`runner-builds.json` defines 25 immutable Phase II configurations: 12 Python,
six JavaScript, three Go, two PHP, and two Ruby. Every process is launched for
one engine/configuration and must keep that identity for its lifetime.

The language READMEs and lock files provide restoration instructions. The
manual/scheduled adapter-certification workflow is the executable Ubuntu setup
path for all 23 reproducible configurations. The two Ruby configurations remain
migrated but are marked unreproducible because RC1 did not retain the required
concurrent-ruby commit. Recover that commit before enabling those builds; do not
select a newer dependency.

Installed dependencies and compiled binaries are ignored. A prepared adapter
must run without network access. The representative fixture file contains only
IDs from corpus `1.0.0-rc3`; it is test configuration and carries no semantic
authority.
