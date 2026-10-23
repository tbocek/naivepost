//! F2.12 Insert — the form's half: its holder, what is open, the drawing of the plan and Apply.
//! Split out of `window.rs`, which is past its size budget; its doors are re-exported through `window`.
//!
//! The decisions are `cut_insert`'s; what is here turns a [`cut_insert::FormPlan`] into widgets with
//! stable names, remembers where the insert was asked for, and hands the spliced cut to the one writer.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut::Cut;
use crate::cut_cards;
use crate::cut_insert;
use crate::cut_select;
use crate::startup;
use crate::ui::cut_form_frame;
use crate::ui::window as ui;

// --- F2.12: insert a card, still, video or sound ---------------------------------------------------------

/// The insert form's holder, wherever this window put it. `None` on a page that drew none (Prepare, Narrate,
/// Produce) — every insert control lives inside this box, so one lookup finds the whole form and nothing
/// outside it.
pub(crate) fn insert_form_box(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    ui::find_widget_by_name(window.upcast_ref(), "insert-form")?
        .downcast::<gtk::Box>()
        .ok()
}

/// The holder that is DRAWING a form right now, as opposed to the empty ones the other tabs carry (see
/// `page_box`: an unscoped name exists once per tab, and a tree-wide lookup can land on a hidden one). A
/// snapshot of the form asks this rather than the name, so the shot cannot be of an empty box.
pub fn visible_insert_form(window: &adw::ApplicationWindow) -> Option<gtk::Widget> {
    fn walk(root: &gtk::Widget) -> Option<gtk::Widget> {
        if root.widget_name() == "insert-form"
            && root.is_visible()
            && root.observe_children().n_items() > 0
        {
            return Some(root.clone());
        }
        for child in root.observe_children().iter::<glib::Object>() {
            let Ok(child) = child else { continue };
            let Ok(widget) = child.downcast::<gtk::Widget>() else {
                continue;
            };
            if let Some(found) = walk(&widget) {
                return Some(found);
            }
        }
        None
    }
    walk(window.upcast_ref())
}

/// What the open form stands for, so Apply can place the file the chooser returned at the second the page
/// read when Insert was pressed. Held per window because two windows may each have an insert half-finished;
/// the newest one is the one whose buttons were just clicked.
#[derive(Debug, Clone)]
pub(crate) struct InsertOpen {
    pub(crate) path: String,
    pub(crate) at: f64,
    pub(crate) seconds: f64,
    /// The card's declared fields, so Apply knows which `insert-field-*` entries this form drew and can read
    /// them back. Empty for a still, a video or a sound, which is why nothing else changed.
    pub(crate) fields: Vec<cut_insert::Field>,
}

thread_local! {
    static INSERT_OPEN: std::cell::RefCell<Option<InsertOpen>> =
        const { std::cell::RefCell::new(None) };
}

/// The form the page is showing, if any — what a widget test reads to assert the same state the logic test
/// checked ([`cut_insert::FormPlan`] plus where the insert will land).
pub fn insert_open(window: &adw::ApplicationWindow) -> Option<(String, f64, f64)> {
    let _ = window;
    INSERT_OPEN.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|open| (open.path.clone(), open.at, open.seconds))
    })
}

