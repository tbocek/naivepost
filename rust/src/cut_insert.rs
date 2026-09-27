//! F2.12 (Insert a card, still, video or sound) — `spec/05-cut.md` F2.12 and
//! `spec/inventory/cut.md` §G.
//!
//! Insert takes a file the session does not contain — a sting, a still, a tier board, a song — and puts it
//! in the cut one of three ways: BETWEEN the footage, which cuts the clip open and makes the video longer by
//! the card's own seconds while costing no session time; OVER it, which takes exactly the seconds it runs
//! (the same as － Remove) and is what a selection asks for; or on a LANE of its own, which adds a row and
//! cuts nothing. A sound has no third way and no picture of its own: it is laid over the kept footage, one
//! piece per stretch, and refuses where there is none.
//!
//! One flag carries the whole sound question — [`crate::cut::Seg::mute`] — because the mode already says what
//! is underneath it, so the tick's sentence changes with the mode and the stored cut does not (see
//! [`tick_label`]).
//!
//! No UI lives here and none is wired: `rust/src/ui/window.rs` builds widgets for the Prepare page only, so
//! there is no Insert button to attach yet (as with F2.1–F2.11) and no `spec/img/05-insert.png` to compare
//! against — §A's toolbar names the button, the images stop at 05-trim. The widget layer will forward the
//! presses, open the chooser at [`chooser_dir`] and print what these return.

use std::path::{Path, PathBuf};

use crate::cut::{Cut, Lane, Seg};
use crate::cut_copy::{footage_stretches, lane_name};
use crate::cut_select::{Scope, Selection, MIN_SCENE_SECONDS};
use crate::cut_screen;
use crate::layout;
use crate::tools;

// --- S1: a line or a selection -------------------------------------------------------------------------------

/// F2.12 S1 (`Needs a line or a selection`): the refusal when neither exists yet. The page opens with no red
/// line, and "there" is not a second until the timeline is clicked (F2.4 owns where the line comes from).
/// Distinct from [`crate::cut_copy::NO_LINE_YET`], which is ⧉ Paste's sentence for the same missing thing —
/// two buttons, two sentences, one rule.
pub const NO_LINE_YET: &str = "click the timeline where the insert goes first";

/// F2.12 S1: where the insert goes. A selection wins over the line and gives its own start, because marking
/// seconds and then pressing Insert is a sentence that says what those seconds are for; with no selection the
/// red line is the answer, and with neither there is nothing to answer with.
///
/// Read once, when the button is pressed, and handed down: the chooser is a window the hand can reach around,
/// so the file that comes back has to be placed where the timeline read at the press.
pub fn needs_place(line: Option<f64>, selection: Option<&Selection>) -> Result<f64, &'static str> {
    if let Some(band) = selection {
        return Ok(band.start.min(band.end));
    }
    line.ok_or(NO_LINE_YET)
}

// --- S2: the chooser, and what kind of file came back ---------------------------------------------------------

/// F2.12 S2 (`chooser "Insert a clip, image, animation or sound"`): the title for footage-scoped seconds — or
/// no seconds at all, since nothing was marked and the form will ask what the file does to the sound.
pub const CHOOSER_TITLE: &str = "Insert a clip, image, animation or sound";

/// F2.12 S2 (`"Insert a sound over the selected seconds" for a sound-scoped selection`): a selection drawn in
/// a lane is about sound, and offering a tier board there would be offering to put a picture where the hand
/// pointed at a waveform. The chooser's filter narrows with it (audio extensions only).
pub const CHOOSER_TITLE_SOUND: &str = "Insert a sound over the selected seconds";

/// F2.12 S2: which chooser to open, read off the scope the selection was drawn with.
pub fn chooser_title(selection: Option<&Selection>) -> &'static str {
    match selection {
        Some(band) if matches!(band.scope, Scope::Sound { .. }) => CHOOSER_TITLE_SOUND,
        _ => CHOOSER_TITLE,
    }
}

/// F2.12 S2 (`opening in ‹root›/assets`): the folder the chooser opens at. Cards live at root level, shared by
/// every project under it, which is [`layout::assets_dir`]'s rule rather than one of this module's.
pub fn chooser_dir(root: &Path) -> PathBuf {
    layout::assets_dir(root)
}

