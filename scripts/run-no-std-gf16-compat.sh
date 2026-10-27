#!/bin/sh
# Build and run the pinned old/current GF16 probe against an extracted crate.
set -eu

if [ "$#" -ne 2 ] || { [ "$1" != wasm ] && [ "$1" != native ]; }; then
    printf 'Usage: %s {wasm|native} EXTRACTED_CANDIDATE_CRATE\n' "$0" >&2
    exit 2
fi
mode=$1
repo_root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
candidate=$(CDPATH='' cd -- "$2" && pwd)
if [ ! -f "$candidate/Cargo.toml" ]; then
    printf 'Candidate has no Cargo.toml: %s\n' "$candidate" >&2
    exit 2
fi
if [ "$mode" = wasm ] && [ "$(wasm-bindgen --version)" != 'wasm-bindgen 0.2.129' ]; then
    printf 'This pinned probe requires wasm-bindgen 0.2.129.\n' >&2
    exit 2
fi

work=$(mktemp -d "${TMPDIR:-/tmp}/rse-wasm-gf16-compat.XXXXXXXX")
trap 'rm -rf "$work"' EXIT HUP INT TERM
mkdir -p "$work/src"
cp "$repo_root/scripts/diagnostics/wasm_no_std_gf16_compat.rs" "$work/src/lib.rs"
cp "$repo_root/scripts/diagnostics/wasm_no_std_gf16_compat.lock" "$work/Cargo.lock"

# JSON string quoting is valid TOML for paths, including spaces and backslashes.
node - "$candidate" "$work/Cargo.toml" "$mode" <<'NODE'
const fs = require('node:fs');
const candidate = JSON.stringify(process.argv[2]);
const crateType = process.argv[4] === 'wasm' ? 'cdylib' : 'rlib';
fs.writeFileSync(process.argv[3], `[package]
name = "rse-wasm-gf16-compat-probe"
version = "0.0.0"
edition = "2024"
[lib]
crate-type = ["${crateType}"]
[dependencies]
codec_old = { package = "rustfs-erasure-codec", version = "=9.0.0", default-features = false }
codec_new = { package = "rustfs-erasure-codec", path = ${candidate}, version = "=9.1.0", default-features = false }
wasm-bindgen = "=0.2.129"
`);
NODE

export RUSTUP_TOOLCHAIN="${RUSTUP_TOOLCHAIN:-stable}"
export CARGO_NET_OFFLINE=true
export CARGO_TARGET_DIR="$work/target"
if [ "$mode" = wasm ]; then
    target=wasm32-unknown-unknown
else
    target=${RSE_COMPAT_TARGET:-$(rustc -vV | sed -n 's/^host: //p')}
    if [ -z "$target" ]; then
        printf 'Could not determine Rust host target.\n' >&2
        exit 1
    fi
    cat > "$work/src/main.rs" <<'RUST'
fn main() {
    let checks = rse_wasm_gf16_compat_probe::validate().expect("GF16 compatibility");
    assert_eq!(checks, 126);
    println!("Packaged native no_std GF16 checks: {checks}");
}
RUST
fi
for version in 9.0.0 9.1.0; do
    tree=$(cargo tree --manifest-path "$work/Cargo.toml" --target "$target" \
        --offline --locked -e features -i "rustfs-erasure-codec@$version")
    printf '%s\n' "$tree"
    if printf '%s\n' "$tree" | grep -F 'rustfs-erasure-codec feature "std"' >/dev/null; then
        printf 'The no_std probe unexpectedly enabled std for %s.\n' "$version" >&2
        exit 1
    fi
done
if [ "$mode" = wasm ]; then
    cargo build --manifest-path "$work/Cargo.toml" --target "$target" --release --locked --offline
    wasm-bindgen --target nodejs --out-dir "$work/out" --out-name probe \
        "$work/target/$target/release/rse_wasm_gf16_compat_probe.wasm"
    node - "$work/out/probe.js" <<'NODE'
const count = require(process.argv[2]).validate();
if (count !== 126) throw new Error(`Expected 126 GF16 checks, got ${count}`);
console.log(`Packaged WASM no_std GF16 checks: ${count}`);
NODE
else
    cargo run --manifest-path "$work/Cargo.toml" --target "$target" --release --locked --offline
fi
