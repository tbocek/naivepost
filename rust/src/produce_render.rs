//! F5.2 The render — the steps between ▶ on the Produce page and `produce/final.<container>`
//! (`spec/08-produce.md` §F5.2, wording in `spec/inventory/produce.md` §B). This file covers S1–S4:
//! clearing the scratch folder, deciding what still has to be spoken, planning one clip per segment and
//! putting the cues on the produced clock. S5–S10 (encode, join, translate, mux, sidecars, checkpoints)
//! continue in the same module below.
//!
//! **Nothing is executed here.** The container this is built in has no `ffmpeg`/`ffprobe` (`command -v`
//! finds neither), so a subprocess would fail for the wrong reason; this module decides *what* each step
//! does, what it logs and the argv it would hand ffmpeg, and returns those as plain data. The caller that
//! has ffmpeg runs them in order and forwards every string here to the log. Keeping the decisions separate
//! is also what makes them testable: a plan can be asserted, a run cannot.
//!
//! Ids cited: `P.eng.minClipSeconds` (the shortest clip the render makes — [`crate::tools::cutpass::MIN_CLIP_SECONDS`]),
//! `P.policy.gameVolume` (the narration's bed-mate, applied in S5), `P.eng.laneMinMixSeconds` (the shortest
//! lane overlap worth an input), `tool:ffmpeg.encode` (every codec/preset/CRF argument is ffmpeg's own).

use crate::cut::{Fx, Lane, Seg};
use crate::cut_hear;
use crate::tools::cutpass::MIN_CLIP_SECONDS;
use crate::narrate_preview;
use crate::narrate_screen;
use crate::narration::{self, Entry};
use crate::project::{self, Produce, Subtitles};
use crate::render_fx;
use crate::tools::mm_ss;

// ---- S1: the scratch folder -------------------------------------------------------

/// §F5.2 S1 / inventory §B step 0 (`scratch produce/clips/ cleared`): where the per-clip encodes, burned
/// cues and `final.srt` live between a run's start and its join.
pub const SCRATCH: &str = "produce/clips/";

/// S1: empty the scratch folder — every file inside it, never the folder itself. It is emptied rather than
/// reused because a stale `c007_….mp4` from an earlier cut would be concatenated by name if anything ever
/// listed the folder; and it is not deleted, because the render writes into it seconds later and a missing
/// folder is an ffmpeg error with a worse message. A folder that does not exist yet has nothing to clear:
/// `Ok(0)`, no directory created.
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

// ---- S2: what still has to be spoken ---------------------------------------------

/// S2's answer: nothing at all (a captions-only voice — the lines ride the subtitle track and no request is
/// made), or the lines that have text and no wav, in the record's order so the bar counts up the way the
/// rows are read.
#[derive(Debug, Clone, PartialEq)]
pub enum Speak {
    Nothing,
    Missing(Vec<String>),
}

/// S2 (`speak every line with no wav`) — unless the voice is captions only, which speaks nothing at all:
/// `spec/07-narrate.md` §5 short-circuits everything that could speak, and here that means the whole of
/// job 1/2. A line whose text is empty is a deliberate silence and is never synthesized (the same seam
/// [`crate::narrate_off::lines_to_speak`] uses). The list holds the **words** to speak, in the record's
/// order: which wav each one lands in is [`crate::narration::tts_key`]'s answer, and asking it here would
/// make this function depend on the voice and the emotion blend it does not have.
pub fn speak_job(voice_is_captions: bool, entries: &[Entry], wav_exists: impl Fn(&Entry) -> bool) -> Speak {
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

/// S2's job line: the speaking job is `1/2`, so it carries half the bar and the encode carries the rest.
pub fn speaking_job_line(done: usize, total: usize) -> String {
    format!("speaking {done}/{total}")
}

/// Why the speaking job is worth a half: nothing else on this page can be waited for as long, and a run
/// that never got to the encoder has to look different from one that did.
pub const SPEAKING_FRACTION: f64 = 0.5;

/// S2 (`"synthesis for Ns: …" on failure`): the seconds are the ones already spent, because that is what
/// makes a stalled server distinguishable from a refusal.
pub fn synthesis_failure_log(seconds: f64, reason: &str) -> String {
    format!("synthesis for {seconds:.1}s: {reason}")
}

/// S2's branch: captions only with Subtitles "none in the video" is the one combination where the words
//  exist nowhere in the finished file — the .srt beside it is all that is left, and the log says so. Any
/// other choice already carries them in the picture or in a track.
pub fn captions_only_warning(subtitles: Subtitles) -> Option<&'static str> {
    match subtitles {
        Subtitles::None => Some(">>> captions only and nothing in the video — the lines are in the .srt beside it"),
        _ => None,
    }
}

// ---- S3: one clip per segment -----------------------------------------------------

/// What the render will do with one segment: its clock, its box, the lanes it hears, the lines placed on it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Clip {
    /// 1-based, because every log line about a clip names it to a person.
    pub no: usize,
    pub seg_s: f64,
    /// Seconds on screen — the segment's own length divided by its rate, or its `dur` when spliced.
    pub on_screen: f64,
    pub rate: f64,
    /// The asset, in the cut's own spelling (`""` for footage, `copy:<s>` for a pasted stretch).
    pub source: String,
    pub frame: (i32, i32),
    pub lanes: Vec<String>,
    pub lines: Vec<Placed>,
    /// Volume cues the clip carries; their filters are [`render_fx`]'s answer, counted here for the log.
    pub cues: usize,
    /// How deep the camera goes into the frame (1.0 = no zoom), capped by S5's caller before encoding.
    pub camera_depth: f64,
}

