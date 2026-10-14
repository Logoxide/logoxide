use std::{
    collections::{BTreeMap, HashMap, HashSet},
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};

use lsp_types::{
    CodeAction, CodeActionOrCommand, CodeActionParams, CodeActionProviderCapability, Command,
    CompletionItem, CompletionItemKind, CompletionOptions, CompletionParams, CompletionResponse,
    Diagnostic, DiagnosticSeverity, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DocumentSymbol, DocumentSymbolParams, DocumentSymbolResponse,
    ExecuteCommandOptions, ExecuteCommandParams, GotoDefinitionParams, GotoDefinitionResponse,
    Hover, HoverContents, HoverParams, HoverProviderCapability, InlayHint, InlayHintLabel,
    InlayHintParams, Location, MarkupContent, MarkupKind, OneOf, Position,
    PublishDiagnosticsParams, Range, ReferenceParams, SemanticToken, SemanticTokenType,
    SemanticTokens, SemanticTokensFullOptions, SemanticTokensLegend, SemanticTokensOptions,
    SemanticTokensParams, SemanticTokensServerCapabilities, ServerCapabilities, SymbolKind,
    TextDocumentPositionParams, TextDocumentSyncCapability, TextDocumentSyncKind, TextEdit, Uri,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use topos_bible::{
    data::{books::BookId, translations::Translation},
    filter::{bible_filter::BibleFilter, filters::testament::TestamentFilter},
    matcher::{BibleMatch, BibleMatcher, LineIndex, TranslatedMatch},
    segments::{
        Passage,
        autocomplete::{CompleteOptions, CompletionKind},
        coverage::VerseRange,
        formatter::{BookStyle, FormatOptions},
    },
};

mod actions;
mod book;
mod completion;
mod diagnostics;
mod hover;
mod search;
mod template;
pub use search::Search;
#[cfg(test)]
mod tests;

use crate::{
    bible::{self, Bible, Bibles, Part, Token},
    dictionary::{self, Dictionary},
    settings::{
        self, BookHoverField, Configured, HoverField, InlayHints, ReferenceDiagnostics, Template,
        TemplateAction, TemplateRef,
    },
    workspace,
};

/// How long hovering a verse in a whole book searches for its references before giving up
const HOVER_SEARCH_TIME: Duration = Duration::from_millis(500);

/// The request for a whole book's text: `{ "uri": "logoxide://BSB/JHN" }` gives
/// [`BookText`]
pub const BOOK_REQUEST: &str = "logoxide/book";

#[derive(Debug, Deserialize)]
pub struct BookParams {
    pub uri: String,
}

/**
The request for a book's text (an EPUB's or PDF's, `{ "uri": "logoxide-file:///…/Gill.epub" }`),
which the `epub` and `pdf` settings' references are in: gives [`BookText`] (its title is the file's name)
*/
pub const FILE_TEXT_REQUEST: &str = "logoxide/fileText";

/// The request for the Bibles: no parameters, gives [`BiblesList`]
pub const BIBLES_REQUEST: &str = "logoxide/bibles";

/// The Bibles from the `bible` and `bibles` settings, the default first
#[derive(Debug, Serialize)]
pub struct BiblesList {
    pub bibles: Vec<BibleInfo>,
}

#[derive(Debug, Serialize)]
pub struct BibleInfo {
    /// Its name in `logoxide://` URIs and after references: `BSB`
    pub name: String,
    /// Other ways to write it after a reference
    pub aliases: Vec<String>,
    /// The books it has, as USFM codes (`JHN`), in order
    pub books: Vec<String>,
}

/**
The request for the same verse in another Bible: `{ "uri": "logoxide://BSB/JHN", "position":
{ "line": 3, "character": 0 }, "bible": "KJV" }` gives the [`Location`] of the verse at that
position (or the chapter, for a chapter's label) in the KJV's John, or `null` if it lacks the book
*/
pub const SAME_VERSE_REQUEST: &str = "logoxide/sameVerse";

#[derive(Debug, Deserialize)]
pub struct SameVerseParams {
    pub uri: String,
    pub position: Position,
    /// The other Bible's name or alias
    pub bible: String,
}

/**
The request for what each template writes at a reference: `{ "textDocument": { "uri": … },
"position": … }` gives a [`TemplateChoice`] per template and Bible (the one the reference names,
else each, the default first; just the passage's text, as `text`, when there are no templates), or
`[]` when no reference is there. Editors use it to copy a passage in a form, or to choose a code
action's template
*/
pub const TEMPLATES_REQUEST: &str = "logoxide/templates";

/// One template, filled for a reference
#[derive(Debug, Serialize)]
pub struct TemplateChoice {
    /// The template's name: `callout`
    pub name: String,
    /// Its name with the Bible's, when there are several: `callout (KJV)`
    pub label: String,
    /// The Bible its text is from
    pub bible: String,
    /// What it writes
    pub text: String,
    /// With an `action`: its code action's title (`Insert callout`) and edit (in the document
    /// as it is)
    pub title: Option<String>,
    pub edit: Option<TextEdit>,
}

/// A whole book, for the editor to show as a read-only document
#[derive(Debug, Serialize)]
pub struct BookText {
    /// The book's name: `John`
    pub title: String,
    pub text: String,
}

/// The command code actions run; its result is the matching locations
pub const SEARCH_COMMAND: &str = "logoxide.search";

/**
The command reformat actions use in unnamed documents: `{ "edits": [TextEdit] }` for the
editor to apply to the document the action came from
- A workspace edit names its document by URI, and an unnamed document's (`file://`) names no
  buffer an editor can find (Neovim makes a new one), so the edits go with the action instead
*/
pub const APPLY_EDITS_COMMAND: &str = "logoxide.applyEdits";

/// Whether a URI names no file (an unnamed buffer's `file://`)
pub fn is_unnamed(uri: &Uri) -> bool {
    workspace::uri_to_path(uri).is_none_or(|path| path.as_os_str().is_empty())
}

/// The kinds of workspace search, like the CLI's passage filters
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SearchMode {
    /// `-o`: references that name a verse of the passage (whole chapters don't count)
    ExplicitOverlap,
    /// `--any-overlap`: references that share any verse with it
    AnyOverlap,
    /// `--exact-overlap`: references that are exactly it (go-to-references)
    ExactOverlap,
    /// `-i`: references entirely inside it
    Inside,
}

impl SearchMode {
    const ALL: [SearchMode; 4] = [
        SearchMode::ExplicitOverlap,
        SearchMode::AnyOverlap,
        SearchMode::ExactOverlap,
        SearchMode::Inside,
    ];

    fn title(self, reference: &str) -> String {
        match self {
            SearchMode::ExplicitOverlap => format!("Search \"{reference}\" for explicit overlap"),
            SearchMode::AnyOverlap => format!("Search \"{reference}\" for any overlap"),
            SearchMode::ExactOverlap => format!("Search \"{reference}\" for exact overlap"),
            SearchMode::Inside => format!("Search inside \"{reference}\""),
        }
    }
}

/// The arguments of [`SEARCH_COMMAND`]
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchArguments {
    pub mode: SearchMode,
    /// The passage, written so it parses back (`John 3:16`)
    pub passage: String,
}

/// Open documents and the matcher, with one method per LSP request or notification
pub struct Server {
    matcher: Arc<BibleMatcher>,
    /// How references are written: completions, hover, symbols, hints, and action titles
    format: FormatOptions,
    inlay_hints: InlayHints,
    reference_diagnostics: ReferenceDiagnostics,
    /// The reference diagnostics' text (`reference-diagnostics-template`): the reference by default
    reference_diagnostics_template: Option<TemplateRef>,
    /// What the hover shows below the reference
    hover: Vec<HoverField>,
    /// Extensions searched in the workspace (empty for every text file)
    extensions: Vec<String>,
    documents: HashMap<Uri, String>,
    /// The workspace folders, for workspace-wide searches
    roots: Vec<PathBuf>,
    /// More folders those searches cover (`search-folders`)
    search_folders: Vec<PathBuf>,
    /// The editor's settings (initialization options, then the latest configuration)
    editor: Map<String, Value>,
    /// Each workspace file's references, reused while it is unchanged
    cache: Arc<workspace::Cache>,
    /// Searching EPUBs and PDFs too (the `epub` and `pdf` settings)
    books: Option<Arc<workspace::BookSearch>>,
    /// The `bible` and `bibles` settings' text, the default first
    bibles: Bibles,
    /// The Bibles whose text hover shows, after the one a reference names
    hover_bibles: Vec<String>,
    /// The commentaries whose notes hover shows after a passage's text (the `commentaries`
    /// setting)
    commentaries: Vec<Bible>,
    /// Whether a commentary's note covers the verses after it up to its next (`stretch-notes`)
    stretch_notes: bool,
    /// Whether completion offers the chapter or verse already typed (`complete-typed`)
    complete_typed: bool,
    /// Whether hover leaves out a text's leading whitespace (`hover-trim`)
    hover_trim: bool,
    /// Whether hovering a line away from any reference shows every reference on it (`hover-line`)
    hover_line: bool,
    /// How many bytes of a text hover shows (`hover-max`; none for no limit)
    hover_limit: Option<usize>,
    /// The hover's first line (`hover-title`; empty for none)
    hover_title: String,
    /// What hovering a verse in a whole book shows (`book-hover`)
    book_hover: Vec<BookHoverField>,
    /// The Strong's dictionaries whose entries hover shows on a Strong's number (`strongs`)
    dictionaries: Vec<Dictionary>,
    /// This machine's logoxide library, where projects' packages are installed (none in tests)
    library: Option<PathBuf>,
    /// The named templates (code actions, copying, and inlay hints)
    templates: BTreeMap<String, Template>,
}

impl Server {
    pub fn new(matcher: BibleMatcher) -> Self {
        Self {
            matcher: Arc::new(matcher),
            format: FormatOptions::default(),
            inlay_hints: InlayHints::default(),
            reference_diagnostics: ReferenceDiagnostics::default(),
            reference_diagnostics_template: None,
            hover: HoverField::DEFAULT.to_vec(),
            extensions: vec![],
            documents: HashMap::new(),
            roots: vec![],
            search_folders: vec![],
            editor: Map::new(),
            cache: Arc::default(),
            books: None,
            bibles: Bibles::default(),
            hover_bibles: vec![],
            commentaries: vec![],
            stretch_notes: false,
            complete_typed: true,
            hover_trim: false,
            hover_line: false,
            hover_limit: Some(bible::MAX_HOVER_TEXT),
            hover_title: settings::DEFAULT_HOVER_TITLE.to_string(),
            book_hover: vec![BookHoverField::References],
            dictionaries: vec![],
            library: None,
            templates: BTreeMap::new(),
        }
    }

    pub fn set_roots(&mut self, roots: Vec<PathBuf>) {
        self.roots = roots;
    }

    /// This machine's logoxide library (`lx`'s), for projects and the `bibles` setting's package ids
    pub fn set_library(&mut self, library: PathBuf) {
        self.library = Some(library);
    }

    /**
    Applies the config file and the editor's settings (`initializationOptions` or
    `workspace/didChangeConfiguration`'s `settings`, either may nest them under `logoxide`)
    - On an error the previous settings stay, and the error says why
    */
    pub fn configure(&mut self, editor: Value) -> Result<(), String> {
        let editor = settings::editor_layer(editor);
        let config = settings::config_layer(&editor)?;
        let Configured {
            matcher,
            format,
            inlay_hints,
            reference_diagnostics,
            reference_diagnostics_template,
            complete_typed,
            hover,
            hover_trim,
            hover_line,
            hover_limit,
            hover_title,
            book_hover,
            extensions,
            search_folders,
            bibles,
            hover_bibles,
            commentaries,
            stretch_notes,
            dictionaries,
            templates,
            epub,
            pdf,
            fingerprint,
        } = settings::settings(config, &editor)?.configure(&self.roots, self.library.as_deref())?;
        self.search_folders = search_folders;
        self.books = workspace::BookSearch::new(epub, pdf, &fingerprint).map(Arc::new);
        self.bibles = bibles;
        self.hover_bibles = hover_bibles;
        self.commentaries = commentaries;
        self.stretch_notes = stretch_notes;
        self.complete_typed = complete_typed;
        self.hover_trim = hover_trim;
        self.hover_line = hover_line;
        self.hover_limit = hover_limit;
        self.hover_title = hover_title;
        self.book_hover = book_hover;
        self.dictionaries = dictionaries;
        self.templates = templates;
        self.matcher = Arc::new(matcher);
        self.format = format;
        self.inlay_hints = inlay_hints;
        self.reference_diagnostics = reference_diagnostics;
        self.reference_diagnostics_template = reference_diagnostics_template;
        self.hover = hover;
        self.extensions = extensions;
        self.editor = editor;
        // New data can change what every file contains (a new cache: a search still running with
        // the old settings fills the old one)
        self.cache = Arc::default();
        Ok(())
    }

    /// The open documents (whose diagnostics change with the settings)
    pub fn documents(&self) -> Vec<Uri> {
        self.documents.keys().cloned().collect()
    }

    pub fn capabilities() -> ServerCapabilities {
        ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Kind(TextDocumentSyncKind::FULL)),
            completion_provider: Some(CompletionOptions {
                trigger_characters: Some([" ", ":", "-", ",", ";", "."].map(String::from).to_vec()),
                // The passage's text, for chapters and verses
                resolve_provider: Some(true),
                ..CompletionOptions::default()
            }),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            document_symbol_provider: Some(OneOf::Left(true)),
            inlay_hint_provider: Some(OneOf::Left(true)),
            references_provider: Some(OneOf::Left(true)),
            definition_provider: Some(OneOf::Left(true)),
            declaration_provider: Some(lsp_types::DeclarationCapability::Simple(true)),
            // Only whole books have tokens: their verse numbers, words of Jesus, ...
            semantic_tokens_provider: Some(
                SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                    legend: SemanticTokensLegend {
                        token_types: Token::LEGEND.map(SemanticTokenType::new).to_vec(),
                        token_modifiers: vec![],
                    },
                    full: Some(SemanticTokensFullOptions::Bool(true)),
                    ..SemanticTokensOptions::default()
                }),
            ),
            code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
            execute_command_provider: Some(ExecuteCommandOptions {
                commands: vec![SEARCH_COMMAND.into()],
                ..ExecuteCommandOptions::default()
            }),
            ..ServerCapabilities::default()
        }
    }

    pub fn did_open(&mut self, params: DidOpenTextDocumentParams) {
        let doc = params.text_document;
        self.documents.insert(doc.uri, doc.text);
    }

    /// Only full syncs are advertised, so the last change is the whole document
    pub fn did_change(&mut self, params: DidChangeTextDocumentParams) {
        if let Some(change) = params.content_changes.into_iter().last() {
            self.documents.insert(params.text_document.uri, change.text);
        }
    }

    pub fn did_close(&mut self, params: DidCloseTextDocumentParams) {
        self.documents.remove(&params.text_document.uri);
    }

    /// The reference at a position (or at the translation named after it), with the document's
    /// line index
    fn reference_at(
        &self,
        uri: &Uri,
        position: Position,
    ) -> Option<(TranslatedMatch, LineIndex<'_>)> {
        let text = self.documents.get(uri)?;
        let index = LineIndex::new(text);
        let cursor = index.offset_of_utf16(position.line as usize, position.character as usize)?;
        let found = self
            .matcher
            .search_with_translations(text, &self.bibles.translations)
            .into_iter()
            .find(|m| {
                let bytes = m.reference.location.bytes;
                let end = m.translation.as_ref().map_or(bytes.end, |t| t.span.end);
                bytes.start <= cursor && cursor <= end
            })?;
        Some((found, index))
    }

    /// The Bible and book of a whole-book document
    fn book_document(&self, uri: &Uri) -> Option<(&Bible, BookId)> {
        self.bibles.book_of(uri.as_str())
    }

    /// The reference in the configured format (OSIS references are written as OSIS ids)
    fn reference(&self, found: &BibleMatch) -> String {
        let data = self.matcher.data();
        let written = match self.format.book {
            BookStyle::Osis => found.psg.to_osis(data.books()),
            BookStyle::Usfm => found.psg.to_usfm(data.books()),
            _ => self.format.passage(&found.psg, data),
        };
        written.unwrap_or_else(|| found.psg.segments.to_string())
    }

    /// The passage written so the filters can read it back, whatever the configured format
    fn parseable(&self, passage: &Passage) -> String {
        FormatOptions::default()
            .passage(passage, self.matcher.data())
            .unwrap_or_else(|| passage.segments.to_string())
    }
}

fn lsp_position(index: &LineIndex, offset: usize) -> Position {
    let position = index.position(offset);
    Position::new(position.line as u32 - 1, position.utf16_column as u32 - 1)
}

pub fn lsp_range(index: &LineIndex, start: usize, end: usize) -> Range {
    Range::new(lsp_position(index, start), lsp_position(index, end))
}
