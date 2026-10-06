//! F2.12 S7 — what the preview shows while a card plays. `spec/05-cut.md` F2.12 S7:
//! "cards render at 8 fps (one frame for stills) via ffmpeg, nearest rendered frame shown; a spliced card
//! holds the footage while it plays on the wall clock; card sound is its own pipeline."
//!
//! `cut_insert` catalogues the numbers (§I's `insPreviewFPS`, the frame-second ladder, the 48-texture stop);
//! this module RUNS them: it asks the real ffmpeg for a card's frames, puts the nearest rendered one on the
//! preview panel, says when the footage is being held under a spliced card, and answers the hearing question
//! a card poses. Nothing here decodes a frame on the fly and nothing here invents a frame: what ffmpeg did
//! not write is not shown.
//!
//! Split out of `window.rs`, which is past its size budget.

use std::path::{Path, PathBuf};

use crate::cut::{Cut, Seg};
use crate::cut_insert::{self, Kind};
use crate::subprocess;

/// The tool this pass asks for by name. The same name `rust/src/ui/settings.rs`'s ffmpeg row tests, and
/// resolved off `PATH` like every other tool the app runs, so a machine with ffmpeg elsewhere still works.
pub const FFMPEG: &str = "ffmpeg";

/// §I's `insFilmMax` in action: the frame folder one card gets. Named off the card itself, parameters
/// included, because `tier.svg?S=Dust II` and `tier.svg?S=Dust III` are two boards and must not share frames.
pub fn frame_dir(root: &Path, card_path: &str) -> PathBuf {
    // The WHOLE path including its `?holes`, flattened: `tier.svg?S=Dust II` and `tier.svg?S=Dust III` are two
    // boards with one file name, and sharing a folder between them would show one board's frames for the other.
    let stamp: String = card_path
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    root.join("cache").join("preview-cards").join(stamp)
}

/// The tool that can say how long a file runs, named the way Settings names it (§5's ffmpeg row resolves
/// `ffprobe` beside the ffmpeg box, and an empty box means the plain name off PATH).
pub const FFPROBE: &str = "ffprobe";

/// Ask the real ffprobe for a file's own seconds — S3's "the file's own length (video/audio duration…)".
/// `None` on any failure, which is what sends the form back to `P.policy.insertDefaultSeconds` rather than
/// refusing the insert: an unreadable duration is a fallback case, not a broken button.
pub fn probe_seconds(ffprobe: &str, path: &str) -> Option<f64> {
    let out = subprocess::run(
        ffprobe,
        &["-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", path],
        &mut |_line: &str| {},
    )
    .ok()?;
    out.trim().parse::<f64>().ok().filter(|s| *s > 0.0)
}

/// How ffmpeg is asked for ONE frame — a still has no clock, so a rate would be a lie about what is there.
/// One door, in `cut_insert::ffmpeg_frames_argv`, with `fps: None`.
pub fn ffmpeg_one_frame(ffmpeg: &str, path: &str, out: &str) -> Vec<String> {
    cut_insert::ffmpeg_frames_argv(ffmpeg, path, None, PREVIEW_WIDTH, out)
}

