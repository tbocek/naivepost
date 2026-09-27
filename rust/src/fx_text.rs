//! F3.4 Text (caption) by hand — `spec/06-effects.md` F3.4, steps S1–S5.
//!
//! ❝ Text arms a gesture on the preview; a drag draws the box the words go in and a click takes the lower third;
//! the form is where the words are typed, and typing them is what places the caption. Everything below is
//! arithmetic or a quoted sentence — including the layout (§S4), which is deliberately an estimate over an average
//! character width rather than a font measurement so that the preview and the render come to the same size to the
//! last decimal. The widget layer holds no rule: `rust/src/ui/window.rs` routes the dropdown's ❝ Text row through
//! `press_text_item` (→ [`arm`], which arms the gesture and records nothing), dispatches the preview's single
//! left-button drag by which entry is armed (→ [`place`], via `text_drag_ended_with_source`), draws the
//! "Text at m:ss" fields from this module's own lists (`show_text_form`), and applies them in `press_text_apply`
//! (→ [`apply`] + `record_edit`, so one ↶ takes the caption back).

use crate::cut::Fx;
use crate::cut_speed;
use crate::fx_zoom;
use crate::tools;

// --- the box ---------------------------------------------------------------------------------------------------

/// A rectangle on the OUTPUT frame — all four numbers fractions of it (§06#1 gives a text its `cx, cy, wf, hf` as
/// output-frame fractions and [`fx_record::fractions_read_off_source_frame`] says which kinds are measured against
/// which frame, so that rule is not repeated here). Stored on [`Fx`] as four `Option`s precisely so "no box" can be
/// told apart from a box at the origin; this type is the settled form, after the default has been taken.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Box_ {
    pub cx: f64,
    pub cy: f64,
    pub wf: f64,
    pub hf: f64,
}

/// F3.4 S2 (`default box: lower third {0.5, 0.78, 0.8, 0.16}`): where a caption goes unless somebody says
/// otherwise — most of the width, low on the frame. [`crate::cut::Fx::centre`] already answers `{0.5, 0.78}` for a
/// text with no stored box; this is the same box with its size spelled, so a caption placed by click and a caption
/// read back from a file cannot diverge.
pub const LOWER_THIRD: Box_ = Box_ { cx: 0.5, cy: 0.78, wf: 0.8, hf: 0.16 };

/// `// effects.boxMinWidthFraction` and `// effects.boxMinHeightFraction` — §10 gives these no `P.` row, so they
/// take the bare prefix of the rule that uses them. The floor a box may be shrunk to is what makes it wide and tall
/// enough to hold a word and to be grabbed again.
pub const MIN_WIDTH_FRACTION: f64 = 0.04;
pub const MIN_HEIGHT_FRACTION: f64 = 0.03;

impl Box_ {
    /// F3.4 S5 (`clamp`): held inside the frame and no smaller than a grabbable box — and, since a box is stored as a
    /// centre plus a size, kept where it was *seen* as well. Clamping the width of a box that hangs off the left edge
    /// slides its centre by half the difference, which is the box jumping under the hand for no reason anyone can
    /// see; so the corners are held inside first and only then turned back into a centre. Both edges may sit exactly
    /// on the frame's, and no further — a box dragged off the picture is not gone, it is only unfindable, and the
    /// hand that put it there cannot get at it to move it back.
    pub fn clamp(self) -> Box_ {
        let wf = self.wf.clamp(MIN_WIDTH_FRACTION, 1.0);
        let hf = self.hf.clamp(MIN_HEIGHT_FRACTION, 1.0);
        // Fractions are already a frame of one, so the corners come straight out of them.
        let x = (self.cx - self.wf / 2.0).clamp(0.0, 1.0 - wf);
        let y = (self.cy - self.hf / 2.0).clamp(0.0, 1.0 - hf);
        Box_ { cx: x + wf / 2.0, cy: y + hf / 2.0, wf, hf }
    }

