/*!
Searching the workspace for references: go-to-references and the search command. A search is made
from what the server holds ([`Search`], its own copy of the documents and settings, sharing the
cache), so it can run on another thread while the server answers other requests; it stops between
files when its request is cancelled or its time is up ([`Stop`])
*/

use super::*;
use crate::workspace::Stop;

/// A workspace search, with what it reads as the server had it when the search was made
pub struct Search {
    mode: SearchMode,
    passage: String,
    matcher: Arc<BibleMatcher>,
    roots: Vec<PathBuf>,
    documents: HashMap<Uri, String>,
    extensions: Vec<String>,
    books: Option<Arc<workspace::BookSearch>>,
    cache: Arc<workspace::Cache>,
    /// The Bibles' folders, whose books aren't searched
    bibles: Vec<PathBuf>,
}

impl Search {
    /// The references in the workspace it keeps, in file order, and whether it searched every
    /// file (not if `stop` was reached first)
    pub fn run(&self, stop: &Stop) -> Result<(Vec<Location>, bool), String> {
        let mut filter = BibleFilter::new(self.matcher.data().clone());
        let passage = self.passage.as_str();
        match self.mode {
            SearchMode::ExplicitOverlap => filter.filter_explicit_overlap(passage),
            SearchMode::AnyOverlap => filter.filter_any_overlap(passage),
            SearchMode::ExactOverlap => filter.filter_exact_overlap(passage),
            SearchMode::Inside => filter.filter_inside(passage),
        }
        .map_err(|e| e.to_string())?;
        let query = filter.create_matcher();
        let bibles: Vec<&Path> = self.bibles.iter().map(PathBuf::as_path).collect();
        let (mut files, complete) = workspace::references(
            &self.roots,
            &self.documents,
            workspace::Scope {
                extensions: &self.extensions,
                books: self.books.as_deref(),
            },
            &self.matcher,
            &self.cache,
            &bibles,
            stop,
        );
        files.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
        let locations = files
            .iter()
            .flat_map(|(uri, found)| {
                found
                    .iter()
                    .filter(|f| query.keeps(&f.passage))
                    .map(|f| Location::new(uri.clone(), f.range))
            })
            .collect();
        Ok((locations, complete))
    }
}

impl Server {
    /// Go to references: every reference in the workspace that is exactly the one under the
    /// cursor (`--exact-overlap`), however it is written (the server runs
    /// [`Server::references_search`] on another thread)
    #[cfg(test)]
    pub fn references(&self, params: ReferenceParams) -> Option<Vec<Location>> {
        let (locations, _) = self.references_search(params)?.run(&Stop::default()).ok()?;
        Some(locations)
    }

    /// [`Server::references`]' search, to run on another thread; `None` if there is no
    /// reference (or verse) at the position
    pub fn references_search(&self, params: ReferenceParams) -> Option<Search> {
        let position = params.text_document_position;
        let uri = &position.text_document.uri;
        let Some((found, _)) = self.reference_at(uri, position.position) else {
            // In a whole book, the verse under the cursor
            let (mode, passage) = self.verse_at(uri, position.position)?;
            return Some(self.search_for(mode, passage));
        };
        let passage = self.parseable(&found.reference.psg);
        Some(self.search_for(SearchMode::ExactOverlap, passage))
    }

    /**
    In a whole book, how to search for what is at a position:
    - In a verse, references that name it (explicit overlap: `John 3` doesn't count)
    - On a chapter's label, references that share any verse with the chapter
    */
    pub(super) fn verse_at(&self, uri: &Uri, position: Position) -> Option<(SearchMode, String)> {
        let (bible, book) = self.book_document(uri)?;
        let rendered = bible.render(book)?;
        let index = LineIndex::new(&rendered.text);
        let cursor = index
            .offset_of_utf16(position.line as usize, position.character as usize)
            .unwrap_or(rendered.text.len());
        let name = bible.book_name(book)?;
        match rendered.part_at(cursor)? {
            Part::Verse {
                chapter,
                verse,
                last,
            } => {
                let passage = match last > verse {
                    true => format!("{name} {chapter}:{verse}-{last}"),
                    false => format!("{name} {chapter}:{verse}"),
                };
                Some((SearchMode::ExplicitOverlap, passage))
            }
            Part::Chapter(chapter) => Some((SearchMode::AnyOverlap, format!("{name} {chapter}"))),
        }
    }

    /// Runs [`SEARCH_COMMAND`], returning the locations it finds (the server runs
    /// [`Server::command_search`] on another thread)
    #[cfg(test)]
    pub fn execute_command(&self, params: ExecuteCommandParams) -> Result<Vec<Location>, String> {
        let (locations, _) = self.command_search(params)?.run(&Stop::default())?;
        Ok(locations)
    }

    /// [`Server::execute_command`]'s search, to run on another thread
    pub fn command_search(&self, params: ExecuteCommandParams) -> Result<Search, String> {
        if params.command != SEARCH_COMMAND {
            return Err(format!("unknown command {}", params.command));
        }
        let argument = params
            .arguments
            .into_iter()
            .next()
            .ok_or("logoxide.search needs { mode, passage }")?;
        let arguments: SearchArguments =
            serde_json::from_value(argument).map_err(|e| e.to_string())?;
        Ok(self.search_for(arguments.mode, arguments.passage))
    }

    /// The references in the workspace that a search keeps, in file order
    #[cfg(test)]
    pub fn search(&self, mode: SearchMode, passage: &str) -> Result<Vec<Location>, String> {
        let (locations, _) = self.search_until(mode, passage, &Stop::default())?;
        Ok(locations)
    }

    /// [`Server::search`], until `stop` is reached; also whether it searched every file
    pub(super) fn search_until(
        &self,
        mode: SearchMode,
        passage: &str,
        stop: &Stop,
    ) -> Result<(Vec<Location>, bool), String> {
        self.search_for(mode, passage.to_string()).run(stop)
    }

    /// A search with what it needs from the server
    fn search_for(&self, mode: SearchMode, passage: String) -> Search {
        Search {
            mode,
            passage,
            matcher: self.matcher.clone(),
            // The workspace folders, then `search-folders` (a file in both is searched once)
            roots: (self.roots.iter())
                .chain(&self.search_folders)
                .cloned()
                .collect(),
            documents: self.documents.clone(),
            extensions: self.extensions.clone(),
            books: self.books.clone(),
            cache: self.cache.clone(),
            bibles: (self.bibles.folders().into_iter())
                .map(Path::to_path_buf)
                .collect(),
        }
    }
}
