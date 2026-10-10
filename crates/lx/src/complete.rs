/*!
Shell completion, run by `lx` itself (`source <(COMPLETE=bash lx)`; also `zsh`, `fish`), so it
knows the project's registries and dependencies: `lx add dorean-local:<Tab>` lists that
registry's packages, by path and by id. The project is the current folder's, else the library's
own, as when `--project` isn't given. Registries on the web aren't downloaded for this
*/

use std::{ffi::OsStr, path::PathBuf};

use clap_complete::engine::CompletionCandidate;
use logoxide::{Project, manifest::MANIFEST};

/// logoxide's config, or its defaults when it can't be read (completing never fails)
fn config() -> logoxide::config::Config {
    logoxide::config::Config::load().unwrap_or_default()
}

/// The project `lx` would use here without `--project`
fn project() -> Option<Project> {
    let dir = match std::path::Path::new(MANIFEST).is_file() {
        true => PathBuf::from("."),
        false => (config().library(None, None).root()).to_path_buf(),
    };
    Project::open(dir).ok()
}

/// A completion, with help from a catalog or manifest made safe to show
fn candidate(value: String, help: String) -> CompletionCandidate {
    let help = logoxide::text::printable(&help).into_owned();
    CompletionCandidate::new(value).help(Some(help.into()))
}

/**
A package to add: `<registry>:` for each of the project's registries and, after one, its
packages (`dorean-local:books/romans`, and by id when no other package there has it), then
`github:`, `gitlab:`, and `codeberg:`
*/
pub fn package(current: &OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_string_lossy();
    let mut found = vec![];
    let registries = project().map(|p| (config().registries(p.manifest()), p.dir().to_path_buf()));
    if let Some((registries, dir)) = &registries {
        match logoxide::catalog::split_registry_name(&current, registries)
            .map(|(r, _)| r)
            .or_else(|| {
                current
                    .strip_suffix(':')
                    .filter(|r| registries.contains_key(*r))
            }) {
            Some(registry) => {
                let location = &registries[registry];
                if location.contains("://") {
                    return vec![];
                }
                let Ok(catalog) =
                    crate::cmd::catalog::load_registry(registry, location, dir, false)
                else {
                    return vec![];
                };
                for entry in &catalog.entries {
                    let help = format!(
                        "{} {}",
                        entry.name,
                        entry.version.as_deref().unwrap_or_default()
                    );
                    let path = entry.path.as_deref().unwrap_or(&entry.id);
                    found.push(candidate(format!("{registry}:{path}"), help.clone()));
                    let shared = (catalog.entries.iter())
                        .filter(|e| e.id == entry.id)
                        .count()
                        > 1;
                    if !shared && path != entry.id {
                        found.push(candidate(format!("{registry}:{}", entry.id), help));
                    }
                }
            }
            None => {
                for (name, location) in registries {
                    found.push(candidate(
                        format!("{name}:"),
                        format!("registry: {location}"),
                    ));
                }
            }
        }
    }
    if !current.contains(':') {
        for (host, url) in logoxide::git::HOSTS {
            found.push(candidate(format!("{host}:"), format!("{url}/owner/repo")));
        }
    }
    found.retain(|c| {
        c.get_value()
            .to_string_lossy()
            .starts_with(current.as_ref())
    });
    found
}

/// What the project's logoxide.toml names (`lx remove`, `lx update`, `lx info`)
pub fn dependency(current: &OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_string_lossy();
    let Some(project) = project() else {
        return vec![];
    };
    (project.manifest().dependencies.keys())
        .filter(|name| name.starts_with(current.as_ref()))
        .map(CompletionCandidate::new)
        .collect()
}

/// The project's registries, by name (`lx search --registry`)
pub fn registry(current: &OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_string_lossy();
    let Some(project) = project() else {
        return vec![];
    };
    (config().registries(project.manifest()).into_iter())
        .filter(|(name, _)| name.starts_with(current.as_ref()))
        .map(|(name, location)| candidate(name, location))
        .collect()
}

/// What `lx catalog remove` takes: the project's own registries and catalogs (not the config's)
pub fn searched(current: &OsStr) -> Vec<CompletionCandidate> {
    let current = current.to_string_lossy();
    let mut found = vec![];
    if let Some(project) = project() {
        found.extend(
            (project.manifest().registries().into_iter())
                .filter(|(name, _)| name.starts_with(current.as_ref()))
                .map(|(name, location)| candidate(name, location)),
        );
        found.extend(
            (project.manifest().catalogs().into_iter())
                .filter(|c| c.starts_with(current.as_ref()))
                .map(CompletionCandidate::new),
        );
    }
    found
}
