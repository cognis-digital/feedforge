//! Feed validation: well-formedness plus required-field and date checks.
//!
//! Validation is a CI gate, so it returns the full list of problems rather
//! than the first failure, letting an operator fix everything in one pass.

use crate::date::DateTime;
use crate::parse::parse_feed;
use crate::{Error, Result};

/// A successful validation summary.
#[derive(Debug, Clone)]
pub struct ValidationReport {
    pub item_count: usize,
    pub format: &'static str,
    /// Non-fatal advisories (e.g. missing optional dates).
    pub warnings: Vec<String>,
}

/// Validate raw feed XML. On structural failure returns
/// [`Error::Validation`] holding every problem found; on malformed XML returns
/// [`Error::Xml`].
pub fn validate_str(xml: &str) -> Result<ValidationReport> {
    let feed = parse_feed(xml)?; // propagates Error::Xml for malformed input

    let mut problems: Vec<String> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // Detect declared format from the root for the report. Both shapes parse
    // into the same model, so we sniff the source text.
    let format = if xml.contains("<feed") && xml.contains("Atom") {
        "atom"
    } else {
        "rss"
    };

    // Channel-level required fields (RSS 2.0: title, link, description).
    if feed.channel.title.trim().is_empty() {
        problems.push("channel is missing a <title>".to_string());
    }
    if feed.channel.link.trim().is_empty() {
        problems.push("channel is missing a <link>".to_string());
    }
    if feed.channel.description.trim().is_empty() {
        // Atom uses <subtitle>; either maps to description in our model.
        warnings.push("channel has no <description>/<subtitle>".to_string());
    }

    if feed.items.is_empty() {
        warnings.push("feed contains no items".to_string());
    }

    // Item-level checks.
    for (idx, item) in feed.items.iter().enumerate() {
        let n = idx + 1;
        if item.title.trim().is_empty() && item.description.is_none() {
            // RSS requires at least one of title/description.
            problems.push(format!("item #{n} has neither <title> nor <description>"));
        }
        let has_id = item
            .guid
            .as_deref()
            .map(|g| !g.trim().is_empty())
            .unwrap_or(false)
            || !item.link.trim().is_empty();
        if !has_id {
            problems.push(format!("item #{n} has neither <link> nor <guid>/<id>"));
        }
        match &item.pub_date {
            Some(raw) if !raw.trim().is_empty() => {
                if DateTime::parse_any(raw).is_none() {
                    problems.push(format!("item #{n} has an unparseable date: {raw:?}"));
                }
            }
            _ => warnings.push(format!("item #{n} has no publication date")),
        }
    }

    if problems.is_empty() {
        Ok(ValidationReport {
            item_count: feed.items.len(),
            format,
            warnings,
        })
    } else {
        Err(Error::Validation(problems))
    }
}

/// Validate a feed file on disk.
pub fn validate_feed(path: &str) -> Result<ValidationReport> {
    let xml = std::fs::read_to_string(path)?;
    validate_str(&xml)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"<?xml version="1.0"?>
    <rss version="2.0"><channel>
      <title>News</title><link>https://e.com</link><description>desc</description>
      <item><title>One</title><link>https://e.com/1</link>
        <pubDate>Tue, 09 Jun 2026 14:30:00 GMT</pubDate></item>
    </channel></rss>"#;

    #[test]
    fn good_feed_passes() {
        let report = validate_str(GOOD).unwrap();
        assert_eq!(report.item_count, 1);
    }

    #[test]
    fn missing_channel_title_fails() {
        let xml = r#"<rss version="2.0"><channel>
          <link>https://e.com</link><description>d</description>
          <item><title>x</title><link>https://e/1</link></item>
        </channel></rss>"#;
        match validate_str(xml) {
            Err(Error::Validation(p)) => {
                assert!(p.iter().any(|m| m.contains("title")));
            }
            other => panic!("expected validation error, got {other:?}"),
        }
    }

    #[test]
    fn bad_date_fails() {
        let xml = r#"<rss version="2.0"><channel>
          <title>N</title><link>https://e</link><description>d</description>
          <item><title>x</title><link>https://e/1</link><pubDate>yesterday</pubDate></item>
        </channel></rss>"#;
        assert!(matches!(validate_str(xml), Err(Error::Validation(_))));
    }

    #[test]
    fn item_without_link_or_guid_fails() {
        let xml = r#"<rss version="2.0"><channel>
          <title>N</title><link>https://e</link><description>d</description>
          <item><title>x</title></item>
        </channel></rss>"#;
        assert!(matches!(validate_str(xml), Err(Error::Validation(_))));
    }

    #[test]
    fn malformed_xml_is_xml_error() {
        let xml = "<rss><channel><title>oops</channel></rss>";
        assert!(matches!(validate_str(xml), Err(Error::Xml(_))));
    }
}
