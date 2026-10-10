/*!
Bibles in one file (OSIS, ThML), or a book a file: which books each file has, found by scanning
its verse tags, and each book's part of the parsed tree.

A book's part is the largest element holding its verses and no other book's (its book division),
or, where books aren't elements of their own (OSIS book milestones), the run of nodes from its
first verse or chapter marker to its last.
*/

use std::{collections::BTreeMap, sync::Mutex};

use topos_bible::data::books::BookId;
use topos_bible_markup::{Element, Kind, Markup, Node, parse, verses::verses};

use super::{Book, SkipReason, Skipped};
use crate::{
    error::{LogoxideError, Result},
    storage::{Storage, join},
};

/**
The files of a format's folder whose extension is one of `extensions`, the books each has, by
`books_in` from the file's text (the books it names that topos knows, and the names of those it
doesn't), and what was passed over: books topos doesn't know, and files that name none. A book in
two files is an error naming both
*/
pub(crate) fn whole_files(
    storage: &dyn Storage,
    dir: &str,
    extensions: &[&str],
    books_in: impl Fn(&str) -> Scanned,
) -> Result<(BTreeMap<BookId, String>, Vec<Skipped>)> {
    let mut files: BTreeMap<BookId, String> = BTreeMap::new();
    let mut skipped = vec![];
    let mut names = storage.list(dir)?;
    names.sort();
    for name in names {
        let wanted = name
            .rsplit_once('.')
            .is_some_and(|(_, ext)| extensions.iter().any(|e| ext.eq_ignore_ascii_case(e)));
        if !wanted {
            continue;
        }
        let path = join(dir, &name);
        let Scanned {
            mut books,
            unknown,
            lettered,
        } = books_in(&storage.read_string(&path)?);
        for book in lettered {
            let (path, reason) = (path.clone(), SkipReason::LetteredChapter);
            skipped.push(Skipped { path, book, reason });
        }
        if books.is_empty() && unknown.is_empty() {
            // A book a file, its markers naming no book: the file's name says which
            let stem = name
                .rsplit_once('.')
                .map_or(name.as_str(), |(stem, _)| stem);
            books.extend(super::code_from_name(stem));
            if books.is_empty() && !skipped.iter().any(|s| s.path == path) {
                skipped.push(Skipped {
                    path: path.clone(),
                    book: String::new(),
                    reason: SkipReason::NoBook,
                });
            }
        }
        for book in unknown {
            let reason = match super::is_peripheral(&book) {
                true => SkipReason::Peripheral,
                false => SkipReason::Unknown,
            };
            let path = path.clone();
            skipped.push(Skipped { path, book, reason });
        }
        for id in books {
            if let Some(first) = files.get(&id) {
                return Err(super::duplicate(id, first, &path));
            }
            files.insert(id, path.clone());
        }
    }
    Ok((files, skipped))
}

/// The book an OSIS id or reference names (`Gen.1.1`, `Bible:Gen.1.1`, `Gen.1.1.seID.00002`),
/// as it's written, whether or not topos knows it
pub(crate) fn osis_book_name(id: &str) -> Option<&str> {
    let id = id.rsplit_once(':').map_or(id, |(_, rest)| rest);
    id.split(['.', ' ']).next().filter(|book| !book.is_empty())
}

/// The book an OSIS id or reference names, if topos knows it
pub(crate) fn osis_book(id: &str) -> Option<BookId> {
    crate::bible_data().books().search_osis(osis_book_name(id)?)
}

/// An attribute's value in a tag's text (`<verse osisID="Gen.1.1" n="1"/>`)
pub(crate) fn tag_attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let pattern = format!("{name}=\"");
    let mut from = 0;
    while let Some(i) = tag.get(from..)?.find(&pattern) {
        let at = from + i;
        // `osisID` mustn't match inside `osisIDWork`, so it must follow whitespace
        if tag[..at].ends_with(char::is_whitespace) {
            let value = &tag[at + pattern.len()..];
            return Some(&value[..value.find('"')?]);
        }
        from = at + pattern.len();
    }
    None
}

/// The books a file's tags name: those topos knows, in order, the names of the others, and the
/// names of books with lettered chapters (`Esth.A.1`), whose verses aren't read
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Scanned {
    pub(crate) books: Vec<BookId>,
    pub(crate) unknown: Vec<String>,
    pub(crate) lettered: Vec<String>,
}

