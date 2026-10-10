#!/usr/bin/env bash
# Builds the npm package (logoxide) into dist/wasm/pkg, with its README, license, and the notices
# of the crates compiled into it (THIRD-PARTY-NOTICES).
set -euo pipefail
cd "$(dirname "$0")/.."
boltffi pack wasm --release
cp README.md dist/wasm/pkg/README.md
cp ../../LICENSE dist/wasm/pkg/LICENSE
python=$(command -v python3 || command -v python)
"$python" scripts/third-party-notices.py --target wasm32-unknown-unknown -o dist/wasm/pkg/THIRD-PARTY-NOTICES
# The generated package.json lists its files, and npm adds only README and LICENSE on its own
node -e '
const fs = require("fs");
const path = "dist/wasm/pkg/package.json";
const pkg = JSON.parse(fs.readFileSync(path, "utf8"));
if (pkg.files && !pkg.files.includes("THIRD-PARTY-NOTICES")) pkg.files.push("THIRD-PARTY-NOTICES");
fs.writeFileSync(path, JSON.stringify(pkg, null, 2) + "\n");
'
