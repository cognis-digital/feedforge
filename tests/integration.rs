//! End-to-end tests exercising the public library surface across the
//! build -> validate -> merge pipeline.

use feedforge::build::{build_feed, feed_from_json};
use feedforge::merge::merge_feeds;
use feedforge::model::FeedFormat;
use feedforge::validate::validate_str;

const ITEMS_JSON: &str = r#"{
  "title": "Cognis Monitoring",
  "link": "https://example.com/feed",
  "description": "Operational alerts",
  "language": "en-us",
  "items": [
    {"title": "Alert A", "link": "https://example.com/a", "guid": "a",
     "pubDate": "2026-06-09T14:30:00Z", "description": "first"},
    {"title": "Alert B", "link": "https://example.com/b", "guid": "b",
     "pubDate": "Wed, 10 Jun 2026 09:00:00 GMT"}
  ]
}"#;

#[test]
fn build_then_validate_rss() {
    let feed = feed_from_json(ITEMS_JSON).unwrap();
    let xml = build_feed(&feed, FeedFormat::Rss).unwrap();
    let report = validate_str(&xml).expect("built RSS must validate");
    assert_eq!(report.item_count, 2);
}

#[test]
fn build_then_validate_atom() {
    let feed = feed_from_json(ITEMS_JSON).unwrap();
    let xml = build_feed(&feed, FeedFormat::Atom).unwrap();
    let report = validate_str(&xml).expect("built Atom must validate");
    assert_eq!(report.item_count, 2);
}

#[test]
fn built_feed_normalizes_mixed_input_dates() {
    let feed = feed_from_json(ITEMS_JSON).unwrap();
    let xml = build_feed(&feed, FeedFormat::Rss).unwrap();
    // Both an RFC3339 input and an RFC822 input come out as RFC822 in RSS.
    assert!(xml.contains("<pubDate>Tue, 09 Jun 2026 14:30:00 GMT</pubDate>"));
    assert!(xml.contains("<pubDate>Wed, 10 Jun 2026 09:00:00 GMT</pubDate>"));
}

#[test]
fn merge_two_built_feeds_dedupes_and_orders() {
    let feed = feed_from_json(ITEMS_JSON).unwrap();
    let doc1 = build_feed(&feed, FeedFormat::Rss).unwrap();

    // A second feed that overlaps item "a" and adds a newer item "c".
    let second = r#"{
      "title": "Mirror",
      "link": "https://mirror.example.com/feed",
      "description": "mirror",
      "items": [
        {"title": "Alert A copy", "link": "https://example.com/a", "guid": "a",
         "pubDate": "2026-06-09T14:30:00Z"},
        {"title": "Alert C", "link": "https://example.com/c", "guid": "c",
         "pubDate": "2026-06-12T00:00:00Z"}
      ]
    }"#;
    let f2 = feed_from_json(second).unwrap();
    let doc2 = build_feed(&f2, FeedFormat::Rss).unwrap();

    let merged = merge_feeds(&[doc1, doc2]).unwrap();
    // a (deduped), b, c => 3 unique items
    assert_eq!(merged.items.len(), 3);
    // Newest first: C (Jun 12), B (Jun 10), A (Jun 9)
    let titles: Vec<&str> = merged.items.iter().map(|i| i.title.as_str()).collect();
    assert_eq!(titles[0], "Alert C");
    assert_eq!(titles[2], "Alert A");

    // And the merged feed itself re-validates.
    let merged_xml = build_feed(&merged, FeedFormat::Rss).unwrap();
    validate_str(&merged_xml).expect("merged feed must validate");
}

#[test]
fn validate_rejects_malformed_and_invalid() {
    assert!(validate_str("<rss><channel><title>x</channel>").is_err());
    let missing_link = r#"<rss version="2.0"><channel>
      <title>N</title><description>d</description>
      <item><title>x</title><link>https://e/1</link></item>
    </channel></rss>"#;
    assert!(validate_str(missing_link).is_err());
}

#[test]
fn example_items_file_builds_and_validates() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/examples/items.json");
    let json = std::fs::read_to_string(path).expect("examples/items.json present");
    let feed = feed_from_json(&json).unwrap();
    let xml = build_feed(&feed, FeedFormat::Rss).unwrap();
    let report = validate_str(&xml).expect("example feed must validate");
    assert!(report.item_count >= 1);
}
