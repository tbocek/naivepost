//! spec/04-prepare.md F1.14: line the recordings up by sound.
//!
//! File names place recordings to the second and unstamped files all sit at the session's start, so two
//! recorders started by hand are seconds apart. This stage measures what is left after the names have placed
//! everything and takes it out of the copy in `sources/`: afterwards every recording's second 0 is where its
//! name says and the sound fits. Nothing is stored, so nothing downstream knows a correction was made.
//!
//! The measuring half is pure (sample slices in, a [`Verdict`] out) and is what the tests drive; the door at
//! the bottom decodes with ffmpeg, decides per file, and writes the copy again.

use std::path::{Path, PathBuf};

use crate::layout::{self, Tree};
use crate::project::Project;
use crate::run_stages;

/// The marker [`crate::run_stages::drive_walk`] reads as "run the sync pass".
pub const SYNC_TASK: &str = "naivepost:sync";

/// Everything is measured on mono sound at this rate: enough for speech, and an hour is 29 MB.
pub const RATE: usize = 8000;
/// The loudness curve's step: 10 ms.
const HOP: usize = RATE / 100;

/// P.eng.syncProbeSeconds: how much shared sound the coarse search compares.
pub const PROBE_SECONDS: f64 = 120.0;
/// P.eng.syncSearchSeconds: how far either way the coarse search looks.
pub const SEARCH_SECONDS: f64 = 600.0;
/// P.eng.syncMinWindows: this many windows in a row must agree before an offset is believed.
pub const MIN_WINDOWS: usize = 6;
/// P.eng.syncMinMatch: the least correlation a window counts with.
pub const MIN_MATCH: f64 = 0.5;
/// P.eng.syncDriftPpm: a slope past this is two clocks drifting apart.
pub const DRIFT_PPM: f64 = 20.0;
/// P.eng.syncVideoCrf: the quality a picture is encoded again at.
pub const VIDEO_CRF: u32 = 16;
/// Under this either way a recording already lines up.
pub const ALIGNED_SECONDS: f64 = 0.005;
/// A picture is only encoded again past half a frame of 25 a second.
pub const VIDEO_ALIGNED_SECONDS: f64 = 0.02;
/// S3: every this many seconds along the recording, a window this long.
const STEP_SECONDS: f64 = 5.0;
const WINDOW_SECONDS: f64 = 10.0;
/// S3: each window looks this far around the one before.
const AROUND_SECONDS: f64 = 0.6;
/// The sample-level search around the loudness answer, either way.
const FINE_SECONDS: f64 = 0.015;
/// How much of a window the sample-level search reads: enough to be sure, a third of the cost.
const FINE_WINDOW_SECONDS: f64 = 4.0;
/// Windows of one run agree within this (sample-level), or this (loudness only).
const AGREE_FINE: f64 = 0.002;
const AGREE_COARSE: f64 = 0.006;
/// A later window this far from the offset in force, twice in a row, is a jump.
const JUMP_SECONDS: f64 = 0.02;

/// One window's answer: where it sits in the reference, the offset it found, how well it matched, and
/// whether the samples themselves agreed (two microphones in one room often agree only in loudness).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Window {
    pub at: f64,
    pub offset: f64,
    pub score: f64,
    pub fine: bool,
}

/// A moment the two recordings slip against each other after the start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Jump {
    /// Seconds into the reference.
    pub at: f64,
    /// The change of the offset: negative when the reference lost sound the other kept.
    pub by: f64,
}

/// What the measuring found.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// `offset` > 0: the other recording started that long AFTER the reference (silence goes in front of
    /// it); < 0: that long before (its start comes off). Measured in each file's own seconds.
    Found {
        offset: f64,
        jumps: Vec<Jump>,
        /// Milliseconds a minute the other drifts against the reference, when past [`DRIFT_PPM`].
        drift_ms_per_minute: Option<f64>,
    },
    NotFound,
}

/// The loudness curve: log energy every 10 ms, its mean taken off.
pub fn envelope(samples: &[f32]) -> Vec<f32> {
    let mut curve: Vec<f32> = samples
        .chunks_exact(HOP)
        .map(|chunk| {
            let power: f32 = chunk.iter().map(|x| x * x).sum::<f32>() / HOP as f32;
            (power + 1e-9).ln()
        })
        .collect();
    let mean = curve.iter().sum::<f32>() / curve.len().max(1) as f32;
    for value in &mut curve {
        *value -= mean;
    }
    curve
}

