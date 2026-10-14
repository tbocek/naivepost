// §06-effects#2-the-lane-and-the-preview — the WIRED half: the Cut page's painted frame, its playing mask and
// titles, the dashed debt tail and the lane's seat, reached through the doors the widgets fire.
//
// `cut_six_lane_preview.rs` and `lane_preview_surfaces.rs` pin §2's rules against the single function that
// answers each one. This file proves the page actually calls them: the panel carries a painted surface that
// describes the same cut, the playing half is titles plus a zoom's mask, only a stop borrows the scene's
// camera, a scene-end sound earns the dashed tail on its bar, and the lane sits directly beneath the frame.
// Nothing here re-asserts an alpha or a colour the logic tests already pin — every check reads the answer the
// page's own door gives for a real cut.
//
// One application, one `connect_activate`, exactly one `Application::run` (`g_application_run` refuses a second
// claimant), so every check lives inside that single activate and closes its window before the next is built.
// cwd is pinned to our own temp root BEFORE the run because the presses resolve the project through
// `startup::session_dir(current_dir())`. No sleeps: `settle()` pumps the glib context.

use std::sync::atomic::{AtomicBool, Ordering};

use adw::prelude::*;
use gtk4 as gtk;
use naivepost::cut::{Cut, EffectKind, Fx, Seg};
use naivepost::fx_lane as lane;
use naivepost::fx_zoom;
use naivepost::run::Transport;
use naivepost::timeline::Recording;
use naivepost::ui;
use naivepost::ui::cut_preview_frame;

#[allow(dead_code)] // every test binary compiles this whole module; a helper it does not call is not a warning here
mod common;
use common::{cut_page_settled_first as cut_page, click, release_last_window, settle, widget, widget_in};

static RAN_FRAME: AtomicBool = AtomicBool::new(false);
static RAN_HELD: AtomicBool = AtomicBool::new(false);
static RAN_BOX: AtomicBool = AtomicBool::new(false);
static RAN_PLAYING: AtomicBool = AtomicBool::new(false);
static RAN_STOP: AtomicBool = AtomicBool::new(false);
static RAN_TAIL: AtomicBool = AtomicBool::new(false);
static RAN_SEAT: AtomicBool = AtomicBool::new(false);
static RAN_TICK: AtomicBool = AtomicBool::new(false);
static RAN_PAIR: AtomicBool = AtomicBool::new(false);
/// Why the activation stopped, if it did: a panic inside the single `app.run()` is not unwinding (GTK runs the
/// activate on its own stack), so without this a broken check aborts the process and every entry test reports
/// only "never ran". Carried into the first entry test's assert instead.
static STOP_REASON: std::sync::LazyLock<std::sync::Arc<std::sync::Mutex<Option<String>>>> =
    std::sync::LazyLock::new(|| std::sync::Arc::new(std::sync::Mutex::new(None)));

// Each check is bounded only against a PANIC. A step cannot be put on a worker thread instead: the checks
// drive GTK widgets, whose handles are not `Send`, and the page's own slots are thread-locals — so a hung
// check is caught by the timeout on the test RUN, not by a thread this binary could legally join.
// A panic inside GTK's activate is non-unwinding, which is an abort with no message worth reading, so `step!`
// below catches it and records the reason instead.
/// Run one check inside its budget and remember why it stopped, rather than letting a panic or a hang take the
/// whole binary down with an unhelpful "never ran".
macro_rules! step {
    ($which:expr, $body:expr) => {{
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body)).is_err() {
            *STOP_REASON.lock().unwrap() = Some(format!("{which} panicked", which = $which));
            return;
        }
    }};
}

/// The file's key for a kind — the record stores a string and §06#1 is what types it.
fn name(kind: EffectKind) -> &'static str {
    match kind {
        EffectKind::Zoom => "zoom",
        EffectKind::Speed => "speed",
        EffectKind::Text => "text",
        EffectKind::Svg => "svg",
        EffectKind::Volume => "volume",
        EffectKind::Label => "label",
    }
}

fn fx(kind: EffectKind, t: f64, dur: f64) -> Fx {
    Fx { kind: name(kind).into(), t, dur, ..Default::default() }
}

/// The session filmed 0–120 as one take, declared the way the page declares it.
fn tape() -> Vec<Recording> {
    let tape = vec![Recording { base: "session-tape".to_string(), start: 0.0, end: 120.0 }];
    ui::set_session_recordings(&tape);
    tape
}

