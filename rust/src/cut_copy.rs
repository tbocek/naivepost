//! F2.9 (Copy, Paste, Lane) — `spec/05-cut.md` F2.9.

use crate::cut::{Cut, Lane, Seg};
use crate::cut_select::{Scope, Selection, MIN_SCENE_SECONDS};
use crate::timeline::Recording;
use crate::{timeline, tools};

/// F2.9: the three buttons' labels and tooltips, spelled once here so the page's widgets and the
/// tests that read them cannot drift apart.
pub const COPY_LABEL: &str = "\u{29c9} Copy";
pub const COPY_TIP: &str =
    "take the selection in hand \u{2014} a second at least; the band keeps showing it";
pub const PASTE_LABEL: &str = "\u{29c9} Paste";
pub const PASTE_TIP: &str = "put what is in hand where the red line is \u{2014} footage splices in, sound lays over kept footage";
pub const LANE_LABEL: &str = "\u{21f2} Lane";
pub const LANE_TIP: &str = "give the copy a row of its own \u{2014} nothing is cut to it yet";

/// F2.9 S2: what Esc says when it drops the copy. Plain words, because the person asked for nothing
/// to happen and the status line is where the app answers.
pub const DROPPED: &str = "the copy is dropped";

// --- S1: taking it in hand -------------------------------------------------------------------------------

/// F2.9 S1: the copy in hand — where its seconds start, how long they are, and what they are of.
/// "What was drawn on is what is copied": footage from the pictures (with the sound filmed with
/// them), sound from a lane. The scope travels with the seconds so the paste never has to ask the
/// band again.
#[derive(Debug, Clone, PartialEq)]
pub struct Hand {
    pub from: f64,
    pub length: f64,
    pub scope: Scope,
}

impl Hand {
    fn is_footage(&self) -> bool {
        matches!(self.scope, Scope::Footage { .. })
    }

    /// F2.9: the file a sound copy came from; a footage copy is windowed from it too (S3).
    pub fn recording(&self) -> Option<&str> {
        match &self.scope {
            Scope::Sound { recording } => Some(recording),
            Scope::Footage { .. } => None,
        }
    }

    fn row(&self) -> i32 {
        match self.scope {
            Scope::Footage { row } if row != crate::cut_select::ANY_ROW => row as i32,
            Scope::Footage { .. } => 0,
            Scope::Sound { .. } => 0,
        }
    }
}

/// F2.9: the three buttons as `(widget name, label, tooltip)`, in the order the spec lists them.
pub const BUTTONS: [(&str, &str, &str); 3] = [
    ("copy-button", COPY_LABEL, COPY_TIP),
    ("paste-button", PASTE_LABEL, PASTE_TIP),
    ("lane-button", LANE_LABEL, LANE_TIP),
];

