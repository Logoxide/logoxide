//! `lx media`: a package's audio and video, listed by URL and hash

use std::sync::Arc;

use logoxide::{FsStorage, Project, manifest::MANIFEST};

use crate::{cmd::package::timings_report, net};

pub(crate) fn media_add(
    file: &std::path::Path,
    urls: &[String],
    book: Option<&str>,
    package: &std::path::Path,
) -> logoxide::Result<()> {
    let bytes = std::fs::read(file).map_err(|e| logoxide::LogoxideError::Io {
        path: file.display().to_string(),
        message: e.to_string(),
    })?;
    let name = file
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let (url, mirrors) = urls.split_first().ok_or_else(|| {
        logoxide::LogoxideError::Media(String::from("where is it served? (--url)"))
    })?;
    let mut entry = logoxide::media::MediaFile::of(&name, url, &bytes);
    entry.mirrors = mirrors.to_vec();
    entry.book = book.map(str::to_uppercase);
    let path = package.join(MANIFEST);
    if !path.is_file() {
        return Err(logoxide::LogoxideError::NotFound(format!(
            "{} (the package's folder: --package)",
            path.display()
        )));
    }
    let mut manifest = Project::open(package)?;
    manifest.add_media(&entry)?;
    manifest.save_manifest()?;
    println!(
        "listed {} ({:.1} MB, sha256 {}) in {}",
        entry.path,
        entry.size as f64 / 1e6,
        &entry.sha256[..16],
        path.display()
    );
    Ok(())
}

pub(crate) fn media_check(package: &std::path::Path, offline: bool) -> logoxide::Result<()> {
    let opened = logoxide::Package::open(Arc::new(FsStorage::new(package)), Default::default())?;
    let files = (opened.manifest().media.as_ref())
        .map(|m| m.files.clone())
        .unwrap_or_default();
    if files.is_empty() {
        println!("no media listed");
        return Ok(());
    }
    let timings = logoxide::media::check_package_timings(&opened)?;
    if opened
        .manifest()
        .media
        .as_ref()
        .is_some_and(|m| m.timings.is_some())
        && timings.is_empty()
    {
        println!("timings: ok");
    }
    timings_report(&timings)?;
    if offline {
        return Ok(());
    }
    // Every mirror too: a host falls back to it, so it must serve the same bytes
    let (mut checked, mut failed) = (0, 0);
    for file in &files {
        for url in file.urls() {
            checked += 1;
            let shown = match url == file.url {
                true => file.path.clone(),
                false => format!("{} (mirror {url})", file.path),
            };
            // Hashed as it downloads: media can be larger than memory should hold
            match net::verify_media(url, file) {
                Ok(()) => println!("ok: {shown}"),
                Err(error) => {
                    failed += 1;
                    println!("FAILED: {shown}: {error}");
                }
            }
        }
    }
    match failed {
        0 => Ok(()),
        n => Err(logoxide::LogoxideError::Fetch {
            url: package.display().to_string(),
            message: format!("{n} of {checked} media downloads don't match"),
        }),
    }
}
