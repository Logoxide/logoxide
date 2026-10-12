//! `logoxide-lsp` speaks the Language Server Protocol over stdin and stdout.

use std::{
    collections::HashMap,
    error::Error,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use lsp_server::{Connection, Message, Notification, Request, RequestId, Response};
use lsp_types::{
    CancelParams, DidChangeConfigurationParams, DidChangeTextDocumentParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, InitializeParams, MessageType,
    NumberOrString, ShowMessageParams, Uri,
    notification::{
        Cancel, DidChangeConfiguration, DidChangeTextDocument, DidCloseTextDocument,
        DidOpenTextDocument, Notification as LspNotification, PublishDiagnostics, ShowMessage,
    },
    request::{
        CodeActionRequest, Completion, DocumentDiagnosticRequest, DocumentSymbolRequest,
        ExecuteCommand, GotoDeclaration, GotoDefinition, HoverRequest, InlayHintRequest,
        References, Request as LspRequest, ResolveCompletionItem, SemanticTokensFullRequest,
        WorkspaceDiagnosticRefresh,
    },
};
use topos_bible::matcher::BibleMatcher;

use crate::{
    server::{Search, Server},
    workspace::Stop,
};

mod bible;
mod dictionary;
mod server;
mod settings;
mod workspace;

type AnyError = Box<dyn Error + Send + Sync>;

fn main() -> Result<(), AnyError> {
    let (connection, io_threads) = Connection::stdio();
    run(&connection)?;
    io_threads.join()?;
    Ok(())
}

/// Tells the editor something went wrong (like bad settings), without stopping the server
fn warn(connection: &Connection, message: String) -> Result<(), AnyError> {
    let params = ShowMessageParams {
        typ: MessageType::WARNING,
        message,
    };
    let notification = Notification::new(ShowMessage::METHOD.into(), params);
    connection
        .sender
        .send(Message::Notification(notification))?;
    Ok(())
}

fn publish_diagnostics(
    connection: &Connection,
    server: &Server,
    uri: &Uri,
) -> Result<(), AnyError> {
    let notification =
        Notification::new(PublishDiagnostics::METHOD.into(), server.diagnostics(uri));
    connection
        .sender
        .send(Message::Notification(notification))?;
    Ok(())
}

fn run(connection: &Connection) -> Result<(), AnyError> {
    let (id, init) = connection.initialize_start()?;
    let init: InitializeParams = serde_json::from_value(init).unwrap_or_default();
    let text_document = init.capabilities.text_document.as_ref();
    // Editors that ask for diagnostics (pull) get them for any buffer, even an unnamed one whose
    // `file://` URI a pushed notification couldn't be matched back to
    let pull = text_document.and_then(|t| t.diagnostic.as_ref()).is_some();
    let refresh = init
        .capabilities
        .workspace
        .as_ref()
        .and_then(|w| w.diagnostic.as_ref())
        .and_then(|d| d.refresh_support)
        .unwrap_or(false);
    let mut capabilities = Server::capabilities();
    if pull {
        capabilities.diagnostic_provider = Some(lsp_types::DiagnosticServerCapabilities::Options(
            lsp_types::DiagnosticOptions {
                identifier: Some(String::from("logoxide")),
                inter_file_dependencies: false,
                workspace_diagnostics: false,
                ..lsp_types::DiagnosticOptions::default()
            },
        ));
    }
    connection.initialize_finish(
        id,
        serde_json::json!({
            "capabilities": capabilities,
            "serverInfo": { "name": "logoxide-lsp", "version": env!("CARGO_PKG_VERSION") },
        }),
    )?;
    let mut server = Server::new(BibleMatcher::default());
    let mut refreshes = 0;
    // The workspace folders, for go-to-references and searches
    #[allow(deprecated)]
    let roots = match &init.workspace_folders {
        Some(folders) => folders.iter().map(|f| f.uri.clone()).collect(),
        None => init.root_uri.iter().cloned().collect::<Vec<_>>(),
    };
    server.set_roots(roots.iter().filter_map(workspace::uri_to_path).collect());
    // The library logoxide's config names (else the environment's, else the XDG folder)
    let config = match logoxide::config::Config::load() {
        Ok(config) => config,
        Err(err) => {
            warn(connection, err.to_string())?;
            Default::default()
        }
    };
    server.set_library(config.library(None, None).root().to_path_buf());
    let options = init.initialization_options.unwrap_or_default();
    if let Err(err) = server.configure(options) {
        warn(connection, err)?;
    }
    let running = Running::default();
    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request)? {
                    running.cancel_all();
                    return Ok(());
                }
                let response = match answer(&server, request) {
                    Answer::Later(id, search) => {
                        running.spawn(connection, id, search);
                        continue;
                    }
                    Answer::Now(response) => response,
                    Answer::Here(request) => respond(&server, request),
                };
                connection.sender.send(Message::Response(response))?;
            }
            Message::Notification(notification) if notification.method == Cancel::METHOD => {
                if let Ok(params) = serde_json::from_value::<CancelParams>(notification.params) {
                    running.cancel(&match params.id {
                        NumberOrString::Number(id) => RequestId::from(id),
                        NumberOrString::String(id) => RequestId::from(id),
                    });
                }
            }
            Message::Notification(notification)
                if notification.method == DidChangeConfiguration::METHOD =>
            {
                let Ok(params) =
                    serde_json::from_value::<DidChangeConfigurationParams>(notification.params)
                else {
                    continue;
                };
                match server.configure(params.settings) {
                    // New data can change which references exist
                    Ok(()) if pull && refresh => {
                        refreshes += 1;
                        let request = Request::new(
                            lsp_server::RequestId::from(format!("refresh-{refreshes}")),
                            WorkspaceDiagnosticRefresh::METHOD.into(),
                            serde_json::Value::Null,
                        );
                        connection.sender.send(Message::Request(request))?;
                    }
                    Ok(()) if pull => {}
                    Ok(()) => {
                        for uri in server.documents() {
                            publish_diagnostics(connection, &server, &uri)?;
                        }
                    }
                    Err(err) => warn(connection, err)?,
                }
            }
            Message::Notification(notification) => {
                if let Some(uri) = notify(&mut server, notification)
                    && !pull
                {
                    publish_diagnostics(connection, &server, &uri)?;
                }
            }
            Message::Response(_) => {}
        }
    }
    Ok(())
}

