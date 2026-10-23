//! Dates as this device's local time (SERVICE.md "Comments": a comment's
//! `date` is local time without a zone or seconds, `YYYY-MM-DDTHH:MM`).
//!
//! A PDF writes a date with its zone (ISO 32000-2 §7.9.4,
//! `D:YYYYMMDDHHmmSSOHH'mm`): `Z` for UTC, as pdfcraft stamps every mark it
//! makes, or an offset such as `+02'00'`. [`local`] takes such a date to the
//! wall clock of this device at that instant; a date written without a zone
//! has no known relation to UTC and is taken as written.
//!
//! The device's offset comes from the same place as the shell's status
//! clock (`crates/shell/src/shell/bar.rs`, `refresh_script_utc_offset`):
//! the C library's `localtime_r` (`tm_gmtoff`) on Unix, chrono's `Local`
//! elsewhere. It is asked for the date's own instant, so a date from before
//! a daylight-saving change reads as the clock read it then. Tests fix it
//! ([`with_offset`]).

#[cfg(test)]
use std::cell::Cell;

/// A PDF date (`D:20261011003256Z`, `D:20261010143000+02'00'`,
/// `D:20261010143000`; the `D:` may be missing, and everything after the
/// day is optional) as the local time `YYYY-MM-DDTHH:MM`, where
/// `offset_at(utc)` gives the seconds this device's clock is ahead of UTC
/// at the instant `utc` (seconds since the Unix epoch). A date with no zone
/// is taken as written. `None` for anything else: fewer than eight digits,
/// a month, day or time out of range.
pub(crate) fn local(raw: &str, offset_at: impl Fn(i64) -> i64) -> Option<String> {
    let s = raw.trim();
    let s = s.strip_prefix("D:").unwrap_or(s);
    let end = s.bytes().position(|b| !b.is_ascii_digit()).unwrap_or(s.len());
    let digits = &s.as_bytes()[..end];
    if digits.len() < 8 {
        return None;
    }
    // Fixed-width fields; one the file leaves out is its default (a lone
    // digit at the end completes no field).
    let field = |from: usize, len: usize, default: i64| -> i64 {
        digits.get(from..from + len).map_or(default, |d| d.iter().fold(0, |n, b| n * 10 + i64::from(b - b'0')))
    };
    let (year, month, day) = (field(0, 4, 0), field(4, 2, 1), field(6, 2, 1));
    let (hour, minute, second) = (field(8, 2, 0), field(10, 2, 0), field(12, 2, 0));
    if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) || hour > 23 || minute > 59 || second > 60 {
        return None;
    }
    let written = (year, month, day, hour, minute);
    let shown = match zone(&s[end..]) {
        // The instant in UTC, then this device's clock at that instant.
        Some(zone) => {
            let utc = days_from_civil(year, month, day) * 86_400 + hour * 3_600 + minute * 60 + second.min(59) - zone;
            let local = utc + offset_at(utc);
            let (y, m, d) = civil_from_days(local.div_euclid(86_400));
            let secs = local.rem_euclid(86_400);
            (y, m, d, secs / 3_600, secs / 60 % 60)
        }
        None => written,
    };
    let (y, m, d, h, mi) = shown;
    Some(format!("{y:04}-{m:02}-{d:02}T{h:02}:{mi:02}"))
}

/// A PDF date's zone, what follows its digits, as seconds ahead of UTC:
/// `Z` is 0; `+hh'mm'` and `-hh'mm'` (the apostrophes and the minutes
/// optional). `None` when the date names no zone it can be read from.
fn zone(rest: &str) -> Option<i64> {
    let rest = rest.trim().as_bytes();
    let sign = match rest.first()? {
        b'Z' | b'z' => return Some(0),
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let two = |at: usize| -> Option<i64> {
        match rest.get(at..at + 2)? {
            [a, b] if a.is_ascii_digit() && b.is_ascii_digit() => Some(i64::from(a - b'0') * 10 + i64::from(b - b'0')),
            _ => None,
        }
    };
    let hours = two(1)?;
    let at = if rest.get(3) == Some(&b'\'') { 4 } else { 3 };
    let minutes = match rest.get(at) {
        Some(b) if b.is_ascii_digit() => two(at)?,
        _ => 0,
    };
    (hours <= 23 && minutes <= 59).then_some(sign * (hours * 3_600 + minutes * 60))
}

fn leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if leap(year) => 29,
        2 => 28,
        _ => 31,
    }
}

