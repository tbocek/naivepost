//! F3.3 ⏩ Speed and stop by hand — the page half of the flow: the press, the form drawer, the Apply.
//!
//! The rules live in [`crate::cut_speed`]; here is only the state a press leaves behind and the widgets that show it. No decision of its own.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut;
use crate::cut_speed;
use crate::fx_zoom;
use crate::shell::Page;
use crate::ui::cut_label_hand::label_entry_of as speed_entry_of;
use crate::ui::window::{
    find_status, find_widget_by_name, line_step_button, live_form_answer_is_undo, log_line,
    press_band_esc, record_edit, refresh_effects_lane, save_insert_cut, selection, trim_seconds,
    zoom_field_row, INSERT_PLACE_KNOWN,
};

// --- F3.3 Speed and stop by hand: the press, the form, the Apply ----------------------------------------------

/// F3.3 S1: what pressing ⏩ Speed did, as the status sentence, with the form opened when there is something to settle. NOTHING is recorded on this press ([`press_speed_apply`] records).
///
/// The refusal is answered BEFORE any form is drawn, so a cancelled dialog never costs the user a refusal (F2.12 Insert). A band under [`cut_speed::MIN_MARKED_SECONDS`] falls through to the line, read through the seam Paste and Insert use (`paste_line`).
pub fn press_speed_item(window: &adw::ApplicationWindow) -> String {
    let band = selection()
        .filter(|band| band.length() > 0.0)
        .map(|band| (band.start, band.end));
    let known = INSERT_PLACE_KNOWN.with(|cell| *cell.borrow()) == Some(true);
    let line = known.then(|| crate::ui::cut_lane::paste_line());
    match cut_speed::press(band, line) {
        cut_speed::Pressed::Refused => {
            close_speed_form(window);
            cut_speed::NO_SECONDS.to_string()
        }
        pressed => {
            // `initial` opens the form: a half-speed stretch over the marked seconds, or a stop faded on and off; its `t`/`dur` are the form's Length.
            let fx = cut_speed::initial(pressed).unwrap_or_default();
            let title = cut_speed::form_title(fx.t, fx.dur);
            show_speed_form(window, &fx);
            format!("{title} \u{2014} say the rate and \u{25b8} Apply places it")
        }
    }
}

/// This window's speed-form holder name. `page_box` runs once per tab, so an unscoped name exists four times and lookups hit whichever box is reached first (the `fold-badges` / `camera-rows` / `zoom-form` rule).
fn cut_speed_form_name() -> String {
    format!("speed-form-{}", Page::Cut.label())
}

/// Resolve THIS window's speed-form holder through its own content, so a closed window's surviving tree cannot answer for a live one.
fn speed_form_box_raw(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    let content = window.content()?;
    find_widget_by_name(&content, &cut_speed_form_name())?
        .downcast::<gtk::Box>()
        .ok()
}

/// Hide and empty the speed form; the refusal path uses it too, so no form lingers while the page says there is nothing to work on.
pub(crate) fn close_speed_form(window: &adw::ApplicationWindow) {
    set_speed_form(None);
    if let Some(holder) = speed_form_box_raw(window) {
        holder.set_visible(false);
    }
}

