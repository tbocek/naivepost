//! F2.3 — ▶✂✂: review every cut, the joins rather than the kept material.
//!
//! What you want to know about a cut is whether the JOIN works — does the sentence run on, is the splice
//! in a silence, did the model take one word too many — and that is a few seconds either side of each
//! seam, not the minutes of kept material between them. So ▶✂✂ plays exactly that, one join after the
//! other, and stops when the last has been heard. The page forwards here and decides nothing
//! (spec/00-principles.md §5).

use crate::{
    cut::{Cut, Seg},
    cut_screen,
    preview::Player,
    tools,
};

/// How much of the finished video is heard on each side of a join — `P.policy.reviewPadSeconds`
/// (spec/10-parameters.md:85), held here because this is the module whose rule reads it.
pub const REVIEW_PAD_SECONDS: f64 = 10.0;

/// S1's refusal. The greyed rule itself is [`cut_screen::can_review`]'s — one place decides whether ▶✂✂
/// has anything to do, and this only words the answer.
pub fn refused(cut: &Cut) -> Option<&'static str> {
    (!cut_screen::can_review(cut))
        .then_some("nothing to review \u{2014} a cut needs two clips to have a join between them")
}

/// How many joins there are to hear: one fewer than the clips, and none at all for one clip.
pub fn joins(cut: &Cut) -> usize {
    cut.segs.len().saturating_sub(1)
}

/// One join's stretch of the cut: what gets heard before it and after it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    pub from: f64,
    pub to: f64,
}

/// The seam between `segs[join]` and `segs[join + 1]`, padded by [`REVIEW_PAD_SECONDS`] of the cut on
/// each side — clamped to the two clips, because what lies before the earlier clip's start is a removed
/// stretch rather than the cut, and a review that rewound into one would show the person footage their
/// own edit took out. A card (`s == e`) has no length of its own, so its lead comes from the clip before
/// it for the same reason.
pub fn window(segs: &[Seg], join: usize) -> Window {
    let (before, after) = (&segs[join], &segs[join + 1]);
    Window {
        from: before.s.max(before.e - REVIEW_PAD_SECONDS),
        to: after.e.min(after.s + REVIEW_PAD_SECONDS),
    }
}

/// Where a review started at the red line begins (S2).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Start {
    /// The line is already inside a join's window: play on from where it stands.
    PlayOn,
    /// The line is short of it — go to the run-up.
    SeekTo(f64),
}

/// S2: "Start from the red line". A review paused to look at a join resumes at that join, and a line
/// parked between two joins goes to the next one. Past the last window it wraps to the first — there is
/// nothing ahead left to hear, and a review that refuses to start is worse than one that starts over.
pub fn start_from_line(cut: &Cut, line: f64) -> (usize, Start) {
    for join in 0..joins(cut) {
        let Window { from, to } = window(&cut.segs, join);
        if line < from {
            return (join, Start::SeekTo(from));
        }
        if line < to {
            return (join, Start::PlayOn);
        }
    }
    // Past the last window, or a cut with no joins at all: start again from the first.
    let first = if cut.segs.len() >= 2 { window(&cut.segs, 0).from } else { 0.0 };
    (0, Start::SeekTo(first))
}

/// S4's status, naming which join out of how many and where it is in the session. The pad is spelled
/// from [`REVIEW_PAD_SECONDS`], so a retuned parameter cannot leave the sentence saying ten.
pub fn reviewing_status(cut: &Cut, join: usize) -> String {
    format!(
        "reviewing cut {} of {} \u{2014} {:.0} s before and after the join at {}",
        join + 1,
        joins(cut),
        REVIEW_PAD_SECONDS,
        tools::mm_ss(cut.segs[join].e)
    )
}

// --- S3 to S5: the hand on the line ------------------------------------------------------------------

/// The review's whole state: which join is being heard, and whether the line has been seen inside its
/// window at all.
///
/// That second flag is what tells a moved line from a seek that has not landed. A line found before a
/// window it had already been inside is somebody's hand on the timeline and the review is over; found
/// there before it has ever been inside, it is the review's own seek — the player answers the old
/// position for a tick or two after a flushing seek, and nothing at all on the first tick of a freshly
/// cued file — and the answer is to wait rather than to declare the person changed their mind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Review {
    pub join: usize,
    pub armed: bool,
}

