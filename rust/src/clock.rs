//! The session clock, spec/01-project-and-files.md §8.

use std::path::Path;
use std::sync::LazyLock;

use regex::bytes::Regex;

static TS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"((?:19|20)[0-9]{2})[-._]?([0-9]{2})[-._]?([0-9]{2})(?:\s?[aA][tT]\s|[-_T. ]{0,3})(?:(?:([0-9]{2})[-.:_]?)?([0-9]{2})[-.:_]?([0-9]{2})|([0-9])[-.:_]([0-9]{2})[-.:_]([0-9]{2}))(?:\s?([aApP][mM]))?",
    )
    .expect("the stamp pattern compiles")
});

static EPOCH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|[^0-9])(1[5-9][0-9]{8})(?:$|[^0-9])").expect("the epoch pattern compiles")
});

/// The instant a file name claims, in seconds, or `None` when it names none.
pub fn name_stamp(name: &str) -> Option<f64> {
    let base = Path::new(name)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(name);
    let bytes = base.as_bytes();
    for caps in TS_RE.captures_iter(bytes) {
        if let Some(seconds) = parse_stamp(&caps) {
            return Some(seconds);
        }
    }
    EPOCH_RE
        .captures(bytes)
        .and_then(|c| group(&c, 1)?.parse().ok())
}

fn parse_stamp(caps: &regex::bytes::Captures) -> Option<f64> {
    let year = group(caps, 1)?.parse().ok()?;
    let month = group(caps, 2)?.parse().ok()?;
    let day = group(caps, 3)?.parse().ok()?;
    let (mut hour, minute, second) = match (group(caps, 4), group(caps, 5), group(caps, 6)) {
        (Some(h), Some(m), Some(s)) => (h.parse().ok()?, m.parse().ok()?, s.parse().ok()?),
        _ => (
            format!("0{}", group(caps, 7)?).parse().ok()?,
            group(caps, 8)?.parse().ok()?,
            group(caps, 9)?.parse().ok()?,
        ),
    };
    if let Some(marker) = group(caps, 10) {
        let pm = marker.eq_ignore_ascii_case("pm");
        if pm && hour < 12 {
            hour += 12;
        } else if !pm && hour == 12 {
            hour = 0;
        }
    }
    if !valid(year, month, day, hour, minute, second) {
        return None;
    }
    Some(seconds_from_civil(year, month, day, hour, minute, second))
}

/// One captured group as text, `None` when the alternation left it out. Names are UTF-8 and the
/// patterns only match ASCII, so this cannot fail on anything they matched.
fn group<'a>(caps: &'a regex::bytes::Captures<'_>, n: usize) -> Option<&'a str> {
    std::str::from_utf8(caps.get(n)?.as_bytes()).ok()
}

/// One source's place on the session clock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Source {
    /// The wall-clock instant this file names, in seconds — [`Clock::zero`] plus the seconds
    /// on the session clock. An unstamped file carries the zero itself.
    pub at: f64,
    /// Did its own name carry the timestamp, or did it have to be placed at the start?
    pub stamped: bool,
}

/// Where a session's sources sit on one wall clock, and where second nought falls.
#[derive(Debug, Clone, PartialEq)]
pub struct Clock {
    /// The earliest stamped instant; 0.0 when nothing in the session is stamped.
    pub zero: f64,
    /// One entry per name handed to [`clock`], in that order.
    pub sources: Vec<Source>,
}

impl Clock {
    /// Seconds on the session clock for source `i`.
    pub fn offset(&self, i: usize) -> f64 {
        self.sources[i].at - self.zero
    }

    /// Does this source's Inputs row show the warning — its name names no moment, so it was
    /// placed at the session start and lined up by ear rather than by a stamp?
    pub fn needs_warning(&self, i: usize) -> bool {
        !self.sources[i].stamped
    }
}

/// Put a session's sources on one clock.
pub fn clock(names: &[&str]) -> Clock {
    let stamps: Vec<Option<f64>> = names.iter().map(|n| name_stamp(n)).collect();
    let zero = stamps
        .iter()
        .flatten()
        .fold(f64::INFINITY, |m, t| m.min(*t));
    let zero = if zero.is_finite() { zero } else { 0.0 };
    let sources = stamps
        .iter()
        // An unstamped file sits at the session's start — never at its mtime — where a drag
        // on the row can line it up by ear.
        .map(|t| match t {
            Some(at) => Source {
                at: *at,
                stamped: true,
            },
            None => Source {
                at: zero,
                stamped: false,
            },
        })
        .collect();
    Clock { zero, sources }
}