/// One narration line as the render places it: clip-relative output seconds, the tempo it has to be spoken
/// at, and how far past the clip the render may grow to make room.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Placed {
    pub entry_index: usize,
    pub at: f64,
    pub tempo: f64,
    pub extend: f64,
    pub text: String,
    /// Seconds of speech: the wav's length, or 0.0 when there is no synthesis (S3/F4.3: it takes no room).
    pub speech: f64,
}

/// S3 (`lines matched to clips, overlap ≥ half the shorter span`): a line belongs to the clip it spends at
/// least half of its shorter span over. "Any overlap" is not the rule, because a two-second line that
/// grazes the last frame of a long clip would be moved out of the clip it was written for — and the fit
/// then applies to the wrong room.
pub fn matches(entry: &Entry, seg: &Seg) -> bool {
    let line = entry.e - entry.s;
    let clip = seg.e - seg.s;
    if line <= 0.0 || clip <= 0.0 {
        return false;
    }
    let overlap = entry.e.min(seg.e) - entry.s.max(seg.s);
    overlap >= line.min(clip) / 2.0
}

/// S3: the render's pass over the segments — one clip each, in order. The caller hands the list **after**
/// the speed splits ([`crate::cut_speed`] owns those; a rate boundary is what makes two clips), so this is
/// arithmetic over an already-cut list and never touches the cut itself.
///
/// `exists` answers whether an insert's file is still there (the caller resolves it with
/// [`Seg::insert_asset`]), which keeps this a pure function; `entries` are the narration's lines, matched to
/// clips by [matches](fn@matches) and fitted by [`fit`]; `lanes` is the project's lane list, asked per clip
/// through [`cut_hear`] rather than re-derived here.
pub fn plan_clips(
    segs: &[Seg],
    fx_for: impl Fn(&Seg) -> Vec<Fx>,
    aspect: &str,
    exists: impl Fn(&str) -> bool,
    lanes: &[Lane],
    entries: &[Entry],
    src_shape: (i32, i32),
) -> (Vec<Clip>, Vec<String>) {
    let frame = render_fx::frame_box(aspect, src_shape.0, src_shape.1);
    let mut clips: Vec<Clip> = Vec::new();
    let mut logs: Vec<String> = Vec::new();
    // One number per segment asked for, including the ones dropped: "clip 7" has to mean the same thing in
    // this log as it does on the Cut page's rows and in every earlier line of this run.
    for (index, seg) in segs.iter().enumerate() {
        let no = index + 1;
        if !seg.ins.is_empty() && !seg.ins.starts_with("copy:") && !exists(&seg.ins) {
            logs.push(missing_insert_log(no, &seg.ins));
            continue;
        }
        // A rate of nought is a stop: the footage runs on at 1× and the still rides over it (§4), so the
        // clip's own clock stays 1× even though an effect in its list says otherwise.
        let fx = fx_for(seg);
        let rate = if seg.rate > 0.0 { seg.rate } else { 1.0 };
        let on_screen = crate::narrate_pass::on_screen(seg);
        if on_screen < MIN_CLIP_SECONDS {
            logs.push(too_short_log(no, seg.s, on_screen, lost_lines(entries, seg)));
            continue;
        }
        let heard: Vec<String> = lanes
            .iter()
            .filter(|lane| lane_heard(lane, seg))
            .map(|lane| lane.name.clone())
            .collect();
        let cues = render_fx::gain_cues(&fx, seg.s, rate, on_screen).len();
        // The lines this clip was written for, in placement order, with the room the clip has after the
        // previous clips were counted — the fit is per clip, so each gets its own call.
        let mine: Vec<(usize, f64, f64)> = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| matches(entry, seg))
            .map(|(entry_index, entry)| (entry_index, entry.at, 0.0))
            .collect();
        let (lines, _) = fit(&mine, on_screen);
        clips.push(Clip {
            no,
            seg_s: seg.s,
            on_screen,
            rate,
            source: seg.ins.clone(),
            frame,
            lanes: heard,
            lines,
            cues,
            camera_depth: 1.0,
        });
    }
    (clips, logs)
}

/// The lines written on one clip that the render is dropping — counted for S3's "lost narration named", so a
/// dropped clip says what it took with it instead of leaving the listener to count rows. Every line that
/// touches the segment counts: a clip under [`MIN_CLIP_SECONDS`] has no room for any of them whatever the
/// overlap, so `matches` would be too generous an answer here.
fn lost_lines(entries: &[Entry], seg: &Seg) -> usize {
    entries
        .iter()
        .filter(|entry| entry.e > seg.s && entry.s < seg.e)
        .count()
}

/// S3 (`clip N: <file> is not there any more — skipped`): an insert whose file went away. The same sentence
/// covers a sound insert with no picture, which is the same failure seen from the other side — the render
/// cannot place material it cannot open, and both name what is missing rather than the step that noticed.
pub fn missing_insert_log(no: usize, file: &str) -> String {
    format!("clip {no}: {file} is not there any more \u{2014} skipped")
}