/// F3.3 S2: draw the form "Speed a – b": the six fields in [`cut_speed::FORM_FIELDS`]'s order, the cost note under Sound, Apply / Cancel. Built into locals and appended LAST in one pass:
/// a widget that already has a parent cannot be appended again (GTK asserts).
pub(crate) fn show_speed_form(window: &adw::ApplicationWindow, fx: &cut::Fx) {
    // Hidden FIRST, so its children cannot read as already parented to a later pass and the clear removes exactly what this holder owns.
    let Some(holder) = speed_form_box_raw(window) else {
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
    LAST_SPEED_WINDOW.with(|cell| *cell.borrow_mut() = Some(window.clone()));

    let heading = gtk::Label::new(Some(&cut_speed::form_title(fx.t, fx.dur)));
    heading.set_xalign(0.0);
    heading.add_css_class("title-4");
    heading.set_widget_name("speed-heading");

    // Speed ×: the listed rates, `Custom…` last, opening on `rate_index` so an unlisted rate shows Custom rather than snapping to a neighbour.
    let rate_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let rate_key = gtk::Label::new(Some(cut_speed::FORM_FIELDS[0]));
    rate_key.set_xalign(0.0);
    // Owned labels first, then borrowed: `from_strings` takes `&[&str]`, so the Strings have to outlive the call.
    let rate_labels: Vec<String> = cut_speed::RATES
        .iter()
        .map(|rate| cut_speed::rate_label(*rate))
        .chain(std::iter::once(cut_speed::CUSTOM.to_string()))
        .collect();
    let rate_items: Vec<&str> = rate_labels.iter().map(String::as_str).collect();
    let rate_choice = gtk::DropDown::from_strings(&rate_items);
    rate_choice.set_widget_name("speed-field-rate");
    rate_choice.set_selected(cut_speed::rate_index(fx.rate) as u32);
    // The typed box behind `Custom…`, greyed until Custom is showing: two live controls for one question would disagree on screen (as the F3.1 zoom form).
    let custom_rate = gtk::Entry::new();
    custom_rate.set_widget_name("speed-field-custom");
    custom_rate.set_width_chars(6);
    custom_rate.set_text(&trim_seconds(fx.rate));
    custom_rate.set_tooltip_text(Some("a rate of your own, if none of the listed ones is it"));
    refresh_custom_rate_sensitivity(&rate_choice, &custom_rate);
    rate_choice.connect_selected_notify({
        let custom = custom_rate.clone();
        move |drop| refresh_custom_rate_sensitivity(drop, &custom)
    });
    rate_row.append(&rate_key);
    rate_row.append(&rate_choice);
    rate_row.append(&custom_rate);

    // Sound: the five answers in §A.3's order, opening on the one this effect stores.
    let sound_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let sound_key = gtk::Label::new(Some(cut_speed::FORM_FIELDS[1]));
    sound_key.set_xalign(0.0);
    let sound_choice = gtk::DropDown::from_strings(&cut_speed::SOUND_CHOICES);
    sound_choice.set_widget_name("speed-field-sound");
    let stored = cut_speed::sound_stored(&fx.snd);
    let start = cut_speed::SOUND_CHOICES
        .iter()
        .position(|choice| cut_speed::sound_stored(choice) == stored)
        .unwrap_or(0);
    sound_choice.set_selected(start as u32);
    sound_row.append(&sound_key);
    sound_row.append(&sound_choice);

    // The cost note sits directly under Sound, where the choice it explains is still the thing being read.
    let cost = gtk::Label::new(Some(&cut_speed::cost_note(stored, fx.rate, fx.dur)));
    cost.set_xalign(0.0);
    cost.set_wrap(true);
    cost.add_css_class("dim-label");
    cost.set_widget_name("speed-cost-note");

    let length = zoom_field_row(
        cut_speed::FORM_FIELDS[2],
        "how long the effect lasts, in the video's own seconds",
        "speed-field-length",
        &trim_seconds(fx.dur),
    );
    let fade_in = zoom_field_row(
        cut_speed::FORM_FIELDS[3],
        &cut_speed::fade_in_help(),
        "speed-field-fade-in",
        &trim_seconds(fx.trans),
    );
    let fade_out = zoom_field_row(
        cut_speed::FORM_FIELDS[4],
        "and back to normal. Same price going out as coming in.",
        "speed-field-fade-out",
        &trim_seconds(fx.tout),
    );
    // Curve: one choice, shown as a label like the zoom form; `cut_speed::apply` feeds it through `fx_zoom::curve_stored`, so both forms must offer what that understands.
    let curve_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let curve_key = gtk::Label::new(Some(cut_speed::FORM_FIELDS[5]));
    curve_key.set_xalign(0.0);
    let curve_value = gtk::Label::new(Some(fx_zoom::CURVE_CHOICES[0]));
    curve_value.set_widget_name("speed-field-curve");
    curve_value.set_tooltip_text(Some(fx_zoom::CURVE_HELP));
    curve_row.append(&curve_key);
    curve_row.append(&curve_value);

    let buttons = super::cut_form_buttons::form_buttons_row("speed");

    let footer = super::cut_form_buttons::form_note("speed");

    holder.append(&heading);
    holder.append(&rate_row);
    holder.append(&sound_row);
    holder.append(&cost);
    holder.append(&length);
    holder.append(&fade_in);
    holder.append(&fade_out);
    holder.append(&curve_row);
    holder.append(&buttons);
    holder.append(&footer);
    holder.set_visible(true);
    // Stored only now that the widgets exist and belong to THIS window, so `speed_form_open()` always means a form is on screen.
    set_speed_form(Some(form_of(fx)));

    wire_speed_buttons(window);
}

/// The `Fx` the press produced, as the form's own state; Apply reads it back rather than reaching into the cut.
pub(crate) fn form_of(fx: &cut::Fx) -> cut_speed::Form {
    cut_speed::Form {
        t: fx.t,
        dur: fx.dur,
        rate: fx.rate,
        trans: fx.trans,
        tout: fx.tout,
        curve: String::new(),
        snd: cut_speed::sound_stored(&fx.snd),
    }
}

/// F3.3 S2/S3: the form waiting on the page, before Apply; what a widget test reads ([`cut_speed::Form`]).
pub fn speed_form_open() -> Option<cut_speed::Form> {
    SPEED_FORM.with(|cell| cell.borrow().clone())
}

fn set_speed_form(form: Option<cut_speed::Form>) {
    SPEED_FORM.with(|cell| *cell.borrow_mut() = form);
}

thread_local! {
    /// The form a Speed press left waiting; `None` is no form open, which also hides the holder.
    static SPEED_FORM: std::cell::RefCell<Option<cut_speed::Form>> =
        const { std::cell::RefCell::new(None) };

    /// The window whose speed form was last shown, so the field readers look in THIS window's tree.
    static LAST_SPEED_WINDOW: std::cell::RefCell<Option<adw::ApplicationWindow>> =
        const { std::cell::RefCell::new(None) };
}

/// Read one of the speed form's entries back from THIS window's tree.
fn speed_entry(name: &str) -> Option<gtk::Entry> {
    LAST_SPEED_WINDOW.with(|cell| {
        cell.borrow()
            .as_ref()
            .and_then(|w| find_widget_by_name(w.upcast_ref(), name))
            .and_then(|w| w.downcast::<gtk::Entry>().ok())
    })
}

/// Read one of the speed form's dropdowns back from THIS window's tree.
fn speed_dropdown(name: &str) -> Option<gtk::DropDown> {
    LAST_SPEED_WINDOW.with(|cell| {
        cell.borrow()
            .as_ref()
            .and_then(|w| find_widget_by_name(w.upcast_ref(), name))
            .and_then(|w| w.downcast::<gtk::DropDown>().ok())
    })
}

/// Does THIS window own the live speed form? Two windows holding one would put Apply's write into whichever tree is reached first.
fn speed_form_owner(window: &adw::ApplicationWindow) -> bool {
    LAST_SPEED_WINDOW.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|w: &adw::ApplicationWindow| w.as_ptr() == window.as_ptr())
            .unwrap_or(false)
    })
}