/// Whether an OSIS reference's chapter is a letter (`Esth.A.1`: Esther's Greek additions, as
/// some Bibles number them)
pub(crate) fn lettered_chapter(reference: &str) -> bool {
    let id = reference
        .rsplit_once(':')
        .map_or(reference, |(_, rest)| rest);
    (id.split(['.', ' ']).nth(1))
        .is_some_and(|c| c.len() == 1 && c.bytes().all(|b| b.is_ascii_alphabetic()))
}

/// The books named by each `<{name} ...>` tag in a file, in order, by `book_of` from the tag (an
/// OSIS reference to its place, `Gen.1.1`, or at least its book's OSIS name)
pub(crate) fn scan_tags(
    text: &str,
    name: &str,
    book_of: impl Fn(&str) -> Option<String>,
) -> Scanned {
    let open = format!("<{name}");
    let mut scanned = Scanned::default();
    let mut rest = text;
    while let Some(i) = rest.find(&open) {
        rest = &rest[i + open.len()..];
        // `<verse ` or `<verse>`, not `<verseX`
        if !rest.starts_with(|c: char| c.is_whitespace() || c == '/' || c == '>') {
            continue;
        }
        let tag = &rest[..rest.find('>').unwrap_or(rest.len())];
        let Some(reference) = book_of(tag) else {
            continue;
        };
        let Some(book) = osis_book_name(&reference).map(str::to_string) else {
            continue;
        };
        if lettered_chapter(&reference) {
            if !scanned.lettered.contains(&book) {
                scanned.lettered.push(book);
            }
            continue;
        }
        match crate::bible_data().books().search_osis(&book) {
            Some(id) if !scanned.books.contains(&id) => scanned.books.push(id),
            None if !scanned.unknown.contains(&book) => scanned.unknown.push(book),
            _ => {}
        }
    }
    scanned
}

/// The OSIS reference a verse or chapter marker names
fn marker_reference(e: &Element) -> Option<&str> {
    ["osisID", "sID", "eID", "osisRef"]
        .iter()
        .find_map(|name| e.attr(name))
}

/**
A tree without its lettered chapters (`Esth.A`), which logoxide doesn't read: a chapter element
naming one goes whole; a verse or chapter milestone naming one goes with what follows it, up to
the next marker of a numbered chapter at its level
*/
pub(crate) fn without_lettered(nodes: Vec<Node>) -> Vec<Node> {
    let mut out = Vec::with_capacity(nodes.len());
    let mut dropping = false;
    for node in nodes {
        let Node::Element(mut e) = node else {
            if !dropping {
                out.push(node);
            }
            continue;
        };
        if matches!(e.kind, Kind::Verse | Kind::Chapter)
            && let Some(reference) = marker_reference(&e)
        {
            if lettered_chapter(reference) {
                // A milestone starts what's left out; an element holds it
                dropping = e.children.is_empty();
                continue;
            }
            dropping = false;
        }
        if dropping {
            continue;
        }
        e.children = without_lettered(std::mem::take(&mut e.children));
        out.push(Node::Element(e));
    }
    out
}

/// Each book's part of a file's tree
type Books = BTreeMap<BookId, Vec<Node>>;

/// Where a book is in a tree
enum Part {
    /// The element at this path
    Element(Vec<usize>),
    /// The children `first..=last` of the element at this path (the top level for none)
    Run(Vec<usize>, usize, usize),
}

/// A parsed file's books, kept while its books are read one by one, so a whole Bible is parsed
/// once instead of once a book
pub(crate) struct Parsed {
    last: Mutex<Option<(String, Books)>>,
}

impl Parsed {
    pub(crate) fn new() -> Self {
        Self {
            last: Mutex::new(None),
        }
    }

    /**
    A book from `path`, parsing it (as `markup`, then `prepare`) if it isn't the file parsed last
    - `book_of` says which book a verse or chapter marker belongs to
    */
    pub(crate) fn book(
        &self,
        storage: &dyn Storage,
        path: &str,
        id: BookId,
        markup: Markup,
        prepare: impl Fn(Vec<Node>) -> Vec<Node>,
        book_of: impl Fn(&Element) -> Option<BookId>,
    ) -> Result<Book> {
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        if last.as_ref().is_none_or(|(p, _)| p != path) {
            let nodes = prepare(parse(markup, &storage.read_string(path)?));
            let nodes = without_lettered(nodes);
            *last = Some((path.to_string(), split_books(nodes, id, &book_of)));
        }
        let nodes = (last.as_ref())
            .and_then(|(_, books)| books.get(&id))
            .cloned()
            .ok_or_else(|| LogoxideError::NotFound(format!("book {} in {path}", id.0)))?;
        let spans = verses(&nodes);
        Ok(Book { id, nodes, spans })
    }
}

