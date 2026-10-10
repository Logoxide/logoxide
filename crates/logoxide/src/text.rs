/*!
Text from elsewhere made safe to show in a terminal. A catalog's, manifest's, or git's text could
hold escape sequences that move the cursor, rewrite what was printed, set the window's title, or
make a link, and bidirectional controls that show text in another order than it's read
(Trojan Source). [`printable`] drops them all, keeping the words.
*/

use std::borrow::Cow;

const ESC: char = '\u{1b}';
const BEL: char = '\u{7}';

/**
`text` without control characters (C0 but `\n` and `\t`, DEL, and C1), whole escape sequences
(CSI `ESC [ … m`, OSC `ESC ] … BEL`, and the other strings ending in `ESC \`, in their 7-bit and
C1 forms), and bidirectional embeddings, overrides, and isolates (U+202A–202E, U+2066–2069).
Text without any is returned as it is
*/
pub fn printable(text: &str) -> Cow<'_, str> {
    if !text.chars().any(unwanted) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            ESC => match chars.next() {
                // CSI: parameters and intermediates, then one final byte
                Some('[') => skip_csi(&mut chars),
                // OSC, DCS, SOS, PM, APC: a string to BEL or ST
                Some(']' | 'P' | 'X' | '^' | '_') => skip_string(&mut chars),
                // A two-character sequence (`ESC c`, `ESC 7`), or ESC at the end
                _ => {}
            },
            '\u{9b}' => skip_csi(&mut chars),
            '\u{9d}' | '\u{90}' | '\u{98}' | '\u{9e}' | '\u{9f}' => skip_string(&mut chars),
            c if unwanted(c) => {}
            c => out.push(c),
        }
    }
    Cow::Owned(out)
}

/// A character [`printable`] never keeps
fn unwanted(c: char) -> bool {
    (c.is_control() && c != '\n' && c != '\t')
        || ('\u{202a}'..='\u{202e}').contains(&c)
        || ('\u{2066}'..='\u{2069}').contains(&c)
}

/// Past a CSI sequence's parameters (0x30–0x3F) and intermediates (0x20–0x2F) and its final
/// byte (0x40–0x7E); anything else ends it early, and is kept
fn skip_csi(chars: &mut std::iter::Peekable<std::str::Chars>) {
    while let Some(&c) = chars.peek() {
        match c {
            '\u{20}'..='\u{3f}' => {
                chars.next();
            }
            '\u{40}'..='\u{7e}' => {
                chars.next();
                return;
            }
            _ => return,
        }
    }
}

/// Past a control string, to BEL, ST (`ESC \` or U+009C), or the end of the text
fn skip_string(chars: &mut std::iter::Peekable<std::str::Chars>) {
    while let Some(c) = chars.next() {
        match c {
            BEL | '\u{9c}' => return,
            ESC => {
                if chars.peek() == Some(&'\\') {
                    chars.next();
                }
                return;
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_kept() {
        for text in ["King James Version", "Ἰησοῦς\tבְּרֵאשִׁית\n", ""] {
            assert!(matches!(printable(text), Cow::Borrowed(t) if t == text));
        }
    }

    #[test]
    fn controls_and_sequences_are_dropped() {
        let cases = [
            ("\u{1b}[31mred\u{1b}[0m", "red"),
            ("\u{1b}[2J\u{1b}[1;1Hcleared", "cleared"),
            ("\u{1b}]0;title\u{7}name", "name"),
            (
                "\u{1b}]8;;https://x.example\u{1b}\\link\u{1b}]8;;\u{1b}\\",
                "link",
            ),
            ("\u{9b}31mC1\u{9d}0;t\u{9c}", "C1"),
            ("a\rb\u{8}c\u{0}d\u{7f}e\u{85}f", "abcdef"),
            ("abc\u{202e}fed\u{202c} \u{2066}x\u{2069}", "abcfed x"),
            ("\u{1b}Pdcs\u{1b}\\ok \u{1b}", "ok "),
            ("line\n\tnext", "line\n\tnext"),
        ];
        for (text, shown) in cases {
            assert_eq!(printable(text), shown, "{text:?}");
        }
    }
}
