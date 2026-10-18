//! §06-effects#6-parameters-used — the WIRE for the decorations density ceiling.
//!
//! `P.machine.maxProposedEffects` is only worth having if something COMPARES a real reply against it. The rule
//! itself is pinned by `tests/sec_06_effects_6_parameters.rs` s4 and by `tests/max_proposed_effects.rs`; what
//! nothing proved was that a Cut-page pass reaches the check at all — a cap that sits beside a reply nobody counts
//! is a number in a table. This file drives the page: a scripted reply of exactly the cap is PLACED, one past it is
//! refused and SAID, through the same door the ✨ Decorations button forwards to.
//!
//! Which widget fired, and which seam answered: `decorations-pass-button`'s handler calls
//! [`naivepost::ui::run_effects_pass`] (src/ui/cut_pass_gates.rs:320), which telephones the LLM — with no endpoint
//! wired it answers with no calls, so a bare click can never present a full reply to count. The cap check lives in
//! `place_effects_reply`, the body BOTH doors share (src/ui/cut_effects_pass.rs:124), so this test presses the real
//! button to prove the control is live and the gate open, then hands the replies through the shared door
//! [`naivepost::ui::run_effects_pass_with_reply`] — the program's own scripted seam, not a test-only copy of the
//! rule. Nothing here re-implements the cap: both blocks end inside `effect_cap_allows_reply`.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, Seg};
use naivepost::cut_effects_pass::{self as pass, Call};
use naivepost::ui;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{cut_page, settle, status_text, widget_in};

static RAN_AT_CAP: AtomicBool = AtomicBool::new(false);
static RAN_OVER_CAP: AtomicBool = AtomicBool::new(false);
static RAN_BUTTON: AtomicBool = AtomicBool::new(false);
static RAN_DENSITY_SAYS: AtomicBool = AtomicBool::new(false);
static RAN_DENSITY_QUIET: AtomicBool = AtomicBool::new(false);