/// Pearson's correlation of two equal stretches; 0 when either is flat.
fn pearson(a: &[f32], b: &[f32]) -> f64 {
    let n = a.len().min(b.len());
    if n == 0 {
        return 0.0;
    }
    let (a, b) = (&a[..n], &b[..n]);
    let mean_a = a.iter().sum::<f32>() / n as f32;
    let mean_b = b.iter().sum::<f32>() / n as f32;
    let (mut ab, mut aa, mut bb) = (0f64, 0f64, 0f64);
    for (x, y) in a.iter().zip(b) {
        let (x, y) = ((x - mean_a) as f64, (y - mean_b) as f64);
        ab += x * y;
        aa += x * x;
        bb += y * y;
    }
    if aa <= 0.0 || bb <= 0.0 {
        return 0.0;
    }
    ab / (aa * bb).sqrt()
}

/// The best lag of `other` against `reference` over `lags` (in steps of the slices), with its score, and the
/// peak placed between two steps by its shape. A lag L pairs `reference[at + i]` with `other[at - L + i]`.
fn best_lag(
    reference: &[f32],
    other: &[f32],
    at: usize,
    len: usize,
    lags: std::ops::RangeInclusive<i64>,
) -> Option<(f64, f64)> {
    let mut scores: Vec<(i64, f64)> = Vec::new();
    for lag in lags {
        let start = at as i64 - lag;
        if start < 0 || at + len > reference.len() || start as usize + len > other.len() {
            continue;
        }
        let start = start as usize;
        scores.push((
            lag,
            pearson(&reference[at..at + len], &other[start..start + len]),
        ));
    }
    let (index, &(lag, score)) = scores
        .iter()
        .enumerate()
        .max_by(|a, b| a.1 .1.total_cmp(&b.1 .1))?;
    let mut exact = lag as f64;
    if index > 0 && index + 1 < scores.len() {
        let (before, after) = (scores[index - 1].1, scores[index + 1].1);
        let bend = before - 2.0 * score + after;
        if bend < 0.0 {
            exact += 0.5 * (before - after) / bend;
        }
    }
    Some((exact, score))
}

/// S3's coarse search on the loudness: the first [`PROBE_SECONDS`] of sound the two share, tried at every
/// lag within `search` seconds of `expected`. Answers the offset in seconds and its score.
pub fn coarse(reference: &[f32], other: &[f32], expected: f64, search: f64) -> Option<(f64, f64)> {
    let probe = (PROBE_SECONDS * 100.0) as usize;
    let centre = (expected * 100.0).round() as i64;
    let reach = (search * 100.0) as i64;
    let mut best: Option<(i64, f64)> = None;
    for lag in centre - reach..=centre + reach {
        // The shared sound starts where the later of the two does.
        let at = lag.max(0) as usize;
        let start = (at as i64 - lag) as usize;
        let len = probe
            .min(reference.len().saturating_sub(at))
            .min(other.len().saturating_sub(start));
        // Under a quarter of the probe there is too little shared sound to believe a peak.
        if len < probe / 4 {
            continue;
        }
        let score = pearson(&reference[at..at + len], &other[start..start + len]);
        if best.is_none_or(|(_, had)| score > had) {
            best = Some((lag, score));
        }
    }
    best.map(|(lag, score)| (lag as f64 / 100.0, score))
}

/// S3's walk along the recording: a window every [`STEP_SECONDS`], each looking around the one before, first
/// on the loudness and then on the samples.
pub fn windows(reference: &[f32], other: &[f32], start_offset: f64) -> Vec<Window> {
    let reference_env = envelope(reference);
    let other_env = envelope(other);
    let overall = rms(reference);
    let window = (WINDOW_SECONDS * RATE as f64) as usize;
    let fine_window = (FINE_WINDOW_SECONDS * RATE as f64) as usize;
    let mut found = Vec::new();
    let mut around = start_offset;
    let mut at = STEP_SECONDS.max(start_offset + STEP_SECONDS);
    while ((at + WINDOW_SECONDS) * RATE as f64) as usize <= reference.len() {
        let from = (at * RATE as f64) as usize;
        // A window with no sound in it says nothing.
        if rms(&reference[from..from + window]) < overall * 0.05 {
            at += STEP_SECONDS;
            continue;
        }
        let env_at = (at * 100.0) as usize;
        let env_len = (WINDOW_SECONDS * 100.0) as usize;
        let centre = (around * 100.0).round() as i64;
        let reach = (AROUND_SECONDS * 100.0) as i64;
        if let Some((lag, score)) = best_lag(
            &reference_env,
            &other_env,
            env_at,
            env_len,
            centre - reach..=centre + reach,
        ) {
            let by_loudness = lag / 100.0;
            let centre = (by_loudness * RATE as f64).round() as i64;
            let reach = (FINE_SECONDS * RATE as f64) as i64;
            let fine = best_lag(
                reference,
                other,
                from,
                fine_window,
                centre - reach..=centre + reach,
            )
            .filter(|(_, score)| *score >= MIN_MATCH);
            let answer = match fine {
                Some((lag, score)) => Some(Window {
                    at,
                    offset: lag / RATE as f64,
                    score,
                    fine: true,
                }),
                None if score >= MIN_MATCH => Some(Window {
                    at,
                    offset: by_loudness,
                    score,
                    fine: false,
                }),
                None => None,
            };
            if let Some(answer) = answer {
                around = answer.offset;
                found.push(answer);
            }
        }
        at += STEP_SECONDS;
    }
    found
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|x| x * x).sum::<f32>() / samples.len() as f32).sqrt()
}

