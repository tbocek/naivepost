//! F2.12 (insert a card, still, video or sound) — `spec/05-cut.md` F2.12, `spec/inventory/cut.md` §G.
//! Placed BETWEEN footage (video longer, no session time), OVER it (takes those seconds like － Remove), or on a
//! LANE (a row, cuts nothing). A sound is only laid over kept footage. No UI here; one flag, [`crate::cut::Seg::mute`], carries the sound question (see [`tick_label`]).

use std::path::Path;

use crate::cut::{Cut, Lane, Seg};
use crate::cut_copy::{footage_stretches, lane_name};
use crate::cut_screen;
use crate::cut_select::{Scope, Selection, MIN_SCENE_SECONDS};
use crate::tools;

// --- S1: a line or a selection -------------------------------------------------------------------------------

/// F2.12 S1: the refusal when neither a line nor a selection exists; ⧉ Paste has its own ([`crate::cut_copy::NO_LINE_YET`]).
pub const NO_LINE_YET: &str = "click the timeline where the insert goes first";

/// F2.12 S1: where the insert goes. A selection wins and gives its own start; otherwise the red line.
///
/// Read once at the press: the chooser is a window, so the place must be the one read then.
pub fn needs_place(line: Option<f64>, selection: Option<&Selection>) -> Result<f64, &'static str> {
    if let Some(band) = selection {
        return Ok(band.start.min(band.end));
    }
    line.ok_or(NO_LINE_YET)
}

// --- S2: the chooser, and what kind of file came back ---------------------------------------------------------

/// F2.12 S2: the chooser title for footage-scoped or no seconds.
pub const CHOOSER_TITLE: &str = "Insert a clip, image, animation or sound";

/// F2.12 S2: the title for a sound-scoped selection; the filter narrows to audio.
pub const CHOOSER_TITLE_SOUND: &str = "Insert a sound over the selected seconds";

/// F2.12 S2: which chooser to open, read off the scope the selection was drawn with.
pub fn chooser_title(selection: Option<&Selection>) -> &'static str {
    match selection {
        Some(band) if matches!(band.scope, Scope::Sound { .. }) => CHOOSER_TITLE_SOUND,
        _ => CHOOSER_TITLE,
    }
}

/// F2.12 S2: the note written beside the cards, documenting the hole format.
pub const CARD_GUIDE: &str = concat!(
    "# Writing a card\n",
    "\n",
    "This folder holds *cards*: SVG files that can be dropped into a cut as an insert.\n",
    "The footage stops, the card is on screen for a few seconds, the footage carries on.\n",
    "\n",
    "A card asks for its contents with holes in it:\n",
    "\n",
    "    {{title}}\n",
    "\n",
    "By default the insert dialog asks for each hole under its bare name. Say more by\n",
    "declaring it, one parameter per comment, in this order:\n",
    "\n",
    "    <!-- Input: title | Title | over the card, empty for none -->\n",
    "    <!-- Input: logo[logo] | Logo | a mark to sit beside the title -->\n",
    "\n",
    "Key, dialog label, hint. Brackets are flags: `logo` opens the Logo\u{2026} picker, `keep`\n",
    "means the value is a file rather than words. The dialog comes out of the file, so a\n",
    "hand-written SVG gets a form; what you type is kept on the end of the path\n",
    "(`card.svg?title=Best maps`), so one file is a different card every time.\n",
    "\n",
    "The file has to travel on its own. The render writes each frame of an animated\n",
    "card into a temporary folder and reads it back from there, so a relative `href`\n",
    "resolves against nothing and an absolute one is refused. Put an image in the\n",
    "document instead: `href=\"data:image/png;base64,...\"`.\n",
    "\n",
    "Fonts are the ones installed on the machine, named as a family list --\n",
    "\"DejaVu Sans, Liberation Sans, Helvetica, Arial, sans-serif\". No `@font-face`\n",
    "with a URL, no webfont. Nothing can measure text before it is drawn, so err\n",
    "small: about 0.58em of advance per character is a safe estimate for these faces.\n",
);

