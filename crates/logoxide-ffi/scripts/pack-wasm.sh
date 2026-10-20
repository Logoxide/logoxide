#!/usr/bin/env bash
# Builds the npm package (logoxide) into dist/wasm/pkg, with its README (README.md in TypeScript
# alone, its examples type-checked against the package: scripts/readme.py, which needs `npm
# install`), license, and the notices of the crates compiled into it (THIRD-PARTY-NOTICES).
set -euo pipefail
cd "$(dirname "$0")/.."
python=$(command -v python3 || command -v python)
boltffi pack wasm --release
"$python" scripts/readme.py ts --api dist/wasm/pkg/logoxide.d.ts -o dist/wasm/pkg/README.md
cp ../../LICENSE dist/wasm/pkg/LICENSE
# web.js, bundler.js, and node.js export `initialized`, which BoltFFI's types leave out
types=dist/wasm/pkg/logoxide.d.ts
if ! grep -q 'export declare const initialized' "$types"; then
  printf '%s\n' '' \
    '/** Resolves once the WebAssembly module is loaded: await it before the first call in a browser (in Node and Bun it already has) */' \
    'export declare const initialized: Promise<void>;' >> "$types"
fi
# BoltFFI's exceptions say only their type (`super("LogoxideError")`), so `error.message` and
# `${error}` lose what went wrong: each takes its value's message instead
node -e '
const fs = require("fs");
let found = 0;
for (const name of fs.readdirSync("dist/wasm/pkg")) {
  if (!/\.(js|ts)$/.test(name) || name.endsWith(".d.ts")) continue;
  const path = "dist/wasm/pkg/" + name;
  const text = fs.readFileSync(path, "utf8");
  const fixed = text.replace(/super\("(ToposError|LogoxideError)"\);/g, (_, type) => {
    found++;
    return `super(value.message ?? "${type}");`;
  });
  if (fixed !== text) fs.writeFileSync(path, fixed);
}
if (found === 0) {
  console.error("pack-wasm.sh: no exception constructors found to give their messages");
  process.exit(1);
}
'
"$python" scripts/third-party-notices.py --target wasm32-unknown-unknown -o dist/wasm/pkg/THIRD-PARTY-NOTICES
# The generated package.json lists its files, and npm adds only README and LICENSE on its own
node -e '
const fs = require("fs");
const path = "dist/wasm/pkg/package.json";
const pkg = JSON.parse(fs.readFileSync(path, "utf8"));
if (pkg.files && !pkg.files.includes("THIRD-PARTY-NOTICES")) pkg.files.push("THIRD-PARTY-NOTICES");
fs.writeFileSync(path, JSON.stringify(pkg, null, 2) + "\n");
'
