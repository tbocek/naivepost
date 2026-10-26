//! §4's source rows: the controls one file gets, drawn from [`crate::sources`] and wired to it.
//!
//! Split out of `ui::window`, which is past its size budget: everything a row is — the four symbol
//! controls with §4's legend on their tooltips, the track menu over the streams ffprobe counted, the ⚠
//! for a name with no timestamp, and the repaint that keeps row indices honest after a removal — lives
//! here. The rules stay in `sources.rs`; this file only draws them and forwards presses (spec/00-principles.md §5).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::project::{Project, Source};
use crate::services;
use crate::source_probe;
use crate::sources::{self, Control, TrackInfo};

/// What a row needs to be drawn: the live session, the status line a press reports on, and the list the
/// row belongs to, so a removal can redraw what follows it.
#[derive(Clone)]
pub struct Rows {
    pub session: Rc<RefCell<Project>>,
    pub status: gtk::Label,
    pub list: gtk::ListBox,
}

/// Append one row per source. Called at build and again by [`repaint`].
pub fn append_all(rows: &Rows) {
    let count = rows.session.borrow().sources.len();
    for index in 0..count {
        rows.list.append(&row_at(rows, index));
    }
}

/// Redraw the whole list. A removal re-indexes every row below the one that left, and these widgets are
/// built once, so without this the row under a removed one still carries the removed row's buttons and
/// its index — pressing 🗑 twice would remove the wrong file.
pub fn repaint(rows: &Rows) {
    while let Some(child) = rows.list.last_child() {
        rows.list.remove(&child);
    }
    append_all(rows);
}

/// The 🎤 narrator button's face: the microphone beside the slot it holds. §4 draws "🎤 1"; a bare number
/// read as a row index, and with the split toggle also wearing a microphone (before this) the pair looked
/// like one control that had moved.
pub const NARRATOR_ICON: &str = "audio-input-microphone-symbolic";
/// The 🎥 footage toggle's face: §4's camera. The first port drew `media-record-symbolic`, a black dot, after
/// finding `camera-video-symbolic` unresolved in the container's theme; both themes the app meets ship it
/// (`symbolic/devices/` on the host and in the container), and a dot did not say "footage" to anyone.
pub const FOOTAGE_ICON: &str = "camera-video-symbolic";
/// The 🗣 split-the-voice toggle's face. §4 names 🗣 and no installed theme ships a speaking-head symbolic
/// (`ls /usr/share/icons/*/symbolic/*/*speaker*` finds only `audio-speakers-symbolic`), so the rewrite
/// draws a loudspeaker: the voice this file splits off, and unmistakably NOT the narrator's microphone.
pub const SEPARATE_ICON: &str = "audio-speakers-symbolic";

/// A symbol control: an icon, flat so a row of them reads as one band rather than a row of boxes, and
/// carrying §4's key to the four symbols at the end of its tooltip.
fn icon_toggle(icon: &str, name: &str, tip: &str) -> gtk::ToggleButton {
    let button = gtk::ToggleButton::builder().icon_name(icon).build();
    button.set_widget_name(name);
    button.add_css_class("flat");
    button.set_tooltip_text(Some(&format!("{tip}{}", sources::ROW_KEY)));
    button
}

/// The same for a control that is pressed rather than ticked. `icon` empty leaves the face to be set by
/// the caller, which is how the narrator button shows the slot it holds.
fn icon_button(icon: &str, name: &str, tip: &str) -> gtk::Button {
    let button = if icon.is_empty() {
        gtk::Button::new()
    } else {
        gtk::Button::from_icon_name(icon)
    };
    button.set_widget_name(name);
    button.add_css_class("flat");
    button.set_tooltip_text(Some(&format!("{tip}{}", sources::ROW_KEY)));
    button
}

