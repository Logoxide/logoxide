//! `lx read` and `lx compare`: passages from the project's Bibles

use logoxide_app::{Style, Text};

use crate::{app, cli::Cli, print_json};

/// A passage's text as JSON: a string, or null when the Bible has none there
fn text_json(text: Option<&Text>) -> serde_json::Value {
    match text {
        None => serde_json::Value::Null,
        Some(text) => serde_json::Value::String(text.text.clone()),
    }
}

/// Prints a passage from one of the project's Bibles
pub(crate) fn read(cli: &Cli, reference: &str, style: Style, json: bool) -> logoxide::Result<()> {
    let project = cli.project(&cli.library())?;
    let read = app().read(&project, reference, &style)?;
    if json {
        return print_json(&serde_json::json!({
            "bible": read.bible,
            "abbreviation": read.abbreviation,
            "reference": read.reference,
            "osis": read.osis,
            "versification": read.versification,
            "text": text_json(read.text.as_ref()),
        }));
    }
    outln!("{} ({})", read.reference, read.abbreviation);
    match &read.text {
        Some(text) => outln!("{}", text.text),
        None => outln!("(no text in {})", read.abbreviation),
    }
    Ok(())
}

/// Prints a passage in several of the project's Bibles, each in its own numbering
pub(crate) fn compare(
    cli: &Cli,
    reference: &str,
    bibles: &[String],
    style: Style,
    json: bool,
) -> logoxide::Result<()> {
    let project = cli.project(&cli.library())?;
    let compared = app().compare(&project, reference, bibles, &style)?;
    if json {
        let all: Vec<serde_json::Value> = (compared.iter())
            .map(|c| {
                serde_json::json!({
                    "bible": c.bible,
                    "abbreviation": c.abbreviation,
                    "reference": c.reference,
                    "versification": c.versification,
                    "mapped": c.mapped,
                    "missing": c.missing,
                    "text": text_json(c.text.as_ref()),
                })
            })
            .collect();
        return print_json(&all);
    }
    for (i, c) in compared.iter().enumerate() {
        if i > 0 {
            outln!();
        }
        match c.reference.is_empty() {
            true => outln!(
                "{}: (no counterpart in {})",
                c.abbreviation,
                c.versification
            ),
            false if c.mapped => outln!(
                "{} ({}, numbered {})",
                c.reference,
                c.abbreviation,
                c.versification
            ),
            false => outln!("{} ({})", c.reference, c.abbreviation),
        }
        if !c.reference.is_empty() {
            match &c.text {
                Some(text) => outln!("{}", text.text),
                None => outln!("(no text in {})", c.abbreviation),
            }
        }
        if !c.missing.is_empty() {
            outln!("not in {}: {}", c.abbreviation, c.missing);
        }
    }
    Ok(())
}
