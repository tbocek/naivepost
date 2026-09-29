//! F3.5 ▨ SVG drawing by hand — the page half of the flow.
//!
//! Split out of `window.rs` unchanged: the rule half lives in [`crate::fx_svg`] (`press`/`Press`,
//! `chose`/`Chose`, `arm_words`/`arm_head`, `MIDDLE`, `camera_layer_stays_up`, `place`,
//! `form_title`/`FORM_FIELDS`/`CHOOSE_BUTTON`, `NO_LINE`/`NO_FILE`, `chooser_dir`/`base`, `Form`/`apply`,
//! and the S5 raster: `PREVIEW_RASTER_PX`/`ffmpeg_raster`/`Rasters`/`Slot` and `fit_inside`). What is here
//! is only the state a press leaves behind and the widgets that show it — no decision of its own. Mirrors
//! the F3.4 text block one-for-one with the `svg_` prefix.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut;
use crate::cut_speed;
use crate::shell::Page;
use crate::fx_record;
use crate::fx_svg;
use crate::fx_text;
use crate::fx_zoom;
use crate::ui::window::{
    find_status, find_widget_by_name, line_step_button, live_form_answer_is_undo,
    live_form_refused, log_line, paste_line, preview_picture_px, record_edit,
    refresh_effects_lane, review_cut_of, save_insert_cut, selection, trim_seconds, zoom_field_row,
    INSERT_PLACE_KNOWN,
};

/// F3.5 S1: what pressing ▨ SVG did — ask for the file, or refuse for want of a line. NOTHING is recorded on
/// this press, and nothing is ARMED either: §F3.5 asks for the drawing first, because arming someone to place a
/// drawing they have not chosen yet puts the box down for nothing. So unlike `press_text_item` this never consults
/// an "already armed" flag — [`fx_svg::press`] has no `Armed` variant at all.
///
/// The refusal is answered BEFORE any chooser is built, so a cancelled dialog can never cost the user a refusal
/// (the F2.12 Insert lesson, restated here by the flowchart's own order: `a line?` comes before the chooser).
pub fn press_svg_item(window: &adw::ApplicationWindow) -> String {
    // The line is read through the same seam Paste / Insert / Speed use, gated by whether one was EVER placed,
    // so the page keeps one notion of "a line exists".
    let known = INSERT_PLACE_KNOWN.with(|cell| *cell.borrow()) == Some(true);
    let line = known.then(|| paste_line(window));
    match fx_svg::press(line) {
        fx_svg::Press::Refused(reason) => {
            close_svg_form(window);
            log_line(reason);
            reason.to_string()
        }
        fx_svg::Press::OpenChooser => {
            let root = std::env::current_dir().unwrap_or_default();
            let dir = fx_svg::chooser_dir(&root);
            open_svg_chooser(window, fx_svg::CHOOSE_TITLE, &dir, fx_svg::CHOOSE_FILTER);
            let said = format!("{} \u{2014} choose the drawing", fx_svg::CHOOSE_TITLE);
            log_line(&said);
            said
        }
    }
}

/// Open the native file chooser over `dir` for a drawing. Kept apart from [`press_svg_item`] so the decision
/// (where, which title, which filter) is testable without a display: a headless run simply never answers the
/// dialog, exactly as `open_insert_chooser` is. Its answer forwards to [`svg_chosen`].
fn open_svg_chooser(
    window: &adw::ApplicationWindow,
    title: &str,
    dir: &std::path::Path,
    filter_name: &str,
) {
    let chooser = gtk::FileChooserNative::builder()
        .title(title)
        .modal(true)
        .action(gtk::FileChooserAction::Open)
        .build();
    let _ = chooser.set_current_folder(Some(&gio::File::for_path(dir)));
    let filter = gtk::FileFilter::new();
    // §A.5 `filter svg`: one suffix only. A drawing is an `.svg`; nothing else belongs in this dialog.
    filter.set_name(Some(filter_name));
    filter.add_pattern("*.svg");
    chooser.add_filter(&filter);
    let win = window.clone();
    chooser.connect_response(move |chooser, response| {
        // Whatever the answer, it goes through the SAME door as a chosen file, so "a file was picked" and "the
        // dialog was cancelled" are two inputs to one path rather than two paths that can drift.
        let picked: Option<String> = if response == gtk::ResponseType::Accept {
            chooser
                .file()
                .as_ref()
                .and_then(|f| f.path())
                .map(|p| p.to_string_lossy().to_string())
        } else {
            None
        };
        let said = svg_chosen(&win, picked.as_deref());
        if let Some(status_line) = find_status(win.upcast_ref()) {
            status_line.set_text(&said);
        }
    });
    chooser.show();
}

