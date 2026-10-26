//! F0.14 (spec/03-shell.md §5b), the Profiles window's controls through their REAL widgets: the list picks a
//! profile; Save writes what the template box holds; Load… and Save as… raise one chooser each, answered through
//! the door the chooser's response runs; 🗑 asks, Cancel keeps the profile and Delete removes it; ⓘ opens the
//! syntax; Esc closes the window; and the view window's Edit template opens the editor on that profile.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::common;
use adw::prelude::*;
use gtk4 as gtk;
use naivepost::profile;
use naivepost::ui::profiles_dialog as dialog;

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

fn named<W: IsA<gtk::Widget>>(window: &impl IsA<gtk::Widget>, name: &str) -> W {
    find_in(window.upcast_ref(), name)
        .unwrap_or_else(|| panic!("no widget named {name}"))
        .downcast::<W>()
        .unwrap_or_else(|_| panic!("{name} is not the kind asked for"))
}

fn text_of(view: &gtk::TextView) -> String {
    let buffer = view.buffer();
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .to_string()
}

/// The visible message dialog asking `question`.
fn asking(question: &str) -> Option<adw::MessageDialog> {
    gtk::Window::list_toplevels()
        .into_iter()
        .filter_map(|widget| widget.downcast::<adw::MessageDialog>().ok())
        .find(|ask| ask.is_visible() && ask.heading().as_deref() == Some(question))
}

#[test]
fn f0_14_the_profiles_window_every_press_through_its_widget() {
    static RAN: AtomicBool = AtomicBool::new(false);
    common::pin_private_cwd();
    let paths = naivepost::settings::from_environment().expect("a settings folder");
    profile::save(&paths, "Lecture", "A lecture.").expect("a profile");
    profile::save(&paths, "Talk", "A talk.").expect("another");
    let outside = common::temp_dir_tagged("profiles-every-press");
    std::fs::create_dir_all(&outside).unwrap();
    common::one_app_round(move |_app| {
        let window = dialog::open_edit(None, None);
        common::settle();
        let editor: gtk::TextView = named(&window, "profile-template");

        // The list: picking Talk shows Talk.
        let list: gtk::DropDown = named(&window, "profiles-list");
        let model = list.model().expect("the list's rows");
        let talk = (0..model.n_items())
            .find(|i| {
                model
                    .item(*i)
                    .and_downcast::<gtk::StringObject>()
                    .is_some_and(|row| row.string() == "Talk")
            })
            .expect("Talk is listed");
        list.set_selected(talk);
        common::settle();
        assert_eq!(
            dialog::edit_state_for_test().map(|s| s.0),
            Some(Some("Talk".into()))
        );
        assert!(
            text_of(&editor).contains("A talk."),
            "Talk's template is in the box"
        );

        // Save writes what the box holds now.
        editor.buffer().set_text("A talk, edited.");
        named::<gtk::Button>(&window, "profile-save").emit_clicked();
        let stored =
            std::fs::read_to_string(profile::dir(&paths).join("Talk.txt")).expect("Talk.txt");
        assert!(
            stored.contains("A talk, edited."),
            "Save wrote the edit: {stored}"
        );

        // Save as…: one chooser, answered with a name in the profiles folder.
        let raised = dialog::choosers_raised_for_test();
        named::<gtk::Button>(&window, "profile-save-as").emit_clicked();
        assert_eq!(
            dialog::choosers_raised_for_test(),
            raised + 1,
            "one chooser"
        );
        assert_eq!(
            dialog::answer_chooser_for_test(None).as_deref(),
            Some("Save the profile as"),
            "Cancel writes nothing"
        );
        assert!(!profile::dir(&paths).join("Panel.txt").exists());
        dialog::answer_chooser_for_test(Some(&profile::dir(&paths).join("Panel.txt")));
        assert!(
            profile::list(&paths).contains(&"Panel".to_string()),
            "Panel is a profile now"
        );
        assert_eq!(
            dialog::edit_state_for_test().map(|s| s.0),
            Some(Some("Panel".into()))
        );

        // Load…: one chooser, answered with a file from elsewhere: it comes in as New.
        std::fs::write(outside.join("theirs.txt"), "Someone else's context.").unwrap();
        named::<gtk::Button>(&window, "profile-load").emit_clicked();
        assert_eq!(dialog::choosers_raised_for_test(), raised + 2);
        dialog::answer_chooser_for_test(Some(&outside.join("theirs.txt")));
        assert_eq!(
            dialog::edit_state_for_test().map(|s| s.0),
            Some(None),
            "in as New"
        );
        assert!(text_of(&editor).contains("Someone else's context."));

        // 🗑 on Panel: Cancel keeps it, Delete removes it.
        named::<gtk::Button>(&window, "profile-load").emit_clicked();
        dialog::answer_chooser_for_test(Some(&profile::dir(&paths).join("Panel.txt")));
        common::settle();
        assert_eq!(
            dialog::edit_state_for_test().map(|s| s.0),
            Some(Some("Panel".into()))
        );
        let trash: gtk::Button = named(&window, "profile-delete");
        trash.emit_clicked();
        asking("Delete the profile Panel?")
            .expect("🗑 asks first")
            .response("cancel");
        common::settle();
        assert!(
            profile::list(&paths).contains(&"Panel".to_string()),
            "Cancel kept it"
        );
        trash.emit_clicked();
        asking("Delete the profile Panel?")
            .expect("asked again")
            .response("delete");
        common::settle();
        assert!(
            !profile::list(&paths).contains(&"Panel".to_string()),
            "Delete removed it"
        );
        assert_eq!(
            dialog::edit_state_for_test().map(|s| s.0),
            Some(None),
            "back to New"
        );

        // ⓘ opens the syntax.
        let help: gtk::MenuButton = named(&window, "profile-syntax");
        help.popup();
        common::settle();
        assert!(
            help.popover().is_some_and(|p| p.is_visible()),
            "ⓘ shows the syntax"
        );
        help.popdown();

        // Esc closes the window, through its own key controller.
        let esc = window
            .observe_controllers()
            .iter::<glib::Object>()
            .flatten()
            .find_map(|c| c.downcast::<gtk::EventControllerKey>().ok())
            .expect("the window listens for Esc");
        let _: bool = esc.emit_by_name(
            "key-pressed",
            &[
                &gtk::gdk::Key::Escape,
                &9u32,
                &gtk::gdk::ModifierType::empty(),
            ],
        );
        common::settle();
        assert!(!window.is_visible(), "Esc closed the Profiles window");

        // The view window's Edit template opens the editor on that profile.
        let view = dialog::open_view(None, "Lecture", "A lecture.");
        common::settle();
        named::<gtk::Button>(&view, "profile-edit-template").emit_clicked();
        common::settle();
        assert_eq!(
            dialog::edit_state_for_test().map(|s| s.0),
            Some(Some("Lecture".into())),
            "Edit template opens the editor on Lecture"
        );
        RAN.store(true, Ordering::SeqCst);
    });
    assert!(RAN.load(Ordering::SeqCst), "the round never ran");
}