/// F2.9 S1: what ⧉ Copy did — took the seconds, refused for length, or found nothing marked.
#[derive(Debug, Clone, PartialEq)]
pub enum Take {
    Taken(Hand),
    TooShort(String),
    NothingSelected(&'static str),
}

/// F2.9 S1 (`Copy takes the selection in hand (≥ 1 s …)`; `// P.policy.minSceneSeconds`, read from
/// [`crate::cut_select`] where §10's row is catalogued). `from` normalises to the lower end and
/// `length` to the positive span, because F2.6 lets a drag run either way.
pub fn copy(selection: Option<&Selection>) -> Take {
    let Some(band) = selection else {
        return Take::NothingSelected(
            "select a stretch of the pictures or of a lane first \u{2014} \u{29c9} Copy takes the selection in hand",
        );
    };
    // A drag may run either way (F2.6), so the length is measured before anything is compared with
    // the floor.
    let (from, end) = if band.start <= band.end {
        (band.start, band.end)
    } else {
        (band.end, band.start)
    };
    let length = end - from;
    if length < MIN_SCENE_SECONDS {
        // Two decimals: half a second must not read as "under 1 s" with a number that rounds to it.
        return Take::TooShort(format!(
            "the selection is {length:.2} s \u{2014} under 1 s there is nothing worth copying"
        ));
    }
    Take::Taken(Hand {
        from,
        length,
        scope: band.scope.clone(),
    })
}

/// F2.9 S1: what the page says once the seconds are in hand.
pub fn copied_status(hand: &Hand) -> String {
    let span = (hand.from, hand.from + hand.length);
    match &hand.scope {
        Scope::Sound { recording } => format!(
            "copied {} of {recording} ({} \u{2013} {}) \u{2014} click where it goes, then \u{29c9} Paste",
            tools::tenths(hand.length),
            tools::mm_ss(span.0),
            tools::mm_ss(span.1)
        ),
        Scope::Footage { .. } => format!(
            "copied {} \u{2013} {} ({}) \u{2014} click where it goes, then \u{29c9} Paste",
            tools::mm_ss(span.0),
            tools::mm_ss(span.1),
            tools::tenths(hand.length)
        ),
    }
}

// --- S2: the paste at the red line ------------------------------------------------------------------------

/// F2.9 S2: what a paste says when there is no line to paste at.
pub const NO_LINE_YET: &str = "click the timeline where the copy goes first";

/// F2.9 S2 (`footage → spliced insert copy:<seconds> · the video gets longer`): the card a footage
/// paste/// puts in the cut. Three decimals, because a hand-placed second like 12.345 must survive
/// the round trip through the file.
pub fn spliced(hand: &Hand, at: f64) -> Seg {
    Seg {
        s: at,
        e: at,
        ins: format!("copy:{:.3}", hand.from),
        dur: hand.length,
        cam: hand.row(),
        ..Default::default()
    }
}

/// F2.9 S2 (`sound → laid over the kept footage at the line, one piece per kept stretch`): the kept
/// footage spans overlapping `[from, to)`, in time order.
pub fn footage_stretches(cut: &Cut, from: f64, to: f64) -> Vec<(f64, f64)> {
    let mut spans: Vec<(f64, f64)> = cut
        .segs
        .iter()
        .filter(|seg| seg.ins.is_empty() && seg.s < to && seg.e > from)
        .map(|seg| (seg.s.max(from), seg.e.min(to)))
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    spans
}

/// The floor a piece of laid-over sound has to clear before it earns a segment: below it the sound
/// would be a blink, and the footage either side of it would have been split for nothing.
pub const MIN_SOUND_PIECE_SECONDS: f64 = 0.05;

/// F2.9 S2: the part of `[from, to)` that a segment offers to a laid-over sound, or `None` when
/// there is nothing worth laying.
pub fn sound_piece_overlap(seg: &Seg, from: f64, to: f64) -> Option<(f64, f64)> {
    if !seg.ins.is_empty() {
        return None;
    }
    let start = seg.s.max(from);
    let stop = seg.e.min(to);
    let overlap = stop - start;
    if overlap > MIN_SOUND_PIECE_SECONDS || (overlap - MIN_SOUND_PIECE_SECONDS).abs() < 1e-9 {
        Some((start, stop))
    } else {
        None
    }
}

/// F2.9 S2 (`sound → laid over the kept footage at the line, one piece per kept stretch`): the
/// copied sound cut into one piece per kept stretch of `[at, at + length)` — each carrying its own
/// part of the file, so pieces either side of a hole are parts of one sound and not its opening
/// seconds twice.
pub fn lay_pieces(cut: &Cut, path: &str, hand: &Hand, at: f64, file_seconds: f64) -> Vec<Seg> {
    let to = at + hand.length;
    // Filtered through the floor rather than straight off `footage_stretches`: a kept stretch
    // shorter than MIN_SOUND_PIECE_SECONDS is not worth a piece, and laying none over it means no
    // split either (see [`lay_over`], which asks the same helper for the same answer).
    cut.segs
        .iter()
        .filter_map(|seg| sound_piece_overlap(seg, at, to))
        .map(|(start, stop)| Seg {
            s: start,
            e: stop,
            ins: path.to_string(),
            // The sound starts where the piece starts, and a piece beginning after a hole resumes
            // mid-file.
            ss: file_seconds + (start - at),
            lane: hand.recording().unwrap_or_default().to_string(),
            ..Default::default()
        })
        .collect()
}

/// F2.9 S2: lay the copied sound over the kept footage at `at` and return how many pieces were laid
/// (zero means there was no picture under it).
pub fn lay_over(cut: &mut Cut, path: &str, hand: &Hand, at: f64, file_seconds: f64) -> usize {
    let to = at + hand.length;
    let pieces = lay_pieces(cut, path, hand, at, file_seconds);
    if pieces.is_empty() {
        return 0;
    }
    // Every kept stretch the span touched is cut into before / piece / after.
    let mut out: Vec<Seg> = Vec::with_capacity(cut.segs.len() + pieces.len() * 2);
    for seg in &cut.segs {
        if !seg.ins.is_empty() || seg.e <= at || seg.s >= to {
            out.push(seg.clone());
            continue;
        }
        // The important half of `P.eng.soundMinPieceSeconds`: an overlap under the floor lays no
        // piece, and so must not split anything either.
        if sound_piece_overlap(seg, at, to).is_none() {
            out.push(seg.clone());
            continue;
        }
        let head = (seg.s, seg.s.max(at));
        let tail = (seg.e.min(to), seg.e);
        if head.1 > head.0 {
            out.push(Seg {
                s: head.0,
                e: head.1,
                ..seg.clone()
            });
        }
        for piece in &pieces {
            if piece.s < seg.e && piece.e > seg.s {
                let (from, to) = (piece.s.max(seg.s), piece.e.min(seg.e));
                out.push(Seg {
                    s: from,
                    e: to,
                    cam: seg.cam,
                    quiet: seg.quiet.clone(),
                    ..piece.clone()
                });
            }
        }
        if tail.1 > tail.0 {
            out.push(Seg {
                s: tail.0,
                e: tail.1,
                ..seg.clone()
            });
        }
    }
    cut.segs = out;
    pieces.len()
}

/// F2.9 S2/S3: which second INSIDE the source the hand's opening second is.
pub fn file_seconds(cut: &Cut, filmed: &[Recording], hand: &Hand) -> f64 {
    let recording = match hand.recording() {
        Some(base) => filmed.iter().find(|rec| rec.base == base).cloned(),
        None => None,
    }
    .or_else(|| {
        let by_time = hand.recording().is_none_or(|base| {
            base.strip_prefix("cam")
                .is_some_and(|row| row.parse::<u32>().is_ok())
        });
        by_time
            .then(|| {
                filmed
                    .iter()
                    .find(|rec| rec.start <= hand.from && hand.from < rec.end)
                    .cloned()
            })
            .flatten()
    });
    let Some(recording) = recording else {
        return 0.0;
    };
    let off = cut
        .lanes
        .iter()
        .find(|lane| lane.name == recording.base)
        .map_or(0.0, |lane| lane.off);
    timeline::local_at(
        hand.from,
        recording.start,
        off,
        timeline::shift_of(cut, &recording.base),
    )
    .max(0.0)
}

/// F2.9 S2 (`Pasting consumes the copy; Esc drops it`): the whole paste in one call, so "consumed
/// only when it happened" is a rule with one home rather than three callers agreeing about it.
pub fn paste(
    cut: &mut Cut,
    hand: &mut Option<Hand>,
    at: Option<f64>,
    path: &str,
    file_seconds: f64,
    session: &[String],
) -> Result<String, String> {
    let Some(at) = at else {
        return Err(NO_LINE_YET.to_string());
    };
    let Some(held) = hand.as_ref() else {
        return Err(
            "nothing is in hand \u{2014} \u{29c9} Copy takes a selection there first".to_string(),
        );
    };
    if held.is_footage() {
        let was = crate::cut_screen::cut_seconds(cut);
        cut.segs.push(spliced(held, at));
        let from = held.from;
        let length = held.length;
        *hand = None;
        return Ok(pasted_status(
            length,
            from,
            at,
            crate::cut_screen::cut_seconds(cut),
            was,
        ));
    }
    let recording = held.recording().unwrap_or_default().to_string();
    if !session.iter().any(|base| base == &recording) {
        return Err(no_source(&recording));
    }
    let length = held.length;
    let pieces = lay_over(cut, path, held, at, file_seconds);
    if pieces == 0 {
        return Err(no_footage_status(at));
    }
    *hand = None;
    Ok(laid_status(length, &recording, pieces, at))
}

/// F2.9 S2: what a footage paste says — the seconds, where they came from, where they went, and
/// both totals, because "the video got longer" is only checkable against what it was.
pub fn pasted_status(length: f64, from: f64, at: f64, now: f64, was: f64) -> String {
    format!(
        "pasted {} from {} at {} \u{2014} the cut is {}, was {}",
        tools::tenths(length),
        tools::mm_ss(from),
        tools::mm_ss(at),
        tools::mm_ss(now),
        tools::mm_ss(was)
    )
}

/// F2.9 S2: what a sound paste says.
pub fn laid_status(length: f64, recording: &str, stretches: usize, at: f64) -> String {
    let over = if stretches > 1 {
        format!("{stretches} stretches of footage")
    } else {
        "the footage".to_string()
    };
    format!(
        "laid {} of {recording} over {over} at {}",
        tools::tenths(length),
        tools::mm_ss(at)
    )
}

/// F2.9 S2 (`refused: "the cut keeps no footage at m:ss — a sound needs a picture under it"`).
pub fn no_footage_status(at: f64) -> String {
    format!(
        "the cut keeps no footage at {} \u{2014} a sound needs a picture under it",
        tools::mm_ss(at)
    )
}

/// F2.9 S2 (`Esc drops it`): let go of the copy.
pub fn drop(hand: &mut Option<Hand>) -> bool {
    hand.take().is_some()
}

// --- S3: ⇲ Lane ------------------------------------------------------------------------------------------

/// §05-cut#8 (`Lane refuses "click the timeline where the new lane starts first"`): ⇲ Lane's own
/// sentence for a page never clicked. ⧉ Paste says the same thing differently ([`NO_LINE_YET`])
/// because it asks where the copy GOES.
pub const NO_LANE_LINE_YET: &str = "click the timeline where the new lane starts first";

/// F2.9 S3: what a lane refuses a copy too short to be a window on.
pub fn too_short() -> &'static str {
    "that copy is too short to be a lane of its own"
}