    /// The box in the pixels of a `frame_w × frame_h` finished frame: corner, width and height. The one place either
    /// side works this geometry out, which is what lets the preview and the render put the words in the same place.
    pub fn to_px(self, frame_w: f64, frame_h: f64) -> (f64, f64, f64, f64) {
        let (w, h) = (self.wf * frame_w, self.hf * frame_h);
        (self.cx * frame_w - w / 2.0, self.cy * frame_h - h / 2.0, w, h)
    }

    /// Pixels back to fractions — the drag's way in. Not clamped here: [`Box_::clamp`] is a separate call so a
    /// caller can see what the hand actually drew before the bound had it.
    #[allow(clippy::too_many_arguments)] // four corners plus the frame they are measured against; one step
    pub fn from_px(x: f64, y: f64, w: f64, h: f64, frame_w: f64, frame_h: f64) -> Box_ {
        Box_ {
            cx: (x + w / 2.0) / frame_w,
            cy: (y + h / 2.0) / frame_h,
            wf: w / frame_w,
            hf: h / frame_h,
        }
    }

    /// The four numbers [`Fx`] stores, in the order its fields read: `cx, cy, hf, wf`. (`hf` before `wf` because
    // that is how §06#1's table lists the box, and a caller assigning them positionally should meet the same order.)
    pub fn stored(self) -> (f64, f64, f64, f64) {
        (self.cx, self.cy, self.hf, self.wf)
    }
}

// --- S1: arming -------------------------------------------------------------------------------------------------

/// F3.4 S1 (`"Drag the box the words go in — anywhere on the picture, any shape. A click puts one across the lower
/// third."`): verbatim, and the trailing space is the join with [`fx_zoom::arm_tail`] — the same second half every
/// armed effect shows, so "3 s" is written down once for all of them.
pub const ARM_WORDS: &str = "Drag the box the words go in \u{2014} anywhere on the picture, any shape. \
                             A click puts one across the lower third. ";

/// F3.4 S1: the arm's whole paragraph — this flow's gesture, then where it will start and for how long. The tail is
/// [`fx_zoom::arm_tail`]'s, shared with ⊕ Zoom because both say the same thing about the red line and the marked
/// stretch, and two copies of a sentence are two sentences to keep true.
pub fn arm_words(marked: Option<(f64, f64)>) -> String {
    format!("{ARM_WORDS}{}", fx_zoom::arm_tail(marked))
}

/// F3.4 S1: what pressing ❝ Text did. Two answers, where ⊕ Zoom's [`fx_zoom::Press`] has three: a text has no
/// refusal branch, because the gesture that places it is a click and a click needs no red line to be a box — the
/// default arrives either way. ([`place`] still asks for seconds, and says so when there are none.)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Press {
    Armed,
    // Consumed by the wire: `press_text_item`'s Disarmed branch clears `TEXT_ARMED` and hides the form when the
    // same entry is pressed a second time. The allowance stays only because this variant carries no data for the
    // module's own tests to assert on beyond equality -- the behaviour it drives is proven widget-side.
    #[allow(dead_code)]
    Disarmed,
}

/// F3.4 S1: the dropdown entry is a toggle — the same kind twice puts the arm down rather than arming a second box.
pub fn arm(already_armed: bool) -> Press {
    if already_armed {
        return Press::Disarmed;
    }
    Press::Armed
}

/// F3.4 S1 (`Camera layer stays up (box is on the output frame)`): the framing stays exactly where it was while a
/// caption is drawn, because the box is measured against what the finished video shows and a camera that stepped
/// aside would move the ground under it. The opposite of ⊕ Zoom's [`fx_zoom::whole_source_shown`], which puts the
/// camera layer DOWN while its box is drawn for the mirror-image reason — a zoom is framed on the source, so it has
/// to see the source. §06#2 already draws a text over both the paused preview and the playing one
/// ([`crate::fx_lane::drawn_paused`], [`crate::fx_lane::drawn_while_playing`]).
pub fn camera_layer_stays_up() -> bool {
    true
}

// --- S2: drag or click, and where in time ------------------------------------------------------------------------

/// F3.4 S2 (`fades 0.3`): P.policy.effectDefaultFades, whose row reads text/svg 0.3 — long enough that a caption
/// does not arrive on one frame, short enough that it is there when read.
pub const FADE_SECONDS: f64 = 0.3;

