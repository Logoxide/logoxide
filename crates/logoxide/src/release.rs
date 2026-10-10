/*!
Releasing a package version: its summary is written, compared with the previous release's, and
the version number checked against what changed, so a version always says what it can break (see
[`Change`]). Then the repository is committed and tagged `v{version}`.
*/

use std::{path::Path, sync::Arc};

use crate::{
    error::{LogoxideError, Result},
    git::{Fetcher, GitCli, GitRef},
    package::{Features, Package},
    storage::{FsStorage, MemoryStorage, Storage},
    summary::{
        Change, Diff, HashRule, SUMMARY, Summary, VERSES, VerseHash, diff_summaries, next_version,
        summarize, version_change,
    },
};

/// A version's summary and verse hashes: published with it, or (for an older release without
/// them) built from its text
pub fn summary_at(
    fetcher: &dyn Fetcher,
    url: &str,
    tag: &str,
) -> Result<(Summary, Vec<VerseHash>)> {
    summary_in(fetcher, url, tag, "")
}

/// [`summary_at`] for a package in a folder of the repository (`prefix`, `romans/`)
pub fn summary_in(
    fetcher: &dyn Fetcher,
    url: &str,
    tag: &str,
    prefix: &str,
) -> Result<(Summary, Vec<VerseHash>)> {
    let reference = GitRef::Tag(tag.to_string());
    let paths: Vec<String> = [SUMMARY, VERSES, crate::references::REFERENCES]
        .iter()
        .map(|p| format!("{prefix}{p}"))
        .collect();
    let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
    let fetched = MemoryStorage::new();
    fetcher.fetch_files(url, &reference, &paths, &fetched)?;
    // Hashes made by another rule (or in the layout from before rules) would differ in every
    // verse: built again from the text, as for a release without them
    if let Ok(published) = Summary::read(&within(&fetched, prefix)?)
        && published.0.verse_hash == HashRule::CURRENT.number()
    {
        return Ok(published);
    }
    let all = MemoryStorage::new();
    fetcher.fetch(url, &reference, &all)?;
    summarize(&Package::open(
        Arc::new(within(&all, prefix)?),
        Features::default(),
    )?)
}

/// The files under a folder (`prefix`, with its slash), as if it were the root
pub fn within(storage: &dyn Storage, prefix: &str) -> Result<MemoryStorage> {
    let out = MemoryStorage::new();
    for path in storage.walk("")? {
        if let Some(inside) = path.strip_prefix(prefix) {
            out.write(inside, &storage.read(&path)?)?;
        }
    }
    Ok(out)
}

/// A package's release tag: `v1.2.0`, or `romans/v1.2.0` for one in a folder of its repository
pub fn tag_for(prefix: &str, version: &semver::Version) -> String {
    format!("{prefix}v{version}")
}

/// What a release would be
pub struct Plan {
    pub summary: Summary,
    pub verses: Vec<VerseHash>,
    /// The previous release's version and what changed since
    pub previous: Option<(semver::Version, Diff)>,
    /// The smallest version this may be released as
    pub minimum: Option<semver::Version>,
    /// Files, or books in them, that the format passed over (peripherals, books topos doesn't
    /// know): their verses aren't in the summary
    pub skipped: Vec<crate::source::Skipped>,
}

impl Plan {
    pub fn change(&self) -> Change {
        self.previous
            .as_ref()
            .map_or(Change::Patch, |(_, d)| d.change())
    }
}

/// Summarizes the package in `repo` and compares it with the newest earlier release. Media
/// timings that don't fit the text are an error. Links in the folder are passed over, as
/// fetching and packing the release pass over them, so the summary is of what is published
pub fn plan(repo: &Path, git: &GitCli) -> Result<Plan> {
    let storage: Arc<dyn Storage> = Arc::new(FsStorage::new(repo).no_symlinks());
    let package = Package::open(storage, Features::default())?;
    let problems = crate::media::check_package_timings(&package)?;
    if !problems.is_empty() {
        return Err(LogoxideError::Media(format!(
            "the timings don't fit the text (lx media check --offline): {}",
            problems.join("; ")
        )));
    }
    let untitled = crate::titles::untitled(&package)?;
    if !untitled.is_empty() {
        return Err(LogoxideError::Manifest(format!(
            "no title for {} (give one in logoxide.toml's [titles])",
            untitled.join(", ")
        )));
    }
    let (summary, verses) = summarize(&package)?;
    let skipped = package.source().skipped().to_vec();
    let tags = git.local_versions(repo)?;
    // The newest release (the version must be newer than it)
    let Some((old_version, tag)) = tags.last().cloned() else {
        return Ok(Plan {
            summary,
            verses,
            previous: None,
            minimum: None,
            skipped,
        });
    };
    // The repository's folder (absolute: git runs in a scratch folder), and the package's in it
    let (root, prefix) = git.package_in(repo)?;
    let url = root.display().to_string();
    let (old_summary, old_verses) = summary_in(git, &url, &tag, &prefix)?;
    let d = diff_summaries((&old_summary, &old_verses), (&summary, &verses));
    let minimum = Some(next_version(&old_version, d.change()));
    Ok(Plan {
        summary,
        verses,
        previous: Some((old_version, d)),
        minimum,
        skipped,
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
        None => Err(LogoxideError::Resolve(format!(
            "version {new} isn't newer than the last release, {old}: release this as {minimum} or later"
        ))),
        Some(change) if change < needed => Err(LogoxideError::Resolve(format!(
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
        (d.joined.len(), "joined into a bridge"),
        (d.bridged.len(), "bridged differently"),
        (d.split.len(), "split from a bridge"),
        (d.changed.len(), "reworded"),
        (d.added.len(), "added"),
    ]
    .into_iter()
    .filter(|(n, _)| *n > 0)
    .map(|(n, what)| format!("{n} verse{} {what}", if n == 1 { "" } else { "s" }))
    .collect();
    let what = if d.files_changed {
        String::from("its files changed (it has no verses to compare)")
    } else if parts.is_empty() {
        String::from("no verse changed")
    } else {
        parts.join(", ")
    };
    match d.retitled.as_slice() {
        [] => what,
        retitled => {
            let titles: Vec<String> = (retitled.iter())
                .map(|(path, old, new)| format!("{path}: “{old}” is “{new}”"))
                .collect();
            format!("{what}; retitled {}", titles.join(", "))
        }
    }
}

/**
Writes the summary into the package's folder, then commits it (only that folder) and tags it
`v{version}` (`romans/v{version}` for a package in a folder of its repository); the tag and commit
*/
pub fn release(repo: &Path, plan: &Plan, git: &GitCli) -> Result<(String, String)> {
    check(plan)?;
    plan.summary
        .write(&plan.verses, &FsStorage::new(repo).no_symlinks())?;
    let (_, prefix) = git.package_in(repo)?;
    let tag = tag_for(&prefix, &plan.summary.version);
    let commit = git.commit_and_tag(repo, &format!("Release {tag}"), &tag)?;
    Ok((tag, commit))
}
