#!/usr/bin/env bash
# Builds the npm package (biblix) into dist/wasm/pkg, with its README.
set -euo pipefail
cd "$(dirname "$0")/.."
boltffi pack wasm --release
cp README.md dist/wasm/pkg/README.md
