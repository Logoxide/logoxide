//! Typst markup in which every element is a call of a function a template defines.

use std::borrow::Cow;
use std::fmt::Write as _;

use crate::flatten::words_match;
use crate::{Element, Filter, Kind, Milestone, Node, filter::Action};

/// Options for [`TypstRenderer`]
#[derive(Debug, Clone)]
pub struct TypstOptions {
    /// Put before every function's name; default: `lx-` (`lx-verse`, `lx-woj`)
    pub prefix: Cow<'static, str>,
    /// Which elements are rendered; default: [`Filter::all`] except metadata
    pub filter: Filter,
    /// Write `'` and `"` bare, so Typst makes them curly; default: false (escaped, printed as
    /// they are)
    pub smart_quotes: bool,
}

impl Default for TypstOptions {
    fn default() -> Self {
        TypstOptions {
            prefix: Cow::Borrowed("lx-"),
            filter: Filter::all().drop(Kind::Metadata),
            smart_quotes: false,
        }
    }
}

/**
Renders Typst markup in which every element is a call of a function named for its kind, never
saying how anything looks: a template defines the functions, as a stylesheet styles
[`HtmlRenderer`](crate::HtmlRenderer)'s classes

Functions (with the prefix, `lx-` by default), and their named arguments, every one a string:

| Element | Function | Arguments |
|---|---|---|
| word | `w` | `strong` (`H7225 H853`), `lemma`, `morph` |
| note | `note` | `note-type`, `n`, `osis-ref` |
| reference | `ref` | `osis-ref` |
| title | `title` | `title-type` |
| words of Jesus, quote, translator addition, divine name, foreign | `woj`, `quote`, `added`, `divine-name`, `foreign` | |
| highlight | `hi` | `hi-style` (`italic`, `small-caps`) |
| line break | `lb` | |
| paragraph, an empty one or its start milestone | `p`, `pb` | |
| line group, line, a line's end milestone | `lg`, `l`, `le` | |
| verse, chapter (milestones) | `verse`, `chapter` | `osis-id`, `n`; a verse's `para-start` |
| dictionary entry | `entry` | |
| USX `sidebar`; any other USX `char`, table `cell`, or `para` | `div`; `char`, `cell`, `para` | |

Every USX `para`, `char`, `note`, `cell`, and `sidebar` also passes its `style` (`style: "q2"`).
A verse that is the first thing in its paragraph or line passes `para-start`: that element's USX
style, else `p` or `l`. Each call ends in `;`, and the output holds no line breaks.
[`TypstRenderer::definitions`] defines every function plainly, so it and the rendered entries
make a document that compiles.

```
use logoxide_markup::{Markup, TypstRenderer, parse};

let mut typst = TypstRenderer::new();
let verse = typst.render(&parse(Markup::Osis, r#"<w lemma="strong:G2424">Jesus</w> said, <q who="Jesus">Follow me</q>"#));
assert_eq!(verse, r#"#lx-w(strong: "G2424", lemma: "strong:G2424")[Jesus]; said, #lx-woj[Follow me];"#);
assert!(typst.definitions().contains("#let lx-woj("));
```
*/
#[derive(Debug, Default, Clone)]
pub struct TypstRenderer {
    pub opts: TypstOptions,
    /// Words of Jesus open across milestones or entries, with the start's id
    words: Option<Option<String>>,
}

/// What follows an element's call
#[derive(Default)]
struct Plan {
    /// Its children are rendered
    children: bool,
    /// Its children are inside words of Jesus
    words: bool,
    /// Its children are in a content block, closed with `];`
    wrapped: bool,
    /// Words of Jesus stay open after it (an unclosed words-of-Jesus element)
    open_words: bool,
}

impl Plan {
    /// Only the children, with no call of its own
    fn children() -> Self {
        Plan {
            children: true,
            ..Plan::default()
        }
    }
}

/// A paragraph or line a verse may start
struct Around {
    /// Its USX style, else `p` or `l`
    value: String,
}

/// One entry being rendered
struct Render<'r> {
    renderer: &'r mut TypstRenderer,
    out: String,
    /// How many words-of-Jesus elements the current node is in (kept or unwrapped)
    in_words: usize,
    /// The USX style of the element being opened, for its call's `style`
    style: Option<String>,
    /// The paragraphs and lines the current node is in, innermost last
    around: Vec<Around>,
    /// The paragraphs and lines from this index on have held no text yet
    unseen_from: usize,
}

