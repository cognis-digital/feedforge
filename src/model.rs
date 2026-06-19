//! Core data model: feeds, channels, and items.

use serde::{Deserialize, Serialize};

/// Output feed format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedFormat {
    /// RSS 2.0
    Rss,
    /// Atom 1.0
    Atom,
}

impl FeedFormat {
    /// Parse a format from a CLI string (`rss` / `atom`, case-insensitive).
    pub fn parse(s: &str) -> Option<FeedFormat> {
        match s.trim().to_ascii_lowercase().as_str() {
            "rss" | "rss2" | "rss2.0" => Some(FeedFormat::Rss),
            "atom" | "atom1" | "atom1.0" => Some(FeedFormat::Atom),
            _ => None,
        }
    }
}

/// Channel-level metadata for a feed.
///
/// `link` and `description` are required by RSS 2.0; defaults are supplied
/// during building if absent so output is always well-formed, but a `title`
/// should always be provided by the caller.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Channel {
    pub title: String,
    pub link: String,
    pub description: String,
    /// RFC 822 timestamp of the most recent build; filled in if empty.
    #[serde(default)]
    pub last_build_date: Option<String>,
    /// Optional language tag, e.g. `en-us`.
    #[serde(default)]
    pub language: Option<String>,
}

/// A single feed entry.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Item {
    pub title: String,
    pub link: String,
    #[serde(default)]
    pub description: Option<String>,
    /// Globally-unique identifier. If absent on build, the link is used.
    #[serde(default)]
    pub guid: Option<String>,
    /// Publication date. Accepted as RFC 822 or RFC 3339 on input; normalized
    /// to the target format's expectation during build.
    #[serde(default, alias = "pubDate", alias = "published", alias = "date")]
    pub pub_date: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
}

impl Item {
    /// The identity used for de-duplication: prefer guid, fall back to link.
    pub fn dedupe_key(&self) -> String {
        match &self.guid {
            Some(g) if !g.trim().is_empty() => g.trim().to_string(),
            _ => self.link.trim().to_string(),
        }
    }
}

/// A complete feed: channel metadata plus items.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Feed {
    #[serde(flatten)]
    pub channel: Channel,
    #[serde(default)]
    pub items: Vec<Item>,
}