/// How a request is answered
enum Answer {
    /// By a workspace search on another thread
    Later(RequestId, Search),
    /// Already known (its parameters were wrong, or there is nothing to search for)
    Now(Response),
    /// On this thread ([`respond`])
    Here(Request),
}

/**
How to answer a request: workspace searches (go-to-references, the search command) run on another
thread, since searching every file can take seconds while completion and hover can't wait; other
requests are answered here
*/
fn answer(server: &Server, request: Request) -> Answer {
    let id = request.id.clone();
    let invalid = |id, err| {
        let code = lsp_server::ErrorCode::InvalidParams as i32;
        Answer::Now(Response::new_err(id, code, err))
    };
    match request.method.as_str() {
        References::METHOD => match params(request).map(|p| server.references_search(p)) {
            Ok(Some(search)) => Answer::Later(id, search),
            Ok(None) => Answer::Now(Response::new_ok(id, serde_json::Value::Null)),
            Err(err) => invalid(id, err),
        },
        ExecuteCommand::METHOD => match params(request).and_then(|p| server.command_search(p)) {
            Ok(search) => Answer::Later(id, search),
            Err(err) => invalid(id, err),
        },
        _ => Answer::Here(request),
    }
}

/// The workspace searches running on other threads, by request, each with the flag that cancels
/// it (`$/cancelRequest`)
#[derive(Clone, Default)]
struct Running(Arc<Mutex<HashMap<RequestId, Arc<AtomicBool>>>>);

