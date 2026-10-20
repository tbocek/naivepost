//! F3.6 🔊 Volume by hand — the page half of the flow: the press, the form drawer, the Apply.
//!
//! Split out of `window.rs` unchanged: the rules live in [`crate::fx_volume`], and what is here is only
//! the state a press leaves behind and the widgets that show it — the percent entry, the fade dropdowns
//! and the two buttons. No decision of its own. A volume has no box and no gesture, so unlike Text and
//! SVG there is nothing to arm: the press goes straight to the form, the way Speed's does.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut;
use crate::fx_volume;
use crate::fx_zoom;
use crate::shell::Page;
use crate::ui::window::{
    find_status, find_widget_by_name, INSERT_PLACE_KNOWN, line_step_button,
    live_form_answer_is_undo, live_form_refused, log_line, paste_line, record_edit,
    refresh_effects_lane, review_cut_of, save_insert_cut, selection, trim_seconds, zoom_field_row,
};

// --- F3.6 Volume by hand: the press, the form, the Apply -------------------------------------------------
//
// The rule half lives in `src/fx_volume.rs` (`press`/`Pressed`, `NO_SECONDS`, `initial`/`initial_at_line`,
// `DEFAULT_GAIN`/`FADE_SECONDS`/`LINE_SECONDS`/`MIN_SECONDS`/`max_percent`, `form_title`, `FORM_FIELDS`,
// the three help sentences, `Form`, `percent_of`/`gain_of_percent`, `apply`, `label`/`verb`/`percent_label`/
// `placed_status`, `heard_while_paused`). What is here is only the state a press leaves behind and the widgets
// that show it — no decision of its own. A volume has no box and no gesture, so unlike Text and SVG there is
// nothing to arm: the press goes straight to the form, the way Speed's does.

/// This window's volume-form holder name. Only the Cut tab's instance is ever drawn into or read from —
/// `page_box` runs once per tab, so an unscoped name exists four times per window and every lookup lands on
/// whichever box the walk reaches first (the `fold-badges` / `camera-rows` / `zoom-form` / `speed-form` rule).
fn cut_volume_form_name() -> String {
    format!("volume-form-{}", Page::Cut.label())
}

/// Resolve THIS window's volume-form holder through its own content, so a closed window's surviving tree cannot
/// answer for a live one.
fn volume_form_box_raw(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    let content = window.content()?;
    find_widget_by_name(&content, &cut_volume_form_name())?
        .downcast::<gtk::Box>()
        .ok()
}

/// F3.6 S1: what pressing 🔊 Volume did — as the sentence for the status line, with the form opened when there
/// is something to settle. NOTHING is recorded on this press: the record happens in [`press_volume_apply`],
/// because until someone gives the percent there is no loudness to place.
///
/// The refusal is answered BEFORE any form is drawn, so a cancelled dialog can never cost the user a refusal
/// (the F2.12 Insert lesson). A band under [`cut_speed::MIN_MARKED_SECONDS`] is not a band at all and falls
/// through to the line exactly as the flowchart draws it; the line is read through the same seam Paste, Insert
/// and Speed use (`paste_line`) and gated by whether one was ever placed, so the page has one notion of
/// "a line exists".
pub fn press_volume_item(window: &adw::ApplicationWindow) -> String {
    let band = selection(window)
        .filter(|band| band.length() > 0.0)
        .map(|band| (band.start, band.end));
    let known = INSERT_PLACE_KNOWN.with(|cell| *cell.borrow()) == Some(true);
    let line = known.then(|| paste_line(window));
    match fx_volume::press(band, line) {
        fx_volume::Pressed::Refused => {
            close_volume_form(window);
            fx_volume::NO_SECONDS.to_string()
        }
        pressed => {
            // `initial` is the effect the press opens the form with: twice as loud over the marked seconds,
            // or two seconds from the line. Its `t`/`dur` are the form's Length; its gain is the percent
            // field's starting number. For the line branch `initial` cannot know the playhead (it leaves
            // 0.0), so `initial_at_line` supplies the real second rather than letting the form title say
            // `Volume 00:00`.
            let fx = match (pressed, line) {
                (fx_volume::Pressed::LoudAtLine, Some(at)) => fx_volume::initial_at_line(at),
                (pressed, _) => fx_volume::initial(pressed).unwrap_or_default(),
            };
            let title = fx_volume::form_title(fx.t, fx.dur);
            show_volume_form(window, &fx);
            format!("{title} \u{2014} say the percent and \u{25b8} Apply places it")
        }
    }
}

