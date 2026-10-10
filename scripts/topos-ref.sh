#!/usr/bin/env bash
# Prints the topos tag this checkout builds against (`v0.6.0`): topos-bible's version in the
# workspace's Cargo.toml, the one place it's written. CI checks out that tag for the bindings
# (BoltFFI only exports path dependencies' items, so logoxide-ffi needs a topos checkout).
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
version=$(sed -n 's/^topos-bible = "\([0-9][^"]*\)".*/\1/p' "$root/Cargo.toml")
if [[ -z "$version" ]]; then
  echo 'no `topos-bible = "<version>"` in Cargo.toml'"'"'s [workspace.dependencies]' >&2
  exit 1
fi
echo "v$version"
