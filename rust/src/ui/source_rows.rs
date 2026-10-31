//! §4's source rows: the controls one file gets, drawn from [`crate::sources`] and wired to it.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::project::{Project, Source};
use crate::services;
use crate::source_probe;
use crate::sources::{self, Control, TrackInfo};

#[derive(Clone)]
pub struct Rows {
    pub session: Rc<RefCell<Project>>,
    pub status: gtk::Label,
    pub list: gtk::ListBox,
}

pub fn append_all(rows: &Rows) {
    let count = rows.session.borrow().sources.len();
    for index in 0..count {
        rows.list.append(&row_at(rows, index));
    }
}

pub fn repaint(rows: &Rows) {
    while let Some(child) = rows.list.last_child() {
        rows.list.remove(&child);
    }
    append_all(rows);
}

/// The 🎤 narrator button's face: the microphone beside the slot it holds. §4 draws "🎤 1".
pub const NARRATOR_ICON: &str = "audio-input-microphone-symbolic";
/// The 🎥 footage toggle's face: §4's camera.
pub const FOOTAGE_ICON: &str = "camera-video-symbolic";
pub const SEPARATE_ICON: &str = "audio-speakers-symbolic";

/// A symbol control: an icon, flat so a row of them reads as one band rather than a row of boxes,
/// and carrying §4's key to the four symbols at the end of its tooltip.
fn icon_toggle(icon: &str, name: &str, tip: &str) -> gtk::ToggleButton {
    let button = gtk::ToggleButton::builder().icon_name(icon).build();
    button.set_widget_name(name);
    button.add_css_class("flat");
    button.set_tooltip_text(Some(&format!("{tip}{}", sources::ROW_KEY)));
    button
}

/// The same for a control that is pressed rather than ticked.
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

fn session_roots() -> (PathBuf, PathBuf) {
    let root = std::env::current_dir().unwrap_or_default();
    (root, crate::startup::open_dir())
}

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

fn ffprobe_now() -> Option<String> {
    let paths = crate::settings::from_environment()?;
    let conf = crate::settings::read(&paths).unwrap_or_default();
    Some(services::ffprobe_for(&conf.ffmpeg, "ffprobe"))
}

type Version = (PathBuf, u64, Option<std::time::SystemTime>);

static PROBED: std::sync::Mutex<Vec<(Version, Vec<TrackInfo>)>> = std::sync::Mutex::new(Vec::new());

pub fn probed_here_for_test() -> usize {
    PROBED_HERE.with(std::cell::Cell::get)
}

thread_local! {
    static PROBED_HERE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static ASKED: RefCell<Vec<Version>> = const { RefCell::new(Vec::new()) };
}

fn version_of(file: &std::path::Path) -> Version {
    let meta = std::fs::metadata(file).ok();
    (
        file.to_path_buf(),
        meta.as_ref().map(|meta| meta.len()).unwrap_or(0),
        meta.and_then(|meta| meta.modified().ok()),
    )
}

fn probed(version: &Version) -> Option<Vec<TrackInfo>> {
    PROBED.lock().ok().and_then(|held| {
        held.iter()
            .find(|(known, _)| known == version)
            .map(|(_, tracks)| tracks.clone())
    })
}

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

/// One row: the file's name, and the controls §4 gives it.
pub fn row_at(rows: &Rows, index: usize) -> gtk::ListBoxRow {
    let session = rows.session.clone();
    let source = session.borrow().sources[index].clone();
    let name = sources::row_name(&source.path);
    // §4's stream count, from the file itself: probed once per file version on a worker, and drawn
    // from what landed.
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
    // §4's face is the mic AND the slot: the mic says what the button is for, the number says which
    // slot it holds.
    let face = gtk::Box::new(gtk::Orientation::Horizontal, 3);
    face.append(&gtk::Image::from_icon_name(NARRATOR_ICON));
    let slot_face = gtk::Label::new(Some(&source.narrator.to_string()));
    slot_face.set_widget_name(&format!("narrator-face-{index}"));
    face.append(&slot_face);
    narrator.set_child(Some(&face));
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
        // §4's ⚠: a name with no stamp starts where the session does, which is a guess worth
        // saying.
        let warn = gtk::Image::from_icon_name("dialog-warning-symbolic");
        warn.set_widget_name(&format!("warning-{index}"));
        warn.set_tooltip_text(Some(tip));
        box_.append(&warn);
    }

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

/// The track menu: a `MenuButton` faced "<on>/<total>" over a popover headed §4's title, the
/// three-line explanation under it, then one check per stream.
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
        // `tracks` holds 0-based AUDIO stream indices (01 §2; ffmpeg's `0:a:N`, the first audio
        // stream 0): the track's place among the file's audio streams, not ffprobe's file-wide
        // index the label shows.
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
                // choice anyone made.
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
        slot_face.set_text(&slot.to_string());
        face.set_tooltip_text(Some(&format!(
            "{}{}",
            sources::slot_tip(slot),
            sources::ROW_KEY
        )));
        status.set_text(&format!("{name}: narrator {slot}"));
    });
}

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

fn resolved(source: &Source) -> PathBuf {
    let (root, project_dir) = session_roots();
    crate::layout::resolve(&root, &project_dir, &source.path)
}
