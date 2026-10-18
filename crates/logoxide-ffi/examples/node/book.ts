// Makes a package from a book, as an app would from a file the person picks: what the EPUB says
// about itself fills the form, the package's files come back to write, and opening it finds the
// Bible references in it. Run with `npm install && node book.ts <file.epub> [license]` after
// `npm run pack:wasm`.
import { readFileSync } from "node:fs";
import { basename } from "node:path";
import { Logoxide } from "logoxide";

const [path, license = "CC0-1.0"] = process.argv.slice(2);
if (!path) throw new Error("give an EPUB: node book.ts <file.epub> [license]");
const logoxide = Logoxide.new();
const bytes = new Uint8Array(readFileSync(path));

// The form, filled in from the file
const metadata = logoxide.fileMetadata(basename(path), bytes);
console.log(`${metadata.title}${metadata.subtitle ? `: ${metadata.subtitle}` : ""}`);
console.log(`by ${metadata.authors.join(", ")} (${metadata.language})`);

// The package's files (the host writes them into a folder, then makes it a git repository)
let start = performance.now();
const files = logoxide.newPackage(basename(path), bytes, {
  id: null, name: null, abbreviation: null, authors: [], description: null, version: "1.0.0",
  kind: null, language: null, license, publisher: null, homepage: null, repository: null,
  keywords: [], categories: [], aboutBooks: [],
});
console.log(`made ${files.map((f) => f.path).join(", ")} in ${(performance.now() - start).toFixed(0)} ms`);
console.log(new TextDecoder().decode(files.find((f) => f.path === "logoxide.toml")!.bytes));

// Its references, by passage
start = performance.now();
const pkg = logoxide.openPackage(files, { features: [], defaultFeatures: true }, []);
const references = pkg.references();
console.log(`${references.length} references in ${(performance.now() - start).toFixed(0)} ms`);
const counts = new Map<string, number>();
for (const r of references) counts.set(r.passage, (counts.get(r.passage) ?? 0) + 1);
const top = [...counts].sort((a, b) => b[1] - a[1]).slice(0, 5);
console.log(`most cited: ${top.map(([passage, n]) => `${passage} (${n})`).join(", ")}`);
const first = references[0];
if (first) console.log(`first: ${first.passage} in "${first.chapter ?? "?"}", ${first.cfi}`);
pkg.dispose();