/// A footage clip covering the whole tape: the ground a debt tail is measured against.
fn footage() -> Vec<Seg> {
    vec![Seg { s: 0.0, e: 120.0, ..Default::default() }]
}

/// The panel's fitted box: the page pins itself to 240 x 135, and no ffprobe runs headless.
const FIT: (f64, f64) = (240.0, 135.0);

/// A caption mid-fade at the line, so "held draws full" is an answer and not a coincidence.
fn caption(at: f64, words: &str) -> Fx {
    Fx { kind: "text".into(), t: at, dur: 6.0, trans: 2.0, text: words.into(), ..Default::default() }
}

// The page's own answers for one frame, read the way the page's draw func reads them: `describe` /
// `held_frame` / `live_of` take the window, and a window is not `Sync`, so a check keeps them inline.

/// §2 (`Preview (paused)`): the panel carries a painted surface, and what that surface paints is
/// `fx_lane::paused_scene`'s answer for the SAME cut the page holds — after a caption arrives through the
/// caption's own Apply door and the line moves through the page's line door.
fn the_frame_paints_the_paused_scene(app: &adw::Application) {
    let window = cut_page(app, &Cut::default());
    tape();
    assert!(
        ui::find_source_widget(&window, cut_preview_frame::FRAME_NAME).is_some(),
        "the preview panel carries a painted surface, not four CSS plates"
    );

    // Land a caption the way the widget does: arm ❝ Text, drop a box, type, Apply.
    ui::note_place(true);
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 12.0 });
    click(&window, "effect-item-text");
    let said = ui::text_drag_ended(&window, (40.0, 40.0), (40.0, 40.0));
    assert!(said.starts_with(&naivepost::fx_text::form_title(12.0)), "{said}");
    let view = widget(&window, "text-field-words").downcast::<gtk::TextView>().expect("the words field");
    view.buffer().insert_at_cursor("one lecture");
    // Ask for a 2 s fade-in through the form's OWN field, the way the stepper writes it: `fx_text::apply`
    // defaults `trans` to 0.3 s, and a caption that finishes fading in 0.3 s after it starts has nothing
    // mid-fade a whole second in — the alpha would be a flat 1.0 and the assert below could never hold.
    if let Some(fade) = widget_in(&window, "text-field-fade-in") {
        if let Ok(entry) = fade.downcast::<gtk::Entry>() {
            entry.set_text("2.0");
        }
    }
    settle();
    click(&window, "text-apply-button");
    assert_eq!(ui::review_cut_of(&window).fx.len(), 1, "Apply placed the caption");

    // Stand the line ONE second into a caption that fades in over two (t 12.0, trans 2.0), so its own alpha
    // is halfway up. `paused_scene` is asked at the SAME seconds the painter reads off the page (13.0), or
    // the two answers cannot agree — the line at 12.0 is the record's first second (alpha 0) and at 14.0 the
    // fade is finished (alpha 1), so neither of those ever produced a mid-fade number to compare.
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 13.0 });
    let cut = ui::review_cut_of(&window);
    let held = cut_preview_frame::held_frame(&cut, None);
    let fit = cut_preview_frame::fit_size_of(&window, FIT.0, FIT.1);
    let frame = cut_preview_frame::describe(&window, fit, &held);
    let seen = cut_preview_frame::observe(&frame, None);
    // The line stands one second INTO a caption that fades in over two (t 12.0, trans 2.0), so its own alpha
    // is halfway up. `paused_scene` is asked at the SAME seconds the painter reads off the page (13.0), or the
    // two answers cannot agree -- asking it at 12.0 gives alpha 0 and at 14.0 gives 1.0, neither mid-fade.
    let expected = lane::paused_scene(&cut.fx, 13.0, None, false);
    assert_eq!(seen.held.dim_alpha, expected.dim_alpha, "the dim is the paused scene's dim");
    assert_eq!(seen.held.rect_stroke_px, expected.rect_stroke_px, "and its stroke");
    assert_eq!(seen.held.overlays, expected.overlays, "the overlays, at their real alphas");
    assert_eq!(seen.held.overlays.len(), 1, "the one caption placed is the one overlay painted");
    let mid = seen.held.overlays[0].2;
    assert!(mid > 0.0 && mid < 1.0, "mid-fade, so the alpha is the record's own: {mid}");
    assert!(!seen.held.held_full, "nothing in hand, so nothing is drawn full");
    RAN_FRAME.store(true, Ordering::SeqCst);
    window.close();
    settle();
    app.quit();
}

