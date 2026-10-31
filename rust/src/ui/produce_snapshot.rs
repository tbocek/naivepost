//! F5.1 S3 — the run's snapshot, gathered ONCE on the GUI thread.
//!
//! §S3 lists what the press owns when it starts: "the cut, lines, settings, sources, publish state,
//! aspect". A render walks subprocesses for minutes, so a leg that re-read the page would describe whatever
//! the person had moved by then. The one file read that stays is `publish.json` when the page holds no
//! publish state (§S4 needs "nothing was ever written" versus "an earlier run wrote this"; see [`publish_of`]).

use crate::cut::Cut;
use crate::layout::Tree;
use crate::narration::Entry;
use crate::project::{Project, Publish};
use crate::ui::produce_page::ProduceState;
use crate::word_list::Word;

/// What the run was told when it started. Public fields: the rules read it.
pub struct Snapshot {
    pub tree: Tree,
    /// S3's cut: the Cut page's own if it holds one, else `cut/cut.json` (`produce_flow::what_is_the_cut`).
    pub cut: Cut,
    /// S3's lines: `narrate/narration.json`, in playing order.
    pub lines: Vec<Entry>,
    pub settings: crate::project::Produce,
    pub sources: Vec<crate::project::Source>,
    /// The session's aligned words (F1.13's list), which §F5.4 S1 reads for a clip's own speech.
    pub session_words: Vec<Word>,
    /// The narrator's own recordings, excluded from a clip's speech by §F5.4 (the narration replaced their words).
    pub narrator_sources: Vec<String>,
    /// The first footage recording's frame, which §9 makes the whole video's frame.
    pub src_shape: (i32, i32),
    pub on_recording: bool,
    /// The filmed spans, so a clip's `footage_left` is answered without the page.
    pub filmed_runs: Vec<(f64, f64)>,
    /// The recordings on the session clock, by name: which file a footage clip is cut from, and where.
    pub recordings: Vec<crate::timeline::Recording>,
    /// S3's publish state, and S4's distinction: `None` = nothing was ever written.
    pub publish: Option<Publish>,
    /// The page's own thumbnail rows, merged into the held publish state by [`publish_of`].
    pub publish_written: bool,
    /// The session's language as the press found it (§1's "own language never translated"), read on the window
    /// thread because the render's worker holds no window session (§03-shell §1).
    pub session_language: String,
}

impl Snapshot {
    /// The cut's aspect, the frame shape §S3's "aspect" means.
    pub fn aspect(&self) -> String {
        self.cut.aspect.clone()
    }

    /// Seconds of footage still there past the end of `seg` (F4.3: "grow the clip, ≤ 4 s, NEVER PAST THE
    /// FOOTAGE"). A clip in no filmed run, or a session with no tape, answers through
    pub fn footage_left(&self, seg: &crate::cut::Seg) -> f64 {
        let covering = self
            .filmed_runs
            .iter()
            .filter(|(start, _end)| *start <= seg.s)
            .map(|(_start, end)| end)
            .copied()
            .max_by(f64::total_cmp);
        match covering {
            Some(run_end) => (run_end - seg.e).max(0.0),
            None => crate::produce_render::unknown_footage(seg),
        }
    }
}

/// Read everything the run will need, once. `recordings` are the page's session recordings, `publish` is
/// the project's held publish state, which may be absent.
pub fn gather(
    tree: &Tree,
    cut: &Cut,
    settings: &crate::project::Produce,
    project: &Project,
    recordings: &[crate::timeline::Recording],
    page_publish: Publish,
) -> Snapshot {
    let written = !page_publish.frames.is_empty()
        || !page_publish.title.trim().is_empty()
        || !page_publish.prompt.trim().is_empty();
    Snapshot {
        tree: tree.clone(),
        cut: cut.clone(),
        // S3's lines. A session never narrated has no file, which `narration::load` treats as no lines.
        lines: crate::narration::load(tree)
            .map(|n| n.entries)
            .unwrap_or_default(),
        settings: settings.clone(),
        sources: project.sources.clone(),
        // S3's publish state: the PAGE's rows, merged over what a previous step wrote ([`publish_of`]).
        session_words: crate::word_list::load(tree)
            .ok()
            .flatten()
            .unwrap_or_default(),
        narrator_sources: project
            .sources
            .iter()
            .filter(|src| src.narrator != 0)
            .map(|src| src.path.clone())
            .collect(),
        src_shape: first_footage_shape(tree, recordings),
        on_recording: !recordings.is_empty(),
        filmed_runs: crate::timeline::filmed_runs(recordings),
        recordings: recordings.to_vec(),
        publish: Some(page_publish),
        // S4's three shapes turn on whether anything was ever written; the page's rows answer that directly.
        publish_written: written,
        session_language: project.policy.language.value.clone(),
    }
}

