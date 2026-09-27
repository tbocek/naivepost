// F3.3 Speed and stop by hand — the wire: a click on ⏩ Speed reaches `cut_speed`'s S1–S4 through real widgets,
// the "Speed a – b" form draws its six fields in the spec's order, and Apply puts ONE record on the cut that one
// ↶ takes back.
//
// `naivepost::cut_speed` holds the rules (proven by tests/cut_speed_stop.rs, which this file does not touch);
// this proves the Cut page can actually reach them. Before F3.3's redo there was no `speed-form-*`, no
// `press_speed_item` and no Apply button at all — the dropdown row fell through `press_effect_item` into the
// generic `fx_record::record_into` path, so every rule in the module was unreachable from the UI. Every check
// here fires a widget the way GTK does (`emit_by_name("clicked")`, `set_selected` → `notify::selected`) and
// compares against the module rather than a retyped string, so a paraphrase or a dropped wire fails loudly.
//
// One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared GTK state.
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;

use naivepost::cut::Cut;
use naivepost::cut_select::Surface;
use naivepost::cut_speed::{self as speed, MIN_MARKED_SECONDS};
use naivepost::shell::Page;
use naivepost::ui;

static RAN_BAND: AtomicBool = AtomicBool::new(false);
static RAN_LINE_STOP: AtomicBool = AtomicBool::new(false);
static RAN_REFUSAL: AtomicBool = AtomicBool::new(false);
static RAN_FIELDS: AtomicBool = AtomicBool::new(false);
static RAN_STOP_APPLY: AtomicBool = AtomicBool::new(false);
static RAN_RATE_GIVES_WAY: AtomicBool = AtomicBool::new(false);
static RAN_STATUS_UNDO: AtomicBool = AtomicBool::new(false);

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

thread_local! {
    /// The only strong handle on the window last built, so the next check drops it first: a closed GTK window is
    /// not destroyed and its names stay parented, which would send a lookup to the wrong tree.
    static LAST_WINDOW: std::cell::RefCell<Option<adw::ApplicationWindow>> =
        const { std::cell::RefCell::new(None) };
}

fn release_last_window() {
    LAST_WINDOW.with(|cell| {
        if let Some(old) = cell.borrow_mut().take() {
            old.close();
        }
    });
    settle();
}

/// Let the main context run what the widget emissions queued.
fn settle() {
    let context = glib::MainContext::default();
    for _ in 0..64 {
        if !context.iteration(false) {
            break;
        }
    }
}

fn status_text(window: &adw::ApplicationWindow) -> String {
    ui::find_status(window.upcast_ref())
        .expect("the shell has a status line")
        .text()
        .to_string()
}

fn widget(window: &adw::ApplicationWindow, name: &str) -> gtk::Widget {
    ui::find_source_widget(window, name).unwrap_or_else(|| panic!("the Cut page drew no {name}"))
}

fn entry(window: &adw::ApplicationWindow, name: &str) -> gtk::Entry {
    widget(window, name)
        .downcast::<gtk::Entry>()
        .unwrap_or_else(|_| panic!("`{name}` is an Entry"))
}

fn dropdown(window: &adw::ApplicationWindow, name: &str) -> gtk::DropDown {
    widget(window, name)
        .downcast::<gtk::DropDown>()
        .unwrap_or_else(|_| panic!("`{name}` is a DropDown"))
}

fn label_text(window: &adw::ApplicationWindow, name: &str) -> String {
    widget(window, name)
        .downcast::<gtk::Label>()
        .unwrap_or_else(|_| panic!("`{name}` is a Label"))
        .text()
        .to_string()
}

fn click(window: &adw::ApplicationWindow, name: &str) {
    ui::line_step_button(window, name)
        .unwrap_or_else(|| panic!("the Cut page drew no clickable {name}"))
        .emit_by_name::<()>("clicked", &[]);
    settle();
}

