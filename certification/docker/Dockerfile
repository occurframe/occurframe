# syntax=docker/dockerfile:1.7
#
# Canonical Occurframe RC3 differential-certification environment.
#
# This image is the *only* sanctioned place the official RC3 evidence set is
# measured. It exists so that the certification profile's declared canonical
# platform (Ubuntu 24.04 x86_64) is a reproducible artifact rather than an
# assertion about somebody's laptop.
#
# Every runtime is pinned to the exact version the runner registry configures,
# and every engine dependency is restored from the exact commit or release the
# Phase II provenance recorded. Nothing is resolved to "latest".
#
# The image is built WITH network access. The certification run itself is
# expected to execute with `--network none`; see certification/README.md.

ARG BASE_IMAGE=ubuntu:24.04@sha256:33ceb71981b602c1a7443a53469e4dba065f7503eab3078a2d7a57a2ab987517
FROM ${BASE_IMAGE}
ARG BASE_IMAGE

# ---------------------------------------------------------------------------
# Pinned component identities. These mirror runners/registry/runner-builds.json
# and runners/*/{requirements.lock,setup.lock.json,package-lock.json,go.sum}.
# ---------------------------------------------------------------------------
ARG TZ_RELEASE=2026a
ARG TZ_DATAFORM=rearguard
ARG PYTHON_VERSION=3.11.15
ARG GO_VERSION=1.24.7
ARG BUN_VERSION=1.3.13
ARG PHP_VERSION=8.4.21
ARG RUBY_VERSION=3.3.6
ARG RUST_VERSION=1.95.0
ARG NODE_MAJOR=22

ENV DEBIAN_FRONTEND=noninteractive \
    LANG=C.UTF-8 \
    LC_ALL=C.UTF-8 \
    OCCURFRAME_PREFIX=/opt/occurframe \
    PATH=/opt/occurframe/bin:/usr/local/go/bin:/opt/bun/bin:/root/.cargo/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin

SHELL ["/bin/bash", "-o", "pipefail", "-c"]

# Persist the pinned identities so the runtime provenance probe can compare what
# was configured against what is actually installed, rather than trusting the
# image tag.
ENV OCCURFRAME_PIN_TZ_RELEASE=${TZ_RELEASE} \
    OCCURFRAME_PIN_TZ_DATAFORM=${TZ_DATAFORM} \
    OCCURFRAME_PIN_PYTHON=${PYTHON_VERSION} \
    OCCURFRAME_PIN_GO=${GO_VERSION} \
    OCCURFRAME_PIN_BUN=${BUN_VERSION} \
    OCCURFRAME_PIN_PHP=${PHP_VERSION} \
    OCCURFRAME_PIN_RUBY=${RUBY_VERSION} \
    OCCURFRAME_PIN_RUST=${RUST_VERSION} \
    OCCURFRAME_BASE_IMAGE=${BASE_IMAGE}

RUN mkdir -p "${OCCURFRAME_PREFIX}/bin" "${OCCURFRAME_PREFIX}/provenance" "${OCCURFRAME_PREFIX}/src"