fn agree(a: &Window, b: &Window) -> bool {
    let within = if a.fine && b.fine {
        AGREE_FINE
    } else {
        AGREE_COARSE
    };
    (a.offset - b.offset).abs() <= within
}

/// S4: the offset of the first [`MIN_WINDOWS`] windows in a row that agree, every later jump, and a drift.
pub fn verdict(found: &[Window]) -> Verdict {
    let Some(first) = (0..found.len()).find(|&i| {
        i + MIN_WINDOWS <= found.len()
            && found[i..i + MIN_WINDOWS].windows(2).all(|pair| {
                agree(&pair[0], &pair[1]) && pair[1].at - pair[0].at <= STEP_SECONDS * 2.5
            })
    }) else {
        return Verdict::NotFound;
    };
    let mut run: Vec<f64> = found[first..first + MIN_WINDOWS]
        .iter()
        .map(|window| window.offset)
        .collect();
    run.sort_by(f64::total_cmp);
    let offset = run[run.len() / 2];

    // Jumps: two windows in a row away from the offset in force, agreeing with each other.
    let mut jumps = Vec::new();
    let mut in_force = offset;
    let mut steady_until = first;
    for i in first + 1..found.len() {
        let away = found[i].offset - in_force;
        if away.abs() <= JUMP_SECONDS {
            if jumps.is_empty() {
                steady_until = i;
            }
            // A slow drift is followed, so it is never taken for a jump.
            in_force = found[i].offset;
            continue;
        }
        if found.get(i + 1).is_some_and(|next| agree(&found[i], next)) {
            jumps.push(Jump {
                at: found[i].at,
                by: away,
            });
            in_force = found[i].offset;
        }
    }

    // Drift: the slope over the stretch before the first jump.
    let (from, to) = (&found[first], &found[steady_until]);
    let span = to.at - from.at;
    let drift_ms_per_minute = (span >= 60.0)
        .then(|| (to.offset - from.offset) / span)
        .filter(|slope| slope.abs() * 1e6 > DRIFT_PPM)
        .map(|slope| slope * 60.0 * 1000.0);

    Verdict::Found {
        offset,
        jumps,
        drift_ms_per_minute,
    }
}

/// The whole measurement of `other` against `reference`, both mono at [`RATE`]. `expected` is the offset
/// their names already account for (the reference's place on the session clock minus the other's, 0 for
/// two unstamped files); the answer is in each file's own seconds.
pub fn measure(reference: &[f32], other: &[f32], expected: f64) -> Verdict {
    let Some((rough, _)) = coarse(
        &envelope(reference),
        &envelope(other),
        expected,
        SEARCH_SECONDS,
    ) else {
        return Verdict::NotFound;
    };
    verdict(&windows(reference, other, rough))
}

// ---------------------------------------------------------------------------------------------------------
// What is done about an offset.

/// S5: what happens to a file that is `offset` seconds off.
#[derive(Debug, Clone, PartialEq)]
pub enum Move {
    /// Under the threshold: nothing is written.
    Aligned,
    /// A `.flac` or `.wav` in `sources/`: written again as itself.
    Sound,
    /// A picture in `sources/`: encoded again.
    Picture,
    /// Sound in a lossy format: converting it changes its name, which this build does not do yet.
    LossySound,
    /// Not inside the project's `sources/`: never touched.
    Outside,
}

pub fn decide(path: &Path, sources_dir: &Path, offset: f64) -> Move {
    let video = crate::add_sources::is_video(path);
    let threshold = if video {
        VIDEO_ALIGNED_SECONDS
    } else {
        ALIGNED_SECONDS
    };
    if offset.abs() < threshold {
        return Move::Aligned;
    }
    if path.parent() != Some(sources_dir) {
        return Move::Outside;
    }
    if video {
        return Move::Picture;
    }
    match extension(path).as_str() {
        "flac" | "wav" => Move::Sound,
        _ => Move::LossySound,
    }
}

