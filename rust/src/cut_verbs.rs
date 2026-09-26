//! F2.7 (Add, Split, Remove, ⌦) — `spec/05-cut.md` F2.7.
//!
//! The three buttons that act on the band F2.6 drew, and the key that acts on whatever is in hand. All
//! four share one shape: they read the cut, answer with a new segment list and the ONE sentence the status
//! line shows, and never touch anything outside the span they were aimed at. Nothing here imports UI —
//! [`crate::ui::window`] draws the buttons, forwards the press to these functions and prints what comes
//! back; it decides none of it.
//!
//! Two rules from §J hold this module together:
//! * rule 13 — Remove takes exactly the selection, floored at [`cut_select::MIN_SECONDS`]
//!   (`P.eng.minPieceSeconds`); the 1 s floor ([`cut_select::MIN_SCENE_SECONDS`],
//!   `P.policy.minSceneSeconds`) is only for proposing and copying, which is why ＋ Add refuses below it
//!   and － Remove does not ask.
//! * rule 14 — a Split border survives coalesce until merged by a drag. That is literally
//!   [`cut::Seg::split`]: the right half of a split carries it, so no automatic pass may join it back.

use crate::cut::{Fx, Seg};
use crate::cut_select::{Scope, Selection, ANY_ROW, MIN_SECONDS, MIN_SCENE_SECONDS};

/// The three F2.7 verbs as (widget name, label, tooltip), in the spec's order. Held here rather than in
/// the page so the buttons a test finds by name and the rules it presses are declared together.
pub const BUTTONS: [(&str, &str, &str); 3] = [
    (
        "add-button",
        "\u{ff0b} Add",
        "keep the selection as scenes \u{2014} one per filmed run",
    ),
    (
        "split-button",
        "| Split",
        "a border at each end of the selection \u{2014} nothing is removed",
    ),
    (
        "remove-button",
        "\u{ff0d} Remove",
        "drop exactly the selection \u{2014} the cut keeps everything else until you undo",
    ),
];

/// What ＋ Add warns with while a sound selection makes it the wrong button. `None` when its plain
/// tooltip is right; the refusal after a press is [`add`]'s own sentence.
pub fn add_tip_now(sel: Option<&Selection>) -> Option<String> {
    match sel.map(|band| &band.scope) {
        Some(Scope::Sound { recording }) => Some(format!(
            "\u{ff0b} Add keeps footage \u{2014} this selection is {recording}'s sound"
        )),
        _ => None,
    }
}

/// `P.policy.snapToleranceSeconds` — how far an edge may move to a word edge, a silence or a visual cut.
/// Reached through its owner rather than restated here.
pub use crate::tools::cutpass::SNAP_TOLERANCE_SECONDS;

/// What one verb did. `Applied` carries the cut as it is now, the sentence to print, and two things the
/// page must do after printing: whether the band stays up (Split leaves it deliberately, so the person
/// can see the borders they asked for) and whether something landed in the hand (a no-selection Split
/// hands over the right half).
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Applied {
        status: String,
        segs: Vec<Seg>,
        keeps_selection: bool,
        into_hand: bool,
    },
    Refused(String),
}

impl Outcome {
    /// The sentence either way — the status line reads one call and does not branch on the outcome.
    pub fn status(&self) -> &str {
        match self {
            Outcome::Applied { status, .. } => status,
            Outcome::Refused(reason) => reason,
        }
    }
}

/// The undo tail ＋ Add and ⌦ end with. Spelled once: the sentences above it differ, never this part.
const UNDO_TAIL: &str = "\u{21b6} Undo (Ctrl+Z) takes it back";

/// － Remove's own tail — `spec/05-cut.md` F2.7 writes that one without the shortcut in parentheses
/// ("removed X s — N scene(s), was M (↶ Undo takes it back)"), so it is a separate constant rather
/// than a trimmed copy of [`UNDO_TAIL`].
const REMOVE_UNDO_TAIL: &str = "\u{21b6} Undo takes it back";

/// A scene kept by ＋ Add: footage of `row`, running `start..end`, nothing spliced over it.
fn kept_scene(row: usize, start: f64, end: f64) -> Seg {
    Seg {
        s: start,
        e: end,
        cam: row as i32,
        ..Default::default()
    }
}

// --- S1: ＋ Add -------------------------------------------------------------------------------------