/// F2.12 S4: draw the form from a [`cut_insert::FormPlan`]. Every label, every conditional control and the
/// opening length come out of the plan; this only turns them into widgets with stable names.
///
/// The three radios are a group by construction: GTK lets one of several `RadioButton`s share a group, and
/// the LANE radio is only ever created when the plan says video, so a non-video file cannot be put on a lane
/// even by a stray click. The sound tick is drawn only when the plan says there is a sound to answer for, and
/// greyed while LANE is chosen — a lane cuts nothing, so there is no sound underneath to keep or drop yet.
pub(crate) fn show_insert_form(
    window: &adw::ApplicationWindow,
    plan: &cut_insert::FormPlan,
    path: &str,
    at: f64,
    fields: Vec<cut_insert::Field>,
) {
    let Some(holder) = insert_form_box(window) else {
        return;
    };
    // Clear the previous form out of THE HOLDER, never by a tree-wide name search (see the fold badges for
    // why: a closed window's widgets stay parented somewhere).
    let stale: Vec<gtk::Widget> = holder
        .observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .collect();
    for old in stale {
        holder.remove(&old);
    }

    let heading = gtk::Label::new(Some(&format!(
        "{} \u{2014} {}",
        cut_insert::edit_verb(cut_insert::hold(ui::held_segment().as_ref())),
        cut_insert::base_name(path)
    )));
    heading.set_widget_name("insert-heading");
    heading.set_xalign(0.0);
    holder.append(&heading);

    // One entry per declared card field, in the order the card asked (S4). A logo field gets its Logo\u{2026}
    // picker beside the entry rather than instead of it: the typed value survives a re-open either way.
    for field in &fields {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let key = gtk::Label::new(Some(&field.label));
        key.set_widget_name(&format!("insert-label-{}", field.key));
        key.set_xalign(0.0);
        row.append(&key);
        let entry = gtk::Entry::new();
        entry.set_widget_name(&format!("insert-field-{}", field.key));
        entry.set_text(&field.value);
        entry.set_hexpand(true);
        if !field.hint.is_empty() {
            entry.set_tooltip_text(Some(&field.hint));
        }
        row.append(&entry);
        if field.logo {
            let picker = gtk::Button::with_label("Logo\u{2026}");
            picker.set_widget_name(&format!("insert-logo-{}", field.key));
            picker.set_tooltip_text(Some("pick a mark to sit beside the title"));
            row.append(&picker);
        }
        holder.append(&row);
    }

    // The three modes (§G's labels verbatim), BETWEEN first as the default for no selection. GTK 4 has no
    // `RadioButton` type: a toggle in a group is a `CheckButton` drawn as a radio and grouped by
    // `set_group`, which is what makes "exactly one mode" true rather than a convention.
    let between = gtk::CheckButton::with_label(cut_insert::BETWEEN_LABEL);
    between.set_widget_name("insert-mode-between");
    let over = gtk::CheckButton::with_label(cut_insert::OVER_LABEL);
    over.set_widget_name("insert-mode-over");
    over.set_group(Some(&between));
    holder.append(&between);
    holder.append(&over);
    // The LANE radio is drawn for video only; `lane_offered` answers that, so a still or a sound cannot be
    // put on a row of its own even by a stray click.
    if plan.lane_shown {
        let lane = gtk::CheckButton::with_label(cut_insert::LANE_LABEL);
        lane.set_widget_name("insert-mode-lane");
        lane.set_group(Some(&between));
        holder.append(&lane);
    }
    match plan.mode {
        cut_insert::Mode::Between => between.set_active(true),
        cut_insert::Mode::Over => over.set_active(true),
        cut_insert::Mode::Lane => {
            if let Some(lane) = ui::find_widget_by_name(window.upcast_ref(), "insert-mode-lane")
                .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
            {
                lane.set_active(true);
            } else {
                between.set_active(true);
            }
        }
    }

    // The sound tick: shown only when there is a sound to answer for, greyed while LANE is chosen.
    if plan.sound_shown {
        let tick = gtk::CheckButton::with_label(plan.sound_label);
        tick.set_widget_name("insert-sound-tick");
        tick.set_sensitive(!matches!(plan.mode, cut_insert::Mode::Lane));
        holder.append(&tick);
    }

    // Seconds, opening at the plan's length (S3).
    let sec_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let sec_key = gtk::Label::new(Some("Seconds"));
    sec_key.set_xalign(0.0);
    sec_row.append(&sec_key);
    let seconds = gtk::Entry::new();
    seconds.set_widget_name("insert-seconds");
    seconds.set_width_chars(6);
    seconds.set_text(&format!("{:.1}", plan.seconds));
    seconds.set_tooltip_text(Some(&format!(
        "at least {:.1} s \u{2014} shorter keeps the length it opened with",
        cut_select::MIN_SCENE_SECONDS
    )));
    sec_row.append(&seconds);
    holder.append(&sec_row);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let apply = gtk::Button::with_label(if cut_insert::hold(ui::held_segment().as_ref()) {
        "Apply"
    } else {
        "Insert it"
    });
    apply.set_widget_name("insert-apply-button");
    let cancel = gtk::Button::with_label("Cancel");
    cancel.set_widget_name("insert-cancel-button");
    buttons.append(&apply);
    buttons.append(&cancel);
    // §05#1's PINNED footer, not the end of the scrolling body: a footer inside `cut-form-scroller` fell
    // below the painted frame and the step that places the insert was never on screen.
    cut_form_frame::set_footer(window, Some(&buttons));
    // §S4: the form replaces the idle readings while it is open.
    cut_form_frame::set_readings(window, false);

    INSERT_OPEN.with(|cell| {
        *cell.borrow_mut() = Some(InsertOpen {
            path: path.to_string(),
            at,
            seconds: plan.seconds,
            fields,
        })
    });
    holder.set_visible(true);

    let win = window.clone();
    apply.connect_clicked(move |_| {
        let status = press_insert_apply(&win);
        if let Some(status_line) = ui::find_status(win.upcast_ref()) {
            status_line.set_text(&status);
        }
        ui::refresh_copy_buttons(&win);
    });
    let win = window.clone();
    cancel.connect_clicked(move |_| {
        let status = close_insert_form(&win);
        if let Some(status_line) = ui::find_status(win.upcast_ref()) {
            status_line.set_text(&status);
        }
    });
    // Switching the mode re-draws the tick's sentence, which is the only thing the change touches on the
    // form itself (what it does to the footage is decided at Apply, by `cut_insert::place`).
    for radio in [&between, &over] {
        let win = window.clone();
        radio.connect_toggled(move |button| {
            if button.is_active() {
                refresh_insert_tick(&win);
            }
        });
    }
    if let Some(lane) = ui::find_widget_by_name(window.upcast_ref(), "insert-mode-lane")
        .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
    {
        let win = window.clone();
        lane.connect_toggled(move |button| {
            if button.is_active() {
                refresh_insert_tick(&win);
            }
        });
    }
}