/// F2.12 S2: the note written beside the cards. Uppercase so it sorts above them and reads as a note about
/// them rather than as one of them, and short for the same reason §G is: what it has to carry is the hole
/// format the form reads, since that is the only part someone writing a card can get wrong silently.
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

/// F2.12 S2: write the built-in cards and [`CARD_GUIDE`] into `dir`, returning what was actually written —
/// which is nothing at all on the second open.
///
/// Never over a card that is already there: these are starting points, and a board someone restyled is theirs.
/// The chooser's opening is also the only moment where a card is what someone is after, so a folder that opens
/// empty teaches that there is nothing to insert.
pub fn seed_cards(dir: &Path, cards: &[(&str, &str)]) -> Vec<String> {
    let mut wrote = Vec::new();
    // A folder we cannot make is not a reason to lose the file someone is about to choose: the chooser simply
    // opens wherever it was going to, and the log line is the caller's.
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

/// What an insert file IS, which is the only thing that decides how it is drawn and what the form offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A moving picture with sound of its own: the one kind that may have a lane.
    Video,
    /// A card drawn by the app from its holes — animated or not, which is a second question (see
    /// [`preview_fps`]).
    Svg,
    /// Sound alone. Its picture is the session's own, which is why it is laid over footage and never between.
    Audio,
    /// A picture with no seconds of its own.
    Still,
}

/// F2.12 S2 (`insKind by extension: video|svg|audio|still`): what a file is, from its name alone.
///
/// By extension deliberately: the alternative is probing every asset on every dialog, and an `.svg` that a
/// probe calls a png stream is not more true than the name someone gave the file. The query comes off first,
/// because `tier.svg?S=Dust II` is one card's parameter and not part of its name.
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

/// F2.12 S3 (`else P.policy.insertDefaultSeconds = 4`) — §10's row, prototype `insDefault`. A card nobody
/// reads in four seconds is a card that was too wordy to be a card, which is why this is the fallback and not
/// something longer.
pub const DEFAULT_SECONDS: f64 = 4.0;

/// F2.12 S3 (`the file's own length (video/audio duration, SVG animation length, else …)`): how long an insert
/// runs when nothing has been typed yet.
///
/// `file_seconds` is what a probe found for a video or audio file, or what the card's own animation says — and
/// `None` for a still, for an SVG whose CSS animation has no `@keyframes` in it (drawn as a still, so it has no
/// length), and for a probe that failed. `animated_svg` is kept beside it rather than folded into it because
/// the two come from different reads of the file and the log names one of them.
pub fn default_length(file_seconds: Option<f64>, animated_svg: bool) -> f64 {
    match file_seconds {
        Some(seconds) if seconds > 0.0 => seconds,
        // An SVG with no animation is a still: its length was never the question, so `animated_svg` only says
        // that there was nothing to find in the first place.
        _ => {
            let _ = animated_svg;
            DEFAULT_SECONDS
        }
    }
}

/// F2.12 S3 (`Defaults: selection → its length, overwrite; none → splice, the file's own length`): the two
/// answers the gesture gives before the form opens, so the radios arrive already meaning what the hand did.
///
/// The length is `None` for no selection on purpose — the file has to be chosen before its length can be read,
/// and inventing 4 s at the press would put a number in the form that the file then contradicts. The caller
/// fills it with [`default_length`] once the chooser comes back. A selection's span is normalised because a
/// drag may run either way (F2.6).
pub fn default_modes(selection: Option<&Selection>) -> (bool, Option<f64>) {
    match selection {
        Some(band) if band.length() >= MIN_SCENE_SECONDS => (false, Some(band.length())),
        _ => (true, None),
    }
}

// --- S4: the form in the column -------------------------------------------------------------------------------

/// One thing the card asks for: its key (what goes on the path), what to call it in the dialog, what to say
/// about it, what is already written on the path, and the two flags that change the widget.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub key: String,
    pub label: String,
    pub hint: String,
    /// The value from the path's `?key=value`, if it carries one — an edit re-opens with what is written.
    pub value: String,
    /// A file to pick rather than words to type: the Logo\u{2026} picker.
    pub logo: bool,
    /// Arrives empty and stays out of the path when untouched (a hole that would only ever repeat its own
    /// placeholder is noise on the path).
    pub keep: bool,
}