/// Days since 1970-01-01 of a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`, as pdfcraft's own `pdf_date` counts the other way).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The date `days` after 1970-01-01: (year, month, day).
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

#[cfg(test)]
thread_local! {
    /// A test's fixed offset ([`with_offset`]).
    static FIXED: Cell<Option<i64>> = const { Cell::new(None) };
}

/// Seconds this device's clock is ahead of UTC at the instant `utc`
/// (module doc); 0 where the C library knows no zone.
pub(crate) fn offset_at(utc: i64) -> i64 {
    #[cfg(test)]
    if let Some(fixed) = FIXED.with(Cell::get) {
        return fixed;
    }
    device_offset_at(utc)
}

#[cfg(unix)]
fn device_offset_at(utc: i64) -> i64 {
    // A 32-bit `time_t` cannot hold every instant.
    let Some(at) = libc::time_t::try_from(utc).ok() else { return 0 };
    // SAFETY: `localtime_r` is the reentrant form: it reads `at` and writes
    // only the `tm` it is handed, which lives on this stack.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&at, &mut tm) }.is_null() {
        return 0;
    }
    // A C `long`: 32 bits on some targets, as the shell's clock reads it.
    tm.tm_gmtoff as i64
}

#[cfg(not(unix))]
fn device_offset_at(utc: i64) -> i64 {
    use chrono::TimeZone;
    chrono::Local.timestamp_opt(utc, 0).single().map_or(0, |t| i64::from(t.offset().local_minus_utc()))
}

/// The minute `YYYY-MM-DDTHH:MM` of a clock `offset` seconds ahead of UTC at
/// the instant `utc` (tests: what a mark made then should read as).
#[cfg(test)]
pub(crate) fn minute_at(utc: i64, offset: i64) -> String {
    let local = utc + offset;
    let (y, m, d) = civil_from_days(local.div_euclid(86_400));
    let secs = local.rem_euclid(86_400);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}", secs / 3_600, secs / 60 % 60)
}