/// The frames already on disk for this card, oldest first. Their names are ffmpeg's own sequence numbers
/// ([`FRAME_PATTERN`]), so the folder sorts in time order and a re-open costs no ffmpeg run.
pub fn rendered_frames(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.file_name()
                        .and_then(|n| n.to_str())
                        .is_some_and(|n| n.starts_with("frame") && n.ends_with(".img"))
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// ffmpeg's own numbering, padded to nine digits so a lexical sort is a time sort. The SECOND each frame
/// stands at comes from its index and the rate ([`frame_second`]). The name goes through ffmpeg's explicit
/// `-f image2 -c:v png` pair (see `cut_insert::ffmpeg_frames_argv`), so it carries no magic extension.
pub const FRAME_PATTERN: &str = "frame%09d.img";

/// The cap every preview frame is decoded inside — §I's `insPreviewW`, catalogued in `cut_insert`.
pub const PREVIEW_WIDTH: u32 = cut_insert::PREVIEW_WIDTH;

/// The second a rendered frame stands at: its place in the ladder at the rate it was rendered at.
pub fn frame_second(path: &Path, fps: f64) -> Option<f64> {
    let digits: String = path
        .file_name()?
        .to_str()?
        .chars()
        .filter(|c| c.is_ascii_digit())
        .collect();
    let index: usize = digits.parse::<usize>().ok()?.checked_sub(1)?;
    (fps > 0.0).then(|| index as f64 / fps)
}


/// The card's frames, rendered for real if they are not there yet.
///
/// Resume first (AGENTS.md: a step's own output is its marker): a folder that already holds frames is the
/// pass having happened before, and re-running ffmpeg over a card nobody changed would cost a second for
/// nothing. Otherwise the ladder comes from `cut_insert::frame_times` at `preview_fps`'s rate (a STILL asks
/// for one frame, not sixty-four of the same one) and stops at `TEXTURE_CAP`, and ffmpeg is asked through
/// `subprocess::run` so the command reaches the log and a failure names the tool and its reason.
pub fn render_card(
    ffmpeg: &str,
    card_path: &str,
    kind: Kind,
    animated: bool,
    length: f64,
    dir: &Path,
    log: &mut dyn FnMut(&str),
) -> Result<Vec<PathBuf>, String> {
    let existing = rendered_frames(dir);
    if !existing.is_empty() {
        return Ok(existing);
    }
    if std::fs::create_dir_all(dir).is_err() {
        return Err(format!("cannot make the preview folder {}", dir.display()));
    }
    // ffmpeg writes an image sequence; the pattern needs an image extension for it to choose the muxer.
    let pattern = dir.join(FRAME_PATTERN);
    match cut_insert::preview_fps(kind, animated) {
        Some(fps) => {
            // How many frames were asked for is what the ladder says; ffmpeg's own numbering does the rest.
            let wanted = cut_insert::frame_times(length, fps)
                .into_iter()
                .take(cut_insert::TEXTURE_CAP)
                .count();
            let args = cut_insert::ffmpeg_frames(
                ffmpeg,
                card_path,
                fps,
                PREVIEW_WIDTH,
                &pattern.to_string_lossy(),
            );
            let args: Vec<&str> = args.iter().map(String::as_str).skip(1).collect();
            subprocess::run(ffmpeg, &args, log).map_err(|err| fold_hint(card_path, err))?;
            keep_first(&rendered_frames(dir), wanted, card_path)
        }
        None => {
            // One frame for a still, and the ladder is that one frame standing at 0 s.
            let args = ffmpeg_one_frame(ffmpeg, card_path, &pattern.to_string_lossy());
            let args: Vec<&str> = args.iter().map(String::as_str).skip(1).collect();
            subprocess::run(ffmpeg, &args, log).map_err(|err| fold_hint(card_path, err))?;
            keep_first(&rendered_frames(dir), 1, card_path)
        }
    }
}

/// Keep only what ffmpeg really wrote: a pass that wrote fewer frames than the ladder promised shows fewer
/// rather than pointing at a file that is not there, and a pass that wrote none is the refusal below.
fn keep_first(on_disk: &[PathBuf], wanted: usize, card_path: &str) -> Result<Vec<PathBuf>, String> {
    let frames: Vec<PathBuf> = on_disk.iter().take(wanted).cloned().collect();
    if frames.is_empty() {
        return Err(format!(
            "{FFMPEG} wrote no preview frame for {card_path} \u{2014} the panel shows nothing from it"
        ));
    }
    Ok(frames)
}

/// A card's own refusal carries its name: "failure is specific and local" — the panel says WHICH card the
/// strip is missing, not only that a tool failed.
fn fold_hint(card_path: &str, err: String) -> String {
    format!("{}: {err}", cut_insert::base_name(card_path))
}

/// The card under the line, if the line is on one. A SPLICED card has no span of its own (`s == e`) so it is
/// asked for as `s .. s + dur`; an overwriting one owns its span outright.
pub fn card_under(cut_: &Cut, t: f64) -> Option<&Seg> {
    cut_.segs.iter().find(|seg| {
        !seg.ins.is_empty()
            && if seg.is_insert() {
                t >= seg.s && t < seg.s + seg.dur
            } else {
                t >= seg.s && t < seg.e
            }
    })
}

thread_local! {
    /// Frame folders the snapshot bench names, because its folder is a fixture and not an open project. Same
    /// reason `cut_picture` carries a snapshot dir: a headless render has no window with a root, and a card's
    /// frames still have to go somewhere beside the session they were made for.
    static SNAPSHOT_CARDS_DIR: std::cell::RefCell<Option<std::path::PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

/// Name the folder a snapshot renders card frames into. Only the snapshot bench calls it.
pub fn set_card_snapshot_dir(dir: &std::path::Path) {
    SNAPSHOT_CARDS_DIR.with(|slot| *slot.borrow_mut() = Some(dir.to_path_buf()));
}

/// Put a card's frames where the preview can use them: the project's own preview folder beside the session,
/// which is where a card's frames belong (the project's work product, not the camera's). A window with no
/// folder open writes beside the temp root instead, because a headless run has no project and must not fail.
pub fn frame_dir_for(window: &adw::ApplicationWindow, card_path: &str) -> PathBuf {
    let root = crate::ui::line_root(window).or_else(|| SNAPSHOT_CARDS_DIR.with(|slot| slot.borrow().clone()));
    match root {
        Some(root) => frame_dir(&root, card_path),
        None => std::env::temp_dir().join("naivepost-preview"),
    }
}

/// F2.12 S3 (`the file's own length`): this window's own probe door, the one the program calls. Handing a
/// window rather than a bare function means the length a form opens with and the frames it renders are asked
/// of the same file, and a test can point the same door at a file it drew itself.
pub fn probe_card_seconds(window: &adw::ApplicationWindow, path: &str) -> Option<f64> {
    let _ = window;
    probe_seconds(FFPROBE, path)
}

/// The card's own frames, on the panel. Answers the sentence the page prints, or `None` when the line is on
/// footage — the caller then shows the session's own frame as it always has.
///
/// The frames live in the folder [`frame_dir_for`] names: this window's project, or the one a snapshot named,
/// because a headless render has no project open and must still not fail. A card whose frames cannot be
/// rendered answers `None`, leaving the panel on the session's frame.
pub fn show_card_frame(
    window: &adw::ApplicationWindow,
    cut_: &Cut,
    t: f64,
) -> Option<CardFrame> {
    // A spliced card costs no session time, so the picture stands still while the wall clock runs: worth
    // saying because everywhere else on this page the picture moves when the clock does. `held` is that fact.
    let card = card_under(cut_, t)?;
    let dir = frame_dir_for(window, &card.ins);
    let kind = cut_insert::kind(&card.ins);
    let animated = svg_is_animated(Path::new(card.ins.split(['?']).next().unwrap_or_default()));
    let length = card.dur.max(card.e - card.s);
    let mut log = |_: &str| {};
    let rendered = render_card(FFMPEG, &card.ins, kind, animated, length, &dir, &mut log).ok()?;
    let rate = cut_insert::preview_fps(kind, animated).unwrap_or(1.0);
    let inside = if card.is_insert() {
        (t - card.s).clamp(0.0, card.dur)
    } else {
        t - card.s
    };
    // The nearest of the LADDER this render was asked for, then that ladder's file — never a guess at a frame
    // ffmpeg did not write: the ladder is trimmed to what really landed on disk, and `nearest_rendered` picks
    // from those seconds alone.
    let seconds: Vec<f64> = rendered.iter().filter_map(|path| frame_second(path, rate)).collect();
    let want = cut_insert::nearest_rendered(&seconds, inside)?;
    let path = rendered
        .iter()
        .find(|path| frame_second(path, rate) == Some(want))
        .cloned()?;
    if !crate::ui::cut_picture::show_frame(window, Some(&path)) {
        return None;
    }
    Some(CardFrame { path, held: card.is_insert() })
}

/// What the card pass put on the panel: the frame it showed, and whether the footage is being held under it.
#[derive(Debug, Clone, PartialEq)]
pub struct CardFrame {
    pub path: PathBuf,
    pub held: bool,
}

/// F2.12 S7: the door the page calls. With the line on a card, its nearest rendered frame goes on the panel
/// and the sentence about the held footage comes back to be printed; with the line on footage, `None`, and the
/// caller shows the session's own frame as it always has.
pub fn card_frame(
    window: &adw::ApplicationWindow,
    cut_: &Cut,
    t: f64,
) -> Option<(PathBuf, Option<String>)> {
    let shown = show_card_frame(window, cut_, t)?;
    let card = card_under(cut_, t)?;
    Some((shown.path, shown.held.then(|| cut_insert::held_status(&card.ins))))
}

/// The card's own hearing answer for this second — does it bring a pipeline of its own, and is the
/// footage held under it? `ui::window::apply_mix` asks exactly this, so the tick's sound answer comes from
/// the same rule the panel's frame answer does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CardHearing {
    /// The card brings sound of its own (see [`card_sound`]).
    pub own_sound: bool,
    /// The footage's own pipeline is held while the card plays (see [`footage_held_while_card`]).
    pub footage_held: bool,
}

pub fn card_hearing(cut_: &Cut, t: f64) -> Option<CardHearing> {
    let card = card_under(cut_, t)?;
    Some(CardHearing { own_sound: card_sound(card), footage_held: footage_held_while_card(card) })
}

/// Whether an SVG animates, read off the document: a `@keyframes` block or an `<animate>` tag. Same test
/// `window.rs` uses for the form's length; kept here because the render asks it too, and a card drawn as a
/// still gets one frame rather than a strip of identical ones.
pub fn svg_is_animated(file: &Path) -> bool {
    std::fs::read_to_string(file)
        .map(|doc| doc.contains("@keyframes") || doc.contains("<animate"))
        .unwrap_or(false)
}

/// F2.12 S7 (`card sound is its own pipeline`): does this card bring sound the app must answer for?
///
/// A card with a `mute` is silent — that is the tick's whole meaning — and a still or a drawn card has no
/// sound track at all, so only a video file (or a sound laid over the picture) has a pipeline of its own.
pub fn card_sound(seg: &Seg) -> bool {
    !seg.mute && matches!(cut_insert::kind(&seg.ins), Kind::Video | Kind::Audio)
}

/// The footage's own sound while a card is up: muted for the seconds a spliced card holds the tape, left
/// alone for an overwriting one, which replaced the picture and — with the tick off — nothing else.
pub fn footage_held_while_card(seg: &Seg) -> bool {
    seg.is_insert()
}

/// F2.12 S7 (`a spliced card holds the footage while it plays on the wall clock`): how far the line may move
/// for a tick's worth of elapsed time. The arithmetic is `cut_insert::playhead_advance`; what lives here is the
/// decision about WHICH second is held, so no tick loop grows a card rule of its own. Asked through
/// `cut_record_tick::WindowPage::card_advance`, the program's own door.
pub fn card_hold_advance(seg: Option<&Seg>, elapsed: f64, rate: f64) -> f64 {
    cut_insert::playhead_advance(seg.is_some_and(|s| s.is_insert()), elapsed, rate)
}

/// The card's whole S7 answer for one second of the preview: the frame that went on the panel, whether the
/// footage is held under it, and how far the line may move for this tick. `cut_picture::refresh` takes the
/// frame and the sentence through [`card_frame`]; this is the same door with the tick's advance asked too.
#[derive(Debug, Clone, PartialEq)]
pub struct CardTick {
    /// The frame that went on the panel.
    pub frame: PathBuf,
    /// The status line's sentence: the card's name and the fact the footage is held (spliced only).
    pub status: Option<String>,
    /// How far the line moves for `elapsed` at `rate` — zero while a spliced card plays.
    pub advance: f64,
    /// Whether the card brings sound of its own (see [`card_sound`]).
    pub own_sound: bool,
}

pub fn card_tick(
    cut_: &Cut,
    t: f64,
    elapsed: f64,
    rate: f64,
) -> Option<CardTick> {
    let card = card_under(cut_, t)?;
    let hearing = card_hearing(cut_, t)?;
    Some(CardTick {
        status: footage_held_while_card(card).then(|| cut_insert::held_status(&card.ins)),
        advance: card_hold_advance(Some(card), elapsed, rate),
        own_sound: hearing.own_sound,
        // The frame itself needs a window to be put ON, which is `card_frame`'s job; the tick asks this door
        // for the arithmetic only, so an empty path means "the panel already has it".
        frame: PathBuf::new(),
    })
}