/// Read the mode the radios hold right now. No lane widget means no lane was offered, so the answer is never
/// `Lane` for a still, a card or a sound.
pub(crate) fn insert_mode_now(window: &adw::ApplicationWindow) -> cut_insert::Mode {
    let radio = |name: &str| {
        ui::find_widget_by_name(window.upcast_ref(), name)
            .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
    };
    if radio("insert-mode-lane").is_some_and(|b| b.is_active()) {
        cut_insert::Mode::Lane
    } else if radio("insert-mode-over").is_some_and(|b| b.is_active()) {
        cut_insert::Mode::Over
    } else {
        cut_insert::Mode::Between
    }
}

/// Re-label and re-enable the sound tick after a mode change: the sentence comes from
/// [`cut_insert::tick_label`], the greying from the fact that a LANE has nothing under it.
pub(crate) fn refresh_insert_tick(window: &adw::ApplicationWindow) {
    let Some(tick) = ui::find_widget_by_name(window.upcast_ref(), "insert-sound-tick")
        .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
    else {
        return;
    };
    let mode = insert_mode_now(window);
    tick.set_label(Some(cut_insert::tick_label(
        mode == cut_insert::Mode::Between,
    )));
    tick.set_sensitive(!matches!(mode, cut_insert::Mode::Lane));
}

/// Hide the form and forget what it stood for. Returns the line the page prints.
pub fn close_insert_form(window: &adw::ApplicationWindow) -> String {
    INSERT_OPEN.with(|cell| *cell.borrow_mut() = None);
    if let Some(holder) = insert_form_box(window) {
        holder.set_visible(false);
    }
    // The pinned parts go back the way they came: no buttons in the footer, the idle readings visible again.
    cut_form_frame::set_footer(window, None);
    cut_form_frame::set_readings(window, true);
    "the insert was cancelled".to_string()
}

