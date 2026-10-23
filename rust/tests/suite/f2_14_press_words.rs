// F2.14 §S2a through the WIRE: a real click on the Cut page's ▶, in the words mode.
//
// `tests/suite/f2_14_words_mode.rs` proves the rules with inputs handed in; this proves the press path: that
// the run bar's ▶ runs the words arm, lands its cut on the page, writes it where the next open reads
// it, says §S2a's sentence, and gives it all back in one ↶.

use adw::prelude::*;
use naivepost::cut::Seg;
use naivepost::project::Origin;
use naivepost::shell::Page;
use naivepost::ui;
use naivepost::ui::cut_suggest_land;

#[path = "_f2_14_press_fixture.rs"]
mod press_fixture;
use press_fixture::{retake, words_cut, Fixture};

use crate::common;
use common::{settle, status_text};

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn round(app: &adw::Application) {
    let f = Fixture::fresh("press-words")
        .with_timeline()
        .with_words(&[
            ("hello", 2.0),
            ("there", 6.0),
            ("half", 20.0),
            ("end", 26.0),
        ])
        .with_marks(&[retake(4.0, 5.0)]);
    // §S1's guards read the launch's folder when nothing points elsewhere; a test points it deliberately.
    cut_suggest_land::point_session_at(Some(f.tree.clone()));
    let window = ui::build_window(app, &f.project, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    let base = words_cut();
    assert!(
        base.segs.len() > 1,
        "the words walk needs more than one scene to trim: {:?}",
        base.segs
    );
    ui::seed_review_cut(&window, &base);
    ui::reopen_history_on(&window, &base);
    ui::set_session_policy_field(&window, "cutMode", "words", "this test", Origin::User);
    assert_eq!(
        ui::session_policy(&window).cut_mode.value,
        naivepost::project::CutMode::Words,
        "the press below is the words arm's"
    );
    settle();

    // THE CLICK. Everything after this is what the page was left with.
    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_by_name::<()>("clicked", &[]);
    settle();

    let on_screen = ui::review_cut_of(&window);
    assert!(
        on_screen.segs != base.segs,
        "the words walk changed the page's cut: {:?} -> {:?}",
        base.segs,
        on_screen.segs
    );
    let footage: Vec<&Seg> = on_screen
        .segs
        .iter()
        .filter(|seg| !seg.is_insert())
        .collect();
    assert!(
        !footage.is_empty(),
        "a words cut leaves footage on the page: {:?}",
        on_screen.segs
    );
    // §S2a's persist, on the file the NEXT open reads. The write goes to the tree the press resolved for
    // this session (`session_tree()`), which is the folder `point_session_at` pointed at — NOT the
    // working copy beside the cwd, and not the temp tree until a landing has written it.
    let saved = naivepost::cut::load(&f.tree).expect("cut.json was written");
    assert_eq!(
        saved.segs, on_screen.segs,
        "cut.json holds what the page shows, so a re-open shows it too"
    );
    assert!(
        status_text(&window).starts_with("cut by the words: "),
        "the status line carries §S2a's own sentence: {:?}",
        status_text(&window)
    );
    assert!(
        ui::window_logs()
            .iter()
            .any(|line| line.starts_with(">>> cut by the words: ")),
        "the log carries §S2a's line: {:?}",
        ui::window_logs()
    );
    // One ↶ takes the whole suggestion back, in one step.
    ui::press_undo(&window);
    settle();
    assert_eq!(
        ui::review_cut_of(&window).segs,
        base.segs,
        "one Undo undid the words cut and left the page's own cut"
    );
    cut_suggest_land::point_session_at(None);
    window.close();
}

#[test]
fn f2_14_s2a_the_cut_page_press_cuts_by_the_words() {
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
        "the words-click round never ran inside activate"
    );
}