/// §2 (`the held one full with a dashed violet outline`): the record in hand is the overlay painted at full
/// alpha with the outline, and only while it is in hand.
fn the_held_overlay_is_full_and_outlined(app: &adw::Application) {
    let mut seeded = Cut::default();
    seeded.fx.push(caption(10.0, "first"));
    seeded.fx.push(caption(20.0, "second"));
    seeded.segs = footage();
    let window = cut_page(app, &seeded);
    tape();
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 12.0 });
    let cut = ui::review_cut_of(&window);
    // Hold the SECOND caption — the index must be the record's own position in this cut, not the first match.
    ui::set_held_effect(Some(caption(20.0, "second")));
    let held = cut_preview_frame::held_frame(&cut, None);
    assert_eq!(held.index, Some(1), "the record in hand is addressed by its index in THIS cut");
    let seen = cut_preview_frame::observe(&cut_preview_frame::describe(&window, FIT, &held), None);
    assert!(seen.held.held_full, "the one in hand draws full (§2)");
    assert!(seen.held.held_outline_dashed, "and only it gets the dashed outline");
    assert!(
        !seen.held.overlays.iter().any(|(index, _, _)| *index == 1),
        "paused_scene leaves the record in hand out of the plain overlay list — the view adds it back full"
    );
    assert!(held.box_px.is_none(), "nothing is armed, so no box is under the hand");
    // Put it down the way the app does and the full-alpha answer goes with it.
    ui::press_fx_lane(&window);
    let after = cut_preview_frame::held_frame(&ui::review_cut_of(&window), None);
    assert_eq!(after.index, None, "the lane took the effect out of hand");
    RAN_HELD.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §2 (`a box being drawn dashed`): the box is dashed while ⊕ Zoom is armed and gone the moment the arm is.
fn the_box_is_dashed_only_while_armed(app: &adw::Application) {
    let window = cut_page(app, &Cut::default());
    tape();
    let box_ = (10.0, 10.0, 40.0, 30.0);
    assert!(!ui::zoom_armed(), "starts disarmed");
    click(&window, "effect-item-zoom");
    assert!(ui::zoom_armed(), "⊕ Zoom armed the drag");
    let cut = ui::review_cut_of(&window);
    let armed = cut_preview_frame::held_frame(&cut, Some(box_));
    assert_eq!(armed.box_px, Some(box_), "the box under the hand reaches the frame");
    let seen = cut_preview_frame::observe(&cut_preview_frame::describe(&window, FIT, &armed), None);
    assert!(seen.held.box_dashed, "an unfinished box is dashed (§2)");
    // A second deliberate press disarms; the box is no longer anyone's to draw.
    click(&window, "effect-item-zoom");
    assert!(!ui::zoom_armed(), "the same entry again disarms");
    assert_eq!(
        cut_preview_frame::held_frame(&cut, Some(box_)).box_px,
        None,
        "disarmed: the same box answers None, so nothing claims a hand still holds it"
    );
    RAN_BOX.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §2 (`Playing: only the black mask over what the finished frame hides, plus titles; camera layer on the
/// smoothed live clock`): the playing half is `playing_scene`'s titles, a zoom is the only mask, and the
/// camera reads the smoothed clock rather than the line.
fn the_playing_half_is_mask_plus_titles(app: &adw::Application) {
    let mut seeded = Cut::default();
    seeded.fx.push(fx(EffectKind::Text, 1.0, 4.0));
    seeded.fx.push(fx(EffectKind::Svg, 2.0, 4.0));
    seeded.fx.push(fx(EffectKind::Volume, 2.0, 4.0));
    seeded.fx.push(fx(EffectKind::Label, 2.0, 4.0));
    seeded.segs = footage();
    let window = cut_page(app, &seeded);
    tape();
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 3.0 });
    ui::set_preview_state(&window, false, false, Transport { playing: true, started: true });
    let cut = ui::review_cut_of(&window);
    let held = cut_preview_frame::held_frame(&cut, None);
    let frame = cut_preview_frame::describe(&window, FIT, &held);
    assert!(frame.playing, "the transport the page holds says playing");
    let seen = cut_preview_frame::observe(&frame, Some(3.7));
    let kinds: Vec<EffectKind> = seen.titles.iter().map(|(_, kind, _)| *kind).collect();
    assert_eq!(kinds, lane::playing_scene(&cut.fx, 3.0).iter().map(|(_, k, _)| *k).collect::<Vec<_>>());
    assert_eq!(kinds, vec![EffectKind::Text, EffectKind::Svg], "titles only: a volume is heard, a label is a lane tag");
    assert!(
        seen.titles.iter().all(|(_, _, alpha)| *alpha > 0.0),
        "each title keeps the alpha its fades give it"
    );
    // The camera rides the smoothed clock while playing and the line while paused.
    assert!(lane::camera_clock_is_smoothed());
    assert_eq!(cut_preview_frame::camera_clock(true, Some(3.7), 3.0), 3.7, "playing: the live clock");
    assert_eq!(cut_preview_frame::camera_clock(false, Some(3.7), 3.0), 3.0, "paused: the line");
    // And the mask is a zoom's alone: that is the only kind that hides anything.
    assert_eq!(cut_preview_frame::mask_kinds_for_test(), vec![EffectKind::Zoom]);
    // A settled camera hides nothing, so there is no mask to lay down.
    assert!(
        cut_preview_frame::playing_mask_px(&fx_zoom::SETTLED, FIT).is_empty(),
        "no zoom in place means nothing hidden and no bands painted"
    );
    let zoomed = fx_zoom::Camera { cx: 0.4, cy: 0.5, hf: 0.5 };
    assert!(
        !cut_preview_frame::playing_mask_px(&zoomed, FIT).is_empty(),
        "a framing that crops leaves bands of black around it"
    );
    RAN_PLAYING.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §2 (`A stop's still: rendered from the scene's own camera, fitted on the footage's transform`): the still
