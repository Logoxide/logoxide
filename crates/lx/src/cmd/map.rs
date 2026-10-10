//! `lx map`: a reference in another numbering

use std::{path::PathBuf, sync::Arc};

use logoxide::{Features, FsStorage, Package};

use crate::cli::Cli;

/// Prints where a reference is in another numbering, and its verses that have no counterpart
pub(crate) fn map(
    cli: &Cli,
    reference: &str,
    from: &str,
    to: &str,
    with: &[PathBuf],
) -> logoxide::Result<()> {
    use logoxide::versification::{VersificationMap, parse_reference};
    let library = cli.library();
    let project = cli.project(&library)?;
    let mut maps = project.versifications(&library)?;
    for dir in with {
        let package = Package::open(Arc::new(FsStorage::new(dir)), Features::default())?;
        maps.add(VersificationMap::from_package(&package)?);
    }
    let passage = parse_reference(reference)?;
    let (from, to) = (
        project.numbering(&library, from)?,
        project.numbering(&library, to)?,
    );
    let counts = from.counts.get(&passage.book);
    let mapped = maps.map(&passage, &from.versification, &to.versification, counts)?;
    match mapped.verses.is_empty() {
        true => outln!("(no counterpart in {})", to.versification),
        false => outln!("{} ({})", mapped.text(), to.versification),
    }
    if !mapped.missing.is_empty() {
        outln!(
            "not in {}: {} ({})",
            to.versification,
            mapped.missing_text(),
            from.versification
        );
    }
    Ok(())
}