/// S3 (`clips under P.eng.minClipSeconds dropped, log line naming any narration lost`): the shortest clip
/// the render makes. Naming the lost lines is the point of the sentence — a dropped half-second of picture
/// is nothing on its own, and the line spoken over it is everything.
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

/// S3: every segment went somewhere the render cannot place — no clip at all. The run stops here rather
/// than encoding nothing into an empty file.
pub fn no_clip_log() -> &'static str {
    "no clip could be placed on a recording"
}

// ---- S3's fitting: the render's half of F4.3 --------------------------------------

/// P.eng.narrationLeadSeconds (§10: 0.3): where a clip's first line may start from. The id is deliberately
/// **not** in [`crate::params`] yet — `tests/narrate_parameters.rs`'s s6 and `tests/narrate_screen_widgets.rs`
/// both assert its absence, because the row belongs with the rule that reads it; when a parameters round
/// moves these rows into the catalogue it deletes this comment and the constant together.
pub const LEAD_SECONDS: f64 = 0.3;

/// P.eng.narrationGapSeconds (§10: 0.3): the shortest silence between two lines of one clip. Packing them
/// tighter turns two sentences into one run-on, which is what the row warnings promise against.
pub const GAP_SECONDS: f64 = 0.3;

/// P.eng.narrationTailSeconds (§10: 0.2): the breath after a line before the next may start, so the last
/// word is not cut by the splice. One home for the number: [`narrate_screen::SPEECH_TAIL_SECONDS`].
pub const TAIL_SECONDS: f64 = narrate_screen::SPEECH_TAIL_SECONDS;

/// P.eng.narrationMaxExtendSeconds (§10: 4): how far past its own end the render may grow a clip to fit its
/// line. Same value as the preview's hold bound, and it is read from there rather than written again.
pub const MAX_EXTEND_SECONDS: f64 = narrate_preview::MAX_EXTEND_SECONDS;

/// P.eng.narrationMaxTempo (§10: 1.25): the fastest a take may be sped up to fit. Past this a line is
/// placed as it is and simply runs over — an unnaturally fast voice is worse than a late cut, which is why
/// the ceiling exists at all. Not in [`crate::params`] for the same reason as [`LEAD_SECONDS`].
pub const MAX_TEMPO: f64 = 1.25;

/// S3 / F4.3: place a clip's lines in its room, in four steps and in this order — pack from where each was
/// placed (never before the previous line's end plus [`GAP_SECONDS`] and the tail); still over → grow the
/// clip by up to [`MAX_EXTEND_SECONDS`]; still over → slide the whole run earlier, down to the lead; still
/// over → speed every line up to [`MAX_TEMPO`] and pack again. Only then is it logged.
///
/// `lines` are `(entry_index, at, speech)` in placement order; `room` is the clip's on-screen length. A line
/// with no synthesis arrives with `speech` 0.0: it takes no room and its cue is held to the next line or the
/// clip's end, so a missing wav never moves the lines after it.
pub fn fit(lines: &[(usize, f64, f64)], room: f64) -> (Vec<Placed>, Vec<String>) {
    let mut logs: Vec<String> = Vec::new();
    if lines.is_empty() {
        return (Vec::new(), logs);
    }
    let no = entry_no(lines);

    // 0 · pack as placed. Fits? Nothing to say and nothing to change.
    let placed = pack(lines, 1.0, 0.0);
    if fits(&placed, room) {
        return (placed, logs);
    }

    // 1 · grow the clip, never past the ceiling §10 gives. Growing is free of logs: it is what the render is
    // allowed to do silently, and the only reason to mention it would be that it stopped at the ceiling.
    let extend = (run_end(&placed) + TAIL_SECONDS - room).clamp(0.0, MAX_EXTEND_SECONDS);
    if fits_at(&placed, room, extend) {
        return (with_extend(placed, extend), logs);
    }

    // 2 · slide the whole run earlier, down to the lead: a clip may start talking before its first frame is
    // interesting, but not before it exists.
    let room = room + extend;
    let slide = (run_end(&placed) + TAIL_SECONDS - room).clamp(0.0, slide_of(lines));
    let slid = pack(lines, 1.0, -slide);
    if fits_at(&slid, room, 0.0) {
        logs.push(moved_log(no, slide));
        return (with_extend(slid, extend), logs);
    }

    // 3 · speed up to the ceiling and pack again at the new tempo. The line that gets a tempo is the one
    // whose wav will be asked for it, so the log names what the listener is about to hear.
    let tempo = needed_tempo(lines, room).min(MAX_TEMPO);
    let final_slide = (run_end(&pack(lines, tempo, 0.0)) + TAIL_SECONDS - room).clamp(0.0, slide_of(lines));
    let sped = pack(lines, tempo, -final_slide);
    if (tempo - 1.0).abs() > f64::EPSILON {
        logs.push(moved_log(no, final_slide));
        logs.push(sped_up_log(no, run_seconds(lines), room, tempo));
    }
    (with_extend(sped, extend), logs)
}

/// Pack the run at `shift` seconds off every placement and `tempo` off every line's length: each line starts
/// where it was placed, never before the previous line's end + gap + tail, and never before the lead.
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