impl Running {
    /// Runs a search on its own thread, which answers the request
    fn spawn(&self, connection: &Connection, id: RequestId, search: Search) {
        let cancelled = Arc::new(AtomicBool::new(false));
        if let Ok(mut running) = self.0.lock() {
            running.insert(id.clone(), cancelled.clone());
        }
        let sender = connection.sender.clone();
        let running = self.clone();
        std::thread::spawn(move || {
            let result = search.run(&Stop::when(cancelled.clone()));
            // Once it's no longer listed, a cancellation can't come in between this check and
            // the answer
            if let Ok(mut running) = running.0.lock() {
                running.remove(&id);
            }
            let response = search_response(id, result, cancelled.load(Ordering::Relaxed));
            // The editor may be gone already
            let _ = sender.send(Message::Response(response));
        });
    }

    fn cancel(&self, id: &RequestId) {
        if let Ok(running) = self.0.lock()
            && let Some(cancelled) = running.get(id)
        {
            cancelled.store(true, Ordering::Relaxed);
        }
    }

    /// At shutdown: no search needs to finish
    fn cancel_all(&self) {
        if let Ok(running) = self.0.lock() {
            for cancelled in running.values() {
                cancelled.store(true, Ordering::Relaxed);
            }
        }
    }
}

/// A workspace search's answer: its locations, or `RequestCancelled` if it was cancelled
fn search_response(
    id: RequestId,
    result: Result<(Vec<lsp_types::Location>, bool), String>,
    cancelled: bool,
) -> Response {
    match result {
        _ if cancelled => {
            let code = lsp_server::ErrorCode::RequestCanceled as i32;
            Response::new_err(id, code, String::from("cancelled"))
        }
        Ok((locations, _)) => Response::new_ok(id, json(locations)),
        Err(err) => Response::new_err(id, lsp_server::ErrorCode::InvalidParams as i32, err),
    }
}

fn respond(server: &Server, request: Request) -> Response {
    let id = request.id.clone();
    let result = match request.method.as_str() {
        Completion::METHOD => params(request).map(|p| json(server.completion(p))),
        HoverRequest::METHOD => params(request).map(|p| json(server.hover(p))),
        DocumentSymbolRequest::METHOD => params(request).map(|p| json(server.document_symbols(p))),
        InlayHintRequest::METHOD => params(request).map(|p| json(server.inlay_hints(p))),
        CodeActionRequest::METHOD => params(request).map(|p| json(server.code_actions(p))),
        GotoDefinition::METHOD | GotoDeclaration::METHOD => {
            params(request).map(|p| json(server.definition(p)))
        }
        ResolveCompletionItem::METHOD => {
            params(request).map(|p| json(server.resolve_completion(p)))
        }
        SemanticTokensFullRequest::METHOD => {
            params(request).map(|p| json(server.semantic_tokens(p)))
        }
        server::BOOK_REQUEST => params(request).and_then(|p| server.book(p).map(json)),
        server::FILE_TEXT_REQUEST => params(request).and_then(|p| server.file_text(p).map(json)),
        server::BIBLES_REQUEST => Ok(json(server.bibles())),
        server::TEMPLATES_REQUEST => params(request).map(|p| json(server.templates(p))),
        server::SAME_VERSE_REQUEST => params(request).and_then(|p| server.same_verse(p).map(json)),
        DocumentDiagnosticRequest::METHOD => {
            params(request).map(|p| json(server.pull_diagnostics(p)))
        }
        method => {
            let code = lsp_server::ErrorCode::MethodNotFound as i32;
            return Response::new_err(id, code, format!("unsupported request {method}"));
        }
    };
    match result {
        Ok(value) => Response::new_ok(id, value),
        Err(err) => Response::new_err(id, lsp_server::ErrorCode::InvalidParams as i32, err),
    }
}

