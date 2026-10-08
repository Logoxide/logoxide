"""Opens packages the way an app does: it reads the files, biblix reads the text.

Run after `pip install dist/python/wheelhouse/*.whl` (see ../../README.md):
    python examples/python/packages.py ~/Development/biblix-packages/bsb ~/Development/biblix-packages/kjv
"""

import pathlib
import sys
import tempfile
import time

from biblix import Biblix, BookStyle, BookTextOptions, OffsetUnit, PackageFeatures, PackageFile, Topos, ToposTranslations


def files(folder: pathlib.Path) -> list[PackageFile]:
    """Every file under a package's folder but git's, as the host gives them"""
    return [
        PackageFile(path=p.relative_to(folder).as_posix(), bytes=p.read_bytes())
        for p in sorted(folder.rglob("*"))
        if p.is_file() and ".git" not in p.relative_to(folder).parts
    ]


biblix = Biblix()
topos = Topos()
translations = ToposTranslations.create()
defaults = PackageFeatures(features=[], default_features=True)
cache_dir = pathlib.Path(tempfile.mkdtemp())
packages = []

for folder in map(pathlib.Path, sys.argv[1:]):
    start = time.perf_counter()
    package = biblix.open_package(files(folder), defaults, [])
    # The host saves what the cache wrote, and gives it back next time
    writes = package.cache_writes()
    for file in writes:
        (cache_dir / file.path).parent.mkdir(parents=True, exist_ok=True)
        (cache_dir / file.path).write_bytes(file.bytes)
    first = time.perf_counter() - start

    start = time.perf_counter()
    cached = [PackageFile(path=p.relative_to(cache_dir).as_posix(), bytes=p.read_bytes()) for p in cache_dir.rglob("*") if p.is_file()]
    package = biblix.open_package(files(folder), defaults, cached)
    assert package.cache_writes() == [], "the cache had everything"
    again = time.perf_counter() - start

    info = package.info()
    print(f"{info.abbreviation} {info.version} ({package.format()}, {info.license}): {len(package.books())} books, opened in {first:.2f} s, {again:.2f} s from the cache")
    translations = package.add_to(translations)
    packages.append(package)

# References naming a translation are checked against its own verse counts (the BSB's 3 John ends
# at verse 14, like the KJV's)
for reference in ["John 3:16 KJV", "3 John 1:14 BSB", "3 John 1:15 BSB"]:
    found = topos.parse_with_translation(reference, OffsetUnit.CHAR, translations, BookStyle.NAME)
    if found is None:
        print(f"{reference}: no such verse")
    else:
        print(f"{reference}: {found.passage.reference} in {found.translation.name if found.translation else 'no translation opened here'}")

# Passages as text, from each package's own book
john = topos.parse("John 3:16", BookStyle.NAME)
options = BookTextOptions(notes=False, headings=False, class_prefix="topos-")
for package in packages:
    print(f"{package.info().abbreviation}:", package.book(john.book_id).text(john, options)[0].strip())