/// F2.7 ＋ Add (`keep the selection as scenes`). One kept scene per filmed run inside the band, both
/// ends snapped, and the span taken off every other row.
///
/// `snap_points` are the places an end may land on instead of where it was drawn — word edges, silences,
/// line edges and visual cuts — and an end moves only to the nearest one within
/// [`SNAP_TOLERANCE_SECONDS`] (`// P.policy.snapToleranceSeconds`, the "within 5 s" of the spec).
/// Points further than that are not candidates: an end dragged 30 s to a word edge would keep footage
/// nobody selected.
///
/// Which of the three sentences the status is depends on rows, not on how much was added: one row touched
/// says plain "added", more than one row but nothing lost elsewhere names the camera, and losing coverage
/// on another row says so too — the failure this guards is a silent theft of another camera's scene.
pub fn add(sel: Option<&Selection>, segs: &[Seg], snap_points: &[f64]) -> Outcome {
    let Some(band) = sel else {
        return Outcome::Refused("drag a region on a track first".to_string());
    };
    if let Scope::Sound { recording } = &band.scope {
        return Outcome::Refused(format!(
            "\u{ff0b} Add keeps footage \u{2014} the selection is {recording}'s sound"
        ));
    }
    if band.length() < MIN_SCENE_SECONDS {
        // P.policy.minSceneSeconds
        return Outcome::Refused(format!(
            "nothing to add: {} selected, a scene is 1 s or more",
            crate::tools::tenths(band.length())
        ));
    }

    let start = snap_end(band.start, snap_points);
    let end = snap_end(band.end, snap_points);
    // Snapping may walk the two ends onto each other; a zero-length band keeps nothing.
    if end - start < MIN_SECONDS {
        return Outcome::Refused(format!(
            "nothing to add: {} selected, a scene is 1 s or more",
            crate::tools::tenths(band.length())
        ));
    }

    let row = match band.scope {
        Scope::Footage { row } => row,
        Scope::Sound { .. } => unreachable!("a sound selection refused above"),
    };
    let targets: Vec<usize> = if row == ANY_ROW {
        distinct_rows(segs)
    } else {
        vec![row]
    };

    // Was any OTHER row covering these seconds before the add? Asked on the list as it was, because the
    // kept scene is added afterwards and would otherwise look like a row that stole from itself.
    let stolen_from: Option<i32> = segs
        .iter()
        .filter(|seg| is_footage(seg) && !targets.contains(&(seg.cam as usize)))
        .find(|seg| (seg.e.min(end) - seg.s.max(start)).max(0.0) >= MIN_SECONDS)
        .map(|seg| seg.cam);

    let mut out: Vec<Seg> = Vec::with_capacity(segs.len() + targets.len());
    for seg in segs {
        if !is_footage(seg) {
            out.push(seg.clone());
            continue;
        }
        if targets.contains(&(seg.cam as usize)) {
            // This row is being added to: keep what lies outside the band, then the kept scene itself.
            if seg.s < start {
                out.push(clip(seg, seg.s, start));
            }
            if seg.e > end {
                out.push(clip(seg, end, seg.e));
            }
            continue;
        }
        // A row outside `targets` loses whatever the band covered on it. "The other camera" is always
        // one of THESE rows — the row being added to can never be the one robbed of the seconds.
        out.extend(subtract_span(seg, start, end));
    }
    for target in &targets {
        out.push(kept_scene(*target, start, end));
    }
    out.sort_by(|a, b| a.s.partial_cmp(&b.s).unwrap_or(std::cmp::Ordering::Equal));

    let status = match (targets.len(), stolen_from) {
        // Another row lost coverage of these seconds: name the camera that now shows them (the row added
        // to) and admit the other one lost them.
        (_, Some(_)) => format!(
            "added on {}, and taken off the other camera \u{2014} {UNDO_TAIL}",
            cam_name(targets[0] as i32)
        ),
        // More than one row covered, nothing lost: name the row the pictures came from.
        (n, _) if n > 1 => format!("added on {} \u{2014} {UNDO_TAIL}", cam_name(0)),
        // One row touched and nothing stolen — including a single-camera project adding over its own scene,
        // where the kept scene replaces what was already there rather than arriving anywhere new.
        _ => format!("added \u{2014} {UNDO_TAIL}"),
    };
    Outcome::Applied {
        status,
        segs: out,
        keeps_selection: false,
        into_hand: false,
    }
}

