//! F0.14 Profiles (spec/03-shell.md §5b): a User Context template with fields. The template's rules (fields and
//! their controls, `{{name}}`, errors with their line, filling in, required fields) and their storage, then the
//! form on Prepare's bench: choosing a profile shows its fields and not the template, each change rebuilds the
//! project's context, ▶ refuses while a required field is empty, and View shows the context as sent.

use std::collections::BTreeMap;

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::profile::{self, Kind};
use naivepost::ui;

use crate::common;

/// `one_app_round` runs its checks once per process; this says they did.
static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// §5b's own example.
const NEWS: &str = "---
title:    text     required \"Title\"
episode:  number   required \"Episode\"
course:   choice+  \"Course\" = Distributed Systems & Blockchain | Software Engineering
language: choice   \"Spoken in\" = English | German
notes:    textarea \"Notes for the editor\"
---
This is \"{{title}}\", episode {{episode}} of {{course}}. The talk is in {{language}}.
{{notes}}";

#[test]
fn f0_14_s1_a_template_declares_its_fields_and_their_controls() {
    let template = profile::parse(NEWS).expect("the spec's example parses");
    let kinds: Vec<(&str, Kind, bool)> = template
        .fields
        .iter()
        .map(|f| (f.name.as_str(), f.kind, f.required))
        .collect();
    assert_eq!(
        kinds,
        [
            ("title", Kind::Text, true),
            ("episode", Kind::Number, true),
            ("course", Kind::ChoiceCustom, false),
            ("language", Kind::Choice, false),
            ("notes", Kind::TextArea, false),
        ]
    );
    assert_eq!(
        template.fields[2].values,
        ["Distributed Systems & Blockchain", "Software Engineering"]
    );
    assert_eq!(
        profile::summary(&template),
        "5 field(s): Title (text, required), Episode (number, required), Course (choice+), Spoken in (choice), Notes for the editor (textarea)"
    );

    // Filled in: defaults where nothing was typed, an empty optional field's line dropped.
    let mut held = BTreeMap::new();
    held.insert(
        "title".to_string(),
        "Distributed Systems & Blockchain - In the News HS26#6".to_string(),
    );
    let values = profile::values_for(&template, &held);
    assert_eq!(
        values["course"], "Distributed Systems & Blockchain",
        "a choice starts on its first value"
    );
    assert_eq!(
        profile::fill(&template, &values),
        "This is \"Distributed Systems & Blockchain - In the News HS26#6\", episode  of Distributed Systems & Blockchain. The talk is in English."
    );
    assert_eq!(
        profile::first_missing(&template, &values).map(|f| f.label.as_str()),
        Some("Episode"),
        "the first required field left empty, in the form's order"
    );

    // A number field takes numbers only; a plain choice only its values.
    assert!(
        profile::accepts(&template.fields[1], "6") && profile::accepts(&template.fields[1], "-2,5")
    );
    assert!(!profile::accepts(&template.fields[1], "six"));
    assert!(
        !profile::accepts(&template.fields[3], "French")
            && profile::accepts(&template.fields[2], "French")
    );

    // No field block: the whole file is the text.
    assert_eq!(profile::parse("A lecture.").unwrap().fields.len(), 0);
}

#[test]
fn f0_14_s1_errors_are_said_with_their_line() {
    // The first error is the one the window shows (a bad field also leaves its `{{name}}` undeclared).
    let errors = |source: &str| vec![profile::parse(source).expect_err("an error")[0].clone()];
    assert_eq!(
        errors("---\ntitle: dropdown \"T\"\n---\n{{title}}"),
        ["line 2: kind `dropdown` -- one of text, number, textarea, choice, choice+"]
    );
    assert_eq!(
        errors("---\ntitle: text\n---\nHello {{tilte}}"),
        ["line 4: `tilte` in the text is not a field above"]
    );
    assert_eq!(
        errors("---\nlang: choice \"L\"\n---\n"),
        ["line 2: `lang` is a choice with no values -- list them after ="]
    );
    assert_eq!(
        errors("---\ntitle: text\ntitle: text\n---\n"),
        ["line 3: `title` is declared twice"]
    );
    assert_eq!(
        errors("---\ntitle: text\n"),
        ["line 1: the field block opened with --- is never closed"]
    );
}

