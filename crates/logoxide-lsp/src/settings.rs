//! Settings: the CLI's `config.toml` (`~/.config/topos/config.toml`), then the editor's settings
//! (initialization options, then `workspace/didChangeConfiguration`), each overriding the last.
//!
//! Keys are the CLI's option names (`psg-fmt`, `merge-data`, ...; `psg_fmt` works too), so one
//! config serves both. Keys the server has no use for (`mode`, `cache`, ...) are ignored.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use serde_json::{Map, Value};
use topos_bible::{
    data::{
        bible_data::{BibleData, BibleDataInput},
        patch::DataPatch,
    },
    filter::bible_filter::BibleFilter,
    matcher::{BibleMatcher, context::BookContext},
    segments::formatter::{BookStyle, FormatOptions},
};

use logoxide_markup::Kind;

use logoxide::{
    Library, Project,
    manifest::{MANIFEST, PackageKind},
};

use crate::{
    bible::{Bible, Bibles},
    dictionary::Dictionary,
};

/// What inlay hints show after each reference
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum InlayHints {
    /// The reference as formatted, where it is written differently (`jn 3:16` → `John 3:16`)
    #[default]
    Changed,
    /// The reference as formatted, everywhere
    Always,
    /// The OSIS id (`John.3.16`)
    Osis,
    /// The USFM reference (`JHN 3:16`)
    Usfm,
    Never,
    /**
    A template after every reference, on one line: one written here (anything with a `{`, like
    `{first:1}`, its first verse's text), or a [`Template`]'s name in `templates` (with its markup)
    */
    Template(TemplateRef),
}

impl<'de> Deserialize<'de> for InlayHints {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Ok(match value.to_lowercase().as_str() {
            "changed" => InlayHints::Changed,
            "always" => InlayHints::Always,
            "osis" => InlayHints::Osis,
            "usfm" => InlayHints::Usfm,
            "never" => InlayHints::Never,
            _ if value.contains('{') => InlayHints::Template(TemplateRef::Inline(value)),
            // Checked against `templates` once they're read
            _ => InlayHints::Template(TemplateRef::Named(value)),
        })
    }
}

/// A template written in a setting (anything with a `{`), or one of `templates` by name
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TemplateRef {
    Inline(String),
    Named(String),
}

impl TemplateRef {
    /// A setting's value, checked against `templates`
    fn read(
        key: &str,
        value: &str,
        templates: &BTreeMap<String, Template>,
    ) -> Result<Self, String> {
        match value.contains('{') {
            true => Ok(TemplateRef::Inline(value.to_string())),
            false if templates.contains_key(value) => Ok(TemplateRef::Named(value.to_string())),
            false => Err(format!(
                "{key}: no template is named {value:?} (a template written here needs a {{placeholder}})"
            )),
        }
    }
}

/// The diagnostic on each reference (alongside the warnings for ones that don't exist)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReferenceDiagnostics {
    /// An information diagnostic with the reference as formatted: `John 3:16`
    #[default]
    #[serde(alias = "information")]
    Info,
    /// The same, as a hint (editors show these more quietly)
    Hint,
    Never,
}

/// A line in the hover, in the order the `hover` setting lists them
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HoverField {
    /// The reference as written in the text: `jn 3:16`
    Written,
    /// With the book's full name: `John 3:16`
    Name,
    /// With the book's abbreviation: `Jn 3:16`
    #[serde(alias = "abbrev")]
    Abbreviation,
    /// The OSIS id: `John.3.16`
    Osis,
    /// The USFM reference: `JHN 3:16`
    Usfm,
    /// The book: its name, its number, and how many chapters it has
    Book,
    /// Old or New Testament
    Testament,
    /// The genres the book is in: `Gospels, Gospels And Acts`
    #[serde(alias = "genre")]
    Genres,
    /// How many verses the reference covers, and which: `36 (3:1-36)`
    Verses,
    /// The passage's text, from the Bible it names (or the default; see `hover-bibles`)
    Text,
    /// Integer keys (`book * 1_000_000 + chapter * 1_000 + verse`): `43003016`
    Bcv,
    /// Where the reference is in the document: line and columns
    Location,
    /// The Bible the reference names (`John 3:16 KJV`); nothing when it names none
    Translation,
}

impl HoverField {
    /// What the hover shows by default
    pub const DEFAULT: [HoverField; 9] = [
        HoverField::Text,
        HoverField::Name,
        HoverField::Abbreviation,
        HoverField::Osis,
        HoverField::Book,
        HoverField::Testament,
        HoverField::Genres,
        HoverField::Verses,
        HoverField::Location,
    ];
}

/// What hovering a verse (or chapter label) in a whole book shows, in the order the
/// `book-hover` setting lists them
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BookHoverField {
    /// How many references name it, and in how many files: `4 references in 3 files`
    #[serde(alias = "refs")]
    References,
    /// The same verse in the other Bibles (`hover-bibles`', else every other), each labeled
    /// with its name; a chapter label has none
    #[serde(alias = "translation", alias = "bibles")]
    Translations,
    /// The verse in the book's own Bible, labeled with its name; a chapter label has none
    #[serde(alias = "text", alias = "own")]
    Current,
}