/// F2.12 S2: write the built-in cards and [`CARD_GUIDE`] into `dir`; returns what was written.
/// Never over an existing card: a restyled board is the user's.
pub fn seed_cards(dir: &Path, cards: &[(&str, &str)]) -> Vec<String> {
    let mut wrote = Vec::new();
    // A folder we cannot make must not lose the chosen file; the caller logs.
    if std::fs::create_dir_all(dir).is_err() {
        return wrote;
    }
    for (file, body) in cards {
        let path = dir.join(file);
        if path.exists() {
            continue;
        }
        if std::fs::write(&path, body).is_ok() {
            wrote.push((*file).to_string());
        }
    }
    let guide = dir.join(CARD_GUIDE_FILE);
    if !guide.exists() && std::fs::write(&guide, CARD_GUIDE).is_ok() {
        wrote.push(CARD_GUIDE_FILE.to_string());
    }
    wrote
}

/// F2.12 S2: the note's filename, uppercase on purpose (see [`CARD_GUIDE`]).
pub const CARD_GUIDE_FILE: &str = "CARDS.md";

/// What an insert file IS: decides how it is drawn and what the form offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A moving picture with sound of its own: the one kind that may have a lane.
    Video,
    /// A card drawn from its holes, animated or not (see [`preview_fps`]).
    Svg,
    /// Sound alone. Its picture is the session's own, which is why it is laid over footage and never between.
    Audio,
    /// A picture with no seconds of its own.
    Still,
}

/// F2.12 S2: what a file is, from its extension alone.
/// The query comes off first: `tier.svg?S=Dust II` is a parameter, not part of the name.
pub fn kind(path: &str) -> Kind {
    let file = path.split(['?', '#']).next().unwrap_or_default();
    let ext = match file.rfind('.') {
        Some(at) => file[at + 1..].to_lowercase(),
        None => return Kind::Still,
    };
    match ext.as_str() {
        "mp4" | "mkv" | "mov" | "webm" | "avi" | "m4v" | "mpg" | "mpeg" | "ts" => Kind::Video,
        "svg" | "svgz" => Kind::Svg,
        "mp3" | "wav" | "ogg" | "oga" | "flac" | "m4a" | "aac" | "opus" => Kind::Audio,
        _ => Kind::Still,
    }
}

// --- S3: what the gesture answers before anything is asked ----------------------------------------------------

/// F2.12 S3 — §10's row, `insDefault`: the fallback length of an insert.
pub const DEFAULT_SECONDS: f64 = 4.0;

/// F2.12 S3: how long an insert runs when nothing has been typed yet.
///
/// `file_seconds` is `None` for a still, an SVG without `@keyframes` (drawn as a still), or a failed probe.
pub fn default_length(file_seconds: Option<f64>) -> f64 {
    match file_seconds {
        Some(seconds) if seconds > 0.0 => seconds,
        // No animation: a still, so there was no length to find.
        _ => DEFAULT_SECONDS,
    }
}

/// F2.12 S3: the gesture's answers before the form opens: a selection asks for OVER, nothing asks for BETWEEN.
///
/// Length is `None` without a selection: the caller fills it with [`default_length`] after the chooser. The span is normalised (F2.6).
pub fn default_modes(selection: Option<&Selection>) -> (bool, Option<f64>) {
    match selection {
        Some(band) if band.length() >= MIN_SCENE_SECONDS => (false, Some(band.length())),
        _ => (true, None),
    }
}

// --- S4: the form in the column -------------------------------------------------------------------------------

/// One card hole: path key, dialog label, hint, current value, and widget flags.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub key: String,
    pub label: String,
    pub hint: String,
    /// The value from the path's `?key=value`, if it carries one — an edit re-opens with what is written.
    pub value: String,
    /// A file to pick rather than words to type: the Logo\u{2026} picker.
    pub logo: bool,
    /// Arrives empty and stays off the path when untouched.
    pub keep: bool,
}

