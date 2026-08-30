#!/usr/bin/env bash
# Execute the complete RC2 evidence population twice inside one prepared image.
set -euo pipefail

WORK="${OCCURFRAME_WORK:-/work/occurframe}"
CORPUS="${OCCURFRAME_CORPUS:-/src-corpus}"
TOOLING_SHA="${OCCURFRAME_TOOLING_SHA:-$(git -C /src rev-parse HEAD)}"
OUT="${OCCURFRAME_OUTPUT:-/out}"

capture-environment > "${OUT}/environment.json"

certify() {
  local destination="$1"
  cargo run --locked --offline -p xtask -- differential-certify \
    --root "${WORK}" \
    --corpus "${CORPUS}" \
    --schema "${CORPUS}/schemas/runner-protocol-v2.schema.json" \
    --registry "${WORK}/runners/registry/runner-builds.json" \
    --profile "${WORK}/certification/profile.json" \
    --legacy-map "${WORK}/certification/phase2-build-map.json" \
    --legacy-matrix "${CORPUS}/legacy/phase2-rc1/matrix/matrix.json" \
    --environment "${OUT}/environment.json" \
    --tooling-sha "${TOOLING_SHA}" \
    --output "${destination}"
}

certify "${OUT}/rc2"
certify "${OUT}/rc2-rerun"

cargo run --locked --offline -p xtask -- differential-verify \
  --profile "${WORK}/certification/profile.json" \
  --first "${OUT}/rc2" \
  --second "${OUT}/rc2-rerun" \
  > "${OUT}/determinism.json"

cargo run --locked --offline -p xtask -- certification-verify \
  --directory "${OUT}/rc2"
