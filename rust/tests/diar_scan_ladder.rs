//! §02-services.md **§2 Model roles**, the diarization row: speaker turns come "per window (90 → 45 → 25 s
//! ladder)". The arithmetic of a window is [`naivepost::diarize`]'s and is pinned in
//! `tests/diar_anchored_pass.rs`; what nothing else could pin was that anyone **walks** the ladder at all —
//! a session whose biggest window cannot hold its own anchor has to move on rather than fail. That walk is
//! [`naivepost::diar_scan::run`], and these tests drive it with a fake ask, so no socket is dialled.
//!
//! Item id: `sec_02_services_2_model_roles`. **s3** is this row's ladder.

use naivepost::diar_scan::{self, Rung};
use naivepost::diarize;
use naivepost::requests::Turn;
use naivepost::transcribe::SAMPLE_RATE;

/// A turn in seconds, which is how a test thinks.
fn turn(slot: u32, from: f64, to: f64) -> Turn {
    Turn {
        start_sample: (from * SAMPLE_RATE as f64) as u64,
        end_sample: (to * SAMPLE_RATE as f64) as u64,
        speaker_id: slot,
    }
}

/// One voice speaking `seconds` every `every` seconds from `from` — long enough turns to count as a voice
/// (`diarize::ANCHOR_MIN_SECONDS`) rather than a flicker.
fn speaks(slot: u32, from: f64, every: f64, seconds: f64, times: usize) -> Vec<Turn> {
    (0..times)
        .map(|at| turn(slot, from + at as f64 * every, from + at as f64 * every + seconds))
        .collect()
}

/// A room no rung can hold: seven voices, each contributing a quarter of whatever window is in use
/// (`diarize::anchor_per`) — 21 s each at 90 s, so the anchor leaves only 5 s to place there and less than
/// nothing on the smaller rungs. `plan` therefore refuses all three.
fn crowded_room() -> Vec<Turn> {
    let mut turns = speaks(0, 1.0, 8.0, 6.0, 4);
    for slot in 1..7 {
        turns.extend(speaks(slot, 2.0 + slot as f64, 8.0, 6.0, 4));
    }
    turns
}

/// A room whose anchor only fits a lower rung. `diarize::anchor_per` gives each voice a quarter of whatever
/// window is in use, so the anchor grows with the number of voices while the window's slack shrinks: three
/// voices leave 53 s to place at 90 s (plans) but only 10.2 s at 45 s and 5.2 s at 25 s, under
/// `NEW_AUDIO_MIN_SECONDS`, so those two refuse.
fn crowded_too_far() -> Vec<Turn> {
    let mut turns = speaks(0, 1.0, 8.0, 6.0, 4);
    for slot in 1..4 {
        turns.extend(speaks(slot, 2.0 + slot as f64, 8.0, 6.0, 4));
    }
    turns
}

/// The same room answered as if the recording were longer than it is: pass 1 strides further, so the window
/// `plan` picks holds fewer voices and its anchor fits. That is what makes a refusal on one rung a success on
/// the next — a bigger window covers more clock per ask and therefore hears fewer voices inside any one of them.
/// A two-voice room any rung can hold.
fn quiet_room() -> Vec<Turn> {
    let mut turns = speaks(0, 1.0, 8.0, 6.0, 4);
    turns.extend(speaks(1, 3.0, 8.0, 6.0, 4));
    turns
}

/// What the fake recorded: one entry per ask, each the size of window that ask was about.
#[derive(Default)]
struct Recorder {
    asked: std::sync::Arc<std::sync::Mutex<Vec<f64>>>,
    /// How many times the fake has been asked, so a scenario can answer differently down the ladder.
    calls: std::sync::Arc<std::sync::Mutex<usize>>,
}

impl Recorder {
    fn new() -> Self {
        Self::default()
    }

