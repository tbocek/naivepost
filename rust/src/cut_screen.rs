//! §05-cut#1-screen — what the Cut page shows and what a press would do.
//!
//! The page renders this module; nothing here imports gtk, so greying, tooltips and wheel routing are testable
//! without a display (spec/00-principles.md §5; tooltips from spec/inventory/cut.md §A). Timeline numbers have no `P.*` rows: they are spec/10-parameters.md:133's "timeline" line.

use crate::cut::Cut;
use crate::{prepare, tools};

/// Horizontal padding of every Cut-toolbar button, in px: at 2 px the verbs' icons touched. The bar must still fit §1's 1240 px floor
/// (asserted by `tests/suite/sec_05_cut_1_screen_toolbar_fits.rs`).
pub const BUTTON_PADDING_PX: u32 = 6;

pub const BUTTON_MIN_WIDTH_PX: u32 = 32;

/// The toolbar's one CSS rule, installed by `ui::window_page_box` and scoped by `#cut-toolbar`; built from [`BUTTON_PADDING_PX`].
pub fn toolbar_css() -> String {
    format!(
        "#cut-toolbar button {{ padding-left: {px}px; padding-right: {px}px; min-width: {min}px; }}\n\
         #group-transport-row button {{ padding-left: {tpx}px; padding-right: {tpx}px; min-width: {tmin}px; }}\n\
         #play-recording-button label, #play-cut-button label, #review-cuts-button label {{ transform: translateY({pdrop}px); }}\n\
         #line-step-back-five label, #line-step-back label, #line-step-forward label, #line-step-forward-five label {{ transform: translateY({fdrop}px); }}",
        px = BUTTON_PADDING_PX,
        min = BUTTON_MIN_WIDTH_PX,
        tpx = TRANSPORT_PADDING_PX,
        tmin = TRANSPORT_MIN_WIDTH_PX,
        pdrop = PLAY_GLYPH_DROP_PX,
        fdrop = FRAME_LABEL_DROP_PX
    )
}

/// The transport buttons are pressed constantly, so they keep a wider target.
pub const TRANSPORT_MIN_WIDTH_PX: u32 = 36;
pub const TRANSPORT_PADDING_PX: u32 = 6;
/// Drop in px of the ▶ ▶✂ ▶✂✂ faces: Noto Sans Symbols 2 draws them above centre. A transform, so the bar's height stays.
pub const PLAY_GLYPH_DROP_PX: u32 = 2;
/// Drop in px of the frame-step labels, measured on `05-cut`.
pub const FRAME_LABEL_DROP_PX: u32 = 1;

/// Pixels between toolbar groups; buttons inside a group are linked.
pub const TOOLBAR_GAP_PX: i32 = 6;

pub const VOLUME_ICON: &str = "audio-volume-high-symbolic";

/// The symbolic icon a toolbar button wears instead of its words (`go/cut.go` 4678-4798); `None` keeps the words.
pub fn tool_icon(name: &str) -> Option<&'static str> {
    Some(match name {
        "add-button" => "list-add-symbolic",
        "split-button" => "edit-cut-symbolic",
        "remove-button" => "list-remove-symbolic",
        "copy-button" => "edit-copy-symbolic",
        "paste-button" => "edit-paste-symbolic",
        "lane-button" => "go-down-symbolic",
        "undo-button" => "edit-undo-symbolic",
        "redo-button" => "edit-redo-symbolic",
        "revert-button" => "document-revert-symbolic",
        "clear-cut-button" => "edit-clear-all-symbolic",
        "zoom-out-button" => "zoom-out-symbolic",
        "zoom-in-button" => "zoom-in-symbolic",
        _ => return None,
    })
}

/// The toolbar's groups, left to right (§1); the window builds one box per entry in this order.
pub const TOOLBAR_GROUPS: [&str; 6] =
    ["transport", "volume", "verbs", "effects", "history", "zoom"];

// --- Thumbnail size (§1 badge 2: the grip at the bottom of the picture rows, clamp 40..160) --------

/// The smallest picture row: below it a face is a smudge.
pub const THUMB_MIN: u32 = 40;

pub const THUMB_MAX: u32 = 160;

pub const THUMB_AT_OPEN: u32 = 64;