/// Applies a document notification, returning the document whose diagnostics changed
fn notify(server: &mut Server, notification: Notification) -> Option<Uri> {
    let params = notification.params;
    match notification.method.as_str() {
        DidOpenTextDocument::METHOD => {
            let p: DidOpenTextDocumentParams = serde_json::from_value(params).ok()?;
            let uri = p.text_document.uri.clone();
            server.did_open(p);
            Some(uri)
        }
        DidChangeTextDocument::METHOD => {
            let p: DidChangeTextDocumentParams = serde_json::from_value(params).ok()?;
            let uri = p.text_document.uri.clone();
            server.did_change(p);
            Some(uri)
        }
        DidCloseTextDocument::METHOD => {
            let p: DidCloseTextDocumentParams = serde_json::from_value(params).ok()?;
            let uri = p.text_document.uri.clone();
            // Clears the document's diagnostics, since it is no longer open
            server.did_close(p);
            Some(uri)
        }
        _ => None,
    }
}

fn params<P: serde::de::DeserializeOwned>(request: Request) -> Result<P, String> {
    serde_json::from_value(request.params).map_err(|e| e.to_string())
}

fn json(value: impl serde::Serialize) -> serde_json::Value {
    serde_json::to_value(value).unwrap_or(serde_json::Value::Null)
}

#[cfg(test)]
mod tests {
    use lsp_server::{Connection, Message, Notification, Request, RequestId};
    use serde_json::json;

    /// A whole session over an in-memory connection
    #[test]
    fn session() {
        let (client, server) = Connection::memory();
        let thread = std::thread::spawn(move || super::run(&server).unwrap());
        let send = |message: Message| client.sender.send(message).unwrap();
        let request = |id: i32, method: &str, params| {
            Message::Request(Request::new(RequestId::from(id), method.into(), params))
        };

        send(request(1, "initialize", json!({ "capabilities": {} })));
        let Message::Response(init) = client.receiver.recv().unwrap() else {
            panic!("expected the initialize response")
        };
        assert!(init.response_result.unwrap()["capabilities"]["completionProvider"].is_object());
        send(Message::Notification(Notification::new(
            "initialized".into(),
            json!({}),
        )));

        let uri = "file:///a.md";
        send(Message::Notification(Notification::new(
            "textDocument/didOpen".into(),
            json!({ "textDocument": { "uri": uri, "languageId": "markdown", "version": 1, "text": "Read Phil 4:1" } }),
        )));
        // Opening a document publishes its diagnostics
        let Message::Notification(diagnostics) = client.receiver.recv().unwrap() else {
            panic!("expected diagnostics")
        };
        assert_eq!(diagnostics.method, "textDocument/publishDiagnostics");
        send(request(
            2,
            "textDocument/hover",
            json!({ "textDocument": { "uri": uri }, "position": { "line": 0, "character": 6 } }),
        ));
        let Message::Response(hover) = client.receiver.recv().unwrap() else {
            panic!("expected the hover response")
        };
        let result = hover.response_result.unwrap();
        let value = result["contents"]["value"].as_str().unwrap();
        assert!(value.starts_with("**Philippians 4:1**\n\n"), "{value}");
        assert!(value.contains("- **OSIS**: `Phil.4.1`"), "{value}");

        // Go to references is answered from another thread; cancelling it once it's answered
        // does nothing
        send(request(
            4,
            "textDocument/references",
            json!({ "textDocument": { "uri": uri }, "position": { "line": 0, "character": 6 }, "context": { "includeDeclaration": true } }),
        ));
        let Message::Response(references) = client.receiver.recv().unwrap() else {
            panic!("expected the references response")
        };
        assert_eq!(references.id, RequestId::from(4));
        let result = references.response_result.unwrap();
        assert_eq!(result[0]["uri"], uri);
        send(Message::Notification(Notification::new(
            "$/cancelRequest".into(),
            json!({ "id": 4 }),
        )));

        send(request(3, "shutdown", json!(null)));
        let Message::Response(shutdown) = client.receiver.recv().unwrap() else {
            panic!("expected the shutdown response")
        };
        assert_eq!(shutdown.id, RequestId::from(3));
        send(Message::Notification(Notification::new(
            "exit".into(),
            json!(null),
        )));
        thread.join().unwrap();
    }