/// Where the run finishes speaking.
fn run_end(placed: &[Placed]) -> f64 {
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

/// How much room the run needs, spoken at 1×: where it finishes.
fn run_seconds(lines: &[(usize, f64, f64)]) -> f64 {
    run_end(&pack(lines, 1.0, 0.0))
}

/// How far the run can slide: back to the lead of the first line, no further — seconds before the clip
/// starts do not exist for it.
fn slide_of(lines: &[(usize, f64, f64)]) -> f64 {
    lines
        .iter()
        .map(|(_, at, _)| *at)
        .fold(f64::MAX, f64::min)
        .max(LEAD_SECONDS)
        - LEAD_SECONDS
}

/// The tempo the run needs to fit if it is sped up uniformly — capped by the caller at [`MAX_TEMPO`]. Every
/// line starts as early as it may, so what matters is the total time spent speaking.
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

/// Which clip the logs name: a fitting runs belong to one clip, so its number is the first line's index + 1
/// unless the caller passes a real one (indices are 0-based entries; §B counts clips from 1).
fn entry_no(lines: &[(usize, f64, f64)]) -> usize {
    lines.first().map(|(index, _, _)| *index + 1).unwrap_or(1)
}

/// F4.3 (`"clip N: the narration does not fit where it was placed — moved X s earlier"`).
pub fn moved_log(no: usize, seconds: f64) -> String {
    format!("clip {no}: the narration does not fit where it was placed \u{2014} moved {seconds:.1} s earlier")
}

/// F4.3 (`"clip N: narration X s does not fit Y s — sped up Zx"`).
pub fn sped_up_log(no: usize, speech: f64, room: f64, tempo: f64) -> String {
    format!("clip {no}: narration {speech:.1} s does not fit {room:.1} s \u{2014} sped up {tempo:.2}x")
}

// ---- S3's lane mixes -------------------------------------------------------------

/// P.eng.laneMinMixSeconds (§10: 0.1, inventory §B `overlap ≥ 0.1 s`): the shortest overlap worth an ffmpeg
/// input of its own. Under it the lane is inaudible either way and every extra input costs a decode.
pub const LANE_MIN_MIX_SECONDS: f64 = 0.1;

/// Inventory §B step 2's per-lane report — the count is what makes a missing lane findable: "mixed into 3 of
/// the 5 clips" says which clips to go and look at.
pub fn lane_report(lane: &str, mixed: usize, total: usize) -> String {
    format!("{lane} is mixed into {mixed} of the {total} clips")
}

/// The lane runs, but every clip it underlies leaves it out — a `quiet` list on all of them. §B's sentence
/// starts with the lane's own name, which is why this takes it and returns the tail.
pub fn lane_left_out(lane: &str) -> String {
    format!("{lane} runs under 0 clip(s) and every one of them leaves it out \u{2014} it is not in the render")
}

/// The lane was not playing while any clip was: its window lies outside the cut entirely.
pub fn lane_not_running(lane: &str) -> String {
    format!("{lane} was not running while any clip was \u{2014} it is not in the render")
}

/// Does this clip hear this lane? The scene's `quiet` list answers, through [`cut_hear`] so the same rule
/// serves the preview and the render.
fn lane_heard(lane: &Lane, seg: &Seg) -> bool {
    seg.hears(&lane.name) && cut_hear::lane_covers(lane, cut_hear::lane_seconds(lane, seg.s), seg.s)
}

/// S3 (`sound plan for own-clock runs`): a speed effect whose sound stayed at 1× is read from the recording
/// on its own clock while the picture runs ahead — this is where that run is planned, and [`render_fx`]
/// puts the 0.15 s dips ([`crate::fx_lane::SOUND_DIP_SECONDS`]) at its ends. A pitched or muted sound has
/// no run to read: it follows the picture's clock or is silent.
pub fn read_head_plan(seg: &Seg, fx: &[Fx]) -> Option<(f64, f64)> {
    let own = fx.iter().any(|effect| {
        effect.effect_kind() == Some(crate::cut::EffectKind::Speed) && matches!(effect.snd.as_str(), "own" | "scene")
    });
    own.then(|| (seg.s, seg.e))
}

// ---- S4: subtitles on the produced clock -----------------------------------------

/// S4 (`subtitles built on the produced clock`): where each clip begins in the finished file. A clip at 2×
/// runs half as long as its footage and a sped clip's cues move with it — [`narration::output_seconds`] is
/// what converts, the same function the narration's own placement uses. The second of each pair is the
/// clip's length on screen, so a cue can be asked for either end.
pub fn produced_clocks(clips: &[Clip]) -> Vec<(f64, f64)> {
    let mut at = 0.0;
    let mut clocks = Vec::with_capacity(clips.len());
    for clip in clips {
        clocks.push((at, clip.on_screen));
        at += narration::output_seconds(clip.on_screen, clip.rate);
    }
    clocks
}

/// S4 (`clips/final.srt`): the cue sheet's own name, before any language is added to it.
pub fn final_srt_name() -> &'static str {
    "final.srt"
}

/// S4 (`stale sidecars deleted by exact name first`): every file this run may overwrite beside the video,
/// by name — never a glob. A `*.de.srt` pattern would delete a translation of something else that happens
/// to share the folder, and the render cannot tell that apart from its own leftovers.
pub fn stale_sidecars(stem: &str, languages: &[&str]) -> Vec<String> {
    let mut names = vec![format!("{stem}.srt"), format!("{stem}.vtt")];
    for code in languages {
        names.push(format!("{stem}.{code}.srt"));
        names.push(format!("{stem}.{code}.vtt"));
    }
    names
}