/// F3.4 S2: what the gesture described. A `Click` is a press that did not travel — or travelled under the tiny-drag
/// floor — and takes the lower third; a `Dragged` box is in preview pixels, which are then fractions of the frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BoxChoice {
    Click,
    Dragged { x: f64, y: f64, w: f64, h: f64 },
}

/// F3.4 S2 (`Drag or click (default box: lower third); t/dur from the line (3 s) or selection; fades 0.3`).
///
/// The length default is [`fx_zoom::DEFAULT_SECONDS`] rather than a second 3.0 — P.policy.effectDefaultSeconds
/// names zoom, text and svg together, so one constant covers all three. A drag under [`fx_zoom::DRAG_MIN_PX`] is a
/// click: §F's "tiny drag 12" is one floor for every drawing gesture, not one per kind. With neither a selection nor
/// a line there is no second to place it at — the same question ⊕ Zoom asks and the same answer it gets; the words
/// for that live on the arm path ([`NO_WORDS`] is about empty words, which is a different missing thing).
pub fn place(
    choice: BoxChoice,
    line: Option<f64>,
    selection: Option<(f64, f64)>,
    frame: (f64, f64),
) -> Option<Fx> {
    let on = match choice {
        BoxChoice::Click => LOWER_THIRD,
        // A press that did not travel is a click wherever it landed.
        BoxChoice::Dragged { w, h, .. } if w < fx_zoom::DRAG_MIN_PX || h < fx_zoom::DRAG_MIN_PX => LOWER_THIRD,
        BoxChoice::Dragged { x, y, w, h } => Box_::from_px(x, y, w, h, frame.0, frame.1).clamp(),
    };
    let (t, dur) = match (selection, line) {
        (Some((from, to)), _) => (from.min(to), (to - from).abs()),
        (None, Some(at)) => (at, fx_zoom::DEFAULT_SECONDS),
        (None, None) => return None,
    };
    let (cx, cy, hf, wf) = on.stored();
    Some(Fx {
        kind: "text".into(),
        t,
        dur,
        trans: FADE_SECONDS,
        tout: FADE_SECONDS,
        cx: Some(cx),
        cy: Some(cy),
        hf: Some(hf),
        wf: Some(wf),
        // The words come later, from the form; `ease` stays empty so an untouched file writes no key (§06#1).
        ..Default::default()
    })
}

// --- S3: the form -----------------------------------------------------------------------------------------------

/// F3.4 S3 (`Form "Text at m:ss"`): titled by the second it belongs to, since a caption is placed at the red line
/// and its form has to say which line it has.
pub fn form_title(t: f64) -> String {
    format!("Text at {}", tools::mm_ss(t))
}

/// F3.4 S3 (`the words (3-line box; Enter = new line), Length, Fade in, Fade out, Curve`): the fields, in the order
/// the form reads them. Order is the sentence a person reads, not a layout detail — as in
/// [`crate::fx_zoom::FORM_FIELDS`] and [`crate::cut_speed::FORM_FIELDS`]. The words come first because they are what
/// the caption is: nothing else on this form can make one.
pub const FORM_FIELDS: [&str; 5] =
    ["Words", "Length (s)", "Fade in (s)", "Fade out (s)", "Curve"];

/// F3.4 S3: the words field's help, verbatim — it says both what is fitted to what and where Enter goes, which is
/// the one thing a person cannot guess from a text box.
pub const WORDS_HELP: &str = "what is written over the picture. The words are fitted to the box you drew \u{2014} \
                              a longer line comes out smaller, and Enter starts a new line.";

/// F3.4 S3 (`Fade in/out (s) ("0 cuts them straight on|off")`): one sentence serves both fades, which is why it is
/// a constant rather than two strings that would have to be kept agreeing.
pub const FADES_HELP: &str = "0 cuts them straight on|off";

/// F3.4 S3 (`the words (3-line box; Enter = new line)`): three rows tall, and its height is exactly why Enter is a
/// newline rather than Apply — a one-line entry that ended the form on Enter would leave no way to break a line.
pub const WORDS_ROWS: usize = 3;

