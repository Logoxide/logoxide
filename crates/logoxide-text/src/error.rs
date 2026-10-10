//! What goes wrong reading text: a book that can't be read, a file that isn't there, a map or
//! timings file that doesn't parse.

#[derive(Debug, thiserror::Error)]
pub enum TextError {
    /// A book, or the file holding it, that can't be read as one
    #[error("book {book}: {message}")]
    Book { book: String, message: String },
    /// A file that isn't there, or a book the text lacks
    #[error("not found: {0}")]
    NotFound(String),
    /// A file that can't be read
    #[error("{0}")]
    Files(String),
    /// A versification map's lines
    #[error("versification: {0}")]
    Versification(String),
    /// A timings file
    #[error("timings: {0}")]
    Timings(String),
    /// An EPUB or other archive, or the metadata in it
    #[error("{0}")]
    Archive(String),
}

pub type Result<T> = std::result::Result<T, TextError>;
