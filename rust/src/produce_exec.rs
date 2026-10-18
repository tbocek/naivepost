//! F5.2 The render — the side that **runs** it.
//!
//! [`crate::produce_render`] decides S1–S10: what each step means, what it logs, the argv it would hand
//! ffmpeg. It spawns nothing on purpose, which is what makes those decisions testable with no encoder
//! standing. Somebody still has to walk the ten steps and start the tools, and that is this module: the
//! queue of commands, in the spec's own order, with the five checkpoints between them.
//!
//! A queue rather than a loop that spawns as it goes, because the run belongs to whichever thread its
//! caller puts it on and this module must not own that. [`plan`] answers every step, every log string and
//! the whole list of subprocesses **before** anything runs; [`walk`] then drains them and asks the
//! checkpoint question between the commands. The GUI thread calls [`plan`] and hands the plan to the
//! run's thread, which never reaches for a widget to learn what comes next.
//!
//! The spawner is injected for the same reason the plan is separate: this container has no ffmpeg
//! (`command -v ffmpeg` finds nothing), so a test that wanted to prove the ORDER of the ten steps could
//! otherwise only prove that a missing tool fails early. A test hands in a closure that records argv; the
//! program hands in [`crate::subprocess::run`], which is the only spawner in the produce tree. That is
//! also how `produce_render`'s own rule (`f5_2_module_runs_nothing`: no `std::process` in it) stays true.
//!
//! Nothing here decides a rule. Every string, every argv and every path comes from `produce_render`,
//! `produce_subtitles` or [`Tree`]; the split is the point.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::cut::{Fx, Lane, Seg};
use crate::layout::Tree;
use crate::narration::Entry;
use crate::produce_details as details;
use crate::produce_flow::Run;
use crate::produce_render as render;
use crate::produce_stamp;
use crate::produce_subtitles as subs;
use crate::project::{Container, Produce};

// --- the checkpoint's question ---------------------------------------------------------------------

/// What kind of stop a checkpoint is: what the run had already written when it halted. The three kinds
/// come out of S10's five points, because a stop is only worth reporting with its cost attached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum At {
    /// Nothing has run yet: the scratch folder is clear and the project has no new files.
    BeforeTools,
    /// The clips are encoded; the join has not started.
    BeforeJoin,
    /// The clips are joined; the model gate and the mux have not run.
    BeforeModelAndMux,
}

// --- the plan ------------------------------------------------------------------------------------

/// One subprocess the run will start, with the line that announces it. `argv[0]` is the program as
/// `produce_render` spells it; [`Command::program`] and [`Command::args`] split it the way
/// [`crate::subprocess::run`] takes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub step: &'static str,
    pub log: String,
    pub argv: Vec<String>,
}

impl Command {
    pub fn program(&self) -> &str {
        &self.argv[0]
    }

    pub fn args(&self) -> &[String] {
        &self.argv[1..]
    }
}

/// The command for a step whose argv `produce_render` builds bare (it returns the arguments, not the
/// program name; the program is [`FFMPEG`]).
fn command(step: &'static str, argv: Vec<String>) -> Command {
    Command {
        step,
        log: format!(">>> {step}: {} {}", FFMPEG, argv.join(" ")),
        argv: std::iter::once(FFMPEG.to_string()).chain(argv).collect(),
    }
}

/// A line the run has to synthesise, and the file its take belongs in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToSpeak {
    /// Position in the record's order, so the bar counts the rows the way they are read.
    pub index: usize,
    pub text: String,
    pub wav: PathBuf,
}

/// The cue sheet: the master cues on the produced clock, and the languages the page ticked for S7.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cues {
    pub all: Vec<subs::Cue>,
    /// S7: the ticked translations, minus the session's own language (which is `all` already).
    pub languages: Vec<String>,
    /// The language the bare-stem track is already in. The page never offers it (§1 keeps that tick off the
    /// row), but a `naivepost.json` written by an older build can still carry it, and only this field lets the
    /// render notice: `plan` drops it from [`Self::languages`] so it costs neither an ask nor a file.
    pub session_language: String,
    /// S1 (§F5.4): the cues grouped per clip, in clip order, each on THAT clip's own seconds and already
    /// run through §S1's `an empty cue extends the previous`. Burned-in subtitles are lettered per clip
    /// inside the clip ([`crate::produce_subtitles_run::burn_file_for`]), so the encoder needs the per-clip
    /// answer,
    /// not the merged video-clock track — and `subs::tidy` deliberately folds cues that sit near each
    /// other, which must not happen across a clip boundary. `produced_clocks` supplies the offset each
    /// list is lifted by to reach the sheet in [`Self::all`], so the two cannot disagree about a span.
    pub per_clip: Vec<Vec<subs::Cue>>,
    /// Where each cue came from, counted by [`crate::produce_subtitles_run::count_cue_source`] as the
    /// clips were asked.
    pub counts: crate::produce_subtitles_run::CueCounts,
}