/// F2.7 ＋ Add (`both ends snap ... within 5 s`): the nearest candidate within reach, else the drawn second.
///
/// Kept separate so a test can pin the reach without building a whole cut: a point at 4.9 s away moves,
/// one at 5.1 s does not. P.policy.snapToleranceSeconds
pub fn snap_end(at: f64, points: &[f64]) -> f64 {
    let mut best: Option<(f64, f64)> = None;
    for &point in points {
        let distance = (point - at).abs();
        if distance > SNAP_TOLERANCE_SECONDS {
            continue;
        }
        if best.map_or(true, |(closest, _)| distance < closest) {
            best = Some((distance, point));
        }
    }
    best.map(|(_, point)| point).unwrap_or(at)
}

// --- S2: | Split ------------------------------------------------------------------------------------

/// F2.7 | Split (`a border at each end of the selection, nothing removed`).
///
/// With a footage band both ends become borders and the RIGHT half of every scene cut there carries
/// [`Seg::split`], so coalescing cannot undo the split (§J rule 14). The band deliberately stays up —
/// `keeps_selection: true` — because the person is looking at the borders they just asked for.
///
/// With no band the border goes at the red line and the right half is taken in hand (`into_hand: true`),
/// which is what makes "split, then move the second half" one gesture rather than three.
///
/// A border that lands on an edge the cut already has is NOT drawn, and the sentence names only the
/// borders actually made: saying "split at 10 and 20" when 20 was already a scene edge would report work
/// that did not happen.
pub fn split(sel: Option<&Selection>, line: f64, segs: &[Seg]) -> Outcome {
    if let Some(Selection { scope: Scope::Sound { recording }, .. }) = sel {
        return Outcome::Refused(format!(
            "| Split cuts footage \u{2014} the selection is {recording}'s sound"
        ));
    }

    let (from, to) = match sel {
        Some(band) => (band.start, band.end),
        None => (line, line),
    };

    // With no band the question is different: a zero-width span keeps nothing by construction, so what
    // decides here is whether the red line stands on kept footage at all.
    if sel.is_none() {
        if !footage_under_line(segs, line) {
            return Outcome::Refused(
                "| Split cuts at the red line \u{2014} click a track to put it somewhere".to_string(),
            );
        }
        let drawn = draw_borders(segs, &[line]);
        if drawn.len() == segs.len() {
            return Outcome::Refused(format!("nothing to split at {}", clock(line)));
        }
        return Outcome::Applied {
            status: format!(
                "split at {} \u{2014} {} scenes, was {}",
                clock(line),
                drawn.len(),
                segs.len()
            ),
            segs: drawn,
            keeps_selection: false,
            into_hand: true,
        };
    }

    if !footage_kept_between(segs, from, to) {
        return Outcome::Refused(format!(
            "nothing to split: the cut keeps nothing between {} and {}",
            crate::tools::mm_ss(from),
            crate::tools::mm_ss(to)
        ));
    }

    // No selection: one border at the red line, right half in hand.
    if sel.is_none() {
        let drawn = draw_borders(segs, &[line]);
        if drawn.is_empty() {
            return Outcome::Refused(format!("nothing to split at {}", clock(line)));
        }
        return Outcome::Applied {
            status: format!(
                "split at {} \u{2014} {} scenes, was {}",
                clock(line),
                segs.len(),
                segs.len()
            ),
            segs: drawn,
            keeps_selection: false,
            into_hand: true,
        };
    }

    let both = dedup_ends(from, to);
    if both.len() < 2 || (to - from) < 2.0 * MIN_SECONDS {
        return Outcome::Refused(format!(
            "nothing to split at {} \u{2013} {}",
            clock(from),
            clock(to)
        ));
    }

    let drawn = draw_borders(segs, &both);
    let named: Vec<String> = both
        .iter()
        .filter(|edge| a_border_was_drawn_at(segs, **edge))
        .map(|edge| clock(*edge))
        .collect();
    if named.is_empty() {
        return Outcome::Refused(format!(
            "nothing to split at {} \u{2013} {}",
            clock(from),
            clock(to)
        ));
    }

    let status = if named.len() == 2 {
        format!(
            "split at {} and {} \u{2014} {} scenes, was {}",
            named[0],
            named[1],
            drawn.len(),
            segs.len()
        )
    } else {
        format!(
            "split at {} \u{2014} {} scenes, was {}",
            named[0],
            drawn.len(),
            segs.len()
        )
    };
    Outcome::Applied {
        status,
        segs: drawn,
        keeps_selection: true,
        into_hand: false,
    }
}