/// The rate the form stands on right now: a listed row's own number, or the typed box when Custom is showing.
/// An entry that does not parse keeps what the form held rather than becoming zero.
fn speed_rate_from_form(stored: &cut_speed::Form) -> f64 {
    let listed = speed_dropdown("speed-field-rate").map(|drop| drop.selected() as usize);
    match listed {
        Some(index) if index < cut_speed::RATES.len() => cut_speed::RATES[index],
        // Custom… — whatever is in the box. The zoom form keeps a typed number the same way.
        _ => speed_entry("speed-field-custom")
            .map(|entry| entry.text().trim().to_string())
            .and_then(|text| text.parse::<f64>().ok())
            .unwrap_or(stored.rate),
    }
}

/// Is THIS window's typed rate box live? Read by a test to assert the greying; live only while the dropdown stands on `Custom…` (index == RATES.len()).
pub fn speed_custom_rate_visible() -> bool {
    LAST_SPEED_WINDOW.with(|cell| {
        cell.borrow()
            .as_ref()
            .and_then(|w| find_widget_by_name(w.upcast_ref(), "speed-field-custom"))
            .and_then(|w| w.downcast::<gtk::Entry>().ok())
            .map(|entry| entry.is_sensitive())
            .unwrap_or(false)
    })
}

