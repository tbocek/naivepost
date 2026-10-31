//! F5.2 The render: the steps between ▶ on the Produce page and `produce/final.<container>` (`spec/08-produce.md`
//! §F5.2, wording in `spec/inventory/produce.md` §B). Nothing is executed here: each step's decision, log line and
//! ffmpeg argv come back as plain data (S1–S10), and the caller runs them in order ([`crate::produce_exec`]).
//! Ids cited: `P.eng.minClipSeconds` ([`crate::tools::cutpass::MIN_CLIP_SECONDS`]), `P.policy.gameVolume`,
//! `P.eng.laneMinMixSeconds`, `tool:ffmpeg.encode`.

use crate::cut::{Fx, Lane, Seg};
use crate::cut_hear;
use crate::narrate_preview;
use crate::narrate_screen;
use crate::narration::{self, Entry};
use crate::project::{self, Produce, Subtitles};
use crate::render_fx;
use crate::tools::cutpass::MIN_CLIP_SECONDS;
use crate::tools::mm_ss;

/// §F5.2 S1 / inventory §B step 0: where per-clip encodes, burned cues and `final.srt` live between a run's start and its join.
pub const SCRATCH: &str = "produce/clips/";

/// S1: empty the scratch folder's files, never the folder: a stale `c007_….mp4` would be concatenated by name, and the
/// render writes into it seconds later. A missing folder is `Ok(0)`, none created.
pub fn clear_scratch(dir: &std::path::Path) -> Result<usize, String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(err) => return Err(format!("{dir:?}: {err}")),
    };
    let mut gone = 0;
    for entry in entries {
        let path = entry.map_err(|err| format!("{dir:?}: {err}"))?.path();
        if path.is_file() {
            std::fs::remove_file(&path).map_err(|err| format!("{path:?}: {err}"))?;
            gone += 1;
        }
    }
    Ok(gone)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Speak {
    Nothing,
    Missing(Vec<String>),
}

/// S2: speak every line with no wav, unless the voice is captions only (`spec/07-narrate.md` §5 short-circuits job 1/2).
/// A blank line is a deliberate silence ([`crate::narrate_off::lines_to_speak`]). The list holds the **words**; which wav
/// each lands in is [`crate::narration::tts_key`]'s answer.
pub fn speak_job(
    voice_is_captions: bool,
    entries: &[Entry],
    wav_exists: impl Fn(&Entry) -> bool,
) -> Speak {
    if voice_is_captions {
        return Speak::Nothing;
    }
    let missing = entries
        .iter()
        .filter(|entry| !entry.text.trim().is_empty() && !wav_exists(entry))
        .map(|entry| entry.text.clone())
        .collect();
    Speak::Missing(missing)
}

pub fn speaking_job_line(done: usize, total: usize) -> String {
    format!("speaking {done}/{total}")
}

pub const SPEAKING_FRACTION: f64 = 0.5;

/// S2 (`"synthesis for Ns: …" on failure`): the seconds already spent tell a stalled server from a refusal.
pub fn synthesis_failure_log(seconds: f64, reason: &str) -> String {
    format!("synthesis for {seconds:.1}s: {reason}")
}