/// Hide and empty the volume form. Used by the refusal path too, so a form left open from an earlier press cannot
/// sit on screen while the page says there is nothing to work on.
pub(crate) fn close_volume_form(window: &adw::ApplicationWindow) {
    set_volume_form(None);
    if let Some(holder) = volume_form_box_raw(window) {
        holder.set_visible(false);
    }
}

/// F3.6 S3: draw the form "Volume a – b" — the five fields in [`fx_volume::FORM_FIELDS`]'s order across two
// rows as `spec/img/06-volume.png` lays them out, plus Apply / Cancel. Built into locals and appended LAST in
/// one pass: a widget that already has a parent cannot be appended again and GTK asserts it, so nothing here may
/// travel between calls.
pub(crate) fn show_volume_form(window: &adw::ApplicationWindow, fx: &cut::Fx) {
    // Hidden FIRST, before anything is touched: while invisible its children cannot read as "already parented
    // here" to a later pass, and the clear below then removes exactly what this holder owns.
    let Some(holder) = volume_form_box_raw(window) else {
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
    LAST_VOLUME_WINDOW.with(|cell| *cell.borrow_mut() = Some(window.clone()));
    // §1's `lane` is settled HERE, not in Apply: these are the seconds the form is about, and only this window
    // knows which lanes it has. A record that already named one keeps it (`lane_answer`); a fresh press gets the
    // lane its seconds fall wholly inside, or `""` for the whole bed — and §A.6 gives the form no field for it,
    // so nothing on screen can contradict what gets stored.
    let fx = &{
        let answered = crate::fx_volume::lane_answer(&review_cut_of(window).lanes, &fx.lane);
        cut::Fx {
            lane: if answered.is_empty() {
                crate::fx_volume::lane_for_press(&review_cut_of(window).lanes, fx.t, fx.dur)
            } else {
                answered
            },
            ..fx.clone()
        }
    };

    let heading = gtk::Label::new(Some(&fx_volume::form_title(fx.t, fx.dur)));
    heading.set_xalign(0.0);
    heading.add_css_class("title-4");
    heading.set_widget_name("volume-heading");

    // Row 1: Volume % then Length (s), the order §A.6 reads them and the order the spec's picture shows.
    // The percent is first because it is the only answer about the sound. Prefilled with what the press put on
    // the record, read through `percent_of` so the field shows 200 for the default gain of 2.0 and nobody
    // writes a second copy of that pairing here.
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let percent_key = gtk::Label::new(Some(fx_volume::FORM_FIELDS[0]));
    percent_key.set_xalign(0.0);
    let percent = gtk::Entry::new();
    percent.set_widget_name("volume-field-percent");
    percent.set_text(&format!("{:.0}", fx_volume::percent_of(fx.gain)));
    percent.set_tooltip_text(Some(fx_volume::VOLUME_HELP_FULL));
    percent.set_width_chars(6);
    top.append(&percent_key);
    top.append(&percent);
    top.append(&zoom_field_row(
        fx_volume::FORM_FIELDS[1],
        &format!(
            "how long the change lasts, in the video's own seconds; {} s is the shortest this form takes",
            fx_volume::MIN_SECONDS
        ),
        "volume-field-length",
        &trim_seconds(fx.dur),
    ));

    // Row 2: Fade in, Fade out, Curve. Curve is a DropDown rather than the plain label the speed form uses:
    // the spec's picture draws a dropdown reading "Linear", and the list is §A.1's, owned by zoom, borrowed
    // here rather than copied so the two forms cannot offer different curves.
    let bottom = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    bottom.append(&zoom_field_row(
        fx_volume::FORM_FIELDS[2],
        fx_volume::FADE_IN_HELP,
        "volume-field-fade-in",
        &trim_seconds(fx.trans),
    ));
    bottom.append(&zoom_field_row(
        fx_volume::FORM_FIELDS[3],
        fx_volume::FADE_OUT_HELP,
        "volume-field-fade-out",
        &trim_seconds(fx.tout),
    ));
    let curve_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let curve_key = gtk::Label::new(Some(fx_volume::FORM_FIELDS[4]));
    curve_key.set_xalign(0.0);
    let curve_choice = gtk::DropDown::from_strings(&fx_zoom::CURVE_CHOICES);
    curve_choice.set_widget_name("volume-field-curve");
    curve_choice.set_tooltip_text(Some(fx_zoom::CURVE_HELP));
    curve_row.append(&curve_key);
    curve_row.append(&curve_choice);
    bottom.append(&curve_row);

    // The spec's picture carries only a ✕ in the corner; every other effect form pairs an explicit Apply with
    // a Cancel, and §F3.6's OK node needs a deliberate "put it on" — so both buttons are here, named like the
    // speed form's.
    let buttons = super::cut_form_buttons::form_buttons_row("volume");

    let footer = super::cut_form_buttons::form_note("volume");

    holder.append(&heading);
    holder.append(&top);
    holder.append(&bottom);
    holder.append(&buttons);
    holder.append(&footer);
    holder.set_visible(true);
    // Stored only now that the widgets exist and belong to THIS window: `volume_form_open()` then always means
    // "there is a form on screen behind it", never "a value was parked somewhere".
    set_volume_form(Some(volume_form_of(fx)));
    wire_volume_buttons(window);
}

/// The form a press starts from, built out of the record so no default is invented in the widget layer. The
/// percent is the stored gain read in the field's own unit; the curve starts on the one choice §A.1 lists; and the
/// lane rides along from the record itself, so ✎ Edit of a volume that names one lane keeps naming it (§F3.8's
/// "always takes its values from the live effect").
fn volume_form_of(fx: &cut::Fx) -> fx_volume::Form {
    fx_volume::Form {
        t: fx.t,
        percent: fx_volume::percent_of(fx.gain),
        dur: fx.dur,
        trans: fx.trans,
        tout: fx.tout,
        curve: fx_zoom::CURVE_CHOICES[0].to_string(),
        lane: fx.lane.clone(),
    }
}

/// F3.6 S2/S3: the form waiting on the page, before Apply. What a widget test reads to assert the same state
/// the logic test asserts ([`fx_volume::Form`]) rather than a painted field.
pub fn volume_form_open() -> Option<fx_volume::Form> {
    VOLUME_FORM.with(|cell| cell.borrow().clone())
}

fn set_volume_form(form: Option<fx_volume::Form>) {
    VOLUME_FORM.with(|cell| *cell.borrow_mut() = form);
}

thread_local! {
    /// The form a Volume press left waiting on the page. `None` is no form open, which is also what hides the
    /// holder — one slot answers both questions so they cannot disagree.
    static VOLUME_FORM: std::cell::RefCell<Option<fx_volume::Form>> =
        const { std::cell::RefCell::new(None) };

    /// The window whose volume form was last shown, so the field readers have THIS window's tree to look in
    /// and a stale window cannot answer for a live one.
    static LAST_VOLUME_WINDOW: std::cell::RefCell<Option<adw::ApplicationWindow>> =
        const { std::cell::RefCell::new(None) };
}

fn volume_form_owner(window: &adw::ApplicationWindow) -> bool {
    LAST_VOLUME_WINDOW.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|w: &adw::ApplicationWindow| w.as_ptr() == window.as_ptr())
            .unwrap_or(false)
    })
}