#[test]
fn f0_14_s2_profiles_are_files_in_the_settings_folder_and_an_error_is_not_saved() {
    let root = common::temp_dir_tagged("profiles-store");
    let paths = naivepost::settings::Paths {
        config_dir: root.join("cfg"),
        data_dir: root.join("data"),
    };
    let file = profile::save(&paths, "News show", NEWS).expect("saved");
    assert_eq!(file, root.join("cfg/profiles/News show.txt"));
    assert_eq!(profile::list(&paths), ["News show"]);
    assert_eq!(profile::load(&paths, "News show").unwrap(), NEWS);
    assert_eq!(
        profile::save(&paths, "Broken", "---\nx: text\n---\n{{y}}"),
        Err("line 4: `y` in the text is not a field above".to_string()),
        "a template with an error is refused with it"
    );
    assert_eq!(
        profile::list(&paths),
        ["News show"],
        "and nothing was written"
    );
    assert_eq!(
        profile::file_of(&paths, "a/b").file_name().unwrap(),
        "a-b.txt"
    );
    profile::delete(&paths, "News show").unwrap();
    assert!(profile::list(&paths).is_empty());
}

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

fn named<W: IsA<gtk::Widget>>(window: &adw::ApplicationWindow, name: &str) -> W {
    common::find_named(window, name)
        .unwrap_or_else(|| panic!("no `{name}`"))
        .downcast::<W>()
        .unwrap_or_else(|_| panic!("`{name}` is not the expected widget"))
}

