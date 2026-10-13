//! `Flat`, `Flattener`, spans, entries, and sources (docs/spec/flatten.md).

use logoxide_markup::{Flattener, Kind, Markup, NodePath, TextOptions, flatten::flatten, parse};

fn path(p: &[usize]) -> NodePath {
    NodePath(p.to_vec())
}

#[test]
fn text_maps_back_to_the_tree() {
    let nodes = parse(
        Markup::Osis,
        "See <hi type=\"italic\">John</hi>   3:16\n<note>n</note> now",
    );
    let flat = flatten(&nodes, &TextOptions::default());
    assert_eq!(flat.text, "See John 3:16 now");
    let (start, end) = flat.source_range(4..13).expect("from the tree");
    assert_eq!(
        (start.entry, start.path.clone(), start.offset),
        (0, path(&[1, 0]), 0)
    );
    assert_eq!(
        (end.entry, end.path.clone(), end.offset),
        (0, path(&[2]), 7)
    );
    assert_eq!(
        start.path.resolve(&nodes).and_then(|n| n.as_text()),
        Some("John")
    );
    assert_eq!(flat.source(8), None, "collapsed whitespace has no source");
    let at = flat.source(10).expect("from the tree");
    assert_eq!((at.path, at.offset), (path(&[2]), 4));
    let hi = flat.spans_at(4..8).next().expect("a span");
    assert_eq!(
        (hi.kind, &flat.text[hi.range.clone()]),
        (Kind::Highlight, "John")
    );
    assert_eq!(flat.source_range(3..4), None);
    assert_eq!(flat.source_range(0..0), None);
}

