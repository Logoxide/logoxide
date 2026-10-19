/*!
Bundles a translator or publisher hands over, read by the metadata that lists their files:
a [Scripture Burrito](https://docs.burrito.bible/en/v1.0.0/)'s `metadata.json` (`burrito:`; what
Paratext, Scribe, Autographa, and Pankosmia write) or a Digital Bible Library text release
bundle's `metadata.xml` (`dbl:`; Burrito's ancestor, the same fields as XML). Neither publisher
has a list to search: an import names where the bundle is, a URL (`burrito:https://…/x.zip`) or a
file or folder (`burrito:file:///home/me/x.zip`), and the bundle names itself.

Its text files (USX, else USFM, else USJ, as its metadata lists them, each checked by its size and
MD5) are the package's format folder, under its file name; the metadata gives the package's name,
abbreviation, language, direction, license, and publisher, its revision the upstream version.
Only a Bible's text is read (Burrito's `scripture/textTranslation`, a DBL bundle of `text`): an
audio, video, typeset, or braille bundle, glosses, and alignments are refused by name.

The import's catalog entry is made from its name ([`entry`]), and only says where the bundle is:
everything else comes from the bundle when it's built ([`import`], or [`bundle_entry`] before
adding it, for the dependency's name), so the same bytes make the same package wherever it's made.
*/

use logoxide::{
    LogoxideError, Result,
    catalog::{Catalog, CatalogEntry, Download},
    language,
    manifest::{Direction, PackageKind},
    storage::{MemoryStorage, Storage},
};
use serde_json::Value;

use crate::{Made, external, finish, package_id};

/// The import prefixes this reads: a Scripture Burrito's, a DBL bundle's
pub const PREFIXES: [&str; 2] = ["burrito", "dbl"];

/// What a bundle's import downloads: the bundle, a `.zip` or `.tar.gz` (a folder's files are
/// given instead as `files/<path>`)
const DOWNLOAD: &str = "bundle";

/// Whether `import` is a bundle's (`burrito:…`, `dbl:…`)
pub fn is_bundle(import: &str) -> bool {
    (import.split_once(':')).is_some_and(|(prefix, _)| PREFIXES.contains(&prefix))
}

/**
The entry for a bundle's import (`burrito:https://example.org/web.zip`), from its name alone:
where the bundle is, its download; its id and name for now from the location's last part, until
the bundle says ([`import`] names the package from its metadata). None for another publisher's
import, or a location that's neither `https://`, `http://`, nor `file://`
*/
pub fn entry(import: &str) -> Option<CatalogEntry> {
    let (prefix, location) = import.split_once(':')?;
    if !PREFIXES.contains(&prefix) || !is_location(location) {
        return None;
    }
    let last = (location.trim_end_matches('/').rsplit('/').next())
        .filter(|l| !l.is_empty())
        .unwrap_or(prefix);
    let stem = (last
        .strip_suffix(".tar.gz")
        .or_else(|| last.strip_suffix(".zip")))
    .unwrap_or(last);
    let downloads = vec![Download {
        name: String::from(DOWNLOAD),
        urls: vec![location.to_string()],
    }];
    let mut source = external(prefix, location, false, downloads);
    // A file on this machine is no download anyone else can make it from: only its import names it
    if location.starts_with("file://")
        && let logoxide::catalog::CatalogSource::External { upstream, .. } = &mut source
    {
        *upstream = None;
    }
    Some(CatalogEntry {
        id: package_id(stem),
        name: stem.to_string(),
        abbreviation: None,
        aliases: vec![],
        language: String::from("und"),
        language_name: None,
        script: None,
        description: None,
        license: None,
        rights: Default::default(),
        copyright: None,
        version: None,
        kind: PackageKind::Bible,
        direction: Direction::Ltr,
        // A bundle's revision is known only once it's downloaded
        source,
        path: None,
        releases: vec![],
        keywords: vec![],
        categories: vec![],
        about_books: vec![],
        work: None,
        edition: None,
        edition_year: None,
        keys: None,
    })
}

/// Where a bundle can be: a URL a host downloads, or a local file or folder as a `file://` URL
fn is_location(location: &str) -> bool {
    ["https://", "http://", "file://"]
        .iter()
        .any(|scheme| location.len() > scheme.len() && location.starts_with(scheme))
}

/// A catalog of the one entry a bundle's import names ([`entry`]), so it's found without a
/// publisher's list
pub fn named_catalog(import: &str) -> Option<Catalog> {
    Some(Catalog {
        name: String::from(publisher(import.split_once(':')?.0)),
        entries: vec![entry(import)?],
        works: vec![],
        warnings: vec![],
    })
}

/// The publisher's name for an import prefix
fn publisher(prefix: &str) -> &'static str {
    match prefix {
        "dbl" => "the Digital Bible Library",
        _ => "Scripture Burrito",
    }
}