/// F3.4 S3 (`length ≥ 0.3`): §06 §6's forms' typed floor "text/svg 0.3" — `// effects.textMinSeconds`, which §10
/// gives no `P.` row. A caption of no length does nothing, cannot be grabbed again on the lane, and looks exactly
/// like the form having thrown the words away.
pub const MIN_SECONDS: f64 = 0.3;

/// F3.4 S3 (`Empty words not placed ("type the words and they go on the picture — the form applies as you type
/// it")`): the refusal when there is nothing to place. A caption of spaces is not a caption, and placing one would
/// put an empty box on the lane that says nothing about why it is there.
pub const NO_WORDS: &str =
    "type the words and they go on the picture \u{2014} the form applies as you type it";

/// F3.4 S3: what the form holds when Apply is pressed — the seconds and fades as typed, the curve's display name,
/// the words (typed newlines kept), and the box the gesture left.
#[derive(Debug, Clone, PartialEq)]
pub struct Form {
    pub t: f64,
    pub dur: f64,
    pub trans: f64,
    pub tout: f64,
    /// The curve's display name; [`fx_zoom::curve_stored`] decides what gets written.
    pub curve: String,
    pub words: String,
    pub on: Box_,
}

/// F3.4 S3: the record the form writes back. Empty words are refused with [`NO_WORDS`]; a length under
/// [`MIN_SECONDS`] is refused naming its floor (the shape [`fx_zoom::apply`]'s refusal uses: what is wrong, then the
/// bound — §F3.1 gives no words for either and a greyed Apply should never reach here).
///
/// The fades go through [`cut_speed::clamp_fades`] — this is the third reader of §A.2's `clampFades` (zoom, speed,
/// text), which is the point of it being one function: three forms cannot disagree about what two overrunning fades
/// share out of a band. The box is clamped on the way in, so no later step has to guard against a hand-edited file.
pub fn apply(form: &Form) -> Result<Fx, String> {
    if form.words.trim().is_empty() {
        return Err(NO_WORDS.to_string());
    }
    if form.dur < MIN_SECONDS {
        return Err(format!(
            "a caption of {:.2} s is shorter than the shortest one this form takes ({MIN_SECONDS} s)",
            form.dur
        ));
    }
    let (trans, tout) = cut_speed::clamp_fades(form.trans, form.tout, form.dur);
    let (cx, cy, hf, wf) = form.on.clamp().stored();
    Ok(Fx {
        kind: "text".into(),
        t: form.t,
        dur: form.dur,
        trans,
        tout,
        cx: Some(cx),
        cy: Some(cy),
        hf: Some(hf),
        wf: Some(wf),
        // Typed newlines are kept verbatim: §S4 breaks on them, so a line the person split stays split.
        text: form.words.clone(),
        ease: fx_zoom::curve_stored(&form.curve).to_string(),
        ..Default::default()
    })
}

// --- S4: fitting -------------------------------------------------------------------------------------------------

/// F3.4 S4 (`0.58 em per character`; `// effects.textAdvance`): the average width of a character as a fraction of
/// the font size. Sans-serif faces sit a little over half an em, and this is an estimate rather than a measurement
/// on purpose — see [`fit`].
pub const CHAR_ADVANCE_EM: f64 = 0.58;

/// F3.4 S4 (`lines·size·1.25 ≤ boxH`; `// effects.textLineHeight`): the distance between baselines, in font sizes.
pub const LINE_HEIGHT_EM: f64 = 1.25;

/// F3.4 S4 (`0.95`; `// effects.textAscent`): how far the first baseline sits below the top of the block.
pub const ASCENT_EM: f64 = 0.95;

/// F3.4 S4 (`floor 7 pt`; `// effects.textMinPoints`): as small as fitting is allowed to go. Past this the words are
/// not readable at video sizes, so the box overflows instead — and a visible mistake can be fixed.
pub const MIN_POINTS: f64 = 7.0;

/// F3.4 S4 (`≤ 12 lines`): the most a box is broken into. A box asked to hold a paragraph would otherwise shrink
/// until it was a grey smudge; this stops at a size that still reads and lets the rest run out of the box.
pub const MAX_LINES: usize = 12;

