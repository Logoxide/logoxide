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
const project = biblix.parseProject(manifestText, lockText); // either can be null
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

## Installing

There's no `git` in WebAssembly, so the host downloads over HTTP and biblix asks for what it
needs: a repository's refs (git's smart HTTP, which GitHub, GitLab, Gitea, Forgejo, and Bitbucket
serve), an archive of a commit (`.tar.gz`), or the files of a package already installed. It
resolves exactly as `bx install` does, so a vault's `biblix.lock` works with `bx` and back.

```ts
const installer = biblix.installer(); // keep one: what it's given is kept
// Adding: read the repository's newest release, then name the dependency by its id
let step = installer.inspect(url, null, null, null);
while (step.tag === "Fetch") { await answer(step.request); step = installer.inspect(url, null, null, null); }
const { info } = step.inspected;
project = project.addDependency(info.id, { git: url, version: info.version, path: null, tag: null,
  branch: null, rev: null, features: [], defaultFeatures: true });

let result = installer.install(project, { tag: "Nothing" }); // or { tag: "All" }, { tag: "Only", ids }
while (result.tag === "Fetch") { await answer(result.request); result = installer.install(project, { tag: "Nothing" }); }
for (const file of result.files) await write(`${library}/${file.path}`, file.bytes);
await write("biblix.toml", project.manifestText());
await write("biblix.lock", result.lock);

async function answer(r: FetchRequest) {
  if (r.tag === "Refs") installer.provideRefs(r.repository, await get(r.url));
  else if (r.tag === "Archive") installer.provideArchive(r.repository, r.commit, await getFirst(r.urls));
  else if (await exists(r.location)) installer.provideFiles(r.location, await readAll(r.location));
  else installer.provideMissing(r.location);
}
```

- `Archive`'s `urls` are the forms hosts use (`/archive/<commit>.tar.gz`, GitLab's
  `/-/archive/...`, Bitbucket's `/get/...`); use the first that answers. An archive's files hash
  the same as a checkout's, unless the repository leaves files out of archives (`export-ignore`)
- Installed versions are checked against the lock file's checksum, so changed files are an error
- `examples/node/install.ts` installs real packages this way (`https://` with `fetch()`; for
  `file://` repositories, git makes the same bytes an HTTP host serves)

## Building

```sh
npm install && npm run pack:wasm   # dist/wasm/pkg (npm: biblix)
npm run pack:python                # dist/python/wheelhouse (PyPI: biblix)
```

`examples/node` and `examples/python` open and install real packages:

```sh
cd examples/node && npm install && npm start -- ~/Development/biblix-packages/bsb ~/Development/biblix-packages/kjv
node install.ts file://$HOME/Development/biblix-packages/kjv file://$HOME/Development/biblix-packages/bsb
python examples/python/packages.py ~/Development/biblix-packages/bsb ~/Development/biblix-packages/kjv
```

topos's crate is a path dependency (`../topos`), as for the rest of biblix.
