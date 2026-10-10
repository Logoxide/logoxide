"""Checks an installed logoxide wheel: it imports, its native library loads, and a few calls that
need no files answer. Release CI runs it in a fresh environment, from outside the source tree:

    python -m venv /tmp/smoke && /tmp/smoke/bin/pip install --no-index --find-links dist/python/wheelhouse logoxide
    (cd /tmp && /tmp/smoke/bin/python path/to/scripts/smoke-test.py)
"""

import logoxide
from logoxide import Logoxide, Topos

lx = Logoxide()
version = lx.version()
assert version, "logoxide has a version"

# A small package's manifest, read without any files
info = lx.read_package_info(
    '[package]\nid = "t"\nname = "Test"\nabbreviation = "T"\nversion = "1.0.0"\nlanguage = "en"\n\n'
    '[formats]\nusx = { path = "usx/" }\n'
)
assert info.id == "t" and info.abbreviation == "T", info

# topos's API comes in the same package
assert Topos() is not None

print(f"logoxide {version} from {logoxide.__file__}: ok")