/// F3.4 S4: how many halvings the search takes (`// effects.fitSearchSteps`). Enough that the answer is settled to
/// far below a pixel, and fixed rather than tolerance-driven so two builds cannot land on different sizes.
pub const FIT_SEARCH_STEPS: usize = 40;

/// F3.4 S4 (`explicit newlines break; long words hard-split`): the words as the layout will break them. Characters
/// per line come from [`CHAR_ADVANCE_EM`]; `CRLF` is normalised first, every explicit newline breaks, a blank typed
/// line survives as an empty line (it was meant), anything longer than the width wraps between words, and one word
/// too long for the box is split rather than left to run out of it — a single over-long word is a URL or a hashtag,
/// not a mistake to preserve.
pub fn wrap_lines(text: &str, width: f64, size: f64) -> Vec<String> {
    let advance = (size * CHAR_ADVANCE_EM).max(1e-6);
    let mut per_line = (width / advance).floor() as usize;
    if per_line < 1 {
        per_line = 1;
    }

    let mut out: Vec<String> = Vec::new();
    for paragraph in text.replace("\r\n", "\n").split('\n') {
        let words: Vec<&str> = paragraph.split_whitespace().collect();
        if words.is_empty() {
            out.push(String::new()); // a blank line was typed on purpose
            continue;
        }
        let mut line = String::new();
        for word in words {
            let mut word = word.to_string();
            while word.chars().count() > per_line {
                if !line.is_empty() {
                    out.push(std::mem::take(&mut line));
                }
                let head: String = word.chars().take(per_line).collect();
                word = word.chars().skip(per_line).collect();
                out.push(head); // a piece too long for the line goes on its own
            }
            let joined = if line.is_empty() {
                word.clone()
            } else {
                format!("{line} {word}")
            };
            if joined.chars().count() <= per_line {
                line = joined;
            } else {
                out.push(std::mem::take(&mut line));
                line = word;
            }
        }
        if !line.is_empty() {
            out.push(line);
        }
    }
    out
}

