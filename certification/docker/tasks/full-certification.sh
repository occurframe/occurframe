#!/usr/bin/env bash
# Execute the complete RC3 evidence population twice inside one prepared image.
set -euo pipefail

WORK="${OCCURFRAME_WORK:-/work/occurframe}"
CORPUS="${OCCURFRAME_CORPUS:-/src-corpus}"
TOOLING_SHA="${OCCURFRAME_TOOLING_SHA:?trusted tooling source attestation is required}"
OUT="${OCCURFRAME_OUTPUT:-/out}"

capture-environment > "${OUT}/environment.json"

certify() {
  local destination="$1"
  cargo run --locked --offline -p xtask -- differential-certify \
    --root "${WORK}" \
    --corpus "${CORPUS}" \
    --schema "${CORPUS}/schemas/runner-protocol-v3.schema.json" \
    --registry "${WORK}/runners/registry/runner-builds.json" \
    --profile "${WORK}/certification/profile-rc3.json" \
    --legacy-map "${WORK}/certification/phase2-build-map.json" \
    --legacy-matrix "${CORPUS}/legacy/phase2-rc1/matrix/matrix.json" \
    --environment "${OUT}/environment.json" \
    --tooling-attested-source-revision "${TOOLING_SHA}" \
    --output "${destination}"
}

certify "${OUT}/rc3"
certify "${OUT}/rc3-rerun"

cargo run --locked --offline -p xtask -- differential-verify \
  --profile "${WORK}/certification/profile-rc3.json" \
  --first "${OUT}/rc3" \
  --second "${OUT}/rc3-rerun" \
  > "${OUT}/determinism.json"

cargo run --locked --offline -p xtask -- certification-verify \
  --directory "${OUT}/rc3"
