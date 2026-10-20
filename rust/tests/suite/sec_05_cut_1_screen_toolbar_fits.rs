//! §05-cut#1-screen, the bar itself: every control §1 numbers **5**-**22** is ON the single toolbar row
//! at the window's own default size, not scrolled out of sight inside its group.
//!
//! `spec/05-cut.md` §1 says "Below it, top to bottom: the toolbar (one row)", and `spec/img/05-cut.png`
//! draws all eighteen controls across that one row. Each group sits in a `gtk::ScrolledWindow` so the shell
//! stays shrinkable (`tests/suite/window_shrinks.rs`), which means a group narrower than its buttons hides
//! the tail rather than widening the page — exactly what happened before this test existed: with the three
//! transport buttons labelled in words, `group-transport` asked for 1255 px, GTK handed `group-verbs` 244
//! of its 675 px, and Copy/Paste/Insert/Lane (§1 **13**-**16**) were at x=769..1194 INSIDE that scroller,
//! off the visible bar. So the check is geometric: each named control's right edge must be inside the
//! toolbar's own right edge.
//!
//! The second check pins the other half of the same fix: §A group 1 spells the transport as glyphs
//! (▶, ▶✂, ▶✂✂) with the sentence in the tooltip, which is what bought back the ~1000 px.

use adw::prelude::*;
use gtk4 as gtk;
use std::sync::atomic::{AtomicBool, Ordering};
use crate::common;
use common::{fixture_dir, hold_last_window, release_last_window, settle};

static RAN_FITS: AtomicBool = AtomicBool::new(false);
static RAN_GLYPHS: AtomicBool = AtomicBool::new(false);

fn find_named(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    fn walk(widget: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
        if widget.widget_name() == name {
            return Some(widget.clone());
        }
        for child in widget.observe_children().iter::<glib::Object>() {
            let Ok(child) = child else { continue };
            let Some(child) = child.downcast_ref::<gtk::Widget>() else { continue };
            if let Some(found) = walk(child, name) {
                return Some(found);
            }
        }
        None
    }
    walk(window.upcast_ref(), name)
}

