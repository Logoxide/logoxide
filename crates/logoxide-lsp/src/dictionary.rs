/*!
Strong's dictionaries for hover (the `strongs` setting): the project's dictionaries keyed by
Strong's numbers (`[package] keys = "strongs-greek"` or `"strongs-hebrew"`), each opened once
and kept (a dictionary indexes its files when it opens, and parses an entry's body when it's
looked up), and the Strong's number under the cursor (`G25`, `H430`, `G0025`).
*/

use logoxide::{Package, manifest::Keys};
use logoxide_text::{Entry, keys::normalize_key};

use crate::{
    bible::{Text, nodes_text},
    settings::MarkupSettings,
};

/// A dictionary of Strong's Greek or Hebrew numbers
pub struct Dictionary {
    /// Its name in hover: the package's abbreviation, or the project's name for it
    pub name: String,
    /// Which numbers it has
    pub keys: Keys,
    package: Package,
    markup: MarkupSettings,
}

impl Dictionary {
    /// A dictionary package, if its keys are Strong's numbers
    pub fn new(name: String, package: Package, markup: MarkupSettings) -> Option<Dictionary> {
        let keys = package
            .info()
            .keys
            .filter(|k| matches!(k, Keys::StrongsGreek | Keys::StrongsHebrew))?;
        Some(Dictionary {
            name,
            keys,
            package,
            markup,
        })
    }

    /// Whether it has numbers of this kind (`G25` is Greek, `H430` Hebrew)
    pub fn covers(&self, key: &str) -> bool {
        match self.keys {
            Keys::StrongsGreek => key.starts_with('G'),
            Keys::StrongsHebrew => key.starts_with('H'),
            Keys::Headword => false,
        }
    }

    /// The entry for a Strong's number in normal form, with its text cut after `limit` bytes;
    /// none when the dictionary lacks it (or it can't be read)
    pub fn entry(&self, key: &str, limit: Option<usize>) -> Option<(Entry, Text)> {
        let entry = self.package.source().entry(key).ok()??;
        let text = nodes_text(&entry.nodes, &self.markup, limit)?;
        Some((entry, text))
    }
}

/**
The Strong's number at a byte of a text, in normal form (`G0025` is `G25`), and where it is: an
uppercase `G` or `H`, one to five digits, and perhaps one lowercase letter (`H430a`), standing
alone (letters and digits on neither side). Lowercase `g`/`h` aren't taken, so `h1` in HTML is
left alone
*/
pub fn strongs_at(text: &str, cursor: usize) -> Option<(String, std::ops::Range<usize>)> {
    let bytes = text.as_bytes();
    let word = |b: u8| b.is_ascii_alphanumeric();
    let mut start = cursor.min(bytes.len());
    while start > 0 && word(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = cursor.min(bytes.len());
    while end < bytes.len() && word(bytes[end]) {
        end += 1;
    }
    let found = text.get(start..end)?;
    let (letter, rest) = found.split_at_checked(1)?;
    if !matches!(letter, "G" | "H") {
        return None;
    }
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let suffix = &rest[digits..];
    let suffix_ok =
        suffix.is_empty() || (suffix.len() == 1 && suffix.bytes().all(|b| b.is_ascii_lowercase()));
    if !(1..=5).contains(&digits) || !suffix_ok {
        return None;
    }
    // A letter or digit outside ASCII glued on is still a word
    let glued = |c: Option<char>| c.is_some_and(char::is_alphanumeric);
    let (before, after) = (text.get(..start)?, text.get(end..)?);
    if glued(before.chars().next_back()) || glued(after.chars().next()) {
        return None;
    }
    Some((normalize_key(found), start..end))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_strongs_numbers() {
        let text = "love (G0025), God (H430a), h1, G25xy, aG25, G123456";
        let at = |needle: &str| strongs_at(text, text.find(needle).unwrap() + 1);
        assert_eq!(at("G0025"), Some((String::from("G25"), 6..11)));
        assert_eq!(at("H430a").map(|(k, _)| k), Some(String::from("H430a")));
        // At the number's end too
        assert_eq!(
            strongs_at(text, 11).map(|(k, _)| k),
            Some(String::from("G25"))
        );
        for needle in ["h1", "G25xy", "aG25", "G123456", "love"] {
            assert_eq!(at(needle), None, "{needle}");
        }
        assert_eq!(strongs_at("éG25", 3), None);
    }
}
