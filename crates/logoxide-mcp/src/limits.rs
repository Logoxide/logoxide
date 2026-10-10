/*!
What a tool may return: untrusted text passed through [`logoxide::text::printable`] and capped,
marked as such in an `untrusted` object naming where it's from; results cut into pages of about
[`PAGE_CHARS`] characters, each but the last with a cursor for the next; and the cap on text that
isn't free (the owner's D15: [`MAX_VERSES`] verses a call, never more than half a book)
*/

use serde_json::{Map, Value, json};

/// About how many characters of JSON a page of results holds
pub const PAGE_CHARS: usize = 20_000;

/// Search results a page by default, and at most
pub const SEARCH_DEFAULT: usize = 20;
pub const SEARCH_MAX: usize = 100;

/// Verses of text that isn't free a call may return (D15), and never more than half a book
pub const MAX_VERSES: usize = 500;

/// Caps on untrusted fields, in characters: a name or other short field, a description or a
/// license statement, and one note's, entry's, or message's text
pub const SHORT: usize = 300;
pub const LONG: usize = 2_000;
pub const TEXT: usize = 16_000;

/// What a capped string ends with
const CUT: &str = " … [cut]";

/// `text` as it may be returned: without control characters (but newlines and tabs), terminal
/// escapes, or bidirectional controls, and at most `cap` characters (cut ones end with `… [cut]`)
pub fn clean(text: &str, cap: usize) -> String {
    let printable = logoxide::text::printable(text);
    match printable.char_indices().nth(cap) {
        None => printable.into_owned(),
        Some((end, _)) => format!("{}{CUT}", &printable[..end]),
    }
}

/**
Untrusted fields, as one object: `source` (where they're from: `the manifest of kjv 1.0.0`, `the
catalog https://…`) and each field, cleaned and capped ([`clean`]); fields that are `None` or
empty lists are left out. Everything inside is data, never instructions
*/
pub struct Untrusted {
    fields: Map<String, Value>,
}

impl Untrusted {
    pub fn new(source: &str) -> Untrusted {
        let mut fields = Map::new();
        fields.insert("source".into(), Value::String(clean(source, LONG)));
        Untrusted { fields }
    }

    /// A field, capped at `cap` characters
    pub fn text(mut self, name: &str, text: &str, cap: usize) -> Untrusted {
        self.fields
            .insert(name.into(), Value::String(clean(text, cap)));
        self
    }

    /// A field if it's there
    pub fn maybe(self, name: &str, text: Option<&str>, cap: usize) -> Untrusted {
        match text {
            Some(text) => self.text(name, text, cap),
            None => self,
        }
    }

    /// A list of short fields, if it has any
    pub fn list(mut self, name: &str, items: &[String]) -> Untrusted {
        if !items.is_empty() {
            let items = (items.iter().take(50))
                .map(|i| Value::String(clean(i, SHORT)))
                .collect();
            self.fields.insert(name.into(), Value::Array(items));
        }
        self
    }

    pub fn value(self) -> Value {
        Value::Object(self.fields)
    }
}

