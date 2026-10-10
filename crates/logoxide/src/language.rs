/*!
Languages, scripts, and which way a text runs.

A package's `language` is a BCP 47 tag (`en`, `grc`, `pt-BR`, `sr-Latn`), checked to be
well-formed (RFC 5646's grammar; its subtags aren't looked up). Its canonical form
([`canonical`]) names a language by its shortest ISO 639 code, as BCP 47 asks: eBible.org's
`eng` is `en`, `heb` is `he`, German's `ger` and `deu` are `de`, while `grc` and `hbo`, which have
no two-letter code, stay. A tag that isn't canonical is warned of, not refused. Its `script` is an ISO 15924 code (`Latn`, `Hebr`), said or else the
tag's own script subtag, and its `direction` follows the script unless it's said too: right to
left for the scripts in [`RTL_SCRIPTS`].

Publishers' languages are made into canonical tags with [`normalize_language`] (SWORD's `en_US`
is `en-US`, eBible.org's `eng` is `en`; what isn't a tag is `und`, with the publisher's text kept
to show), and a text whose
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

/**
A language tag in its canonical form: deprecated and extended-language subtags replaced as the
IANA registry says (`iw` is `he`, `zh-yue` is `yue`), the language named by its two-letter ISO
639-1 code when it has one (`eng` and `eng-Latn` are `en` and `en-Latn`, `ger` and `deu` are
`de`; `grc` stays), and each subtag in its case (`en-latn-us` is `en-Latn-US`). A script the
language usually has is kept (`en-Latn` stays): a package's script can come from its tag. What
isn't a tag is given back as it is
*/
pub fn canonical(tag: &str) -> String {
    let tag = tag.trim();
    let Ok(parsed) = language_tags::LanguageTag::parse(tag) else {
        return tag.to_string();
    };
    // More than one extended language can't be made canonical: it keeps its case alone
    let canonical = parsed.canonicalize().unwrap_or_else(|_| parsed.clone());
    let text = canonical.as_str();
    let primary = canonical.primary_language();
    let full = canonical.full_language();
    let language = two_letter_code(primary).unwrap_or(primary);
    // The registry's canonical form leaves out a script the language suppresses (`en-Latn` is
    // `en`), which would lose the package's script: it's put back
    let script = match (canonical.script(), parsed.script()) {
        (None, Some(script)) => format!("-{script}"),
        _ => String::new(),
    };
    // Each is a start of the one before (a grandfathered tag is its own language)
    let extended = full.get(primary.len()..).unwrap_or_default();
    let rest = text.get(full.len()..).unwrap_or_default();
    format!("{language}{extended}{script}{rest}")
}

/// The two-letter ISO 639-1 code of a language its three-letter code names (ISO 639-3, 639-2/T, or
/// 639-2/B, in lowercase: `eng`, `deu`, `ger`), if it has one
fn two_letter_code(code: &str) -> Option<&'static str> {
    (code.len() == 3)
        .then(|| ISO_639_1.iter().find(|(_, t, b)| *t == code || *b == code))
        .flatten()
        .map(|(two, _, _)| *two)
}

/**
The codes a search for a language could use for a tag: the canonical tag and, for its language,
its two-letter and three-letter codes (`en-US` gives `en-US`, `en`, and `eng`), in lowercase
*/
pub fn search_codes(tag: &str) -> Vec<String> {
    let canonical = canonical(tag).to_ascii_lowercase();
    let language = (canonical.split('-').next().unwrap_or_default()).to_string();
    let mut codes = vec![canonical];
    if let Some((two, t, b)) = ISO_639_1.iter().find(|(two, _, _)| *two == language) {
        codes.extend([two, t, b].map(|c| c.to_string()));
    } else {
        codes.push(language);
    }
    codes.sort();
    codes.dedup();
    codes
}