/// S2's branch: captions only with Subtitles "none in the video" leaves the words only in the .srt beside it, and the log says so.
pub fn captions_only_warning(subtitles: Subtitles) -> Option<&'static str> {
    match subtitles {
        Subtitles::None => {
            Some(">>> captions only and nothing in the video — the lines are in the .srt beside it")
        }
        _ => None,
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Clip {
    pub no: usize,
    pub seg_s: f64,
    pub on_screen: f64,
    pub rate: f64,
    pub source: String,
    pub frame: (i32, i32),
    pub lanes: Vec<String>,
    pub lines: Vec<Placed>,
    pub cues: usize,
    pub camera_depth: f64,
    /// The effects this segment carries, read through the same `fx_for` door as the cues; every §4 filter is built from them.
    pub fx: Vec<Fx>,
    /// §8 bullet 4: the planned 1× read head in the clip's own seconds, `None` when it does not open (no drift, a card, no tape).
    pub read_head: Option<(f64, f64)>,
    pub on_recording: bool,
    pub src: (i32, i32),
    pub scratch: std::path::PathBuf,
    pub lane_files: Vec<LaneFile>,
    pub ss: f64,
    pub sound_input: usize,
    pub voice_files: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LaneFile {
    pub name: String,
    pub path: String,
    pub seek: f64,
    pub delay: f64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Placed {
    pub entry_index: usize,
    pub at: f64,
    pub tempo: f64,
    pub extend: f64,
    pub text: String,
    pub speech: f64,
}

/// S3 (`overlap ≥ half the shorter span`): not "any overlap", which would move a two-second line grazing a long clip's last frame out of its clip.
pub fn matches(entry: &Entry, seg: &Seg) -> bool {
    let line = entry.e - entry.s;
    let clip = seg.e - seg.s;
    if line <= 0.0 || clip <= 0.0 {
        return false;
    }
    let overlap = entry.e.min(seg.e) - entry.s.max(seg.s);
    overlap >= line.min(clip) / 2.0
}

fn holding_clip(entry: &Entry, segs: &[Seg]) -> Option<usize> {
    let second = entry.s + entry.at;
    let matching = || {
        segs.iter()
            .enumerate()
            .filter(|(_, seg)| matches(entry, seg))
    };
    matching()
        .find(|(_, seg)| seg.s <= second && second < seg.e)
        .or_else(|| matching().next())
        .map(|(index, _)| index)
}

/// For a caller with no footage in hand: `0.0` lets [`fit_in`] keep §10's ceiling; not knowing is not having no tape left.
pub fn unknown_footage(_: &Seg) -> f64 {
    0.0
}

/// S3: one clip per segment, in order, over the list AFTER the speed splits ([`crate::cut_speed`]). `exists` says whether an
/// insert's file is there ([`Seg::insert_asset`]); `entries` are matched by [matches](fn@matches) and fitted by [`fit`];
/// `lanes` are asked per clip through [`cut_hear`].
#[allow(clippy::too_many_arguments)] // the plan reads the session through these doors by design
pub fn plan_clips(
    segs: &[Seg],
    fx_for: impl Fn(&Seg) -> Vec<Fx>,
    aspect: &str,
    exists: impl Fn(&str) -> bool,
    lanes: &[Lane],
    entries: &[Entry],
    src_shape: (i32, i32),
    on_recording: bool,
    footage_left: impl Fn(&Seg) -> f64,
) -> (Vec<Clip>, Vec<String>) {
    // §9 (`both MUST use the first footage clip's frame`): `frame_box` returns `src_shape` unchanged under aspect "source"
    // ([`crate::ui::cut_picture`] is the agreeing preview side).
    let frame = render_fx::frame_box(aspect, src_shape.0, src_shape.1);
    let mut clips: Vec<Clip> = Vec::new();
    let mut logs: Vec<String> = Vec::new();
    for (index, seg) in segs.iter().enumerate() {
        let no = index + 1;
        if !seg.ins.is_empty() && !seg.ins.starts_with("copy:") && !exists(&seg.ins) {
            logs.push(missing_insert_log(no, &seg.ins));
            continue;
        }
        // A rate of nought is a stop: footage runs on at 1× and the still rides over it (§4).
        let fx = crate::effect_overlap::drawn_on_scene(&fx_for(seg), seg);
        let rate = if seg.rate > 0.0 { seg.rate } else { 1.0 };
        let on_screen = crate::narrate_pass::on_screen(seg);
        if on_screen < MIN_CLIP_SECONDS {
            logs.push(too_short_log(
                no,
                seg.s,
                on_screen,
                lost_lines(entries, seg),
            ));
            continue;
        }
        let heard: Vec<String> = lanes
            .iter()
            .filter(|lane| lane_heard(lane, seg))
            .map(|lane| lane.name.clone())
            .collect();
        // §9's reach rule: a lane-named cue acts only where that lane is heard; a whole-bed cue reaches whatever the scene hears,
        // INCLUDING nothing (a card's own sound), which a "is any lane heard" test dropped.
        let cues = render_fx::all_gain_cues(&fx, seg.s, rate, on_screen)
            .into_iter()
            .filter(|cue| cue.reaches_the_clip(&heard))
            .count();
        let mine: Vec<(usize, f64, f64)> = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| holding_clip(entry, segs) == Some(index))
            .map(|(entry_index, entry)| {
                let into_clip = (entry.s + entry.at - seg.s).max(0.0);
                (entry_index, narration::output_seconds(into_clip, rate), 0.0)
            })
            .collect();
        // F4.3: the CLIP number names the log, footage past the clip's end caps the grow, and what the ladder did is logged.
        let (lines, fit_logs) = fit_in(&mine, on_screen, no, footage_left(seg));
        logs.extend(fit_logs);
        // §8 bullet 4: planned here. The debt is the footage's seconds less what the picture shows (as the Cut page's debt tail).
        let head = read_head_plan(seg, &fx).filter(|_| {
            crate::produce_read_head::opens(
                seg.e - seg.s,
                on_screen,
                crate::produce_read_head::source_is_footage(seg),
                on_recording,
            )
        });
        clips.push(Clip {
            no,
            seg_s: seg.s,
            ss: seg.ss,
            sound_input: 0,
            on_screen,
            rate,
            source: seg.ins.clone(),
            frame,
            lanes: heard,
            lines,
            cues,
            camera_depth: 1.0,
            fx,
            read_head: head,
            on_recording: false,
            src: src_shape,
            scratch: std::path::PathBuf::new(),
            lane_files: Vec::new(),
            voice_files: Vec::new(),
        });
    }
    (clips, logs)
}

/// The lines on a clip being dropped, counted for S3's "lost narration named". Every touching line counts, since a clip
/// under [`MIN_CLIP_SECONDS`] has no room for any.
fn lost_lines(entries: &[Entry], seg: &Seg) -> usize {
    entries
        .iter()
        .filter(|entry| entry.e > seg.s && entry.s < seg.e)
        .count()
}

/// S3 (`clip N: <file> is not there any more — skipped`); also covers a sound insert with no picture.
pub fn missing_insert_log(no: usize, file: &str) -> String {
    format!("clip {no}: {file} is not there any more \u{2014} skipped")
}

/// S3 (`clips under P.eng.minClipSeconds dropped, log line naming any narration lost`).
pub fn too_short_log(no: usize, at: f64, length: f64, lost_lines: usize) -> String {
    let mut log = format!(
        "clip {no} at {} is {length:.1} s \u{2014} too short to render, dropped",
        mm_ss(at)
    );
    if lost_lines > 0 {
        log.push_str(" \u{2014} the narration line(s) written on it are dropped with it");
    }
    log
}

pub fn no_clip_log() -> &'static str {
    "no clip could be placed on a recording"
}

/// P.eng.narrationLeadSeconds (§10: 0.3). Deliberately **not** in [`crate::params`]: `tests/suite/narrate_parameters.rs`'s s6
/// and `tests/suite/narrate_screen_widgets.rs` assert its absence until the row moves with the rule that reads it.
pub const LEAD_SECONDS: f64 = 0.3;

/// P.eng.narrationGapSeconds (§10: 0.3): the shortest silence between two lines of one clip.
pub const GAP_SECONDS: f64 = 0.3;

/// P.eng.narrationTailSeconds (§10: 0.2): the breath after a line; one home, [`narrate_screen::SPEECH_TAIL_SECONDS`].
pub const TAIL_SECONDS: f64 = narrate_screen::SPEECH_TAIL_SECONDS;

/// P.eng.narrationMaxExtendSeconds (§10: 4): same value as the preview's hold bound, read from there.
pub const MAX_EXTEND_SECONDS: f64 = narrate_preview::MAX_EXTEND_SECONDS;

/// P.eng.narrationMaxTempo (§10: 1.25): past this a line is placed as it is and runs over. Not in [`crate::params`], as [`LEAD_SECONDS`].
pub const MAX_TEMPO: f64 = 1.25;

/// S3 / F4.3: place a clip's lines in its room, in order: pack from where each was placed (previous end plus
/// [`GAP_SECONDS`] and the tail); still over, grow the clip by up to [`MAX_EXTEND_SECONDS`]; still over, slide the run earlier
/// to the lead; still over, speed every line up to [`MAX_TEMPO`] and pack again. Only then is it logged.
/// `lines` are `(entry_index, at, speech)`; a line with `speech` 0.0 takes no room, so a missing wav never moves later lines.
pub fn fit(lines: &[(usize, f64, f64)], room: f64) -> (Vec<Placed>, Vec<String>) {
    // The page's view: no footage in hand, so only §10's ceiling caps the grow, and the log names the entry.
    fit_in(lines, room, entry_no(lines), MAX_EXTEND_SECONDS)
}

/// The same ladder with the two facts only a caller knows: which clip, and the footage left past its end (§F4.3 step 1:
/// "never past the footage"). The grow is capped by `min(MAX_EXTEND_SECONDS, footage_left)`; 0 means unknown.
pub fn fit_in(
    lines: &[(usize, f64, f64)],
    room: f64,
    clip_no: usize,
    footage_left: f64,
) -> (Vec<Placed>, Vec<String>) {
    let mut logs: Vec<String> = Vec::new();
    if lines.is_empty() {
        return (Vec::new(), logs);
    }
    let no = clip_no;
    let grow = if footage_left > 0.0 {
        MAX_EXTEND_SECONDS.min(footage_left)
    } else {
        MAX_EXTEND_SECONDS
    };

    let placed = pack(lines, 1.0, 0.0);
    if fits(&placed, room) {
        return (placed, logs);
    }

    // 1 · grow the clip, silently; only stopping at the ceiling is worth a mention.
    let extend = (run_end(&placed) + TAIL_SECONDS - room).clamp(0.0, grow);
    if fits_at(&placed, room, extend) {
        return (with_extend(placed, extend), logs);
    }

    let room = room + extend;
    let slide = (run_end(&placed) + TAIL_SECONDS - room).clamp(0.0, slide_of(lines));
    let slid = pack(lines, 1.0, -slide);
    if fits_at(&slid, room, 0.0) {
        logs.push(moved_log(no, slide));
        return (with_extend(slid, extend), logs);
    }

    let tempo = needed_tempo(lines, room).min(MAX_TEMPO);
    let final_slide =
        (run_end(&pack(lines, tempo, 0.0)) + TAIL_SECONDS - room).clamp(0.0, slide_of(lines));
    let sped = pack(lines, tempo, -final_slide);
    if (tempo - 1.0).abs() > f64::EPSILON {
        logs.push(moved_log(no, final_slide));
        logs.push(sped_up_log(no, run_seconds(lines), room, tempo));
    }
    (with_extend(sped, extend), logs)
}

fn pack(lines: &[(usize, f64, f64)], tempo: f64, shift: f64) -> Vec<Placed> {
    let mut placed: Vec<Placed> = Vec::new();
    let mut previous_end: Option<f64> = None;
    for (index, at, speech) in lines {
        let wanted = (*at + shift).max(LEAD_SECONDS);
        let start = match previous_end {
            Some(end) => wanted.max(end + GAP_SECONDS + TAIL_SECONDS),
            None => wanted,
        };
        placed.push(Placed {
            entry_index: *index,
            at: start,
            tempo,
            extend: 0.0,
            text: String::new(),
            speech: *speech,
        });
        previous_end = Some(start + speech / tempo.max(1.0));
    }
    placed
}

/// The pack rung alone, for [`crate::narrate_screen::mirror_fit`], which tells a run tight against the clip's end from one
/// pushed off its placement by the previous gap (§1 spells those warnings differently).
pub fn packed(lines: &[(usize, f64, f64)]) -> Vec<Placed> {
    pack(lines, 1.0, 0.0)
}

pub fn run_end(placed: &[Placed]) -> f64 {
    placed
        .iter()
        .map(|line| line.at + line.speech / line.tempo.max(1.0))
        .fold(0.0_f64, f64::max)
}

fn fits(placed: &[Placed], room: f64) -> bool {
    fits_at(placed, room, 0.0)
}

fn fits_at(placed: &[Placed], room: f64, extend: f64) -> bool {
    run_end(placed) + TAIL_SECONDS <= room + extend + 1e-9
}

fn with_extend(mut placed: Vec<Placed>, extend: f64) -> Vec<Placed> {
    for line in placed.iter_mut() {
        line.extend = extend;
    }
    placed
}

fn run_seconds(lines: &[(usize, f64, f64)]) -> f64 {
    run_end(&pack(lines, 1.0, 0.0))
}

fn slide_of(lines: &[(usize, f64, f64)]) -> f64 {
    lines
        .iter()
        .map(|(_, at, _)| *at)
        .fold(f64::MAX, f64::min)
        .max(LEAD_SECONDS)
        - LEAD_SECONDS
}

fn needed_tempo(lines: &[(usize, f64, f64)], room: f64) -> f64 {
    let speaking: f64 = lines.iter().map(|(_, _, speech)| speech).sum::<f64>();
    let gaps = GAP_SECONDS + TAIL_SECONDS;
    let room = (room - LEAD_SECONDS - gaps * (lines.len().saturating_sub(1) as f64)).max(0.1);
    let tempo = speaking / room;
    if tempo <= 1.0 {
        1.0
    } else {
        tempo
    }
}

fn entry_no(lines: &[(usize, f64, f64)]) -> usize {
    lines.first().map(|(index, _, _)| *index + 1).unwrap_or(1)
}

pub fn moved_log(no: usize, seconds: f64) -> String {
    format!("clip {no}: the narration does not fit where it was placed \u{2014} moved {seconds:.1} s earlier")
}

pub fn sped_up_log(no: usize, speech: f64, room: f64, tempo: f64) -> String {
    format!(
        "clip {no}: narration {speech:.1} s does not fit {room:.1} s \u{2014} sped up {tempo:.2}x"
    )
}

/// P.eng.laneMinMixSeconds (§10: 0.1, inventory §B `overlap ≥ 0.1 s`): the shortest overlap worth an ffmpeg input.
pub const LANE_MIN_MIX_SECONDS: f64 = 0.1;

pub fn lane_report(lane: &str, mixed: usize, total: usize) -> String {
    format!("{lane} is mixed into {mixed} of the {total} clips")
}

/// The lane runs but every clip it underlies leaves it out (a `quiet` list); returns the tail of §B's sentence, which starts with the lane's name.
pub fn lane_left_out(lane: &str) -> String {
    format!("{lane} runs under 0 clip(s) and every one of them leaves it out \u{2014} it is not in the render")
}

pub fn lane_not_running(lane: &str) -> String {
    format!("{lane} was not running while any clip was \u{2014} it is not in the render")
}

fn lane_heard(lane: &Lane, seg: &Seg) -> bool {
    seg.hears(&lane.name) && cut_hear::lane_covers(lane, cut_hear::lane_seconds(lane, seg.s), seg.s)
}

/// S3 (`sound plan for own-clock runs`): a speed effect whose sound stayed at 1× is read on its own clock while the picture
/// runs ahead; [`render_fx`] puts the 0.15 s dips ([`crate::fx_lane::SOUND_DIP_SECONDS`]) at its ends. Pitched or muted sound has no run.
pub fn read_head_plan(seg: &Seg, fx: &[Fx]) -> Option<(f64, f64)> {
    let own = fx.iter().any(|effect| {
        effect.effect_kind() == Some(crate::cut::EffectKind::Speed)
            && matches!(effect.snd.as_str(), "own" | "scene")
    });
    own.then_some((seg.s, seg.e))
}

/// S4 (`subtitles built on the produced clock`): where each clip begins in the finished file, and its on-screen length;
/// [`narration::output_seconds`] converts, as the narration's own placement does.
pub fn produced_clocks(clips: &[Clip]) -> Vec<(f64, f64)> {
    let mut at = 0.0;
    let mut clocks = Vec::with_capacity(clips.len());
    for clip in clips {
        clocks.push((at, clip.on_screen));
        at += clip.on_screen;
    }
    clocks
}

pub fn final_srt_name() -> &'static str {
    "final.srt"
}