/// Thumbnail height after dragging the grip; `rows` rows above the edge, itself included, all grow equally. Clamped to 40..160.
pub fn thumb_dragged(was: u32, rows: usize, dy: f64) -> u32 {
    let grown = f64::from(was) + dy / rows.max(1) as f64;
    (grown.round().max(0.0) as u32).clamp(THUMB_MIN, THUMB_MAX)
}

// --- Zoom (§A's toolbar group 7 and §B's zoom rules) --------------------------------------------

/// px per second at open.
pub const ZOOM_AT_OPEN: f64 = 4.0;

/// The deepest zoom (§A: "ceiling 240 px/s").
pub const ZOOM_MAX: f64 = 240.0;

/// §1: the buttons' factor, and the base of the wheel's `1.25^-dy`.
pub const ZOOM_STEP: f64 = 1.25;

/// The floor: the whole filmed session across the view (§A); the caller subtracts the gutter from `view_px`.
/// An empty session answers [`ZOOM_AT_OPEN`] (a zero zoom would divide by itself).
pub fn fit_pps(filmed_seconds: f64, view_px: f64) -> f64 {
    if filmed_seconds <= 0.0 || view_px <= 0.0 {
        return ZOOM_AT_OPEN;
    }
    view_px / filmed_seconds
}

/// − : out by one step, stopping at the floor ([`fit_pps`], not a fixed number).
pub fn zoom_down(pps: f64, floor: f64) -> f64 {
    (pps / ZOOM_STEP).max(floor).min(ZOOM_MAX)
}

/// + : in by one step, stopping at [`ZOOM_MAX`].
pub fn zoom_up(pps: f64, floor: f64) -> f64 {
    (pps * ZOOM_STEP).clamp(floor, ZOOM_MAX)
}

/// Zoom keeping the second under `anchor_x` where it is (§A); the scroll moves too.
///
/// `anchor_x` is a pixel of the VIEW (0 right of the gutter), `origin_x` the tape pixels off screen left: the second under the anchor is
/// `(anchor_x + origin_x) / pps`.
pub fn zoom_around(pps: f64, factor: f64, anchor_x: f64, origin_x: f64, floor: f64) -> (f64, f64) {
    let time = (anchor_x + origin_x) / pps;
    let next = (pps * factor).clamp(floor, ZOOM_MAX);
    (next, (time * next - anchor_x).max(0.0))
}

// --- Frame steps and the wheel (§1's two wheel rules) ------------------------------------------

/// One frame: ‹f and f› step the red line by a frame.
pub const FRAME_STEP: i64 = 1;

/// With Shift, five.
pub const FRAME_STEP_SHIFT: i64 = 5;

/// ‹f/f› with or without Shift held (§A: "back 5 frames (pauses) — or whatever is held, 5 frames").
pub fn frame_step(shift: bool) -> i64 {
    if shift {
        FRAME_STEP_SHIFT
    } else {
        FRAME_STEP
    }
}

/// What the wheel is over, which decides what it does (§1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WheelOver {
    Transport,
    Preview,
    Tracks,
}

/// What one wheel motion means. A no-op is `StepFrames { frames: 0 }`, not its own variant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Wheel {
    StepFrames { frames: i64 },
    Zoom { factor: f64, at_x: f64 },
    Pan { px: f64 },
}

/// An eighth of the view (§A: "Shift+wheel pans by dx·viewW/8"); a fixed pixel pan would crawl at 240 px/s.
pub fn pan_px(view_px: f64) -> f64 {
    view_px / 8.0
}

/// One wheel event. `dy` is the vertical notch (positive away from the user, as GDK reports it), `dx` the sideways one,
/// which a trackpad sends with no `dy`: that is how a swipe pans while a notch over the tracks zooms.
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
            Wheel::StepFrames {
                frames: step * direction,
            }
        }
        WheelOver::Tracks => {
            if shift || dx.abs() > dy.abs() {
                let direction = if dx < 0.0 {
                    -1.0
                } else if dx > 0.0 {
                    1.0
                } else {
                    dy.signum()
                };
                Wheel::Pan {
                    px: pan_px(view_px) * direction,
                }
            } else {
                Wheel::Zoom {
                    factor: ZOOM_STEP.powf(-dy),
                    at_x: 0.0,
                }
            }
        }
    }
}

