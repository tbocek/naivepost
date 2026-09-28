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

use std::path::{Path, PathBuf};

use crate::cut::{Fx, Lane, Seg};
use crate::layout::Tree;
use crate::narration::Entry;
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
    /// S5: the file one clip reads its picture from. Footage and a `copy:` stretch read the run's first
    /// footage recording; an insert reads the asset named in the cut. The caller resolves the cut's own
    /// spelling (`project:` under the root, absolute elsewhere), exactly as it does for [`Materials::exists`]
    /// — this module never guesses at roots. `None` means the clip has nothing to open, and its encode is
    /// skipped rather than sent to ffmpeg with no input.
    pub source_file: &'a dyn Fn(&render::Clip) -> Option<String>,
    /// The cues to write, and the languages to translate them into.
    pub cues: Cues,
    /// The project's source list, which the stamp weighs: `run.sources` is the run's own snapshot shape and
    /// the stamp wants `project::Source`s, so the caller who owns the project hands them over here rather
    /// than this module guessing at a conversion.
    pub sources: &'a dyn Fn() -> Vec<crate::project::Source>,
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

/// The produced file's stem: `final`, whichever container follows it.
fn video_stem() -> &'static str {
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

/// S4's cue for one narration line: the seconds it is up, and what is said on it.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipCue {
    pub s: f64,
    pub e: f64,
    pub text: String,
}

/// The clip's cue list: the narration lines the plan placed on it, turned into cues on the clip's own
/// clock. A line with no take gets the half-second floor, because S4's `tidy` folds a zero-length cue
/// into the next one and a silent line should not vanish from the sheet.
fn clip_cues(clip: &render::Clip) -> Vec<ClipCue> {
    clip.lines
        .iter()
        .map(|line| ClipCue {
            s: line.at,
            e: line.at + line.speech.max(0.5),
            text: line.text.clone(),
        })
        .collect()
}

/// Move a [`ClipCue`] onto the video's timeline the way `produce_subtitles::on_clock` moves a cue: the
/// clip's start plus each second shrunk by the clip's rate. Kept here rather than in `subs` because the
/// render's own cue carries the narration's text, which `subs::Cue` has no reason to know about.
fn cue_on_clock(cue: &ClipCue, produced_start: f64, rate: f64) -> subs::Cue {
    subs::Cue {
        s: crate::narration::output_seconds(cue.s, rate) + produced_start,
        e: crate::narration::output_seconds(cue.e, rate) + produced_start,
        text: cue.text.clone(),
        pos: String::new(),
    }
}

/// S2: what still has to be spoken, asked of [`render::speak_job`] so the rule stays in one place, and
/// turned into the files each line lands in. A captions-only voice answers [`render::Speak::Nothing`], which
/// is why nothing here synthesises for it.
fn speech_plan(
    run: &Run,
    tree: &Tree,
    mats: &Materials,
    speaking: bool,
) -> Vec<ToSpeak> {
    if !speaking {
        return Vec::new();
    }
    match render::speak_job(false, &run.lines, |entry| {
        (mats.wav_of)(entry).is_some_and(|wav| wav.is_file())
    }) {
        render::Speak::Nothing => Vec::new(),
        render::Speak::Missing(_) => run
            .lines
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                !entry.text.trim().is_empty()
                    && !(mats.wav_of)(entry).is_some_and(|wav| wav.is_file())
            })
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

