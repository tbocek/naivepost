//! F5.2 The render: the side that **runs** it. [`crate::produce_render`] decides S1–S10 and spawns nothing;
//! this module is the queue of commands in the spec's order with the five checkpoints between them.
//! [`plan`] answers every step, log string and subprocess before anything runs on the GUI thread; [`walk`] drains
//! them on the run's thread and asks the checkpoint question between commands. The spawner is injected (a test
//! records argv; the program hands in [`crate::subprocess::run`], the only spawner in the produce tree), which keeps
//! `produce_render`'s `f5_2_module_runs_nothing` true. Every string, argv and path comes from `produce_render`,
//! `produce_subtitles` or [`Tree`].

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

/// What the run had already written when a checkpoint halted it (S10's five points, in three kinds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum At {
    BeforeTools,
    BeforeJoin,
    BeforeModelAndMux,
}

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

fn command(step: &'static str, argv: Vec<String>) -> Command {
    // The program goes first exactly once: the render builders already start their argv with it, and a second one reached ffmpeg as a stray argument.
    let mut argv: Vec<String> = if argv.first().map(String::as_str) == Some(FFMPEG) {
        argv
    } else {
        std::iter::once(FFMPEG.to_string()).chain(argv).collect()
    };
    // `-y` right after the program: ffmpeg asks before overwriting, which a run with no terminal answers with no.
    if argv.get(1).map(String::as_str) != Some("-y") {
        argv.insert(1, "-y".to_string());
    }
    Command {
        step,
        log: format!(">>> {step}: {}", argv.join(" ")),
        argv,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToSpeak {
    pub index: usize,
    pub text: String,
    pub wav: PathBuf,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Cues {
    pub all: Vec<subs::Cue>,
    pub languages: Vec<String>,
    /// The language the bare-stem track is already in. An older build's `naivepost.json` can still tick it; `plan` drops it from [`Self::languages`].
    pub session_language: String,
    /// S1 (§F5.4): the cues per clip, each on THAT clip's own seconds with §S1's `an empty cue extends the previous` applied.
    /// Burned-in subtitles are lettered per clip (`crate::produce_subtitles_run::burn_file_for`) and `subs::tidy` folds nearby
    /// cues, which must not cross a clip boundary. `produced_clocks` gives the offset into [`Self::all`].
    pub per_clip: Vec<Vec<subs::Cue>>,
    pub counts: crate::produce_subtitles_run::CueCounts,
}

pub struct Materials<'a> {
    /// S3: whether an insert's asset is still on disk; the caller resolves the cut-relative spelling.
    pub exists: &'a dyn Fn(&str) -> bool,
    pub fx_for: &'a dyn Fn(&Seg) -> Vec<Fx>,
    pub lanes: &'a [Lane],
    pub src_shape: (i32, i32),
    pub wav_of: &'a dyn Fn(&Entry) -> Option<PathBuf>,
    /// F4.3: how long a line's synthesis IS, in seconds (`None` without a wav). The only source of the render's speech length.
    pub wav_seconds: &'a dyn Fn(&Entry) -> Option<f64>,
    /// F4.3 step 1 ("grow the clip … never past the footage"): footage seconds left past this segment. `0.0` means unknown, so only the ceiling applies.
    pub footage_left: &'a dyn Fn(&Seg) -> f64,
    /// S5: the file one clip reads its picture from, and the second inside it the encode seeks to. Footage and a `copy:`
    /// stretch read the first footage recording; an insert its asset. `None` skips the encode rather than giving ffmpeg no input.
    pub source_file: &'a dyn Fn(&render::Clip) -> Option<(String, f64)>,
    /// §8 bullet 4: does this session have tape at all? A join is dipped only where sound can be read off a recording.
    pub on_recording: bool,
    pub cues: Cues,
    /// The project's source list, which the stamp weighs (`run.sources` has a different shape).
    pub sources: &'a dyn Fn() -> Vec<crate::project::Source>,
    /// S1 (§F5.4): the clip's OWN aligned words on its recording clock (narrator mic excluded), asked only for footage
    /// with no narration line ([`crate::produce_clip_cues`]). Empty yields no cues rather than a guess.
    pub words: &'a dyn Fn(&render::Clip) -> Vec<subs::Word>,
    /// S3 (§F5.4): the model leg for a translated track, caller-supplied so no test dials a socket; a refusal ships the original text with the merge's warning.
    pub translate: &'a crate::produce_translate::Ask,
    /// §F5.2 S5: a clip's encoded frame size, read back off the file (ffprobe), since a stream-copy join cannot mix sizes.
    /// `None` is NO CLAIM (no ffprobe, unreadable), contributing no mismatch.
    pub probe: &'a dyn Fn(&Path) -> Option<(i32, i32)>,
    /// §6 bullet 1's planning refusals: the filmed spans (a `copy:` stretch and a stop are placed on SECONDS). Empty means no tape anywhere.
    pub filmed_runs: &'a [(f64, f64)],
    /// The name §6's "past the end of <base>" says: the footage row for footage and a `copy:` stretch, the asset's file name for an insert.
    pub base_of: &'a dyn Fn(&Seg) -> String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClipEncode {
    pub stem: String,
    pub file: PathBuf,
    pub command: Command,
    /// How long the clip runs on screen: its share of the bar while the clips encode.
    pub seconds: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sidecar {
    /// `None` for the session's own language, which takes the bare stem (§1).
    pub language: Option<String>,
    pub srt: PathBuf,
    pub vtt: PathBuf,
    pub cues: Vec<subs::Cue>,
    pub log: String,
}

/// The whole run, decided before a subprocess starts, in S1–S9 order.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub scratch_files: Vec<(PathBuf, String)>,
    pub speaking: bool,
    pub captions_warning: Option<&'static str>,
    pub speech: Vec<ToSpeak>,
    pub clips: Vec<render::Clip>,
    pub plan_logs: Vec<String>,
    pub cues: Cues,
    pub burns: Vec<Option<PathBuf>>,
    pub encodes: Vec<ClipEncode>,
    pub join: Option<Command>,
    pub mux: Command,
    pub translated: Vec<String>,
    pub sidecars: Vec<Sidecar>,
    /// §08 §6's webm note: the run had subtitles but the container cannot carry a track; `walk` says [`crate::produce_details::WEBM_SRT_LOG`] after the mux.
    pub webm_without_track: bool,
    pub stamp: String,
    pub video: PathBuf,
    pub scratch: PathBuf,
    pub commands: Vec<Command>,
}

impl Plan {
    /// The step names in command order; a test pins the walk's order with this.
    pub fn steps(&self) -> Vec<&'static str> {
        self.commands.iter().map(|c| c.step).collect()
    }
}

/// Where one translated track's sheet is written for the mux: the scratch folder beside `final.srt`, as soon as the
/// translation is back. The sidecars beside the video are written AFTER the mux, and ffmpeg could not open them.
fn track_sheet(scratch: &Path, code: &str) -> PathBuf {
    scratch.join(format!("final.{code}.srt"))
}

fn is_track_code(code: &str) -> bool {
    // A language code, not a filename: any alphabetic tag, but not blank or punctuation (`final..srt` is a file no run writes or should delete).
    !code.is_empty() && code.bytes().all(|byte| byte.is_ascii_alphabetic())
}

/// The stems whose sidecars this run owns (`final.<tick>` per ticked language) and will overwrite, so those are the stale ones to clear first.
/// The session's own stem is NEVER listed: its `.vtt` may be left from an earlier render when a stamp let this press skip the
/// encode (§F5.3), and F5.5's `<video>` page lists it as the default track.
fn sidecar_stems(ticked: &[String]) -> Vec<String> {
    let stem = render::final_srt_name()
        .trim_end_matches(".srt")
        .to_string();
    let mut stems: Vec<String> = Vec::new();
    stems.extend(
        crate::produce_subtitles::track_languages("", ticked)
            .into_iter()
            .filter(|code| is_track_code(code.trim()))
            .map(|code| format!("{stem}.{code}")),
    );
    stems
}

pub fn video_stem() -> &'static str {
    "final"
}