/// One of this window's volume-field entries, by name, looked up through the window that owns the form.
fn volume_entry(name: &str) -> Option<gtk::Entry> {
    LAST_VOLUME_WINDOW.with(|cell| {
        cell.borrow()
            .as_ref()
            .and_then(|w| find_widget_by_name(w.upcast_ref(), name))
            .and_then(|w| w.downcast::<gtk::Entry>().ok())
    })
}

/// This window's Curve dropdown.
fn volume_dropdown(name: &str) -> Option<gtk::DropDown> {
    LAST_VOLUME_WINDOW.with(|cell| {
        cell.borrow()
            .as_ref()
            .and_then(|w| find_widget_by_name(w.upcast_ref(), name))
            .and_then(|w| w.downcast::<gtk::DropDown>().ok())
    })
}

/// F3.6 S3/S4: Apply. Reads the five fields back, hands them to [`fx_volume::apply`] (which clamps the
/// percent to the ceiling, shares the fades by §A.2 and decides what `ease` key the curve writes), and puts
/// the result on the cut as ONE edit.
pub fn press_volume_apply(window: &adw::ApplicationWindow) -> String {
    if !volume_form_owner(window) {
        return "no volume form on this page \u{2014} mark a stretch or put the line down first".to_string();
    }
    let Some(stored) = volume_form_open() else {
        return "no volume to apply \u{2014} press \u{1f50a} Volume first".to_string();
    };
    // A field that does not parse keeps what the form held rather than becoming 0: a half-typed "20" must not
    // silence a passage. Same rule as the speed form's reader.
    let read = |name: &str, keep: f64| -> f64 {
        volume_entry(name)
            .map(|entry| entry.text().trim().to_string())
            .and_then(|text| text.parse::<f64>().ok())
            .unwrap_or(keep)
    };
    let curve = volume_dropdown("volume-field-curve")
        .and_then(|drop| {
            drop.selected_item()
                .and_downcast::<gtk::StringObject>()
                .map(|item| item.string().to_string())
        })
        .unwrap_or_else(|| stored.curve.clone());
    let form = fx_volume::Form {
        t: stored.t,
        percent: read("volume-field-percent", stored.percent),
        dur: read("volume-field-length", stored.dur),
        trans: read("volume-field-fade-in", stored.trans),
        tout: read("volume-field-fade-out", stored.tout),
        curve,
        // §1's `lane`: not a field, so nothing re-derives it here — [`show_volume_form`] settled it against this
        // window's lanes when the form opened (the press's own seconds, or the edited record's answer), and no
        // field the user can type moves who owns those seconds.
        lane: stored.lane.clone(),
    };
    let effect = match fx_volume::apply(&form) {
        Ok(effect) => effect,
        Err(reason) => {
            // The reason prints verbatim and the form STAYS OPEN: a refusal that closed the form would throw
            log_line(&reason);
            live_form_refused();
            return reason;
        }
    };
    let mut cut_ = review_cut_of(window);
    cut_.fx.push(effect.clone());
    save_insert_cut(&cut_);
    // `record_edit`, NOT `seed_review_cut`: the addition must sit on the history so F2.13's ↶ takes it back.
    live_form_answer_is_undo(window);
    record_edit(window, &cut_);
    refresh_effects_lane(window);
    close_volume_form(window);
    let said = fx_volume::placed_status(&effect);
    log_line(&said);
    said
}