/// S3's publish state, built from what the page holds (image row, edit instruction, title, printed words,
/// description). Reading `publish.json` instead would describe what an earlier run left, not what is on screen.
///
/// `crop` has no page row and keeps its default: the merge below takes it from the record.
///
/// Lives here, not beside the rows in [`crate::ui::produce_page`], which is at its size budget.
pub fn publish_of_state(s: &ProduceState) -> Publish {
    Publish {
        frames: s.frames.clone(),
        own: s.own,
        texts: s.texts.clone(),
        title: s.title.clone(),
        thumb_title: s.thumb_title.clone(),
        title_seeded: s.title_seeded,
        title_box: s.title_box,
        prompt: s.instruction.clone(),
        negative: s.negative.clone(),
        description: s.description.clone(),
        ..Default::default()
    }
}

/// The publish state the words half works from: what the page holds, merged over what `publish.json` already
/// records. §S4 turns on "when `publish.json` exists", so neither side alone is right: the rows are what
/// §S3 means, but a fresh session's file has none of that; the file carries what no page row spells (the
/// crop, the title box, `frames` from an earlier `＋ Add frame…` or redraw), which rows alone would drop.
///
/// A run built without any publish state (a bare `Run`) falls back to the file.
pub fn publish_of(snap: &Snapshot) -> Publish {
    let on_disk = crate::publish::load(&snap.tree).unwrap_or_default();
    merge_publish(snap.publish.clone(), &on_disk)
}

/// The two-record half of [`publish_of`], so "which side wins" is answerable without a folder or a window:
/// `held` is what the page describes, `on_disk` what an earlier step wrote.
pub fn merge_publish(held: Option<Publish>, on_disk: &Publish) -> Publish {
    match held {
        Some(mut held) => {
            if held.frames.is_empty() {
                held.frames = on_disk.frames.clone();
            }
            if held.crop == crate::project::Crop::default() {
                held.crop = on_disk.crop;
            }
            if held.title_box.is_none() {
                held.title_box = on_disk.title_box;
            }
            held
        }
        None => on_disk.clone(),
    }
}

/// The file a footage clip at session second `at` is cut from, and the second inside it to seek to: the
/// recording whose span holds `at`, resolved against the project, `at` counted from that recording's start
/// (plus the cut's hand shift for it). Several recordings must not all come from the first footage row.
fn footage_file(snap: &Snapshot, at: f64) -> Option<(String, f64)> {
    let resolve = |stored: &str| {
        crate::layout::resolve(snap.tree.dir(), snap.tree.dir(), stored)
            .to_string_lossy()
            .to_string()
    };
    let recording = snap
        .recordings
        .iter()
        .find(|rec| rec.start <= at + 1e-6 && at < rec.end)
        .or_else(|| snap.recordings.iter().rfind(|rec| rec.start <= at));
    if let Some(rec) = recording {
        if let Some(source) = snap
            .sources
            .iter()
            .filter(|src| src.footage)
            .find(|src| crate::audio_task::source_lane(&src.path) == rec.base)
        {
            let seek = at - rec.start + crate::timeline::shift_of(&snap.cut, &rec.base);
            return Some((resolve(&source.path), seek.max(0.0)));
        }
    }
    // No tape known (a session the page never laid out): the one footage row, on its own clock from 0.
    snap.sources
        .iter()
        .find(|src| src.footage)
        .map(|src| (resolve(&src.path), at.max(0.0)))
}

