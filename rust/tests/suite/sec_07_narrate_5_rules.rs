//! §07-narrate#5-rules, the four clauses this round closed, each pinned through the module that owns it.
//!
//! * the run bar is the preview's transport only once the preview **started** — the rule is
//!   [`naivepost::narrate_rules::bar_serves_the_preview`], but what a person sees is the icon
//!   [`naivepost::run::controls`] paints from what the page published, so both halves are checked and the
//!   page's own publication is driven through the ⏹ door;
//! * a voice not on offer leaves the picker **empty** rather than pointing at the wrong speaker —
//!   [`naivepost::narrate_rules::picker_row`] answers `None`, and the dropdown shows no row;
//! * preview plays the cut and **sound equals the render's** — the tick's numbers come from the same
//!   calls the render makes, and `bed_gain_ask` is what stops a beat that changed nothing from re-cuing;
//! * captions-only / narration-off short-circuits **everything that speaks** — the preview asks
//!   [`naivepost::narrate_off::lines_to_speak`] rather than testing the text itself, so a wav sitting on
//!   disk in a project with the tick off is still not a take the picture will stop for.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, Seg};
use naivepost::narrate_off;
use naivepost::narrate_preview::{self, Failed, Take};
use naivepost::narrate_preview_leg as leg;
use naivepost::narrate_preview_media as media;
use naivepost::narrate_rules as rules;
use naivepost::narrate_screen as page;
use naivepost::narration::Entry;
use naivepost::run::{self, Transport};
use naivepost::shell::Page;
use naivepost::ui;

use crate::common;
use common::{hold_last_window, release_last_window, settle};

static RAN_PICKER: AtomicBool = AtomicBool::new(false);
static RAN_BAR: AtomicBool = AtomicBool::new(false);

// --- the headless halves ---------------------------------------------------------------------------

/// §5 (`the run bar is the preview's transport only once the preview started`): the four transport
/// states, read through the function that draws the bar rather than through the rule's own boolean.
#[test]
fn sec_07_narrate_5_rules_the_bar_follows_the_page_only_once_the_preview_started() {
    // Never cued: ▶ stays the step's button and ⏹ has nothing to end.
    let idle = Transport::default();
    assert!(!rules::bar_serves_the_preview(idle));
    assert_eq!(
        run::controls(&None, run::transport_for(Page::Narrate, idle)).icon,
        run::PLAY_ICON,
        "an untouched Narrate page shows ▶ as \"write and speak\", not as a paused preview"
    );
    assert!(!run::controls(&None, run::transport_for(Page::Narrate, idle)).stop_sensitive);

    // Going: the bar is the preview's, so ▶ is ⏸.
    let playing = Transport {
        playing: true,
        started: true,
    };
    assert!(rules::bar_serves_the_preview(playing));
    assert_eq!(
        run::controls(&None, run::transport_for(Page::Narrate, playing)).icon,
        run::PAUSE_ICON,
        "a started preview turns ▶ into ⏸"
    );

    // Started and paused: still the preview's — ⏸ must be able to resume it and ⏹ to end it.
    let cued = Transport {
        playing: false,
        started: true,
    };
    assert!(rules::bar_serves_the_preview(cued));
    assert!(run::controls(&None, run::transport_for(Page::Narrate, cued)).stop_sensitive);
}

/// §5 (`a voice not on offer leaves the picker empty rather than pointing at the wrong speaker`): the
/// row lookup must answer `None` — and NOT the first row, which is captions-only, because a picker
/// silently showing "no audio" while the project speaks someone else is how the wrong voice reaches a
/// finished video.
#[test]
fn sec_07_narrate_5_rules_a_voice_that_is_not_on_offer_has_no_row() {
    // One narrator tagged, no voices folder: the offer is captions + Narrator 1.
    let options = page::voice_options(1, &[]);
    assert_eq!(
        options[0].id,
        page::CAPTIONS,
        "row 0 is captions-only, which is what a wrong fallback would show"
    );
    assert_eq!(rules::picker_row(&options, "narrator1"), Some(1));
    assert_eq!(
        rules::picker_row(&options, "narrator3"),
        None,
        "a third narrator Prepare never tagged is not on offer, so no row may be shown for it"
    );
    assert_eq!(
        rules::picker_row(&options, "warm.wav"),
        None,
        "and a voices-folder file that is gone is not on offer either"
    );
    // The refusal names where to look, per kind: a slot is tagged on Prepare, a file lives in the folder.
    assert!(rules::missing_voice("narrator3", "/voices").contains("not tagged on the Prepare step"));
    assert!(rules::missing_voice("warm.wav", "/voices").contains("/voices"));
}