/// The functions [`TypstRenderer::definitions`] defines: name, arguments before `style`, and
/// whether it takes a body
const FUNCTIONS: &[(&str, &[&str], bool, &str)] = &[
    ("woj", &[], true, "body"),
    ("w", &["strong", "lemma", "morph"], true, "body"),
    (
        "note",
        &["note-type", "n", "osis-ref"],
        true,
        "footnote(body)",
    ),
    ("ref", &["osis-ref"], true, "body"),
    ("title", &["title-type"], true, "block(strong(body))"),
    ("quote", &[], true, "body"),
    ("added", &[], true, "emph(body)"),
    ("divine-name", &[], true, "smallcaps(body)"),
    ("foreign", &[], true, "body"),
    (
        "hi",
        &["hi-style"],
        true,
        "if hi-style == \"italic\" { emph(body) } else if hi-style == \"bold\" { strong(body) } else if hi-style == \"underline\" { underline(body) } else if hi-style == \"small-caps\" { smallcaps(body) } else if hi-style == \"super\" { super(body) } else if hi-style == \"sub\" { sub(body) } else { body }",
    ),
    ("lb", &[], false, "linebreak()"),
    ("p", &[], true, "{ parbreak(); body; parbreak() }"),
    ("pb", &[], false, "parbreak()"),
    ("lg", &[], true, "{ parbreak(); body; parbreak() }"),
    ("l", &[], true, "{ body; linebreak() }"),
    ("le", &[], false, "linebreak()"),
    (
        "verse",
        &["osis-id", "n", "para-start"],
        false,
        "if n != none { super(n) }",
    ),
    (
        "chapter",
        &["osis-id", "n"],
        false,
        "if n != none { strong(n) + \" \" }",
    ),
    ("entry", &[], true, "block(body)"),
    ("div", &[], true, "block(body)"),
    ("char", &[], true, "body"),
    ("cell", &[], true, "body"),
    ("para", &[], true, "{ parbreak(); body; parbreak() }"),
];

impl TypstRenderer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_options(opts: TypstOptions) -> Self {
        TypstRenderer {
            opts,
            ..Self::default()
        }
    }

    /// One entry's nodes; each call is the next entry, as in [`HtmlRenderer::render`](crate::HtmlRenderer::render)
    pub fn render(&mut self, nodes: &[Node]) -> String {
        let mut render = Render {
            renderer: self,
            out: String::new(),
            in_words: 0,
            style: None,
            around: Vec::new(),
            unseen_from: 0,
        };
        render.nodes(nodes);
        render.out
    }

    /// A Typst file defining every function the renderer calls, with this prefix, plainly
    pub fn definitions(&self) -> String {
        let prefix = &self.opts.prefix;
        let mut out = String::from(
            "// Every function logoxide-markup's Typst renderer calls, plainly set: a template's starting point\n",
        );
        for (name, args, body, value) in FUNCTIONS {
            let _ = write!(out, "#let {prefix}{name}(");
            for arg in *args {
                let _ = write!(out, "{arg}: none, ");
            }
            out.push_str("style: none, ..rest");
            if *body {
                out.push_str(", body");
            }
            let _ = writeln!(out, ") = {value}");
        }
        out
    }
}

/// Writes a Typst string literal
fn string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{{{:x}}}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// Whether a character ends a line in Typst
fn is_line_break(c: char) -> bool {
    matches!(
        c,
        '\n' | '\u{0b}' | '\u{0c}' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}'
    )
}

/// Whether what's after the output's last `[` (or all of it) is one or more ASCII digits
fn ends_in_leading_digits(out: &str) -> bool {
    let digits = out.len() - out.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    digits > 0 && {
        let before = &out[..out.len() - digits];
        before.is_empty() || before.ends_with('[')
    }
}

/// Writes one character of text, escaped by what the output already holds
fn text_char(c: char, smart_quotes: bool, out: &mut String) {
    let escape = match c {
        '\\' | '#' | '$' | '*' | '_' | '`' | '[' | ']' | '<' | '>' | '@' | '~' | '=' | '+'
        | '/' => true,
        '\'' | '"' => !smart_quotes,
        '-' => out.is_empty() || out.ends_with(['-', '[']),
        '.' => out.ends_with("..") || ends_in_leading_digits(out),
        c if c == '?' || c.is_numeric() => out.ends_with('-'),
        _ => false,
    };
    if escape {
        out.push('\\');
    }
    out.push(c);
}