/// §05-cut#8 (`Lane refuses "nothing is rolling at m:ss any more"`): the refusal when the second
/// the new row would start at lies outside every recording.
pub fn not_rolling(at: f64) -> String {
    format!("nothing is rolling at {} any more", tools::mm_ss(at))
}

pub fn no_source(base: &str) -> String {
    format!(
        "{base} is not in the session any more \u{2014} the copied sound has nowhere to come from"
    )
}

/// F2.9 S3: a name for the new lane that nothing already uses.
pub fn lane_name(want: &str, taken: &[String]) -> String {
    if !taken.iter().any(|name| name == want) {
        return want.to_string();
    }
    let mut n = 2;
    loop {
        let try_name = format!("{want}-{n}");
        if !taken.iter().any(|name| name == &try_name) {
            return try_name;
        }
        n += 1;
    }
}

/// F2.9 S3 (`footage copy on its own row (a cut lane windowing the file), nothing cut yet`): where
/// ⇲ Lane may put the new row, decided from the seconds alone so the two refusals have one home: no
/// line yet ([`NO_LANE_LINE_YET`]), a line no recording is running at ([`not_rolling`]), or a copy
/// too short for a row of its own ([`too_short`]).
#[derive(Debug, Clone, PartialEq)]
pub enum LaneStart {
    Start(f64),
    Refusal(String),
}