/// F2.12 S4: what a card asks for, in order, read from the document's own comments:
///
/// ```text
/// <!-- Input: title | Title | over the board, empty for none -->
/// <!-- Input: S[keep logo] | Tier S | what is in it, comma separated -->
/// ```
///
/// Key, label, hint; brackets are flags. The hint keeps any bars. A repeated key (case-insensitive) is dropped.
pub fn card_fields(svg: &[u8]) -> Vec<Field> {
    let text = String::from_utf8_lossy(svg);
    let mut out: Vec<Field> = Vec::new();
    for rest in text.split("<!--").filter_map(|chunk: &str| {
        chunk
            .strip_prefix(" Input:")
            .or_else(|| chunk.strip_prefix("Input:"))
    }) {
        let Some(decl) = rest.split("-->").next() else {
            continue;
        };
        // Three parts at most: the hint keeps any bar it contains.
        let mut parts = decl.splitn(3, '|');
        let head = parts.next().unwrap_or_default().trim();
        if head.is_empty() {
            continue;
        }
        let mut field = Field {
            key: String::new(),
            label: parts.next().unwrap_or_default().trim().to_string(),
            hint: parts.next().unwrap_or_default().trim().to_string(),
            value: String::new(),
            logo: false,
            keep: false,
        };

        let key = match head.find('[') {
            Some(at) => {
                for flag in head[at..]
                    .trim_matches(|c| c == '[' || c == ']')
                    .split([',', ' '])
                {
                    match flag.trim().to_lowercase().as_str() {
                        "keep" => field.keep = true,
                        "logo" => field.logo = true,
                        _ => {}
                    }
                }
                head[..at].trim()
            }
            None => head,
        };
        if key.is_empty() {
            continue;
        }
        field.key = key.to_string();
        // No label is the key said back: a card that declares only what to call a hole still gets a named entry.
        if field.label.is_empty() {
            field.label = key.to_string();
        }
        if out
            .iter()
            .any(|seen| seen.key.eq_ignore_ascii_case(&field.key))
        {
            continue;
        }
        out.push(field);
    }
    out
}

/// §G: the file's path with the form's answers written onto it.
///
/// The producer reads a card's arguments off the path (`cut_cards::split_path`). Blank values write nothing, so a hole keeps its fallback (§5);
/// no answers returns the path unchanged. Existing keys are replaced in place, new ones follow in declared order.
pub fn answer_path(path: &str, answers: &[(String, String)]) -> String {
    let mut split = crate::cut_cards::split_path(path);
    for (key, value) in answers {
        let value = value.trim();
        if key.is_empty() || value.is_empty() {
            continue;
        }
        match split
            .args
            .iter_mut()
            .find(|(known, _)| known.eq_ignore_ascii_case(key))
        {
            Some(slot) => slot.1 = value.to_string(),
            None => split.args.push((key.clone(), value.to_string())),
        }
    }
    crate::cut_cards::join_path(&split)
}

/// The holes a card declares, valued as [`crate::cut_cards::fill`] will read them (empty falls back, so counts as untouched).
/// Compares holes, not whole documents, so whitespace does not matter.
pub fn answered_holes(doc: &str, args: &[(String, String)]) -> Vec<(String, String)> {
    crate::cut_cards::holes(doc)
        .into_iter()
        .filter_map(|hole| {
            args.iter()
                .find(|(key, value)| key == &hole.name && !value.is_empty())
                .map(|(_, value)| (hole.name, value.clone()))
        })
        .collect()
}

/// How the card sits in the cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The footage is cut open for it; the video gets longer and nothing filmed is lost.
    Between,
    /// It replaces exactly the seconds it runs, taking the footage as － Remove would.
    Over,
    /// A row of its own, with nothing cut to it. Video only.
    Lane,
}

/// F2.12 S4: the three radios' labels, verbatim (§G).
pub const BETWEEN_LABEL: &str =
    "Insert BETWEEN the footage \u{2014} the video gets longer by the card, nothing filmed is lost";