/// F2.7 | Split (`halves ≥ 0.04 s`): cut every scene that straddles `edge` in two, marking the RIGHT
/// half so no automatic pass joins it back. A scene merely touching the edge is already bordered and is
/// left alone. P.eng.minPieceSeconds
fn draw_borders(segs: &[Seg], edges: &[f64]) -> Vec<Seg> {
    let mut out: Vec<Seg> = Vec::with_capacity(segs.len() + edges.len());
    for seg in segs {
        let mut pieces = vec![(seg.clone(), false)];
        for &edge in edges {
            let mut next: Vec<(Seg, bool)> = Vec::with_capacity(pieces.len() + 1);
            for (piece, marked) in pieces {
                if edge <= piece.s + MIN_SECONDS || edge >= piece.e - MIN_SECONDS {
                    next.push((piece, marked));
                    continue;
                }
                next.push((clip(&piece, piece.s, edge), marked));
                let mut right = clip(&piece, edge, piece.e);
                // Only a real split marks: a piece carried over from an earlier border keeps its own flag.
                right.split = true;
                next.push((right, true));
            }
            pieces = next;
        }
        out.extend(pieces.into_iter().map(|(piece, _)| piece));
    }
    out.sort_by(|a, b| a.s.partial_cmp(&b.s).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// Was a border actually made at `edge`? Yes when some scene straddles it — a scene that merely starts or
/// ends there means the border the person asked for already existed, and the sentence must not claim it.
fn a_border_was_drawn_at(segs: &[Seg], edge: f64) -> bool {
    segs.iter()
        .any(|seg| edge > seg.s + MIN_SECONDS && edge < seg.e - MIN_SECONDS)
}

// --- S3: － Remove ----------------------------------------------------------------------------------

/// F2.7 － Remove (`drop exactly the selection`). Remainders of at least [`MIN_SECONDS`] survive
/// (`P.eng.minPieceSeconds`); shorter slivers are dropped with the selection rather than left as a
/// sub-frame flicker.
pub fn remove(sel: Option<&Selection>, segs: &[Seg]) -> Outcome {
    let Some(band) = sel else {
        return Outcome::Refused("drag a region on a track first".to_string());
    };
    if let Scope::Sound { recording } = &band.scope {
        return Outcome::Refused(format!(
            "\u{ff0d} Remove drops footage \u{2014} the selection is {recording}'s sound"
        ));
    }
    if !footage_kept_between(segs, band.start, band.end) {
        return Outcome::Refused(format!(
            "nothing to remove: the cut keeps nothing between {} and {}",
            crate::tools::mm_ss(band.start),
            crate::tools::mm_ss(band.end)
        ));
    }

    let mut out: Vec<Seg> = Vec::with_capacity(segs.len());
    let mut dropped = 0.0_f64;
    for seg in segs {
        if !is_footage(seg) {
            out.push(seg.clone());
            continue;
        }
        let overlap = (seg.e.min(band.end) - seg.s.max(band.start)).max(0.0);
        if overlap <= 0.0 {
            out.push(seg.clone());
            continue;
        }
        dropped += overlap;
        for piece in subtract_span(seg, band.start, band.end) {
            out.push(piece);
        }
    }
    out.sort_by(|a, b| a.s.partial_cmp(&b.s).unwrap_or(std::cmp::Ordering::Equal));

    let status = if scene_split_in_two(segs, band.start, band.end) {
        format!(
            "removed {} \u{2014} the scene it went through is two now ({REMOVE_UNDO_TAIL})",
            crate::tools::tenths(dropped)
        )
    } else {
        format!(
            "removed {} \u{2014} {} scene(s), was {} ({REMOVE_UNDO_TAIL})",
            crate::tools::tenths(dropped),
            out.iter().filter(|s| is_footage(s)).count(),
            segs.iter().filter(|s| is_footage(s)).count()
        )
    };
    Outcome::Applied {
        status,
        segs: out,
        keeps_selection: false,
        into_hand: false,
    }
}

/// The "scene it went through is two now" case: the band sat inside ONE kept scene and left a piece on
/// each side of the gap, so the removal turned that one scene into two. A band starting or ending at an
/// existing edge is not this case — nothing was cut in two, only trimmed back.
fn scene_split_in_two(segs: &[Seg], start: f64, end: f64) -> bool {
    let straddlers: Vec<&Seg> = segs
        .iter()
        .filter(|seg| {
            is_footage(seg)
                && seg.s < start - MIN_SECONDS
                && seg.e > end + MIN_SECONDS
                && start - seg.s >= MIN_SECONDS
                && seg.e - end >= MIN_SECONDS
        })
        .collect();
    straddlers.len() == 1
}

// --- S4: ⌦ / Delete / BackSpace ----------------------------------------------------------------------

/// F2.7 ⌦ (`in order: a held effect; a held clip; a selection; the scene under the line`).
///
/// The order is the whole rule and is tested as such: a person who holds an effect AND has a band wants
/// the effect gone, because the effect is what they last touched. Taking the band instead would delete
/// footage to answer a press aimed at a decoration.
///
/// A held clip is `the only way to remove a spliced card` (§J rule 8): an insert is a file, never
/// trimmed or dropped by a re-suggest, so ⌦ on the card in hand is the only door it has.
///
/// A sound selection still refuses through [`crate::cut_delete::refuses`] — that refusal is §8's wording
/// and its tests, and dropping footage cannot be pointed at a lane's seconds.
pub fn delete_verb(
    held_effect: Option<&Fx>,
    held_clip: Option<&Seg>,
    sel: Option<&Selection>,
    scene_under_line: Option<&Seg>,
    segs: &[Seg],
) -> Outcome {
    if held_effect.is_some() {
        // First branch, and nothing else is looked at: the effect is what the hand last touched.
        return Outcome::Applied {
            status: format!("removed the effect \u{2014} {UNDO_TAIL}"),
            segs: Vec::new(),
            keeps_selection: false,
            into_hand: false,
        };
    }
    if let Some(card) = held_clip {
        // The only door a spliced card has (§J rule 8), so it outranks even a live selection.
        return Outcome::Applied {
            status: format!(
                "removed the card \u{2014} {} ({REMOVE_UNDO_TAIL})",
                card.ins.as_str()
            ),
            segs: Vec::new(),
            keeps_selection: false,
            into_hand: false,
        };
    }
    if let Some(band) = sel {
        if let Scope::Sound { recording } = &band.scope {
            return Outcome::Refused(crate::cut_delete::drops_footage(recording));
        }
        // "removed — N segment(s), was M": N is the pieces of footage the band takes, M what it leaves.
        let taken = segs
            .iter()
            .filter(|seg| is_footage(seg) && (seg.e.min(band.end) - seg.s.max(band.start)) > 0.0)
            .count();
        let left = segs.iter().filter(|seg| is_footage(seg)).count();
        return Outcome::Applied {
            status: format!("removed \u{2014} {taken} segment(s), was {left}"),
            segs: Vec::new(),
            keeps_selection: false,
            into_hand: false,
        };
    }
    if scene_under_line.is_some() {
        return Outcome::Applied {
            status: format!("removed the scene under the line \u{2014} {UNDO_TAIL}"),
            segs: Vec::new(),
            keeps_selection: false,
            into_hand: false,
        };
    }
    Outcome::Refused(
        "nothing selected \u{2014} click a kept scene, or drag a region on a track".to_string(),
    )
}

// --- S5: the ✕ badges -------------------------------------------------------------------------------

/// F2.7 ✕ badges (`on a green bar, on the selection, on a cut lane, on an empty row, on an effect`):
/// the five places a ✕ appears on the Cut page, each meaning "put this down / take this away".
///
/// Only [`Badge::Selection`] has its widget today — that is F2.6 S2's `clear-selection`. The other four
/// draw with their own surface rounds (the green bar and the empty row with F2.8/F2.11, the cut lane with
/// F2.10, the effect with the effects-lane rounds), so this enumeration exists so the five meanings are
/// stated once and a test can pin them before the pixels arrive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Badge {
    /// On a green bar: drop that scene.
    GreenBar,
    /// On the selection: clear it — nothing is removed (`clear-selection`, F2.6 S2).
    Selection,
    /// On a cut lane: take the lane away.
    CutLane,
    /// On an empty row: drop the row.
    EmptyRow,
    /// On an effect: put the effect down.
    Effect,
}

/// The five places, in the order the spec lists them.
pub const BADGE_TARGETS: [Badge; 5] = [
    Badge::GreenBar,
    Badge::Selection,
    Badge::CutLane,
    Badge::EmptyRow,
    Badge::Effect,
];

/// What pressing the ✕ at `badge` means, in the words the tooltip uses.
pub fn badge_action(badge: Badge) -> &'static str {
    match badge {
        Badge::GreenBar => "drop that scene",
        Badge::Selection => "clear the selection \u{2014} nothing is removed",
        Badge::CutLane => "take this lane away",
        Badge::EmptyRow => "drop this row",
        Badge::Effect => "put this effect down",
    }
}