/// S4 (`stale sidecars deleted by exact name first`): by name, never a glob, which would delete a translation of something else in the folder.
pub fn stale_sidecars(stem: &str, languages: &[&str]) -> Vec<String> {
    let mut names = vec![format!("{stem}.srt"), format!("{stem}.vtt")];
    for code in languages {
        names.push(format!("{stem}.{code}.srt"));
        names.push(format!("{stem}.{code}.vtt"));
    }
    names
}

/// S5's "limiter −1 dBFS" (`P.eng.clipLimiter`): the amplitude `alimiter` accepts; the catalogue reads it off this string.
pub const LIMITER: &str = "alimiter=limit=0.891:level=disabled";

/// §4's frame-edge blur: the backdrop's smear as a fraction of the frame's height (prototype `blurSigma`, go/produce.go:1310).
/// §10 files the pair as `P.eng.blurSigma` ("0.02·height, min 4").
pub const BLUR_SIGMA_FRACTION: f64 = 0.02;

/// The floor on the derived sigma: below 200 px of frame height the smear stops hiding the edge seam.
pub const BLUR_SIGMA_MIN: f64 = 4.0;

/// The sigma `gblur` is asked for: [`BLUR_SIGMA_FRACTION`] of the height, floored at [`BLUR_SIGMA_MIN`], ceiled to a whole pixel.
pub fn blur_sigma(frame_height: i32) -> i32 {
    (f64::from(frame_height) * BLUR_SIGMA_FRACTION)
        .max(BLUR_SIGMA_MIN)
        .ceil() as i32
}

