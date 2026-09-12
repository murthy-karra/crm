//! Exact source instants and confirmed IANA date conversion for 010f2.
//! The caller freezes outputs and the exported profile/tzdb versions in a plan;
//! a worker consumes those instants and never reruns this conversion on retry.

use chrono::{DateTime, Datelike, Duration, LocalResult, NaiveDate, TimeZone, Utc};
use chrono_tz::{GapInfo, Tz};

pub(crate) fn timezone(raw: &str) -> Option<Tz> {
    // Case-sensitive IANA names/aliases, without platform TZ or local fallbacks.
    raw.parse::<Tz>().ok()
}

pub(crate) fn date(raw: &str) -> Option<NaiveDate> {
    if raw.len() != 10
        || !raw.bytes().enumerate().all(|(i, b)| match i {
            4 | 7 => b == b'-',
            _ => b.is_ascii_digit(),
        })
    {
        return None;
    }
    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .ok()
        .filter(|value| value.year() >= 1)
}

/// RFC 3339 with a known explicit offset and exact PostgreSQL microseconds.
/// Check discarded fractional digits *before* Chrono can truncate to nanos.
pub(crate) fn instant(raw: &str) -> Option<DateTime<Utc>> {
    let bytes = raw.as_bytes();
    if bytes.len() < 20
        || date(raw.get(..10)?).is_none()
        || !matches!(bytes[10], b'T' | b't')
        || !bytes[11..19].iter().enumerate().all(|(i, byte)| {
            if matches!(i, 2 | 5) {
                *byte == b':'
            } else {
                byte.is_ascii_digit()
            }
        })
        || &raw[17..19] == "60"
    {
        return None;
    }
    let mut offset_start = 19;
    if bytes.get(offset_start) == Some(&b'.') {
        offset_start += 1;
        let fraction_start = offset_start;
        while bytes.get(offset_start).is_some_and(u8::is_ascii_digit) {
            if offset_start - fraction_start >= 6 && bytes[offset_start] != b'0' {
                return None;
            }
            offset_start += 1;
        }
        if offset_start == fraction_start {
            return None;
        }
    }
    let offset = raw.get(offset_start..)?;
    if offset != "Z"
        && offset != "z"
        && !(offset.len() == 6
            && matches!(offset.as_bytes()[0], b'+' | b'-')
            && offset.as_bytes()[3] == b':'
            && offset.as_bytes()[1..3].iter().all(u8::is_ascii_digit)
            && offset.as_bytes()[4..6].iter().all(u8::is_ascii_digit))
    {
        return None;
    }
    // RFC 3339 -00:00 states an unknown local offset, not known UTC.
    if offset == "-00:00" {
        return None;
    }
    let value = DateTime::parse_from_rfc3339(raw).ok()?.with_timezone(&Utc);
    (value.timestamp_subsec_nanos() % 1000 == 0).then_some(value)
}

/// Last whole second before the next local calendar date first begins.
/// Midnight gaps may resolve to the tzdb's exact first valid instant; repeated
/// midnight uses its first occurrence, verified against the preceding date.
/// Entire skipped dates, out-of-range boundaries and inconsistent gaps hold.
pub(crate) fn end_of_day(source_date: NaiveDate, zone: Tz) -> Option<DateTime<Utc>> {
    let next_date = source_date.succ_opt()?;
    let midnight = next_date.and_hms_opt(0, 0, 0)?;
    let boundary = match zone.from_local_datetime(&midnight) {
        LocalResult::Single(value) => value,
        LocalResult::Ambiguous(first, second) => first.min(second),
        LocalResult::None => GapInfo::new(&midnight, &zone)?.end?,
    };
    if boundary.date_naive() != next_date {
        return None;
    }
    let last = boundary.checked_sub_signed(Duration::seconds(1))?;
    (last.date_naive() == source_date && last.timestamp_subsec_nanos() == 0)
        .then(|| last.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requires_an_explicit_known_offset_without_rounding_or_leap_seconds() {
        for raw in [
            "2026-09-11T12:13:14Z",
            "2026-09-11T12:13:14.123456Z",
            "2026-09-11T12:13:14.123456000000Z",
            "2026-09-11T12:13:14.100000000000+05:45",
        ] {
            assert!(instant(raw).is_some());
        }
        for raw in [
            "2026-09-11T12:13:14",
            "2026-09-11 12:13:14Z",
            "2026-09-11T12:13:14-00:00",
            "2026-09-11T12:13:14.1234561Z",
            "2026-09-11T12:13:14.1234560001Z",
            "2026-09-11T12:13:14.Z",
            "2026-09-11T12:13:14+24:00",
            "2026-09-11T12:13:60Z",
            "2026-02-29T12:13:14Z",
            "0000-01-01T00:00:00Z",
            "2026-09-11T12:13:14Z ",
        ] {
            assert!(instant(raw).is_none());
        }
    }

    #[test]
    fn microsecond_and_non_hour_offsets_survive_exactly() {
        assert_eq!(
            instant("2026-09-11T12:13:14.123456+05:45"),
            instant("2026-09-11T06:28:14.123456Z")
        );
    }

    #[test]
    fn end_of_day_uses_the_source_date_across_both_dst_transitions() {
        let zone = timezone("America/Los_Angeles").unwrap();
        for (day, expected) in [
            ("2026-01-15", "2026-01-16T07:59:59Z"),
            ("2026-03-08", "2026-03-09T06:59:59Z"),
            ("2026-11-01", "2026-11-02T07:59:59Z"),
        ] {
            assert_eq!(end_of_day(date(day).unwrap(), zone), instant(expected));
        }
    }

    #[test]
    fn midnight_gap_and_repetition_resolve_only_with_verified_date_boundary() {
        assert_eq!(
            end_of_day(
                date("2018-11-03").unwrap(),
                timezone("America/Sao_Paulo").unwrap()
            ),
            instant("2018-11-04T02:59:59Z")
        );
        assert_eq!(
            end_of_day(
                date("2020-10-31").unwrap(),
                timezone("America/Havana").unwrap()
            ),
            instant("2020-11-01T03:59:59Z")
        );
    }

    #[test]
    fn skipped_date_and_missing_next_date_are_held() {
        let zone = timezone("Pacific/Apia").unwrap();
        assert!(end_of_day(date("2011-12-30").unwrap(), zone).is_none());
        assert!(end_of_day(date("2011-12-29").unwrap(), zone).is_none());
        for raw in ["2023-02-29", "2026-9-11", "2026-09-11 ", "0000-01-01"] {
            assert!(date(raw).is_none());
        }
        for raw in ["", "america/los_angeles", "UTC ", "+05:00", "local"] {
            assert!(timezone(raw).is_none());
        }
        assert!(timezone("UTC").is_some());
    }
}
