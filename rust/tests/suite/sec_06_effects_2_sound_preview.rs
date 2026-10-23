// §06-effects#2-the-lane-and-the-preview — the sound answer a SPEED carries, read by the PREVIEW MIXER.
//
// `fx_lane::mute_silences_the_preview` and its twin `previewable_sound` are the rules; what §2 asks for is that
// the preview OBEYS them: "the two 1× sound answers cannot be previewed; mute mutes the preview"
// (inventory/effects.md §A.3), and §9's row `Speed, sound mute; Stop … MUST: silence the whole bed, as the
// preview does`. A rule nothing calls is a description of the program, so every test here goes through the door
// the tick goes through — `cut_hear::mix_at`, whose `gain` is the number handed to every pipeline — plus the two
// other doors that ask the same question while paused (`fx_volume::heard_while_paused`) and while playing
// (`cut_play_leg::route`). Plain data throughout: no widget, no display, no socket.

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, Fx, Lane, Seg};
use naivepost::ui;
use naivepost::{cut_hear, cut_play, cut_play_leg, fx_record, fx_volume};

use crate::common;
use crate::common::settle;

/// One speed effect, at `t` for `dur` seconds, carrying `snd` as its sound answer.
fn speed(t: f64, dur: f64, rate: f64, snd: &str) -> Fx {
    Fx {
        kind: "speed".into(),
        t,
        dur,
        rate,
        snd: snd.into(),
        ..Default::default()
    }
}

/// The footage the line runs over: one scene covering 0..20, heard, so a silence can only come from an effect.
fn footage() -> Vec<Seg> {
    vec![Seg {
        s: 0.0,
        e: 20.0,
        ..Default::default()
    }]
}

/// One separate recording running the whole time, so "the whole bed" has more than the footage in it.
fn lanes() -> Vec<Lane> {
    vec![Lane {
        name: "mic-lav".into(),
        src: "project:audio/mic-lav.wav".into(),
        at: 0.0,
        off: 0.0,
        dur: 20.0,
    }]
}

/// §06-effects#2 — `mute mutes the preview`: the mix the tick reads goes to zero across the silent stretch and
/// nowhere else, and only `mute` does it.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s8_mute_silences_the_preview_mix() {
    let muted = speed(10.0, 4.0, 2.0, "mute");
    assert!(
        fx_record::snd_is_known(&muted.snd),
        "`mute` is one of §1's answers"
    );

    // The mix runs at the first covering speed's rate (§A.3), so what travels beside the silence is ×2.
    let inside = cut_hear::mix_at(
        &footage(),
        &lanes(),
        std::slice::from_ref(&muted),
        11.0,
        1.0,
    );
    assert_eq!(
        inside.rate, 2.0,
        "the picture still runs at ×2 under the silence"
    );
    // Inside: the whole bed, gain 0, and the mix SAYS which rule did it.
    assert!(
        inside.bed_silenced,
        "a speed asking for silence silences the bed at 11 s"
    );
    assert_eq!(
        inside.gain, 0.0,
        "every pipeline is asked for nothing — footage AND lane together"
    );
    // The scene's own mute property is NOT borrowed for this (§F2.5 S2 keeps that flag for the scene).
    assert!(
        !inside.footage_muted,
        "the footage was not muted by its scene; a speed answered instead"
    );
    // The lane is still told to start: it is silenced by the gain, exactly as the render's whole-bed
    // `volume=0` expression does it (§9), not by refusing the pipeline.
    assert_eq!(
        inside.lane_starts.len(),
        1,
        "the heard lane is still part of the answer"
    );

    // Outside on either side the bed plays at the slider again.
    for outside in [9.0, 14.0, 15.0] {
        let mix = cut_hear::mix_at(
            &footage(),
            &lanes(),
            std::slice::from_ref(&muted),
            outside,
            1.0,
        );
        assert!(!mix.bed_silenced, "nothing covers {outside} s");
        assert!(
            mix.gain > 0.0,
            "{outside} s is not silent: gain {}",
            mix.gain
        );
    }

    // Only `mute` silences. The other four answers all leave the bed running: `""` and `pitch` move with the
    // picture's clock, `own`/`scene` are the pair the preview cannot copy (and copying badly beats muting),
    // and no speed at all says nothing about silence.
    for snd in ["", "pitch", "own", "scene"] {
        let mix = cut_hear::mix_at(
            &footage(),
            &lanes(),
            &[speed(10.0, 4.0, 2.0, snd)],
            11.0,
            1.0,
        );
        assert!(!mix.bed_silenced, "`{snd}` is not a request for silence");
        assert!(mix.gain > 0.0, "`{snd}` leaves the bed audible");
    }
    assert!(
        !cut_hear::bed_silenced_under(&[], 11.0),
        "no effect, no silence"
    );
}

