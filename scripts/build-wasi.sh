#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
: "${WASI_SDK_PATH:?Set WASI_SDK_PATH to the unpacked WASI SDK 34 directory}"
export CC_wasm32_wasip3="$WASI_SDK_PATH/bin/wasm32-wasip3-clang"
export AR_wasm32_wasip3="$WASI_SDK_PATH/bin/llvm-ar"
# WASI sockets in Tokio are still behind this target-specific flag.
export CARGO_TARGET_WASM32_WASIP3_RUSTFLAGS='--cfg tokio_unstable'

cargo +1.100.0 build --locked --release --target wasm32-wasip3 \
  -p lingua-tutor-local --bin lingua-tutor
mkdir -p dist-wasi
cp "${CARGO_TARGET_DIR:-target}/wasm32-wasip3/release/lingua-tutor.wasm" \
  "dist-wasi/lingua-tutor-v$(cat VERSION)-wasi-p3.wasm"

if [[ "${1:-}" == test ]]; then
  test_state="$(mktemp -d)"
  trap 'rm -rf "$test_state"' EXIT
  export CARGO_TARGET_WASM32_WASIP3_RUNNER="wasmtime run -S p3=y -S inherit-network=y --dir $test_state::."
  cargo +1.100.0 test --locked --target wasm32-wasip3 -p lingua-tutor-local --lib \
    server::tests::browser_connect_waits_for_poll_and_allows_retry -- --exact
fi