/**
How Bible text is written (hover, whole books, and templates): `markup = { verse_numbers = {
show = true, display = "[{verse}]" }, ... }`
- Labels' `display` (verse and chapter numbers) can use `{verse}`, `{chapter}`, `{book}` (the
  USX code, `JHN`), and `{name}` (`John`)
- Other elements' `display` is a template with `{text}` for their text (and `{name}`), like
  `## {text}` for headings or `“{text}”` for words of Jesus
- Headings and notes that aren't shown are left out; the others are plain text
- `indent` is how many spaces each kind of poetic line (or indented paragraph) starts with, by
  its USFM marker: `indent = { q1 = 0, q2 = 2 }` (see [`DEFAULT_INDENTS`]; the keys it sets
  replace those)
- `verse_lines = true` starts each verse on a line of its own, and `paragraph_gaps = true` puts
  a blank line between paragraphs (both off by default)
- Other keys, like the editor's `color`, are for the editor
*/
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct MarkupSettings {
    /// Shown by default, as `[{verse}]`
    #[serde(alias = "verse-numbers")]
    pub verse_numbers: MarkupElement,
    /// Shown by default, as `{name} {chapter}` on its own line (in whole books)
    #[serde(alias = "chapter-numbers")]
    pub chapter_numbers: MarkupElement,
    /// Section headings, shown by default
    pub headings: MarkupElement,
    /// Footnotes; hidden by default, and `[{text}]` when shown
    pub notes: MarkupElement,
    /// Words of Christ (red letters)
    #[serde(alias = "words-of-jesus")]
    pub words_of_jesus: MarkupElement,
    /// Translators' additions (KJV italics)
    pub added: MarkupElement,
    /// LORD in small capitals
    #[serde(alias = "divine-name")]
    pub divine_name: MarkupElement,
    /// Quotations other than words of Jesus
    pub quotes: MarkupElement,
    /// Lines longer than this many characters break at a space (none by default)
    pub width: Option<usize>,
    /// Spaces before each line of a kind of poetry or indented paragraph, by USFM marker
    pub indent: BTreeMap<String, usize>,
    /// Each verse on a line of its own (off by default)
    #[serde(alias = "verse-lines")]
    pub verse_lines: Option<bool>,
    /// A blank line between paragraphs (off by default)
    #[serde(alias = "paragraph-gaps")]
    pub paragraph_gaps: Option<bool>,
}

/// Poetry's indents unless `indent` sets them: a verse's first line at the margin and the lines
/// that continue it two spaces further in each level (`q1` 0, `q2` 2, `q3` 4, `q4` 6)
pub const DEFAULT_INDENTS: [(&str, usize); 4] = [("q1", 0), ("q2", 2), ("q3", 4), ("q4", 6)];

impl MarkupSettings {
    /// The elements with a `{text}` template: their kind, settings, and whether they're shown by
    /// default
    pub fn elements(&self) -> [(Kind, &MarkupElement, bool); 6] {
        [
            (Kind::Title, &self.headings, true),
            (Kind::Note, &self.notes, false),
            (Kind::WordsOfJesus, &self.words_of_jesus, true),
            (Kind::Added, &self.added, true),
            (Kind::DivineName, &self.divine_name, true),
            (Kind::Quote, &self.quotes, true),
        ]
    }

    /// How text is written, for logoxide-text: hidden elements, the shown ones' templates, and
    /// the labels (verse numbers `[{verse}]` and chapters `{name} {chapter}` unless set), each
    /// with `{name}` for the book's name
    pub fn style(&self) -> logoxide_text::passage::Style {
        let label = |element: &MarkupElement, default: &str| {
            element.shown(true).then(|| {
                element
                    .display
                    .clone()
                    .unwrap_or_else(|| default.to_string())
            })
        };
        let mut style = logoxide_text::passage::Style {
            verse_label: label(&self.verse_numbers, "[{verse}]"),
            chapter_label: label(&self.chapter_numbers, "{name} {chapter}"),
            width: self.width,
            indents: (DEFAULT_INDENTS.iter())
                .map(|(marker, spaces)| (marker.to_string(), *spaces))
                .chain(self.indent.clone())
                .collect(),
            verse_lines: self.verse_lines.unwrap_or(false),
            paragraph_gaps: self.paragraph_gaps.unwrap_or(false),
            ..Default::default()
        };
        for (kind, element, default) in self.elements() {
            if !element.shown(default) {
                style.hidden.push(kind);
            } else if let Some(display) = &element.display {
                style.templates.insert(kind, display.clone());
            }
        }
        style
    }

    /// These settings with `over`'s on top (each `show` and `display` it sets)
    pub fn merged(&self, over: &MarkupSettings) -> MarkupSettings {
        let merge = |base: &MarkupElement, over: &MarkupElement| MarkupElement {
            show: over.show.or(base.show),
            display: over.display.clone().or_else(|| base.display.clone()),
        };
        MarkupSettings {
            verse_numbers: merge(&self.verse_numbers, &over.verse_numbers),
            chapter_numbers: merge(&self.chapter_numbers, &over.chapter_numbers),
            headings: merge(&self.headings, &over.headings),
            notes: merge(&self.notes, &over.notes),
            words_of_jesus: merge(&self.words_of_jesus, &over.words_of_jesus),
            added: merge(&self.added, &over.added),
            divine_name: merge(&self.divine_name, &over.divine_name),
            quotes: merge(&self.quotes, &over.quotes),
            width: over.width.or(self.width),
            indent: (self.indent.clone().into_iter())
                .chain(over.indent.clone())
                .collect(),
            verse_lines: over.verse_lines.or(self.verse_lines),
            paragraph_gaps: over.paragraph_gaps.or(self.paragraph_gaps),
        }
    }
}

/// The hover's first line unless `hover-title` sets it: the reference in bold, and the Bible it
/// names in parentheses (none when it names none)
pub const DEFAULT_HOVER_TITLE: &str = "**{reference}** ({translation})";