/**
The entry for the bundle at `location` (a URL, or a `file://` one), from its files (`files`, as a
bundle's import downloads them: the `.zip` or `.tar.gz` as `bundle`, or a folder's files as
`files/<path>`): `burrito:` or `dbl:` by the metadata it has, named, languaged, and licensed as it
says, so a dependency can be named for its package before it's built
*/
pub fn bundle_entry(location: &str, files: &[(String, Vec<u8>)]) -> Result<CatalogEntry> {
    let unpacked = unpacked(files)?;
    let (root, kind) = find_root(&unpacked)?;
    let prefix = match kind {
        Kind::Burrito => "burrito",
        Kind::Dbl => "dbl",
    };
    let import = format!("{prefix}:{location}");
    let entry = entry(&import).ok_or_else(|| {
        LogoxideError::Import(format!(
            "{location}: give a URL (https://) or a file:// URL to a bundle"
        ))
    })?;
    let bundle = read(&unpacked, &root, kind, &import)?;
    Ok(described(&entry, &bundle))
}

/**
A Bible from a bundle (`files`, as [`bundle_entry`] takes them) as a package in `dest`:
`logoxide.toml`, its text files in `usx/`, `usfm/`, or `usj/` (each checked by the size and MD5 the
metadata gives), and `COPYRIGHT.txt` (the license file) when it states its copyright, or the
license file it names. Named, languaged, and licensed by its metadata, whatever `entry` says but
where the bundle is; its revision is the upstream version
*/
pub fn import(entry: &CatalogEntry, files: &[(String, Vec<u8>)], dest: &dyn Storage) -> Result<()> {
    let import = (entry.source.import_name())
        .filter(|i| is_bundle(i))
        .ok_or_else(|| LogoxideError::Import(format!("{} isn't a bundle's import", entry.id)))?;
    let unpacked = unpacked(files)?;
    let (root, kind) = find_root(&unpacked)?;
    let bundle = read(&unpacked, &root, kind, import)?;
    let entry = described(entry, &bundle);
    let staged = MemoryStorage::new();
    let format = copy_text(&unpacked, &root, &bundle, &staged, import)?;
    let mut license_file = None;
    if let Some(path) = &bundle.license_ingredient {
        let name = path.rsplit('/').next().unwrap_or(path);
        let ingredient = (bundle.ingredients.iter()).find(|i| &i.path == path);
        let bytes = ingredient_bytes(&unpacked, &root, path, ingredient, import)?;
        staged.write(name, &bytes)?;
        license_file = Some(name.to_string());
    } else if !bundle.statements.is_empty() {
        let text = format!("{}\n", bundle.statements.join("\n\n"));
        staged.write("COPYRIGHT.txt", text.as_bytes())?;
        license_file = Some(String::from("COPYRIGHT.txt"));
    }
    let homepage = (bundle.homepage.clone())
        .or_else(|| {
            (entry.source.import_id())
                .filter(|l| !l.starts_with("file://"))
                .map(String::from)
        })
        .unwrap_or_default();
    let made = Made {
        format,
        license_file,
        publisher: publisher(kind.prefix()),
        publisher_name: bundle.publisher.clone(),
        homepage,
        versification: None,
        keys: None,
        note: Some(bundle.note()),
    };
    finish(&entry, made, staged, dest)
}

/// Which metadata a bundle has
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// `metadata.json`
    Burrito,
    /// `metadata.xml`
    Dbl,
}

impl Kind {
    fn prefix(self) -> &'static str {
        match self {
            Kind::Burrito => "burrito",
            Kind::Dbl => "dbl",
        }
    }
}

/// A file a bundle's metadata lists
#[derive(Clone, Debug, Default)]
struct Ingredient {
    /// Its path from the bundle's root, `/`-separated
    path: String,
    size: Option<u64>,
    /// Its MD5, in lowercase hex
    md5: Option<String>,
    mime: Option<String>,
}

/// What a bundle's metadata says, either kind's
#[derive(Debug, Default)]
struct Bundle {
    kind: Option<Kind>,
    name: Option<String>,
    abbreviation: Option<String>,
    description: Option<String>,
    aliases: Vec<String>,
    /// A BCP 47 tag, or an ISO 639 code
    language: Option<String>,
    language_name: Option<String>,
    direction: Option<Direction>,
    license: Option<String>,
    /// Its copyright statements, as text
    statements: Vec<String>,
    /// The ingredient its license is in (Burrito's `copyright.licenses[].ingredient`)
    license_ingredient: Option<String>,
    publisher: Option<String>,
    homepage: Option<String>,
    /// Its identity, in words (`ptx b01d38db…`, DBL's `317e03834065a236`)
    identity: Option<String>,
    revision: Option<String>,
    /// What made it (`Paratext 9.1.0.0`)
    generator: Option<String>,
    ingredients: Vec<Ingredient>,
    /// The paths of a DBL bundle's default publication's books, when it lists them
    publication: Vec<String>,
}

impl Bundle {
    /// Where its text came from, in words, for `[package.upstream] note`
    fn note(&self) -> String {
        let mut note = match self.kind {
            Some(Kind::Dbl) => String::from("Digital Bible Library text release bundle"),
            _ => String::from("Scripture Burrito"),
        };
        if let Some(identity) = &self.identity {
            note.push_str(&format!(" {identity}"));
        }
        if let Some(revision) = &self.revision {
            note.push_str(&format!(", revision {revision}"));
        }
        if let Some(generator) = &self.generator {
            note.push_str(&format!(" ({generator})"));
        }
        note
    }
}

