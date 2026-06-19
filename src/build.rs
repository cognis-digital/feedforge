//! Feed building: render a [`Feed`] to RSS 2.0 or Atom 1.0 XML.
//!
//! Emission is hand-written with explicit escaping (see [`crate::escape`]) so
//! `build` carries no XML-writer dependency. Dates supplied on items are
//! normalized into the target format's conventional shape; unparseable dates
//! are passed through verbatim so we never silently drop caller data.

use crate::date::DateTime;
use crate::escape::xml_escape;
use crate::model::{Feed, FeedFormat, Item};
use crate::Result;

/// Render a [`Feed`] to a complete XML document string in the given format.
pub fn build_feed(feed: &Feed, format: FeedFormat) -> Result<String> {
    match format {
        FeedFormat::Rss => Ok(build_rss(feed)),
        FeedFormat::Atom => Ok(build_atom(feed)),
    }
}

/// The most-recent item date, or `None` if nothing parseable is present.
fn latest_item_unix(feed: &Feed) -> Option<i64> {
    feed.items
        .iter()
        .filter_map(|i| i.pub_date.as_deref().and_then(DateTime::parse_any))
        .map(|d| d.to_unix())
        .max()
}

fn build_rss(feed: &Feed) -> String {
    let ch = &feed.channel;
    let mut out = String::with_capacity(1024 + feed.items.len() * 256);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<rss version=\"2.0\">\n");
    out.push_str("  <channel>\n");
    push_tag(&mut out, 4, "title", &ch.title);
    push_tag(&mut out, 4, "link", &ch.link);
    push_tag(&mut out, 4, "description", &ch.description);
    if let Some(lang) = &ch.language {
        push_tag(&mut out, 4, "language", lang);
    }
    let build_date = ch
        .last_build_date
        .as_deref()
        .and_then(DateTime::parse_any)
        .map(|d| d.to_rfc822())
        .or_else(|| latest_item_unix(feed).map(|u| DateTime::from_unix(u).to_rfc822()));
    if let Some(bd) = build_date {
        push_tag(&mut out, 4, "lastBuildDate", &bd);
    }
    push_tag(&mut out, 4, "generator", "feedforge");

    for item in &feed.items {
        out.push_str("    <item>\n");
        push_tag(&mut out, 6, "title", &item.title);
        if !item.link.is_empty() {
            push_tag(&mut out, 6, "link", &item.link);
        }
        if let Some(desc) = &item.description {
            push_tag(&mut out, 6, "description", desc);
        }
        if let Some(author) = &item.author {
            push_tag(&mut out, 6, "author", author);
        }
        if let Some(date) = normalize_date(item, FeedFormat::Rss) {
            push_tag(&mut out, 6, "pubDate", &date);
        }
        let guid = item.guid.clone().unwrap_or_else(|| item.link.clone());
        if !guid.is_empty() {
            out.push_str("      <guid isPermaLink=\"false\">");
            out.push_str(&xml_escape(&guid));
            out.push_str("</guid>\n");
        }
        out.push_str("    </item>\n");
    }

    out.push_str("  </channel>\n");
    out.push_str("</rss>\n");
    out
}

fn build_atom(feed: &Feed) -> String {
    let ch = &feed.channel;
    let mut out = String::with_capacity(1024 + feed.items.len() * 256);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<feed xmlns=\"http://www.w3.org/2005/Atom\">\n");
    push_tag(&mut out, 2, "title", &ch.title);
    if !ch.description.is_empty() {
        push_tag(&mut out, 2, "subtitle", &ch.description);
    }
    if !ch.link.is_empty() {
        out.push_str("  <link href=\"");
        out.push_str(&xml_escape(&ch.link));
        out.push_str("\"/>\n");
    }
    // Atom requires a stable feed id; use the channel link or the title.
    let feed_id = if ch.link.is_empty() {
        ch.title.clone()
    } else {
        ch.link.clone()
    };
    push_tag(&mut out, 2, "id", &feed_id);

    let updated = ch
        .last_build_date
        .as_deref()
        .and_then(DateTime::parse_any)
        .map(|d| d.to_rfc3339())
        .or_else(|| latest_item_unix(feed).map(|u| DateTime::from_unix(u).to_rfc3339()));
    if let Some(u) = updated {
        push_tag(&mut out, 2, "updated", &u);
    }
    push_tag(&mut out, 2, "generator", "feedforge");

    for item in &feed.items {
        out.push_str("  <entry>\n");
        push_tag(&mut out, 4, "title", &item.title);
        if !item.link.is_empty() {
            out.push_str("    <link href=\"");
            out.push_str(&xml_escape(&item.link));
            out.push_str("\"/>\n");
        }
        let id = item.guid.clone().unwrap_or_else(|| item.link.clone());
        if !id.is_empty() {
            push_tag(&mut out, 4, "id", &id);
        }
        if let Some(date) = normalize_date(item, FeedFormat::Atom) {
            push_tag(&mut out, 4, "updated", &date);
            push_tag(&mut out, 4, "published", &date);
        }
        if let Some(author) = &item.author {
            out.push_str("    <author>\n");
            push_tag(&mut out, 6, "name", author);
            out.push_str("    </author>\n");
        }
        if let Some(desc) = &item.description {
            push_tag(&mut out, 4, "summary", desc);
        }
        out.push_str("  </entry>\n");
    }

    out.push_str("</feed>\n");
    out
}