/**
A template's `{translation}`: the Bible's name, or when `name` is none, nothing, with the brackets
around it and the space before it (`John 3:16 ({translation})` is `John 3:16`)
*/
pub fn fill_translation(template: &str, name: Option<&str>) -> String {
    match name {
        Some(name) => template.replace("{translation}", name),
        None => ["({translation})", "[{translation}]", "{translation}"]
            .iter()
            .fold(template.to_string(), |text, gone| {
                text.replace(&format!(" {gone}"), "").replace(gone, "")
            }),
    }
}

/// One kind of element in [`MarkupSettings`]
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct MarkupElement {
    pub show: Option<bool>,
    pub display: Option<String>,
}

impl MarkupElement {
    pub fn shown(&self, default: bool) -> bool {
        self.show.unwrap_or(default)
    }
}

/**
A named template for a reference: `templates = { callout = { display = "> [!bible]
{reference}\n{text}", line_prefix = "> ", action = "insert" } }`. One with an `action` is offered
as a code action (`Insert callout`); any can be copied (the editor's `logoxide/templates`
request), and named by `inlay-hints`
*/
#[derive(Clone, Debug, Deserialize)]
#[serde(default)]
pub struct Template {
    /// What it writes: `{text}` (the passage), `{first:N}` and `{last:N}` (its first or last N
    /// verses), `{reference}` (as formatted, like `John 3:16`), `{osis}` (`John.3.16`), `{usfm}`
    /// (`JHN 3:16`), `{bible}` (the Bible's name), and `{translation}` (its name unless it's the
    /// default Bible and the reference names none; see [`fill_translation`]); default `{text}`
    pub display: String,
    /// Written before each line of the text, like `> ` for a quote
    #[serde(alias = "line-prefix")]
    pub line_prefix: String,
    /// The `markup` settings for this template, over the others
    pub markup: MarkupSettings,
    /// Where its code action writes it; none offers no code action
    pub action: Option<TemplateAction>,
}

/// Where a [`Template`]'s code action writes it
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TemplateAction {
    /// On the lines after the reference's line: `Insert callout`
    Insert,
    /// In place of the reference's whole line: `Replace line with callout`
    ReplaceLine,
    /// In place of the reference itself, keeping the rest of its line: `Replace with callout`
    Replace,
}

impl Default for Template {
    fn default() -> Self {
        Template {
            display: String::from("{text}"),
            line_prefix: String::new(),
            markup: MarkupSettings::default(),
            action: None,
        }
    }
}

/**
The `bibles` setting: packages from the project's `logoxide.lock` by id (`bibles = ["bsb", "kjv"]`),
or folders by name (`bibles = { BSB = "~/Bibles/BSB", KJV = { path = "~/Bibles/KJV", aliases =
["AV"] } }`)
*/
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum BiblesSetting {
    Packages(Vec<String>),
    Folders(BTreeMap<String, BibleSetting>),
}

impl Default for BiblesSetting {
    fn default() -> Self {
        BiblesSetting::Folders(BTreeMap::new())
    }
}

impl BiblesSetting {
    fn is_empty(&self) -> bool {
        match self {
            BiblesSetting::Packages(ids) => ids.is_empty(),
            BiblesSetting::Folders(folders) => folders.is_empty(),
        }
    }
}

/// A folder in the `bibles` setting: its path, or its path and other ways to write its name
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum BibleSetting {
    Path(String),
    Full {
        path: String,
        #[serde(default)]
        aliases: Vec<String>,
    },
}

impl BibleSetting {
    fn path(&self) -> &str {
        match self {
            BibleSetting::Path(path) | BibleSetting::Full { path, .. } => path,
        }
    }

    fn aliases(&self) -> &[String] {
        match self {
            BibleSetting::Path(_) => &[],
            BibleSetting::Full { aliases, .. } => aliases,
        }
    }
}

/**
The `commentaries` setting: none by default, `true` for every commentary the project uses, or
package ids (`["mhcc"]`, or `"mhcc, jfb"`)
*/
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum CommentariesSetting {
    All(bool),
    One(String),
    Many(Vec<String>),
}

impl Default for CommentariesSetting {
    fn default() -> Self {
        CommentariesSetting::All(false)
    }
}

impl CommentariesSetting {
    /// The ids named, or [`None`] for all of them; an empty list for none
    fn ids(&self) -> Option<Vec<String>> {
        let split = |values: &[String]| {
            (values.iter())
                .flat_map(|v| v.split(','))
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(String::from)
                .collect()
        };
        match self {
            CommentariesSetting::All(true) => None,
            CommentariesSetting::All(false) => Some(vec![]),
            CommentariesSetting::One(id) => Some(split(std::slice::from_ref(id))),
            CommentariesSetting::Many(ids) => Some(split(ids)),
        }
    }
}

/// One value or a list (`merge-data = "a.json"` or `["a.json", "b.json"]`)
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany {
    #[default]
    None,
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    fn values(&self) -> Vec<String> {
        match self {
            OneOrMany::None => vec![],
            OneOrMany::One(value) => vec![value.clone()],
            OneOrMany::Many(values) => values.clone(),
        }
    }
}

