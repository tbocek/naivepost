//! §05-cut#1-screen — what the Cut page shows and what a press would do.
//!
//! The page renders this module and forwards to it; nothing here imports gtk, so which button is
//! greyed, what its tooltip says and where a wheel goes can be answered without a display
//! (spec/00-principles.md §5). Tooltips and greyed rules come from [`crate::cut`]'s companion,
//! spec/inventory/cut.md §A, which the spec calls normative.
//!
//! The timeline's own numbers have no `P.*` rows of their own — they are spec/10-parameters.md:133's
//! "The rest, by area: timeline: …" line (thumb height 40..160, 64 at open, steps ×4/3 and ÷3/4;
//! zoom 4 → 240 px/s, step 1.25; pan viewW/8). They live here because this is the module whose rule
//! uses them.

use crate::cut::Cut;
use crate::{prepare, tools};

/// The toolbar's groups, left to right (§1: transport, preview volume, verbs, effects dropdown,
/// history, zoom). The window builds one box per entry in this order, which is what makes the order
/// a fact a test can hold rather than a fact about how a file happens to read.
pub const TOOLBAR_GROUPS: [&str; 6] = ["transport", "volume", "verbs", "effects", "history", "zoom"];

// --- Thumbnail size (§A's idle row: "Thumbnails [🖼− 🖼+] … ×3/4, ×4/3, clamp 40..160") ----------

/// The smallest row the tracks will draw: below this a face is a smudge and zooming further out is
/// what the zoom control is for.
pub const THUMB_MIN: u32 = 40;

/// The largest: past it one recording runs off the end of the world at any usable zoom.
pub const THUMB_MAX: u32 = 160;

/// Where a page that opens meets you — big enough to read a slide, small enough to see an hour.
pub const THUMB_AT_OPEN: u32 = 64;

/// 🖼+ : a third bigger, because the steps are ×4/3 and ÷3/4 rather than ±1 px — the same ratio in
/// both directions means 🖼− after 🖼+ lands where you started.
pub fn thumb_up(px: u32) -> u32 {
    ((px as f64 * 4.0 / 3.0).round() as u32).min(THUMB_MAX)
}

/// 🖼− : the inverse step, clamped at [`THUMB_MIN`].
pub fn thumb_down(px: u32) -> u32 {
    ((px as f64 * 3.0 / 4.0).round() as u32).max(THUMB_MIN)
}

// --- Zoom (§A's toolbar group 7 and §B's zoom rules) --------------------------------------------

/// px per second at open: the whole session is rarely this narrow, so the first thing a person does
/// is zoom out to the floor — which is why the floor matters more than this.
pub const ZOOM_AT_OPEN: f64 = 4.0;

/// The deepest zoom (§A: "ceiling 240 px/s"). It is a ceiling rather than a limit because past it a
/// second stops being a thing you can point at.
pub const ZOOM_MAX: f64 = 240.0;

/// §1: the buttons' factor, and the base of the wheel's `1.25^-dy`.
pub const ZOOM_STEP: f64 = 1.25;

/// The floor: the whole filmed session across the view (§A: "floor fits the filmed duration into the
/// view minus the gutter" — the gutter is the caller's to subtract from `view_px`, since it is what
/// the caller drew). An empty session answers [`ZOOM_AT_OPEN`] rather than 0 or infinity: a cut with
/// no footage still has a timeline, and a zoom of zero would divide by itself on the first draw.
pub fn fit_pps(filmed_seconds: f64, view_px: f64) -> f64 {
    if filmed_seconds <= 0.0 || view_px <= 0.0 {
        return ZOOM_AT_OPEN;
    }
    view_px / filmed_seconds
}

/// − : out by one step, stopping at the floor. §A's tooltip says it stops where the whole session is
/// on screen, which is [`fit_pps`] rather than a fixed number — a two-hour session fits at a px per
/// second that would leave a five-minute one in a corner.
pub fn zoom_down(pps: f64, floor: f64) -> f64 {
    (pps / ZOOM_STEP).max(floor).min(ZOOM_MAX)
}

/// + : in by one step, stopping at [`ZOOM_MAX`].
pub fn zoom_up(pps: f64, floor: f64) -> f64 {
    (pps * ZOOM_STEP).clamp(floor, ZOOM_MAX)
}

