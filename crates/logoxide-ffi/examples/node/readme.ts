// Runs every TypeScript example in ../../README.md, in order, as one program, and checks what each
// prints against its `// → ` comments (one for each console.log, in order; `…` stands for any
// text). Run with `npm install && node readme.ts [packages] [--show]` after `npm run pack:wasm`, where
// `packages` holds the KJV and BSB packages (git repositories, as in ~/Development/logoxide-packages).
//
// The examples run in a new folder holding `kjv` and `bsb` (links to them) and the files in
// ../fixtures. Their downloads are answered here, offline, as the hosts would answer them:
// `https://example.org/kjv` and `/bsb` are those repositories (their refs, and archives made by
// git), `https://example.org/catalog.toml` and eBible.org's list and zip are ../fixtures' files.
import { execFileSync } from "node:child_process";
import { cpSync, mkdtempSync, readFileSync, readdirSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { homedir, tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { format } from "node:util";

const here = dirname(fileURLToPath(import.meta.url));
const packages = resolve(process.argv[2] ?? join(homedir(), "Development/logoxide-packages"));
const fixtures = join(here, "../fixtures");
const readme = readFileSync(join(here, "../../README.md"), "utf8");
const show = process.argv.includes("--show");

// The examples, their imports gathered into one statement for each module
const blocks = [...readme.matchAll(/^```ts\n([\s\S]*?)^```$/gm)].map((m) => m[1]);
const imports = new Map<string, Set<string>>();
const bodies = blocks.map((block) =>
  block.replace(/^import \{([^}]*)\} from "([^"]+)";\n/gm, (_, names: string, from: string) => {
    const set = imports.get(from) ?? new Set();
    for (const name of names.split(",").map((n) => n.trim()).filter(Boolean)) set.add(name);
    imports.set(from, set);
    return "";
  }),
);
const resolved = (from: string) => (from === "logoxide" ? import.meta.resolve("logoxide") : from);
const program = [
  ...[...imports].map(([from, names]) => `import { ${[...names].join(", ")} } from "${resolved(from)}";`),
  ...bodies.map((body, i) => `globalThis.__block(${i});\n${body}`),
].join("\n");

// The downloads, answered offline
const repository = (name: string) => (["kjv", "bsb"].includes(name) ? join(packages, name) : null);
function served(url: string): Uint8Array | null {
  let m = url.match(/^https:\/\/example\.org\/(\w+?)(?:\.git)?\/info\/refs\?service=git-upload-pack$/);
  if (m && repository(m[1])) return execFileSync("git", ["upload-pack", "--http-backend-info-refs", repository(m[1])!]);
  m = url.match(/^https:\/\/example\.org\/(\w+)\/archive\/([0-9a-f]{40})\.tar\.gz$/);
  if (m && repository(m[1])) {
    const args = ["-C", repository(m[1])!, "archive", "--format=tar.gz", `--prefix=${m[1]}-${m[2]}/`, m[2]];
    return execFileSync("git", args, { maxBuffer: 1 << 30 });
  }
  const file = {
    "https://example.org/catalog.toml": "catalog.toml",
    "https://ebible.org/Scriptures/translations.csv": "translations.csv",
    "https://ebible.org/Scriptures/eng-asv_usfm.zip": "eng-asv_usfm.zip",
    "https://example.org/sermons-on-love/sermons-on-love-1.0.0.tar.gz": "sermons-on-love-1.0.0.tar.gz",
  }[url];
  return file ? readFileSync(join(fixtures, file)) : null;
}
globalThis.fetch = async (input: string | URL | Request) => {
  const url = input instanceof Request ? input.url : String(input);
  const bytes = served(url);
  return bytes ? new Response(bytes) : new Response("Not Found", { status: 404, statusText: "Not Found" });
};

// What each example prints
const printed: string[][] = blocks.map(() => []);
let current = 0;
(globalThis as any).__block = (i: number) => (current = i);
const log = console.log;
// Text is compared a line at a time (blank lines left out), anything else as one line
console.log = (...args: unknown[]) => {
  const text = format(...args);
  if (args.every((a) => typeof a === "string")) printed[current].push(...text.split("\n").filter((l) => l.trim()));
  else printed[current].push(text);
};

const dir = mkdtempSync(join(tmpdir(), "logoxide-readme-"));
for (const name of ["kjv", "bsb"]) symlinkSync(join(packages, name), join(dir, name));
for (const name of readdirSync(fixtures)) cpSync(join(fixtures, name), join(dir, name));
writeFileSync(join(dir, "readme.mts"), program);
process.chdir(dir);
try {
  await import(pathToFileURL(join(dir, "readme.mts")).href);
} finally {
  console.log = log;
}

const flat = (s: string) => s.replace(/\s+/g, " ").trim();
function matches(expected: string, actual: string) {
  if (expected === actual) return true;
  const parts = expected.split("…");
  if (parts.length === 1) return false;
  if (!actual.startsWith(parts[0]) || !actual.endsWith(parts[parts.length - 1])) return false;
  let at = parts[0].length;
  for (const part of parts.slice(1, -1)) {
    const found = actual.indexOf(part, at);
    if (found < 0) return false;
    at = found + part.length;
  }
  return at <= actual.length - parts[parts.length - 1].length;
}

let failed = 0;
blocks.forEach((block, i) => {
  const expected = [...block.matchAll(/\/\/ → (.*)$/gm)].map((m) => flat(m[1]));
  const actual = printed[i].map(flat);
  const line = readme.slice(0, readme.indexOf(block)).split("\n").length;
  const ok = expected.length === actual.length && expected.every((e, j) => matches(e, actual[j]));
  if (!ok) failed++;
  if (!ok || show) {
    log(`${ok ? "ok" : "FAILED"}: the example at README.md:${line}`);
    for (let j = 0; j < Math.max(expected.length, actual.length); j++)
      log(`  ${expected[j] === undefined ? "(nothing)" : expected[j]}\n  ${actual[j] === undefined ? "(nothing printed)" : actual[j]}`);
  }
});
process.chdir(here);
if (failed) log(`${failed} of ${blocks.length} examples print otherwise than README.md says (run in ${dir})`);
else {
  rmSync(dir, { recursive: true, force: true });
  log(`all ${blocks.length} examples print what README.md says`);
}
process.exit(failed ? 1 : 0);
