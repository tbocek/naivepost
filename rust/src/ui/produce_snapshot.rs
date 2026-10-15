//! F5.1 S3 — the run's snapshot, gathered ONCE on the GUI thread.
//!
//! §S3 lists what the press owns at the moment it starts: "the cut, lines, settings, sources, publish
//! state, aspect". Everything in [`Snapshot`] is one of those, read once here, before either half runs.
//! The reason is not tidiness: a render walks subprocesses for minutes, and a leg that re-read the page
//! inside the run would describe whatever the person had moved by then — a cut edited mid-encode, a
//! narration line deleted while its take was being spoken. The render closure gets `&Snapshot` and asks
//! the live page nothing.
//!
//! The one file read that stays on the disk side is `publish.json` when the page holds no publish state at
//! all: §S4's wording needs the difference between "nothing was ever written" and "an earlier run wrote
//! this", and the page only knows the second one from the file. See [`publish_of`].

use crate::cut::Cut;
use crate::layout::Tree;
use crate::narration::Entry;
use crate::project::{Project, Publish};
use crate::ui::produce_page::ProduceState;
use crate::word_list::Word;

/// What the run was told when it started. Public fields: nothing here decides anything, the rules read it.
pub struct Snapshot {
    /// The project folder the takes, the stamp and the video live in.
    pub tree: Tree,
    /// S3's cut: the Cut page's own if it holds one, else `cut/cut.json` — `produce_flow::what_is_the_cut`
    /// answers that, and its `fx` is the effect list every filter graph is built from.
    pub cut: Cut,
    /// S3's lines: `narrate/narration.json`, in playing order. Empty only because the session has nothing
    /// to say, never because a leg could not be bothered to read it.
    pub lines: Vec<Entry>,
    /// S3's settings (the encoder row as the page drew it).
    pub settings: crate::project::Produce,
    /// S3's sources, as the project held them at the press.
    pub sources: Vec<crate::project::Source>,
    /// The session's aligned words (F1.13's list), which is what §F5.4 S1 reads for a clip's own speech.
    pub session_words: Vec<Word>,
    /// The narrator's own recordings — the ones §F5.4 excludes from a clip's speech, because their words
    /// are what the narration replaced.
    pub narrator_sources: Vec<String>,
    /// The first footage recording's frame, which §9 makes the whole video's frame.
    pub src_shape: (i32, i32),
    /// §8 bullet 4: whether this session has tape at all.
    pub on_recording: bool,
    /// The filmed spans, which is how a clip's `footage_left` is answered without walking the page again.
    pub filmed_runs: Vec<(f64, f64)>,
    /// S3's publish state, and S4's distinction: `None` = nothing was ever written.
    pub publish: Option<Publish>,
    /// The page's own thumbnail rows, merged into the held publish state by [`publish_of`].
    pub publish_written: bool,
}

impl Snapshot {
    /// The cut's aspect, which is the frame shape §S3's "aspect" means.
    pub fn aspect(&self) -> String {
        self.cut.aspect.clone()
    }

    /// Seconds of footage still there past the end of `seg` — F4.3's "grow the clip, ≤ 4 s, NEVER PAST
    /// THE FOOTAGE" bound. A clip in no filmed run, or a session with no tape, answers through
    /// [`crate::produce_render::unknown_footage`]: unknown is not the same as zero.
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

/// Read everything the run will need, once. `recordings` are the page's session recordings (the caller
/// holds them), `publish` is the project's held publish state, which may be absent.
/// what the page holds: the image row, the title, the edit instruction, the printed words, the
/// description, and whether a thumbnail was chosen rather than drawn. This is §S3's "publish state".
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
        // S3's lines: `narrate/narration.json`, in playing order. A session that has never been narrated
        // has no file, which `narration::load` already says is the same state as a narration with no lines.
        lines: crate::narration::load(tree).map(|n| n.entries).unwrap_or_default(),
        settings: settings.clone(),
        sources: project.sources.clone(),
        // S3's publish state: the PAGE's rows, merged over what a previous step wrote. See [`publish_of`].
        session_words: crate::word_list::load(tree).ok().flatten().unwrap_or_default(),
        narrator_sources: project
            .sources
            .iter()
            .filter(|src| src.narrator != 0)
            .map(|src| src.path.clone())
            .collect(),
        src_shape: first_footage_shape(tree, recordings),
        on_recording: !recordings.is_empty(),
        filmed_runs: crate::timeline::filmed_runs(recordings),
        publish: Some(page_publish),
        // S4's three shapes turn on whether anything was ever written for this video; the page's rows answer
        // that directly (an image row, a title or an edit instruction means something has been described).
        publish_written: written,
    }
}