/// F3.5 S2: the chooser answered. A file arms the gesture and names itself in the arm sentence; no file places
/// nothing. Never a toggle-off — [`fx_svg::chose`] takes no `already_armed` precisely so a second ▨ SVG press
/// cannot quietly disarm the drawing someone just picked on purpose.
pub fn svg_chosen(window: &adw::ApplicationWindow, file: Option<&str>) -> String {
    match fx_svg::chose(file) {
        fx_svg::Chose::Refused(reason) => {
            // No arm, no form, no record: there is no ink to place and no point arming a gesture without it.
            log_line(reason);
            reason.to_string()
        }
        fx_svg::Chose::Armed(name) => {
            set_svg_armed(true);
            set_svg_file(Some(name.clone()));
            // THE CAMERA STAYS UP. `fx_svg::camera_layer_stays_up()` is true (§A.5: a drawing is a text with ink,
            // measured against the OUTPUT frame), so NO svg path calls `set_camera_layer_down` — stepping the
            // camera aside would move the ground the box is being drawn on. That call belongs to ⊕ Zoom alone.
            request_svg_raster(&name);
            // A marked stretch outranks the line for the tail's wording; a band under every effect's floor for
            // "marked" (cut_speed::MIN_MARKED_SECONDS) is not a band at all, exactly as F3.3/F3.4 read it.
            let marked = selection(window)
                .filter(|band| band.length() >= cut_speed::MIN_MARKED_SECONDS)
                .map(|band| (band.start, band.end));
            let said = fx_svg::arm_words(&name, marked);
            log_line(&said);
            said
        }
    }
}

/// Ask for this file's preview raster and say what the cache answered. The CACHE and the argument vector are the
/// testable halves; running ffmpeg stays with `crate::subprocess`, so nothing is spawned here (and nothing could
/// run headless anyway). One logo placed six times therefore asks once.
fn request_svg_raster(path: &str) {
    // // effects.previewRasterPx — 512 px, rgba, cached per file (S5).
    let slot = SVG_RASTERS.with(|cell| cell.borrow_mut().request(path));
    if slot == fx_svg::Slot::NeedsRaster {
        // This is the call that WOULD run for a cold file, logged rather than executed:
        // fx_svg::ffmpeg_raster("ffmpeg", path, out) -> argv.
        let argv = fx_svg::ffmpeg_raster("ffmpeg", path, "/tmp/svg-raster.png");
        log_line(&format!(
            "preview raster queued: {} px for {} ({} args)",
            fx_svg::PREVIEW_RASTER_PX as i32,
            fx_svg::base(path),
            argv.len()
        ));
    }
}

/// The cache's answer for a file right now — Ready / Busy / Failed / NeedsRaster. `Rasters` has no read-only
/// lookup (asking IS how the module learns about a file), so this asks and reports what it was told, without
/// claiming a second copy of the cache's rule.
pub fn svg_raster_state(path: &str) -> fx_svg::Slot {
    SVG_RASTERS.with(|cell| cell.borrow_mut().request(path))
}

