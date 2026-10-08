# biblix (bindings)

biblix's packages with topos's whole API, in one package for TypeScript/WebAssembly, Python,
Swift, and Kotlin (through [BoltFFI](https://github.com/boltffi/boltffi)). `Topos`, `ToposBook`,
`ToposTranslations`, and the rest are the same as in topos's own package (`topos-bible`).

The host reads and writes the files: an app's file API is often asynchronous (Obsidian's vault),
and WebAssembly has none. biblix reads the text from the bytes it's given, and hands back the
cache files it wrote for the host to save.

```ts
import { Biblix, Topos, ToposTranslations, BookStyle, OffsetUnit } from "biblix";

const biblix = Biblix.new();
const topos = Topos.new();

// A vault's biblix.toml and biblix.lock say which packages it uses, and where their files are
const project = biblix.parseProject(manifestText, lockText);
let translations = ToposTranslations.create();
const packages = new Map();
for (const entry of project.packages()) {
  // entry.location: { tag: "Library", dir: "packages/bsb/1.0.1" } (in the library) or
  // { tag: "Path", dir } (relative to the vault)
  const pkg = biblix.openPackage(await readAll(entry.location.dir), entry.features, await readCache());
  for (const file of pkg.cacheWrites()) await saveCache(file.path, file.bytes);
  translations = pkg.addTo(translations); // its name, aliases, and verse counts
  packages.set(entry.id, pkg);
}

const found = topos.parseWithTranslation("3 John 1:15 BSB", OffsetUnit.Utf16, translations, BookStyle.Name);
// null: the BSB's 3 John ends at verse 14

const john = topos.parse("John 3:16", BookStyle.Name)!;
packages.get("bsb").book(john.bookId).html(john, { notes: false, headings: true, classPrefix: "topos-" });
```

- `openPackage` takes every file under the package's folder (but `.git`), with paths like
  `usx/JHN.usx`, and the cache files saved before; natively, `openPackageDir` reads a folder and
  keeps its own cache
- Opening counts every book's verses unless the cache has them; for USFM that means parsing the
  whole text (under a second for the KJV), so give the cache back next time
- `checksum()` is what the lock file's `checksum` should be, to catch changed files
- Errors are thrown as `BiblixErrorException` (`error.value` is `{ tag, message }`)

Fetching and installing packages over the host's HTTP comes next.

## Building

```sh
npm install && npm run pack:wasm   # dist/wasm/pkg (npm: biblix)
npm run pack:python                # dist/python/wheelhouse (PyPI: biblix)
```

`examples/node` and `examples/python` open real packages:

```sh
cd examples/node && npm install && npm start -- ~/Development/biblix-packages/bsb ~/Development/biblix-packages/kjv
python examples/python/packages.py ~/Development/biblix-packages/bsb ~/Development/biblix-packages/kjv
```

topos's crate is a path dependency (`../topos`), as for the rest of biblix.
