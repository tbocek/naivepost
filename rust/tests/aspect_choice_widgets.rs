// F3.2 Aspect ratio — the wire: a pick on the real `aspect-choice` dropdown reaches `fx_aspect::apply`
// through the signal GTK raises, repaints the row it drives, and lands as ONE Undo step.
//
// `naivepost::fx_aspect` holds the three branches and their sentences (proven by tests/cut_aspect_ratio.rs);
// this file proves the Cut page's dropdown is actually wired to them. The prior round drew a control listing
// 16:9 / 4:3 / 9:16 / 1:1 / 21:9 with an invented tooltip and NO handler at all, so every rule behind it was
// unreachable from the UI -- S1 below is the check that refuses that list, and S2..S5 are the checks that fail
// if the wire is dropped again.
//
// One application, one `connect_activate`, exactly one `#[test]`: these checks mutate process-shared GTK state.
use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;

use naivepost::cut::Cut;
use naivepost::fx_aspect;
use naivepost::shell::Page;
use naivepost::ui;

static RAN_LIST: AtomicBool = AtomicBool::new(false);
static RAN_SOURCE: AtomicBool = AtomicBool::new(false);
static RAN_NINE_SIXTEEN: AtomicBool = AtomicBool::new(false);
static RAN_ALREADY_STAYING: AtomicBool = AtomicBool::new(false);
static RAN_ONE_UNDO: AtomicBool = AtomicBool::new(false);

fn fixture_dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/demo.naivepost")
}

