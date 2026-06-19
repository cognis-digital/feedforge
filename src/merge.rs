//! Feed merging: combine multiple feeds, de-duplicate, and sort newest-first.
//!
//! De-duplication uses each item's [`Item::dedupe_key`] (guid, else link).
//! When duplicates collide the first-seen item wins, except a later duplicate
//! that carries a parseable date replaces an earlier one that does not — so we
//! never lose ordering information to an undated copy.

use crate::build::build_feed;
use crate::date::DateTime;
use crate::model::{Channel, Feed, FeedFormat, Item};
use crate::parse::parse_feed;
use crate::Result;
use std::collections::HashMap;

/// Sort key for an item: parseable date as Unix seconds, else `i64::MIN` so
/// undated items sort to the bottom (oldest) of a newest-first list.
fn sort_key(item: &Item) -> i64 {
    item.pub_date
        .as_deref()
        .and_then(DateTime::parse_any)
        .map(|d| d.to_unix())
        .unwrap_or(i64::MIN)
}

/// Merge several already-parsed feeds into one, de-duplicated and sorted
/// newest-first. The merged channel borrows metadata from the first feed that
/// supplies it.
pub fn merge_parsed(feeds: &[Feed]) -> Feed {
    let mut channel = Channel::default();
    for f in feeds {
        if channel.title.is_empty() {
            channel.title = f.channel.title.clone();
        }
        if channel.link.is_empty() {
            channel.link = f.channel.link.clone();
        }
        if channel.description.is_empty() {
            channel.description = f.channel.description.clone();
        }
        if channel.language.is_none() {
            channel.language = f.channel.language.clone();
        }
    }
    if channel.title.is_empty() {
        channel.title = "Merged feed".to_string();
    }

    // De-duplicate, preferring a dated copy over an undated one.
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut items: Vec<Item> = Vec::new();
    for f in feeds {
        for item in &f.items {
            let key = item.dedupe_key();
            if key.is_empty() {
                items.push(item.clone());
                continue;
            }
            match index.get(&key).copied() {
                None => {
                    index.insert(key, items.len());
                    items.push(item.clone());
                }
                Some(existing) => {
                    let existing_dated = sort_key(&items[existing]) != i64::MIN;
                    let new_dated = sort_key(item) != i64::MIN;
                    if new_dated && !existing_dated {
                        items[existing] = item.clone();
                    }
                }
            }
        }
    }

    // Newest first; stable so equal-dated items keep insertion order.
    items.sort_by_key(|i| std::cmp::Reverse(sort_key(i)));

    channel.last_build_date = items
        .iter()
        .filter_map(|i| i.pub_date.as_deref().and_then(DateTime::parse_any))
        .map(|d| d.to_unix())
        .max()
        .map(|u| DateTime::from_unix(u).to_rfc822());

    Feed { channel, items }
}

/// Parse each XML document and merge them.
pub fn merge_feeds(xml_docs: &[String]) -> Result<Feed> {
    let mut feeds = Vec::with_capacity(xml_docs.len());
    for doc in xml_docs {
        feeds.push(parse_feed(doc)?);
    }
    Ok(merge_parsed(&feeds))
}

/// Read feed files from disk, merge them, and render the result.
pub fn merge_files(paths: &[String], format: FeedFormat) -> Result<String> {
    let mut docs = Vec::with_capacity(paths.len());
    for p in paths {
        docs.push(std::fs::read_to_string(p)?);
    }
    let merged = merge_feeds(&docs)?;
    build_feed(&merged, format)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str, link: &str, date: Option<&str>) -> Item {
        Item {
            title: title.into(),
            link: link.into(),
            description: None,
            guid: None,
            pub_date: date.map(|s| s.to_string()),
            author: None,
        }
    }

    #[test]
    fn dedupes_by_link() {
        let a = Feed {
            channel: Default::default(),
            items: vec![item("A", "https://e/1", Some("2026-06-01T00:00:00Z"))],
        };
        let b = Feed {
            channel: Default::default(),
            items: vec![
                item("A-dup", "https://e/1", Some("2026-06-01T00:00:00Z")),
                item("B", "https://e/2", Some("2026-06-05T00:00:00Z")),
            ],
        };
        let merged = merge_parsed(&[a, b]);
        assert_eq!(merged.items.len(), 2);
    }

    #[test]
    fn sorts_newest_first() {
        let a = Feed {
            channel: Default::default(),
            items: vec![
                item("old", "https://e/1", Some("2026-06-01T00:00:00Z")),
                item("new", "https://e/2", Some("2026-06-10T00:00:00Z")),
                item("mid", "https://e/3", Some("2026-06-05T00:00:00Z")),
            ],
        };
        let merged = merge_parsed(&[a]);
        let titles: Vec<&str> = merged.items.iter().map(|i| i.title.as_str()).collect();
        assert_eq!(titles, vec!["new", "mid", "old"]);
    }

    #[test]
    fn dedupe_prefers_dated_copy() {
        let a = Feed {
            channel: Default::default(),
            items: vec![item("undated", "https://e/1", None)],
        };
        let b = Feed {
            channel: Default::default(),
            items: vec![item("dated", "https://e/1", Some("2026-06-09T00:00:00Z"))],
        };
        let merged = merge_parsed(&[a, b]);
        assert_eq!(merged.items.len(), 1);
        assert_eq!(merged.items[0].title, "dated");
    }

    #[test]
    fn dedupes_by_guid_across_links() {
        let mut i1 = item("A", "https://e/1", Some("2026-06-01T00:00:00Z"));
        i1.guid = Some("shared".into());
        let mut i2 = item(
            "A-elsewhere",
            "https://mirror/1",
            Some("2026-06-01T00:00:00Z"),
        );
        i2.guid = Some("shared".into());
        let merged = merge_parsed(&[
            Feed {
                channel: Default::default(),
                items: vec![i1],
            },
            Feed {
                channel: Default::default(),
                items: vec![i2],
            },
        ]);
        assert_eq!(merged.items.len(), 1);
    }
}
