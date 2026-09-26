//! §05-cut#1-screen — the Cut page's view rules.
//!
//! Which button is greyed, what it says when hovered, where one wheel motion goes and how far the
//! two ladders may run: all of it is answered in [`naivepost::cut_screen`], so all of it is checked
//! here without a window. The widgets themselves are the page's business; they hold no rule
//! (spec/00-principles.md §5).

use naivepost::cut::{Cut, Seg};
use naivepost::cut_screen::{self as cut_screen, Preview, Wheel, WheelOver};

/// A kept stretch of footage — what §A counts as a clip.
fn footage(start: f64, end: f64) -> Seg {
    Seg { s: start, e: end, ..Default::default() }
}

#[test]
fn sec_05_cut_1_screen_s1_the_toolbar_groups_are_six_and_in_spec_order() {
    // §1: "Toolbar groups (left to right): transport …; preview volume; verbs …; effects dropdown
    // …; history …; zoom". The window builds one box per entry, so this list is that order.
    assert_eq!(cut_screen::TOOLBAR_GROUPS, ["transport", "volume", "verbs", "effects", "history", "zoom"]);
}

#[test]
fn sec_05_cut_1_screen_s2_the_two_cut_buttons_are_greyed_until_there_is_a_cut() {
    // ▶✂ is "sensitive when segs > 0"; ▶✂✂ "sensitive with ≥2 clips"; ▶ is "(never greyed)".
    let empty = Cut::default();
    assert!(!cut_screen::can_play_cut(&empty));
    assert!(!cut_screen::can_review(&empty));

    let one = Cut { segs: vec![footage(0.0, 10.0)], ..Default::default() };
    assert!(cut_screen::can_play_cut(&one), "one scene is still a cut to play");
    assert!(!cut_screen::can_review(&one), "one scene has no join to review");

    let two = Cut { segs: vec![footage(0.0, 10.0), footage(20.0, 30.0)], ..Default::default() };
    assert!(cut_screen::can_play_cut(&two));
    assert!(cut_screen::can_review(&two));

    // A card is not a clip: reviewing it would play one still and call the tour done.
    let with_card = Cut {
        segs: vec![footage(0.0, 10.0), Seg { ins: "card.svg".into(), dur: 3.0, ..Default::default() }],
        ..Default::default()
    };
    assert!(cut_screen::can_play_cut(&with_card));
    assert!(!cut_screen::can_review(&with_card), "an insert is not a second clip");

    assert!(cut_screen::play_recording_sensitive());
}

#[test]
fn sec_05_cut_1_screen_s3_the_lit_state_follows_what_the_preview_is_doing() {
    // ▶✂ is "lit while cutOnly and no review"; ▶✂✂ is lit while the review runs.
    let cut_running = Preview { cut_only: true, reviewing: false };
    assert!(cut_screen::play_cut_lit(&cut_running));
    assert!(!cut_screen::review_lit(&cut_running));

    let reviewing = Preview { cut_only: false, reviewing: true };
    assert!(cut_screen::review_lit(&reviewing));
    assert!(!cut_screen::play_cut_lit(&reviewing), "nothing is running but the review");

    // The review skips as ▶✂ does, so it holds the light while it runs.
    let both = Preview { cut_only: true, reviewing: true };
    assert!(cut_screen::review_lit(&both));
    assert!(!cut_screen::play_cut_lit(&both), "two lit play buttons read as two previews");

    assert!(!cut_screen::play_cut_lit(&Preview::default()));
    assert!(!cut_screen::review_lit(&Preview::default()));
}

#[test]
fn sec_05_cut_1_screen_s4_the_wheel_steps_frames_over_the_transport_and_the_picture() {
    // "Wheel over the transport bar **and over the preview picture** steps frames (Shift = 5)".
    assert_eq!(cut_screen::frame_step(false), 1);
    assert_eq!(cut_screen::frame_step(true), 5);

    for over in [WheelOver::Transport, WheelOver::Preview] {
        assert_eq!(cut_screen::wheel(over, false, 1.0, 0.0, 1000.0), Wheel::StepFrames { frames: 1 });
        assert_eq!(cut_screen::wheel(over, false, -1.0, 0.0, 1000.0), Wheel::StepFrames { frames: -1 });
        assert_eq!(cut_screen::wheel(over, true, 1.0, 0.0, 1000.0), Wheel::StepFrames { frames: 5 });
        assert_eq!(cut_screen::wheel(over, true, -2.0, 0.0, 1000.0), Wheel::StepFrames { frames: -5 });
    }

    // Nothing moved is nothing stepped — a wheel that reports an event it did not make.
    assert_eq!(cut_screen::wheel(WheelOver::Transport, false, 0.0, 0.0, 1000.0), Wheel::StepFrames { frames: 0 });
}

