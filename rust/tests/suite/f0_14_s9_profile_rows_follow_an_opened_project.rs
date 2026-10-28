//! F0.14 S9 (spec/03-shell.md §5b), the profile read again: a project on a profile whose `## produce` rows were
//! changed while the project was closed opens with the profile's rows, in the window, on disk and on the Produce
//! page, which greys them as the profile's. The first rewrite applied them only when the profile was chosen, so a
//! Translate tick set in Profiles never reached the project or the run.
//! Its own file: `one_app_round` runs one round per process.

use adw::prelude::*;
use naivepost::profile;
use naivepost::ui;

use crate::common;

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[test]
fn f0_14_s9_an_opened_project_takes_its_profiles_produce_rows() {
    common::pin_private_cwd();
    let paths = naivepost::settings::from_environment().expect("a settings folder");
    profile::save(
        &paths,
        "Weekly",
        "A weekly news show.\n## produce\nTranslate: German\n",
    )
    .expect("the profile is on this machine");
    let root = common::temp_dir_tagged("profile-follows");
    let dir = root.join("demo.naivepost");
    common::copy_fixture(&dir);
    // The project chose the profile before its Translate row said German, and was kept that way.
    let mut project = naivepost::project::load(&dir).expect("the project loads");
    project.profile = Some(naivepost::project::ProfileUse {
        name: "Weekly".to_string(),
        values: Default::default(),
        ..Default::default()
    });
    project.produce.translate = Vec::new();
    naivepost::project::save(&project, &dir).expect("the project is kept");
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("the project loads");
        let window = ui::window::build_window_at(app, &loaded, "Produce", &dir);
        window.present();
        common::settle();
        let german = vec!["German".to_string()];
        assert_eq!(
            ui::session_handle(&window).borrow().produce.translate,
            german,
            "in the window"
        );
        assert_eq!(
            naivepost::project::load(&dir)
                .expect("the project file")
                .produce
                .translate,
            german,
            "kept in naivepost.json"
        );
        assert_eq!(
            ui::produce_page::read_state().translate,
            german,
            "and shown on the Produce page"
        );
        window.close();
        RAN.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the round never ran"
    );
}