/// What the plan needs beyond the run's snapshot: the four lookups only a caller can answer, kept in one
/// struct so the walk's signature stays readable and no call site guesses an argument order.
pub struct Materials<'a> {
    /// S3: whether an insert's asset is still on disk. The caller resolves the cut-relative spelling;
    /// guessing at roots is not this module's business.
    pub exists: &'a dyn Fn(&str) -> bool,
    /// S3: the effects one segment carries, which is what S5's filter graph is built from.
    pub fx_for: &'a dyn Fn(&Seg) -> Vec<Fx>,
    /// S3: the project's lanes, asked per clip through `cut_hear`.
    pub lanes: &'a [Lane],
    /// The footage's own shape, which the frame box falls back to when the cut names no aspect.
    pub src_shape: (i32, i32),
    /// Where a narration line's synthesis already is — `None` when it still has to be spoken.
    pub wav_of: &'a dyn Fn(&Entry) -> Option<PathBuf>,
    /// F4.3: how long a line's synthesis IS, in seconds — `None` when there is no wav to measure.
    /// The render's speech length comes from here and nowhere else, so a line with no take takes no
    /// room. `plan` fills this from [`Materials::wav_of`] plus [`crate::voice_ref::wav_seconds`].
    pub wav_seconds: &'a dyn Fn(&Entry) -> Option<f64>,
    /// F4.3 step 1 ("grow the clip … never past the footage"): how many seconds of footage are
    /// still there past the end of this segment. A clip whose footage runs out after 1 s may be
    /// grown by 1 s, not by the full [`crate::produce_render::MAX_EXTEND_SECONDS`]. `0.0` means the
    /// caller does not know, and the grow step then keeps only its own ceiling.
    pub footage_left: &'a dyn Fn(&Seg) -> f64,
    /// S5: the file one clip reads its picture from. Footage and a `copy:` stretch read the run's first
    /// footage recording; an insert reads the asset named in the cut. The caller resolves the cut's own
    /// spelling (`project:` under the root, absolute elsewhere), exactly as it does for [`Materials::exists`]
    /// — this module never guesses at roots. `None` means the clip has nothing to open, and its encode is
    /// skipped rather than sent to ffmpeg with no input.
    pub source_file: &'a dyn Fn(&render::Clip) -> Option<String>,
    /// §8 bullet 4: does this session have tape at all? The render dips a join
    /// only where the sound can actually be read back off a recording, so the
    /// door that resolves each clip's source also answers whether there is one.
    pub on_recording: bool,
    /// The cues to write, and the languages to translate them into.
    pub cues: Cues,
    /// The project's source list, which the stamp weighs: `run.sources` is the run's own snapshot shape and
    /// the stamp wants `project::Source`s, so the caller who owns the project hands them over here rather
    /// than this module guessing at a conversion.
    pub sources: &'a dyn Fn() -> Vec<crate::project::Source>,
    /// S1 (§F5.4): the clip's OWN aligned words, on the clip's own recording clock — the narrator mic
    /// already excluded and the spellings already taken from the fixed transcript (`word_list`). Asked only
    /// where the clip has no narration line of its own and is footage; [`crate::produce_clip_cues`] asks
    /// that question, this answers where the words come from. An empty list means "this session has no
    /// aligned words for this clip", which yields no cues rather than a guess.
    pub words: &'a dyn Fn(&render::Clip) -> Vec<subs::Word>,
    /// S3 (§F5.4): the model leg for a translated track. Caller-supplied, as `narrate_tts::speak_line`
    /// supplies both of its network legs, so no test dials a socket. Its answers are what each `.code`
    /// sidecar carries; a refusal ships the original text with the merge's warning.
    pub translate: &'a crate::produce_translate::Ask,
    /// §F5.2 S5: what frame size an encoded clip's file came out at, read back off the file (ffprobe).
    /// The join is a stream copy and cannot mix sizes, so this is the only honest way to answer "does the
    /// video break at this clip" — bytes cannot say it. `None` means NO CLAIM about that clip (no ffprobe,
    /// a file the probe could not read), which contributes no mismatch rather than a false all-clear.
    pub probe: &'a dyn Fn(&Path) -> Option<(i32, i32)>,
    /// §6 bullet 1's planning refusals need two answers `plan_clips` does not carry: which filmed spans the
    /// session has (a `copy:` stretch and a stop are placed on SECONDS, not on "is there tape at all"), and
    /// what base a clip copies or reads its picture from, which §6's two "past the end of <base>" sentences
    /// name out loud. Empty spans mean no tape anywhere.
    pub filmed_runs: &'a [(f64, f64)],
    /// The name §6's "past the end of <base>" says: the footage row for footage and a `copy:` stretch, the
    /// asset's own file name for an insert. `Default` answers the empty name, which a test that never places
    /// a paste or a stop never reads.
    pub base_of: &'a dyn Fn(&Seg) -> String,
}

/// One clip's encode: its stem, the file it writes, the command that writes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipEncode {
    pub stem: String,
    pub file: PathBuf,
    pub command: Command,
}

/// One language's two files beside the video, with the text both carry.
#[derive(Debug, Clone, PartialEq)]
pub struct Sidecar {
    /// `None` for the session's own language, which takes the bare stem (§1: that is the track a player
    /// picks by default, and a second copy named after its own language says the same thing twice).
    pub language: Option<String>,
    pub srt: PathBuf,
    pub vtt: PathBuf,
    pub cues: Vec<subs::Cue>,
    pub log: String,
}

/// The whole run, decided before a subprocess starts: S1's clear, S2's speech, S3's clips, S4's cue
/// sheet, S5's encodes, S6's join, S7's translations, S8's mux and S9's sidecars, in that order.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// S2: whether this voice speaks at all (false for captions-only: the lines ride the subtitle track).
    pub speaking: bool,
    /// S2: captions only with Subtitles "none in the video" — the words are in the .srt beside it only.
    pub captions_warning: Option<&'static str>,
    /// S2: what has to be synthesised before the encoder starts.
    pub speech: Vec<ToSpeak>,
    /// S3: the clips the cut becomes.
    pub clips: Vec<render::Clip>,
    /// S3: the drops and their reasons, in clip-number order.
    pub plan_logs: Vec<String>,
    /// S4: the cue sheet on the produced clock.
    pub cues: Cues,
    /// S5: the file each clip is burned with its own cues, `None` where the page does not ask for burn-in
    /// ("track in file" and "none in the video" both letter nothing on the picture).
    pub burns: Vec<Option<PathBuf>>,
    /// S5: one encode per surviving clip.
    pub encodes: Vec<ClipEncode>,
    /// S6: the join, present only when at least one clip was encoded.
    pub join: Option<Command>,
    /// S8: loudness and mux.
    pub mux: Command,
    /// S7: the languages this run translates into, in the order S9 writes them.
    pub translated: Vec<String>,
    /// S9: the sidecars beside the video.
    pub sidecars: Vec<Sidecar>,
    /// §08 §6's webm note: true when this run had something to say but its container cannot carry a subtitle
    /// track. `walk` says [`crate::produce_details::WEBM_SRT_LOG`] after the mux, so the person learns that
    /// their subtitles are the files beside the video rather than missing from it.
    pub webm_without_track: bool,
    /// S10: the stamp, written only if the run comes back clean.
    pub stamp: String,
    pub video: PathBuf,
    pub scratch: PathBuf,
    /// Every subprocess of the run, in the order they start.
    pub commands: Vec<Command>,
}

impl Plan {
    /// The step names, in the order the commands arrive — a test pins the walk's order with this.
    pub fn steps(&self) -> Vec<&'static str> {
        self.commands.iter().map(|c| c.step).collect()
    }
}

/// A language a subtitle track can be named after: §C's codes are two letters (`en`, `de`, `fr`). A tick
/// that is not one describes a file this run never writes and must not delete — the same reason an unknown
/// stored subtitle mode reads as "none" rather than lettering somebody's picture.
fn is_track_code(code: &str) -> bool {
    // A language code, not a filename: any alphabetic tag names a sidecar the run owns a translation for
    // (`de`, `en`, and §C's three-letter forms alike). What it must not be is blank or punctuation, which
    // would name `final..srt` — a file no run writes and no run should delete.
    !code.is_empty()
        && code.bytes().all(|byte| byte.is_ascii_alphabetic())
}

