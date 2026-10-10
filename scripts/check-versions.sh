#!/usr/bin/env bash
# Checks that the versions that must agree do: the workspace's (the crates'), the `logoxide`
# workspace dependency's, and logoxide-ffi's (the npm and PyPI packages'; a workspace of its own,
# so it can't inherit the version). With a tag (`v0.1.0`), it must be `v` and that version.
#   scripts/check-versions.sh            # in CI
#   scripts/check-versions.sh v0.1.0     # before releasing
set -euo pipefail
root=$(cd "$(dirname "$0")/.." && pwd)
# The first `version = "…"` in each section
workspace=$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' "$root/Cargo.toml")
dependency=$(sed -n 's/^logoxide = {.*version = "\([^"]*\)".*/\1/p' "$root/Cargo.toml")
ffi=$(sed -n '/^\[package\]/,/^\[/s/^version = "\(.*\)"/\1/p' "$root/crates/logoxide-ffi/Cargo.toml")
if [[ -z "$workspace" ]]; then
  echo "Cargo.toml: no [workspace.package] version" >&2
  exit 1
fi
status=0
if [[ "$dependency" != "$workspace" ]]; then
  echo "Cargo.toml: [workspace.dependencies] logoxide is ${dependency:-missing}, the workspace $workspace" >&2
  status=1
fi
if [[ "$ffi" != "$workspace" ]]; then
  echo "crates/logoxide-ffi/Cargo.toml: version ${ffi:-missing}, the workspace $workspace" >&2
  status=1
fi
if [[ $# -gt 0 && "$1" != "v$workspace" ]]; then
  echo "tag $1 isn't v$workspace" >&2
  status=1
fi
if [[ $status -eq 0 ]]; then
  echo "versions agree: $workspace${1:+ (tag $1)}"
fi
exit $status