/// One frame of a wheel notch's glide: the share of the way left that a frame covers, so a notch starts quickly and
/// settles, and a further notch simply moves the target. Within half a pixel it lands.
pub fn glide_step(origin: f64, target: f64) -> f64 {
    const SHARE: f64 = 0.25;
    if (target - origin).abs() < 0.5 {
        target
    } else {
        origin + (target - origin) * SHARE
    }
}

// --- The transport's three play buttons (§A's group 1) -----------------------------------------

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Preview {
    /// ▶✂ is running: the removed stretches are skipped.
    pub cut_only: bool,
    /// ▶✂✂ is running: one join after the other.
    pub reviewing: bool,
}

/// ▶ is never greyed (§A).
pub fn play_recording_sensitive() -> bool {
    true
}

/// ▶✂ needs something to skip to (§A: "sensitive when segs > 0").
pub fn can_play_cut(cut: &Cut) -> bool {
    !cut.segs.is_empty()
}

/// ▶✂✂ needs joins to review (§A: "sensitive with ≥2 clips"); a card is not a clip.
pub fn can_review(cut: &Cut) -> bool {
    cut.segs.iter().filter(|seg| !seg.is_insert()).count() >= 2
}

/// Lit while ▶✂ runs but not ▶✂✂ (§A: "lit while cutOnly and no review").
pub fn play_cut_lit(preview: &Preview) -> bool {
    preview.cut_only && !preview.reviewing
}

pub fn review_lit(preview: &Preview) -> bool {
    preview.reviewing
}

// --- Tooltips (§A's groups 1, 6 and 7, verbatim) -----------------------------------------------

pub const PLAY_TIP: &str = "play or pause the preview at the playhead";

pub const PLAY_CUT_TIP: &str = "play the CUT instead of the recording: the removed stretches are \
skipped, so this runs the finished video. The clock reads the cut's own time while it does. Changes \
nothing that is saved.";

pub const PLAY_CUT_TIP_LIVE: &str = "pause the cut preview";

pub const REVIEW_TIP: &str = "review every cut in one go: plays 10 s of the finished video before \
each join and 10 s after it, one join after the other, and stops after the last. The removed \
stretches are skipped as under ▶✂. Changes nothing that is saved.";

pub const REVIEW_TIP_LIVE: &str = "pause the cut review";

/// ‹‹f/‹f/f›/f››: §A elides this, so the four share one sentence.
pub const FRAME_TIP: &str = "step the red line by a frame (pauses) — Shift holds five";

/// Undo — §A group 6. The keys are §1's own ("Undo Ctrl+Z").
pub const UNDO_TIP: &str = "Undo — take back the last Add, Remove or Suggest";

/// §1 lists two spellings for Redo, so both go in the tooltip.
pub const REDO_KEYS: &str = " (Ctrl+Shift+Z or Ctrl+Y)";

pub const UNDO_KEYS: &str = " (Ctrl+Z)";

pub const REDO_TIP: &str = "Redo — put it back";

pub const REVERT_TIP: &str = "Revert edits — drop everything you added or removed by hand and go \
back to the last suggestion — or, if you have not suggested yet, to the cut this page opened with";

pub const CLEAR_TIP: &str = "Clear: take every kept stretch and every effect off the timeline, \
leaving the recordings as they were loaded (↶ Undo brings them back)";

pub const ZOOM_OUT_TIP: &str = "zoom the timeline out — it stops where the whole session is on \
screen (the scroll wheel does the same, around the cursor)";

/// + — §A elides this one, so it matches its own sentence.
pub const ZOOM_IN_TIP: &str = "zoom the timeline in — around the middle of what is on screen, and \
it stops at 240 px a second";

// --- What each toolbar group holds (§1 items 5-22, §A groups 1-7) -------------------------------

/// One toolbar control: the name tests and the snapshot entry point find it by, its label and its tooltip.
/// `label` carries the glyph plus words; a button with a [`tool_icon`] wears the icon instead.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tool {
    pub name: &'static str,
    pub label: &'static str,
    pub tip: &'static str,
}