/// The stems whose sidecar files this run owns — `final.<tick>` for every language the page ticked — which is
/// exactly the set of files it would overwrite, so exactly those are the stale ones that must go first. A tick
/// naming no track (blank, or a name `is_track_code` refuses) owns no file: leaving it is what "by exact name"
/// means when the name is not this run's.
///
/// The session's own stem is NEVER in the list. Its `.vtt` may well have been written by an earlier render,
/// left in place because a stamp let this press skip the encode (§F5.3), and F5.5's `<video>` page lists
/// whatever `.vtt` files are on disk with the video's own language first and `default`. Deleting it here
/// would silently drop that default track from the page over a press that wrote no subtitles at all.
fn sidecar_stems(ticked: &[String]) -> Vec<String> {
    let stem = render::final_srt_name().trim_end_matches(".srt").to_string();
    // Only translated tracks are cleared; see the note above about the session's own language.
    let mut stems: Vec<String> = Vec::new();
    stems.extend(
        crate::produce_subtitles::track_languages("", ticked)
            .into_iter()
            // The same `is_track_code` filter `plan` applies to the list it hands the translation leg, so the
            // sweep can only name a file some language of THIS run would have written.
            .filter(|code| is_track_code(code.trim()))
            .map(|code| format!("{stem}.{code}")),
    );
    stems
}

/// The produced file's stem: `final`, whichever container follows it.
pub fn video_stem() -> &'static str {
    "final"
}

/// The container's own extension, spelled the way the settings row spells it.
fn container_ext(settings: &Produce) -> &'static str {
    match settings.container {
        Container::Mp4 => "mp4",
        Container::Mkv => "mkv",
        Container::Webm => "webm",
    }
}

/// S1: the run clears the scratch folder. The clear itself is `render::clear_scratch`; what this module
/// adds is that it happens first, before anything can write into the folder, and that a missing folder is
/// made rather than treated as an error (a project that has never rendered has none).
pub fn clear_scratch(scratch: &Path) -> Result<usize, String> {
    if !scratch.exists() {
        std::fs::create_dir_all(scratch).map_err(|why| format!("{}: {why}", scratch.display()))?;
        return Ok(0);
    }
    render::clear_scratch(scratch)
}

/// S5's clip stem. The plan's own index rather than the clip's 1-based number, so the stems sort in play
/// order: the concat list is read in filename order and a stem that disagreed with playback order would
/// join the video wrong while every individual clip looked right. The run's stamp rides on the name —
/// §B's `c%03d_<stamp>` — because a leftover `c000_x.mp4` of an older cut would otherwise join into this
/// one by name alone.
fn stem_at(index: usize, stamp: &str) -> String {
    render::clip_stem(index, stamp)
}

// §S5's burned sheets and §S4's writes live in [`crate::produce_subtitles_run`]; this module only
// decides the order they happen in.

/// Each clip's cue list comes from [`crate::produce_clip_cues::build`], which reads the placed lines
/// and the clip's own words and asks §F5.4's first diamond in one place.
///
/// Move one of the sheet's cues onto the video's timeline timeline the way [`subs::on_clock`] moves a cue: the
/// clip's start plus each second shrunk by the clip's rate. Used instead of `subs::on_clock` because
/// the sheet's cues arrive with their seconds still relative to the clip and this walk already holds the
/// offset `produced_clocks` answered.
fn cue_on_clock(cue: &subs::Cue, produced_start: f64, rate: f64) -> subs::Cue {
    subs::Cue {
        s: crate::narration::output_seconds(cue.s, rate) + produced_start,
        e: crate::narration::output_seconds(cue.e, rate) + produced_start,
        text: cue.text.clone(),
        pos: cue.pos.clone(),
    }
}

/// S2: what still has to be spoken. Which lines a run is ALLOWED to speak is not this module's rule:
/// it comes through [`crate::narrate_off::lines_to_speak`], the one seam F4.8 names, so neither the
/// narration tick nor a blank line (a deliberate silence) can be missed here. The list that seam
/// returns is then asked of [`render::speak_job`] so the "which of these still need a take" rule stays
/// in one place, and each answer is turned into the file it lands in.
pub fn speech_plan(
    run: &Run,
    tree: &Tree,
    mats: &Materials,
    speaking: bool,
) -> Vec<ToSpeak> {
    if !speaking {
        return Vec::new();
    }
    // The seam first: narration off answers empty, and a blank line is never spoken. Everything below
    // works on THAT list, so no caller can speak a line the tick or a deliberate silence rules out.
    let speakable = crate::narrate_off::lines_to_speak(run.no_narration, &run.lines);
    match render::speak_job(false, &speakable, |entry| {
        (mats.wav_of)(entry).is_some_and(|wav| wav.is_file())
    }) {
        render::Speak::Nothing => Vec::new(),
        // The seam's OWN list is what gets spoken, walked in its own order. `index` is the position in
        // that list — which is the record with the dropped lines taken out — so a silent row disappears
        // without renumbering the survivors and two identical worded lines each keep their own take.
        render::Speak::Missing(_) => speakable
            .iter()
            .enumerate()
            .filter(|(_, entry)| !(mats.wav_of)(entry).is_some_and(|wav| wav.is_file()))
            .map(|(index, entry)| ToSpeak {
                index,
                text: entry.text.clone(),
                wav: (mats.wav_of)(entry)
                    .or_else(|| produce_stamp::wav_of(tree, entry, Some(&run.voice), 0))
                    .unwrap_or_else(|| tree.tts_wav("pending")),
            })
            .collect(),
    }
}

/// S2's bar line for one spoken line: the speaking job is `1/2`, so its count reads the way the rows are
/// read. Kept as one call into [`render::speaking_job_line`] rather than a formatted string here.
pub fn speaking_line(done: usize, total: usize) -> String {
    render::speaking_job_line(done, total)
}