/// F2.12 S4 (`one entry per declared card field (Logo… picker for logo fields)`): what a card asks for, in the
/// order it asks, read out of the document's own comments —
///
/// ```text
/// <!-- Input: title | Title | over the board, empty for none -->
/// <!-- Input: S[keep logo] | Tier S | what is in it, comma separated -->
/// ```
///
/// Key, label, hint; brackets are flags. Everything after the second bar is the hint, bars included, so a hint
/// may quote a path. A repeat of a key (compared without case) is dropped: two entries for one hole would be
/// two widgets writing one value, and which wins would depend on where the hand clicked.
pub fn card_fields(svg: &[u8]) -> Vec<Field> {
    let text = String::from_utf8_lossy(svg);
    let mut out: Vec<Field> = Vec::new();
    for rest in text.split("<!--").filter_map(|chunk: &str| {
        chunk.strip_prefix(" Input:").or_else(|| chunk.strip_prefix("Input:"))
    }) {
        let Some(decl) = rest.split("-->").next() else { continue };
        // Three parts at most: the hint keeps any bar it contains.
        let mut parts = decl.splitn(3, '|');
        let head = parts.next().unwrap_or_default().trim();
        if head.is_empty() {
            continue;
        }
        let mut field = Field {
            key: String::new(),
            label: parts.next().unwrap_or_default().trim().to_string(),
            // Everything after the second bar, bars included.
            hint: parts.next().unwrap_or_default().trim().to_string(),
            value: String::new(),
            logo: false,
            keep: false,
        };

        let key = match head.find('[') {
            Some(at) => {
                for flag in head[at..].trim_matches(|c| c == '[' || c == ']').split([',', ' ']) {
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
        if out.iter().any(|seen| seen.key.eq_ignore_ascii_case(&field.key)) {
            continue;
        }
        out.push(field);
    }
    out
}

/// How the card sits in the cut. Three radios rather than a tick: it is what the card DOES to the footage.
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
/// F2.12 S4/S5: the first radio's placement when a sound-scoped card is silenced — the same splice, plus the
/// reason its sound is not there (§G's status clause). One string, not a concatenation: the `\` continuation
/// swallows the newline and the indent, so this stays one space wide.
pub const BETWEEN_SILENT_LABEL: &str = "between the footage, which is cut open for it, and silent \u{2014} the \
                                        selection was scoped to the picture alone";

/// F2.12 S4: §G's second radio.
pub const OVER_LABEL: &str = "Play OVER the footage \u{2014} the card replaces those seconds (the same as Remove)";
/// F2.12 S4: §G's third, offered for video alone ([`lane_offered`]).
pub const LANE_LABEL: &str =
    "Put it on a LANE of its own \u{2014} a row of the band to cut to, and nothing is cut yet";

/// F2.12 S4 (`LANE, video only`): only a moving picture may have a row of its own. A still on a lane would be
/// a row that never changes, and an audio file has no picture to cut to — the green could choose it and show
/// nothing.
pub fn lane_offered(kind: Kind) -> bool {
    matches!(kind, Kind::Video)
}

/// F2.12 S4 (`sound tick — shown only when there is a sound to answer for`): whether the form asks the
/// question at all.
///
/// A sound insert IS the sound, so asking what it does to the sound answers nothing; a lane cuts nothing, so
/// there is nothing underneath it yet and the question waits until ＋ Add chooses between the rows. Spliced,
/// only the file's own sound is in play; over, either the file's or the session's under it opens the question —
/// a still with no sound laid over silent footage asks nothing, which is what keeps the form short.
pub fn sound_open(kind: Kind, mode: Mode, file_has_sound: bool, footage_under_has_sound: bool) -> bool {
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
pub const KEEP_LABEL: &str = "Keep the sound running under it \u{2014} only the picture is replaced";

/// F2.12 S4: which sentence the one stored flag says. `mute` is one flag because it is one sentence — this
/// insert brings no sound of its own — and two behaviours, because the mode already says what is underneath it:
/// spliced and silent plays nothing, over and silent lets the session carry on.
pub fn tick_label(splice: bool) -> &'static str {
    if splice {
        SILENT_LABEL
    } else {
        KEEP_LABEL
    }
}

/// F2.12 S4 (`Seconds`): the floor the entry holds to — `P.policy.minSceneSeconds`, read from
/// [`crate::cut_select`] where §10's row is catalogued. A card under a second is a flash nobody can grab an
/// edge of, and the same bound ＋ Add keeps a scene to.
pub fn seconds_accepts(seconds: f64) -> bool {
    seconds >= MIN_SCENE_SECONDS
}

/// F2.12 S4: what was typed, or the length already settled on. Unparsable text and a number under the floor
/// both keep the fallback rather than refusing the press: the entry is where someone types `3` and means three
/// seconds, and an entry that ate it silently would be worse — hence the floor's own sentence living in the
/// tooltip, not in an error.
pub fn seconds_or_default(text: &str, fallback: f64) -> f64 {
    match text.trim().parse::<f64>() {
        Ok(seconds) if seconds_accepts(seconds) => seconds,
        _ => fallback,
    }
}

// --- S5: placing it -----------------------------------------------------------------------------------------

/// What a placement did: the segments it wrote (empty for a lane, which cuts nothing), one clause on HOW the
/// card sits, and the status line the page prints.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub segs: Vec<Seg>,
    pub how: String,
    pub status: String,
}

/// F2.12 S5 (`Placed`): put the file in the cut and say so.
///
/// * [`Mode::Between`] — one segment with `s == e` and a `dur`, which is what makes it spliced
///   ([`Seg::is_insert`]): the cut is opened there, the footage carries on from the very next frame, and the
///   session's clock is untouched (see [`crate::cut_screen::cut_seconds`], which counts the card for its `dur`
///   and no footage twice).
/// * [`Mode::Over`] — the seconds are taken as － Remove would take them and the card laid in their place, so
///   it carries NO `dur` of its own: an overwriting insert runs for exactly the footage it replaced. One piece
///   per kept stretch, because footage that was already gone stays gone rather than being covered twice.
/// * [`Mode::Lane`] — a [`crate::cut::Lane`] and no segment at all: the point of a lane is that ＋ Add chooses
///   between it and the cameras beside it, and a row that arrived already green would have made that choice.
/// * A sound goes over the kept footage whatever was asked, one piece per stretch — it is the one insert that
///   must leave the picture exactly as it found it — and refuses where there is no picture under it.
///
/// `silent` is [`Seg::mute`], read by the mode (see [`tick_label`]); `lane_row` is unused here because a lane's
/// row comes from the rows already in use, which the caller owns.
pub fn place(
    cut: &mut Cut,
    path: &str,
    at: f64,
    mode: Mode,
    length: f64,
    silent: bool,
) -> Result<Placed, String> {
    let was = cut_screen::cut_seconds(cut);
    // A sound is placed through its own door: it has no picture, so BETWEEN would have nothing to be between
    // and a lane would be a row that shows nothing.
    if kind(path) == Kind::Audio {
        return place_sound(cut, path, at, length, was);
    }
    if mode == Mode::Lane {
        let name = lane_name(&base_name(path), &cut.lanes.iter().map(|lane| lane.name.clone()).collect::<Vec<_>>());
        let lane = Lane { name: name.clone(), src: path.to_string(), at, off: 0.0, dur: length };
        cut.lanes.push(lane);
        let how = format!(
            "on a lane of its own ({name}) \u{2014} select on that row and press \u{ff0b} Add to cut to it"
        );
        return Ok(Placed { segs: Vec::new(), how: how.clone(), status: status_line(path, at, length, &how, was, was) });
    }
    let card = if mode == Mode::Between {
        // Spliced: `s == e` with a `dur`, and the footage under it untouched.
        Seg { s: at, e: at, ins: path.to_string(), dur: length, mute: silent, ..Default::default() }
    } else {
        // Overwriting: it runs for the seconds it took, so it carries no `dur`.
        Seg { s: at, e: at + length, ins: path.to_string(), mute: silent, ..Default::default() }
    };
    // One string, not a concatenation: `\` inside a literal swallows the newline and the indent with it, and a
    // status line with the words welded together reads wrong on the page.
    let how = match mode {
        Mode::Between if silent => BETWEEN_SILENT_LABEL,
        Mode::Between => "between the footage, which is cut open for it",
        _ => "over the footage \u{2014} drag its edges to retime it",
    }
    .to_string();

    let segs = if mode == Mode::Between {
        cut.segs.push(card.clone());
        vec![card]
    } else {
        lay_over_footage(cut, &card)
    };
    let now = cut_screen::cut_seconds(cut);
    Ok(Placed { segs, how: how.clone(), status: status_line(path, at, length, &how, now, was) })
}

/// F2.12 S5 (`sound → over kept footage`): the sound laid over the picture, one piece per kept stretch, each
/// carrying its own part of the file so pieces either side of a hole are parts of one sound and not its opening
/// seconds twice. Zero stretches means no picture under it, which is what the refusal says.
fn place_sound(cut: &mut Cut, path: &str, at: f64, length: f64, was: f64) -> Result<Placed, String> {
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
            out.push(Seg { s: seg.s, e: at, ..seg.clone() });
        }
        for piece in &pieces {
            if piece.s < seg.e && piece.e > seg.s {
                out.push(piece.clone());
            }
        }
        if seg.e > to {
            out.push(Seg { s: to, e: seg.e, ..seg.clone() });
        }
    }
    cut.segs = out;
    let how = if pieces.len() > 1 {
        format!("over {} stretches of footage, which keep their frames", pieces.len())
    } else {
        "over the footage, which keeps its frames \u{2014} drag its edges to retime it".to_string()
    };
    let now = cut_screen::cut_seconds(cut);
    Ok(Placed { segs: pieces, how: how.clone(), status: status_line(path, at, length, &how, now, was) })
}