#[test]
fn sec_05_cut_1_screen_s5_the_wheel_zooms_around_the_cursor_and_pans_an_eighth() {
    // "over the tracks it zooms around the cursor; Shift+wheel or a trackpad sideways swipe pans an
    // eighth of the view".
    let view = 1000.0;
    assert_eq!(
        cut_screen::wheel(WheelOver::Tracks, false, 1.0, 0.0, view),
        Wheel::Zoom { factor: 1.0 / cut_screen::ZOOM_STEP, at_x: 0.0 }
    );
    assert_eq!(
        cut_screen::wheel(WheelOver::Tracks, false, -1.0, 0.0, view),
        Wheel::Zoom { factor: cut_screen::ZOOM_STEP, at_x: 0.0 }
    );

    let expected = cut_screen::pan_px(view);
    assert_eq!(expected, 125.0, "an eighth of the view");
    for swipe in [
        cut_screen::wheel(WheelOver::Tracks, true, 1.0, 0.0, view),
        cut_screen::wheel(WheelOver::Tracks, false, 0.0, 3.0, view),
    ] {
        match swipe {
            Wheel::Pan { px } => assert_eq!(px.abs(), expected),
            other => panic!("a sideways gesture pans, not {other:?}"),
        }
    }

    // Zooming at the cursor keeps the second under it: what you were looking at stays there.
    let (pps, origin) = cut_screen::zoom_around(4.0, cut_screen::ZOOM_STEP, 400.0, 100.0, 1.0);
    let before = (400.0 - 100.0) / 4.0;
    let after = (400.0 - origin) / pps;
    assert!((before - after).abs() < 1e-9, "{before} moved to {after}");
    assert!(pps > 4.0, "zooming in is more px per second");

    // The two ladders stop where §A says: the floor is what fits, the ceiling is 240 px/s.
    let floor = cut_screen::fit_pps(600.0, 1200.0);
    let mut pps = cut_screen::ZOOM_AT_OPEN;
    for _ in 0..40 {
        pps = cut_screen::zoom_down(pps, floor);
    }
    assert_eq!(pps, floor, "zooming out stops where the whole session is on screen");
    let mut pps = cut_screen::ZOOM_AT_OPEN;
    for _ in 0..80 {
        pps = cut_screen::zoom_up(pps, floor);
    }
    assert_eq!(pps, cut_screen::ZOOM_MAX, "240 px a second is the deepest zoom");

    // An empty session still has a timeline: the floor is a number that draws.
    assert_eq!(cut_screen::fit_pps(0.0, 1200.0), cut_screen::ZOOM_AT_OPEN);
}

// --- S11-S14: the toolbar and form-column tables (§1 items 4, 10-22) ----------------------------
//
// The page assembles its controls from these tables, so the tables are where the ORDER lives — and an
// order is a rule. What is checked here is that each group holds what §1 lists, in the order it lists
// it, spelled the one way the rest of the code spells it.

/// The verb run's names, §1 items 10 → 15.
fn verb_names() -> Vec<&'static str> {
    cut_screen::VERB_BUTTONS.iter().map(|tool| tool.name).collect()
}