/// F2.12 S4/S5: the first radio when a sound-scoped card is silenced (§G's status clause).
/// One string: the `\` continuation swallows the newline and indent.
pub const BETWEEN_SILENT_LABEL: &str =
    "between the footage, which is cut open for it, and silent \u{2014} the \
                                        selection was scoped to the picture alone";

/// F2.12 S4: §G's second radio.
pub const OVER_LABEL: &str =
    "Play OVER the footage \u{2014} the card replaces those seconds (the same as Remove)";
/// F2.12 S4: §G's third, offered for video alone ([`lane_offered`]).
pub const LANE_LABEL: &str =
    "Put it on a LANE of its own \u{2014} a row of the band to cut to, and nothing is cut yet";

/// F2.12 S4 (`LANE, video only`): a still on a lane never changes and audio has no picture.
pub fn lane_offered(kind: Kind) -> bool {
    matches!(kind, Kind::Video)
}

/// F2.12 S4: whether the form asks the sound question at all.
///
/// A sound insert IS the sound; a lane has nothing underneath until ＋ Add chooses a row. Spliced, only the file's own sound is in play;
/// over, either the file's or the session's sound opens the question.
pub fn sound_open(
    kind: Kind,
    mode: Mode,
    file_has_sound: bool,
    footage_under_has_sound: bool,
) -> bool {
    if matches!(kind, Kind::Audio) || mode == Mode::Lane {
        return false;
    }
    if mode == Mode::Between {
        return file_has_sound;
    }
    file_has_sound || footage_under_has_sound
}

/// F2.12 S4: the tick's sentence when the card goes BETWEEN the footage (§G, verbatim).
pub const SILENT_LABEL: &str = "Play it SILENT \u{2014} the insert's own sound is not used";
/// F2.12 S4: the same tick's other reading, for a card laid OVER the footage (§G, verbatim).
pub const KEEP_LABEL: &str =
    "Keep the sound running under it \u{2014} only the picture is replaced";

/// F2.12 S4: which sentence the stored `mute` flag says; the mode says what is underneath.
pub fn tick_label(splice: bool) -> &'static str {
    if splice {
        SILENT_LABEL
    } else {
        KEEP_LABEL
    }
}

/// F2.12 S4 (`Seconds`): the floor, `P.policy.minSceneSeconds` (see [`crate::cut_select`]); same bound ＋ Add keeps a scene to.
pub fn seconds_accepts(seconds: f64) -> bool {
    seconds >= MIN_SCENE_SECONDS
}

/// What was typed, or the length already settled on. Unparsable or under-floor text keeps the fallback rather than refusing the press.
pub fn seconds_or_default(text: &str, fallback: f64) -> f64 {
    match text.trim().parse::<f64>() {
        Ok(seconds) if seconds_accepts(seconds) => seconds,
        _ => fallback,
    }
}

// --- S5: placing it -----------------------------------------------------------------------------------------

/// What a placement did: segments written (none for a lane), how the card sits, and the status line.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub segs: Vec<Seg>,
    pub how: String,
    pub status: String,
}

