/*!
Languages, scripts, and which way a text runs.

A package's `language` is a BCP 47 tag (`en`, `grc`, `pt-BR`, `sr-Latn`), checked to be
well-formed (RFC 5646's grammar; its subtags aren't looked up, so eBible.org's ISO 639-3 codes,
`eng`, stay as they are). Its `script` is an ISO 15924 code (`Latn`, `Hebr`), said or else the
tag's own script subtag, and its `direction` follows the script unless it's said too: right to
left for the scripts in [`RTL_SCRIPTS`].

Publishers' languages are made into tags with [`normalize_language`] (SWORD's `en_US` is
`en-US`; what isn't a tag is `und`, with the publisher's text kept to show), and a text whose
script nobody says is judged by its letters ([`dominant_script`]).
*/

use crate::{
    error::{LogoxideError, Result},
    manifest::Direction,
};

/// Checks a language tag: well-formed BCP 47 (`en`, `eng`, `pt-BR`, `zh-Hant-TW`, `und`)
pub fn check_language(tag: &str) -> Result<()> {
    match language_tags::LanguageTag::parse(tag) {
        Ok(_) => Ok(()),
        Err(e) => Err(LogoxideError::Manifest(format!(
            "language `{tag}` isn't a BCP 47 tag ({e}): `en`, `grc`, `pt-BR`, `und` if unknown"
        ))),
    }
}

/// Checks a script code: ISO 15924's form, four letters with the first a capital (`Latn`, `Hebr`)
pub fn check_script(code: &str) -> Result<()> {
    let mut chars = code.chars();
    let ok = code.len() == 4
        && chars.next().is_some_and(|c| c.is_ascii_uppercase())
        && chars.all(|c| c.is_ascii_lowercase());
    match ok {
        true => Ok(()),
        false => Err(LogoxideError::Manifest(format!(
            "script `{code}` isn't an ISO 15924 code: four letters, the first a capital \
             (`Latn`, `Hebr`, `Grek`)"
        ))),
    }
}

/// The script a language tag names (`sr-Latn` is `Latn`, written as ISO 15924 does), if it names
/// one
pub fn script_of_tag(tag: &str) -> Option<String> {
    let tag = language_tags::LanguageTag::parse(tag).ok()?;
    tag.script().map(title_case)
}

/// `latn` as ISO 15924 writes it, `Latn`
fn title_case(code: &str) -> String {
    let mut chars = code.chars();
    (chars.next().map(|c| c.to_ascii_uppercase()).into_iter())
        .chain(chars.map(|c| c.to_ascii_lowercase()))
        .collect()
}

/// The scripts written right to left (Unicode's scripts whose letters are of bidirectional class
/// R or AL), by ISO 15924 code
pub const RTL_SCRIPTS: [&str; 35] = [
    "Adlm", "Arab", "Armi", "Avst", "Chrs", "Cprt", "Elym", "Hatr", "Hebr", "Hung", "Khar", "Lydi",
    "Mand", "Mani", "Mend", "Merc", "Mero", "Narb", "Nbat", "Nkoo", "Orkh", "Ougr", "Palm", "Phli",
    "Phlp", "Phnx", "Prti", "Rohg", "Samr", "Sarb", "Sogd", "Sogo", "Syrc", "Thaa", "Yezi",
];

/// Which way a script runs: right to left for [`RTL_SCRIPTS`], else left to right (and without one)
pub fn script_direction(script: Option<&str>) -> Direction {
    match script.is_some_and(|s| RTL_SCRIPTS.contains(&s)) {
        true => Direction::Rtl,
        false => Direction::Ltr,
    }
}

/**
A publisher's language as a BCP 47 tag, and the publisher's own text when it isn't one: `_` read
as `-` (SWORD's `en_US` is `en-US`), spaces trimmed. Anything else (`English`, an empty value) is
`und` ("undetermined"), with what was given kept to show
*/
pub fn normalize_language(raw: &str) -> (String, Option<String>) {
    let given = raw.trim();
    let tag = given.replace('_', "-");
    // A tag of letters and digits only: a name with spaces or other marks isn't one, even where
    // the grammar would take it
    let plain = !tag.is_empty() && tag.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    match plain && language_tags::LanguageTag::parse(&tag).is_ok() {
        true => (tag, None),
        false => (
            String::from("und"),
            (!given.is_empty()).then(|| given.to_string()),
        ),
    }
}