/// `format = "abbreviation"` (the book style, like `-f`), or a whole format like `psg-fmt`
#[derive(Clone, Debug, Deserialize)]
#[serde(untagged)]
pub enum FormatSetting {
    Style(BookStyle),
    Options(FormatOptions),
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Settings {
    pub format: Option<FormatSetting>,
    pub psg_fmt: Option<FormatOptions>,
    pub fmt_book_separator: Option<String>,
    pub fmt_chapter_verse: Option<String>,
    pub fmt_range: Option<String>,
    pub fmt_verse_separator: Option<String>,
    pub fmt_chapter_separator: Option<String>,
    pub fmt_join_adjacent: Option<bool>,
    pub fmt_omit_first_verse_of_chapter_range: Option<bool>,
    pub fmt_chapter_in_single_chapter_books: Option<bool>,
    pub data: Option<String>,
    pub merge_data: OneOrMany,
    pub remove_data: OneOrMany,
    pub context_book: Option<String>,
    pub context_heading: Option<String>,
    /// Extensions searched for workspace references (`"md,txt"` or a list; empty means every
    /// text file)
    pub ext: OneOrMany,
    /// More folders for workspace searches (go-to-references and the search command), beside
    /// the workspace folders: `"~/Obsidian"` or a list (relative ones are in the first workspace
    /// folder). Only searched: a project (`logoxide.toml`) in one isn't used
    pub search_folders: OneOrMany,
    /// Search EPUBs for workspace references too, whatever `ext` says: off by default, since a
    /// book takes a while to read the first time. Their references are kept in topos's cache (the
    /// CLI's, shared with it), and they open as `logoxide-file://` documents of the book's text
    pub epub: bool,
    /// Search PDFs the same way (in a build with the `pdf` feature; otherwise ignored)
    pub pdf: bool,
    pub inlay_hints: InlayHints,
    pub reference_diagnostics: ReferenceDiagnostics,
    /// The reference diagnostics' text: a template's name in `templates` (with its markup), or a
    /// template (`"{first:1}"`), on one line; the reference as formatted by default
    pub reference_diagnostics_template: Option<String>,
    /// Completion offers the chapter or verse already typed, first (`Matthew 1` offers
    /// `Matthew 1`, so its passage shows), like the CLI's `--complete-typed`; on by default
    pub complete_typed: Option<bool>,
    /// What the hover shows below the reference (`"osis,book"` or a list; `[]` for none)
    pub hover: Option<OneOrMany>,
    /// Hover (and completion documentation) leaves out the leading whitespace of a passage's
    /// text, a note's, or an entry's (a chapter label or heading displayed after a line break);
    /// off by default
    pub hover_trim: bool,
    /// Hovering a line away from any reference shows every reference on it, one after another
    /// (so a line's only reference hovers from anywhere on it); off by default
    pub hover_line: bool,
    /// How many bytes of a passage's text (a note's, an entry's) hover and completion
    /// documentation show before cutting it with ` …`: 2000 by default, 0 for no limit
    pub hover_max: Option<usize>,
    /**
    The hover's first line, a template: `{reference}` (as formatted), `{bible}` (the Bible whose
    text is shown first), and `{translation}` (the Bible the reference names; see
    [`fill_translation`]); `""` for no first line. Default [`DEFAULT_HOVER_TITLE`]
    */
    pub hover_title: Option<String>,
    /// What hovering a verse in a whole book shows: `"current,translations,references"` or a list
    /// ([`BookHoverField`]; `[]` for no hover); `references` by default
    pub book_hover: Option<OneOrMany>,
    /// A Bible's folder (a logoxide package, or USX files, one book each), for passage text in
    /// hover and go-to-definition
    pub bible: Option<String>,
    /// More Bibles (translations): package ids from the project's lock, or folders by name. A
    /// reference followed by a name or alias (`John 3:16 KJV`) shows that Bible's text. Without
    /// `bible` or `bibles`, the project's Bibles are used
    pub bibles: BiblesSetting,
    /// The logoxide library (else `LOGOXIDE_HOME`, else logoxide's config.toml's `library`, else
    /// `$XDG_DATA_HOME/logoxide`)
    pub library: Option<String>,
    /// The folder with the `logoxide.toml` and `logoxide.lock` naming the Bibles (else a workspace
    /// folder that has one, else the library's own)
    pub project: Option<String>,
    /// The Bible for references that name none: `bible`'s, else the first of `bibles`
    pub default_bible: Option<String>,
    /// The Bibles whose text hover shows, after the one the reference names (`["BSB", "KJV"]`);
    /// only that one (or the default) without it. One numbered differently (its package's
    /// `versification`) shows the passage as it numbers it, when the project has a map package
    /// between the two, labeled with that numbering
    pub hover_bibles: OneOrMany,
    pub markup: MarkupSettings,
    /// Commentaries whose notes on the passage hover shows after its text, each note labeled
    /// with the commentary's name and the verses it's on: `true` for every commentary the
    /// project uses, or their ids (`["mhcc"]`); none by default
    pub commentaries: CommentariesSetting,
    /// A commentary's note also covers the verses after it up to its next note in the chapter,
    /// so hovering a verse it skips shows the note before it; off by default, since a
    /// commentary may skip verses on purpose
    pub stretch_notes: bool,
    /// Hovering a Strong's number (`G25`, `H430`, `G0025`) shows its entry in the project's
    /// Strong's dictionaries (`keys = "strongs-greek"` or `"strongs-hebrew"`); off by default
    pub strongs: bool,
    /// Named templates for a reference (with a `bible` for their text), offered as code actions
    /// and to copy, and named by `inlay-hints`
    pub templates: BTreeMap<String, Template>,
    /// The old `insertions`, now `templates` (an error, saying so)
    pub insertions: Option<Value>,
    /// Read this config file instead of `~/.config/topos/config.toml`
    pub config: Option<String>,
    /// Don't read a config file
    pub no_config: bool,
}

/// What the server works with, built from [`Settings`]
pub struct Configured {
    pub matcher: BibleMatcher,
    pub format: FormatOptions,
    pub inlay_hints: InlayHints,
    pub reference_diagnostics: ReferenceDiagnostics,
    /// The `reference-diagnostics-template` setting
    pub reference_diagnostics_template: Option<TemplateRef>,
    /// The `complete-typed` setting
    pub complete_typed: bool,
    pub hover: Vec<HoverField>,
    /// The `hover-trim` setting
    pub hover_trim: bool,
    /// The `hover-line` setting
    pub hover_line: bool,
    /// The `hover-max` setting: none for no limit
    pub hover_limit: Option<usize>,
    /// The `hover-title` setting
    pub hover_title: String,
    /// The `book-hover` setting
    pub book_hover: Vec<BookHoverField>,
    pub extensions: Vec<String>,
    /// The `search-folders` setting, `~` expanded and relative ones in the first root
    pub search_folders: Vec<PathBuf>,
    pub bibles: Bibles,
    /// The `hover-bibles` setting, as the Bibles' names
    pub hover_bibles: Vec<String>,
    /// The `commentaries` setting's commentaries, in the project's order
    pub commentaries: Vec<Bible>,
    /// The `stretch-notes` setting
    pub stretch_notes: bool,
    /// The `strongs` setting's dictionaries (none when it's off)
    pub dictionaries: Vec<Dictionary>,
    pub templates: BTreeMap<String, Template>,
    /// The `epub` and `pdf` settings
    pub epub: bool,
    pub pdf: bool,
    /// Everything that changes which references are found, for topos's cache (the CLI's form, so
    /// the two share its entries)
    pub fingerprint: String,
}

/// `psg_fmt` and `psg-fmt` are the same key (only at the top level: the format's own fields are
/// snake_case, like `join_adjacent`)
fn normalize(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map
            .into_iter()
            .map(|(key, value)| (key.replace('_', "-"), value))
            .collect(),
        _ => Map::new(),
    }
}

