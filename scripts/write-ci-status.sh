#!/usr/bin/env bash
# Preserve a rejection journal even if installing/building the native tool failed.
set -euo pipefail
kind="${1:?input or preflight is required}"
output="${2:?output path is required}"
case "$kind" in input|preflight) ;; *) echo "invalid CI status kind" >&2; exit 2 ;; esac
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
mkdir -p -- "$(dirname -- "$output")"
if ! cargo run --quiet --locked --offline --manifest-path "$root/xtask/Cargo.toml" \
    --package xtask -- write-ci-status --kind "$kind" --output "$output"; then
  # Fixed negative data needs no interpreter, compiler, or untrusted interpolation.
  printf '%s\n' '{"status":"failure","error":"native Rust status writer unavailable","registry_uploaded":false}' > "$output"
  exit 1
fi