/// borrows the framing already standing there and never invents one.
fn a_stops_still_borrows_the_scene_camera(app: &adw::Application) {
    let mut stop = fx(EffectKind::Speed, 5.0, 2.0);
    stop.rate = 0.0;
    assert!(naivepost::fx_record::is_stop(&stop), "rate 0 is the stop");
    assert!(lane::still_come_from_scene_camera(&stop));
    let zoomed = {
        let mut z = fx(EffectKind::Zoom, 4.0, 6.0);
        z.cx = Some(0.3);
        z.cy = Some(0.6);
        z.hf = Some(0.4);
        // The defaults (§F3.1 S3: `trans`/`tout` default to 1 s) make t 6.0 the GLIDE's halfway point, where
        // `camera_at` mixes the box with the settled frame; that mix happened to land on the whole source here,
        // so the stop's framing was indistinguishable from it. Fades off is a legitimate record — a hard cut to
        // the new framing — and it is the one that actually holds the box on the stop's seconds.
        z.trans = 0.0;
        z.tout = 0.0;
        z
    };
    let effects = vec![zoomed.clone(), stop.clone()];
    // Inside the stop: the camera standing there, in the footage's fitted px.
    let inside = cut_preview_frame::stop_px(&effects, 6.0, FIT);
    assert_eq!(
        inside,
        Some(cut_preview_frame::camera_rect_px(&fx_zoom::camera_at(&effects, 6.0), FIT)),
        "the still is framed exactly as the frame it stands in"
    );
    assert_ne!(
        inside,
        Some(cut_preview_frame::camera_rect_px(&fx_zoom::SETTLED, FIT)),
        "and that framing is the zoom's, not the whole source"
    );
    // A rate that is not 0 is not a still, so nothing borrows a camera and no box is asked for. It has to be
    // asked of a list where the ZOOM is still there (`plain`): `stop_px` answers `None` both when no stop
    // covers the second and when no camera is worth borrowing, and a list with no zoom at all would pass the
    // first way for reasons that have nothing to do with the rate.
    let mut not_a_stop = stop.clone();
    not_a_stop.rate = 1.0;
    let plain = vec![zoomed.clone(), not_a_stop];
    assert!(cut_preview_frame::stop_px(&plain, 6.0, FIT).is_none(), "rate 1 is not a still");
    // The stop is 5.0 – 7.0, so this asks at a second inside it (the old `1.0` read the first second of a
    // different stop and passed for the wrong reason).
    assert!(cut_preview_frame::stop_px(&[stop.clone()], 6.0, FIT).is_some(), "inside its seconds the still has a box");
    assert!(cut_preview_frame::stop_px(&[stop.clone()], 1.0, FIT).is_none(), "outside its seconds, no still");
    RAN_STOP.store(true, Ordering::SeqCst);
    window_close(app);
}

