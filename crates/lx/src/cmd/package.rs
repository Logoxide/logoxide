//! Commands on a package's own folder: show, check, new, release, diff, verify

use std::{path::PathBuf, sync::Arc, time::Instant};

use logoxide::reader::DefaultReader;
use logoxide::{
    Cache, Features, FsStorage, MemoryStorage, Package, Storage, bible_data,
    manifest::{Dependency, MANIFEST},
};

use crate::{
    book_names,
    cli::{Cli, Format, NewArgs, PackageArgs},
    print_json,
};

pub(crate) fn show(
    cli: &Cli,
    args: &PackageArgs,
    references: Option<(&str, bool)>,
) -> logoxide::Result<()> {
    let package = args.open()?;
    let info = package.info();
    let manifest = package.manifest();
    outln!(
        "{} {} ({}), {}",
        info.id,
        info.version,
        info.abbreviation,
        info.name
    );
    if let Some(description) = &info.description {
        outln!("{description}");
    }
    if !info.authors.is_empty() {
        outln!("by {}", info.authors.join(", "));
    }
    let language = match &info.language_name {
        Some(name) => format!("{} ({name})", info.language),
        None => info.language.clone(),
    };
    let script = (info.text_script())
        .map(|s| format!(", script: {s}"))
        .unwrap_or_default();
    outln!(
        "kind: {:?}, language: {language}{script}, direction: {:?}",
        info.kind,
        info.text_direction()
    );
    outln!(
        "license: {}",
        info.license
            .as_deref()
            .or(info.license_file.as_deref())
            .unwrap_or("none given")
    );
    if let Some(work) = logoxide::works::describe(info) {
        outln!("work: {work}");
    }
    let rights = info.text_rights();
    if !rights.is_empty() {
        outln!("rights: {}", rights.describe());
    }
    if let Some(media) = manifest.media.as_ref().filter(|m| !m.files.is_empty()) {
        let bytes: u64 = media.files.iter().map(|f| f.size).sum();
        outln!(
            "media: {} files, {:.1} MB, listed by URL and hash",
            media.files.len(),
            bytes as f64 / 1e6
        );
    }
    if !info.about_books.is_empty() {
        outln!("about: {}", book_names(&info.about_books));
    }
    if !info.categories.is_empty() {
        outln!("categories: {}", info.categories.join(", "));
    }
    if !info.keywords.is_empty() {
        outln!("keywords: {}", info.keywords.join(", "));
    }
    for (label, link) in [
        ("homepage", &info.homepage),
        ("repository", &info.repository),
        ("documentation", &info.documentation),
        ("readme", &info.readme),
    ] {
        if let Some(link) = link {
            outln!("{label}: {link}");
        }
    }
    if let Some(upstream) = &info.upstream {
        let mut about = vec![];
        if let Some(import) = &upstream.import {
            about.push(import.clone());
        }
        if let Some(version) = &upstream.version {
            about.push(format!("the publisher's version {version}"));
        }
        let from = (upstream.url.as_deref())
            .or(upstream.note.as_deref())
            .unwrap_or("not said");
        match about.is_empty() {
            true => outln!("upstream: {from}"),
            false => outln!("upstream: {from} ({})", about.join(", ")),
        }
    }
    let enabled: Vec<&str> = manifest
        .formats(package.enabled())
        .iter()
        .map(|(n, _)| *n)
        .collect();
    let all: Vec<&str> = manifest.formats.keys().map(String::as_str).collect();
    outln!(
        "formats: {} (enabled: {}; reading {})",
        all.join(", "),
        enabled.join(", "),
        match package.text_format() {
            Some((format, _)) => format!("{format:?}"),
            None => String::from("no verse text"),
        }
    );
    let features: Vec<&str> = package
        .enabled()
        .features
        .iter()
        .map(String::as_str)
        .collect();
    outln!(
        "features: {}",
        if features.is_empty() {
            "none".into()
        } else {
            features.join(", ")
        }
    );
    for (name, dependency) in &manifest.dependencies {
        match dependency {
            Dependency::Version(req) => outln!("dependency {name}: {req}"),
            Dependency::Detailed(d) => outln!(
                "dependency {name}: {}{}",
                (d.git
                    .as_deref()
                    .or(d.registry.as_deref())
                    .or(d.path.as_deref()))
                .unwrap_or_default(),
                d.version
                    .as_ref()
                    .map(|v| format!(" ({v})"))
                    .unwrap_or_default()
            ),
        }
    }
    let books = package.books();
    let codes: Vec<&str> = books
        .iter()
        .filter_map(|b| bible_data().books().get_usfm(*b).map(String::as_str))
        .collect();
    if !books.is_empty() {
        outln!("books: {} ({})", books.len(), codes.join(" "));
    }
    outln!("checksum: {}", package.checksum()?);
    // Found once per version, then read from the cache
    let found = cli.library().book_cache().references(&package)?;
    if !found.is_empty() {
        outln!("references: {} (in its books)", found.len());
    }
    match references {
        Some((format, true)) => print_references(&found, format)?,
        Some((format, false)) => print_cited(&logoxide::references::cited(&found), format)?,
        None => {}
    }
    Ok(())
}