fn container_ext(settings: &Produce) -> &'static str {
    match settings.container {
        Container::Mp4 => "mp4",
        Container::Mkv => "mkv",
        Container::Webm => "webm",
    }
}

fn is_card_source(source: &str) -> bool {
    crate::cut_insert::kind(source) == crate::cut_insert::Kind::Svg
}

/// S1: the scratch folder is cleared first (`render::clear_scratch`); a missing folder is made, not an error.
pub fn clear_scratch(scratch: &Path) -> Result<usize, String> {
    if !scratch.exists() {
        std::fs::create_dir_all(scratch).map_err(|why| format!("{}: {why}", scratch.display()))?;
        return Ok(0);
    }
    render::clear_scratch(scratch)
}

/// Move one sheet cue onto the video's timeline like [`subs::on_clock`]: the clip's start plus each second shrunk by the
/// clip's rate. The sheet's cues arrive relative to the clip and this walk already holds the offset `produced_clocks` answered.
fn cue_on_clock(cue: &subs::Cue, produced_start: f64, rate: f64) -> subs::Cue {
    subs::Cue {
        s: crate::narration::output_seconds(cue.s, rate) + produced_start,
        e: crate::narration::output_seconds(cue.e, rate) + produced_start,
        text: cue.text.clone(),
        pos: cue.pos.clone(),
    }
}