/// Zoom keeping the second under `anchor_x` where it is (§A: the wheel "does the same, around the
/// cursor"). The scroll back has to move too, or zooming in at the left edge pushes what you were
/// looking at off screen and every zoom is a hunt for the same moment again.
pub fn zoom_around(pps: f64, factor: f64, anchor_x: f64, origin_x: f64, floor: f64) -> (f64, f64) {
    let time = (anchor_x - origin_x) / pps;
    let next = (pps * factor).clamp(floor, ZOOM_MAX);
    (next, (anchor_x - time * next).max(0.0))
}

// --- Frame steps and the wheel (§1's two wheel rules) ------------------------------------------

/// One frame: ‹f and f› step the red line by a single frame, which is the only way to land a cut on
/// a word edge by hand.
pub const FRAME_STEP: i64 = 1;

/// Shift makes it five — far enough to cross a seam, short enough that overshoot costs one press.
pub const FRAME_STEP_SHIFT: i64 = 5;

/// ‹f/f› with or without Shift held (§A: "back 5 frames (pauses) — or whatever is held, 5 frames").
pub fn frame_step(shift: bool) -> i64 {
    if shift {
        FRAME_STEP_SHIFT
    } else {
        FRAME_STEP
    }
}

/// What the wheel is over, which decides what it does (§1: "Wheel over the transport bar **and over
/// the preview picture** steps frames (Shift = 5); over the tracks it zooms around the cursor").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelOver {
    Transport,
    Preview,
    Tracks,
}

/// What one wheel motion means. A no-op is `StepFrames { frames: 0 }` rather than its own variant:
/// a caller that steps the line by nothing has done exactly what the wheel asked for, and a third
/// answer would be a branch that can only ever do nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Wheel {
    StepFrames { frames: i64 },
    Zoom { factor: f64, at_x: f64 },
    Pan { px: f64 },
}

/// An eighth of the view (§A: "Shift+wheel pans by dx·viewW/8", and §1's "a trackpad sideways swipe"
/// is the same gesture). A fixed pixel pan would crawl at 240 px/s and fly at the floor.
pub fn pan_px(view_px: f64) -> f64 {
    view_px / 8.0
}

/// One wheel event, decided by what it was over. `dy` is the vertical notch (positive away from the
/// user, as GDK reports it) and `dx` the sideways one, which on a trackpad arrives with no `dy` at
/// all — that is how a swipe pans while a notch over the tracks zooms.
pub fn wheel(over: WheelOver, shift: bool, dy: f64, dx: f64, view_px: f64) -> Wheel {
    match over {
        WheelOver::Transport | WheelOver::Preview => {
            let step = frame_step(shift);
            // The sign comes from the vertical notch; a pure sideways swipe steps nowhere.
            let direction = if dy < 0.0 {
                -1
            } else if dy > 0.0 {
                1
            } else {
                0
            };
            Wheel::StepFrames { frames: step * direction }
        }
        WheelOver::Tracks => {
            if shift || dx.abs() > dy.abs() {
                let direction = if dx < 0.0 { -1.0 } else if dx > 0.0 { 1.0 } else { -dy.signum() };
                Wheel::Pan { px: pan_px(view_px) * direction }
            } else {
                // Banked deltas arrive as a count of notches, so the factor is the step to that power.
                Wheel::Zoom { factor: ZOOM_STEP.powf(-dy), at_x: 0.0 }
            }
        }
    }
}

// --- The transport's three play buttons (§A's group 1) -----------------------------------------

/// What the preview is doing, which is all the two cut buttons need to know about themselves.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Preview {
    /// ▶✂ is running: the removed stretches are skipped.
    pub cut_only: bool,
    /// ▶✂✂ is running: one join after the other.
    pub reviewing: bool,
}

/// ▶ is never greyed (§A: "(never greyed)") — pausing a stopped preview is not an error worth a dead
/// button over, and a control that is only sometimes clickable teaches nobody anything.
pub fn play_recording_sensitive() -> bool {
    true
}

/// ▶✂ needs something to skip to (§A: "sensitive when segs > 0").
pub fn can_play_cut(cut: &Cut) -> bool {
    !cut.segs.is_empty()
}

/// ▶✂✂ needs joins to review (§A: "sensitive with ≥2 clips"). A card is not a clip: reviewing it
/// would play one still and call the tour complete.
pub fn can_review(cut: &Cut) -> bool {
    cut.segs.iter().filter(|seg| !seg.is_insert()).count() >= 2
}

