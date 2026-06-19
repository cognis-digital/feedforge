//! feedforge — RSS/Atom feed builder, validator, and merger.
//!
//! This crate exposes library functions used by the `feedforge` CLI:
//!
//! * [`build::build_feed`] — render a [`Feed`] to RSS 2.0 or Atom 1.0 XML.
//! * [`validate::validate_feed`] — parse a feed file and check structure/fields.
//! * [`merge::merge_feeds`] — combine feeds, de-duplicate, and sort newest-first.
//!
//! All XML emission for `build` is hand-written and properly escaped (std-only
//! logic). Parsing for `validate`/`merge` uses the lightweight `quick-xml`
//! crate. The items input format is plain JSON via `serde`.
//!
//! Defensive / analytical scope. Original Cognis Digital IP.

pub mod date;
pub mod escape;
pub mod model;

pub mod build;
pub mod merge;
pub mod parse;
pub mod validate;

pub use model::{Channel, Feed, FeedFormat, Item};

use std::fmt;

/// Unified error type for feedforge operations.
#[derive(Debug)]
pub enum Error {
    /// I/O failure reading or writing a file.
    Io(std::io::Error),
    /// Failure decoding the items JSON.
    Json(String),
    /// XML parsing failure (malformed document).
    Xml(String),
    /// The feed parsed but failed structural validation. Holds the list of
    /// human-readable problems found.
    Validation(Vec<String>),
    /// A caller-supplied argument was invalid.
    Invalid(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::Json(m) => write!(f, "json error: {m}"),
            Error::Xml(m) => write!(f, "xml error: {m}"),
            Error::Validation(problems) => {
                writeln!(f, "feed failed validation ({} problem(s)):", problems.len())?;
                for p in problems {
                    writeln!(f, "  - {p}")?;
                }
                Ok(())
            }
            Error::Invalid(m) => write!(f, "invalid input: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e.to_string())
    }
}

/// Convenience result alias.
pub type Result<T> = std::result::Result<T, Error>;