#[test]
fn sec_05_cut_1_screen_s11_the_seven_verbs_are_in_spec_order_with_insert_between_paste_and_lane() {
    // §1: "10 Add · 11 Split · 12 Remove · 13 Copy · 14 Paste · 15 Insert · 16 Lane" and §A's
    // linked(＋ Add, | Split, － Remove, ⧉ Copy, ⧉ Paste, Insert/Edit, ⇲ Lane).
    assert_eq!(
        verb_names(),
        [
            "add-button",
            "split-button",
            "remove-button",
            "copy-button",
            "paste-button",
            "insert-button",
            "lane-button",
        ],
        "seven verbs, and Insert sits between Paste and Lane"
    );

    // One spelling of each name: the three F2.7 verbs come from cut_verbs and the three F2.9 ones from
    // cut_copy, so this table cannot invent a fourth spelling of a control those modules already own.
    let verbs: Vec<&str> = cut_screen::VERB_BUTTONS[..3].iter().map(|t| t.name).collect();
    assert_eq!(
        verbs,
        naivepost::cut_verbs::BUTTONS.iter().map(|(n, _, _)| *n).collect::<Vec<_>>(),
        "Add/Split/Remove keep cut_verbs' names"
    );
    let copies: Vec<&str> =
        vec![cut_screen::VERB_BUTTONS[3], cut_screen::VERB_BUTTONS[4], cut_screen::VERB_BUTTONS[6]]
            .iter()
            .map(|t| t.name)
            .collect();
    assert_eq!(
        copies,
        naivepost::cut_copy::BUTTONS.iter().map(|(n, _, _)| *n).collect::<Vec<_>>(),
        "Copy/Paste/Lane keep cut_copy's names"
    );

    // Insert is the one control no other module owns; its label and tooltip are its own.
    assert_eq!(cut_screen::INSERT_TOOL.name, "insert-button");
    assert_eq!(cut_screen::INSERT_TOOL.label, "Insert");
    assert!(!cut_screen::INSERT_TOOL.tip.is_empty(), "a bare word on a button says nothing about what it opens");
}

#[test]
fn sec_05_cut_1_screen_s12_the_effects_dropdown_holds_the_six_kinds_the_page_can_add() {
    // §1 item 17: "✚ Effect ▾: ⊕ Zoom, ❝ Text, ▨ SVG, ⏩ Speed, 🔊 Volume, 🏷 Label".
    assert_eq!(cut_screen::EFFECT_ITEMS.len(), 6);
    for item in &cut_screen::EFFECT_ITEMS {
        // The third field is the kind id, and a dropdown entry must not offer what the cut cannot hold:
        // `Fx::effect_kind` answers None for anything EffectKind::parse does not know.
        let fx = naivepost::cut::Fx { kind: item.tip.to_string(), ..Default::default() };
        assert!(fx.effect_kind().is_some(), "{} offers kind {:?}, which the cut cannot store", item.name, item.tip);
        assert!(item.name.starts_with("effect-item-"), "dropdown entries share one name prefix: {}", item.name);
    }
    // Exactly the six kinds, no duplicates and no seventh: a stop is folded into Speed by EffectKind,
    // so listing one here would be a second door to the same record.
    let kinds: Vec<&str> = cut_screen::EFFECT_ITEMS.iter().map(|i| i.tip).collect();
    let unique: std::collections::HashSet<&str> = kinds.iter().copied().collect();
    assert_eq!(unique.len(), 6, "six distinct kinds: {kinds:?}");
    assert_eq!(
        kinds,
        ["zoom", "text", "svg", "speed", "volume", "label"],
        "in the order §1 writes them"
    );
}

#[test]
fn sec_05_cut_1_screen_s13_undo_and_redo_grey_with_nothing_to_do_and_revert_with_no_edit() {
    // HISTORY_BUTTONS order: Undo, Redo, Revert, Clear.
    // P.layout.undoDepth 50 — cut::UNDO_DEPTH bounds how far either count can grow.
    let [undo, redo, revert, clear] = cut_screen::history_buttons_enabled(0, 0, false);
    assert!(!undo, "nothing below the pointer to take back");
    assert!(!redo, "nothing above it to put back");
    assert!(!revert, "the screen IS the base, so there is nothing to revert to");
    assert!(!clear, "and nothing of the person's own to clear away");

    let [undo, redo, revert, clear] = cut_screen::history_buttons_enabled(2, 0, true);
    assert!(undo, "two states behind: Undo is live");
    assert!(!redo, "the branch ahead was thrown away by the last new edit");
    assert!(revert && clear, "there is hand work on the timeline to drop");

    let [undo, redo, revert, _clear] = cut_screen::history_buttons_enabled(0, 1, true);
    assert!(!undo, "at the bottom of the stack");
    assert!(redo, "one state ahead: Redo is live again");
    assert!(revert, "Revert does not care where the pointer is, only that there is something to lose");

    // The counts travel with the real stack: after N recorded edits the pointer sits at the top, so
    // undo_left is everything but the current state and redo_left is nought. `History::can_undo` /
    // `can_redo` are the same facts the table recomputes from the counts, which is what ties them.
    let mut history = naivepost::cut::History::open(&Cut::default());
    let mut edited = Cut::default();
    edited.segs.push(footage(0.0, 5.0));
    history.push(&edited);
    history.push(&edited);
    let depth = history.depth();
    let [undo, redo, _, _] = cut_screen::history_buttons_enabled(depth - 1, 0, true);
    assert!(undo && !redo, "freshly-edited page: Undo live, Redo dead (depth {depth})");
    assert_eq!(history.can_undo(), undo, "the count agrees with the stack's own answer");
    assert_eq!(history.can_redo(), redo, "and so does the one ahead");
    history.undo();
    // One step back from the top of a three-snapshot stack: one state below the pointer and one above,
    // which is what the two live doors are counting. Read off `can_undo`/`can_redo` rather than the
    // private pointer — the table takes counts, the stack answers the same question about itself.
    let [undo, redo, _, _] = cut_screen::history_buttons_enabled(1, 1, true);
    assert!(undo && redo, "one step back from the top, both doors open");
    assert_eq!(history.can_undo(), undo, "the stack agrees there is something behind");
    assert_eq!(history.can_redo(), redo, "and something ahead");
    // And the depth never outruns the catalogue's bound.
    assert!(depth <= naivepost::cut::UNDO_DEPTH + 1, "// P.layout.undoDepth {}", naivepost::cut::UNDO_DEPTH);
}