/// §5 (`preview plays the cut, sound equals the render's`): the numbers the tick reads for a second are
/// the render's own — the duck appears only while a line speaks — and the bed's level is the product of
/// that duck with the mix. Then the stamp: a beat whose answer did not change must not re-cue.
#[test]
fn sec_07_narrate_5_rules_the_mix_reaches_the_bed_and_a_still_second_requeues_nothing() {
    let game = 0.3;
    let quiet = Cut {
        segs: vec![clip(0.0, 20.0)],
        ..Default::default()
    };
    let silent_second = leg::heard_at(&quiet, 5.0, false, game);
    let spoken_second = leg::heard_at(&quiet, 5.0, true, game);
    assert_eq!(
        silent_second.duck, 1.0,
        "nothing speaking: the bed keeps its own level"
    );
    assert_eq!(
        spoken_second.duck, game,
        "while a line speaks the bed sits at P.policy.gameVolume"
    );
    assert_ne!(
        silent_second.duck, spoken_second.duck,
        "the tick must not read one level for every second"
    );

    // The same product the render would mix, through the media layer the players are cued from.
    let idle_gain = media::bed_gain(&quiet, 5.0, false, game, 1.0);
    let ducked_gain = media::bed_gain(&quiet, 5.0, true, game, 1.0);
    assert!(
        ducked_gain < idle_gain,
        "the level handed to the player has to drop while the line speaks: {idle_gain} -> {ducked_gain}"
    );
    assert_eq!(
        ducked_gain, game,
        "with the slider at full the bed is exactly the duck"
    );
    // A hushed second is mute by property, not a paused stream (§05-cut#8 bullet 1).
    let hushed = Cut {
        segs: vec![Seg {
            quiet: vec!["cam0".into()],
            ..clip(0.0, 20.0)
        }],
        ..Default::default()
    };
    assert_eq!(media::bed_gain(&hushed, 5.0, false, game, 1.0), 0.0);
    assert_eq!(
        leg::heard_at(&hushed, 5.0, false, game).hushed,
        vec!["cam0".to_string()]
    );

    // The stamp: asking for the level the bed already runs at answers `false`, so the tick spawns nothing.
    let mut preview = leg::Preview::default();
    assert!(
        preview.bed_gain_ask(ducked_gain),
        "a first answer is always a change"
    );
    assert!(
        !preview.bed_gain_ask(ducked_gain),
        "the same answer twice must not re-cue the bed"
    );
    assert!(
        preview.bed_gain_ask(idle_gain),
        "and a new duck is a change again"
    );
}

/// §5 (`captions-only short-circuits everything that speaks`) + F4.8: the preview asks the ONE seam
/// which lines may be spoken, so with the tick off a wav that really exists on disk is still not a take
/// the picture would stop for, and a deliberately blank line is not either.
#[test]
fn sec_07_narrate_5_rules_a_narration_that_is_off_offers_no_line_to_speak() {
    let (_root, tree) = common::tree_in("rules-seams");
    let spoken = Entry {
        s: 0.0,
        e: 10.0,
        at: 0.5,
        text: "a written line".into(),
        ..Default::default()
    };
    let blank = Entry {
        s: 10.0,
        e: 20.0,
        at: 0.5,
        text: String::new(),
        ..Default::default()
    };
    let entries = vec![spoken.clone(), blank.clone()];

    // The seam itself, in both states.
    assert_eq!(
        narrate_off::lines_to_speak(true, &entries),
        Vec::new(),
        "narration off speaks nothing"
    );
    assert_eq!(
        narrate_off::lines_to_speak(false, &entries),
        vec![spoken.clone()],
        "a blank line is a deliberate silence, never a spoken one"
    );

    // A real wav on disk for the line the seam excludes.
    let key = naivepost::narration::key_for(&spoken, Some(&leg::voice()));
    let wav = naivepost::narrate_tts::take_path(&tree, &key);
    std::fs::create_dir_all(wav.parent().expect("tts/ has a parent")).expect("tts/");
    std::fs::write(&wav, b"stub wav bytes").expect("a take on disk");
    let failed = Failed::default();

    assert_eq!(
        leg::take_states(&tree, &entries, &failed, false),
        vec![Take::Exists, Take::Missing],
        "narration on: the written line's take exists, the blank one has none to look for"
    );
    assert_eq!(
        leg::take_states(&tree, &entries, &failed, true),
        vec![Take::Missing, Take::Missing],
        "narration off must offer no line even though the wav is really on disk"
    );
    assert!(
        leg::take_states(&tree, &entries, &failed, true)
            .iter()
            .all(|take| !matches!(take, Take::Exists)),
        "not one Exists for a project whose narration is switched off"
    );
}