/// The IDLE face of the three preview buttons: `ui::cut_*_play` paints these over the label and keeps its own words for ⏸.
pub const PLAY_GLYPH_RECORDING: &str = "\u{25b6}";
pub const PLAY_GLYPH_CUT: &str = "\u{25b6}\u{2702}";
pub const PLAY_GLYPH_REVIEW: &str = "\u{25b6}\u{2702}\u{2702}";

/// Transport, §1 items 5 → 8. The play buttons keep the names F2.1/F2.2/F2.3; the frame steps share [`FRAME_TIP`].
///
/// Glyphs only (`spec/inventory/cut.md` §A group 1): words cost ~1000 px and clipped Copy/Paste/Insert/Lane off the bar at a 1260 px window.
/// `ui::cut_*_play::PLAY_LABEL`/`PAUSE_LABEL` are the faces a RUNNING preview paints over these.
pub const TRANSPORT_BUTTONS: [Tool; 7] = [
    Tool {
        name: "play-recording-button",
        label: PLAY_GLYPH_RECORDING,
        tip: "Play the recording from the red line \u{2014} every second of it, cuts and all",
    },
    Tool {
        name: "play-cut-button",
        label: PLAY_GLYPH_CUT,
        tip: PLAY_CUT_TIP,
    },
    Tool {
        name: "review-cuts-button",
        label: PLAY_GLYPH_REVIEW,
        tip: REVIEW_TIP,
    },
    Tool {
        name: "line-step-back-five",
        label: "\u{2039}\u{2039}f",
        tip: FRAME_TIP,
    },
    Tool {
        name: "line-step-back",
        label: "\u{2039}f",
        tip: FRAME_TIP,
    },
    Tool {
        name: "line-step-forward",
        label: "f\u{203a}",
        tip: FRAME_TIP,
    },
    Tool {
        name: "line-step-forward-five",
        label: "f\u{203a}\u{203a}",
        tip: FRAME_TIP,
    },
];

/// Verbs, §1 items 10 → 15 in one row: ＋ Add, | Split, － Remove, ⧉ Copy, ⧉ Paste, Insert, ⇲ Lane.
///
/// This table only ORDERS the controls; names, labels and tooltips stay in `cut_verbs::BUTTONS` and `cut_copy::BUTTONS`,
/// and `verb_tools()` builds this list from them plus [`INSERT_TOOL`].
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
    Tool {
        name: "paste-button",
        // §A's long form cost 150 px and scrolled Copy/Paste/Insert out of the bar; the rest of the sentence is in the tooltip.
        label: "\u{29c9} Paste",
        tip: "put what is in hand where the red line is \u{2014} footage splices in, sound lays over kept footage",
    },
    INSERT_TOOL,
    Tool { name: "lane-button", label: "\u{21f2} Lane", tip: "give the copy a row of its own \u{2014} nothing is cut to it yet" },
];

/// Insert/Edit, §1 item 15. `cut_insert.rs` owns the form; this is only the button that opens it.
pub const INSERT_TOOL: Tool = Tool {
    name: "insert-button",
    label: "Insert",
    tip: "put a clip, image, animation or sound into the cut \u{2014} between the footage, over it, or on a lane",
};

pub const EFFECT_MENU_TITLE: &str = "\u{271a} Effect \u{2713}";

/// The effects dropdown's six entries (§1 item 17). The third field is a kind id `cut::EffectKind::parse` accepts; a stop is not a seventh entry
/// (`EffectKind` folds `"speed" | "stop"` into Speed).
pub const EFFECT_ITEMS: [Tool; 6] = [
    Tool {
        name: "effect-item-zoom",
        label: "\u{2295} Zoom",
        tip: "zoom",
    },
    Tool {
        name: "effect-item-text",
        label: "\u{275d} Text",
        tip: "text",
    },
    Tool {
        name: "effect-item-svg",
        label: "\u{25a8} SVG",
        tip: "svg",
    },
    Tool {
        name: "effect-item-speed",
        label: "\u{23e9} Speed",
        tip: "speed",
    },
    Tool {
        name: "effect-item-volume",
        label: "\u{1f50a} Volume",
        tip: "volume",
    },
    Tool {
        name: "effect-item-label",
        label: "\u{1f3f7} Label",
        tip: "label",
    },
];