/// F3.5 S2/S3: the box came off the preview, now what? The seam BOTH the gesture AND the widget test drive,
/// shaped exactly like `text_drag_ended_with_source`: the source's pixel size is handed in because no ffprobe
/// runs headless, and nothing here composes a rule — every answer comes from `fx_svg`.
pub fn svg_drag_ended_with_source(
    window: &adw::ApplicationWindow,
    from: (f64, f64),
    to: (f64, f64),
    source: (f64, f64),
) -> String {
    let dx = to.0 - from.0;
    let dy = to.1 - from.1;
    // A press that did not travel is a CLICK wherever it landed, and for a drawing a click takes the MIDDLE
    // ([`fx_svg::MIDDLE`]), not text's lower third: a drawing is as often the subject as a decoration.
    let choice = if dx.abs() < fx_zoom::DRAG_MIN_PX || dy.abs() < fx_zoom::DRAG_MIN_PX {
        fx_text::BoxChoice::Click
    } else {
        fx_text::BoxChoice::Dragged {
            x: from.0.min(to.0),
            y: from.1.min(to.1),
            w: dx.abs(),
            h: dy.abs(),
        }
    };
    // The file must already be chosen — the arm carries it, and the record's `src` is that same string.
    let Some(file) = svg_chosen_file().filter(|f| !f.trim().is_empty()) else {
        log_line(fx_svg::NO_FILE);
        return fx_svg::NO_FILE.to_string();
    };
    let known = INSERT_PLACE_KNOWN.with(|cell| *cell.borrow()) == Some(true);
    let line = known.then(|| paste_line(window));
    let sel = selection(window)
        .filter(|band| band.length() >= cut_speed::MIN_MARKED_SECONDS)
        .map(|band| (band.start, band.end));
    match fx_svg::place(choice, &file, line, sel, source) {
        Some(fx) => {
            show_svg_form(window, &fx);
            wire_svg_buttons(window);
            let said = format!(
                "{} \u{2014} press \u{25b8} Apply puts the drawing on the picture",
                fx_svg::form_title(fx.t)
            );
            log_line(&said);
            said
        }
        // No seconds to place it at: no line and no selection. NO form is drawn -- a chooser with no moment to
        // happen in cannot be answered. `fx_svg::NO_LINE` is this flow's own sentence for it (NOT zoom's, which
        // says "the effect"; four buttons ask for the same missing thing in the words of what each one does).
        None => {
            log_line(fx_svg::NO_LINE);
            fx_svg::NO_LINE.to_string()
        }
    }
}

/// The app-side door: same seam, source size read off the pinned preview frame.
pub fn svg_drag_ended(
    window: &adw::ApplicationWindow,
    from: (f64, f64),
    to: (f64, f64),
) -> String {
    let source = preview_picture_px(window);
    svg_drag_ended_with_source(window, from, to, source)
}

/// This window's svg-form holder name. Only the Cut tab's instance is ever drawn into or read from —
/// `page_box` runs once per tab, so an unscoped name exists four times per window (the `fold-badges` /
/// `camera-rows` / `zoom-form` / `speed-form` / `text-form` rule).
fn cut_svg_form_name() -> String {
    format!("svg-form-{}", Page::Cut.label())
}

/// Resolve THIS window's svg-form holder through its own content, so a closed window's surviving tree cannot
/// answer for a live one.
fn svg_form_box_raw(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    let content = window.content()?;
    find_widget_by_name(&content, &cut_svg_form_name())?
        .downcast::<gtk::Box>()
        .ok()
}

/// Hide and empty the drawing form. Used by the disarm path too, so a form left open from an earlier press
/// cannot sit on screen after the entry that opened it has been released.
pub(crate) fn close_svg_form(window: &adw::ApplicationWindow) {
    set_svg_form(None);
    if let Some(holder) = svg_form_box_raw(window) {
        holder.set_visible(false);
    }
}