/// The two roots the one path rule needs: the application root (the working folder, where the session's
/// own `session.naivepost` sits) and this project's folder. Same pair every page reads.
fn session_roots() -> (PathBuf, PathBuf) {
    let root = std::env::current_dir().unwrap_or_default();
    (root, crate::startup::open_dir())
}

/// The file's audio streams, measured by the real ffprobe — §4 counts streams FROM THE FILE, so neither
/// the track menu nor its `<on>/<total>` face may come from what the session happens to have ticked.
///
/// The tool comes from Settings the way every other tool call does (`ffprobe_for` beside the configured
/// ffmpeg), and the path through the project's one path rule. Any failure means `None`: a menu over
/// audio nobody measured would be a dead click, and §4 offers no badge for "unknown", so the row simply
/// shows no track button. Saying it on the log instead would shout on every paint of a folder that has
/// no ffprobe installed.
pub fn audio_streams(source: &Source) -> Vec<TrackInfo> {
    if !super::background::on_worker() {
        PROBED_HERE.with(|count| count.set(count.get() + 1));
    }
    let Some(ffprobe) = ffprobe_now() else {
        return vec![];
    };
    let file = resolved(source);
    source_probe::audio_streams(&ffprobe, &file, &source_probe::run_ffprobe).unwrap_or_default()
}

/// The ffprobe the settings name, or `None` with no settings folder at all.
fn ffprobe_now() -> Option<String> {
    let paths = crate::settings::from_environment()?;
    let conf = crate::settings::read(&paths).unwrap_or_default();
    Some(services::ffprobe_for(&conf.ffmpeg, "ffprobe"))
}

/// A file as far as a probe is concerned: where it is, how big, when it last changed. A file replaced under
/// the same name is a new file to probe.
type Version = (PathBuf, u64, Option<std::time::SystemTime>);

/// What ffprobe read, per file version: a row is drawn from here and never probes itself (§03-shell §1, the
/// window thread never waits; the first rewrite ran one ffprobe per row on every repaint, a 🗑 included).
static PROBED: std::sync::Mutex<Vec<(Version, Vec<TrackInfo>)>> = std::sync::Mutex::new(Vec::new());

/// How many probes ran on this (the window's) thread: none ever should; a test reads it.
pub fn probed_here_for_test() -> usize {
    PROBED_HERE.with(std::cell::Cell::get)
}

thread_local! {
    /// Probes run on the window thread itself, counted for [`probed_here_for_test`].
    static PROBED_HERE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    /// Versions a worker is probing now, so a repaint while it runs does not ask again.
    static ASKED: RefCell<Vec<Version>> = const { RefCell::new(Vec::new()) };
}

/// The file's version, read with one `stat`.
fn version_of(file: &std::path::Path) -> Version {
    let meta = std::fs::metadata(file).ok();
    (
        file.to_path_buf(),
        meta.as_ref().map(|meta| meta.len()).unwrap_or(0),
        meta.and_then(|meta| meta.modified().ok()),
    )
}

/// The streams already probed for this version, or `None` when no probe has landed for it.
fn probed(version: &Version) -> Option<Vec<TrackInfo>> {
    PROBED.lock().ok().and_then(|held| {
        held.iter()
            .find(|(known, _)| known == version)
            .map(|(_, tracks)| tracks.clone())
    })
}

/// Probe this version on a worker and redraw the list when it lands. The row meanwhile has no track button,
/// the face §4 gives a file whose streams nobody measured.
fn ask_probe(rows: &Rows, version: Version) {
    let asked = ASKED.with(|asked| {
        let mut asked = asked.borrow_mut();
        if asked.contains(&version) {
            return false;
        }
        asked.push(version.clone());
        true
    });
    if !asked {
        return;
    }
    let Some(ffprobe) = ffprobe_now() else {
        return;
    };
    let rows = rows.clone();
    let owed = crate::ui::settle::owe("source row probe", &rows.list);
    let file = version.0.clone();
    super::background::run(
        "probe",
        move || {
            source_probe::audio_streams(&ffprobe, &file, &source_probe::run_ffprobe)
                .unwrap_or_default()
        },
        move |tracks| {
            if let Ok(mut held) = PROBED.lock() {
                held.retain(|(known, _)| known.0 != version.0);
                held.push((version.clone(), tracks));
            }
            ASKED.with(|asked| asked.borrow_mut().retain(|known| *known != version));
            repaint(&rows);
            drop(owed);
        },
    );
}

