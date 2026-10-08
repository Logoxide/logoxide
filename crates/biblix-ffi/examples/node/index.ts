// Run with `npm install && npm start -- <package folders>` after `npm run pack:wasm` (see
// ../../README.md). In Node and Bun the WebAssembly module is ready on import (browsers: `await
// initialized` first). The host reads and writes the files, as an Obsidian plugin does through its
// vault; biblix reads the text.
import { readFileSync, readdirSync } from "node:fs";
import { join, relative, sep } from "node:path";
import { Biblix, BiblixErrorException, BookStyle, OffsetUnit, Topos, ToposTranslations, type BiblixPackage, type PackageFile } from "biblix";

/** Every file under a folder but git's */
function files(dir: string, root = dir): PackageFile[] {
  return readdirSync(dir, { withFileTypes: true })
    .filter((entry) => entry.name !== ".git")
    .flatMap((entry) => {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) return files(path, root);
      return [{ path: relative(root, path).split(sep).join("/"), bytes: new Uint8Array(readFileSync(path)) }];
    });
}

const biblix = Biblix.new();
console.log("biblix", biblix.version());
const topos = Topos.new();
const defaults = { features: [], defaultFeatures: true };
const cache: PackageFile[] = [];
let translations = ToposTranslations.create();
const packages: BiblixPackage[] = [];

for (const dir of process.argv.slice(2)) {
  const start = performance.now();
  const pkg = biblix.openPackage(files(dir), defaults, cache);
  cache.push(...pkg.cacheWrites()); // a plugin saves these and gives them back next time
  const info = pkg.info();
  console.log(`${info.abbreviation} ${info.version} (${pkg.format()}, ${info.license}): ${pkg.books().length} books in ${Math.round(performance.now() - start)} ms`);
  translations = pkg.addTo(translations);
  packages.push(pkg);
}

for (const reference of ["John 3:16 KJV", "3 John 1:14 BSB", "3 John 1:15 BSB"]) {
  const found = topos.parseWithTranslation(reference, OffsetUnit.Utf16, translations, BookStyle.Name);
  console.log(`${reference}:`, found ? `${found.passage.reference} in ${found.translation?.name}` : "no such verse");
}

const john = topos.parse("John 3:16", BookStyle.Name)!;
for (const pkg of packages) {
  const html = pkg.book(john.bookId).html(john, { notes: false, headings: false, classPrefix: "topos-" });
  console.log(`${pkg.info().abbreviation}:`, html[0].slice(0, 120));
}

// A vault's manifest and lock file say which packages to read, and from where
const project = biblix.parseProject(
  `[dependencies]\nbsb = { git = "https://example.org/bsb", version = "1" }\n\n[settings]\ndefault = "bsb"\n`,
  `version = 1\ndependencies = ["bsb 1.0.1"]\n\n[[package]]\nid = "bsb"\nversion = "1.0.1"\ngit = "https://example.org/bsb"\ntag = "v1.0.1"\ncommit = "0123456789"\n`,
);
console.log(project.defaultBible(), project.packages().map((p) => p.location));

// Errors are thrown with what went wrong
try {
  biblix.openPackage([{ path: "notes.md", bytes: new Uint8Array() }], defaults, []);
} catch (error) {
  if (!(error instanceof BiblixErrorException)) throw error;
  console.log("not a package:", error.value); // { tag: "NotFound", message: "biblix.toml: not found" }
}
