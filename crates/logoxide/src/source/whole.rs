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

use super::Book;
use crate::{
    error::{LogoxideError, Result},
    storage::{Storage, join},
};

/// The files of a format's folder whose extension is one of `extensions`, and the books each has
/// (the first file with a book keeps it), by `books_in` from the file's text
pub(crate) fn whole_files(
    storage: &dyn Storage,
    dir: &str,
    extensions: &[&str],
    books_in: impl Fn(&str) -> Vec<BookId>,
) -> Result<BTreeMap<BookId, String>> {
    let mut files = BTreeMap::new();
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
        let mut books = books_in(&storage.read_string(&path)?);
        if books.is_empty() {
            // A book a file, its markers naming no book: the file's name says which
            let stem = name
                .rsplit_once('.')
                .map_or(name.as_str(), |(stem, _)| stem);
            books.extend(super::code_from_name(stem));
        }
        for id in books {
            files.entry(id).or_insert_with(|| path.clone());
        }
    }
    Ok(files)
}

/// The book an OSIS id or reference names (`Gen.1.1`, `Bible:Gen.1.1`, `Gen.1.1.seID.00002`)
pub(crate) fn osis_book(id: &str) -> Option<BookId> {
    let id = id.rsplit_once(':').map_or(id, |(_, rest)| rest);
    let book = id.split(['.', ' ']).next()?;
    crate::bible_data().books().search_osis(book)
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

/// The books named by each `<{name} ...>` tag in a file, in order, by `book_of` from the tag
pub(crate) fn scan_tags(
    text: &str,
    name: &str,
    book_of: impl Fn(&str) -> Option<BookId>,
) -> Vec<BookId> {
    let open = format!("<{name}");
    let mut books: Vec<BookId> = vec![];
    let mut rest = text;
    while let Some(i) = rest.find(&open) {
        rest = &rest[i + open.len()..];
        // `<verse ` or `<verse>`, not `<verseX`
        if !rest.starts_with(|c: char| c.is_whitespace() || c == '/' || c == '>') {
            continue;
        }
        let tag = &rest[..rest.find('>').unwrap_or(rest.len())];
        if let Some(book) = book_of(tag)
            && !books.contains(&book)
        {
            books.push(book);
        }
    }
    books
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
        .and_then(osis_book)
}
