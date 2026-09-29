// F3.7 Label by hand — the wire: a click on 🏷 Label reaches `fx_label`'s S1–S5 through real widgets, the
// "Label at m:ss" form draws its two fields in the spec's order, an empty name is refused without placing
// anything, and Apply puts ONE mark on the cut that one ↶ takes back.
//
// `naivepost::fx_label` holds the rules (proven by tests/cut_label_hand.rs, which this file does not touch);
// this proves the Cut page can actually reach them. Before F3.7's redo there was no `label-form-*`, no
// `press_label_item` and no Apply button at all — the dropdown row fell through `press_effect_item` into the
// generic `fx_record::record_into` path, which laid down an UNNAMED record and so skipped both of §F3.7's
// refusals and the whole form. Every check here fires a widget the way GTK does
// (`emit_by_name("clicked")`) and compares against the module rather than a retyped string, so a paraphrase or
// a dropped wire fails loudly.
//
// One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared GTK state.
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;

use naivepost::cut::{Cut, EffectKind};
use naivepost::cut_line::LinePos;
use naivepost::cut_select::Surface;
use naivepost::cut_speed::MIN_MARKED_SECONDS;
use naivepost::fx_label::{self, Pressed};
use naivepost::fx_lane;
use naivepost::fx_record;
use naivepost::shell::Page;
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{fixture_dir, hold_last_window, release_last_window, settle, status_text, entry, widget};

static RAN_REFUSED: AtomicBool = AtomicBool::new(false);
static RAN_LINE: AtomicBool = AtomicBool::new(false);
static RAN_BAND: AtomicBool = AtomicBool::new(false);
static RAN_FIELDS: AtomicBool = AtomicBool::new(false);
static RAN_NO_NAME: AtomicBool = AtomicBool::new(false);
static RAN_FLOOR: AtomicBool = AtomicBool::new(false);
static RAN_APPLY_UNDO: AtomicBool = AtomicBool::new(false);
static RAN_TAG: AtomicBool = AtomicBool::new(false);

const EPS: f64 = 1e-9;

fn assert_close(asked: &str, got: f64, want: f64) {
    assert!((got - want).abs() < EPS, "{asked}: got {got}, want {want}");
}

/// Let the main context run what the widget emissions queued.

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

