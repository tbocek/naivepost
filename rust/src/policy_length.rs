//! §12 entry 21 — reading a target length out of the User Context's own words.
//!
//! The prototype found the length with a regex over free text and had no fallback: "half an hour" matched
//! nothing, became `targetLengthSeconds: 0`, and that single silent miss changed three things at once — the
//! cut brief the model was sent, both segment-count gates and the length gate. §12's answer is that the
//! number is a policy field (`P.policy.targetLengthSeconds`), derived by F0.7 and shown in the policy form,
//! so a missed reading is visible instead of load-bearing.
//!
//! This module is that reading, and nothing else: words in, seconds out. It is deliberately not a model call
//! — a length a person typed in plain words is arithmetic, not interpretation, and asking a model costs a
//! round trip to learn what the sentence already says. F0.7's derivation still runs alongside it and can
//! still set the field from prose a parser would never catch ("about the length of a podcast episode"); this
//! covers the plainly-spelled lengths that used to be missed for want of one more regex arm.
//!
//! Two facts a caller must be able to tell apart, so [`Reading`] carries both: no length mentioned, and a
//! length mentioned that this reader cannot spell ("a while"). Both leave the field at 0, but only one of
//! them is worth saying out loud — the other is what a person who never mentions a length expects.

/// One read of a User Context: the seconds, and whether the text actually spoke a length.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    /// The length in seconds, or [`crate::tools::cutpass::NO_TARGET`] when nothing was understood.
    pub seconds: f64,
    /// Whether the text named a length at all, even one that could not be parsed. A `false` here is the
    /// ordinary case (most contexts talk only about style) and must not be reported as a failure.
    pub mentioned: bool,
}

impl Reading {
    /// The 0 = none answer with nothing claimed: no length, nothing to say.
    pub const NONE: Reading = Reading {
        seconds: crate::tools::cutpass::NO_TARGET,
        mentioned: false,
    };
}

/// Spelled number words, lower case, as they appear inside a length phrase ("twenty minutes", "two hours").
///
/// A named list rather than a spelled-out grammar on purpose: §12 entry 21 asks that "twenty minutes" stop
/// being a silent miss, and every value here is one a person actually writes. Anything beyond nineteen is
/// read as digits, which is what people type for an odd length ("45 min", "90 seconds"); the words are for
/// the round numbers prose prefers.
const SPELLED_NUMBERS: [(&str, f64); 20] = [
    ("one", 1.0),
    ("two", 2.0),
    ("three", 3.0),
    ("four", 4.0),
    ("five", 5.0),
    ("six", 6.0),
    ("seven", 7.0),
    ("eight", 8.0),
    ("nine", 9.0),
    ("ten", 10.0),
    ("eleven", 11.0),
    ("twelve", 12.0),
    ("thirteen", 13.0),
    ("fourteen", 14.0),
    ("fifteen", 15.0),
    ("sixteen", 16.0),
    ("seventeen", 17.0),
    ("eighteen", 18.0),
    ("nineteen", 19.0),
    ("twenty", 20.0),
];

/// `half` on its own, which is not in the list above because it is never a count of anything.
const HALF: &str = "half";

/// The unit words, mapped to the seconds one of them counts. Plurals are handled by trimming an `s`, so
/// only the singular is listed. A phrase must carry exactly one of these; that is also what makes a number
/// a length rather than a stray digit in the prose.
const UNITS: [(&str, f64); 5] = [
    ("second", 1.0),
    ("sec", 1.0),
    ("minute", 60.0),
    ("min", 60.0),
    ("hour", 3600.0),
];