fn refresh_custom_rate_sensitivity(drop: &gtk::DropDown, custom: &gtk::Entry) {
    let custom_showing = drop.selected() as usize >= cut_speed::RATES.len();
    custom.set_sensitive(custom_showing);
}

/// F3.3 S3: Apply. Reads the six fields back, hands them to [`cut_speed::apply`] (which decides stop-vs-clamp
/// and shares the fades), and puts the result on the cut as ONE edit.
pub fn press_speed_apply(window: &adw::ApplicationWindow) -> String {
    if !speed_form_owner(window) {
        return "no speed form on this page \u{2014} mark a stretch or put the line down first"
            .to_string();
    }
    let Some(stored) = speed_form_open() else {
        return "no speed to apply \u{2014} press \u{23e9} Speed first".to_string();
    };
    let read = |name: &str, keep: f64| -> f64 {
        speed_entry(name)
            .map(|entry| entry.text().trim().to_string())
            .and_then(|text| text.parse::<f64>().ok())
            .unwrap_or(keep)
    };
    let sound_index = speed_dropdown("speed-field-sound")
        .map(|drop| drop.selected() as usize)
        .unwrap_or(0);
    let snd = cut_speed::SOUND_CHOICES
        .get(sound_index)
        .copied()
        .unwrap_or(cut_speed::SOUND_CHOICES[0]);
    let form = cut_speed::Form {
        t: stored.t,
        dur: read("speed-field-length", stored.dur),
        rate: speed_rate_from_form(&stored),
        trans: read("speed-field-fade-in", stored.trans),
        tout: read("speed-field-fade-out", stored.tout),
        curve: stored.curve.clone(),
        snd: cut_speed::sound_stored(snd),
    };
    let effect = cut_speed::apply(&form);
    let mut cut_ = crate::ui::window_preview::newest_review_cut();
    // §9 (`A stop at a dropped second SHOULD be refused when placed`): refuse BEFORE writing: a stop holds a frame, and the cut keeps none at a dropped second or a card.
    // The form stays open with the reason, as for a refused length. A cut with no scenes has no say.
    if crate::fx_record::is_stop(&effect) && !cut_.segs.is_empty() {
        if let Err(reason) = crate::effect_overlap::place_stop(effect.t, &cut_.segs) {
            log_line(&reason);
            crate::ui::live_form_refused();
            return reason;
        }
    }
    cut_.fx.push(effect.clone());
    save_insert_cut(&cut_);
    // `record_edit`, NOT `seed_review_cut`: the addition must sit on the history so F2.13's ↶ takes it back.
    live_form_answer_is_undo(window);
    record_edit(window, &cut_);
    refresh_effects_lane(window);
    close_speed_form(window);
    let said = cut_speed::placed_status(&effect);
    log_line(&said);
    // §9 (`it SHOULD say "sound at 1× in the render only"`): `own` and `scene` run on a different clock from the picture, which one player cannot do, so the preview plays the
    // sped sound and says what the render will not match, on its own line (the placement status is the quote §F3.3 fixes).
    if let Some(notice) = crate::effect_overlap::says_render_only(&effect.snd) {
        log_line(notice);
    } else if let Some(agree) = sound_promise(&effect.snd) {
        // The other two rows of §9's table: `""` and `pitch` move with the picture's clock in both players; `mute` is settled by the render joining the preview.
        log_line(agree);
    }
    said
}

/// §9's two remaining sound rows, one sentence each ([`crate::effect_overlap::says_render_only`] covers `own`/`scene`).
pub fn sound_promise(snd: &str) -> Option<&'static str> {
    if crate::effect_overlap::preview_plays_pitch_as_render(snd) {
        return Some("the preview plays this sound the way the render does");
    }
    crate::effect_overlap::says_render_only(snd)
        .is_none()
        .then_some("the preview silences the whole bed, and so now does the render")
}