# ---------------------------------------------------------------------------
# Toolchain prerequisites.
# ---------------------------------------------------------------------------
RUN apt-get update && apt-get install -y --no-install-recommends \
      autoconf bison build-essential ca-certificates curl file git jq \
      libbz2-dev libcurl4-openssl-dev libffi-dev libgdbm-dev libicu-dev \
      liblzma-dev libncurses-dev libonig-dev libreadline-dev libsqlite3-dev \
      libssl-dev libxml2-dev libxslt1-dev libyaml-dev libzip-dev make \
      patch pkg-config re2c unzip uuid-dev xz-utils zlib1g-dev zip \
 && rm -rf /var/lib/apt/lists/*

# ---------------------------------------------------------------------------
# Timezone database.
#
# The system zoneinfo tree is the tzdb provenance source for every `.system`
# Python build, for the Ruby runner, and for the Go runner. Ubuntu's packaged
# tzdata rolls forward, so pinning it to the distribution package would make the
# evidence set silently depend on when the image happened to be built. We build
# the IANA release directly instead, so `/usr/share/zoneinfo/tzdata.zi` reports
# a release this certification profile actually pins.
#
# DATAFORM=rearguard matches Debian/Ubuntu packaging, which is the shape the
# Phase II RC1 observations were produced against. Both the release and the
# dataform are recorded as observed provenance rather than assumed.
# ---------------------------------------------------------------------------
RUN set -euxo pipefail; \
    cd /tmp; \
    curl -fsSLO "https://data.iana.org/time-zones/releases/tzcode${TZ_RELEASE}.tar.gz"; \
    curl -fsSLO "https://data.iana.org/time-zones/releases/tzdata${TZ_RELEASE}.tar.gz"; \
    sha256sum "tzcode${TZ_RELEASE}.tar.gz" "tzdata${TZ_RELEASE}.tar.gz" \
      > "${OCCURFRAME_PREFIX}/provenance/tzdb-downloads.sha256"; \
    mkdir -p /tmp/tz && tar -C /tmp/tz -xzf "tzcode${TZ_RELEASE}.tar.gz" \
      && tar -C /tmp/tz -xzf "tzdata${TZ_RELEASE}.tar.gz"; \
    cd /tmp/tz; \
    make DATAFORM="${TZ_DATAFORM}" TOPDIR=/usr TZDIR=/usr/share/zoneinfo \
         REDO=posix_only install; \
    test -r /usr/share/zoneinfo/tzdata.zi; \
    head -2 /usr/share/zoneinfo/tzdata.zi; \
    grep -q "^# version ${TZ_RELEASE}\$" /usr/share/zoneinfo/tzdata.zi; \
    rm -rf /tmp/tz /tmp/tz*.tar.gz \
    && ln -sf /usr/share/zoneinfo/Etc/UTC /etc/localtime \
    && echo Etc/UTC > /etc/timezone

# ---------------------------------------------------------------------------
# CPython, pinned to the exact interpreter the registry configures.
# Ubuntu 24.04 ships 3.12, and the deadsnakes archive does not carry a
# guaranteed 3.11.15, so the interpreter is built from the release tarball.
# ---------------------------------------------------------------------------
RUN set -euxo pipefail; \
    cd /tmp; \
    curl -fsSLO "https://www.python.org/ftp/python/${PYTHON_VERSION}/Python-${PYTHON_VERSION}.tgz"; \
    sha256sum "Python-${PYTHON_VERSION}.tgz" >> "${OCCURFRAME_PREFIX}/provenance/downloads.sha256"; \
    tar -xzf "Python-${PYTHON_VERSION}.tgz"; \
    cd "Python-${PYTHON_VERSION}"; \
    ./configure --prefix="/opt/python-${PYTHON_VERSION}" --with-ensurepip=install \
                --enable-shared LDFLAGS="-Wl,-rpath,/opt/python-${PYTHON_VERSION}/lib"; \
    make -j"$(nproc)"; \
    make install; \
    "/opt/python-${PYTHON_VERSION}/bin/python3" --version | grep -Fx "Python ${PYTHON_VERSION}"; \
    ln -s "/opt/python-${PYTHON_VERSION}/bin/python3" "${OCCURFRAME_PREFIX}/bin/python3.11"; \
    cd /tmp && rm -rf "Python-${PYTHON_VERSION}" "Python-${PYTHON_VERSION}.tgz"

# ---------------------------------------------------------------------------
# Go toolchain (registry pins runtime.Version() == go1.24.7).
# ---------------------------------------------------------------------------
RUN set -euxo pipefail; \
    cd /tmp; \
    curl -fsSLO "https://go.dev/dl/go${GO_VERSION}.linux-amd64.tar.gz"; \
    sha256sum "go${GO_VERSION}.linux-amd64.tar.gz" >> "${OCCURFRAME_PREFIX}/provenance/downloads.sha256"; \
    tar -C /usr/local -xzf "go${GO_VERSION}.linux-amd64.tar.gz"; \
    /usr/local/go/bin/go version | grep -F "go${GO_VERSION}"; \
    rm -f "go${GO_VERSION}.linux-amd64.tar.gz"

# ---------------------------------------------------------------------------
# Bun (registry pins Bun.version == 1.3.13).
# ---------------------------------------------------------------------------
RUN set -euxo pipefail; \
    cd /tmp; \
    curl -fsSLO "https://github.com/oven-sh/bun/releases/download/bun-v${BUN_VERSION}/bun-linux-x64.zip"; \
    sha256sum bun-linux-x64.zip >> "${OCCURFRAME_PREFIX}/provenance/downloads.sha256"; \
    unzip -q bun-linux-x64.zip; \
    mkdir -p /opt/bun/bin; \
    mv bun-linux-x64/bun /opt/bun/bin/bun; \
    chmod +x /opt/bun/bin/bun; \
    /opt/bun/bin/bun --version | grep -Fx "${BUN_VERSION}"; \
    rm -rf bun-linux-x64 bun-linux-x64.zip

# ---------------------------------------------------------------------------
# Node.js is required only to run `npm ci` against the checked-in lockfile.
# It is never a measured runtime; the JavaScript engines run under Bun.
# ---------------------------------------------------------------------------
RUN set -euxo pipefail; \
    curl -fsSL "https://deb.nodesource.com/setup_${NODE_MAJOR}.x" -o /tmp/nodesource.sh; \
    bash /tmp/nodesource.sh; \
    apt-get install -y --no-install-recommends nodejs; \
    rm -rf /var/lib/apt/lists/* /tmp/nodesource.sh

# ---------------------------------------------------------------------------
# PHP, pinned exactly. The PHP runner reports PHP_VERSION and derives tzdb
# provenance from timezone_version_get(), i.e. the tzdb bundled in this build.
# ---------------------------------------------------------------------------
RUN set -euxo pipefail; \
    cd /tmp; \
    curl -fsSLO "https://www.php.net/distributions/php-${PHP_VERSION}.tar.xz"; \
    sha256sum "php-${PHP_VERSION}.tar.xz" >> "${OCCURFRAME_PREFIX}/provenance/downloads.sha256"; \
    tar -xJf "php-${PHP_VERSION}.tar.xz"; \
    cd "php-${PHP_VERSION}"; \
    ./configure --prefix="/opt/php-${PHP_VERSION}" --disable-cgi --disable-phpdbg \
                --enable-mbstring --enable-calendar --enable-intl --with-openssl \
                --with-zlib --with-curl --enable-bcmath --with-system-tzdata; \
    make -j"$(nproc)"; \
    make install; \
    ln -s "/opt/php-${PHP_VERSION}/bin/php" "${OCCURFRAME_PREFIX}/bin/php"; \
    php --version | head -1 | grep -F "PHP ${PHP_VERSION}"; \
    cd /tmp && rm -rf "php-${PHP_VERSION}" "php-${PHP_VERSION}.tar.xz"

# ---------------------------------------------------------------------------
# Ruby, pinned exactly. Both Ruby builds are provenance-blocked at the time of
# writing; the runtime is installed so the provenance investigation can be
# carried out against a real interpreter, and so that the two builds can be
# certified without an image change if exact provenance is ever recovered.
# ---------------------------------------------------------------------------
RUN set -euxo pipefail; \
    cd /tmp; \
    curl -fsSLO "https://cache.ruby-lang.org/pub/ruby/${RUBY_VERSION%.*}/ruby-${RUBY_VERSION}.tar.gz"; \
    sha256sum "ruby-${RUBY_VERSION}.tar.gz" >> "${OCCURFRAME_PREFIX}/provenance/downloads.sha256"; \
    tar -xzf "ruby-${RUBY_VERSION}.tar.gz"; \
    cd "ruby-${RUBY_VERSION}"; \
    ./configure --prefix="/opt/ruby-${RUBY_VERSION}" --disable-install-doc; \
    make -j"$(nproc)"; \
    make install; \
    ln -s "/opt/ruby-${RUBY_VERSION}/bin/ruby" "${OCCURFRAME_PREFIX}/bin/ruby"; \
    ruby --version | grep -F "ruby ${RUBY_VERSION}"; \
    cd /tmp && rm -rf "ruby-${RUBY_VERSION}" "ruby-${RUBY_VERSION}.tar.gz"

# ---------------------------------------------------------------------------
# Rust toolchain for the orchestration workspace.
# ---------------------------------------------------------------------------
RUN set -euxo pipefail; \
    curl -fsSL https://sh.rustup.rs -o /tmp/rustup-init.sh; \
    sh /tmp/rustup-init.sh -y --no-modify-path --profile minimal \
       --default-toolchain "${RUST_VERSION}" --component rustfmt --component clippy; \
    rustc --version | grep -F "${RUST_VERSION}"; \
    rm -f /tmp/rustup-init.sh
ENV CARGO_HOME=/root/.cargo \
    RUSTUP_HOME=/root/.rustup \
    CARGO_TARGET_DIR=/opt/occurframe/cargo-target

# ---------------------------------------------------------------------------
# Engine dependencies, restored from recorded provenance.
#
# These are staged outside the working tree so that the certification run can
# bind-mount a source tree from the host without the host's own (Windows)
# dependency directories shadowing them, and so the run needs no network.
# ---------------------------------------------------------------------------
COPY runners/python/requirements.lock ${OCCURFRAME_PREFIX}/src/requirements.lock
RUN set -euxo pipefail; \
    "/opt/python-${PYTHON_VERSION}/bin/python3" -m venv "${OCCURFRAME_PREFIX}/venv"; \
    "${OCCURFRAME_PREFIX}/venv/bin/python" -m pip --version \
        > "${OCCURFRAME_PREFIX}/provenance/pip-version.txt"; \
    "${OCCURFRAME_PREFIX}/venv/bin/python" -m pip install \
        -r "${OCCURFRAME_PREFIX}/src/requirements.lock"; \
    "${OCCURFRAME_PREFIX}/venv/bin/python" -m pip freeze \
        > "${OCCURFRAME_PREFIX}/provenance/python-freeze.txt"; \
    "${OCCURFRAME_PREFIX}/venv/bin/python" --version | grep -Fx "Python ${PYTHON_VERSION}"

COPY runners/javascript/package.json runners/javascript/package-lock.json ${OCCURFRAME_PREFIX}/js/
RUN set -euxo pipefail; \
    cd "${OCCURFRAME_PREFIX}/js"; \
    npm ci --ignore-scripts=false; \
    mkdir -p node_modules/bun/bin; \
    if [ ! -x node_modules/bun/bin/bun ]; then ln -sf /opt/bun/bin/bun node_modules/bun/bin/bun; fi; \
    node_modules/bun/bin/bun --version | grep -Fx "${BUN_VERSION}"; \
    npm ls --all --json > "${OCCURFRAME_PREFIX}/provenance/npm-tree.json" || true

COPY runners/php/setup.lock.json ${OCCURFRAME_PREFIX}/src/php-setup.lock.json
RUN set -euxo pipefail; \
    mkdir -p "${OCCURFRAME_PREFIX}/php-deps"; \
    jq -c '.repositories[]' "${OCCURFRAME_PREFIX}/src/php-setup.lock.json" | while read -r entry; do \
      name="$(jq -r '.directory | split("/") | last' <<<"$entry")"; \
      repo="$(jq -r '.repository' <<<"$entry")"; \
      commit="$(jq -r '.commit' <<<"$entry")"; \
      [ "$commit" = "null" ] && { echo "refusing unpinned PHP dependency $name" >&2; exit 1; }; \
      git clone --quiet "$repo" "${OCCURFRAME_PREFIX}/php-deps/$name"; \
      git -C "${OCCURFRAME_PREFIX}/php-deps/$name" checkout --quiet "$commit"; \
      git -C "${OCCURFRAME_PREFIX}/php-deps/$name" rev-parse HEAD; \
    done

COPY runners/ruby/setup.lock.json ${OCCURFRAME_PREFIX}/src/ruby-setup.lock.json
RUN set -euxo pipefail; \
    mkdir -p "${OCCURFRAME_PREFIX}/ruby-deps"; \
    jq -c '.repositories[]' "${OCCURFRAME_PREFIX}/src/ruby-setup.lock.json" | while read -r entry; do \
      name="$(jq -r '.directory | split("/") | last' <<<"$entry")"; \
      repo="$(jq -r '.repository' <<<"$entry")"; \
      commit="$(jq -r '.commit' <<<"$entry")"; \
      if [ "$commit" = "null" ]; then \
        echo "UNPINNED (left unprovisioned by design): $name" \
          >> "${OCCURFRAME_PREFIX}/provenance/ruby-unpinned.txt"; \
        continue; \
      fi; \
      git clone --quiet "$repo" "${OCCURFRAME_PREFIX}/ruby-deps/$name"; \
      git -C "${OCCURFRAME_PREFIX}/ruby-deps/$name" checkout --quiet "$commit"; \
    done; \
    touch "${OCCURFRAME_PREFIX}/provenance/ruby-unpinned.txt"

COPY runners/go/go.mod runners/go/go.sum ${OCCURFRAME_PREFIX}/gomod/
ENV GOPATH=/opt/occurframe/gopath \
    GOMODCACHE=/opt/occurframe/gopath/pkg/mod \
    GOFLAGS=-mod=mod
RUN set -euxo pipefail; \
    cd "${OCCURFRAME_PREFIX}/gomod"; \
    go mod download all; \
    go env GOMODCACHE

# Cargo dependencies are fetched while image construction still has network
# access. Certification itself runs with --network none and --offline.
COPY Cargo.toml Cargo.lock ${OCCURFRAME_PREFIX}/cargo-source/
COPY crates ${OCCURFRAME_PREFIX}/cargo-source/crates
COPY xtask ${OCCURFRAME_PREFIX}/cargo-source/xtask
RUN set -euxo pipefail; \
    cargo fetch --locked --manifest-path "${OCCURFRAME_PREFIX}/cargo-source/Cargo.toml"; \
    rm -rf "${OCCURFRAME_PREFIX}/cargo-source"

# Node's repository bootstrap performs a later apt transaction, which can
# reinstall Ubuntu's rolling tzdata package over the IANA tree prepared above.
# Reassert the pinned release as the final system-zoneinfo mutation and verify
# it mechanically. This ordering is part of the certification environment.
RUN set -euxo pipefail; \
    cd /tmp; \
    curl -fsSLO "https://data.iana.org/time-zones/releases/tzcode${TZ_RELEASE}.tar.gz"; \
    curl -fsSLO "https://data.iana.org/time-zones/releases/tzdata${TZ_RELEASE}.tar.gz"; \
    mkdir -p /tmp/tz-final; \
    tar -C /tmp/tz-final -xzf "tzcode${TZ_RELEASE}.tar.gz"; \
    tar -C /tmp/tz-final -xzf "tzdata${TZ_RELEASE}.tar.gz"; \
    cd /tmp/tz-final; \
    make DATAFORM="${TZ_DATAFORM}" TOPDIR=/usr TZDIR=/usr/share/zoneinfo \
         REDO=posix_only install; \
    grep -q "^# version ${TZ_RELEASE}\$" /usr/share/zoneinfo/tzdata.zi; \
    grep -q "^# dataform ${TZ_DATAFORM}\$" /usr/share/zoneinfo/tzdata.zi; \
    rm -rf /tmp/tz-final /tmp/tzcode*.tar.gz /tmp/tzdata*.tar.gz

# ---------------------------------------------------------------------------
# Runtime provenance probe and entrypoint.
# ---------------------------------------------------------------------------
COPY certification/docker/capture-environment.sh ${OCCURFRAME_PREFIX}/bin/capture-environment
COPY certification/docker/entrypoint.sh ${OCCURFRAME_PREFIX}/bin/entrypoint
RUN chmod +x "${OCCURFRAME_PREFIX}/bin/capture-environment" "${OCCURFRAME_PREFIX}/bin/entrypoint"

WORKDIR /work
ENTRYPOINT ["/opt/occurframe/bin/entrypoint"]
CMD ["bash"]
