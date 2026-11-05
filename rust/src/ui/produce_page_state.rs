//! Everything the Produce page holds between presses, in one place. One per window thread: the page is only ever
//! touched from there, and the render worker reads what it was handed.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::PathBuf;

use gtk4 as gtk;

use crate::project::TitleBox;
use crate::ui::cut_picture_cache::FolderStamps;
use crate::ui::produce_languages::DrawSlot;
use crate::ui::produce_page::ProduceState;
use crate::ui::produce_paint::{Held, SessionFrames, Stamp};
use crate::ui::produce_words_press::Hold;

pub(crate) struct ProducePage {
    /// The session's frames as last walked, and the folder stamps they were walked at.
    pub(crate) session_frames: RefCell<Option<(FolderStamps, SessionFrames)>>,
    /// The path the save door last answered with; `None` means nobody typed anything.
    pub(crate) asked_path: RefCell<Option<String>>,
    pub(crate) state: RefCell<ProduceState>,
    /// `set_selected` / `set_value` / `set_active` emit their own change signals; handlers check this first so the page does not answer its own repaint as an edit.
    pub(crate) settings_guard: Cell<bool>,
    pub(crate) css_loaded: Cell<bool>,
    /// The page's root, remembered at `build()`; ONE per process, so a second window cannot also claim it (see `wire`).
    pub(crate) root: RefCell<Option<gtk::Widget>>,
    pub(crate) draw: RefCell<Option<(DrawSlot, Vec<u8>)>>,
    pub(crate) words_dialog: RefCell<Option<adw::MessageDialog>>,
    pub(crate) confirm_dialog: RefCell<Option<adw::MessageDialog>>,
    /// Every plate picture decoded so far, with the stamp it was decoded at; `asked` holds what is on a worker now.
    pub(crate) pictures: RefCell<HashMap<PathBuf, (Stamp, Held)>>,
    pub(crate) asked: RefCell<HashMap<PathBuf, Stamp>>,
    /// The picture each slot's last real draw showed, for the test that must see a slot fill in without painting it itself.
    pub(crate) slot_shown: RefCell<HashMap<usize, Option<PathBuf>>>,
    /// The folder a snapshot asked this page to render. `build_window` resets `save_apply::open_dir` AFTER a
    /// snapshot stages the real one, so the plates keep their own slot for it.
    pub(crate) open_project: RefCell<Option<PathBuf>>,
    pub(crate) asks: RefCell<HashMap<&'static str, usize>>,
    /// [`dragging`]'s answer: set on every step of a drag that holds a box, cleared when it ends.
    pub(crate) dragging: RefCell<Option<(Hold, TitleBox)>>,
    /// The picture's drag controller, kept so a test can fire a real drag; nothing in the program asks for it.
    pub(crate) words_gesture: RefCell<Option<gtk::GestureDrag>>,
    /// A press of the page is on its worker: a second press is ignored until the first one's answer lands, so
    /// two walks never write `publish.json` or the video at once.
    pub(crate) busy: Cell<bool>,
}

thread_local! {
    pub(crate) static PAGE: ProducePage = ProducePage {
        session_frames: RefCell::new(None),
        asked_path: RefCell::new(None),
        state: RefCell::new(ProduceState { frames: Vec::new(), own: false, texts: Vec::new(), title: String::new(), thumb_title: String::new(), title_seeded: false, title_box: None, instruction: String::new(), negative: String::new(), description: String::new(), container: String::new(), codec: String::new(), preset: String::new(), resolution: String::new(), frame_rate: String::new(), audio: String::new(), subtitles: String::new(), translate: Vec::new(), game_volume: 0.22, crf: 24.0, mono: false, blurred_edges: true, clips: 0, seconds: 0.0, to_speak: 0, publish_written: false, session_language: String::new(), cut_line: String::new(), first_spoken: String::new(), mixed_recordings: Vec::new(), voice: String::new(), out_files: 0, out_bytes: 0, base_shape: None, video_shape: (0, 0), }),
        settings_guard: Cell::new(false),
        css_loaded: Cell::new(false),
        root: RefCell::new(None),
        draw: RefCell::new(None),
        words_dialog: RefCell::new(None),
        confirm_dialog: RefCell::new(None),
        pictures: RefCell::new(HashMap::new()),
        asked: RefCell::new(HashMap::new()),
        slot_shown: RefCell::new(HashMap::new()),
        open_project: RefCell::new(None),
        asks: RefCell::new(HashMap::new()),
        dragging: RefCell::new(None),
        words_gesture: RefCell::new(None),
        busy: Cell::new(false),
    };
}