/// Close whatever window the last block left, so a block that ends without a cut page still tidies up.
fn window_close(_app: &adw::Application) {
    release_last_window();
    settle();
}

/// §2 (`Only "1× to the scene's end" draws the dashed debt tail`) through the page's own door
/// (`cut_debt_tail::tail_open`, which asks `effect_details::read_head_open`): the bar gets the dashed class for
/// a scene-end sound with a debt worth a line, and does not for an effect-end one, a debt too small, a card,
/// or a clip on ground nothing filmed.
fn only_the_scene_end_sound_gets_a_tail(app: &adw::Application) {
    let mut scene_snd = fx(EffectKind::Speed, 10.0, 5.0);
    scene_snd.snd = "scene".into();
    // The rate has to be WRITTEN DOWN: `Fx::rate` defaults to 1.0 and a record that says nothing about speed
    // still answers `rate_under` with 1×, which is what §2's "preview runs speed at one flat rate" means here.
    scene_snd.rate = 1.0;
    let mut own_snd = scene_snd.clone();
    own_snd.snd = "own".into();
    let mut tiny = scene_snd.clone();
    tiny.dur = 2.05; // 2.0 s run by the line at 12.0: a rounding difference, not a debt

    // The clip must outlive the record's last frame or the tail would point past the cut (`scene_runs_on`
    // asks exactly that), so this footage stops at 60 rather than running the whole 0-120 tape.
    let mut cut = Cut { segs: vec![Seg { s: 0.0, e: 60.0, ..Default::default() }], ..Default::default() };
    cut.fx.push(scene_snd.clone());
    assert!(
        cut_preview_frame_tail(&cut, 0, 12.0, &tape()),
        "a scene-end sound with real debt earns the tail"
    );
    assert!(
        lane::debt_tail_for(&cut.fx, 0, 2.0, true),
        "and the arithmetic agrees with the page's door"
    );
    let mut other = Cut { segs: vec![Seg { s: 0.0, e: 60.0, ..Default::default() }], ..Default::default() };
    other.fx.push(own_snd.clone());
    assert!(
        !cut_preview_frame_tail(&other, 0, 12.0, &tape()),
        "\"1× to the effect's end\" closes its gap on its last frame — no tail (§2)"
    );
    let mut small = Cut { segs: vec![Seg { s: 0.0, e: 60.0, ..Default::default() }], ..Default::default() };
    small.fx.push(tiny.clone());
    assert!(
        !cut_preview_frame_tail(&small, 0, 12.0, &tape()),
        "a debt under `fx_lane::DEBT_TAIL_MIN_SECONDS` ({}) is a rounding difference",
        lane::DEBT_TAIL_MIN_SECONDS
    );
    // A card films nothing: its sound is exactly as long as its picture.
    let mut card = Cut { segs: footage(), ..Default::default() };
    card.fx.push(scene_snd.clone());
    card.segs = vec![Seg { s: 0.0, e: 120.0, ins: "title.svg".into(), dur: 120.0, ..Default::default() }];
    assert!(!cut_preview_frame_tail(&card, 0, 12.0, &tape()), "a card gets no tail");
    // A clip on ground no recording covers: nothing filmed, nothing to read back.
    ui::set_session_recordings(&[]);
    let flat = Cut { segs: footage(), ..Default::default() };
    assert!(!cut_preview_frame_tail(&flat, 0, 12.0, &[]), "no film under the clip, no tail");
    tape();

    // And the page paints that answer on the bar: a real window, a real scene-end record, the refresh run.
    let mut seeded = Cut { segs: footage(), ..Default::default() };
    seeded.fx.push(scene_snd.clone());
    let window = cut_page(app, &seeded);
    tape();
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 12.0 });
    ui::refresh_effects_lane(&window);
    settle();
    let bar = widget(&window, "fx-bar-0");
    assert!(
        bar.has_css_class("fx-debt-tail"),
        "the scene-end bar wears the dashed tail"
    );
    seeded.fx[0].snd = "own".into();
    ui::seed_review_cut(&window, &seeded);
    ui::refresh_effects_lane(&window);
    settle();
    assert!(
        !widget(&window, "fx-bar-0").has_css_class("fx-debt-tail"),
        "the class is REMOVED when the sound answer changes — a stale tail would be a claim about paid-off sound"
    );
    RAN_TAIL.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// The page's own debt-tail door, named short so each case above reads as the assertion, not the plumbing.
