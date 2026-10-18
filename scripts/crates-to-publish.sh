#!/usr/bin/env bash
# Prints the workspace's published crates (all but `publish = false`) whose version isn't on
# crates.io yet, one name a line, so a release publishes only what's missing: a version already
# up (published by hand, or before a rate limit stopped the rest) is left alone.
#   scripts/crates-to-publish.sh
set -euo pipefail
cd "$(dirname "$0")/.."
python=$(command -v python3 || command -v python)
cargo metadata --no-deps --format-version 1 |
  "$python" -c '
import json, sys
for p in json.load(sys.stdin)["packages"]:
    if p.get("publish") != []:
        print(p["name"], p["version"])
' |
  while read -r name version; do
    code=$(curl -s -o /dev/null -w '%{http_code}' -A 'logoxide-release (github.com/Logoxide/logoxide)' \
      "https://crates.io/api/v1/crates/$name/$version")
    case $code in
      200) ;;
      404) echo "$name" ;;
      *)
        echo "crates.io answered $code for $name $version" >&2
        exit 1
        ;;
    esac
  done
