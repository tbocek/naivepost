//! F2.9 (Copy, Paste, Lane) — `spec/05-cut.md` F2.9.
//!
//! ⧉ Copy takes the selection in hand — a second at least, and the selection itself stays on the band,
//! because taking a copy is reading and not editing. ⧉ Paste puts those seconds at the red line: footage is
//! spliced in as a `copy:<seconds>` card, so the video gets longer and nothing filmed is lost; sound is laid
//! over the kept footage, one piece per stretch, replacing the recording it was copied from and leaving the
//! picture exactly as it was. ⇲ Lane gives the copy a row of its own — a cut lane windowing the file — with
//! nothing cut to it. Pasting consumes the copy; Esc drops it.
//!
//! Which of the two a paste is was settled by the band the selection was drawn on (F2.6's scope), not asked
//! again here: the hand carries that [`Scope`] and never re-reads the band, which may have been cleared or
//! moved somewhere else in between.
//!
//! No UI lives here and none is wired: `rust/src/ui/window.rs` renders only the Prepare page, so there are
//! no ⧉ Copy / ⧉ Paste / ⇲ Lane buttons to attach yet (as with F2.1–F2.8) and no widgets of this flow to
//! compare against a spec image. The widget layer will forward the three presses and print what these return.

use crate::cut::{Cut, Lane, Seg};
use crate::cut_select::{Scope, Selection, MIN_SCENE_SECONDS};
use crate::tools;

// --- S1: taking it in hand -------------------------------------------------------------------------------

/// F2.9 S1: the copy in hand — where its seconds start, how long they are, and what they are of. "What was
/// drawn on is what is copied": footage from the pictures (with the sound filmed with them), sound from a
/// lane. The scope travels with the seconds so the paste never has to ask the band again.
#[derive(Debug, Clone, PartialEq)]
pub struct Hand {
    pub from: f64,
    pub length: f64,
    pub scope: Scope,
}

impl Hand {
    /// Is this a stretch of footage, or of one recording's sound?
    fn is_footage(&self) -> bool {
        matches!(self.scope, Scope::Footage { .. })
    }

    /// The recording the hand holds the sound of, when it holds a sound.
    fn recording(&self) -> Option<&str> {
        match &self.scope {
            Scope::Sound { recording } => Some(recording),
            Scope::Footage { .. } => None,
        }
    }

    /// The camera row to show the pasted footage on — the row it was copied from. A sound has no row of its
    /// own, and the pieces it lays inherit the footage they stand over instead.
    fn row(&self) -> i32 {
        match self.scope {
            Scope::Footage { row } => row as i32,
            Scope::Sound { .. } => 0,
        }
    }
}

