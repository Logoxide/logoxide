# logoxide (bindings)

logoxide's packages with topos's whole API, in one package for TypeScript/WebAssembly, Python,
Swift, and Kotlin (through [BoltFFI](https://github.com/boltffi/boltffi)). `Topos`,
`ToposTranslations`, and the rest are the same as in topos's own package (`topos-bible`). A book's
text is logoxide's own `LogoxideBook` (`pkg.book(id)`, or `LogoxideBook.parse(topos, "usx", text)`
for a file of your own): its verses, and passages as HTML or plain text. It was topos's
`ToposBook`, with the same methods; its `BookVerse` and `BookTextOptions` keep their names.

The host reads and writes the files: an app's file API is often asynchronous (Obsidian's vault),
and WebAssembly has none. logoxide reads the text from the bytes it's given, and hands back the
cache files it wrote for the host to save.

```ts
import { Logoxide, Topos, ToposTranslations, BookStyle, OffsetUnit } from "logoxide";

const logoxide = Logoxide.new();
const topos = Topos.new();

// A vault's logoxide.toml and logoxide.lock say which packages it uses, and where their files are
const project = logoxide.parseProject(manifestText, lockText); // either can be null
let translations = ToposTranslations.create();
const packages = new Map();
for (const entry of project.packages()) {
  // entry.location: { tag: "Library", dir: "packages/bsb/1.0.1+g1a2b3c4d" } (in the library) or
  // { tag: "Path", dir } (relative to the vault)
  const pkg = logoxide.openPackage(await readAll(entry.location.dir), entry.features, await readCache());
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
- Errors are thrown as `LogoxideErrorException` (`error.value` is `{ tag, message }`; the tag is
  the core's kind: `NotFound`, `Io`, `Invalid`, `Network`, `Git`, `Archive`, `Integrity`,
  `Resolve`, `Busy`, `Conflict`, and `InvalidMarkup` for `LogoxideBook.parse`)

## Installing

There's no `git` in WebAssembly, so the host downloads over HTTP and logoxide asks for what it
needs: a repository's refs (git's smart HTTP, which GitHub, GitLab, Gitea, Forgejo, and Bitbucket
serve), an archive of a commit (`.tar.gz`), or the files of a package already installed. It
resolves exactly as `lx install` does, so a vault's `logoxide.lock` works with `lx` and back.

```ts
const installer = logoxide.installer(); // keep one: what it's given is kept
// Adding: read the repository's newest release, then name the dependency by its id
let step = installer.inspect(url, null, null, null);
while (step.tag === "Fetch") { await answer(step.request); step = installer.inspect(url, null, null, null); }
const { info } = step.inspected;
project = project.addDependency(info.id, { git: url, registry: null, version: info.version, path: null, tag: null,
  branch: null, rev: null, features: [], defaultFeatures: true, packageId: null, abbreviation: null, aliases: null });

let result = installer.install(project, { tag: "Nothing" }); // or { tag: "All" }, { tag: "Only", ids }
while (result.tag === "Fetch") { await answer(result.request); result = installer.install(project, { tag: "Nothing" }); }
for (const file of result.files) await write(`${library}/${file.path}`, file.bytes);
await write("logoxide.toml", project.manifestText());
await write("logoxide.lock", result.lock);

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

The project's catalogs (its `[settings] catalogs`, then the default ones) are read the same way,
as `lx search` reads them, each asked for as a `Download`:

```ts
let step = installer.loadCatalogs(project);
while (step.tag === "Fetch") {
  const { url, urls } = step.request as { url: string; urls: string[] };
  try { installer.provideDownload(url, await getFirst(urls)); }
  catch (e) { installer.provideDownloadFailed(url, String(e)); } // a warning, unless none can be read
  step = installer.loadCatalogs(project);
}
const catalog = installer.catalog(); // all of them merged; step.warnings, also catalog.warnings()
catalog.search("world english", 20);
```

A catalog given by path isn't read there (a warning says so): read it and merge
`parseCatalogBytes`'s.

## Reading natively

Where there are files (Python, Node), `readPassage` and `comparePassage` read a project's Bibles
from its folder and the library's, as `lx read` and `lx compare` do (not in WebAssembly):

```ts
const style = { verseLabel: "[{verse}]", chapterLabel: null, notes: false, headings: false, templates: [], width: null, limit: null };
const read = logoxide.readPassage(vault, library, cache, "John 3:16 KJV", style); // { bible, abbreviation, passage, text, versification }
// Each Bible in its own numbering, mapped through the project's versification maps
for (const c of logoxide.comparePassage(vault, library, cache, "Psalm 51:1", ["KJV", "BSB"], style))
  console.log(c.abbreviation, c.reference, c.text?.text, c.missing);
```

## Making packages

A package can be made from one file the person picks: an EPUB fills in its own title, subtitle,
authors, language, and publisher; a Bible's USX or USFM needs a name and a language. logoxide gives
back the package's files to write into a new folder (then a git repository, to release it):

```ts
const metadata = logoxide.fileMetadata(name, bytes); // to fill a form: { title, subtitle, authors, language, publisher, rights }
const files = logoxide.newPackage(name, bytes, {
  id: null, name: null, abbreviation: null, authors: [], description: null, version: "1.0.0",
  kind: null, language: null, license: "CC0-1.0", publisher: null, homepage: null,
  repository: null, keywords: [], categories: [], aboutBooks: ["ROM"], // a book's name or code
}); // logoxide.toml, and epub/<name>
for (const file of files) await write(`${folder}/${file.path}`, file.bytes);

// A collection: a package of packages, its dependencies added afterwards
const manifest = logoxide.newCollection({ ...given, name: "My Library", language: "en" });
```

- What's given wins over the file; the id comes from the name (`the-dorean-principle`), a Bible's
  or commentary's abbreviation from its initials (`BSB`; the other kinds have one only when
  given, so `PackageInfo.abbreviation` and `CatalogEntry.abbreviation` are `null` for a book or
  a collection without one: show the id then), the version is 0.1.0 unless given. A collection
  needs no `language` (its manifest reads as `und` without one)
- A book (`kind: Book`) has no verse text: `pkg.references()` reads its EPUBs for the Bible
  references in them (each a `passage` in OSIS, the `file`, its `chapter`, a `cfi` EPUB readers
  open, and its UTF-16 range, line, column, spine item, and paragraph: what
  `ToposIndex.setEpub` takes, with `topos.parseOsis(passage)`). `parseReferences` reads a
  release's `logoxide.references.bin` without its books: the passages cited, and how often
- A package is what its manifest declares, so files a reader adds beside it (annotations) don't
  change its checksum. `installer.install(project, { tag: "Repair" })` hands back the files of
  any installed version whose own files changed, to write over them
- `examples/node/book.ts` makes a package from a real EPUB and lists what it cites

## Building

```sh
npm install && npm run pack:wasm   # dist/wasm/pkg (npm: logoxide)
npm run pack:python                # dist/python/wheelhouse (PyPI: logoxide)
```

`examples/node` and `examples/python` open and install real packages:

```sh
cd examples/node && npm install && npm start -- ~/Development/logoxide-packages/bsb ~/Development/logoxide-packages/kjv
node install.ts file://$HOME/Development/logoxide-packages/kjv file://$HOME/Development/logoxide-packages/bsb
python examples/python/packages.py ~/Development/logoxide-packages/bsb ~/Development/logoxide-packages/kjv
```

topos's crate is a path dependency (`../topos`), as for the rest of logoxide.
