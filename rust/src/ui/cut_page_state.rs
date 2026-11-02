//! Everything the Cut page holds between presses, in one place. One per window thread.

pub(crate) struct CutPage {
    pub(crate) whole:
        std::cell::RefCell<Option<(std::path::PathBuf, std::time::SystemTime, Vec<String>)>>,
    pub(crate) band_press_drags: std::cell::RefCell<Option<crate::ui::cut_band_press::BarDrag>>,
    /// The window the CURRENT gesture belongs to: a closed GTK window survives and its controllers keep answering, so an old gesture must not be read against another window's cut.
    pub(crate) active: std::cell::RefCell<Option<adw::ApplicationWindow>>,
    /// This window's bar gestures, newest last, each tagged with the record its bar stands for.
    pub(crate) gestures:
        std::cell::RefCell<Vec<(crate::ui::cut_band_press::BarSlot, gtk4::GestureDrag)>>,
    /// Is a bar drag in flight, so the whole gesture costs ONE history entry?
    pub(crate) in_flight: std::cell::Cell<bool>,
    /// Has this drag already spent its single Undo step?
    pub(crate) history_spent: std::cell::Cell<bool>,
    pub(crate) left: std::cell::RefCell<Option<crate::ui::cut_band_trim::BorderDrag>>,
    pub(crate) right: std::cell::RefCell<Option<crate::ui::cut_band_trim::RightPress>>,
    /// The thumbnail-size grip in hand (§1 badge 2): how many rows sit above the edge taken, and the height the
    /// thumbnails had at the press.
    pub(crate) grip: std::cell::Cell<Option<(usize, u32)>>,
    /// A drag on the line above the sound strip: the strip's height at the press.
    pub(crate) wave_grip: std::cell::Cell<Option<f64>>,
    /// The pointer the band last asked for: set only when it changes, since asking the display for a pointer
    /// it already shows at every motion event is a round trip for nothing.
    pub(crate) shown: std::cell::Cell<Option<&'static str>>,
    /// The band's left-button drag, kept so a test can fire the grip and the border drags the way GTK does.
    pub(crate) left_gesture: std::cell::RefCell<Option<gtk4::GestureDrag>>,
    pub(crate) cam_watches:
        std::cell::RefCell<Vec<std::rc::Rc<std::cell::RefCell<crate::cut_cam::Watch>>>>,
    /// The watched-row frame's CSS, installed on the first refresh that needs it and never again.
    pub(crate) watched_css_loaded: std::cell::Cell<bool>,
    pub(crate) camera_refresh: std::cell::Cell<bool>,
    /// The hold gesture of the band built most recently, for a test; [`wire`] attaches and forgets it.
    pub(crate) card_hold_clicks: std::cell::RefCell<Option<gtk4::GestureClick>>,
    /// Frame folders the snapshot bench names, because its folder is a fixture and not an open project. Same
    /// reason `cut_picture` carries a snapshot dir: a headless render has no window with a root, and a card's
    /// frames still have to go somewhere beside the session they were made for.
    pub(crate) snapshot_cards_dir: std::cell::RefCell<Option<std::path::PathBuf>>,
    /// Each recording file's cache folder, worked out once per process (its path, size and mtime).
    pub(crate) folders: std::cell::RefCell<
        std::collections::HashMap<std::path::PathBuf, Option<std::path::PathBuf>>,
    >,
    /// Pictures known to be on disk, so a draw does not ask the file system again.
    pub(crate) present: std::cell::RefCell<std::collections::HashSet<std::path::PathBuf>>,
    /// Asked and not landed: a repaint does not ask again.
    pub(crate) film_cache_asked: std::cell::RefCell<std::collections::HashSet<std::path::PathBuf>>,
    /// Seconds ffmpeg could not cut (past the end, an unreadable file): asked once, then left to the fallback.
    pub(crate) failed: std::cell::RefCell<std::collections::HashSet<std::path::PathBuf>>,
    pub(crate) queue: std::cell::RefCell<Option<crate::ui::cut_film_cache::Queue>>,
    /// F2.11: one recording list per window, newest last like [`REVIEW_CUTS`].
    pub(crate) session_recordings:
        std::cell::RefCell<Vec<std::rc::Rc<std::cell::RefCell<Vec<crate::timeline::Recording>>>>>,
    pub(crate) session_declared: std::cell::RefCell<Vec<std::cell::Cell<bool>>>,
    /// F2.11 S1: the gaps this window's numbered badges stand for, newest window last like
    /// [`REVIEW_CUTS`].
    pub(crate) fold_tables:
        std::cell::RefCell<Vec<std::rc::Rc<std::cell::RefCell<Vec<(f64, f64)>>>>>,
    /// Per-form-kind debounce state: when the last keystroke landed, and whether a timer is already waiting for the
    /// pause to run out. `false` means none is, which is what makes a burst cost ONE commit: the first keystroke
    /// arms the timer, the rest only move the stamp.
    pub(crate) form_stamps:
        std::cell::RefCell<std::collections::HashMap<String, (std::time::Instant, bool)>>,
    /// The window's one hold. `None` = nothing picked up, also what a release leaves behind.
    pub(crate) held: std::cell::RefCell<Option<crate::ui::cut_hand::Hand>>,
    pub(crate) label_form: std::cell::RefCell<Option<crate::fx_label::Form>>,
    /// The window whose label form was last shown, so the field readers have THIS window's tree to
    /// look in and a stale window cannot answer for a live one.
    pub(crate) last_label_window: std::cell::RefCell<Option<adw::ApplicationWindow>>,
    /// Set while [`refresh_effects_lane`] pushes state into the widgets, so its own property writes cannot re-enter it.
    pub(crate) lane_refresh: std::cell::Cell<bool>,
    /// Whether the lane's CSS classes have been loaded once (loading per refresh would stack providers).
    pub(crate) lane_css_loaded: std::cell::Cell<bool>,
    pub(crate) picture_owed: std::cell::RefCell<Option<crate::ui::settle::Owed>>,
    pub(crate) frame_shown: std::cell::RefCell<Option<std::path::PathBuf>>,
    pub(crate) preview_asked: std::cell::RefCell<Option<(String, f64)>>,
    pub(crate) preview_base: std::cell::RefCell<Option<String>>,
    pub(crate) snapshot_dir: std::cell::RefCell<Option<std::path::PathBuf>>,
    /// The preview's picture shows the built-in player's paintable (F2.1), not an extracted frame.
    pub(crate) video_shown: std::cell::Cell<bool>,
    pub(crate) shapes:
        std::cell::RefCell<std::collections::HashMap<std::path::PathBuf, Option<(i32, i32)>>>,
    pub(crate) plate_shape: std::cell::RefCell<Option<(i32, i32)>>,
    pub(crate) lists: std::cell::RefCell<
        std::collections::HashMap<
            std::path::PathBuf,
            (crate::ui::cut_picture_cache::Stamp, Vec<std::path::PathBuf>),
        >,
    >,
    pub(crate) listing: std::cell::RefCell<std::collections::HashSet<std::path::PathBuf>>,
    pub(crate) pictures: std::cell::RefCell<
        std::collections::HashMap<
            std::path::PathBuf,
            (
                crate::ui::cut_picture_cache::Stamp,
                Option<crate::ui::cut_picture_cache::Picture>,
            ),
        >,
    >,
    pub(crate) decoding: std::cell::RefCell<Option<std::path::PathBuf>>,
    pub(crate) next: std::cell::RefCell<Option<std::path::PathBuf>>,
    pub(crate) rendering: std::cell::RefCell<std::collections::HashSet<std::path::PathBuf>>,
    pub(crate) on_landed: std::cell::Cell<Option<fn()>>,
    pub(crate) listed_here: std::cell::Cell<usize>,
    /// §7: "a draw must never block on a decode", so the LIVE page's picture row draws placeholder
    /// cells and reads no pixels at all. Set-only because a snapshot process is one-shot: there is
    /// no second page to return to a non-decoding state.
    pub(crate) decode_frames: std::cell::Cell<bool>,
    /// Decoded frames by (file, row height): `Some` once landed, `None` for a file that would not decode.
    pub(crate) thumbs: std::cell::RefCell<
        std::collections::HashMap<
            crate::ui::cut_picture_rows::ThumbKey,
            Option<cairo::ImageSurface>,
        >,
    >,
    pub(crate) picture_rows_asked:
        std::cell::RefCell<std::collections::HashSet<crate::ui::cut_picture_rows::ThumbKey>>,
    pub(crate) worker:
        std::cell::RefCell<Option<std::sync::mpsc::Sender<crate::ui::cut_picture_rows::ThumbKey>>>,
    pub(crate) bands: std::cell::RefCell<Vec<glib::WeakRef<gtk4::DrawingArea>>>,
    /// The box under the hand, in panel px, written by [`note_box`] and read at paint time.
    pub(crate) drawn_box: std::cell::Cell<Option<(f64, f64, f64, f64)>>,
    /// Whether the §F3.4 S5 snap lines are shown: set while a placed caption's box is dragged so the frame draws the six `fx_text::snap_lines_px` lines ([`paint_paused`]).
    pub(crate) edit_snap: std::cell::Cell<bool>,
    /// The stop's still: the frame file, its change time and its surface; a paint never decodes (§03-shell §1).
    pub(crate) still: std::cell::RefCell<
        Option<(
            std::path::PathBuf,
            Option<std::time::SystemTime>,
            Option<cairo::ImageSurface>,
        )>,
    >,
    pub(crate) still_asked:
        std::cell::RefCell<Option<(std::path::PathBuf, Option<std::time::SystemTime>)>>,
    /// The fingerprint the page was last built from. `Some(None)` means "this state has been looked at and there
    /// is no output" — distinct from `None`, which means nobody has looked yet. Collapsing those two would make
    /// a session with nothing prepared either rebuild on every beat or never notice its first Prepare.
    pub(crate) seen: std::cell::RefCell<Option<Option<(u64, u64)>>>,
    pub(crate) in_video: std::cell::Cell<Option<(f64, f64)>>,
    pub(crate) drag_begins: std::cell::RefCell<Vec<Option<f64>>>,
    pub(crate) select_press_drags: std::cell::RefCell<
        Vec<std::rc::Rc<std::cell::RefCell<Option<crate::ui::cut_select_press::Drag>>>>,
    >,
    /// This window's band gesture, kept here because `attach` is the only thing that writes it and a test
    /// firing it is the only thing that reads it back.
    pub(crate) band_select_gestures: std::cell::RefCell<Option<gtk4::GestureDrag>>,
    pub(crate) lengths: std::cell::RefCell<std::collections::HashMap<String, f64>>,
    pub(crate) unreadable: std::cell::RefCell<Vec<String>>,
    pub(crate) measuring: std::cell::RefCell<(Vec<String>, Option<crate::ui::settle::Owed>)>,
    /// The form a Speed press left waiting; `None` is no form open, which also hides the holder.
    pub(crate) speed_form: std::cell::RefCell<Option<crate::cut_speed::Form>>,
    /// The window whose speed form was last shown, so the field readers look in THIS window's tree.
    pub(crate) last_speed_window: std::cell::RefCell<Option<adw::ApplicationWindow>>,
    /// The band's Esc controller, kept so a test can fire a real Escape through it.
    pub(crate) band_esc_keys: std::cell::RefCell<Option<gtk4::EventControllerKey>>,
    /// The strip's own scroll in px.
    pub(crate) scroll: std::cell::Cell<f64>,
    pub(crate) recorder: std::cell::Cell<Option<crate::ui::cut_suggest_land::Recorder>>,
    pub(crate) read: std::cell::Cell<Option<fn() -> crate::cut::Cut>>,
    pub(crate) session: std::cell::RefCell<Option<crate::ui::cut_suggest_land::Session>>,
    pub(crate) paint: std::cell::Cell<Option<crate::ui::cut_suggest_progress::Paint>>,
    /// Every reading painted on this thread, oldest first: GTK's `ProgressBar` has no getter and the press overwrites the sentence.
    pub(crate) said: std::cell::RefCell<Vec<crate::ui::cut_suggest_watch::Progress>>,
    /// How many drawing choosers this thread has raised: a headless test cannot answer one, but it can see that a
    /// press raised exactly one.
    pub(crate) svg_chooser_asks: std::cell::Cell<usize>,
    pub(crate) svg_armed: std::cell::Cell<bool>,
    /// The chosen drawing. Carried apart from the form because the arm happens before the form exists: the arm
    /// sentence names the file, and the record's `src` is that same string.
    pub(crate) svg_file: std::cell::RefCell<Option<String>>,
    /// The form a landed box left waiting on the page.
    pub(crate) svg_form: std::cell::RefCell<Option<crate::fx_svg::Form>>,
    /// The only strong handle on the window whose form is live, so a closed window's surviving tree cannot
    /// answer for a live one (see `zoom_form_box`'s comment on the `gtk_box_append` assertion).
    pub(crate) last_svg_window: std::cell::RefCell<Option<adw::ApplicationWindow>>,
    /// The per-file preview raster cache, process-wide so one logo placed six times asks ffmpeg ONCE (S5). Not
    /// per-window: the raster belongs to the file, not to whoever opened it.
    pub(crate) svg_rasters: std::cell::RefCell<crate::fx_svg::Rasters>,
    pub(crate) text_armed: std::cell::Cell<bool>,
    pub(crate) text_form: std::cell::RefCell<Option<crate::fx_text::Form>>,
    pub(crate) last_text_window: std::cell::RefCell<Option<adw::ApplicationWindow>>,
    /// §A's 🖼− / 🖼+ thumbnail height (40..160 px), kept for the session, not saved.
    pub(crate) thumb_px: std::cell::Cell<u32>,
    /// The sound strip's height as the grip left it; session only, like the thumbnail height.
    pub(crate) wave_px: std::cell::Cell<f64>,
    /// Has this gesture's hold already spent its Undo step? One act of the hand is one step
    /// (`cut_trim::pushes_undo`). Reset at `drag-begin`.
    pub(crate) move_pushed: std::cell::Cell<bool>,
    /// The cut as the hold's press found it, set at `drag-begin`, dropped at the release; `None` with no hold open.
    pub(crate) move_open: std::cell::RefCell<Option<crate::cut::Cut>>,
    /// The folds THIS drag opened, as spans, so the release refolds exactly those.
    pub(crate) drag_folds: std::cell::RefCell<Vec<[f64; 2]>>,
    /// The folds the gesture in flight asked to open, newest write last: what `ui::open_folds` reads.
    pub(crate) drag_open: std::cell::RefCell<Vec<usize>>,
    /// The border this hold has moved, settled by [`land_trim`].
    pub(crate) trimmed: std::cell::Cell<Option<(usize, crate::cut_trim::Border)>>,
    pub(crate) verb_hand: std::cell::RefCell<Option<crate::cut_select::Selection>>,
    /// No zoom was saved for this project and none has been chosen: the session stays fitted to the band.
    pub(crate) fits_the_band: std::cell::Cell<bool>,
    /// The project the zoom was read from, and where a new one is kept.
    pub(crate) zoom_project: std::cell::RefCell<Option<crate::layout::Tree>>,
    /// The band's pixels-per-second, seeded at the open zoom: the one zoom the band paints with, and the one the
    /// toolbar's −/+ and the wheel both step.
    pub(crate) pps: std::cell::Cell<f64>,
    /// The left edge of the view, in timeline pixels from the start of the gutter.
    pub(crate) origin: std::cell::Cell<f64>,
    /// Where a Shift pan is gliding to, and where it last put the view, while one is under way.
    pub(crate) glide: std::cell::Cell<Option<(f64, f64)>>,
    pub(crate) bar_width: std::cell::Cell<f64>,
    /// The form a Volume press left waiting on the page. `None` is no form open, which is also what hides the
    /// holder — one slot answers both questions so they cannot disagree.
    pub(crate) volume_form: std::cell::RefCell<Option<crate::fx_volume::Form>>,
    /// The window whose volume form was last shown, so the field readers have THIS window's tree to look in
    /// and a stale window cannot answer for a live one.
    pub(crate) last_volume_window: std::cell::RefCell<Option<adw::ApplicationWindow>>,
    pub(crate) waves: std::cell::RefCell<
        std::collections::HashMap<
            std::path::PathBuf,
            (
                std::time::SystemTime,
                Option<std::rc::Rc<crate::wave::Wave>>,
            ),
        >,
    >,
    pub(crate) sources: std::cell::RefCell<
        std::collections::HashMap<
            std::path::PathBuf,
            (
                std::time::SystemTime,
                std::collections::HashMap<String, std::path::PathBuf>,
            ),
        >,
    >,
    /// Whether this session was ever told where its red line stands, like `ui::INSERT_PLACE_KNOWN`; `None` means never, so arming refuses instead of arming at t = 0.
    pub(crate) zoom_line_known: std::cell::RefCell<Option<bool>>,
    /// Whether ⊕ Zoom is armed, waiting for a box. Read by the drag area's tooltip and `zoom_drag_ended`.
    pub(crate) zoom_armed: std::cell::Cell<bool>,
    /// The form a finished drag left before Apply; `None` hides `zoom-form`. Held by value, as the cut keeps moving under it.
    pub(crate) zoom_form: std::cell::RefCell<Option<crate::fx_zoom::Form>>,
    pub(crate) zoom_form_refresh: std::cell::Cell<bool>,
    /// The window whose form was last shown, so readers look in THIS window's tree.
    pub(crate) last_zoom_window: std::cell::RefCell<Option<adw::ApplicationWindow>>,
}

