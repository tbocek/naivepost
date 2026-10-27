//! §08-produce#6 bullet 1: the clip-planning refusals and log lines, said BY THE RENDER.
//!
//! §6 spells eight sentences about what a plan found on a particular clip — a paste that lands in no
//! recording, a clip that runs past the end of its base, an insert that plays silent, a stop whose still
//! cannot be taken, a clip nobody wrote a line for, a line nobody synthesised, a camera that needs a fixed
//! grid. Every one of those wordings lives in [`crate::produce_details`] (its "S1: clip planning refusals"
//! block), and until now nothing reached them: a person reading a render log could not tell an absent clip
//! from a clip the plan quietly refused.
//!
//! This module is that reach. It walks the SAME sped segment list `produce_exec::plan` hands
//! [`crate::produce_render::plan_clips`] — so "clip 7" here means clip 7 in every other line of the run —
//! and answers only the sentences §6 lists. `plan_clips` keeps its own two refusals (a missing insert, a clip
//! too short) and its fit logs, so nothing is said twice: the numbers here skip exactly the segments
//! `plan_clips` drops, and say nothing about the ones it already named.
//!
//! It is a separate file because the walk needs three lookups a caller must answer (is this second on tape,
//! how much tape is left, what does this clip read its picture from) and `plan_clips` already takes ten
//! arguments — a fourth door there would be a eleventh.

use crate::cut::Seg;
use crate::effect_overlap;
use crate::narration::Entry;
use crate::produce_details as details;
use crate::produce_read_head;
use crate::produce_render as render;
use crate::timeline::Recording;

/// What the walk needs beyond the segments: the doors only a caller with the session in hand can answer,
/// held in one struct so the walk's signature stays readable.
pub struct Doors<'a> {
    /// The project's lanes, asked per clip through `cut_hear` exactly as `plan_clips` asks them: a clip that
    /// hears no lane has no sound at all, which is §6's "with no sound — it plays silent".
    pub lanes: &'a [crate::cut::Lane],
    /// The filmed spans, so a SECOND can be placed on tape or not. `on_recording` in
    /// [`crate::produce_exec::Materials`] only answers "does this session have tape at all"; a `copy:`
    /// stretch and a stop need to know about THIS clip's seconds. Empty means no tape anywhere, which is
    /// what a session with no recordings hands over. Built by [`Doors::filmed`] from
    /// [`crate::timeline::filmed_runs`], the same answer the Cut page's debt tail reads.
    pub runs: &'a [(f64, f64)],
    /// The base each clip copies or reads from — the footage row's name for footage and a `copy:` stretch,
    /// the asset's own name for an insert. §6's two "past the end of <base>" sentences name it, so the
    /// caller answers with the same door that answers where the picture comes from.
    pub base_of: &'a dyn Fn(&Seg) -> String,
    /// Where a line's synthesis already is. A line with no wav is §6's "no synthesis for a line".
    pub wav_of: &'a dyn Fn(&Entry) -> Option<std::path::PathBuf>,
    /// Does an insert's asset still exist — the same door `plan_clips` answers its "is not there any more"
    /// line with, so a clip that already has a reason says it once. Asked with the cut's own spelling.
    pub exists: &'a dyn Fn(&str) -> bool,
}

impl<'a> Doors<'a> {
    /// The filmed spans a walk needs, from the recordings a snapshot gathered — [`crate::timeline::filmed_runs`],
    /// the same answer the Cut page's debt tail reads.
    pub fn filmed(recordings: &[Recording]) -> Vec<(f64, f64)> {
        crate::timeline::filmed_runs(recordings)
    }
}

/// A caller with no tape to speak of: no spans, so every clip of the run sits on no recording and each §6
/// sentence about a missing recording can be read in the log. A `&'static` empty so a caller can borrow it
/// straight into its doors without owning a vec; the program hands its snapshot's real spans instead.
pub const NO_TAPE: &[(f64, f64)] = &[];

/// The seconds the clip keeps when its tape gives out, or `None` when nothing is shortened: the filmed run the
/// clip starts in ends before the clip does, and what is left is that run's end less the clip's start. A clip
/// that starts in no run is not "shortened": the no-recording sentence above already refused it. The first
/// rewrite compared the clip's length with the tape left AFTER its end (F4.3's grow bound), so every clip
/// longer than that leftover was said to be shortened to it, on a session whose tape covered all of them.
fn kept_length(seg: &Seg, runs: &[(f64, f64)]) -> Option<f64> {
    let (_, end) = runs
        .iter()
        .find(|(start, end)| *start <= seg.s + 1e-6 && seg.s < *end)?;
    (seg.e > end + 1e-6).then(|| end - seg.s)
}