/// §06-effects#2 + inventory/effects.md §A.3 (`The earliest covering effect's answer wins`): two speeds over one
/// second are decided by the first listed, for silence exactly as for the rate.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s8_the_first_covering_speed_answers_for_silence() {
    let earlier = speed(9.0, 8.0, 2.0, "");
    let later = speed(11.0, 2.0, 2.0, "mute");
    // Listed first, starting earlier: `""` wins at 12 s, so the `mute` underneath it is not heard.
    let both = [earlier.clone(), later.clone()];
    assert!(
        !cut_hear::mix_at(&footage(), &lanes(), &both, 12.0, 1.0).bed_silenced,
        "the earliest covering answer is the empty one, so the bed stays audible"
    );
    // Same two effects, opposite order: the walk takes the FIRST COVERING one, and at 12 s both cover it.
    let swapped = [later.clone(), earlier.clone()];
    assert!(
        cut_hear::mix_at(&footage(), &lanes(), &swapped, 12.0, 1.0).bed_silenced,
        "listed first, the silent answer is the one that decides"
    );
    // Where only the later one reaches, it decides whatever the order.
    for order in [&both, &swapped] {
        assert!(
            cut_hear::mix_at(&footage(), &lanes(), order, 12.5, 1.0).bed_silenced
                == cut_hear::bed_silenced_under(order, 12.5),
            "the mix and the rule agree at 12.5 s whatever the order"
        );
    }
    // A stop needs no special case: §1 migrated a legacy silence to `snd: "mute"`, so a silent stop arrives here
    // already carrying the answer this walk reads.
    let stop = speed(10.0, 2.0, 0.0, "mute");
    assert!(fx_record::is_stop(&stop), "rate 0 is a stop");
    assert!(
        cut_hear::bed_silenced_under(std::slice::from_ref(&stop), 11.0),
        "and a silent stop silences"
    );
    let loud_stop = speed(10.0, 2.0, 0.0, "");
    assert!(
        !cut_hear::bed_silenced_under(std::slice::from_ref(&loud_stop), 11.0),
        "a stop that kept its sound is not silence"
    );
}

