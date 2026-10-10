"""Adds PyPI metadata to the Python package that `boltffi generate python` makes.

Run after generating and before building (`boltffi pack python --regenerate false`); setuptools
merges setup.cfg with the generated setup.py.
"""

import shutil
import sys
from pathlib import Path

root = Path(__file__).resolve().parent.parent
package = root / "dist" / "python"
if not (package / "setup.py").is_file():
    sys.exit(f"{package} has no setup.py; run `boltffi generate python` first")
if not (package / "THIRD-PARTY-NOTICES").is_file():
    sys.exit(f"{package} has no THIRD-PARTY-NOTICES; run scripts/third-party-notices.py first")

shutil.copy(root / "README.md", package / "README.md")
shutil.copy(root.parent.parent / "LICENSE", package / "LICENSE")
(package / "setup.cfg").write_text(
    """[metadata]
description = Bibles, commentaries, lexicons, and books as versioned packages, with topos's Bible references
long_description = file: README.md
long_description_content_type = text/markdown
license_expression = CC0-1.0
license_files =
    LICENSE
    THIRD-PARTY-NOTICES
url = https://github.com/Logoxide/logoxide
project_urls =
    Source = https://github.com/Logoxide/logoxide
    Issues = https://github.com/Logoxide/logoxide/issues
keywords = bible, scripture, verse, reference, usx, usfm, sword, package manager
classifiers =
    Development Status :: 3 - Alpha
    Intended Audience :: Developers
    Programming Language :: Python :: 3
    Programming Language :: Rust
    Topic :: Religion
    Topic :: Text Processing
"""
)
print(f"added metadata to {package}")