/// S5: the length of speech each placed line carries — **the wav's own duration**, measured
/// from the file (§F4.3: "speech length = the wav's duration if it exists"). `plan_clips`
/// fits the lines with 0.0 because it places them before any wav's length is known; the real
/// take arrives by S2, so this is where the render measures it.
///
/// A line whose wav is missing keeps length 0 and therefore takes no room: its cue is held to
/// the next line or the clip's end, not to the span the row was written over. Nothing here
/// reads `entry.e - entry.s` any more — that was a stand-in for a measurement, and it handed a
/// silent line the room of a spoken one.
pub fn fill_speech(
    clips: &mut [render::Clip],
    entries: &[Entry],
    seconds_of: &dyn Fn(&Entry) -> Option<f64>,
) {
    for clip in clips.iter_mut() {
        for line in clip.lines.iter_mut() {
            let Some(entry) = entries.get(line.entry_index) else { continue };
            line.speech = seconds_of(entry).unwrap_or(0.0).max(0.0);
        }
    }
}

/// S3–S9, decided before anything runs. Every field is a call into the module that owns the rule; this
/// function only puts the calls in the spec's order and turns their answers into paths.
pub fn plan(run: &Run, tree: &Tree, mats: &Materials) -> Plan {
    let scratch = tree.clips_dir();
    let settings = &run.settings;
    // S1's clear and S5's writes both need the folder to exist; a project that has never rendered has none.
    let _ = std::fs::create_dir_all(&scratch);

    // S2: does this voice speak? `crate::narrate_screen::CAPTIONS` is the id the voice dropdown stores
    // for "No audio — captions only" (§1), so the stored id, not the row's label, is the question.
    // §1's `no_narration` flag short-circuits the same way, and the warning branches only where the lines
    // exist but nothing is spoken and no track carries them.
    let voice_is_captions = run.voice == crate::narrate_screen::CAPTIONS;
    let speaking = !voice_is_captions && !run.no_narration;
    let captions_warning = if speaking {
        None
    } else {
        render::captions_only_warning(settings.subtitles)
    };
    let speech: Vec<ToSpeak> = speech_plan(run, tree, mats, speaking);

    // S3: one clip per segment, after the speed splits, with every drop already named. The split is
    // `cut_speed_split::sped_segs`, which cuts footage at each rate boundary and stamps the averaged, ramped rate —
    // without it the planner sees an unstamped list and every clip comes out at ×1 however many speeds are on the
    // lane. Clip numbers follow THIS list, so "clip 7" means the same here as in the log and on the Cut page.
    let sped = crate::cut_speed_split::sped_cut(&run.cut);
    let (clips, plan_logs) = render::plan_clips(
        &sped,
        mats.fx_for,
        &run.aspect,
        mats.exists,
        mats.lanes,
        &run.lines,
        mats.src_shape,
        mats.on_recording,
        // F4.3 step 1's footage bound: how far past this clip's end the tape actually runs.
        mats.footage_left,
    );

    // §6 bullet 1: the clip-planning refusals §6 spells but `plan_clips` never said. Same sped list, same
    // clip numbers, so "clip 7" means clip 7 here as in every other line of this run.
    let refusal_logs = crate::produce_plan_logs::refusals(
        &sped,
        mats.fx_for,
        &run.lines,
        &crate::produce_plan_logs::Doors {
            lanes: mats.lanes,
            runs: mats.filmed_runs,
            base_of: mats.base_of,
            wav_of: mats.wav_of,
            exists: mats.exists,
            footage_left: mats.footage_left,
        },
    );

    // Speech lengths before the cues are cut, so a cue spans what will actually be heard on it.
    let mut clips = clips;
    fill_speech(&mut clips, &run.lines, mats.wav_seconds);
    // §4's effects need two things the planner cannot guess: where their files go and what the footage looks like.
    // The scratch folder is this run's `produce/clips/`, already created above; the shape came in with `Materials`.
    for clip in clips.iter_mut() {
        clip.scratch = scratch.clone();
        clip.on_recording = mats.on_recording;
    }

    // S4: the cues on the produced clock. Each clip's lines become cues on its own clock and are moved
    // onto the video's with the offsets `produced_clocks` answers, then tidied once for the whole track:
    // `tidy` compares cues with each other, so it runs across clips and not per clip.
    // §S4's rule is that the stale sidecars go before ANY of this run's writes — including the scratch
    // folder's own sheet, which is why this happens here rather than inside `walk`, whose first step is
    // `clear_scratch`. The stems are the tracks this run writes beside the video: the session's own language
    // bare, plus `.code` for every language the page ticked. A language nobody ticked keeps its file — it
    // describes a translation somebody asked for and this run was not told to drop it.
    // The bare-stem pair is swept only when this run has cues to rewrite it with. An up-to-date video lets a
    // press skip the encodes but not the sheet, and F5.5's `<video>` page lists whatever `.vtt` files are on
    // disk with the video's own language first and `default`, so deleting the bare pair on a run that writes
    // nothing would drop the default track over a press that said nothing.
    // §5's "never an empty cue" and §S4's "stale ones deleted first" meet here: the sweep may only clear the
    // bare-stem pair if this run has cues to replace it with. Whether it does is `sheet_will_have_cues`, which
    // `plan` answers a little later from the same clip list, so the two cannot disagree.
    let sheet_will_have_cues = clips.iter().any(|clip| {
        !crate::produce_clip_cues::build(clip, &clip.lines, &run.lines, &(mats.words)(clip)).is_empty()
    });
    // The bare-stem pair is swept only when this run has cues to rewrite it with. An up-to-date video lets a
    // press skip the encodes but not the sheet, and F5.5's `<video>` page lists whatever `.vtt` files are on
    // disk with the video's own language first and `default`, so deleting the bare pair on a run that writes
    // nothing would drop the default track over a press that said nothing.
    let bare_stem: &[String] = if sheet_will_have_cues {
        &[render::final_srt_name().trim_end_matches(".srt").to_string()]
    } else {
        &[]
    };
    let sheet_sides = sidecar_stems(&mats.cues.languages)
        .into_iter()
        .chain(bare_stem.iter().cloned())
        .collect::<Vec<String>>();
    crate::produce_subtitles_run::clear_stale_sidecars(
        // The folder beside the video is where the sidecars live; `video` is this run's own file there.
        &tree.final_video(container_ext(settings)),
        &scratch,
        &sheet_sides,
    );

    let mut cues = Cues {
        all: Vec::new(),
        // §1's second door. The page hides the session's own language; a setting that carries it anyway must
        // still cost no ask and no `final.<own>.srt`, which is what `track_languages` answers. A blank tick
        // names no track at all, and `is_track_code` is the same rule `sidecar_stems` applies to the stale
        // sweep, so both ends of the list agree.
        languages: crate::produce_subtitles::track_languages(
            &mats.cues.session_language,
            &mats.cues.languages,
        )
        .into_iter()
        .filter(|code| is_track_code(code.trim()))
        .collect(),
        // The walk re-reads this list through `plan.translated`, and the stale sweep must use the SAME answer
        // or a tick that names no track would sweep files the run never owns.
        ..Default::default()
    };
    for (clip, (start, _len)) in clips.iter().zip(render::produced_clocks(&clips).iter()) {
        // S1 (§F5.4): the clip's lines if it has any, else its own speech where it is footage. The kind
        // of clip and the branch order are `produce_clip_cues`' answer, read off the fields the render
        // itself filled, and the cue rule is `produce_subtitles::cues`'.
        let mine = crate::produce_clip_cues::build(clip, &clip.lines, &run.lines, &(mats.words)(clip));
        if mine.is_empty() {
            continue;
        }
        // S1's last clause runs here rather than inside `subs::cues`, so a clip's own words and a clip's
        // written lines get it the same way: a cue with nothing on it lengthens the caption already up.
        let mut mine = mine;
        subs::extend_previous(&mut mine);
        // Which half of S1 this clip answered, counted before anything is folded or moved.
        crate::produce_subtitles_run::count_cue_source(
            &mut cues.counts,
            &mine,
            clip.lines.len(),
        );
        cues.all.extend(mine.iter().map(|c| cue_on_clock(c, *start, clip.rate)));
        cues.per_clip.push(mine);
    }
    cues.all = subs::tidy(&cues.all);

    // S10: the stamp this run would write if it comes back clean. Computed BEFORE the encodes because the
    // clip stems carry it (§B's `c%03d_<stamp>`): the file names are part of what this run writes, so the
    // answer has to be in hand before the first encode command is built.
    let sources_now = (mats.sources)();
    let stamp_input = produce_stamp::Input {
        settings,
        segs: &run.cut.segs,
        lines: &run.lines,
        sources: &sources_now,
        aspect: &run.aspect,
        voice: &run.voice,
        no_narration: run.no_narration,
        // The three the encoder reads beside the segments, taken from the same hands that feed it: `mats.fx_for`
        // is answered from `run.cut.fx` and the bed list from `run.cut.lanes`, and `plan_clips` builds every
        // frame box on `mats.src_shape`. A stamp that missed them could skip a render that had never seen them.
        fx: &run.cut.fx,
        lanes: &run.cut.lanes,
        shape: mats.src_shape,
    };
    let stamp = stamp_input.stamp_with(tree, Some(&run.voice), 0);

    // S5: one encode per clip. No burned-in sheet is passed: `Subtitles::BurnedIn`'s overlay is F5.4's
    // own rendering round, and the plan carries the cues it would be drawn from.
    let mut effect_logs: Vec<String> = Vec::new();
    let encodes: Vec<ClipEncode> = clips
        .iter()
        .enumerate()
        .map(|(index, clip)| {
            let stem = stem_at(index, &stamp);
            let file = scratch.join(format!("{stem}.{}", container_ext(settings)));
            // The picture's own file goes in as `[0:v]`/`[0:a]`, the inputs the filter graph names.
            // Clip 0 gets no `-ss` at all because `encode_command` omits it when `seg_s == 0.0`, which is
            // right and not something a test should expect to find there.
            let source = (mats.source_file)(clip);
            let mut inputs: Vec<(&str, Option<&str>)> = match source.as_deref() {
                Some(path) => vec![("video", Some(path))],
                None => Vec::new(),
            };
            // Set when this clip reads a baked card that has no own length; see `produce_cards::Staged::loops`.
            let mut loops_card = false;
            // Kept beside `inputs` because the card's paths are invented here and must outlive the block that made them.
            let mut card_paths: Vec<(&str, String)> = Vec::new();
            // §4: whatever this clip's effects read has to exist before ffmpeg is asked for it — a generated title
            // sheet is only an input once it is on disk. One call per clip; the same answer builds the filters.
            let built = crate::produce_fx_chain::for_clip(clip, &clip.fx, clip.src, &clip.scratch);
            for (path, text) in &built.title_files {
                if let Err(err) = std::fs::write(path, text) {
                    eprintln!("produce: {}: {err}", path.display());
                }
            }
            for (kind, path) in built.inputs.iter() {
                inputs.push((kind.as_str(), path.as_deref()));
            }
            // §5: a card is not read as the file it is. Its holes are filled (or it is redrawn from its generator),
            // and an animated one is baked to one static SVG per frame at the bake fps — all of that is
            // `produce_cards`' decision; this writes what it produced into the scratch folder BEFORE the command is
            // built, exactly as the title sheets above are, and hands ffmpeg the frames or the one filled file.
            if let Some(mut card_input) = crate::produce_cards::stage(clip, source.as_deref()) {
                effect_logs.extend(card_input.logs.iter().cloned());
                for (path, text) in &card_input.files {
                    if let Err(err) = std::fs::write(path, text) {
                        eprintln!("produce: {}: {err}", path.display());
                    }
                }
                for (kind, path) in std::mem::take(&mut card_input.inputs) {
                    // `stage` answers with one of two kinds, so the spelling is fixed here rather than carried as a
                    // `String` the caller would have to borrow from a value that is about to be dropped.
                    let kind: &'static str = if kind == "card" { "card" } else { "svg" };
                    card_paths.push((kind, path));
                }
                // A card with no own length (an indefinite repeat) runs its frames again until the slot is full.
                if card_input.loops {
                    loops_card = true;
                }
            }
            for (kind, path) in card_paths.iter() {
                inputs.push((*kind, Some(path.as_str())));
            }
            // The > 10× cap and a missing drawing are said here, beside the clip they belong to, so the render log
            // reads in clip order like every other line about a clip.
            effect_logs.extend(built.logs.iter().cloned());
            // §F5.2 S5 names the burned sheet as one of the clip's inputs and §B lists it beside the
            // encode (`c%03d_<stamp>.srt`). Only "burned in" asks for it, and what it letters is the
            // CLIP's own cues on the clip's own seconds — the encode cuts that span out of the recording
            // and never sees the video clock, so a lifted cue list would be seconds away from every
            // caption on it. `plan_burns` writes the file beside the plan, from the same name.
            let burn = crate::produce_subtitles_run::burn_file_name(index, &stamp, settings);
            let argv = render::encode_command_with(
                clip,
                settings,
                &inputs,
                burn.as_deref(),
                &file,
                loops_card,
            );
            ClipEncode {
                stem,
                file,
                command: command("clip", argv),
            }
        })
        .collect();
    // §4's cap-and-log line joins the plan's own lines so a reader sees it with the clip's other news.
    let plan_logs: Vec<String> = plan_logs
        .into_iter()
        .chain(refusal_logs)
        .chain(effect_logs)
        .collect();
    let join = if encodes.is_empty() {
        None
    } else {
        let list = scratch.join("concat.txt");
        Some(command(
            render::JOINING,
            // §6 names this file `joined.<container>` — the same container the run writes `final.<ext>`
            // under, so the scratch join and the finished video agree about their shape. One expression
            // feeds both the writer here and the mux's input below, so the two cannot disagree.
            render::join_command(&list, &scratch.join(details::joined(container_ext(settings)))),
        ))
    };

    // S8: the mux. Whether subtitle tracks go in is the container rule inside `mux_command`, so the
    // settings row and the command cannot disagree about what webm carries.
    // §6: EVERY subtitle track is its own input, tagged with its ISO-639-2 code and name — so a run that
    // translated into German muxes the session's own sheet AND the German one, not one untitled stream.
    // The list is the same one S9 writes sidecars from (own language bare first, then each ticked code that
    // names a track), which is what keeps the file beside the video and the stream inside it one fact.
    // Paths are the ones the sidecar step below writes to; a track whose file this run never writes would be
    // an `-i` ffmpeg has to fail on, so an empty cue list means no tracks at all.
    let (_, carried) = crate::produce_screen::apply_container_rules(
        settings.container,
        settings.codec,
        settings.subtitles,
    );
    let sheet_path = scratch.join(render::final_srt_name());
    let mut tracks: Vec<(String, PathBuf)> = Vec::new();
    if carried == crate::project::Subtitles::TrackInFile && !cues.all.is_empty() {
        tracks.push((cues.session_language.clone(), sheet_path.clone()));
        for code in cues
            .languages
            .iter()
            .filter(|code| is_track_code(code.trim()))
        {
            tracks.push((code.clone(), tree.final_srt(Some(code))));
        }
    }
    let track_refs: Vec<(&str, &Path)> =
        tracks.iter().map(|(code, path)| (code.as_str(), path.as_path())).collect();
    // The tag/name pair comes from the app's own language list (`SUBTITLE_LANGUAGES`, 03-shell §6), read
    // here rather than inside `mux_command` so that stays a pure argv builder. No settings file = no list,
    // and every track then falls back the way §6 spells it: `und`, named by its upper-cased code.
    let known_languages = crate::roles::subtitle_languages(
        &crate::settings::from_environment()
            .and_then(|paths| crate::settings::read(&paths).ok())
            .map(|conf| conf.subtitle_languages)
            .unwrap_or_default(),
    );
    let joined = scratch.join(details::joined(container_ext(settings)));
    let mux = command(
        "muxing",
        render::mux_command(
            &joined,
            &joined,
            &track_refs,
            settings,
            &tree.final_video(container_ext(settings)),
            &known_languages,
        ),
    );

    // S9: the sidecars. The session's own language takes the bare stem; each ticked language takes
    // `.code`, and each of them gets both files (§1: the .vtt is the one a browser needs).
    let mut sidecars: Vec<Sidecar> = Vec::new();
    if !cues.all.is_empty() {
        sidecars.push(Sidecar {
            language: None,
            srt: tree.final_srt(None),
            vtt: tree.final_vtt(None),
            cues: cues.all.clone(),
            log: render::sidecar_log(video_stem(), None),
        });
        // One sidecar per language the run translates into. A tick that names no track (blank) writes nothing
        // rather than a file named `final..srt`, and a name that is not a §C code owns no file either — see
        // `is_track_code`. What the page's own list can never contain is the session's language: §1 keeps it
        // off the row, and `track_languages` is the second door for a setting written elsewhere.
        for code in cues
            .languages
            .iter()
            .filter(|code| is_track_code(code.trim()))
        {
            sidecars.push(Sidecar {
                language: Some(code.clone()),
                srt: tree.final_srt(Some(code)),
                vtt: tree.final_vtt(Some(code)),
                cues: Vec::new(),
                log: render::sidecar_log(video_stem(), Some(code)),
            });
        }
    }

    let commands: Vec<Command> = encodes
        .iter()
        .map(|e| e.command.clone())
        .chain(join.clone())
        .chain(std::iter::once(mux.clone()))
        .collect();

    Plan {
        speaking,
        captions_warning,
        speech,
        clips,
        plan_logs,
        cues: cues.clone(),
        // One entry per encode, in the same order: `None` wherever the page did not ask for burn-in or the
        // clip had nothing to letter.
        burns: crate::produce_subtitles_run::write_burn_files(
            &cues.per_clip,
            &stamp,
            settings,
            &scratch,
        ),
        encodes,
        join,
        mux,
        translated: cues.languages.clone(),
        sidecars,
        // §6: "after a webm mux: webm cannot carry an srt track …". The note belongs to a run that HAD cues:
        // with nothing to say there is no track anywhere and nothing to explain.
        webm_without_track: settings.container == Container::Webm && !cues.all.is_empty(),
        stamp,
        video: tree.final_video(container_ext(settings)),
        scratch,
        commands,
    }
}

