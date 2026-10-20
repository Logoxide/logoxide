//! Translations read from logoxide-text's books
#![cfg(feature = "text")]

use logoxide_quotes::{Engine, Options, Transcript, Translation};
use logoxide_text::{MemoryFiles, source::open_loose};

#[test]
fn a_translation_from_loose_files() {
    let usx = r#"<usx version="3.0"><book code="JHN" style="id"/><chapter number="11" style="c"/>
<para style="p"><verse number="35" style="v"/>Jesus wept.<note caller="+" style="f">The shortest verse</note>
<verse number="36" style="v"/>Then said the Jews, Behold how he loved him!</para></usx>"#;
    let tobit = "\\id TOB\n\\c 1\n\\p\n\\v 1 The book of the words of Tobit, son of Tobiel.\n";
    let files = MemoryFiles::with([("JHN.usx", usx), ("TOB.usfm", tobit)]);
    let source = open_loose(&files, "").expect("opens");
    let kjv = Translation::from_source("KJV", &source).expect("reads");
    let engine = Engine::new([kjv]);
    let transcript = Transcript::from_text(
        "and then it says jesus wept then said the jews behold how he loved him and so",
    );
    let found = engine.searcher().search(&transcript, &Options::default());
    assert_eq!(engine.format(&found.runs[0].passage()), "John 11:35-36");
    // The deuterocanon's books are numbered and named as logoxide-text's are
    let said = Transcript::from_text("the book of the words of tobit son of tobiel");
    let found = engine.searcher().search(&said, &Options::default());
    assert_eq!(
        engine.format(&found.quotations[0].reference.passage()),
        "Tobit 1:1"
    );
}