/// One row: the file's name, and the controls §4 gives it. A control that does not apply to this file
/// is absent rather than insensitive — an audio row has no footage tick to grey out.
pub fn row_at(rows: &Rows, index: usize) -> gtk::ListBoxRow {
    let session = rows.session.clone();
    let source = session.borrow().sources[index].clone();
    let name = sources::row_name(&source.path);
    // §4's stream count, from the file itself: probed once per file version on a worker, and drawn from
    // what landed. Until it lands the row has no track button.
    let version = version_of(&resolved(&source));
    let tracks = match probed(&version) {
        Some(tracks) => tracks,
        None => {
            ask_probe(rows, version);
            Vec::new()
        }
    };
    let count = tracks.len();
    let any_track_out = false; // probe state (§5) owns "still being read"; nothing is mid-read at build.

    let row = gtk::ListBoxRow::new();
    row.set_widget_name(&format!("source-row-{index}"));
    row.set_tooltip_text(Some(&source.path));

    let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let controls = sources::row_controls(&source, count);

    if controls.contains(&Control::Footage) {
        let footage = icon_toggle(
            FOOTAGE_ICON,
            &format!("footage-{index}"),
            sources::FOOTAGE_TIP,
        );
        footage.set_active(source.footage);
        wire_footage(&footage, session.clone(), &rows.status, index, name.clone());
        box_.append(&footage);
    }

    let narrator = icon_button("", &format!("narrator-{index}"), sources::NARRATOR_TIP);
    // §4's face is the mic AND the slot: the mic says what the button is for, the number says which slot it
    // holds. Slot 1 is the narration's voice, so it is drawn as suggested (§4's "slot 1 highlighted").
    let face = gtk::Box::new(gtk::Orientation::Horizontal, 3);
    face.append(&gtk::Image::from_icon_name(NARRATOR_ICON));
    let slot_face = gtk::Label::new(Some(&source.narrator.to_string()));
    slot_face.set_widget_name(&format!("narrator-face-{index}"));
    face.append(&slot_face);
    narrator.set_child(Some(&face));
    // No colour for voice 1: every recording starts there, so a highlight would mark nothing.
    wire_narrator(
        &narrator,
        &slot_face,
        session.clone(),
        &rows.status,
        index,
        name.clone(),
    );
    box_.append(&narrator);

    let label = gtk::Label::new(Some(&name));
    label.set_widget_name(&format!("source-name-{index}"));
    label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    label.set_hexpand(true);
    label.set_xalign(0.0);
    box_.append(&label);

    if let Some(face) = sources::tracks_face(&source, count) {
        let menu = track_menu(
            rows,
            index,
            &tracks,
            &face,
            sources::tracks_dimmed(any_track_out),
        );
        box_.append(&menu);
    }

    if let Some(tip) = sources::warning(&source) {
        // §4's ⚠: a name with no stamp starts where the session does, which is a guess worth saying.
        // No ROW_KEY — the warning explains itself, and the legend belongs to the four symbols.
        let warn = gtk::Image::from_icon_name("dialog-warning-symbolic");
        warn.set_widget_name(&format!("warning-{index}"));
        warn.set_tooltip_text(Some(tip));
        box_.append(&warn);
    }

    // 🗣 split-the-voice toggle. Greyed on a split product (§4), never dropped: a row without it would
    // pull the trash to a different x than its neighbours. The prototype's ✂ is NOT reused, because on
    // Cut ✂ means Split; the face is [`SEPARATE_ICON`], a loudspeaker — see its note: it must not read as
    // the narrator's microphone standing after the file name.
    let separate = icon_toggle(
        SEPARATE_ICON,
        &format!("separate-{index}"),
        sources::SEPARATE_TIP,
    );
    separate.set_active(source.sepvoice);
    separate.set_sensitive(sources::separable(&source));
    wire_separate(
        &separate,
        session.clone(),
        &rows.status,
        index,
        name.clone(),
    );
    box_.append(&separate);

    let remove = icon_button(
        "user-trash-symbolic",
        &format!("remove-{index}"),
        sources::REMOVE_TIP,
    );
    wire_remove(&remove, rows.clone(), index);
    box_.append(&remove);

    row.set_child(Some(&box_));
    row
}