/// F2.12 S5 (`Placed`): put the file in the cut and say so.
///
/// * [`Mode::Between`] — one segment with `s == e` and a `dur` ([`Seg::is_insert`]); the session's clock is untouched
///   (see [`crate::cut_screen::cut_seconds`]).
/// * [`Mode::Over`] — the seconds are taken as － Remove would; the card has NO `dur`, one piece per kept stretch.
/// * [`Mode::Lane`] — a [`crate::cut::Lane`] and no segment: ＋ Add chooses between it and the cameras.
/// * A sound goes over kept footage, one piece per stretch, and refuses where there is none.
///
/// `silent` is [`Seg::mute`]; `lane_row` is unused here (the caller owns row numbering).
pub fn place(
    cut: &mut Cut,
    path: &str,
    at: f64,
    mode: Mode,
    length: f64,
    silent: bool,
) -> Result<Placed, String> {
    let was = cut_screen::cut_seconds(cut);
    // A sound has no picture: BETWEEN has nothing to split and a lane would show nothing.
    if kind(path) == Kind::Audio {
        return place_sound(cut, path, at, length, was);
    }
    if mode == Mode::Lane {
        let name = lane_name(
            &base_name(path),
            &cut.lanes
                .iter()
                .map(|lane| lane.name.clone())
                .collect::<Vec<_>>(),
        );
        let lane = Lane {
            name: name.clone(),
            src: path.to_string(),
            at,
            off: 0.0,
            dur: length,
        };
        cut.lanes.push(lane);
        let how = format!(
            "on a lane of its own ({name}) \u{2014} select on that row and press \u{ff0b} Add to cut to it"
        );
        return Ok(Placed {
            segs: Vec::new(),
            how: how.clone(),
            status: status_line(path, at, length, &how, was, was),
        });
    }
    let card = if mode == Mode::Between {
        // Spliced: `s == e` with a `dur`, and the footage under it untouched.
        Seg {
            s: at,
            e: at,
            ins: path.to_string(),
            dur: length,
            mute: silent,
            ..Default::default()
        }
    } else {
        // Overwriting: it runs for the seconds it took, so it carries no `dur`.
        Seg {
            s: at,
            e: at + length,
            ins: path.to_string(),
            mute: silent,
            ..Default::default()
        }
    };
    // One string: `\` in a literal swallows the newline and indent.
    let how = match mode {
        Mode::Between if silent => BETWEEN_SILENT_LABEL,
        Mode::Between => "between the footage, which is cut open for it",
        _ => "over the footage \u{2014} drag its edges to retime it",
    }
    .to_string();

    let segs = if mode == Mode::Between {
        cut.segs.push(card.clone());
        cut.segs = crate::cut::spliced(std::mem::take(&mut cut.segs));
        vec![card]
    } else {
        lay_over_footage(cut, &card)
    };
    let now = cut_screen::cut_seconds(cut);
    Ok(Placed {
        segs,
        how: how.clone(),
        status: status_line(path, at, length, &how, now, was),
    })
}

/// F2.12 S5: a sound laid over the kept footage, one piece per stretch, each with its own part of the file.
/// Zero stretches means no picture under it (the refusal).
fn place_sound(
    cut: &mut Cut,
    path: &str,
    at: f64,
    length: f64,
    was: f64,
) -> Result<Placed, String> {
    let to = at + length;
    let stretches = footage_stretches(cut, at, to);
    if stretches.is_empty() {
        return Err(format!(
            "the cut keeps no footage at {} \u{2014} {} is a sound, and one needs a picture under it",
            tools::mm_ss(at),
            base_name(path)
        ));
    }
    let pieces: Vec<Seg> = stretches
        .iter()
        .map(|(start, stop)| Seg {
            s: *start,
            e: *stop,
            ins: path.to_string(),
            // The file resumes where the piece stands in the span, hole and all.
            ss: start - at,
            ..Default::default()
        })
        .collect();
    let mut out: Vec<Seg> = Vec::new();
    for seg in &cut.segs {
        if !seg.ins.is_empty() || seg.e <= at || seg.s >= to {
            out.push(seg.clone());
            continue;
        }
        if seg.s < at {
            out.push(Seg {
                s: seg.s,
                e: at,
                ..seg.clone()
            });
        }
        for piece in &pieces {
            if piece.s < seg.e && piece.e > seg.s {
                out.push(piece.clone());
            }
        }
        if seg.e > to {
            out.push(Seg {
                s: to,
                e: seg.e,
                ..seg.clone()
            });
        }
    }
    cut.segs = out;
    let how = if pieces.len() > 1 {
        format!(
            "over {} stretches of footage, which keep their frames",
            pieces.len()
        )
    } else {
        "over the footage, which keeps its frames \u{2014} drag its edges to retime it".to_string()
    };
    let now = cut_screen::cut_seconds(cut);
    Ok(Placed {
        segs: pieces,
        how: how.clone(),
        status: status_line(path, at, length, &how, now, was),
    })
}

