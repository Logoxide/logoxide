"""Writes one package's README from README.md, which has every language's examples: npm's in
TypeScript (`ts`), PyPI's in Python (`python`).

    python scripts/readme.py ts|python [-o README.md]

README.md is the repository's, read on GitHub, and the source of both (examples/node/readme.ts
and examples/python/readme.py run its examples). For a language, the other language's code blocks
are left out, and so is:
- `<!-- source -->…<!-- /source -->`: what's for the repository alone (building, the checks)
- `<!-- ts: … -->`, `<!-- python: … -->`: hidden in README.md, the text in that language's README
  (where the repository's says both languages' ways, or names only TypeScript's)
Python's README also names things as Python does: in inline code, a camelCase name is
snake_case (`openPackage` is `open_package`), `null` is `None`, and `true` and `false` are
`True` and `False`, but for the few in `PYTHON_NAMES`, which Python spells otherwise.
"""

import argparse
import re
import sys
from pathlib import Path

LANGUAGES = {"ts": "python", "python": "ts"}

# Inline code that Python spells otherwise, not by snake_case
PYTHON_NAMES = {
    "null": "None",
    "true": "True",
    "false": "False",
    "_import": "import_",
    "Logoxide.new": "Logoxide()",
}

# A name, a call (`book(id)`), or a chain of them (`info().upstream`)
CHAIN = re.compile(r"[A-Za-z_]\w*(\([\w, ]*\))?(\.[A-Za-z_]\w*(\([\w, ]*\))?)*")


def snake(name: str) -> str:
    """`openPackage` as `open_package`; any other name (a class, OSIS's `sID`) as it is"""
    if not re.fullmatch(r"[a-z][a-z0-9]*([A-Z][a-z0-9]+)+", name):
        return name
    return re.sub(r"(?<=[a-z0-9])([A-Z])", lambda m: "_" + m[1].lower(), name)


def python_code(code: str) -> str:
    if code in PYTHON_NAMES:
        return PYTHON_NAMES[code]
    if not CHAIN.fullmatch(code):
        return code
    return re.sub(r"[A-Za-z_]\w*", lambda m: snake(m[0]), code)


def marked(text: str, language: str) -> str:
    """The marked passages for one language: the repository's alone left out, the language's own
    shown"""
    other = LANGUAGES[language]
    # Each stops at its own end, never another's
    source = r"<!-- source -->(?:(?!<!-- /?source -->).)*<!-- /source -->"
    for left_out in [source, rf"<!-- {other}:(?:(?!-->).)*-->"]:
        # Lines of their own go with their line break; inline, the text around them stays
        text = re.sub(rf"^{left_out}\n", "", text, flags=re.S | re.M)
        text = re.sub(left_out, "", text, flags=re.S)
    return re.sub(rf"<!-- {language}:[ \n]?(.*?)[ \n]?-->", r"\1", text, flags=re.S)


def prose(text: str, language: str) -> str:
    """Text outside code blocks, for one language"""
    if language == "python":
        text = re.sub(r"`([^`\n]+)`", lambda m: f"`{python_code(m[1])}`", text)
    return text


def readme(source: str, language: str) -> str:
    other = LANGUAGES[language]
    source = marked(source, language)
    # Code blocks and the text between them, in turn
    parts = re.split(r"(^```[^\n]*\n.*?^```\n?)", source, flags=re.M | re.S)
    out = []
    for i, part in enumerate(parts):
        if i % 2 == 1:
            if not part.startswith(f"```{other}\n"):
                out.append(part)
        else:
            out.append(prose(part, language))
    text = "".join(out)
    # A block left out leaves the blank lines around it
    return re.sub(r"\n{3,}", "\n\n", text)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("language", choices=sorted(LANGUAGES))
    parser.add_argument("-o", "--output", type=Path)
    args = parser.parse_args()
    source = (Path(__file__).resolve().parent.parent / "README.md").read_text()
    text = readme(source, args.language)
    if args.output:
        args.output.write_text(text)
    else:
        sys.stdout.write(text)


if __name__ == "__main__":
    main()
