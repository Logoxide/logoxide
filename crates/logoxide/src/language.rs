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
script nobody says is judged by its letters (logoxide-text's `dominant_script`).
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
        .then(|| {
            ISO_639_1
                .iter()
                .find(|(_, t, b, _)| *t == code || *b == code)
        })
        .flatten()
        .map(|(two, _, _, _)| *two)
}

/**
The codes a search for a language could use for a tag: the canonical tag and, for its language,
its two-letter and three-letter codes (`en-US` gives `en-US`, `en`, and `eng`), in lowercase
*/
pub fn search_codes(tag: &str) -> Vec<String> {
    let canonical = canonical(tag).to_ascii_lowercase();
    let language = (canonical.split('-').next().unwrap_or_default()).to_string();
    let mut codes = vec![canonical];
    if let Some((two, t, b, _)) = ISO_639_1.iter().find(|(two, _, _, _)| *two == language) {
        codes.extend([two, t, b].map(|c| c.to_string()));
    } else {
        codes.push(language);
    }
    codes.sort();
    codes.dedup();
    codes
}

/**
Whether `tag` is in the language `range` names, as RFC 4647's basic filtering has it, both made
canonical first: a range matches a tag that is it or starts with it and a `-`, ignoring case
(`en` matches `en`, `en-US`, and `en-Latn`; `pt-BR` matches `pt-BR` but not `pt`; `eng` is `en`,
so it matches `en-GB`), and `*` matches every tag
*/
pub fn matches_range(range: &str, tag: &str) -> bool {
    let range = canonical(range).to_ascii_lowercase();
    let tag = canonical(tag).to_ascii_lowercase();
    range == "*"
        || (tag.strip_prefix(&range)).is_some_and(|rest| rest.is_empty() || rest.starts_with('-'))
}

/**
The language a tag names, without its script, region, or variants: the canonical form's first
subtag in lowercase (`en-US` and `eng` are `en`, `sr-Latn` is `sr`, `grc` is `grc`), which
groups texts by language
*/
pub fn primary_language(tag: &str) -> String {
    let canonical = canonical(tag).to_ascii_lowercase();
    (canonical.split('-').next().unwrap_or_default()).to_string()
}