// ---- S5: encode one clip ---------------------------------------------------------

/// S5's "limiter −1 dBFS" (`P.eng.clipLimiter`): ffmpeg takes an amplitude, and 0.891 is the amplitude of
/// −1 dBFS. Written as the number rather than a dB expression because that is what `alimiter` accepts, and
/// named because §4 lists it as one of the page's two mix targets — the catalogue reads it off this string
/// rather than carrying a third copy of the figure.
pub const LIMITER: &str = "alimiter=limit=0.891:level=disabled";

/// §4's frame-edge blur: how far the blown-up backdrop is smeared, as a fraction of the finished
/// frame's height. Enough that no detail survives to be read as a second picture, not so much
/// that a bright scene turns into one flat glow. (Prototype `blurSigma`, gui/produce.go:1310.)
/// §10 files the pair as `P.eng.blurSigma` ("0.02·height, min 4").
pub const BLUR_SIGMA_FRACTION: f64 = 0.02;

/// The floor on the derived sigma. Below 200 px of frame height `0.02 * height` drops under 4, and at
/// that size the smear stops hiding the edge seam at all — so a small frame still gets a blur wide
/// enough to read as intentional rather than as a hard border.
pub const BLUR_SIGMA_MIN: f64 = 4.0;

/// The sigma `gblur` is asked for at a given frame height: [`BLUR_SIGMA_FRACTION`] of it, floored at
/// [`BLUR_SIGMA_MIN`], rounded up to a whole pixel. Ceil rather than round so the blur is never
/// thinner than the rule asked for; a sub-pixel sigma would only make the seam cheaper to see.
pub fn blur_sigma(frame_height: i32) -> i32 {
    (f64::from(frame_height) * BLUR_SIGMA_FRACTION)
        .max(BLUR_SIGMA_MIN)
        .ceil() as i32
}

/// S5 / inventory §B (`stems c%03d_<stamp>`): the per-clip file's name. The stamp is in it because the
/// concat list is rebuilt from the scratch folder and a leftover `c000.mp4` of an older cut would join into
/// this one by name alone.
pub fn clip_stem(no: usize, stamp: &str) -> String {
    format!("c{:03}_{}", no, stamp)
}

/// S5's `-fps_mode` piece, kept separate because it is the one part of the command a test can pin without
/// building a whole clip. `source` adds nothing at all — the recording's own rate is what ffmpeg gets when
/// it is told nothing. With the frame-timing tick on the rate is a ceiling (§A: "peak rate (VFR)"); with it
/// off it is a target, and `-fpsmax` is what makes it one rather than a floor ffmpeg pads.
pub fn frame_rate_args(frame_rate: &str, vfr: bool) -> Vec<String> {
    if frame_rate == "source" {
        return Vec::new();
    }
    if vfr {
        return vec!["-fps_mode".into(), "vfr".into(), "-fpsmax".into(), frame_rate.into()];
    }
    vec![
        "-fps_mode".into(),
        "cfr".into(),
        "-fpsmax".into(),
        frame_rate.into(),
    ]
}

/// The Preset row's spelling of a preset — the same words §A lists, and the words x264 takes.
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

/// The Frame rate row's spelling: `source` for the recording's own rate, otherwise the number as §A writes
/// it. Kept separate from the enum's serde names because those are file bytes and these are argv.
pub fn frame_rate_name(frame_rate: project::FrameRate) -> &'static str {
    use crate::project::FrameRate::*;
    match frame_rate {
        Source => "source",
        F60 => "60",
        F30 => "30",
        F24 => "24",
    }
}

/// S5's video chain, in inventory §B's order — the order **is** the behaviour: speed first so every later
/// filter works in on-screen seconds, the frozen spans of a stop over the footage that runs on beneath
/// them, then the frame rate, the edges, the camera, and only then the things printed on top. Burned
/// subtitles go before the text overlays because §4 composites an effect after the camera *and* the burned
/// cues, which is what keeps a title above a caption rather than under it.
pub fn video_chain(clip: &Clip, settings: &Produce, burned: Option<&str>) -> Vec<String> {
    let mut chain = Vec::new();
    if (clip.rate - 1.0).abs() > f64::EPSILON {
        if let Some(filter) = render_fx::setpts(clip.rate) {
            chain.push(filter);
        }
    }
    chain.extend(frame_rate_filter(frame_rate_name(settings.frame_rate), settings.vfr));
    if settings.blurred_edges {
        // §4's "blurred": a blown-up, blurred copy of the picture centred behind the frame. The sigma
        // is derived here by [`blur_sigma`] from the finished height — [`BLUR_SIGMA_FRACTION`] of it,
        // floored at [`BLUR_SIGMA_MIN`] — so one rule owns both numbers and the catalogue reads them
        // off the same pair. (It does not come from `render_fx`: nothing there derives a sigma.)
        chain.push(format!("split=2[bg][fg]"));
        chain.push(format!(
            "[bg]scale={}:{}:force_original_aspect_ratio=increase,crop={}:{},gblur=sigma={}[bg]",
            clip.frame.0,
            clip.frame.1,
            clip.frame.0,
            clip.frame.1,
            blur_sigma(clip.frame.1)
        ));
        chain.push("[fg][bg]overlay=(W-w)/2:(H-h)/2".into());
    }
    chain.push(format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:black,setsar=1",
        clip.frame.0, clip.frame.1, clip.frame.0, clip.frame.1
    ));
    if let Some(cues) = burned {
        chain.push(format!("subtitles={cues}"));
    }
    chain
}