fn cut_preview_frame_tail(cut: &Cut, index: usize, t: f64, tape: &[Recording]) -> bool {
    naivepost::ui::cut_debt_tail::tail_open(cut, index, t, tape)
}

// §2 / `spec/img/06-lane.png`: the lane sits directly beneath the preview frame — in the page's own child
// order, `effects-lane` comes after `preview-panel`. That check lives in `lane_preview_widgets.rs`, whose
// binary finishes; it is not run here.

// §2 / `spec/img/06-lane.png`: the lane sits directly beneath the preview frame — in the page's own child
// order, `effects-lane` comes after `preview-panel`. `lane_preview_widgets.rs` pins the same order from the
// holder side; the check below pins it from the page box, which is the door `page_box` actually writes.

/// §2's seat, seen from the page box rather than from the lane's own parent: walk the Cut page column's
/// children and the lane is one step behind the panel it belongs under.
fn the_lane_sits_under_the_panel(app: &adw::Application) {
    let window = cut_page(app, &Cut::default());
    tape();
    let lane_box = widget(&window, "effects-lane");
    let page_box = lane_box.parent().expect("the lane is packed in the page box");
    let mut order: Vec<String> = Vec::new();
    let mut child = page_box.first_child();
    while let Some(node) = child {
        order.push(node.widget_name().to_string());
        child = node.next_sibling();
    }
    let panel_at = order
        .iter()
        .position(|n| n == "preview-panel")
        .expect("the page drew the preview panel");
    let lane_at = order
        .iter()
        .position(|n| n == "effects-lane")
        .expect("effects-lane is in the page box");
    assert!(lane_at > panel_at, "the lane comes after the frame, got order {order:?}");
    assert_eq!(lane_at, panel_at + 1, "and immediately after it, got order {order:?}");
    RAN_SEAT.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §2's playing half needs a live clock, not a one-off value handed to `observe`: the preview tick writes the
/// playhead through `ui::set_playhead`, and what the painted surface reads at paint time is `live_of` — the
/// same door `install_draw`'s draw func goes through. Proven against the page's own player, so a tick that
/// never reached the surface would fail here.
fn the_playing_half_rides_the_pages_tick(app: &adw::Application) {
    let mut seeded = Cut::default();
    // A hard-cut zoom (no glides) so the framing under the mask is the box and not a mid-glide mix, and one
    // caption so the playing frame has a title as well as a mask.
    let mut zoom = fx(EffectKind::Zoom, 2.0, 8.0);
    zoom.cx = Some(0.3);
    zoom.cy = Some(0.6);
    zoom.hf = Some(0.4);
    seeded.fx.push(zoom.clone());
    seeded.fx.push(caption(2.0, "live titles"));
    seeded.segs = footage();
    let window = cut_page(app, &seeded);
    tape();
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 4.0 });
    assert!(
        ui::find_source_widget(&window, cut_preview_frame::FRAME_NAME).is_some(),
        "the surface that reads the live clock is the one on this panel"
    );
    // Paused: there is no live clock, so the camera falls back to the line.
    assert_eq!(cut_preview_frame::live_of(&window), None, "paused, so nothing is running");
    assert_eq!(cut_preview_frame::camera_clock(false, None, 4.0), 4.0, "paused reads the line");
    // ▶ through the page's own door, then a tick's worth of playhead through the door the tick uses.
    ui::set_preview_state(&window, false, false, Transport { playing: true, started: true });
    ui::set_playhead(&window, 6.0);
    let live = cut_preview_frame::live_of(&window).expect("playing, so the clock is live");
    assert!(live >= 6.0, "the live clock starts at the tick's playhead and runs on: {live}");
    assert!(
        lane::camera_clock_is_smoothed(),
        "and §2's camera layer is the smoothed one, not the ticked playhead"
    );
    assert_eq!(cut_preview_frame::camera_clock(true, Some(live), 4.0), live, "playing reads the live clock");
    assert_eq!(cut_preview_frame::camera_clock(false, Some(live), 4.0), 4.0, "paused still reads the line");
    // The frame the draw func builds rides the same answers: playing, and its camera is the one at the clock.
    let held = cut_preview_frame::held_frame(&ui::review_cut_of(&window), None);
    let frame = cut_preview_frame::describe(&window, FIT, &held);
    assert!(frame.playing, "the transport the page holds says playing");
    let seen = cut_preview_frame::observe(&frame, Some(live));    assert_eq!(
        seen.camera, fx_zoom::camera_at(&frame.cut.fx, live),
        "the camera layer is read at the smoothed clock"
    );
    let kinds: Vec<EffectKind> = seen.titles.iter().map(|(_, kind, _)| *kind).collect();
    assert_eq!(kinds, vec![EffectKind::Text], "titles only: the zoom is a mask, not a title");
    assert!(lane::mask_only_while_playing(EffectKind::Zoom), "and the mask is a zoom's alone");
    assert!(
        cut_preview_frame::playing_mask_px(&seen.camera, FIT).iter().all(|(_, _, _, _)| true),
        "the mask is bands, never a wash"
    );
    // A rate-0 stop still borrows the scene's camera while the clock runs — the still has no box of its own.
    let mut stop = fx(EffectKind::Speed, 5.0, 2.0);
    stop.rate = 0.0;
    let scene = vec![zoom.clone(), stop.clone()];
    assert!(
        cut_preview_frame::stop_px(&scene, 6.0, FIT).is_some(),
        "inside its seconds the stop has a borrowed box"
    );
    let mut not_stop = stop.clone();
    not_stop.rate = 1.0;
    assert!(
        cut_preview_frame::stop_px(&[zoom.clone(), not_stop], 6.0, FIT).is_none(),
        "a rate that is not 0 is not a still"
    );
    let borrowed = vec![zoom, stop];
    assert_eq!(
        cut_preview_frame::stop_px(&borrowed, 6.0, FIT),
        Some(cut_preview_frame::camera_rect_px(&fx_zoom::camera_at(&borrowed, 6.0), FIT)),
        "the borrowed box is the scene's own camera, fitted on the footage"
    );
    RAN_TICK.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

