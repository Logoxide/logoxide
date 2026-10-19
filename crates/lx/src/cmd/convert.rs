//! `lx convert`: a book's text between USFM, USX, and USJ

use std::{
    io::{IsTerminal, Read, Write},
    path::{Path, PathBuf},
};

use logoxide::{LogoxideError, storage::write_atomic};
use logoxide_text::source::{TextFormat, convert as convert_text};

/// The formats converted, by the names `--to` and `--from` take
pub(crate) const FORMATS: [&str; 3] = ["usfm", "usx", "usj"];

/**
Writes a book file in `to`: a file (`-`: stdin) to `output` (a file, or a folder to write it
in), else to stdout; a folder's book files each into the folder `output`, named as they were with `to`'s
extension. `from` is the format read (else each file's own, from its extension or how it
starts); in a folder, only its files in `from` are converted
*/
pub(crate) fn convert(
    input: &Path,
    to: &str,
    from: Option<&str>,
    output: Option<&Path>,
) -> logoxide::Result<()> {
    let to = format(to)?;
    let from = from.map(format).transpose()?;
    let stdin = input == Path::new("-");
    if stdin || !input.is_dir() {
        let text = match stdin {
            true => {
                let mut text = String::new();
                (std::io::stdin().read_to_string(&mut text))
                    .map_err(|e| LogoxideError::io("stdin", e))?;
                text
            }
            false => read(input)?,
        };
        let from = from.or_else(|| format_of(input, &text)).ok_or_else(|| {
            LogoxideError::io_message(
                &input.display().to_string(),
                "not USFM, USX, or USJ (--from says which it is)",
            )
        })?;
        let converted = converted(input, &text, from, to)?;
        return match output {
            Some(dir) if dir.is_dir() && !stdin => {
                write(&dir.join(file_name(input, to)), &converted)
            }
            Some(file) => write(file, &converted),
            // What's read on a terminal is made safe for it; what's piped is the book as written
            None if std::io::stdout().is_terminal() => {
                out!("{converted}");
                Ok(())
            }
            None => (std::io::stdout().write_all(converted.as_bytes()))
                .map_err(|e| LogoxideError::io("stdout", e)),
        };
    }
    let output = output.ok_or_else(|| {
        LogoxideError::io_message(
            &input.display().to_string(),
            "a folder's books are written into another folder: give it with --output",
        )
    })?;
    let entries = std::fs::read_dir(input).map_err(|e| io(input, e))?;
    let mut paths: Vec<PathBuf> = (entries.flatten())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    paths.sort();
    std::fs::create_dir_all(output).map_err(|e| io(output, e))?;
    let (mut written, mut failed) = (0, 0);
    for path in paths {
        // A file that isn't text, or isn't a book in any of the formats, isn't one to convert
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Some(format) = format_of(&path, &text) else {
            continue;
        };
        if from.is_some_and(|from| from != format) {
            continue;
        }
        let result = converted(&path, &text, format, to)
            .and_then(|text| write(&output.join(file_name(&path, to)), &text));
        match result {
            Ok(()) => written += 1,
            Err(e) => {
                errln!("lx: {e}");
                failed += 1;
            }
        }
    }
    if written + failed == 0 {
        return Err(LogoxideError::io_message(
            &input.display().to_string(),
            "no USFM, USX, or USJ files in it",
        ));
    }
    outln!(
        "{written} {} written in {} into {}",
        match written {
            1 => "book",
            _ => "books",
        },
        to.name().to_ascii_uppercase(),
        output.display()
    );
    match failed {
        0 => Ok(()),
        n => Err(LogoxideError::io_message(
            &input.display().to_string(),
            format!("{n} of its files couldn't be converted"),
        )),
    }
}

fn format(name: &str) -> logoxide::Result<TextFormat> {
    (TextFormat::from_name(name))
        .filter(|f| FORMATS.contains(&f.name()))
        .ok_or_else(|| LogoxideError::Manifest(format!("{name}: not usfm, usx, or usj")))
}

/// A file's format, from its extension (`.usfm`, `.sfm`, `.usx`, `.usj`), else from how it
/// starts: `\id` is USFM, `<usx` USX, and `{"type": "USJ"` USJ (a `.json` or `.xml` file)
fn format_of(path: &Path, text: &str) -> Option<TextFormat> {
    let extension = (path.extension())
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "usfm" | "sfm" => return Some(TextFormat::Usfm),
        "usx" => return Some(TextFormat::Usx),
        "usj" => return Some(TextFormat::Usj),
        _ => {}
    }
    let head: String = (text.trim_start_matches('\u{feff}').trim_start().chars())
        .take(4096)
        .collect();
    let compact: String = head.split_whitespace().collect::<String>().to_lowercase();
    match head.chars().next() {
        Some('\\') if head.starts_with("\\id") => Some(TextFormat::Usfm),
        Some('<') if compact.contains("<usx") => Some(TextFormat::Usx),
        Some('{') if compact.contains("\"type\":\"usj\"") => Some(TextFormat::Usj),
        _ => None,
    }
}

/// The book in `path`, read as `from`, written as `to` (an error names the file)
fn converted(
    path: &Path,
    text: &str,
    from: TextFormat,
    to: TextFormat,
) -> logoxide::Result<String> {
    convert_text(text, from, to).map_err(|e| match LogoxideError::from(e) {
        LogoxideError::Book { message, .. } => LogoxideError::Book {
            book: path.display().to_string(),
            message,
        },
        e => e,
    })
}

/// Its name with `to`'s extension (USJ's is `.json`, as a package's `usj/` folder has them)
fn file_name(path: &Path, to: TextFormat) -> PathBuf {
    let extension = match to {
        TextFormat::Usj => "json",
        to => to.name(),
    };
    let stem = path.file_stem().unwrap_or(path.as_os_str());
    PathBuf::from(stem).with_extension(extension)
}

fn read(path: &Path) -> logoxide::Result<String> {
    std::fs::read_to_string(path).map_err(|e| io(path, e))
}

fn write(path: &Path, text: &str) -> logoxide::Result<()> {
    write_atomic(path, text.as_bytes()).map_err(|e| io(path, e))
}

fn io(path: &Path, e: std::io::Error) -> LogoxideError {
    LogoxideError::io(&path.display().to_string(), e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_from_names_and_starts() {
        let of = |name: &str, text: &str| format_of(Path::new(name), text);
        assert_eq!(of("JHN.SFM", ""), Some(TextFormat::Usfm));
        assert_eq!(of("JHN.usj", ""), Some(TextFormat::Usj));
        assert_eq!(of("JHN.txt", "\u{feff}\\id JHN"), Some(TextFormat::Usfm));
        assert_eq!(
            of("JHN.xml", "<?xml?>\n<usx version=\"3.0\">"),
            Some(TextFormat::Usx)
        );
        assert_eq!(
            of("JHN.json", "{\n  \"type\": \"USJ\""),
            Some(TextFormat::Usj)
        );
        assert_eq!(of("data.json", "{\"type\": \"other\"}"), None);
        assert_eq!(of("notes.txt", "Notes"), None);
        assert_eq!(
            file_name(Path::new("a/43-JHN.usfm"), TextFormat::Usj),
            PathBuf::from("43-JHN.json")
        );
        assert!(format("osis").is_err());
    }
}
