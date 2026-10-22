//! `TypstRenderer` and `TypstOptions` (docs/spec/typst.md), with the `typst` feature.

#![cfg(feature = "typst")]

use logoxide_markup::{Filter, Kind, Markup, TypstOptions, TypstRenderer, parse};

fn render(markup: Markup, input: &str) -> String {
    TypstRenderer::new().render(&parse(markup, input))
}

#[test]
fn calls_and_arguments() {
    let nodes = parse(
        Markup::Osis,
        r#"<title type="psalm">A Psalm</title><w lemma="strong:H3068" morph="x">LORD</w><note type="crossReference" n="a">Ps 1:1</note><note type="explanation">Or, Jehovah</note> <hi type="small-caps">x</hi><transChange type="added">is</transChange>"#,
    );
    assert_eq!(
        TypstRenderer::new().render(&nodes),
        r#"#lx-title(title-type: "psalm")[A Psalm];#lx-w(strong: "H3068", lemma: "strong:H3068", morph: "x")[LORD];#lx-note(note-type: "crossReference", n: "a")[Ps 1:1];#lx-note(note-type: "explanation")[Or, Jehovah]; #lx-hi(hi-style: "small-caps")[x];#lx-added[is];"#
    );
    let mut bare = TypstRenderer::with_options(TypstOptions {
        prefix: "".into(),
        filter: Filter::all().drop(Kind::Title).unwrap(Kind::Word),
        ..TypstOptions::default()
    });
    assert!(
        bare.render(&nodes)
            .starts_with(r#"LORD#note(note-type: "crossReference", n: "a")[Ps 1:1];"#)
    );
}

#[test]
fn usx_verses_styles_and_paragraph_starts() {
    let usx = r#"<usx><book code="JHN" style="id">x</book><para style="p"><verse number="16" style="v" sid="JHN 3:16"/>For God<verse eid="JHN 3:16"/> <verse number="17" sid="JHN 3:17"/>so<verse eid="JHN 3:17"/></para><para style="m"> <verse number="18" sid="JHN 3:18"/>He</para><para style="q1">Text <verse number="19" sid="JHN 3:19"/>x</para><para style="b"/></usx>"#;
    assert_eq!(
        render(Markup::Usx, usx),
        concat!(
            r#"#lx-p(style: "p")[#lx-verse(osis-id: "JHN 3:16", n: "16", para-start: "p");For God #lx-verse(osis-id: "JHN 3:17", n: "17");so];"#,
            r#"#lx-p(style: "m")[ #lx-verse(osis-id: "JHN 3:18", n: "18", para-start: "m");He];"#,
            r#"#lx-l(style: "q1")[Text #lx-verse(osis-id: "JHN 3:19", n: "19");x];"#,
            r#"#lx-lb(style: "b");"#,
        )
    );
}

#[test]
fn paragraph_start_counts_dropped_text() {
    // A dropped note's text still comes before the verse
    let usx = r#"<usx><para style="p"><note caller="+" style="f">n</note><verse number="1" sid="GEN 1:1"/>In</para><para style="p"><note caller="+" style="f"> </note><verse number="2" sid="GEN 1:2"/>And</para></usx>"#;
    let mut typst = TypstRenderer::with_options(TypstOptions {
        filter: Filter::all().drop(Kind::Note),
        ..TypstOptions::default()
    });
    assert_eq!(
        typst.render(&parse(Markup::Usx, usx)),
        r#"#lx-p(style: "p")[#lx-verse(osis-id: "GEN 1:1", n: "1");In];#lx-p(style: "p")[#lx-verse(osis-id: "GEN 1:2", n: "2", para-start: "p");And];"#
    );
}

#[test]
fn paragraphs_lines_and_breaks() {
    assert_eq!(
        render(
            Markup::Osis,
            r#"<p>a</p><p/><lg><l>one</l><l sID="x"/>two<l eID="x"/></lg>x<lb/>"#
        ),
        "#lx-p[a];#lx-pb();#lx-lg[#lx-l[one];two#lx-le();];x#lx-lb();"
    );
    assert_eq!(render(Markup::Osis, "<lg/>"), "#lx-lg[];");
}

#[test]
fn escaping() {
    let text = |s: &str| render(Markup::Plaintext, s);
    assert_eq!(text("a--b"), r"a-\-b");
    assert_eq!(text("a---b"), r"a-\-\-b");
    assert_eq!(text("wait..."), r"wait..\.");
    assert_eq!(text("3. Then"), r"3\. Then");
    assert_eq!(text("in 3. Then"), "in 3. Then");
    assert_eq!(text("- x"), r"\- x");
    assert_eq!(text("a-? -1"), r"a-\? -\1");
    assert_eq!(text("well-known"), "well-known");
    assert_eq!(
        text(r#""Yes," he said; it's <b> #1 $5 *a* _b_ `c` @d ~ = + / \ [x]"#),
        r#"\"Yes,\" he said; it\'s \<b\> \#1 \$5 \*a\* \_b\_ \`c\` \@d \~ \= \+ \/ \\ \[x\]"#
    );
    let mut smart = TypstRenderer::with_options(TypstOptions {
        smart_quotes: true,
        ..TypstOptions::default()
    });
    assert_eq!(
        smart.render(&parse(Markup::Plaintext, r#""Yes," it's"#)),
        r#""Yes," it's"#
    );
    // Escaping looks at what was written, across text nodes and calls
    assert_eq!(
        render(Markup::Osis, "<w>-</w><hi type=\"bold\">1. a</hi>"),
        r#"#lx-w[\-];#lx-hi(hi-style: "bold")[1\. a];"#
    );
}

#[test]
fn no_line_breaks() {
    let out = render(
        Markup::Osis,
        "<p>In the\n\n  beginning\r\nGod</p>\n\n<p>\t x</p><w lemma=\"a\nb\">c</w>",
    );
    assert_eq!(
        out,
        "#lx-p[In the beginning God]; #lx-p[\t x];#lx-w(lemma: \"a\\nb\")[c];"
    );
    assert!(!out.contains('\n'));
}

#[test]
fn strings_are_escaped() {
    assert_eq!(
        render(Markup::Osis, "<w lemma='a\"b\\c&#9;\u{1}'>x</w>"),
        r#"#lx-w(lemma: "a\"b\\c\t\u{1}")[x];"#
    );
}

#[test]
fn words_of_jesus_across_entries() {
    let mut typst = TypstRenderer::new();
    let first = typst.render(&parse(
        Markup::Osis,
        r#"He said, <q who="Jesus" sID="q1"/>I am <q sID="q2"/>the way<q eID="q2"/>"#,
    ));
    assert_eq!(first, "He said, #lx-woj[I am ];#lx-woj[the way];");
    let second = typst.render(&parse(Markup::Osis, r#"and the life.<q eID="q1"/> Then"#));
    assert_eq!(second, "#lx-woj[and the life.]; Then");
    let mut typst = TypstRenderer::new();
    assert_eq!(
        typst.render(&parse(Markup::Osis, r#"New <q who="Jesus">start"#)),
        "New #lx-woj[start];"
    );
    assert_eq!(
        typst.render(&parse(Markup::Osis, "more</q> narration")),
        "#lx-woj[more]; narration"
    );
}

#[test]
fn chapters_references_and_others() {
    assert_eq!(
        render(
            Markup::Osis,
            r#"<chapter osisID="Gen.1" n="1"/><chapter eID="x"/><reference osisRef="Gen.1.1">here</reference><divineName>Lord</divineName><foreign>x</foreign><q>q</q>"#
        ),
        r#"#lx-chapter(osis-id: "Gen.1", n: "1");#lx-ref(osis-ref: "Gen.1.1")[here];#lx-divine-name[Lord];#lx-foreign[x];#lx-quote[q];"#
    );
    assert_eq!(
        render(
            Markup::Usx,
            r#"<usx><para style="p"><char style="pn">Ur</char><char style="nd">LORD</char></para><sidebar style="esb">s</sidebar></usx>"#
        ),
        r#"#lx-p(style: "p")[#lx-char(style: "pn")[Ur];#lx-divine-name(style: "nd")[LORD];];#lx-div(style: "esb")[s];"#
    );
}

#[test]
fn definitions_cover_every_call() {
    let defs = TypstRenderer::new().definitions();
    for name in [
        "woj",
        "w",
        "note",
        "ref",
        "title",
        "quote",
        "added",
        "divine-name",
        "foreign",
        "hi",
        "lb",
        "p",
        "pb",
        "lg",
        "l",
        "le",
        "verse",
        "chapter",
        "entry",
        "div",
        "char",
        "cell",
        "para",
    ] {
        assert!(defs.contains(&format!("\n#let lx-{name}(")), "{name}");
    }
    assert!(defs.contains(
        "#let lx-verse(osis-id: none, n: none, para-start: none, style: none, ..rest) = "
    ));
    let other = TypstRenderer::with_options(TypstOptions {
        prefix: "bible-".into(),
        ..TypstOptions::default()
    });
    assert!(
        other
            .definitions()
            .contains("#let bible-woj(style: none, ..rest, body) = body")
    );
}
