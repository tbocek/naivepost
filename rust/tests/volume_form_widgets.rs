// F3.6 Volume by hand — the wire: a click on 🔊 Volume reaches `fx_volume`'s S1–S5 through real widgets, the
// "Volume a – b" form draws its five fields in the spec's order, and Apply puts ONE record on the cut that one
// ↶ takes back.
//
// `naivepost::fx_volume` holds the rules (proven by tests/cut_volume_hand.rs, which this file does not touch);
// this proves the Cut page can actually reach them. Before F3.6's redo there was no `volume-form-*`, no
// `press_volume_item` and no Apply button at all — the dropdown row fell through `press_effect_item` into the
// generic `fx_record::record_into` path, so every rule in the module was unreachable from the UI. Every check
// here fires a widget the way GTK does (`emit_by_name("clicked")`) and compares against the module rather than
// a retyped string, so a paraphrase or a dropped wire fails loudly.
//
// One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared GTK state.
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;

use naivepost::cut::{Cut, EffectKind};
use naivepost::cut_hear;
use naivepost::cut_line::LinePos;
use naivepost::cut_select::Surface;
use naivepost::cut_speed::MIN_MARKED_SECONDS;
use naivepost::fx_lane;
use naivepost::fx_record;
use naivepost::fx_volume::{self, Pressed};
use naivepost::fx_zoom;
use naivepost::shell::Page;
use naivepost::ui;

static RAN_REFUSED: AtomicBool = AtomicBool::new(false);
static RAN_LINE: AtomicBool = AtomicBool::new(false);
static RAN_BAND: AtomicBool = AtomicBool::new(false);
static RAN_FIELDS: AtomicBool = AtomicBool::new(false);
static RAN_APPLY_UNDO: AtomicBool = AtomicBool::new(false);
static RAN_FLOOR: AtomicBool = AtomicBool::new(false);
static RAN_PAUSED: AtomicBool = AtomicBool::new(false);

const EPS: f64 = 1e-9;

fn assert_close(asked: &str, got: f64, want: f64) {
    assert!((got - want).abs() < EPS, "{asked}: got {got}, want {want}");
}

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

fn tooltip(window: &adw::ApplicationWindow, name: &str) -> String {
    widget(window, name)
        .tooltip_text()
        .map(|t| t.to_string())
        .unwrap_or_default()
}

/// The strings a dropdown offers, in order. `DropDown::model()` is an `Option<ListModel>`, so it is unwrapped
/// here once rather than at every call site.
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