/**
The script most of a text's letters are in (outside markup's tags), by ISO 15924 code, if any
letters are in the scripts this knows: Latin, Greek, Cyrillic, Armenian, Hebrew, Arabic, Syriac,
Thaana, Devanagari, Bengali, Tamil, Thai, Georgian, Ethiopic, Hangul, Han, Hiragana, Katakana,
Coptic, Gothic. Only the first 200,000 characters are read
*/
pub fn dominant_script(text: &str) -> Option<&'static str> {
    let mut counts: std::collections::BTreeMap<&'static str, usize> = Default::default();
    let mut in_tag = false;
    for c in text.chars().take(200_000) {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if in_tag || !c.is_alphabetic() => {}
            _ => {
                if let Some(script) = script_of_char(c) {
                    *counts.entry(script).or_default() += 1;
                }
            }
        }
    }
    (counts.into_iter())
        .max_by_key(|(_, n)| *n)
        .map(|(script, _)| script)
}

/// The script a letter is in, for the scripts [`dominant_script`] knows
fn script_of_char(c: char) -> Option<&'static str> {
    Some(match c {
        'A'..='Z' | 'a'..='z' | '\u{00c0}'..='\u{024f}' | '\u{1e00}'..='\u{1eff}' => "Latn",
        '\u{0370}'..='\u{03e1}' | '\u{03f0}'..='\u{03ff}' | '\u{1f00}'..='\u{1fff}' => "Grek",
        '\u{03e2}'..='\u{03ef}' | '\u{2c80}'..='\u{2cff}' => "Copt",
        '\u{0400}'..='\u{052f}' => "Cyrl",
        '\u{0530}'..='\u{058f}' => "Armn",
        '\u{0590}'..='\u{05ff}' | '\u{fb1d}'..='\u{fb4f}' => "Hebr",
        '\u{0600}'..='\u{06ff}'
        | '\u{0750}'..='\u{077f}'
        | '\u{08a0}'..='\u{08ff}'
        | '\u{fb50}'..='\u{fdff}'
        | '\u{fe70}'..='\u{feff}' => "Arab",
        '\u{0700}'..='\u{074f}' => "Syrc",
        '\u{0780}'..='\u{07bf}' => "Thaa",
        '\u{0900}'..='\u{097f}' => "Deva",
        '\u{0980}'..='\u{09ff}' => "Beng",
        '\u{0b80}'..='\u{0bff}' => "Taml",
        '\u{0e00}'..='\u{0e7f}' => "Thai",
        '\u{10a0}'..='\u{10ff}' => "Geor",
        '\u{1200}'..='\u{139f}' => "Ethi",
        '\u{1100}'..='\u{11ff}' | '\u{ac00}'..='\u{d7af}' => "Hang",
        '\u{3040}'..='\u{309f}' => "Hira",
        '\u{30a0}'..='\u{30ff}' => "Kana",
        '\u{4e00}'..='\u{9fff}' | '\u{3400}'..='\u{4dbf}' => "Hani",
        '\u{10330}'..='\u{1034f}' => "Goth",
        _ => return None,
    })
}

/**
A script's ISO 15924 code from its English name, as eBible.org's list names them (`Latin`,
`Hebrew`, `Cyrillic`), or from a code in any case (`latn`); none for a name this doesn't know
*/
pub fn script_code(name: &str) -> Option<String> {
    let name = name.trim();
    if name.len() == 4 && name.chars().all(|c| c.is_ascii_alphabetic()) {
        let code = title_case(name);
        if KNOWN_SCRIPTS.iter().any(|(_, c)| *c == code) || RTL_SCRIPTS.contains(&code.as_str()) {
            return Some(code);
        }
    }
    let lower = name.to_lowercase();
    (KNOWN_SCRIPTS.iter())
        .find(|(n, _)| *n == lower)
        .map(|(_, code)| code.to_string())
}