/// F3.5 S4: draw the form "SVG at m:ss" — the five fields in [`fx_svg::FORM_FIELDS`]'s order, with Choose…,
/// Apply and Cancel. Built into locals and appended LAST in one pass, as `show_text_form` does.
pub(crate) fn show_svg_form(window: &adw::ApplicationWindow, fx: &cut::Fx) {
    // Hidden FIRST, before anything is touched: while invisible its children cannot read as "already parented
    // here" to a later pass, and the clear below then removes exactly what this holder owns.
    let Some(holder) = svg_form_box_raw(window) else {
        return;
    };
    holder.set_visible(false);
    let stale: Vec<gtk::Widget> = holder
        .observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .collect();
    for old in stale {
        holder.remove(&old);
    }
    LAST_SVG_WINDOW.with(|cell| *cell.borrow_mut() = Some(window.clone()));

    let heading = gtk::Label::new(Some(&fx_svg::form_title(fx.t)));
    heading.set_xalign(0.0);
    heading.add_css_class("title-4");
    heading.set_widget_name("svg-heading");

    // File row: a PICKER, not a plain value. A drawing is the only effect whose form can change what it is made
    // of, so its first row shows the base name and offers Choose… beside it (§F3.4's "file + Choose…").
    let file_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let file_key = gtk::Label::new(Some(fx_svg::FORM_FIELDS[0]));
    file_key.set_xalign(0.0);
    let file_value = gtk::Label::new(Some(&fx_svg::base(&fx.src)));
    file_value.set_xalign(0.0);
    file_value.set_tooltip_text(Some(&fx.src));
    file_value.set_widget_name("svg-field-file");
    let choose = gtk::Button::with_label(fx_svg::CHOOSE_BUTTON);
    choose.set_widget_name("svg-choose-button");
    file_row.append(&file_key);
    file_row.append(&file_value);
    file_row.append(&choose);

    let length = zoom_field_row(
        fx_svg::FORM_FIELDS[1],
        "how long the drawing stays on screen",
        "svg-field-length",
        &trim_seconds(fx.dur),
    );
    let fade_in = zoom_field_row(
        fx_svg::FORM_FIELDS[2],
        fx_text::FADES_HELP,
        "svg-field-fade-in",
        &trim_seconds(fx.trans),
    );
    let fade_out = zoom_field_row(
        fx_svg::FORM_FIELDS[3],
        fx_text::FADES_HELP,
        "svg-field-fade-out",
        &trim_seconds(fx.tout),
    );

    let curve_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let curve_key = gtk::Label::new(Some(fx_svg::FORM_FIELDS[4]));
    curve_key.set_xalign(0.0);
    let curve_value = gtk::Label::new(Some(fx_zoom::CURVE_CHOICES[0]));
    curve_value.set_xalign(0.0);
    curve_value.set_tooltip_text(Some(fx_zoom::CURVE_HELP));
    curve_value.set_widget_name("svg-field-curve");
    curve_row.append(&curve_key);
    curve_row.append(&curve_value);

    // The preview raster row: how big the preview's copy of this drawing is rendered, and where that job stands.
    // // effects.previewRasterPx (512, transparent, cached per file).
    let raster = gtk::Label::new(Some(&svg_raster_note(&fx.src)));
    raster.set_xalign(0.0);
    raster.add_css_class("dim-label");
    raster.set_widget_name("svg-preview-raster");

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let apply = gtk::Button::with_label("Apply");
    apply.set_widget_name("svg-apply-button");
    let cancel = gtk::Button::with_label("Cancel");
    cancel.set_widget_name("svg-cancel-button");
    buttons.append(&apply);
    buttons.append(&cancel);

    let footer = gtk::Label::new(Some(
        "Kept as you type \u{2014} \u{21b6} Undo takes the whole edit back.",
    ));
    footer.set_xalign(0.0);
    footer.add_css_class("dim-label");
    footer.set_widget_name("svg-form-footer");

    holder.append(&heading);
    holder.append(&file_row);
    holder.append(&length);
    holder.append(&fade_in);
    holder.append(&fade_out);
    holder.append(&curve_row);
    holder.append(&raster);
    holder.append(&buttons);
    holder.append(&footer);
    holder.set_visible(true);
    // Stored only now that the widgets exist and belong to THIS window: `svg_form_open()` then always means
    // "there is a form on screen behind it", never "a value was parked somewhere".
    set_svg_form(Some(svg_form_of(fx)));
    // Buttons wired after the widgets are in the tree, so the lookup by name finds them.
    wire_svg_buttons(window);
}

