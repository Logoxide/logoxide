//! `lx import`: packages made from other publishers' downloads, and updating them

use logoxide::resolve::{Frozen, Update};
use logoxide_app::{
    import::{Outcome, UpdatePlan},
    importer::Importing,
};

use crate::{
    app,
    cli::{Cli, Format},
    cmd::package::{DiffReport, print_skipped},
    print_json, warn,
};

pub(crate) fn import(cli: &Cli, source: &str, vendored: bool) -> logoxide::Result<()> {
    logoxide_app::import::check_source(source)?;
    let library = cli.library();
    let mut project = cli.project(&library)?;
    // The project's catalogs, in order: the first that lists it is the one used
    let imported = app().import(&mut project, source, vendored, &mut warn())?;
    print_skipped(&imported.skipped);
    outln!(
        "imported {} {} ({}) into {}",
        imported.id,
        imported.version,
        imported.name,
        imported.dir.display()
    );
    crate::cmd::project::warn_of_same_works(&project, &library);
    Ok(())
}

/**
`lx import --update`: each of the project's imports (`import` dependencies, and path packages
whose `upstream` a catalog entry's import writes), or the one named, made again from its
publisher's download and compared with the one the project has; changed ones are replaced
(unless `dry_run`: a new version in the library, or a vendored one's files) and locked again. An
entry whose versions follow its updates (eBible.org's dates, SWORD's `Version`) is only
downloaded when the catalog lists a newer version. One that fails is said, and the rest go on;
those replaced are locked again whatever failed, and then it's an error. Files an update removes
from a vendored import are asked about first in a terminal (unless `yes`); elsewhere, `yes` must
allow it
*/
pub(crate) fn import_update(
    cli: &Cli,
    name: Option<&str>,
    dry_run: bool,
    yes: bool,
) -> logoxide::Result<()> {
    let library = cli.library();
    let mut project = cli.project(&library)?;
    let json = cli.format == Format::Json;
    let plans = app().plan_updates(&project, name, &mut warn())?;
    let importing = app().importing(&project);
    let mut changed = std::collections::BTreeSet::new();
    let (mut reports, mut failed) = (vec![], vec![]);
    for plan in plans {
        let mut report = ImportReport {
            dependency: plan.dependency.clone(),
            import: plan.import.clone(),
            status: "current",
            from: plan.from.clone(),
            to: None,
            upstream_from: plan.upstream_from.clone(),
            upstream_to: None,
            diff: None,
            removed: vec![],
            error: None,
        };
        let dep = plan.dependency.clone();
        let options = UpdateOptions { dry_run, yes, json };
        match update_import(plan, &mut report, options, &importing) {
            Ok(()) if report.status == "updated" => {
                changed.insert(dep);
            }
            Ok(()) => {}
            Err(error) => {
                errln!("lx: {dep}: {error}");
                report.status = "failed";
                report.error = Some(error.to_string());
                failed.push(dep);
            }
        }
        reports.push(report);
    }
    if reports.is_empty() && !json {
        outln!("this project has no imports");
    }
    // Those replaced are locked again, whatever else failed
    let locked = match changed.is_empty() {
        true => Ok(()),
        false => {
            let update = Update::Only(changed);
            let locked = app().install_with(&mut project, &importing, update, Frozen::default());
            importing.warnings().into_iter().for_each(&mut warn());
            locked.map(|_| ())
        }
    };
    if json {
        print_json(&reports)?;
    }
    locked?;
    match failed.len() {
        0 => Ok(()),
        n => Err(logoxide::LogoxideError::Import(format!(
            "{n} import{} not updated: {}",
            if n == 1 { "" } else { "s" },
            failed.join(", ")
        ))),
    }
}

/// What `lx import --update` did with one import, for `--format json`
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportReport {
    /// What the manifest calls it
    dependency: String,
    /// `ebible:eng-asv`
    import: String,
    /// `current` (nothing to do), `changed` (with --dry-run), `updated`, `declined` (its removals
    /// weren't allowed), or `failed`
    status: &'static str,
    /// The package's versions (logoxide's, raised by what the update changes)
    from: String,
    to: Option<String>,
    /// The publisher's versions (`[package.upstream] version`), as given
    upstream_from: Option<String>,
    upstream_to: Option<String>,
    diff: Option<DiffReport>,
    /// The files the update removes (those the fresh import doesn't have)
    removed: Vec<String>,
    error: Option<String>,
}

/// How `lx import --update` goes: only saying what would change, removing files without asking,
/// printing JSON
#[derive(Clone, Copy)]
struct UpdateOptions {
    dry_run: bool,
    yes: bool,
    json: bool,
}

/// One import's plan said and, if it changed, carried out (unless `dry_run`): what happened is
/// in `report`
fn update_import(
    plan: UpdatePlan,
    report: &mut ImportReport,
    options: UpdateOptions,
    importing: &Importing<'_>,
) -> logoxide::Result<()> {
    let UpdateOptions { dry_run, yes, json } = options;
    let UpdatePlan {
        dependency: dep,
        import: import_name,
        dir,
        in_library,
        from,
        outcome,
        ..
    } = plan;
    let say = |text: String| {
        if !json {
            outln!("{text}");
        }
    };
    let change = match outcome {
        Outcome::Current => {
            say(format!("{dep} {from}: up to date ({import_name})"));
            return Ok(());
        }
        Outcome::Failed(error) => return Err(error),
        Outcome::Changed(change) => change,
    };
    let update = &change.update;
    let publisher = match (&update.upstream_from, &update.upstream_to) {
        (Some(old), Some(new)) if old != new => format!("; the publisher's {old} → {new}"),
        (None, Some(new)) => format!("; the publisher's {new}"),
        _ => String::new(),
    };
    say(format!(
        "{dep} {} → {}: {} (a {:?} change{publisher})",
        update.from,
        update.to,
        logoxide::release::describe(&update.diff),
        update.diff.change()
    ));
    let removed = &change.removed;
    report.status = "changed";
    report.to = Some(update.to.to_string());
    report.diff = Some(DiffReport::of(&update.diff));
    report.upstream_to = update.upstream_to.clone();
    report.removed = removed.clone();
    if dry_run {
        if !removed.is_empty() && !in_library {
            say(format!("    would remove {}", removed.join(", ")));
        }
        return Ok(());
    }
    // Kept in the library: the new version goes beside the old, which other projects may lock
    if in_library {
        importing.prepare(&import_name, change.build()?);
        report.status = "updated";
        return Ok(());
    }
    if !removed.is_empty() && !yes {
        use std::io::IsTerminal;
        let what = format!(
            "{dep}'s update removes {} file{} from {}: {}",
            removed.len(),
            if removed.len() == 1 { "" } else { "s" },
            dir.display(),
            removed.join(", ")
        );
        if !std::io::stdin().is_terminal() {
            return Err(logoxide::LogoxideError::Import(format!(
                "{what}; pass --yes to allow that"
            )));
        }
        eprint!("{what}. Remove them? [y/N] ");
        let mut answer = String::new();
        let _ = std::io::stdin().read_line(&mut answer);
        if !matches!(answer.trim(), "y" | "Y" | "yes") {
            say(format!("{dep}: left as it was"));
            report.status = "declined";
            return Ok(());
        }
    }
    logoxide_app::import::apply_update(&change, &dir, true)?;
    report.status = "updated";
    Ok(())
}
