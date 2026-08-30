#!/usr/bin/env bash
# Materialise a certification working tree inside the canonical image.
#
# The host source tree is bind-mounted read-only at /src (tooling) and
# /src-corpus (corpus authority). It is copied — not worked on in place — for
# two reasons:
#
#   1. the corpus is read-only authority and must be provably unmodified;
#   2. the host tree carries its own platform's dependency directories
#      (a Windows virtualenv, bun.exe, a Windows Go binary). Those must not
#      shadow the canonical Linux dependencies provisioned into this image.
#
# Engine dependencies are symlinked in from /opt/occurframe, so the tree the
# runners see is byte-identical in layout to a developer checkout while the
# actual dependencies remain the pinned ones baked into the image.
set -euo pipefail

PREFIX="${OCCURFRAME_PREFIX:-/opt/occurframe}"
SRC="${OCCURFRAME_SRC:-/src}"
CORPUS="${OCCURFRAME_CORPUS:-/src-corpus}"
WORK="${OCCURFRAME_WORK:-/work/occurframe}"

if [ ! -f "${SRC}/Cargo.toml" ]; then
  echo "entrypoint: ${SRC} does not look like the occurframe tooling repository" >&2
  exit 2
fi

if [ ! -e "${WORK}/.materialised" ] || [ "${OCCURFRAME_REMATERIALISE:-0}" = "1" ]; then
  rm -rf "${WORK}"
  mkdir -p "${WORK}"
  tar -C "${SRC}" \
      --exclude=./.git \
      --exclude=./target \
      --exclude=./dist \
      --exclude=./smoke-artifacts \
      --exclude=./research \
      --exclude=./runners/python/.venv \
      --exclude=./runners/javascript/node_modules \
      --exclude=./runners/php/.adapter-deps \
      --exclude=./runners/ruby/.adapter-deps \
      --exclude=./runners/go/bin \
      --exclude=./runners/go/.gopath \
      --exclude='./**/__pycache__' \
      -cf - . | tar -C "${WORK}" -xf -

  # Pinned dependencies, linked in rather than copied, so their provenance is
  # exactly what the image build recorded.
  mkdir -p "${WORK}/runners/go/bin"
  ln -sfn "${PREFIX}/venv"            "${WORK}/runners/python/.venv"
  ln -sfn "${PREFIX}/js/node_modules" "${WORK}/runners/javascript/node_modules"
  ln -sfn "${PREFIX}/php-deps"        "${WORK}/runners/php/.adapter-deps"
  ln -sfn "${PREFIX}/ruby-deps"       "${WORK}/runners/ruby/.adapter-deps"
  touch "${WORK}/.materialised"
fi

# The Go adapter is compiled from the working tree so that adapter source
# changes are honoured, but strictly offline from the module cache baked into
# the image. A certification run must never reach the network.
if [ ! -x "${WORK}/runners/go/bin/occurframe-go-runner" ] || [ "${OCCURFRAME_REBUILD_GO:-1}" = "1" ]; then
  ( cd "${WORK}/runners/go" \
    && GOPROXY=off GOFLAGS=-mod=readonly \
       go build -trimpath -o bin/occurframe-go-runner . )
fi

if [ -d /out ]; then mkdir -p /out; fi

export OCCURFRAME_WORK="${WORK}"
export OCCURFRAME_CORPUS="${CORPUS}"

cd "${WORK}"
exec "$@"
