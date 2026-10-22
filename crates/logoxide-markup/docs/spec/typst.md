# Typst

`TypstRenderer` writes Typst markup (typst.app, the markup mode of Typst 0.13 and later) in which
every element is a call of a function named for what it is: `#lx-verse(n: "16");`,
`#lx-note(note-type: "crossReference", n: "a")[Ps 1:1];`. The renderer never says how anything
looks: a template defines the functions (`TypstRenderer::definitions` gives plain ones to start
from), the way a stylesheet styles the HTML renderer's classes. Function names and their
arguments are API. One renderer is meant to be kept for a passage: `render` is called once per
entry, in order, and words of Jesus carry across calls, as in the HTML renderer (html.md).

The whole module, and the root's re-export of `TypstOptions` and `TypstRenderer`, exist only with
the `typst` feature, off by default; without it nothing else changes.

## Options

`TypstOptions { prefix, filter, smart_quotes }`, default `lx-`, `Filter::all().drop(Kind::Metadata)`,
and `false`. `TypstRenderer::new()` (= `default()`) uses the default options; `with_options`
others. The options are the public field `opts` and can be changed between calls.

- `prefix` is put before every function name, as is (`lx-` gives `lx-verse`). It is not checked:
  the caller chooses one that makes Typst identifiers (a letter or `_`, then letters, digits, `_`,
  or `-`). The empty prefix gives bare names, some of which are Typst's own (`quote`, `ref`,
  `title`), which a template then replaces.
- `smart_quotes`: when false, `'` and `"` in text are escaped, so they print as they are in the
  text; when true they are written bare and Typst turns them into curly quotes.

## Output conventions

- A call is `#`, the prefix, the function's name, then the arguments in parentheses when there
  are any, then a content block `[`…`]` when the element is wrapped (its children rendered inside),
  then `;`. A call that is neither given arguments nor wrapped is written with empty parentheses
  (`#lx-lb();`). The `;` ends the call, so the text after it is never read as more arguments, a
  field access, or a method call.
- Arguments are named, written `name: "value"` and separated by `, `, in the order the table below
  lists them, each only when it has a value; every value is a string.
- A string is `"`, the value with `\` written `\\`, `"` written `\"`, line feed `\n`, carriage
  return `\r`, tab `\t`, and any other control character (`char::is_control`) as `\u{…}` with its
  code point in lowercase hexadecimal, then `"`.
- Nothing is added between calls: no newlines, no indentation, no spaces. The output never holds a
  line break, so it never holds a blank line (which is a paragraph break in Typst).

### Text

Each text node is written with these rules, in which "the output" is what this `render` call has
written so far (so text beside a call, or beside another text node, is judged by what is really
next to it):

1. A run of whitespace (`char::is_whitespace`) holding at least one line break (line feed,
   vertical tab, form feed, carriage return, U+0085, U+2028, U+2029) is written as one space.
   Other whitespace is written as it is.
2. These are always written with a `\` before them: `\` `#` `$` `*` `_` `` ` `` `[` `]` `<` `>` `@`
   `~` `=` `+` `/`, and `'` and `"` unless `smart_quotes` is on.
