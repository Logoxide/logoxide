//! The script a text is written in, judged from its letters.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dominant_script_is_found() {
        assert_eq!(dominant_script("בְּרֵאשִׁית בָּרָא"), Some("Hebr"));
        assert_eq!(dominant_script("Ἐν ἀρχῇ ἦν ὁ λόγος"), Some("Grek"));
        assert_eq!(dominant_script("In the beginning"), Some("Latn"));
        assert_eq!(
            dominant_script("<tag attribute=\"ignored\">123</tag>"),
            None
        );
    }
}