// --- shared helpers ---------------------------------------------------------------------------------

/// Is this segment filmed footage rather than an inserted card? An insert carries a path in `ins`; footage
/// carries none. (Wider than [`Seg::is_insert`], which also counts a spliced copy: for these verbs a
/// pasted stretch is not footage to add, split or remove.)
fn is_footage(seg: &Seg) -> bool {
    seg.ins.is_empty()
}

/// A copy of `seg` narrowed to `start..end`, keeping everything that describes it.
fn clip(seg: &Seg, start: f64, end: f64) -> Seg {
    let mut out = seg.clone();
    out.s = start;
    out.e = end;
    out
}

fn duration_of(seg: &Seg) -> f64 {
    (seg.e - seg.s).max(0.0)
}

/// The pieces of `seg` with `span_start..span_end` punched out. A remainder shorter than
/// [`MIN_SECONDS`] is dropped rather than kept: a sliver under a frame plays as a flicker.
/// P.eng.minPieceSeconds
fn subtract_span(seg: &Seg, span_start: f64, span_end: f64) -> Vec<Seg> {
    let mut out = Vec::with_capacity(2);
    if seg.s < span_start {
        let piece = clip(seg, seg.s, span_start.min(seg.e));
        if duration_of(&piece) >= MIN_SECONDS {
            out.push(piece);
        }
    }
    if seg.e > span_end {
        let piece = clip(seg, span_end.max(seg.s), seg.e);
        if duration_of(&piece) >= MIN_SECONDS {
            out.push(piece);
        }
    }
    out
}

