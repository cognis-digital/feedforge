//! Feed parsing: read an RSS 2.0 or Atom 1.0 document into a [`Feed`].
//!
//! Uses `quick-xml`'s pull reader. The parser auto-detects RSS vs Atom by the
//! root-level structure (`<channel>`/`<item>` vs `<entry>`), so the same
//! routine serves both `validate` and `merge`.

use crate::model::{Channel, Feed, Item};
use crate::{Error, Result};
use quick_xml::events::Event;
use quick_xml::Reader;

/// What kind of element are we currently accumulating text into.
#[derive(Clone, Copy, PartialEq)]
enum Scope {
    None,
    Channel,
    Item,  // RSS <item>
    Entry, // Atom <entry>
}

/// Parse a feed document. Returns a structurally-loaded [`Feed`]; field-level
/// validation is performed separately by [`crate::validate`].
pub fn parse_feed(xml: &str) -> Result<Feed> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut feed = Feed::default();
    let mut channel = Channel::default();
    let mut scope = Scope::None;
    let mut cur_item = Item::default();
    let mut path: Vec<String> = Vec::new();
    // For Atom links we read the href attribute rather than text.
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Err(e) => {
                return Err(Error::Xml(format!(
                    "malformed XML at position {}: {e}",
                    reader.buffer_position()
                )))
            }
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let name = local_name(e.name().as_ref());
                match name.as_str() {
                    "channel" => scope = Scope::Channel,
                    "item" => {
                        scope = Scope::Item;
                        cur_item = Item::default();
                    }
                    "entry" => {
                        scope = Scope::Entry;
                        cur_item = Item::default();
                    }
                    "link" => {
                        // Atom: <link href="..."/>. RSS link is text (handled
                        // in Text). Capture href if present.
                        if let Some(href) = attr(&e, "href") {
                            apply_field(&mut channel, &mut cur_item, scope, "link", &href);
                        }
                    }
                    _ => {}
                }
                path.push(name);
            }
            Ok(Event::Empty(e)) => {
                let name = local_name(e.name().as_ref());
                if name == "link" {
                    if let Some(href) = attr(&e, "href") {
                        apply_field(&mut channel, &mut cur_item, scope, "link", &href);
                    }
                }
            }
            Ok(Event::Text(t)) => {
                let text = t
                    .unescape()
                    .map_err(|e| Error::Xml(e.to_string()))?
                    .into_owned();
                if text.is_empty() {
                    continue;
                }
                if let Some(tag) = path.last() {
                    let tag = tag.clone();
                    apply_field(&mut channel, &mut cur_item, scope, &tag, &text);
                }
            }
            Ok(Event::CData(t)) => {
                let text = String::from_utf8_lossy(t.as_ref()).into_owned();
                if let Some(tag) = path.last() {
                    let tag = tag.clone();
                    apply_field(&mut channel, &mut cur_item, scope, &tag, &text);
                }
            }
            Ok(Event::End(e)) => {
                let name = local_name(e.name().as_ref());
                match name.as_str() {
                    "item" | "entry" => {
                        feed.items.push(std::mem::take(&mut cur_item));
                        scope = Scope::Channel;
                    }
                    "channel" => scope = Scope::None,
                    _ => {}
                }
                path.pop();
            }
            _ => {}
        }
        buf.clear();
    }

    feed.channel = channel;
    Ok(feed)
}

/// Route a `(tag, value)` pair to the right field of channel or current item.
fn apply_field(channel: &mut Channel, item: &mut Item, scope: Scope, tag: &str, value: &str) {
    match scope {
        Scope::Channel | Scope::None => match tag {
            "title" => set_if_empty(&mut channel.title, value),
            "link" => set_if_empty(&mut channel.link, value),
            "description" | "subtitle" => set_if_empty(&mut channel.description, value),
            "language" => channel.language = Some(value.to_string()),
            "lastBuildDate" | "pubDate" | "updated" if channel.last_build_date.is_none() => {
                channel.last_build_date = Some(value.to_string());
            }
            _ => {}
        },
        Scope::Item | Scope::Entry => match tag {
            "title" => set_if_empty(&mut item.title, value),
            "link" => set_if_empty(&mut item.link, value),
            "description" | "summary" | "content" => {
                if item.description.is_none() {
                    item.description = Some(value.to_string());
                }
            }
            "guid" | "id" => {
                if item.guid.is_none() {
                    item.guid = Some(value.to_string());
                }
            }
            "pubDate" | "published" | "updated" => {
                if item.pub_date.is_none() {
                    item.pub_date = Some(value.to_string());
                }
            }
            "author" | "name" if item.author.is_none() => {
                item.author = Some(value.to_string());
            }
            _ => {}
        },
    }
}

fn set_if_empty(slot: &mut String, value: &str) {
    if slot.is_empty() {
        *slot = value.to_string();
    }
}

/// Strip any namespace prefix (`atom:title` -> `title`).
fn local_name(raw: &[u8]) -> String {
    let s = String::from_utf8_lossy(raw);
    match s.rsplit_once(':') {
        Some((_, local)) => local.to_string(),
        None => s.into_owned(),
    }
}

/// Read a named attribute's value as a UTF-8 string.
fn attr(e: &quick_xml::events::BytesStart<'_>, key: &str) -> Option<String> {
    for a in e.attributes().flatten() {
        if local_name(a.key.as_ref()) == key {
            return Some(String::from_utf8_lossy(&a.value).into_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rss() {
        let xml = r#"<?xml version="1.0"?>
        <rss version="2.0"><channel>
          <title>News</title>
          <link>https://e.com</link>
          <description>desc</description>
          <item>
            <title>One</title>
            <link>https://e.com/1</link>
            <pubDate>Tue, 09 Jun 2026 14:30:00 GMT</pubDate>
            <guid>g-1</guid>
          </item>
        </channel></rss>"#;
        let feed = parse_feed(xml).unwrap();
        assert_eq!(feed.channel.title, "News");
        assert_eq!(feed.items.len(), 1);
        assert_eq!(feed.items[0].guid.as_deref(), Some("g-1"));
    }

    #[test]
    fn parses_atom_with_href_links() {
        let xml = r#"<?xml version="1.0"?>
        <feed xmlns="http://www.w3.org/2005/Atom">
          <title>News</title>
          <link href="https://e.com"/>
          <id>https://e.com</id>
          <entry>
            <title>One</title>
            <link href="https://e.com/1"/>
            <id>g-1</id>
            <updated>2026-06-09T14:30:00Z</updated>
          </entry>
        </feed>"#;
        let feed = parse_feed(xml).unwrap();
        assert_eq!(feed.channel.link, "https://e.com");
        assert_eq!(feed.items[0].link, "https://e.com/1");
        assert_eq!(feed.items[0].guid.as_deref(), Some("g-1"));
    }

    #[test]
    fn rejects_malformed() {
        let xml = "<rss><channel><title>oops</channel></rss>";
        assert!(parse_feed(xml).is_err());
    }

    #[test]
    fn handles_cdata_and_entities() {
        let xml = r#"<rss version="2.0"><channel>
          <title>A &amp; B</title><link>https://e</link><description>d</description>
          <item><title><![CDATA[Raw <tag>]]></title><link>https://e/1</link></item>
        </channel></rss>"#;
        let feed = parse_feed(xml).unwrap();
        assert_eq!(feed.channel.title, "A & B");
        assert_eq!(feed.items[0].title, "Raw <tag>");
    }
}