/// This window's volume-form holder, and whether it is showing.
fn form_holder(window: &adw::ApplicationWindow) -> gtk::Box {
    widget(window, &format!("volume-form-{}", Page::Cut.label()))
        .downcast::<gtk::Box>()
        .expect("the volume form holder is a Box")
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

/// Fire a named button the way a press does: the `clicked` signal, then let the queued handlers run.
fn click(window: &adw::ApplicationWindow, name: &str) {
    ui::line_step_button(window, name)
        .unwrap_or_else(|| panic!("the Cut page drew no clickable {name}"))
        .emit_by_name::<()>("clicked", &[]);
    settle();
}

/// Mark a stretch on the ruler/selection ground — the same seam a drag leaves behind — so S1 has a band.
fn mark_band(window: &adw::ApplicationWindow, from: f64, to: f64) {
    let band = ui::draw_selection(window, Surface::SelectionBand, None, from, to);
    assert!(band.is_some(), "the band was drawn");
    settle();
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

/// Open the volume form over a marked band, the way a user does: mark, then press the dropdown's row.
fn open_over_band(app: &adw::Application, from: f64, to: f64) -> adw::ApplicationWindow {
    let window = cut_page(app, &Cut::default());
    mark_band(&window, from, to);
    click(&window, "effect-item-volume");
    assert!(form_visible(&window), "a marked stretch opens the volume form");
    window
}

fn volume_round(app: &adw::Application) {
    // --- S1: neither a band nor a line → the refusal, and NOTHING drawn -------------------------
    {
        let window = cut_page(app, &Cut::default());
        ui::clear_selection(&window);
        ui::note_place(false);
        click(&window, "effect-item-volume");
        assert_eq!(
            status_text(&window),
            fx_volume::NO_SECONDS,
            "the refusal is `fx_volume::NO_SECONDS` verbatim -- volume asks in a volume's words, not speed's \
             with the noun swapped"
        );
        assert!(
            !form_visible(&window),
            "and nothing was drawn: a form with no seconds to work on would invite an Apply that cannot work"
        );
        assert!(ui::volume_form_open().is_none(), "no form is held either");
        assert!(
            ui::review_cut_of(&window).fx.is_empty(),
            "the generic `fx_record::record_into` path must be unreachable for Volume: a refused press adds no \
             record"
        );
        RAN_REFUSED.store(true, Ordering::SeqCst);
        window.close();
    }

    // --- S1/S2: only a line → two seconds from it, 200 %, 0.25 s ramps --------------------------
    {
        let window = cut_page(app, &Cut::default());
        ui::clear_selection(&window);
        ui::note_place(true);
        ui::set_line_position(&window, LinePos { t: 42.0 });
        click(&window, "effect-item-volume");
        assert!(form_visible(&window), "a bare line opens the form too");
        let form = ui::volume_form_open().expect("the press left a form waiting");
        // P.policy.effectDefaultSeconds ("stop/speed/volume/label 2"): two seconds off the red line.
        assert_close("P.policy.effectDefaultSeconds", form.dur, fx_volume::LINE_SECONDS);
        assert_close("at the line", form.t, 42.0);
        // effects.defaultGain: twice as loud, printed in the field's own unit.
        assert_close("effects.defaultGain as a percent", form.percent, fx_volume::DEFAULT_GAIN * 100.0);
        assert_close("the default is 200 %", form.percent, 200.0);
        // P.policy.effectDefaultFades: volume 0.25 both ways -- a gain arriving on one sample is a click.
        assert_close("P.policy.effectDefaultFades in", form.trans, fx_volume::FADE_SECONDS);
        assert_close("...and out", form.tout, fx_volume::FADE_SECONDS);
        let title = fx_volume::form_title(42.0, fx_volume::LINE_SECONDS);
        assert_eq!(label_text(&window, "volume-heading"), title, "the heading IS the module's title");
        assert!(
            status_text(&window).starts_with(&title),
            "and the status line opens with it too, got {}",
            status_text(&window)
        );
        RAN_LINE.store(true, Ordering::SeqCst);
        window.close();
    }

    // --- S1: a marked band → the band's OWN seconds, never LINE_SECONDS -------------------------
    {
        let window = open_over_band(app, 10.0, 25.0);
        let form = ui::volume_form_open().expect("the band opened the form");
        assert_close("the band's own start", form.t, 10.0);
        assert_close("the band's own length", form.dur, 15.0);
        assert_ne!(
            form.dur,
            fx_volume::LINE_SECONDS,
            "a stretch is worked over its own seconds, not over the line default"
        );
        assert_eq!(label_text(&window, "volume-heading"), fx_volume::form_title(10.0, 15.0));
        window.close();

        // A band under every effect's floor is not a band: with a placed line the line answers instead of
        // refusing, exactly as the flowchart's fall-through draws it.
        let thin = cut_page(app, &Cut::default());
        ui::note_place(true);
        ui::set_line_position(&thin, LinePos { t: 42.0 });
        mark_band(&thin, 30.0, 30.0 + MIN_MARKED_SECONDS * 0.5);
        click(&thin, "effect-item-volume");
        assert!(form_visible(&thin), "a slipped band falls through to the line rather than refusing");
        let fell = ui::volume_form_open().expect("the line answered");
        assert_close("at the line, not at the slipped band", fell.t, 42.0);
        assert_close("the line's two seconds", fell.dur, fx_volume::LINE_SECONDS);
        RAN_BAND.store(true, Ordering::SeqCst);
        thin.close();
    }

    // --- S3: the five fields, their texts, their tooltips, in FORM_FIELDS' order -----------------
    {
        let window = open_over_band(app, 10.0, 25.0);
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
                "volume-field-percent",
                "volume-field-length",
                "volume-field-fade-in",
                "volume-field-fade-out",
                "volume-field-curve",
            ] {
                if find_in(row, field).is_some() {
                    seen.push(field.to_string());
                }
            }
        }
        assert_eq!(
            seen,
            vec![
                "volume-field-percent",
                "volume-field-length",
                "volume-field-fade-in",
                "volume-field-fade-out",
                "volume-field-curve"
            ],
            "the form reads Volume % · Length · Fade in · Fade out · Curve, in `fx_volume::FORM_FIELDS`' \
             order -- a reordered form makes someone answer the wrong question"
        );
        assert_eq!(
            seen.len(),
            fx_volume::FORM_FIELDS.len(),
            "one widget per field §A.6 names, no more and no fewer"
        );

        // The prefilled numbers are the pressed record read in each field's own unit, not literals.
        assert_eq!(
            entry(&window, "volume-field-percent").text(),
            format!("{:.0}", fx_volume::percent_of(fx_volume::DEFAULT_GAIN)),
            "the percent field shows 200 because that is what gain 2.0 IS in the field's unit"
        );
        assert_eq!(entry(&window, "volume-field-length").text(), trim(15.0), "the band's length");
        assert_eq!(
            entry(&window, "volume-field-fade-in").text(),
            trim(fx_volume::FADE_SECONDS),
            "P.policy.effectDefaultFades pre-filled"
        );
        assert_eq!(entry(&window, "volume-field-fade-out").text(), trim(fx_volume::FADE_SECONDS));

        // Tooltips are the module's sentences, whole.
        let percent_tip = tooltip(&window, "volume-field-percent");
        assert_eq!(percent_tip, fx_volume::VOLUME_HELP_FULL, "the field's tooltip IS the module's sentence");
        assert!(percent_tip.contains("1000"), "and it states where the field tops out: {percent_tip}");
        assert!(percent_tip.contains("hiss"), "...with the reason not to lean on it: {percent_tip}");
        assert_eq!(tooltip(&window, "volume-field-fade-in"), fx_volume::FADE_IN_HELP);
        assert!(
            tooltip(&window, "volume-field-fade-in").contains("hard step"),
            "nought is explained rather than forbidden"
        );

        // Curve is a dropdown over §A.1's list, owned by zoom and borrowed here.
        let curve = dropdown(&window, "volume-field-curve");
        assert_eq!(strings_of(&curve), fx_zoom::CURVE_CHOICES, "one curve today, offered as the module lists it");
        RAN_FIELDS.store(true, Ordering::SeqCst);
        window.close();
    }

    // --- S4: Apply writes ONE record, and one ↶ takes it back -----------------------------------
    {
        let window = open_over_band(app, 10.0, 25.0);
        entry(&window, "volume-field-percent").set_text("500");
        click(&window, "volume-apply-button");
        let cut_ = ui::review_cut_of(&window);
        assert_eq!(cut_.fx.len(), 1, "Apply put exactly one record on the cut");
        let fx = &cut_.fx[0];
        assert_eq!(fx.kind, "volume");
        assert_close("500 % is five times", fx.gain, 5.0);
        assert_close("over the band's own seconds", fx.t, 10.0);
        assert_close("...of its own length", fx.dur, 15.0);
        // §A.6 "No box, no drag": there is no field for a drag to have written.
        assert_eq!((fx.cx, fx.cy, fx.hf, fx.wf), (None, None, None, None));
        assert_eq!((fx.text.as_str(), fx.src.as_str()), ("", ""));
        assert!(
            fx_record::rides_whole_bed(fx),
            "an unnamed lane is the whole bed -- raising everything, which is why §A.6 gives it no lane"
        );
        assert_eq!(
            status_text(&window),
            fx_volume::placed_status(fx),
            "the status IS the module's placed_status"
        );
        assert!(
            status_text(&window).ends_with(fx_volume::PICTURE_UNTOUCHED),
            "...ending in the reassurance §F3.6 quotes: {}",
            status_text(&window)
        );
        assert!(ui::volume_form_open().is_none(), "Apply closed the form");
        assert!(!form_visible(&window), "and hid the holder");

        // F2.13's ↶: the addition went on the history via `record_edit`, never `seed_review_cut`.
        click(&window, "undo-button");
        assert!(
            ui::review_cut_of(&window).fx.is_empty(),
            "one ↶ takes the whole volume back"
        );
        RAN_APPLY_UNDO.store(true, Ordering::SeqCst);
        window.close();
    }

    // --- S4: the length floor refuses, keeping the form and the typed numbers --------------------
    {
        let window = open_over_band(app, 10.0, 25.0);
        entry(&window, "volume-field-length").set_text("0.05");
        click(&window, "volume-apply-button");
        let refused = fx_volume::apply(&fx_volume::Form {
            t: 10.0,
            percent: 200.0,
            dur: 0.05,
            trans: fx_volume::FADE_SECONDS,
            tout: fx_volume::FADE_SECONDS,
            curve: fx_zoom::CURVE_CHOICES[0].to_string(),
        })
        .expect_err("0.05 s is under effects.volumeMinSeconds, so the rule itself refuses");
        assert_eq!(
            status_text(&window),
            refused,
            "the page prints the rule's own error verbatim -- not a paraphrase of the floor"
        );
        assert!(
            ui::volume_form_open().is_some(),
            "a refused Apply keeps the form OPEN: closing it would throw away the numbers being typed"
        );
        assert!(form_visible(&window), "still on screen, still editable");
        assert!(ui::review_cut_of(&window).fx.is_empty(), "and it wrote nothing");

        // Cancel drops the form and changes nothing on the cut -- no record, no history write.
        click(&window, "volume-cancel-button");
        assert!(ui::volume_form_open().is_none(), "Cancel cleared the held form");
        assert!(!form_visible(&window), "and hid the holder");
        assert!(ui::review_cut_of(&window).fx.is_empty(), "Cancel records nothing");
        RAN_FLOOR.store(true, Ordering::SeqCst);
        window.close();
    }

    // --- S5: heard while paused, and nowhere to be seen -----------------------------------------
    {
        let window = open_over_band(app, 10.0, 25.0);
        click(&window, "volume-apply-button");
        let cut_ = ui::review_cut_of(&window);
        assert_eq!(cut_.fx.len(), 1, "the default 200 % volume is on the cut");
        let fx = &cut_.fx;

        // Mid-band, fades long done: the page asks the SAME function the playing preview asks, so a
        // parked red line gives what will play -- the only way the fades can be judged without motion.
        let mid = 17.0;
        assert_close("heard paused mid-band", ui::paused_preview_gain(&window, mid), 2.0);
        assert_eq!(
            ui::paused_preview_gain(&window, mid),
            fx_volume::heard_while_paused(fx, mid),
            "`paused_preview_gain` is `fx_volume::heard_while_paused` over THIS window's cut, nothing more"
        );
        assert_eq!(
            ui::paused_preview_gain(&window, mid),
            cut_hear::gain_under(fx, mid),
            "...which is `cut_hear::gain_under` by another name, so the preview and the render cannot grow \
             separate answers"
        );

        // Inside the ramp the number is NOT the flat gain, which is what makes the equality above a
        // statement about a live envelope rather than about a constant. With a 0.25 s ramp, half a second
        // in is past the ramp, so take a point a quarter of the way up it: 10.0625 s.
        let ramped = 10.0625;
        let at_ramp = ui::paused_preview_gain(&window, ramped);
        assert!(
            (at_ramp - 2.0).abs() > EPS,
            "inside the fade the loudness is still climbing ({at_ramp}), not yet the full 2.0 -- so the \
             paused reading tracks the envelope, not just the peak"
        );
        assert_eq!(at_ramp, cut_hear::gain_under(fx, ramped), "same function at the ramp point too");

        // And nothing to see: §A.6's "No box, no drag" is a statement about the picture.
        assert!(fx_volume::gain_has_no_visual(), "a volume is heard, never seen");
        assert!(
            !fx_lane::drawn_paused(EffectKind::Volume),
            "§06#2 lists nothing to draw for it, so pausing changes no pixel -- hence \"untouched\""
        );
        RAN_PAUSED.store(true, Ordering::SeqCst);
        window.close();
    }

    // Keep the imports honest about what the round exercised, and prove the refusal branch shape.
    assert_eq!(fx_volume::press(None, None), Pressed::Refused);
}

#[test]
fn f3_6_the_volume_form_is_wired_through_the_real_widgets() {
    // Pin cwd to our own temp root BEFORE anything builds: the press resolves the project through
    // `startup::session_dir(current_dir())`, and leaving it at rust/ writes a stray
    // `rust/session.naivepost/` into the repo.
    let root = std::env::temp_dir().join(format!("np-volume-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(volume_round);
        app.run();
    });

    assert!(RAN_REFUSED.load(Ordering::SeqCst), "the S1 refusal check never ran");
    assert!(RAN_LINE.load(Ordering::SeqCst), "the S1/S2 line-branch check never ran");
    assert!(RAN_BAND.load(Ordering::SeqCst), "the S1 band-branch check never ran");
    assert!(RAN_FIELDS.load(Ordering::SeqCst), "the S3 five-field check never ran");
    assert!(
        RAN_APPLY_UNDO.load(Ordering::SeqCst),
        "the S4 apply-and-undo check never ran"
    );
    assert!(RAN_FLOOR.load(Ordering::SeqCst), "the S4 length-floor check never ran");
    assert!(RAN_PAUSED.load(Ordering::SeqCst), "the S5 heard-while-paused check never ran");
}