/// A frame's name on the file's own clock: `YYYY-MM-DD_HH-MM-SS.mmm`, §1's
/// `inputs/frames/<source>/` grid.
pub fn frame_name(seconds: f64) -> String {
    let whole = seconds.floor();
    // Rounded up here rather than truncated: 1.9999 s is the third millisecond of the second
    // it is in, and printing `.999` for it would name a frame that never was.
    let millis = ((seconds - whole) * 1000.0).round() as i64;
    let (whole, millis) = if millis == 1000 {
        (whole + 1.0, 0)
    } else {
        (whole, millis)
    };
    // Rounding to nearest after a floor can land on -0.001 of the second below, and `floor`
    // of that is the second before; clamping keeps a frame inside its own second.
    let millis = millis.clamp(0, 999);
    let (y, mo, d, h, mi, s) = civil_from_seconds(whole);
    format!("{y:04}-{mo:02}-{d:02}_{h:02}-{mi:02}-{s:02}.{millis:03}")
}

pub const SESSION_NAMED_BEFORE: f64 = 1.0e9;

/// The session second a frame name stands for, given the session's zero ([`Clock::zero`]): a real name is its
/// instant less the zero, and a name from before [`SESSION_NAMED_BEFORE`] already counts session seconds.
pub fn frame_session_seconds(name: &str, zero: f64) -> Option<f64> {
    let at = read_frame(name)?;
    Some(if at < SESSION_NAMED_BEFORE {
        at
    } else {
        at - zero
    })
}

pub fn read_frame(name: &str) -> Option<f64> {
    let b = name.as_bytes();
    let digit = |i: usize| b.get(i).is_some_and(u8::is_ascii_digit);
    if b.len() < 19
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'_'
        || b[13] != b'-'
        || b[16] != b'-'
        || ![0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18]
            .iter()
            .all(|i| digit(*i))
    {
        return None;
    }
    // Milliseconds when the name carries them; a whole-second name ends, or goes on to its extension, after
    // the seconds.
    let millis_given = b.get(19) == Some(&b'.') && digit(20);
    if millis_given && !(digit(21) && digit(22) && !digit(23)) {
        return None;
    }
    if !millis_given && b.len() > 19 && b[19] != b'.' {
        return None;
    }
    let num = |s: &str| s.parse::<i64>().ok();
    let year = num(str::from_utf8(&b[0..4]).ok()?)?;
    let month = num(str::from_utf8(&b[5..7]).ok()?)?;
    let day = num(str::from_utf8(&b[8..10]).ok()?)?;
    let hour = num(str::from_utf8(&b[11..13]).ok()?)?;
    let min = num(str::from_utf8(&b[14..16]).ok()?)?;
    let sec = num(str::from_utf8(&b[17..19]).ok()?)?;
    let millis = if millis_given {
        num(str::from_utf8(&b[20..23]).ok()?)?
    } else {
        0
    };
    if !valid(year, month, day, hour, min, sec) || !(0..=999).contains(&millis) {
        return None;
    }
    Some(seconds_from_civil(year, month, day, hour, min, sec) + millis as f64 / 1000.0)
}

fn valid(year: i64, month: i64, day: i64, hour: i64, min: i64, sec: i64) -> bool {
    (1900..=2100).contains(&year)
        && (1..=12).contains(&month)
        && day >= 1
        && day <= days_in_month(year, month)
        && (0..24).contains(&hour)
        && (0..60).contains(&min)
        && (0..60).contains(&sec)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
    }
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// The civil date for days since 1970-01-01 — [`days_from_civil`] backwards.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn seconds_from_civil(y: i64, mo: i64, d: i64, h: i64, mi: i64, s: i64) -> f64 {
    (days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + s) as f64
}

fn civil_from_seconds(seconds: f64) -> (i64, i64, i64, i64, i64, i64) {
    let secs = seconds.floor() as i64;
    let days = secs.div_euclid(86400);
    let rest = secs.rem_euclid(86400);
    let (y, mo, d) = civil_from_days(days);
    (y, mo, d, rest / 3600, rest % 3600 / 60, rest % 60)
}