/// Whether any text node under these nodes is not whitespace-only
fn has_text(nodes: &[Node]) -> bool {
    let mut stack: Vec<&[Node]> = vec![nodes];
    while let Some(nodes) = stack.pop() {
        for node in nodes {
            match node {
                Node::Text(t) if !t.trim().is_empty() => return true,
                Node::Text(_) => {}
                Node::Element(e) => stack.push(&e.children),
            }
        }
    }
    false
}

impl Render<'_> {
    fn nodes(&mut self, nodes: &[Node]) {
        for node in nodes {
            match node {
                Node::Text(t) => self.text(t),
                Node::Element(e) => self.element(e),
            }
        }
    }

    /// Notes that text came: no verse after it starts the paragraphs and lines it's in
    fn seen(&mut self) {
        self.unseen_from = self.around.len();
    }

    fn text(&mut self, text: &str) {
        let visible = !text.trim().is_empty();
        if visible {
            self.seen();
        }
        let words = self.renderer.words.is_some() && self.in_words == 0 && visible;
        if words {
            self.call_open("woj", &[]);
            self.out.push('[');
        }
        let smart = self.renderer.opts.smart_quotes;
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c.is_whitespace() {
                let mut run = String::from(c);
                while let Some(&next) = chars.peek().filter(|c| c.is_whitespace()) {
                    run.push(next);
                    chars.next();
                }
                if run.chars().any(is_line_break) {
                    self.out.push(' ');
                } else {
                    self.out.push_str(&run);
                }
            } else {
                text_char(c, smart, &mut self.out);
            }
        }
        if words {
            self.out.push_str("];");
        }
    }

    /// Writes `#name(args)`, with the opening element's `style` last; the parentheses only when
    /// there are arguments
    fn call_open(&mut self, name: &str, args: &[(&str, Option<String>)]) {
        let prefix = self.renderer.opts.prefix.clone();
        let _ = write!(self.out, "#{prefix}{name}");
        let style = self.style.take();
        let args = args
            .iter()
            .filter_map(|(n, v)| v.as_deref().map(|v| (*n, v)))
            .chain(style.as_deref().map(|s| ("style", s)));
        let mut first = true;
        for (arg, value) in args {
            self.out.push_str(if first { "(" } else { ", " });
            first = false;
            let _ = write!(self.out, "{arg}: ");
            string(value, &mut self.out);
        }
        if !first {
            self.out.push(')');
        }
    }

    /// A call without a body: `#name(args);`
    fn call(&mut self, name: &str, args: &[(&str, Option<String>)]) {
        let had_args = args.iter().any(|(_, v)| v.is_some()) || self.style.is_some();
        self.call_open(name, args);
        if !had_args {
            self.out.push_str("()");
        }
        self.out.push(';');
    }

    /// A call whose children follow in its content block
    fn wrapped(&mut self, name: &str, args: &[(&str, Option<String>)]) -> Plan {
        self.call_open(name, args);
        self.out.push('[');
        Plan {
            children: true,
            wrapped: true,
            ..Plan::default()
        }
    }

    fn element(&mut self, e: &Element) {
        // A paragraph or line holds the verses it starts with, whatever the filter does with it
        let around = matches!(e.kind, Kind::Paragraph | Kind::Line);
        if around {
            let value = usx_style(e).unwrap_or(if e.kind == Kind::Line { "l" } else { "p" });
            self.around.push(Around {
                value: value.to_string(),
            });
        }
        match self.renderer.opts.filter.action(e) {
            Action::Drop => {
                if self.unseen_from < self.around.len() && has_text(&e.children) {
                    self.seen();
                }
            }
            Action::Unwrap => {
                // An unwrapped words-of-Jesus element still holds its text: open words of Jesus
                // don't mark it again
                let words = e.kind == Kind::WordsOfJesus;
                if words {
                    self.in_words += 1;
                }
                self.nodes(&e.children);
                if words {
                    self.in_words -= 1;
                }
            }
            Action::Keep => self.keep(e),
        }
        if around {
            self.around.pop();
            self.unseen_from = self.unseen_from.min(self.around.len());
        }
    }

    fn keep(&mut self, e: &Element) {
        self.style = usx_style(e).map(str::to_string);
        let plan = self.open(e);
        self.style = None;
        if plan.children {
            if plan.words {
                self.in_words += 1;
            }
            self.nodes(&e.children);
            if plan.words {
                self.in_words -= 1;
            }
        } else if self.unseen_from < self.around.len() && has_text(&e.children) {
            self.seen();
        }
        self.close(&plan);
    }

    /// Writes what comes after an element's contents
    #[inline(never)]
    fn close(&mut self, plan: &Plan) {
        if plan.wrapped {
            self.out.push_str("];");
        }
        if plan.open_words {
            self.renderer.words = Some(None);
        }
    }

    /// Writes an element's call (or the whole of an element without contents) and says what
    /// follows; kept apart from the recursion so each level of nesting takes little stack
    #[inline(never)]
    fn open(&mut self, e: &Element) -> Plan {
        let attr = |name: &str| e.attr(name).map(str::to_string);
        match e.kind {
            Kind::WordsOfJesus | Kind::Quote if e.milestone.is_some() => match e.milestone {
                Some(Milestone::Start) if e.kind == Kind::WordsOfJesus => {
                    self.renderer.words = Some(e.attr("sID").map(str::to_string));
                }
                Some(Milestone::End) => {
                    if let Some(open) = &self.renderer.words
                        && words_match(open.as_deref(), e)
                    {
                        self.renderer.words = None;
                    }
                }
                _ => {}
            },
            Kind::WordsOfJesus => {
                let mut plan = self.wrapped("woj", &[]);
                plan.words = true;
                plan.open_words = e.unclosed;
                return plan;
            }
            Kind::Word => {
                let strongs: Vec<String> = e.strongs().iter().map(ToString::to_string).collect();
                let strong = (!strongs.is_empty()).then(|| strongs.join(" "));
                let args = [
                    ("strong", strong),
                    ("lemma", attr("lemma")),
                    ("morph", attr("morph")),
                ];
                return self.wrapped("w", &args);
            }
            Kind::Note if !e.children.is_empty() => {
                let args = [
                    ("note-type", e.note_type().map(str::to_string)),
                    ("n", attr("n").or_else(|| attr("caller"))),
                    ("osis-ref", attr("osisRef")),
                ];
                return self.wrapped("note", &args);
            }
            Kind::Reference => {
                let args = [("osis-ref", e.reference_target().map(str::to_string))];
                return self.wrapped("ref", &args);
            }
            Kind::Title if !e.children.is_empty() && e.milestone.is_none() => {
                let args = [("title-type", e.title_type().map(str::to_string))];
                return self.wrapped("title", &args);
            }
            Kind::Quote | Kind::Added | Kind::DivineName | Kind::Foreign => {
                return self.wrapped(e.kind.class_name(), &[]);
            }
            Kind::Highlight => {
                let args = [("hi-style", e.style().map(str::to_string))];
                return self.wrapped("hi", &args);
            }
            Kind::LineBreak => self.call("lb", &[]),
            Kind::Paragraph => match e.milestone {
                Some(Milestone::End) => {}
                None if !e.children.is_empty() => return self.wrapped("p", &[]),
                _ => self.call("pb", &[]),
            },
            Kind::LineGroup if e.milestone.is_none() => return self.wrapped("lg", &[]),
            Kind::Line => match e.milestone {
                Some(Milestone::End) => self.call("le", &[]),
                Some(Milestone::Start) => {}
                None => return self.wrapped("l", &[]),
            },
            Kind::Verse | Kind::Chapter if e.children.is_empty() => {
                if e.milestone != Some(Milestone::End) {
                    let starts = (e.kind == Kind::Verse && self.unseen_from < self.around.len())
                        .then(|| self.around.last().map(|a| a.value.clone()))
                        .flatten();
                    let args = [
                        ("osis-id", attr("osisID").or_else(|| attr("sID"))),
                        ("n", attr("n").or_else(|| attr("number"))),
                        ("para-start", starts),
                    ];
                    self.call(e.kind.class_name(), &args);
                }
            }
            Kind::Entry => return self.wrapped("entry", &[]),
            Kind::Division if self.style.is_some() => return self.wrapped("div", &[]),
            Kind::Other if self.style.is_some() && !e.children.is_empty() => {
                let name = class_safe(&e.name);
                return self.wrapped(&name, &[]);
            }
            _ => return Plan::children(),
        }
        Plan::default()
    }
}

/// A USX element's `style` (`q2`, `pn`): of `para`, `char`, `cell`, `sidebar`, and notes (which
/// have a `caller`, unlike other markups' `note`s)
fn usx_style(e: &Element) -> Option<&str> {
    let usx = match e.name.as_str() {
        "para" | "char" | "cell" | "sidebar" => true,
        "note" => e.attr("caller").is_some(),
        _ => false,
    };
    usx.then(|| e.attr("style"))
        .flatten()
        .filter(|s| !s.is_empty())
}

/// A tag name made safe for a function's name: ASCII letters (lowercased), digits, and `-`;
/// anything else is `-`
fn class_safe(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}