/// Scripts by English name (lowercase), as publishers' lists name them
const KNOWN_SCRIPTS: [(&str, &str); 40] = [
    ("latin", "Latn"),
    ("greek", "Grek"),
    ("cyrillic", "Cyrl"),
    ("armenian", "Armn"),
    ("hebrew", "Hebr"),
    ("arabic", "Arab"),
    ("syriac", "Syrc"),
    ("thaana", "Thaa"),
    ("devanagari", "Deva"),
    ("bengali", "Beng"),
    ("bangla", "Beng"),
    ("gurmukhi", "Guru"),
    ("gujarati", "Gujr"),
    ("oriya", "Orya"),
    ("odia", "Orya"),
    ("tamil", "Taml"),
    ("telugu", "Telu"),
    ("kannada", "Knda"),
    ("malayalam", "Mlym"),
    ("sinhala", "Sinh"),
    ("thai", "Thai"),
    ("lao", "Laoo"),
    ("tibetan", "Tibt"),
    ("myanmar", "Mymr"),
    ("burmese", "Mymr"),
    ("georgian", "Geor"),
    ("hangul", "Hang"),
    ("ethiopic", "Ethi"),
    ("khmer", "Khmr"),
    ("mongolian", "Mong"),
    ("han", "Hani"),
    ("chinese", "Hani"),
    ("simplified chinese", "Hans"),
    ("traditional chinese", "Hant"),
    ("japanese", "Jpan"),
    ("korean", "Kore"),
    ("cherokee", "Cher"),
    ("canadian aboriginal syllabics", "Cans"),
    ("coptic", "Copt"),
    ("gothic", "Goth"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_are_bcp_47() {
        for good in [
            "en",
            "eng",
            "grc",
            "pt-BR",
            "zh-Hant-TW",
            "sr-Latn",
            "und",
            "x-mine",
        ] {
            assert!(check_language(good).is_ok(), "{good}");
        }
        for bad in ["", "en_US", "en-", "e", "en US"] {
            assert!(check_language(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn scripts_and_directions() {
        assert!(check_script("Latn").is_ok());
        for bad in ["latn", "LATN", "Lat", "Lat1"] {
            assert!(check_script(bad).is_err(), "{bad}");
        }
        assert_eq!(script_of_tag("sr-latn").as_deref(), Some("Latn"));
        assert_eq!(script_of_tag("he"), None);
        assert_eq!(script_direction(Some("Hebr")), Direction::Rtl);
        assert_eq!(script_direction(Some("Syrc")), Direction::Rtl);
        assert_eq!(script_direction(Some("Grek")), Direction::Ltr);
        assert_eq!(script_direction(None), Direction::Ltr);
        assert_eq!(script_code("Latin").as_deref(), Some("Latn"));
        assert_eq!(script_code("hebr").as_deref(), Some("Hebr"));
        assert_eq!(script_code("Klingon"), None);
    }

    #[test]
    fn publishers_languages_become_tags() {
        assert_eq!(normalize_language("en_US"), ("en-US".into(), None));
        assert_eq!(normalize_language(" grc "), ("grc".into(), None));
        assert_eq!(
            normalize_language("Old English"),
            ("und".into(), Some("Old English".into()))
        );
        assert_eq!(
            normalize_language("en__x"),
            ("und".into(), Some("en__x".into()))
        );
        assert_eq!(normalize_language(""), ("und".into(), None));
    }

    #[test]
    fn the_dominant_script_is_found() {
        let hebrew = "<p class=\"x\">בְּרֵאשִׁית בָּרָא אֱלֹהִים</p> (Gen 1:1)";
        assert_eq!(dominant_script(hebrew), Some("Hebr"));
        assert_eq!(dominant_script("Ἐν ἀρχῇ ἦν ὁ λόγος"), Some("Grek"));
        assert_eq!(dominant_script("In the beginning"), Some("Latn"));
        assert_eq!(
            dominant_script("<tag attribute=\"ignored\">123</tag>"),
            None
        );
    }
}