/// Editor settings may be the settings themselves or nested under `logoxide`
/// (`settings = { logoxide = { ... } }` in Neovim)
pub fn editor_layer(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(mut map) if map.contains_key("logoxide") => {
            normalize(map.remove("logoxide").unwrap_or_default())
        }
        value => normalize(value),
    }
}

/// `$XDG_CONFIG_HOME/topos/config.toml`, else `~/.config/topos/config.toml`
fn default_config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("topos").join("config.toml"))
}

fn expand_home(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home).join(rest),
        _ => PathBuf::from(path),
    }
}

/// The config file's settings (none if there is no file, or the editor turns it off)
pub fn config_layer(editor: &Map<String, Value>) -> Result<Map<String, Value>, String> {
    if editor.get("no-config").and_then(Value::as_bool) == Some(true) {
        return Ok(Map::new());
    }
    let path = match editor.get("config").and_then(Value::as_str) {
        Some(path) => expand_home(path),
        None => match default_config_path().filter(|path| path.exists()) {
            Some(path) => path,
            None => return Ok(Map::new()),
        },
    };
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let table: toml::Table = text
        .parse()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let value = serde_json::to_value(table).map_err(|e| e.to_string())?;
    Ok(normalize(value))
}

/// The config file, then the editor's settings on top
pub fn settings(
    config: Map<String, Value>,
    editor: &Map<String, Value>,
) -> Result<Settings, String> {
    let mut merged = config;
    merged.extend(editor.clone());
    serde_json::from_value(Value::Object(merged)).map_err(|e| format!("logoxide settings: {e}"))
}

impl Settings {
    /// The defaults, then `psg-fmt`, then each `fmt-*`, then `format` (like the CLI)
    pub fn format_options(&self) -> FormatOptions {
        let mut options = self.psg_fmt.clone().unwrap_or_default();
        if let Some(FormatSetting::Options(whole)) = &self.format {
            options = whole.clone();
        }
        let text = |value: &Option<String>, field: &mut String| {
            if let Some(value) = value {
                field.clone_from(value);
            }
        };
        text(&self.fmt_book_separator, &mut options.book_separator);
        text(&self.fmt_chapter_verse, &mut options.chapter_verse);
        text(&self.fmt_range, &mut options.range);
        text(&self.fmt_verse_separator, &mut options.verse_separator);
        text(&self.fmt_chapter_separator, &mut options.chapter_separator);
        if let Some(join) = self.fmt_join_adjacent {
            options.join_adjacent = join;
        }
        if let Some(omit) = self.fmt_omit_first_verse_of_chapter_range {
            options.omit_first_verse_of_chapter_range = omit;
        }
        if let Some(chapter) = self.fmt_chapter_in_single_chapter_books {
            options.chapter_in_single_chapter_books = chapter;
        }
        if let Some(FormatSetting::Style(style)) = self.format {
            options.book = style;
        }
        options
    }