/// S5 / inventory §B (`stems c%03d_<stamp>`): the stamp is in the name because the concat list is rebuilt from the
/// scratch folder and a leftover `c000.mp4` of an older cut would join in by name.
pub fn clip_stem(no: usize, stamp: &str) -> String {
    format!("c{:03}_{}", no, stamp)
}

/// S5's `-fps_mode` piece. `source` adds nothing. With frame timing off the `fps` filter set the grid, so the output is
/// constant; with it on the rate is a ceiling (§A "peak rate (VFR)"), `-fpsmax` ALONE: ffmpeg rejects it beside `-fps_mode vfr`.
pub fn frame_rate_args(frame_rate: &str, vfr: bool) -> Vec<String> {
    if frame_rate == "source" {
        return Vec::new();
    }
    if vfr {
        return vec!["-fpsmax".into(), frame_rate.into()];
    }
    vec!["-fps_mode".into(), "cfr".into()]
}

pub fn preset_name(preset: project::Preset) -> &'static str {
    use crate::project::Preset::*;
    match preset {
        Ultrafast => "ultrafast",
        Veryfast => "veryfast",
        Fast => "fast",
        Medium => "medium",
        Slow => "slow",
        VerySlow => "veryslow",
    }
}

/// The Frame rate row's spelling: `source` for the recording's own rate. Separate from the enum's serde names, which are file bytes.
pub fn frame_rate_name(frame_rate: project::FrameRate) -> &'static str {
    use crate::project::FrameRate::*;
    match frame_rate {
        Source => "source",
        F60 => "60",
        F30 => "30",
        F24 => "24",
    }
}