/// F2.12 S5 (`overwriting → replaces those seconds`): the card put over the footage, which gives up the seconds
/// under it exactly as － Remove would. Footage the span never reached stays in the list untouched, and a hole
/// already in the cut stays a hole — one piece per kept stretch, so the card is never drawn over nothing.
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
            out.push(Seg { s: seg.s, e: from, ..seg.clone() });
        }
        if stretches.iter().any(|(start, stop)| start < &seg.e && stop > &seg.s) {
            let mut piece = card.clone();
            piece.s = seg.s.max(from);
            piece.e = seg.e.min(to);
            pieces.push(piece.clone());
            out.push(piece);
        }
        if seg.e > to {
            out.push(Seg { s: to, e: seg.e, ..seg.clone() });
        }
    }
    cut.segs = out;
    pieces
}

/// F2.12 S5 (`Status "<file> inserted at m:ss for X s, <how> — the cut is now a (was b) — ↶ Undo takes it
/// back"`): the whole sentence, in §G's order.
///
/// The length of the finished video is said here because this is the one edit whose effect on it cannot be read
/// off the timeline: the timeline is the session's clock and stays as long as the recording, while a card
/// spliced into it makes the VIDEO longer by its own seconds. One decimal, which is how §G spells `X s` — this
/// does NOT go through [`tools::tenths`], whose `… s` would put the unit twice.
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