fn extension(path: &Path) -> String {
    path.extension()
        .map(|ext| ext.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// S5's ffmpeg arguments for sound: `offset` seconds of silence in front, or that much off the start, every
/// sample otherwise as it was. `sample_rate` is the file's own, `codec` its own for a `.wav`.
pub fn sound_args(
    from: &Path,
    to: &Path,
    offset: f64,
    sample_rate: u32,
    codec: &str,
) -> Vec<String> {
    let samples = (offset.abs() * sample_rate as f64).round() as u64;
    let filter = if offset > 0.0 {
        format!("adelay={samples}S:all=1")
    } else {
        format!("atrim=start_sample={samples},asetpts=PTS-STARTPTS")
    };
    let mut args: Vec<String> = ["-v", "error", "-y", "-i"].map(String::from).to_vec();
    args.push(from.display().to_string());
    args.extend(["-map", "0:a", "-af"].map(String::from));
    args.push(filter);
    args.push("-c:a".into());
    args.push(codec.to_string());
    args.push(to.display().to_string());
    args
}

/// S5's ffmpeg arguments for a picture: black in front or its start off, its own frame times kept, its
/// sound moved by the same amount.
pub fn picture_args(from: &Path, to: &Path, offset: f64) -> Vec<String> {
    let seconds = format!("{:.6}", offset.abs());
    let millis = format!("{:.3}", offset.abs() * 1000.0);
    let (video, audio) = if offset > 0.0 {
        (
            format!("tpad=start_duration={seconds}:color=black"),
            format!("adelay={millis}:all=1"),
        )
    } else {
        (
            format!("trim=start={seconds},setpts=PTS-STARTPTS"),
            format!("atrim=start={seconds},asetpts=PTS-STARTPTS"),
        )
    };
    // FLAC rides in Matroska; the other containers take AAC.
    let audio_codec: &[&str] = if extension(from) == "mkv" {
        &["-c:a", "flac"]
    } else {
        &["-c:a", "aac", "-b:a", "256k"]
    };
    let mut args: Vec<String> = ["-v", "error", "-y", "-i"].map(String::from).to_vec();
    args.push(from.display().to_string());
    args.extend(["-map", "0:v:0", "-map", "0:a?", "-vf"].map(String::from));
    args.push(video);
    args.push("-af".into());
    args.push(audio);
    args.extend(
        [
            "-fps_mode",
            "passthrough",
            "-c:v",
            "libx264",
            "-preset",
            "fast",
            "-crf",
        ]
        .map(String::from),
    );
    args.push(VIDEO_CRF.to_string());
    args.extend(audio_codec.iter().map(|part| part.to_string()));
    args.push(to.display().to_string());
    args
}

/// "X.XXX s after" / "X.XXX s before": which way a file is off.
fn started(offset: f64) -> String {
    if offset > 0.0 {
        format!("{:.3} s after", offset)
    } else {
        format!("{:.3} s before", -offset)
    }
}

/// S5's sentence for a file that was written again.
pub fn moved_line(name: &str, reference: &str, offset: f64, how: &Move) -> String {
    let what = match (how, offset > 0.0) {
        (Move::Picture, true) => format!(
            "the copy in sources/ is encoded again with that much black in front (one generation, CRF {VIDEO_CRF}; the recording itself is untouched)"
        ),
        (Move::Picture, false) => format!(
            "the copy in sources/ is encoded again with that much taken off its start (one generation, CRF {VIDEO_CRF}; the recording itself is untouched)"
        ),
        (_, true) => "that much silence is put in front of the copy in sources/".to_string(),
        (_, false) => "that much is taken off the start of the copy in sources/".to_string(),
    };
    format!(
        ">>> sync: {name} started {} {reference} -- {what}",
        started(offset)
    )
}

// ---------------------------------------------------------------------------------------------------------
// The door.

/// The stage: one row for the whole session, planned whenever two sources could share seconds. Whether
/// they do is only known once they are decoded, so a session with nothing to line up runs it and says
/// nothing (S1).
pub fn stage(project: &Project) -> run_stages::Stage {
    let tasks = if project.sources.len() >= 2 {
        vec![run_stages::Task {
            label: "lining up the recordings".to_string(),
            program: SYNC_TASK.to_string(),
            args: Vec::new(),
            already_done: false,
            session: None,
        }]
    } else {
        Vec::new()
    };
    run_stages::Stage {
        track: crate::runqueue::TRACK_FRAMES,
        name: "sync",
        phase: 1,
        of: 2,
        kind: "file",
        tasks,
    }
}

/// The walk's door: resolve the session as the app does, then run the pass over the project on disk.
pub fn run_task(_task: &run_stages::Task, log: &mut impl FnMut(&str)) -> Result<(), String> {
    let session = crate::startup::open_dir();
    let tree = Tree::new(&session)
        .map_err(|_| format!("{}: not a project folder to line up in", session.display()))?;
    let project = crate::project::load(&session).map_err(|err| err.to_string())?;
    let configured = crate::settings::from_environment()
        .map(|paths| crate::settings::read(&paths).unwrap_or_default().ffmpeg)
        .unwrap_or_default();
    let tools = Tools {
        ffmpeg: crate::separate_media::program(&configured),
        ffprobe: crate::services::ffprobe_for(&configured, "ffprobe"),
    };
    pass(&tree, &project, &tools, log)
}

pub struct Tools {
    pub ffmpeg: String,
    pub ffprobe: String,
}

/// One source of the session as the pass sees it.
struct Heard {
    name: String,
    path: PathBuf,
    footage: bool,
    /// §4's sound button, off: this video is to hear the separate recording (F1.15).
    sound_off: bool,
    /// Its place on the session clock, from its name.
    placed: f64,
    samples: Vec<f32>,
}

impl Heard {
    fn seconds(&self) -> f64 {
        self.samples.len() as f64 / RATE as f64
    }
    fn shares_with(&self, other: &Heard) -> bool {
        self.placed < other.placed + other.seconds() && other.placed < self.placed + self.seconds()
    }
}

/// S1–S6 over a project on disk.
pub fn pass(
    tree: &Tree,
    project: &Project,
    tools: &Tools,
    log: &mut impl FnMut(&str),
) -> Result<(), String> {
    let names: Vec<String> = project
        .sources
        .iter()
        .map(|source| crate::audio_task::source_lane(&source.path))
        .collect();
    let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();
    let clock = crate::clock::clock(&borrowed);

    // S3: every source decoded whole, never by seeking.
    let mut heard = Vec::new();
    for (index, source) in project.sources.iter().enumerate() {
        if crate::cancel_leg::cancelled() {
            return Ok(());
        }
        let path = layout::resolve(tree.dir(), tree.dir(), &source.path);
        if !path.exists() {
            continue;
        }
        // A file with no sound, or one ffmpeg cannot read, has nothing to be lined up by: it stays where
        // its name puts it and the run goes on (the step that needs the file names it).
        let samples = match decode(tree, &path, &names[index], tools, log) {
            Ok(samples) if samples.len() >= RATE => samples,
            Ok(_) => continue,
            Err(why) => {
                log(&format!(
                    ">>> sync: {} has no sound to line it up by -- left where its name puts it ({})",
                    names[index],
                    why.lines().last().unwrap_or_default()
                ));
                continue;
            }
        };
        heard.push(Heard {
            name: names[index].clone(),
            path,
            footage: source.footage,
            sound_off: source.sound_off && crate::add_sources::is_video(Path::new(&source.path)),
            placed: clock.offset(index),
            samples,
        });
    }
    if heard.len() < 2 {
        return lend(tree, &heard, &[], tools, log);
    }

    // S2: the reference is the first footage row, else the first row.
    let reference = heard.iter().position(|source| source.footage).unwrap_or(0);
    let sources_dir = tree.dir().join("sources");
    let mut placed_ok = vec![false; heard.len()];
    placed_ok[reference] = true;
    for index in 0..heard.len() {
        if index == reference {
            continue;
        }
        if crate::cancel_leg::cancelled() {
            return Ok(());
        }
        // Against the reference when they share seconds, else against the first placed source that does.
        let Some(against) = std::iter::once(reference)
            .chain(0..heard.len())
            .find(|&other| {
                other != index && placed_ok[other] && heard[index].shares_with(&heard[other])
            })
        else {
            continue;
        };
        let (this, that) = (&heard[index], &heard[against]);
        let expected = this.placed - that.placed;
        let found = measure(&that.samples, &this.samples, expected);
        let Verdict::Found {
            offset,
            jumps,
            drift_ms_per_minute,
        } = found
        else {
            log(&format!(
                "!!! sync: {} could not be placed against {} by its sound -- it stays where its name puts it; slide it on the Cut page",
                this.name, that.name
            ));
            continue;
        };
        // What is left once the names have placed both.
        let left = offset - expected;
        for jump in &jumps {
            log(&format!(
                ">>> sync: {}'s own sound jumps against {} around {} ({:+.2} s)",
                that.name,
                this.name,
                crate::tools::mm_ss(jump.at),
                jump.by
            ));
        }
        if let Some(drift) = drift_ms_per_minute {
            log(&format!(
                ">>> sync: {} drifts {:.1} ms a minute against {} -- lined up at the start",
                this.name, drift, that.name
            ));
        }
        let how = decide(&this.path, &sources_dir, left);
        match how {
            Move::Aligned => log(&format!(
                ">>> sync: {} already lines up with {}",
                this.name, that.name
            )),
            Move::Outside => log(&format!(
                "!!! sync: {} is {:.3} s off {} and is not in the project's sources/ -- add it again to have it lined up",
                this.name,
                left.abs(),
                that.name
            )),
            Move::LossySound => log(&format!(
                "!!! sync: {} started {} {} -- a .{} is not written again yet; slide it on the Cut page",
                this.name,
                started(left),
                that.name,
                extension(&this.path)
            )),
            Move::Sound | Move::Picture => {
                write_again(&this.path, left, &how, tools, log)?;
                log(&moved_line(&this.name, &that.name, left, &how));
                let inputs = tree.input_dir(&this.name);
                if inputs.exists() && std::fs::remove_dir_all(&inputs).is_ok() {
                    log(&format!(
                        ">>> sync: {}'s transcript and frames are made again, on its new start",
                        this.name
                    ));
                }
                placed_ok[index] = true;
                // The samples in hand are the old file's; nothing later reads them as placed sound
                // except as a reference, so they are moved the same way.
                let moved = shifted(&heard[index].samples, left);
                heard[index].samples = moved;
                continue;
            }
        }
        placed_ok[index] = matches!(how, Move::Aligned);
    }
    lend(tree, &heard, &placed_ok, tools, log)
}

// ---------------------------------------------------------------------------------------------------------
// F1.15: a video whose own sound is off hears the separate recording.

/// The tag a camera's copy carries once it has been given a recording's sound, so a second ▶ knows.
pub const SOUND_TAG: &str = "NAIVEPOST_SOUND";

/// `prepare/same_sound.tsv`: one line `‹base›\t‹carrier›` for every row whose sound is the same as the
/// carrier's and is therefore not transcribed again.
pub fn same_sound_path(tree: &Tree) -> PathBuf {
    tree.prepare_dir().join("same_sound.tsv")
}

/// The row whose words stand for `base`'s, when `base` carries the same sound as another row.
pub fn same_as(tree: &Tree, base: &str) -> Option<String> {
    let listed = std::fs::read_to_string(same_sound_path(tree)).ok()?;
    listed.lines().find_map(|line| {
        let (row, carrier) = line.split_once('\t')?;
        (row == base).then(|| carrier.to_string())
    })
}

/// The sound a container takes without loss where it can: FLAC in Matroska, ALAC in MP4 and MOV.
fn carried_codec(camera: &Path) -> &'static [&'static str] {
    match extension(camera).as_str() {
        "mkv" => &["-c:a", "flac"],
        "mp4" | "mov" => &["-c:a", "alac"],
        "webm" => &["-c:a", "libopus", "-b:a", "192k"],
        _ => &["-c:a", "aac", "-b:a", "256k"],
    }
}

/// F1.15 S2's ffmpeg arguments: the camera's pictures copied as they are, the recording's sound in place
/// of its own, cut to the camera's own seconds. `lead` is where the camera's second 0 falls in the
/// recording (negative: the recording starts after the camera does).
pub fn lend_args(
    camera: &Path,
    recording: &Path,
    to: &Path,
    lead: f64,
    camera_seconds: f64,
    recording_name: &str,
) -> Vec<String> {
    let placed = if lead >= 0.0 {
        format!("atrim=start={lead:.6},asetpts=PTS-STARTPTS")
    } else {
        format!("adelay={:.3}:all=1", -lead * 1000.0)
    };
    let mut args: Vec<String> = ["-v", "error", "-y", "-i"].map(String::from).to_vec();
    args.push(camera.display().to_string());
    args.push("-i".into());
    args.push(recording.display().to_string());
    args.extend(["-map", "0:v", "-map", "1:a:0", "-c:v", "copy", "-af"].map(String::from));
    args.push(format!("{placed},atrim=end={camera_seconds:.6}"));
    args.extend(carried_codec(camera).iter().map(|part| part.to_string()));
    args.push("-metadata".into());
    args.push(format!("{SOUND_TAG}={recording_name}"));
    args.push(to.display().to_string());
    args
}

/// The sound-derived files of one row: gone when the sound it was transcribed from is.
fn forget_transcript(tree: &Tree, base: &str) {
    for file in [
        tree.voice16k_wav(base),
        tree.input_dir(base).join("voice16k.clock"),
        tree.words_json(base),
        tree.asrchunks_json(base),
        tree.words_aligned_json(base),
        tree.turns_json(base),
        tree.transcript_txt(base),
        tree.transcript_tsv(base),
        tree.transcript_srt(base),
    ] {
        let _ = std::fs::remove_file(file);
    }
}

/// F1.15: every video with its sound off is given the sound of the separate recording that shares the
/// most seconds with it, and `same_sound.tsv` says which rows now carry one sound, so it is transcribed
/// once. `placed_ok[i]` is whether row `i` lines up (an unplaced recording is lent to nobody).
fn lend(
    tree: &Tree,
    heard: &[Heard],
    placed_ok: &[bool],
    tools: &Tools,
    log: &mut impl FnMut(&str),
) -> Result<(), String> {
    let before = std::fs::read_to_string(same_sound_path(tree)).unwrap_or_default();
    let mut same: Vec<(String, String)> = Vec::new();
    // The first camera given a recording's sound carries the words for everyone with that sound.
    let mut carriers: Vec<(usize, String)> = Vec::new();
    for camera in heard.iter().filter(|source| source.sound_off) {
        if crate::cancel_leg::cancelled() {
            return Ok(());
        }
        let shared = |other: &Heard| {
            (camera.placed + camera.seconds()).min(other.placed + other.seconds())
                - camera.placed.max(other.placed)
        };
        let lender = heard
            .iter()
            .enumerate()
            .filter(|(index, other)| {
                !crate::add_sources::is_video(&other.path)
                    && placed_ok.get(*index).copied().unwrap_or(false)
                    && shared(other) > 0.0
            })
            .max_by(|a, b| shared(a.1).total_cmp(&shared(b.1)));
        let Some((lender_index, lender)) = lender else {
            log(&format!(
                ">>> [{}] its own sound is off and no separate recording runs over it -- its own sound is used",
                camera.name
            ));
            continue;
        };
        if carried_tag(&camera.path, tools, log).as_deref() != Some(lender.name.as_str()) {
            let mut flight = camera.path.as_os_str().to_os_string();
            flight.push(format!(".part.{}", extension(&camera.path)));
            let flight = PathBuf::from(flight);
            let args = lend_args(
                &camera.path,
                &lender.path,
                &flight,
                camera.placed - lender.placed,
                camera.seconds(),
                &lender.name,
            );
            let flags: Vec<&str> = args.iter().map(String::as_str).collect();
            if let Err(err) = crate::subprocess::run_listed(&tools.ffmpeg, &flags, |line| log(line))
            {
                let _ = std::fs::remove_file(&flight);
                return Err(format!(
                    "{}: could not be given {}'s sound -- {err}",
                    camera.name, lender.name
                ));
            }
            std::fs::rename(&flight, &camera.path)
                .map_err(|err| format!("{}: {err}", camera.path.display()))?;
            forget_transcript(tree, &camera.name);
            log(&format!(
                ">>> sound: {} now carries {}'s sound -- its own is replaced in the copy in sources/, the pictures are as they were",
                camera.name, lender.name
            ));
        }
        match carriers.iter().find(|(index, _)| *index == lender_index) {
            Some((_, carrier)) => same.push((camera.name.clone(), carrier.clone())),
            None => {
                carriers.push((lender_index, camera.name.clone()));
                same.push((lender.name.clone(), camera.name.clone()));
            }
        }
    }
    let now: String = same
        .iter()
        .map(|(row, carrier)| format!("{row}\t{carrier}\n"))
        .collect();
    if now != before {
        // A row that joins or leaves the list was transcribed (or written up as silence) under the other
        // rule: its transcript is made again.
        let listed = |text: &str| -> Vec<String> {
            text.lines()
                .filter_map(|line| line.split_once('\t').map(|(row, _)| row.to_string()))
                .collect()
        };
        let (was, is) = (listed(&before), listed(&now));
        for base in was
            .iter()
            .filter(|base| !is.contains(base))
            .chain(is.iter().filter(|base| !was.contains(base)))
        {
            forget_transcript(tree, base);
        }
        if now.is_empty() {
            let _ = std::fs::remove_file(same_sound_path(tree));
        } else {
            std::fs::create_dir_all(tree.prepare_dir())
                .and_then(|()| std::fs::write(same_sound_path(tree), &now))
                .map_err(|err| format!("{}: {err}", same_sound_path(tree).display()))?;
        }
    }
    for (row, carrier) in &same {
        log(&format!(
            ">>> sound: {row} is the same sound as {carrier} -- transcribed once, as {carrier}'s"
        ));
    }
    Ok(())
}

/// The recording a camera's copy says it carries the sound of.
fn carried_tag(path: &Path, tools: &Tools, log: &mut impl FnMut(&str)) -> Option<String> {
    let shown = path.display().to_string();
    let args = [
        "-v",
        "error",
        "-show_entries",
        "format_tags",
        "-of",
        "default=nw=1",
        &shown,
    ];
    let answer = crate::subprocess::run(&tools.ffprobe, &args, |line| log(line)).ok()?;
    answer.lines().find_map(|line| {
        let (key, value) = line.trim().strip_prefix("TAG:")?.split_once('=')?;
        key.eq_ignore_ascii_case(SOUND_TAG)
            .then(|| value.to_string())
    })
}

/// The samples as the written-again file will hold them.
fn shifted(samples: &[f32], offset: f64) -> Vec<f32> {
    let count = (offset.abs() * RATE as f64).round() as usize;
    if offset > 0.0 {
        let mut out = vec![0.0; count];
        out.extend_from_slice(samples);
        out
    } else {
        samples[count.min(samples.len())..].to_vec()
    }
}

/// Decode a source whole to mono [`RATE`] on its own clock (as F1.3 S1 decodes its wav).
fn decode(
    tree: &Tree,
    path: &Path,
    name: &str,
    tools: &Tools,
    log: &mut impl FnMut(&str),
) -> Result<Vec<f32>, String> {
    let scratch = tree.dir().join("cache");
    std::fs::create_dir_all(&scratch).map_err(|err| format!("{}: {err}", scratch.display()))?;
    let raw = scratch.join(format!("sync-{name}.s16"));
    let shown = path.display().to_string();
    let out = raw.display().to_string();
    let rate = RATE.to_string();
    let args = [
        "-v",
        "error",
        "-y",
        "-i",
        &shown,
        "-vn",
        "-map",
        "0:a:0",
        "-af",
        "aresample=async=1:first_pts=0",
        "-ac",
        "1",
        "-ar",
        &rate,
        "-f",
        "s16le",
        &out,
    ];
    let ran = crate::subprocess::run_listed(&tools.ffmpeg, &args, |line| log(line));
    let bytes = std::fs::read(&raw);
    let _ = std::fs::remove_file(&raw);
    ran.map_err(|err| format!("{name}: its sound could not be read to line it up -- {err}"))?;
    let bytes = bytes.map_err(|err| format!("{}: {err}", raw.display()))?;
    Ok(bytes
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as f32 / 32768.0)
        .collect())
}

