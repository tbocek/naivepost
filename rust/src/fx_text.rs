//! F3.4 Text (caption) by hand — `spec/06-effects.md` F3.4, steps S1–S5.

use crate::cut::Fx;
use crate::cut_speed;
use crate::fx_zoom;
use crate::tools;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Box_ {
    pub cx: f64,
    pub cy: f64,
    pub wf: f64,
    pub hf: f64,
}

pub const LOWER_THIRD: Box_ = Box_ {
    cx: 0.5,
    cy: 0.78,
    wf: 0.8,
    hf: 0.16,
};

pub const MIN_WIDTH_FRACTION: f64 = 0.04;
pub const MIN_HEIGHT_FRACTION: f64 = 0.03;

impl Box_ {
    pub fn clamp(self) -> Box_ {
        let wf = self.wf.clamp(MIN_WIDTH_FRACTION, 1.0);
        let hf = self.hf.clamp(MIN_HEIGHT_FRACTION, 1.0);
        // Fractions are already a frame of one, so the corners come straight out of them.
        let x = (self.cx - self.wf / 2.0).clamp(0.0, 1.0 - wf);
        let y = (self.cy - self.hf / 2.0).clamp(0.0, 1.0 - hf);
        Box_ {
            cx: x + wf / 2.0,
            cy: y + hf / 2.0,
            wf,
            hf,
        }
    }

    pub fn to_px(self, frame_w: f64, frame_h: f64) -> (f64, f64, f64, f64) {
        let (w, h) = (self.wf * frame_w, self.hf * frame_h);
        (
            self.cx * frame_w - w / 2.0,
            self.cy * frame_h - h / 2.0,
            w,
            h,
        )
    }

    #[allow(clippy::too_many_arguments)] // four corners plus the frame they are measured against; one step
    pub fn from_px(x: f64, y: f64, w: f64, h: f64, frame_w: f64, frame_h: f64) -> Box_ {
        Box_ {
            cx: (x + w / 2.0) / frame_w,
            cy: (y + h / 2.0) / frame_h,
            wf: w / frame_w,
            hf: h / frame_h,
        }
    }

    // that is how §06#1's table lists the box, and a caller assigning them positionally should meet the same order.)
    pub fn stored(self) -> (f64, f64, f64, f64) {
        (self.cx, self.cy, self.hf, self.wf)
    }
}

pub const ARM_WORDS: &str =
    "Drag the box the words go in \u{2014} anywhere on the picture, any shape. \
                             A click puts one across the lower third. ";