pub fn lane_start(at: Option<f64>, hand: &Hand, filmed: &[(f64, f64)]) -> LaneStart {
    let Some(at) = at else {
        return LaneStart::Refusal(NO_LANE_LINE_YET.to_string());
    };
    if !filmed.iter().any(|(from, to)| *from <= at && at < *to) {
        return LaneStart::Refusal(not_rolling(at));
    }
    if hand.length < MIN_SCENE_SECONDS {
        return LaneStart::Refusal(too_short().to_string());
    }
    LaneStart::Start(at)
}

/// F2.9 S3 (`footage copy on its own row (a cut lane windowing the file), nothing cut yet`): the
/// lane this copy becomes.
pub fn lane(hand: &Hand, source: &str, file_seconds: f64, at: f64, name: String) -> Option<Lane> {
    if hand.length < MIN_SCENE_SECONDS {
        return None;
    }
    Some(Lane {
        name,
        src: source.to_string(),
        at,
        off: file_seconds,
        dur: hand.length,
    })
}

/// F2.9 S3: what the lane says when it arrives — the seconds, where they came from, its name, and
/// where it starts.
pub fn lane_status(length: f64, from: f64, name: &str, at: f64) -> String {
    format!(
        "{} from {} is now the {name} lane, starting at {}",
        tools::tenths(length),
        tools::mm_ss(from),
        tools::mm_ss(at)
    )
}
