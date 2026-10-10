#!/usr/bin/env bash
# Builds release wheels into dist/python/wheelhouse.
# Pass interpreters to build for several Pythons: scripts/pack-python.sh --python python3.12 ...
set -euo pipefail
cd "$(dirname "$0")/.."
rm -rf dist/python
boltffi generate python
boltffi pack python --release --regenerate false "$@"