/// F2.12 S5: the card put over the footage, which gives up those seconds as － Remove would; one piece per kept stretch.
/// Returns the pieces written, one per stretch that had footage under it.
fn lay_over_footage(cut: &mut Cut, card: &Seg) -> Vec<Seg> {
    let (from, to) = (card.s, card.e);
    let stretches = footage_stretches(cut, from, to);
    let mut out: Vec<Seg> = Vec::new();
    let mut pieces: Vec<Seg> = Vec::new();
    for seg in &cut.segs {
        if !seg.ins.is_empty() || seg.e <= from || seg.s >= to {
            out.push(seg.clone());
            continue;
        }
        if seg.s < from {
            out.push(Seg {
                s: seg.s,
                e: from,
                ..seg.clone()
            });
        }
        if stretches
            .iter()
            .any(|(start, stop)| start < &seg.e && stop > &seg.s)
        {
            let mut piece = card.clone();
            piece.s = seg.s.max(from);
            piece.e = seg.e.min(to);
            pieces.push(piece.clone());
            out.push(piece);
        }
        if seg.e > to {
            out.push(Seg {
                s: to,
                e: seg.e,
                ..seg.clone()
            });
        }
    }
    cut.segs = out;
    pieces
}

/// F2.12 S5: the status sentence in §G's order.
///
/// The finished length is stated because a spliced card makes the VIDEO longer while the timeline stays the session's clock.
/// One decimal, as §G spells `X s`; not [`tools::tenths`], whose `… s` would put the unit twice.
pub fn status_line(path: &str, at: f64, length: f64, how: &str, now: f64, was: f64) -> String {
    format!(
        "{} inserted at {} for {:.1} s, {how} \u{2014} the cut is now {} (was {}) \u{2014} \u{21b6} Undo takes it back",
        base_name(path),
        tools::mm_ss(at),
        length,
        tools::mm_ss(now),
        tools::mm_ss(was)
    )
}

/// The file's name off a path that may carry card parameters (`?S=Dust II` is not part of the name).
pub fn base_name(path: &str) -> String {
    let file = path.split('?').next().unwrap_or_default();
    match file.rfind(['/', '\\']) {
        Some(at) => file[at + 1..].to_string(),
        None => file.to_string(),
    }
}

// --- S6: holding a card makes Insert into Edit ----------------------------------------------------------------

/// F2.12 S6: whether the button opens the held card instead of choosing a new file.
pub fn hold(seg: Option<&Seg>) -> bool {
    seg.is_some_and(Seg::is_insert)
}

/// F2.12 S6: find the held card again by file (parameters included) and start second, not by index:
/// a trim, merge or insert renumbers the list while the dialog is open.
pub fn find_card(cut: &Cut, path: &str, at: f64) -> Option<usize> {
    cut.segs
        .iter()
        .position(|seg| seg.ins == path && (seg.s - at).abs() < 0.01)
}

/// F2.12 S6: what the page says when the card went away while its dialog was open.
pub const GONE: &str = "that card is no longer in the cut";

/// F2.12 S6: the button's word — a held card is edited, anything else is inserted.
pub fn edit_verb(held: bool) -> &'static str {
    if held {
        "Edit"
    } else {
        "Insert"
    }
}

/// F2.12 S6: what a mode switch does to the seconds under the card. `Some(true)` takes them (BETWEEN → OVER),
/// `Some(false)` returns them (OVER → BETWEEN), `None` means no footage changes.
pub fn switching_takes_footage(from: Mode, to: Mode) -> Option<bool> {
    match (from, to) {
        (Mode::Between, Mode::Over) => Some(true),
        (Mode::Over, Mode::Between) => Some(false),
        _ => None,
    }
}

// --- S7: what the preview shows while it plays -----------------------------------------------------------------