/// The raster row's sentence: the size, and what the per-file cache says about this file right now.
fn svg_raster_note(path: &str) -> String {
    // // effects.previewRasterPx
    let state = match svg_raster_state(path) {
        fx_svg::Slot::Ready => "ready",
        fx_svg::Slot::Busy => "being made",
        fx_svg::Slot::Failed => "failed once, left alone",
        fx_svg::Slot::NeedsRaster => "not made yet",
    };
    format!(
        "preview raster {} px, {} \u{2014} one per file",
        fx_svg::PREVIEW_RASTER_PX as i32,
        state
    )
}

/// Snapshot the landed Fx into the form's shape, so Apply reads the same values the widgets show.
fn svg_form_of(fx: &cut::Fx) -> fx_svg::Form {
    fx_svg::Form {
        t: fx.t,
        dur: fx.dur,
        trans: fx.trans,
        tout: fx.tout,
        curve: fx_zoom::CURVE_CHOICES[0].to_string(),
        file: fx.src.clone(),
        on: fx_text::Box_ {
            cx: fx.cx.unwrap_or(fx_svg::MIDDLE.cx),
            cy: fx.cy.unwrap_or(fx_svg::MIDDLE.cy),
            wf: fx.wf.unwrap_or(fx_svg::MIDDLE.wf),
            hf: fx.hf.unwrap_or(fx_svg::MIDDLE.hf),
        },
    }
}

/// F3.5: whether ▨ SVG is armed right now — waiting for a click or a drag on the preview.
pub fn svg_armed() -> bool {
    SVG_ARMED.with(|cell| cell.get())
}

fn set_svg_armed(armed: bool) {
    SVG_ARMED.with(|cell| cell.set(armed));
}

/// Replace the carried drawing without touching the arm or the open form -- what Choose… ends up doing once the
/// chooser answers and the box has already been drawn.
pub fn swap_svg_file(window: &adw::ApplicationWindow, file: &str) {
    let _ = window;
    set_svg_file(Some(file.to_string()));
}

/// Drop the arm and the chosen file WITHOUT touching an open form. `press_svg_cancel` takes the form down too,
/// which is what Cancel should do but not what a caller tidying between checks wants; this clears only the two
/// pieces of state the arm owns.
pub fn release_svg_arm_only() {
    set_svg_armed(false);
    set_svg_file(None);
}

/// The drawing the chooser handed over, which the arm sentence and the record's `src` both use.
pub fn svg_chosen_file() -> Option<String> {
    SVG_FILE.with(|cell| cell.borrow().clone())
}

fn set_svg_file(file: Option<String>) {
    SVG_FILE.with(|cell| *cell.borrow_mut() = file);
}

/// F3.5 S4: the drawing form waiting on the page, before Apply. `None` is no form open, which is also what hides
/// the holder — one slot answers both questions so they cannot disagree.
pub fn svg_form_open() -> Option<fx_svg::Form> {
    SVG_FORM.with(|cell| cell.borrow().clone())
}

fn set_svg_form(form: Option<fx_svg::Form>) {
    SVG_FORM.with(|cell| *cell.borrow_mut() = form);
}

