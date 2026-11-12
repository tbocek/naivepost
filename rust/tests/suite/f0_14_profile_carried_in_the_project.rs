//! spec/03-shell.md §5b "Carried in the project": a project keeps the whole text of the profile it uses, so
//! its folder copied to another machine brings the profile along: opened where the profile is missing, it
//! writes it there, and the form, the prompts' sections and the encoder rows work as they did.
//! Its own file: `one_app_round` runs one round per process.

use adw::prelude::*;
use naivepost::profile::{self, Carry};
use naivepost::ui;

use crate::common;

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

const TEXT: &str = "---\ntitle: text required \"Title\"\n---\nAn interview: {{title}}.\n\n## cut\nKeep most of it.\n";

/// The rule alone: this machine's profile wins where it exists; a machine without one takes the project's.
#[test]
fn f0_14_carried_the_rule() {
    assert_eq!(
        profile::carry("", None),
        Carry::Nothing,
        "no profile anywhere"
    );
    assert_eq!(
        profile::carry(TEXT, Some(TEXT)),
        Carry::Nothing,
        "the same on both sides"
    );
    assert_eq!(
        profile::carry("", Some(TEXT)),
        Carry::Keep {
            text: TEXT.to_string(),
            replaced: false
        },
        "an older project takes the text the first time"
    );
    assert_eq!(
        profile::carry("old words", Some(TEXT)),
        Carry::Keep {
            text: TEXT.to_string(),
            replaced: true
        },
        "edited on this machine since: this machine's wins, and that is said"
    );
    assert_eq!(
        profile::carry(TEXT, None),
        Carry::Restore(TEXT.to_string()),
        "another machine"
    );
}

/// The text rides in naivepost.json, and a project without one writes no key.
#[test]
fn f0_14_carried_it_is_kept_in_the_project_file() {
    common::pin_private_cwd();
    let root = common::temp_dir_tagged("profile-carried-json");
    let dir = root.join("demo.naivepost");
    common::copy_fixture(&dir);
    let mut project = naivepost::project::load(&dir).expect("the project loads");
    project.profile = Some(naivepost::project::ProfileUse {
        name: "Interview".to_string(),
        template: TEXT.to_string(),
        ..Default::default()
    });
    naivepost::project::save(&project, &dir).expect("kept");
    let again = naivepost::project::load(&dir).expect("read again");
    assert_eq!(again.profile.expect("the profile").template, TEXT);

    project.profile.as_mut().unwrap().template.clear();
    naivepost::project::save(&project, &dir).expect("kept");
    let file = std::fs::read_to_string(dir.join("naivepost.json")).unwrap();
    assert!(
        !file.contains("\"template\""),
        "nothing carried, no key: {file}"
    );
}

/// Another machine: the profile is not in its settings folder, the project carries it, and opening the
/// project writes it there, so the form is the profile's again.
#[test]
fn f0_14_carried_a_project_brings_its_profile_to_a_machine_without_it() {
    common::pin_private_cwd();
    let paths = naivepost::settings::from_environment().expect("a settings folder");
    assert!(
        profile::load(&paths, "Interview").is_err(),
        "this machine starts without the profile"
    );
    let root = common::temp_dir_tagged("profile-carried");
    let dir = root.join("demo.naivepost");
    common::copy_fixture(&dir);
    let mut project = naivepost::project::load(&dir).expect("the project loads");
    project.profile = Some(naivepost::project::ProfileUse {
        name: "Interview".to_string(),
        values: [("title".to_string(), "Stablecoins".to_string())]
            .into_iter()
            .collect(),
        template: TEXT.to_string(),
        ..Default::default()
    });
    naivepost::project::save(&project, &dir).expect("the project is kept");
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("the project loads");
        let window = ui::window::build_window_at(app, &loaded, "Prepare", &dir);
        window.present();
        common::settle();
        assert_eq!(
            profile::load(&paths, "Interview").expect("written to this machine"),
            TEXT,
            "the profile is on this machine now, word for word"
        );
        assert!(
            profile::list(&paths).contains(&"Interview".to_string()),
            "and in the list every dropdown reads"
        );
        assert_eq!(
            ui::session_handle()
                .borrow()
                .profile
                .as_ref()
                .map(|used| used.template.clone()),
            Some(TEXT.to_string()),
            "the project still carries it"
        );
        window.close();
        RAN.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the round never ran"
    );
}