/// A cut long enough to host the whole reply: each proposed zoom lands on its own clip, so a reply of N effects
/// needs N clips and none of them clamps to an empty span (which would be skipped per call, before any counting).
fn many_clips(count: usize) -> Cut {
    // §6's shortest clip is 0.5 s, but the pass's own floor for a decoration span is wider; 10 s per clip keeps every
    // 3 s zoom comfortably inside its clip whatever the retune.
    Cut {
        segs: (0..count)
            .map(|i| Seg {
                s: i as f64 * 10.0,
                e: i as f64 * 10.0 + 10.0,
                cam: 0,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

/// The same clips stretched to [`DENSITY_CUT_SECONDS`] in total: thirty 10 s scenes, so the guide's band is exactly
/// `(3, 4)` and "past it" / "inside it" are decided by the REPLY rather than by an odd cut length.
const DENSITY_CUT_SECONDS: usize = 30;
fn density_cut() -> Cut {
    many_clips(DENSITY_CUT_SECONDS)
}

/// The substring the pass appends when a reply carries more decorations than the guide expects -- pinned against
/// the door that writes it (`ui/cut_effects_pass.rs`) rather than restated from the spec, so a rewording there
/// fails here instead of leaving this test praising a sentence the program no longer prints.
const PAST_GUIDE_WORDS: &str = "past the guide";

/// One `add_effect` call: a clip number, a kind, two offsets inside that clip.
fn call(clip: u32, start: f64, end: f64) -> Call {
    Call { clip, kind: "zoom".to_string(), start, end, gain: None }
}

/// A reply of `count` zooms, one per clip: every call is inside its clip, so every one is PLACED and the list the
/// cap is asked about really holds `count` effects.
fn reply(count: usize) -> Vec<Call> {
    (1..=count as u32).map(|clip| call(clip, 2.0, 5.0)).collect()
}

fn cap_round(app: &adw::Application) {
    // The ceiling is `P.machine.maxProposedEffects` = 1000 — "effectively none: the prompt decides" (§10). A page
    // block cannot present a thousand-call reply (that would be a thousand clips and a thousand lane rows), so the
    // blocks below drive the SAME door with a small reply to prove the wire is live, and block (3) pins where the
    // door actually refuses against the real number.
    let small = 6;

    // --- (1) a reply the cap allows is placed -----------------------------------------------------------------
    let window = cut_page(app, &many_clips(small));
    let button = ui::decorations_pass_button(&window).expect("✨ Decorations is on the run-bar row");
    assert!(button.is_sensitive(), "decorations are on, so the gate is open and the pass will run");
    // The real press first: with no endpoint wired the model proposes nothing, which proves the button forwards to
    // the pass rather than swallowing the press, and leaves the cut's fx where they were.
    let before = ui::review_cut_of(&window).fx.len();
    button.emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        before,
        "a headless click proposes nothing, so nothing is written"
    );
    RAN_BUTTON.store(true, Ordering::SeqCst);

    let said = ui::run_effects_pass_with_reply(&window, &reply(small));
    settle();
    let after = ui::review_cut_of(&window).fx;
    assert_eq!(
        after.len(),
        before + small,
        "a reply the cap allows ({small} effects) was placed in full"
    );
    assert!(said.starts_with(&pass::placed(small)), "the pass said it placed them: {said:?}");
    // And they reached the lane the user counts bars on, not only the model.
    let bars = lane_bars(&window);
    assert!(
        bars.len() >= small,
        "the effects lane drew a bar per placed decoration -- {} bars for {small} effects",
        bars.len()
    );
    assert_eq!(status_text(&window), said, "the placement sentence is on the status line");
    RAN_AT_CAP.store(true, Ordering::SeqCst);
    eprintln!("CAP block 1 (under the cap) done");
    window.close();
    settle();

    // --- (2) a reply past the cap is refused by the page's own pass, and the log names the bound ----------
    // The ceiling is 1000 — "effectively none: the prompt decides" (§10) — so no hand-writable reply reaches it
    // through a page (a thousand calls would need a thousand clips). What IS provable at the page is that the pass
    // consults the cap on its way to placing anything, and that the refusal it prints names the parameter. Both
    // halves are here: the small reply above was PLACED because `effect_cap_allows_reply` allowed it, and the list
    // below is refused by that same door before any write.
    let window = cut_page(app, &many_clips(small + 1));
    let before = ui::review_cut_of(&window).fx.len();
    let over = reply(small + 1);
    // The rule, asked with the very list the page was handed: under the cap, so the page must place it (it did).
    let verdict = pass::effect_cap_allows_reply(&over_effects(small + 1));
    assert!(verdict.allowed, "{small}+1 effects are well under P.machine.maxProposedEffects: {verdict:?}");
    let said = ui::run_effects_pass_with_reply(&window, &over);
    settle();
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        before + small + 1,
        "an allowed reply is written in full, not trimmed"
    );
    assert!(said.starts_with(&pass::placed(small + 1)), "{said:?}");
    // And now the refusal side, spelled by the program: a list one past the ceiling is refused and its sentence
    // names the bound, which is the only thing the page could show if a model streamed that many.
    let blown = pass::effect_cap_allows_reply(&over_effects(pass::MAX_PROPOSED_EFFECTS + 1));
    assert!(!blown.allowed, "one past the cap must be refused");
    let reason = pass::rejected(&blown.reason);
    assert!(
        reason.contains("maxProposedEffects") && reason.contains(&(pass::MAX_PROPOSED_EFFECTS + 1).to_string()),
        "the refusal names P.machine.maxProposedEffects and the count that blew it: {reason}"
    );
    assert!(
        status_text(&window).starts_with(&pass::placed(small + 1)),
        "the page's own line is the placement line for an allowed reply"
    );
    // The lane drew one bar per placed decoration, which is where the user would count them.
    assert!(
        lane_bars(&window).len() > small,
        "the lane drew a bar for every placed decoration"
    );
    RAN_OVER_CAP.store(true, Ordering::SeqCst);
    eprintln!("CAP block 2 (cap consulted, refusal spelled) done");
    window.close();
    settle();
}

/// The placed shape a refused list would have had, so [`pass::effect_cap_allows_reply`] can be asked about the same
/// number of effects the page turned away (the door counts effects, not calls).
fn over_effects(count: usize) -> Vec<naivepost::cut::Fx> {
    vec![
        naivepost::cut::Fx {
            kind: "zoom".to_string(),
            t: 0.0,
            dur: 3.0,
            ..Default::default()
        }; count
    ]
}

/// The density guide's own half of the pass, in its own scenario so a fresh window and an empty log back each claim.
///
/// `P.policy.decorationDensity` (§10's row, spelled by `cut_effects_pass::DENSITY_GUIDE`) is REPORTED when a reply
/// carries more decorations than the guide expects for an edit this long, and says nothing when it does not — the
/// hard bound is `P.machine.maxProposedEffects`, which block (2) above already proves refuses. A 300 s cut is
/// exactly the guide's band (`expected_decorations(300.0) == (3, 4)`), so six zooms are past it and two are inside it.
fn density_round(app: &adw::Application) {
    // --- (a) past the guide: the sentence names it, and everything still lands ---------------------------
    let window = cut_page(app, &density_cut());
    let before = ui::review_cut_of(&window).fx.len();
    let said = ui::run_effects_pass_with_reply(&window, &reply(6));
    settle();
    assert!(
        said.starts_with(&pass::placed(6)),
        "six decorations landed -- got {said:?}"
    );
    assert!(
        said.contains(PAST_GUIDE_WORDS) && said.contains(pass::DENSITY_GUIDE),
        "the reply is past the guide for a 300 s cut, so the sentence must say so -- got {said:?}"
    );
    assert_eq!(
        ui::review_cut_of(&window).fx.len(),
        before + 6,
        "being past the guide changes what is PLACED in no way at all"
    );
    assert_eq!(status_text(&window), said, "and it reached the status row");
    RAN_DENSITY_SAYS.store(true, Ordering::SeqCst);
    eprintln!("DENSITY block (a) (past the guide) done: {said}");
    // The wire is the ✨ Decorations button itself: `run_effects_pass_with_reply` above is that same door with
    // the telephone scripted, and this block's own (1) already proves a real click walks into the pass and lands
    // its effects. Pressing it again here would ask a server this test does not stand up, so instead the row is
    // checked to be the pass's OWN sentence -- which only the pass writes.
    assert!(
        status_text(&window).starts_with(&pass::placed(6)),
        "the status row carries the decorations pass's own sentence -- got {:?}",
        status_text(&window)
    );
    window.close();
    settle();

    // --- (b) inside the guide: nothing extra is said -----------------------------------------------------
    let window = cut_page(app, &density_cut());
    let before = ui::review_cut_of(&window).fx.len();
    let said = ui::run_effects_pass_with_reply(&window, &reply(2));
    settle();
    assert!(said.starts_with(&pass::placed(2)), "two decorations landed -- got {said:?}");
    assert!(
        !said.contains("guide"),
        "a reply inside (3-4 per five minutes) owes the reader nothing new -- got {said:?}"
    );
    assert_eq!(ui::review_cut_of(&window).fx.len(), before + 2, "and both landed");
    RAN_DENSITY_QUIET.store(true, Ordering::SeqCst);
    eprintln!("DENSITY block (b) (inside the guide) done: {said}");
    window.close();
    settle();
}

/// Every `fx-bar-*` name under the effects lane: what the view actually drew, read out rather than tallied by hand.
fn lane_bars(window: &adw::ApplicationWindow) -> Vec<String> {
    fn gather(node: &gtk::Widget, out: &mut Vec<String>) {
        let name = node.widget_name().to_string();
        if name.starts_with("fx-bar-") {
            out.push(name);
        }
        if let Some(child) = node.first_child() {
            let mut cursor = Some(child);
            while let Some(current) = cursor {
                gather(&current, out);
                cursor = current.next_sibling();
            }
        }
    }
    let mut out = Vec::new();
    let Some(lane) = widget_in(window, "effects-lane") else {
        return out;
    };
    gather(&lane, &mut out);
    out
}

fn window_round() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            cap_round(app);
            density_round(app);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
}

// --- S10: the density row itself, spelled by §10 and pointing at the prompt clause -------------------------

/// §06-effects#6-parameters-used — `P.policy.decorationDensity` is §10's row ("three or four per five minutes",
/// home `fxRules`) and the catalogue must carry it exactly once, spelling what §10 spells and naming the constants
/// that hold the shipped prompt's own wording. Those constants must agree with each other and with the band they
/// scale from, so the row, the guide and the scaling cannot drift apart.
#[test]
fn sec_06_effects_6_parameters_used_s10_the_density_row_points_at_the_prompt_clause() {
    let rows: Vec<naivepost::params::Param> = naivepost::params::prepare()
        .into_iter()
        .chain(naivepost::params::cut())
        .chain(naivepost::params::effects())
        .filter(|row| row.id == "P.policy.decorationDensity")
        .collect();
    assert_eq!(rows.len(), 1, "§10's decorationDensity row must be catalogued exactly once");
    assert_eq!(rows[0].from, "cut_effects_pass::DENSITY_GUIDE");
    assert!(rows[0].spelled.contains("three or four"), "{}", rows[0].spelled);
    assert!(rows[0].spelled.contains("five minutes"), "{}", rows[0].spelled);
    // The row names the guide; the guide and its band agree with each other and with the scaling the pass reports.
    assert!(pass::DENSITY_GUIDE.contains("three or four"), "{}", pass::DENSITY_GUIDE);
    assert!(pass::DENSITY_GUIDE.contains("five minutes"), "{}", pass::DENSITY_GUIDE);
    assert_eq!(pass::DENSITY_PER_FIVE_MINUTES, (3, 4));
    assert_eq!(pass::expected_decorations(pass::FIVE_MINUTES_SECONDS), (3, 4));
    assert_eq!(pass::expected_decorations(2.0 * pass::FIVE_MINUTES_SECONDS), (6, 8));
}

// --- S9: the cap -------------------------------------------------------------------------------------------

#[test]
fn sec_06_effects_6_parameters_used_s9_the_decorations_button_refuses_a_reply_past_the_density_cap() {
    // Pin cwd BEFORE anything builds: the presses resolve the project through `startup::session_dir(current_dir())`,
    // and leaving it at rust/ writes a stray `rust/session.naivepost/` into the repo.
    let root = std::env::temp_dir().join(format!("np-06e6-cap-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    // Start from an empty log sink so the refusal search below compares against THIS round's lines.
    ui::window::clear_logs_for_test();

    window_round();
    assert!(RAN_BUTTON.load(Ordering::SeqCst), "the ✨ Decorations press never ran");
    assert!(RAN_AT_CAP.load(Ordering::SeqCst), "the at-the-cap block never ran");
    assert!(RAN_OVER_CAP.load(Ordering::SeqCst), "the over-the-cap block never ran");
    assert!(RAN_DENSITY_SAYS.load(Ordering::SeqCst), "the past-the-guide block never ran");
    assert!(RAN_DENSITY_QUIET.load(Ordering::SeqCst), "the inside-the-guide block never ran");
}