/// Read a plain length out of the User Context's words. Returns [`Reading::NONE`] when the text speaks no
/// length, and a `mentioned: true` reading with 0 seconds when it speaks one this reader cannot spell —
/// the caller tells those two apart in its log, which is the whole point of the entry.
///
/// Recognised shapes, case-insensitive, anywhere in the prose:
/// * `45 min`, `90 seconds`, `2 hours`, `1h`, `30m` — a digit directly followed by a unit;
/// * `twenty minutes`, `half an hour`, `an hour and a half`, `two and a half hours` — the spelled forms;
/// * `1h20` — a spelled-out hour with trailing minutes, the way a duration gets typed in a hurry.
pub fn target_length_of(context: &str) -> Reading {
    let text = context.to_lowercase();
    // Half FIRST, and before any article rule: "half an hour" contains the phrase "an hour", so a reader that
    // looked for `an hour` first would answer 3600 for a stated 1800. §12's own example is this one.
    if let Some(seconds) = read_half(&text) {
        return Reading {
            seconds,
            mentioned: true,
        };
    }
    // `an hour`, `a minute` — the indefinite article is a count of one, and nothing else reads it because no
    // digit or spelled number sits before the unit.
    if let Some(seconds) = read_article(&text) {
        return Reading {
            seconds,
            mentioned: true,
        };
    }
    // `1h20`: the hour form with minutes glued on. Checked before the general digit arm because the same
    // digits read as `1 h` = 3600 would otherwise swallow the `20`.
    if let Some(seconds) = read_hour_minutes(&text) {
        return Reading {
            seconds,
            mentioned: true,
        };
    }
    // A spelled count next to a spelled unit: "twenty minutes", "two hours".
    if let Some(seconds) = read_spelled(&text) {
        return Reading {
            seconds,
            mentioned: true,
        };
    }
    // A digit next to a unit, spelled or abbreviated: "45 min", "90 seconds", "2h", "30m".
    if let Some(seconds) = read_digits(&text) {
        return Reading {
            seconds,
            mentioned: true,
        };
    }
    // Nothing parseable. Did the text still try to name a length? "a while", "short and punchy", "the length
    // of a train journey" all mention duration without a number this reader can act on, and the log has to
    // say "a length was mentioned and not understood" rather than "no length".
    Reading {
        seconds: crate::tools::cutpass::NO_TARGET,
        mentioned: mentions_length(&text),
    }
}

/// `half an hour`, `half hour`, `half-hour`, and `an hour and a half`.
fn read_half(text: &str) -> Option<f64> {
    if !text.contains(HALF) {
        return None;
    }
    let hour = unit_seconds("hour")?;
    if !text.contains(HALF) {
        return None;
    }
    // "an hour and a half" has no leading `half`, so a bare hour plus the phrase counts as 1.5.
    if text.contains("half an hour") || text.contains("half hour") || text.contains("half-hour") {
        return Some(hour * 0.5);
    }
    if text.contains("hour and a half") || text.contains("hours and a half") {
        return Some(hour * 1.5);
    }
    None
}

/// `1h20` — an hour count glued to two-digit minutes. Only the glued form: a space means the digit arm
/// reads it, and two numbers with a space between them are more likely two separate thoughts.
fn read_hour_minutes(text: &str) -> Option<f64> {
    let bytes: Vec<char> = text.chars().collect();
    for (index, ch) in bytes.iter().enumerate() {
        if *ch != 'h' {
            continue;
        }
        // The digits before `h` are the hours...
        let mut hour_start = index;
        while hour_start > 0 && bytes[hour_start - 1].is_ascii_digit() {
            hour_start -= 1;
        }
        if hour_start == index {
            continue;
        }
        let hours: f64 = bytes[hour_start..index]
            .iter()
            .collect::<String>()
            .parse()
            .ok()?;
        // ...and up to two digits straight after it are the minutes.
        let mut end = index + 1;
        let mut digits = String::new();
        while end < bytes.len() && bytes[end].is_ascii_digit() && digits.len() < 2 {
            digits.push(bytes[end]);
            end += 1;
        }
        if digits.len() != 2 {
            continue;
        }
        let minutes: f64 = digits.parse().ok()?;
        if minutes >= 60.0 {
            continue;
        }
        return Some(hours * 3600.0 + minutes * 60.0);
    }
    None
}

/// `an hour`, `a minute`, `a few seconds`-free: the indefinite article standing in for the count one.
/// Matched with a following space so the unit is a word boundary, and only for units that spell a length.
fn read_article(text: &str) -> Option<f64> {
    for unit in ["hour", "minute", "second"] {
        for article in ["an ", "a "] {
            for suffix in ["", "s"] {
                if text.contains(&format!("{article}{unit}{suffix} "))
                    || text.ends_with(&format!("{article}{unit}{suffix}"))
                {
                    return unit_seconds(unit);
                }
            }
        }
    }
    None
}