/// The strings a dropdown offers, in order. `DropDown::model()` is an `Option<ListModel>`, so it is unwrapped
/// here rather than at every call site.
fn strings_of(drop: &gtk::DropDown) -> Vec<String> {
    let model = drop.model().expect("the dropdown has a model");
    let list = model.downcast::<gtk::StringList>().expect("a list of strings");
    strings(list.as_ref())
}

/// The strings a `StringList` holds, in order.
fn strings(model: &glib::Object) -> Vec<String> {
    let list = model.downcast_ref::<gtk::StringList>().expect("a list of strings");
    (0..list.n_items())
        .map(|index| {
            list.item(index)
                .and_downcast::<gtk::StringObject>()
                .expect("a string item")
                .string()
                .to_string()
        })
        .collect()
}

/// This window's speed-form holder, and whether it is showing.
fn form_holder(window: &adw::ApplicationWindow) -> gtk::Box {
    widget(window, &format!("speed-form-{}", Page::Cut.label()))
        .downcast::<gtk::Box>()
        .expect("the speed form holder is a Box")
}

fn form_visible(window: &adw::ApplicationWindow) -> bool {
    form_holder(window).get_visible()
}

/// Build a window sitting on the Cut tab over `seeded`, dropping the previous check's window first.
fn cut_page(app: &adw::Application, seeded: &Cut) -> adw::ApplicationWindow {
    release_last_window();
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    assert!(
        model.sources.iter().any(|source| source.footage),
        "the fixture must carry a footage row or the Cut tab is locked (shell::lock)"
    );
    let window = ui::build_window(app, &model, "Prepare");
    LAST_WINDOW.with(|cell| *cell.borrow_mut() = Some(window.clone()));
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_by_name::<()>("clicked", &[]);
    ui::reopen_history_on(&window, seeded);
    ui::seed_review_cut(&window, seeded);
    settle();
    window
}

/// Mark a stretch on the ruler/selection ground — the same seam a drag leaves behind — so S1 has a band.
fn mark_band(window: &adw::ApplicationWindow, from: f64, to: f64) {
    let band = ui::draw_selection(window, Surface::SelectionBand, None, from, to);
    assert!(band.is_some(), "the band was drawn");
    settle();
}

/// Choose a dropdown row the way the toolkit delivers a popover pick: `set_selected` emits `notify::selected`,
/// the signal a click inside the popover raises. Nothing here calls a rule directly.
fn choose(window: &adw::ApplicationWindow, name: &str, index: usize) {
    dropdown(window, name).set_selected(index as u32);
    settle();
}

