//! F0.14 S1/S2 (spec/03-shell.md §5b): the Profiles window. New is the dropdown's first row and can only be kept
//! with Save as…; Save as… into the profiles folder makes a profile, anywhere else a copy; Load… selects a profile
//! or brings any file in as New; 🗑 deletes; ⓘ shows the template syntax. Its own file: `one_app_round` runs one
//! round per process.

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::profile;

use crate::common;

static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Walk down from `node` for the widget named `name` (the Profiles window is a window of its own).
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

#[test]
fn f0_14_s1_s2_the_profiles_window_new_load_save_as_trash_and_syntax() {
    use naivepost::ui::profiles_dialog as dialog;
    common::pin_private_cwd();
    let paths = naivepost::settings::from_environment().expect("a settings folder");
    profile::save(&paths, "Lecture", "A lecture.").expect("a profile on this machine");
    let outside = common::temp_dir_tagged("profiles-outside");
    std::fs::create_dir_all(&outside).unwrap();
    common::one_app_round(move |_app| {
        let window = dialog::open_edit(None, None);
        common::settle();
        // New is the dropdown's first row, always there: it can be kept only with Save as….
        assert_eq!(
            dialog::edit_state_for_test(),
            Some((None, false, false)),
            "New: no Save, no 🗑"
        );
        let syntax = find_in(window.upcast_ref(), "profile-syntax-text")
            .and_then(|w| w.downcast::<gtk::Label>().ok())
            .expect("ⓘ carries the syntax");
        assert!(syntax.text().contains("choice+") && syntax.text().contains("{{name}}"));
        assert!(find_in(window.upcast_ref(), "profile-delete")
            .and_then(|w| w.downcast::<gtk::Button>().ok())
            .is_some_and(|b| b.icon_name().as_deref() == Some("user-trash-symbolic")));

        // Save as… into the profiles folder makes a profile named by the file.
        let said = dialog::save_as_to(&profile::dir(&paths).join("News.txt"));
        assert!(said.starts_with("profile News saved"), "{said}");
        assert_eq!(
            dialog::edit_state_for_test(),
            Some((Some("News".into()), true, true))
        );
        assert!(
            profile::list(&paths).contains(&"News".to_string()),
            "News is a profile now"
        );
        // Anywhere else it is a copy to share, and the profile in the box stays.
        let shared = outside.join("shared.txt");
        assert_eq!(
            dialog::save_as_to(&shared),
            format!("written to {}", shared.display())
        );
        assert!(std::fs::read_to_string(&shared)
            .unwrap()
            .contains("{{title}}"));
        assert_eq!(
            dialog::edit_state_for_test().unwrap().0,
            Some("News".into())
        );

        // Load… of any file comes in as New; of a profile file, selects that profile.
        std::fs::write(outside.join("other.txt"), "Someone else's context.").unwrap();
        assert!(dialog::load_from(&outside.join("other.txt")).starts_with("loaded from"));
        assert_eq!(dialog::edit_state_for_test(), Some((None, false, false)));
        dialog::load_from(&profile::dir(&paths).join("Lecture.txt"));
        assert_eq!(
            dialog::edit_state_for_test(),
            Some((Some("Lecture".into()), true, true))
        );

        // 🗑 (confirmed) deletes it and goes back to New.
        dialog::delete_for_test();
        assert!(
            !profile::list(&paths).contains(&"Lecture".to_string()),
            "Lecture is gone"
        );
        assert_eq!(dialog::edit_state_for_test(), Some((None, false, false)));
        window.close();
        RAN.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the window round never ran"
    );
}