    /// A cancelled search stops between files and answers `RequestCancelled`
    #[test]
    fn cancelled_searches() {
        use std::sync::{Arc, atomic::Ordering};

        let (client, server) = Connection::memory();
        let dir = std::env::temp_dir().join(format!("logoxide-lsp-cancel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for i in 0..50 {
            std::fs::write(dir.join(format!("{i}.md")), "Read John 3:16").unwrap();
        }
        let mut lsp = super::Server::new(Default::default());
        lsp.set_roots(vec![dir.clone()]);
        let running = super::Running::default();
        // Cancelled before it starts: the flag is the one `$/cancelRequest` sets
        let search = || {
            let command = json!({ "command": "logoxide.search", "arguments": [{ "mode": "explicit-overlap", "passage": "John 3:16" }] });
            lsp.command_search(serde_json::from_value(command).unwrap())
                .unwrap()
        };
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        running
            .0
            .lock()
            .unwrap()
            .insert(RequestId::from(7), cancelled.clone());
        running.cancel(&RequestId::from(7));
        assert!(cancelled.load(Ordering::Relaxed));
        let result = search().run(&super::Stop::when(cancelled.clone()));
        assert!(!result.as_ref().unwrap().1, "the walk stops");
        let response = super::search_response(RequestId::from(7), result, true);
        let error = response.response_result.unwrap_err();
        assert_eq!(error.code, -32800);
        // Not cancelled: every file's reference
        running.spawn(&server, RequestId::from(8), search());
        let Message::Response(response) = client.receiver.recv().unwrap() else {
            panic!("expected the search's response")
        };
        assert_eq!(response.id, RequestId::from(8));
        assert_eq!(
            response.response_result.unwrap().as_array().unwrap().len(),
            50
        );
        // Answered, so no longer running
        assert!(!running.0.lock().unwrap().contains_key(&RequestId::from(8)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Editors that support pull diagnostics ask for them, and get none pushed
    #[test]
    fn pull_diagnostics() {
        let (client, server) = Connection::memory();
        let thread = std::thread::spawn(move || super::run(&server).unwrap());
        let send = |message: Message| client.sender.send(message).unwrap();
        let request = |id: i32, method: &str, params| {
            Message::Request(Request::new(RequestId::from(id), method.into(), params))
        };
        send(request(
            1,
            "initialize",
            json!({ "capabilities": { "textDocument": { "diagnostic": {} } } }),
        ));
        let Message::Response(init) = client.receiver.recv().unwrap() else {
            panic!("expected the initialize response")
        };
        let result = init.response_result.unwrap();
        assert_eq!(
            result["capabilities"]["diagnosticProvider"]["identifier"],
            "logoxide"
        );
        send(Message::Notification(Notification::new(
            "initialized".into(),
            json!({}),
        )));
        send(Message::Notification(Notification::new(
            "textDocument/didOpen".into(),
            json!({ "textDocument": { "uri": "file://", "languageId": "", "version": 1, "text": "Read jn 3:16" } }),
        )));
        // No diagnostics are pushed: the next message is the answer to this request
        send(request(
            2,
            "textDocument/diagnostic",
            json!({ "textDocument": { "uri": "file://" } }),
        ));
        let Message::Response(report) = client.receiver.recv().unwrap() else {
            panic!("expected the diagnostic report, not a pushed notification")
        };
        let report = report.response_result.unwrap();
        assert_eq!(report["kind"], "full");
        assert_eq!(report["items"][0]["message"], "John 3:16");

        send(request(3, "shutdown", json!(null)));
        client.receiver.recv().unwrap();
        send(Message::Notification(Notification::new(
            "exit".into(),
            json!(null),
        )));
        thread.join().unwrap();
    }
}