/// This window's label-form holder, and whether it is showing.
fn form_holder(window: &adw::ApplicationWindow) -> gtk::Box {
    widget(window, &format!("label-form-{}", Page::Cut.label()))
        .downcast::<gtk::Box>()
        .expect("the label form holder is a Box")
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
    hold_last_window(window.clone());
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

/// A widget somewhere in THIS window's tree, found from the window itself rather than from any global slot —
/// the F2.12 lesson: a tree-wide search can land on another window's copy because a closed window survives.
fn widget_in(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    fn walk(node: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if node.widget_name() == name {
            return Some(node.clone());
        }
        let mut child = node.first_child();
        while let Some(current) = child {
            if let Some(found) = walk(&current, name) {
                return Some(found);
            }
            child = current.next_sibling();
        }
        None
    }
    walk(window.upcast_ref::<gtk::Widget>(), name)
}

/// Does `holder` directly parent the named widget? A direct-child test, so a match elsewhere in the tree
/// cannot stand in for the structural claim being made.
fn holds(holder: &gtk::Widget, name: &str) -> bool {
    let mut child = holder.first_child();
    while let Some(node) = child {
        if node.widget_name() == name {
            return true;
        }
        child = node.next_sibling();
    }
    false
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

/// Open the label form over a marked band, the way a user does: mark, then press the dropdown's row.
fn open_over_band(app: &adw::Application, from: f64, to: f64) -> adw::ApplicationWindow {
    let window = cut_page(app, &Cut::default());
    mark_band(&window, from, to);
    click(&window, "effect-item-label");
    assert!(form_visible(&window), "a marked stretch opens the label form");
    window
}

fn label_round(app: &adw::Application) {
    // --- S1: neither a band nor a line → the refusal, and NOTHING drawn -------------------------
    {
        let window = cut_page(app, &Cut::default());
        ui::clear_selection(&window);
        ui::note_place(false);
        click(&window, "effect-item-label");
        assert_eq!(
            status_text(&window),
            fx_label::NO_SECONDS,
            "the refusal is `fx_label::NO_SECONDS` verbatim -- a label asks in a label's words, not volume's \
             or speed's with the noun swapped"
        );
        assert!(
            !form_visible(&window),
            "and nothing was drawn: a form with no moment to name would invite an Apply that cannot work"
        );
        assert!(ui::label_form_open().is_none(), "no form is held either");
        assert!(
            ui::review_cut_of(&window).fx.is_empty(),
            "the generic `fx_record::record_into` path must be unreachable for Label: a refused press adds no \
             record, least of all an unnamed one"
        );
        RAN_REFUSED.store(true, Ordering::SeqCst);
        window.close();
    }

    // --- S1/S2: only a line → the mark starts there, two seconds wide, UNNAMED ------------------
    {
        let window = cut_page(app, &Cut::default());
        ui::clear_selection(&window);
        ui::note_place(true);
        ui::set_line_position(&window, LinePos { t: 42.0 });
        click(&window, "effect-item-label");
        assert!(form_visible(&window), "a bare line opens the form too");
        let form = ui::label_form_open().expect("the press left a form waiting");
        assert_close("at the line", form.t, 42.0);
        // P.policy.effectDefaultSeconds ("stop/speed/volume/label 2"): two seconds off the red line.
        assert_close("P.policy.effectDefaultSeconds", form.dur, fx_label::LINE_SECONDS);
        assert_eq!(
            form.name, "",
            "the name arrives EMPTY -- it is the question the form asks, and §F3.7 refuses an unnamed mark"
        );
        let title = fx_label::form_title(42.0);
        assert_eq!(label_text(&window, "label-heading"), title, "the heading IS the module's title");
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
        let form = ui::label_form_open().expect("the band opened the form");
        assert_close("the band's own start", form.t, 10.0);
        assert_close("the band's own length", form.dur, 15.0);
        assert_ne!(
            form.dur,
            fx_label::LINE_SECONDS,
            "a stretch is marked over its own seconds, not over the line default"
        );
        assert_eq!(label_text(&window, "label-heading"), fx_label::form_title(10.0));
        window.close();

        // A band under every effect's floor is not a band: with a placed line the line answers instead of
        // refusing, exactly as the flowchart's fall-through draws it.
        let thin = cut_page(app, &Cut::default());
        ui::note_place(true);
        ui::set_line_position(&thin, LinePos { t: 42.0 });
        mark_band(&thin, 30.0, 30.0 + MIN_MARKED_SECONDS * 0.5);
        click(&thin, "effect-item-label");
        assert!(form_visible(&thin), "a slipped band falls through to the line rather than refusing");
        let fell = ui::label_form_open().expect("the line answered");
        assert_close("at the line, not at the slipped band", fell.t, 42.0);
        assert_close("the line's two seconds", fell.dur, fx_label::LINE_SECONDS);
        RAN_BAND.store(true, Ordering::SeqCst);
        thin.close();
    }

    // --- S3: the two fields, their tooltips, their order ----------------------------------------
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
            for field in ["label-field-name", "label-field-length"] {
                if find_in(row, field).is_some() {
                    seen.push(field.to_string());
                }
            }
        }
        assert_eq!(
            seen,
            vec!["label-field-name", "label-field-length"],
            "the form reads Name · Length (s), in `fx_label::FORM_FIELDS`' order -- a reordered form makes \
             someone answer the wrong question"
        );
        assert_eq!(
            seen.len(),
            fx_label::FORM_FIELDS.len(),
            "one widget per field §A.7 names: a label has no fades, no curve and no box to ask about"
        );

        // Tooltips are the module's sentences, whole -- including the promise that placing one is safe.
        let name_tip = tooltip(&window, "label-field-name");
        assert_eq!(name_tip, fx_label::NAME_HELP, "the Name tooltip IS the module's sentence");
        assert!(name_tip.contains("brief"), "and it says where the name goes: {name_tip}");
        assert!(name_tip.contains("changes nothing"), "...and that placing one is safe: {name_tip}");
        assert_eq!(
            tooltip(&window, "label-field-length"),
            fx_label::LENGTH_HELP,
            "the Length tooltip IS the module's wording"
        );

        // The name starts empty and the field itself caps at the tag's budget.
        assert_eq!(entry(&window, "label-field-name").text(), "", "prefilled with nothing: the name is asked for");
        assert_eq!(
            entry(&window, "label-field-name").max_length(),
            fx_label::NAME_MAX_CHARS as i32,
            "GTK enforces the lane tag's {}-char budget at the keyboard too",
            fx_label::NAME_MAX_CHARS
        );
        // The heading is the module's title for the band's own second.
        assert_eq!(label_text(&window, "label-heading"), fx_label::form_title(10.0));
        RAN_FIELDS.store(true, Ordering::SeqCst);
        window.close();
    }

    // --- S2: the empty NAME is refused, and the form stays open ---------------------------------
    {
        let window = open_over_band(app, 10.0, 25.0);
        click(&window, "label-apply-button");
        assert_eq!(
            status_text(&window),
            fx_label::NO_NAME,
            "an empty name answers nothing, so nothing is placed -- §A.7's sentence verbatim"
        );
        assert_eq!(
            status_text(&window),
            fx_label::apply(&fx_label::Form { t: 10.0, name: String::new(), dur: 15.0 }).unwrap_err(),
            "the page prints the rule's own error, not a second wording of it"
        );
        assert!(
            ui::label_form_open().is_some(),
            "a refused Apply keeps the form OPEN: closing it would throw away the moment being named"
        );
        assert!(form_visible(&window), "still on screen, still editable");
        assert!(ui::review_cut_of(&window).fx.is_empty(), "\"nothing is placed until then\" means NOTHING");

        // Whitespace is not a name either: it would draw an empty tag.
        entry(&window, "label-field-name").set_text("   ");
        click(&window, "label-apply-button");
        assert_eq!(status_text(&window), fx_label::NO_NAME, "spaces are trimmed away and leave no name");
        assert!(ui::review_cut_of(&window).fx.is_empty(), "and still nothing was placed");
        RAN_NO_NAME.store(true, Ordering::SeqCst);
        window.close();
    }

    // --- S4: the length floor refuses, keeping the form and the typed name ----------------------
    {
        let window = open_over_band(app, 10.0, 25.0);
        entry(&window, "label-field-name").set_text("boss fight");
        entry(&window, "label-field-length").set_text("0.3");
        click(&window, "label-apply-button");
        let refused = fx_label::apply(&fx_label::Form {
            t: 10.0,
            name: "boss fight".into(),
            dur: 0.3,
        })
        .expect_err("0.3 s is under effects.labelMinSeconds, so the rule itself refuses");
        assert_eq!(
            status_text(&window),
            refused,
            "the page prints the rule's own error verbatim -- not a paraphrase of the floor"
        );
        assert!(
            ui::label_form_open().is_some(),
            "the form stays open with the name still typed in it"
        );
        assert_eq!(entry(&window, "label-field-name").text(), "boss fight", "the name was not thrown away");
        assert!(ui::review_cut_of(&window).fx.is_empty(), "and it wrote nothing");
        RAN_FLOOR.store(true, Ordering::SeqCst);
        window.close();
    }

    // --- S5: Apply marks ONE moment, and one ↶ takes it back -----------------------------------
    {
        let window = open_over_band(app, 10.0, 25.0);
        entry(&window, "label-field-name").set_text("the reveal");
        entry(&window, "label-field-length").set_text("3");
        click(&window, "label-apply-button");
        let cut_ = ui::review_cut_of(&window);
        assert_eq!(cut_.fx.len(), 1, "Apply put exactly one mark on the cut");
        let fx = &cut_.fx[0];
        assert_eq!(fx.kind, "label");
        // The name lives in `text`, the same key a caption's words use (§1's note on the shared field).
        assert_eq!(fx.text, "the reveal");
        assert_close("over the band's own second", fx.t, 10.0);
        assert_close("...of its own length", fx.dur, 3.0);
        // Nothing else on the record: no fades, no ease, no box, no lane.
        assert_eq!((fx.trans, fx.tout, fx.ease.as_str()), (0.0, 0.0, ""), "a label fades nothing");
        assert_eq!((fx.cx, fx.cy, fx.hf, fx.wf), (None, None, None, None), "and draws no box");
        assert_eq!(fx.lane, "");
        for denied in [fx_record::Field::Trans, fx_record::Field::Cx, fx_record::Field::Gain] {
            assert!(!fx_record::uses(EffectKind::Label, denied), "§1 denies a label {denied:?}");
        }
        assert_eq!(
            status_text(&window),
            fx_label::placed_status(fx),
            "the status IS the module's placed_status"
        );
        assert!(
            status_text(&window).contains("nothing changes in the video; the narration is told about it"),
            "...ending in the reassurance §A.7 quotes: {}",
            status_text(&window)
        );
        assert!(ui::label_form_open().is_none(), "Apply closed the form");
        assert!(!form_visible(&window), "and hid the holder");

        // F2.13's ↶: the mark went on the history via `record_edit`, never `seed_review_cut`.
        click(&window, "undo-button");
        assert!(
            ui::review_cut_of(&window).fx.is_empty(),
            "one ↶ takes the whole mark back"
        );
        RAN_APPLY_UNDO.store(true, Ordering::SeqCst);
        window.close();
    }

    // --- S5: the tag is DRAWN on the lane, and Cancel leaves everything alone -------------------
    {
        let window = open_over_band(app, 10.0, 25.0);
        entry(&window, "label-field-name").set_text("boss fight");
        click(&window, "label-apply-button");
        // The lane drew it: a bar in the row `refresh_effects_lane` laid down for it.
        let row = widget_in(&window, "fx-lane-row-0").expect("row 0 exists after the mark");
        assert!(
            holds(&row, "fx-bar-0"),
            "the new mark's bar is a CHILD of the row it was laid in, not packed loose in the lane"
        );
        let bar = widget_in(&window, "fx-bar-0").expect("fx-bar-0 exists");
        assert!(
            bar.has_css_class("fx-kind-label"),
            "and carries the kind class the lane's CSS colours a label tag by"
        );
        // Drawn while paused -- a label IS visible to be judged -- yet never rendered into the video.
        assert!(fx_lane::drawn_paused(EffectKind::Label), "§A.7's grey-white tag shows on the lane");
        assert!(fx_label::never_rendered(&cut_marker()), "but the render gets no filter from it");
        RAN_TAG.store(true, Ordering::SeqCst);
        window.close();
    }

    // Cancel on a freshly opened form writes nothing and marks nothing.
    {
        let window = open_over_band(app, 10.0, 25.0);
        entry(&window, "label-field-name").set_text("abandoned idea");
        click(&window, "label-cancel-button");
        assert!(ui::label_form_open().is_none(), "Cancel cleared the held form");
        assert!(!form_visible(&window), "and hid the holder");
        assert!(ui::review_cut_of(&window).fx.is_empty(), "Cancel records nothing");
        assert_eq!(status_text(&window), "left as it was \u{2014} nothing marked");
        window.close();
    }

    // Keep the imports honest about what the round exercised, and prove the refusal branch shape.
    assert_eq!(fx_label::press(None, None), Pressed::Refused);
}