/// S5's video chain in inventory §B's order, which **is** the behaviour: speed first so later filters work in on-screen
/// seconds, stop freezes, frame rate, edges, camera, then things printed on top. Burned subtitles go before the text
/// overlays (§4) so a title sits above a caption.
pub fn video_chain(clip: &Clip, settings: &Produce, burned: Option<&str>) -> Vec<String> {
    // §4: camera under the burned cues, printed things after them (`picture_order()`); `produce_fx_chain` builds both halves.
    let built = crate::produce_fx_chain::for_clip(clip, &clip.fx, clip.src, &clip.scratch);
    let mut chain = Vec::new();
    if (clip.rate - 1.0).abs() > f64::EPSILON {
        if let Some(filter) = render_fx::setpts(clip.rate) {
            chain.push(filter);
        }
    }
    chain.extend(frame_rate_filter(
        frame_rate_name(settings.frame_rate),
        settings.vfr,
    ));
    if settings.blurred_edges {
        let (wide, high) = clip.frame;
        chain.push("split=2[bdb][bdf]".into());
        chain.push(format!(
            "[bdb]scale={wide}:{high}:force_original_aspect_ratio=increase,crop={wide}:{high},gblur=sigma={},setsar=1[bdb]",
            blur_sigma(high)
        ));
        chain.push(format!(
            "[bdf]scale={wide}:{high}:force_original_aspect_ratio=decrease[bdf]"
        ));
        chain.push("[bdb][bdf]overlay=(W-w)/2:(H-h)/2".into());
    }
    chain.push(format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:black,setsar=1",
        clip.frame.0, clip.frame.1, clip.frame.0, clip.frame.1
    ));
    if let Some(cues) = burned {
        chain.push(format!("subtitles={cues}"));
    }
    chain.extend(built.video);
    chain
}
/// The frame rate as a filter, the in-graph half of [`frame_rate_args`]: only for a constant rate; under VFR the rate is the
/// encoder's ceiling, and a moving camera brings its own grid (`zoompan`'s `fps`).
fn frame_rate_filter(frame_rate: &str, vfr: bool) -> Vec<String> {
    if frame_rate == "source" || vfr {
        return Vec::new();
    }
    vec![format!("fps=fps={frame_rate}")]
}

/// S5's audio chain in §B/S5's order: speed → hush → lane bed → gain cues → seam dips → narration at `P.policy.gameVolume` →
/// limiter → format. Each `amix` is a mix point and exists only when something joins; [`audio_graph`] wires the same steps
/// with labels, and [`lane_chain`] and [`voice_chain`] cover what happens before an input joins.
pub fn audio_chain(
    clip: &Clip,
    settings: &Produce,
    lanes: usize,
    hush: &[(f64, f64)],
) -> Vec<String> {
    let parts = audio_parts(clip, settings, lanes, spoken_lines(clip).len(), hush);
    let mut chain = parts.head;
    chain.extend(parts.lane_mix);
    chain.extend(parts.middle);
    if let Some((volume, mix)) = parts.voice_mix {
        chain.push(volume);
        chain.push(mix);
    }
    chain.extend(parts.tail);
    chain
}

fn spoken_lines(clip: &Clip) -> Vec<&Placed> {
    clip.lines.iter().filter(|line| line.speech > 0.0).collect()
}

struct AudioParts {
    head: Vec<String>,
    lane_mix: Option<String>,
    middle: Vec<String>,
    voice_mix: Option<(String, String)>,
    tail: Vec<String>,
}

fn audio_parts(
    clip: &Clip,
    settings: &Produce,
    lanes: usize,
    voices: usize,
    hush: &[(f64, f64)],
) -> AudioParts {
    let mut head = Vec::new();
    if (clip.rate - 1.0).abs() > f64::EPSILON && clip.read_head.is_none() {
        head.push(render_fx::atempo_chain(clip.rate));
    }
    for (from, to) in hush {
        head.push(format!(
            "volume=0:eval=frame:enable='between(t,{from:.3},{to:.3})'"
        ));
    }
    let lane_mix = (lanes > 0).then(|| mix(lanes + 1));
    // §4's volume cues: one `volume=<expr>:eval=frame` per cue, chained so overlapping ones multiply, after the bed and before the narration.
    // A clip with no seam has no dip: `afade=t=out:st=0` on such clips silenced them after 0.15 s.
    let built = crate::produce_fx_chain::for_clip(clip, &clip.fx, clip.src, &clip.scratch);
    let middle: Vec<String> = built.audio.to_vec();
    // P.policy.gameVolume is the bed's level, not the voice's: the narration rides at full level over a bed turned down to it.
    let voice_mix = (voices > 0).then(|| {
        (
            format!("volume={:.2}", settings.game_volume),
            mix(voices + 1),
        )
    });
    let layout = if settings.mono { "mono" } else { "stereo" };
    let tail = vec![
        LIMITER.to_string(),
        format!("aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts={layout}"),
    ];
    AudioParts {
        head,
        lane_mix,
        middle,
        voice_mix,
        tail,
    }
}

fn mix(inputs: usize) -> String {
    format!("amix=inputs={inputs}:duration=first:normalize=0")
}

/// What a spoken line's take goes through before joining the bed: tempo (atempo takes no less than 0.5), 48 kHz, panned across
/// the output layout, and its delay in whole milliseconds once per channel.
pub fn voice_chain(line: &Placed, settings: &Produce) -> Vec<String> {
    let ms = (line.at * 1000.0).round() as i64;
    vec![
        format!("atempo={:.3}", line.tempo.max(0.5)),
        "aresample=48000".to_string(),
        if settings.mono {
            "pan=mono|c0=c0".to_string()
        } else {
            "pan=stereo|c0=c0|c1=c0".to_string()
        },
        format!("adelay={ms}ms|{ms}ms"),
    ]
}

pub fn lane_chain(lane: &LaneFile, clip: &Clip, settings: &Produce) -> Vec<String> {
    let layout = if settings.mono { "mono" } else { "stereo" };
    let mut chain = vec![format!(
        "aformat=sample_fmts=fltp:sample_rates=48000:channel_layouts={layout}"
    )];
    if (clip.rate - 1.0).abs() > f64::EPSILON {
        chain.push(render_fx::atempo_chain(clip.rate));
    }
    let ms = (lane.delay * 1000.0).round() as i64;
    if ms > 0 {
        chain.push(format!("adelay={ms}ms|{ms}ms"));
    }
    chain
}