/// The entry `entry` is, as `bundle` describes it
fn described(entry: &CatalogEntry, bundle: &Bundle) -> CatalogEntry {
    let mut entry = entry.clone();
    let name = (bundle.name.clone()).or_else(|| bundle.abbreviation.clone());
    if let Some(name) = name {
        entry.name = name;
    }
    if let Some(id) = (bundle.abbreviation.as_deref())
        .or(bundle.name.as_deref())
        .map(package_id)
        .filter(|id| logoxide::manifest::check_id("package", id).is_ok())
    {
        entry.id = id;
    }
    entry.abbreviation = bundle.abbreviation.clone();
    entry.aliases = (bundle.aliases.iter())
        .filter(|a| **a != entry.name)
        .cloned()
        .collect();
    entry.description = bundle.description.clone();
    let (tag, raw) = match bundle.language.as_deref() {
        Some(code) => language::normalize_language(code),
        None => (String::from("und"), None),
    };
    entry.language = tag;
    entry.language_name = raw.or_else(|| bundle.language_name.clone());
    entry.direction = (bundle.direction).unwrap_or_else(|| {
        language::script_direction(language::script_of_tag(&entry.language).as_deref())
    });
    entry.license = bundle.license.clone();
    entry.version = bundle.revision.clone();
    entry
}

/// A bundle's files from what its import downloaded: a `.zip` or `.tar.gz` unpacked (a
/// `.tar.gz`'s one top folder left out), or a folder's files
fn unpacked(files: &[(String, Vec<u8>)]) -> Result<MemoryStorage> {
    let unpacked = MemoryStorage::new();
    for (name, bytes) in files {
        if let Some(path) = name.strip_prefix("files/") {
            unpacked.write(path, bytes)?;
        } else if bytes.starts_with(b"PK") {
            for (path, bytes) in crate::unzip(bytes)? {
                unpacked.write(&path, &bytes)?;
            }
        } else if bytes.starts_with(&[0x1f, 0x8b]) {
            logoxide::net::archive::unpack(bytes, &unpacked)?;
        } else {
            return Err(LogoxideError::Import(format!(
                "{name}: not a .zip or .tar.gz, nor a folder's files"
            )));
        }
    }
    Ok(unpacked)
}

/// The folder a bundle is in (`""`, or `<top>/` in a `.zip` of a folder), and its kind: where
/// its `metadata.json`, else its `metadata.xml`, is nearest the top
fn find_root(unpacked: &MemoryStorage) -> Result<(String, Kind)> {
    let mut best: Option<(usize, Kind, String)> = None;
    for path in unpacked.walk("")? {
        let (dir, name) = match path.rsplit_once('/') {
            Some((dir, name)) => (format!("{dir}/"), name),
            None => (String::new(), path.as_str()),
        };
        let kind = match name {
            "metadata.json" => Kind::Burrito,
            "metadata.xml" => Kind::Dbl,
            _ => continue,
        };
        let depth = dir.matches('/').count();
        let better = best.as_ref().is_none_or(|(d, k, _)| {
            depth < *d || (depth == *d && kind == Kind::Burrito && *k == Kind::Dbl)
        });
        if better {
            best = Some((depth, kind, dir));
        }
    }
    best.map(|(_, kind, dir)| (dir, kind)).ok_or_else(|| {
        LogoxideError::Import(String::from(
            "no metadata.json (a Scripture Burrito's) or metadata.xml (a DBL bundle's)",
        ))
    })
}

/// The bundle's metadata, read
fn read(unpacked: &MemoryStorage, root: &str, kind: Kind, import: &str) -> Result<Bundle> {
    match kind {
        Kind::Burrito => {
            let text = unpacked.read_string(&format!("{root}metadata.json"))?;
            read_burrito(&text).map_err(|e| LogoxideError::Import(format!("{import}: {e}")))
        }
        Kind::Dbl => {
            let text = unpacked.read_string(&format!("{root}metadata.xml"))?;
            read_dbl(&text).map_err(|e| LogoxideError::Import(format!("{import}: {e}")))
        }
    }
}