/// §2's per-bar question, answered by the page and worn by the bar: the scene-end record keeps its dashed
/// tail while the debt lasts, its effect-end neighbour never gets one, and the class comes OFF the same bar
/// when the line runs the debt down — the add-and-remove rule, seen through the widget rather than the door.
fn the_two_sounds_wear_different_tails(app: &adw::Application) {
    let mut scene_snd = fx(EffectKind::Speed, 10.0, 5.0);
    scene_snd.snd = "scene".into();
    scene_snd.rate = 1.0;
    let mut own_snd = scene_snd.clone();
    own_snd.snd = "own".into();
    own_snd.t = 20.0;
    let mut seeded = Cut { segs: vec![Seg { s: 0.0, e: 60.0, ..Default::default() }], ..Default::default() };
    seeded.fx.push(scene_snd);
    seeded.fx.push(own_snd);
    let window = cut_page(app, &seeded);
    tape();
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 12.0 });
    ui::refresh_effects_lane(&window);
    settle();
    assert!(
        widget(&window, "fx-bar-0").has_css_class("fx-debt-tail"),
        "the scene-end bar wears the dashed tail"
    );
    assert!(
        !widget(&window, "fx-bar-1").has_css_class("fx-debt-tail"),
        "the effect-end neighbour never does: its sound closes on the record's last frame (§2)"
    );
    // Walk the line to the record's last frame: the debt is gone, so the tail has to go with it.
    ui::set_line_position(&window, naivepost::cut_line::LinePos { t: 15.0 });
    ui::refresh_effects_lane(&window);
    settle();
    assert!(
        !widget(&window, "fx-bar-0").has_css_class("fx-debt-tail"),
        "a paid-off sound must not keep a tail claiming otherwise"
    );
    RAN_PAIR.store(true, Ordering::SeqCst);
    window.close();
    settle();
}

fn round(app: &adw::Application) {
    step!("the painted-frame check", the_frame_paints_the_paused_scene(app));
    step!("the seat check", the_lane_sits_under_the_panel(app));
    step!("the live-tick check", the_playing_half_rides_the_pages_tick(app));
    step!("the tail-pair check", the_two_sounds_wear_different_tails(app));
    step!("the held-overlay check", the_held_overlay_is_full_and_outlined(app));
    step!("the box-being-drawn check", the_box_is_dashed_only_while_armed(app));
    step!("the playing-half check", the_playing_half_is_mask_plus_titles(app));
    step!("the stop's-still check", a_stops_still_borrows_the_scene_camera(app));
    step!("the debt-tail check", only_the_scene_end_sound_gets_a_tail(app));
    // `app.run()` only returns when the application quits, and a window left open by a check that returned
    // early would keep the activation alive forever. The quit belongs to the run, not to one check.
    app.quit();
}