3. `-` gets a `\` when the output ends with `-` or `[`, or is empty (so no `--`, `---`, or list item
   can start).
4. `?` and every numeric character (`char::is_numeric`) get a `\` when the output ends with `-`
   (so no soft hyphen `-?` or minus sign `-1` is made).
5. `.` gets a `\` when the output ends with `..`, or when everything the output holds after its
   last `[` (or the whole output, when there is no `[`) is one or more ASCII digits (so no `...`
   ellipsis and no numbered list item `1.` can start).
6. Every other character is written as it is (non-ASCII as UTF-8).

`a--b` is written `a-\-b`; `...` is `..\.`; `3. Then` at the start is `3\. Then`; `"Yes," he
said` is `\"Yes,\" he said`; `<b>` is `\<b\>`.

## Elements

Each element goes through the filter first: `Drop` writes nothing; `Unwrap` writes its children
(with the current state) and nothing of its own; `Keep` writes by kind. In the table, `P` is the
prefix, "wrapped" means a call with the children rendered in its content block, and arguments
are listed in output order.

| Kind and shape | Output |
|---|---|
| `WordsOfJesus` or `Quote` with a milestone | nothing (state only, below); its children are not written |
| `WordsOfJesus` | `Pwoj`, wrapped; its children are inside words of Jesus |
| `Word` | `Pw`, wrapped; `strong`: `Element::strongs()` displayed and joined by single spaces (omitted when there are none); `lemma`: the `lemma` attribute; `morph`: the `morph` attribute |
| `Note` with children | `Pnote`, wrapped; `note-type`: `note_type()`; `n`: `n`, else `caller`; `osis-ref`: `osisRef` |
| `Reference` | `Pref`, wrapped; `osis-ref`: `reference_target()` |
| `Title` with children and no milestone | `Ptitle`, wrapped; `title-type`: `title_type()` |
| `Quote` (no milestone), `Added`, `DivineName`, `Foreign` | `Pquote`, `Padded`, `Pdivine-name`, `Pforeign`, wrapped |
| `Highlight` | `Phi`, wrapped; `hi-style`: `style()` |
| `LineBreak` | `Plb` (children not written) |
| `Paragraph`, `End` milestone | nothing |
| `Paragraph`, no milestone, with children | `Pp`, wrapped |
| `Paragraph`, otherwise (a `Start` milestone, or empty) | `Ppb` (children of a `Start` milestone are not written) |
| `LineGroup` with no milestone | `Plg`, wrapped (also when empty: `#lx-lg[];`) |
| `Line`, `End` milestone | `Ple` |
| `Line`, `Start` milestone | nothing (children not written) |
| `Line`, no milestone | `Pl`, wrapped |
| `Verse` or `Chapter` with no children, not an `End` milestone | `Pverse` (or `Pchapter`); `osis-id`: `osisID`, else `sID`; `n`: `n`, else `number`; for a verse only, `para-start` (below) |
| `Verse` or `Chapter` with no children, `End` milestone | nothing |
| `Entry` | `Pentry`, wrapped |
| `Division` with a USX style (`sidebar`) | `Pdiv`, wrapped |
| `Other` with a USX style and children (a `char` of no other kind: `pn`, `fr`, `jmp`; a table `cell`; a `para` of no other kind) | `Pname`, `name` the element's tag name made class-safe as in html.md (`char`, `cell`, `para`), wrapped |
| everything else: `Division`, `Image`, `Metadata` (when kept), `Other`, `Milestone`, `Note` without children, `Title` with a milestone or without children, `LineGroup` with a milestone, `Verse`/`Chapter` with children | children only, no call of its own |

**USX styles.** An element's USX style is as in html.md (the `style` attribute, when not empty,
of elements named `para`, `char`, `cell`, `sidebar`, and of a `note` with a `caller`). When a kept
element that writes a call has one, its call ends with the argument `style: "…"` after the others:
`#lx-p(style: "m")[…];`, `#lx-l(style: "q2")[…];`, `#lx-lb(style: "b");`.

**Verses that start a paragraph.** A verse's `para-start` is set when it is the first thing in the
paragraph or poetic line around it: its nearest ancestor of kind `Paragraph` or `Line` in this
entry's tree (whatever the filter does with that ancestor) holds no text node before the verse,
in document order, that is not whitespace-only (by `trim`), counting every text node whatever the
filter does with it. Its value is that ancestor's USX style when it has one (`p`, `m`, `q1`), else
`p` for a paragraph and `l` for a line. A template can then show where a paragraph starts even
when it puts every verse on its own line. A verse with no such ancestor (OSIS with paragraph
milestones, a verse in a heading) has none.

Notes on the table:

- Every call made for a kept element is the same whether the element is empty or not, except the
  shapes the table separates (notes, titles, paragraphs, and verses with and without children).
- Images are not rendered (there is no source to point to); their children, if any, are.

## Words of Jesus across milestones and entries

As in html.md: a kept `WordsOfJesus` start milestone opens them (a second start replaces the
first), a kept `WordsOfJesus` element that is `unclosed` is rendered and then opens them, and a
kept end milestone of kind `WordsOfJesus` or `Quote` closes them when it matches (flatten.md's
rule). The state is kept between `render` calls.

While they are open, every text node that is not inside a `WordsOfJesus` element (kept or
unwrapped) and is not whitespace-only (by `trim`) is written as `#Pwoj[`text`];`, one call per text
node. Whitespace-only text nodes are written plainly.

## `definitions`

`definitions()` is a Typst file's text that defines, with `#let`, every function the renderer can
call with the current prefix: `woj`, `w`, `note`, `ref`, `title`, `quote`, `added`, `divine-name`,
`foreign`, `hi`, `lb`, `p`, `pb`, `lg`, `l`, `le`, `verse`, `chapter`, `entry`, `div`, `char`,
`cell`, `para`. Each takes the arguments listed above as named arguments defaulting to `none`, a
`style` argument defaulting to `none`, any other named arguments (`..rest`), and, when wrapped, its
body as the last positional argument. So `definitions()` followed by rendered entries is a Typst
document that compiles, plainly set: notes as footnotes, titles as bold blocks, verse numbers
superscript, chapter numbers bold, paragraphs and line groups as paragraphs, each poetic line on
its own line, divine names in small capitals, highlights in their style (`italic`, `bold`,
`underline`, `small-caps`, `super`, `sub`), and everything else as its text. A template copies it
and changes what it wants, or defines its own.

## Full example

OSIS `<title type="psalm">A Psalm</title><w lemma="strong:H3068" morph="x">LORD</w><note
type="crossReference" n="a">Ps 1:1</note><note type="explanation">Or, Jehovah</note> <hi
type="small-caps">x</hi><transChange type="added">is</transChange>` with the default renderer:

```typst
#lx-title(title-type: "psalm")[A Psalm];#lx-w(strong: "H3068", lemma: "strong:H3068", morph: "x")[LORD];#lx-note(note-type: "crossReference", n: "a")[Ps 1:1];#lx-note(note-type: "explanation")[Or, Jehovah]; #lx-hi(hi-style: "small-caps")[x];#lx-added[is];
```

USX `<para style="p"><verse number="16" style="v" sid="JHN 3:16"/>For God<verse eid="JHN
3:16"/></para>` renders

```typst
#lx-p(style: "p")[#lx-verse(osis-id: "JHN 3:16", n: "16", para-start: "p");For God];
```

(A USX `verse` is not one of the elements with a USX style, so it has no `style` argument.)