/// §8's arrival sentence for the answers typed into the form, said now rather than left to the render: `filled in`
/// only when a hole actually took a value. A stamped card is redrawn by its generator whatever was typed and
/// already says so at the render; an unreadable file or a non-SVG says nothing here — the producer owns that note.
fn card_arrival(path: &str, answers: &[(String, String)]) -> Option<String> {
    if answers.is_empty() || !path.to_lowercase().ends_with(".svg") {
        return None;
    }
    let doc =
        std::fs::read_to_string(std::path::Path::new(&cut_cards::split_path(path).file)).ok()?;
    (!cut_insert::answered_holes(&doc, answers).is_empty())
        .then_some(cut_cards::FILLED_IN.to_string())
}

/// F2.12 S5: Apply / Insert it. Reads the form's own widgets, hands the answers to
/// [`cut_insert::FormAnswer::from_form`] and [`cut_insert::place`], saves the cut through the same door the
/// folds use, and returns [`cut_insert::Placed::status`] (or the refusal). No rule lives here.
pub fn press_insert_apply(window: &adw::ApplicationWindow) -> String {
    let Some(open) = INSERT_OPEN.with(|cell| cell.borrow().clone()) else {
        return cut_insert::NO_LINE_YET.to_string();
    };
    let fallback = open.seconds;
    let text = ui::find_widget_by_name(window.upcast_ref(), "insert-seconds")
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
        .map(|entry| entry.text().to_string())
        .unwrap_or_default();
    let silent = ui::find_widget_by_name(window.upcast_ref(), "insert-sound-tick")
        .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
        .is_some_and(|tick| tick.is_active());
    let answer =
        cut_insert::FormAnswer::from_form(insert_mode_now(window), &text, fallback, silent);
    // §G: what was typed into the card's own fields goes ONTO THE PATH, because that is where the render reads
    // its arguments from. Reading them off the widgets the same form built (the names it wrote) keeps the widget
    // layer to reading; deciding what an answer means is `cut_insert::answer_path`'s.
    let answers: Vec<(String, String)> = open
        .fields
        .iter()
        .map(|field| {
            let typed = ui::find_widget_by_name(
                window.upcast_ref(),
                &format!("insert-field-{}", field.key),
            )
            .and_then(|w| w.downcast::<gtk::Entry>().ok())
            .map(|entry| entry.text().to_string())
            .unwrap_or_default();
            (field.key.clone(), typed)
        })
        .collect();
    let said = card_arrival(&open.path, &answers);
    let mut cut_ = ui::newest_review_cut();
    match cut_insert::place(
        &mut cut_,
        &cut_insert::answer_path(&open.path, &answers),
        open.at,
        answer.mode,
        answer.seconds,
        answer.silent,
    ) {
        Ok(placed) => {
            ui::record_edit(window, &cut_);
            save_insert_cut(&cut_);
            INSERT_OPEN.with(|cell| *cell.borrow_mut() = None);
            if let Some(holder) = insert_form_box(window) {
                holder.set_visible(false);
            }
            cut_form_frame::set_footer(window, None);
            cut_form_frame::set_readings(window, true);
            match said {
                // The card's own note first: what the parameters did, then where the card landed.
                Some(note) => format!("{note} \u{2014} {}", placed.status),
                None => placed.status,
            }
        }
        Err(refusal) => refusal,
    }
}

/// Write the mutated cut where it was read from. Same path as `save_folds`: `cut::save` writes the whole cut,
/// and the segments an insert added are fields of it. A project with no tree keeps the insert in memory only.
pub(crate) fn save_insert_cut(cut_: &Cut) {
    // §7's one writer: the session resolution and the call to `cut::save` are `cut_save`'s, not this door's.
    let _ = crate::cut_save::write_cut(cut_);
}

/// A landed cut is written by the flow that landed it — `cut_suggest_words_run` for the words cut,
/// its model twin for a model cut — because only that flow knows which session folder it read.
/// Write `cut_` to this window's own cut.json — the door every editing press uses, exposed so a check can
/// start from a file on disk rather than only from an in-memory cut.
pub fn save_seed_for_test(window: &adw::ApplicationWindow, cut_: &Cut) {
    let _ = window;
    save_insert_cut(cut_);
}

