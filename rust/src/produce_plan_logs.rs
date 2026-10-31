//! §08-produce#6 bullet 1: the clip-planning refusals and log lines, said BY THE RENDER.
//!
//! §6 spells eight sentences about what a plan found on a clip; their wordings live in [`crate::produce_details`]
//! ("S1: clip planning refusals"). This walks the SAME sped segment list `produce_exec::plan` hands
//! [`crate::produce_render::plan_clips`] (so "clip 7" means the same clip everywhere) and says only §6's
//! sentences; the segments `plan_clips` drops are skipped so nothing is said twice. Separate because it needs
//! three caller lookups and `plan_clips` already takes ten arguments.

use crate::cut::Seg;
use crate::effect_overlap;
use crate::narration::Entry;
use crate::produce_details as details;
use crate::produce_read_head;
use crate::produce_render as render;
use crate::timeline::Recording;

/// What the walk needs beyond the segments: the doors only a caller with the session in hand can answer.
pub struct Doors<'a> {
    /// The project's lanes, asked per clip through `cut_hear` as `plan_clips` asks: a clip hearing no lane has
    /// no sound at all (§6's "with no sound — it plays silent").
    pub lanes: &'a [crate::cut::Lane],
    /// The filmed spans, so a SECOND can be placed on tape: `Materials::on_recording` only says whether the
    /// session has tape at all. Empty means no tape anywhere. Built by [`Doors::filmed`].
    pub runs: &'a [(f64, f64)],
    /// The base each clip copies or reads from: the footage row's name for footage and `copy:`, the asset's own
    /// for an insert (§6's "past the end of <base>" sentences name it).
    pub base_of: &'a dyn Fn(&Seg) -> String,
    /// Where a line's synthesis already is. A line with no wav is §6's "no synthesis for a line".
    pub wav_of: &'a dyn Fn(&Entry) -> Option<std::path::PathBuf>,
    /// Does an insert's asset still exist: the door `plan_clips` answers its "is not there any more" line with,
    /// so a clip with a reason says it once. Asked with the cut's own spelling.
    pub exists: &'a dyn Fn(&str) -> bool,
}

impl<'a> Doors<'a> {
    /// The filmed spans a walk needs, from the recordings a snapshot gathered ([`crate::timeline::filmed_runs`]).
    pub fn filmed(recordings: &[Recording]) -> Vec<(f64, f64)> {
        crate::timeline::filmed_runs(recordings)
    }
}

/// A caller with no tape: no spans, so every clip sits on no recording and each missing-recording sentence is
/// readable in the log.
pub const NO_TAPE: &[(f64, f64)] = &[];

/// The seconds the clip keeps when its tape gives out, or `None` when nothing is shortened: the filmed run the
/// clip starts in ends before the clip does. A clip in no run is not "shortened" (the no-recording sentence
/// refused it). Comparing with the tape left AFTER the clip's end (F4.3's grow bound) wrongly shortened clips.
fn kept_length(seg: &Seg, runs: &[(f64, f64)]) -> Option<f64> {
    let (_, end) = runs
        .iter()
        .find(|(start, end)| *start <= seg.s + 1e-6 && seg.s < *end)?;
    (seg.e > end + 1e-6).then(|| end - seg.s)
}