/// Run `f` with this device's offset fixed at `seconds` ahead of UTC, on
/// this thread (a service call in a test runs on the test's thread).
#[cfg(test)]
pub(crate) fn with_offset<T>(seconds: i64, f: impl FnOnce() -> T) -> T {
    struct Restore(Option<i64>);
    impl Drop for Restore {
        fn drop(&mut self) {
            FIXED.with(|fixed| fixed.set(self.0));
        }
    }
    let _restore = Restore(FIXED.with(|fixed| fixed.replace(Some(seconds))));
    f()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PDT: i64 = -7 * 3_600;

    #[test]
    fn a_utc_date_reads_as_this_devices_clock() {
        // The bug: a mark made at 17:32 PDT on 10 Oct is stamped 00:32 UTC
        // on 11 Oct, and read as local time, it said "11 Oct".
        assert_eq!(local("D:20261011003256Z", |_| PDT).as_deref(), Some("2026-10-10T17:32"));
        assert_eq!(local("D:20261011003256Z", |_| 0).as_deref(), Some("2026-10-11T00:32"));
        assert_eq!(local("D:20261011003256Z", |_| 5 * 3_600 + 1_800).as_deref(), Some("2026-10-11T06:02"), "a half-hour zone");
        assert_eq!(local("20261011003256Z", |_| PDT).as_deref(), Some("2026-10-10T17:32"), "without the D: prefix");
        assert_eq!(local("D:20261011003256Z00'00'", |_| PDT).as_deref(), Some("2026-10-10T17:32"), "Z with a redundant offset");
    }

    #[test]
    fn an_offset_date_reads_as_this_devices_clock() {
        assert_eq!(local("D:20261010143000+02'00'", |_| 0).as_deref(), Some("2026-10-10T12:30"));
        assert_eq!(local("D:20261010143000+02'00'", |_| PDT).as_deref(), Some("2026-10-10T05:30"));
        assert_eq!(local("D:20261010010000+02'00'", |_| PDT).as_deref(), Some("2026-10-09T16:00"), "back across midnight");
        assert_eq!(local("D:20261010220000-04'00'", |_| 2 * 3_600).as_deref(), Some("2026-10-11T04:00"), "forward across midnight");
        assert_eq!(local("D:20261231233000-01'00'", |_| 0).as_deref(), Some("2027-01-01T00:30"), "into the next year");
        assert_eq!(local("D:20240229120000+0530", |_| 0).as_deref(), Some("2024-02-29T06:30"), "no apostrophes");
        assert_eq!(local("D:20261010143000+02'", |_| 0).as_deref(), Some("2026-10-10T12:30"), "hours only");
        assert_eq!(local("D:20261010143000+02", |_| 0).as_deref(), Some("2026-10-10T12:30"));
    }

    #[test]
    fn a_date_without_a_zone_is_taken_as_written() {
        assert_eq!(local("D:20261010143000", |_| PDT).as_deref(), Some("2026-10-10T14:30"));
        assert_eq!(local("D:202610101430", |_| PDT).as_deref(), Some("2026-10-10T14:30"));
        assert_eq!(local("D:20261010", |_| PDT).as_deref(), Some("2026-10-10T00:00"), "a day alone");
        assert_eq!(local("D:20261010143000 ", |_| PDT).as_deref(), Some("2026-10-10T14:30"));
        // A zone that cannot be read is no zone.
        assert_eq!(local("D:20261010143000+2", |_| PDT).as_deref(), Some("2026-10-10T14:30"));
        assert_eq!(local("D:20261010143000+25'00'", |_| PDT).as_deref(), Some("2026-10-10T14:30"));
        assert_eq!(local("D:20261010143000 local", |_| PDT).as_deref(), Some("2026-10-10T14:30"));
    }

    #[test]
    fn what_is_not_a_date_is_none() {
        for raw in ["", "D:", "D:2026", "D:202610", "yesterday", "2026-10-10 14:30", "D:20261310", "D:20260230", "D:20261010246000Z", "D:20261010236000Z"] {
            assert_eq!(local(raw, |_| 0), None, "{raw:?}");
        }
        assert_eq!(local("D:20240229", |_| 0).as_deref(), Some("2024-02-29T00:00"), "a leap day");
        assert_eq!(local("D:20250229", |_| 0), None, "not a leap year");
    }

    #[test]
    fn the_offset_is_the_one_at_the_dates_own_instant() {
        // A zone whose offset changes at 2026-11-01T09:00Z, from -7 h to -8 h.
        let change = days_from_civil(2026, 11, 1) * 86_400 + 9 * 3_600;
        let zone = |utc: i64| if utc < change { -7 * 3_600 } else { -8 * 3_600 };
        assert_eq!(local("D:20261101083000Z", zone).as_deref(), Some("2026-11-01T01:30"));
        assert_eq!(local("D:20261101093000Z", zone).as_deref(), Some("2026-11-01T01:30"), "an hour later, the clock went back");
    }

    #[test]
    fn civil_days_round_trip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2026, 10, 11), 20_737);
        for days in [-719_468, -1, 0, 59, 60, 11_016, 20_737, 2_932_896] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days, "{y}-{m}-{d}");
        }
    }

    #[test]
    fn a_fixed_offset_holds_only_inside_and_this_device_answers_outside() {
        let device = offset_at(0);
        assert_eq!(with_offset(PDT, || offset_at(0)), PDT);
        assert_eq!(with_offset(3_600, || with_offset(PDT, || offset_at(0))), PDT, "the innermost wins");
        assert_eq!(offset_at(0), device, "restored");
        // This device's own offset is a real one, within a day of UTC.
        assert!(device.abs() <= 14 * 3_600, "{device}");
    }
}
