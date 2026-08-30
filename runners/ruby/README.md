# Ruby protocol-v2 runner

Use MRI Ruby 3.3.6. Restore the exact repositories in `setup.lock.json` into
`.adapter-deps`. Phase II did not record a concurrent-ruby commit, so that one
transitive dependency is explicitly unreproducible without recovering the
original checkout; no newer substitute is silently selected. System tzdb is
exact only when `tzdata.zi` proves it. Each process is fixed to one `--engine`.