/// §06-effects#2 (`Only "1× to the scene's end" draws the dashed debt tail`), read off the real lane bar: the
/// scene-end record's bar wears the dashed tail and its effect-end neighbour does not, and the two differ ONLY in
/// the sound answer — which is the pair §2 says cannot be previewed. `sec_06_effects_2_lane_preview_wired.rs`
/// pins the same CSS answer from the wired file's own single activation; this file's checks are the silent-speed
/// ones, so the tail pair is repeated here through the same doors (`set_line_position` + `refresh_effects_lane`)
/// rather than being left to a binary that may have stopped for an unrelated reason.
fn the_scene_end_sound_wears_the_tail_and_its_neighbour_does_not(app: &adw::Application) {
    let scene_snd = speed(10.0, 5.0, 1.0, "scene");
    let own_snd = speed(20.0, 5.0, 1.0, "own");
    let mut seeded = Cut {
        segs: vec![Seg {
            s: 0.0,
            e: 60.0,
            ..Default::default()
        }],
        ..Default::default()
    };
    seeded.fx.push(scene_snd);
    seeded.fx.push(own_snd);
    let window = common::cut_page(app, &seeded);
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 12.0 });
    ui::refresh_effects_lane(&window);
    settle();
    println!(
        "TAIL CLASSES fx-bar-0={} fx-bar-1={}",
        common::widget(&window, "fx-bar-0").has_css_class("fx-debt-tail"),
        common::widget(&window, "fx-bar-1").has_css_class("fx-debt-tail")
    );
    assert!(
        common::widget(&window, "fx-bar-0").has_css_class("fx-debt-tail"),
        "the `scene` bar carries the dashed debt tail (§2: only the scene's end leaves a gap to point at)"
    );
    assert!(
        !common::widget(&window, "fx-bar-1").has_css_class("fx-debt-tail"),
        "the `own` bar next to it never does: its sound closes on the record's last frame"
    );
    window.close();
    settle();
    RAN_TAIL.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// §06-effects#2's dashed debt tail on the real lane bar: the `scene` record's bar wears it, the `own` neighbour
/// beside it does not. Asserted through `run_once`, like every other check in this binary.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s8_only_the_scene_end_sound_wears_the_tail() {
    run_once();
    assert_eq!(stopped_early(), None, "the activation stopped early");
    assert!(
        RAN_TAIL.load(std::sync::atomic::Ordering::SeqCst),
        "the tail check never finished"
    );
}

/// §06-effects#2 (`mute mutes the preview`) + §A.6 (`Preview gain applies even paused`): the PAUSED door asks the
/// same question, so parking the red line inside a silent stretch gives silence without a moving frame.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s8_a_parked_line_in_a_silent_stretch_is_silent() {
    let muted = speed(10.0, 4.0, 2.0, "mute");
    assert_eq!(
        fx_volume::heard_while_paused(std::slice::from_ref(&muted), 11.0),
        0.0,
        "paused mid-silence"
    );
    assert!(
        fx_volume::heard_while_paused(std::slice::from_ref(&muted), 9.0) > 0.0,
        "paused outside it"
    );
    // Still volume's own rule where no speed asks for silence: the two doors cannot grow separate answers.
    let loud = Fx {
        kind: "volume".into(),
        t: 12.0,
        dur: 4.0,
        gain: 2.0,
        ..Default::default()
    };
    assert_eq!(
        fx_volume::heard_while_paused(std::slice::from_ref(&loud), 14.0),
        cut_hear::gain_under(std::slice::from_ref(&loud), 14.0),
        "a raised passage is still heard raised"
    );
    // …and a silent speed outranks it over the seconds they share: the bed is the bed.
    assert_eq!(
        fx_volume::heard_while_paused(&[loud0(), muted], 13.0),
        0.0,
        "silence over a boost"
    );
}

/// A volume effect raised to ×2 over 12..16 — for the last check above, kept out of its assertions.
fn loud0() -> Fx {
    Fx {
        kind: "volume".into(),
        t: 12.0,
        dur: 4.0,
        gain: 2.0,
        ..Default::default()
    }
}

// --- the two 1× sound answers: said about while they are being heard --------------------------------------------

/// Set by the wired check when it reaches its last line, and read by both entry tests. Shared across threads on
/// purpose: GTK runs `connect_activate` on a thread of its own (observed `ThreadId(5)` against the test's
/// `ThreadId(3)`), so a flag set inside the activation is invisible to the entry test unless it is shared.
static RAN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The same for the tail check, which is the second step of the one activation.
static RAN_TAIL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// …and for the two checks that place a speed by CLICKING the toolbar and the form's own buttons.
static RAN_SCENE_CLICK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static RAN_MUTE_CLICK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Why the activation stopped, if it did — a panic inside GTK's activate is non-unwinding, so without this an
/// entry test reports only "never ran". Same shape as `sec_06_effects_2_lane_preview_wired.rs`.
static STOP_REASON: std::sync::LazyLock<std::sync::Arc<std::sync::Mutex<Option<String>>>> =
    std::sync::LazyLock::new(|| std::sync::Arc::new(std::sync::Mutex::new(None)));