#[test]
fn f0_14_s3_to_s6_prepare_fills_a_profile_in_and_refuses_while_a_required_field_is_empty() {
    common::pin_private_cwd();
    let paths = naivepost::settings::from_environment().expect("a settings folder");
    profile::save(
        &paths,
        "News show",
        &format!("{NEWS}\n## produce\nPreset: veryslow\nChannels: on\n"),
    )
    .expect("the profile is on this machine");
    let root = common::temp_dir_tagged("profiles-prepare");
    let dir = root.join("demo.naivepost");
    common::copy_fixture(&dir);
    common::one_app_round(move |app| {
        let loaded = naivepost::project::load(&dir).expect("the project loads");
        let window = ui::window::build_window_at(app, &loaded, "Prepare", &dir);
        window.present();
        common::settle();
        // S1: the header's Profiles button wears the desktop's template icon and opens the Profiles window.
        let button: gtk::Button = named(&window, "profiles-button");
        assert_eq!(
            button.icon_name().as_deref(),
            Some("folder-templates-symbolic")
        );
        button.emit_clicked();
        common::settle();
        ui::profiles_dialog::open_window()
            .expect("the Profiles window opened")
            .close();
        common::settle();
        let picker: gtk::DropDown = named(&window, "profile-picker");
        assert!(
            picker.is_visible(),
            "the User Context row carries the profile picker"
        );
        assert!(
            common::find_named(&window, "profile-field-title").is_none(),
            "(none): no form"
        );

        // S3: a profile chosen replaces the text box with its form; the template is not shown.
        picker.set_selected(1);
        common::settle();
        let title: gtk::Entry = named(&window, "profile-field-title");
        assert!(title.is_mapped(), "the form is on screen");
        // A text area spans the column (its label on its own line) and is a third of the window tall.
        for _ in 0..10 {
            common::settle();
        }
        let notes = common::find_named(&window, "profile-field-notes-box").expect("the notes box");
        let at = |w: &gtk::Widget| w.compute_bounds(&window).expect("laid out");
        let (notes_box, title_box) = (at(&notes), at(title.upcast_ref()));
        assert!(
            notes_box.x() < title_box.x() - 20.0,
            "the text area starts left of the one-line fields: {notes_box:?} vs {title_box:?}"
        );
        assert!(
            (notes_box.height() - window.height() as f32 / 3.0).abs() <= 12.0,
            "a third of the window tall: {} of {}",
            notes_box.height(),
            window.height()
        );
        assert!(
            !common::find_named(&window, "bench-text").is_some_and(|w| w.is_mapped()),
            "and the template's text box is not"
        );
        let session = ui::session_handle(&window);
        assert_eq!(
            session.borrow().profile.as_ref().map(|p| p.name.as_str()),
            Some("News show")
        );
        // S9: the profile's produce rows set the project's encoder settings, in the window and on disk.
        assert_eq!(
            session.borrow().produce.preset,
            naivepost::project::Preset::VerySlow
        );
        assert!(session.borrow().produce.mono);
        let on_disk =
            naivepost::project::load(&naivepost::startup::open_dir()).expect("the project file");
        assert_eq!(
            on_disk.produce.preset,
            naivepost::project::Preset::VerySlow,
            "kept in naivepost.json"
        );
        // And the Produce page shows them greyed: they are changed in Profiles now.
        let settings = common::find_named(&window, "encoder-settings").expect("the Produce rows");
        assert!(
            !settings.is_sensitive(),
            "the Produce page's rows are the profile's"
        );
        assert!(
            settings
                .tooltip_text()
                .is_some_and(|tip| tip.contains("News show")),
            "and say which profile sets them"
        );

        // S6: Title and Episode are required and empty, so ▶ refuses with the first.
        ui::play_button(&window).expect("▶").emit_clicked();
        common::settle();
        assert_eq!(
            common::status_text(&window),
            "fill in Title first \u{2014} the profile News show needs it"
        );

        // S1: with the project on a profile, the header's Profiles button opens on that profile, not on New.
        named::<gtk::Button>(&window, "profiles-button").emit_clicked();
        common::settle();
        assert_eq!(
            ui::profiles_dialog::edit_state_for_test().map(|(name, _, _)| name),
            Some(Some("News show".to_string())),
            "the Profiles window opens on the project's profile"
        );
        ui::profiles_dialog::open_window().expect("open").close();
        common::settle();

        // S4: each change stores the value and rebuilds the context.
        title.set_text("Distributed Systems & Blockchain - In the News HS26#6");
        named::<gtk::Entry>(&window, "profile-field-episode").set_text("6");
        common::settle();
        {
            let held = session.borrow();
            assert_eq!(held.profile.as_ref().unwrap().values["episode"], "6");
            assert!(
                held.context.starts_with("This is \"Distributed Systems & Blockchain - In the News HS26#6\", episode 6 of"),
                "the context is the template filled in: {}",
                held.context
            );
        }
        assert_ne!(
            ui::profile_form::refusal(&session.borrow()),
            Some("fill in Title first \u{2014} the profile News show needs it".to_string()),
        );
        assert_eq!(
            ui::profile_form::refusal(&session.borrow()),
            None,
            "nothing required is missing"
        );

        // S5: View shows the context as sent, read-only.
        named::<gtk::Button>(&window, "profile-view").emit_clicked();
        common::settle();
        let shown = ui::profiles_dialog::open_window().expect("the Profiles window is open");
        let text = find_in(shown.upcast_ref(), "profile-view-text")
            .and_then(|w| w.downcast::<gtk::TextView>().ok())
            .expect("the context, read-only");
        assert!(!text.is_editable());
        let buffer = text.buffer();
        let (start, end) = buffer.bounds();
        assert_eq!(
            buffer.text(&start, &end, false),
            session.borrow().context.as_str()
        );
        shown.close();

        // Back to (none): the free-text box, holding the filled-in text.
        picker.set_selected(0);
        common::settle();
        assert!(session.borrow().profile.is_none());
        assert!(
            session.borrow().context.contains("episode 6"),
            "the last filled-in text stays"
        );
        window.close();
        RAN.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the Prepare round never ran"
    );
}

#[test]
fn f0_14_s8_a_profile_carries_a_section_per_step_added_to_that_steps_prompt() {
    let source = "---
title: text required \"Title\"
---
A weekly news segment.
## Notes
Swiss numbers.
## narrate
First person, at most 12 words a line.
## youtube
Title: {{title}}
";
    let template = profile::parse(source).expect("sections parse");
    // `## Notes` is no step: it stays in the User Context.
    assert_eq!(
        template.text,
        "A weekly news segment.\n## Notes\nSwiss numbers."
    );
    assert_eq!(
        template
            .sections
            .iter()
            .map(|(key, _)| key.as_str())
            .collect::<Vec<_>>(),
        ["narrate", "youtube"]
    );
    let mut held = BTreeMap::new();
    held.insert("title".to_string(), "In the News HS26#6".to_string());
    let values = profile::values_for(&template, &held);
    assert_eq!(
        profile::section(&template, "youtube", &values).as_deref(),
        Some("Title: In the News HS26#6"),
        "fields work inside a section"
    );
    assert_eq!(
        profile::section(&template, "describe", &values),
        None,
        "no section, nothing added"
    );
    assert!(
        !profile::fill(&template, &values).contains("12 words"),
        "a section is not the context"
    );
    assert!(profile::summary(&template).ends_with("; sections for narrate, youtube"));
    assert_eq!(
        profile::parse("A.\n## narrate\nx\n## narrate\ny").expect_err("twice")[0],
        "line 4: the section `narrate` appears twice"
    );
}

#[test]
fn f0_14_s9_a_profiles_produce_rows_set_the_encoder_through_the_pages_rules() {
    let source = "A news segment.
## produce
Container: webm          # forces vp9 and no subtitle track
Preset: veryslow
Frame rate: 30
Translate: German, French
Quality (CRF): 20
Frame timing: on
Channels: on
Frame edges: off
";
    let template = profile::parse(source).expect("a produce section parses");
    assert_eq!(
        template.produce.len(),
        8,
        "the comment is not part of the value"
    );
    assert!(template.sections.is_empty(), "produce is no prompt section");
    assert!(
        !template.text.contains("Preset"),
        "nor part of the User Context"
    );
    assert!(profile::summary(&template).ends_with("; sections for produce"));

    let before = naivepost::project::Produce::default();
    let after = ui::produce_page::apply_rows(&before, &template.produce);
    use naivepost::project::{Codec, Container, Preset};
    assert_eq!(after.container, Container::Webm);
    assert_eq!(after.codec, Codec::Vp9, "webm forces vp9, as on the page");
    assert_eq!(after.preset, Preset::VerySlow);
    assert_eq!(after.crf, 20);
    assert!(after.vfr && after.mono && !after.blurred_edges);
    assert_eq!(after.translate, ["German", "French"]);
    assert_eq!(
        after.audio_kbps, before.audio_kbps,
        "a row left out keeps the project's value"
    );

    let errors = |line: &str| {
        profile::parse(&format!("x\n## produce\n{line}\n")).expect_err("refused")[0].clone()
    };
    assert_eq!(
        errors("Preset: turbo"),
        "line 3: `turbo` is not a Preset -- ultrafast, veryfast, fast, medium, slow, veryslow"
    );
    assert!(errors("Bitrate: 5000")
        .starts_with("line 3: `Bitrate` is not a Produce row -- one of Container, Codec,"));
    assert_eq!(
        errors("Channels: yes"),
        "line 3: `yes` is not a Channels -- on or off"
    );
    assert_eq!(
        errors("Quality (CRF): 50"),
        "line 3: `50` is not a Quality (CRF) -- 14 to 34"
    );
}

#[test]
fn f0_14_s1_new_is_one_big_template_with_every_section() {
    let starter = profile::starter();
    let template = profile::parse(&starter).expect("New's template parses as it stands");
    let keys: Vec<&str> = template
        .sections
        .iter()
        .map(|(key, _)| key.as_str())
        .collect();
    assert_eq!(
        keys,
        naivepost::prompts::KEYS,
        "a section for every step, in pipeline order"
    );
    for (key, body) in &template.sections {
        assert_eq!(
            Some(body.as_str()),
            naivepost::prompts::shipped_text(key)
                .as_deref()
                .map(str::trim),
            "the `{key}` section starts as the words the app would send"
        );
    }
    assert_eq!(
        template.produce.len(),
        13,
        "and every Produce row, at its default"
    );
    assert!(template
        .produce
        .contains(&("Preset".to_string(), "slow".to_string())));
    assert_eq!(template.fields.len(), 1);
}
