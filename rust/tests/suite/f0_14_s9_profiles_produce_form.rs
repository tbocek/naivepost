//! F0.14 S9 (spec/03-shell.md §5b): the Profiles window sets a profile's Produce settings with the Produce page's
//! own controls, not as text. The file keeps them as its `## produce` section; the template box never shows it.
//! Its own file: `one_app_round` runs one round per process.

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
fn f0_14_s9_the_profiles_window_sets_produce_with_the_produce_pages_own_controls() {
    use naivepost::ui::profiles_dialog as dialog;
    common::pin_private_cwd();
    let paths = naivepost::settings::from_environment().expect("a settings folder");
    common::one_app_round(move |_app| {
        let window = dialog::open_edit(None, None);
        common::settle();
        let template = find_in(window.upcast_ref(), "profile-template")
            .and_then(|w| w.downcast::<gtk::TextView>().ok())
            .expect("the template box");
        let text = || {
            let buffer = template.buffer();
            let (start, end) = buffer.bounds();
            buffer.text(&start, &end, false).to_string()
        };
        assert!(
            !text().contains("## produce"),
            "the Produce rows are not text in the box"
        );
        // The Produce page's own controls, seeded from New's defaults.
        let preset = find_in(window.upcast_ref(), "setting-preset")
            .and_then(|w| w.downcast::<gtk::DropDown>().ok())
            .expect("the Preset dropdown");
        let face = |picker: &gtk::DropDown| {
            picker
                .selected_item()
                .and_then(|i| i.downcast::<gtk::StringObject>().ok())
                .map(|i| i.string().to_string())
                .unwrap_or_default()
        };
        assert_eq!(face(&preset), "slow");
        let mono = find_in(window.upcast_ref(), "tick-mono")
            .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
            .expect("the mono tick");
        assert!(!mono.is_active());

        // Set them in the form and save: the file carries them as its `## produce` section.
        let at = naivepost::produce_screen::PRESETS
            .iter()
            .position(|p| *p == "veryslow")
            .unwrap();
        preset.set_selected(at as u32);
        mono.set_active(true);
        dialog::save_as_to(&profile::dir(&paths).join("Encoded.txt"));
        let saved = profile::load(&paths, "Encoded").expect("saved");
        let parsed = profile::parse(&saved).expect("it parses");
        assert!(
            parsed
                .produce
                .contains(&("Preset".to_string(), "veryslow".to_string())),
            "{saved}"
        );
        assert!(parsed
            .produce
            .contains(&("Channels".to_string(), "on".to_string())));
        assert_eq!(parsed.produce.len(), 13, "every row is written");

        // Opened again, the form shows them and the box still holds no produce text.
        dialog::load_from(&profile::dir(&paths).join("Encoded.txt"));
        common::settle();
        let preset = find_in(window.upcast_ref(), "setting-preset")
            .and_then(|w| w.downcast::<gtk::DropDown>().ok())
            .expect("the Preset dropdown, rebuilt");
        assert_eq!(face(&preset), "veryslow");
        assert!(!text().contains("## produce"));

        // Laid out like a flowing form: wide, the pairs share lines; narrow, they stack; and however short the
        // window, the template keeps its room to be read (the form once squeezed it out of sight).
        let place = |name: &str| {
            find_in(window.upcast_ref(), name)
                .and_then(|w| w.compute_bounds(&window))
                .map(|b| (b.x(), b.y()))
                .expect(name)
        };
        let resize = |w: i32, h: i32| {
            window.set_default_size(w, h);
            for _ in 0..100 {
                common::settle();
                if window.width() == w && window.height() == h {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            for _ in 0..10 {
                common::settle();
            }
        };
        resize(1280, 900);
        let (container, codec, preset) = (
            place("setting-container"),
            place("setting-codec"),
            place("setting-preset"),
        );
        assert!(
            (container.1 - codec.1).abs() < 1.0 && (codec.1 - preset.1).abs() < 1.0,
            "wide: the first rows share a line: {container:?} {codec:?} {preset:?}"
        );
        assert!(
            template.height() >= dialog::TEMPLATE_MIN_PX,
            "and the template has its room: {}",
            template.height()
        );
        resize(520, 480);
        let (container, codec) = (place("setting-container"), place("setting-codec"));
        assert!(
            codec.1 > container.1 + 1.0,
            "narrow: the rows stack: {container:?} {codec:?}"
        );
        assert!(
            template.height() >= dialog::TEMPLATE_MIN_PX,
            "and a short window scrolls rather than squeezing the template: {}",
            template.height()
        );
        window.close();
        RAN.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the round never ran"
    );
}
