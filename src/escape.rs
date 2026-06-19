//! Minimal, correct XML text/attribute escaping for hand-written emission.
//!
//! We escape the five predefined XML entities. Text content does not strictly
//! require `"` or `'` to be escaped, but escaping them is always safe and keeps
//! a single routine usable for both text and attribute values.

/// Escape a string for safe inclusion in XML text or attribute context.
pub fn xml_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            // Strip characters that are illegal in XML 1.0 to avoid emitting
            // a document that cannot be parsed back.
            c if is_illegal_xml_char(c) => {}
            c => out.push(c),
        }
    }
    out
}

/// XML 1.0 forbids most C0 control characters (except tab, LF, CR).
fn is_illegal_xml_char(c: char) -> bool {
    let n = c as u32;
    match n {
        0x09 | 0x0A | 0x0D => false,
        0x00..=0x1F => true,
        // Surrogate range and the two non-characters at the end of the BMP.
        0xFFFE | 0xFFFF => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_entities() {
        assert_eq!(xml_escape("a & b < c > d"), "a &amp; b &lt; c &gt; d");
        assert_eq!(xml_escape("\"x\" 'y'"), "&quot;x&quot; &apos;y&apos;");
    }

    #[test]
    fn strips_control_chars_but_keeps_whitespace() {
        let s = "ok\u{0007}\tnewline\nreturn\r";
        assert_eq!(xml_escape(s), "ok\tnewline\nreturn\r");
    }
}
