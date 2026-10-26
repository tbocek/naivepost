// F2.14 §S2b through the WIRE: a real click on the Cut page's ▶, in the model mode, over a socket.
//
// `tests/suite/f2_14_model_mode.rs` proves the rules with answers handed in; this proves that the press
// telephones the address the settings hold, that a second round happens when the walk-back cannot fix
// the first plan, and that what lands is the walked-back cut with §S3's two log lines.

use adw::prelude::*;

use naivepost::project::Origin;
use naivepost::shell::Page;
use naivepost::ui;
use naivepost::ui::cut_suggest_land;

#[path = "_f2_14_press_fixture.rs"]
mod press_fixture;
use press_fixture::{one_scene_cut, point_llm_at, retake, tool_turn, Fixture};

use crate::common;
use common::settle;

#[path = "captions_wire_harness.rs"]
mod harness;

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn round(app: &adw::Application) {
    // ONE mark, so §S1 passes, and words whose seconds the model's stamps can be snapped against.
    let f = Fixture::fresh("press-model")
        .with_timeline()
        // The mark sits past every word the walk-back can snap to, so §S1's "is anything marked" gate
        // passes while §S3's mark-removal step has nothing to take out of a 4–18 s scene.
        .with_marks(&[retake(50.0, 51.0)])
        // The walk-back snaps every edge to a word edge (`walk_of`), so the model's stamps only survive
        // where a word starts or ends: 4.0 and 18.0 are both word edges here.
        .with_words(&[
            ("point", 4.0),
            ("mid", 10.0),
            ("here", 17.0),
            ("next", 25.0),
            ("end", 38.0),
        ]);
    // The fake server, one reply per ROUNDED ASK (it repeats its last reply to anything after). TWO asks,
    // and the reason is `tool_loop`'s own rule: a turn that arrives with calls ENDS the run when one of
    // them is `finish_cut`, so each ask holds exactly one round.
    //   ask 1 — refused by the tool itself: a 0.5 s scene is under `cutpass::MIN_SCENE_SECONDS`, and no
    //           step of the walk-back can repair that silently.
    //   ask 2 — §S4's correction round: the slip out, both scenes re-placed past the words that bound them,
    //           then `finish_cut`, which now passes. §S3's snap then moves an edge the model typed, so what
    //           lands on the page is not what the model wrote.
    // `remove_segment` names its scene by START (§3.6), never by index.
    let llm = harness::FakeLlm::start(vec![
        tool_turn(&[
            (
                "add_segment",
                serde_json::json!({ "start": 4.0, "end": 18.0, "why": "the point" }),
            ),
            (
                "add_segment",
                serde_json::json!({ "start": 30.0, "end": 30.5, "why": "a slip" }),
            ),
            ("finish_cut", serde_json::json!({})),
        ]),
        tool_turn(&[
            ("remove_segment", serde_json::json!({ "start": 30.0 })),
            ("remove_segment", serde_json::json!({ "start": 4.0 })),
            (
                "add_segment",
                serde_json::json!({ "start": 3.0, "end": 19.0, "why": "the point" }),
            ),
            (
                "add_segment",
                serde_json::json!({ "start": 24.0, "end": 40.0, "why": "the second" }),
            ),
            ("finish_cut", serde_json::json!({})),
        ]),
    ]);
    point_llm_at(&llm.url());
    cut_suggest_land::point_session_at(Some(f.tree.clone()));
    let window = ui::build_window(app, &f.project, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    ui::seed_review_cut(&window, &one_scene_cut());
    ui::reopen_history_on(&window, &ui::review_cut_of(&window));
    ui::set_session_policy_field(&window, "cutMode", "model", "this test", Origin::User);
    settle();

    // §S3's persist, on the file the NEXT open reads — which is the session folder the press resolved
    // (`session_tree()`), not the working copy beside the cwd. Read it before the click and after: a cut
    // that landed rewrote it.
    let before = std::fs::read(f.tree.cut_json()).ok();

    // THE CLICK, and the socket it opens.
    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_by_name::<()>("clicked", &[]);
    // Suggest asks on a worker (§03-shell §1, F2.14 "Where it runs"): the press lands when it answers.
    assert!(
        naivepost::ui::background::settle_for_test(std::time::Duration::from_secs(60)),
        "the suggest workers answered"
    );
    settle();

    // What the program actually sent — read from the server's side, not from a hook in the program.
    // Drained AFTER the click settled: the flow runs synchronously inside the press, so every request is
    // already queued and reading them here cannot race the loop.
    let seen = llm.drain();
    assert!(
        seen.len() >= 2,
        "an unfixable first plan means a second ask (§S4): {} request(s) reached the fake server",
        seen.len()
    );
    let first = &seen[0];
    assert!(
        first.body.contains("SESSION TIMELINE:"),
        "the ask carries the timeline: {}",
        &first.body[..200.min(first.body.len())]
    );
    for name in [
        "add_segment",
        "remove_segment",
        "set_speed",
        "cut_status",
        "finish_cut",
    ] {
        assert!(
            first.body.contains(name),
            "the offer lists the tool {name}: {}",
            &first.body[..400.min(first.body.len())]
        );
    }
    // §S4: the second ask carries the refusal back to the model, in `cutpass`'s own words.
    let second = &seen[1];
    assert!(
        second.body.contains("That is not a valid cut"),
        "the second ask opens with the correction sentence: {}",
        &second.body[..2000.min(second.body.len())]
    );
    assert!(
        second.body.contains("shortest stretch this cut keeps"),
        "and it names the fault that refused the first plan: {}",
        &second.body[..2000.min(second.body.len())]
    );

    let on_screen = ui::review_cut_of(&window);
    assert!(
        !on_screen.segs.is_empty(),
        "a suggestion landed on the page: {:?}",
        on_screen.segs
    );
    // §S3: the page's edges are the walk-back's, so at least one is not the number the model typed.
    let typed = [3.0_f64, 19.0, 24.0, 40.0, 4.0, 18.0, 30.0, 30.5];
    assert!(
        on_screen
            .segs
            .iter()
            .flat_map(|seg| [seg.s, seg.e])
            .any(|edge| !typed.iter().any(|t| (edge - t).abs() < 0.001)),
        "an edge moved off the model's number: {:?}",
        on_screen.segs
    );
    let logs = ui::window_logs();
    assert!(
        logs.iter()
            .any(|line| line.starts_with(">>> suggested ") && line.contains(" segments, ")),
        "§S3's first line reached the log: {logs:?}"
    );
    assert!(
        logs.iter().any(|line| {
            line.starts_with(">>> \u{2026}and ")
                && line.contains("effect(s): the speeds, the captions and the decorations")
        }),
        "§S3's second line reached the log: {logs:?}"
    );
    let saved = std::fs::read(f.tree.cut_json()).expect("the suggestion was persisted");
    assert_ne!(
        Some(saved.as_slice()),
        before.as_deref(),
        "cut.json changed, as §S3's persist says"
    );
    // And what a re-open would read is what the page shows.
    assert_eq!(
        naivepost::cut::load(&f.tree).expect("re-read").segs,
        on_screen.segs,
        "the written cut holds the walked-back scenes"
    );
    cut_suggest_land::point_session_at(None);
    window.close();
}

#[test]
fn f2_14_s2b_the_cut_page_press_asks_the_server_and_lands() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            round(app);
            RAN.store(true, std::sync::atomic::Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the model-click round never ran inside activate"
    );
}
