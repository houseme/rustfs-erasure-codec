#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT_DIR}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "[backend-consistency] cargo not found"
  exit 1
fi

run_test() {
  local backend="$1"
  local test_name="$2"
  echo
  echo "[backend-consistency] backend=${backend} test=${test_name}"
  RSE_BACKEND_OVERRIDE="${backend}" \
  RSE_STRICT_BACKEND_OVERRIDE=1 \
    cargo test --features "std simd-accel" "${test_name}" -- --nocapture
}

BACKENDS=()
case "$(uname -m)" in
  x86_64)
    BACKENDS=(auto scalar rust-ssse3 rust-avx2 rust-avx512 rust-gfni-avx2 rust-gfni-avx512)
    ;;
  arm64|aarch64)
    BACKENDS=(auto scalar rust-neon)
    ;;
  ppc64le|powerpc64le)
    BACKENDS=(auto scalar rust-vsx)
    ;;
  *)
    BACKENDS=(auto scalar)
    ;;
esac

for backend in "${BACKENDS[@]}"; do
  # Metadata test validates the default runtime dispatch for the host.
  # It should run only with auto selection, not with forced override backends.
  if [[ "${backend}" == "auto" ]]; then
    run_test "${backend}" "test_active_backend_metadata"
  fi
  run_test "${backend}" "test_backend_override_affects_active_backend"
done
