# PHP protocol-v3 runner

Use PHP 8.4.21. Restore the two repositories in `setup.lock.json` at the exact
commits into `.adapter-deps`; both are loaded directly without Composer at
runtime. Dependency restoration may use the network, while runner execution is
offline. A process is fixed to one `--engine`. System tzdb is exact only when
`tzdata.zi` proves it; PHP-bundled tzdb uses `timezone_version_get()`.