/// The planned clips' refusals and news, in clip-number order, in the wording §6 spells.
///
/// `segs` is the sped list (`crate::cut_speed_split::sped_cut`) — the same one `plan_clips` saw — and
/// `entries` the narration lines in playing order. A segment `plan_clips` drops (a missing insert, a clip
/// under §10's floor) contributes nothing here: its own line already names it.
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
        // The drops plan_clips already named. Skipping them here keeps one failure to one line, and the
        // clip's number still matches what the other line said.
        let missing_asset =
            !seg.ins.is_empty() && !seg.ins.starts_with("copy:") && !(doors.exists)(&seg.ins);
        if missing_asset || on_screen < crate::tools::cutpass::MIN_CLIP_SECONDS {
            continue;
        }
        let on_tape = produce_read_head::on_recording(seg, doors.runs);
        let base = (doors.base_of)(seg);
        let copied = seg.ins.starts_with("copy:");
        // What §6's "shortened to X s" names is the CLIP's own length after the tape gives out.
        let shortened = kept_length(seg, doors.runs);

        // "clip N copies footage at T s that falls in no recording — skipped" and its past-the-end twin:
        // a pasted stretch points at session seconds, and session seconds with no file behind them decode
        // to nothing.
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
        // The sound-only halves: the picture exists, its recording does not, or stops short. §6 lists these
        // as their own sentences, so a clip whose picture survived while its sound did not says so.
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
        // "clip N keeps the sound under it — it plays silent": an insert is the picture that keeps what was
        // running beneath it, and with no tape under it there is nothing to keep — or the scene silences
        // every lane, which is the same silence arrived at from the other side.
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
        // "clip N at T s has no narration entry — it keeps its own audio": a card or a held frame carries no
        // session span, so its line is matched by exact bounds (§6: "within 0.05 s"), which is
        // `details::bounds_match`'s question. Nothing matched means the clip talks in its own voice.
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
        // "<clip>: the moving camera needs a fixed frame rate — this clip is N fps". The clip's own camera
        // is what the encode samples for its `zoompan` grid; §6 does not name the field, and the one the
        // filter graph actually reads is the camera sample list, so this asks the same thing and reports
        // the grid it has to run at rather than a second opinion about what a camera is.
        if camera_moves(&fx, seg, on_screen) {
            logs.push(details::fixed_rate_log(
                &format!("clip {no}"),
                details::fixed_rate(),
            ));
        }
    }
    logs
}

/// How many of the scene's heard lanes a clip actually hears — the "or with no sound" half of §6's insert
/// sentence. A scene that silences every lane keeps nothing, however much tape is running. One rule with
/// `plan_clips`'s own filter, spelled here because `produce_render`'s copy is private and this question is
/// not the render's to lend.
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

/// Is this the clip's line? Both doors at once: the half-overlap rule the fit uses for a line with a session
/// span, and §6's exact-bounds rule for a card or held frame, whose bounds are points and so can never
/// satisfy "half of the shorter span".
fn line_matches(line: &Entry, seg: &Seg) -> bool {
    render::matches(line, seg) || details::bounds_match(line.s, line.e, seg.s, seg.e)
}

/// Does a line have a take to speak? An empty line is silence by choice and needs none, so it is not §6's
/// "no synthesis for a line" — §4 keeps a deliberately silent line as an entry precisely so it stays silent.
/// Does a line have a take to speak? A line whose text is empty was silenced on purpose (§4 keeps it as an
/// entry precisely so it stays silent), and a deliberately silent line is not §6's "no synthesis for a line".
fn has_take(doors: &Doors, line: &Entry) -> bool {
    if line.text.trim().is_empty() {
        return false;
    }
    (doors.wav_of)(line).is_some_and(|wav| wav.is_file())
}

/// Does a SECOND lie inside a filmed run? A run that CONTAINS the instant, not merely one that grazes its
/// edge — which is what "that frame exists somewhere on tape" means. [`crate::produce_read_head::on_recording`]
/// answers the span question (it overlaps), and a zero-width span would answer no for every run, so the stop
/// asks this instead.
fn instant_on_tape(at: f64, runs: &[(f64, f64)]) -> bool {
    runs.iter().any(|(start, end)| *start <= at && at < *end)
}

/// Does this clip's camera move? `zoompan` is what a moving camera renders with and `zoompan` samples a
/// fixed grid, so the answer is the one the filter graph uses: the clip carries a zoom whose rectangle
/// differs from the settled frame the camera rests on. §6 does not name the field; this asks the same thing
/// the encode's `zoompan` branch asks, off the same drawn-effect list, and reports the grid it must run at.
fn camera_moves(fx: &[crate::cut::Fx], _seg: &Seg, _on_screen: f64) -> bool {
    fx.iter()
        .filter(|e| e.effect_kind() == Some(crate::cut::EffectKind::Zoom))
        .any(|zoom| zoom.cx.is_some() || zoom.cy.is_some() || zoom.hf.is_some())
}