/// F2.12 S1 + S2: ⧉ Insert. The placement question is answered FIRST, before any dialog opens, because a
/// chooser the user cancels should not have cost them a refusal they did not ask for — and because the file
/// that comes back has to land where the timeline read at the press.
///
/// With a line or a selection this seeds `assets/` with the built-in cards (so the folder never opens empty),
/// then opens the file chooser titled for the scope of the seconds. Headless there is nobody to answer the
/// chooser, so nothing more happens here; the seam a widget test drives past the chooser is
/// [`insert_chosen`], which is what the chooser's callback calls.
pub fn press_insert(window: &adw::ApplicationWindow) -> String {
    // S6 first: a held card is edited, not replaced. A card that went away while its dialog was open says so
    // rather than opening a form for something that is no longer there.
    if let Some(held) = ui::held_segment() {
        let cut_ = ui::newest_review_cut();
        if held.is_insert() && cut_insert::find_card(&cut_, &held.ins, held.s).is_none() {
            return cut_insert::GONE.to_string();
        }
    }
    // S1: the placement question is answered BEFORE any chooser opens — a cancelled dialog should not have
    // cost the user a refusal they did not ask for, and the file that comes back has to land where the
    // timeline read at the press.
    let band = ui::selection(window);
    let placed = place_known().then(|| ui::line_position(window).t);
    let at = match cut_insert::needs_place(placed, band.as_ref()) {
        Ok(at) => at,
        Err(refusal) => return refusal.to_string(),
    };
    let root = std::env::current_dir().unwrap_or_default();
    let dir = startup::session_dir(&root);
    let assets = cut_insert::chooser_dir(&dir);
    cut_insert::seed_cards(&assets, &cut_cards::seeds());
    let title = cut_insert::chooser_title(band.as_ref());
    let sound_only = matches!(
        band.as_ref().map(|b| &b.scope),
        Some(cut_select::Scope::Sound { .. })
    );
    open_insert_chooser(window, title, &assets, sound_only, at);
    format!("{} \u{2014} choose the file", title)
}

/// Open the native file chooser over `dir`. Kept apart from [`press_insert`] so the decision (where, which
/// title, which filter) is testable without a display: a headless run simply never answers the dialog.
#[allow(deprecated)] // GTK 4.10 deprecates the whole GtkFileChooser API; its replacement is a rewrite
                     // of this chooser, so the deprecated calls stay and are named here rather than silently
pub(crate) fn open_insert_chooser(
    window: &adw::ApplicationWindow,
    title: &str,
    dir: &std::path::Path,
    sound_only: bool,
    at: f64,
) {
    let chooser = gtk::FileChooserNative::builder()
        .title(title)
        .modal(true)
        .action(gtk::FileChooserAction::Open)
        .build();
    let _ = chooser.set_current_folder(Some(&gio::File::for_path(dir)));
    let filter = gtk::FileFilter::new();
    if sound_only {
        filter.set_name(Some("sounds"));
        for ext in ["mp3", "wav", "ogg", "oga", "flac", "m4a", "aac", "opus"] {
            filter.add_pattern(&format!("*.{}", ext));
        }
    } else {
        filter.set_name(Some("clips, images, animations and sounds"));
        for ext in [
            "mp4", "mkv", "mov", "webm", "avi", "svg", "svgz", "png", "jpg", "jpeg", "mp3", "wav",
            "ogg", "flac", "m4a",
        ] {
            filter.add_pattern(&format!("*.{}", ext));
        }
    }
    chooser.add_filter(&filter);
    let win = window.clone();
    chooser.connect_response(move |chooser, response| {
        if response != gtk::ResponseType::Accept {
            return;
        }
        let Some(file) = chooser.file().and_then(|f| f.path()) else {
            return;
        };
        // The same body a widget test drives, so the chooser and the seam cannot drift apart.
        insert_chosen(&win, &file, at);
    });
    chooser.show();
}

