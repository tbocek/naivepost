// F2.14 §S2b, at the widgets: ▶ on the Cut page in the MODEL mode.
//
// spec/05-cut.md §F2.14 S1 gives the press its refusals before anything is asked, and S2b says the
// model path then dials whatever Settings names. This file presses the real run-bar ▶ in that mode and
// reads the status line, exactly as `f2_14_guard_order.rs` stages the refused presses — same
// `build_window` + Cut tab + `point_session_at` staging, same "no socket anywhere" rule.
//
// Only the two states a widget test can honestly reach are pinned: a session with no `session.txt`
// (§S1's third refusal, which the mode cannot get past), and a session whose SETTINGS FILE cannot be
// read, which is the case §S2b answers with its own reason rather than a dial. Neither asserts about a
// model reply; the scripted-answer cases live in `f2_14_model_mode.rs`, where an `ask` closure can
// answer instead of a server.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use naivepost::cut::{Cut, Seg};
use naivepost::layout::Tree;
use naivepost::project::Origin;
use naivepost::settings::Paths;
use naivepost::shell::Page;
use naivepost::suggest;
use naivepost::ui;
use naivepost::ui::cut_suggest_land;

use crate::common;
use common::{fixture_dir, settle, status_text, temp_dir};

static RAN_NO_TIMELINE: AtomicBool = AtomicBool::new(false);
static RAN_UNREADABLE_SETTINGS: AtomicBool = AtomicBool::new(false);
static RAN_ALL: AtomicBool = AtomicBool::new(false);

/// A throwaway project folder, plus the settings folder the press reads. The settings half is left
/// EMPTY on purpose: `press` resolves it through the process environment, so what a case controls here
/// is the project, and the settings file it wants is written by `broken_settings`.
struct Project_ {
    root: std::path::PathBuf,
    tree: Tree,
}

impl Project_ {
    fn fresh(tag: &str) -> Self {
        let root = temp_dir(tag);
        let dir = root.join(format!("{tag}.naivepost"));
        std::fs::create_dir_all(&dir).expect("create the throwaway project");
        let tree = Tree::new(&dir).expect("a .naivepost folder is a project");
        std::fs::create_dir_all(
            tree.session_txt().parent().expect("session.txt sits in a folder"),
        )
        .expect("the transcript folder");
        Project_ { root, tree }
    }

    /// §S1's marker that a timeline exists.
    fn with_timeline(self) -> Self {
        std::fs::write(self.tree.session_txt(), "0:00 hello\n").expect("write session.txt");
        self
    }

    /// Words for cam0 that the aligner answered, so the word gate passes and only the case's own cause
    /// can refuse the press.
    fn with_words(self, source: &str) -> Self {
        let doc = naivepost::requests::WordsDoc {
            text: "hello".to_string(),
            words: vec![naivepost::requests::Word {
                word: "hello".to_string(),
                start_sample: 16000,
                end_sample: 20800,
            }],
        };
        naivepost::requests::write_words(&self.tree, source, &doc).expect("write words.json");
        naivepost::requests::write_aligned(
            &self.tree,
            source,
            &naivepost::requests::AlignedDoc {
                words: doc.words.clone(),
            },
        )
        .expect("write words.aligned.json");
        self
    }
}