/// S3's publish state, built from what the page holds: the image row, the edit instruction, the title, the
/// words printed over the picture and the description. §S3 says the snapshot takes the "publish state", and
/// the page is what holds it — reading `publish.json` instead would describe what an earlier run left rather
/// than what the person has on screen now (which, on a fresh session, is an empty image row and no title).
///
/// The three fields no page row spells (`crop`, `title_box` and `frames`' crop half) keep their defaults: a
/// page field that does not exist cannot be published, and the crop the page draws a box from is
/// `base_shape`/`video_shape`, which a press may not write into the record it hands the render.
///
/// This is the row→record mapping the snapshot needs. It lives here rather than beside the rows in
/// [`crate::ui::produce_page`] because this file owns what a run carries and that one is at its size budget.
pub fn publish_of_state(s: &ProduceState) -> Publish {
    Publish {
        frames: s.frames.clone(),
        own: s.own,
        texts: s.texts.clone(),
        title: s.title.clone(),
        thumb_title: s.thumb_title.clone(),
        title_seeded: s.title_seeded,
        prompt: s.instruction.clone(),
        negative: s.negative.clone(),
        description: s.description.clone(),
        ..Default::default()
    }
}

/// The publish state the words half works from: what the page holds, merged over what `publish.json`
/// already records. §S4 turns on exactly this question — "when `publish.json` exists" — so a press that read
/// the file over the page's state would print "left as they are" about a thumbnail it is about to redraw,
/// and one that read the page over the file would describe a fresh session's empty image row and no title
/// as if it were what an earlier run had written.
///
/// Both halves matter. The rows are what §S3's "publish state" means — the image row, the edit
/// instruction, the title, the printed words and the description as the person sees them now — and on a
/// fresh session `publish.json` has none of that, so taking the file over the rows would ask the model to
/// draw a thumbnail from an empty instruction and no title. But the file also carries what no page row
/// spells: the crop, the title box, and `frames` left by an earlier `＋ Add frame…` or a redraw. Taking the
/// rows alone would drop those on the floor and reprint a picture the person had chosen.
///
/// A run built without any publish state at all (no page: a bare `Run`) falls back to the file.
pub fn publish_of(snap: &Snapshot) -> Publish {
    let on_disk = crate::publish::load(&snap.tree).unwrap_or_default();
    merge_publish(snap.publish.clone(), &on_disk)
}

/// The two-record half of [`publish_of`], split out so the question "which side wins" is answerable without
/// a folder or a window: `held` is what the page describes, `on_disk` what an earlier step wrote.
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