/// F3.6 S3: Cancel drops the form and changes nothing on the cut — no record pushed, no history written, so
/// ↶ still points where it did before the press.
pub fn press_volume_cancel(window: &adw::ApplicationWindow) -> String {
    close_volume_form(window);
    "left as it was \u{2014} no volume placed".to_string()
}

/// F3.6: Esc drops an open volume form, and claims no other key. Returns `None` when nothing was open, or when
/// this window is not the one holding it, so the key travels on.
pub fn press_volume_esc(window: &adw::ApplicationWindow) -> Option<String> {
    if volume_form_open().is_none() || !volume_form_owner(window) {
        return None;
    }
    Some(press_volume_cancel(window))
}

/// Wire the form's two buttons BY NAME. Each forwards one press and prints what comes back; neither holds a rule.
fn wire_volume_buttons(window: &adw::ApplicationWindow) {
    for (name, door) in [("volume-apply-button", true), ("volume-cancel-button", false)] {
        if let Some(button) = line_step_button(window, name) {
            let win = window.clone();
            button.connect_clicked(move |_| {
                let said = if door {
                    press_volume_apply(&win)
                } else {
                    press_volume_cancel(&win)
                };
                if let Some(status_line) = find_status(win.upcast_ref()) {
                    status_line.set_text(&said);
                }
            });
        }
    }
}

/// Esc releases an open volume form, wired after `set_content` like every other control. Claims Escape only when
/// this window owns a form; otherwise the key travels on to whatever else is listening.
pub(crate) fn wire_volume_esc(window: &adw::ApplicationWindow) {
    let controller = gtk::EventControllerKey::new();
    let win = window.clone();
    controller.connect_key_pressed(move |_ctrl, key, _code, _mods| {
        if key != gtk::gdk::Key::Escape {
            return glib::Propagation::Proceed;
        }
        match press_volume_esc(&win) {
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

/// F3.6 S5 (`applied in the preview even while paused`): the page's name for the loudness of one second. It is
/// the SAME function the playing preview asks (§06#2) — `fx_volume::heard_while_paused` is `cut_hear::gain_under`
/// by another name — so parking the red line inside a raised passage gives what will play, which is the only way
/// the two fades can be judged without a moving frame. The mix-side twin is [`apply_mix`] / `cut_hear::mix_at`.
pub fn paused_preview_gain(window: &adw::ApplicationWindow, t: f64) -> f64 {
    fx_volume::heard_while_paused(&review_cut_of(window).fx, t)
}