/// F2.12 S3 + S4: the file came back from the chooser — build the plan and draw the form.
///
/// `file_seconds` is NOT probed here: reading a video's duration means an ffprobe call, and the container
/// this runs in has neither ffmpeg nor a reason to spawn one per click. The plan therefore falls back to
/// [`cut_insert::DEFAULT_SECONDS`] (P.policy.insertDefaultSeconds = 4) unless the caller passes a length it
/// already knows, which is exactly what a still and an un-probed file get anyway.
pub fn insert_chosen(window: &adw::ApplicationWindow, file: &std::path::Path, at: f64) -> String {
    insert_chosen_with_length(window, file, at, None)
}

/// [`insert_chosen`] with the length the caller already has (a probe, or a card's own animation). Split out
/// so a test can hand in a length and see S3's precedence — selection seconds beat the file's, and both beat
/// the default — without a media probe.
pub fn insert_chosen_with_length(
    window: &adw::ApplicationWindow,
    file: &std::path::Path,
    at: f64,
    file_seconds: Option<f64>,
) -> String {
    let raw = file.to_string_lossy().to_string();
    let band = ui::selection(window);
    let animated = svg_is_animated(file);
    // S3: the file's own length comes from the real probe (ffprobe), which is also what S7's render is asked
    // against. `None` — a still, an unreadable file, a machine with no ffprobe — falls back to
    // `P.policy.insertDefaultSeconds`, which is the whole point of the fallback rather than a failure.
    let probed = crate::ui::cut_card_preview::probe_card_seconds(window, &raw);
    // S4's first tick question — "does the FILE bring a sound of its own?" — is a fact about the file, so it is
    // measured with the same probe §4's track menu measures its stream count with. A missing tool or an
    // unparsable answer reads as no sound, which is what `audio_streams` already decides for a row's menu.
    let file_has_sound = file_has_sound(&raw);
    // §S4's tick is a question about SOUND, and an audio file IS all sound: `sound_open` reads the two flags as
    // "does anything have to be answered for", so a bare WAV must arrive with its own flag set or the door
    // answers nothing (which is what `Kind::Audio` alone cannot say on its own).
    let carries_sound = file_has_sound || cut_insert::kind(&raw) == cut_insert::Kind::Audio;
    // S3's order: a selection's seconds beat the file's, and the file's beat the default. For a card "the file's
    // own length" is its last moving moment (§5), which no probe can answer — ffprobe reads an SVG as one frame.
    // `offer_seconds` is that door; it reads the document only when there is nothing else to ask.
    let file_seconds = file_seconds
        .or(probed)
        .or_else(|| crate::produce_cards::offer_seconds(&raw));
    let plan = cut_insert::form_plan(
        &raw,
        band.as_ref(),
        file_seconds,
        animated,
        false,
        carries_sound,
        false,
    );
    // The card's holes are read off the document; a non-SVG gets no fields and the door ignores the argument.
    let mut fields: Vec<cut_insert::Field> = Vec::new();
    if plan.kind == cut_insert::Kind::Svg {
        if let Ok(doc) = std::fs::read_to_string(file) {
            fields = cut_insert::card_fields(doc.as_bytes());
            for field in fields.iter_mut() {
                if let Some(value) = plan.values.iter().find_map(|(key, value)| {
                    key.eq_ignore_ascii_case(&field.key)
                        .then_some(value.clone())
                }) {
                    field.value = value;
                }
            }
        }
    }
    show_insert_form(window, &plan, &raw, at, fields);
    cut_form_frame::set_pinned(window, cut_form_frame::open_form_title(window).as_deref());
    format!(
        "{} \u{2014} {:.1} s, {} \u{2014} the form is open",
        cut_insert::base_name(&raw),
        plan.seconds,
        mode_word(plan.mode)
    )
}

