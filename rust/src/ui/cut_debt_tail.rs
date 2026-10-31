//! §06-effects#2-the-lane-and-the-preview — the dashed debt tail on a lane bar.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut::{Cut, Fx, Seg};
use crate::cut_hear;
use crate::effect_details;
use crate::fx_lane;
use crate::narrate_pass;
use crate::timeline;

pub fn tail_open(cut: &Cut, index: usize, t: f64, recordings: &[timeline::Recording]) -> bool {
    let Some(effect) = cut.fx.get(index) else {
        return false;
    };
    let Some(clip) = clip_under(&cut.segs, effect.t) else {
        return false;
    };
    // The debt is the whole record's length less the seconds it has run by the line, clamped inside
    // the record exactly as `fx_lane::plate_debt_at` clamps the plate's number, so a stale line
    // cannot draw a tail longer than the effect.
    let on_screen = (t - effect.t).clamp(0.0, effect.dur);
    effect_details::read_head_open(
        &effect.snd,
        fx_lane::debt(effect.dur, on_screen),
        scene_runs_on(cut, effect, clip, t),
        filmed(clip, recordings),
        is_insert(clip, effect),
    )
}

fn clip_under(segs: &[Seg], at: f64) -> Option<&Seg> {
    segs.iter().find(|seg| seg.s <= at && at < seg.e)
}

fn scene_runs_on(cut: &Cut, effect: &Fx, clip: &Seg, t: f64) -> bool {
    let rate = cut_hear::rate_under(&cut.fx, t);
    let survived = narrate_pass::on_screen(clip) / rate.max(f64::EPSILON);
    // The record's end measured from the clip's start: a record whose own seconds outlast the clip
    // leaves the scene no room for a tail.
    t + fx_lane::debt(effect.dur, (t - effect.t).clamp(0.0, effect.dur)) < clip.s + survived
}

fn filmed(clip: &Seg, recordings: &[timeline::Recording]) -> bool {
    let runs = timeline::filmed_runs(recordings);
    if runs.is_empty() {
        // No session named at all (a page with no tape yet): say nothing rather than a tail about
        // nothing.
        return false;
    }
    runs.iter()
        .any(|(start, end)| start < &clip.e && clip.s < *end)
}

fn is_insert(clip: &Seg, _effect: &Fx) -> bool {
    !clip.ins.is_empty()
}

pub fn wear_tail(
    bar: &gtk::Widget,
    cut: &Cut,
    index: usize,
    t: f64,
    recordings: &[timeline::Recording],
) {
    let held_classes = bar.css_classes();
    let mut classes: Vec<&str> = held_classes
        .iter()
        .map(|class| class.as_str())
        .filter(|class| *class != "fx-debt-tail")
        .collect();
    if tail_open(cut, index, t, recordings) {
        classes.push("fx-debt-tail");
    }
    bar.set_css_classes(&classes);
}

pub fn css() -> String {
    let violet = fx_lane::HELD_OUTLINE;
    format!(
        ".fx-debt-tail {{ border-right: 2px dashed rgba({}, {}, {}, 1.0); }}\n",
        (violet[0] * 255.0).round() as i32,
        (violet[1] * 255.0).round() as i32,
        (violet[2] * 255.0).round() as i32
    )
}