/// What `lx show` looks up in a package that isn't a Bible's text
pub(crate) enum Lookup<'a> {
    /// A commentary's notes on a passage (`--notes "John 3:16"`), stretched over the verses
    /// after each up to the next (`--stretch`)
    Notes(&'a str, bool),
    /// A dictionary's entry (`--entry G25`)
    Entry(&'a str),
    /// A dictionary's keys, the first so many (`--keys --limit 10`)
    Keys(Option<usize>),
    /// A verse's or passage's cross-references (`--xref "John 3:16"`)
    CrossReferences(&'a str),
}

/// The plain text of a note's or an entry's tree: everything but metadata (notes in brackets)
fn entry_text(nodes: &[logoxide_text::markup::Node]) -> String {
    use logoxide_text::markup::{Filter, Kind, TextOptions, plain_text};
    let options = TextOptions {
        filter: Filter::all().drop(Kind::Metadata),
        ..TextOptions::default()
    };
    plain_text(nodes, &options).trim().to_string()
}

/// A note's verses as a reference: `John 3:16`, `John 3:16-18`, `John 3:36-4:2`, `John 3:20a`
fn note_reference(book: &str, note: &logoxide_text::Note) -> String {
    let part = match note.part {
        0 => String::new(),
        n => char::from(b'a' + (n - 1).min(25)).to_string(),
    };
    let first = format!("{book} {}:{}{part}", note.chapter, note.verse);
    if note.last_chapter != note.chapter {
        format!("{first}-{}:{}", note.last_chapter, note.last)
    } else if note.last != note.verse {
        format!("{first}-{}", note.last)
    } else {
        first
    }
}

/// A reference as topos reads it (`John 3:16-18`, `Jn 3`), or an error naming it
fn parse_reference(reference: &str) -> logoxide::Result<topos_bible::segments::Passage> {
    bible_data().parse(reference).ok_or_else(|| {
        logoxide::LogoxideError::Manifest(format!("`{reference}` isn't a Bible reference"))
    })
}

/// `lx show --notes`, `--entry`, `--keys`, and `--xref`: what a commentary, a dictionary, or a
/// cross-reference package says, as text or JSON
pub(crate) fn lookup(args: &PackageArgs, lookup: Lookup, json: bool) -> logoxide::Result<()> {
    let package = args.open()?;
    let source = package.source();
    match lookup {
        Lookup::Notes(reference, stretch) => {
            let passage = parse_reference(reference)?;
            let books = bible_data().books();
            let name = (books.get_name(passage.book).cloned()).unwrap_or_default();
            // A book the package lacks has no notes (no error: a commentary may cover a few)
            let notes = match package.books().contains(&passage.book) {
                true => logoxide_text::source::book_notes(
                    &source.book(passage.book)?,
                    &passage,
                    stretch,
                ),
                false => vec![],
            };
            if json {
                let notes: Vec<serde_json::Value> = (notes.iter())
                    .map(|n| {
                        serde_json::json!({
                            "reference": note_reference(&name, n),
                            "chapter": n.chapter,
                            "verse": n.verse,
                            "last": n.last,
                            "lastChapter": n.last_chapter,
                            "part": n.part,
                            "text": entry_text(&n.nodes),
                        })
                    })
                    .collect();
                return print_json(&notes);
            }
            if notes.is_empty() {
                outln!("no notes on {reference}");
            }
            for (i, note) in notes.iter().enumerate() {
                if i > 0 {
                    outln!();
                }
                outln!("{}", note_reference(&name, note));
                outln!("{}", entry_text(&note.nodes));
            }
        }
        Lookup::Entry(key) => {
            let entry = source.entry(key)?;
            if json {
                let entry = entry.as_ref().map(|e| {
                    serde_json::json!({
                        "key": e.key,
                        "headword": e.headword,
                        "text": entry_text(&e.nodes),
                    })
                });
                return print_json(&entry);
            }
            let entry = entry.ok_or_else(|| {
                logoxide::LogoxideError::NotFound(format!(
                    "{}: no entry `{key}`",
                    package.info().id
                ))
            })?;
            match &entry.headword {
                Some(headword) => outln!("{} {headword}", entry.key),
                None => outln!("{}", entry.key),
            }
            outln!("{}", entry_text(&entry.nodes));
        }
        Lookup::Keys(limit) => {
            let mut keys = source.keys();
            if let Some(limit) = limit {
                keys.truncate(limit);
            }
            if json {
                return print_json(&keys);
            }
            for key in keys {
                outln!("{key}");
            }
        }
        Lookup::CrossReferences(reference) => {
            let passage = parse_reference(reference)?;
            let osis = (passage.to_osis(bible_data().books())).ok_or_else(|| {
                logoxide::LogoxideError::Manifest(format!(
                    "`{reference}` can't be written as an OSIS reference"
                ))
            })?;
            let found = source.cross_references(&osis)?;
            if json {
                let found: Vec<serde_json::Value> = (found.iter())
                    .map(|x| serde_json::json!({ "from": x.from, "to": x.to, "weight": x.weight }))
                    .collect();
                return print_json(&found);
            }
            if found.is_empty() {
                outln!("no cross-references for {osis}");
            }
            for x in found {
                match x.weight {
                    Some(weight) => outln!("{}\t{}\t{weight}", x.from, x.to),
                    None => outln!("{}\t{}", x.from, x.to),
                }
            }
        }
    }
    Ok(())
}

/// The passages cited, with how often, as text, TSV, or JSON
fn print_cited(cited: &[logoxide::references::Cited], format: &str) -> logoxide::Result<()> {
    match format {
        "json" => println!(
            "{}",
            serde_json::to_string_pretty(cited)
                .map_err(|e| logoxide::LogoxideError::Manifest(e.to_string()))?
        ),
        "tsv" => {
            outln!("passage\tcount");
            for c in cited {
                outln!("{}\t{}", c.passage, c.count);
            }
        }
        "text" => {
            for c in cited {
                match c.count {
                    1 => outln!("{}", c.passage),
                    n => outln!("{} ({n})", c.passage),
                }
            }
        }
        other => {
            return Err(logoxide::LogoxideError::Manifest(format!(
                "--format {other}: text, tsv, or json"
            )));
        }
    }
    Ok(())
}

/// Every reference, with where it is: as text (its passage, chapter, and paragraph, shortened),
/// TSV, or JSON
fn print_references(
    found: &[logoxide::references::FoundReference],
    format: &str,
) -> logoxide::Result<()> {
    match format {
        "json" => println!(
            "{}",
            serde_json::to_string_pretty(found)
                .map_err(|e| logoxide::LogoxideError::Manifest(e.to_string()))?
        ),
        "tsv" => {
            outln!("passage\tfile\tchapter\tcfi\tstart\tend\tline\tcolumn\tspine\tline_text");
            let clean = |s: &str| s.replace(['\t', '\n', '\r'], " ");
            for r in found {
                outln!(
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    r.passage,
                    r.file,
                    clean(r.chapter.as_deref().unwrap_or("")),
                    r.cfi,
                    r.start,
                    r.end,
                    r.line,
                    r.column,
                    r.spine,
                    clean(&r.line_text)
                );
            }
        }
        "text" => {
            for r in found {
                let text: String = r.line_text.split_whitespace().collect::<Vec<_>>().join(" ");
                let short: String = text.chars().take(80).collect();
                let more = if short.len() < text.len() { "…" } else { "" };
                outln!(
                    "{}  [{}]  {short}{more}",
                    r.passage,
                    r.chapter.as_deref().unwrap_or("-")
                );
            }
        }
        other => {
            return Err(logoxide::LogoxideError::Manifest(format!(
                "--format {other}: text, tsv, or json"
            )));
        }
    }
    Ok(())
}

fn is_zero(n: &usize) -> bool {
    *n == 0
}

/// What `lx check --format json` prints
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckReport {
    id: String,
    version: String,
    kind: logoxide::manifest::PackageKind,
    books: usize,
    verses: usize,
    /// A dictionary's or cross-references' entries (0 for other packages)
    #[serde(skip_serializing_if = "is_zero")]
    entries: usize,
    /// A versification map's: what it maps from and to, its lines, and the verses it maps
    #[serde(skip_serializing_if = "Option::is_none")]
    map: Option<serde_json::Value>,
    /// Each file's title (none when it has none)
    titles: Vec<serde_json::Value>,
    untitled: Vec<String>,
    /// Files, or books in them, passed over: peripherals, and books logoxide doesn't know
    skipped: Vec<logoxide::reader::Skipped>,
    /// Problems with the timings, and books whose cached counts differ
    problems: Vec<String>,
    /// What's allowed but should change: a language tag that isn't canonical
    warnings: Vec<String>,
    ok: bool,
}

/// What a package's format passed over: peripherals as notes, books logoxide doesn't know (and
/// files naming none) as warnings
pub(crate) fn print_skipped(skipped: &[logoxide::reader::Skipped]) {
    for skip in skipped {
        match skip.is_warning() {
            true => errln!("warning: {skip}"),
            false => errln!("note: {skip}"),
        }
    }
}

/// What's said of a versification map that it can still be used with: verses past its counts,
/// and lines naming books logoxide doesn't know (passed over)
fn map_warnings(map: &logoxide_text::versification::VersificationMap) -> Vec<String> {
    let mut warnings = map.problems();
    warnings
        .extend((map.passed_over().iter()).map(|(at, code)| {
            format!("{at}: {code} is a book logoxide doesn't know, passed over")
        }));
    warnings
}

pub(crate) fn check(
    args: &PackageArgs,
    cache: Option<PathBuf>,
    json: bool,
    library: &logoxide::Library,
) -> logoxide::Result<()> {
    let package = args.open()?;
    let (id, version) = (
        package.info().id.clone(),
        package.info().version.to_string(),
    );
    let mut problems = vec![];
    let skipped: Vec<logoxide::reader::Skipped> = (package.source().skipped().iter())
        .cloned()
        .map(Into::into)
        .collect();
    // A language tag that isn't canonical (`eng`): what it should be
    let warnings = package.manifest().warnings();
    if !json {
        print_skipped(&skipped);
        for warning in &warnings {
            errln!("warning: {warning}");
        }
    }
    let package_kind = package.info().kind;
    let text = crate::cmd::media::media_text(&args.path, &package, library);
    let timings = logoxide::media::check_package_timings_with(&package, text.as_ref())?;
    // What a recording without a text of its own reads
    let reads = (package.manifest().media.as_ref())
        .filter(|m| m.names_its_text())
        .map(|m| match (&m.text, &m.versification) {
            (Some(text), _) => format!("{text}'s text"),
            (None, Some(versification)) => format!("the {versification} versification"),
            (None, None) => String::new(),
        });
    let titles = logoxide::titles::titles(&package)?;
    // A map is read whole: its lines parse, name known books, and don't overlap
    let map = (package_kind == logoxide::manifest::PackageKind::VersificationMap)
        .then(|| logoxide::reader::versification_map(&package))
        .transpose()?;
    let start = Instant::now();
    let (mut verses, mut books) = (0, 0);
    // A dictionary's or cross-references' entries, each read (so its files parse)
    let mut entries = 0;
    if (package.text_format()).is_some_and(|(f, _)| !f.is_verse_text()) {
        for key in package.source().keys() {
            package.source().entry(&key)?;
            entries += 1;
        }
    }
    match cache {
        Some(dir) => {
            let cache = Cache::new(Arc::new(FsStorage::new(dir)));
            let cached = cache.open(package, &DefaultReader)?;
            for id in cached.package().books() {
                let book = cached.book(id)?;
                verses += book.spans.len();
                books += 1;
                let counts = cached
                    .manifest()
                    .books
                    .iter()
                    .find(|(b, _)| *b == id)
                    .map(|(_, c)| c);
                if counts != Some(&book.chapter_verses()) {
                    let problem = format!("{}: the cached counts differ from the book's", id.0);
                    if !json {
                        outln!("{problem}");
                    }
                    problems.push(problem);
                }
            }
        }
        None => {
            for id in package.books() {
                verses += package.source().book(id)?.spans.len();
                books += 1;
            }
        }
    }
    // Each file's title: a few are listed, many counted
    let untitled: Vec<&str> = (titles.iter())
        .filter(|t| t.title.is_none())
        .map(|t| t.path.as_str())
        .collect();
    let no_title = || {
        logoxide::LogoxideError::Manifest(format!(
            "no title for {} (give one in logoxide.toml's [titles])",
            untitled.join(", ")
        ))
    };
    if json {
        problems.extend(timings.iter().map(|p| format!("timings: {p}")));
        let ok = problems.is_empty() && untitled.is_empty();
        print_json(&CheckReport {
            id,
            version,
            kind: package_kind,
            books,
            verses,
            entries,
            map: map.as_ref().map(|map| {
                serde_json::json!({
                    "from": map.from,
                    "to": map.to,
                    "lines": map.mappings().len(),
                    "verses": map.len(),
                    "excluded": map.excluded().len(),
                    "warnings": map_warnings(map),
                })
            }),
            titles: (titles.iter())
                .map(|t| serde_json::json!({ "path": t.path, "title": t.title }))
                .collect(),
            untitled: untitled.iter().map(|p| p.to_string()).collect(),
            skipped,
            problems,
            warnings,
            ok,
        })?;
        timings_result(&timings)?;
        return match untitled.is_empty() {
            true => Ok(()),
            false => Err(no_title()),
        };
    }
    match (package_kind, map) {
        (_, Some(map)) => {
            for warning in map_warnings(&map) {
                errln!("warning: {warning}");
            }
            outln!(
                "a versification map from {} to {}: {} lines, {} verses mapped",
                map.from,
                map.to,
                map.mappings().len(),
                map.len()
            )
        }
        (logoxide::manifest::PackageKind::Collection, _) => {
            outln!("a collection: no text of its own, only its dependencies")
        }
        _ if books == 0 && reads.is_some() => outln!(
            "a recording of {}: no text of its own",
            reads.unwrap_or_default()
        ),
        _ if entries > 0 => outln!("{entries} entries, read in {:.0?}", start.elapsed()),
        _ if books == 0 => {
            outln!("no verse text (a {package_kind:?}): its files are kept as they are")
        }
        // A commentary's verse spans are its notes, each read
        (logoxide::manifest::PackageKind::Commentary, _) => outln!(
            "{books} books, {verses} notes, read in {:.0?}",
            start.elapsed()
        ),
        _ => outln!(
            "{books} books, {verses} verses, read in {:.0?}",
            start.elapsed()
        ),
    }
    let titled: Vec<String> = (titles.iter())
        .filter_map(|t| Some(format!("{} ({})", t.title.as_deref()?, t.path)))
        .collect();
    match titled.len() {
        0 => {}
        1..=5 => outln!("titles: {}", titled.join(", ")),
        n => outln!("{n} files titled: {} … {}", titled[0], titled[n - 1]),
    }
    timings_report(&timings)?;
    if !untitled.is_empty() {
        return Err(no_title());
    }
    Ok(())
}

/// Prints a package's timing problems; an error if it has any
pub(crate) fn timings_report(problems: &[String]) -> logoxide::Result<()> {
    for problem in problems {
        outln!("timings: {problem}");
    }
    timings_result(problems)
}

/// An error if a package has timing problems
fn timings_result(problems: &[String]) -> logoxide::Result<()> {
    match problems.len() {
        0 => Ok(()),
        n => Err(logoxide::LogoxideError::Media(format!(
            "{n} problem{} with the timings",
            if n == 1 { "" } else { "s" }
        ))),
    }
}

pub(crate) fn new(args: &NewArgs) -> logoxide::Result<()> {
    use logoxide::{create, manifest::PackageKind};
    let kind = (args.kind.as_deref())
        .map(|k| {
            serde_json::from_value::<PackageKind>(serde_json::Value::String(k.to_string()))
                .map_err(|_| {
                    logoxide::LogoxideError::Manifest(format!(
                        "--kind {k}: bible, commentary, lexicon, cross-references, versification-map, book, or collection"
                    ))
                })
        })
        .transpose()?;
    // Like `cargo new <dir>`: the folder names it, unless --id does
    let from_dir = (args.dir.as_ref())
        .and_then(|d| d.file_name())
        .map(|n| create::id_from(&n.to_string_lossy()));
    let given = create::NewPackage {
        id: args.id.clone().or(from_dir),
        name: args.name.clone(),
        abbreviation: args.abbreviation.clone(),
        authors: args.authors.clone(),
        description: args.description.clone(),
        version: (args.version.as_deref())
            .map(|v| {
                v.parse()
                    .map_err(|e| logoxide::LogoxideError::Manifest(format!("--version {v}: {e}")))
            })
            .transpose()?,
        kind,
        language: args.language.clone(),
        license: args.license.clone(),
        publisher: args.publisher.clone(),
        homepage: args.homepage.clone(),
        repository: args.repository.clone(),
        keywords: args.keywords.clone(),
        categories: args.categories.clone(),
        about_books: args.about_books.clone(),
    };
    let staged = MemoryStorage::new();
    let (manifest, rights) = match (&args.from, kind) {
        (Some(file), _) => {
            let bytes = std::fs::read(file)
                .map_err(|e| logoxide::LogoxideError::io(&file.display().to_string(), e))?;
            let name = file.file_name().map(|n| n.to_string_lossy().into_owned());
            let name = name.unwrap_or_default();
            let rights = match create::file_format(&name, &bytes) {
                Some(("epub", _)) => logoxide_text::metadata::epub_metadata(&bytes)?.rights,
                _ => None,
            };
            (
                create::new_from_file(&name, &bytes, &given, &staged)?,
                rights,
            )
        }
        (None, Some(PackageKind::Collection)) => (create::new_collection(&given, &staged)?, None),
        (None, _) => {
            return Err(logoxide::LogoxideError::Manifest(String::from(
                "give the file it's made from (--from), or --kind collection",
            )));
        }
    };
    let info = manifest
        .package
        .as_ref()
        .ok_or_else(|| logoxide::LogoxideError::Manifest(String::from("no [package]")))?;
    let dir = args.dir.clone().unwrap_or_else(|| PathBuf::from(&info.id));
    if dir.exists() && std::fs::read_dir(&dir).map_or(true, |mut d| d.next().is_some()) {
        return Err(logoxide::LogoxideError::Manifest(format!(
            "{} already exists",
            dir.display()
        )));
    }
    let dest = FsStorage::new(&dir);
    for path in staged.walk("")? {
        dest.write(&path, &staged.read(&path)?)?;
    }
    if !args.no_git {
        let status = std::process::Command::new("git")
            .args(["init", "-q"])
            .current_dir(&dir)
            .status();
        if !status.is_ok_and(|s| s.success()) {
            errln!("warning: `git init` failed in {}", dir.display());
        }
    }
    outln!("created {} ({:?}) in {}", info.id, info.kind, dir.display());
    out!("{}", staged.read_string(MANIFEST)?);
    if info.license.is_none() {
        outln!(
            "no license given: add one with --license (an SPDX expression, CC0-1.0); it's shown, never enforced"
        );
    }
    if let Some(rights) = rights {
        outln!("the file says of its rights: {rights}");
    }
    match info.kind {
        PackageKind::Collection => outln!(
            "next: lx add --project {} <each package's git URL>, then lx release there",
            dir.display()
        ),
        _ => outln!(
            "next: lx release in {} (commits and tags it)",
            dir.display()
        ),
    }
    Ok(())
}

fn print_summary(summary: &logoxide::summary::Summary) {
    match summary.verses {
        0 => outln!("{} {}", summary.id, summary.version),
        verses => outln!(
            "{} {}: {} books, {verses} verses",
            summary.id,
            summary.version,
            summary.books.len(),
        ),
    }
    let codes: Vec<&str> = summary.books.keys().map(String::as_str).collect();
    if !codes.is_empty() {
        outln!("books: {}", codes.join(" "));
    }
    for (format, size) in &summary.sizes {
        outln!("{format}: {:.1} MB", *size as f64 / 1_000_000.0);
    }
    let contents: Vec<String> = summary
        .contents
        .iter()
        .map(|(k, n)| format!("{k} {n}"))
        .collect();
    if !contents.is_empty() {
        outln!("contents: {}", contents.join(", "));
    }
    if summary.references > 0 {
        outln!("references: {} (in its books)", summary.references);
    }
    outln!("text checksum: {}", summary.text_checksum);
}

pub(crate) fn show_remote(
    cli: &Cli,
    url: &str,
    tag: Option<&str>,
    references: Option<&str>,
) -> logoxide::Result<()> {
    let fetcher = cli.fetcher();
    let tag = match tag {
        Some(tag) => tag.to_string(),
        None => {
            let tags = fetcher.tags(url)?;
            let newest = tags
                .into_iter()
                .filter_map(|(t, _)| Some((logoxide::resolve::tag_version(&t)?, t)))
                .max();
            newest
                .map(|(_, t)| t)
                .ok_or_else(|| logoxide::LogoxideError::Resolve(format!("{url} has no releases")))?
        }
    };
    let paths = [
        MANIFEST,
        logoxide::summary::SUMMARY,
        logoxide::summary::VERSES,
        logoxide::summary::ENTRIES,
        logoxide::references::REFERENCES,
    ];
    let files = MemoryStorage::new();
    fetcher.fetch_files(
        url,
        &logoxide::git::GitRef::Tag(tag.clone()),
        &paths,
        &files,
    )?;
    let manifest = logoxide::Manifest::parse(&files.read_string(MANIFEST)?)?;
    let summary = logoxide::summary::Summary::read(&files).map(|(s, _)| s);
    if let Some(info) = &manifest.package {
        outln!(
            "{} ({}), {}",
            info.abbreviation,
            info.name,
            info.license.as_deref().unwrap_or("no license given")
        );
    }
    match summary {
        Ok(summary) => {
            print_summary(&summary);
            if let Some(format) = references {
                print_cited(&summary.cited, format)?;
            }
        }
        Err(e @ logoxide::LogoxideError::Newer(_)) => outln!("{tag}'s summary: {e}"),
        Err(_) => outln!(
            "{tag} has no summary (released before `lx release`): install it to see what it holds"
        ),
    }
    Ok(())
}

pub(crate) fn release(
    cli: &Cli,
    path: &std::path::Path,
    set_version: bool,
    dry_run: bool,
) -> logoxide::Result<()> {
    let library = cli.library();
    let git = cli.git(&library);
    let mut plan = logoxide::release::plan(path, &git, &DefaultReader)?;
    print_skipped(&plan.skipped);
    if let Some((old, d)) = &plan.previous {
        outln!(
            "since {old}: {} ({:?} change)",
            logoxide::release::describe(d),
            d.change()
        );
        for v in d.changed.iter().take(10) {
            outln!("  reworded {}", v.reference());
        }
        for (from, to) in d.moved.iter().take(10) {
            outln!("  {} is now {}", from.reference(), to.reference());
        }
        for (from, to) in d.joined.iter().take(10) {
            outln!("  {} is joined into {}", from.reference(), to.reference());
        }
        for v in d.removed.iter().take(10) {
            outln!("  removed {}", v.reference());
        }
    }
    if logoxide::release::check(&plan).is_err()
        && set_version
        && let Some(minimum) = plan.minimum.clone()
    {
        let manifest_path = path.join(MANIFEST);
        let text = std::fs::read_to_string(&manifest_path)
            .map_err(|e| logoxide::LogoxideError::io(&manifest_path.display().to_string(), e))?;
        let mut doc: toml_edit::DocumentMut = text
            .parse()
            .map_err(|e: toml_edit::TomlError| logoxide::LogoxideError::Manifest(e.to_string()))?;
        doc["package"]["version"] = toml_edit::value(minimum.to_string());
        if !dry_run {
            logoxide::storage::write_atomic(&manifest_path, doc.to_string().as_bytes()).map_err(
                |e| logoxide::LogoxideError::io(&manifest_path.display().to_string(), e),
            )?;
        }
        outln!("version set to {minimum}");
        plan.summary.version = minimum;
        // The manifest changed: summarize again (its checksum covers logoxide.toml)
        if !dry_run {
            plan = logoxide::release::plan(path, &git, &DefaultReader)?;
        }
    }
    logoxide::release::check(&plan)?;
    if dry_run {
        let (_, prefix) = git.package_in(path)?;
        let tag = logoxide::release::tag_for(&prefix, &plan.summary.version);
        outln!("would release {tag}");
        return Ok(());
    }
    let (tag, commit) = logoxide::release::release(path, &plan, &git)?;
    outln!("released {tag} ({})", &commit[..commit.len().min(12)]);
    Ok(())
}

/// What changed between two versions, as `--format json` prints it (`lx diff`, `lx import
/// --update`)
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiffReport {
    /// In words: `3 verses reworded, 1 removed`
    description: String,
    /// `patch`, `minor`, or `major`
    change: String,
    reworded: Vec<String>,
    /// Each verse's old reference and its new one
    moved: Vec<[String; 2]>,
    removed: Vec<String>,
    added: Vec<String>,
    /// Each verse now inside another's bridge, and that bridge
    joined: Vec<[String; 2]>,
    /// Bridges split, each number still a verse: the new verse at the bridge's number
    split: Vec<String>,
    /// Bridges that lost a number
    bridged: Vec<String>,
    /// Files whose title changed: each one's path, old title, and new title
    retitled: Vec<[String; 3]>,
    /// For packages without verses: whether their files changed
    files_changed: bool,
}

impl DiffReport {
    pub(crate) fn of(d: &logoxide::summary::Diff) -> DiffReport {
        let references = |verses: &[logoxide::summary::VerseHash]| -> Vec<String> {
            verses.iter().map(|v| v.reference()).collect()
        };
        DiffReport {
            description: logoxide::release::describe(d),
            change: format!("{:?}", d.change()).to_lowercase(),
            reworded: references(&d.changed),
            moved: (d.moved.iter())
                .map(|(from, to)| [from.reference(), to.reference()])
                .collect(),
            removed: references(&d.removed),
            added: references(&d.added),
            joined: (d.joined.iter())
                .map(|(from, to)| [from.reference(), to.reference()])
                .collect(),
            split: references(&d.split),
            bridged: references(&d.bridged),
            retitled: (d.retitled.iter())
                .map(|(path, old, new)| [path.clone(), old.clone(), new.clone()])
                .collect(),
            files_changed: d.files_changed,
        }
    }
}

pub(crate) fn diff(
    cli: &Cli,
    path: &std::path::Path,
    from: &str,
    to: Option<&str>,
) -> logoxide::Result<()> {
    let library = cli.library();
    let git = cli.git(&library);
    // A package in a folder of its repository: its tags are `<folder>/v…`
    let (root, prefix) = git.package_in(path)?;
    let url = root.display().to_string();
    let tag = |t: &str| match t.starts_with(&prefix) {
        true => t.to_string(),
        false => format!("{prefix}{t}"),
    };
    let old = logoxide::release::summary_in(&git, &url, &tag(from), &prefix, &DefaultReader)?;
    let new = match to {
        Some(to) => logoxide::release::summary_in(&git, &url, &tag(to), &prefix, &DefaultReader)?,
        None => {
            let storage: Arc<dyn Storage> = Arc::new(FsStorage::new(path));
            logoxide::summary::summarize(
                &Package::open(storage, Features::default())?,
                &DefaultReader,
            )?
        }
    };
    let d = logoxide::summary::diff_summaries((&old.0, &old.1), (&new.0, &new.1));
    if cli.format == Format::Json {
        return print_json(&DiffReport::of(&d));
    }
    outln!(
        "{}: a {:?} change",
        logoxide::release::describe(&d),
        d.change()
    );
    for v in &d.changed {
        outln!("reworded {}", v.reference());
    }
    for (from, to) in &d.moved {
        outln!("{} is now {}", from.reference(), to.reference());
    }
    for v in &d.removed {
        outln!("removed {}", v.reference());
    }
    for v in &d.added {
        outln!("added {}", v.reference());
    }
    for (from, to) in &d.joined {
        outln!("{} is joined into {}", from.reference(), to.reference());
    }
    for v in &d.split {
        outln!("split {}", v.reference());
    }
    for v in &d.bridged {
        outln!("bridged differently {}", v.reference());
    }
    Ok(())
}

pub(crate) fn verify(args: &PackageArgs, json: bool) -> logoxide::Result<()> {
    let package = args.open()?;
    let (built, verses) = logoxide::summary::summarize(&package, &DefaultReader)?;
    let (published, published_verses) =
        logoxide::summary::Summary::read(package.storage().as_ref()).map_err(|e| match e {
            // Written by a newer lx: this one can't say whether it matches
            e @ logoxide::LogoxideError::Newer(_) => e,
            e => logoxide::LogoxideError::Manifest(format!(
                "no published summary ({e}): run `lx release`"
            )),
        })?;
    let mut problems = vec![];
    if published.verse_hash != built.verse_hash {
        // An older rule's hashes differ in every verse: comparing them says nothing
        problems.push(format!(
            "its verse hashes were made with rule {}, before this lx's {}: run `lx release` to write them again",
            published.verse_hash, built.verse_hash
        ));
    } else if built != published {
        problems.push(String::from("logoxide.summary.toml doesn't match the text"));
    }
    if published.verse_hash == built.verse_hash && verses != published_verses {
        let d = logoxide::summary::diff(&published_verses, &verses);
        problems.push(format!(
            "logoxide.verses.bin doesn't match the text: {}",
            logoxide::release::describe(&d)
        ));
    }
    if json {
        print_json(&serde_json::json!({
            "id": built.id,
            "version": built.version.to_string(),
            "matches": problems.is_empty(),
            "problems": problems,
        }))?;
    }
    if problems.is_empty() {
        if json {
            return Ok(());
        }
        outln!(
            "{} {}: the summary matches its text",
            built.id,
            built.version
        );
        Ok(())
    } else {
        Err(logoxide::LogoxideError::Manifest(problems.join("; ")))
    }
}