/// Lit while ▶✂ runs — but not while ▶✂✂ runs, which is the same skipping with a tour through it
/// (§A: "lit while cutOnly and no review").
pub fn play_cut_lit(preview: &Preview) -> bool {
    preview.cut_only && !preview.reviewing
}

/// Lit while the review runs.
pub fn review_lit(preview: &Preview) -> bool {
    preview.reviewing
}

// --- Tooltips (§A's groups 1, 6 and 7, verbatim) -----------------------------------------------

/// ▶ — §A group 1.
pub const PLAY_TIP: &str = "play or pause the preview at the playhead";

/// ▶✂ idle — §A group 1.
pub const PLAY_CUT_TIP: &str = "play the CUT instead of the recording: the removed stretches are \
skipped, so this runs the finished video. The clock reads the cut's own time while it does. Changes \
nothing that is saved.";

/// ▶✂ running: the same button becomes the pause (§A group 1).
pub const PLAY_CUT_TIP_LIVE: &str = "pause the cut preview";

/// ▶✂✂ idle — §A group 1.
pub const REVIEW_TIP: &str = "review every cut in one go: plays 10 s of the finished video before \
each join and 10 s after it, one join after the other, and stops after the last. The removed \
stretches are skipped as under ▶✂. Changes nothing that is saved.";

/// ▶✂✂ running — §A group 1.
pub const REVIEW_TIP_LIVE: &str = "pause the cut review";

/// ‹‹f/‹f/f›/f››. §A elides this with "back 5 frames (pauses) — or whatever is held, 5 frames", so
/// the four share one sentence naming what they hold in common: they pause, and they move the line.
pub const FRAME_TIP: &str = "step the red line by a frame (pauses) — Shift holds five";

/// Undo — §A group 6. The keys are §1's own ("Undo Ctrl+Z").
pub const UNDO_TIP: &str = "Undo — take back the last Add, Remove or Suggest";

/// §1 lists two spellings for Redo where §A names one, so both go in the tooltip rather than one of
/// them being true and invisible.
pub const REDO_KEYS: &str = " (Ctrl+Shift+Z or Ctrl+Y)";

/// Undo's keys, §1's "Undo Ctrl+Z".
pub const UNDO_KEYS: &str = " (Ctrl+Z)";

/// Redo — §A group 6.
pub const REDO_TIP: &str = "Redo — put it back";

/// Revert — §A group 6.
pub const REVERT_TIP: &str = "Revert edits — drop everything you added or removed by hand and go \
back to the last suggestion — or, if you have not suggested yet, to the cut this page opened with";

/// Clear — §A group 6.
pub const CLEAR_TIP: &str = "Clear: take every kept stretch and every effect off the timeline, \
leaving the recordings as they were loaded (↶ Undo brings them back)";

/// − — §A group 7.
pub const ZOOM_OUT_TIP: &str = "zoom the timeline out — it stops where the whole session is on \
screen (the scroll wheel does the same, around the cursor)";

/// + — §A elides this one ("…around the middle of what is on screen…"), so it is written to match
/// its own sentence: the buttons differ from the wheel only in where they anchor.
pub const ZOOM_IN_TIP: &str = "zoom the timeline in — around the middle of what is on screen, and \
it stops at 240 px a second";

// --- What each toolbar group holds (§1 items 5-22, §A groups 1-7) -------------------------------

/// One toolbar control: the name a test and the snapshot entry point find it by, what it says, and the
/// sentence that says what pressing it would do. `label` carries the glyph the spec draws plus the
/// words the page shows — the headless container has no icon theme, so a glyph alone would render as
/// a box (the same reason `cut_hear::VOLUME_LABEL` spells "preview volume" as a word).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tool {
    pub name: &'static str,
    pub label: &'static str,
    pub tip: &'static str,
}