/// The planned clips' refusals and news, in clip-number order, in the wording §6 spells.
///
/// `segs` is the sped list (`crate::cut_speed_split::sped_cut`) `plan_clips` saw; `entries` the narration
/// lines in playing order.
pub fn refusals(
    segs: &[Seg],
    fx_for: impl Fn(&Seg) -> Vec<crate::cut::Fx>,
    entries: &[Entry],
    doors: &Doors,
) -> Vec<String> {
    let mut logs: Vec<String> = Vec::new();
    for (index, seg) in segs.iter().enumerate() {
        let no = index + 1;
        let fx = effect_overlap::drawn_on_scene(&fx_for(seg), seg);
        let on_screen = crate::narrate_pass::on_screen(seg);
        // Skip the drops plan_clips already named: one failure, one line, same clip number.
        let missing_asset =
            !seg.ins.is_empty() && !seg.ins.starts_with("copy:") && !(doors.exists)(&seg.ins);
        if missing_asset || on_screen < crate::tools::cutpass::MIN_CLIP_SECONDS {
            continue;
        }
        // A pasted stretch is read from the seconds it was copied from, not where it was pasted.
        let read = seg.copy_seconds().map_or_else(
            || seg.clone(),
            |from| crate::cut::Seg {
                s: from,
                e: from + on_screen,
                ..seg.clone()
            },
        );
        let seg = &read;
        let on_tape = produce_read_head::on_recording(seg, doors.runs);
        let base = (doors.base_of)(seg);
        let copied = seg.ins.starts_with("copy:");
        // §6's "shortened to X s" names the CLIP's own length after the tape gives out.
        let shortened = kept_length(seg, doors.runs);

        // "clip N copies footage at T s that falls in no recording — skipped" and its past-the-end twin: session
        // seconds with no file behind them decode to nothing.
        if copied && !on_tape {
            logs.push(details::copy_in_no_recording_log(no, seg.s));
            continue;
        }
        if copied && shortened.is_some() {
            logs.push(details::copy_past_end_log(
                no,
                &base,
                shortened.unwrap_or(0.0),
            ));
        }
        // The sound-only halves: the picture exists, its recording does not, or stops short (their own §6 sentences).
        if !copied && seg.ins.is_empty() && !on_tape {
            logs.push(details::sound_in_no_recording_log(no, seg.s));
        }
        if !copied && seg.ins.is_empty() && shortened.is_some() {
            logs.push(details::sound_past_end_log(
                no,
                &base,
                shortened.unwrap_or(0.0),
            ));
        }
        // "clip N keeps the sound under it — it plays silent": an insert keeps what ran beneath it; with no tape
        // there is nothing to keep, or the scene silences every lane.
        if !seg.ins.is_empty()
            && !copied
            && crate::cut_hear::footage_sound_heard(seg)
            && (!on_tape || heard_lanes(doors.lanes, seg) == 0)
        {
            logs.push(details::insert_plays_silent_log(no));
        }
        // "a stop at T s falls in no recording — its still is skipped": a freeze needs one real frame.
        for stop in fx.iter().filter(|e| effect_overlap::is_a_stop(e)) {
            if !instant_on_tape(stop.t, doors.runs) {
                logs.push(details::stop_in_no_recording_log(stop.t));
            }
        }
        // "clip N at T s has no narration entry": a card or held frame has no session span, so its line is matched
        // by exact bounds (§6: "within 0.05 s", `details::bounds_match`).
        if !seg.ins.is_empty() && !entries.iter().any(|line| line_matches(line, seg)) {
            logs.push(details::no_entry_keeps_own_audio_log(no, seg.s));
        }
        // "clip N: no synthesis for a line — it is captioned only": the cue survives with the voice missing.
        if entries
            .iter()
            .filter(|line| line_matches(line, seg))
            .any(|line| !has_take(doors, line))
        {
            logs.push(details::captioned_only_log(no));
        }
        // "<clip>: the moving camera needs a fixed frame rate — this clip is N fps": asks what the filter graph's
        // `zoompan` grid reads (the camera sample list), since §6 does not name the field.
        if camera_moves(&fx, seg, on_screen) {
            logs.push(details::fixed_rate_log(
                &format!("clip {no}"),
                details::fixed_rate(),
            ));
        }
    }
    logs
}

/// How many of the scene's heard lanes a clip actually hears (the "or with no sound" half of §6's insert
/// sentence). One rule with `plan_clips`'s filter, whose copy in `produce_render` is private.
fn heard_lanes(lanes: &[crate::cut::Lane], seg: &Seg) -> usize {
    lanes
        .iter()
        .filter(|lane| {
            seg.hears(&lane.name)
                && crate::cut_hear::lane_covers(
                    lane,
                    crate::cut_hear::lane_seconds(lane, seg.s),
                    seg.s,
                )
        })
        .count()
}

/// Is this the clip's line? The half-overlap rule the fit uses for a line with a session span, and §6's
/// exact-bounds rule for a card or held frame (bounds are points, so they never satisfy "half of the shorter span").
fn line_matches(line: &Entry, seg: &Seg) -> bool {
    render::matches(line, seg) || details::bounds_match(line.s, line.e, seg.s, seg.e)
}

/// Does a line have a take to speak? An empty line was silenced on purpose (§4 keeps it as an entry), so it
/// is not §6's "no synthesis for a line".
fn has_take(doors: &Doors, line: &Entry) -> bool {
    if line.text.trim().is_empty() {
        return false;
    }
    (doors.wav_of)(line).is_some_and(|wav| wav.is_file())
}

/// Does a SECOND lie inside a filmed run? A run that CONTAINS the instant, not one that grazes its edge;
/// [`crate::produce_read_head::on_recording`] answers the span question and a zero-width span matches no run.
fn instant_on_tape(at: f64, runs: &[(f64, f64)]) -> bool {
    runs.iter().any(|(start, end)| *start <= at && at < *end)
}

/// Does this clip's camera move? `zoompan` samples a fixed grid, so the answer is the one the filter graph
/// uses: the clip carries a zoom whose rectangle differs from the settled frame. §6 does not name the field.
fn camera_moves(fx: &[crate::cut::Fx], _seg: &Seg, _on_screen: f64) -> bool {
    fx.iter()
        .filter(|e| e.effect_kind() == Some(crate::cut::EffectKind::Zoom))
        .any(|zoom| zoom.cx.is_some() || zoom.cy.is_some() || zoom.hf.is_some())
}