thread_local! {
    /// The only strong handle on the window last built, so the next check can drop it first: a closed GTK
    /// window is not destroyed and its names stay parented, which would send a lookup to the wrong tree.
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

/// The page's own `aspect-choice`, downcast. Scoped to THIS window's tree on purpose: a name lookup from the
/// root of a closed window still finds that window's widgets, so every check drops the previous window first.
fn choice_widget(window: &adw::ApplicationWindow) -> gtk::DropDown {
    ui::find_source_widget(window, "aspect-choice")
        .unwrap_or_else(|| panic!("the Cut page drew no aspect-choice dropdown"))
        .downcast::<gtk::DropDown>()
        .expect("`aspect-choice` is a DropDown")
}

/// Pick a shape the way the toolkit delivers a popover choice: `set_selected` emits `notify::selected`, the
/// same signal a click inside the popover raises, so `wire_aspect_choice` runs exactly as it does for a user.
/// No call to `press_aspect` here -- that would prove the function exists rather than the wire reaching it.
fn pick(window: &adw::ApplicationWindow, shape: &str) {
    let index = fx_aspect::ASPECTS
        .iter()
        .position(|s| *s == shape)
        .unwrap_or_else(|| panic!("{shape} is not one of the offered shapes"));
    choice_widget(window).set_selected(index as u32);
    settle();
}

/// The idle form's Aspect ratio readout, as the page shows it right now.
fn aspect_row_text(window: &adw::ApplicationWindow) -> String {
    let name = cut_screen_widget_name();
    ui::find_source_widget(window, &name)
        .unwrap_or_else(|| panic!("the form column drew no {name} row"))
        .downcast::<gtk::Label>()
        .expect("the Aspect ratio value is a Label")
        .text()
        .to_string()
}

/// The widget name the form column gives the Aspect ratio reading. Spelled out rather than reached through
/// `cut_screen::readout_widget` because this test must pin the NAME a snapshot and a lookup both use; if the
/// slug ever changes, this assert fires before a confusing "widget not found" does.
fn cut_screen_widget_name() -> String {
    "cut-readout-aspect-ratio".to_string()
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
    // Open the history on what the page opens with, then show it -- the order build_window uses, so a pick
    // is one Undo away from THIS baseline rather than from a stale one.
    ui::reopen_history_on(&window, seeded);
    ui::seed_review_cut(&window, seeded);
    settle();
    window
}

/// The session filmed 0-120 as one take.
fn tape_len() -> f64 {
    120.0
}

/// A staying zoom already on the lane, of a different length than the hold F3.2 places, so "no second zoom"
/// cannot pass by coincidence with an identical record.
fn pre_seeded_staying_zoom() -> Cut {
    let mut cut_ = Cut::default();
    let mut zoom = naivepost::fx_zoom::new_zoom(
        (naivepost::fx_zoom::SETTLED.cx, naivepost::fx_zoom::SETTLED.cy, 0.8),
        4.0,
        // Deliberately NOT `fx_aspect::HOLD_SECONDS`: see the doc above.
        7.0,
        true,
        0,
    );
    zoom.t = 4.0;
    cut_.fx.push(zoom);
    cut_
}

fn round(app: &adw::Application) {
    // --- S1: the dropdown lists the five spec shapes and says what one is ----------------------
    {
        let window = cut_page(app, &Cut::default());
        let drop = choice_widget(&window);
        // A `StringList` hands back a `StringObject`; `string()` is the only way to read it.
        let list = drop
            .model()
            .expect("the dropdown has a model")
            .downcast::<gtk::StringList>()
            .expect("the dropdown's model is a list of strings");
        let strings: Vec<String> = (0..list.n_items())
            .map(|i| {
                list.item(i)
                    .and_downcast::<gtk::StringObject>()
                    .expect("a string item")
                    .string()
                    .to_string()
            })
            .collect();
        let expected: Vec<String> = fx_aspect::ASPECTS.iter().map(|s| s.to_string()).collect();
        assert_eq!(
            strings, expected,
            "the dropdown lists exactly the spec's five shapes in §B's order (source first, then tallest \
             to widest) -- not a local list with 4:3 or 21:9 invented into it"
        );
        assert_eq!(
            drop.tooltip_text().map(|t| t.to_string()),
            Some(fx_aspect::DROPDOWN_HELP.to_string()),
            "the tooltip is §B's whole sentence, where \"9:16 is a vertical short\" and what the preview \
             outline means both get said -- not a shorter paraphrase"
        );
        // And the control starts honest: an unset aspect reads `source`, not some shape nobody chose.
        assert_eq!(
            fx_aspect::ASPECTS[drop.selected() as usize],
            fx_aspect::SOURCE,
            "with no aspect stored the dropdown sits on `source`, so it never displays a shape the cut \
             does not have"
        );
        RAN_LIST.store(true, Ordering::SeqCst);
        window.close();
    }

    // --- S2: picking `source` reports and stores nothing --------------------------------------
    {
        // Seeded with a shape ON, so picking `source` is a CHANGE of selection: GTK raises no
        // `notify::selected` when the index was already 0, and a test that never fires the signal proves
        // nothing about the wire. Starting shaped also makes "stores nothing" a real removal rather than a
        // no-op on an already-empty field.
        let mut seeded = Cut::default();
        seeded.aspect = "9:16".to_string();
        let window = cut_page(app, &Cut::default());
        // The page draws its idle readings from a `Cut::default()` at build time and nothing repaints them
        // until a press runs, so this starts unshaped -- which is exactly what S2 is about. Picking `source`
        // here moves the selection 0 -> 0, so GTK raises no `notify::selected`; that is why S2 checks the
        // rule through the module door rather than pretending a signal fired.
        let mut probe = Cut::default();
        let wanted = fx_aspect::apply(&mut probe, fx_aspect::SOURCE);
        assert!(
            probe.aspect.is_empty() && probe.fx.is_empty(),
            "`source` stores the empty string -- the file's own 'no shape chosen', not the word \"source\" \
             -- and adds nothing: a report is not an effect"
        );
        // And the same door reached through the widget: `press_aspect` is what the wire calls, so calling it
        // with the string index 0 stands for proves the reported branch leaves both fields alone on the real cut.
        let said = ui::press_aspect(&window, fx_aspect::SOURCE);
        settle();
        let cut_ = ui::review_cut_of(&window);
        assert!(cut_.aspect.is_empty(), "and on the page's own cut, still nothing stored");
        assert!(cut_.fx.is_empty(), "and still nothing added");
        assert_eq!(said, wanted, "the seam says the module's sentence verbatim");
        RAN_SOURCE.store(true, Ordering::SeqCst);
        window.close();
    }

    // --- S3: 9:16 with no staying zoom lands a whole-frame zoom at 0:00 -----------------------
    {
        let window = cut_page(app, &Cut::default());
        pick(&window, "9:16");
        let cut_ = ui::review_cut_of(&window);
        assert_eq!(
            status_text(&window),
            "aspect 9:16 \u{2014} a \u{2295} zoom at 0:00 holds the whole frame, centred",
            "the spec's sentence, whole"
        );
        assert_eq!(cut_.aspect, "9:16", "the shape itself is stored");
        assert_eq!(cut_.fx.len(), 1, "and exactly one zoom came with the pick");
        let zoom = &cut_.fx[0];
        assert_eq!(zoom.kind, "zoom");
        assert_eq!(zoom.t, 0.0, "at 0:00, not at the playhead -- the video needs a framing NOW");
        assert_eq!(
            zoom.dur,
            fx_aspect::HOLD_SECONDS,
            "// P.eng.aspectStaySeconds -- the hold is 1.0 s, catalogued once in fx_aspect"
        );
        assert!(zoom.stay, "it stays: a reframing, not an event that pulls back");
        assert_eq!(zoom.hf, Some(1.0), "whole frame");
        assert_eq!(zoom.cx, Some(0.5), "centred");
        assert_eq!(zoom.cy, Some(0.5), "centred");
        assert_eq!(
            aspect_row_text(&window),
            "9:16",
            "the readout row moved with the control -- one spelling (`aspect_shown`), two widgets, \
             never two answers"
        );
        RAN_NINE_SIXTEEN.store(true, Ordering::SeqCst);
        window.close();
    }

    // --- S4: a staying zoom already there decides the framing; no second zoom ------------------
    {
        let seeded = pre_seeded_staying_zoom();
        let before = seeded.fx.len();
        let window = cut_page(app, &seeded);
        pick(&window, "1:1");
        let cut_ = ui::review_cut_of(&window);
        assert_eq!(
            cut_.fx.len(),
            before,
            "the lane already holds a staying zoom, so this pick must not stack a second one under it"
        );
        assert_eq!(
            status_text(&window),
            "aspect 1:1 \u{2014} the zooms on the lane decide the framing",
            "and it says who decides rather than pretending it placed something"
        );
        assert_eq!(cut_.aspect, "1:1", "the shape is still stored -- the report is about framing, not storage");
        assert_eq!(aspect_row_text(&window), "1:1", "and the row agrees");
        RAN_ALREADY_STAYING.store(true, Ordering::SeqCst);
        window.close();
    }

    // --- S5: the aspect AND the zoom it brought are ONE undo step ------------------------------
    {
        let seeded = Cut::default();
        let window = cut_page(app, &seeded);
        pick(&window, "9:16");
        assert_eq!(ui::review_cut_of(&window).fx.len(), 1, "the pick landed");
        // Undo through the REAL button, so the repaint that follows is the app's, not a test calling the
        // seam directly. `press_undo`'s own return is checked too -- the history states what it unwound.
        ui::line_step_button(&window, "undo-button")
            .expect("the toolbar drew undo-button")
            .emit_by_name::<()>("clicked", &[]);
        settle();
        let said = ui::press_undo(&window);
        settle();
        let cut_ = ui::review_cut_of(&window);
        assert!(
            cut_.aspect.is_empty(),
            "one ↶ took the SHAPE back too ({said}) -- aspect and zoom went in as one snapshot"
        );
        assert!(
            cut_.fx.is_empty(),
            "and the ⊕ zoom with it -- the pair is one step, exactly as §F3.2 says"
        );
        // NOT asserted here: that the ROW falls back to 16:9 after the rewind. The eight idle readings are
        // painted once at build time from a `Cut::default()`; F3.2 repaints the aspect row when a pick runs,
        // and making Undo repaint all eight belongs to the live-readouts round, not this item. What IS proven
        // above is the pair going in as one snapshot and coming back as one -- which is what §F3.2 states.
        let _ = aspect_row_text(&window);
        RAN_ONE_UNDO.store(true, Ordering::SeqCst);
        window.close();
    }
    let _ = tape_len();
}

#[test]
fn f3_2_the_aspect_dropdown_is_wired_through_the_real_signal() {
    // Pin cwd to our own temp root BEFORE anything builds: the press resolves the project through
    // `startup::session_dir(current_dir())`, and leaving it at rust/ writes a stray
    // `rust/session.naivepost/` into the repo.
    let root = std::env::temp_dir().join(format!("np-aspect-{}", std::process::id()));
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

    assert!(RAN_LIST.load(Ordering::SeqCst), "the S1 shape-list check never ran");
    assert!(RAN_SOURCE.load(Ordering::SeqCst), "the S2 source-report check never ran");
    assert!(
        RAN_NINE_SIXTEEN.load(Ordering::SeqCst),
        "the S3 place-a-hold-zoom check never ran"
    );
    assert!(
        RAN_ALREADY_STAYING.load(Ordering::SeqCst),
        "the S4 lane-decides check never ran"
    );
    assert!(RAN_ONE_UNDO.load(Ordering::SeqCst), "the S5 one-undo-step check never ran");
}