/// The clip's sound as `-filter_complex` wires it, ending in `[a]`: lanes join the bed at inputs `lane_base + k` and spoken lines the narration mix at `voice_base + k`.
pub fn audio_graph(clip: &Clip, settings: &Produce, lane_base: usize, voice_base: usize) -> String {
    let voices: Vec<&Placed> = spoken_lines(clip)
        .into_iter()
        .take(clip.voice_files.len())
        .collect();
    let parts = audio_parts(clip, settings, clip.lane_files.len(), voices.len(), &[]);
    let mut graph = String::new();
    let mut current = format!("{}:a", clip.sound_input);
    let step = |graph: &mut String, current: &mut String, filters: &[String], to: &str| {
        if !filters.is_empty() {
            graph.push_str(&format!("[{current}]{}[{to}];", filters.join(",")));
            *current = to.to_string();
        }
    };
    let built_gains =
        crate::produce_fx_chain::for_clip(clip, &clip.fx, clip.src, &clip.scratch).lane_gains;
    step(&mut graph, &mut current, &parts.head, "own");
    if let Some(mix) = &parts.lane_mix {
        let mut joined = String::new();
        for (k, lane) in clip.lane_files.iter().enumerate() {
            graph.push_str(&format!(
                "[{}:a]{}[ln{k}];",
                lane_base + k,
                lane_chain(lane, clip, settings)
                    .into_iter()
                    .chain(
                        built_gains
                            .iter()
                            .filter(|(name, _)| *name == lane.name)
                            .map(|(_, filter)| filter.clone()),
                    )
                    .collect::<Vec<_>>()
                    .join(",")
            ));
            joined.push_str(&format!("[ln{k}]"));
        }
        graph.push_str(&format!("[{current}]{joined}{mix}[bed];"));
        current = "bed".to_string();
    }
    step(&mut graph, &mut current, &parts.middle, "mid");
    let tail = parts.tail.join(",");
    match &parts.voice_mix {
        Some((volume, mix)) => {
            let mut joined = String::new();
            for (k, line) in voices.iter().enumerate() {
                graph.push_str(&format!(
                    "[{}:a]{}[nr{k}];",
                    voice_base + k,
                    voice_chain(line, settings).join(",")
                ));
                joined.push_str(&format!("[nr{k}]"));
            }
            graph.push_str(&format!(
                "[{current}]{volume}[bg];[bg]{joined}{mix},{tail}[a]"
            ));
        }
        None => graph.push_str(&format!("[{current}]{tail}[a]")),
    }
    graph
}

/// S5's codec arguments (`tool:ffmpeg.encode`, ffmpeg's own names and flags). h264 carries `-refs 4` as the prototype shipped;
/// vp9 takes `-b:v 0` and `-cpu-used` from the same preset row.
pub fn codec_args(settings: &Produce) -> Vec<String> {
    let preset = preset_name(settings.preset).to_string();
    match settings.codec {
        project::Codec::H264 => vec![
            "-c:v".into(),
            "libx264".into(),
            "-preset".into(),
            preset,
            "-crf".into(),
            settings.crf.to_string(),
            "-pix_fmt".into(),
            "yuv420p".into(),
            "-refs".into(),
            "4".into(),
        ],
        project::Codec::H265 => vec![
            "-c:v".into(),
            "libx265".into(),
            "-tag:v".into(),
            "hvc1".into(),
            "-preset".into(),
            preset,
            "-crf".into(),
            settings.crf.to_string(),
            "-pix_fmt".into(),
            "yuv420p".into(),
        ],
        project::Codec::Vp9 => vec![
            "-c:v".into(),
            "libvpx-vp9".into(),
            "-crf".into(),
            settings.crf.to_string(),
            "-b:v".into(),
            "0".into(),
            "-row-mt".into(),
            "1".into(),
            "-cpu-used".into(),
            cpu_used(&preset).to_string(),
        ],
    }
}

/// vp9 has no preset ladder: the one setting is walked to `-cpu-used` (0 = slowest and smallest, 5 = fastest).
fn cpu_used(preset: &str) -> u32 {
    match preset {
        "ultrafast" => 5,
        "veryfast" => 4,
        "fast" => 3,
        "medium" => 2,
        "slow" => 1,
        _ => 0,
    }
}

pub fn audio_codec(container: project::Container) -> &'static str {
    match container {
        project::Container::Webm => "libopus",
        _ => "aac",
    }
}

/// §5 (`-t` on the output for inserts, sounds, freezes, rated clips): how long this clip's file runs when its input's own clock
/// does not answer that. A **freeze** arrives as `rate == 0.0` with [`Clip::on_screen`] holding the still's length
/// ([`crate::narrate_pass::on_screen`]), so the rate arm trims it. Plain footage at 1× is `None`: a `-t` from session seconds would trim a frame.
pub fn output_limit(clip: &Clip) -> Option<String> {
    let rated = (clip.rate - 1.0).abs() > f64::EPSILON;
    (!clip.source.is_empty() || rated).then(|| format!("{:.3}", clip.on_screen))
}

pub fn encode_command(
    clip: &Clip,
    settings: &Produce,
    inputs: &[(&str, Option<&str>)],
    burned: Option<&str>,
    out: &std::path::Path,
) -> Vec<String> {
    encode_command_with(clip, settings, inputs, burned, out, false, clip.seg_s)
}