/// S5: the copy written again through a file in flight beside it, then moved onto its name.
fn write_again(
    path: &Path,
    offset: f64,
    how: &Move,
    tools: &Tools,
    log: &mut impl FnMut(&str),
) -> Result<(), String> {
    let mut flight = path.as_os_str().to_os_string();
    flight.push(format!(".part.{}", extension(path)));
    let flight = PathBuf::from(flight);
    let args = match how {
        Move::Picture => picture_args(path, &flight, offset),
        _ => {
            let (rate, codec) = sound_facts(path, tools, log)?;
            // A FLAC is packed again as FLAC; a WAV keeps the samples' own width.
            let codec = if extension(path) == "flac" {
                "flac".to_string()
            } else {
                codec
            };
            sound_args(path, &flight, offset, rate, &codec)
        }
    };
    let flags: Vec<&str> = args.iter().map(String::as_str).collect();
    if let Err(err) = crate::subprocess::run_listed(&tools.ffmpeg, &flags, |line| log(line)) {
        let _ = std::fs::remove_file(&flight);
        return Err(format!(
            "{}: could not be written again to line it up -- {err}",
            path.display()
        ));
    }
    std::fs::rename(&flight, path).map_err(|err| format!("{}: {err}", path.display()))
}

/// The file's own sample rate and codec, so the samples are moved by a whole number of them and a WAV
/// stays the width it was.
fn sound_facts(
    path: &Path,
    tools: &Tools,
    log: &mut impl FnMut(&str),
) -> Result<(u32, String), String> {
    let shown = path.display().to_string();
    let args = [
        "-v",
        "error",
        "-select_streams",
        "a:0",
        "-show_entries",
        "stream=codec_name,sample_rate",
        "-of",
        "default=nw=1",
        &shown,
    ];
    let answer = crate::subprocess::run(&tools.ffprobe, &args, |line| log(line))?;
    let field = |key: &str| {
        answer
            .lines()
            .find_map(|line| line.trim().strip_prefix(key))
            .map(str::to_string)
    };
    let rate = field("sample_rate=")
        .and_then(|text| text.parse::<u32>().ok())
        .ok_or_else(|| format!("{}: ffprobe named no sample rate", path.display()))?;
    let codec = field("codec_name=")
        .ok_or_else(|| format!("{}: ffprobe named no codec", path.display()))?;
    Ok((rate, codec))
}
