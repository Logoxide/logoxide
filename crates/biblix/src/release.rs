/*!
Releasing a package version: its summary is written, compared with the previous release's, and
the version number checked against what changed, so a version always says what it can break (see
[`Change`]). Then the repository is committed and tagged `v{version}`.
*/

use std::{path::Path, sync::Arc};

use crate::{
    error::{BiblixError, Result},
    git::{Fetcher, GitCli, GitRef},
    package::{Features, Package},
    storage::{FsStorage, Storage},
    summary::{
        Change, Diff, SUMMARY, Summary, VERSES, VerseHash, diff, next_version, summarize,
        version_change,
    },
};

/// A version's summary and verse hashes: published with it, or (for an older release without
/// them) built from its text
pub fn summary_at(
    fetcher: &dyn Fetcher,
    url: &str,
    tag: &str,
    scratch: &Path,
) -> Result<(Summary, Vec<VerseHash>)> {
    let dir = scratch.join(format!("summary-{}-{tag}", std::process::id()));
    let result = (|| {
        let reference = GitRef::Tag(tag.to_string());
        fetcher.fetch_files(url, &reference, &[SUMMARY, VERSES], &dir)?;
        let storage = FsStorage::new(&dir);
        if let Ok(published) = Summary::read(&storage) {
            return Ok(published);
        }
        let _ = std::fs::remove_dir_all(&dir);
        fetcher.fetch(url, &reference, &dir)?;
        let storage: Arc<dyn Storage> = Arc::new(FsStorage::new(&dir));
        summarize(&Package::open(storage, Features::default())?)
    })();
    let _ = std::fs::remove_dir_all(&dir);
    result
}

/// What a release would be
pub struct Plan {
    pub summary: Summary,
    pub verses: Vec<VerseHash>,
    /// The previous release's version and what changed since
    pub previous: Option<(semver::Version, Diff)>,
    /// The smallest version this may be released as
    pub minimum: Option<semver::Version>,
}

impl Plan {
    pub fn change(&self) -> Change {
        self.previous
            .as_ref()
            .map_or(Change::Patch, |(_, d)| d.change())
    }
}

/// Summarizes the package in `repo` and compares it with the newest earlier release
pub fn plan(repo: &Path, git: &GitCli, scratch: &Path) -> Result<Plan> {
    let storage: Arc<dyn Storage> = Arc::new(FsStorage::new(repo));
    let package = Package::open(storage, Features::default())?;
    let (summary, verses) = summarize(&package)?;
    let tags = git.local_versions(repo)?;
    // The newest release (the version must be newer than it)
    let Some((old_version, tag)) = tags.last().cloned() else {
        return Ok(Plan {
            summary,
            verses,
            previous: None,
            minimum: None,
        });
    };
    // Absolute: git runs in a scratch folder
    let url = repo
        .canonicalize()
        .map_err(|e| BiblixError::io(&repo.display().to_string(), e))?
        .display()
        .to_string();
    let (_, old_verses) = summary_at(git, &url, &tag, scratch)?;
    let d = diff(&old_verses, &verses);
    let minimum = Some(next_version(&old_version, d.change()));
    Ok(Plan {
        summary,
        verses,
        previous: Some((old_version, d)),
        minimum,
    })
}

/// Whether the package's version may be released: newer than the previous release by at least as
/// much as what changed
pub fn check(plan: &Plan) -> Result<()> {
    let Some((old, d)) = &plan.previous else {
        return Ok(());
    };
    let new = &plan.summary.version;
    let needed = d.change();
    let minimum = next_version(old, needed);
    match version_change(old, new) {
        None => Err(BiblixError::Resolve(format!(
            "version {new} isn't newer than the last release, {old}: release this as {minimum} or later"
        ))),
        Some(change) if change < needed => Err(BiblixError::Resolve(format!(
            "{} since {old} is a {needed:?} change, so {new} is too small: release it as {minimum} or later",
            describe(d)
        ))),
        Some(_) => Ok(()),
    }
}

/// `3 verses reworded, 1 renumbered`
pub fn describe(d: &Diff) -> String {
    let parts: Vec<String> = [
        (d.removed.len(), "removed"),
        (d.moved.len(), "renumbered"),
        (d.bridged.len(), "bridged differently"),
        (d.changed.len(), "reworded"),
        (d.added.len(), "added"),
    ]
    .into_iter()
    .filter(|(n, _)| *n > 0)
    .map(|(n, what)| format!("{n} verse{} {what}", if n == 1 { "" } else { "s" }))
    .collect();
    if parts.is_empty() {
        String::from("no verse changed")
    } else {
        parts.join(", ")
    }
}

/// Writes the summary into the repository, then commits and tags it `v{version}`
pub fn release(repo: &Path, plan: &Plan, git: &GitCli) -> Result<String> {
    check(plan)?;
    plan.summary.write(&plan.verses, &FsStorage::new(repo))?;
    let tag = format!("v{}", plan.summary.version);
    git.commit_and_tag(repo, &format!("Release {tag}"), &tag)
}