thread_local! {
    pub(crate) static PAGE: CutPage = CutPage {
        whole: std::cell::RefCell::new(None),
        band_press_drags: std::cell::RefCell::new(None),
        active: std::cell::RefCell::new(None),
        gestures: std::cell::RefCell::new(Vec::new()),
        in_flight: std::cell::Cell::new(false),
        history_spent: std::cell::Cell::new(false),
        left: std::cell::RefCell::new(None),
        right: std::cell::RefCell::new(None),
        grip: std::cell::Cell::new(None),
        wave_grip: std::cell::Cell::new(None),
        shown: std::cell::Cell::new(None),
        left_gesture: std::cell::RefCell::new(None),
        cam_watches: std::cell::RefCell::new(Vec::new()),
        watched_css_loaded: std::cell::Cell::new(false),
        camera_refresh: std::cell::Cell::new(false),
        card_hold_clicks: std::cell::RefCell::new(None),
        snapshot_cards_dir: std::cell::RefCell::new(None),
        folders: std::cell::RefCell::new(std::collections::HashMap::new()),
        present: std::cell::RefCell::new(std::collections::HashSet::new()),
        film_cache_asked: std::cell::RefCell::new(std::collections::HashSet::new()),
        failed: std::cell::RefCell::new(std::collections::HashSet::new()),
        queue: std::cell::RefCell::new(None),
        session_recordings: std::cell::RefCell::new(Vec::new()),
        session_declared: std::cell::RefCell::new(Vec::new()),
        fold_tables: std::cell::RefCell::new(Vec::new()),
        form_stamps: std::cell::RefCell::new(std::collections::HashMap::new()),
        held: std::cell::RefCell::new(None),
        label_form: std::cell::RefCell::new(None),
        last_label_window: std::cell::RefCell::new(None),
        lane_refresh: std::cell::Cell::new(false),
        lane_css_loaded: std::cell::Cell::new(false),
        picture_owed: std::cell::RefCell::new(None),
        frame_shown: std::cell::RefCell::new(None),
        preview_asked: std::cell::RefCell::new(None),
        preview_base: std::cell::RefCell::new(None),
        snapshot_dir: std::cell::RefCell::new(None),
        video_shown: std::cell::Cell::new(false),
        shapes: std::cell::RefCell::new(std::collections::HashMap::new()),
        plate_shape: std::cell::RefCell::new(None),
        lists: std::cell::RefCell::new(std::collections::HashMap::new()),
        listing: std::cell::RefCell::new(std::collections::HashSet::new()),
        pictures: std::cell::RefCell::new(std::collections::HashMap::new()),
        decoding: std::cell::RefCell::new(None),
        next: std::cell::RefCell::new(None),
        rendering: std::cell::RefCell::new(std::collections::HashSet::new()),
        on_landed: std::cell::Cell::new(None),
        listed_here: std::cell::Cell::new(0),
        decode_frames: std::cell::Cell::new(false),
        thumbs: std::cell::RefCell::new(std::collections::HashMap::new()),
        picture_rows_asked: std::cell::RefCell::new(std::collections::HashSet::new()),
        worker: std::cell::RefCell::new(None),
        bands: std::cell::RefCell::new(Vec::new()),
        drawn_box: std::cell::Cell::new(None),
        edit_snap: std::cell::Cell::new(false),
        still: std::cell::RefCell::new(None),
        still_asked: std::cell::RefCell::new(None),
        seen: std::cell::RefCell::new(None),
        in_video: std::cell::Cell::new(None),
        drag_begins: std::cell::RefCell::new(Vec::new()),
        select_press_drags: std::cell::RefCell::new(Vec::new()),
        band_select_gestures: std::cell::RefCell::new(None),
        lengths: std::cell::RefCell::new(std::collections::HashMap::new()),
        unreadable: std::cell::RefCell::new(Vec::new()),
        measuring: std::cell::RefCell::new((Vec::new(), None)),
        speed_form: std::cell::RefCell::new(None),
        last_speed_window: std::cell::RefCell::new(None),
        band_esc_keys: std::cell::RefCell::new(None),
        scroll: std::cell::Cell::new(0.0),
        recorder: std::cell::Cell::new(None),
        read: std::cell::Cell::new(None),
        session: std::cell::RefCell::new(None),
        paint: std::cell::Cell::new(None),
        said: std::cell::RefCell::new(Vec::new()),
        svg_chooser_asks: std::cell::Cell::new(0),
        svg_armed: std::cell::Cell::new(false),
        svg_file: std::cell::RefCell::new(None),
        svg_form: std::cell::RefCell::new(None),
        last_svg_window: std::cell::RefCell::new(None),
        svg_rasters: std::cell::RefCell::default(),
        text_armed: std::cell::Cell::new(false),
        text_form: std::cell::RefCell::new(None),
        last_text_window: std::cell::RefCell::new(None),
        thumb_px: std::cell::Cell::new(crate::ui::cut_tracks::THUMB_DEFAULT_PX),
        wave_px: std::cell::Cell::new(crate::ui::cut_tracks::ROW_WAVE_PX),
        move_pushed: std::cell::Cell::new(false),
        move_open: std::cell::RefCell::new(None),
        drag_folds: std::cell::RefCell::new(Vec::new()),
        drag_open: std::cell::RefCell::new(Vec::new()),
        trimmed: std::cell::Cell::new(None),
        verb_hand: std::cell::RefCell::new(None),
        fits_the_band: std::cell::Cell::new(false),
        zoom_project: std::cell::RefCell::new(None),
        pps: std::cell::Cell::new(crate::cut_screen::ZOOM_AT_OPEN),
        origin: std::cell::Cell::new(0.0),
        glide: std::cell::Cell::new(None),
        bar_width: std::cell::Cell::new(0.0),
        volume_form: std::cell::RefCell::new(None),
        last_volume_window: std::cell::RefCell::new(None),
        waves: std::cell::RefCell::new(std::collections::HashMap::new()),
        sources: std::cell::RefCell::new(std::collections::HashMap::new()),
        zoom_line_known: std::cell::RefCell::new(None),
        zoom_armed: std::cell::Cell::new(false),
        zoom_form: std::cell::RefCell::new(None),
        zoom_form_refresh: std::cell::Cell::new(false),
        last_zoom_window: std::cell::RefCell::new(None),
    };
}