/// A placed mark for the render-side claim, built by the rule rather than hand-rolled.
fn cut_marker() -> naivepost::cut::Fx {
    fx_label::apply(&fx_label::Form { t: 10.0, name: "boss fight".into(), dur: 2.0 }).unwrap()
}

#[test]
fn f3_7_the_label_form_is_wired_through_the_real_widgets() {
    // Pin cwd to our own temp root BEFORE anything builds: the press resolves the project through
    // `startup::session_dir(current_dir())`, and leaving it at rust/ writes a stray
    // `rust/session.naivepost/` into the repo.
    let root = std::env::temp_dir().join(format!("np-label-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");

    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(label_round);
        app.run();
    });

    assert!(RAN_REFUSED.load(Ordering::SeqCst), "the S1 refusal check never ran");
    assert!(RAN_LINE.load(Ordering::SeqCst), "the S1/S2 line-branch check never ran");
    assert!(RAN_BAND.load(Ordering::SeqCst), "the S1 band-branch check never ran");
    assert!(RAN_FIELDS.load(Ordering::SeqCst), "the S3 two-field check never ran");
    assert!(RAN_NO_NAME.load(Ordering::SeqCst), "the S2 empty-name refusal check never ran");
    assert!(RAN_FLOOR.load(Ordering::SeqCst), "the S4 length-floor check never ran");
    assert!(
        RAN_APPLY_UNDO.load(Ordering::SeqCst),
        "the S5 apply-and-undo check never ran"
    );
    assert!(RAN_TAG.load(Ordering::SeqCst), "the S5 tag-drawn check never ran");
}