/// Warns of a language tag that isn't canonical, saying what it should be
pub fn canonical_warning(tag: &str) -> Option<String> {
    let canonical = canonical(tag);
    (canonical != tag).then(|| {
        format!(
            "language `{tag}` isn't canonical BCP 47: use `{canonical}` (the shortest ISO 639 \
             code, in its case)"
        )
    })
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
        true => (canonical(&tag), None),
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

/**
The languages with a two-letter ISO 639-1 code: that code, the three-letter code ISO 639-3 and
639-2/T give, and ISO 639-2/B's (the same but for twenty, `ger` for `deu`). Made from SIL's ISO
639-3 code table (`iso-639-3.tab`, its `Part1`, `Part2t`, `Id`, and `Part2b` columns); `sh`
(Serbo-Croatian) has only its ISO 639-3 code, `hbs`
*/
const ISO_639_1: [(&str, &str, &str); 184] = [
    ("aa", "aar", "aar"),
    ("ab", "abk", "abk"),
    ("ae", "ave", "ave"),
    ("af", "afr", "afr"),
    ("ak", "aka", "aka"),
    ("am", "amh", "amh"),
    ("an", "arg", "arg"),
    ("ar", "ara", "ara"),
    ("as", "asm", "asm"),
    ("av", "ava", "ava"),
    ("ay", "aym", "aym"),
    ("az", "aze", "aze"),
    ("ba", "bak", "bak"),
    ("be", "bel", "bel"),
    ("bg", "bul", "bul"),
    ("bi", "bis", "bis"),
    ("bm", "bam", "bam"),
    ("bn", "ben", "ben"),
    ("bo", "bod", "tib"),
    ("br", "bre", "bre"),
    ("bs", "bos", "bos"),
    ("ca", "cat", "cat"),
    ("ce", "che", "che"),
    ("ch", "cha", "cha"),
    ("co", "cos", "cos"),
    ("cr", "cre", "cre"),
    ("cs", "ces", "cze"),
    ("cu", "chu", "chu"),
    ("cv", "chv", "chv"),
    ("cy", "cym", "wel"),
    ("da", "dan", "dan"),
    ("de", "deu", "ger"),
    ("dv", "div", "div"),
    ("dz", "dzo", "dzo"),
    ("ee", "ewe", "ewe"),
    ("el", "ell", "gre"),
    ("en", "eng", "eng"),
    ("eo", "epo", "epo"),
    ("es", "spa", "spa"),
    ("et", "est", "est"),
    ("eu", "eus", "baq"),
    ("fa", "fas", "per"),
    ("ff", "ful", "ful"),
    ("fi", "fin", "fin"),
    ("fj", "fij", "fij"),
    ("fo", "fao", "fao"),
    ("fr", "fra", "fre"),
    ("fy", "fry", "fry"),
    ("ga", "gle", "gle"),
    ("gd", "gla", "gla"),
    ("gl", "glg", "glg"),
    ("gn", "grn", "grn"),
    ("gu", "guj", "guj"),
    ("gv", "glv", "glv"),
    ("ha", "hau", "hau"),
    ("he", "heb", "heb"),
    ("hi", "hin", "hin"),
    ("ho", "hmo", "hmo"),
    ("hr", "hrv", "hrv"),
    ("ht", "hat", "hat"),
    ("hu", "hun", "hun"),
    ("hy", "hye", "arm"),
    ("hz", "her", "her"),
    ("ia", "ina", "ina"),
    ("id", "ind", "ind"),
    ("ie", "ile", "ile"),
    ("ig", "ibo", "ibo"),
    ("ii", "iii", "iii"),
    ("ik", "ipk", "ipk"),
    ("io", "ido", "ido"),
    ("is", "isl", "ice"),
    ("it", "ita", "ita"),
    ("iu", "iku", "iku"),
    ("ja", "jpn", "jpn"),
    ("jv", "jav", "jav"),
    ("ka", "kat", "geo"),
    ("kg", "kon", "kon"),
    ("ki", "kik", "kik"),
    ("kj", "kua", "kua"),
    ("kk", "kaz", "kaz"),
    ("kl", "kal", "kal"),
    ("km", "khm", "khm"),
    ("kn", "kan", "kan"),
    ("ko", "kor", "kor"),
    ("kr", "kau", "kau"),
    ("ks", "kas", "kas"),
    ("ku", "kur", "kur"),
    ("kv", "kom", "kom"),
    ("kw", "cor", "cor"),
    ("ky", "kir", "kir"),
    ("la", "lat", "lat"),
    ("lb", "ltz", "ltz"),
    ("lg", "lug", "lug"),
    ("li", "lim", "lim"),
    ("ln", "lin", "lin"),
    ("lo", "lao", "lao"),
    ("lt", "lit", "lit"),
    ("lu", "lub", "lub"),
    ("lv", "lav", "lav"),
    ("mg", "mlg", "mlg"),
    ("mh", "mah", "mah"),
    ("mi", "mri", "mao"),
    ("mk", "mkd", "mac"),
    ("ml", "mal", "mal"),
    ("mn", "mon", "mon"),
    ("mr", "mar", "mar"),
    ("ms", "msa", "may"),
    ("mt", "mlt", "mlt"),
    ("my", "mya", "bur"),
    ("na", "nau", "nau"),
    ("nb", "nob", "nob"),
    ("nd", "nde", "nde"),
    ("ne", "nep", "nep"),
    ("ng", "ndo", "ndo"),
    ("nl", "nld", "dut"),
    ("nn", "nno", "nno"),
    ("no", "nor", "nor"),
    ("nr", "nbl", "nbl"),
    ("nv", "nav", "nav"),
    ("ny", "nya", "nya"),
    ("oc", "oci", "oci"),
    ("oj", "oji", "oji"),
    ("om", "orm", "orm"),
    ("or", "ori", "ori"),
    ("os", "oss", "oss"),
    ("pa", "pan", "pan"),
    ("pi", "pli", "pli"),
    ("pl", "pol", "pol"),
    ("ps", "pus", "pus"),
    ("pt", "por", "por"),
    ("qu", "que", "que"),
    ("rm", "roh", "roh"),
    ("rn", "run", "run"),
    ("ro", "ron", "rum"),
    ("ru", "rus", "rus"),
    ("rw", "kin", "kin"),
    ("sa", "san", "san"),
    ("sc", "srd", "srd"),
    ("sd", "snd", "snd"),
    ("se", "sme", "sme"),
    ("sg", "sag", "sag"),
    ("sh", "hbs", "hbs"),
    ("si", "sin", "sin"),
    ("sk", "slk", "slo"),
    ("sl", "slv", "slv"),
    ("sm", "smo", "smo"),
    ("sn", "sna", "sna"),
    ("so", "som", "som"),
    ("sq", "sqi", "alb"),
    ("sr", "srp", "srp"),
    ("ss", "ssw", "ssw"),
    ("st", "sot", "sot"),
    ("su", "sun", "sun"),
    ("sv", "swe", "swe"),
    ("sw", "swa", "swa"),
    ("ta", "tam", "tam"),
    ("te", "tel", "tel"),
    ("tg", "tgk", "tgk"),
    ("th", "tha", "tha"),
    ("ti", "tir", "tir"),
    ("tk", "tuk", "tuk"),
    ("tl", "tgl", "tgl"),
    ("tn", "tsn", "tsn"),
    ("to", "ton", "ton"),
    ("tr", "tur", "tur"),
    ("ts", "tso", "tso"),
    ("tt", "tat", "tat"),
    ("tw", "twi", "twi"),
    ("ty", "tah", "tah"),
    ("ug", "uig", "uig"),
    ("uk", "ukr", "ukr"),
    ("ur", "urd", "urd"),
    ("uz", "uzb", "uzb"),
    ("ve", "ven", "ven"),
    ("vi", "vie", "vie"),
    ("vo", "vol", "vol"),
    ("wa", "wln", "wln"),
    ("wo", "wol", "wol"),
    ("xh", "xho", "xho"),
    ("yi", "yid", "yid"),
    ("yo", "yor", "yor"),
    ("za", "zha", "zha"),
    ("zh", "zho", "chi"),
    ("zu", "zul", "zul"),
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
        assert_eq!(normalize_language("eng"), ("en".into(), None));
        assert_eq!(normalize_language("deu_ch"), ("de-CH".into(), None));
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
    fn tags_are_made_canonical() {
        for (given, canonical_tag) in [
            ("eng", "en"),
            ("eng-Latn", "en-Latn"),
            ("en-Latn", "en-Latn"),
            ("ENG-latn-us", "en-Latn-US"),
            ("heb", "he"),
            ("grc", "grc"),
            ("hbo", "hbo"),
            ("ger", "de"),
            ("deu-CH", "de-CH"),
            ("gre", "el"),
            ("ell", "el"),
            ("chi-Hant", "zh-Hant"),
            ("zho-Hans-CN", "zh-Hans-CN"),
            ("hbs", "sh"),
            ("arb", "arb"),
            ("ara", "ar"),
            ("iw", "he"),
            ("zh-yue", "yue"),
            ("pt-br", "pt-BR"),
            ("sr-latn", "sr-Latn"),
            ("und", "und"),
            ("x-mine", "x-mine"),
            ("art-lojban", "jbo"),
            ("i-default", "i-default"),
            ("en_US", "en_US"),
        ] {
            assert_eq!(canonical(given), canonical_tag, "{given}");
        }
        assert_eq!(canonical_warning("en"), None);
        assert!(canonical_warning("eng").is_some_and(|w| w.contains("`en`")));
        assert_eq!(search_codes("en-US"), ["en", "en-us", "eng"]);
        assert_eq!(search_codes("eng"), ["en", "eng"]);
        assert_eq!(search_codes("de"), ["de", "deu", "ger"]);
        assert_eq!(search_codes("grc"), ["grc"]);
    }

    #[test]
    fn the_iso_639_table_is_whole() {
        // ISO 639-1's 183 codes and `sh`, each once, with three-letter codes in lowercase
        let mut twos: Vec<&str> = ISO_639_1.iter().map(|(two, _, _)| *two).collect();
        let mut threes: Vec<&str> = (ISO_639_1.iter()).flat_map(|(_, t, b)| [*t, *b]).collect();
        twos.dedup();
        assert_eq!(twos.len(), 184, "sorted and unique");
        threes.sort();
        threes.dedup();
        assert_eq!(
            threes.len(),
            184 + 20,
            "twenty languages have a 639-2/B code of their own"
        );
        for (two, t, b) in ISO_639_1 {
            assert!(two.len() == 2 && two.chars().all(|c| c.is_ascii_lowercase()));
            for code in [t, b] {
                assert!(code.len() == 3 && code.chars().all(|c| c.is_ascii_lowercase()));
            }
        }
        // Spot checks against ISO 639-2's list
        for (code, two) in [
            ("eng", "en"),
            ("fre", "fr"),
            ("fra", "fr"),
            ("tib", "bo"),
            ("bod", "bo"),
            ("wel", "cy"),
            ("nld", "nl"),
            ("dut", "nl"),
            ("lat", "la"),
            ("hin", "hi"),
            ("swa", "sw"),
            ("zul", "zu"),
            ("aar", "aa"),
        ] {
            assert_eq!(two_letter_code(code), Some(two), "{code}");
        }
        for code in ["grc", "hbo", "syc", "cop", "got", "arc", "und"] {
            assert_eq!(two_letter_code(code), None, "{code}");
        }
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