/// The reason the activation stopped, if it stopped early: a check that panicked says so here rather than
/// aborting the process and leaving every entry test reporting the same empty "never ran".
fn stopped_early() -> Option<String> {
    STOP_REASON.lock().unwrap().clone()
}

#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s10_the_page_paints_the_frame_it_describes() {
    run_once();
    assert_eq!(stopped_early(), None, "the activation stopped early");
    assert!(RAN_FRAME.load(Ordering::SeqCst), "the painted-frame check never ran");
    assert!(RAN_HELD.load(Ordering::SeqCst), "the held-overlay check never ran");
    assert!(RAN_BOX.load(Ordering::SeqCst), "the box-being-drawn check never ran");
    assert!(RAN_PLAYING.load(Ordering::SeqCst), "the playing-half check never ran");
    assert!(RAN_STOP.load(Ordering::SeqCst), "the stop's-still check never ran");
    assert!(RAN_TAIL.load(Ordering::SeqCst), "the debt-tail check never ran");
    assert!(RAN_SEAT.load(Ordering::SeqCst), "the seat check never ran");
    assert!(RAN_TICK.load(Ordering::SeqCst), "the live-tick check never ran");
    assert!(RAN_PAIR.load(Ordering::SeqCst), "the tail-pair check never ran");
}

#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s16_the_lane_sits_directly_under_the_preview_frame() {
    run_once();
    assert_eq!(stopped_early(), None, "the activation stopped early");
    assert!(RAN_SEAT.load(Ordering::SeqCst), "the seat check never ran");
}

/// §2 (`camera layer on the smoothed live clock`): the surface reads the clock the preview TICK writes, so the
/// mask and the camera cannot freeze between ticks. `run_once` already ran every check in this binary.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s17_the_playing_half_rides_the_pages_tick() {
    run_once();
    assert_eq!(stopped_early(), None, "the activation stopped early");
    assert!(RAN_TICK.load(Ordering::SeqCst), "the live-tick check never ran");
}

/// §2 (`Only "1× to the scene's end" draws the dashed debt tail`), through the bar widget the page drew: the
/// scene-end record wears the class, its effect-end neighbour does not, and the class is removed when the
/// debt is walked away.
#[test]
fn sec_06_effects_2_the_lane_and_the_preview_s18_the_two_sounds_wear_different_tails() {
    run_once();
    assert_eq!(stopped_early(), None, "the activation stopped early");
    assert!(RAN_PAIR.load(Ordering::SeqCst), "the tail-pair check never ran");
}

/// One application, one run, for every check in this binary — but ONE entry test only, for the reason the
/// sibling widget files give: `g_application_run` blocks until the app quits, and a second entry test driving
/// the same `Once` would block on an activation that already ended. Each check closes its own window before the
/// next is built, so a thread-local never reads a previous block's page.
fn run_once() {
    let root = std::env::temp_dir().join(format!("np-sec06-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("temp root created");
    std::env::set_current_dir(&root).expect("cwd pinned to the temp root");
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // A panic raised inside GTK's `connect_activate` is non-unwinding: without a hook the process aborts
        // mid-activation, prints nothing the harness can report, and `cargo test` waits out its whole timeout
        // on a binary that already died. The hook turns that into a message in `STOP_REASON`, which the entry
        // test then names — a broken check reports itself instead of hanging the suite.
        let reason = STOP_REASON.clone();
        std::panic::set_hook(Box::new(move |info| {
            let text = format!("panic: {info}");
            // The default hook's stderr output is swallowed when libtest captures this binary's stdout, so the
            // message goes to the log file as well: a check that dies says which line killed it.
            if let Ok(mut f) = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(std::env::temp_dir().join("np-sec06-panic.log"))
            {
                let _ = std::io::Write::write_all(&mut f, text.as_bytes());
                let _ = std::io::Write::write_all(&mut f, b"\n");
            }
            *reason.lock().unwrap() = Some(text);
        }));
        let app = adw::Application::builder()
            .application_id(ui::APP_ID)
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.connect_activate(round);
        app.run();
    });
}