/// The track menu: a `MenuButton` faced "<on>/<total>" over a popover headed §4's title, the three-line
/// explanation under it, then one check per stream. Built as a Box of named rows rather than a
/// `PopoverMenu` because a menu model cannot be inspected by name under `GSK_RENDERER=cairo` — the same
/// reason the Cut page's effect dropdown is a Box.
fn track_menu(
    rows: &Rows,
    index: usize,
    tracks: &[TrackInfo],
    face: &str,
    dimmed: bool,
) -> gtk::MenuButton {
    let menu = gtk::MenuButton::builder().label(face).build();
    menu.set_widget_name(&format!("track-{index}"));
    menu.add_css_class("flat");
    // §4: the track button's tooltip is its own, without the four-symbol legend.
    menu.set_tooltip_text(Some(sources::TRACKS_HELP));
    menu.set_sensitive(!dimmed);

    let popover = gtk::Popover::new();
    let list = gtk::Box::new(gtk::Orientation::Vertical, 6);
    list.set_widget_name(&format!("track-popover-{index}"));
    let head = gtk::Label::new(Some(sources::TRACKS_TITLE));
    head.set_widget_name(&format!("track-title-{index}"));
    head.set_xalign(0.0);
    head.add_css_class("heading");
    list.append(&head);
    let help = gtk::Label::new(Some(sources::TRACKS_HELP));
    help.set_widget_name(&format!("track-help-{index}"));
    help.set_wrap(true);
    help.set_xalign(0.0);
    list.append(&help);

    let held = rows.session.clone();
    let status = rows.status.clone();
    let face_button = menu.clone();
    let shown = face.to_string();
    let total = tracks.len();
    for (at, track) in tracks.iter().enumerate() {
        // `tracks` holds 0-based AUDIO stream indices (01 §2; ffmpeg's `0:a:N`, the first audio stream 0):
        // the track's place among the file's audio streams, not ffprobe's file-wide index the label shows.
        let stream = at as u32;
        let ticked = sources::ticked_tracks(&held.borrow().sources[index]).contains(&stream);
        let check = gtk::CheckButton::with_label(&track.label());
        check.set_widget_name(&format!("track-{index}-{}", track.index));
        check.set_active(ticked);
        let held = held.clone();
        let status = status.clone();
        let face_button = face_button.clone();
        let shown = shown.clone();
        let number = track.index;
        check.connect_toggled(move |box_| {
            let on = box_.is_active();
            if !sources::tick_track(&mut held.borrow_mut(), index, stream, on) {
                // The last ticked track cannot come unticked (§4): a row hearing nothing is not a
                // choice anyone made. The box goes back the way it came and the reason is said.
                box_.set_active(true);
                status.set_text(&sources::last_track_says(number));
                return;
            }
            let left = sources::ticked_tracks(&held.borrow().sources[index]).len();
            face_button.set_label(&format!("{left}/{total}"));
            status.set_text(&format!(
                "{shown}: track {number} {}",
                if on { "in" } else { "out" }
            ));
        });
        list.append(&check);
    }
    popover.set_child(Some(&list));
    menu.set_popover(Some(&popover));
    menu
}

