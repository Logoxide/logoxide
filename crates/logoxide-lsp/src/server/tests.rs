use std::str::FromStr;

use std::path::Path;

use lsp_types::{
    PartialResultParams, TextDocumentIdentifier, TextDocumentItem, TextDocumentPositionParams,
    WorkDoneProgressParams,
};

use serde_json::json;

use super::*;

fn server(text: &str) -> (Server, Uri) {
    let uri = Uri::from_str("file:///notes.md").unwrap();
    let mut server = Server::new(BibleMatcher::default());
    server.did_open(DidOpenTextDocumentParams {
        text_document: TextDocumentItem::new(uri.clone(), "markdown".into(), 1, text.into()),
    });
    (server, uri)
}

fn at(uri: &Uri, line: u32, character: u32) -> TextDocumentPositionParams {
    TextDocumentPositionParams::new(
        TextDocumentIdentifier::new(uri.clone()),
        Position::new(line, character),
    )
}

/// The shared completion cases, as an editor asks: a copy of topos-lib's
/// `tests/cases/complete.txt` (so the tests build without topos beside logoxide), to copy
/// again when topos's changes (tests/fixtures/README.md)
#[test]
fn completion_cases() {
    let cases = include_str!("../../tests/fixtures/complete.txt");
    let mut failures = vec![];
    for line in cases
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .filter(|l| !l.starts_with("apply: "))
    {
        let (input, expected) = line.split_once(" =>").unwrap();
        let (mut server, uri) = server(
            input
                .trim_start_matches("[join] ")
                .trim_start_matches("[abbreviation] "),
        );
        let format = serde_json::json!({
            "join_adjacent": input.starts_with("[join] "),
            "book": if input.starts_with("[abbreviation] ") { "abbreviation" } else { "name" },
        });
        server
            .configure(serde_json::json!({ "no-config": true, "psg-fmt": format }))
            .unwrap();
        let input = input
            .trim_start_matches("[join] ")
            .trim_start_matches("[abbreviation] ");
        let character = input.encode_utf16().count() as u32;
        let labels: Vec<String> = match server.completion(CompletionParams {
            text_document_position: at(&uri, 0, character),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        }) {
            Some(CompletionResponse::List(list)) => {
                let items = list.items;
                items.into_iter().map(|item| item.label).collect()
            }
            _ => vec![],
        };
        let expected: Vec<&str> = expected
            .split(" | ")
            .map(str::trim)
            .filter(|e| !e.is_empty())
            .collect();
        let ok = if expected.is_empty() {
            labels.is_empty()
        } else {
            labels.len() >= expected.len() && labels[..expected.len()] == expected[..]
        };
        if !ok {
            failures.push(format!("{input:?}: expected {expected:?}, got {labels:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The shared `apply:` completion cases (`|` is the cursor), as an editor asks
#[test]
fn applied_completion_cases() {
    let cases = include_str!("../../tests/fixtures/complete.txt");
    let mut failures = vec![];
    for case in cases.lines().filter_map(|l| l.strip_prefix("apply: ")) {
        let (input, expected) = case.split_once(" => ").unwrap();
        let (input, expected) = (input.replace("\\n", "\n"), expected.replace("\\n", "\n"));
        let (input, expected) = (input.as_str(), expected.as_str());
        let character = input[..input.find('|').unwrap()].encode_utf16().count() as u32;
        let text = input.replacen('|', "", 1);
        let (server, uri) = server(&text);
        let first = match server.completion(CompletionParams {
            text_document_position: at(&uri, 0, character),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        }) {
            Some(CompletionResponse::List(list)) => list.items.into_iter().next(),
            _ => None,
        };
        let applied = first.and_then(|item| match item.text_edit? {
            lsp_types::CompletionTextEdit::Edit(edit) => {
                let index = LineIndex::new(&text);
                let start = index.offset_of_utf16(0, edit.range.start.character as usize)?;
                let end = index.offset_of_utf16(0, edit.range.end.character as usize)?;
                let mut applied = text.clone();
                applied.replace_range(start..end, &edit.new_text);
                Some(applied)
            }
            lsp_types::CompletionTextEdit::InsertAndReplace(_) => None,
        });
        if applied.as_deref() != Some(expected) {
            failures.push(format!("{input:?}: expected {expected:?}, got {applied:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn completes_with_utf16_ranges() {
    // `𝄞` is 2 UTF-16 units, so `jn` starts at character 3
    let (server, uri) = server("intro\n𝄞 jn 3:1");
    let Some(CompletionResponse::List(lsp_types::CompletionList {
        items,
        is_incomplete: true,
    })) = server.completion(CompletionParams {
        text_document_position: at(&uri, 1, 9),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        context: None,
    })
    else {
        panic!("no completions");
    };
    let first = &items[0];
    assert_eq!(first.label, "John 3:1");
    assert_eq!(first.filter_text.as_deref(), Some("jn 3:1 John 3:1"));
    let Some(lsp_types::CompletionTextEdit::Edit(edit)) = &first.text_edit else {
        panic!("no edit");
    };
    assert_eq!(
        edit.range,
        Range::new(Position::new(1, 3), Position::new(1, 9))
    );
}

#[test]
fn diagnostics_for_references_and_missing_verses() {
    let (mut server, uri) = server("Fine: jn 3:16\nBad: John 3:99");
    let summary = |server: &Server| -> Vec<(Option<DiagnosticSeverity>, String, Range)> {
        server
            .diagnostics(&uri)
            .diagnostics
            .into_iter()
            .map(|d| (d.severity, d.message, d.range))
            .collect()
    };
    assert_eq!(
        summary(&server),
        [
            (
                Some(DiagnosticSeverity::INFORMATION),
                String::from("John 3:16"),
                Range::new(Position::new(0, 6), Position::new(0, 13))
            ),
            (
                Some(DiagnosticSeverity::WARNING),
                String::from("John 3:99 does not exist"),
                Range::new(Position::new(1, 10), Position::new(1, 14))
            ),
        ]
    );
    // Codes tell them apart
    let codes: Vec<_> = server
        .diagnostics(&uri)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect();
    assert_eq!(
        codes,
        [
            Some(lsp_types::NumberOrString::String("reference".into())),
            Some(lsp_types::NumberOrString::String("missing".into()))
        ]
    );

    server
        .configure(serde_json::json!({ "no-config": true, "reference-diagnostics": "hint" }))
        .unwrap();
    assert_eq!(summary(&server)[0].0, Some(DiagnosticSeverity::HINT));
    server
        .configure(serde_json::json!({ "no-config": true, "reference_diagnostics": "never" }))
        .unwrap();
    assert_eq!(summary(&server).len(), 1);
}

/// A workspace folder with notes on disk, plus one open document (`name` keeps tests that
/// run at once apart)
fn workspace(open: &str, name: &str) -> (Server, Uri, PathBuf) {
    let dir = std::env::temp_dir().join(format!("logoxide-lsp-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("My Notes")).unwrap();
    std::fs::write(
        dir.join("My Notes/a.md"),
        "Jn 3:16 again\nand John 3:14-18\n",
    )
    .unwrap();
    std::fs::write(dir.join("b.txt"), "John 3, John 2; 3:16, Rom 8:28").unwrap();
    std::fs::write(dir.join("image.png"), b"John 3:16\x00").unwrap();
    let uri = workspace::path_to_uri(&dir.join("open.md")).unwrap();
    let mut server = Server::new(BibleMatcher::default());
    server.set_roots(vec![dir.clone()]);
    server.did_open(DidOpenTextDocumentParams {
        text_document: TextDocumentItem::new(uri.clone(), "markdown".into(), 1, open.into()),
    });
    (server, uri, dir)
}

/// `file name: reference text` for each location
fn found(dir: &Path, locations: &[Location]) -> Vec<String> {
    locations
        .iter()
        .map(|location| {
            let path = workspace::uri_to_path(&location.uri).unwrap();
            let name = path.strip_prefix(dir).unwrap().display().to_string();
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|_| String::from("See John 3:16 here"));
            let index = LineIndex::new(&text);
            let range = location.range;
            let start = index
                .offset_of_utf16(range.start.line as usize, range.start.character as usize)
                .unwrap();
            let end = index
                .offset_of_utf16(range.end.line as usize, range.end.character as usize)
                .unwrap();
            format!("{name}: {}", &text[start..end])
        })
        .collect()
}

/// A one-chapter EPUB with these paragraphs
fn epub(paragraphs: &[&str]) -> Vec<u8> {
    use std::io::Write;
    let body: String = paragraphs.iter().map(|p| format!("<p>{p}</p>")).collect();
    let files = [
            ("mimetype", "application/epub+zip".to_string()),
            (
                "META-INF/container.xml",
                r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#.to_string(),
            ),
            (
                "content.opf",
                r#"<?xml version="1.0"?><package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>T</dc:title></metadata><manifest><item id="c" href="c.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="c"/></spine></package>"#.to_string(),
            ),
            (
                "c.xhtml",
                format!(r#"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>c</title></head><body>{body}</body></html>"#),
            ),
        ];
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, text) in files {
        zip.start_file(name, options).unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn books_are_searched_when_asked() {
    let (mut server, _, dir) = workspace("", "books");
    std::fs::write(
        dir.join("Gill.epub"),
        epub(&["Preface", "On grace, see John 3:16 and 📖 Rom 8:28."]),
    )
    .unwrap();
    let book = |locations: &[Location]| -> Vec<Location> {
        (locations.iter())
            .filter(|l| l.uri.as_str().starts_with("logoxide-file:"))
            .cloned()
            .collect()
    };
    // Off by default; `pdf` alone does nothing without the `pdf` feature
    assert!(workspace::BookSearch::new(false, false, "test").is_none());
    if !cfg!(feature = "pdf") {
        assert!(workspace::BookSearch::new(false, true, "test").is_none());
    }
    let search = |server: &Server| server.search(SearchMode::AnyOverlap, "Rom 8").unwrap();
    assert!(book(&search(&server)).is_empty());

    // On: in the book's text, at the reference
    let cache_dir = dir.with_extension("cache");
    let _ = std::fs::remove_dir_all(&cache_dir);
    server.books = Some(std::sync::Arc::new(workspace::BookSearch {
        disk: topos_bible_index::files::Cache::open_in(&cache_dir, "test"),
        epub: true,
        pdf: false,
    }));
    // As configuring does
    server.cache = Default::default();
    let found = book(&search(&server));
    assert_eq!(found.len(), 1, "{found:?}");
    let uri = found[0].uri.as_str();
    let text = server
        .file_text(BookParams { uri: uri.into() })
        .unwrap()
        .text;
    let range = found[0].range;
    let line: Vec<u16> = (text.lines().nth(range.start.line as usize).unwrap())
        .encode_utf16()
        .collect();
    let (start, end) = (range.start.character as usize, range.end.character as usize);
    assert_eq!(String::from_utf16(&line[start..end]).unwrap(), "Rom 8:28");

    // Kept in topos's cache, and found again from it
    server.cache = Default::default();
    let entries = std::fs::read_dir(&cache_dir).unwrap().flatten();
    let options = entries.filter(|e| e.path().is_dir()).count();
    assert!(options > 0);
    assert_eq!(book(&search(&server)).len(), 1);
    let _ = std::fs::remove_dir_all(&cache_dir);
}

#[test]
fn searches_the_workspace() {
    let (server, uri, dir) = workspace("See John 3:16 here", "search");
    // Go to references: exactly John 3:16, however it is written
    let references = server
        .references(ReferenceParams {
            text_document_position: at(&uri, 0, 6),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: lsp_types::ReferenceContext {
                include_declaration: true,
            },
        })
        .unwrap();
    assert_eq!(
        found(&dir, &references),
        ["My Notes/a.md: Jn 3:16", "open.md: John 3:16"]
    );

    // One code action per search, for the reference under the cursor
    let actions = server
        .code_actions(CodeActionParams {
            text_document: TextDocumentIdentifier::new(uri.clone()),
            range: Range::new(Position::new(0, 8), Position::new(0, 8)),
            context: lsp_types::CodeActionContext::default(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        })
        .unwrap();
    let titles: Vec<_> = actions
        .iter()
        .map(|a| match a {
            CodeActionOrCommand::CodeAction(action) => action.title.clone(),
            CodeActionOrCommand::Command(command) => command.title.clone(),
        })
        .collect();
    assert_eq!(
        titles,
        [
            "Search \"John 3:16\" for explicit overlap",
            "Search \"John 3:16\" for any overlap",
            "Search \"John 3:16\" for exact overlap",
            "Search inside \"John 3:16\"",
        ]
    );

    // Running an action's command finds that search's references
    let run = |index: usize| {
        let CodeActionOrCommand::CodeAction(action) = &actions[index] else {
            panic!("not an action")
        };
        let command = action.command.clone().unwrap();
        let locations = server
            .execute_command(ExecuteCommandParams {
                command: command.command,
                arguments: command.arguments.unwrap(),
                work_done_progress_params: WorkDoneProgressParams::default(),
            })
            .unwrap();
        found(&dir, &locations)
    };
    assert_eq!(
        run(0),
        [
            "My Notes/a.md: Jn 3:16",
            "My Notes/a.md: John 3:14-18",
            "b.txt: John 2; 3:16",
            "open.md: John 3:16"
        ]
    );
    let any = run(1);
    assert!(any.contains(&"b.txt: John 3".to_string()), "{any:?}");
    assert_eq!(any.len(), 5);

    // The files' references are cached (the binary one too, as having none)
    assert_eq!(server.cache.len(), 3);
    let exact = |server: &Server| {
        let locations = server
            .search(SearchMode::ExactOverlap, "Romans 8:28")
            .unwrap();
        locations.len()
    };
    assert_eq!(exact(&server), 1);
    // Same size and time: the cache is used, so the edit isn't seen yet
    let b = dir.join("b.txt");
    let modified = std::fs::metadata(&b).unwrap().modified().unwrap();
    std::fs::write(&b, "John 3, John 2; 3:16, Rom 8:29").unwrap();
    let file = std::fs::File::options().write(true).open(&b).unwrap();
    file.set_modified(modified).unwrap();
    assert_eq!(exact(&server), 1);
    // A real change is read again
    std::fs::write(&b, "Nothing here now").unwrap();
    assert_eq!(exact(&server), 0);
    // Deleted files are forgotten
    std::fs::remove_file(&b).unwrap();
    exact(&server);
    assert_eq!(server.cache.len(), 2);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn settings_format_completions_hints_and_symbols() {
    let (mut server, uri) = server("See jn 3:16, 17 and Jude 1:5");
    server
        .configure(serde_json::json!({
            "no-config": true,
            "logoxide": {
                "no_config": true,
                "psg-fmt": { "join_adjacent": true, "chapter_in_single_chapter_books": false },
                "format": "abbreviation"
            }
        }))
        .unwrap();
    let hints = server
        .inlay_hints(InlayHintParams {
            work_done_progress_params: WorkDoneProgressParams::default(),
            text_document: TextDocumentIdentifier::new(uri.clone()),
            range: Range::new(Position::new(0, 0), Position::new(1, 0)),
        })
        .unwrap();
    let labels: Vec<_> = hints
        .iter()
        .map(|hint| match &hint.label {
            InlayHintLabel::String(label) => (hint.position, label.clone()),
            InlayHintLabel::LabelParts(_) => panic!("parts"),
        })
        .collect();
    assert_eq!(
        labels,
        [
            (Position::new(0, 15), String::from("Jn 3:16-17")),
            (Position::new(0, 28), String::from("Jude 5"))
        ]
    );
    // Unchanged references get no hint by default; "always" and "osis" show one
    server
        .configure(serde_json::json!({ "no-config": true, "inlay-hints": "osis" }))
        .unwrap();
    let hints = server
        .inlay_hints(InlayHintParams {
            work_done_progress_params: WorkDoneProgressParams::default(),
            text_document: TextDocumentIdentifier::new(uri.clone()),
            range: Range::new(Position::new(0, 0), Position::new(1, 0)),
        })
        .unwrap();
    assert_eq!(hints.len(), 2);

    // Bad settings are reported, and the previous settings stay
    let err = server
        .configure(serde_json::json!({ "no-config": true, "psg-fmt": { "joins": true } }))
        .unwrap_err();
    assert!(err.contains("joins"), "{err}");
}

#[test]
fn reformats_and_explains_missing_verses() {
    let (server, uri) = server("See jn 3:16 and rom 8:28, John 1:1\nBad: John 3:99");
    let actions = |line: u32, character: u32| -> Vec<(String, Vec<TextEdit>)> {
        server
            .code_actions(CodeActionParams {
                text_document: TextDocumentIdentifier::new(uri.clone()),
                range: Range::new(
                    Position::new(line, character),
                    Position::new(line, character),
                ),
                context: lsp_types::CodeActionContext::default(),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            })
            .unwrap_or_default()
            .into_iter()
            .filter_map(|a| match a {
                CodeActionOrCommand::CodeAction(action) => {
                    let edits = action
                        .edit
                        .and_then(|e| e.changes)
                        .map(|changes| changes.into_values().flatten().collect())?;
                    Some((action.title, edits))
                }
                CodeActionOrCommand::Command(_) => None,
            })
            .collect()
    };
    let on_jn = actions(0, 5);
    assert_eq!(on_jn.len(), 2);
    assert_eq!(on_jn[0].0, "Reformat as \"John 3:16\"");
    assert_eq!(
        on_jn[0].1,
        [TextEdit::new(
            Range::new(Position::new(0, 4), Position::new(0, 11)),
            "John 3:16".into()
        )]
    );
    // Every reference written differently (John 1:1 already is)
    assert_eq!(on_jn[1].0, "Reformat all 2 references in this file");
    assert_eq!(on_jn[1].1.len(), 2);
    // A reference already in the format gets no reformat for itself
    let on_john = actions(0, 28);
    assert_eq!(on_john.len(), 1);
    assert_eq!(on_john[0].0, "Reformat all 2 references in this file");

    // Hovering a verse that doesn't exist says what does
    let hover = server
        .hover(HoverParams {
            text_document_position_params: at(&uri, 1, 11),
            work_done_progress_params: WorkDoneProgressParams::default(),
        })
        .unwrap();
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("no markup");
    };
    assert_eq!(
        markup.value,
        "**John 3:99 does not exist**\n\nJohn 3 has 36 verses"
    );
}

#[test]
fn hovers_and_lists_symbols() {
    let (server, uri) = server("See Rom 8:28 and\nJude 5.");
    let hover = server
        .hover(HoverParams {
            text_document_position_params: at(&uri, 0, 6),
            work_done_progress_params: WorkDoneProgressParams::default(),
        })
        .unwrap();
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("no markup");
    };
    assert_eq!(
        markup.value,
        "**Romans 8:28**\n\n\
             - **Abbreviation**: Rom 8:28\n\
             - **OSIS**: `Rom.8.28`\n\
             - **Book**: Romans, book 45 of 66, 16 chapters\n\
             - **Testament**: New Testament\n\
             - **Genres**: Pauline Epistles, Epistles\n\
             - **Verses**: 1 (8:28)\n\
             - **Location**: line 1, columns 5-12"
    );
    assert_eq!(
        hover.range,
        Some(Range::new(Position::new(0, 4), Position::new(0, 12)))
    );

    let Some(DocumentSymbolResponse::Nested(symbols)) =
        server.document_symbols(DocumentSymbolParams {
            text_document: TextDocumentIdentifier::new(uri),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        })
    else {
        panic!("no symbols");
    };
    let names: Vec<_> = symbols.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Romans 8:28", "Jude 1:5"]);
}

/// The `hover` setting picks the lines and their order; a full name that is the title already
/// is left out
#[test]
fn hover_lines_follow_the_setting() {
    let hover = |text: &str, settings: Value| {
        let (mut server, uri) = server(text);
        server.configure(settings).unwrap();
        let hover = server
            .hover(HoverParams {
                text_document_position_params: at(&uri, 0, 1),
                work_done_progress_params: WorkDoneProgressParams::default(),
            })
            .unwrap();
        let HoverContents::Markup(markup) = hover.contents else {
            panic!("no markup");
        };
        markup.value
    };
    let settings =
        json!({ "no-config": true, "hover": "written,name,bcv,verses", "format": "abbreviation" });
    assert_eq!(
        hover("jn 3; 4:1-2", settings),
        "**Jn 3; 4:1-2**\n\n\
             - **Written**: `jn 3; 4:1-2`\n\
             - **Name**: John 3; 4:1-2\n\
             - **BCV**: `43003001-43003999`, `43004001-43004002`\n\
             - **Verses**: 38 (3:1-36; 4:1-2)"
    );
    let settings = json!({ "no-config": true, "hover": ["name", "testament"] });
    assert_eq!(
        hover("Gen 1:1", settings),
        "**Genesis 1:1**\n\n- **Testament**: Old Testament"
    );
    let settings = json!({ "no-config": true, "hover": [] });
    assert_eq!(hover("Gen 1:1", settings), "**Genesis 1:1**");
}

const JHN: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<usx version="3.0"><book code="JHN" style="id">x</book><para style="usfm">3.1</para>
<para style="mt1">John</para>
<chapter number="3" style="c" sid="JHN 3"/>
<para style="s1">Jesus and Nicodemus</para>
<para style="p"><verse number="16" style="v" sid="JHN 3:16"/>For God so loved the world. <verse eid="JHN 3:16"/><verse number="17" style="v" sid="JHN 3:17"/>Jesus said, <char style="wj">I did not come
to condemn.</char> <verse eid="JHN 3:17"/></para>
<chapter eid="JHN 3"/>
<chapter number="4" style="c" sid="JHN 4"/>
<para style="p"><verse number="1" style="v" sid="JHN 4:1"/>Now Jesus learned<verse eid="JHN 4:1"/></para>
<chapter eid="JHN 4"/></usx>"#;

/// A workspace with notes and a Bible (a folder of USX files) set
fn with_bible(open: &str, markup: Value, name: &str) -> (Server, Uri, PathBuf) {
    let (mut server, uri, dir) = workspace(open, name);
    let bible = dir.join("Bibles/BSB");
    std::fs::create_dir_all(&bible).unwrap();
    std::fs::write(bible.join("JHN.usx"), JHN).unwrap();
    let settings = json!({
        "no-config": true,
        "bible": bible.to_str().unwrap(),
        "markup": markup,
    });
    server.configure(settings).unwrap();
    (server, uri, dir)
}

fn book_uri() -> Uri {
    Uri::from_str("logoxide://BSB/JHN").unwrap()
}

#[test]
fn bible_text_in_hover_and_whole_books() {
    let (server, uri, dir) = with_bible("See John 3:16-17 and John 4", json!({}), "bible");
    let hover = server
        .hover(HoverParams {
            text_document_position_params: at(&uri, 0, 6),
            work_done_progress_params: WorkDoneProgressParams::default(),
        })
        .unwrap();
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("no markup");
    };
    // The heading before 3:16 belongs to it; the info string says what to color: the heading,
    // verse numbers, and words of Jesus
    assert!(
            markup.value.starts_with(
                "**John 3:16-17**\n\n```logoxide 0:0:19:3 1:0:4:0 1:33:4:0 1:50:26:2\nJesus and Nicodemus\n[16] For God so loved the world. [17] Jesus said, I did not come to condemn.\n```\n\n- **Abbreviation**"
            ),
            "{}",
            markup.value
        );

    // The whole book, with chapter labels; its title is a heading
    let book = server
        .book(BookParams {
            uri: book_uri().to_string(),
        })
        .unwrap();
    assert_eq!(book.title, "John");
    assert_eq!(
        book.text,
        "John\nJohn 3\nJesus and Nicodemus\n[16] For God so loved the world. [17] Jesus said, I did not come to condemn.\nJohn 4\n[1] Now Jesus learned"
    );
    assert!(
        server
            .book(BookParams {
                uri: "logoxide://KJV/JHN".into()
            })
            .is_err()
    );

    // Go to definition: from the first verse's number to the last one's end
    let definition = |character| {
        server.definition(GotoDefinitionParams {
            text_document_position_params: at(&uri, 0, character),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        })
    };
    let Some(GotoDefinitionResponse::Scalar(location)) = definition(6) else {
        panic!("no definition");
    };
    assert_eq!(location.uri, book_uri());
    assert_eq!(
        location.range,
        Range::new(Position::new(3, 0), Position::new(3, 76))
    );
    let Some(GotoDefinitionResponse::Scalar(chapter)) = definition(23) else {
        panic!("no definition");
    };
    assert_eq!(chapter.range.start, Position::new(5, 0));

    // Tokens: chapter labels, headings, verse numbers, and words of Jesus (each on its line)
    let tokens = server
        .semantic_tokens(SemanticTokensParams {
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            text_document: TextDocumentIdentifier::new(book_uri()),
        })
        .data;
    let mut absolute = vec![];
    let (mut line, mut character) = (0, 0);
    for t in &tokens {
        line += t.delta_line;
        character = if t.delta_line == 0 {
            character + t.delta_start
        } else {
            t.delta_start
        };
        absolute.push((
            line,
            character,
            t.length,
            Token::LEGEND[t.token_type as usize],
        ));
    }
    assert_eq!(
        absolute,
        [
            (0, 0, 4, "heading"),
            (1, 0, 6, "chapterNumber"),
            (2, 0, 19, "heading"),
            (3, 0, 4, "verseNumber"),
            (3, 33, 4, "verseNumber"),
            (3, 50, 26, "wordsOfJesus"),
            (4, 0, 6, "chapterNumber"),
            (5, 0, 3, "verseNumber"),
        ]
    );
    // Notes have none
    let notes = server.semantic_tokens(SemanticTokensParams {
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        text_document: TextDocumentIdentifier::new(uri.clone()),
    });
    assert!(notes.data.is_empty());

    // Symbols: each chapter from its label, with its verses
    let Some(DocumentSymbolResponse::Nested(chapters)) =
        server.document_symbols(DocumentSymbolParams {
            text_document: TextDocumentIdentifier::new(book_uri()),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        })
    else {
        panic!("no symbols");
    };
    let outline: Vec<_> = (chapters.iter())
        .map(|c| {
            let verses = c.children.iter().flatten();
            let verses: Vec<_> = verses.map(|v| (v.name.as_str(), v.range.start)).collect();
            (
                c.name.as_str(),
                c.selection_range.start,
                c.range.end,
                verses,
            )
        })
        .collect();
    let pos = Position::new;
    assert_eq!(
        outline,
        [
            (
                "John 3",
                pos(1, 0),
                pos(3, 76),
                vec![("John 3:16", pos(3, 0)), ("John 3:17", pos(3, 33))]
            ),
            (
                "John 4",
                pos(4, 0),
                pos(5, 21),
                vec![("John 4:1", pos(5, 0))]
            ),
        ]
    );

    // References from a verse in the book: every reference that names it
    let references = |line, character| {
        server.references(ReferenceParams {
            text_document_position: at(&book_uri(), line, character),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: lsp_types::ReferenceContext {
                include_declaration: true,
            },
        })
    };
    let verse = references(3, 10).unwrap();
    assert_eq!(
        found(&dir, &verse[..3]),
        [
            "My Notes/a.md: Jn 3:16",
            "My Notes/a.md: John 3:14-18",
            "b.txt: John 2; 3:16",
        ]
    );
    // The open note's `John 3:16-17` (the book itself isn't searched)
    assert_eq!((verse.len(), verse[3].uri.clone()), (4, uri.clone()));
    assert_eq!(verse[3].range.end, Position::new(0, 16));
    // On the chapter's label, anything in the chapter
    assert_eq!(references(1, 2).unwrap().len(), 5);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The markup settings: labels, what is shown, and words of Jesus left uncolored
#[test]
fn markup_settings() {
    let markup = json!({
        "verse_numbers": { "display": "{chapter}:{verse}", "color": "blue" },
        "chapter-numbers": { "show": false },
        "words_of_jesus": { "show": false },
        "headings": { "show": false },
    });
    let (server, _, dir) = with_bible("John 3:16", markup, "markup");
    let book = server
        .book(BookParams {
            uri: book_uri().to_string(),
        })
        .unwrap();
    assert_eq!(
        book.text,
        "3:16 For God so loved the world. 3:17 Jesus said, I did not come to condemn.\n4:1 Now Jesus learned"
    );
    let tokens = server.semantic_tokens(SemanticTokensParams {
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        text_document: TextDocumentIdentifier::new(book_uri()),
    });
    assert!(
        tokens
            .data
            .iter()
            .all(|t| t.token_type == Token::VerseNumber as u32)
    );
    assert_eq!(tokens.data.len(), 3);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Templates for headings and words of Jesus, and elements that aren't shown
#[test]
fn markup_templates() {
    let markup = json!({
        "headings": { "display": "## {text}" },
        "words_of_jesus": { "display": "“{text}”" },
        "verse_numbers": { "show": false },
        "chapter_numbers": { "display": "Chapter {chapter}" },
    });
    let (server, _, dir) = with_bible("John 3:16", markup, "templates");
    let book = server
        .book(BookParams {
            uri: book_uri().to_string(),
        })
        .unwrap();
    assert_eq!(
        book.text,
        "## John\nChapter 3\n## Jesus and Nicodemus\nFor God so loved the world. Jesus said, “I did not come to condemn.”\nChapter 4\nNow Jesus learned"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Hovering a verse in a whole book counts its references; go to declaration opens the book
/// like definition; completions resolve to the passage's text
#[test]
fn verse_hover_declaration_and_completion_text() {
    let (server, uri, dir) = with_bible("See John 3:16", json!({}), "verse-hover");
    let hover = |uri: &Uri, line, character| {
        let hover = server.hover(HoverParams {
            text_document_position_params: at(uri, line, character),
            work_done_progress_params: WorkDoneProgressParams::default(),
        })?;
        match hover.contents {
            HoverContents::Markup(markup) => Some(markup.value),
            _ => None,
        }
    };
    assert_eq!(
        hover(&book_uri(), 3, 10).unwrap(),
        "**John 3:16**\n\n4 references in 3 files"
    );
    assert_eq!(
        hover(&book_uri(), 1, 1).unwrap(),
        "**John 3**\n\n5 references in 3 files"
    );
    // Out of time: the hover says `???`
    let (_, complete) = server
        .search_until(
            SearchMode::ExplicitOverlap,
            "John 3:16",
            &workspace::Stop::at(Instant::now()),
        )
        .unwrap();
    assert!(!complete);
    // Cancelled: no files are searched, only the open documents
    let cancelled = workspace::Stop::when(std::sync::Arc::new(true.into()));
    let (found, complete) = server
        .search_until(SearchMode::ExplicitOverlap, "John 3:16", &cancelled)
        .unwrap();
    assert!(!complete);
    assert!(found.iter().all(|l| l.uri == uri), "{found:?}");

    let declaration = server.definition(GotoDefinitionParams {
        text_document_position_params: at(&uri, 0, 6),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    });
    assert!(matches!(declaration, Some(GotoDefinitionResponse::Scalar(l)) if l.uri == book_uri()));

    let (mut server, uri) = server_with("John 3:1", &server);
    server.did_open(DidOpenTextDocumentParams {
        text_document: TextDocumentItem::new(uri.clone(), "markdown".into(), 1, "John 3:1".into()),
    });
    let Some(CompletionResponse::List(list)) = server.completion(CompletionParams {
        text_document_position: at(&uri, 0, 8),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        context: None,
    }) else {
        panic!("no completions");
    };
    let item = list
        .items
        .into_iter()
        .find(|i| i.label == "John 3:16")
        .unwrap();
    let resolved = server.resolve_completion(item);
    let Some(lsp_types::Documentation::MarkupContent(doc)) = resolved.documentation else {
        panic!("no documentation");
    };
    // No token list for completion plugins to show; it is in the data instead
    assert_eq!(
        doc.value,
        "```logoxide\nJesus and Nicodemus\n[16] For God so loved the world.\n```"
    );
    assert_eq!(resolved.data.unwrap()["tokens"], "0:0:19:3 1:0:4:0");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The same settings, for another document
fn server_with(_text: &str, other: &Server) -> (Server, Uri) {
    let mut server = Server::new(BibleMatcher::default());
    server
        .configure(Value::Object(other.editor.clone()))
        .unwrap();
    (server, Uri::from_str("file:///typing.md").unwrap())
}

/// Insertions: a code action per insertion, with its own markup over the others
#[test]
fn insertions() {
    let (mut server, uri, dir) = with_bible("Read John 3:16-17 today\nnext", json!({}), "insert");
    let mut settings = server.editor.clone();
    settings.insert(
            "insertions".into(),
            json!([
                {
                    "name": "Callout",
                    "display": "> [!bible] {reference}\n{text}",
                    "line_prefix": "> ",
                    "markup": { "headings": { "show": false }, "verse_numbers": { "display": "{verse}" } }
                },
                { "name": "Plain", "markup": { "verse_numbers": { "show": false }, "headings": { "show": false } } },
                { "name": "Line", "replace": "line", "display": "> {reference}: {text}", "markup": { "verse_numbers": { "show": false }, "headings": { "show": false } } },
                { "name": "Inline", "replace": "inline", "display": "“{text}” ({reference})", "markup": { "verse_numbers": { "show": false }, "headings": { "show": false } } }
            ]),
        );
    server.configure(Value::Object(settings)).unwrap();
    let actions = server
        .code_actions(CodeActionParams {
            text_document: TextDocumentIdentifier::new(uri.clone()),
            range: Range::new(Position::new(0, 6), Position::new(0, 6)),
            context: lsp_types::CodeActionContext::default(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        })
        .unwrap();
    let inserts: Vec<(String, TextEdit)> = actions
        .into_iter()
        .filter_map(|a| match a {
            CodeActionOrCommand::CodeAction(a) if a.title.starts_with("Insert") => {
                let edit = a.edit?.changes?.into_values().next()?.pop()?;
                Some((a.title, edit))
            }
            _ => None,
        })
        .collect();
    let at = Range::new(Position::new(0, 23), Position::new(0, 23));
    assert_eq!(
            inserts,
            [
                (
                    String::from("Insert Callout"),
                    TextEdit::new(at, "\n> [!bible] John 3:16-17\n> 16 For God so loved the world. 17 Jesus said, I did not come to condemn.".into())
                ),
                (
                    String::from("Insert Plain"),
                    TextEdit::new(at, "\nFor God so loved the world. Jesus said, I did not come to condemn.".into())
                ),
                // The whole line
                (
                    String::from("Insert Line"),
                    TextEdit::new(
                        Range::new(Position::new(0, 0), Position::new(0, 23)),
                        "> John 3:16-17: For God so loved the world. Jesus said, I did not come to condemn.".into()
                    )
                ),
                // Just the reference: `Read ` and ` today` stay
                (
                    String::from("Insert Inline"),
                    TextEdit::new(
                        Range::new(Position::new(0, 5), Position::new(0, 17)),
                        "“For God so loved the world. Jesus said, I did not come to condemn.” (John 3:16-17)".into()
                    )
                ),
            ]
        );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `markup.width` breaks long lines at spaces, keeping every byte offset
#[test]
fn width_wraps_at_spaces() {
    // The wrapping itself is logoxide-text's (`passage::wrap`, tested there)
    let (server, uri, dir) = with_bible("See John 3:16", json!({ "width": 30 }), "width");
    let book = server
        .book(BookParams {
            uri: book_uri().to_string(),
        })
        .unwrap();
    assert!(
        book.text.lines().all(|l| l.chars().count() <= 30),
        "{}",
        book.text
    );
    assert!(
        book.text.contains("\n[16] For God so loved the\nworld."),
        "{}",
        book.text
    );
    // Go to definition still finds the verse
    let Some(GotoDefinitionResponse::Scalar(location)) = server.definition(GotoDefinitionParams {
        text_document_position_params: at(&uri, 0, 6),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    }) else {
        panic!("no definition");
    };
    assert_eq!(location.range.start, Position::new(3, 0));
    let _ = std::fs::remove_dir_all(&dir);
}

/// A Bible's `JHN.usx` with only John 3:16, worded like the KJV
const KJV_JHN: &str = r#"<usx version="3.0"><book code="JHN" style="id">John</book>
<chapter number="3" style="c" sid="JHN 3"/>
<para style="p"><verse number="16" style="v" sid="JHN 3:16"/>For God so loved the world, that he gave his only begotten Son.<verse eid="JHN 3:16"/></para>
<chapter eid="JHN 3"/></usx>"#;

/// Several Bibles: a reference followed by one's name or alias uses it, others the default
#[test]
fn several_bibles() {
    let open = "See John 3:16 KJV and John 3:16.\nJohn 3:17 AV\nJohn 3:16 K";
    let (mut server, uri, dir) = with_bible(open, json!({}), "several");
    let kjv = dir.join("Bibles/KJV");
    std::fs::create_dir_all(&kjv).unwrap();
    std::fs::write(kjv.join("JHN.usx"), KJV_JHN).unwrap();
    let mut settings = server.editor.clone();
    settings.insert(
        "bibles".into(),
        json!({ "KJV": { "path": kjv.to_str().unwrap(), "aliases": ["AV"] } }),
    );
    let plain = json!({ "verse_numbers": { "show": false }, "headings": { "show": false } });
    settings.insert(
        "insertions".into(),
        json!([{ "name": "Plain", "markup": plain }]),
    );
    server.configure(Value::Object(settings.clone())).unwrap();

    let hover = |server: &Server, line, character| {
        let hover = server.hover(HoverParams {
            text_document_position_params: at(&uri, line, character),
            work_done_progress_params: WorkDoneProgressParams::default(),
        });
        match hover.map(|h| h.contents) {
            Some(HoverContents::Markup(markup)) => markup.value,
            _ => panic!("no hover"),
        }
    };
    // The Bible a reference names, labeled, else the default (the `bible` setting's)
    let named = hover(&server, 0, 6);
    assert!(
        named.starts_with("**John 3:16** (KJV)\n\n*KJV*\n```logoxide"),
        "{named}"
    );
    assert!(
        named.contains("that he gave") && !named.contains("*BSB*"),
        "{named}"
    );
    // On the translation's name too
    assert_eq!(hover(&server, 0, 15), named);
    let default = hover(&server, 0, 24);
    assert!(default.starts_with("**John 3:16**\n\n*BSB*\n"), "{default}");
    assert!(!default.contains("that he gave"), "{default}");

    // The KJV's own verse counts (only 3:16 here), with its name in the message
    let messages: Vec<String> = server
        .diagnostics(&uri)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::WARNING))
        .map(|d| d.message)
        .collect();
    assert_eq!(messages, ["John 3:17 does not exist in the KJV"]);

    // Go to definition opens the named Bible's book
    let Some(GotoDefinitionResponse::Scalar(location)) = server.definition(GotoDefinitionParams {
        text_document_position_params: at(&uri, 0, 6),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    }) else {
        panic!("no definition");
    };
    assert_eq!(location.uri.as_str(), "logoxide://KJV/JHN");
    let book = server
        .book(BookParams {
            uri: "logoxide://KJV/JHN".into(),
        })
        .unwrap();
    assert!(book.text.contains("that he gave"), "{}", book.text);

    // Completing a translation, whose documentation is that Bible's text
    let Some(CompletionResponse::List(list)) = server.completion(CompletionParams {
        text_document_position: at(&uri, 2, 11),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        context: None,
    }) else {
        panic!("no completions");
    };
    let item = list.items.into_iter().next().unwrap();
    assert_eq!(item.label, "KJV");
    let resolved = server.resolve_completion(item);
    let Some(lsp_types::Documentation::MarkupContent(doc)) = resolved.documentation else {
        panic!("no documentation");
    };
    assert!(doc.value.contains("that he gave"), "{}", doc.value);

    // The Bibles, the default first
    let bibles = server.bibles().bibles;
    let names: Vec<(&str, &[String])> = bibles
        .iter()
        .map(|b| (b.name.as_str(), b.aliases.as_slice()))
        .collect();
    assert_eq!(names, [("BSB", &[][..]), ("KJV", &["AV".to_string()][..])]);
    assert_eq!(bibles[1].books, ["JHN"]);

    // The same verse in the other Bible's book
    let same = |line, bible: &str| {
        server
            .same_verse(SameVerseParams {
                uri: "logoxide://BSB/JHN".into(),
                position: Position::new(line, 1),
                bible: bible.into(),
            })
            .unwrap()
            .map(|l| (l.uri.to_string(), l.range.start))
    };
    let bsb = server
        .book(BookParams {
            uri: "logoxide://BSB/JHN".into(),
        })
        .unwrap();
    let line_of =
        |text: &str, find: &str| text.lines().position(|l| l.contains(find)).unwrap() as u32;
    let kjv_book = server
        .book(BookParams {
            uri: "logoxide://KJV/JHN".into(),
        })
        .unwrap();
    let kjv_316 = line_of(&kjv_book.text, "[16]");
    assert_eq!(
        same(line_of(&bsb.text, "[16]"), "av"),
        Some(("logoxide://KJV/JHN".into(), Position::new(kjv_316, 0)))
    );
    // A chapter it lacks (and nothing after it) goes to the book's start
    assert_eq!(
        same(line_of(&bsb.text, "John 4"), "KJV").map(|(_, p)| p),
        Some(Position::new(0, 0))
    );
    assert!(
        server
            .same_verse(SameVerseParams {
                uri: "logoxide://BSB/JHN".into(),
                position: Position::new(0, 0),
                bible: "NIV".into(),
            })
            .is_err()
    );

    // Insertions: from the named Bible, else one per Bible
    let inserts = |server: &Server, character| -> Vec<String> {
        server
            .code_actions(CodeActionParams {
                text_document: TextDocumentIdentifier::new(uri.clone()),
                range: Range::new(Position::new(0, character), Position::new(0, character)),
                context: lsp_types::CodeActionContext::default(),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            })
            .unwrap()
            .into_iter()
            .filter_map(|a| match a {
                CodeActionOrCommand::CodeAction(a) if a.title.starts_with("Insert") => {
                    Some(a.title)
                }
                _ => None,
            })
            .collect()
    };
    assert_eq!(inserts(&server, 6), ["Insert Plain (KJV)"]);
    assert_eq!(
        inserts(&server, 24),
        ["Insert Plain (BSB)", "Insert Plain (KJV)"]
    );

    // `hover-bibles` adds Bibles after the named one; `default-bible` changes the default
    settings.insert("hover-bibles".into(), json!(["kjv"]));
    settings.insert("default-bible".into(), json!("AV"));
    server.configure(Value::Object(settings.clone())).unwrap();
    let both = hover(&server, 0, 24);
    assert!(both.starts_with("**John 3:16**\n\n*KJV*\n"), "{both}");
    settings.insert("hover-bibles".into(), json!(["BSB"]));
    server.configure(Value::Object(settings.clone())).unwrap();
    let both = hover(&server, 0, 24);
    let (kjv, bsb) = (both.find("*KJV*").unwrap(), both.find("*BSB*").unwrap());
    assert!(kjv < bsb, "{both}");
    settings.insert("hover-bibles".into(), json!(["NIV"]));
    assert!(server.configure(Value::Object(settings)).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Bibles from a logoxide project in the workspace: without `bible` or `bibles`, its packages;
/// with `bibles = ["id"]`, those
#[test]
fn bibles_from_a_project() {
    let (mut server, uri, dir) = workspace("See John 3:16 TB here", "project");
    // A USFM package, used in place (a path dependency)
    let package = dir.join("Bibles/TB");
    std::fs::create_dir_all(package.join("usfm")).unwrap();
    std::fs::write(
            package.join("logoxide.toml"),
            "[package]\nid = \"tb\"\nname = \"Test\"\nabbreviation = \"TB\"\naliases = [\"Testing\"]\nversion = \"1.0.0\"\nlanguage = \"en\"\n\n[formats]\nusfm = { path = \"usfm/\" }\n",
        )
        .unwrap();
    std::fs::write(
            package.join("usfm/43JHNTB.usfm"),
            "\\id JHN\n\\c 3\n\\p\n\\v 16 \\w For|strong=\"G1063\"\\w* \\w God|strong=\"G2316\"\\w* so loved the world.\n\\v 17 For God sent not.\n",
        )
        .unwrap();
    let mut project = logoxide::Project::init(&dir).unwrap();
    let dependency = logoxide::manifest::DetailedDependency {
        path: Some("Bibles/TB".into()),
        default_features: true,
        ..Default::default()
    };
    project.add("tb", &dependency).unwrap();
    project.save_manifest().unwrap();
    let library = logoxide::Library::new(dir.join("library"), dir.join("cache"));
    project
        .install(
            &library,
            &logoxide::git::GitCli::default(),
            logoxide::resolve::Update::Nothing,
        )
        .unwrap();

    // No Bible settings: the project's
    server.configure(json!({ "no-config": true })).unwrap();
    let names: Vec<String> = server.bibles().bibles.into_iter().map(|b| b.name).collect();
    assert_eq!(names, ["TB"]);
    let hover = server
        .hover(HoverParams {
            text_document_position_params: at(&uri, 0, 6),
            work_done_progress_params: WorkDoneProgressParams::default(),
        })
        .unwrap();
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("no markup");
    };
    // The package's own name, and its USFM text (with the spaces between Strong's-tagged words)
    assert!(
        markup.value.starts_with("**John 3:16** (TB)"),
        "{}",
        markup.value
    );
    assert!(
        markup.value.contains("For God so loved the world."),
        "{}",
        markup.value
    );

    // By id, and an id the project doesn't have
    server
        .configure(json!({ "no-config": true, "bibles": ["tb"] }))
        .unwrap();
    assert_eq!(server.bibles().bibles[0].aliases, ["Testing"]);
    let error = server
        .configure(json!({ "no-config": true, "bibles": ["nope"] }))
        .unwrap_err();
    assert!(error.contains("`nope` isn't in"), "{error}");
    // The package's folder as a `bible` folder works too
    server
        .configure(json!({ "no-config": true, "bible": package.to_str().unwrap() }))
        .unwrap();
    assert_eq!(server.bibles().bibles[0].name, "TB");
    let _ = std::fs::remove_dir_all(&dir);
}