/// History, §1 items 18 → 21. The key chords live in the tooltip: the page draws no key hints elsewhere (§1).
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
        label: "\u{2717}",
        tip: "Clear: take every kept stretch and every effect off the timeline, leaving the recordings \
as they were loaded (\u{21b6} Undo brings them back)",
    },
];

pub const ZOOM_BUTTONS: [Tool; 2] = [
    Tool {
        name: "zoom-out-button",
        label: "\u{2212}",
        tip: ZOOM_OUT_TIP,
    },
    Tool {
        name: "zoom-in-button",
        label: "+",
        tip: ZOOM_IN_TIP,
    },
];

/// Which history buttons may be pressed, in [`HISTORY_BUTTONS`]' order: Undo, Redo, Revert, Clear.
///
/// `undo_left` and `redo_left` are the counts either side of the pointer in `cut::History` (bounded by [`crate::cut::UNDO_DEPTH`]).
/// `ever_edited` is whether anything was recorded: with none, Revert and Clear have nothing to act on.
pub fn history_buttons_enabled(undo_left: usize, redo_left: usize, ever_edited: bool) -> [bool; 4] {
    [
        undo_left > 0,
        // Nothing above it: the last new edit threw the branch away (§A).
        redo_left > 0,
        // With no edit the screen IS the base.
        ever_edited,
        // Clear earns its colour only once there is something to lose.
        ever_edited,
    ]
}

/// The seven idle rows of the form column, §1 item 4's order; the labels [`idle_readouts`] emits, in its order,
/// so a renamed row fails a test instead of silently reordering the column.
pub const IDLE_FORM_ROWS: [&str; 7] = [
    "Aspect ratio",
    "Playhead",
    "Selection",
    "Cut",
    "Cut at 1\u{00d7}",
    "Source",
    "Segments",
];

/// The widget name the form column gives one reading, derived from its label (spelled only in [`IDLE_FORM_ROWS`] and [`idle_readouts`]).
///
/// Non-alphanumerics become `-` and `×` is spelled out first, so "Cut at 1×" cannot collide with "Cut at 1".
pub fn readout_widget(label: &str) -> String {
    let spelled = label.replace('\u{00d7}', " times");
    let slug: String = spelled
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug = slug.trim_matches('-').replace("--", "-");
    format!("cut-readout-{slug}")
}

// --- The form column's idle readings (§A: "Idle rows: …") --------------------------------------

/// One row of the idle form: label, reading, and §A's tooltip sentence (empty means no tooltip).
#[derive(Debug, Clone, PartialEq)]
pub struct Readout {
    pub label: &'static str,
    pub value: String,
}

/// The shape shown for a cut with no `aspect`: the shape recordings are made in, until the aspect round (F2.x).
pub const ASPECT_DEFAULT: &str = "16:9";

/// The cut's own length, as the render makes it: each clip it plays for what it shows, a card for its `dur`, footage
/// over the rate its speed effects give it.
pub fn cut_seconds(cut: &Cut) -> f64 {
    crate::cut_speed_split::sped_cut(cut)
        .iter()
        .map(crate::narrate_pass::on_screen)
        .sum()
}

/// The same stretches at normal speed (§A's "Cut at 1×" row).
pub fn cut_seconds_at_1x(cut: &Cut) -> f64 {
    cut.segs
        .iter()
        .map(|seg| {
            if seg.is_insert() {
                seg.dur
            } else {
                seg.e - seg.s
            }
        })
        .sum()
}

/// How many scenes the timeline holds, cards included — the count §A calls totalSegs.
pub fn segment_count(cut: &Cut) -> usize {
    cut.segs.len()
}

/// How a stored aspect reads on the page: unset shows the default. The ONE spelling of that rule, shared by `idle_readouts` and F3.2's refresh.
pub fn aspect_shown(aspect: &str) -> String {
    if aspect.is_empty() {
        ASPECT_DEFAULT.to_string()
    } else {
        aspect.to_string()
    }
}