/// The file's name off a path that may carry a card's parameters: the status names the card someone chose, and
/// `?S=Dust II` is what makes it a different board, not part of its name.
pub fn base_name(path: &str) -> String {
    let file = path.split('?').next().unwrap_or_default();
    match file.rfind(['/', '\\']) {
        Some(at) => file[at + 1..].to_string(),
        None => file.to_string(),
    }
}

// --- S6: holding a card makes Insert into Edit ----------------------------------------------------------------

/// F2.12 S6 (`Right-click or double left click a card on the track to hold it; Insert becomes Edit`): whether
/// the button opens the held card instead of choosing a new file. Holding one is a statement about what someone
/// is working on, and "insert another card at the playhead" is not what anyone means while holding one.
pub fn hold(seg: Option<&Seg>) -> bool {
    seg.is_some_and(Seg::is_insert)
}

/// F2.12 S6 (`re-opens the form ("that card is no longer in the cut" if gone)`): find the card again by what it
/// IS rather than by where it was — its file, parameters included, and the second it starts at. The dialog does
/// not hold the timeline still: a trim, a merge or another insert renumbers the list under it, so an index kept
/// across the window would open the wrong card.
pub fn find_card(cut: &Cut, path: &str, at: f64) -> Option<usize> {
    cut.segs.iter().position(|seg| seg.ins == path && (seg.s - at).abs() < 0.01)
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

/// F2.12 S6 (`switching modes returns or takes footage`): what the change does to the seconds under the card.
/// `Some(true)` takes them (BETWEEN → OVER, which costs the footage as － Remove would), `Some(false)` returns
/// them (OVER → BETWEEN, which gives them back and lengthens the video by the card), and `None` means the card
/// was already in that mode — or only its lane moved, which touches no footage at all.
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

/// F2.12 S7 (`one frame for stills`): the rate to ask ffmpeg for, or `None` for one frame. A picture that never
/// changes needs one texture and not sixty-four of the same one, and an SVG with a CSS animation but no
/// `@keyframes` in the file is drawn as a still — so it is a still here too, whatever its extension says.
pub fn preview_fps(kind: Kind, animated: bool) -> Option<f64> {
    match kind {
        Kind::Video => Some(PREVIEW_FPS),
        Kind::Svg if animated => Some(PREVIEW_FPS),
        _ => None,
    }
}

/// F2.12 S7: the seconds each rendered frame stands at, first frame included — a card that runs 2 s at 8 fps is
/// sixteen frames and not fifteen, because the last one is what plays in its final tenth of a second.
pub fn frame_times(length: f64, fps: f64) -> Vec<f64> {
    if length <= 0.0 || fps <= 0.0 {
        return Vec::new();
    }
    let count = (length * fps).ceil() as usize;
    (0..count).map(|n| n as f64 / fps).collect()
}

/// F2.12 S7 (`nearest rendered frame shown`): which texture to put on screen for the playhead at `t`. Nothing
/// is decoded on the fly — a card whose frames have not arrived shows the nearest one that has, and an empty
/// list shows nothing rather than guessing.
pub fn nearest_rendered(times: &[f64], t: f64) -> Option<f64> {
    // `total_cmp` on a difference compares magnitudes only when the difference does not cross zero, and a
    // frame before the playhead produces exactly that: 0.0 - 0.34 would have ranked first for being negative.
    times.iter().copied().min_by(|a, b| (a - t).abs().total_cmp(&(b - t).abs()))
}

/// F2.12 S7 (`48-texture cap`, §I's `insFilmMax`): frames held in memory for one card. A minute of a sting at
/// 8 fps is four hundred and eighty textures nobody will look at, so the strip stops filling there and the
/// nearest rendered frame rule above covers what falls outside it.
pub const TEXTURE_CAP: usize = 48;

/// F2.12 S7 (`rendered via ffmpeg at ≤960 px`, §I's `insPreviewW`): the width a preview is decoded at. The
/// preview is a film strip in a column, never the exported picture, so nothing wider than that is ever drawn.
pub fn preview_width(width: u32) -> u32 {
    width.min(PREVIEW_WIDTH)
}

/// F2.12 S7: the cap [`preview_width`] holds previews to.
pub const PREVIEW_WIDTH: u32 = 960;

/// F2.12 S7: how ffmpeg is asked for a card's frames — an argument vector, no subprocess, so the shape of the
/// call is testable and the running of it stays with [`crate::subprocess`]. `-an` because a preview strip has
/// no business decoding sound it will not play: a card's own audio is its own pipeline (see [`held_status`]).
pub fn ffmpeg_frames(ffmpeg: &str, path: &str, fps: f64, width: u32, out: &str) -> Vec<String> {
    vec![
        ffmpeg.to_string(),
        "-hide_banner".into(),
        "-loglevel".into(),
        "error".into(),
        "-i".into(),
        path.to_string(),
        "-an".into(),
        "-vf".into(),
        format!("fps={fps:.6},scale='min({},-2)'", preview_width(width)),
        out.to_string(),
    ]
}

/// F2.12 S7 (`a spliced card holds the footage while it plays on the wall clock`): what the page says while the
/// picture stands still. A spliced card costs no session time, so the playhead cannot carry it — the card runs
/// on the clock and the footage waits under it, which is worth saying because everywhere else on this page the
/// picture moves when the clock does.
pub fn held_status(card: &str) -> String {
    format!("{} \u{2014} the footage is held while it plays", base_name(card))
}

// --- the form's opening plan -------------------------------------------------------------------------------

/// What the insert form shows when a file comes back from the chooser (or when a held card is re-opened):
/// the length the `Seconds` entry starts with, which radio is lit, and which of the two conditional controls
/// are drawn at all.
///
/// This is the whole S3+S4 decision for one file, kept out of the widget layer so the page only draws what it
/// reads. The spec is silent on how the mode is picked when a *held* card is edited rather than a new file
/// chosen; the answer taken here is that the card's own shape says — `s == e` means it was spliced, a span
/// means it was laid over, a lane card is not a segment at all and never reaches this door.
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
    /// Whether the sound tick is drawn (S4, [`sound_open`]). A still with no sound over silent footage asks
    /// nothing, which is what keeps the form short.
    pub sound_shown: bool,
    /// The tick's sentence for the lit mode ([`tick_label`]).
    pub sound_label: &'static str,
    /// The card's declared fields, in the order it declares them (S4). Empty for anything that is not an SVG.
    pub fields: Vec<Field>,
    /// The values written on the file's own path (`card.svg?title=Best maps`), so a re-opened form shows what
    /// is there and [`with_card_fields`] can drop each one into its field.
    pub values: Vec<(String, String)>,
}