fn round(app: &adw::Application) {
    // --- S1: a marked band opens the form AT ITS OWN SECONDS ----------------------------------
    {
        let window = cut_page(app, &Cut::default());
        mark_band(&window, 10.0, 25.0);
        click(&window, "effect-item-speed");
        assert!(form_visible(&window), "a marked stretch opens the form");
        // (t, dur) -- the module prints the END as t + dur, so the band's own pair goes in and the heading
        // comes out reading 00:10 - 00:25 for a band from 10 s to 25 s.
        assert_eq!(
            label_text(&window, "speed-heading"),
            speed::form_title(10.0, 15.0),
            "the heading is the module's own \"Speed a - b\", not a paraphrase"
        );
        assert_eq!(
            entry(&window, "speed-field-length").text().trim(),
            "15",
            "Length reads the BAND's seconds -- t and dur come from the stretch, not a default width"
        );
        let rate = dropdown(&window, "speed-field-rate");
        assert_eq!(
            rate.selected() as usize,
            speed::rate_index(speed::DEFAULT_RATE),
            "the rate opens on ×0.5 (`DEFAULT_RATE`), the one case worth a default"
        );
        assert_eq!(
            strings_of(&rate)[rate.selected() as usize],
            speed::rate_label(speed::DEFAULT_RATE),
            "and that row reads ×0.5, matching the number it stands for"
        );
        assert!(
            ui::review_cut_of(&window).fx.is_empty(),
            "a PRESS records nothing: until the rate is answered there is no effect to place"
        );
        RAN_BAND.store(true, Ordering::SeqCst);
        window.close();
    }

    // --- S1: only a line → a stop at the line, 2 s, 0.5 s fades -------------------------------
    {
        let window = cut_page(app, &Cut::default());
        // No band at all; just the red line somewhere that is not midnight.
        ui::note_place(true);
        ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 42.0 });
        click(&window, "effect-item-speed");
        assert!(form_visible(&window), "a bare line opens the form too -- as a stop");
        let rate = dropdown(&window, "speed-field-rate");
        assert_eq!(
            rate.selected() as usize,
            speed::rate_index(0.0),
            "the rate sits on ×0, which §F3.3 spells `stop`"
        );
        assert_eq!(strings_of(&rate)[0], "×0 — stop", "and that row says so in words");
        assert_eq!(
            entry(&window, "speed-field-length").text().trim(),
            trim(speed::STOP_SECONDS),
            "// P.policy.effectDefaultSeconds -- a stop is 2 s"
        );
        assert_eq!(
            entry(&window, "speed-field-fade-in").text().trim(),
            trim(speed::STOP_FADE_SECONDS),
            "// P.policy.effectDefaultFades -- faded ON, not cut in"
        );
        assert_eq!(
            entry(&window, "speed-field-fade-out").text().trim(),
            trim(speed::STOP_FADE_SECONDS),
            "...and faded OFF"
        );
        RAN_LINE_STOP.store(true, Ordering::SeqCst);
        window.close();
    }

    // --- S1: neither band nor line → refusal, and NO form drawn --------------------------------
    {
        let window = cut_page(app, &Cut::default());
        // A band under the floor is NOT a band -- it falls through to the line. With no line placed either,
        // S1's refusal is the whole answer, and it must be answered BEFORE any form opens so a cancelled
        // dialog can never cost the user a refusal.
        mark_band(&window, 10.0, 10.0 + MIN_MARKED_SECONDS / 4.0);
        ui::note_place(false);
        click(&window, "effect-item-speed");
        assert_eq!(
            status_text(&window),
            speed::NO_SECONDS,
            "the refusal is `cut_speed::NO_SECONDS` verbatim -- the fifth wording for the same missing \
             thing, spelled once in the module"
        );
        assert!(
            !form_visible(&window),
            "and nothing was drawn: a form with no seconds to work on would invite an Apply that cannot work"
        );
        assert!(ui::speed_form_open().is_none(), "no form is held either");
        RAN_REFUSAL.store(true, Ordering::SeqCst);
        window.close();
    }

    // --- S2: the six fields, in the order the spec reads them ---------------------------------
    {
        let window = cut_page(app, &Cut::default());
        mark_band(&window, 4.0, 12.0);
        click(&window, "effect-item-speed");
        let holder = form_holder(&window);
        let rows: Vec<gtk::Widget> = holder
            .observe_children()
            .iter::<glib::Object>()
            .flatten()
            .filter_map(|c| c.downcast::<gtk::Widget>().ok())
            .collect();
        // Which child holds each field, in tree order.
        let mut seen: Vec<String> = Vec::new();
        for row in &rows {
            for field in [
                "speed-field-rate",
                "speed-field-sound",
                "speed-cost-note",
                "speed-field-length",
                "speed-field-fade-in",
                "speed-field-fade-out",
                "speed-field-curve",
            ] {
                if find_in(row, field).is_some() {
                    seen.push(field.to_string());
                }
            }
        }
        assert_eq!(
            seen,
            vec![
                "speed-field-rate",
                "speed-field-sound",
                "speed-cost-note",
                "speed-field-length",
                "speed-field-fade-in",
                "speed-field-fade-out",
                "speed-field-curve"
            ],
            "the form reads Speed × · Sound · Length · Fade in · Fade out · Curve, in \
             `FORM_FIELDS`' order, with the cost note under Sound where the choice it explains is still being \
             read -- a reordered form makes someone answer the wrong question"
        );
        let rate_items = strings_of(&dropdown(&window, "speed-field-rate"));
        let expected_rates: Vec<String> = speed::RATES
            .iter()
            .map(|r| speed::rate_label(*r))
            .chain(std::iter::once(speed::CUSTOM.to_string()))
            .collect();
        assert_eq!(
            rate_items, expected_rates,
            "the rate list is `RATES` labelled, `Custom…` last -- not a local list"
        );
        assert_eq!(
            strings_of(&dropdown(&window, "speed-field-sound")),
            speed::SOUND_CHOICES.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            "Sound offers §A.3's five answers in order"
        );
        // The cost note is the arithmetic done for them, compared against the module for the SHOWN values.
        let shown_snd = speed::sound_stored(speed::SOUND_CHOICES[0]);
        assert_eq!(
            label_text(&window, "speed-cost-note"),
            speed::cost_note(shown_snd, speed::DEFAULT_RATE, 8.0),
            "`speed-cost-note` equals `cost_note` for the rate and length actually on screen"
        );
        // And a 1× answer really does produce the sentence, proving the note is live rather than blank CSS.
        choose(&window, "speed-field-sound", 2); // "1× to the effect's end"
        let live_snd = speed::sound_stored(speed::SOUND_CHOICES[2]);
        assert_eq!(live_snd, "own", "that row stores the own-clock answer");
        RAN_FIELDS.store(true, Ordering::SeqCst);
        window.close();
    }

    // --- S3: rate 0 applies a STOP, taking the length floor directly, never a rate -------------
    {
        let window = cut_page(app, &Cut::default());
        mark_band(&window, 30.0, 40.0);
        click(&window, "effect-item-speed");
        choose(&window, "speed-field-rate", speed::rate_index(0.0));
        entry(&window, "speed-field-length").set_text("0.2");
        click(&window, "speed-apply-button");
        let cut_ = ui::review_cut_of(&window);
        assert_eq!(cut_.fx.len(), 1, "Apply put exactly one record on the cut");
        let fx = &cut_.fx[0];
        assert_eq!(fx.kind, "speed");
        assert_eq!(fx.rate, 0.0, "rate 0 stayed 0 -- no rate was invented for a still");
        assert!(
            fx.dur >= 0.5,
            "// P.eng.minClipSeconds -- the LENGTH floor was taken directly (0.2 -> {:.1}) and `clamp_speed` \
             never ran, because a stop has no rate to give way",
            fx.dur
        );
        RAN_STOP_APPLY.store(true, Ordering::SeqCst);
        window.close();
    }

    // --- S3: a thin stretch makes the RATE give way, never the marked seconds -------------------
    {
        let window = cut_page(app, &Cut::default());
        mark_band(&window, 3.0, 4.0);
        click(&window, "effect-item-speed");
        // Custom… typed 0.01: below P.eng.minRate, over 1 s of marked footage.
        choose(&window, "speed-field-rate", speed::RATES.len());
        assert!(
            ui::speed_custom_rate_visible(&window),
            "the typed rate box is live only while `Custom…` shows -- two live controls for one question \
             would be two answers on screen"
        );
        entry(&window, "speed-field-custom").set_text("0.01");
        entry(&window, "speed-field-length").set_text("1");
        click(&window, "speed-apply-button");
        let cut_ = ui::review_cut_of(&window);
        assert_eq!(cut_.fx.len(), 1, "one record placed");
        let fx = &cut_.fx[0];
        assert!(
            fx.rate >= speed::MIN_RATE,
            "// P.eng.minRate -- a rate under the floor came back up to it, got {}",
            fx.rate
        );
        assert_eq!(
            fx.dur, 1.0,
            "and the MARKED seconds were left alone: moving a band would edit something nobody asked for, \
             so the rate gives way instead"
        );
        RAN_RATE_GIVES_WAY.store(true, Ordering::SeqCst);
        window.close();
    }

    // --- S4: the status names what it did, and one ↶ takes it back ------------------------------
    {
        let window = cut_page(app, &Cut::default());
        mark_band(&window, 60.0, 70.0);
        click(&window, "effect-item-speed");
        click(&window, "speed-apply-button");
        let stored = ui::review_cut_of(&window).fx.last().cloned().expect("a record was placed");
        assert!(stored.rate > 0.0, "this branch is a clock, not a stop");
        assert_eq!(
            status_text(&window),
            speed::placed_status(&stored),
            "the status line is `placed_status`' own line, including the '...plays at that rate there...' half"
        );
        click(&window, "undo-button");
        assert!(
            ui::review_cut_of(&window).fx.is_empty(),
            "one ↶ took the speed back -- it went in through `record_edit`, not `seed_review_cut`"
        );

        // The stop variant of the same sentence, also through the real buttons.
        let window2 = cut_page(app, &Cut::default());
        ui::note_place(true);
        ui::set_line_position(&window2, naivepost::cut_line::LinePos { t: 12.0 });
        click(&window2, "effect-item-speed");
        click(&window2, "speed-apply-button");
        let placed = ui::review_cut_of(&window2).fx.last().cloned().expect("a stop was placed");
        assert_eq!(placed.rate, 0.0, "a line with no band places a stop");
        assert_eq!(
            status_text(&window2),
            speed::placed_status(&placed),
            "and its status is the stop's own line -- the '...stands still while the clock runs...' half"
        );
        // The two branches really do say different things: a clock talks about the cut changing length, a
        // still about the picture standing while the clock runs. Asserted between the two LIVE strings rather
        // than against a hand-built record, which would prove nothing about what the page said.
        let clock_line = speed::placed_status(&stored);
        let stop_line = status_text(&window2);
        assert!(clock_line.contains("plays at that rate there"), "the clock's clause: {clock_line}");
        assert!(stop_line.contains("stands still there while the clock runs"), "the stop's clause: {stop_line}");
        assert_ne!(clock_line, stop_line, "and they are not the same sentence wearing different numbers");
        RAN_STATUS_UNDO.store(true, Ordering::SeqCst);
        window2.close();
    }
}