/// The seven idle rows in §A's order; clocks go through [`crate::tools::mm_ss`].
pub fn idle_readouts(
    aspect: &str,
    playhead: f64,
    selection: Option<(f64, f64)>,
    cut: &Cut,
    source_seconds: f64,
) -> Vec<Readout> {
    let aspect = aspect_shown(aspect);
    // No selection reads as silence, not a prompt (§A gives it no wording).
    let selection = match selection {
        Some((start, end)) => format!("{} \u{2013} {}", tools::mm_ss(start), tools::mm_ss(end)),
        None => String::new(),
    };
    vec![
        Readout {
            label: "Aspect ratio",
            value: aspect.to_string(),
        },
        Readout {
            label: "Playhead",
            // The preview's own clock face (inventory §D: "mm:ss.d"), the one the status line reads while ▶ runs.
            value: crate::preview::clock(Some(playhead)),
        },
        Readout {
            label: "Selection",
            value: selection,
        },
        Readout {
            label: "Cut",
            value: tools::mm_ss(cut_seconds(cut)),
        },
        Readout {
            label: "Cut at 1\u{00d7}",
            value: tools::mm_ss(cut_seconds_at_1x(cut)),
        },
        Readout {
            label: "Source",
            value: tools::mm_ss(source_seconds),
        },
        Readout {
            label: "Segments",
            value: segment_count(cut).to_string(),
        },
    ]
}

// --- The picture band's grid (§1: "on the 250 ms grid restarted at each scene change") ----------

/// Seconds one thumbnail slot spans: a `thumb_px` row of `w:h` shape is that wide, and zoom sets px per second.
pub fn slot_seconds(thumb_px: u32, aspect: &str, pps: f64) -> f64 {
    let (w, h) = parse_aspect(aspect);
    if pps <= 0.0 || h == 0 {
        return 0.0;
    }
    (thumb_px as f64 * w / h as f64) / pps
}

/// Does the band have a picture at every zoom? True while one slot spans no more than
/// [`crate::prepare::EXTRACTION_GRID`] (`P.eng.frameGridSeconds`, the 250 ms grid restarted at every scene change).
///
/// Depends on row height and aspect as well as the grid; a 64 px 16:9 row wants a frame every 0.47 s at the ceiling,
/// finer than the grid, so this answers `false`. The band's own round asks Prepare for one frame per slot.
pub fn picture_at_every_zoom(pps: f64, thumb_px: u32, aspect: &str) -> bool {
    slot_seconds(thumb_px, aspect, pps) <= prepare::EXTRACTION_GRID
}

/// `"{w}:{h}"`, or the default shape for anything else.
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

pub const RULER_PX: f64 = 18.0;

pub const SELECTION_BAND_PX: f64 = 22.0;

pub const EFFECT_ROW_PX: f64 = 26.0;

pub const WAVE_LANE_PX: f64 = 30.0;

pub const ROW_GAP_PX: f64 = 3.0;

/// The red playhead: "2 px line on its own layer across both bands" (§1).
pub const RED_LINE_PX: f64 = 2.0;

/// One screenful of drag, geared so the thumb stays 40 px at any zoom (§A).
pub const SCROLLBAR_DRAG_PX: f64 = 40.0;

/// Whether there is a scrollbar at all (§1: "hidden when everything fits").
pub fn scrollbar_shown(total_px: f64, view_px: f64) -> bool {
    total_px > view_px
}

/// §7 (`the tracks are a window drawn under a translate`): the seconds the translate puts on screen, as the range the tracks may be filled from.
/// It never asks for a frame: nothing decodes inside a draw ([`crate::cut_trim::scrubs`] throttles the one place that may).
///
/// `offset_px` is how far the translate has moved the band left, clamped into the band so a drag past either end yields the edge.
/// The gutter is not subtracted: the caller draws it over the band's first 30 px ([`GUTTER_PX`]).
pub fn visible_window(offset_px: f64, view_px: f64, pps: f64, filmed_px: f64) -> (f64, f64) {
    if pps <= 0.0 || view_px <= 0.0 {
        return (0.0, 0.0);
    }
    let start = offset_px.clamp(0.0, filmed_px.max(0.0));
    (start / pps, (start + view_px).min(filmed_px.max(0.0)) / pps)
}

/// ✚ Effect ▾ — §1 item 17's dropdown; the tooltip says what the six entries are for.
pub const EFFECT_MENU_TIP: &str =
    "add an effect over the seconds under the red line \u{2014} zoom, text, \
a drawing, a speed change, a volume or a label";