/// The wording §9 fixes, read from its single source rather than paraphrased here.
fn render_only_sentence() -> &'static str {
    naivepost::effect_overlap::says_render_only("scene").expect("`scene` earns the sentence")
}

/// What `apply_mix`'s own door hands the log for a sound answer — `preview_sound::report_sound_clock_change`
/// called through the same seam the tick calls it, so a check reads what that door CHOSE to say. The wording is
/// its own to pick (and `says_render_only`'s to hold); this only catches it.
fn notice_for(snd: &str) -> Option<String> {
    let said = std::cell::RefCell::new(Vec::<String>::new());
    ui::preview_sound::report_sound_clock_change(
        false,
        !naivepost::fx_lane::previewable_sound(snd),
        snd,
        &|line| said.borrow_mut().push(line.to_string()),
    );
    said.into_inner().into_iter().next()
}

/// §06-effects#2 (`the two 1× sound answers cannot be previewed`; §9's `it SHOULD say "sound at 1× in the render
/// only"`), through the door the tick goes through: a speed whose sound needs its own clock is said about from
/// `ui::apply_mix`, ONCE on entry — not once per 100 ms, and not only when the speed form placed it.
fn a_two_clock_speed_says_so_in_the_preview(app: &adw::Application) {
    let window = common::cut_page(app, &Cut::default());
    let scene_speed = speed(10.0, 4.0, 2.0, "scene");
    // The mix itself carries the question, which is what lets any door ask it.
    let asked = cut_hear::mix_at(
        &footage(),
        &lanes(),
        std::slice::from_ref(&scene_speed),
        11.0,
        1.0,
    );
    assert!(
        asked.sound_needs_two_clocks,
        "a `scene` speed needs a clock the preview does not have"
    );
    // §9's sentence, in §9's words, and only for the two answers that need a second clock.
    println!("PREVIEW NOTICE: {}", render_only_sentence());
    assert_eq!(notice_for("scene").as_deref(), Some(render_only_sentence()));
    assert_eq!(
        notice_for("own").as_deref(),
        naivepost::effect_overlap::says_render_only("own")
    );
    for snd in ["", "pitch", "mute"] {
        assert_eq!(notice_for(snd), None, "`{snd}` owes no sentence");
    }
    // Before it (9 s): nothing owed. Then three ticks INSIDE it: one sentence for the entry, not three — which
    // is what the state the door keeps on the player is FOR, exactly as `report_mute_change`'s twin avoids
    // printing one mute eight times per second.
    ui::apply_mix(&window, &footage(), &lanes(), &[], 9.0);
    assert!(
        !ui::preview_sound::sound_clock_noticed(),
        "9 s is an ordinary stretch"
    );
    for _ in 0..3 {
        let mix = ui::apply_mix(
            &window,
            &footage(),
            &lanes(),
            std::slice::from_ref(&scene_speed),
            11.0,
        );
        assert!(
            mix.sound_needs_two_clocks,
            "the tick's own answer carries the question"
        );
    }
    assert!(
        ui::preview_sound::sound_clock_noticed(),
        "the door remembers it has already said this — a 100 ms tick would otherwise print it ten times a second"
    );
    // Walking out and back in earns it again: a per-entry notice, not a once-per-process one.
    ui::apply_mix(&window, &footage(), &lanes(), &[], 16.0);
    assert!(
        !ui::preview_sound::sound_clock_noticed(),
        "leaving the stretch clears the remembered notice"
    );
    ui::apply_mix(
        &window,
        &footage(),
        &lanes(),
        std::slice::from_ref(&scene_speed),
        11.0,
    );
    assert!(
        ui::preview_sound::sound_clock_noticed(),
        "re-entering says it again"
    );
    // And the answers the preview CAN copy leave that state alone: promising something it is already doing would
    // be worse than silence.
    for snd in ["", "pitch", "mute"] {
        let effect = speed(10.0, 4.0, 2.0, snd);
        let mix = ui::apply_mix(
            &window,
            &footage(),
            &lanes(),
            std::slice::from_ref(&effect),
            11.0,
        );
        assert!(
            !mix.sound_needs_two_clocks,
            "`{snd}` previews as the render does"
        );
        assert!(
            !ui::preview_sound::sound_clock_noticed(),
            "`{snd}` says nothing about the clock"
        );
    }
    window.close();
    settle();
    RAN.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Mark a stretch on the selection ground — the same seam a drag leaves behind — so the Speed form has seconds
/// to work on. Same call `tests/suite/speed_form_widgets.rs` makes for its own S1.
fn mark_band(window: &adw::ApplicationWindow, from: f64, to: f64) {
    let band = ui::draw_selection(
        window,
        naivepost::cut_select::Surface::SelectionBand,
        None,
        from,
        to,
    );
    assert!(band.is_some(), "the band was drawn");
    settle();
}

/// Choose a dropdown row the way the toolkit delivers a popover pick: `set_selected` emits `notify::selected`,
/// the signal a click inside the popover raises. Copied from `speed_form_widgets.rs`, which defines it locally.
fn choose(window: &adw::ApplicationWindow, name: &str, index: usize) {
    common::widget(window, name)
        .downcast::<gtk::DropDown>()
        .unwrap_or_else(|_| panic!("`{name}` is a DropDown"))
        .set_selected(index as u32);
    settle();
}

/// Open the Speed form over `from..to` by CLICKING the toolbar's ✚ Effect row and its Speed entry, pick the sound
/// answer at `sound_row` (`cut_speed::SOUND_CHOICES`: 0 with-the-picture, 1 pitched, 2 own, 3 scene, 4 Silent),
/// set the length, and press Apply. Nothing here calls a rule directly — every step is a named widget.
fn place_speed_by_click(
    app: &adw::Application,
    from: f64,
    to: f64,
    sound_row: usize,
    length: &str,
) -> adw::ApplicationWindow {
    let window = common::cut_page(app, &Cut::default());
    mark_band(&window, from, to);
    common::click(&window, "effect-item-speed");
    settle();
    choose(&window, "speed-field-sound", sound_row);
    common::entry(&window, "speed-field-length").set_text(length);
    common::click(&window, "speed-apply-button");
    settle();
    window
}

/// The one speed record a placed cut carries, or a message naming what it holds instead.
fn only_speed(cut: &Cut) -> &naivepost::cut::Fx {
    let speeds: Vec<&naivepost::cut::Fx> = cut
        .fx
        .iter()
        .filter(|effect| effect.kind == "speed")
        .collect();
    assert_eq!(
        speeds.len(),
        1,
        "Apply was to place exactly one speed: {:?}",
        cut.fx
            .iter()
            .map(|f| (&f.kind, f.t, f.dur))
            .collect::<Vec<_>>()
    );
    speeds[0]
}

/// §06-effects#2 (`the two 1× sound answers cannot be previewed`), reached the way a person reaches it: ✚ Effect
/// ▾ → Speed on the toolbar, the sound row chosen in the form, Apply pressed — and then the preview's OWN mix
/// door asked what it can do with what that click wrote.
fn placed_scene_sound_speed_is_said_about_in_the_preview(app: &adw::Application) {
    // A 4 s band at 30..34, "1× to the scene's end" (row 3), length 4.
    let window = place_speed_by_click(app, 30.0, 34.0, 3, "4");
    let cut = ui::review_cut_of(&window);
    let placed = only_speed(&cut);
    assert_eq!(
        placed.snd, "scene",
        "the row the click chose stored the scene-end answer"
    );
    let inside = placed.t + placed.dur / 2.0;
    // The tick's own door, at a second inside what the click placed.
    let mix = ui::apply_mix(&window, &cut.segs, &cut.lanes, &cut.fx, inside);
    assert!(
        mix.sound_needs_two_clocks,
        "a placed `scene` speed is one the preview cannot copy (§2)"
    );
    assert!(
        ui::preview_sound::sound_clock_noticed(),
        "entering the stretch said §9's sentence once: {}",
        render_only_sentence()
    );
    // Another tick INSIDE the same stretch says nothing again — the notice is per entry, not per 100 ms.
    for _ in 0..3 {
        ui::apply_mix(&window, &cut.segs, &cut.lanes, &cut.fx, inside);
    }
    let lines = std::cell::RefCell::new(Vec::new());
    ui::preview_sound::report_sound_clock_change(true, true, "scene", &|line| {
        lines.borrow_mut().push(line.to_string())
    });
    assert!(
        lines.borrow().is_empty(),
        "already noticed, so a further tick adds no line: {:?}",
        lines.borrow()
    );
    println!(
        "CLICK PLACED snd={} at {:.1} for {:.1}s -> {}",
        placed.snd,
        placed.t,
        placed.dur,
        render_only_sentence()
    );
    window.close();
    settle();
}

/// §06-effects#2 (`mute mutes the preview`) + §9's `silence the whole bed`, again through the click path: the
/// Silent row lands `snd: "mute"`, and both preview doors answer silence for the whole bed — the playing mix and
/// a parked line alike (§A.6 "Preview gain applies even paused").
fn placed_silent_speed_silences_the_preview_bed(app: &adw::Application) {
    let window = place_speed_by_click(app, 30.0, 34.0, 4, "4");
    let cut = ui::review_cut_of(&window);
    let placed = only_speed(&cut);
    assert_eq!(
        placed.snd, "mute",
        "the Silent row stores the mute answer (§1's migration spelling)"
    );
    let inside = placed.t + placed.dur / 2.0;
    let mix = ui::apply_mix(&window, &cut.segs, &cut.lanes, &cut.fx, inside);
    assert!(
        mix.bed_silenced,
        "the mix says the bed is silenced under this speed"
    );
    assert_eq!(
        mix.gain, 0.0,
        "every pipeline is asked for nothing — footage AND lane together"
    );
    assert_eq!(
        ui::paused_preview_gain(&window, inside),
        0.0,
        "and a line PARKED inside the silent stretch is silent too (§A.6)"
    );
    let outside = ui::apply_mix(&window, &cut.segs, &cut.lanes, &cut.fx, placed.t - 2.0);
    assert!(
        !outside.bed_silenced && outside.gain > 0.0,
        "nothing covers the second before it: gain {}",
        outside.gain
    );
    window.close();
    settle();
}

/// The item's name, carried by the check above: the click path proves the wire, the logic tests prove the rule.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s5_a_placed_scene_sound_speed_is_said_about_in_the_preview(
) {
    run_once();
    assert_eq!(stopped_early(), None, "the activation stopped early");
    assert!(
        RAN_SCENE_CLICK.load(std::sync::atomic::Ordering::SeqCst),
        "the clicked scene-sound check never finished"
    );
}