/// F3.3 S3: Cancel drops the form and changes nothing on the cut (no record, no history, ↶ unchanged).
pub fn press_speed_cancel(window: &adw::ApplicationWindow) -> String {
    close_speed_form(window);
    "left as it was \u{2014} no speed placed".to_string()
}

/// F3.3: Esc drops an open speed form and claims no other key; `None` when nothing was open, so the key travels on.
pub fn press_speed_esc(window: &adw::ApplicationWindow) -> Option<String> {
    if speed_form_open().is_none() || !speed_form_owner(window) {
        return None;
    }
    Some(press_speed_cancel(window))
}

/// The debounced commit: re-read THIS window's number entries into the stored form state and print the kept length; the cut waits for Apply (§F3.8 S5, `wire_live_fields`).
fn keep_speed_fields(window: &adw::ApplicationWindow) {
    let Some(stored) = speed_form_open() else {
        return;
    };
    let read = |name: &str, keep: f64| -> f64 {
        speed_entry_of(window, name)
            .map(|entry| entry.text().trim().to_string())
            .and_then(|text| text.parse::<f64>().ok())
            .unwrap_or(keep)
    };
    let dur = read("speed-field-length", stored.dur);
    set_speed_form(Some(cut_speed::Form {
        dur,
        trans: read("speed-field-fade-in", stored.trans),
        tout: read("speed-field-fade-out", stored.tout),
        ..stored
    }));
    super::cut_form_buttons::say_kept(
        window,
        &format!("{} \u{2014} kept as you type", trim_seconds(dur)),
    );
}

/// Wire the form's two buttons BY NAME. Each forwards one press and prints what comes back; neither holds a rule.
pub(crate) fn wire_speed_buttons(window: &adw::ApplicationWindow) {
    for (name, door) in [("speed-apply-button", true), ("speed-cancel-button", false)] {
        if let Some(button) = line_step_button(window, name) {
            let win = window.clone();
            button.connect_clicked(move |_| {
                let said = if door {
                    press_speed_apply(&win)
                } else {
                    press_speed_cancel(&win)
                };
                if let Some(status_line) = find_status(win.upcast_ref()) {
                    status_line.set_text(&said);
                }
            });
        }
    }
    // F3.8 S5: the same pause for this form's number fields as every other live form gets.
    super::cut_form_buttons::wire_live_fields(window, "speed", keep_speed_fields);
}

/// Esc releases an open speed form; claims Escape only when this window owns a form.
pub(crate) fn wire_speed_esc(window: &adw::ApplicationWindow) {
    crate::ui::cut_form_buttons::wire_esc(window, press_speed_esc);
}

/// F3.8 S3 (`Esc drops the hold and disarms`): the band's own Esc controller, shaped like `wire_speed_esc`: it prints what [`press_band_esc`] answered and lets the key
/// travel on when this window holds no effect, so an open form or arm is handled by that kind's controller, wired before this one.
pub(crate) fn wire_band_esc(window: &adw::ApplicationWindow) {
    let controller = gtk::EventControllerKey::new();
    let win = window.clone();
    controller.connect_key_pressed(move |_ctrl, key, _code, _mods| {
        if key != gtk::gdk::Key::Escape {
            return glib::Propagation::Proceed;
        }
        match press_band_esc(&win) {
            Some(said) => {
                if let Some(status_line) = find_status(win.upcast_ref()) {
                    status_line.set_text(&said);
                }
                glib::Propagation::Stop
            }
            None => glib::Propagation::Proceed,
        }
    });
    window.add_controller(controller.clone());
    BAND_ESC_KEYS.with(|cell| *cell.borrow_mut() = Some(controller));
}

thread_local! {
    /// The band's Esc controller, kept so a test can fire a real Escape through it.
    static BAND_ESC_KEYS: std::cell::RefCell<Option<gtk::EventControllerKey>> =
        const { std::cell::RefCell::new(None) };
}

/// This window's band-Esc controller, so a test can see it is attached. `None` on a page that drew none.
pub fn band_esc_controller() -> Option<gtk::EventControllerKey> {
    BAND_ESC_KEYS.with(|cell| cell.borrow().clone())
}
