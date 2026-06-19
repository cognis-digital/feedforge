//! Self-contained date parsing and formatting (std-only, no `chrono`).
//!
//! feedforge needs three things from dates:
//!   1. Recognize the two timestamp shapes that appear in feeds:
//!      RFC 822 / RFC 2822 (RSS `pubDate`, e.g. `Mon, 09 Jun 2026 14:30:00 GMT`)
//!      and RFC 3339 / ISO 8601 (Atom `published`, e.g. `2026-06-09T14:30:00Z`).
//!   2. Produce a stable, comparable key (UTC Unix seconds) for sorting.
//!   3. Re-emit a timestamp in the format the target feed expects.
//!
//! Timezone offsets are honored and normalized to UTC. Sub-second precision is
//! discarded (feeds do not rely on it).

/// A parsed instant, normalized to UTC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateTime {
    pub year: i64,
    pub month: u32, // 1..=12
    pub day: u32,   // 1..=31
    pub hour: u32,  // 0..=23
    pub min: u32,   // 0..=59
    pub sec: u32,   // 0..=60 (leap second tolerated)
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

impl DateTime {
    /// Parse either RFC 822 or RFC 3339. Returns `None` if neither matches.
    pub fn parse_any(s: &str) -> Option<DateTime> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        Self::parse_rfc3339(s).or_else(|| Self::parse_rfc822(s))
    }

    /// Parse RFC 3339 / ISO 8601, e.g. `2026-06-09T14:30:00Z` or
    /// `2026-06-09T14:30:00+02:00`. Honors the offset, normalizes to UTC.
    pub fn parse_rfc3339(s: &str) -> Option<DateTime> {
        let bytes = s.as_bytes();
        // Need at least a `YYYY-MM-DD HH:MM:SS` shape; check separators.
        if s.len() < 19 {
            return None;
        }
        let year: i64 = s.get(0..4)?.parse().ok()?;
        if bytes[4] != b'-' {
            return None;
        }
        let month: u32 = s.get(5..7)?.parse().ok()?;
        if bytes[7] != b'-' {
            return None;
        }
        let day: u32 = s.get(8..10)?.parse().ok()?;
        // 'T' or space separator.
        if bytes[10] != b'T' && bytes[10] != b't' && bytes[10] != b' ' {
            return None;
        }
        let hour: u32 = s.get(11..13)?.parse().ok()?;
        if bytes[13] != b':' {
            return None;
        }
        let min: u32 = s.get(14..16)?.parse().ok()?;
        if bytes[16] != b':' {
            return None;
        }
        let sec: u32 = s.get(17..19)?.parse().ok()?;

        // Remainder may hold fractional seconds and/or a timezone.
        let mut rest = &s[19..];
        // Drop fractional seconds.
        if rest.starts_with('.') {
            let end = rest[1..]
                .find(|c: char| !c.is_ascii_digit())
                .map(|i| i + 1)
                .unwrap_or(rest.len());
            rest = &rest[end..];
        }

        let offset_minutes = parse_tz_offset(rest)?;
        let dt = DateTime {
            year,
            month,
            day,
            hour,
            min,
            sec,
        };
        dt.validate()?;
        Some(dt.shift_to_utc(offset_minutes))
    }

    /// Parse RFC 822 / RFC 2822, e.g. `Mon, 09 Jun 2026 14:30:00 GMT` or
    /// `9 Jun 2026 14:30 +0200`. The leading weekday and seconds are optional.
    pub fn parse_rfc822(s: &str) -> Option<DateTime> {
        // Strip an optional leading "Wkd," token.
        let s = match s.find(',') {
            Some(i) if i <= 4 => s[i + 1..].trim(),
            _ => s.trim(),
        };
        let toks: Vec<&str> = s.split_whitespace().collect();
        // Expect: day month year time [tz]
        if toks.len() < 4 {
            return None;
        }
        let day: u32 = toks[0].parse().ok()?;
        let month = month_from_abbrev(toks[1])?;
        let mut year: i64 = toks[2].parse().ok()?;
        // RFC 822 two-digit years: 00-49 => 2000s, 50-99 => 1900s.
        if toks[2].len() == 2 {
            year += if year < 50 { 2000 } else { 1900 };
        }
        let time_parts: Vec<&str> = toks[3].split(':').collect();
        if time_parts.len() < 2 {
            return None;
        }
        let hour: u32 = time_parts[0].parse().ok()?;
        let min: u32 = time_parts[1].parse().ok()?;
        let sec: u32 = if time_parts.len() >= 3 {
            time_parts[2].parse().ok()?
        } else {
            0
        };
        let offset_minutes = if toks.len() >= 5 {
            parse_named_or_numeric_tz(toks[4])?
        } else {
            0
        };
        let dt = DateTime {
            year,
            month,
            day,
            hour,
            min,
            sec,
        };
        dt.validate()?;
        Some(dt.shift_to_utc(offset_minutes))
    }

    fn validate(&self) -> Option<()> {
        if (1..=12).contains(&self.month)
            && (1..=31).contains(&self.day)
            && self.hour <= 23
            && self.min <= 59
            && self.sec <= 60
        {
            Some(())
        } else {
            None
        }
    }

    /// Days since the Unix epoch for this calendar date (proleptic Gregorian).
    fn days_from_epoch(&self) -> i64 {
        // Howard Hinnant's days_from_civil algorithm.
        let y = if self.month <= 2 {
            self.year - 1
        } else {
            self.year
        };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = y - era * 400; // [0, 399]
        let m = self.month as i64;
        let d = self.day as i64;
        let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1; // [0, 365]
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
        era * 146097 + doe - 719468
    }

    /// UTC Unix timestamp in seconds. Suitable as a sort key.
    pub fn to_unix(&self) -> i64 {
        self.days_from_epoch() * 86400
            + self.hour as i64 * 3600
            + self.min as i64 * 60
            + self.sec as i64
    }

    /// Apply a timezone offset (in minutes east of UTC) to normalize to UTC.
    fn shift_to_utc(self, offset_minutes: i64) -> DateTime {
        if offset_minutes == 0 {
            return self;
        }
        let unix = self.to_unix() - offset_minutes * 60;
        DateTime::from_unix(unix)
    }

    /// Reconstruct a UTC `DateTime` from a Unix timestamp.
    pub fn from_unix(unix: i64) -> DateTime {
        let days = unix.div_euclid(86400);
        let secs_of_day = unix.rem_euclid(86400);
        let (year, month, day) = civil_from_days(days);
        DateTime {
            year,
            month,
            day,
            hour: (secs_of_day / 3600) as u32,
            min: ((secs_of_day % 3600) / 60) as u32,
            sec: (secs_of_day % 60) as u32,
        }
    }

    /// Day of week, 0 = Sunday .. 6 = Saturday.
    fn weekday(&self) -> usize {
        // 1970-01-01 (epoch) was a Thursday (index 4).
        let d = self.days_from_epoch();
        (((d % 7) + 4 + 7) % 7) as usize
    }

    /// Format as RFC 822 (RSS `pubDate`), always in GMT.
    pub fn to_rfc822(&self) -> String {
        format!(
            "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
            WEEKDAYS[self.weekday()],
            self.day,
            MONTHS[(self.month - 1) as usize],
            self.year,
            self.hour,
            self.min,
            self.sec
        )
    }

    /// Format as RFC 3339 (Atom `published`/`updated`), always in UTC (`Z`).
    pub fn to_rfc3339(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            self.year, self.month, self.day, self.hour, self.min, self.sec
        )
    }
}