thread_local! {
    /// Whether ▨ SVG is armed, i.e. the next press on the preview draws a drawing box. Shaped exactly like
    /// F3.1's `ZOOM_ARMED` / F3.4's `TEXT_ARMED`: the preview's ONE drag gesture dispatches on this to decide
    /// whose flow a press is.
    static SVG_ARMED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };

    /// The chosen drawing. Carried apart from the form because the arm happens before the form exists: the arm
    /// sentence names the file, and the record's `src` is that same string.
    static SVG_FILE: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };

    /// The form a landed box left waiting on the page.
    static SVG_FORM: std::cell::RefCell<Option<fx_svg::Form>> =
        const { std::cell::RefCell::new(None) };

    /// The only strong handle on the window whose form is live, so a closed window's surviving tree cannot
    /// answer for a live one (see `zoom_form_box`'s comment on the `gtk_box_append` assertion).
    static LAST_SVG_WINDOW: std::cell::RefCell<Option<adw::ApplicationWindow>> =
        const { std::cell::RefCell::new(None) };

    /// Re-entry guard for re-greying the form's rows: GTK fires `toggled`/`notify` from its own
    /// `set_*` calls, and a refresh that writes a field would otherwise re-enter itself.
    static SVG_FORM_REFRESH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };

    /// The per-file preview raster cache, process-wide so one logo placed six times asks ffmpeg ONCE (S5). Not
    /// per-window: the raster belongs to the file, not to whoever opened it.
    // `RefCell::default()` rather than `new(Rasters::default())`: the latter is not a const fn (it builds a
    // HashMap), and thread_local statics need a const initialiser. Same empty cache either way.
    static SVG_RASTERS: std::cell::RefCell<fx_svg::Rasters> = std::cell::RefCell::default();
}

/// Does THIS window own the live drawing form? Two windows both holding one would put Apply's write into
/// whichever tree the reader reached first.
fn svg_form_owner(window: &adw::ApplicationWindow) -> bool {
    LAST_SVG_WINDOW.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|w: &adw::ApplicationWindow| w.as_ptr() == window.as_ptr())
            .unwrap_or(false)
    })
}

/// Read a named Entry out of THIS window's tree, through the window that owns the form.
fn svg_entry(name: &str) -> Option<gtk::Entry> {
    LAST_SVG_WINDOW.with(|cell| {
        cell.borrow()
            .as_ref()
            .and_then(|w| find_widget_by_name(w.upcast_ref(), name))
            .and_then(|w| w.downcast::<gtk::Entry>().ok())
    })
}

/// F3.5 S4: Apply. Reads the fields back into an [`fx_svg::Form`] and hands them to [`fx_svg::apply`], which
/// decides whether there is a file at all and shares the fades. A refusal keeps the form OPEN.
///
/// That matters most for the no-file case: `fx_svg::NO_FILE` says "choose a drawing and it goes on the picture".
/// Closing the form on that refusal would throw away the box that was just drawn and the Length that was just
/// typed, both of which still stand.
pub fn press_svg_apply(window: &adw::ApplicationWindow) -> String {
    if !svg_form_owner(window) {
        return "no drawing form on this page \u{2014} pick a drawing first".to_string();
    }
    let Some(stored) = svg_form_open() else {
        return "no drawing to apply \u{2014} press \u{25a8} SVG first".to_string();
    };
    // A non-parsing entry keeps what the form already held rather than becoming zero, so a half-typed number
    // cannot silently place a 0-second drawing -- the same rule `press_text_apply` holds to.
    let read = |name: &str, keep: f64| -> f64 {
        svg_entry(name)
            .map(|entry| entry.text().trim().to_string())
            .and_then(|text| text.parse::<f64>().ok())
            .unwrap_or(keep)
    };
    // The file row is a Label + Choose…, not an Entry: it changes by choosing, never by typing. So the stored
    // file stands unless a Choose… since the box landed replaced it.
    let form = fx_svg::Form {
        t: stored.t,
        dur: read("svg-field-length", stored.dur),
        trans: read("svg-field-fade-in", stored.trans),
        tout: read("svg-field-fade-out", stored.tout),
        curve: stored.curve.clone(),
        file: svg_chosen_file().unwrap_or(stored.file.clone()),
        on: stored.on,
    };
    let fx = match fx_svg::apply(&form) {
        Ok(fx) => fx,
        Err(reason) => {
            // Verbatim, form still open. No record, no history write: nothing happened yet.
            log_line(&reason);
            live_form_refused();
            return reason;
        }
    };
    let mut cut_ = review_cut_of(window);
    cut_.fx.push(fx.clone());
    save_insert_cut(&cut_);
    // `record_edit`, NOT `seed_review_cut`: the addition must sit on the history so F2.13's ↶ takes it back.
    live_form_answer_is_undo(window);
    record_edit(window, &cut_);
    refresh_effects_lane(window);
    close_svg_form(window);
    set_svg_armed(false);
    // The sentence is `fx_record`'s own ("SVG recorded — ↶ Undo takes it back"), reused rather than composed
    // here: a hand-placed drawing and a dropdown-recorded one are the same news about the same kind.
    let said = fx_record::recorded_status(cut::EffectKind::Svg);
    log_line(&said);
    said
}