/// The 🎥 toggle. Each handler takes the handles it needs by value, which is what keeps this to one
/// `Rc` clone per control — the rule itself is [`sources::toggle_footage`], and the page only repeats
/// what it did.
fn wire_footage(
    footage: &gtk::ToggleButton,
    session: Rc<RefCell<Project>>,
    status: &gtk::Label,
    index: usize,
    name: String,
) {
    let status = status.clone();
    footage.connect_clicked(move |button| {
        if sources::toggle_footage(&mut session.borrow_mut(), index, button.is_active()) {
            status.set_text(&format!(
                "{name}: {}",
                if button.is_active() {
                    "footage"
                } else {
                    "not footage"
                }
            ));
            return;
        }
        // Refused (only a video may be footage): the button goes back, so it never shows what the
        // session does not hold, as 🗣's refusal does.
        button.set_active(!button.is_active());
    });
}

/// The 🎤 narrator button. `face` is the button's own handle: `connect_clicked` takes the closure by
/// moving it, so the button cannot also be borrowed into it.
fn wire_narrator(
    narrator: &gtk::Button,
    slot_face: &gtk::Label,
    session: Rc<RefCell<Project>>,
    status: &gtk::Label,
    index: usize,
    name: String,
) {
    let status = status.clone();
    let face = narrator.clone();
    let slot_face = slot_face.clone();
    narrator.connect_clicked(move |_| {
        let slot = sources::cycle_narrator(&mut session.borrow_mut(), index);
        // The number in the face is the slot it just took. Only the number: `set_label` on the button
        // replaced its whole face (the microphone with the number) with a bare number.
        slot_face.set_text(&slot.to_string());
        face.set_tooltip_text(Some(&format!(
            "{}{}",
            sources::slot_tip(slot),
            sources::ROW_KEY
        )));
        status.set_text(&format!("{name}: narrator {slot}"));
    });
}

/// The 🗣 toggle: whose voice is split off on ▶. The refusal (a split product has no voice left to
/// take off) is [`sources::set_sepvoice`]'s; a refused tick springs back so the box never disagrees
/// with the session.
fn wire_separate(
    separate: &gtk::ToggleButton,
    session: Rc<RefCell<Project>>,
    status: &gtk::Label,
    index: usize,
    name: String,
) {
    let status = status.clone();
    separate.connect_clicked(move |button| {
        let asked = button.is_active();
        if sources::set_sepvoice(&mut session.borrow_mut(), index, asked) {
            status.set_text(&format!(
                "{name}: {}",
                if asked {
                    "voice split off on \u{25b6}"
                } else {
                    "used as it stands"
                }
            ));
            return;
        }
        button.set_active(!asked);
        status.set_text(&format!(
            "{name}: already a split product \u{2014} nothing left to split"
        ));
    });
}

/// The 🗑 button. Removal re-indexes every row below it, so the list is redrawn from the session right
/// after — otherwise the row under the one that left still points at the removed index.
fn wire_remove(remove: &gtk::Button, rows: Rows, index: usize) {
    remove.connect_clicked(move |_| {
        // The path is read before the list changes, so the sentence can name what left.
        let Some(path) = rows
            .session
            .borrow()
            .sources
            .get(index)
            .map(|source| source.path.clone())
        else {
            return;
        };
        if sources::remove(&mut rows.session.borrow_mut(), index).is_some() {
            let left = rows.session.borrow().sources.len();
            repaint(&rows);
            rows.status.set_text(&sources::removal_status(&path, left));
        }
    });
}

/// The row's own view of its resolved path, so nothing outside has to know which of the one path rule's
/// three spellings a row uses.
fn resolved(source: &Source) -> PathBuf {
    let (root, project_dir) = session_roots();
    crate::layout::resolve(&root, &project_dir, &source.path)
}
