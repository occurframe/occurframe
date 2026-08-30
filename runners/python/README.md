# Python protocol-v2 runner

Use CPython 3.11.15 and install `requirements.lock` into `runners/python/.venv`.
Every process receives exactly one `--engine` and one `--tzdata` mode, so its
engine/configuration identity cannot change between cases. `system` uses host
zoneinfo and reports an exact release only when `tzdata.zi` proves it;
`vendored` uses the pinned `tzdata==2026.3` package (IANA 2026c).

The five engine versions and two croniter configurations are unchanged from
Phase II RC1. Dependency installation may use the network; execution does not.