/// A Burrito's localized text (`{"en": "…"}`): in `locale`, else English, else the first
fn localized(value: &Value, locale: &str) -> Option<String> {
    let map = value.as_object()?;
    let text = (map.get(locale))
        .or_else(|| map.get("en"))
        .or_else(|| map.values().next())?
        .as_str()?
        .trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// A string at `pointer` in `value`, trimmed, if it's there and not empty
fn text_at(value: &Value, pointer: &str) -> Option<String> {
    let text = value.pointer(pointer)?.as_str()?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// What HTML or XHTML says, without its tags, each paragraph on its own line
fn without_tags(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('<') {
        out.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('>') else {
            rest = "";
            break;
        };
        let tag = rest[open + 1..open + close]
            .trim_start_matches('/')
            .to_ascii_lowercase();
        if ["p", "br", "div", "li"]
            .iter()
            .any(|t| tag == *t || tag.starts_with(&format!("{t} ")))
        {
            out.push('\n');
        }
        rest = &rest[open + close + 1..];
    }
    out.push_str(rest);
    let lines: Vec<&str> = (out.lines())
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    lines.join("\n")
}

/**
A Scripture Burrito's `metadata.json`: 1.x, a source or derived burrito of a Bible's text
(`scripture/textTranslation`); its identification, first language, copyright, rights holder (else
publisher), and ingredients
*/
fn read_burrito(text: &str) -> std::result::Result<Bundle, String> {
    let json: Value =
        serde_json::from_str(text).map_err(|e| format!("metadata.json isn't JSON: {e}"))?;
    let version = text_at(&json, "/meta/version").unwrap_or_default();
    if version.split('.').next() != Some("1") {
        return Err(format!(
            "a Scripture Burrito {} (logoxide reads 1.x)",
            if version.is_empty() {
                "of no version"
            } else {
                &version
            }
        ));
    }
    match text_at(&json, "/meta/category").as_deref() {
        Some("source" | "derived") | None => {}
        Some(other) => return Err(format!("a {other} burrito, not one of a text")),
    }
    let flavor_type = text_at(&json, "/type/flavorType/name").unwrap_or_default();
    let flavor = text_at(&json, "/type/flavorType/flavor/name").unwrap_or_default();
    if (flavor_type.as_str(), flavor.as_str()) != ("scripture", "textTranslation") {
        return Err(format!(
            "a {flavor_type}/{flavor} burrito: only a Bible's text (scripture/textTranslation) \
             is imported"
        ));
    }
    let locale = text_at(&json, "/meta/defaultLocale").unwrap_or_else(|| String::from("en"));
    let identification = json.get("identification").cloned().unwrap_or_default();
    let mut bundle = Bundle {
        kind: Some(Kind::Burrito),
        name: identification
            .get("name")
            .and_then(|v| localized(v, &locale)),
        abbreviation: identification
            .get("abbreviation")
            .and_then(|v| localized(v, &locale)),
        description: identification
            .get("description")
            .and_then(|v| localized(v, &locale)),
        ..Bundle::default()
    };
    // `primary`: one authority, one id, its revision
    if let Some((authority, ids)) = (identification.pointer("/primary"))
        .and_then(Value::as_object)
        .and_then(|p| p.iter().next())
        && let Some((id, about)) = ids.as_object().and_then(|ids| ids.iter().next())
    {
        bundle.identity = Some(format!("{authority} {id}"));
        bundle.revision = text_at(about, "/revision");
    }
    let generator = [
        text_at(&json, "/meta/generator/softwareName"),
        text_at(&json, "/meta/generator/softwareVersion"),
    ];
    let generator: Vec<String> = generator.into_iter().flatten().collect();
    bundle.generator = (!generator.is_empty()).then(|| generator.join(" "));
    if let Some(language) = json.pointer("/languages/0") {
        bundle.language = text_at(language, "/tag");
        bundle.language_name = language.get("name").and_then(|v| localized(v, &locale));
        bundle.direction = match text_at(language, "/scriptDirection").as_deref() {
            Some("rtl") => Some(Direction::Rtl),
            Some("ltr") => Some(Direction::Ltr),
            _ => None,
        };
    }
    read_burrito_copyright(&json, &mut bundle);
    // The rights holder, else the publisher
    let agencies = json.get("agencies").and_then(Value::as_array);
    let agency = ["rightsHolder", "publication"].iter().find_map(|role| {
        agencies?.iter().find(|a| {
            (a.get("roles").and_then(Value::as_array))
                .is_some_and(|roles| roles.iter().any(|r| r.as_str() == Some(role)))
        })
    });
    if let Some(agency) = agency {
        bundle.publisher = agency.get("name").and_then(|v| localized(v, &locale));
        bundle.homepage = text_at(agency, "/url").filter(|u| u.starts_with("http"));
    }
    let ingredients = json.get("ingredients").and_then(Value::as_object);
    for (path, about) in ingredients.into_iter().flatten() {
        bundle.ingredients.push(Ingredient {
            path: path.clone(),
            size: about.get("size").and_then(Value::as_u64),
            md5: text_at(about, "/checksum/md5").map(|m| m.to_ascii_lowercase()),
            mime: text_at(about, "/mimeType"),
        });
    }
    Ok(bundle)
}

/// A Burrito's `copyright`: in the public domain, its licenses (by URL, else an ingredient), or
/// its short statements
fn read_burrito_copyright(json: &Value, bundle: &mut Bundle) {
    let Some(copyright) = json.get("copyright") else {
        return;
    };
    if copyright.get("publicDomain").and_then(Value::as_bool) == Some(true) {
        bundle.license = Some(String::from("LicenseRef-PublicDomain"));
    }
    for license in (copyright.get("licenses").and_then(Value::as_array))
        .into_iter()
        .flatten()
    {
        if let Some(url) = text_at(license, "/url") {
            bundle.license = bundle
                .license
                .take()
                .or_else(|| crate::license::license_from_text(&url));
            bundle.statements.push(url);
        }
        if let Some(ingredient) = text_at(license, "/ingredient") {
            bundle.license_ingredient.get_or_insert(ingredient);
        }
    }
    let statements = copyright.get("shortStatements").and_then(Value::as_array);
    for statement in statements.into_iter().flatten() {
        let Some(text) = text_at(statement, "/statement") else {
            continue;
        };
        let text = without_tags(&text);
        bundle.license = bundle
            .license
            .take()
            .or_else(|| crate::license::license_from_text(&text));
        bundle.statements.push(text);
    }
}

/**
A DBL text release bundle's `metadata.xml` (DBLMetadata 2.x): a `text` bundle's identification,
language, rights holder, copyright statement, manifest of files, and its default publication's
books
*/
fn read_dbl(text: &str) -> std::result::Result<Bundle, String> {
    let text = text.trim_start_matches('\u{feff}');
    let document =
        roxmltree::Document::parse(text).map_err(|e| format!("metadata.xml isn't XML: {e}"))?;
    let root = document.root_element();
    if root.tag_name().name() != "DBLMetadata" {
        return Err(String::from(
            "metadata.xml isn't a DBL bundle's (no DBLMetadata)",
        ));
    }
    let medium = text_of(child(root, "type"), "medium").unwrap_or_default();
    if medium != "text" {
        return Err(format!(
            "a DBL bundle of {}: only a Bible's text is imported",
            if medium.is_empty() {
                "no medium"
            } else {
                &medium
            }
        ));
    }
    let identification = child(root, "identification");
    let language = child(root, "language");
    let mut bundle = Bundle {
        kind: Some(Kind::Dbl),
        name: text_of(identification, "name"),
        abbreviation: text_of(identification, "abbreviation"),
        description: text_of(identification, "description"),
        aliases: [text_of(identification, "nameLocal")]
            .into_iter()
            .flatten()
            .collect(),
        identity: root.attribute("id").map(String::from),
        revision: root.attribute("revision").map(String::from),
        generator: text_of(identification, "bundleProducer").map(|p| p.replace('/', " ")),
        language: text_of(language, "ldml").or_else(|| text_of(language, "iso")),
        language_name: text_of(language, "name"),
        direction: match text_of(language, "scriptDirection").map(|d| d.to_ascii_lowercase()) {
            Some(d) if d == "rtl" => Some(Direction::Rtl),
            Some(d) if d == "ltr" => Some(Direction::Ltr),
            _ => None,
        },
        ..Bundle::default()
    };
    let rights_holder = child(root, "agencies").and_then(|a| child(a, "rightsHolder"));
    bundle.publisher = text_of(rights_holder, "name");
    bundle.homepage = text_of(rights_holder, "url").filter(|u| u.starts_with("http"));
    if let Some(copyright) = child(root, "copyright") {
        for statement in copyright
            .descendants()
            .filter(|n| n.has_tag_name("statementContent"))
        {
            let mut text = String::new();
            for node in statement.descendants() {
                if node.is_element() && ["p", "br", "div"].contains(&node.tag_name().name()) {
                    text.push('\n');
                }
                if let Some(t) = node.text().filter(|_| node.is_text()) {
                    text.push_str(t);
                }
            }
            let text = without_tags(&text);
            if !text.is_empty() {
                bundle.license = bundle
                    .license
                    .take()
                    .or_else(|| crate::license::license_from_text(&text));
                bundle.statements.push(text);
            }
        }
    }
    let manifest = child(root, "manifest");
    for resource in (manifest.into_iter())
        .flat_map(|m| m.children())
        .filter(|n| n.has_tag_name("resource"))
    {
        let Some(uri) = resource.attribute("uri") else {
            continue;
        };
        bundle.ingredients.push(Ingredient {
            path: uri.to_string(),
            size: resource.attribute("size").and_then(|s| s.parse().ok()),
            md5: resource
                .attribute("checksum")
                .map(|m| m.to_ascii_lowercase()),
            mime: resource.attribute("mimeType").map(String::from),
        });
    }
    let publications = child(root, "publications");
    let publication = (publications.into_iter())
        .flat_map(|p| p.children())
        .filter(|n| n.has_tag_name("publication"))
        .fold(None, |chosen, p| match (chosen, p.attribute("default")) {
            (_, Some("true")) | (None, _) => Some(p),
            (chosen, _) => chosen,
        });
    if let Some(structure) = publication.and_then(|p| child(p, "structure")) {
        bundle.publication = (structure.descendants())
            .filter(|n| n.has_tag_name("content"))
            .filter_map(|n| n.attribute("src").map(String::from))
            .collect();
    }
    Ok(bundle)
}

/// An element's first child element named `name`
fn child<'a, 'i>(node: roxmltree::Node<'a, 'i>, name: &str) -> Option<roxmltree::Node<'a, 'i>> {
    node.children().find(|c| c.has_tag_name(name))
}

/// The text of `node`'s child `name`, trimmed, if it has some
fn text_of(node: Option<roxmltree::Node<'_, '_>>, name: &str) -> Option<String> {
    let text = child(node?, name)?.text()?.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// The formats a bundle's text can be in, as logoxide prefers them
const FORMATS: [&str; 3] = ["usx", "usfm", "usj"];

/// The format an ingredient's text is in, by its extension and MIME type (USJ by what it starts
/// with too, since it's JSON)
fn text_format(
    ingredient: &Ingredient,
    bytes: impl FnOnce() -> Option<Vec<u8>>,
) -> Option<&'static str> {
    let path = ingredient.path.to_ascii_lowercase();
    let extension = path.rsplit_once('.').map(|(_, e)| e).unwrap_or_default();
    let mime = ingredient.mime.as_deref().unwrap_or_default();
    match extension {
        "usx" => Some("usx"),
        "usfm" | "sfm" => Some("usfm"),
        "usj" => Some("usj"),
        _ if mime == "text/x-usfm" => Some("usfm"),
        "json" if mime.is_empty() || mime == "application/json" => {
            let bytes = bytes()?;
            let head = String::from_utf8_lossy(&bytes[..bytes.len().min(256)])
                .replace([' ', '\n', '\r', '\t'], "");
            head.contains("\"type\":\"USJ\"").then_some("usj")
        }
        _ => None,
    }
}

/// Where an ingredient is: at its path from the bundle's root, else in `ingredients/` (which some
/// burritos' paths leave out)
fn ingredient_path(unpacked: &MemoryStorage, root: &str, path: &str) -> Option<String> {
    if path.starts_with('/') || path.contains('\\') || path.split('/').any(|p| p == "..") {
        return None;
    }
    [format!("{root}{path}"), format!("{root}ingredients/{path}")]
        .into_iter()
        .find(|p| unpacked.exists(p))
}

/**
An ingredient's bytes, checked by the size and MD5 the metadata gives. A file whose line endings
alone differ from what was listed (a burrito kept in git, which turned its CRLFs into LFs) is
taken as it is
*/
fn ingredient_bytes(
    unpacked: &MemoryStorage,
    root: &str,
    path: &str,
    ingredient: Option<&Ingredient>,
    import: &str,
) -> Result<Vec<u8>> {
    let found = ingredient_path(unpacked, root, path).ok_or_else(|| {
        LogoxideError::Import(format!("{import}: {path} is listed, but not in the bundle"))
    })?;
    let bytes = unpacked.read(&found)?;
    let Some(ingredient) = ingredient else {
        return Ok(bytes);
    };
    let Err(problem) = as_listed(&bytes, ingredient) else {
        return Ok(bytes);
    };
    let other_endings = match bytes.contains(&b'\r') {
        true => String::from_utf8_lossy(&bytes).replace("\r\n", "\n"),
        false => String::from_utf8_lossy(&bytes).replace('\n', "\r\n"),
    };
    // How it differs in the line endings it was listed with, once those are its size
    let problem = match as_listed(other_endings.as_bytes(), ingredient) {
        Ok(()) => return Ok(bytes),
        Err(other) if ingredient.size == Some(other_endings.len() as u64) => other,
        Err(_) => problem,
    };
    Err(LogoxideError::Import(format!("{import}: {path} {problem}")))
}

/// Whether `bytes` have the size and MD5 `ingredient` lists, else how they differ
fn as_listed(bytes: &[u8], ingredient: &Ingredient) -> std::result::Result<(), String> {
    if let Some(size) = ingredient.size
        && size != bytes.len() as u64
    {
        return Err(format!(
            "is {} bytes, but the metadata says {size}",
            bytes.len()
        ));
    }
    if let Some(md5) = &ingredient.md5 {
        use md5::Digest;
        let digest = md5::Md5::digest(bytes);
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        if hex != *md5 {
            return Err(format!("has the MD5 {hex}, but the metadata says {md5}"));
        }
    }
    Ok(())
}

/**
The bundle's text files, checked, into `staged`'s folder for their format (the first of
[`FORMATS`] it has), each under its file name: those its metadata lists (a DBL bundle's default
publication's books, when it names them), not a URL's. Two of one name are an error
*/
fn copy_text(
    unpacked: &MemoryStorage,
    root: &str,
    bundle: &Bundle,
    staged: &MemoryStorage,
    import: &str,
) -> Result<&'static str> {
    let listed = (bundle.ingredients.iter()).filter(|i| !i.path.contains("://"));
    let listed: Vec<&Ingredient> = match bundle.publication.is_empty() {
        true => listed.collect(),
        false => listed
            .filter(|i| bundle.publication.contains(&i.path))
            .collect(),
    };
    let formats: Vec<(&Ingredient, &'static str)> = (listed.into_iter())
        .filter_map(|i| {
            let bytes = || {
                unpacked
                    .read(&ingredient_path(unpacked, root, &i.path)?)
                    .ok()
            };
            Some((i, text_format(i, bytes)?))
        })
        .collect();
    let format = (FORMATS.iter())
        .find(|f| formats.iter().any(|(_, g)| g == *f))
        .ok_or_else(|| {
            LogoxideError::Import(format!(
                "{import}: its metadata lists no USX, USFM, or USJ files"
            ))
        })?;
    for (ingredient, _) in formats.iter().filter(|(_, f)| f == format) {
        let path = &ingredient.path;
        let bytes = ingredient_bytes(unpacked, root, path, Some(ingredient), import)?;
        let name = path.rsplit('/').next().unwrap_or(path);
        let dest = format!("{format}/{name}");
        if staged.exists(&dest) {
            return Err(LogoxideError::Import(format!(
                "{import}: two {format} files named {name}"
            )));
        }
        staged.write(&dest, &bytes)?;
    }
    Ok(format)
}

#[cfg(test)]
mod tests {
    use super::*;
    use logoxide::package::{Features, Package};

    /// A fixture folder's files, as a host gives a folder's: `files/<path>`
    fn folder(name: &str) -> Vec<(String, Vec<u8>)> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        let storage = logoxide::FsStorage::new(&root);
        let mut files = vec![];
        for path in storage.walk("").unwrap() {
            if path.ends_with(".md") && !path.contains('/') || path.starts_with('.') {
                continue;
            }
            files.push((format!("files/{path}"), storage.read(&path).unwrap()));
        }
        files
    }

    /// A `.zip` of a fixture folder, in a folder of its own, as a host downloads a bundle
    fn zipped(name: &str, top: &str) -> Vec<(String, Vec<u8>)> {
        let files = folder(name);
        let named: Vec<(String, &[u8])> = (files.iter())
            .map(|(p, b)| (format!("{top}/{}", &p["files/".len()..]), b.as_slice()))
            .collect();
        let named: Vec<(&str, &[u8])> = named.iter().map(|(p, b)| (p.as_str(), *b)).collect();
        vec![(String::from(DOWNLOAD), crate::tests::zip_bytes(&named))]
    }

    fn open(dest: MemoryStorage) -> Package {
        Package::open(std::sync::Arc::new(dest), Features::default()).unwrap()
    }

    #[test]
    fn entries_from_names() {
        let entry = super::entry("burrito:https://example.org/b/web-burrito.zip").unwrap();
        assert_eq!(
            (entry.id.as_str(), entry.name.as_str()),
            ("web-burrito", "web-burrito")
        );
        assert_eq!(
            entry.source.downloads()[0].urls,
            ["https://example.org/b/web-burrito.zip"]
        );
        assert!(!entry.source.versions_track_updates());
        assert_eq!(entry.source.prefix(), Some("burrito"));
        let entry = super::entry("dbl:file:///home/me/My%20Bibles/bsb/").unwrap();
        assert_eq!(entry.id, "bsb");
        assert!(super::entry("burrito:ftp://example.org/x.zip").is_none());
        assert!(super::entry("burrito:/home/me/x.zip").is_none());
        assert!(super::entry("ebible:eng-asv").is_none());
        let catalog = crate::named_catalog("dbl:https://example.org/bsb.tar.gz").unwrap();
        assert_eq!(catalog.name, "the Digital Bible Library");
        assert!(
            catalog
                .find_import("dbl:https://example.org/bsb.tar.gz")
                .is_some()
        );
    }

    #[test]
    fn imports_a_burrito() {
        let location = "file:///fixtures/web";
        let files = folder("burrito/web");
        let entry = bundle_entry(location, &files).unwrap();
        assert_eq!(
            entry.source.import_name(),
            Some("burrito:file:///fixtures/web")
        );
        assert_eq!(
            (entry.id.as_str(), entry.short_name()),
            ("engweb14", "engWEB14")
        );
        let dest = MemoryStorage::new();
        crate::tests::build_twice(&entry, &files, &dest);
        assert_eq!(
            dest.walk("").unwrap(),
            // The front matter's file is in the bundle, but its metadata doesn't list it
            [
                "logoxide.toml",
                "usfm/57TITengWEB14.SFM",
                "usfm/66JUDengWEB14.SFM"
            ]
        );
        let package = open(dest);
        let info = package.info();
        assert_eq!(info.name, "World English Bible (American Edition)");
        assert_eq!(info.language, "en");
        assert_eq!(info.license.as_deref(), Some("LicenseRef-PublicDomain"));
        assert_eq!(info.license_file, None);
        assert_eq!(info.publisher.as_deref(), Some("Scripture Burrito"));
        assert_eq!(info.homepage, None);
        assert_eq!(info.version.to_string(), "1.0.0");
        let upstream = info.upstream.clone().unwrap();
        assert_eq!(
            upstream.import.as_deref(),
            Some("burrito:file:///fixtures/web")
        );
        assert_eq!(upstream.url, None);
        assert_eq!(
            upstream.version.as_deref(),
            Some("c772aae2277babbe71dce9e3e061656b6909f5e2")
        );
        assert_eq!(
            upstream.note.as_deref(),
            Some(
                "Scripture Burrito ptx b01d38db1c30059b892c8cbbc65b70271743d32d, revision \
                 c772aae2277babbe71dce9e3e061656b6909f5e2 (Paratext 9.1.0.0)"
            )
        );
        assert_eq!(package.books().len(), 2);
        // Made from a zip of it the same
        let again = MemoryStorage::new();
        let entry = bundle_entry(location, &zipped("burrito/web", "web")).unwrap();
        import(&entry, &zipped("burrito/web", "web"), &again).unwrap();
        assert_eq!(open(again).checksum().unwrap(), package.checksum().unwrap());
    }

    #[test]
    fn imports_a_loose_burrito() {
        // Its ingredients' paths leave out `ingredients/`, its USFM has no MIME type, and its
        // copyright is a statement
        let files = zipped("burrito/vandyke", "vandyke-master");
        let entry = bundle_entry("https://example.org/vandyke.zip", &files).unwrap();
        let dest = MemoryStorage::new();
        import(&entry, &files, &dest).unwrap();
        assert_eq!(
            dest.walk("").unwrap(),
            ["COPYRIGHT.txt", "logoxide.toml", "usfm/TIT.usfm"]
        );
        assert_eq!(
            dest.read_string("COPYRIGHT.txt").unwrap(),
            "Public Domain\n"
        );
        let package = open(dest);
        let info = package.info();
        assert_eq!(
            (info.id.as_str(), info.name.as_str()),
            ("vdat", "Van Dyck Test Version")
        );
        assert_eq!(info.license.as_deref(), Some("LicenseRef-PublicDomain"));
        assert_eq!(info.license_file.as_deref(), Some("COPYRIGHT.txt"));
        assert_eq!(
            info.homepage.as_deref(),
            Some("https://example.org/vandyke.zip")
        );
        assert_eq!(
            info.description.as_deref(),
            Some("Arabic: Titus from Van Dyck to test Graphite")
        );
        assert_eq!(package.books().len(), 1);
    }

    #[test]
    fn imports_a_dbl_bundle() {
        let files = folder("dbl/bsb");
        let entry = bundle_entry("file:///bundles/bsb", &files).unwrap();
        assert_eq!(entry.source.import_name(), Some("dbl:file:///bundles/bsb"));
        let dest = MemoryStorage::new();
        crate::tests::build_twice(&entry, &files, &dest);
        assert_eq!(
            dest.walk("").unwrap(),
            ["COPYRIGHT.txt", "logoxide.toml", "usx/TIT.usx"]
        );
        assert_eq!(
            dest.read_string("COPYRIGHT.txt").unwrap(),
            "Public Domain\n"
        );
        let package = open(dest);
        let info = package.info();
        assert_eq!(
            (info.id.as_str(), info.name.as_str()),
            ("bsb", "Berean Standard Bible")
        );
        assert_eq!(info.abbreviation.as_deref(), Some("BSB"));
        assert_eq!(info.language, "en");
        assert_eq!(info.license.as_deref(), Some("LicenseRef-PublicDomain"));
        assert_eq!(info.publisher.as_deref(), Some("BSB Publishing, LLC"));
        assert_eq!(info.homepage.as_deref(), Some("https://berean.bible/"));
        let upstream = info.upstream.clone().unwrap();
        assert_eq!(upstream.version.as_deref(), Some("1"));
        assert_eq!(
            upstream.note.as_deref(),
            Some(
                "Digital Bible Library text release bundle 317e03834065a236, revision 1 (Paratext 9.5.110.1)"
            )
        );
        assert_eq!(package.books().len(), 1);
    }

    /// `files` with the one at `path` (`files/<path>`) changed by `change`
    fn changed(
        mut files: Vec<(String, Vec<u8>)>,
        path: &str,
        change: impl Fn(&mut Vec<u8>),
    ) -> Vec<(String, Vec<u8>)> {
        for (name, bytes) in &mut files {
            if name == &format!("files/{path}") {
                change(bytes);
            }
        }
        files
    }

    #[test]
    fn what_isnt_read() {
        let error = |files: &[(String, Vec<u8>)]| {
            let entry = entry("burrito:https://example.org/x.zip").unwrap();
            import(&entry, files, &MemoryStorage::new())
                .unwrap_err()
                .to_string()
        };
        let web = || folder("burrito/web");
        let metadata = |change: &dyn Fn(&mut Value)| {
            changed(web(), "metadata.json", |bytes| {
                let mut json: Value = serde_json::from_slice(bytes).unwrap();
                change(&mut json);
                *bytes = serde_json::to_vec(&json).unwrap();
            })
        };
        // A file that isn't as listed
        let files = changed(web(), "ingredients/57TITengWEB14.SFM", |b| b[100] ^= 1);
        let said = error(&files);
        assert!(said.contains("57TITengWEB14.SFM has the MD5"), "{said}");
        let files = changed(web(), "ingredients/57TITengWEB14.SFM", |b| b.push(b'\n'));
        assert!(error(&files).contains("but the metadata says 5799"));
        // One listed but missing
        let files: Vec<_> = (web().into_iter())
            .filter(|(p, _)| !p.ends_with("66JUDengWEB14.SFM"))
            .collect();
        assert!(error(&files).contains("is listed, but not in the bundle"));
        // Other flavors, versions, and templates
        let audio =
            metadata(&|j| j["type"]["flavorType"]["flavor"]["name"] = "audioTranslation".into());
        assert!(error(&audio).contains("a scripture/audioTranslation burrito"));
        let two = metadata(&|j| j["meta"]["version"] = "2.0.0".into());
        assert!(error(&two).contains("Scripture Burrito 2.0.0"));
        let template = metadata(&|j| j["meta"]["category"] = "template".into());
        assert!(error(&template).contains("a template burrito"));
        // An ingredient's path out of the bundle is never read
        let outside = metadata(&|j| {
            let ingredients = j["ingredients"].as_object_mut().unwrap();
            let tit = ingredients.remove("ingredients/57TITengWEB14.SFM").unwrap();
            ingredients.insert(String::from("../57TITengWEB14.SFM"), tit);
        });
        assert!(error(&outside).contains("is listed, but not in the bundle"));
        // No text
        let none = metadata(&|j| j["ingredients"] = serde_json::json!({}));
        assert!(error(&none).contains("lists no USX, USFM, or USJ"));
        // No metadata
        let bare: Vec<_> = (web().into_iter())
            .filter(|(p, _)| !p.ends_with("metadata.json"))
            .collect();
        assert!(error(&bare).contains("no metadata.json"));
        let junk = vec![(String::from(DOWNLOAD), b"<html>".to_vec())];
        assert!(error(&junk).contains("not a .zip or .tar.gz"));
        // A DBL bundle of audio
        let dbl = changed(folder("dbl/bsb"), "metadata.xml", |b| {
            let text = String::from_utf8(b.clone()).unwrap();
            *b = text
                .replace("<medium>text</medium>", "<medium>audio</medium>")
                .into_bytes();
        });
        assert!(error(&dbl).contains("a DBL bundle of audio"));
    }

    #[test]
    fn statements_without_tags() {
        assert_eq!(
            without_tags("<p>© 2020 <b>Someone</b></p><p>All rights reserved.</p>"),
            "© 2020 Someone\nAll rights reserved."
        );
    }
}