/**
A language's English name by its tag (`en-US` and `eng` are `English`, `ko` is `Korean`), for the
languages with a two-letter code; none for others (`grc`, `und`)
*/
pub fn language_name(tag: &str) -> Option<&'static str> {
    let language = primary_language(tag);
    (ISO_639_1.iter())
        .find(|(two, _, _, _)| *two == language)
        .map(|(_, _, _, name)| *name)
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
(Serbo-Croatian) has only its ISO 639-3 code, `hbs`. Last, the language's English name, the
table's `Ref_Name` without its qualifiers (`Swahili`, not `Swahili (macrolanguage)`; `Modern
Greek`, `Tongan`, `Ossetian`)
*/
const ISO_639_1: [(&str, &str, &str, &str); 184] = [
    ("aa", "aar", "aar", "Afar"),
    ("ab", "abk", "abk", "Abkhazian"),
    ("ae", "ave", "ave", "Avestan"),
    ("af", "afr", "afr", "Afrikaans"),
    ("ak", "aka", "aka", "Akan"),
    ("am", "amh", "amh", "Amharic"),
    ("an", "arg", "arg", "Aragonese"),
    ("ar", "ara", "ara", "Arabic"),
    ("as", "asm", "asm", "Assamese"),
    ("av", "ava", "ava", "Avaric"),
    ("ay", "aym", "aym", "Aymara"),
    ("az", "aze", "aze", "Azerbaijani"),
    ("ba", "bak", "bak", "Bashkir"),
    ("be", "bel", "bel", "Belarusian"),
    ("bg", "bul", "bul", "Bulgarian"),
    ("bi", "bis", "bis", "Bislama"),
    ("bm", "bam", "bam", "Bambara"),
    ("bn", "ben", "ben", "Bengali"),
    ("bo", "bod", "tib", "Tibetan"),
    ("br", "bre", "bre", "Breton"),
    ("bs", "bos", "bos", "Bosnian"),
    ("ca", "cat", "cat", "Catalan"),
    ("ce", "che", "che", "Chechen"),
    ("ch", "cha", "cha", "Chamorro"),
    ("co", "cos", "cos", "Corsican"),
    ("cr", "cre", "cre", "Cree"),
    ("cs", "ces", "cze", "Czech"),
    ("cu", "chu", "chu", "Church Slavic"),
    ("cv", "chv", "chv", "Chuvash"),
    ("cy", "cym", "wel", "Welsh"),
    ("da", "dan", "dan", "Danish"),
    ("de", "deu", "ger", "German"),
    ("dv", "div", "div", "Dhivehi"),
    ("dz", "dzo", "dzo", "Dzongkha"),
    ("ee", "ewe", "ewe", "Ewe"),
    ("el", "ell", "gre", "Modern Greek"),
    ("en", "eng", "eng", "English"),
    ("eo", "epo", "epo", "Esperanto"),
    ("es", "spa", "spa", "Spanish"),
    ("et", "est", "est", "Estonian"),
    ("eu", "eus", "baq", "Basque"),
    ("fa", "fas", "per", "Persian"),
    ("ff", "ful", "ful", "Fulah"),
    ("fi", "fin", "fin", "Finnish"),
    ("fj", "fij", "fij", "Fijian"),
    ("fo", "fao", "fao", "Faroese"),
    ("fr", "fra", "fre", "French"),
    ("fy", "fry", "fry", "Western Frisian"),
    ("ga", "gle", "gle", "Irish"),
    ("gd", "gla", "gla", "Scottish Gaelic"),
    ("gl", "glg", "glg", "Galician"),
    ("gn", "grn", "grn", "Guarani"),
    ("gu", "guj", "guj", "Gujarati"),
    ("gv", "glv", "glv", "Manx"),
    ("ha", "hau", "hau", "Hausa"),
    ("he", "heb", "heb", "Hebrew"),
    ("hi", "hin", "hin", "Hindi"),
    ("ho", "hmo", "hmo", "Hiri Motu"),
    ("hr", "hrv", "hrv", "Croatian"),
    ("ht", "hat", "hat", "Haitian"),
    ("hu", "hun", "hun", "Hungarian"),
    ("hy", "hye", "arm", "Armenian"),
    ("hz", "her", "her", "Herero"),
    ("ia", "ina", "ina", "Interlingua"),
    ("id", "ind", "ind", "Indonesian"),
    ("ie", "ile", "ile", "Interlingue"),
    ("ig", "ibo", "ibo", "Igbo"),
    ("ii", "iii", "iii", "Sichuan Yi"),
    ("ik", "ipk", "ipk", "Inupiaq"),
    ("io", "ido", "ido", "Ido"),
    ("is", "isl", "ice", "Icelandic"),
    ("it", "ita", "ita", "Italian"),
    ("iu", "iku", "iku", "Inuktitut"),
    ("ja", "jpn", "jpn", "Japanese"),
    ("jv", "jav", "jav", "Javanese"),
    ("ka", "kat", "geo", "Georgian"),
    ("kg", "kon", "kon", "Kongo"),
    ("ki", "kik", "kik", "Kikuyu"),
    ("kj", "kua", "kua", "Kuanyama"),
    ("kk", "kaz", "kaz", "Kazakh"),
    ("kl", "kal", "kal", "Kalaallisut"),
    ("km", "khm", "khm", "Khmer"),
    ("kn", "kan", "kan", "Kannada"),
    ("ko", "kor", "kor", "Korean"),
    ("kr", "kau", "kau", "Kanuri"),
    ("ks", "kas", "kas", "Kashmiri"),
    ("ku", "kur", "kur", "Kurdish"),
    ("kv", "kom", "kom", "Komi"),
    ("kw", "cor", "cor", "Cornish"),
    ("ky", "kir", "kir", "Kirghiz"),
    ("la", "lat", "lat", "Latin"),
    ("lb", "ltz", "ltz", "Luxembourgish"),
    ("lg", "lug", "lug", "Ganda"),
    ("li", "lim", "lim", "Limburgan"),
    ("ln", "lin", "lin", "Lingala"),
    ("lo", "lao", "lao", "Lao"),
    ("lt", "lit", "lit", "Lithuanian"),
    ("lu", "lub", "lub", "Luba-Katanga"),
    ("lv", "lav", "lav", "Latvian"),
    ("mg", "mlg", "mlg", "Malagasy"),
    ("mh", "mah", "mah", "Marshallese"),
    ("mi", "mri", "mao", "Maori"),
    ("mk", "mkd", "mac", "Macedonian"),
    ("ml", "mal", "mal", "Malayalam"),
    ("mn", "mon", "mon", "Mongolian"),
    ("mr", "mar", "mar", "Marathi"),
    ("ms", "msa", "may", "Malay"),
    ("mt", "mlt", "mlt", "Maltese"),
    ("my", "mya", "bur", "Burmese"),
    ("na", "nau", "nau", "Nauru"),
    ("nb", "nob", "nob", "Norwegian Bokmål"),
    ("nd", "nde", "nde", "North Ndebele"),
    ("ne", "nep", "nep", "Nepali"),
    ("ng", "ndo", "ndo", "Ndonga"),
    ("nl", "nld", "dut", "Dutch"),
    ("nn", "nno", "nno", "Norwegian Nynorsk"),
    ("no", "nor", "nor", "Norwegian"),
    ("nr", "nbl", "nbl", "South Ndebele"),
    ("nv", "nav", "nav", "Navajo"),
    ("ny", "nya", "nya", "Chichewa"),
    ("oc", "oci", "oci", "Occitan"),
    ("oj", "oji", "oji", "Ojibwa"),
    ("om", "orm", "orm", "Oromo"),
    ("or", "ori", "ori", "Oriya"),
    ("os", "oss", "oss", "Ossetian"),
    ("pa", "pan", "pan", "Panjabi"),
    ("pi", "pli", "pli", "Pali"),
    ("pl", "pol", "pol", "Polish"),
    ("ps", "pus", "pus", "Pushto"),
    ("pt", "por", "por", "Portuguese"),
    ("qu", "que", "que", "Quechua"),
    ("rm", "roh", "roh", "Romansh"),
    ("rn", "run", "run", "Rundi"),
    ("ro", "ron", "rum", "Romanian"),
    ("ru", "rus", "rus", "Russian"),
    ("rw", "kin", "kin", "Kinyarwanda"),
    ("sa", "san", "san", "Sanskrit"),
    ("sc", "srd", "srd", "Sardinian"),
    ("sd", "snd", "snd", "Sindhi"),
    ("se", "sme", "sme", "Northern Sami"),
    ("sg", "sag", "sag", "Sango"),
    ("sh", "hbs", "hbs", "Serbo-Croatian"),
    ("si", "sin", "sin", "Sinhala"),
    ("sk", "slk", "slo", "Slovak"),
    ("sl", "slv", "slv", "Slovenian"),
    ("sm", "smo", "smo", "Samoan"),
    ("sn", "sna", "sna", "Shona"),
    ("so", "som", "som", "Somali"),
    ("sq", "sqi", "alb", "Albanian"),
    ("sr", "srp", "srp", "Serbian"),
    ("ss", "ssw", "ssw", "Swati"),
    ("st", "sot", "sot", "Southern Sotho"),
    ("su", "sun", "sun", "Sundanese"),
    ("sv", "swe", "swe", "Swedish"),
    ("sw", "swa", "swa", "Swahili"),
    ("ta", "tam", "tam", "Tamil"),
    ("te", "tel", "tel", "Telugu"),
    ("tg", "tgk", "tgk", "Tajik"),
    ("th", "tha", "tha", "Thai"),
    ("ti", "tir", "tir", "Tigrinya"),
    ("tk", "tuk", "tuk", "Turkmen"),
    ("tl", "tgl", "tgl", "Tagalog"),
    ("tn", "tsn", "tsn", "Tswana"),
    ("to", "ton", "ton", "Tongan"),
    ("tr", "tur", "tur", "Turkish"),
    ("ts", "tso", "tso", "Tsonga"),
    ("tt", "tat", "tat", "Tatar"),
    ("tw", "twi", "twi", "Twi"),
    ("ty", "tah", "tah", "Tahitian"),
    ("ug", "uig", "uig", "Uighur"),
    ("uk", "ukr", "ukr", "Ukrainian"),
    ("ur", "urd", "urd", "Urdu"),
    ("uz", "uzb", "uzb", "Uzbek"),
    ("ve", "ven", "ven", "Venda"),
    ("vi", "vie", "vie", "Vietnamese"),
    ("vo", "vol", "vol", "Volapük"),
    ("wa", "wln", "wln", "Walloon"),
    ("wo", "wol", "wol", "Wolof"),
    ("xh", "xho", "xho", "Xhosa"),
    ("yi", "yid", "yid", "Yiddish"),
    ("yo", "yor", "yor", "Yoruba"),
    ("za", "zha", "zha", "Zhuang"),
    ("zh", "zho", "chi", "Chinese"),
    ("zu", "zul", "zul", "Zulu"),
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
        let mut twos: Vec<&str> = ISO_639_1.iter().map(|(two, _, _, _)| *two).collect();
        let mut threes: Vec<&str> = (ISO_639_1.iter())
            .flat_map(|(_, t, b, _)| [*t, *b])
            .collect();
        twos.dedup();
        assert_eq!(twos.len(), 184, "sorted and unique");
        threes.sort();
        threes.dedup();
        assert_eq!(
            threes.len(),
            184 + 20,
            "twenty languages have a 639-2/B code of their own"
        );
        for (two, t, b, name) in ISO_639_1 {
            assert!(!name.is_empty() && !name.contains('('), "{name}");
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
    fn ranges_match_a_language_and_its_subtags() {
        for (range, tag) in [
            ("en", "en"),
            ("en", "en-US"),
            ("EN", "en-Latn-GB"),
            ("eng", "en-GB"),
            ("en", "eng"),
            ("pt-BR", "pt-BR"),
            ("de", "ger"),
            ("*", "grc"),
        ] {
            assert!(matches_range(range, tag), "{range} {tag}");
        }
        for (range, tag) in [
            ("en", "enm"),
            ("pt-BR", "pt"),
            ("pt-BR", "pt-PT"),
            ("sr-Latn", "sr"),
        ] {
            assert!(!matches_range(range, tag), "{range} {tag}");
        }
        assert_eq!(primary_language("en-US"), "en");
        assert_eq!(primary_language("eng"), "en");
        assert_eq!(primary_language("sr-Latn"), "sr");
        assert_eq!(primary_language("grc"), "grc");
        assert_eq!(language_name("en-US"), Some("English"));
        assert_eq!(language_name("kor"), Some("Korean"));
        assert_eq!(language_name("sw"), Some("Swahili"));
        assert_eq!(language_name("grc"), None);
        assert_eq!(language_name("und"), None);
    }
}