/// Normalize an item's date into the target format, passing through verbatim
/// (but escaped at emission) if it cannot be parsed.
fn normalize_date(item: &Item, format: FeedFormat) -> Option<String> {
    let raw = item.pub_date.as_deref()?;
    if raw.trim().is_empty() {
        return None;
    }
    match DateTime::parse_any(raw) {
        Some(dt) => Some(match format {
            FeedFormat::Rss => dt.to_rfc822(),
            FeedFormat::Atom => dt.to_rfc3339(),
        }),
        None => Some(raw.to_string()),
    }
}

/// Append `<tag>escaped(value)</tag>` at the given indent, skipping empty tags.
fn push_tag(out: &mut String, indent: usize, tag: &str, value: &str) {
    if value.is_empty() {
        return;
    }
    for _ in 0..indent {
        out.push(' ');
    }
    out.push('<');
    out.push_str(tag);
    out.push('>');
    out.push_str(&xml_escape(value));
    out.push_str("</");
    out.push_str(tag);
    out.push_str(">\n");
}

/// Parse the items-JSON input into a [`Feed`].
///
/// Two shapes are accepted:
///   * a full object `{ "title": ..., "items": [ ... ] }`, or
///   * a bare array `[ {item}, {item} ]` (channel metadata then empty).
pub fn feed_from_json(json: &str) -> Result<Feed> {
    let trimmed = json.trim_start();
    if trimmed.starts_with('[') {
        let items: Vec<Item> = serde_json::from_str(json)?;
        Ok(Feed {
            channel: Default::default(),
            items,
        })
    } else {
        let feed: Feed = serde_json::from_str(json)?;
        Ok(feed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Channel;

    fn sample_feed() -> Feed {
        Feed {
            channel: Channel {
                title: "Test & Demo".into(),
                link: "https://example.com/".into(),
                description: "A <demo> feed".into(),
                last_build_date: None,
                language: Some("en-us".into()),
            },
            items: vec![Item {
                title: "First <item>".into(),
                link: "https://example.com/1".into(),
                description: Some("Body & more".into()),
                guid: None,
                pub_date: Some("2026-06-09T14:30:00Z".into()),
                author: None,
            }],
        }
    }

    #[test]
    fn rss_escapes_and_contains_required_fields() {
        let xml = build_feed(&sample_feed(), FeedFormat::Rss).unwrap();
        assert!(xml.contains("<rss version=\"2.0\">"));
        assert!(xml.contains("<title>Test &amp; Demo</title>"));
        assert!(xml.contains("A &lt;demo&gt; feed"));
        assert!(xml.contains("<pubDate>Tue, 09 Jun 2026 14:30:00 GMT</pubDate>"));
        // guid falls back to link
        assert!(xml.contains("https://example.com/1</guid>"));
    }

    #[test]
    fn atom_has_namespace_and_id() {
        let xml = build_feed(&sample_feed(), FeedFormat::Atom).unwrap();
        assert!(xml.contains("xmlns=\"http://www.w3.org/2005/Atom\""));
        assert!(xml.contains("<id>https://example.com/</id>"));
        assert!(xml.contains("<updated>2026-06-09T14:30:00Z</updated>"));
    }

    #[test]
    fn json_array_form() {
        let feed = feed_from_json(r#"[{"title":"x","link":"https://e/1"}]"#).unwrap();
        assert_eq!(feed.items.len(), 1);
    }

    #[test]
    fn json_object_form_with_alias() {
        let feed = feed_from_json(
            r#"{"title":"T","link":"https://e","description":"d",
                "items":[{"title":"a","link":"https://e/a","pubDate":"2026-06-09T00:00:00Z"}]}"#,
        )
        .unwrap();
        assert_eq!(feed.channel.title, "T");
        assert_eq!(
            feed.items[0].pub_date.as_deref(),
            Some("2026-06-09T00:00:00Z")
        );
    }
}