/// Convert a Unix day count back into a civil (year, month, day).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}

fn month_from_abbrev(s: &str) -> Option<u32> {
    let lower = s.to_ascii_lowercase();
    MONTHS
        .iter()
        .position(|m| m.to_ascii_lowercase() == lower[..lower.len().min(3)])
        .map(|i| (i + 1) as u32)
}

/// Parse a trailing RFC 3339 timezone designator into minutes east of UTC.
/// Accepts `Z`/`z`, `+HH:MM`, `-HH:MM`, `+HHMM`, or empty (assume UTC).
fn parse_tz_offset(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() || s == "Z" || s == "z" {
        return Some(0);
    }
    let sign = match s.as_bytes()[0] {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let rest = &s[1..];
    let (h, m) = if let Some((h, m)) = rest.split_once(':') {
        (h, m)
    } else if rest.len() == 4 {
        (&rest[0..2], &rest[2..4])
    } else if rest.len() == 2 {
        (rest, "00")
    } else {
        return None;
    };
    let hours: i64 = h.parse().ok()?;
    let mins: i64 = m.parse().ok()?;
    Some(sign * (hours * 60 + mins))
}

/// Parse an RFC 822 timezone: either a named zone or a numeric `+HHMM`.
fn parse_named_or_numeric_tz(s: &str) -> Option<i64> {
    match s.to_ascii_uppercase().as_str() {
        "GMT" | "UT" | "UTC" | "Z" => Some(0),
        "EST" => Some(-5 * 60),
        "EDT" => Some(-4 * 60),
        "CST" => Some(-6 * 60),
        "CDT" => Some(-5 * 60),
        "MST" => Some(-7 * 60),
        "MDT" => Some(-6 * 60),
        "PST" => Some(-8 * 60),
        "PDT" => Some(-7 * 60),
        _ => parse_tz_offset(s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rfc3339_utc() {
        let dt = DateTime::parse_rfc3339("2026-06-09T14:30:00Z").unwrap();
        assert_eq!(dt.year, 2026);
        assert_eq!(dt.month, 6);
        assert_eq!(dt.day, 9);
        assert_eq!(dt.hour, 14);
    }

    #[test]
    fn parses_rfc3339_with_offset_and_normalizes() {
        let dt = DateTime::parse_rfc3339("2026-06-09T14:30:00+02:00").unwrap();
        // 14:30 +02:00 == 12:30 UTC
        assert_eq!(dt.hour, 12);
        assert_eq!(dt.min, 30);
    }

    #[test]
    fn parses_rfc3339_fractional() {
        let dt = DateTime::parse_rfc3339("2026-06-09T14:30:00.123Z").unwrap();
        assert_eq!(dt.sec, 0);
    }

    #[test]
    fn parses_rfc822_full() {
        let dt = DateTime::parse_rfc822("Mon, 09 Jun 2026 14:30:00 GMT").unwrap();
        assert_eq!(dt.year, 2026);
        assert_eq!(dt.month, 6);
        assert_eq!(dt.hour, 14);
    }

    #[test]
    fn parses_rfc822_with_numeric_offset() {
        let dt = DateTime::parse_rfc822("9 Jun 2026 14:30 +0200").unwrap();
        assert_eq!(dt.hour, 12); // normalized to UTC
    }

    #[test]
    fn roundtrip_unix() {
        let dt = DateTime::parse_rfc3339("2026-06-09T14:30:45Z").unwrap();
        let back = DateTime::from_unix(dt.to_unix());
        assert_eq!(dt, back);
    }

    #[test]
    fn ordering_via_unix() {
        let older = DateTime::parse_any("2026-06-01T00:00:00Z").unwrap();
        let newer = DateTime::parse_any("Mon, 09 Jun 2026 00:00:00 GMT").unwrap();
        assert!(newer.to_unix() > older.to_unix());
    }

    #[test]
    fn weekday_known_date() {
        // 2026-06-09 is a Tuesday.
        let dt = DateTime::parse_rfc3339("2026-06-09T00:00:00Z").unwrap();
        assert_eq!(WEEKDAYS[dt.weekday()], "Tue");
    }

    #[test]
    fn rejects_garbage() {
        assert!(DateTime::parse_any("not a date").is_none());
        assert!(DateTime::parse_any("2026-13-40T99:99:99Z").is_none());
    }
}