pub fn arm_words(marked: Option<(f64, f64)>) -> String {
    format!("{ARM_WORDS}{}", fx_zoom::arm_tail(marked))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Press {
    Armed,
    // Consumed by the wire: `press_text_item`'s Disarmed branch clears `TEXT_ARMED` and hides the form when the
    // same entry is pressed a second time. The allowance stays only because this variant carries no data for the
    // module's own tests to assert on beyond equality -- the behaviour it drives is proven widget-side.
    #[allow(dead_code)]
    Disarmed,
}

pub fn arm(already_armed: bool) -> Press {
    if already_armed {
        return Press::Disarmed;
    }
    Press::Armed
}

pub fn camera_layer_stays_up() -> bool {
    true
}

pub const FADE_SECONDS: f64 = 0.3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BoxChoice {
    Click,
    Dragged { x: f64, y: f64, w: f64, h: f64 },
}

/// A drag on the picture read as a box. A press that did not travel is a CLICK wherever it landed.
pub fn drag_choice(from: (f64, f64), to: (f64, f64)) -> BoxChoice {
    let (w, h) = ((to.0 - from.0).abs(), (to.1 - from.1).abs());
    if w < fx_zoom::DRAG_MIN_PX || h < fx_zoom::DRAG_MIN_PX {
        return BoxChoice::Click;
    }
    BoxChoice::Dragged {
        x: from.0.min(to.0),
        y: from.1.min(to.1),
        w,
        h,
    }
}

pub fn place(
    choice: BoxChoice,
    line: Option<f64>,
    selection: Option<(f64, f64)>,
    frame: (f64, f64),
) -> Option<Fx> {
    let on = match choice {
        BoxChoice::Click => LOWER_THIRD,
        // A press that did not travel is a click wherever it landed.
        BoxChoice::Dragged { w, h, .. } if w < fx_zoom::DRAG_MIN_PX || h < fx_zoom::DRAG_MIN_PX => {
            LOWER_THIRD
        }
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

pub fn form_title(t: f64) -> String {
    format!("Text at {}", tools::mm_ss(t))
}

pub const FORM_FIELDS: [&str; 5] = [
    "Words",
    "Length (s)",
    "Fade in (s)",
    "Fade out (s)",
    "Curve",
];

pub const WORDS_HELP: &str =
    "what is written over the picture. The words are fitted to the box you drew \u{2014} \
                              a longer line comes out smaller, and Enter starts a new line.";

pub const FADES_HELP: &str = "0 cuts them straight on|off";

pub const WORDS_ROWS: usize = 3;

pub const SNAPSHOT_CAPTION: &str = "Ethereum in one lecture";

pub const MIN_SECONDS: f64 = 0.3;

pub const NO_WORDS: &str =
    "type the words and they go on the picture \u{2014} the form applies as you type it";

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

pub const CHAR_ADVANCE_EM: f64 = 0.58;

pub const LINE_HEIGHT_EM: f64 = 1.25;

pub const ASCENT_EM: f64 = 0.95;

pub const MIN_POINTS: f64 = 7.0;

pub const MAX_LINES: usize = 12;

pub const FIT_SEARCH_STEPS: usize = 40;

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

pub const EDGE_RADIUS_EM: f64 = 0.08;

pub const EDGE_ALPHA: f64 = 0.85;

pub const EDGE_STEPS: usize = 16;

/// The radius the preview dilates by — never under a pixel, or the edge of small type disappears between samples.
pub fn edge_radius(size: f64) -> f64 {
    (size * EDGE_RADIUS_EM).max(1.0)
}

pub fn render_stroke_width(size: f64) -> f64 {
    edge_radius(size) * 2.0
}

pub const SNAP_PX: f64 = 10.0;

pub const MIN_BOX_PX: f64 = 16.0;

pub fn frame_lines(frame_px: f64) -> [f64; 3] {
    [0.0, frame_px / 2.0, frame_px]
}

pub fn move_lines(edge_px: f64, size_px: f64) -> [f64; 3] {
    [edge_px, edge_px + size_px / 2.0, edge_px + size_px]
}

pub fn snap_to(to: f64, lines: &[f64], reach_px: f64) -> f64 {
    lines
        .iter()
        .filter(|line| (*line - to).abs() <= reach_px)
        .min_by(|a, b| (*a - to).total_cmp(&(*b - to)))
        .copied()
        .unwrap_or(to)
}

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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

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

pub const EDGE_GRIP_PX: f64 = 8.0;

pub fn grab_edge(box_px: (f64, f64, f64, f64), at_px: (f64, f64)) -> Option<Edge> {
    let (x, y, w, h) = box_px;
    let gaps = [
        ((Edge::Left), (at_px.0 - x).abs()),
        ((Edge::Right), (at_px.0 - (x + w)).abs()),
        ((Edge::Top), (at_px.1 - y).abs()),
        ((Edge::Bottom), (at_px.1 - (y + h)).abs()),
    ];
    gaps.iter()
        .filter(|(_, gap)| *gap <= EDGE_GRIP_PX)
        .min_by(|(_, a), (_, b)| a.total_cmp(b))
        .map(|(edge, _)| *edge)
}

pub fn snap_lines_px(frame: (f64, f64)) -> Vec<(f64, f64, f64, f64)> {
    let vertical = frame_lines(frame.0);
    let horizontal = frame_lines(frame.1);
    vertical
        .iter()
        .map(|x| (*x - 0.5, 0.0, 1.0, frame.1))
        .chain(horizontal.iter().map(|y| (0.0, *y - 0.5, frame.0, 1.0)))
        .collect()
}