/// Every picture row the cut shows, in order, without repeats.
fn distinct_rows(segs: &[Seg]) -> Vec<usize> {
    let mut rows: Vec<usize> = Vec::new();
    for seg in segs {
        if !is_footage(seg) || seg.cam < 0 {
            continue;
        }
        let row = seg.cam as usize;
        if !rows.contains(&row) {
            rows.push(row);
        }
    }
    rows
}

/// Does the cut keep any footage inside `from..to`? Every "the cut keeps nothing between a and b" refusal
/// asks this one question.
fn footage_kept_between(segs: &[Seg], from: f64, to: f64) -> bool {
    segs.iter().any(|seg| {
        is_footage(seg) && (seg.e.min(to) - seg.s.max(from)).max(0.0) >= MIN_SECONDS
    })
}

/// Is the red line standing on kept footage? The no-selection Split asks this rather than
/// [`footage_kept_between`]: its span is zero-width, so only the second itself matters.
fn footage_under_line(segs: &[Seg], line: f64) -> bool {
    segs.iter()
        .any(|seg| is_footage(seg) && line >= seg.s && line < seg.e)
}

/// The camera's name in a status sentence. Rows are numbers in the model; the page spells the row it
/// added from, and a project with one camera never reaches the plural forms anyway.
fn cam_name(cam: i32) -> String {
    format!("cam {cam}")
}

/// The `m:ss` clock face the spec's refusals use ("nothing to split at 1:20"). Reached through the
/// project's own formatter so a refusal reads like every other time the app prints.
fn clock(seconds: f64) -> String {
    crate::tools::mm_ss(seconds)
}

/// Two ends as the ordered pair the split works on, collapsed when the band closed on itself.
fn dedup_ends(from: f64, to: f64) -> Vec<f64> {
    if (to - from).abs() < f64::EPSILON {
        vec![from]
    } else {
        vec![from, to]
    }
}
