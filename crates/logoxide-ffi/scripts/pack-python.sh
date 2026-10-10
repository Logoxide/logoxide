#!/usr/bin/env bash
# Builds release wheels with PyPI metadata into dist/python/wheelhouse, each with the notices of
# the crates compiled into it (THIRD-PARTY-NOTICES, for the host's target).
# Pass interpreters to build for several Pythons: scripts/pack-python.sh --python python3.12 ...
set -euo pipefail
cd "$(dirname "$0")/.."
rm -rf dist/python
boltffi generate python
python=$(command -v python3 || command -v python || echo /opt/python/cp312-cp312/bin/python)
"$python" scripts/third-party-notices.py -o dist/python/THIRD-PARTY-NOTICES
"$python" scripts/python-metadata.py
boltffi pack python --release --regenerate false "$@"