/// F2.12 S7 (`cards render at 8 fps`): frames a second for anything that moves — §I's `insPreviewFPS`.
pub const PREVIEW_FPS: f64 = 8.0;

/// F2.12 S7: the fps to ask ffmpeg for, or `None` for one frame (stills, and SVGs with CSS animation but no `@keyframes`).
pub fn preview_fps(kind: Kind, animated: bool) -> Option<f64> {
    match kind {
        Kind::Video => Some(PREVIEW_FPS),
        Kind::Svg if animated => Some(PREVIEW_FPS),
        _ => None,
    }
}

/// F2.12 S7: the seconds each rendered frame stands at, first included (2 s at 8 fps is sixteen frames).
pub fn frame_times(length: f64, fps: f64) -> Vec<f64> {
    if length <= 0.0 || fps <= 0.0 {
        return Vec::new();
    }
    let count = (length * fps).ceil() as usize;
    (0..count).map(|n| n as f64 / fps).collect()
}

/// F2.12 S7: the rendered frame nearest the playhead `t`; an empty list shows nothing.
pub fn nearest_rendered(times: &[f64], t: f64) -> Option<f64> {
    // Rank by absolute difference: a signed 0.0 - 0.34 would rank first for being negative.
    times
        .iter()
        .copied()
        .min_by(|a, b| (a - t).abs().total_cmp(&(b - t).abs()))
}

/// F2.12 S7 (`48-texture cap`, §I's `insFilmMax`): frames held in memory for one card; the nearest-frame rule covers the rest.
pub const TEXTURE_CAP: usize = 48;

/// F2.12 S7 (§I's `insPreviewW`): the width a preview is decoded at; never widened.
pub fn preview_width(width: u32) -> u32 {
    width.min(PREVIEW_WIDTH)
}

/// F2.12 S7: the cap [`preview_width`] holds previews to.
pub const PREVIEW_WIDTH: u32 = 960;