/// F2.9 S1: what ⧉ Copy did — took the seconds, refused for length, or found nothing marked.
#[derive(Debug, Clone, PartialEq)]
pub enum Take {
    Taken(Hand),
    TooShort(String),
    NothingSelected(&'static str),
}

/// F2.9 S1 (`Copy takes the selection in hand (≥ 1 s …)`; `// P.policy.minSceneSeconds`, read from
/// [`crate::cut_select`] where §10's row is catalogued).
///
/// Taking a copy is READING, not editing: this borrows the selection and clears nothing, so the band keeps
/// what it showed and the person can aim the paste with the source still visible. `from` normalises to the
/// lower end and `length` to the positive span, because F2.6 lets a drag run either way.
pub fn copy(selection: Option<&Selection>) -> Take {
    let Some(band) = selection else {
        return Take::NothingSelected(
            "select a stretch of the pictures or of a lane first \u{2014} \u{29c9} Copy takes the selection in hand",
        );
    };
    // A drag may run either way (F2.6), so the length is measured before anything is compared with the floor —
    // a right-to-left band is eight seconds, not minus eight, and refusing it for its sign would be absurd.
    let (from, end) = if band.start <= band.end { (band.start, band.end) } else { (band.end, band.start) };
    let length = end - from;
    if length < MIN_SCENE_SECONDS {
        // Two decimals: half a second must not read as "under 1 s" with a number that rounds to it.
        return Take::TooShort(format!(
            "the selection is {length:.2} s \u{2014} under 1 s there is nothing worth copying"
        ));
    }
    Take::Taken(Hand { from, length, scope: band.scope.clone() })
}

/// F2.9 S1: what the page says once the seconds are in hand — and where to take them next. A sound names its
/// recording, because "the footage at 4:10" is not what a lane's seconds are.
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

/// F2.9 S2: what a paste says when there is no line to paste at — the page has never been clicked, and
/// "the red line" is not yet a second (F2.4 owns where the line comes from).
pub const NO_LINE_YET: &str = "click the timeline where the copy goes first";

/// F2.9 S2 (`footage → spliced insert copy:<seconds> · the video gets longer`): the card a footage paste/// puts in the cut. `s == e` with a `dur` is what makes it SPLICED — the cut is opened there, those seconds
/// play again, and the footage carries on from the very next frame, so nothing filmed is lost.
///
/// The "file" is the session itself: `copy:<seconds>`, read back by [`Seg::copy_seconds`]. Three decimals,
/// because a hand-placed second like 12.345 must survive the round trip through the file. A card names no
/// asset of its own (`ins` carries the copy) and reads no file second: `ss` stays 0, so [`Seg::is_insert`]
/// reports it as spliced rather than overwriting.
pub fn spliced(hand: &Hand, at: f64) -> Seg {
    Seg { s: at, e: at, ins: format!("copy:{:.3}", hand.from), dur: hand.length, cam: hand.row(), ..Default::default() }
}

/// F2.9 S2 (`sound → laid over the kept footage at the line, one piece per kept stretch`): the kept footage
/// spans overlapping `[from, to)`, in time order. An insert is not a picture under a sound — only footage is
/// — which is what the refusal below asks for.
///
/// This is the generic "what kept footage lies under this span" query and deliberately applies no minimum: the
/// sound-over floor lives in [`sound_piece_overlap`], which is what the laying path filters through. Leaving it
/// unfiltered keeps this usable as a plain read (F2.10's hearing checks ask it that way) instead of quietly
/// answering two different questions with one function.
pub fn footage_stretches(cut: &Cut, from: f64, to: f64) -> Vec<(f64, f64)> {
    let mut spans: Vec<(f64, f64)> = cut
        .segs
        .iter()
        // An insert covers no footage of its own; a card is skipped whether spliced or overwriting.
        .filter(|seg| seg.ins.is_empty() && seg.s < to && seg.e > from)
        .map(|seg| (seg.s.max(from), seg.e.min(to)))
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    spans
}

/// `P.eng.soundMinPieceSeconds` (spec/10-parameters.md §5.1: 0.05, "under this a sound-over piece is not worth
/// a segment of its own"; prototype `sndMinLn`, gui/cut.go). The floor a piece of laid-over sound has to clear
/// before it earns a segment: below it the sound would be a blink, and the footage either side of it would have
/// been split for nothing.
///
/// Three floors look alike here and answer three different questions; this is only the third:
/// * [`crate::cut_select::MIN_SECONDS`] (`P.eng.minPieceSeconds`, 0.04) — is a remainder left by a REMOVAL
///   worth keeping at all;
/// * [`crate::fx_lane::MIN_BAND_SECONDS`] (`P.eng.effectMinSeconds`, 0.1) — how short an effect band may be
///   DRAGGED down to without vanishing from under the hand;
/// * this one — is a stretch of kept footage under a sound long enough that laying a piece over it buys
///   something rather than costing a split.
pub const MIN_SOUND_PIECE_SECONDS: f64 = 0.05;

/// F2.9 S2: the part of `[from, to)` that a segment offers to a laid-over sound, or `None` when there is
/// nothing worth laying. Two reasons for `None`, matching the prototype's one condition
/// (`if f.isInsert() || t1-t0 < sndMinLn`):
/// * the segment is an insert — a card or a spliced copy covers no footage of its own, so a sound cannot lie
///   over it and take the place of a recording;
/// * the clipped overlap is under [`MIN_SOUND_PIECE_SECONDS`] — a blink of sound, with a split of the picture
///   bought for it.
///
/// The boundary is tested as "not below" rather than "at or above": 0.05 is not representable in binary, so an
/// overlap clipped to exactly the floor can come out a ulp under it and would be thrown away as a blink. Same
/// shape as [`crate::cut_speed::press`]'s tolerance on `MIN_MARKED_SECONDS`.
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

/// F2.9 S2 (`sound → laid over the kept footage at the line, one piece per kept stretch`): the copied sound cut
/// into one piece per kept stretch of `[at, at + length)` — each carrying its own part of the file, so pieces
/// either side of a hole are parts of one sound and not its opening seconds twice. The caller splices them in
/// where they fall: footage before and after each piece is kept, so the picture is left exactly as it was.
///
/// `ss` walks with the SESSION second a piece stands at, hole and all: at 30 the copy is sixteen seconds in,
/// even though only six of its seconds played, because the sound being replaced was there too. `lane` does NOT
/// walk — every piece stands in for the same recording, named once for the whole span, because a hole in the
/// footage is no reason to change what is being replaced. Empty means there was no picture under the copy at
/// all, which is what the refusal below says.
pub fn lay_pieces(cut: &Cut, path: &str, hand: &Hand, at: f64, file_seconds: f64) -> Vec<Seg> {
    let to = at + hand.length;
    // Filtered through the floor rather than straight off `footage_stretches`: a kept stretch shorter than
    // MIN_SOUND_PIECE_SECONDS is not worth a piece, and laying none over it means no split either (see
    // [`lay_over`], which asks the same helper for the same answer).
    cut.segs
        .iter()
        .filter_map(|seg| sound_piece_overlap(seg, at, to))
        .map(|(start, stop)| Seg {
            s: start,
            e: stop,
            ins: path.to_string(),
            // The sound starts where the piece starts, and a piece beginning after a hole resumes mid-file.
            ss: file_seconds + (start - at),
            lane: hand.recording().unwrap_or_default().to_string(),
            ..Default::default()
        })
        .collect()
}

/// F2.9 S2: lay the copied sound over the kept footage at `at` and return how many pieces were laid (zero means
/// there was no picture under it).
///
/// The cut is rebuilt as before / piece / after for every stretch the span touched; footage the span never
/// reached stays in the list untouched, so the video does not get longer by a single second.
pub fn lay_over(cut: &mut Cut, path: &str, hand: &Hand, at: f64, file_seconds: f64) -> usize {
    let to = at + hand.length;
    let pieces = lay_pieces(cut, path, hand, at, file_seconds);
    if pieces.is_empty() {
        return 0;
    }
    // Every kept stretch the span touched is cut into before / piece / after; untouched footage stays in the
    // list as it was, so only the seconds under the copy change.
    let mut out: Vec<Seg> = Vec::with_capacity(cut.segs.len() + pieces.len() * 2);
    for seg in &cut.segs {
        if !seg.ins.is_empty() || seg.e <= at || seg.s >= to {
            out.push(seg.clone());
            continue;
        }
        // The important half of `P.eng.soundMinPieceSeconds`: an overlap under the floor lays no piece, and so
        // must not split anything either. Asked before head/tail are computed, because a split around a piece that
        // was never laid is exactly the cost the floor exists to avoid — the footage would come back in two
        // pieces with nothing heard over the join.
        if sound_piece_overlap(seg, at, to).is_none() {
            out.push(seg.clone());
            continue;
        }
        let head = (seg.s, seg.s.max(at));
        let tail = (seg.e.min(to), seg.e);
        if head.1 > head.0 {
            out.push(Seg { s: head.0, e: head.1, ..seg.clone() });
        }
        for piece in &pieces {
            if piece.s < seg.e && piece.e > seg.s {
                let (from, to) = (piece.s.max(seg.s), piece.e.min(seg.e));
                out.push(Seg { s: from, e: to, ..piece.clone() });
            }
        }
        if tail.1 > tail.0 {
            out.push(Seg { s: tail.0, e: tail.1, ..seg.clone() });
        }
    }
    cut.segs = out;
    pieces.len()
}

/// F2.9 S2 (`Pasting consumes the copy; Esc drops it`): the whole paste in one call, so "consumed only when
/// it happened" is a rule with one home rather than three callers agreeing about it.
///
/// `Err` for a page with no line yet ([`NO_LINE_YET`]), for a sound whose recording has left the session
/// ([`no_source`]) and for a sound with no picture under it ([`no_footage_status`]) — in every case the copy
/// STAYS in hand, because "the paste did not fail so much as
/// miss: the answer to missing is to move the red line and press again, not to go and copy the same seconds a
/// second time". `Ok(status)` consumes it.
///
/// `path` is the copied recording's project-relative path (what each laid piece names as its asset) and
/// `file_seconds` the file second the span starts at: converting session→file seconds is the caller's job, and
/// it clamps to the lane's own beginning (`max(from, recording start)`), because a selection that began before
/// the recording did has nothing earlier on that lane to play. A footage paste ignores both.
pub fn paste(
    cut: &mut Cut,
    hand: &mut Option<Hand>,
    at: Option<f64>,
    path: &str,
    file_seconds: f64,
    session: &[String],
) -> Result<String, String> {
    let Some(at) = at else { return Err(NO_LINE_YET.to_string()) };
    let Some(held) = hand.as_ref() else {
        // Nothing in hand: the button on the page is ⧉ Insert, not ⧉ Paste, so this is a call that should
        // not happen — said plainly rather than pasting nothing quietly.
        return Err("nothing is in hand \u{2014} \u{29c9} Copy takes a selection there first".to_string());
    };
    if held.is_footage() {
        let was = cut_length(cut);
        cut.segs.push(spliced(held, at));
        let from = held.from;
        let length = held.length;
        *hand = None;
        return Ok(pasted_status(length, from, at, cut_length(cut), was));
    }
    let recording = held.recording().unwrap_or_default().to_string();
    // Asked before anything is laid: a copy of a file the session no longer has must leave the cut as it was
    // AND stay in hand, so the person can still aim it at a recording that does exist.
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

/// The cut's own length in seconds — the total the paste reports before and after. Cards count for their
/// `dur`, footage for its span, which is what the page's "Cut" readout shows too: this calls
/// [`crate::cut_screen::cut_seconds`] rather than repeating it, so a paste can never report a total the screen
/// would not print.
fn cut_length(cut: &Cut) -> f64 {
    crate::cut_screen::cut_seconds(cut)
}

/// F2.9 S2: what a footage paste says — the seconds, where they came from, where they went, and both totals,
/// because "the video got longer" is only checkable against what it was.
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

/// F2.9 S2: what a sound paste says. Naming the stretches is the difference between a puzzling second marker
/// in the lanes and an expected one, so a copy that crossed a hole in the cut says so.
pub fn laid_status(length: f64, recording: &str, stretches: usize, at: f64) -> String {
    let over = if stretches > 1 { format!("{stretches} stretches of footage") } else { "the footage".to_string() };
    format!(
        "laid {} of {recording} over {over} at {}",
        tools::tenths(length),
        tools::mm_ss(at)
    )
}

/// F2.9 S2 (`refused: "the cut keeps no footage at m:ss — a sound needs a picture under it"`).
pub fn no_footage_status(at: f64) -> String {
    format!("the cut keeps no footage at {} \u{2014} a sound needs a picture under it", tools::mm_ss(at))
}

/// F2.9 S2 (`Esc drops it`): let go of the copy. `true` when there was one, so the page only says the copy is
/// dropped when something actually was in hand.
pub fn drop(hand: &mut Option<Hand>) -> bool {
    hand.take().is_some()
}

// --- S3: ⇲ Lane ------------------------------------------------------------------------------------------

/// §05-cut#8 (`Lane refuses "click the timeline where the new lane starts first"`): ⇲ Lane's own sentence for a
/// page never clicked. ⧉ Paste says the same thing differently ([`NO_LINE_YET`]) because it asks where the copy
/// GOES; a lane asks where it STARTS, and the two buttons must not answer one question with the other's words.
pub const NO_LANE_LINE_YET: &str = "click the timeline where the new lane starts first";

/// F2.9 S3: what a lane refuses a copy too short to be a window on — a row under [`MIN_SCENE_SECONDS`] is a
/// band no handle can be grabbed on, and a lane nobody can resize is not a row of its own.
pub fn too_short() -> &'static str {
    "that copy is too short to be a lane of its own"
}

/// §05-cut#8 (`Lane refuses "nothing is rolling at m:ss any more"`): the refusal when the second the new row
/// would start at lies outside every recording — a lane windowing a file from a minute nothing filmed has
/// nothing to window, and starting the row anyway would be a row of silence nobody asked for.
pub fn not_rolling(at: f64) -> String {
    format!("nothing is rolling at {} any more", tools::mm_ss(at))
}

/// §05-cut#8 (`Paste refuses "<base> is not in the session any more — the copied sound has nowhere to come
/// from"`): the refusal when the recording the copy was taken from is no longer one of the session's files.
pub fn no_source(base: &str) -> String {
    format!("{base} is not in the session any more \u{2014} the copied sound has nowhere to come from")
}

/// F2.9 S3: a name for the new lane that nothing already uses — `want`, then `want-2`, `want-3`… The name is
/// the lane's identity in `Cut::rows` and in every scene's `quiet` list, so a second copy of the same file
/// cannot share it without two rows answering to one speaker badge.
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

/// F2.9 S3 (`footage copy on its own row (a cut lane windowing the file), nothing cut yet`): where ⇲ Lane may
/// put the new row, decided from the seconds alone so the two refusals have one home: no line yet
/// ([`NO_LANE_LINE_YET`]), a line no recording is running at ([`not_rolling`]), or a copy too short for a row
/// of its own ([`too_short`]). `filmed` is the session's filmed runs — the same list the band is laid out from.
#[derive(Debug, Clone, PartialEq)]
pub enum LaneStart {
    Start(f64),
    Refusal(String),
}

pub fn lane_start(at: Option<f64>, hand: &Hand, filmed: &[(f64, f64)]) -> LaneStart {
    let Some(at) = at else { return LaneStart::Refusal(NO_LANE_LINE_YET.to_string()) };
    if !filmed.iter().any(|(from, to)| *from <= at && at < *to) {
        return LaneStart::Refusal(not_rolling(at));
    }
    if hand.length < MIN_SCENE_SECONDS {
        return LaneStart::Refusal(too_short().to_string());
    }
    LaneStart::Start(at)
}

/// F2.9 S3 (`footage copy on its own row (a cut lane windowing the file), nothing cut yet`): the lane this
/// copy becomes — `off` is the FILE second the copy was taken at, since "the copy was taken at a session
/// second, and the two differ by wherever that recording sits", and `at` is where the row starts on the clock.
///
/// The caller passes the name it settled on ([`crate::cut::Cut::lane_rows`] offers one per recording; this flow
/// offers `"Copied"`, suffixed past any row already taken — see [`lane_name`]), because a second copy of the
/// same file cannot share a name: `Lane::name` is what `Cut::rows` keys on and what every scene's `quiet` list
/// names. Nothing is cut to the new row: the caller adds no [`Seg`], because a lane that arrived already green
/// would be a cut nobody made — the row is offered, and ＋ Add is what lays scenes on it later.
pub fn lane(hand: &Hand, source: &str, file_seconds: f64, at: f64, name: String) -> Option<Lane> {
    if hand.length < MIN_SCENE_SECONDS {
        return None;
    }
    Some(Lane { name, src: source.to_string(), at, off: file_seconds, dur: hand.length })
}

/// F2.9 S3: what the lane says when it arrives — the seconds, where they came from, its name, and where it
/// starts.
pub fn lane_status(length: f64, from: f64, name: &str, at: f64) -> String {
    format!(
        "{} from {} is now the {name} lane, starting at {}",
        tools::tenths(length),
        tools::mm_ss(from),
        tools::mm_ss(at)
    )
}