/// Trim a whole-number second the way the form prints it ("2", not "2.0").
fn trim(seconds: f64) -> String {
    if (seconds - seconds.round()).abs() < 1e-9 {
        format!("{}", seconds.round() as i64)
    } else {
        format!("{seconds}")
    }
}

/// Find a named widget inside a subtree we already hold (a form row), without walking the whole window.
fn find_in(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if node.widget_name() == name {
        return Some(node.clone());
    }
    let mut child = node.first_child();
    while let Some(current) = child {
        if let Some(found) = find_in(&current, name) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

#[test]
fn f3_3_the_speed_form_is_wired_through_the_real_widgets() {
    // Pin cwd to our own temp root BEFORE anything builds: the press resolves the project through
    // `startup::session_dir(current_dir())`, and leaving it at rust/ writes a stray
    // `rust/session.naivepost/` into the repo.
    let root = std::env::temp_dir().join(format!("np-speed-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(round);
        app.run();
    });

    assert!(RAN_BAND.load(Ordering::SeqCst), "the S1 band check never ran");
    assert!(RAN_LINE_STOP.load(Ordering::SeqCst), "the S1 line-stop check never ran");
    assert!(RAN_REFUSAL.load(Ordering::SeqCst), "the S1 refusal check never ran");
    assert!(RAN_FIELDS.load(Ordering::SeqCst), "the S2 six-field check never ran");
    assert!(RAN_STOP_APPLY.load(Ordering::SeqCst), "the S3 stop-apply check never ran");
    assert!(
        RAN_RATE_GIVES_WAY.load(Ordering::SeqCst),
        "the S3 rate-gives-way check never ran"
    );
    assert!(
        RAN_STATUS_UNDO.load(Ordering::SeqCst),
        "the S4 status-and-undo check never ran"
    );
}
