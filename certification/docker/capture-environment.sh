#!/usr/bin/env bash
# Probe the *actual* environment this certification is executing in.
#
# The certification profile names a canonical platform, but naming a platform
# does not guarantee a future tzdb or runtime state. This probe records what was
# genuinely observed at runtime so that the evidence set carries its own
# provenance instead of relying on the image tag being trustworthy.
#
# Output is deterministic: sorted keys, no timestamps, no host paths that vary
# between runs of the same image.
set -euo pipefail

PREFIX="${OCCURFRAME_PREFIX:-/opt/occurframe}"

read_os() {
  # shellcheck disable=SC1091
  . /etc/os-release
  jq -n --arg id "${ID:-}" --arg version_id "${VERSION_ID:-}" \
        --arg pretty "${PRETTY_NAME:-}" --arg codename "${VERSION_CODENAME:-}" \
    '{id:$id, version_id:$version_id, pretty_name:$pretty, codename:$codename}'
}

read_tzdata_zi() {
  local path=/usr/share/zoneinfo/tzdata.zi
  if [ -r "$path" ]; then
    jq -n --arg path "$path" \
          --arg version "$(sed -n 's/^# version //p' "$path" | head -1)" \
          --arg dataform "$(sed -n 's/^# dataform //p' "$path" | head -1)" \
          --arg sha256 "$(sha256sum "$path" | cut -d' ' -f1)" \
      '{available:true, path:$path, release:$version, dataform:$dataform, sha256:$sha256}'
  else
    jq -n '{available:false}'
  fi
}

python_tzdata() {
  "${PREFIX}/venv/bin/python" - <<'PY' 2>/dev/null || echo '{"available":false}'
import json
try:
    import tzdata
    print(json.dumps({"available": True, "package_version": tzdata.__version__,
                      "iana_version": tzdata.IANA_VERSION}, sort_keys=True))
except Exception:
    print(json.dumps({"available": False}, sort_keys=True))
PY
}

php_tzdb() {
  if command -v php >/dev/null 2>&1; then
    local reported
    reported="$(php -r 'echo timezone_version_get();')"
    if [ "$reported" = "0.system" ] && [ -r /usr/share/zoneinfo/tzdata.zi ]; then
      jq -n --arg reported "$reported" \
            --arg release "$(sed -n 's/^# version //p' /usr/share/zoneinfo/tzdata.zi | head -1)" \
        '{available:true, reported_release:$reported, effective_source:"system zoneinfo",
          release_kind:"exact", release:$release}'
    else
      jq -n --arg release "$reported" \
        '{available:true, effective_source:"PHP bundled tzdb",
          release_kind:"exact", release:$release}'
    fi
  else
    jq -n '{available:false}'
  fi
}

bun_tzdb() {
  if command -v bun >/dev/null 2>&1; then
    bun -e '
      const o = (z, iso) => new Intl.DateTimeFormat("en-US",{timeZone:z,timeZoneName:"longOffset"})
        .formatToParts(new Date(iso)).find(p=>p.type==="timeZoneName").value;
      const out = {available:true, versions:{...process.versions}};
      out.probe = {
        "America/Vancouver@2026-11-02": o("America/Vancouver","2026-11-02T12:00:00Z"),
        "America/Edmonton@2026-11-02": o("America/Edmonton","2026-11-02T12:00:00Z"),
        "Africa/Casablanca@2026-09-21": o("Africa/Casablanca","2026-09-21T12:00:00Z"),
      };
      process.stdout.write(JSON.stringify(out));
    ' 2>/dev/null || jq -n '{available:false}'
  else
    jq -n '{available:false}'
  fi
}

runtime_entry() {
  jq -n --arg language "$1" --arg runtime "$2" --arg configured "$3" \
        --arg observed "$4" --arg path "$5" \
    '{language:$language, runtime:$runtime, configured_requirement:$configured,
      observed_version:$observed, executable:$path,
      matches_configured: ($configured == $observed)}'
}

PY_OBSERVED="$("${PREFIX}/venv/bin/python" -c 'import platform;print(platform.python_version())' 2>/dev/null || echo unknown)"
GO_OBSERVED="$(go version 2>/dev/null | awk '{print $3}' || echo unknown)"
BUN_OBSERVED="$(bun --version 2>/dev/null || echo unknown)"
PHP_OBSERVED="$(php -r 'echo PHP_VERSION;' 2>/dev/null || echo unknown)"
RUBY_OBSERVED="$(ruby -e 'print RUBY_VERSION' 2>/dev/null || echo unknown)"
RUST_OBSERVED="$(rustc --version 2>/dev/null | awk '{print $2}' || echo unknown)"