/// [`encode_command`] plus whether the clip reads a baked card with no own length; its frames loop (`-stream_loop -1`) and `-t` trims to the cut's seconds.
pub fn encode_command_with(
    clip: &Clip,
    settings: &Produce,
    inputs: &[(&str, Option<&str>)],
    burned: Option<&str>,
    out: &std::path::Path,
    loops_card: bool,
    seek: f64,
) -> Vec<String> {
    #![allow(clippy::too_many_arguments)]
    let mut argv = vec!["ffmpeg".into()];
    // The picture's own seek is always before its -i (inventory §B); `seek` is the second inside the FILE, i.e. the session second less where its recording starts.
    if seek > 0.0 {
        argv.push(SEEK_FLAG.into());
        argv.push(format!("{seek:.3}"));
    }
    let footage = clip.source.is_empty() || clip.source.starts_with("copy:");
    if footage && clip.rate > 0.0 {
        argv.push("-t".into());
        argv.push(format!("{:.3}", clip.on_screen * clip.rate));
    }
    let mut filter_inputs = 0;
    if loops_card {
        argv.extend(["-stream_loop".into(), "-1".into()]);
    }
    for (kind, path) in inputs {
        match (*kind, path) {
            ("sound", None) => {
                argv.extend([
                    "-f".into(),
                    "lavfi".into(),
                    "-i".into(),
                    "anullsrc=r=48000:cl=stereo".into(),
                ]);
            }
            (_, Some(path)) => {
                if *kind == "still" {
                    let moment: f64 = path.parse().unwrap_or(clip.seg_s);
                    argv.extend([
                        SEEK_FLAG.into(),
                        format!("{:.3}", (seek + moment - clip.seg_s).max(0.0)),
                        "-i".into(),
                        inputs
                            .first()
                            .and_then(|(_, p)| *p)
                            .unwrap_or_default()
                            .into(),
                    ]);
                    filter_inputs += 1;
                    continue;
                }
                if *kind == "soundfile" && clip.ss > 0.0 {
                    argv.extend([SEEK_FLAG.into(), format!("{:.3}", clip.ss)]);
                }
                if matches!(*kind, "text" | "svg" | "image") {
                    argv.extend([
                        "-loop".into(),
                        "1".into(),
                        "-framerate".into(),
                        render_fx::OVERLAY_FPS.to_string(),
                    ]);
                }
                // §5: a baked card is an image2 sequence at the bake's frame rate, which `produce_cards` owns and spells as an integer.
                if *kind == "card" {
                    argv.extend(["-framerate".into(), crate::produce_cards::bake_fps_arg()]);
                }
                argv.push("-i".into());
                argv.push(path.to_string());
            }
            (_, None) => {}
        }
        filter_inputs += 1;
    }
    // Lanes then spoken lines go after everything the picture reads, so indices only append (§5).
    let lane_base = argv.iter().filter(|arg| *arg == "-i").count();
    for lane in &clip.lane_files {
        argv.extend([
            SEEK_FLAG.into(),
            format!("{:.3}", lane.seek.max(0.0)),
            "-i".into(),
            lane.path.clone(),
        ]);
    }
    let voice_base = lane_base + clip.lane_files.len();
    for take in &clip.voice_files {
        argv.extend(["-i".into(), take.clone()]);
    }
    let video = video_graph(clip, settings, burned, lane_base);
    let audio = audio_graph(clip, settings, lane_base, voice_base);
    argv.extend([
        "-filter_complex".into(),
        format!("{video};{audio}"),
        // The graph's labelled outputs ARE the clip: ffmpeg maps only unlabelled outputs by itself.
        "-map".into(),
        "[v]".into(),
        "-map".into(),
        "[a]".into(),
    ]);
    argv.extend(codec_args(settings));
    argv.extend(frame_rate_args(
        frame_rate_name(settings.frame_rate),
        settings.vfr,
    ));
    argv.extend([
        "-c:a".into(),
        audio_codec(settings.container).into(),
        "-b:a".into(),
        format!("{}k", settings.audio_kbps),
        "-ac".into(),
        if settings.mono { "1" } else { "2" }.into(),
    ]);
    // §5: the length goes on the output after every input and filter argument; before `-i` it would trim what the filters were built from.
    if let Some(limit) = output_limit(clip) {
        argv.extend(["-t".into(), limit]);
    }
    argv.push(out.display().to_string());
    let _ = filter_inputs;
    argv
}

fn video_graph(
    clip: &Clip,
    settings: &Produce,
    burned: Option<&str>,
    first_after: usize,
) -> String {
    let chain = video_chain(clip, settings, burned).join(",");
    let overlays =
        crate::produce_fx_chain::for_clip(clip, &clip.fx, clip.src, &clip.scratch).overlays;
    if overlays.is_empty() {
        return format!("[0:v]{chain}[v]");
    }
    let mut graph = format!("[0:v]{chain}[b0]");
    let first = first_after - overlays.len();
    let last = overlays.len() - 1;
    for (index, overlay) in overlays.iter().enumerate() {
        let out = if index == last {
            "v".to_string()
        } else {
            format!("b{}", index + 1)
        };
        graph.push_str(&format!(
            ";[{input}:v]{prep}[o{index}];[b{index}][o{index}]{place}[{out}]",
            input = first + index,
            prep = overlay.prep.join(","),
            place = overlay.place,
        ));
    }
    graph
}

pub fn command_log(no: usize, argv: &[String]) -> String {
    format!(">>> clip {no}: ffmpeg {}", argv.join(" ").trim_start())
}

/// S5 (`size mismatch vs clip 0 logged`): the join is a stream copy and cannot mix sizes. Compares PROBED frame sizes
/// ([`crate::produce_probe::probe`], asked by `produce_exec::walk`), never byte counts.
pub fn join_mismatch(first: (i32, i32), size: (i32, i32)) -> Option<String> {
    (size != first).then(|| JOIN_MISMATCH.to_string())
}

/// The sentence [`join_mismatch`] says, spelled once for the walk and the rule test (§B).
pub const JOIN_MISMATCH: &str =
    "!!! the join is a stream copy and cannot mix sizes, so the video breaks \
at this clip";

/// S6 (`join by stream copy`): the concat demuxer's list, one `file '…'` per clip in play order.
pub fn concat_list(stems: &[String]) -> String {
    stems
        .iter()
        .map(|stem| format!("file '{stem}'"))
        .collect::<Vec<String>>()
        .join("\n")
}

/// S6: `-c copy`, since re-encoding the joined file would add a second generation of loss.
pub fn join_command(list: &std::path::Path, out: &std::path::Path) -> Vec<String> {
    vec![
        "ffmpeg".into(),
        "-f".into(),
        "concat".into(),
        "-safe".into(),
        "0".into(),
        "-i".into(),
        list.display().to_string(),
        "-c".into(),
        "copy".into(),
        out.display().to_string(),
    ]
}

pub const JOINING: &str = "joining";
pub const JOIN_FRACTION: f64 = 0.92;

/// The run's ten steps in order, so a test can pin that translation comes after the encodes and before the mux.
pub fn stages() -> Vec<(&'static str, &'static str)> {
    vec![
        ("scratch", "clear produce/clips/"),
        ("speaking", "speak every line with no wav"),
        ("planning", "plan one clip per segment and fit its lines"),
        ("subtitles", "build the cues on the produced clock"),
        ("clip", "encode each clip"),
        ("joining", "join the clips by stream copy"),
        (
            "translating",
            "translate the cues into the ticked languages",
        ),
        ("muxing", "loudness, subtitle tracks and the container"),
        ("sidecars", "write the .srt and .vtt beside the video"),
        ("stamp", "record what this file is up to date with"),
    ]
}

