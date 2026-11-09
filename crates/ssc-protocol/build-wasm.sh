#!/usr/bin/env bash
set -euo pipefail
crate_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
wasm_bindgen="${WASM_BINDGEN:-wasm-bindgen}"
if [[ "$("$wasm_bindgen" --version)" != "wasm-bindgen 0.2.130" ]]; then
  echo 'Required: wasm-bindgen-cli 0.2.130 (cargo install wasm-bindgen-cli --version 0.2.130 --locked)' >&2
  exit 1
fi
cargo build --manifest-path "$crate_dir/Cargo.toml" --locked --target wasm32-unknown-unknown --release --target-dir "$crate_dir/target"
"$wasm_bindgen" "$crate_dir/target/wasm32-unknown-unknown/release/ssc_protocol.wasm" --target web --out-dir "${1:-$crate_dir/pkg}"