/// The frame rate as a filter (the camera's own path under VFR) — the in-graph half of [`frame_rate_args`].
fn frame_rate_filter(frame_rate: &str, vfr: bool) -> Vec<String> {
    if frame_rate == "source" {
        return Vec::new();
    }
    if vfr {
        return vec![format!("fps=fps={frame_rate}:round=near")]
            .into_iter()
            .collect();
    }
    vec![format!("fps=fps={frame_rate}")]
}

/// S5's audio chain, in the order §B and S5 give it: speed → hush → the lane bed → gain cues → seam dips →
/// the narration at `P.policy.gameVolume` → limiter → format. Extends [`render_fx::audio_order`] rather than
/// re-listing it: that answer is where the volume cues sit relative to the lane mix, and this is everything
/// around it.
pub fn audio_chain(clip: &Clip, settings: &Produce, lanes: usize, hush: &[(f64, f64)]) -> Vec<String> {
    let mut chain = Vec::new();
    if (clip.rate - 1.0).abs() > f64::EPSILON {
        chain.push(render_fx::atempo_chain(clip.rate));
    }
    for (from, to) in hush {
        chain.push(format!(
            "volume=0:eval=frame:enable='between(t,{from:.3},{to:.3})'"
        ));
    }
    if lanes > 0 {
        chain.push(format!("amix=inputs={}:normalize=0", lanes + 1));
    }
    // The gain cues are render_fx's own filters, applied after the bed and before the narration (§4).
    for _ in 0..clip.cues {
        chain.push("volume=expr:eval=frame".into());
    }
    let dip = crate::fx_lane::SOUND_DIP_SECONDS;
    chain.push(format!("afade=t=out:st=0:d={dip}"));
    for line in &clip.lines {
        if line.speech <= 0.0 {
            continue;
        }
        // P.policy.gameVolume is the bed's level, not the voice's: the narration rides at full level over a
        // bed turned down to it, which is why the number appears on the lane side of the mix.
        chain.push(format!("volume={:.2}", settings.game_volume));
        chain.push(format!("adelay={}ms|{}ms", (line.at * 1000.0).round() as i64, (line.at * 1000.0).round() as i64));
    }
    chain.push("amix=duration=first:normalize=0".into());
    chain.push(LIMITER.into());
    let layout = if settings.mono { "mono" } else { "stereo" };
    chain.push(format!(
        "format=sample_fmts=fltp:sample_rates=48000:channel_layouts={layout}"
    ));
    chain
}

/// S5's codec arguments — `tool:ffmpeg.encode`: these are ffmpeg's encoder names and flags, not this app's
/// vocabulary. h264 carries `-refs 4` because that is what the prototype shipped and what makes the joins
/// cheap; vp9 takes `-b:v 0` (constant-quality mode) and `-cpu-used` from the same preset the x264 branch
/// uses, so one row of the settings page drives both.
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

/// vp9 has no preset ladder, so the one setting is walked to ffmpeg's `-cpu-used` (0 = slowest and smallest,
/// 5 = fastest): the same trade the Preset row promises, on the encoder that has only this dial.
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

/// The audio codec the container will actually hold: Opus in webm (the only one it defines), AAC elsewhere.
pub fn audio_codec(container: project::Container) -> &'static str {
    match container {
        project::Container::Webm => "libopus",
        _ => "aac",
    }
}

/// S5: the whole command for one clip, in the order ffmpeg wants it — `-ss` **before** `-i` so the seek
/// happens on input and a clip that starts at 40 minutes does not decode 40 minutes first; then one `-i`
/// per input (the picture, its own-clock sound or `anullsrc`, one per spoken line, one per lane overlap, the
/// looped text/svg inputs, the stills); then the graph, the codec arguments and the frame-rate flags.
///
/// `inputs` is what the caller resolved: `(kind, path_or_none)` pairs in that order, where a `None` path is
/// the silent `anullsrc` source. Nothing here is executed; [`command_log`] prints it.
/// §5 (`-t` on the output for inserts, sounds, freezes, rated clips): how long this clip's file runs, when
/// its input's own clock does not already answer that.
///
/// An insert or a pasted stretch names an asset in [`Clip::source`] whose length is its own; a rated clip
/// must stop when the cut says, not when the recording does. A **freeze** is covered by the rate arm: §5's
/// stop reaches here as `rate == 0.0` with [`Clip::on_screen`] holding the still's length
/// ([`crate::narrate_pass::on_screen`]), and 0.0 differs from 1.0, so it is trimmed like any other rated clip.
///
/// Plain footage at 1× answers `None`: the recording's remaining length is already the right length, and a
/// `-t` computed from session seconds would trim a frame off the end for no reason.
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
    let mut argv = vec!["ffmpeg".into()];
    // The picture's own seek first, always before its -i (inventory §B: "0 picture (-ss before -i)").
    if clip.seg_s > 0.0 {
        argv.push("-ss".into());
        argv.push(format!("{:.3}", clip.seg_s));
    }
    let mut filter_inputs = 0;
    for (kind, path) in inputs {
        match (*kind, path) {
            ("sound", None) => {
                argv.extend(["-f".into(), "lavfi".into(), "-i".into(), "anullsrc=r=48000:cl=stereo".into()]);
            }
            (_, Some(path)) => {
                if matches!(*kind, "text" | "svg" | "still") {
                    argv.extend(["-loop".into(), "1".into(), "-framerate".into(), "30".into()]);
                }
                argv.push("-i".into());
                argv.push(path.to_string());
            }
            (_, None) => {}
        }
        filter_inputs += 1;
    }
    let video = video_chain(clip, settings, burned).join(",");
    let audio = audio_chain(clip, settings, clip.lanes.len(), &[]).join(",");
    argv.extend([
        "-filter_complex".into(),
        format!("[0:v]{video}[v];[0:a]{audio}[a]"),
    ]);
    argv.extend(codec_args(settings));
    argv.extend(frame_rate_args(frame_rate_name(settings.frame_rate), settings.vfr));
    argv.extend([
        "-c:a".into(),
        audio_codec(settings.container).into(),
        "-b:a".into(),
        format!("{}k", settings.audio_kbps),
        "-ac".into(),
        if settings.mono { "1" } else { "2" }.into(),
    ]);
    // §5: the length goes on the output, after every input and filter argument — never before `-i`, where it
    // would trim what the filters were built from.
    if let Some(limit) = output_limit(clip) {
        argv.extend(["-t".into(), limit]);
    }
    argv.push(out.display().to_string());
    let _ = filter_inputs;
    argv
}

