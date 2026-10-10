//! `logoxide-mcp`: logoxide's MCP server over stdio (see the crate's README)

use std::{path::PathBuf, process::ExitCode, sync::Arc};

use clap::Parser;
use logoxide::manifest::MANIFEST;
use logoxide_mcp::{Context, Server};
use rmcp::ServiceExt;

/// An MCP server for a logoxide project, over stdio
#[derive(Parser, Debug)]
#[command(name = "logoxide-mcp", version)]
struct Args {
    /// The folder with the logoxide.toml to use: the current one if it has one, else the
    /// library's own
    #[arg(long)]
    project: Option<PathBuf>,
    /// The library (LOGOXIDE_HOME, else the config's `library`, else $XDG_DATA_HOME/logoxide)
    #[arg(long)]
    library: Option<PathBuf>,
    /// The cache (LOGOXIDE_CACHE, else the config's `cache`, else $XDG_CACHE_HOME/logoxide)
    #[arg(long)]
    cache_dir: Option<PathBuf>,
    /// Offer the tools that change the project and the library: install and import (from the
    /// project's catalogs only)
    #[arg(long)]
    allow_writes: bool,
}

/// What the server's downloads say they are
fn user_agent() -> String {
    format!(
        "logoxide-mcp/{} (+https://github.com/Logoxide/logoxide)",
        env!("CARGO_PKG_VERSION")
    )
}

/// Something said on stderr (stdout is the protocol's), cleaned for a terminal
fn say(text: &str) {
    eprintln!("logoxide-mcp: {}", logoxide::text::printable(text));
}

#[tokio::main]
async fn main() -> ExitCode {
    let args = Args::parse();
    let config = match logoxide::config::Config::load() {
        Ok(config) => config,
        Err(e) => {
            say(&e.to_string());
            return ExitCode::FAILURE;
        }
    };
    let library = config.library(args.library.clone(), args.cache_dir.clone());
    let project = match &args.project {
        Some(dir) => dir.clone(),
        None if std::path::Path::new(MANIFEST).is_file() => PathBuf::from("."),
        None => library.root().to_path_buf(),
    };
    let project = std::fs::canonicalize(&project).unwrap_or(project);
    // Downloads and their warnings go to stderr, which clients log
    let notes: logoxide_app::Notes = Arc::new(|note: &logoxide_app::Note| {
        if note.is_warning() {
            say(&format!("warning: {note}"));
        }
    });
    let app = logoxide_app::Logoxide::default_native(config, library, &user_agent(), notes);
    let context = Context::new(app, project, args.allow_writes);
    let service = match Server::new(context).serve(rmcp::transport::stdio()).await {
        Ok(service) => service,
        Err(e) => {
            say(&e.to_string());
            return ExitCode::FAILURE;
        }
    };
    match service.waiting().await {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            say(&e.to_string());
            ExitCode::FAILURE
        }
    }
}