/// Transport, in §1's order 5 → 6 → 7 → 8. The three play buttons keep the names F2.1/F2.2/F2.3
/// gave them and their own tooltips; the four frame steps are §1 item 8 and share [`FRAME_TIP`].
/// Labels and tips are the very strings `ui/window.rs` uses today — moved here so the ORDER is a fact
/// a test can read rather than something only the assembly code knows.
pub const TRANSPORT_BUTTONS: [Tool; 7] = [
    Tool {
        name: "play-recording-button",
        label: "\u{25b6} Play the recording",
        tip: "Play the recording from the red line \u{2014} every second of it, cuts and all",
    },
    Tool {
        name: "play-cut-button",
        label: "\u{25b6}\u{2702} Play the cut",
        tip: PLAY_CUT_TIP,
    },
    Tool {
        name: "review-cuts-button",
        label: "\u{25b6}\u{2702}\u{2702} Review every cut",
        tip: REVIEW_TIP,
    },
    Tool { name: "line-step-back-five", label: "\u{2039}\u{2039} f  step back 5 frames", tip: FRAME_TIP },
    Tool { name: "line-step-back", label: "\u{2039} f  step back 1 frame", tip: FRAME_TIP },
    Tool { name: "line-step-forward", label: "f \u{203a}  step forward 1 frame", tip: FRAME_TIP },
    Tool { name: "line-step-forward-five", label: "f \u{203a}\u{203a}  step forward 5 frames", tip: FRAME_TIP },
];

/// Verbs, §1 items 10 → 15 in one row: ＋ Add, | Split, － Remove, ⧉ Copy, ⧉ Paste, Insert, ⇲ Lane.
///
/// This table only ORDERS the controls. Their names, labels and tooltips stay single-sourced where they
/// already live — `cut_verbs::BUTTONS` for the first three, `cut_copy::BUTTONS` for copy/paste/lane —
/// and `verb_tools()` returns this list built from those two modules plus [`INSERT_TOOL`], so a change
/// to either is a change here automatically and the two cannot drift apart.
pub const VERB_BUTTONS: [Tool; 7] = [
    Tool {
        name: "add-button",
        label: "\u{ff0b} Add",
        tip: "keep the selection as scenes \u{2014} one per filmed run",
    },
    Tool {
        name: "split-button",
        label: "| Split",
        tip: "a border at each end of the selection \u{2014} nothing is removed",
    },
    Tool {
        name: "remove-button",
        label: "\u{ff0d} Remove",
        tip: "drop exactly the selection \u{2014} the cut keeps everything else until you undo",
    },
    Tool { name: "copy-button", label: "\u{29c9} Copy", tip: "take the selection in hand \u{2014} a second at least; the band keeps showing it" },
    Tool { name: "paste-button", label: "\u{29c9} Paste at the red line", tip: "put what is in hand where the red line is \u{2014} footage splices in, sound lays over kept footage" },
    INSERT_TOOL,
    Tool { name: "lane-button", label: "\u{21f2} Lane", tip: "give the copy a row of its own \u{2014} nothing is cut to it yet" },
];

/// Insert/Edit, §1 item 15 / §A group 4's last-but-one entry. `cut_insert.rs` owns the insert FORM
/// (its `CHOOSING` wording, the between/over choice, the seconds entry); this is only the button that
/// opens it, which is why the tooltip points at the choice the form then asks for.
pub const INSERT_TOOL: Tool = Tool {
    name: "insert-button",
    label: "Insert",
    tip: "put a clip, image, animation or sound into the cut \u{2014} between the footage, over it, or on a lane",
};

/// The effects dropdown's six entries, §1 item 17 ("✚ Effect ▾"). The third field is the effect kind
/// id `cut::EffectKind::parse` accepts (`src/cut.rs`: zoom / speed / text / svg / volume / label),
/// so an entry here cannot name a kind the cut cannot hold. A stop is not a seventh entry: §1 lists
/// none, and `EffectKind` folds `"speed" | "stop"` into Speed.
pub const EFFECT_ITEMS: [Tool; 6] = [
    Tool { name: "effect-item-zoom", label: "\u{2295} Zoom", tip: "zoom" },
    Tool { name: "effect-item-text", label: "\u{275d} Text", tip: "text" },
    Tool { name: "effect-item-svg", label: "\u{25a8} SVG", tip: "svg" },
    Tool { name: "effect-item-speed", label: "\u{23e9} Speed", tip: "speed" },
    Tool { name: "effect-item-volume", label: "\u{1f50a} Volume", tip: "volume" },
    Tool { name: "effect-item-label", label: "\u{1f3f7} Label", tip: "label" },
];

