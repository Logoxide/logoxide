/*!
A dictionary's keys: a Strong's number's normal form (`G0025` and `g25` are `G25`), what a key
is found by when it isn't matched exactly ([`fold`], as catalog search matches names), and the
hash a summary records a key by.
*/

use sha2::{Digest, Sha256};

/// Bytes of SHA-256 kept of a key
pub const KEY_HASH_BYTES: usize = 8;

/**
A key's normal form: a Strong's number (`G` or `H` in either case, digits, and an optional
suffix) is the uppercase letter, the number without leading zeros, and the suffix as given
(`G0025` is `G25`, `h0430a` is `H430a`); any other key is kept as given, trimmed
*/
pub fn normalize_key(key: &str) -> String {
    let key = key.trim();
    let mut chars = key.chars();
    let Some(letter) = chars.next() else {
        return String::new();
    };
    if !matches!(letter, 'G' | 'g' | 'H' | 'h') {
        return key.to_string();
    }
    let rest = chars.as_str();
    let digits = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    if digits == 0 {
        return key.to_string();
    }
    let (number, suffix) = rest.split_at(digits);
    let number = number.trim_start_matches('0');
    let number = if number.is_empty() { "0" } else { number };
    format!("{}{number}{suffix}", letter.to_ascii_uppercase())
}

/// Text as matched loosely: lowercase, without combining marks (accents, breathings, Hebrew
/// points), and with final sigma as sigma, as catalog search matches names
pub fn fold(text: &str) -> String {
    use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};
    text.to_lowercase()
        .nfd()
        .filter(|c| !is_combining_mark(*c))
        .map(|c| if c == 'ς' { 'σ' } else { c })
        .nfc()
        .collect()
}

/**
A text's words as matched loosely: split at whitespace, dashes, and slashes, each [`fold`]ed and
kept to its letters and digits (`Éphraïm's` is `ephraims`), empty ones left out. What a typed
answer or a heard quotation is compared by, whatever the translation's punctuation
*/
pub fn words(text: &str) -> Vec<String> {
    (text.split(|c: char| c.is_whitespace() || matches!(c, '-' | '‐' | '–' | '—' | '/')))
        .map(|word| {
            fold(word)
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
        })
        .filter(|word| !word.is_empty())
        .collect()
}

/// The first [`KEY_HASH_BYTES`] bytes of SHA-256 of a key's normal form: what a summary records
/// an entry by
pub fn key_hash(normal_key: &str) -> [u8; KEY_HASH_BYTES] {
    let full = Sha256::digest(normal_key.as_bytes());
    let mut hash = [0; KEY_HASH_BYTES];
    hash.copy_from_slice(&full[..KEY_HASH_BYTES]);
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strongs_numbers_have_one_form() {
        assert_eq!(normalize_key("G0025"), "G25");
        assert_eq!(normalize_key("g25"), "G25");
        assert_eq!(normalize_key(" G25 "), "G25");
        assert_eq!(normalize_key("h0430a"), "H430a");
        assert_eq!(normalize_key("H0000"), "H0");
        assert_eq!(normalize_key("G3056!b"), "G3056!b");
        // Not Strong's numbers: kept as given
        assert_eq!(normalize_key("Aaron"), "Aaron");
        assert_eq!(normalize_key("Gad"), "Gad");
        assert_eq!(normalize_key("G"), "G");
        assert_eq!(normalize_key("00025"), "00025");
        assert_eq!(fold("Ἰησοῦς"), fold("ιησους"));
        assert_eq!(fold("AARON"), "aaron");
        assert_eq!(key_hash("G25").len(), KEY_HASH_BYTES);
        assert_ne!(key_hash("G25"), key_hash("G26"));
    }

    #[test]
    fn loose_words() {
        assert_eq!(
            words("For God so loved—the world, “Éphraïm’s” x/y 3:16"),
            [
                "for", "god", "so", "loved", "the", "world", "ephraims", "x", "y", "316"
            ]
        );
        assert!(words(" — , ").is_empty());
    }
}