/// S2: what still has to be spoken. Which lines a run may speak comes through `crate::narrate_off::lines_to_speak`
/// (F4.8's one seam); [`render::speak_job`] then decides which still need a take, and each is turned into its file.
pub fn speech_plan(run: &Run, tree: &Tree, mats: &Materials, speaking: bool) -> Vec<ToSpeak> {
    if !speaking {
        return Vec::new();
    }
    // The seam first: a blank line (a deliberate silence) is never spoken; a narration with no words answers empty.
    let speakable = crate::narration::lines_to_speak(&run.lines);
    match render::speak_job(false, &speakable, |entry| {
        (mats.wav_of)(entry).is_some_and(|wav| wav.is_file())
    }) {
        render::Speak::Nothing => Vec::new(),
        // `index` is the position in the seam's list (dropped lines taken out), so a silent row does not renumber survivors and identical lines keep their own take.
        render::Speak::Missing(_) => speakable
            .iter()
            .enumerate()
            .filter(|(_, entry)| !(mats.wav_of)(entry).is_some_and(|wav| wav.is_file()))
            .map(|(index, entry)| ToSpeak {
                index,
                text: entry.text.clone(),
                wav: (mats.wav_of)(entry)
                    .or_else(|| produce_stamp::wav_of(tree, entry, Some(&run.voice), entry.roll))
                    .unwrap_or_else(|| tree.tts_wav("pending")),
            })
            .collect(),
    }
}

/// S5: the length of speech each placed line carries: **the wav's own duration** (§F4.3). `plan_clips` fits with 0.0
/// since it places lines before any wav exists. A missing wav keeps length 0 and takes no room.
pub fn fill_speech(
    clips: &mut [render::Clip],
    entries: &[Entry],
    seconds_of: &dyn Fn(&Entry) -> Option<f64>,
) {
    for clip in clips.iter_mut() {
        for line in clip.lines.iter_mut() {
            let Some(entry) = entries.get(line.entry_index) else {
                continue;
            };
            line.speech = seconds_of(entry).unwrap_or(0.0).max(0.0);
        }
    }
}

pub fn fit_spoken(clips: &mut [render::Clip], footage_left: &dyn Fn(&Seg) -> f64) -> Vec<String> {
    let mut logs = Vec::new();
    for clip in clips.iter_mut().filter(|clip| !clip.lines.is_empty()) {
        let lines: Vec<(usize, f64, f64)> = clip
            .lines
            .iter()
            .map(|line| (line.entry_index, line.at, line.speech))
            .collect();
        let seg = Seg {
            s: clip.seg_s,
            e: clip.seg_s + clip.on_screen * clip.rate,
            ..Default::default()
        };
        let (fitted, said) = render::fit_in(&lines, clip.on_screen, clip.no, footage_left(&seg));
        logs.extend(said);
        let grown = fitted.iter().map(|line| line.extend).fold(0.0, f64::max);
        clip.on_screen += grown;
        clip.lines = fitted
            .into_iter()
            .zip(clip.lines.iter())
            .map(|(line, was)| render::Placed {
                text: was.text.clone(),
                ..line
            })
            .collect();
    }
    logs
}