/// History, §1 items 18 → 21 with the keys §1 spells: Undo Ctrl+Z, Redo Ctrl+Shift+Z **or Ctrl+Y**,
/// Revert, ✗ Clear. The key chords live in the tooltip because the page draws no key hints anywhere
/// else (§1: "No timeline tooltips, by design; status line and cursor shapes say what a press would do").
pub const HISTORY_BUTTONS: [Tool; 4] = [
    Tool {
        name: "undo-button",
        label: "Undo",
        tip: "Undo \u{2014} take back the last Add, Remove or Suggest (Ctrl+Z)",
    },
    Tool {
        name: "redo-button",
        label: "Redo",
        tip: "Redo \u{2014} put it back again (Ctrl+Shift+Z, or Ctrl+Y)",
    },
    Tool {
        name: "revert-button",
        label: "Revert",
        tip: "Revert edits \u{2014} drop everything you added or removed by hand and go back to the last \
suggestion, or to the cut this page opened with if you have not suggested yet",
    },
    Tool {
        name: "clear-cut-button",
        label: "\u{2717} Clear",
        tip: "Clear: take every kept stretch and every effect off the timeline, leaving the recordings \
as they were loaded (\u{21b6} Undo brings them back)",
    },
];

/// Zoom, §1 item 22: − then +.
pub const ZOOM_BUTTONS: [Tool; 2] = [
    Tool { name: "zoom-out-button", label: "\u{2212}", tip: ZOOM_OUT_TIP },
    Tool { name: "zoom-in-button", label: "+", tip: ZOOM_IN_TIP },
];

/// Which of the four history buttons may be pressed right now, in [`HISTORY_BUTTONS`]' order:
/// Undo, Redo, Revert, Clear.
///
/// `undo_left` and `redo_left` are the counts either side of the pointer in `cut::History` (bounded by
/// `P.layout.undoDepth` = [`crate::cut::UNDO_DEPTH`] = 50 snapshots, so neither count can grow past
/// that however long the session runs). `ever_edited` is whether anything has been recorded at all:
/// with nothing but the state the page opened on, Revert has nothing to throw away and Clear would
/// clear a timeline nobody touched, so both sit greyed next to a greyed Undo.
pub fn history_buttons_enabled(undo_left: usize, redo_left: usize, ever_edited: bool) -> [bool; 4] {
    [
        // Nothing below the pointer: there is no earlier state to go back to.
        undo_left > 0,
        // Nothing above it: the branch was thrown away by the last new edit (§A: redo survives only
        // while the branch it belongs to does).
        redo_left > 0,
        // Revert answers "back to the base", and with no edit the screen IS the base.
        ever_edited,
        // Clear is the destructive one; it earns its colour only once there is something of yours on
        // the timeline to lose.
        ever_edited,
    ]
}

/// The eight idle rows of the form column, §1 item 4's order: thumbnail size, aspect, playhead,
/// selection, cut, cut at 1×, source, segments. These are the labels [`idle_readouts`] emits, in
/// the order it emits them — listed here so the page's row set is checkable without building a Cut,
/// and so a renamed row fails a test instead of silently reordering the column.
pub const IDLE_FORM_ROWS: [&str; 8] = [
    "Thumbnails",
    "Aspect ratio",
    "Playhead",
    "Selection",
    "Cut",
    "Cut at 1\u{00d7}",
    "Source",
    "Segments",
];

/// The widget name the form column gives one reading, keyed off its label so the label is spelled in
/// exactly one place ([`IDLE_FORM_ROWS`] and [`idle_readouts`]) and the name follows from it.
///
/// Non-alphanumerics become `-` and a `×` is spelled out first: "Cut at 1×" must not come out as
/// `cut-readout-cut-at-1`, which would collide with a hypothetical "Cut at 1" and read as truncated.
pub fn readout_widget(label: &str) -> String {
    let spelled = label.replace('\u{00d7}', " times");
    let slug: String = spelled
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let slug = slug.trim_matches('-').replace("--", "-");
    format!("cut-readout-{slug}")
}

// --- The form column's idle readings (§A: "Idle rows: …") --------------------------------------

/// One row of the idle form: what it is called on the page, what it reads, and the sentence §A puts
/// behind it when there is one. A row with an empty tip has no sentence to borrow, so it gets no
/// tooltip rather than one that restates its own label.
#[derive(Debug, Clone, PartialEq)]
pub struct Readout {
    pub label: &'static str,
    pub value: String,
    pub tip: &'static str,
}