#[test]
fn sec_05_cut_1_screen_s14_the_idle_form_reads_eight_things_in_the_order_the_spec_lists() {
    // §1 item 4: "readings: playhead, selection, cut, cut at 1×, source, segments" plus §A's two
    // leading rows (thumbnail size, aspect) — eight in all.
    assert_eq!(
        cut_screen::IDLE_FORM_ROWS,
        [
            "Thumbnails",
            "Aspect ratio",
            "Playhead",
            "Selection",
            "Cut",
            "Cut at 1\u{00d7}",
            "Source",
            "Segments",
        ]
    );
    assert_eq!(cut_screen::IDLE_FORM_ROWS.len(), 8);

    // The list describes what idle_readouts actually emits, in the same order: no drift between the
    // row set the page builds and the values it fills them with.
    let cut = Cut { segs: vec![footage(0.0, 10.0)], ..Default::default() };
    let rows = cut_screen::idle_readouts(64, "", 3.0, None, &cut, 12.0);
    let emitted: Vec<&str> = rows.iter().map(|row| row.label).collect();
    assert_eq!(emitted, cut_screen::IDLE_FORM_ROWS.to_vec(), "eight rows, this order");

    // Every row gets its own widget name off its label, so a label is spelled once and the name follows.
    let names: Vec<String> =
        cut_screen::IDLE_FORM_ROWS.iter().map(|l| cut_screen::readout_widget(l)).collect();
    let unique: std::collections::HashSet<&str> = names.iter().map(String::as_str).collect();
    assert_eq!(unique.len(), 8, "no two readings collide on a widget name: {names:?}");
    assert_eq!(names[2], "cut-readout-playhead");
    assert_eq!(names[7], "cut-readout-segments");
    // "Cut at 1×" spells its × out rather than dropping it: a truncated-looking name reads as a bug.
    assert_eq!(names[5], "cut-readout-cut-at-1-times");

    // The two lengths that disagree when a speed effect is on: the s7 fixture's five seconds played at
    // 2× beside a three-second card. The rate rides the SEGMENT (`Seg::rate`), which is what
    // `cut_seconds` divides by — an `Fx` of kind speed records the effect but does not retime the cut,
    // so building this with the Fx alone would leave both rows reading the same and prove nothing.
    let sped = Cut {
        segs: vec![
            Seg { s: 0.0, e: 10.0, rate: 2.0, ..Default::default() },
            Seg { ins: "card.svg".into(), dur: 3.0, ..Default::default() },
        ],
        fx: vec![naivepost::cut::Fx { kind: "speed".into(), t: 0.0, dur: 10.0, rate: 2.0, ..Default::default() }],
        ..Default::default()
    };
    let rows = cut_screen::idle_readouts(64, "", 0.0, None, &sped, 13.0);
    let value = |label: &str| {
        rows.iter().find(|r| r.label == label).unwrap_or_else(|| panic!("no {label} row")).value.clone()
    };
    assert_ne!(
        value("Cut"),
        value("Cut at 1\u{00d7}"),
        "the running length and the plain length differ while a speed effect is on"
    );
    assert_eq!(value("Cut"), "00:08", "five seconds at 2\u{00d7}, plus the card's three");
    assert_eq!(value("Cut at 1\u{00d7}"), "00:13");
    assert_eq!(value("Segments"), "2");
}