/// Whether the file brings a sound of its own (§S4's tick question). ffprobe through the project's own leg,
/// so the command reaches the log like every other tool call; any failure reads as "no sound", which is the
/// same reading §4 gives a source row whose probe fails.
fn file_has_sound(path: &str) -> bool {
    let ffprobe = crate::services::ffprobe_for("", crate::ui::cut_card_preview::FFPROBE);
    let asked = [
        "-v",
        "error",
        "-select_streams",
        "a",
        "-show_entries",
        "stream=index",
        "-of",
        "csv=p=0",
        path,
    ];
    let answer = crate::source_probe::run_ffprobe(&ffprobe, &asked).unwrap_or_default();
    // One line per audio stream (`csv=p=0`), so a non-empty answer is the file's own sound. Parsed here rather
    // than through `source_probe::audio_streams`, which asks for JSON and fails on an older ffprobe that prints
    // no `streams` object at all — a silent failure would hide §S4's tick from every machine but this one.
    !answer.trim().is_empty()
}

/// Whether an SVG animates, read off the document: a `@keyframes` block or an `<animate>` tag. An SVG with
/// neither is drawn as a still, so it has no length of its own (see [`cut_insert::default_length`]).
pub(crate) fn svg_is_animated(file: &std::path::Path) -> bool {
    crate::ui::cut_card_preview::svg_is_animated(file)
}

/// The mode said back as one word, for the status line.
pub(crate) fn mode_word(mode: cut_insert::Mode) -> &'static str {
    match mode {
        cut_insert::Mode::Between => "spliced between the footage",
        cut_insert::Mode::Over => "over the footage",
        cut_insert::Mode::Lane => "on a lane of its own",
    }
}

/// F2.12 S6: set Insert's sensitivity and its word. Runs from the same refresh path as the verb and copy
/// buttons, so a greyed Insert is always today's answer: with no line and no selection there is nowhere for
/// an insert to go, and while a card is held the button edits that card rather than choosing a new file.
pub fn refresh_insert_button(window: &adw::ApplicationWindow) {
    if let Some(button) = ui::line_step_button(window, "insert-button") {
        // A target means a SECOND someone chose: a placed red line, or a band whose length clears the floor
        // every other verb holds a scene to. `line_position` answers 0.0 both for "the line is on frame
        // zero" and for "no line was ever placed", so the page keeps its own answer beside it rather than
        // reading a false yes out of the default.
        let has_target = place_known() || ui::selection(window).is_some();
        let live = match ui::selection(window) {
            Some(band) => band.length() >= cut_select::MIN_SCENE_SECONDS,
            None => has_target,
        };
        button.set_sensitive(live);
        // F3.8 S1: `Insert becomes ✎ Edit` when an EFFECT is held too, not only a card. §A.9 says it in the
        // same breath as "band → hold", so the held-effect case joins the held-card one here rather than being
        // a second rule somewhere else. The word still comes from `cut_insert::edit_verb`, never retyped.
        let editing = cut_insert::hold(ui::held_segment().as_ref()) || ui::held_effect().is_some();
        button.set_label(cut_insert::edit_verb(editing));
    }
}

/// Whether this session has been told where its red line stands. Read back by
/// [`refresh_insert_button`] and by [`press_insert`]: `false` is a page that has never had a line placed,
/// which is what makes S1's refusal reachable.
pub fn place_known() -> bool {
    ui::INSERT_PLACE_KNOWN.with(|cell| cell.borrow().unwrap_or(false))
}

/// S1: ⧉ Insert's wire. The button forwards here and prints what comes back, so no rule sits in the callback.
///
/// F3.8 S1 (`Insert becomes ✎ Edit`): the label switch and this door are the two halves of that one sentence.
/// `refresh_insert_button` already relabels the button to `cut_insert::edit_verb(true)` whenever an EFFECT is in
/// hand (as well as a held card), so pressing a button that says "✎ Edit" must not open the insert chooser — it
/// re-opens the record in hand's form (§S4). Checked before `press_insert` because §F2.12 S1 answers the placement
/// question first thing inside it, and an Edit press has no placement question to answer.
pub(crate) fn press_insert_from_button(window: &adw::ApplicationWindow) -> String {
    if ui::held_effect().is_some() {
        return ui::press_edit_effect(window);
    }
    press_insert(window)
}