#[test]
fn added_text_has_no_source() {
    let opts = TextOptions {
        strongs: true,
        verse_label: Some("[{verse}]".into()),
        ..TextOptions::default()
    };
    let nodes = parse(Markup::Osis, r#"<verse n="1"/><w lemma="strong:G1">x</w>"#);
    let flat = flatten(&nodes, &opts);
    assert_eq!(flat.text, "[1] x [G1]");
    assert_eq!(flat.source(0), None);
    assert!(flat.source(4).is_some());
    assert_eq!(flat.source(7), None);
    // The word's span covers its Strong's list
    let word = flat
        .spans
        .iter()
        .find(|s| s.kind == Kind::Word)
        .expect("a word span");
    assert_eq!(&flat.text[word.range.clone()], "x [G1]");
    // The verse's span covers its label
    let verse = flat
        .spans
        .iter()
        .find(|s| s.kind == Kind::Verse)
        .expect("a verse span");
    assert_eq!(&flat.text[verse.range.clone()], "[1]");
}

#[test]
fn spans_order_and_trimming() {
    let nodes = parse(
        Markup::Osis,
        "<lg><l> first </l><l>second</l></lg><milestone/>",
    );
    let flat = flatten(&nodes, &TextOptions::default());
    assert_eq!(flat.text, "first\nsecond");
    let spans: Vec<(Kind, &str, String)> = flat
        .spans
        .iter()
        .map(|s| (s.kind, &flat.text[s.range.clone()], s.path.to_string()))
        .collect();
    assert_eq!(
        spans,
        [
            (Kind::Line, "first", "/0/0".into()),
            (Kind::Line, "second", "/0/1".into()),
            (Kind::LineGroup, "first\nsecond", "/0".into()),
            (Kind::Milestone, "", "/1".into()),
        ]
    );
    // `spans_at` keeps that order
    let at: Vec<Kind> = flat.spans_at(0..2).map(|s| s.kind).collect();
    assert_eq!(at, [Kind::Line, Kind::LineGroup]);
}

#[test]
fn last_before_finds_the_verse() {
    let usx = r#"<usx><chapter number="3" style="c" sid="JHN 3"/><para style="p"><verse number="16" style="v" sid="JHN 3:16"/>For God so loved the world.<verse eid="JHN 3:16"/><verse number="17" style="v" sid="JHN 3:17"/> For God sent</para></usx>"#;
    let nodes = parse(Markup::Usx, usx);
    let flat = flatten(&nodes, &TextOptions::default());
    let at = flat.text.find("world").expect("in the text");
    let verse = flat.last_before(Kind::Verse, at).expect("a verse");
    let marker = verse
        .path
        .resolve(&nodes)
        .and_then(|n| n.as_element())
        .expect("an element");
    assert_eq!(marker.attr("sid"), Some("JHN 3:16"));
    let at = flat.text.find("sent").expect("in the text");
    let verse = flat.last_before(Kind::Verse, at).expect("a verse");
    let marker = verse
        .path
        .resolve(&nodes)
        .and_then(|n| n.as_element())
        .expect("an element");
    assert_eq!(marker.attr("sid"), Some("JHN 3:17"));
    assert!(flat.last_before(Kind::Note, at).is_none());
}

#[test]
fn entries_and_their_ranges() {
    let mut flat = Flattener::new(TextOptions::default());
    flat.push(&parse(Markup::Osis, "In the beginning "))
        .push(&parse(Markup::Osis, "<note>dropped</note>"))
        .push(&parse(Markup::Osis, " was <q who=\"Jesus\">the Word"))
        .push(&parse(Markup::Osis, "</q>"));
    let flat = flat.finish();
    assert_eq!(flat.text, "In the beginning was the Word");
    let entries: Vec<&str> = flat.entries.iter().map(|r| &flat.text[r.clone()]).collect();
    assert_eq!(entries, ["In the beginning", "", "was the Word", ""]);
    assert_eq!(flat.entries[1], 17..17);
    assert_eq!(flat.entries[3], 29..29);
}

#[test]
fn entries_are_separated_by_a_space() {
    let mut flat = Flattener::new(TextOptions::default());
    flat.push(&parse(Markup::Osis, "one"))
        .push(&parse(Markup::Osis, "two"));
    assert_eq!(flat.finish().text, "one two");
    let empty = Flattener::new(TextOptions::default()).finish();
    assert_eq!(empty.text, "");
    assert!(empty.entries.is_empty() && empty.spans.is_empty());
}

#[test]
fn words_of_jesus_across_entries() {
    // Unclosed in one entry, closed by an orphan end in the next
    let mut flat = Flattener::new(TextOptions::default());
    flat.push(&parse(Markup::Osis, r#"He said, <q who="Jesus">Follow"#));
    flat.push(&parse(Markup::Osis, "me.</q> And they did."));
    let flat = flat.finish();
    assert_eq!(flat.text, "He said, Follow me. And they did.");
    let red: Vec<(&str, usize, String)> = flat
        .spans
        .iter()
        .filter(|s| s.kind == Kind::WordsOfJesus)
        .map(|s| (&flat.text[s.range.clone()], s.entry, s.path.to_string()))
        .collect();
    assert_eq!(red, [("Follow", 0, "/1".into()), ("me.", 1, String::new())]);
}

#[test]
fn words_of_jesus_milestones_match_by_id() {
    let a = parse(
        Markup::Osis,
        r#"He said, <q who="Jesus" sID="q1"/>I am <q sID="q2"/>the way<q eID="q2"/>"#,
    );
    let b = parse(Markup::Osis, r#"and the life.<q eID="q1"/> Then"#);
    let mut flat = Flattener::new(TextOptions::default());
    flat.push(&a).push(&b);
    let flat = flat.finish();
    let red: Vec<&str> = flat
        .spans
        .iter()
        .filter(|s| s.kind == Kind::WordsOfJesus)
        .map(|s| &flat.text[s.range.clone()])
        .collect();
    assert_eq!(red, ["I am the way", "and the life."]);
    // Unmatched ids leave them open
    let nodes = parse(
        Markup::Osis,
        r#"a<q who="Jesus" sID="q1"/>b<q eID="q9"/>c<q eID="q1"/>d"#,
    );
    let flat = flatten(&nodes, &TextOptions::default());
    let red: Vec<&str> = flat
        .spans
        .iter()
        .filter(|s| s.kind == Kind::WordsOfJesus)
        .map(|s| &flat.text[s.range.clone()])
        .collect();
    assert_eq!(red, ["bc"]);
}

#[test]
fn templates_across_entries() {
    let opts = TextOptions {
        templates: [(Kind::WordsOfJesus, "“{text}”".to_string())]
            .into_iter()
            .collect(),
        ..TextOptions::default()
    };
    let mut flat = Flattener::new(opts.clone());
    flat.push(&parse(
        Markup::Osis,
        r#"He said, <q who="Jesus" sID="q1"/>Follow"#,
    ))
    .push(&parse(Markup::Osis, r#"me.<q eID="q1"/> And they did."#));
    assert_eq!(flat.finish().text, "He said, “Follow me.” And they did.");
    let mut flat = Flattener::new(opts);
    flat.push(&parse(Markup::Osis, r#"<q who="Jesus">never closed"#));
    assert_eq!(flat.finish().text, "“never closed”");
}

#[test]
fn poetry_lines_stanzas_and_indents() {
    let usx = r#"<usx version="3.0"><para style="p">Prose, then a psalm:</para><para style="q1"><verse number="1" style="v" sid="PSA 1:1"/>Blessed is the man</para><para style="q2">who does not walk</para><para style="b"/><para style="q">Not so</para><para style="qr">Selah</para><para style="pi">Indented prose</para></usx>"#;
    let nodes = parse(Markup::Usx, usx);
    let opts = TextOptions {
        verse_label: Some("[{verse}]".into()),
        ..TextOptions::default()
    };
    // Each line on its own line, and a blank line at the stanza break
    assert_eq!(
        flatten(&nodes, &opts).text,
        "Prose, then a psalm:\n[1] Blessed is the man\nwho does not walk\n\nNot so\nSelah\nIndented prose"
    );
    // Spaces by marker (`q` is `q1`, `pi` is `pi1`); the spans leave them out
    let opts = TextOptions {
        indents: [("q1", 2), ("q2", 4), ("qr", 6), ("pi1", 1)]
            .into_iter()
            .map(|(k, n)| (k.to_string(), n))
            .collect(),
        ..opts
    };
    let flat = flatten(&nodes, &opts);
    assert_eq!(
        flat.text,
        "Prose, then a psalm:\n  [1] Blessed is the man\n    who does not walk\n\n  Not so\n      Selah\n Indented prose"
    );
    let lines: Vec<&str> = (flat.spans.iter())
        .filter(|s| s.kind == Kind::Line)
        .map(|s| &flat.text[s.range.clone()])
        .collect();
    assert_eq!(
        lines,
        [
            "[1] Blessed is the man",
            "who does not walk",
            "Not so",
            "Selah"
        ]
    );
    // OSIS lines by level; the first line of the text isn't indented
    let osis = parse(
        Markup::Osis,
        r#"<lg><l level="1">The LORD is my shepherd;</l><l level="2">I shall not want.</l></lg>"#,
    );
    let opts = TextOptions {
        indents: [("q2".to_string(), 3)].into(),
        ..TextOptions::default()
    };
    assert_eq!(
        flatten(&osis, &opts).text,
        "The LORD is my shepherd;\n   I shall not want."
    );
}

#[test]
fn verse_lines_and_paragraph_gaps() {
    let usx = r#"<usx version="3.0"><chapter number="1" style="c" sid="JHN 1"/><para style="s1">The Word</para><para style="p"><verse number="1" style="v" sid="JHN 1:1"/>In the beginning was the Word. <verse number="2" style="v" sid="JHN 1:2"/>He was with God.</para><para style="p"><verse number="3" style="v" sid="JHN 1:3"/>All things were made.</para><para style="q2"><verse number="4" style="v" sid="JHN 1:4"/>In him was life,</para><para style="q2">and the life <verse number="5" style="v" sid="JHN 1:5"/>was light.</para><para style="p">The end.</para></usx>"#;
    let nodes = parse(Markup::Usx, usx);
    let base = TextOptions {
        filter: logoxide_markup::Filter::all(),
        verse_label: Some("[{verse}]".into()),
        chapter_label: Some("Chapter {chapter}".into()),
        indents: [("q2".to_string(), 2)].into(),
        ..TextOptions::default()
    };
    assert_eq!(
        flatten(&nodes, &base).text,
        "Chapter 1\nThe Word\n[1] In the beginning was the Word. [2] He was with God.\n[3] All things were made.\n  [4] In him was life,\n  and the life [5] was light.\nThe end."
    );
    // Each verse on a line of its own; one that starts a poetic line keeps its indent
    let verses = TextOptions {
        verse_lines: true,
        ..base.clone()
    };
    assert_eq!(
        flatten(&nodes, &verses).text,
        "Chapter 1\nThe Word\n[1] In the beginning was the Word.\n[2] He was with God.\n[3] All things were made.\n  [4] In him was life,\n  and the life\n[5] was light.\nThe end."
    );
    // A blank line between paragraphs and around poetry, not under the heading
    let gaps = TextOptions {
        paragraph_gaps: true,
        ..base.clone()
    };
    assert_eq!(
        flatten(&nodes, &gaps).text,
        "Chapter 1\nThe Word\n[1] In the beginning was the Word. [2] He was with God.\n\n[3] All things were made.\n\n  [4] In him was life,\n  and the life [5] was light.\n\nThe end."
    );
    // Both, and neither without line breaks
    let both = TextOptions {
        verse_lines: true,
        paragraph_gaps: true,
        ..base.clone()
    };
    assert_eq!(
        flatten(&nodes, &both).text,
        "Chapter 1\nThe Word\n[1] In the beginning was the Word.\n[2] He was with God.\n\n[3] All things were made.\n\n  [4] In him was life,\n  and the life\n[5] was light.\n\nThe end."
    );
    let flat = TextOptions {
        line_breaks: false,
        ..both
    };
    assert_eq!(
        flatten(&nodes, &flat).text,
        "Chapter 1 The Word [1] In the beginning was the Word. [2] He was with God. [3] All things were made. [4] In him was life, and the life [5] was light. The end."
    );
    // OSIS verses as containers and paragraph milestones
    let osis = parse(
        Markup::Osis,
        r#"<div type="paragraph" sID="p1"/><verse osisID="Gen.1.1">In the beginning.</verse><verse osisID="Gen.1.2">And the earth.</verse><div type="paragraph" eID="p1"/><div type="paragraph" sID="p2"/><verse osisID="Gen.1.3">Let there be light.</verse><div type="paragraph" eID="p2"/>"#,
    );
    let both = TextOptions {
        verse_lines: true,
        paragraph_gaps: true,
        verse_label: Some("{verse}".into()),
        ..TextOptions::default()
    };
    assert_eq!(
        flatten(&osis, &both).text,
        "1 In the beginning.\n2 And the earth.\n\n3 Let there be light."
    );
}