/// A spelled count immediately before a spelled unit: `twenty minutes`, `three hours`.
fn read_spelled(text: &str) -> Option<f64> {
    for (unit, seconds_each) in UNITS {
        for suffix in ["s", ""] {
            let word = format!("{unit}{suffix}");
            // Search for the unit as a whole word, then read the word just before it.
            let Some(at) = find_word(text, &word) else {
                continue;
            };
            let before = text[..at].trim_end();
            let last_word = before
                .split(|c: char| !c.is_alphabetic())
                .next_back()
                .unwrap_or("");
            if let Some((_, count)) = SPELLED_NUMBERS.iter().find(|(name, _)| *name == last_word) {
                return Some(count * seconds_each);
            }
        }
    }
    None
}

/// A digit run followed by a unit word or its one-letter abbreviation: `45 min`, `90s`, `2h`, `30m`.
fn read_digits(text: &str) -> Option<f64> {
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        if !chars[index].is_ascii_digit() {
            index += 1;
            continue;
        }
        let mut number = String::new();
        while index < chars.len() && chars[index].is_ascii_digit() {
            number.push(chars[index]);
            index += 1;
        }
        // Skip one space, then look for a unit: `45 min` and `45min` both count; `45 minutes` counts
        // through the plural.
        let mut after = index;
        while after < chars.len() && (chars[after] == ' ' || chars[after] == '-') {
            after += 1;
        }
        let tail: String = chars[after..].iter().collect();
        if let Some(seconds_each) = unit_at_start(&tail) {
            let count: f64 = number.parse().ok()?;
            return Some(count * seconds_each);
        }
    }
    None
}

/// The unit seconds when `tail` starts with a unit word (plurals trimmed), else `None`.
fn unit_at_start(tail: &str) -> Option<f64> {
    for (unit, seconds_each) in UNITS {
        if tail.starts_with(unit) {
            return Some(seconds_each);
        }
    }
    // The one-letter abbreviations, and only as a word: `s` inside "seconds" is already handled above, and
    // matching `m` in "minutes" here would answer twice for the same phrase.
    let first = tail.chars().next()?;
    match first {
        's' => Some(1.0),
        'm' => Some(60.0),
        'h' => Some(3600.0),
        _ => None,
    }
}

/// Whether the text talks about a length at all, for the log line that says a mentioned one went unread.
///
/// The words are duration nouns and duration adjectives, and "how long" — the question a person writes when
/// they have a length in mind but typed no number. Deliberately narrow: "no length mentioned" has to answer
/// `false`, or every context that never mentions a duration would be logged as an unread one.
fn mentions_length(text: &str) -> bool {
    for word in [
        "length",
        "duration",
        "runtime",
        "run time",
        "how long",
        "too long",
        "as long as",
    ] {
        if text.contains(word) {
            return true;
        }
    }
    // "a while", "a bit", "ages": indefinite durations. Phrased as a phrase, not a bare word, so "while
    // editing" (a perfectly ordinary clause about workflow) does not read as a mentioned length.
    for phrase in [
        "a while",
        "for ages",
        "a bit long",
        "keep it short",
        "short and",
    ] {
        if text.contains(phrase) {
            return true;
        }
    }
    false
}

/// The seconds one unit counts, looked up by its singular name.
fn unit_seconds(unit: &str) -> Option<f64> {
    UNITS
        .iter()
        .find(|(name, _)| *name == unit)
        .map(|(_, seconds)| *seconds)
}

/// Position of `word` in `text` when it stands as a whole word (not a piece of a longer one).
fn find_word(text: &str, word: &str) -> Option<usize> {
    let mut from = 0;
    while let Some(offset) = text[from..].find(word) {
        let at = from + offset;
        let before_ok = at == 0 || !text.as_bytes()[at - 1].is_ascii_alphabetic();
        let after_at = at + word.len();
        let after_ok = after_at >= text.len() || !text.as_bytes()[after_at].is_ascii_alphabetic();
        if before_ok && after_ok {
            return Some(at);
        }
        from = at + word.len();
    }
    None
}