    /// `data` (or the defaults), then each `merge-data`, then each `remove-data`, and the book
    /// context (like the CLI)
    /**
    Builds what the server uses
    - `roots` are the workspace folders, where a project (`logoxide.toml`) may be
    - `machine` is this machine's library (the server's; tests have none), which the `library`
      setting overrides
    */
    pub fn configure(
        &self,
        roots: &[PathBuf],
        machine: Option<&Path>,
    ) -> Result<Configured, String> {
        fn read<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
            let text =
                std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
        }
        let mut input = match &self.data {
            Some(path) => read::<BibleDataInput>(&expand_home(path))?,
            None => BibleDataInput::defaults(),
        };
        for path in self.merge_data.values() {
            let path = expand_home(&path);
            input
                .merge(read::<DataPatch>(&path)?)
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
        for path in self.remove_data.values() {
            let path = expand_home(&path);
            input
                .remove(read::<DataPatch>(&path)?)
                .map_err(|e| format!("{}: {e}", path.display()))?;
        }
        let data = BibleData::new(input).map_err(|e| e.to_string())?;
        let context = match (&self.context_book, &self.context_heading) {
            (Some(book), _) => {
                Some(BookContext::Book(data.books().search(book).ok_or_else(
                    || format!("context-book: unknown book `{book}`"),
                )?))
            }
            (None, Some(pattern)) => Some(
                BookContext::headings(data.books(), pattern)
                    .map_err(|e| format!("context-heading: {e}"))?,
            ),
            (None, None) => None,
        };
        let bibles = self.bibles(&data, roots, machine)?;
        let commentaries = self.commentaries(&data, roots, machine)?;
        let dictionaries = self.dictionaries(roots, machine)?;
        let matcher = BibleFilter::new(data).create_matcher();
        // As the CLI writes it: the data files' text and the book context
        let contents = |paths: Vec<String>| -> Vec<Option<String>> {
            (paths.iter())
                .map(|path| std::fs::read_to_string(expand_home(path)).ok())
                .collect()
        };
        let config = format!(
            "{:?}",
            (
                &self.context_book,
                &self.context_heading,
                contents(self.data.iter().cloned().collect()),
                contents(self.merge_data.values()),
                contents(self.remove_data.values()),
            )
        );
        let fingerprint = topos_bible_index::files::fingerprint(
            &config,
            false,
            topos_bible_index::files::CfiOptions::default(),
        );
        let extensions = self
            .ext
            .values()
            .iter()
            .flat_map(|ext| ext.split(','))
            .map(|ext| ext.trim().trim_start_matches('.').to_ascii_lowercase())
            .filter(|ext| !ext.is_empty())
            .collect();
        if self.insertions.is_some() {
            return Err(String::from(
                "insertions: now `templates`, by name: `templates = { callout = { display = ..., \
                 action = \"insert\" } }` (`replace = \"line\"` is `action = \"replace-line\"`, \
                 `replace = \"inline\"` is `action = \"replace\"`)",
            ));
        }
        if let InlayHints::Template(TemplateRef::Named(name)) = &self.inlay_hints
            && !self.templates.contains_key(name)
        {
            return Err(format!(
                "inlay-hints: expected changed, always, osis, usfm, never, a template's name, or a \
                 template with {{placeholders}}, not {name:?}"
            ));
        }
        let hover = match &self.hover {
            None => HoverField::DEFAULT.to_vec(),
            Some(fields) => fields
                .values()
                .iter()
                .flat_map(|field| field.split(','))
                .map(str::trim)
                .filter(|field| !field.is_empty())
                .map(|field| {
                    serde_json::from_value(Value::String(field.to_ascii_lowercase()))
                        .map_err(|_| format!("hover: unknown field `{field}`"))
                })
                .collect::<Result<_, _>>()?,
        };
        Ok(Configured {
            matcher: match context {
                Some(context) => matcher.with_context(context),
                None => matcher,
            },
            format: self.format_options(),
            inlay_hints: self.inlay_hints.clone(),
            reference_diagnostics: self.reference_diagnostics,
            reference_diagnostics_template: (self.reference_diagnostics_template.as_deref())
                .map(|value| {
                    TemplateRef::read("reference-diagnostics-template", value, &self.templates)
                })
                .transpose()?,
            complete_typed: self.complete_typed.unwrap_or(true),
            hover,
            hover_trim: self.hover_trim,
            hover_line: self.hover_line,
            hover_limit: match self.hover_max {
                Some(0) => None,
                max => Some(max.unwrap_or(crate::bible::MAX_HOVER_TEXT)),
            },
            hover_title: (self.hover_title.clone())
                .unwrap_or_else(|| DEFAULT_HOVER_TITLE.to_string()),
            book_hover: match &self.book_hover {
                None => vec![BookHoverField::References],
                Some(fields) => (fields.values().iter())
                    .flat_map(|field| field.split(','))
                    .map(str::trim)
                    .filter(|field| !field.is_empty())
                    .map(|field| {
                        serde_json::from_value(Value::String(field.to_ascii_lowercase())).map_err(
                            |_| {
                                format!(
                                    "book-hover: expected references, translations, or current, not \
                                     `{field}`"
                                )
                            },
                        )
                    })
                    .collect::<Result<_, _>>()?,
            },
            extensions,
            search_folders: (self.search_folders.values().iter())
                .map(|folder| folder.trim())
                .filter(|folder| !folder.is_empty())
                .map(|folder| match (expand_home(folder), roots.first()) {
                    (path, Some(root)) if path.is_relative() => root.join(path),
                    (path, _) => path,
                })
                .collect(),
            hover_bibles: (self.hover_bibles.values().iter())
                .flat_map(|names| names.split(','))
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(|name| {
                    bibles
                        .get(name)
                        .map(|bible| bible.name.clone())
                        .ok_or_else(|| format!("hover-bibles: no Bible is named `{name}`"))
                })
                .collect::<Result<_, _>>()?,
            bibles,
            commentaries,
            stretch_notes: self.stretch_notes,
            dictionaries,
            templates: self.templates.clone(),
            epub: self.epub,
            pdf: self.pdf,
            fingerprint,
        })
    }