// --- the widget halves (one main loop, one scenario each) ------------------------------------------

fn run_round(app: &adw::Application) {
    unsafe {
        // This container has no decoder; the seam stays the leg's own spawn (see f4_5_transport_widgets).
        std::env::set_var("NAIVEPOST_PICTURE_PLAYER", "/bin/true");
        std::env::set_var("NAIVEPOST_VOICE_PLAYER", "/bin/true");
    }
    run_picker_empty(app);
    run_bar_follows_the_page(app);
}

/// One scenario per `#[test]`: libgio lets only the first `run_with_args` acquire the default main
/// context, so the first test drives the whole round and each test asserts its own flag.
fn run_scenario(flag: &AtomicBool) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gtk::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(run_round);
        app.run_with_args(&["naivepost"]);
    });
    assert!(
        flag.load(Ordering::SeqCst),
        "the scenario never reached its last assertion"
    );
}

/// A session folder pinned as the process's cwd, written BEFORE any window is built: the page resolves
/// its session through `startup::session_dir(current_dir())`, so the tree the assertions read is that
/// one and not a folder named by hand.
fn session() -> naivepost::layout::Tree {
    let root = std::env::temp_dir().join(format!("np-rules5-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root");
    std::env::set_current_dir(&root).expect("cwd pinned");
    let dir = std::env::current_dir()
        .expect("cwd just pinned")
        .join(naivepost::startup::SESSION_NAME);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("session folder");
    naivepost::layout::Tree::new(&dir).expect("the folder is a project")
}

fn state_with(voice: &str, narrators: usize, files: Vec<String>) -> ui::NarrateState {
    ui::NarrateState {
        entries: vec![],
        segs: vec![clip(0.0, 20.0)],
        takes: vec![],
        voice: voice.into(),
        pitch: 0.0,
        session: 5.0,
        cut_at: 5.0,
        length: 20.0,
        clips: 1,
        unwritten: 0,
        off_cut: 0,
        has_cut: true,
        has_timeline: true,
        narrators,
        voice_files: files,
        recording_covers_clip: true,
        playing: false,
        started: false,
        busy: false,
        narration_off: false,
        language: String::new(),
        covered_spans: vec![],
    }
}

/// A project whose only row is a footage recording: no narrator is tagged, so the offer holds
/// captions-only and whatever the voices folder adds — never a narrator slot.
fn footage_project() -> naivepost::project::Project {
    naivepost::project::Project {
        sources: vec![naivepost::project::Source {
            path: "cam0.mp4".into(),
            footage: true,
            ..Default::default()
        }],
        ..Default::default()
    }
}

fn narrate_window(
    app: &adw::Application,
    model: &naivepost::project::Project,
) -> adw::ApplicationWindow {
    release_last_window();
    let window = ui::build_window(app, model, "Narrate");
    hold_last_window(window.clone());
    window.set_default_size(1334, 900);
    window.present();
    ui::tab_button(&window, Page::Narrate)
        .expect("the shell has a Narrate tab")
        .emit_by_name::<()>("clicked", &[]);
    settle();
    // NOT `set_state` + `refresh`: publishing a state and repainting would press the page's OWN voice
    // seam (`wire`'s `selected-notify`, which the seeding flag does not cover once the model is already
    // the right list), and that seam is a hand on the dropdown — it would switch the voice and destroy
    // what this check is about. The build below is the state the project itself states, through the
    // seed the page does at build time.
    window
}

/// The rows the dropdown actually carries. §5's "leaves the picker EMPTY" is about these: a stored
/// voice that is not on offer must not be shown as a row of its own, and must not be quietly mapped
/// onto row 0 either.
fn picker_rows(picker: &gtk::DropDown) -> Vec<String> {
    let Some(list) = picker
        .model()
        .and_then(|m| m.downcast::<gtk::StringList>().ok())
    else {
        return Vec::new();
    };
    (0..list.n_items())
        .filter_map(|row| {
            list.item(row)
                .and_then(|item| item.downcast::<gtk::StringObject>().ok())
                .map(|o| o.string().to_string())
        })
        .collect()
}

/// §5's empty-picker clause at the WIDGET: a project that stores a voice the offer no longer holds.
/// The picker must NOT point at captions-only for it. Dropping the list to empty would be a worse
/// face than the truth (§05: "never empty — with no voices the picker still has its captions-only
/// row"), so the rule pins that an unoffered voice resolves to no row at all.
fn run_picker_empty(app: &adw::Application) {
    let tree = session();
    // Published BEFORE the build: the page seeds its voice from `narrate/voice.txt` at build time.
    naivepost::narrate_data::write_voice(&tree, "narrator3").expect("a stored voice id");
    let window = narrate_window(app, &footage_project());
    let state = ui::read_state();
    let files: Vec<&str> = state.voice_files.iter().map(String::as_str).collect();
    let offered = page::voice_options(state.narrators, &files);
    assert_eq!(
        offered.len(),
        1,
        "nothing was tagged, so the offer is captions-only alone and narrator3 is NOT on it"
    );
    // §05's "never empty": the captions-only row stays visible. What §5 pins is that the unoffered
    // voice is not on the list at all, so nothing can be shown for it and nothing maps onto row 0.
    let shown = common::widget(&window, "voice-picker");
    let picker = shown
        .downcast_ref::<gtk::DropDown>()
        .expect("`voice-picker` is a dropdown");
    let rows = picker_rows(picker);
    assert_eq!(
        rows,
        offered.iter().map(|o| o.label.clone()).collect::<Vec<_>>(),
        "the dropdown carries exactly the rows the offer holds, in its order"
    );
    assert!(
        !rows.iter().any(|row| row.contains("Narrator 3")),
        "a voice that is not on the offer has no row in the picker"
    );
    assert!(
        naivepost::narrate_rules::picker_row(&offered, &ui::read_state().voice).is_none(),
        "the stored voice resolves to NO row (§5: the picker is left empty of it) — it does NOT fall
        back to row 0, which would read 'no audio' while the page still speaks the vanished voice"
    );
    // And the refusal says which: an id the offer does not hold is refused at the rule that decides
    // whether a sample can be spoken at all (§F4.6's S1 refusal), not silently replayed.
    assert!(
        naivepost::narrate_rules::picker_row(&offered, "narrator99").is_none(),
        "an id off the list has no row, so ▶ Sample has nothing to ask for"
    );
    assert_eq!(
        ui::read_state().voice, "narrator3",
        "clearing the row must not rewrite the voice the page holds: the refusal says which one is missing"
    );

    // The same dropdown, once the voice IS on offer: the row comes back, so the empty answer is the
    // rule's, not a picker that can never select anything. The page owns its `narrators` count, so this
    // publish goes through `set_state` on a page whose stored voice is the one now on the list — no seam
    // can fire on a row that already matches.
    let mut offered = ui::read_state();
    offered.voice = "narrator3".into();
    offered.narrators = 3;
    ui::set_state(offered);
    ui::refresh(&window);
    settle();
    let rows = page::voice_options(3, &[]);
    let want = rows
        .iter()
        .position(|option| option.id == "narrator3")
        .expect("three narrators published means row 3 exists");
    let shown = common::widget(&window, "voice-picker");
    let picker = shown
        .downcast_ref::<gtk::DropDown>()
        .expect("`voice-picker` is a dropdown");
    let now = picker_rows(picker);
    assert_eq!(
        now,
        rows.iter().map(|o| o.label.clone()).collect::<Vec<_>>(),
        "the picker carries the whole offer now, in its order"
    );
    assert!(
        now.contains(&rows[want].label),
        "once the voice is on offer its row is in the picker again"
    );
    assert_eq!(picker.selected() as usize, want, "and on its row");
    RAN_PICKER.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §5's bar clause at the PAGE: a window that never cued a preview publishes `started: false`, and ⏹
/// puts it back there after a preview ran. The bar reads only what the page published, so this is the
/// half that makes the rule visible rather than merely true.
fn run_bar_follows_the_page(app: &adw::Application) {
    let _tree = session();
    let window = narrate_window(app, &footage_project());
    // Fresh window: the slot exists (the page registered it) but says the preview never started.
    assert!(
        !rules::bar_serves_the_preview(ui::preview_player(&window).transport),
        "a Narrate window nobody has played leaves ▶ as the step's button"
    );
    assert!(
        !ui::read_state().started,
        "and the page itself does not claim a started preview"
    );

    // Drive the preview through the real doors, with the cueable footage the picture needs.
    let segs = vec![clip(0.0, 20.0)];
    naivepost::cut::save(
        &naivepost::cut::Cut {
            segs: segs.clone(),
            ..Default::default()
        },
        &_tree,
    )
    .expect("cut.json written");
    std::fs::write(
        std::env::current_dir().unwrap().join("cam0.mp4"),
        b"stub footage",
    )
    .expect("footage on disk");
    naivepost::project::save(&footage_project(), _tree.dir()).expect("naivepost.json written");
    // The play seam reads the CUT off disk and the record off the page's state, so the state goes in
    // here (after the cut and the footage exist) — no voice seam can fire on a page with no voice.
    let mut playing = state_with("", 1, vec![]);
    playing.segs = segs;
    playing.entries = vec![Entry {
        s: 0.0,
        e: 20.0,
        at: 1.0,
        text: "one line".into(),
        ..Default::default()
    }];
    naivepost::narration::save(
        &naivepost::narration::Narration {
            entries: playing.entries.clone(),
            silent: Vec::new(),
        },
        &_tree,
    )
    .expect("narration.json written");
    ui::set_state(playing);
    ui::refresh(&window);
    settle();
    ui::press_narrate_play(&window);
    settle();
    assert!(
        ui::read_state().playing,
        "▶ played the cut (or this check proves nothing)"
    );
    assert!(
        rules::bar_serves_the_preview(ui::preview_player(&window).transport),
        "once the preview started the bar answers for it"
    );

    naivepost::ui::narrate_page::press_preview_stop(&window);
    settle();
    let after = ui::preview_player(&window).transport;
    assert!(!after.playing, "⏹ stopped both players");
    assert!(
        !rules::bar_serves_the_preview(after),
        "and ⏹ ENDS the preview: the bar goes back to the step rather than serving a dead preview"
    );
    assert!(
        !ui::read_state().started,
        "the page clears its own flag, so a re-press starts fresh instead of thawing nothing"
    );
    RAN_BAR.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

#[test]
fn sec_07_narrate_5_rules_an_unoffered_voice_leaves_the_picker_empty() {
    run_scenario(&RAN_PICKER);
}

#[test]
fn sec_07_narrate_5_rules_the_bar_serves_the_preview_only_once_it_started() {
    run_scenario(&RAN_BAR);
}

/// A kept footage clip off camera 0.
fn clip(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        dur: e - s,
        cam: 0,
        ..Default::default()
    }
}

/// `duck` reaches the bed only through the preview's stamp; a hush is `0.0`. Pinning that `narrate_preview::duck`
/// is what the media layer multiplies, so the two modules cannot drift apart in a later retune.
#[test]
fn sec_07_narrate_5_rules_the_duck_rule_is_the_render_s() {
    assert_eq!(narrate_preview::duck(false, 0.3), 1.0);
    assert_eq!(narrate_preview::duck(true, 0.3), 0.3);
    let quiet = Cut {
        segs: vec![clip(0.0, 10.0)],
        ..Default::default()
    };
    assert_eq!(
        media::bed_gain(&quiet, 4.0, true, 0.0, 1.0),
        0.0,
        "a game volume of zero is a bed that ducks out of the way entirely"
    );
}