/// The shape a cut with no `aspect` is shown in. §A lists Aspect ratio as a dropdown and leaves its
/// choices to F2.x; the prototype opens at the first recording's own shape, which this module cannot
/// know without probing files, so the page falls back to the shape recordings are made in and the
/// aspect round (F2.x) replaces it with the real thing.
pub const ASPECT_DEFAULT: &str = "16:9";

/// The cut's own length: what ▶✂ runs for. A spliced card counts its `dur`, since it owns its
/// seconds; footage is its span over the speed it plays at — and `rate` absent means 1, because only
/// produceSegs writes one (§B), so a hand-made scene has never been timed at anything else.
pub fn cut_seconds(cut: &Cut) -> f64 {
    cut.segs
        .iter()
        .map(|seg| {
            if seg.is_insert() {
                seg.dur
            } else {
                let rate = if seg.rate > 0.0 { seg.rate } else { 1.0 };
                (seg.e - seg.s) / rate
            }
        })
        .sum()
}

/// The same stretches at normal speed (§A's "Cut at 1×" row). The gap between this and
/// [`cut_seconds`] is what the speed effects saved, which is the only reason the row exists.
pub fn cut_seconds_at_1x(cut: &Cut) -> f64 {
    cut.segs
        .iter()
        .map(|seg| if seg.is_insert() { seg.dur } else { seg.e - seg.s })
        .sum()
}

/// How many scenes the timeline holds, cards included — the count §A calls totalSegs.
pub fn segment_count(cut: &Cut) -> usize {
    cut.segs.len()
}

/// How a stored aspect reads on the page: an unset shape shows the default, a chosen one shows itself.
/// The ONE spelling of that rule — `idle_readouts` and the F3.2 refresh both call this, so the Aspect ratio
/// row and the `aspect-choice` dropdown beside it can never be painted with two different answers.
pub fn aspect_shown(aspect: &str) -> String {
    if aspect.is_empty() {
        ASPECT_DEFAULT.to_string()
    } else {
        aspect.to_string()
    }
}

/// The eight rows of the idle form, in §A's order and with its spellings. The clocks go through
/// [`crate::tools::mm_ss`], so a page and a log line never disagree about what 4271 seconds is.
pub fn idle_readouts(
    thumb_px: u32,
    aspect: &str,
    playhead: f64,
    selection: Option<(f64, f64)>,
    cut: &Cut,
    source_seconds: f64,
) -> Vec<Readout> {
    let aspect = aspect_shown(aspect);
    // A row that reads "nothing selected" would be a prompt, and this is a reading: the silence is
    // the answer, and §A gives it no wording of its own.
    let selection = match selection {
        Some((start, end)) => format!("{} \u{2013} {}", tools::mm_ss(start), tools::mm_ss(end)),
        None => String::new(),
    };
    vec![
        Readout {
            label: "Thumbnails",
            value: format!("{thumb_px} px"),
            tip: "smaller|larger thumbnails on the tracks",
        },
        Readout { label: "Aspect ratio", value: aspect.to_string(), tip: "" },
        Readout { label: "Playhead", value: tools::mm_ss(playhead), tip: "" },
        Readout { label: "Selection", value: selection, tip: "" },
        Readout { label: "Cut", value: tools::mm_ss(cut_seconds(cut)), tip: "" },
        Readout { label: "Cut at 1\u{00d7}", value: tools::mm_ss(cut_seconds_at_1x(cut)), tip: "" },
        Readout { label: "Source", value: tools::mm_ss(source_seconds), tip: "" },
        Readout { label: "Segments", value: segment_count(cut).to_string(), tip: "" },
    ]
}

// --- The picture band's grid (§1: "on the 250 ms grid restarted at each scene change") ----------

/// How many seconds one thumbnail slot spans: a row `thumb_px` tall of `w:h` shape is that much
/// wider, and the zoom decides how many px a second is. Below one frame per slot the band draws
/// black between pictures, which is why the prototype's band went empty when you zoomed in.
pub fn slot_seconds(thumb_px: u32, aspect: &str, pps: f64) -> f64 {
    let (w, h) = parse_aspect(aspect);
    if pps <= 0.0 || h == 0 {
        return 0.0;
    }
    (thumb_px as f64 * w / h as f64) / pps
}