    /**
    `bible`, then each of `bibles`, with `default-bible` (or `bible`) first; without either, the
    project's Bibles (if there's a project)
    */
    fn bibles(
        &self,
        data: &BibleData,
        roots: &[PathBuf],
        machine: Option<&Path>,
    ) -> Result<Bibles, String> {
        let mut list = vec![];
        let cache = book_cache_dir(machine);
        if let Some(folder) = &self.bible {
            list.push(Bible::load(
                &expand_home(folder),
                None,
                data.clone(),
                self.markup.clone(),
                cache.as_deref(),
            )?);
        }
        let add = |bible: Bible, list: &mut Vec<Bible>| {
            list.retain(|b: &Bible| b.name != bible.name);
            list.push(bible);
        };
        match &self.bibles {
            BiblesSetting::Folders(folders) => {
                for (name, setting) in folders {
                    let path = expand_home(setting.path());
                    let mut bible = Bible::load(
                        &path,
                        Some(name),
                        data.clone(),
                        self.markup.clone(),
                        cache.as_deref(),
                    )
                    .map_err(|e| format!("bibles.{name}: {e}"))?;
                    if !setting.aliases().is_empty() {
                        bible.aliases = setting.aliases().to_vec();
                    }
                    add(bible, &mut list);
                }
            }
            BiblesSetting::Packages(ids) => {
                let (project, library) = self.project(roots, machine)?
                    .ok_or("bibles: no logoxide.toml names packages (run `lx add` in the workspace or the library)")?;
                for id in ids {
                    add(self.package_bible(&project, &library, id, data)?, &mut list);
                }
            }
        }
        if self.bible.is_none()
            && self.bibles.is_empty()
            && let Some((project, library)) = self.project(roots, machine)?
        {
            // Those turned off (`enabled = false`) are left out; naming one in `bibles` reads it
            let ids: Vec<String> = (project.lock())
                .map(|l| l.roots().map(|p| p.dependency_name().to_string()).collect())
                .unwrap_or_default();
            let ids: Vec<String> = ids
                .into_iter()
                .filter(|id| project.is_enabled(id))
                .collect();
            for id in ids.iter().filter(|id| bible_kind(&project, &library, id)) {
                add(self.package_bible(&project, &library, id, data)?, &mut list);
            }
        }
        if let Some(default) = &self.default_bible {
            let at = (list.iter())
                .position(|b| b.name.eq_ignore_ascii_case(default) || b.aliases.contains(default))
                .ok_or_else(|| format!("default-bible: no Bible is named `{default}`"))?;
            let bible = list.remove(at);
            list.insert(0, bible);
        }
        // The project's versification maps carry a reference between Bibles numbered
        // differently (hover shows the other Bible's numbering); none without a project
        let maps = (self.project(roots, machine).ok().flatten())
            .and_then(|(project, library)| project.versifications(&library).ok())
            .unwrap_or_default();
        Ok(Bibles::new(list).with_maps(maps))
    }
}

impl Settings {
    /// The project naming the Bibles (the `project` setting, else a workspace folder with a
    /// `logoxide.toml`, else the library's own) and the library they're installed in
    fn project(
        &self,
        roots: &[PathBuf],
        machine: Option<&Path>,
    ) -> Result<Option<(Project, Library)>, String> {
        let library_root = match (&self.library, machine) {
            (Some(path), _) => Some(expand_home(path)),
            (None, Some(machine)) => Some(machine.to_path_buf()),
            (None, None) => None,
        };
        let dir = match &self.project {
            Some(path) => Some(expand_home(path)),
            None => roots
                .iter()
                .find(|root| root.join(MANIFEST).is_file())
                .cloned()
                .or_else(|| {
                    library_root
                        .clone()
                        .filter(|root| root.join(MANIFEST).is_file())
                }),
        };
        let Some(dir) = dir else {
            return Ok(None);
        };
        let project = Project::open(&dir).map_err(|e| format!("project {}: {e}", dir.display()))?;
        // The cache logoxide's config names, like the library
        let config = logoxide::config::Config::load().unwrap_or_default();
        let library = config.library(library_root, None);
        Ok(Some((project, library)))
    }

    /**
    The `commentaries` setting's commentaries: every one the project uses (`true`; those turned
    off are left out), or those it names, in the project's order; each opened once, its books
    parsed when first needed and kept as a Bible's are
    */
    fn commentaries(
        &self,
        data: &BibleData,
        roots: &[PathBuf],
        machine: Option<&Path>,
    ) -> Result<Vec<Bible>, String> {
        let ids = self.commentaries.ids();
        if ids.as_ref().is_some_and(Vec::is_empty) {
            return Ok(vec![]);
        }
        let (project, library) = self.project(roots, machine)?.ok_or(
            "commentaries: no logoxide.toml names packages (run `lx add` in the workspace or the library)",
        )?;
        let found = (project.commentaries(&library)).map_err(|e| format!("commentaries: {e}"))?;
        let named = |locked: &logoxide::LockedPackage, id: &str| {
            locked.dependency_name() == id || locked.id == id
        };
        if let Some(ids) = &ids
            && let Some(missing) = (ids.iter()).find(|id| !found.iter().any(|(l, _)| named(l, id)))
        {
            return Err(format!(
                "commentaries: `{missing}` isn't a commentary {} uses",
                project.dir().join(MANIFEST).display()
            ));
        }
        let cache = book_cache_dir(machine);
        let mut list = vec![];
        for (locked, package) in found {
            if ids
                .as_ref()
                .is_some_and(|ids| !ids.iter().any(|id| named(locked, id)))
            {
                continue;
            }
            let (name, aliases) = project.names(locked, package.info());
            let dir = library.package_dir(locked, project.dir());
            let mut commentary = Bible::from_package(
                package,
                &dir,
                Some(&name),
                data.clone(),
                self.markup.clone(),
                cache.as_deref(),
            )
            .map_err(|e| format!("commentaries: {}: {e}", locked.dependency_name()))?;
            commentary.aliases = aliases;
            list.push(commentary);
        }
        Ok(list)
    }

