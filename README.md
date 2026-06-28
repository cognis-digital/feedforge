# feedforge

RSS/Atom feed builder, validator, and merger for monitoring stacks.

`feedforge` turns a small JSON description of items into a valid RSS 2.0 or
Atom 1.0 feed, validates existing feeds as a CI gate, and merges several feeds
into one — de-duplicated by link/guid and sorted newest-first. It is a single
small Rust binary with a tiny dependency footprint, built for use inside CI
containers and OSINT/monitoring pipelines.

Defensive / analytical use only.


<!-- cognis:example:start -->
## 🔎 Example output

**Sample result format** _(illustrative values — run on your own data for real findings):_

```
{
  "feeds": [
    {
      "id": "1234567890",
      "title": "Example Feed 1",
      "url": "https://example.com/feed1",
      "updated": "2022-01-01T12:00:00Z"
    },
    {
      "id": "2345678901",
      "title": "Example Feed 2",
      "url": "https://example.com/feed2",
      "updated": "2022-02-01T14:30:00Z"
    }
  ]
}
```

<!-- cognis:example:end -->

## Install

```sh
# From a checkout
cargo install --path .

# Or just build the binary
cargo build --release
# -> target/release/feedforge
```

## Usage

### Build a feed

```sh
feedforge build examples/items.json --format rss -o feed.xml
```

```console
$ feedforge build examples/items.json --format rss
<?xml version="1.0" encoding="UTF-8"?>
<rss version="2.0">
  <channel>
    <title>Cognis Threat Monitoring</title>
    <link>https://example.com/monitoring</link>
    <description>Defensive monitoring alerts and advisories.</description>
    <language>en-us</language>
    <lastBuildDate>Fri, 12 Jun 2026 08:15:00 GMT</lastBuildDate>
    <generator>feedforge</generator>
    <item>
      <title>Advisory: anomalous outbound DNS volume on edge segment</title>
      <link>https://example.com/advisories/2026-0612-dns</link>
      <description>Sustained spike in NXDOMAIN responses; review resolver logs.</description>
      <author>soc@example.com</author>
      <pubDate>Fri, 12 Jun 2026 08:15:00 GMT</pubDate>
      <guid isPermaLink="false">advisory-2026-0612-dns</guid>
    </item>
    ...
  </channel>
</rss>
```

The items file may be a full object (`{ "title": ..., "items": [ ... ] }`) or a
bare array of items. Item dates are accepted as RFC 822 (`Wed, 10 Jun 2026
13:45:00 GMT`) **or** RFC 3339 (`2026-06-09T17:00:00+02:00`); both are
normalized to the target format and offsets are converted to UTC.

### Validate a feed (CI gate)

```sh
feedforge validate feed.xml
```

```console
$ feedforge validate feed.xml
OK: rss feed, 3 item(s) valid
```

`validate` checks that the document is well-formed XML, that the channel has a
`title` and `link`, that each item has a `title` or `description` and a
`link`/`guid`, and that any present dates parse. It exits non-zero on any
failure, so it drops straight into a CI pipeline:

```console
$ feedforge validate broken.xml; echo "exit=$?"
INVALID: feed failed validation (1 problem(s)):
  - item #2 has an unparseable date: "yesterday"
exit=1
```

### Merge feeds

```sh
feedforge merge feed_a.xml feed_b.xml feed_c.xml --format rss -o merged.xml
```

Items are de-duplicated by `guid` (falling back to `link`); when a duplicate is
seen, a copy carrying a parseable date is preferred over an undated one. The
result is sorted newest-first and re-emitted in the chosen format.

## Features

- **build** — std-only, hand-written, fully-escaped XML emission for RSS 2.0
  and Atom 1.0.
- **validate** — well-formedness plus required-field and date-parsing checks;
  non-zero exit for CI gating; reports every problem at once.
- **merge** — dedupe by link/guid, newest-first ordering, format conversion.
- **Library API** — `build`, `validate`, and `merge` are exposed as library
  functions; the CLI is a thin wrapper.
- **Dates without `chrono`** — a self-contained parser handles RFC 822 and
  RFC 3339, honoring timezone offsets and normalizing to UTC.
- **Tiny dependency set** — `quick-xml` for parsing, `serde`/`serde_json` for
  the items JSON. No XML-writer or date crate.

## Library example

```rust
use feedforge::build::{build_feed, feed_from_json};
use feedforge::model::FeedFormat;
use feedforge::validate::validate_str;

let json = std::fs::read_to_string("examples/items.json")?;
let feed = feed_from_json(&json)?;
let xml = build_feed(&feed, FeedFormat::Rss)?;
validate_str(&xml)?; // ok
# Ok::<(), feedforge::Error>(())
```

## Testing

```sh
cargo test
```

Unit tests cover escaping, date parsing/normalization, build output validity,
parsing, and merge dedupe/ordering; integration tests exercise the full
build → validate → merge pipeline.

## License

License: COCL 1.0 (`LicenseRef-COCL-1.0`).

Maintainer: Cognis Digital.