/// F3.4 S4 (`fitted: the largest size where ≤ 12 lines of 0.58 em per character fit the box (min 7 pt)`): the whole
/// of the layout — the largest font size at which the words fit, and the lines at that size, in the box's own units.
///
/// A binary search over an estimate rather than a font measurement, and that is what makes §S4's `preview = render`
/// true: cairo could measure, librsvg will not, so the one number either side has to agree on is arithmetic. Same
/// function, same answer, to the last decimal — see [`preview_is_the_render`].
pub fn fit(text: &str, box_w: f64, box_h: f64) -> (f64, Vec<String>) {
    if text.trim().is_empty() || box_w <= 0.0 || box_h <= 0.0 {
        // Nothing to say, or nowhere to say it: no text at all rather than a zero-sized draw to guard against.
        return (0.0, Vec::new());
    }
    let fits = |size: f64| -> bool {
        let lines = wrap_lines(text, box_w, size);
        lines.len() <= MAX_LINES && (lines.len() as f64) * size * LINE_HEIGHT_EM <= box_h
    };

    // One line as tall as the box is the ceiling; nothing bigger can fit.
    let mut hi = box_h / LINE_HEIGHT_EM;
    if fits(hi) {
        return (hi, wrap_lines(text, box_w, hi));
    }
    // The floor is always allowed: a hopeless box overflows visibly rather than vanishing.
    let mut lo = MIN_POINTS.min(hi);
    for _ in 0..FIT_SEARCH_STEPS {
        let mid = (lo + hi) / 2.0;
        if fits(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo, wrap_lines(text, box_w, lo))
}

/// F3.4 S4: where each line's baseline sits, given the box in pixels and the size [`fit`] chose. The block is
/// centred in the box vertically and each line is centred horizontally, so the x is the box's middle for all of
/// them and only these y values travel to the drawer.
pub fn baselines(y: f64, box_h: f64, size: f64, n: usize) -> Vec<f64> {
    if n == 0 {
        return Vec::new();
    }
    let block = n as f64 * size * LINE_HEIGHT_EM;
    let top = y + (box_h - block).max(0.0) / 2.0;
    (0..n)
        .map(|i| top + i as f64 * size * LINE_HEIGHT_EM + size * ASCENT_EM)
        .collect()
}

/// F3.4 S4 (`a dark dilated edge`; `// effects.edgeRadius`): how far the edge reaches, as a fraction of the font
/// size — so it scales with the words instead of turning into a halo on a small caption.
pub const EDGE_RADIUS_EM: f64 = 0.08;

/// F3.4 S4 (`alpha 0.85`; `// effects.edgeAlpha`): how dark the edge is. Not fully opaque, because a black outline
/// at full strength reads as a second set of words rather than as ground for these ones.
pub const EDGE_ALPHA: f64 = 0.85;

/// F3.4 S4 (`16 directions`); `// effects.edgeSteps`): eight was faceted and fatter on diagonals.
pub const EDGE_STEPS: usize = 16;

/// The radius the preview dilates by — never under a pixel, or the edge of small type disappears between samples.
pub fn edge_radius(size: f64) -> f64 {
    (size * EDGE_RADIUS_EM).max(1.0)
}

/// The stroke width the render draws with: twice [`edge_radius`], because a stroke straddles its path. One constant
/// for both, or the thumbnail and the video differ in a way only a screenshot would show.
pub fn render_stroke_width(size: f64) -> f64 {
    edge_radius(size) * 2.0
}

/// F3.4 S4 (`same function for preview and render`): the bargain [`fit`] strikes, published as a fact so it can be
/// asserted rather than argued — there is one layout function, so there is nothing to keep in step.
pub fn preview_is_the_render() -> bool {
    true
}

// --- S5: moving and sizing the box on the preview -----------------------------------------------------------------

/// F3.4 S5 (`snapping within 10 px`; `// effects.snapPx`): §06 §6's "snap 8/10 px" pair, and this is the preview's
/// half — a box is dragged across a picture, not onto a clip border. Deliberately NOT [`crate::cut_select::SNAP_PX`]
/// (8 px), which pulls a timeline band onto a word edge: two different hands on two different surfaces, so
/// unifying the constants would silently change one of the two rules.
pub const SNAP_PX: f64 = 10.0;

/// F3.4 S5 (`resize independent axes (16 px floor)`; `// effects.boxResizeFloorPx`): the smallest box an edge may be
/// dragged to, so a caption cannot be shrunk into nothing and lost.
pub const MIN_BOX_PX: f64 = 16.0;

/// F3.4 S5 (`the finished frame's left edge, centre, right edge (and top, middle, bottom)`): the lines a box snaps
/// to on one axis. The same three serve x and y — width and height are independent, so the rule is stated once per
/// axis and applied twice.
pub fn frame_lines(frame_px: f64) -> [f64; 3] {
    [0.0, frame_px / 2.0, frame_px]
}

/// F3.4 S5 (`a moved box offers all three of its own lines`): moving is aimed with the box as a whole, so any of
/// its left edge, middle and right edge may be what was lined up with a frame line.
pub fn move_lines(edge_px: f64, size_px: f64) -> [f64; 3] {
    [edge_px, edge_px + size_px / 2.0, edge_px + size_px]
}

/// F3.4 S5 (`a dragged edge only itself`): sizing is aimed with one edge, so the other two lines are not offered —
/// pulling a side to the centre must not be second-guessed into moving the middle there instead.
pub fn resize_lines(edge_px: f64) -> [f64; 1] {
    [edge_px]
}

/// F3.4 S5: pull `to` onto the nearest of `lines` within `reach_px`, or leave it exactly where the hand left it.
pub fn snap_to(to: f64, lines: &[f64], reach_px: f64) -> f64 {
    lines
        .iter()
        .filter(|line| (*line - to).abs() <= reach_px)
        .min_by(|a, b| (*a - to).total_cmp(&(*b - to)))
        .copied()
        .unwrap_or(to)
}

/// F3.4 S5: the shift a gesture takes. Snapping is a comparison between two sets of lines — the box's own and the
/// frame's — not a pull towards a list containing the thing being moved, which would always land on itself at
/// distance zero. The closest (box line, frame line) pair within reach decides it, and its gap is the shift; with no
/// pair in reach the hand's position stands. `None` rather than `Some(0.0)` for "nothing snapped" keeps a genuine
/// zero gap (a line already on a line) from looking different from no snap at all — the arithmetic is the same, and
/// that is the point.
fn snap_shift(box_lines: &[f64], frame: &[f64], reach_px: f64) -> Option<f64> {
    let mut best: Option<f64> = None;
    for line in box_lines {
        for edge in frame {
            let gap = edge - line;
            if gap.abs() <= reach_px && best.is_none_or(|b| gap.abs() < b.abs()) {
                best = Some(gap);
            }
        }
    }
    best
}

/// F3.4 S5 (`On the preview a box moves — snapping within 10 px to the finished frame's left edge, centre, right
/// edge (and top, middle, bottom); a moved box offers all three of its own lines`): the box after a drag of `delta`
/// pixels. Each axis is decided on its own (independent axes) by [`snap_shift`] — the box's own left edge, middle and
/// right edge against the frame's three — and then held inside the frame.
///
/// The snap moves the box's top-left corner: which of its three lines was near a frame line decides how far it ends
/// up, but the size never changes. Snapping the centre instead would resize the box under the hand whenever its
/// middle lined up with something.
pub fn move_box(from: Box_, delta: (f64, f64), frame: (f64, f64)) -> Box_ {
    let (x, y, w, h) = from.to_px(frame.0, frame.1);
    let sx = snap_shift(&move_lines(x + delta.0, w), &frame_lines(frame.0), SNAP_PX).unwrap_or(0.0);
    let sy = snap_shift(&move_lines(y + delta.1, h), &frame_lines(frame.1), SNAP_PX).unwrap_or(0.0);
    Box_ {
        cx: (x + delta.0 + sx + w / 2.0) / frame.0,
        cy: (y + delta.1 + sy + h / 2.0) / frame.1,
        wf: from.wf,
        hf: from.hf,
    }
    .clamp()
}

/// F3.4 S5: which edge a resize drag has hold of.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

/// F3.4 S5 (`and resizes (independent axes, 16 px floor)`): the box after one of its edges is dragged to `to_px`.
/// The line offered for snapping is only the dragged edge itself ([`resize_lines`]) — pulling a side to the centre
/// must not be second-guessed into moving the middle there instead — and it is compared against the frame's lines on
/// its own axis. The other axis is left exactly as it was, and the box never goes under [`MIN_BOX_PX`] on the axis
/// being pulled: the floor is applied after the snap, so a line that happens to sit inside the floor still cannot
/// make an ungrabbable box.
pub fn resize_box(from: Box_, edge: Edge, to_px: f64, frame: (f64, f64)) -> Box_ {
    let (mut x, mut y, mut w, mut h) = from.to_px(frame.0, frame.1);
    // The far edge is the anchor; only this edge and the size on this axis move.
    match edge {
        Edge::Left => {
            let right = x + w;
            let to = snap_to(to_px, &frame_lines(frame.0), SNAP_PX).min(right - MIN_BOX_PX);
            x = to;
            w = right - to;
        }
        Edge::Right => {
            let to = snap_to(to_px, &frame_lines(frame.0), SNAP_PX).max(x + MIN_BOX_PX);
            w = to - x;
        }
        Edge::Top => {
            let bottom = y + h;
            let to = snap_to(to_px, &frame_lines(frame.1), SNAP_PX).min(bottom - MIN_BOX_PX);
            y = to;
            h = bottom - to;
        }
        Edge::Bottom => {
            let to = snap_to(to_px, &frame_lines(frame.1), SNAP_PX).max(y + MIN_BOX_PX);
            h = to - y;
        }
    }
    Box_::from_px(x, y, w, h, frame.0, frame.1).clamp()
}

/// F3.4 S5 (`a press without travel toggles play/pause wherever it lands`): the box must not swallow a press that
/// was meant for the picture under it — the same press elsewhere on the preview does the same thing, so landing on
/// a caption cannot be allowed to mean something else. What the box does with a press it keeps is governed by
/// [`crate::fx_lane::held_drawn_full`] and [`crate::fx_lane::held_outline_dashed`], not by anything here.
pub fn press_without_travel_toggles_play() -> bool {
    true
}
