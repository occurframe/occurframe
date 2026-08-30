# Go protocol-v2 runner

Build with Go 1.24.7 after `go mod download`, then execute the resulting binary
offline. `go.mod` pins robfig/cron v3.0.1 and rrule-go v1.8.2; their Phase II
provenance commits are declared in `hello`. Each process receives exactly one
`--engine` configuration. Host tzdb is exact only when `tzdata.zi` proves it.