/// A Cut page over a session with footage, sized the way `just snapshot 05-cut` sizes it: the layout
/// collapses to one column without an explicit size before `present()`.
fn cut_page(app: &adw::Application) -> adw::ApplicationWindow {
    release_last_window();
    let root = std::env::temp_dir().join(format!("cut-bar-w-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("temp root");
    std::env::set_current_dir(&root).expect("cwd pinned away from rust/");
    let session = naivepost::startup::session_dir(&root);
    let mut seed = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    seed.sources.iter_mut().for_each(|source| source.footage = true);
    std::fs::create_dir_all(&session).expect("session folder");
    naivepost::project::save(&seed, &session).expect("the session is a real project on disk");

    let window = naivepost::ui::build_window(app, &seed, "Prepare");
    hold_last_window(window.clone());
    // §1's minimum size: the toolbar must hold every control at the size the window can never go under.
    window.set_default_size(1240, 900);
    window.present();
    naivepost::ui::tab_button(&window, naivepost::shell::Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    naivepost::ui::set_session_recordings(&[naivepost::timeline::Recording {
        base: "lecture".into(),
        start: 0.0,
        end: 120.0,
    }]);
    let seeded = naivepost::cut::Cut {
        segs: vec![naivepost::cut::Seg { s: 0.0, e: 60.0, ..Default::default() }],
        ..Default::default()
    };
    naivepost::ui::seed_review_cut(&window, &seeded);
    naivepost::ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 10.0 });
    settle();
    window
}

/// §1 items **5**-**22**, by the names every flow's tests already use.
const BAR_CONTROLS: [(&str, &str); 18] = [
    ("play-recording-button", "5 \u{25b6} the recording"),
    ("play-cut-button", "6 \u{25b6}\u{2702} the cut"),
    ("review-cuts-button", "7 \u{25b6}\u{2702}\u{2702} review every cut"),
    ("line-step-back-five", "8 frame steps"),
    ("preview-volume", "9 preview volume"),
    ("add-button", "10 Add"),
    ("split-button", "11 Split"),
    ("remove-button", "12 Remove"),
    ("copy-button", "13 Copy"),
    ("paste-button", "14 Paste"),
    ("insert-button", "15 Insert"),
    ("lane-button", "16 Lane"),
    ("effect-button", "17 Effect \u{25be}"),
    ("undo-button", "18 Undo"),
    ("redo-button", "19 Redo"),
    ("revert-button", "20 Revert"),
    ("clear-cut-button", "21 Clear"),
    ("zoom-in-button", "22 zoom +"),
];

/// Every control is realized, mapped, and ends inside the bar's own right edge. Printed as numbers
/// because "clipped" is a distance: the failing line names the control and both edges.
fn check_every_control_sits_on_the_bar(app: &adw::Application) {
    let window = cut_page(app);
    let bar = find_named(&window, "cut-toolbar").expect("\u{a7}A's toolbar is on the page");
    let bar_bounds = bar
        .compute_bounds(window.upcast_ref::<gtk::Widget>())
        .expect("the toolbar has bounds");
    let bar_right = bar_bounds.x() + bar_bounds.width();
    println!(
        "BAR w={} x={:.0} right={:.0}",
        bar.width(),
        bar_bounds.x(),
        bar_right
    );
        let mut natural_by_group: std::collections::BTreeMap<String, i32> =
            std::collections::BTreeMap::new();
    let mut clipped: Vec<String> = Vec::new();
    // `compute_bounds` answers in NATIVE coordinates, which is what "inside the bar" has to mean here; a
    // widget's own x/y getters answer against its parent and would compare two different frames.
    for (name, what) in BAR_CONTROLS {
        let widget = find_named(&window, name)
            .unwrap_or_else(|| panic!("\u{a7}1 item {what} is missing from the page: `{name}`"));
        let bounds = widget
            .compute_bounds(window.upcast_ref::<gtk::Widget>())
            .unwrap_or_else(|| panic!("`{name}` ({what}) has no bounds -- never laid out"));
        let right = bounds.x() + bounds.width();
        // The group's own viewport: a control can sit inside the BAR and still paint clipped, because
        // each group is a ScrolledWindow whose row may be wider than the share the bar gave it.
        let mut owner: Option<gtk::Widget> = Some(widget.clone().upcast());
        let mut group_name = String::from("-");
        let mut group_left = f64::NAN;
        let mut group_right = f64::NAN;
        let mut row_width = 0;
        let mut natural = 0;
        while let Some(node) = owner {
            let gname = node.widget_name().to_string();
            if gname.starts_with("group-") && !gname.ends_with("-row") {
                // The VIEWPORT is the scroller inside the named box, not the box: its own width is the
                // hole the buttons show through, and the row it holds may be wider than that -- which is
                // exactly what paints a control clipped while it still sits inside the BAR.
                let scroll = node
                    .downcast_ref::<gtk::Box>()
                    .and_then(|b| b.first_child())
                    .unwrap_or_else(|| node.clone());
                let sb = scroll
                    .compute_bounds(window.upcast_ref::<gtk::Widget>())
                    .unwrap_or(bounds);
                group_name = gname;
                group_left = f64::from(sb.x());
                group_right = f64::from(sb.x() + sb.width());
                row_width = scroll
                    .downcast_ref::<gtk::ScrolledWindow>()
                    .and_then(|s| s.child())
                    .map(|c| c.width())
                    .unwrap_or(0);
                natural = scroll
                    .measure(gtk::Orientation::Horizontal, -1)
                    .1;
                break;
            }
            owner = node.parent();
        }
        println!("GROUP {name} -> {group_name} viewport={group_left:.0}..{group_right:.0} row={row_width} natural={natural}");
        natural_by_group.insert(group_name.clone(), natural);
        println!(
            "CTRL {name}: x={:.0} right={:.0} w={} mapped={} visible={}",
            bounds.x(),
            right,
            widget.width(),
            widget.is_mapped(),
            widget.is_visible()
        );
        if !widget.is_visible() {
            clipped.push(format!("{name} ({what}) is not visible"));
        } else if right > bar_right + 1.0 {
            clipped.push(format!(
                "{name} ({what}) ends at {:.0}, past the bar's right edge {:.0}",
                right, bar_right
            ));
        } else if f64::from(right) > group_right + 1.0 {
            // The check `spec/img/05-cut.png` actually asks for: a control inside the BAR but past its own
            // group's viewport is PAINTED clipped -- the scroller shows the hole, not the row.
            clipped.push(format!(
                "{name} ({what}) ends at {right:.0}, past {group_name}'s viewport {group_left:.0}..{group_right:.0}",
            ));
        }
    }
    let sum: i32 = natural_by_group.values().sum();
    // The bar's own arithmetic at §1's floor: five 4 px gaps between the six groups. This is the number
    // that decides whether every \u{a7}1 control can sit inside its own group, so it is asserted, not just
    // printed — a wider label or fatter padding that pushes it past the bar clips a control again.
    let needed = sum + 5 * 4;
    println!("SUM natural={sum} + spacing={} = {} against bar {}", 5 * 4, needed, bar.width());
    assert!(
        needed <= bar.width(),
        "\u{a7}A's one row needs {needed} px (six groups' natural widths + their gaps) but the bar is {} px \
wide at \u{a7}1's floor: some group scrolls and hides its controls — {natural_by_group:?}",
        bar.width()
    );
    assert!(
        clipped.is_empty(),
        "{} of {} \u{a7}1 controls are NOT on the visible toolbar row: {}",
        clipped.len(),
        BAR_CONTROLS.len(),
        clipped.join("; ")
    );
    RAN_FITS.store(true, Ordering::SeqCst);
    window.close();
}

/// §A group 1 spells the transport as glyphs and puts the sentence in the tooltip; that is what keeps the
/// row to one line. The table is the single source, so pinning it pins the bar.
fn check_the_transport_is_glyphs_with_the_sentence_in_the_tip(app: &adw::Application) {
    let transport = &naivepost::cut_screen::TRANSPORT_BUTTONS;
    assert_eq!(
        transport[0].label, "\u{25b6}",
        "\u{a7}1 item 5 wears the glyph \u{a7}A gives it, not a sentence"
    );
    assert_eq!(transport[1].label, "\u{25b6}\u{2702}", "\u{a7}1 item 6");
    assert_eq!(transport[2].label, "\u{25b6}\u{2702}\u{2702}", "\u{a7}1 item 7");
    // ...and the words did not disappear, they moved where \u{a7}A puts them.
    assert!(
        transport[0].tip.contains("cuts and all"),
        "the tooltip carries F2.1's sentence: {:?}",
        transport[0].tip
    );
    assert!(
        transport[1].tip.contains("the removed stretches are skipped"),
        "the tooltip carries F2.2's sentence: {:?}",
        transport[1].tip
    );
    assert!(
        transport[2].tip.contains("one join after the other"),
        "the tooltip carries F2.3's sentence: {:?}",
        transport[2].tip
    );

    // And on screen: the realized button answers with the same glyph and the same tip.
    let window = cut_page(app);
    let button = find_named(&window, "play-recording-button").expect("\u{a7}1 item 5 is on the page");
    let downcast = button.downcast::<gtk::Button>().expect("it is a button");
    assert_eq!(
        downcast.label().as_deref(),
        Some("\u{25b6}"),
        "the button on the page wears the table's glyph"
    );
    assert!(
        downcast
            .tooltip_text()
            .is_some_and(|tip| tip.contains("cuts and all")),
        "and its tooltip says what it plays"
    );
    // The four frame steps too: \u{a7}A group 1 gives them the arrows, and the word "f" would only widen
    // the row (the sentence is `FRAME_TIP`).
    for (name, glyph) in [
        ("line-step-back-five", "\u{2039}\u{2039}"),
        ("line-step-back", "\u{2039}"),
        ("line-step-forward", "\u{203a}"),
        ("line-step-forward-five", "\u{203a}\u{203a}"),
    ] {
        let step = &naivepost::cut_screen::TRANSPORT_BUTTONS
            .iter()
            .find(|tool| tool.name == name)
            .unwrap_or_else(|| panic!("\u{a7}1 item 8 names `{name}`"));
        assert_eq!(step.label, glyph, "`{name}` wears the bare arrow \u{a7}A draws");
        assert!(
            step.tip.contains("frame"),
            "`{name}` leaves which way and how far to the tooltip: {:?}",
            step.tip
        );
    }
    RAN_GLYPHS.store(true, Ordering::SeqCst);
    window.close();
}

fn round() {
    common::one_app_round(|app| {
        println!("CHECK 1 every \u{a7}1 control 5-22 is on the visible bar");
        check_every_control_sits_on_the_bar(app);
        println!("CHECK 2 the transport wears \u{a7}A's glyphs with the sentence in the tooltip");
        check_the_transport_is_glyphs_with_the_sentence_in_the_tip(app);
    });
}

#[test]
fn sec_05_cut_1_screen_toolbar_fits_every_numbered_control_on_one_row() {
    common::pin_private_cwd();
    round();
    assert!(RAN_FITS.load(Ordering::SeqCst), "check 1 did not run");
}

#[test]
fn sec_05_cut_1_screen_toolbar_fits_the_transport_wears_glyps_and_keeps_its_words_in_the_tip() {
    common::pin_private_cwd();
    round();
    assert!(RAN_GLYPHS.load(Ordering::SeqCst), "check 2 did not run");
}