// --- the walk ------------------------------------------------------------------------------------

/// How far the run got, and what it may still write.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ran {
    /// S1: files that were in the scratch folder and are now gone.
    pub cleared: usize,
    /// S2: lines the caller synthesised.
    pub spoken: usize,
    /// S2: lines that would not synthesise — logged, not fatal.
    pub speech_failed: usize,
    /// S3: what was dropped, with the reason.
    pub logs: Vec<String>,
    /// S5: the clips encoded, by stem.
    pub encoded: Vec<String>,
    /// S6 / S8: how far past the encodes the run came.
    pub joined: bool,
    pub muxed: bool,
    /// S9: the sidecars written, by .srt path.
    pub sidecars: Vec<String>,
    /// The checkpoint that stopped the run, if one did.
    pub stopped_at: Option<&'static str>,
    /// The step that failed, with what it said.
    pub failed: Option<String>,
}

impl Ran {
    /// The only case that writes a stamp: the mux finished, nothing failed, nobody stopped it.
    pub fn clean(&self) -> bool {
        self.muxed && self.stopped_at.is_none() && self.failed.is_none()
    }
}

/// Drain the plan in order, asking the checkpoint between the tools. `ask` answers false once the run has
/// been stopped; the walk halts at the first false and records where, so a stop never lands in the middle
/// of a subprocess and never leaves a half-written deliverable that the next press would mistake for a
/// finished one.
///
/// A line that will not synthesise is logged and skipped rather than fatal: a missing take costs the
/// listener one silent sentence, which is a good deal less than a failed render costs them.
pub fn walk<FLog, FSpeak, FAsk, FSpawn>(
    plan: &Plan,
    mats: &Materials,
    mut log: FLog,
    mut speak: FSpeak,
    mut ask: FAsk,
    mut spawn: FSpawn,
) -> Ran
where
    FLog: FnMut(&str),
    FSpeak: FnMut(&ToSpeak) -> Result<(), String>,
    FAsk: FnMut(At, &'static str) -> bool,
    FSpawn: FnMut(&Command) -> Result<(), String>,
{
    let mut ran = Ran::default();

    // S1 — the scratch folder, before anything can write into it. The count of §3 + §6 scratch groups
    // rides on the line so the log says what kinds the sweep covers, not only how many files were there.
    match render::clear_scratch(&plan.scratch) {
        Ok(gone) => {
            ran.cleared = gone;
            log(&crate::produce_data::scratch_clear_log(crate::produce_data::scratch_groups(), gone));
        }
        Err(why) => {
            ran.failed = Some(why);
            return ran;
        }
    }

    // S2 — the spoken half of the bar's first half.
    if let Some(warning) = plan.captions_warning {
        log(warning);
    }
    for line in &plan.speech {
        if !ask(At::BeforeTools, "after each line spoken") {
            ran.stopped_at = Some("after each line spoken");
            return ran;
        }
        match speak(line) {
            Ok(()) => {
                ran.spoken += 1;
                log(&speaking_line(ran.spoken, plan.speech.len()));
            }
            Err(why) => {
                log(&render::synthesis_failure_log(ran.spoken as f64, &why));
                ran.speech_failed += 1;
            }
        }
    }

    // S3 — the plan's own news: what it dropped and why, in clip-number order.
    for line in &plan.plan_logs {
        log(line);
    }
    ran.logs = plan.plan_logs.clone();
    if plan.clips.is_empty() {
        log(render::no_clip_log());
        ran.failed = Some(render::no_clip_log().to_string());
        return ran;
    }

    // S4 — the stale sidecars go first, by exact name, whether or not this run has cues: a cut that lost
    // its narration since the last render must not keep last render's subtitle files beside its video.
    // Then this run's sheet, and the refusal when there is nothing to write.
    // The stale sidecars already went, by exact name, in `plan` — before the scratch folder was cleared,
    // because §S4 puts the delete ahead of every write this run makes. What is left here is the run's own
    // sheet, or the refusal to write one.
    let sheet = plan.scratch.join(render::final_srt_name());
    if plan.cues.all.is_empty() {
        // §5: the video is finished and correct; it is the file beside it that will not exist.
        log(render::no_subtitle_cue_log());
    } else {
        log(&crate::produce_subtitles_run::cue_count_log(
            plan.cues.all.len(),
            &plan.cues.counts,
        ));
        if let Err(why) = std::fs::write(&sheet, subs::srt_text(&plan.cues.all)) {
            ran.failed = Some(format!("{sheet}: {why}", sheet = sheet.display()));
            return ran;
        }
    }

    // S5 — one encode per clip, its command logged before it runs rather than after: the command that
    // hung is the one worth reading, and there is no "after" for it.
    for encode in &plan.encodes {
        log(&encode.command.log);
        match spawn(&encode.command) {
            Ok(()) => ran.encoded.push(encode.stem.clone()),
            Err(why) => {
                ran.failed = Some(format!("clip {}: {why}", encode.stem));
                return ran;
            }
        }
    }

    // S6 — the join, by stream copy, behind the checkpoint that keeps a stop off the middle of it.
    if !ask(At::BeforeJoin, render::JOINING) {
        ran.stopped_at = Some(render::JOINING);
        return ran;
    }
    if let Some(join) = &plan.join {
        let list = plan.scratch.join("concat.txt");
        let body = render::concat_list(
            &plan.encodes
                .iter()
                .map(|e| e.file.display().to_string())
                .collect::<Vec<String>>(),
        );
        if let Err(why) = std::fs::write(&list, format!("{body}\n")) {
            ran.failed = Some(format!("{}: {why}", list.display()));
            return ran;
        }
        log(&join.log);
        if let Err(why) = spawn(join) {
            ran.failed = Some(format!("{}: {why}", render::JOINING));
            return ran;
        }
        ran.joined = true;
        // S5's other rule, measured AFTER the encodes because only the encoder knows what it wrote: the
        // join is a copy and cannot mix sizes, so a clip that came out at another FRAME SIZE breaks the
        // video at exactly that clip. Sizes are probed off the files (`Materials::probe`, ffprobe behind
        // the program's own door), never guessed from byte counts — two clips at one size differ in bytes
        // every time. The base is the first clip that yields a size, so a session with no ffprobe probes
        // everything to `None`, claims nothing, and says nothing: absence is not an all-clear.
        let sizes: Vec<Option<(i32, i32)>> =
            plan.encodes.iter().map(|encode| (mats.probe)(&encode.file)).collect();
        if let Some(base) = sizes.iter().flatten().next().copied() {
            for (encode, size) in plan.encodes.iter().zip(sizes.iter()).skip(1) {
                // A clip the probe could not read contributes no claim; a clip that disagrees with the
                // first readable size is the one the copy join breaks at.
                if size.is_some() && *size != Some(base) {
                    log(&format!("{}: {}", encode.stem, render::JOIN_MISMATCH));
                }
            }
        }
    }

    // S7 + S8 — one checkpoint for both, and the model gate first: the translation is the slow thing,
    // and putting it before the mux is what stops the encoder idling behind it.
    if !ask(At::BeforeModelAndMux, "before translate and mux") {
        ran.stopped_at = Some("before translate and mux");
        return ran;
    }
    // S7 — the walk asks each ticked language once per batch. `plan.translated` came from
    // `Cues::languages`, which is already the ticked list minus the session's own (`track_languages`
    // drops it), so nothing here is translated into itself and the bare-stem track stays the original.
    let tracked = crate::produce_translate::track(
        &plan.cues.all,
        // The same language `plan` filtered on, so the two doors cannot disagree about what "the video's own
        // language" means.
        &plan.cues.session_language,
        &plan.translated,
        mats.translate,
        // The walk's own log: every word the translation half says lands in the run's log, beside the ask it
        // explains, and nowhere else.
        |line| log(line),
    );
    for code in &plan.translated {
        log(&render::translate_log(plan.cues.all.len(), code));
    }
    // S3: a line that stayed in the source language is said out loud, not left for a viewer to notice.
    for t in tracked.iter().filter(|t| t.warning.is_some()) {
        log(t.warning.as_deref().unwrap_or_default());
    }
    log(&plan.mux.log);
    match spawn(&plan.mux) {
        Ok(()) => ran.muxed = true,
        Err(why) => {
            ran.failed = Some(format!("mux: {why}"));
            return ran;
        }
    }
    // §6: webm cannot carry an srt track, and the run said so AFTER the mux — the video is finished and
    // correct by then; what the person needs to know is where their subtitles are instead (the sidecars S9
    // writes beside it, which come next).
    if plan.webm_without_track {
        log(crate::produce_details::WEBM_SRT_LOG);
    }

    // S9 — the sidecars beside the video, each one logged. A `.code` track carries the TRANSLATED cues
    // (never a copy of the session's track, which is what made a German file say English); the bare-stem
    // one keeps the originals, since that is the track the viewer hears.
    let by_code: BTreeMap<String, Vec<subs::Cue>> = tracked
        .into_iter()
        .map(|t| (t.language.clone(), t.cues))
        .collect();
    for sidecar in &plan.sidecars {
        // A `.code` sidecar must hold the translation the walk got back. When the translation leg could not
        // be reached at all — a server down, a settings file unreadable — `track` still returns that language
        // with the ORIGINAL cues and a warning, so the file is written from the plan's own track rather than
        // from an empty cue list: an empty sidecar beside a finished video is the one answer §5 forbids.
        let out = match &sidecar.language {
            None => sidecar.clone(),
            Some(code) => match by_code.get(code) {
                Some(cues) if !cues.is_empty() => Sidecar { cues: cues.clone(), ..sidecar.clone() },
                _ => Sidecar { cues: plan.cues.all.clone(), ..sidecar.clone() },
            },
        };
        if let Err(why) = write_sidecar(&out) {
            ran.failed = Some(why);
            return ran;
        }
        log(&out.log);
        ran.sidecars.push(out.srt.display().to_string());
    }

    ran
}

/// S9: both files for one language, written from the same cue list so the two cannot disagree about a
/// span. `subs` owns both formatters and neither re-derives a time.
fn write_sidecar(sidecar: &Sidecar) -> Result<(), String> {
    for (path, text) in [
        (&sidecar.srt, subs::srt_text(&sidecar.cues)),
        (&sidecar.vtt, subs::vtt_text(&sidecar.cues)),
    ] {
        std::fs::write(path, text).map_err(|why| format!("{}: {why}", path.display()))?;
    }
    Ok(())
}

/// S10: the stamp, written only by the run that made the file. `stamp_written` is the rule (an encode
/// that did not happen, or a run that failed, writes none); this only asks it and does the writing.
pub fn write_stamp(plan: &Plan, tree: &Tree, ran: &Ran, mut log: impl FnMut(&str)) -> bool {
    // S10: the stamp is the promise that this file matches these settings, so only the run that finished
    // writing it may make that promise. A stopped run has a joined file and no deliverables beside it, so
    // `clean()` (which requires the mux) outranks `stamp_written` here: without it, an encode that got
    // partway would hide a stale render from the next press.
    if !ran.clean() || !render::stamp_written(!ran.encoded.is_empty(), ran.failed.is_some()) {
        return false;
    }
    // §6: an unwritable stamp is SAID, not swallowed — the next ▶ encodes again and the person should know why.
    // It still never fails the run: the video is finished, only the promise that it is up to date is missing.
    match produce_stamp::write_stamp(tree, &plan.stamp) {
        Ok(()) => {
            // Said once per clean render: the hash goes in because these lines are compared between runs, and a
            // stale stamp somebody wants to delete has to be findable by eye.
            log(&produce_stamp::wrote_stamp_log(&plan.stamp));
            true
        }
        Err(why) => {
            log(&details::stamp_unwritable_log(&why));
            false
        }
    }
}

/// The produced file's size, or 0 when it is not there yet.
fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// The bar's size spelling: MB above a megabyte, KB below.
fn human_size(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    if bytes == 0 {
        return "0 B".to_string();
    }
    if bytes < MIB {
        return format!("{} KB", bytes.div_ceil(1024));
    }
    format!("{} MB", (bytes + MIB / 2) / MIB)
}

// --- the program's spawner ------------------------------------------------------------------------

/// The program every tool step runs: `ffmpeg` resolved through PATH. §10 gives no `P.machine.*` row for
/// an explicit path — `params.rs` names ffmpeg only in filter wording — so this constant is the only
/// place the name is written, and a row added later replaces it here rather than in twelve call sites.
pub const FFMPEG: &str = "ffmpeg";

/// The program's spawner: [`crate::subprocess::run`], which logs the command as a shell would take it
/// before running it and carries the failing output inside the error.
pub fn spawn_tool(command: &Command) -> Result<(), String> {
    let args: Vec<&str> = command.args().iter().map(|a| a.as_str()).collect();
    crate::subprocess::run(command.program(), &args, |line| {
        crate::ui::window::log_line(line)
    })
    .map(|_| ())
}

/// The one call a render half makes: plan, walk, stamp, report. `log`, `speak`, `ask` and `spawn` come
/// from the caller, so the GUI thread decides what runs where and a test decides what runs at all.
///
/// `tag_ok` is the stamp's answer rather than a second verdict: F5.1 S5 treats the tag page as an input
/// to the ending, never as the verdict, and that is what this field feeds.
pub fn run_render<FLog, FSpeak, FAsk, FSpawn>(
    run: &Run,
    tree: &Tree,
    mats: &Materials,
    mut log: FLog,
    speak: FSpeak,
    ask: FAsk,
    spawn: FSpawn,
) -> crate::produce_flow::Rendered
where
    FLog: FnMut(&str),
    FSpeak: FnMut(&ToSpeak) -> Result<(), String>,
    FAsk: FnMut(At, &'static str) -> bool,
    FSpawn: FnMut(&Command) -> Result<(), String>,
{
    let started = std::time::Instant::now();
    let plan = plan(run, tree, mats);
    let ran = walk(&plan, mats, &mut log, speak, ask, spawn);
    let stamp_ok = write_stamp(&plan, tree, &ran, &mut log);
    if let Some(why) = &ran.failed {
        log(&format!("produce FAILED: {why}"));
    }
    let seconds = started.elapsed().as_secs_f64();
    let size = human_size(file_size(&plan.video));
    crate::produce_flow::Rendered {
        ok: ran.clean(),
        tag_ok: stamp_ok,
        stopped: ran.stopped_at.is_some(),
        seconds,
        size,
    }
}
