//! F2.7 (Add, Split, Remove, ⌦) — `spec/05-cut.md` F2.7.

use crate::cut::{Fx, Seg};
use crate::cut_rules::{self, Floor};
use crate::cut_select::{Scope, Selection, ANY_ROW, MIN_SECONDS};

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

/// F2.7 ＋ Add (`keep the selection as scenes`). One kept scene per filmed run inside the band, both
/// ends snapped, and the span taken off every other row.
pub fn add(sel: Option<&Selection>, segs: &[Seg], snap_points: &[f64]) -> Outcome {
    let Some(band) = sel else {
        return Outcome::Refused("drag a region on a track first".to_string());
    };
    if let Scope::Sound { recording } = &band.scope {
        return Outcome::Refused(format!(
            "\u{ff0b} Add keeps footage \u{2014} the selection is {recording}'s sound"
        ));
    }
    if band.length() < cut_rules::floor(Floor::Keep) {
        // P.policy.minSceneSeconds, read as the KEEP verb's floor: a scene shorter than a second is not
        // something anyone chose to watch.
        return Outcome::Refused(format!(
            "nothing to add: {} selected, a scene is 1 s or more",
            crate::tools::tenths(band.length())
        ));
    }

    let start = snap_end(band.start, snap_points);
    let end = snap_end(band.end, snap_points);
    // Snapping may walk the two ends onto each other; a zero-length band keeps nothing.
    if end - start < cut_rules::floor(Floor::Remove) {
        return Outcome::Refused(format!(
            "nothing to add: {} selected, a scene is 1 s or more",
            crate::tools::tenths(band.length())
        ));
    }

    let row = match band.scope {
        Scope::Footage { row } => row,
        Scope::Sound { .. } => unreachable!("a sound selection refused above"),
    };
    // An empty cut has no rows yet: the scene starts the first.
    let targets: Vec<usize> = match distinct_rows(segs) {
        rows if row == ANY_ROW && !rows.is_empty() => rows,
        _ if row == ANY_ROW => vec![0],
        _ => vec![row],
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
                out.push(clip(seg, seg.s, seg.e.min(start)));
            }
            if seg.e > end {
                out.push(clip(seg, seg.s.max(end), seg.e));
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
pub fn snap_end(at: f64, points: &[f64]) -> f64 {
    let mut best: Option<(f64, f64)> = None;
    for &point in points {
        let distance = (point - at).abs();
        if distance > SNAP_TOLERANCE_SECONDS {
            continue;
        }
        if best.is_none_or(|(closest, _)| distance < closest) {
            best = Some((distance, point));
        }
    }
    best.map(|(_, point)| point).unwrap_or(at)
}

/// F2.7 | Split (`a border at each end of the selection, nothing removed`).
pub fn split(sel: Option<&Selection>, line: f64, segs: &[Seg]) -> Outcome {
    if let Some(Selection {
        scope: Scope::Sound { recording },
        ..
    }) = sel
    {
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
                "| Split cuts at the red line \u{2014} click a track to put it somewhere"
                    .to_string(),
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

/// The same scene: its span and what it shows. A held clip is a copy taken when the hand picked it up, so it is
/// found again in the cut by what it is rather than by an index an edit since may have moved.
fn same_seg(a: &Seg, b: &Seg) -> bool {
    (a.s - b.s).abs() < 1e-9 && (a.e - b.e).abs() < 1e-9 && a.ins == b.ins && a.cam == b.cam
}

/// F2.7 ⌦ (`in order: a held effect; a held clip; a selection; the scene under the line`).
pub fn delete_verb(
    held_effect: Option<&Fx>,
    held_clip: Option<&Seg>,
    sel: Option<&Selection>,
    scene_under_line: Option<&Seg>,
    segs: &[Seg],
) -> Outcome {
    let without = |gone: &Seg| -> Vec<Seg> {
        let mut left = segs.to_vec();
        if let Some(at) = left.iter().position(|seg| same_seg(seg, gone)) {
            left.remove(at);
        }
        left
    };
    if held_effect.is_some() {
        // First branch, and nothing else is looked at: the effect is what the hand last touched. The effect
        // record is the effects lane's to drop (`ui::remove_held_effect`); the scenes stay as they are.
        return Outcome::Applied {
            status: format!("removed the effect \u{2014} {UNDO_TAIL}"),
            segs: segs.to_vec(),
            keeps_selection: false,
            into_hand: false,
        };
    }
    if let Some(clip) = held_clip {
        // The only door a spliced card has (§J rule 8), so it outranks even a live selection.
        let status = if clip.ins.is_empty() {
            format!(
                "removed the clip {} \u{2013} {} \u{2014} {UNDO_TAIL}",
                crate::tools::mm_ss(clip.s),
                crate::tools::mm_ss(clip.e)
            )
        } else {
            format!(
                "removed the card \u{2014} {} ({REMOVE_UNDO_TAIL})",
                clip.ins.as_str()
            )
        };
        return Outcome::Applied {
            status,
            segs: without(clip),
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
        // The cutting itself is － Remove's: the same band leaves the same scenes whichever door took it.
        let cut_down = match remove(Some(band), segs) {
            Outcome::Applied { segs, .. } => segs,
            Outcome::Refused(_) => segs.to_vec(),
        };
        return Outcome::Applied {
            status: format!("removed \u{2014} {taken} segment(s), was {left}"),
            segs: cut_down,
            keeps_selection: false,
            into_hand: false,
        };
    }
    if let Some(scene) = scene_under_line {
        return Outcome::Applied {
            status: format!("removed the scene under the line \u{2014} {UNDO_TAIL}"),
            segs: without(scene),
            keeps_selection: false,
            into_hand: false,
        };
    }
    Outcome::Refused(
        "nothing selected \u{2014} click a kept scene, or drag a region on a track".to_string(),
    )
}

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
    segs.iter()
        .any(|seg| is_footage(seg) && (seg.e.min(to) - seg.s.max(from)).max(0.0) >= MIN_SECONDS)
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