pub const TRANSLATE_FRACTION: f64 = 0.94;

/// S7 (`translate the cues after the encodes`): late so the encoder never idles behind the LLM gate. Languages are the page's ticks minus the session's own (§1, [`crate::produce_screen`]).
pub fn translate_log(lines: usize, language: &str) -> String {
    format!(">>> subtitles: translating {lines} lines into {language}")
}

/// S8's audio filter, verbatim from §B step 7: resample first so `loudnorm`'s two passes see a constant rate.
pub const LOUDNORM: &str = "aresample=async=1:first_pts=0,loudnorm=I=-14:TP=-1.5:LRA=11";

pub const MUX_FRACTION: f64 = 0.96;

/// S8's mp4 stream flags. The mux is `-c:v copy`; subtitle tracks go in only for "track in file", never for webm
/// ([`crate::produce_screen::apply_container_rules`] decides).
pub const MP4_STREAM_FLAGS: [&str; 2] = ["-movflags", "+faststart+negative_cts_offsets"];

// No edit list, so there is no start offset to read two ways (browsers and VLC disagree; the picture ran two frames behind the sound).
pub const MP4_START_FLAGS: [&str; 2] = ["-use_editlist", "0"];

/// The picture's seek flag, which must precede [`INPUT_FLAG`] (inventory §B "0 picture (-ss before -i)").
pub const SEEK_FLAG: &str = "-ss";
pub const INPUT_FLAG: &str = "-i";

/// `languages` is the app's `SUBTITLE_LANGUAGES`, read by the caller to keep this a pure argv builder; tracks are tagged via [`crate::produce_details::language_tag`].
pub fn mux_command(
    video: &std::path::Path,
    audio: &std::path::Path,
    srt_tracks: &[(&str, &std::path::Path)],
    settings: &Produce,
    out: &std::path::Path,
    languages: &[crate::roles::Language],
) -> Vec<String> {
    let (_, subtitles) = crate::produce_screen::apply_container_rules(
        settings.container,
        settings.codec,
        settings.subtitles,
    );
    let mut argv = vec!["ffmpeg".into(), "-i".into(), video.display().to_string()];
    let audio_input = if audio == video {
        0
    } else {
        argv.extend(["-i".into(), audio.display().to_string()]);
        1
    };
    let tracks: &[(&str, &std::path::Path)] = if subtitles == Subtitles::TrackInFile {
        srt_tracks
    } else {
        &[]
    };
    let first_track = audio_input + 1;
    for (_, path) in tracks {
        argv.extend(["-i".into(), path.display().to_string()]);
    }
    argv.extend([
        "-map".into(),
        "0:v".into(),
        "-map".into(),
        format!("{audio_input}:a"),
        "-c:v".into(),
        "copy".into(),
        "-af".into(),
        LOUDNORM.into(),
        "-ar".into(),
        "48000".into(),
        "-c:a".into(),
        audio_codec(settings.container).into(),
        "-b:a".into(),
        format!("{}k", settings.audio_kbps),
        "-ac".into(),
        if settings.mono { "1" } else { "2" }.into(),
    ]);
    for (index, (code, _)) in tracks.iter().enumerate() {
        let (tag, name) = crate::produce_details::language_tag(code, languages);
        argv.extend([
            "-map".into(),
            format!("{}:0", first_track + index),
            "-c:s".into(),
            subtitle_codec(settings.container).into(),
            format!("-metadata:s:s:{index}"),
            format!("language={tag}"),
            format!("-metadata:s:s:{index}"),
            format!("title={name}"),
        ]);
    }
    if settings.container == project::Container::Mp4 {
        argv.extend(MP4_STREAM_FLAGS.iter().map(|flag| flag.to_string()));
        argv.extend(MP4_START_FLAGS.iter().map(|flag| flag.to_string()));
    }
    argv.push(out.display().to_string());
    argv
}

pub fn subtitle_codec(container: project::Container) -> &'static str {
    match container {
        project::Container::Mp4 => "mov_text",
        _ => "srt",
    }
}

/// S8 (`"!!! nothing to put in a subtitle"`): the page asked for cues and there are none. A warning: the video itself is finished and correct.
pub fn no_subtitle_cue_log() -> &'static str {
    "!!! nothing to put in a subtitle: no narration, and no speech in the clips"
}

/// S9: the two files beside the video for one language; the session's own language carries no code.
pub fn sidecar_names(stem: &str, language: Option<&str>) -> [String; 2] {
    let stem = match language {
        Some(code) => format!("{stem}.{code}"),
        None => stem.to_string(),
    };
    [format!("{stem}.srt"), format!("{stem}.vtt")]
}

/// S9 (`each logged`): one line per language naming both files; the .vtt is the one a browser needs.
pub fn sidecar_log(stem: &str, language: Option<&str>) -> String {
    let [srt, _] = sidecar_names(stem, language);
    format!(">>> subtitles: {srt} and .vtt")
}

/// S10 (`checkpoints between every subprocess`) / inventory §B: the five points a run asks whether to go on, each *between*
/// subprocesses; the fifth is before the model call, the one step with a cost that cannot be got back.
pub fn checkpoints() -> Vec<&'static str> {
    vec![
        "after each line spoken",
        "after each clip encoded",
        "before the join",
        "before translate and mux",
        "before the publish model call",
    ]
}

/// S10: the stamp may only be written by the run that made the file; a ▶ that encoded nothing would hide a stale render.
pub fn stamp_written(encoded: bool, failed: bool) -> bool {
    encoded && !failed
}

pub const STAGE_DONE: &str = "done";

/// S10 / §B (`">>> <file>  (X s, size)"`): two spaces before the parenthesis, as the prototype writes it.
pub fn finished_log(file: &str, seconds: f64, size: &str) -> String {
    format!(">>> {file}  ({size}, took {seconds:.1} s)")
}

pub fn failure_log() -> &'static str {
    "production failed \u{2014} see log"
}

pub fn stopped_log() -> &'static str {
    "production stopped"
}