pub fn plan(run: &Run, tree: &Tree, mats: &Materials) -> Plan {
    let scratch = tree.clips_dir();
    let settings = &run.settings;
    let _ = std::fs::create_dir_all(&scratch);

    // S2: `crate::narrate_screen::CAPTIONS` is the id the voice dropdown stores for "No audio — captions only" (§1). The warning
    // branches only where lines exist but nothing is spoken and no track carries them.
    let voice_is_captions = run.voice == crate::narrate_screen::CAPTIONS;
    let speaking = !voice_is_captions;
    let captions_warning = if speaking {
        None
    } else {
        render::captions_only_warning(settings.subtitles)
    };
    let speech: Vec<ToSpeak> = speech_plan(run, tree, mats, speaking);

    // S3: one clip per segment after `cut_speed_split::sped_segs` (without it the planner sees unstamped rates and every clip is ×1).
    // Clip numbers follow THIS list. Cuts saved before a card opened its footage still play it where placed.
    let opened = crate::cut::Cut {
        segs: crate::cut::spliced(run.cut.segs.clone()),
        ..run.cut.clone()
    };
    let sped = crate::cut_speed_split::sped_cut(&opened);
    let (clips, plan_logs) = render::plan_clips(
        &sped,
        mats.fx_for,
        &run.aspect,
        mats.exists,
        mats.lanes,
        &run.lines,
        mats.src_shape,
        mats.on_recording,
        mats.footage_left,
    );

    // §6 bullet 1: refusals `plan_clips` never said; same sped list, same clip numbers.
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
        },
    );

    let mut clips = clips;
    fill_speech(&mut clips, &run.lines, mats.wav_seconds);
    let mut plan_logs = plan_logs;
    plan_logs.extend(fit_spoken(&mut clips, mats.footage_left));
    for clip in clips.iter_mut() {
        clip.scratch = scratch.clone();
        clip.on_recording = mats.on_recording;
    }
    // §5 frame-exact clips: every clip's frames from the video's running clock, on the setting's grid ("source": the
    // first recording's own rate). With no grid known the clips are cut by their seconds, as before.
    let grid = match settings.frame_rate {
        crate::project::FrameRate::F60 => 60.0,
        crate::project::FrameRate::F30 => 30.0,
        crate::project::FrameRate::F24 => 24.0,
        crate::project::FrameRate::Source => clips
            .iter()
            .find(|clip| clip.source.is_empty())
            .and_then(|clip| (mats.source_file)(clip))
            .and_then(|(path, _)| crate::produce_probe::frame_rate_of(Path::new(&path)))
            .unwrap_or(0.0),
    };
    render::frame_grid(&mut clips, grid);

    // S4: each clip's lines become cues on its own clock, moved onto the video's by `produced_clocks`' offsets, then tidied
    // ONCE across clips (`tidy` compares cues with each other).
    // §S4: stale sidecars go before ANY of this run's writes, hence here and not in `walk` (its first step is `clear_scratch`).
    // A language nobody ticked keeps its file.
    let sheet_will_have_cues = clips.iter().any(|clip| {
        !crate::produce_clip_cues::build(clip, &clip.lines, &run.lines, &(mats.words)(clip))
            .is_empty()
    });
    // The bare-stem pair is swept only when this run has cues to rewrite it with (§5's "never an empty cue", `sheet_will_have_cues`):
    // an up-to-date video skips the encodes but not the sheet, and F5.5's `<video>` page lists the `.vtt` as the default track.
    let bare_stem: &[String] = if sheet_will_have_cues {
        &[render::final_srt_name()
            .trim_end_matches(".srt")
            .to_string()]
    } else {
        &[]
    };
    let sheet_sides = sidecar_stems(&mats.cues.languages)
        .into_iter()
        .chain(bare_stem.iter().cloned())
        .collect::<Vec<String>>();
    crate::produce_subtitles_run::clear_stale_sidecars(
        &tree.final_video(container_ext(settings)),
        &scratch,
        &sheet_sides,
    );

    let mut cues = Cues {
        all: Vec::new(),
        // §1's second door: a setting carrying the session's own language still costs no ask and no `final.<own>.srt` (`track_languages`).
        languages: crate::produce_subtitles::track_languages(
            &mats.cues.session_language,
            &mats.cues.languages,
        )
        .into_iter()
        .filter(|code| is_track_code(code.trim()))
        .collect(),
        // The walk re-reads this through `plan.translated`; the stale sweep must use the SAME answer.
        ..Default::default()
    };
    for (clip, (start, _len)) in clips.iter().zip(render::produced_clocks(&clips).iter()) {
        let mine =
            crate::produce_clip_cues::build(clip, &clip.lines, &run.lines, &(mats.words)(clip));
        if mine.is_empty() {
            continue;
        }
        // S1's last clause runs here so a clip's own words and its written lines get it alike: an empty cue lengthens the caption already up.
        let mut mine = mine;
        subs::extend_previous(&mut mine);
        crate::produce_subtitles_run::count_cue_source(&mut cues.counts, &mine, clip.lines.len());
        cues.all
            .extend(mine.iter().map(|c| cue_on_clock(c, *start, clip.rate)));
        cues.per_clip.push(mine);
    }
    cues.all = subs::tidy(&cues.all);

    // S10: computed BEFORE the encodes because the clip stems carry the stamp (§B's `c%03d_<stamp>`).
    let sources_now = (mats.sources)();
    let stamp_input = produce_stamp::Input {
        settings,
        segs: &run.cut.segs,
        lines: &run.lines,
        sources: &sources_now,
        aspect: &run.aspect,
        voice: &run.voice,
        // Taken from the hands that feed the encoder; a stamp that missed them could skip a render that had never seen them.
        fx: &run.cut.fx,
        lanes: &run.cut.lanes,
        shape: mats.src_shape,
    };
    let stamp = stamp_input.stamp_with(tree, Some(&run.voice), 0);

    // S5: no burned-in sheet is passed; `Subtitles::BurnedIn`'s overlay is F5.4's own round. The walk writes these after emptying the folder.
    let mut scratch_files: Vec<(PathBuf, String)> = Vec::new();
    let mut effect_logs: Vec<String> = Vec::new();
    let encodes: Vec<ClipEncode> = clips
        .iter()
        .enumerate()
        .map(|(index, clip)| {
            let stem = render::clip_stem(index, &stamp);
            let file = scratch.join(format!("{stem}.{}", render::CLIP_EXT));
            let mut clip = clip.clone();
            clip.lane_files = mats
                .lanes
                .iter()
                .filter(|lane| {
                    clip.lanes.contains(&lane.name)
                        && crate::cut_hear::lane_covers(
                            lane,
                            crate::cut_hear::lane_seconds(lane, clip.seg_s),
                            clip.seg_s,
                        )
                })
                .map(|lane| render::LaneFile {
                    name: lane.name.clone(),
                    path: crate::layout::resolve(tree.dir(), tree.dir(), &lane.src)
                        .display()
                        .to_string(),
                    seek: crate::cut_hear::lane_seconds(lane, clip.seg_s),
                    delay: 0.0,
                })
                .collect();
            clip.voice_files = clip
                .lines
                .iter()
                .filter(|line| line.speech > 0.0)
                .map_while(|line| {
                    run.lines
                        .get(line.entry_index)
                        .and_then(|entry| (mats.wav_of)(entry))
                        .map(|wav| wav.display().to_string())
                })
                .collect();
            let source = (mats.source_file)(&clip);
            let insert = !clip.source.is_empty() && !clip.source.starts_with("copy:");
            let sound_file = (insert
                && crate::cut_insert::kind(&clip.source) == crate::cut_insert::Kind::Audio)
                .then(|| source.as_ref().map(|(path, _)| path.clone()))
                .flatten();
            let source = if sound_file.is_some() {
                let footage = render::Clip {
                    source: String::new(),
                    ..clip.clone()
                };
                (mats.source_file)(&footage)
            } else {
                source
            };
            let picture_kind = if insert {
                match crate::cut_insert::kind(&clip.source) {
                    crate::cut_insert::Kind::Still => "image",
                    _ => "video",
                }
            } else {
                "video"
            };
            // A card or picture brings no sound and a video insert may not, but the graph needs one.
            let silent = insert
                && sound_file.is_none()
                && source.as_ref().is_some_and(|(path, _)| {
                    picture_kind == "image"
                        || is_card_source(&clip.source)
                        || !crate::produce_probe::has_sound(std::path::Path::new(path))
                });
            clip.sound_input = usize::from(silent || sound_file.is_some());
            let clip = &clip;
            let seek = source.as_ref().map_or(clip.seg_s, |(_, seek)| *seek);
            let mut inputs: Vec<(&str, Option<&str>)> = match source.as_ref() {
                Some((path, _)) => vec![(picture_kind, Some(path.as_str()))],
                None => Vec::new(),
            };
            let mut loops_card = false;
            // Beside `inputs`: the card's paths are invented here and must outlive the block that made them.
            let mut card_paths: Vec<(&str, String)> = Vec::new();
            // §4: whatever a clip's effects read must exist before ffmpeg is asked, e.g. a generated title sheet.
            let built = crate::produce_fx_chain::for_clip(clip, &clip.fx, clip.src, &clip.scratch);
            scratch_files.extend(built.title_files.iter().cloned());
            // §5: `produce_cards` decides how a card is filled or baked to one SVG per frame; the result is written into the
            // scratch folder BEFORE the command is built, like the title sheets above.
            if let Some(mut card_input) =
                crate::produce_cards::stage(clip, source.as_ref().map(|(path, _)| path.as_str()))
            {
                effect_logs.extend(card_input.logs.iter().cloned());
                scratch_files.extend(card_input.files.iter().cloned());
                for (kind, path) in std::mem::take(&mut card_input.inputs) {
                    let kind: &'static str = if kind == "card" { "card" } else { "svg" };
                    card_paths.push((kind, path));
                }
                loops_card = card_input.loops && card_paths.iter().any(|(kind, _)| *kind == "card");
            }
            if let Some((kind, path)) = card_paths.first() {
                inputs = vec![(*kind, Some(path.as_str()))];
            }
            if silent {
                inputs.push(("sound", None));
            }
            if let Some(file) = sound_file.as_deref() {
                inputs.push(("soundfile", Some(file)));
            }
            // Last: `video_graph` counts the overlays' inputs back from here.
            for (kind, path) in built.inputs.iter() {
                inputs.push((kind.as_str(), path.as_deref()));
            }
            effect_logs.extend(built.logs.iter().cloned());
            // §F5.2 S5/§B: the burned sheet (`c%03d_<stamp>.srt`) letters the CLIP's own cues on the clip's own seconds; the encode
            // never sees the video clock. `plan_burns` writes the file from the same name.
            let burn = crate::produce_subtitles_run::burn_file_name(index, &stamp, settings);
            let argv = render::encode_command_with(
                clip,
                settings,
                &inputs,
                burn.as_deref(),
                &file,
                loops_card,
                seek,
            );
            ClipEncode {
                stem,
                file,
                command: command("clip", argv),
                seconds: clip.on_screen,
            }
        })
        .collect();
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
            // §6 names this file `joined.<container>`, the container of `final.<ext>`; one expression feeds the writer and the mux's input.
            render::join_command(
                &list,
                &scratch.join(details::joined(render::CLIP_EXT)),
            ),
        ))
    };

    // S8: whether subtitle tracks go in is `mux_command`'s container rule. §6: EVERY track is its own input, tagged with
    // its ISO-639-2 code and name, from the same list S9 writes sidecars from; a track whose file this run never writes
    // would be an `-i` that fails, so an empty cue list means no tracks.
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
            tracks.push((code.clone(), track_sheet(&scratch, code)));
        }
    }
    let track_refs: Vec<(&str, &Path)> = tracks
        .iter()
        .map(|(code, path)| (code.as_str(), path.as_path()))
        .collect();
    // The tag/name pair comes from the app's language list (`SUBTITLE_LANGUAGES`, 03-shell §6), read here to keep `mux_command`
    // pure; with no settings file a track falls back to `und` named by its upper-cased code (§6).
    let known_languages = crate::roles::languages_or_built_in(
        &crate::settings::from_environment()
            .and_then(|paths| crate::settings::read(&paths).ok())
            .map(|conf| conf.subtitle_languages)
            .unwrap_or_default(),
    );
    let joined = scratch.join(details::joined(render::CLIP_EXT));
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

    // S9: the session's own language takes the bare stem, each ticked language `.code`; both files each (§1: the .vtt is the one a browser needs).
    let mut sidecars: Vec<Sidecar> = Vec::new();
    if !cues.all.is_empty() {
        sidecars.push(Sidecar {
            language: None,
            srt: tree.final_srt(None),
            vtt: tree.final_vtt(None),
            cues: cues.all.clone(),
            log: render::sidecar_log(video_stem(), None),
        });
        // A tick naming no track writes nothing (not `final..srt`; see `is_track_code`). The session's language never appears here (§1, `track_languages`).
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
        scratch_files,
        speaking,
        captions_warning,
        speech,
        clips,
        plan_logs,
        cues: cues.clone(),
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
        // §6: the webm note belongs to a run that HAD cues.
        webm_without_track: settings.container == Container::Webm && !cues.all.is_empty(),
        stamp,
        video: tree.final_video(container_ext(settings)),
        scratch,
        commands,
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ran {
    pub cleared: usize,
    pub spoken: usize,
    pub speech_failed: usize,
    pub logs: Vec<String>,
    pub encoded: Vec<String>,
    pub joined: bool,
    pub muxed: bool,
    pub sidecars: Vec<String>,
    pub stopped_at: Option<&'static str>,
    pub failed: Option<String>,
}

impl Ran {
    pub fn clean(&self) -> bool {
        self.muxed && self.stopped_at.is_none() && self.failed.is_none()
    }
}

/// Drain the plan in order, asking the checkpoint between tools. `ask` answers false once stopped; the walk halts at the
/// first false and records where, so a stop never lands mid-subprocess or leaves a half-written deliverable.
/// A line that will not synthesise is logged and skipped, not fatal.
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

    // S1: the scratch folder, before anything can write into it.
    match render::clear_scratch(&plan.scratch) {
        Ok(gone) => {
            ran.cleared = gone;
            log(&crate::produce_data::scratch_clear_log(
                crate::produce_data::scratch_groups(),
                gone,
            ));
        }
        Err(why) => {
            ran.failed = Some(why);
            return ran;
        }
    }
    for (path, text) in &plan.scratch_files {
        if let Some(folder) = path.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        if let Err(why) = std::fs::write(path, text) {
            ran.failed = Some(format!("{}: {why}", path.display()));
            return ran;
        }
    }

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
                log(&render::speaking_job_line(ran.spoken, plan.speech.len()));
            }
            Err(why) => {
                log(&render::synthesis_failure_log(ran.spoken as f64, &why));
                ran.speech_failed += 1;
            }
        }
    }

    for line in &plan.plan_logs {
        log(line);
    }
    ran.logs = plan.plan_logs.clone();
    if plan.clips.is_empty() {
        log(render::no_clip_log());
        ran.failed = Some(render::no_clip_log().to_string());
        return ran;
    }

    // S4: stale sidecars already went by exact name in `plan` (§S4 puts the delete ahead of every write); what is left is the run's own sheet or the refusal to write one.
    let sheet = plan.scratch.join(render::final_srt_name());
    if plan.cues.all.is_empty() {
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

    // S5: the command is logged before it runs: the one that hung is the one worth reading.
    // Each clip's share of the bar is its time on screen, filled as ffmpeg says how far it got: by count alone the
    // bar stood empty through the whole first clip and then crept by one 64th (first rewrite: it looked gone).
    let clips = plan.encodes.len();
    let total: f64 = plan.encodes.iter().map(|e| e.seconds.max(0.0)).sum();
    let mut done = 0.0;
    for (index, encode) in plan.encodes.iter().enumerate() {
        let (from, share) = if total > 0.0 {
            (done / total, encode.seconds.max(0.0) / total)
        } else {
            (index as f64 / clips as f64, 1.0 / clips as f64)
        };
        crate::run_stages::encoding(
            &format!("encoding clip {} of {clips}", index + 1),
            from,
            share,
            encode.seconds,
        );
        done += encode.seconds.max(0.0);
        if !ask(At::BeforeTools, "between clips") {
            crate::run_stages::encoding_done();
            ran.stopped_at = Some("between clips");
            return ran;
        }
        log(&encode.command.log);
        let spawned = spawn(&encode.command);
        match spawned {
            Ok(()) => ran.encoded.push(encode.stem.clone()),
            Err(why) => {
                crate::run_stages::encoding_done();
                ran.failed = Some(format!("clip {}: {why}", encode.stem));
                return ran;
            }
        }
    }
    crate::run_stages::encoding_done();

    crate::run_stages::report_progress("joining the clips", Some(1.0));
    if !ask(At::BeforeJoin, render::JOINING) {
        ran.stopped_at = Some(render::JOINING);
        return ran;
    }
    if let Some(join) = &plan.join {
        let list = plan.scratch.join("concat.txt");
        let body = render::concat_list(
            &plan
                .encodes
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
        // S5's other rule, measured AFTER the encodes: the join is a copy and cannot mix sizes, so a clip at another FRAME SIZE breaks it.
        // Sizes are probed off the files (`Materials::probe`), never guessed from bytes; no ffprobe means no claim, not an all-clear.
        let sizes: Vec<Option<(i32, i32)>> = plan
            .encodes
            .iter()
            .map(|encode| (mats.probe)(&encode.file))
            .collect();
        if let Some(base) = sizes.iter().flatten().next().copied() {
            for (encode, size) in plan.encodes.iter().zip(sizes.iter()).skip(1) {
                if size.is_some() && *size != Some(base) {
                    log(&format!("{}: {}", encode.stem, render::JOIN_MISMATCH));
                }
            }
        }
    }

    // S7 + S8: the translation is the slow part; before the mux so the encoder does not idle behind it.
    crate::run_stages::report_progress("writing subtitles and the finished file", Some(1.0));
    if !ask(At::BeforeModelAndMux, "before translate and mux") {
        ran.stopped_at = Some("before translate and mux");
        return ran;
    }
    // The subtitles go on the clock the join made, measured off the encoded files, so they stay with the speech to the last clip.
    let lengths: Vec<f64> = plan
        .encodes
        .iter()
        .filter_map(|encode| crate::produce_probe::seconds_of(&encode.file))
        .collect();
    let timed = subs::onto_measured(
        &plan.cues.all,
        &render::produced_clocks(&plan.clips),
        &lengths,
    );
    if !timed.is_empty() {
        if let Err(why) = std::fs::write(&sheet, subs::srt_text(&timed)) {
            ran.failed = Some(format!("{sheet}: {why}", sheet = sheet.display()));
            return ran;
        }
    }
    // S7: `plan.translated` is already the ticked list minus the session's own language (`track_languages`), so nothing translates into itself.
    if let Ok(tree) = Tree::new(crate::startup::open_dir()) {
        crate::produce_translate_ask::warm(
            &tree,
            &crate::produce_translate::first_asks(
                &timed,
                &plan.cues.session_language,
                &plan.translated,
            ),
            &mut |line| log(line),
        );
    }
    let tracked = crate::produce_translate::track(
        &timed,
        &plan.cues.session_language,
        &plan.translated,
        mats.translate,
        |line| log(line),
    );
    for code in &plan.translated {
        log(&render::translate_log(
            plan.cues.all.len(),
            &crate::roles::language_name(code),
        ));
    }
    for t in tracked.iter().filter(|t| t.warning.is_some()) {
        log(t.warning.as_deref().unwrap_or_default());
    }
    // Read twice: the mux's track sheets now, the sidecars after. An unreachable language returns the ORIGINAL cues and a warning, so its sheet is never empty.
    let by_code: BTreeMap<String, Vec<subs::Cue>> = tracked
        .into_iter()
        .map(|t| (t.language.clone(), t.cues))
        .collect();
    let translated_or_original = |code: &str| -> Vec<subs::Cue> {
        match by_code.get(code) {
            Some(cues) if !cues.is_empty() => cues.clone(),
            _ => timed.clone(),
        }
    };
    if plan.mux.argv.iter().any(|arg| arg == "-c:s") {
        for code in &plan.translated {
            let sheet = track_sheet(&plan.scratch, code);
            if let Err(why) = std::fs::write(&sheet, subs::srt_text(&translated_or_original(code)))
            {
                ran.failed = Some(format!("{}: {why}", sheet.display()));
                return ran;
            }
        }
    }
    log(&plan.mux.log);
    match spawn(&plan.mux) {
        Ok(()) => ran.muxed = true,
        Err(why) => {
            ran.failed = Some(format!("mux: {why}"));
            return ran;
        }
    }
    // §6: webm cannot carry an srt track; said AFTER the mux, pointing at the sidecars S9 writes next.
    if plan.webm_without_track {
        log(crate::produce_details::WEBM_SRT_LOG);
    }

    // S9: a `.code` track carries the TRANSLATED cues; the bare-stem one keeps the originals.
    for sidecar in &plan.sidecars {
        // Write from the plan's own track when the translation leg was unreachable (it returns the original cues): an empty sidecar beside a finished video is forbidden by §5.
        let out = match &sidecar.language {
            None => Sidecar {
                cues: timed.clone(),
                ..sidecar.clone()
            },
            Some(code) => Sidecar {
                cues: translated_or_original(code),
                ..sidecar.clone()
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

fn write_sidecar(sidecar: &Sidecar) -> Result<(), String> {
    for (path, text) in [
        (&sidecar.srt, subs::srt_text(&sidecar.cues)),
        (&sidecar.vtt, subs::vtt_text(&sidecar.cues)),
    ] {
        std::fs::write(path, text).map_err(|why| format!("{}: {why}", path.display()))?;
    }
    Ok(())
}

pub fn write_stamp(plan: &Plan, tree: &Tree, ran: &Ran, mut log: impl FnMut(&str)) -> bool {
    // S10: only a finished run may promise the file matches these settings. `clean()` (requires the mux) outranks `stamp_written`:
    // a stopped run has a joined file and no deliverables, and a partial encode would hide a stale render from the next press.
    if !ran.clean() || !render::stamp_written(!ran.encoded.is_empty(), ran.failed.is_some()) {
        return false;
    }
    // §6: an unwritable stamp is SAID, never fails the run (the video is finished); the next ▶ encodes again.
    match produce_stamp::write_stamp(tree, &plan.stamp) {
        Ok(()) => {
            log(&produce_stamp::wrote_stamp_log(&plan.stamp));
            true
        }
        Err(why) => {
            log(&details::stamp_unwritable_log(&why));
            false
        }
    }
}

fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

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

/// The program every tool step runs, via PATH. §10 gives no `P.machine.*` row for a path, so this is the one place the name is written.
pub const FFMPEG: &str = "ffmpeg";

pub fn spawn_tool(command: &Command) -> Result<(), String> {
    let args: Vec<&str> = command.args().iter().map(|a| a.as_str()).collect();
    crate::subprocess::run_listed_timed(
        command.program(),
        &args,
        crate::ui::window::log_line,
        crate::run_stages::encoded_to,
    )
    .map(|_| ())
}

/// The one call a render half makes: plan, walk, stamp, report. `tag_ok` is the stamp's input only: F5.1 S5 treats the tag page as never the verdict.
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
