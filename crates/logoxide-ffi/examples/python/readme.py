"""Runs every Python example in ../../README.md, in order, as one program, and checks what each
prints against its `# → ` comments (one for each line printed, in order; `…` stands for any text).

Run after `pip install dist/python/wheelhouse/*.whl` (see ../../README.md):
    python examples/python/readme.py [packages] [--show]
where `packages` holds the KJV and BSB packages (git repositories, as in
~/Development/logoxide-packages).

The examples run in a new folder holding `kjv` and `bsb` (links to them) and the files in
../fixtures. Their downloads are answered here, offline, as the hosts would answer them:
`https://example.org/kjv` and `/bsb` are those repositories (their refs, and archives made by git),
`https://example.org/catalog.toml` and eBible.org's list and zip are ../fixtures' files.
"""

import contextlib
import io
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.error
import urllib.request

here = pathlib.Path(__file__).resolve().parent
args = [a for a in sys.argv[1:] if a != "--show"]
show = "--show" in sys.argv
packages = pathlib.Path(args[0] if args else "~/Development/logoxide-packages").expanduser().resolve()
fixtures = here.parent / "fixtures"
readme = (here.parent.parent / "README.md").read_text()
blocks = re.findall(r"^```python\n(.*?)^```$", readme, flags=re.M | re.S)


def served(url: str) -> bytes | None:
    """A download, answered offline"""
    repository = lambda name: packages / name if name in ("kjv", "bsb") else None
    m = re.fullmatch(r"https://example\.org/(\w+?)(?:\.git)?/info/refs\?service=git-upload-pack", url)
    if m and repository(m[1]):
        return subprocess.check_output(["git", "upload-pack", "--http-backend-info-refs", repository(m[1])])
    m = re.fullmatch(r"https://example\.org/(\w+)/archive/([0-9a-f]{40})\.tar\.gz", url)
    if m and repository(m[1]):
        archive = ["git", "-C", repository(m[1]), "archive", "--format=tar.gz", f"--prefix={m[1]}-{m[2]}/", m[2]]
        return subprocess.check_output(archive)
    name = {
        "https://example.org/catalog.toml": "catalog.toml",
        "https://ebible.org/Scriptures/translations.csv": "translations.csv",
        "https://ebible.org/Scriptures/eng-asv_usfm.zip": "eng-asv_usfm.zip",
        "https://example.org/sermons-on-love/sermons-on-love-1.0.0.tar.gz": "sermons-on-love-1.0.0.tar.gz",
    }.get(url)
    return (fixtures / name).read_bytes() if name else None


def urlopen(url, *args, **kwargs):
    url = url.full_url if isinstance(url, urllib.request.Request) else url
    body = served(url)
    if body is None:
        raise urllib.error.HTTPError(url, 404, "Not Found", None, None)
    return io.BytesIO(body)


urllib.request.urlopen = urlopen

folder = pathlib.Path(tempfile.mkdtemp(prefix="logoxide-readme-"))
for name in ("kjv", "bsb"):
    (folder / name).symlink_to(packages / name)
for file in fixtures.iterdir():
    shutil.copy(file, folder / file.name)
os.chdir(folder)

namespace: dict = {"__name__": "__readme__"}
printed = []
for block in blocks:
    out = io.StringIO()
    with contextlib.redirect_stdout(out):
        exec(compile(block, "README.md", "exec"), namespace)
    # A line at a time, blank lines left out
    printed.append([line for line in out.getvalue().splitlines() if line.strip()])


def flat(text: str) -> str:
    return " ".join(text.split())


def matches(expected: str, actual: str) -> bool:
    if expected == actual:
        return True
    parts = expected.split("…")
    if len(parts) == 1 or not actual.startswith(parts[0]) or not actual.endswith(parts[-1]):
        return False
    at = len(parts[0])
    for part in parts[1:-1]:
        found = actual.find(part, at)
        if found < 0:
            return False
        at = found + len(part)
    return at <= len(actual) - len(parts[-1])


failed = 0
for block, actual in zip(blocks, printed):
    expected = [flat(e) for e in re.findall(r"# → (.*)$", block, flags=re.M)]
    actual = [flat(a) for a in actual]
    ok = len(expected) == len(actual) and all(matches(e, a) for e, a in zip(expected, actual))
    failed += not ok
    if not ok or show:
        line = readme[: readme.index(block)].count("\n") + 1
        print(f"{'ok' if ok else 'FAILED'}: the example at README.md:{line}")
        for j in range(max(len(expected), len(actual))):
            print(f"  {expected[j] if j < len(expected) else '(nothing)'}")
            print(f"  {actual[j] if j < len(actual) else '(nothing printed)'}")
os.chdir(here)
if failed:
    print(f"{failed} of {len(blocks)} examples print otherwise than README.md says (run in {folder})")
else:
    shutil.rmtree(folder)
    print(f"all {len(blocks)} examples print what README.md says")
sys.exit(1 if failed else 0)
