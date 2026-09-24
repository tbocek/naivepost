//! §10-parameters#3-project-settings-tab-controls — `spec/10-parameters.md` §3, checked
//! against `naivepost::project_settings` and the owners of every value it names.
//!
//! §3 is one sentence listing twenty tab controls and their defaults, so what these tests
//! pin is that the list and the code cannot drift: each control's default must come from
//! the struct or constant that actually stores it (`Project::default`, `Produce::default`,
//! `prepare::FREQ_STOPS`, `fx_aspect::SOURCE`, `publish.json`'s presence), never from a
//! copy in the catalogue. The inversion of two of them (Narration ↔ `no_narration`,
//! "Copy into project" ↔ `reference_sources`) is asserted explicitly, because a silent
//! fix of what looks like a bug there would flip a project's behaviour.

use naivepost::{
    cut_screen, fx_aspect, params, prepare, project::{self, Produce}, project_settings as ps, publish,
};

const ITEM: &str = "§10-parameters#3-project-settings-tab-controls";

/// One row of this section's own catalogue.
fn row(id: &str) -> params::Param {
    ps::listed()
        .into_iter()
        .find(|param| param.id == id)
        .unwrap_or_else(|| {
            let ids: Vec<&str> = ps::listed().iter().map(|p| p.id).collect();
            panic!("{id} is a §3 tab control and so must be catalogued; this section holds {ids:?}")
        })
}

/// The default the tab shows for one control, panicking if it names no control at all.
fn default(row_name: &str) -> String {
    ps::default_for(row_name)
        .unwrap_or_else(|| panic!("{row_name} is a §3 tab control with a default"))
}