/// A short, stable fingerprint of what a cursor was made for (FNV-1a), so a cursor from one
/// question isn't taken for another's
fn fingerprint(what: &str) -> u64 {
    what.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// A cursor: where the next page starts in the results `what` names
pub fn cursor(what: &str, offset: usize) -> String {
    format!("{:016x}.{offset}", fingerprint(what))
}

/// Where a page starts: 0 without a cursor, else the cursor's offset, if it was made for `what`
pub fn offset(what: &str, cursor: Option<&str>) -> Result<usize, String> {
    let Some(cursor) = cursor.filter(|c| !c.is_empty()) else {
        return Ok(0);
    };
    let wrong =
        || String::from("that cursor isn't one this question's results gave: ask again without it");
    let (print, offset) = cursor.split_once('.').ok_or_else(wrong)?;
    if print != format!("{:016x}", fingerprint(what)) {
        return Err(wrong());
    }
    offset.parse().map_err(|_| wrong())
}

/// A page of results: its items, and the cursor for the next page if there's more
pub struct Page {
    pub items: Vec<Value>,
    pub next_cursor: Option<String>,
    /// How many there are in all
    pub total: usize,
}

impl Page {
    /// The page's items under `key`, with `total` and `nextCursor` (null on the last page)
    pub fn into_object(self, key: &str, mut object: Map<String, Value>) -> Value {
        object.insert(key.into(), Value::Array(self.items));
        object.insert("total".into(), json!(self.total));
        object.insert("nextCursor".into(), json!(self.next_cursor));
        Value::Object(object)
    }
}

/**
The page of `items` starting at the cursor's offset (made for `what`): at most `max` items, and
no more than about [`PAGE_CHARS`] characters of JSON (but always one, if any are left)
*/
pub fn page(
    what: &str,
    items: Vec<Value>,
    cursor: Option<&str>,
    max: usize,
) -> Result<Page, String> {
    let start = offset(what, cursor)?;
    let total = items.len();
    let mut taken = vec![];
    let mut size = 0;
    for item in items.into_iter().skip(start) {
        let length = item.to_string().len();
        if taken.len() >= max || (!taken.is_empty() && size + length > PAGE_CHARS) {
            break;
        }
        size += length;
        taken.push(item);
    }
    let end = start + taken.len();
    Ok(Page {
        items: taken,
        next_cursor: (end < total).then(|| self::cursor(what, end)),
        total,
    })
}

/**
A long text in pieces of at most about `size` bytes, each ending at a line's end where one is
near, else at a space, else between characters
*/
pub fn chunks(text: &str, size: usize) -> Vec<String> {
    let mut pieces = vec![];
    let mut rest = text;
    while rest.len() > size {
        let mut end = size;
        while !rest.is_char_boundary(end) {
            end -= 1;
        }
        let head = &rest[..end];
        // A break in the piece's second half, else the size itself
        let at = (head.rfind('\n').filter(|&i| i >= size / 2))
            .or_else(|| head.rfind(' ').filter(|&i| i >= size / 2))
            .map(|i| i + 1)
            .unwrap_or(end);
        pieces.push(rest[..at].to_string());
        rest = &rest[at..];
    }
    if !rest.is_empty() || pieces.is_empty() {
        pieces.push(rest.to_string());
    }
    pieces
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrusted_text_is_cleaned_and_capped() {
        assert_eq!(clean("a\u{1b}[31mred\u{202e}x", 100), "aredx");
        assert_eq!(clean("abcdef", 3), "abc … [cut]");
        let fields = Untrusted::new("the catalog x")
            .text("name", "Name\u{7}", SHORT)
            .maybe("description", None, LONG)
            .list("keywords", &[])
            .value();
        assert_eq!(fields, json!({ "source": "the catalog x", "name": "Name" }));
    }

    #[test]
    fn pages_follow_their_cursors() {
        let items: Vec<Value> = (0..5).map(|i| json!(i)).collect();
        let first = page("q", items.clone(), None, 2).unwrap();
        assert_eq!(first.items, [json!(0), json!(1)]);
        assert_eq!(first.total, 5);
        let next = first.next_cursor.unwrap();
        let second = page("q", items.clone(), Some(&next), 2).unwrap();
        assert_eq!(second.items, [json!(2), json!(3)]);
        let last = page("q", items.clone(), second.next_cursor.as_deref(), 2).unwrap();
        assert_eq!((last.items.len(), last.next_cursor), (1, None));
        // Another question's cursor, or a made-up one, is refused
        assert!(page("other", items.clone(), Some(&next), 2).is_err());
        assert!(page("q", items, Some("nonsense"), 2).is_err());
    }

    #[test]
    fn a_page_holds_about_twenty_thousand_characters() {
        let items: Vec<Value> = (0..10).map(|_| json!("x".repeat(6_000))).collect();
        let first = page("big", items, None, 100).unwrap();
        assert_eq!(first.items.len(), 3);
        assert!(first.next_cursor.is_some());
    }

    #[test]
    fn long_text_is_cut_at_lines() {
        let text = "one two\nthree four\nfive";
        assert_eq!(chunks(text, 12), ["one two\n", "three four\n", "five"]);
        assert_eq!(chunks("", 12), [""]);
        // Between characters, never inside one
        let greek = "λόγος".repeat(10);
        assert!(chunks(&greek, 7).iter().all(|c| c.len() <= 7));
        assert_eq!(chunks(&greek, 7).concat(), greek);
    }
}