#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s5_a_silent_speed_silences_the_preview_bed() {
    run_once();
    assert_eq!(stopped_early(), None, "the activation stopped early");
    assert!(
        RAN_MUTE_CLICK.load(std::sync::atomic::Ordering::SeqCst),
        "the clicked silent-speed check never finished"
    );
}

/// One application, one activation, one run — the shape every wired file here uses, because `g_application_run`
/// refuses a second claimant. Each entry test below asserts on what that single activation managed to do.
fn run_once() {
    let root = std::env::temp_dir().join(format!("np-sec06-sound-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(move |app| {
            // A panic raised inside GTK's `connect_activate` is non-unwinding — an abort with nothing readable
            // printed — so it is caught here and named, and the entry test reports WHICH check died.
            if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                a_two_clock_speed_says_so_in_the_preview(app)
            })) {
                *STOP_REASON.lock().unwrap() = Some(format_check_panic(&e));
            }
            if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                the_scene_end_sound_wears_the_tail_and_its_neighbour_does_not(app)
            })) {
                let reason = format_check_panic(&e);
                STOP_REASON.lock().unwrap().get_or_insert(reason);
            }
            // Each clicked check gets its own catch: a panic inside GTK's activate is non-unwinding, so an
            // uncaught one would abort the process and leave every entry test reporting "never ran".
            for (check, done) in [
                (
                    &placed_scene_sound_speed_is_said_about_in_the_preview
                        as &dyn Fn(&adw::Application),
                    &RAN_SCENE_CLICK,
                ),
                (
                    &placed_silent_speed_silences_the_preview_bed as &dyn Fn(&adw::Application),
                    &RAN_MUTE_CLICK,
                ),
            ] {
                if let Err(e) =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| check(app)))
                {
                    let reason = format_check_panic(&e);
                    STOP_REASON.lock().unwrap().get_or_insert(reason);
                } else {
                    done.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            }
            app.quit();
        });
        app.run_with_args::<&str>(&[]);
    });
}