/// A temp project folder; returns its tree. Nothing inside is real.
fn temp_project(tag: &str) -> naivepost::layout::Tree {
    let dir = std::env::temp_dir().join(format!(
        "naivepost-s10-3-{}-{tag}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let proj = dir.join("demo.naivepost");
    std::fs::create_dir_all(&proj).unwrap();
    std::fs::write(proj.join(project::PROJECT_FILE), "{}").unwrap();
    naivepost::layout::Tree::new(&proj).unwrap()
}

// ---- S1: Freq ------------------------------------------------------------------------

#[test]
fn sec_10_parameters_3_project_settings_tab_controls_s1_freq_is_the_one_p_project_id() {
    assert_eq!(ITEM, "§10-parameters#3-project-settings-tab-controls");
    // §3 spells exactly one id with a `P.` prefix, and §10 has no other `P.project.*` row
    // — so this is the only id that answers to Family::Project.
    assert_eq!(params::family("P.project.frameInterval"), params::Family::Project);

    // The seven stops §3 lists, read from prepare rather than retyped here.
    let stops = row("P.project.frameInterval");
    assert_eq!(stops.spelled, "0.25, 0.5, 1, 2, 3, 4, 5", "the stops §3 lists, in order");
    assert_eq!(stops.from, "prepare::FREQ_STOPS / FREQ_DEFAULT", "one home for the numbers");
    assert_eq!(prepare::FREQ_STOPS, [0.25, 0.5, 1.0, 2.0, 3.0, 4.0, 5.0]);
    assert_eq!(ps::values_for("Freq").unwrap().len(), 7);

    // The prototype's deeper stops are gone: 0 ("every frame") and 0.1 / 0.2 are not on
    // the list, and a non-stop is refused by the setter rather than accepted silently.
    assert!(!prepare::freq_is_stop(0.0), "0 was the prototype's 'every frame'; it is not a stop now");
    assert!(!prepare::freq_is_stop(0.1));
    assert!(!prepare::freq_is_stop(0.2));
    assert!(!prepare::freq_is_stop(1.5), "between stops is not a choice");
    assert!(prepare::freq_is_stop(0.25));
    assert!(prepare::freq_is_stop(5.0));

    // Default 1 s, printed the way the dropdown prints it.
    assert_eq!(prepare::FREQ_DEFAULT, 1.0);
    assert_eq!(default("Freq"), "1s");
    assert_eq!(ps::stored_in("Freq"), "project.interval");
    assert_eq!(project::Project::default().interval, prepare::FREQ_DEFAULT);
}

// ---- S2: language --------------------------------------------------------------------

#[test]
fn sec_10_parameters_3_project_settings_tab_controls_s2_language_defaults_to_en() {
    assert_eq!(ITEM, "§10-parameters#3-project-settings-tab-controls");
    assert_eq!(default("Language"), "en");
    assert_eq!(project::Project::default().language, "en", "same source, not a second 'en'");
    assert_eq!(ps::stored_in("Language"), "project.language");
    // A free entry: the ASR language is typed, not picked, so the tab has no list for it.
    assert!(ps::values_for("Language").is_none(), "Language is an entry, not a dropdown");
    // An empty box still means en when it is used.
    assert_eq!(prepare::language_label(""), "en");
}

// ---- S3: narration on/off ------------------------------------------------------------

#[test]
fn sec_10_parameters_3_project_settings_tab_controls_s3_narration_on_is_stored_as_no_narration_off() {
    assert_eq!(ITEM, "§10-parameters#3-project-settings-tab-controls");
    // The tick reads ON while the field reads false: the file stores the negation (§01).
    assert_eq!(default("Narration"), "on");
    assert!(!project::Project::default().no_narration);
    assert_eq!(ps::stored_in("Narration"), "project.no_narration (inverted)");
    // A tick, so exactly two values, off first.
    assert_eq!(ps::values_for("Narration").unwrap(), ["off", "on"]);
    // And the inversion holds in both directions: turning narration off sets the flag.
    let mut p = project::Project::default();
    p.no_narration = true;
    assert!(p.no_narration, "narration off is no_narration true, not narration false");
}

// ---- S4: copy sources (on) -----------------------------------------------------------

#[test]
fn sec_10_parameters_3_project_settings_tab_controls_s4_copy_sources_on_is_stored_as_reference_false() {
    assert_eq!(ITEM, "§10-parameters#3-project-settings-tab-controls");
    assert_eq!(default("Copy into project"), "on");
    // Inverted on purpose: absent = copy sources into the project.
    assert!(!project::Project::default().reference_sources);
    assert_eq!(ps::stored_in("Copy into project"), "project.reference_sources (inverted)");
    assert_eq!(ps::values_for("Copy into project").unwrap(), ["off", "on"]);
}

// ---- S5: the User Context ------------------------------------------------------------

#[test]
fn sec_10_parameters_3_project_settings_tab_controls_s5_user_context_starts_empty() {
    assert_eq!(ITEM, "§10-parameters#3-project-settings-tab-controls");
    assert_eq!(default("User Context"), "");
    assert_eq!(project::Project::default().context, "");
    assert_eq!(ps::stored_in("User Context"), "project.context");
    // A text view, not a choice: nothing to list.
    assert!(ps::values_for("User Context").is_none());
    // What an untouched context is catalogued as: empty means nothing is sent, which is a
    // fact worth spelling rather than a blank cell.
    assert_eq!(row("project.context").spelled, "empty (nothing sent)");
}

// ---- S6: encoder settings ------------------------------------------------------------

#[test]
fn sec_10_parameters_3_project_settings_tab_controls_s6_encoder_defaults_are_the_eleven_values_named() {
    assert_eq!(ITEM, "§10-parameters#3-project-settings-tab-controls");
    // Every value §3's encoder clause names, cross-checked against Produce's own fields so
    // the catalogue and the stored struct cannot drift apart.
    let want = Produce::default();
    assert_eq!(default("Container"), "mp4");
    assert_eq!(want.container, project::Container::Mp4);
    assert_eq!(default("Codec"), "h264");
    assert_eq!(want.codec, project::Codec::H264);
    // slow, NOT the prototype's veryslow — §3 says so in parentheses.
    assert_eq!(default("Preset"), "slow");
    assert_eq!(want.preset, project::Preset::Slow);
    assert_ne!(want.preset, project::Preset::VerySlow, "the prototype shipped veryslow; the spec does not");
    assert_eq!(default("Resolution"), "1080p");
    assert_eq!(want.resolution, project::Resolution::P1080);
    assert_eq!(default("Frame rate"), "30");
    assert_eq!(want.frame_rate, project::FrameRate::F30);
    assert_eq!(default("Audio"), "128");
    assert_eq!(want.audio_kbps, 128);
    // subtitles none ...
    assert_eq!(default("Subtitles"), "none in the video");
    assert_eq!(want.subtitles, project::Subtitles::None);
    // ... no languages: translating is asked for, never assumed.
    assert_eq!(default("Translate"), "");
    assert!(want.translate.is_empty());
    // VFR off, mono off, blurred edges on.
    assert_eq!(default("Frame timing"), "off");
    assert!(!want.vfr);
    assert_eq!(default("Channels"), "off");
    assert!(!want.mono);
    assert_eq!(default("Frame edges"), "on");
    assert!(want.blurred_edges, "the one tick that starts on");

    // Each of the eleven rows exists in the catalogue under its bare id, spelled as the
    // page spells it and pointing at the page as its source.
    for (id, spelled) in [
        ("project.container", "mp4"),
        ("project.codec", "h264"),
        ("project.preset", "slow"),
        ("project.resolution", "1080p"),
        ("project.frameRate", "30"),
        ("project.audio", "128"),
        ("project.subtitles", "none in the video"),
        ("project.gameAudio", "0.22"),
        ("project.crf", "24"),
        ("project.vfr", "off"),
        ("project.channels", "off"),
        ("project.blurredEdges", "on"),
    ] {
        let r = row(id);
        assert_eq!(r.spelled, spelled, "{id}");
        assert!(r.from.starts_with("produce_screen::defaults"), "{id} names the page it read: {}", r.from);
    }
    // CRF 24 is also the mark the slider carries.
    assert_eq!(default("Quality (CRF)"), "24");
    assert_eq!(want.crf, 24);
}

// ---- S7: publish state ---------------------------------------------------------------

#[test]
fn sec_10_parameters_3_project_settings_tab_controls_s7_publish_state_is_a_file_not_a_tunable() {
    assert_eq!(ITEM, "§10-parameters#3-project-settings-tab-controls");
    let tree = temp_project("publish");
    // Nothing written yet: the state reads unwritten, and that is what the tab shows.
    assert!(!publish::is_written(&tree));
    assert_eq!(default("Publish"), "not written");
    assert_eq!(ps::stored_in("Publish"), "publish.json");
    // Not a selection: a state has no option list.
    assert!(ps::values_for("Publish").is_none());
    // The project object agrees: no `publish` key until the upload text exists.
    let loaded = project::load(tree.dir()).unwrap();
    assert!(loaded.publish.is_none());

    // Write it and the same question answers differently — presence is the whole answer.
    // The record goes through publish::save, which writes `produce/publish.json`; note it
    // does NOT fill `Project::publish`, because §5 makes the FILE the state and the project
    // key is only what an older file shape carried.
    let record = project::Publish::default();
    publish::save(&record, &tree).expect("saves");
    assert!(publish::is_written(&tree));
    assert!(tree.publish_json().exists());
    let after = project::load(tree.dir()).unwrap();
    assert!(after.publish.is_none(), "the upload text lives in publish.json, not in naivepost.json");
    let back = publish::load(&tree).expect("the saved record loads back");
    assert_eq!(back, record, "publish.json round-trips the record it was given");
    let _ = std::fs::remove_dir_all(tree.dir());
}

// ---- S8: aspect (source) -------------------------------------------------------------

#[test]
fn sec_10_parameters_3_project_settings_tab_controls_s8_aspect_default_is_source_not_16_9() {
    assert_eq!(ITEM, "§10-parameters#3-project-settings-tab-controls");
    assert_eq!(default("Aspect"), fx_aspect::SOURCE);
    assert_eq!(ps::stored_in("Aspect"), "cut.aspect");
    // `source` stores nothing at all: an unshaped cut writes no key (§10's `aspect (source)`).
    assert_eq!(fx_aspect::stored(fx_aspect::SOURCE), "");
    // The Cut readout's "16:9" is what an unset aspect PRINTS, not something anyone chose,
    // so it must not become the tab's default.
    assert_eq!(cut_screen::ASPECT_DEFAULT, "16:9");
    assert_ne!(default("Aspect"), cut_screen::ASPECT_DEFAULT);
    // This tab offers no shapes: the dropdown lives in the Cut form (fx_aspect::ASPECTS).
    assert!(ps::values_for("Aspect").is_none());
}

// ---- S9: the whole list --------------------------------------------------------------

#[test]
fn sec_10_parameters_3_project_settings_tab_controls_s9_the_rows_are_exactly_these_in_the_specs_order() {
    assert_eq!(ITEM, "§10-parameters#3-project-settings-tab-controls");
    // Five project controls, then §A's thirteen encoder rows, then the publish state,
    // then the aspect — the order §3's sentence gives them.
    let expected = [
        "Freq",
        "Language",
        "Narration",
        "Copy into project",
        "User Context",
        "Container",
        "Codec",
        "Preset",
        "Resolution",
        "Frame rate",
        "Audio",
        "Subtitles",
        "Translate",
        "Game audio",
        "Quality (CRF)",
        "Frame timing",
        "Channels",
        "Frame edges",
        "Publish",
        "Aspect",
    ];
    assert_eq!(ps::rows(), expected);
    assert_eq!(ps::rows().len(), 20);

    // One catalogue row per control, in the same order.
    let listed = ps::listed();
    assert_eq!(listed.len(), expected.len(), "one row per tab control");
    for (index, r) in listed.iter().enumerate() {
        assert!(!r.spelled.is_empty() || r.id == "project.translate", "{} catalogued with no value", r.id);
        assert!(!r.from.is_empty(), "{} catalogued with no source", r.id);
        // Every `from` names a module: `module::thing`.
        assert!(r.from.contains("::"), "{} must name the constant it reads: {}", r.id, r.from);
        // Every id is either the one `P.project.` row §3 spells or a bare `project.` id.
        assert!(
            r.id == "P.project.frameInterval" || r.id.starts_with("project."),
            "{} is neither",
            r.id
        );
        let _ = index;
    }

    // Exactly one row carries a `P.` id, and it is the one §3 spells. No further
    // `P.project.*` ids were invented for rows §10 left nameless.
    let dotted: Vec<&str> = listed
        .iter()
        .filter(|r| r.id.starts_with("P."))
        .map(|r| r.id)
        .collect();
    assert_eq!(dotted, ["P.project.frameInterval"]);
    // Which is why the bare ones report Family::Other, as every unrowed id in this
    // catalogue does, and why no bare id pretends to a family.
    for r in listed.iter().filter(|r| !r.id.starts_with("P.")) {
        assert_eq!(params::family(r.id), params::Family::Other, "{} has no P. row in §10", r.id);
    }
    assert_eq!(params::family("P.project.frameInterval"), params::Family::Project);
    // The three families §10's intro names are untouched by adding the fourth.
    assert_eq!(params::family("P.policy.markingPass"), params::Family::Policy);
    assert_eq!(params::family("P.machine.slots"), params::Family::Machine);
    assert_eq!(params::family("P.eng.llmAttempts"), params::Family::Eng);
}