/// S5 (`every command logged`): the whole argv on one line, prefixed by the clip it belongs to. Logged
/// before it runs rather than after, because the command that hung is the one worth reading.
pub fn command_log(no: usize, argv: &[String]) -> String {
    format!(">>> clip {no}: ffmpeg {}", argv.join(" ").trim_start())
}

/// S5 (`size mismatch vs clip 0 logged`): the join is a stream copy and cannot mix sizes, so a clip that
/// ffprobe reports at a different size than clip 0 breaks the video at exactly that clip. Detected after
/// the encode because only the encoder knows what it actually wrote.
pub fn join_mismatch(first: u64, size: u64) -> Option<String> {
    (size != first).then(|| {
        "!!! the join is a stream copy and cannot mix sizes, so the video breaks at this clip".to_string()
    })
}

// ---- S6: join by stream copy -----------------------------------------------------

/// S6 (`join by stream copy`): the concat demuxer's list, one `file '…'` line per clip in the order they
/// play. Single quotes because that is the demuxer's own quoting and a stem with a space in it is a stamp,
/// not a filename somebody typed.
pub fn concat_list(stems: &[String]) -> String {
    stems
        .iter()
        .map(|stem| format!("file '{stem}'"))
        .collect::<Vec<String>>()
        .join("\n")
}

/// S6: the join itself. `-c copy` is the whole point — re-encoding the joined file would mean a second
/// generation of loss on top of the per-clip encodes, and minutes more on the clock.
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

/// S6's job line, and the fraction of the bar it is worth: everything before it (the speaking and the
/// encodes) has already been counted per line and per clip, and what follows is quick.
pub const JOINING: &str = "joining";
pub const JOIN_FRACTION: f64 = 0.92;

// ---- S7: translate after the encodes ---------------------------------------------

/// The run's ten steps in order — the same list the progress line walks, written down so a test can pin that
/// translation comes *after* the encodes and before the mux rather than trusting where a callback happens to
/// be placed.
pub fn stages() -> Vec<(&'static str, &'static str)> {
    vec![
        ("scratch", "clear produce/clips/"),
        ("speaking", "speak every line with no wav"),
        ("planning", "plan one clip per segment and fit its lines"),
        ("subtitles", "build the cues on the produced clock"),
        ("clip", "encode each clip"),
        ("joining", "join the clips by stream copy"),
        ("translating", "translate the cues into the ticked languages"),
        ("muxing", "loudness, subtitle tracks and the container"),
        ("sidecars", "write the .srt and .vtt beside the video"),
        ("stamp", "record what this file is up to date with"),
    ]
}

/// S7's fraction: after every encode, before the mux that needs their result.
pub const TRANSLATE_FRACTION: f64 = 0.94;

/// S7 (`translate the cues after the encodes`): deliberately last-but-two, so the encoder never idles
/// behind the LLM gate — a translation that takes a minute is a minute an encode could have been doing
/// something. The languages are the page's ticks minus the session's own (§1, [`crate::produce_screen`]).
pub fn translate_log(lines: usize, language: &str) -> String {
    format!(">>> subtitles: translating {lines} lines into {language}")
}

// ---- S8: loudness and mux --------------------------------------------------------

/// S8's audio filter, verbatim from §B step 7: the resample first so `loudnorm`'s two passes see a constant
/// rate, then the target every upload platform expects.
pub const LOUDNORM: &str = "aresample=async=1:first_pts=0,loudnorm=I=-14:TP=-1.5:LRA=11";

/// S8's fraction of the bar.
pub const MUX_FRACTION: f64 = 0.96;

