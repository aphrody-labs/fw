//! Calendar helpers on UTC timestamps in the formats the dump uses (`2026-11-16 12:42:23.66749+00`, RFC 3339).

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's `days_from_civil`).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Date of a day count since the epoch, as `(year, month, day)`.
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Unix seconds of `YYYY-MM-DD[ T]HH:MM:SS...`; a bare `YYYY-MM-DD` is midnight. Offsets and fractions are ignored (all UTC).
pub fn parse_ts(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let num = |r: std::ops::Range<usize>| s.get(r)?.parse::<i64>().ok();
    let days = days_from_civil(num(0..4)?, num(5..7)?, num(8..10)?);
    let secs = if b.len() >= 19 && (b[10] == b' ' || b[10] == b'T') { num(11..13)? * 3600 + num(14..16)? * 60 + num(17..19)? } else { 0 };
    Some(days * 86_400 + secs)
}

/// `YYYY-MM-DD` of unix seconds.
pub fn date_of(ts: i64) -> String {
    let (y, m, d) = civil_from_days(ts.div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let ts = parse_ts("2026-11-21 02:00:22.021216857+00").unwrap();
        assert_eq!(date_of(ts), "2026-11-21");
        assert_eq!(parse_ts("1970-01-02"), Some(86_400));
        assert_eq!(parse_ts("2026-11-21T02:00:22Z"), Some(ts));
        assert_eq!(parse_ts("nope"), None);
    }

    #[test]
    fn leap_day() {
        assert_eq!(date_of(parse_ts("2024-02-29").unwrap()), "2024-02-29");
        assert_eq!(parse_ts("2024-03-01").unwrap() - parse_ts("2024-02-28").unwrap(), 2 * 86_400);
    }
}