/// S5: the length of speech each placed line carries. `plan_clips` fits the lines against the room they
/// have without knowing any wav's length (it builds `(index, at, 0.0)`), and the real take is only known
/// once S2 has spoken — so the render fills the gap with the span the row was written to fill, which is
/// F4.3's rule seen from this side: a line occupies the seconds between its own bounds. Without this the
/// bed would never be turned down (`audio_chain` skips a zero-length line) and every cue would fall to the
/// half-second floor.
pub fn fill_speech(clips: &mut [render::Clip], entries: &[Entry]) {
    for clip in clips.iter_mut() {
        for line in clip.lines.iter_mut() {
            if line.speech > 0.0 {
                continue;
            }
            if let Some(entry) = entries.get(line.entry_index) {
                line.speech = (entry.e - entry.s).max(0.0);
            }
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

    // S3: one clip per segment, after the speed splits, with every drop already named.
    let (clips, plan_logs) = render::plan_clips(
        &run.cut.segs,
        mats.fx_for,
        &run.aspect,
        mats.exists,
        mats.lanes,
        &run.lines,
        mats.src_shape,
    );

    // Speech lengths before the cues are cut, so a cue spans what will actually be heard on it.
    let mut clips = clips;
    fill_speech(&mut clips, &run.lines);

    // S4: the cues on the produced clock. Each clip's lines become cues on its own clock and are moved
    // onto the video's with the offsets `produced_clocks` answers, then tidied once for the whole track:
    // `tidy` compares cues with each other, so it runs across clips and not per clip.
    let mut cues = Cues {
        all: Vec::new(),
        languages: mats.cues.languages.clone(),
    };
    for (clip, (start, _len)) in clips.iter().zip(render::produced_clocks(&clips).iter()) {
        let mine = clip_cues(clip);
        if !mine.is_empty() {
            cues.all.extend(mine.iter().map(|c| cue_on_clock(c, *start, clip.rate)));
        }
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
    };
    let stamp = stamp_input.stamp_with(tree, Some(&run.voice), 0);

    // S5: one encode per clip. No burned-in sheet is passed: `Subtitles::BurnedIn`'s overlay is F5.4's
    // own rendering round, and the plan carries the cues it would be drawn from.
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
            let inputs: Vec<(&str, Option<&str>)> = match source.as_deref() {
                Some(path) => vec![("video", Some(path))],
                None => Vec::new(),
            };
            let argv = render::encode_command(clip, settings, &inputs, None, &file);
            ClipEncode {
                stem,
                file,
                command: command("clip", argv),
            }
        })
        .collect();

    // S6: the join, only when there is something to join.
    let join = if encodes.is_empty() {
        None
    } else {
        let list = scratch.join("concat.txt");
        Some(command(
            render::JOINING,
            render::join_command(&list, &scratch.join("joined.bin")),
        ))
    };

    // S8: the mux. Whether subtitle tracks go in is the container rule inside `mux_command`, so the
    // settings row and the command cannot disagree about what webm carries.
    let sheet_path = scratch.join(render::final_srt_name());
    let tracks: Vec<(&str, &Path)> = if cues.all.is_empty() {
        Vec::new()
    } else {
        vec![("", sheet_path.as_path())]
    };
    let joined = scratch.join("joined.bin");
    let mux = command(
        "muxing",
        render::mux_command(
            &joined,
            &joined,
            &tracks,
            settings,
            &tree.final_video(container_ext(settings)),
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
        for code in &cues.languages {
            sidecars.push(Sidecar {
                language: Some(code.clone()),
                srt: tree.final_srt(Some(code)),
                vtt: tree.final_vtt(Some(code)),
                cues: cues.all.clone(),
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
        encodes,
        join,
        mux,
        translated: cues.languages.clone(),
        sidecars,
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

    // S1 — the scratch folder, before anything can write into it.
    match render::clear_scratch(&plan.scratch) {
        Ok(gone) => ran.cleared = gone,
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

    // S4 — the stale sidecars go first, by exact name, then this run's sheet is written. A glob would
    // delete a translation of something else that shares the folder and the render cannot tell which.
    if !plan.cues.all.is_empty() {
        let stale = render::stale_sidecars(
            video_stem(),
            &plan
                .cues
                .languages
                .iter()
                .map(|code| code.as_str())
                .collect::<Vec<&str>>(),
        );
        let produce = plan.video.parent().map(Path::to_path_buf).unwrap_or_default();
        for name in &stale {
            // The scratch sheet is this run's own output and gets rewritten below; only the copies beside
            // the finished video are deleted first, so a stale sidecar never outlives the render that made it.
            let victim = produce.join(name);
            if victim.is_file() && victim != plan.scratch.join(render::final_srt_name()) {
                let _ = std::fs::remove_file(&victim);
            }
        }
        let sheet = plan.scratch.join(render::final_srt_name());
        if let Err(why) = std::fs::write(&sheet, subs::srt_text(&plan.cues.all)) {
            ran.failed = Some(format!("{}: {why}", sheet.display()));
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
        // S5's other rule, measured here because only the encoder knows what it wrote: the join is a
        // copy and cannot mix sizes, so a clip that came out at a different size breaks the video at
        // exactly that clip.
        if let Some(first) = plan.encodes.first() {
            let base = file_size(&first.file);
            for encode in plan.encodes.iter().skip(1) {
                if let Some(warning) = render::join_mismatch(base, file_size(&encode.file)) {
                    log(&format!("{}: {warning}", encode.stem));
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
    for code in &plan.translated {
        log(&render::translate_log(plan.cues.all.len(), code));
    }
    log(&plan.mux.log);
    match spawn(&plan.mux) {
        Ok(()) => ran.muxed = true,
        Err(why) => {
            ran.failed = Some(format!("mux: {why}"));
            return ran;
        }
    }

    // S9 — the sidecars beside the video, each one logged.
    for sidecar in &plan.sidecars {
        if let Err(why) = write_sidecar(sidecar) {
            ran.failed = Some(why);
            return ran;
        }
        log(&sidecar.log);
        ran.sidecars.push(sidecar.srt.display().to_string());
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
    let ok = produce_stamp::write_stamp(tree, &plan.stamp).is_ok();
    // Said once per clean render: the hash goes in because these lines are compared between runs, and a
    // stale stamp somebody wants to delete has to be findable by eye.
    if ok {
        log(&produce_stamp::wrote_stamp_log(&plan.stamp));
    }
    ok
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
        return format!("{} KB", (bytes + 1023) / 1024);
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
    let ran = walk(&plan, &mut log, speak, ask, spawn);
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