/// F2.12 S3 + S4: the plan the form is built from. `file_seconds` is what a probe found for a video or audio
/// file, or what the card's own animation runs for; `None` (a still, a probe-less headless run) falls back to
/// [`default_length`]'s [`DEFAULT_SECONDS`]. `silent` is what the tick is set to, which decides the sentence
/// along with the mode.
pub fn form_plan(
    path: &str,
    selection: Option<&Selection>,
    file_seconds: Option<f64>,
    animated_svg: bool,
    silent: bool,
    file_has_sound: bool,
    footage_under_has_sound: bool,
) -> FormPlan {
    let file_kind = kind(path);
    let (splice, selected) = default_modes(selection);
    let mode = if splice { Mode::Between } else { Mode::Over };
    let seconds = selected.unwrap_or_else(|| default_length(file_seconds, animated_svg));
    FormPlan {
        kind: file_kind,
        seconds,
        mode,
        lane_shown: lane_offered(file_kind),
        sound_shown: sound_open(
            file_kind,
            mode,
            file_has_sound,
            footage_under_has_sound,
        ),
        sound_label: tick_label(mode == Mode::Between),
        fields: Vec::new(),
        values: crate::cut_cards::split_path(path).args,
    }
}

/// F2.12 S4: fill in the fields a card declares, read off the document the chooser just returned. Kept apart
/// from [`form_plan`] because the document has to be read from disk and only an SVG has holes to ask about;
/// every other kind gets the plan unchanged. Values already written on the path (`card.svg?title=Best maps`)
/// come back into each field, so an edit re-opens with what is there — which is why the path is passed in
/// beside the plan rather than being guessed at from it.
pub fn with_card_fields(mut plan: FormPlan, doc: &str) -> FormPlan {
    if plan.kind != Kind::Svg {
        return plan;
    }
    let mut fields = card_fields(doc.as_bytes());
    for field in fields.iter_mut() {
        if let Some(value) = plan.values.iter().find_map(|(key, value)| {
            key.eq_ignore_ascii_case(&field.key)
                .then_some(value.clone())
        }) {
            field.value = value;
        }
    }
    plan.fields = fields;
    plan
}

// --- the answer the form gives back ------------------------------------------------------------------------

/// What was typed into the open form, ready to hand to [`place`]. One struct so the widget layer collects
/// answers instead of deciding anything: the mode comes off the radios, the seconds through
/// [`seconds_or_default`] (an unparsable or too-short entry keeps the length the plan opened with rather than
/// refusing the press), and `silent` off the sound tick.
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

/// F2.12 S7: whether the playhead has moved. Held by a spliced card, the footage does not advance however much
/// time passes; released, seconds are seconds. `rate` is the player's own rate (F2.1), so a card played at half
/// speed still holds the footage for twice as long on the wall clock.
pub fn playhead_advance(held: bool, elapsed: f64, rate: f64) -> f64 {
    if held {
        0.0
    } else {
        elapsed * rate
    }
}
