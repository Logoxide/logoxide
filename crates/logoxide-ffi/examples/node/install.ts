// Installs packages the way an app without git does: logoxide says what it needs, the host fetches
// it. Run with `npm install && node install.ts <git URLs>` after `npm run pack:wasm`.
//
// https URLs are downloaded with fetch(), as an Obsidian plugin would with requestUrl. For
// file:// URLs (local repositories, like ~/Development/logoxide-packages) git makes the same bytes
// an HTTP host serves: the refs (`git upload-pack --http-backend-info-refs`) and archives
// (`git archive`).
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, relative, sep } from "node:path";
import {
  Logoxide, LogoxideInstaller, BookStyle, OffsetUnit, Topos, ToposTranslations,
  type FetchRequest, type PackageFile, type PackageLocation,
} from "logoxide";

const logoxide = Logoxide.new();
const root = mkdtempSync(join(tmpdir(), "logoxide-install-"));
const library = join(root, "library");
const vault = join(root, "vault");
mkdirSync(vault, { recursive: true });

function files(dir: string, base = dir): PackageFile[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return files(path, base);
    return [{ path: relative(base, path).split(sep).join("/"), bytes: new Uint8Array(readFileSync(path)) }];
  });
}

async function get(url: string): Promise<Uint8Array> {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url}: ${response.status}`);
  return new Uint8Array(await response.arrayBuffer());
}

const local = (repository: string) => repository.startsWith("file://") ? repository.slice("file://".length) : null;

/** Answers one request, as a plugin would */
async function answer(installer: LogoxideInstaller, request: FetchRequest) {
  if (request.tag === "Refs") {
    const dir = local(request.repository);
    const bytes = dir ? execFileSync("git", ["upload-pack", "--http-backend-info-refs", dir]) : await get(request.url);
    console.log("  refs of", request.repository);
    installer.provideRefs(request.repository, new Uint8Array(bytes));
  } else if (request.tag === "Archive") {
    const dir = local(request.repository);
    let bytes: Uint8Array | null = dir ? new Uint8Array(execFileSync("git", ["-C", dir, "archive", "--format=tar.gz", request.commit], { maxBuffer: 1 << 30 })) : null;
    for (const url of dir ? [] : request.urls) {
      try { bytes = await get(url); break; } catch { /* the next host's form */ }
    }
    if (!bytes) throw new Error(`no archive of ${request.repository} at ${request.commit}`);
    console.log(`  archive of ${request.repository} at ${request.commit.slice(0, 8)} (${bytes.length} bytes)`);
    installer.provideArchive(request.repository, request.commit, bytes);
  } else {
    const dir = locationDir(request.location);
    if (existsSync(dir)) installer.provideFiles(request.location, files(dir));
    else installer.provideMissing(request.location);
  }
}

const locationDir = (location: PackageLocation) => location.tag === "Library" ? join(library, location.dir) : join(vault, location.dir);

async function install(installer: LogoxideInstaller, update = { tag: "Nothing" } as const) {
  const project = logoxide.parseProject(readFileSync(join(vault, "logoxide.toml"), "utf8"), existsSync(join(vault, "logoxide.lock")) ? readFileSync(join(vault, "logoxide.lock"), "utf8") : null);
  for (;;) {
    const step = installer.install(project, update);
    if (step.tag === "Fetch") { await answer(installer, step.request); continue; }
    for (const file of step.files) {
      mkdirSync(dirname(join(library, file.path)), { recursive: true });
      writeFileSync(join(library, file.path), file.bytes);
    }
    writeFileSync(join(vault, "logoxide.lock"), step.lock);
    return step.files.length;
  }
}

// Adding: inspect each repository (its newest release), then name it by its id. One installer
// keeps what it was given, so installing doesn't download the archives again
const installer = logoxide.installer();
let project = logoxide.parseProject(null, null);
for (const url of process.argv.slice(2)) {
  let step = installer.inspect(url, null, null, null);
  while (step.tag === "Fetch") { await answer(installer, step.request); step = installer.inspect(url, null, null, null); }
  const { info, tag } = step.inspected;
  console.log(`${info.abbreviation} ${info.version} (${info.license}) at ${tag}`);
  project = project.addDependency(info.id, { git: url, path: null, version: info.version, tag: null, branch: null, rev: null, features: [], defaultFeatures: true, packageId: null, abbreviation: null, aliases: null });
}
writeFileSync(join(vault, "logoxide.toml"), project.manifestText());

let start = performance.now();
console.log(`installed ${await install(installer)} files in ${Math.round(performance.now() - start)} ms`);
start = performance.now();
console.log(`again: ${await install(logoxide.installer())} files in ${Math.round(performance.now() - start)} ms (checked against the lock file)`);

// The vault's Bibles, from the library
const topos = Topos.new();
let translations = ToposTranslations.create();
const vaultProject = logoxide.parseProject(readFileSync(join(vault, "logoxide.toml"), "utf8"), readFileSync(join(vault, "logoxide.lock"), "utf8"));
for (const entry of vaultProject.packages()) {
  const pkg = logoxide.openPackage(files(locationDir(entry.location)), entry.features, []);
  console.log(`${entry.id} ${entry.version}: ${pkg.checksum() === entry.checksum ? "matches" : "DIFFERS FROM"} the lock file`);
  translations = pkg.addTo(translations);
}
console.log(topos.parseWithTranslation("John 3:16 KJV", OffsetUnit.Utf16, translations, BookStyle.Name)?.translation?.name);
console.log(readFileSync(join(vault, "logoxide.toml"), "utf8"));
console.log("vault and library in", root);