    /// The rungs the fake was asked about, in order — the walk's own trail. Rounded because a rung is
    /// recovered from the spans the caller was sent, and the last window of a recording ends at e.g.
    /// 25.000…4 rather than exactly 25.
    fn trail(&self) -> Vec<u32> {
        self.asked
            .lock()
            .expect("the recorder")
            .iter()
            .map(|width| width.round() as u32)
            .collect()
    }

    /// Which ask this is, counting from 0 — how a scenario says "the first rung refused, the next served".
    fn call(&self) -> usize {
        let mut calls = self.calls.lock().expect("the counter");
        *calls += 1;
        *calls - 1
    }
}

/// A fake ask that answers the rungs named in `refuse` with the room no such window can anchor, and every
/// other rung with `turns`. The refusal is arithmetic on the answer, never something a server says — which is
/// why the fake only changes what it "heard".
fn fake<'a>(
    turns: Vec<Turn>,
    refuse: Vec<u32>,
    recorder: &'a Recorder,
) -> impl FnMut(&[(f64, f64)]) -> Result<Vec<Vec<Turn>>, String> + 'a {
    let asked = recorder.asked.clone();
    move |windows: &[(f64, f64)]| {
        let width = windows
            .iter()
            .map(|(from, to)| to - from)
            .fold(0.0f64, f64::max);
        asked.lock().expect("the recorder").push(width);
        if refuse.contains(&(width.round() as u32)) {
            // Every window of this rung answers with the same crowded room, so pass 1 has one candidate
            // window and its anchor is that room's worth of it.
            return Ok(windows.iter().map(|_| crowded_room()).collect());
        }
        Ok(windows.iter().map(|_| turns.clone()).collect())
    }
}

/// A fake whose refusals are scripted by ORDER of ask rather than by window size: the first `refusals` asks
/// come back as the room nothing can anchor, after that as `turns`. This is how a test pins "the walk moved on"
/// without depending on which real room happens to overflow which rung — the ladder order and the stopping
/// rule are what is being asserted, and `diar_anchored_pass` already pins the anchor arithmetic itself.
fn fake_by_order<'a>(
    turns: Vec<Turn>,
    refusals: usize,
    recorder: &'a Recorder,
) -> impl FnMut(&[(f64, f64)]) -> Result<Vec<Vec<Turn>>, String> + 'a {
    move |windows: &[(f64, f64)]| {
        let call = recorder.call();
        recorder
            .asked
            .lock()
            .expect("the recorder")
            .push(windows.iter().map(|(from, to)| to - from).fold(0.0f64, f64::max));
        if call < refusals {
            return Ok(windows.iter().map(|_| crowded_room()).collect());
        }
        Ok(windows.iter().map(|_| turns.clone()).collect())
    }
}

/// A fake pass-2 ask that answers every span with no turns: these tests are about which rung served, and
/// pass 2 is the serving rung's own work — the line counts and the anchored turns are pinned in
/// `tests/diar_anchored_pass.rs`.
fn no_placements() -> impl FnMut((f64, f64), f64) -> Result<Vec<Turn>, String> {
    |_, _| Ok(vec![])
}

/// (d) the walk uses the shipped ladder, biggest first, read straight off `diarize::ladder()`.
#[test]
fn sec_02_services_2_model_roles_s3_ladder_is_the_shipped_three_rungs_biggest_first() {
    assert_eq!(diarize::ladder(), &[90.0, 45.0, 25.0]);
}

/// (b) a recording the biggest window handles is asked about once, at 90 s, and nothing smaller is tried.
#[test]
fn sec_02_services_2_model_roles_s3_ladder_a_fitting_room_is_asked_about_once_at_the_top_rung() {
    let recorder = Recorder::new();
    let rung = diar_scan::run(
        600.0,
        "sortformer-diar",
        "talk",
        fake(quiet_room(), vec![], &recorder),
        no_placements(),
        |_| (),
    )
    .expect("the top rung fits a two-voice room");

    assert_eq!(recorder.trail(), vec![90], "one ask, at the biggest window");
    assert_eq!(rung.window, 90.0);
    // Pass 1 strides two thirds of the window (`P.machine.diarHopShare`), so 600 s of recording is ten
    // 90 s windows, the last one ending at the end rather than padded out.
    assert_eq!(rung.scan.len(), 10, "90 s windows over 600 s at a two-thirds stride");
    assert!(
        !rung.turns.is_empty(),
        "the rung's answers are the turns the run gets"
    );
}