/// S2's start, with `armed` set from it: playing on means the line is already in the window, a seek has
/// not arrived yet.
pub fn start(cut: &Cut, line: f64) -> Review {
    let (join, begun) = start_from_line(cut, line);
    Review { join, armed: matches!(begun, Start::PlayOn) }
}

/// What the review does with the line this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    /// The join is still being heard, or the seek has not landed.
    Stay,
    /// The seconds after this join have played: go to the next join's run-up (S3).
    Seek,
    /// The last join has been heard (S4).
    Done,
    /// Somebody moved the line (S5).
    Lost,
}

/// One tick of the review (S3's "once the seconds after a join have played, seek to the next run-up, or
/// play on into it if already behind the line").
pub fn tick(cut: &Cut, review: &mut Review, line: f64) -> Move {
    let last = joins(cut).saturating_sub(1);
    if review.join > last {
        return Move::Done;
    }

    let here = window(&cut.segs, review.join);
    if line < here.from {
        return if review.armed { Move::Lost } else { Move::Stay };
    }
    if line < here.to {
        review.armed = true;
        return Move::Stay;
    }

    // Past this join: walk forward to the next one worth hearing. A run-up already under the line is not
    // sought but played into, so a stretch of short clips is heard once, in order, and never rewound for
    // every seam inside it.
    while review.join < last {
        review.join += 1;
        review.armed = false;
        let next = window(&cut.segs, review.join);
        if line < next.from {
            return Move::Seek;
        }
        if line < next.to {
            review.armed = true;
            return Move::Stay;
        }
    }
    Move::Done
}

/// S5's sentence for a hand on the line. The pause needs no sentence of its own — see [`PAUSE_STATUS`].
pub const LINE_MOVED: &str = "the line was moved \u{2014} the cut review is over; ▶✂✂ starts it again";

/// What pressing ▶✂✂ while it is the running one answers. §A gives that press a ⏸ face and no status, so
/// the button's own live tooltip is what it reports: pausing its own preview is not an event worth a
/// sentence over the icon that just changed.
pub const PAUSE_STATUS: &str = "pause the cut review";

/// S4's closing status.
pub fn reviewed_all(cut: &Cut) -> String {
    format!("reviewed all {} cuts", joins(cut))
}

/// What one press of ▶✂✂ did.
#[derive(Debug, Clone, PartialEq)]
pub enum Pressed {
    PausedAndEnded { status: &'static str },
    Refused(&'static str),
    Started { join: usize, seek_to: Option<f64>, status: String },
}

/// ▶✂✂ pressed (S5's first half, then S1 and S2).
pub fn pressed(player: &mut Player, cut: &Cut) -> Pressed {
    // S5: while it is the one running, it pauses — and pausing ends the review, because resuming from
    // here on is ▶✂'s job: play the cut from wherever the line stands.
    if player.reviewing && player.transport.playing {
        player.transport.playing = false;
        player.reviewing = false;
        return Pressed::PausedAndEnded { status: PAUSE_STATUS };
    }

    if let Some(reason) = refused(cut) {
        return Pressed::Refused(reason);
    }

    // The review IS the ▶✂ preview with one more hand on the line (S3's "play as the cut"), so it takes
    // the cut preview on rather than being a fourth kind of preview.
    player.reviewing = true;
    player.cut_only = true;
    player.transport.started = true;
    player.transport.playing = true;

    let line = player.playhead.unwrap_or(0.0);
    let (join, begun) = start_from_line(cut, line);
    let seek_to = match begun {
        Start::PlayOn => None,
        Start::SeekTo(from) => Some(from),
    };
    // A play-on leaves the line where it stands; a seek moves it to the run-up now, so the status and the
    // player agree before the first tick.
    player.playhead = seek_to.or(player.playhead);
    Pressed::Started { join, seek_to, status: reviewing_status(cut, join) }
}

/// ▶ or ▶✂ pressed during a review (S5's second half): the review ends and nothing else changes — "each
/// wears ⏸ only while its own thing runs; pressing another switches the preview without stopping; exactly
/// one is lit". Whatever is playing plays on as the plain cut preview.
///
/// Only ▶✂ needs this: [`crate::preview::press_recording`] already clears `reviewing` when it switches to
/// the recording, and keeps the transport running while it does. This is the switch that STAYS on the cut.
pub fn switch_over(player: &mut Player) -> bool {
    let was = player.reviewing;
    player.reviewing = false;
    was
}