/// F5.2's half of the run, driven from the snapshot: plan the whole encode, walk the subprocesses and report
/// what was made. Every lookup `produce_exec` asks is answered from `snap` and nothing else — no
/// `session_tree()`, no `live_project()`, no word-list or recording re-read — because §S3 says the press owns
/// its inputs at the moment it starts, and a leg that asked the live page minutes later would describe
/// whatever had been moved since. Each field below names the snapshot entry it came from.
///
/// Split out of `produce_page` (which is at its size budget) and called from the run's render closure.
pub fn render_half(
    run: &crate::produce_flow::Run,
    snap: &Snapshot,
) -> crate::produce_flow::Rendered {
    let root = std::env::current_dir().unwrap_or_default();
    let render = crate::produce_exec::run_render(
        run,
        &snap.tree,
        &crate::produce_exec::Materials {
            exists: &|file| std::path::Path::new(file).exists(),
            // S3's effects: the cut's own list. `plan_clips` filters it per scene through
            // `effect_overlap::drawn_on_scene`, so handing it the whole list drops nothing and invents
            // nothing — an empty one here would render a cut with no zoom, no volume, no text, and nobody
            // would notice until the file came out flat.
            fx_for: &|_| snap.cut.fx.clone(),
            lanes: &snap.cut.lanes,
            // §9: the first footage clip's frame is the whole video's frame; §10's resolution can only
            // shrink it, and `frame_box` falls back to this when the cut names no aspect.
            src_shape: snap.src_shape,
            wav_of: &|entry| crate::produce_stamp::wav_of(&snap.tree, entry, Some(&run.voice), 0),
            cues: crate::produce_exec::Cues {
                all: Vec::new(),
                languages: run.settings.translate.clone(),
            },
            // S1 (§F5.4): the clip's own speech, off the session word list gathered at the press; the
            // narrator's own recordings are excluded there because their words are what the narration
            // replaced.
            words: &|clip| {
                crate::produce_clip_cues::words_for(clip, &snap.session_words, &snap.narrator_sources)
            },
            translate: &crate::ui::produce_page_rows::translate_leg(),
            sources: &|| snap.sources.clone(),
            // §8 bullet 4: does this session have tape at all.
            on_recording: snap.on_recording,
            // F4.3 step 1's bound, answered off the snapshot's filmed runs.
            footage_left: &|seg: &crate::cut::Seg| snap.footage_left(seg),
            // F4.3: speech length is the wav's duration.
            wav_seconds: &|entry| {
                crate::produce_stamp::wav_of(&snap.tree, entry, Some(&run.voice), 0)
                    .as_ref()
                    .and_then(|wav| crate::voice_ref::wav_seconds(std::path::Path::new(wav)))
            },
            // S5's picture input: an insert reads its own asset, footage and a `copy:` stretch read the
            // first footage recording of the snapshot's source list.
            source_file: &|clip| {
                if clip.source.is_empty() || clip.source.starts_with("copy:") {
                    return snap
                        .sources
                        .iter()
                        .find(|src| src.footage)
                        .map(|src| src.path.clone());
                }
                // `Seg::insert_asset` is the one path rule of §1; the clip carries the same spelling.
                Some(
                    crate::cut::Seg { ins: clip.source.clone(), ..Default::default() }
                        .insert_asset(&root, &root)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| clip.source.clone()),
                )
            },
        },
        crate::ui::window::log_line,
        crate::ui::produce_page_rows::speak_a_line,
        crate::ui::produce_page_rows::render_asked,
        crate::ui::produce_languages::spawn,
    );
    // A render that stopped or failed says so in the log with the spec's own line, on top of the ending's
    // word on the status line: `run_with` turns the report into "production stopped" or "production failed
    // — see log", and the reason belongs where a person can read it.
    if let Some(why) = crate::ui::produce_page_rows::render_failure_note(&render) {
        crate::ui::window::log_line(&crate::produce_flow::failed_log(&why));
    }
    render
}

/// The first footage recording's shape, measured the way the Cut page measures its plate: through the
/// frames Prepare already extracted for that recording (`cut_picture::first_footage_shape` is the same
/// rule on the page's side, and `produce_render::plan_clips` agrees through `frame_box`). Read here rather
/// than there because that helper is private to `cut_picture` and takes a window; the two lines that do the
/// work are the file read and the decode, so copying them cannot make the answer disagree.
///
/// With no tape or no frames there is nothing to measure, and the fallback is the encoder's own default
/// 1080p frame — what `frame_box` needs to be given when a session has not told it anything, not a guess
/// about this session's cameras.
fn first_footage_shape(tree: &Tree, recordings: &[crate::timeline::Recording]) -> (i32, i32) {
    const NO_TAPE: (i32, i32) = (1920, 1080);
    let Some(first) = recordings.first() else {
        return NO_TAPE;
    };
    let path = crate::ui::cut_tracks::frame_paths(tree, &first.base)
        .first()
        .cloned();
    let Some(path) = path else { return NO_TAPE };
    let Ok(pixbuf) = gtk4::gdk_pixbuf::Pixbuf::from_file(&path) else {
        return NO_TAPE;
    };
    let (w, h) = (pixbuf.width(), pixbuf.height());
    // A zero edge is not a frame: a stub or truncated jpg decodes to something with no size, and handing
    // the encoder a 0x0 shape would fail in the filter graph rather than here.
    if w > 0 && h > 0 {
        (w, h)
    } else {
        NO_TAPE
    }
}