RUNTIMES="$(jq -s '.' <(
  runtime_entry Python CPython "${OCCURFRAME_PIN_PYTHON:-}" "$PY_OBSERVED" "${PREFIX}/venv/bin/python"
  runtime_entry Go gc "go${OCCURFRAME_PIN_GO:-}" "$GO_OBSERVED" "$(command -v go || true)"
  runtime_entry JavaScript Bun "${OCCURFRAME_PIN_BUN:-}" "$BUN_OBSERVED" "$(command -v bun || true)"
  runtime_entry PHP PHP "${OCCURFRAME_PIN_PHP:-}" "$PHP_OBSERVED" "$(command -v php || true)"
  runtime_entry Ruby MRI "${OCCURFRAME_PIN_RUBY:-}" "$RUBY_OBSERVED" "$(command -v ruby || true)"
))"

ENGINE_DEPS="$(
  {
    for dir in "${PREFIX}"/php-deps/* "${PREFIX}"/ruby-deps/*; do
      [ -d "$dir/.git" ] || continue
      jq -n --arg name "$(basename "$dir")" \
            --arg commit "$(git -C "$dir" rev-parse HEAD)" \
            --arg origin "$(git -C "$dir" config --get remote.origin.url)" \
        '{name:$name, commit:$commit, origin:$origin, acquisition:"git checkout"}'
    done
  } | jq -s 'sort_by(.name)'
)"

PIP_FREEZE="$(jq -Rs 'split("\n") | map(select(length>0)) | sort' \
  < "${PREFIX}/provenance/python-freeze.txt" 2>/dev/null || echo '[]')"

DOWNLOADS="$(jq -Rs 'split("\n") | map(select(length>0)) | sort' \
  < "${PREFIX}/provenance/downloads.sha256" 2>/dev/null || echo '[]')"

TZ_DOWNLOADS="$(jq -Rs 'split("\n") | map(select(length>0)) | sort' \
  < "${PREFIX}/provenance/tzdb-downloads.sha256" 2>/dev/null || echo '[]')"

RUBY_UNPINNED="$(jq -Rs 'split("\n") | map(select(length>0)) | sort' \
  < "${PREFIX}/provenance/ruby-unpinned.txt" 2>/dev/null || echo '[]')"

jq -S -n \
  --arg canonical_platform "Ubuntu 24.04 x86_64" \
  --arg architecture "$(uname -m)" \
  --arg kernel "$(uname -r)" \
  --arg container_runtime "docker" \
  --arg image_reference "${OCCURFRAME_IMAGE_REFERENCE:-unrecorded}" \
  --arg image_digest "${OCCURFRAME_IMAGE_DIGEST:-unrecorded}" \
  --arg base_image "${OCCURFRAME_BASE_IMAGE:-unrecorded}" \
  --arg base_image_digest "${OCCURFRAME_BASE_IMAGE_DIGEST:-unrecorded}" \
  --arg rust "$RUST_OBSERVED" \
  --argjson os "$(read_os)" \
  --argjson runtimes "$RUNTIMES" \
  --argjson system_zoneinfo "$(read_tzdata_zi)" \
  --argjson python_tzdata_package "$(python_tzdata)" \
  --argjson php_bundled_tzdb "$(php_tzdb)" \
  --argjson bun_runtime_icu "$(bun_tzdb)" \
  --argjson engine_dependencies "$ENGINE_DEPS" \
  --argjson python_packages "$PIP_FREEZE" \
  --argjson toolchain_downloads "$DOWNLOADS" \
  --argjson tzdb_downloads "$TZ_DOWNLOADS" \
  --argjson ruby_unpinned "$RUBY_UNPINNED" \
  '{
     canonical_platform: $canonical_platform,
     observed: {
       os: $os,
       architecture: $architecture,
       kernel: $kernel
     },
     container: {
       runtime: $container_runtime,
       image_reference: $image_reference,
       image_digest: $image_digest,
       base_image: $base_image,
       base_image_digest: $base_image_digest
     },
     runtimes: $runtimes,
     orchestration_toolchain: { rustc: $rust },
     runner_environment_policy: {
       name: "hermetic_allowlist_v1",
       launch_resolution: "resolve_before_environment_clear",
       inherited_environment: "cleared",
       deliberate_timezone: "UTC",
       locale_policy: "fixed_c",
       explicit_build_variables: "names recorded per observation; values excluded"
     },
     tzdb_provenance_sources: {
       system_zoneinfo: $system_zoneinfo,
       python_tzdata_package: $python_tzdata_package,
       php_bundled_tzdb: $php_bundled_tzdb,
       bun_runtime_icu: $bun_runtime_icu
     },
     engine_dependencies: $engine_dependencies,
     python_packages: $python_packages,
     toolchain_download_digests: $toolchain_downloads,
     tzdb_download_digests: $tzdb_downloads,
     unprovisioned_dependencies: $ruby_unpinned
   }'
