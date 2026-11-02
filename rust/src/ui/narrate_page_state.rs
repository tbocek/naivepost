//! Everything the Narrate page holds between presses, in one place. One per window thread.

pub(crate) struct NarratePage {
    /// The drag this window put on the band, so a test can reach the one the page installed.
    pub(crate) drag: std::cell::RefCell<Option<gtk4::GestureDrag>>,
    /// Fit-relative zoom of the view. 1.0 = the whole recording across the lane.
    pub(crate) zoom: std::cell::Cell<f64>,
    /// Px scrolled from the recording's start; never outside the file ([`band::clamp_view`]).
    pub(crate) offset: std::cell::Cell<f64>,
    /// The red ▶-start bar, in seconds of the recording.
    pub(crate) red_bar: std::cell::Cell<f64>,
    /// Has anyone put the red bar down yet? Until then it rides the cut's playhead.
    pub(crate) red_bar_seen: std::cell::Cell<bool>,
    /// The preview's second the bar last followed ([`follow_head`]).
    pub(crate) head_seen: std::cell::Cell<f64>,
    /// The live selection in seconds: written by motion, emptied by a click.
    pub(crate) selection: std::cell::RefCell<Option<(f64, f64)>>,
    /// The drag's press second, so release can ask `screen::take_press` with both ends.
    pub(crate) press_x: std::cell::Cell<f64>,
    /// The pointer's x in lane px, or negative when it is off the band.
    pub(crate) pointer_x: std::cell::Cell<f64>,
    pub(crate) view_width: std::cell::Cell<f64>,
    /// The lane's own painted width: the whole recording at the zoom, scrolled through by the scroller.
    pub(crate) lane_width: std::cell::Cell<f64>,
    pub(crate) walk_head: std::cell::Cell<f64>,
    /// The recording's seconds, as last handed to the painter.
    pub(crate) duration: std::cell::Cell<f64>,
    pub(crate) envelope: std::cell::RefCell<
        Option<(
            crate::ui::narrate_band::EnvelopeKey,
            Option<crate::ui::narrate_band::Peaks>,
        )>,
    >,
    /// The project and tape the page's takes were last read for.
    pub(crate) tape_seen:
        std::cell::RefCell<Option<(std::path::PathBuf, crate::narrate_tape::Tape)>>,
    pub(crate) narrate_state: std::cell::RefCell<crate::ui::narrate_page::NarrateState>,
    pub(crate) page_css_loaded: std::cell::Cell<bool>,
    /// True while the PAGE moves the scrub thumb: a programmatic `set_value` fires `value-changed` like a drag, which would seek.
    pub(crate) syncing_scrub: std::cell::Cell<bool>,
    /// Set while [`draw_rows`] rebuilds the list: GTK self-selects a row when the selected one is removed, and F4.5 S5's jump belongs to a person's selection only.
    pub(crate) rebuilding_rows: std::cell::Cell<bool>,
    pub(crate) game_guard: std::cell::Cell<bool>,
    pub(crate) pending_scrub: std::cell::RefCell<Option<(f64, glib::SourceId)>>,
    pub(crate) walk: std::cell::RefCell<Option<crate::ui::narrate_page_take::Walk>>,
    pub(crate) picker_seeding: std::cell::Cell<bool>,
    pub(crate) last_spoken_voice: std::cell::RefCell<Option<(String, bool)>>,
    /// What `narrate-video` shows now, or `None` before anything was put there.
    pub(crate) shown: std::cell::RefCell<Option<crate::ui::narrate_picture::Shown>>,
    /// A recheck is already set for a listing or a decode a worker has not answered yet.
    pub(crate) recheck: std::cell::Cell<bool>,
    /// How many "Add file…" choosers the button raised.
    pub(crate) choosers_raised: std::cell::Cell<usize>,
    pub(crate) opened_by_narrate: std::cell::Cell<bool>,
}

thread_local! {
    pub(crate) static PAGE: NarratePage = NarratePage {
        drag: std::cell::RefCell::new(None),
        zoom: std::cell::Cell::new(1.0),
        offset: std::cell::Cell::new(0.0),
        red_bar: std::cell::Cell::new(0.0),
        red_bar_seen: std::cell::Cell::new(false),
        head_seen: std::cell::Cell::new(-1.0),
        selection: std::cell::RefCell::new(None),
        press_x: std::cell::Cell::new(0.0),
        pointer_x: std::cell::Cell::new(-1.0),
        view_width: std::cell::Cell::new(240.0),
        lane_width: std::cell::Cell::new(240.0),
        walk_head: std::cell::Cell::new(-1.0),
        duration: std::cell::Cell::new(0.0),
        envelope: std::cell::RefCell::new(None),
        tape_seen: std::cell::RefCell::new(None),
        narrate_state: std::cell::RefCell::new(crate::ui::narrate_page::NarrateState { entries: Vec::new(), segs: Vec::new(), takes: Vec::new(), voice: String::new(), pitch: 0.0, session: 0.0, cut_at: 0.0, length: 0.0, clips: 0, unwritten: 0, off_cut: 0, has_cut: false, has_timeline: false, narrators: 0, voice_files: Vec::new(), recording_covers_clip: true, playing: false, started: false, busy: false, covered_spans: Vec::new(), language: String::new(), }),
        page_css_loaded: std::cell::Cell::new(false),
        syncing_scrub: std::cell::Cell::new(false),
        rebuilding_rows: std::cell::Cell::new(false),
        game_guard: std::cell::Cell::new(false),
        pending_scrub: std::cell::RefCell::new(None),
        walk: std::cell::RefCell::new(None),
        picker_seeding: std::cell::Cell::new(false),
        last_spoken_voice: std::cell::RefCell::new(None),
        shown: std::cell::RefCell::new(None),
        recheck: std::cell::Cell::new(false),
        choosers_raised: std::cell::Cell::new(0),
        opened_by_narrate: std::cell::Cell::new(false),
    };
}