/// F3.5 S4 (`+ Choose…`): swap the drawing WITHOUT losing the box or the seconds. Same door the chooser answers
/// through, so "a file was picked" is one code path whether it came from the row or from the form.
pub fn press_svg_choose(window: &adw::ApplicationWindow) -> String {
    let root = std::env::current_dir().unwrap_or_default();
    let dir = fx_svg::chooser_dir(&root);
    open_svg_chooser(window, fx_svg::CHOOSE_TITLE, &dir, fx_svg::CHOOSE_FILTER);
    let said = format!(
        "{} \u{2014} the new drawing lands in the box already drawn",
        fx_svg::CHOOSE_TITLE
    );
    log_line(&said);
    said
}

/// F3.5 S4: Cancel drops the form and the arm and changes nothing on the cut — no record pushed, no history
/// written, so ↶ still points where it did before ▨ SVG was pressed.
pub fn press_svg_cancel(window: &adw::ApplicationWindow) -> String {
    close_svg_form(window);
    set_svg_armed(false);
    "left as it was \u{2014} no drawing placed".to_string()
}

/// F3.5: Esc releases the arm or drops an open drawing form, and claims no other key. Returns `None` when this
/// window holds neither, so the key travels on to whatever else is listening.
pub fn press_svg_esc(window: &adw::ApplicationWindow) -> Option<String> {
    if !svg_form_owner(window) || (svg_form_open().is_none() && !svg_armed()) {
        return None;
    }
    Some(press_svg_cancel(window))
}

/// Wire the drawing form's buttons BY NAME. Each forwards one press and prints what comes back; none holds a
/// rule — same shape as `wire_text_buttons`.
fn wire_svg_buttons(window: &adw::ApplicationWindow) {
    for (name, door) in [
        ("svg-apply-button", 0),
        ("svg-choose-button", 1),
        ("svg-cancel-button", 2),
    ] {
        if let Some(button) = line_step_button(window, name) {
            let win = window.clone();
            button.connect_clicked(move |_| {
                let said = match door {
                    0 => press_svg_apply(&win),
                    1 => press_svg_choose(&win),
                    _ => press_svg_cancel(&win),
                };
                if let Some(status_line) = find_status(win.upcast_ref()) {
                    status_line.set_text(&said);
                }
            });
        }
    }
}

/// Esc releases the drawing arm / form, wired after `set_content` like every other control. Claims Escape only
/// when this window owns something to release; otherwise the key travels on.
pub(crate) fn wire_svg_esc(window: &adw::ApplicationWindow) {
    let controller = gtk::EventControllerKey::new();
    let win = window.clone();
    controller.connect_key_pressed(move |_ctrl, key, _code, _mods| {
        if key != gtk::gdk::Key::Escape {
            return glib::Propagation::Proceed;
        }
        match press_svg_esc(&win) {
            Some(said) => {
                if let Some(status_line) = find_status(win.upcast_ref()) {
                    status_line.set_text(&said);
                }
                glib::Propagation::Stop
            }
            None => glib::Propagation::Proceed,
        }
    });
    window.add_controller(controller);
}