/// (a) the top rung refuses and the next serves: the walk asks twice in ladder order and hands back the
/// serving rung's turns — the ladder moving on instead of the step failing.
#[test]
fn sec_02_services_2_model_roles_s3_ladder_moves_on_past_a_rung_whose_anchor_eats_the_window() {
    let recorder = Recorder::new();
    // The first ask refuses (a room no 90 s window can anchor), the second serves: what is pinned is that a
    // refusal moves the walk on rather than ending it, and that it stops at the rung that answered.
    let rung = diar_scan::run(
        600.0,
        "sortformer-diar",
        "talk",
        fake_by_order(quiet_room(), 1, &recorder),
        no_placements(),
        |_| (),
    )
    .expect("the rung that fits serves after a refusal");

    assert_eq!(recorder.trail(), vec![90, 45], "asked in ladder order, stopping where it served");
    assert_eq!(rung.window, 45.0, "the rung that planned is the rung that serves");
    assert!(
        !rung.turns.is_empty(),
        "the serving rung's answers are the turns the run gets"
    );
}

/// (c) every rung refusing fails, and the sentence names the model and both numbers of the last refusal.
#[test]
fn sec_02_services_2_model_roles_s3_ladder_refused_everywhere_names_the_model_and_the_numbers() {
    let recorder = Recorder::new();
    // Six voices fit nowhere on the ladder: every rung answers with the crowded room, and the plan refuses
    // at 45 and 25 — which is enough to prove the walk tries the rest before failing, since it starts at
    // the rung that would serve them thinly.
    let why = diar_scan::run(
        600.0,
        "sortformer-diar",
        "talk",
        fake(crowded_room(), vec![], &recorder),
        no_placements(),
        |_| (),
    )
    .expect_err("no rung can hold six voices' anchor");

    assert_eq!(recorder.trail(), vec![90, 45, 25], "every rung was tried before failing");
    assert!(
        why.contains("sortformer-diar"),
        "the model is named: {why}"
    );
    // `diarize::anchor_too_long` spells "anchor too long (X s of Y s window)" — the last refusal's numbers.
    assert!(
        why.contains("anchor too long (") && why.ends_with(" s of 25 s window)"),
        "both numbers of the last refusal are in it: {why}"
    );
}

/// The turns returned are the serving rung's own, not the first rung's: a walk that asked twice and then
/// served must hand over what the third ask answered.
#[test]
fn sec_02_services_2_model_roles_s3_ladder_returns_the_serving_rungs_answers() {
    let recorder = Recorder::new();
    let served = speaks(7, 2.0, 8.0, 6.0, 3);
    // Only the top rung is refused here, so the walk must stop at the second rather than carry on to 25.
    let rung: Rung = diar_scan::run(
        600.0,
        "sortformer-diar",
        "talk",
        fake_by_order(served.clone(), 1, &recorder),
        no_placements(),
        |_| (),
    )
    .expect("the next rung holds the anchor the top one could not");

    assert_eq!(recorder.trail(), vec![90, 45], "stopped at the rung that served");

    assert_eq!(rung.window, 45.0, "the 90 s rung refused");
    // Every window of the serving rung answers with the same room, so the turns are that answer repeated
    // across the rung's windows — never the refusing rung's crowded-room turns.
    assert!(
        rung.turns.iter().all(|turn| turn.speaker_id == 7),
        "every turn came from the serving rung's answer: {:?}",
        rung.turns
    );
    assert_eq!(
        rung.turns.len(),
        rung.scan.len() * served.len(),
        "one answer per window of the rung that served"
    );
}