impl Drop for Project_ {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// Make the settings file unreadable — a directory where `llm.conf` belongs (`Paths::conf_path` is
/// `config_dir/llm.conf`). `settings::read` answers its reason for that, and it is the reason the
/// press must print instead of dialling anything.
fn broken_settings(paths: &Paths) {
    std::fs::create_dir_all(paths.conf_path()).expect("conf_path taken by a folder");
}

fn checks(app: &adw::Application) {
    let model = naivepost::project::load(&fixture_dir()).expect("fixture loads");
    let window = ui::build_window(app, &model, "Prepare");
    window.present();
    ui::tab_button(&window, Page::Cut)
        .expect("the shell has a Cut tab")
        .emit_clicked();
    assert_eq!(ui::state(&window).page, Page::Cut);
    // The mode the press must take: set by hand, through the same door the policy form uses.
    ui::set_session_policy_field(&window, "cutMode", "model", "this test", Origin::User);
    assert_eq!(
        ui::session_policy(&window).cut_mode.value,
        naivepost::project::CutMode::Model,
        "the press below is the model arm's"
    );
    // Footage on the page, so nothing but the case's own cause can refuse the press.
    ui::seed_review_cut(
        &window,
        &Cut {
            segs: vec![Seg { s: 0.0, e: 30.0, cam: 0, ..Default::default() }],
            ..Default::default()
        },
    );
    settle();

    // --- S1: no session timeline ------------------------------------------------------------------
    //
    // The mode cannot get past the guard, so nothing is asked and the bar stays at rest.
    let no_timeline = Project_::fresh("notimeline");
    cut_suggest_land::point_session_at(Some(no_timeline.tree.clone()));
    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    assert_eq!(
        status_text(&window),
        suggest::NO_TIMELINE,
        "the model mode says §S1's own sentence, not a model's"
    );
    // The bar stayed at rest: the ▶ is still drawn as ▶, because the icon is what `paint_run_bar`
    // changes when a run starts.
    assert_eq!(
        ui::play_button(&window)
            .expect("the run bar has a ▶")
            .icon_name()
            .map(|n| n.to_string()),
        Some("media-playback-start-symbolic".to_string()),
        "no run began, so no ⏸ appeared"
    );
    RAN_NO_TIMELINE.store(true, Ordering::SeqCst);

    // --- S2b: the settings it would dial are unreadable -------------------------------------------
    //
    // A timeline now exists AND the word gate passes, so §S1 lets the press through and the first
    // question left is the one S1 cannot answer: what to dial. With no settings it can read, the press
    // says that and does not claim a cut.
    let timeline = Project_::fresh("badsettings")
        .with_timeline()
        .with_words("cam0");
    let paths = Paths {
        config_dir: timeline.root.join("config"),
        data_dir: timeline.root.join("data"),
    };
    // The settings file is made unreadable, so the press must not get as far as a dial: `settings::save`
    // refuses to overwrite the directory with a file, which is exactly the state being pinned.
    std::fs::create_dir_all(&paths.config_dir).expect("config folder");
    broken_settings(&paths);
    // The environment is the only door `press` has to the settings folder, and this binary owns it
    // (one scenario per binary, as `captions_wire_harness` notes for the same reason).
    std::env::set_var("XDG_CONFIG_HOME", timeline.root.join("config-home"));
    std::env::set_var("XDG_DATA_HOME", &paths.data_dir);
    // `settings::from_environment()` files its settings under `$XDG_CONFIG_HOME/naivepost`, so the
    // unreadable `llm.conf` must sit at `conf_path` of THOSE paths — writing it one folder up leaves
    // the press reading a file that is not there and dialling straight past this case.
    let env_paths = naivepost::settings::from_environment().expect("env settings folder");
    std::fs::create_dir_all(&env_paths.config_dir).expect("the env settings folder");
    broken_settings(&env_paths);
    // The first press of the round may have written the project's cut (this binary shares one window,
    // and a window keeps the cut it was shown). What matters is that THIS press wrote nothing, so the
    // file is stamped and compared, not assumed absent.
    naivepost::cut::save(&Cut::default(), &timeline.tree).expect("stamp the cut file");
    let before = std::fs::read(timeline.tree.cut_json()).expect("the stamped cut file");
    cut_suggest_land::point_session_at(Some(timeline.tree.clone()));
    ui::play_button(&window)
        .expect("the run bar has a ▶")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    let said = status_text(&window);
    assert!(
        said.contains("settings could not be read"),
        "the press names the settings file that stopped it: {said}"
    );
    assert!(
        !said.contains("suggested"),
        "and it does not claim a cut landed: {said}"
    );
    assert_eq!(
        std::fs::read(timeline.tree.cut_json()).expect("the cut file is still there"),
        before,
        "a press with nothing to dial left the project's cut byte-for-byte as it was"
    );
    RAN_UNREADABLE_SETTINGS.store(true, Ordering::SeqCst);

    cut_suggest_land::point_session_at(None);
    window.close();
}

#[test]
fn f2_14_s2b_the_model_press_refuses_the_way_the_page_says() {
    common::pin_private_cwd();
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(|app| {
            checks(app);
            RAN_ALL.store(true, Ordering::SeqCst);
            app.quit();
        });
        app.run_with_args::<String>(&[]);
    });
    assert!(RAN_ALL.load(Ordering::SeqCst), "the round never ran");
    assert!(
        RAN_NO_TIMELINE.load(Ordering::SeqCst),
        "the no-timeline press never ran"
    );
    assert!(
        RAN_UNREADABLE_SETTINGS.load(Ordering::SeqCst),
        "the unreadable-settings press never ran"
    );
}