    /// The project's Strong's dictionaries, with `strongs` on (none without a project)
    fn dictionaries(
        &self,
        roots: &[PathBuf],
        machine: Option<&Path>,
    ) -> Result<Vec<Dictionary>, String> {
        if !self.strongs {
            return Ok(vec![]);
        }
        let Some((project, library)) = self.project(roots, machine)? else {
            return Ok(vec![]);
        };
        let found = (project.dictionaries(&library)).map_err(|e| format!("strongs: {e}"))?;
        Ok((found.into_iter())
            .filter_map(|(locked, package)| {
                let (name, _) = project.names(locked, package.info());
                Dictionary::new(name, package, self.markup.clone())
            })
            .collect())
    }

    /// A package from the project's lock file, as a Bible named by its abbreviation
    fn package_bible(
        &self,
        project: &Project,
        library: &Library,
        id: &str,
        data: &BibleData,
    ) -> Result<Bible, String> {
        let lock = project.lock().ok_or_else(|| {
            format!(
                "bibles: {} has no logoxide.lock (run `lx install` there)",
                project.dir().display()
            )
        })?;
        let locked = lock.get(id).ok_or_else(|| {
            format!(
                "bibles: `{id}` isn't in {} (run `lx add`)",
                project.dir().join(MANIFEST).display()
            )
        })?;
        if !library.is_installed(locked, project.dir()) {
            return Err(format!(
                "bibles: {} isn't installed (run `lx install`)",
                locked.name()
            ));
        }
        let dir = library.package_dir(locked, project.dir());
        let mut bible = Bible::load(
            &dir,
            None,
            data.clone(),
            self.markup.clone(),
            Some(library.cache_dir()),
        )
        .map_err(|e| format!("bibles: {id}: {e}"))?;
        // The project's own names for it, if it gives any (a fork named `KJVB`)
        let package = library
            .open(locked, project.dir())
            .map_err(|e| format!("bibles: {id}: {e}"))?;
        (bible.name, bible.aliases) = project.names(locked, package.info());
        Ok(bible)
    }
}

/// Where parsed books are kept: logoxide's cache (its config's, as `lx` and the library's
/// packages use it); none without a machine library (the tests), so books stay in memory
fn book_cache_dir(machine: Option<&Path>) -> Option<PathBuf> {
    machine?;
    let config = logoxide::config::Config::load().unwrap_or_default();
    Some(config.library(None, None).cache_dir().to_path_buf())
}

/// Whether a project's package is a Bible (not a lexicon or cross references)
fn bible_kind(project: &Project, library: &Library, id: &str) -> bool {
    let locked = project
        .lock()
        .and_then(|l| l.roots().find(|p| p.dependency_name() == id).cloned());
    locked
        .and_then(|locked| library.open(&locked, project.dir()).ok())
        .is_some_and(|package| package.info().kind == PackageKind::Bible)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn parse(config: &str, editor: Value) -> Settings {
        let table: toml::Table = config.parse().unwrap();
        let config = normalize(serde_json::to_value(table).unwrap());
        settings(config, &editor_layer(editor)).unwrap()
    }

    #[test]
    fn config_then_editor() {
        let settings = parse(
            "mode = \"grouped\"\nformat = \"abbreviation\"\npsg-fmt = { join_adjacent = true }\nfmt-range = \"–\"\n",
            json!({ "logoxide": { "fmt_range": "~", "inlay-hints": "osis" } }),
        );
        let options = settings.format_options();
        assert_eq!(options.book, BookStyle::Abbreviation);
        assert!(options.join_adjacent);
        // The editor's setting wins, and snake_case keys work
        assert_eq!(options.range, "~");
        assert_eq!(settings.inlay_hints, InlayHints::Osis);
    }

    #[test]
    fn format_as_a_whole_object() {
        let settings = parse(
            "",
            json!({ "format": { "book": "osis", "chapter_verse": "." } }),
        );
        let options = settings.format_options();
        assert_eq!(
            (options.book, options.chapter_verse.as_str()),
            (BookStyle::Osis, ".")
        );
    }

    #[test]
    fn extensions_and_bad_values() {
        let settings = parse("ext = \"md, .TXT\"", json!({}));
        assert_eq!(
            settings.configure(&[], None).unwrap().extensions,
            ["md", "txt"]
        );
        // Not a mode, so a template's name, and there's none by that name
        let sometimes = parse("inlay-hints = \"sometimes\"", json!({})).configure(&[], None);
        assert!(sometimes.err().unwrap().contains("a template's name"));
    }

    #[test]
    fn hover_fields() {
        let default = parse("", json!({})).configure(&[], None).unwrap().hover;
        assert_eq!(default, HoverField::DEFAULT);
        let listed = parse("hover = \"OSIS, bcv\"", json!({}))
            .configure(&[], None)
            .unwrap();
        assert_eq!(listed.hover, [HoverField::Osis, HoverField::Bcv]);
        let none = parse("", json!({ "hover": [] }))
            .configure(&[], None)
            .unwrap();
        assert!(none.hover.is_empty());
        let unknown = parse("hover = [\"chapter\"]", json!({})).configure(&[], None);
        assert_eq!(unknown.err().unwrap(), "hover: unknown field `chapter`");
    }
}
