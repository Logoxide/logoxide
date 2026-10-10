//! A scripted client session: `logoxide-mcp` spawned, then `initialize`, `tools/list`, and
//! `tools/call` sent over stdio as JSON-RPC, on the fixture project and (when they're there) the
//! real KJV and BSB in `~/Development/logoxide-packages`. Each run has its own library, cache,
//! and config

use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::Instant,
};

use serde_json::{Value, json};

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        match entry.file_type().unwrap().is_dir() {
            true => copy(&entry.path(), &target),
            false => {
                std::fs::copy(entry.path(), target).unwrap();
            }
        }
    }
}

/// A server, and the client's end of its stdio
struct Session {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next: u64,
    root: PathBuf,
}

impl Session {
    /// The server on `project` (in `root`, which holds its library, cache, and config)
    fn start(root: &Path, project: &Path, writes: bool) -> Session {
        std::fs::write(root.join("config.toml"), "").unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_logoxide-mcp"));
        command
            .arg("--project")
            .arg(project)
            .env("LOGOXIDE_HOME", root.join("library"))
            .env("LOGOXIDE_CACHE", root.join("cache"))
            .env("LOGOXIDE_CONFIG", root.join("config.toml"))
            .env("XDG_RUNTIME_DIR", root.join("runtime"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        if writes {
            command.arg("--allow-writes");
        }
        let mut child = command.spawn().unwrap();
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        let mut session = Session {
            child,
            stdin,
            stdout,
            next: 1,
            root: root.to_path_buf(),
        };
        let init = session.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "session-test", "version": "1" },
            }),
        );
        assert_eq!(init["serverInfo"]["name"], "logoxide-mcp");
        assert!(init["capabilities"]["tools"].is_object());
        assert!(init["instructions"].as_str().unwrap().contains("untrusted"));
        session.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        session
    }

    fn send(&mut self, message: Value) {
        writeln!(self.stdin, "{message}").unwrap();
        self.stdin.flush().unwrap();
    }

    /// A request's answer (its result or its error); every line the server writes must be
    /// JSON-RPC
    fn answer(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let mut line = String::new();
            assert!(
                self.stdout.read_line(&mut line).unwrap() > 0,
                "the server stopped"
            );
            let message: Value = serde_json::from_str(&line)
                .unwrap_or_else(|e| panic!("not JSON-RPC on stdout ({e}): {line}"));
            assert_eq!(message["jsonrpc"], "2.0");
            if message["id"] == json!(id) {
                return message;
            }
        }
    }

    /// A request's result
    fn request(&mut self, method: &str, params: Value) -> Value {
        let message = self.answer(method, params);
        assert!(message["error"].is_null(), "{method}: {message}");
        message["result"].clone()
    }

    /// A tool's structured result, which must not be an error
    fn call(&mut self, tool: &str, arguments: Value) -> Value {
        let result = self.call_result(tool, arguments.clone());
        assert_eq!(result["isError"], false, "{tool} {arguments}: {result}");
        // The same JSON as text, for clients that read only that
        let text: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert_eq!(text, result["structuredContent"]);
        result["structuredContent"].clone()
    }

    fn call_result(&mut self, tool: &str, arguments: Value) -> Value {
        self.request(
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
        )
    }

    /// The tools listed, by name
    fn tools(&mut self) -> Vec<String> {
        let listed = self.request("tools/list", json!({}));
        (listed["tools"].as_array().unwrap().iter())
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect()
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn temp(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("logoxide-mcp-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn a_session_on_the_fixture_project() {
    let root = temp("session");
    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"),
        &root,
    );
    let mut session = Session::start(&root, &root.join("vault"), true);
    let tools = session.tools();
    for name in [
        "search_catalogs",
        "package_info",
        "list_packages",
        "read_passage",
        "compare_passage",
        "map_reference",
        "show_summary",
        "diff_versions",
        "cited_references",
        "read_notes",
        "lookup_entry",
        "cross_references",
        "install",
        "import",
    ] {
        assert!(tools.iter().any(|t| t == name), "{name}: {tools:?}");
    }
    let installed = session.call("install", json!({}));
    assert_eq!(installed["installed"].as_array().map(Vec::len), Some(6));
    let read = session.call(
        "read_passage",
        json!({ "reference": "John 3:16", "bible": "ONE" }),
    );
    assert_eq!(
        read["untrusted"]["text"],
        "[16] For God so loved the world."
    );
    assert_eq!(read["license"]["free"], true);
    let compared = session.call(
        "compare_passage",
        json!({ "reference": "John 3:16 ONE", "bibles": ["ONE", "ARR"] }),
    );
    assert_eq!(compared["passages"][1]["reference"], "John 3:17");
    // A tool's failure is a tool error, in words
    let failed = session.call_result("read_passage", json!({ "reference": "nowhere 9:9" }));
    assert_eq!(failed["isError"], true);
    assert!(
        (failed["content"][0]["text"].as_str().unwrap()).contains("isn't a Bible reference"),
        "{failed}"
    );
    // Writes aren't listed without --allow-writes
    drop(session);
    let root = temp("session-read-only");
    copy(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures"),
        &root,
    );
    let mut session = Session::start(&root, &root.join("vault"), false);
    let tools = session.tools();
    assert_eq!(tools.len(), 12, "{tools:?}");
    let refused = session.answer("tools/call", json!({ "name": "install", "arguments": {} }));
    assert_eq!(refused["error"]["code"], -32602, "{refused}");
}

/// The installed package's summary
fn summary_of(session: &mut Session, package: &str) -> Value {
    session.call("show_summary", json!({ "package": package }))
}

/// The hand-made packages' repositories, if they're on this machine
fn real_packages() -> Option<(PathBuf, PathBuf)> {
    let home = std::env::var_os("HOME")?;
    let packages = Path::new(&home).join("Development/logoxide-packages");
    let (kjv, bsb) = (packages.join("kjv"), packages.join("bsb"));
    match kjv.join(".git").exists() && bsb.join(".git").exists() {
        true => Some((kjv, bsb)),
        false => None,
    }
}

#[test]
fn a_session_on_the_kjv_and_the_bsb() {
    let Some((kjv, bsb)) = real_packages() else {
        eprintln!("skipped: no ~/Development/logoxide-packages/kjv and bsb");
        return;
    };
    let root = temp("real");
    let vault = root.join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    std::fs::write(
        vault.join("logoxide.toml"),
        format!(
            "[settings]\ndefault-catalogs = false\n\n[dependencies]\nkjv = {{ git = \"{}\" }}\nbsb = {{ git = \"{}\" }}\n",
            kjv.display(),
            bsb.display()
        ),
    )
    .unwrap();
    let started = Instant::now();
    let mut session = Session::start(&root, &vault, true);
    session.call("install", json!({}));
    let listed = session.call("list_packages", json!({}));
    assert_eq!(listed["total"], 2);
    let info = session.call("package_info", json!({ "package": "KJV" }));
    assert_eq!(info["id"], "kjv");
    assert_eq!(info["books"].as_array().map(Vec::len), Some(66));
    assert_eq!(info["free"], true);
    let read = session.call(
        "read_passage",
        json!({ "reference": "John 3:16 KJV", "labels": false }),
    );
    let text = read["untrusted"]["text"].as_str().unwrap();
    assert!(text.contains("For God so loved the world"), "{text}");
    assert_eq!(
        read["license"]["untrusted"]["license"],
        "LicenseRef-PublicDomain"
    );
    // A long passage comes in pages, each with its license
    let first = session.call("read_passage", json!({ "reference": "Genesis 1-10 BSB" }));
    assert!(first["parts"].as_u64().unwrap() >= 2, "{}", first["parts"]);
    let next = session.call(
        "read_passage",
        json!({ "reference": "Genesis 1-10 BSB", "cursor": first["nextCursor"] }),
    );
    assert_eq!(next["part"], 2);
    assert!(next["license"].is_object());
    let compared = session.call(
        "compare_passage",
        json!({ "reference": "Romans 8:28", "bibles": ["KJV", "BSB"] }),
    );
    assert_eq!(compared["passages"].as_array().map(Vec::len), Some(2));
    let summary = session.call("show_summary", json!({ "package": "bsb" }));
    assert_eq!(summary["published"], true);
    assert_eq!(summary["books"].as_array().map(Vec::len), Some(66));
    // The released version, read from the repository's tag: the same text as installed
    let released = session.call(
        "show_summary",
        json!({ "package": "kjv", "version": "1.0.3" }),
    );
    assert_eq!(
        released["textChecksum"],
        summary_of(&mut session, "kjv")["textChecksum"]
    );
    let diff = session.call(
        "diff_versions",
        json!({ "package": "kjv", "from": "1.0.3" }),
    );
    assert_eq!(
        (diff["to"].as_str(), diff["total"].as_u64()),
        (Some("1.0.3"), Some(0))
    );
    // Its tags before 1.0.3 are from before logoxide's rename (biblix.toml): a tool error
    let old = session.call_result(
        "diff_versions",
        json!({ "package": "kjv", "from": "1.0.0" }),
    );
    assert_eq!(old["isError"], true, "{old}");
    eprintln!(
        "KJV and BSB session: {:.1} s",
        started.elapsed().as_secs_f64()
    );
}