/// Does the band have a picture at every zoom? True while one slot spans no more than
/// [`crate::prepare::EXTRACTION_GRID`] — `P.eng.frameGridSeconds`, the 250 ms grid Prepare extracts on
/// and restarts at every scene change.
///
/// Reported rather than assumed: whether a given zoom has a picture depends on the row height and the
/// aspect as well as on the grid, so this answers from all three and the page says what it finds. The
/// prototype's band stepped one frame per Freq seconds (Freq ships at 1 s), which is why §1 says it
/// "zoomed in … shows black between them"; a 64 px 16:9 row wants a frame every 0.47 s at the ceiling,
/// finer than even the new grid, and this answers `false` for that rather than promising a picture the
/// extractor was never asked for. The band's own round asks Prepare for one frame per slot instead.
pub fn picture_at_every_zoom(pps: f64, thumb_px: u32, aspect: &str) -> bool {
    slot_seconds(thumb_px, aspect, pps) <= prepare::EXTRACTION_GRID
}

/// `"{w}:{h}"`, or the default shape for anything else: a page that cannot read its own aspect still
/// has to lay the band out.
fn parse_aspect(aspect: &str) -> (f64, u32) {
    let Some((w, h)) = aspect.split_once(':') else {
        return (16.0, 9);
    };
    match (w.trim().parse::<f64>(), h.trim().parse::<u32>()) {
        (Ok(w), Ok(h)) if w > 0.0 && h > 0 => (w, h),
        _ => (16.0, 9),
    }
}

// --- The bands' heights (§A's "Tracks" paragraph) ----------------------------------------------

/// The gutter: fold-all badge, sound switches, empty-row ✕ (§A: "first 30 px of timeline space").
pub const GUTTER_PX: f64 = 30.0;

/// The ruler, whose ticks are the only place the timeline names a time in words.
pub const RULER_PX: f64 = 18.0;

/// The selection band: green bar per kept scene, blue selection, fold badges. Taller than the ruler
/// because it carries things you grab, and a ruler is not grabbed.
pub const SELECTION_BAND_PX: f64 = 22.0;

/// One row of the effects lane — one row per overlapping group, so this is what they stack by.
pub const EFFECT_ROW_PX: f64 = 26.0;

/// One channel of a row's wave strip.
pub const WAVE_LANE_PX: f64 = 30.0;

/// Between a picture row and its wave strip (§A: "gap 3").
pub const ROW_GAP_PX: f64 = 3.0;

/// The red playhead: "2 px line on its own layer across both bands" (§1). Its own layer is why it
/// stays one width however the bands above and below it are sized.
pub const RED_LINE_PX: f64 = 2.0;

/// One screenful of drag, however deep the zoom: the thumb is geared so the last 40 px of a
/// three-hour session are still 40 px of thumb (§A: "thumb geared at high zoom, 40 px per screenful").
pub const SCROLLBAR_DRAG_PX: f64 = 40.0;

/// Whether there is a scrollbar at all (§1: "hidden when everything fits"). A bar over a timeline
/// that does not scroll is a groove nobody can use and one less row of footage.
pub fn scrollbar_shown(total_px: f64, view_px: f64) -> bool {
    total_px > view_px
}

/// §7 (`the tracks are a window drawn under a translate`): the seconds the drawing is asked for. The band is
/// laid out in pixels and the model holds seconds, so this converts what the translate puts on screen back into
/// the range the tracks may be filled from — and it never asks for a frame. Nothing decodes inside a draw: the
/// caller fills the window from what has already arrived, and the one place a draw may ask for work at all is
/// throttled by [`crate::cut_trim::scrubs`].
///
/// `offset_px` is how far the translate has moved the band left (0 at its start) and is clamped into the band, so
/// a drag past either end yields the edge rather than an empty track. The gutter is not subtracted here: it is
/// drawn over the band's own first 30 px ([`GUTTER_PX`]) by the caller, which knows where its translate begins.
pub fn visible_window(offset_px: f64, view_px: f64, pps: f64, filmed_px: f64) -> (f64, f64) {
    if pps <= 0.0 || view_px <= 0.0 {
        return (0.0, 0.0);
    }
    let start = offset_px.clamp(0.0, filmed_px.max(0.0));
    (start / pps, (start + view_px).min(filmed_px.max(0.0)) / pps)
}

/// ✚ Effect ▾ — §1 item 17's dropdown. The tooltip says what the six entries are for, since the
/// button's own label cannot.
pub const EFFECT_MENU_TIP: &str = "add an effect over the seconds under the red line \u{2014} zoom, text, \
a drawing, a speed change, a volume or a label";