/// F5.2's half of the run, driven from the snapshot: plan the encode, walk the subprocesses, report what was
/// made. Every lookup is answered from `snap` alone (no `session_tree()`, `live_project()` or re-read),
/// because §S3 says the press owns its inputs when it starts. Called from the run's render closure.
pub fn render_half(
    run: &crate::produce_flow::Run,
    snap: &Snapshot,
) -> crate::produce_flow::Rendered {
    let root = std::env::current_dir().unwrap_or_default();
    let render = crate::produce_exec::run_render(
        run,
        &snap.tree,
        &crate::produce_exec::Materials {
            // Through §1's one path rule, as the clip's own input is: a stored `project:` is no file name.
            exists: &|file| {
                crate::cut::Seg {
                    ins: file.to_string(),
                    ..Default::default()
                }
                .insert_asset(&root, snap.tree.dir())
                .is_some_and(|path| path.exists())
            },
            // S3's effects: the cut's own list. `plan_clips` filters per scene (`effect_overlap::drawn_on_scene`), so
            // the whole list drops nothing; an empty one would render a cut with no zoom, volume or text.
            fx_for: &|_| snap.cut.fx.clone(),
            lanes: &snap.cut.lanes,
            // §9: the first footage clip's frame is the whole video's; §10's resolution can only shrink it.
            // `frame_box` falls back to this when the cut names no aspect.
            src_shape: snap.src_shape,
            wav_of: &|entry| {
                crate::produce_stamp::wav_of(&snap.tree, entry, Some(&run.voice), entry.roll)
            },
            // Only the LANGUAGES are handed over: the render cuts the cues from the clips it planned
            // (`produce_exec::plan`), since a cue needs the clip's rate, placed lines and words.
            cues: crate::produce_exec::Cues {
                languages: run.settings.translate.clone(),
                // §1's "own language never translated": the page has hidden its tick, so only the render can catch a setting that still carries it.
                session_language: snap.session_language.clone(),
                ..Default::default()
            },
            // S1 (§F5.4): the clip's own speech off the session word list; the narrator's recordings are excluded
            // because the narration replaced their words.
            words: &|clip| {
                crate::produce_clip_cues::words_for(
                    clip,
                    &snap.session_words,
                    &snap.narrator_sources,
                )
            },
            translate: &crate::ui::produce_page_rows::translate_leg(),
            // §F5.2 S5's size question over the real wire: ffprobe beside the configured ffmpeg. No ffprobe gives
            // `None` everywhere and the run says nothing about sizes.
            probe: &crate::produce_probe::size_of,
            sources: &|| snap.sources.clone(),
            // §8 bullet 4: does this session have tape at all.
            on_recording: snap.on_recording,
            footage_left: &|seg: &crate::cut::Seg| snap.footage_left(seg),
            // F4.3: speech length is the wav's duration.
            wav_seconds: &|entry| {
                crate::produce_stamp::wav_of(&snap.tree, entry, Some(&run.voice), entry.roll)
                    .as_ref()
                    .and_then(|wav| crate::voice_ref::wav_seconds(std::path::Path::new(wav)))
            },
            // S5's picture input: an insert reads its own asset; footage and a `copy:` stretch read the first footage recording.
            source_file: &|clip| {
                if clip.source.is_empty() || clip.source.starts_with("copy:") {
                    let from = clip
                        .source
                        .strip_prefix("copy:")
                        .and_then(|second| second.parse().ok())
                        .unwrap_or(clip.seg_s);
                    return footage_file(snap, from);
                }
                // `Seg::insert_asset` is the one path rule of §1; the clip carries the same spelling.
                Some(
                    crate::cut::Seg {
                        ins: clip.source.clone(),
                        ..Default::default()
                    }
                    .insert_asset(&root, snap.tree.dir())
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|| clip.source.clone()),
                )
                .map(|path| (path, clip.ss))
            },
            // §6 bullet 1's doors: the filmed spans already gathered, and the name a clip's base goes by (the footage
            // row's for footage and `copy:`, the asset's own for an insert).
            filmed_runs: &snap.filmed_runs,
            base_of: &|seg: &crate::cut::Seg| {
                // Footage is named by the recording the clip starts in, the file its encode opens.
                let path = if seg.ins.is_empty() || seg.ins.starts_with("copy:") {
                    footage_file(snap, seg.s)
                        .map(|(path, _)| path)
                        .unwrap_or_default()
                } else {
                    seg.insert_asset(&root, snap.tree.dir())
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| seg.ins.clone())
                };
                crate::cut_insert::base_name(&path)
            },
        },
        crate::ui::window::log_line,
        // §F5.2 S2's voice: the run's, so a take spoken by the render is the take its stamp weighs.
        move |line| crate::ui::produce_page_rows::speak_a_line(line, &run.voice),
        crate::ui::produce_page_rows::render_checkpoint,
        crate::ui::produce_languages::spawn,
    );
    // A stopped or failed render says so in the log with the spec's own line, besides the ending's status word.
    if let Some(why) = crate::ui::produce_page_rows::render_failure_note(&render) {
        crate::ui::window::log_line(&crate::produce_flow::failed_log(&why));
    }
    render
}

/// The first footage recording's shape, measured as the Cut page measures its plate: through the frames
/// Prepare extracted (`cut_picture::first_footage_shape` is the same rule on the page's side). Read here
/// because that helper is private and takes a window.
///
/// With no tape or no frames the fallback is the encoder's default 1080p frame, not a guess about the cameras.
///
/// `pub` because §F5.3's stamp weighs this number (the `"frame"` group): the gate asks BEFORE the snapshot
/// is gathered, and both must hash one answer or every press would look stale.
pub fn first_footage_shape(tree: &Tree, recordings: &[crate::timeline::Recording]) -> (i32, i32) {
    const NO_TAPE: (i32, i32) = (1920, 1080);
    let Some(first) = recordings.first() else {
        return NO_TAPE;
    };
    let path = crate::ui::cut_tracks::frame_paths(tree, &first.base)
        .first()
        .cloned();
    let Some(path) = path else { return NO_TAPE };
    // The header alone: the window thread never decodes a whole frame to learn two numbers (§03-shell §1).
    let Some((_, w, h)) = gtk4::gdk_pixbuf::Pixbuf::file_info(&path) else {
        return NO_TAPE;
    };
    // A zero edge is not a frame: a stub or truncated jpg would fail in the filter graph, not here.
    if w > 0 && h > 0 {
        (w, h)
    } else {
        NO_TAPE
    }
}