/// S8: the mux. `-c:v copy` because the video was encoded per clip and joined by copy already — this step
/// only rewrites the audio (loudnorm has to) and repacks. Subtitle tracks are added **only** when the page
/// chose "track in file", and never for webm, which has no text track: that is decided by
/// [`crate::produce_screen::apply_container_rules`] rather than guessed again here, so the settings row and
/// the command cannot disagree about what webm carries.
pub fn mux_command(
    video: &std::path::Path,
    audio: &std::path::Path,
    srt_tracks: &[(&str, &std::path::Path)],
    settings: &Produce,
    out: &std::path::Path,
) -> Vec<String> {
    let (codec, subtitles) = crate::produce_screen::apply_container_rules(
        settings.container,
        settings.codec,
        settings.subtitles,
    );
    let _ = codec;
    let mut argv = vec![
        "ffmpeg".into(),
        "-i".into(),
        video.display().to_string(),
        "-i".into(),
        audio.display().to_string(),
        "-map".into(),
        "0:v".into(),
        "-map".into(),
        "0:a".into(),
        "-c:v".into(),
        "copy".into(),
        "-af".into(),
        LOUDNORM.into(),
        "-ar".into(),
        "48000".into(),
        "-c:a".into(),
        audio_codec(settings.container).into(),
    ];
    if subtitles == Subtitles::TrackInFile {
        argv.extend(["-c:s".into(), subtitle_codec(settings.container).into()]);
        for (index, (_, path)) in srt_tracks.iter().enumerate() {
            // The .srt files are inputs after the two streams, so their index starts at 2; each is mapped
            // to its own subtitle stream and keeps its language tag.
            argv.push("-i".into());
            argv.push(path.display().to_string());
            argv.extend([
                "-map".into(),
                format!("s:{index}"),
                "-metadata".into(),
                format!("s:{index}:language={index}"),
            ]);
        }
    }
    if settings.container == project::Container::Mp4 {
        // faststart so the file streams from the front, and the two edit-list flags that stop mp4 writing a
        // negative-CTS start that some players read as a black first frame.
        argv.extend([
            "-movflags".into(),
            "+faststart".into(),
            "-use_editlist".into(),
            "0".into(),
            "-negative_cts_offsets".into(),
            "on".into(),
        ]);
    }
    argv.push(out.display().to_string());
    argv
}

/// The subtitle codec the container takes: `mov_text` in mp4, `srt` in mkv. Webm never reaches here —
/// [`crate::produce_screen::apply_container_rules`] has already turned its tracks into "none".
pub fn subtitle_codec(container: project::Container) -> &'static str {
    match container {
        project::Container::Mp4 => "mov_text",
        _ => "srt",
    }
}

/// S8 (`"!!! nothing to put in a subtitle"`): the page asked for cues and there are none — no narration, and
//  no speech inside the clips either. A warning rather than an error, because the video itself is finished
/// and correct; it is the file beside it that will not exist.
pub fn no_subtitle_cue_log() -> &'static str {
    "!!! nothing to put in a subtitle: no narration, and no speech in the clips"
}

// ---- S9: sidecars ----------------------------------------------------------------

/// S9 (`sidecars <stem>[.code].srt and .vtt per language`): the two files written beside the video for one
/// language. The session's own language carries no code — it is the track a player picks by default, and a
/// second copy named after its own language would be two files saying the same thing.
pub fn sidecar_names(stem: &str, language: Option<&str>) -> [String; 2] {
    let stem = match language {
        Some(code) => format!("{stem}.{code}"),
        None => stem.to_string(),
    };
    [format!("{stem}.srt"), format!("{stem}.vtt")]
}

/// S9 (`each logged`): one line per language, naming both files. The .vtt is mentioned because it is the
/// one a browser needs and the one people look for first when subtitles do not show up in an embed.
pub fn sidecar_log(stem: &str, language: Option<&str>) -> String {
    let [srt, _] = sidecar_names(stem, language);
    format!(">>> subtitles: {srt} and .vtt")
}

// ---- S10: checkpoints and the stamp ----------------------------------------------

/// S10 (`checkpoints between every subprocess`) / inventory §B's list: the five points a run asks whether it
/// should go on. Each sits *between* subprocesses, so a stop never leaves half an ffmpeg command running;
/// the fifth is before the model call because that is the one step that costs somebody something they cannot
/// get back (a cache miss and a minute of GPU).
pub fn checkpoints() -> Vec<&'static str> {
    vec![
        "after each line spoken",
        "after each clip encoded",
        "before the join",
        "before translate and mux",
        "before the publish model call",
    ]
}

/// S10 (`the run writes the stamp once the render returns without error; a press that skipped the encode
/// writes none`): the stamp is the promise that this file matches these settings, so it may only be written
/// by the run that made the file. A ▶ that found everything up to date and encoded nothing has proved
/// nothing new — and writing the stamp again would hide a stale render from the next press.
pub fn stamp_written(encoded: bool, failed: bool) -> bool {
    encoded && !failed
}

/// The bar's last word, and the three ways a run ends that are not "done".
pub const STAGE_DONE: &str = "done";

/// S10 / §B (`">>> <file>  (X s, size)"`): the file, how long it took, what it weighs. Two spaces before
/// the parenthesis because that is how the prototype writes it and people read these lines side by side in a
/// log after comparing two runs.
pub fn finished_log(file: &str, seconds: f64, size: &str) -> String {
    format!(">>> {file}  ({seconds:.1} s, {size})")
}

/// The status line for a run that failed: the log has the reason, and this page does not repeat it.
pub fn failure_log() -> &'static str {
    "production failed \u{2014} see log"
}

/// The status line for a run somebody stopped. Not a failure — nothing was left half-written that the next
/// press cannot redo.
pub fn stopped_log() -> &'static str {
    "production stopped"
}