/// The payload `catch_unwind` handed back, as one line. A failed assert's own message is printed by the panic
/// hook and swallowed when libtest captures this binary's stdout, so it is written to a file here too: a check
/// that dies says WHICH assertion killed it instead of "a panic with no message".
fn format_check_panic(payload: &(dyn std::any::Any + Send)) -> String {
    let text = payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_else(|| "a panic with no message".to_string());
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(std::env::temp_dir().join("np-sec06-sound-panic.log"))
    {
        use std::io::Write;
        let _ = writeln!(f, "{text}");
    }
    format!("the wired check panicked: {text}")
}

#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s8_a_speed_that_needs_two_clocks_says_so_in_the_preview(
) {
    run_once();
    assert_eq!(stopped_early(), None, "the activation stopped early");
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the wired check never finished"
    );
}

#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s8_the_answers_the_preview_can_copy_say_nothing() {
    run_once();
    assert_eq!(stopped_early(), None, "the activation stopped early");
    assert!(
        RAN.load(std::sync::atomic::Ordering::SeqCst),
        "the wired check never finished"
    );
}

/// The reason the activation stopped, if it stopped early.
fn stopped_early() -> Option<String> {
    STOP_REASON.lock().unwrap().clone()
}

/// §06-effects#2: the PLAYING leg routes the same silence, because the tick hands its gain straight to the
/// player — a mute that only the paused door honoured would un-mute the moment ▶ was pressed.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s8_the_playing_leg_routes_the_same_silence() {
    let cut = naivepost::cut::Cut {
        segs: footage(),
        lanes: lanes(),
        fx: vec![speed(10.0, 4.0, 1.5, "mute")],
        ..Default::default()
    };
    assert_eq!(
        cut_play_leg::route(&cut, 11.0, 1.0).gain,
        0.0,
        "routing a silent stretch asks for nothing"
    );
    assert!(
        cut_play_leg::route(&cut, 9.0, 1.0).gain > 0.0,
        "and everything else runs at the slider"
    );
    // The rate still travels: silence is a gain question, never a rate one. What is routed is the clip's own
    // played rate — `cut_play::played_rate`, whose answer for a clip with no rate written is [`cut_play::FLAT_RATE`].
    assert_eq!(
        cut_play_leg::route(&cut, 11.0, 1.0).rate,
        cut_play::FLAT_RATE,
        "the picture keeps its own clock under the silence"
    );
}