/// Each book's part of a tree (see the module's docs); all of it is `whole`'s when no marker
/// names its book
pub(crate) fn split_books(
    mut nodes: Vec<Node>,
    whole: BookId,
    book_of: &dyn Fn(&Element) -> Option<BookId>,
) -> Books {
    let mut markers: Vec<(Vec<usize>, BookId)> = vec![];
    find_markers(&nodes, &mut vec![], book_of, &mut markers);
    if markers.is_empty() {
        return BTreeMap::from([(whole, nodes)]);
    }
    let mut order: Vec<BookId> = vec![];
    for (_, book) in &markers {
        if !order.contains(book) {
            order.push(*book);
        }
    }
    let mut parts: Vec<(BookId, Part)> = vec![];
    for &book in &order {
        let paths: Vec<&Vec<usize>> = (markers.iter())
            .filter(|(_, b)| *b == book)
            .map(|(p, _)| p)
            .collect();
        let Some(mut common) = paths.first().map(|p| p.to_vec()) else {
            continue;
        };
        for path in &paths {
            let same = common.iter().zip(path.iter()).take_while(|(a, b)| a == b);
            common.truncate(same.count());
        }
        // The common ancestor is an element, not a marker itself
        if paths.iter().any(|p| **p == common) {
            common.pop();
        }
        let others = |at: &[usize]| {
            (markers.iter()).any(|(p, b)| *b != book && p.len() > at.len() && p.starts_with(at))
        };
        if common.is_empty() || others(&common) {
            // Books aren't elements here: the run of `common`'s children with its markers
            let depth = common.len();
            let first = paths.iter().filter_map(|p| p.get(depth)).min();
            let last = paths.iter().filter_map(|p| p.get(depth)).max();
            if let (Some(&first), Some(&last)) = (first, last) {
                parts.push((book, Part::Run(common, first, last)));
            }
            continue;
        }
        // The largest element above it that holds no other book's markers
        while common.len() > 1 && !others(&common[..common.len() - 1]) {
            common.pop();
        }
        parts.push((book, Part::Element(common)));
    }
    let mut books = BTreeMap::new();
    for (book, part) in parts {
        let taken = match part {
            Part::Element(path) => take(&mut nodes, &path).into_iter().collect(),
            Part::Run(path, first, last) => match children_mut(&mut nodes, &path) {
                Some(children) => (children.iter_mut().skip(first).take(last + 1 - first))
                    .map(|n| std::mem::replace(n, Node::Text(String::new())))
                    .collect(),
                None => vec![],
            },
        };
        books.entry(book).or_insert(taken);
    }
    books
}

fn find_markers(
    nodes: &[Node],
    path: &mut Vec<usize>,
    book_of: &dyn Fn(&Element) -> Option<BookId>,
    out: &mut Vec<(Vec<usize>, BookId)>,
) {
    for (i, node) in nodes.iter().enumerate() {
        let Node::Element(e) = node else { continue };
        path.push(i);
        if matches!(e.kind, Kind::Verse | Kind::Chapter)
            && let Some(book) = book_of(e)
        {
            out.push((path.clone(), book));
        }
        find_markers(&e.children, path, book_of, out);
        path.pop();
    }
}

/// The children of the element at `path` (the top level for an empty path)
fn children_mut<'a>(nodes: &'a mut Vec<Node>, path: &[usize]) -> Option<&'a mut Vec<Node>> {
    let mut children = nodes;
    for &i in path {
        match children.get_mut(i)? {
            Node::Element(e) => children = &mut e.children,
            Node::Text(_) => return None,
        }
    }
    Some(children)
}

/// The node at `path`, left empty in the tree
fn take(nodes: &mut Vec<Node>, path: &[usize]) -> Option<Node> {
    let (&last, parent) = path.split_last()?;
    let node = children_mut(nodes, parent)?.get_mut(last)?;
    Some(std::mem::replace(node, Node::Text(String::new())))
}

/// The book a verse or chapter marker names in its OSIS id (`osisID`, `sID`, `eID`, `osisRef`)
pub(crate) fn marker_book(e: &Element) -> Option<BookId> {
    ["osisID", "sID", "eID", "osisRef"]
        .iter()
        .find_map(|name| e.attr(name))
        .filter(|reference| !lettered_chapter(reference))
        .and_then(osis_book)
}