/// F2.12 S7: the ffmpeg argv for a card's frames, run by [`crate::ui::cut_card_preview::render_card`]. `-an`: no sound is played.
///
/// The scale `if(gte(iw\,960)\,960\,iw):-2` needs a literal `-2` to keep the aspect; the commas inside `if()` are escaped
/// because a bare comma separates filters. A frame under the cap keeps its width.
pub fn ffmpeg_frames_argv(
    ffmpeg: &str,
    path: &str,
    fps: Option<f64>,
    width: u32,
    out: &str,
) -> Vec<String> {
    let cap = preview_width(width);
    let mut args = vec![
        ffmpeg.to_string(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-i".into(),
        path.to_string(),
        "-an".into(),
    ];
    args.push("-vf".into());
    let scale = format!("scale=if(gte(iw\\,{cap})\\,{cap}\\,iw):-2");
    args.push(match fps {
        Some(rate) => format!("fps={rate:.6},{scale}"),
        None => scale,
    });
    if fps.is_none() {
        // A still has no clock: one frame, not a strip of identical ones.
        args.push("-frames:v".into());
        args.push("1".into());
    }
    // Explicit image2/png: ffmpeg needs a recognisable output name to pick a muxer.
    args.push("-f".into());
    args.push("image2".into());
    args.push("-c:v".into());
    args.push("png".into());
    args.push(out.to_string());
    args
}

/// The same argv with fps as a rate (§I's `insPreviewFPS` tests read it by name).
pub fn ffmpeg_frames(ffmpeg: &str, path: &str, fps: f64, width: u32, out: &str) -> Vec<String> {
    ffmpeg_frames_argv(ffmpeg, path, Some(fps), width, out)
}

/// F2.12 S7: the page status while a spliced card holds the footage; it costs no session time, so the clock runs under a still picture.
pub fn held_status(card: &str) -> String {
    format!(
        "{} \u{2014} the footage is held while it plays",
        base_name(card)
    )
}

// --- the form's opening plan -------------------------------------------------------------------------------

/// What the insert form shows for a file: the starting length, the lit radio, and which conditional controls are drawn.
///
/// The S3+S4 decision for one file, kept out of the widget layer. For a *held* card its shape picks the mode: `s == e` was spliced,
/// a span was laid over; a lane card is not a segment and never reaches here.
#[derive(Debug, Clone, PartialEq)]
pub struct FormPlan {
    /// What the file IS, off its extension ([`kind`]).
    pub kind: Kind,
    /// The length the `Seconds` entry opens with: the selection's own seconds, else the file's, else
    /// [`DEFAULT_SECONDS`] (S3).
    pub seconds: f64,
    /// Which radio is lit (S3: a selection asks for OVER, nothing asked for BETWEEN).
    pub mode: Mode,
    /// Whether the LANE radio is drawn at all — video only (S4, [`lane_offered`]).
    pub lane_shown: bool,
    /// Whether the sound tick is drawn (S4, [`sound_open`]).
    pub sound_shown: bool,
    /// The tick's sentence for the lit mode ([`tick_label`]).
    pub sound_label: &'static str,
    /// The card's declared fields, in the order it declares them (S4). Empty for anything that is not an SVG.
    pub fields: Vec<Field>,
    /// The values written on the file's own path (`card.svg?title=Best maps`), so a re-opened form shows what
    /// is there in each field.
    pub values: Vec<(String, String)>,
}

/// F2.12 S3 + S4: the plan the form is built from. `file_seconds` is `None` for a still or a probe-less run
/// (falls back to [`DEFAULT_SECONDS`]); `silent` is the tick's state.
pub fn form_plan(
    path: &str,
    selection: Option<&Selection>,
    file_seconds: Option<f64>,
    _silent: bool,
    file_has_sound: bool,
    footage_under_has_sound: bool,
) -> FormPlan {
    let file_kind = kind(path);
    let (splice, selected) = default_modes(selection);
    let mode = if splice { Mode::Between } else { Mode::Over };
    let seconds = selected.unwrap_or_else(|| default_length(file_seconds));
    FormPlan {
        kind: file_kind,
        seconds,
        mode,
        lane_shown: lane_offered(file_kind),
        sound_shown: sound_open(file_kind, mode, file_has_sound, footage_under_has_sound),
        sound_label: tick_label(mode == Mode::Between),
        fields: Vec::new(),
        values: crate::cut_cards::split_path(path).args,
    }
}

// --- the answer the form gives back ------------------------------------------------------------------------

/// What was typed into the open form, ready for [`place`]; an unparsable or too-short entry keeps the plan's length.
#[derive(Debug, Clone, PartialEq)]
pub struct FormAnswer {
    pub mode: Mode,
    pub seconds: f64,
    pub silent: bool,
}

impl FormAnswer {
    /// Read the answer against the plan the form was built from: `text` is what the `Seconds` entry holds now,
    /// `fallback` the length it opened with ([`FormPlan::seconds`]).
    pub fn from_form(mode: Mode, text: &str, fallback: f64, silent: bool) -> Self {
        FormAnswer {
            mode,
            seconds: seconds_or_default(text, fallback),
            silent,
        }
    }
}

/// F2.12 S7: whether the playhead has moved. A held (spliced) card holds the footage; `rate` is the player's (F2.1).
pub fn playhead_advance(held: bool, elapsed: f64, rate: f64) -> f64 {
    if held {
        0.0
    } else {
        elapsed * rate
    }
}

/// F2.12 S4's Logo… picker: picked images added to a field's comma-separated items as bare paths.
/// A file inside the project is written relative to it (no `project:` prefix), one outside absolute; earlier text stays.
pub fn add_logos(existing: &str, picked: &[std::path::PathBuf], project: &Path) -> String {
    let mut items: Vec<String> = existing
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect();
    for path in picked {
        let written = path
            .strip_prefix(project)
            .map(|inside| inside.display().to_string())
            .unwrap_or_else(|_| path.display().to_string());
        items.push(written);
    }
    items.join(", ")
}
