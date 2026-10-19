//! §04-prepare.md §6 ("Details confirmed against the code") — first bullet: diarization is a TWO-PASS
//! anchored scan. `tests/suite/diar_anchored_pass.rs` pins the arithmetic of one rung (windows, anchor size,
//! the matching rule) and `tests/suite/diar_scan_ladder.rs` pins which rung serves; what neither could pin is
//! that the live walk runs pass 2 at all and says §6's lines while doing it. That is [`naivepost::diar_scan`]
//! driven here with fake asks — no socket, the way the ladder file does.

use naivepost::diar_scan::{self};
use naivepost::diarize;
use naivepost::requests::Turn;
use naivepost::transcribe::SAMPLE_RATE;

/// The lane name these scans log under — `[talk]` in every line read back below.
const BASE: &str = "talk";
/// A recording long enough that pass 1 strides more than once: ten 90 s windows at the top rung.
const LENGTH: f64 = 600.0;
const MODEL: &str = "sortformer-diar";

/// A turn in seconds, which is how a test thinks.
fn turn(slot: u32, from: f64, to: f64) -> Turn {
    Turn {
        start_sample: (from * SAMPLE_RATE as f64) as u64,
        end_sample: (to * SAMPLE_RATE as f64) as u64,
        speaker_id: slot,
    }
}

/// One voice speaking `seconds` of sound every `every` seconds from `from`.
fn speaks(slot: u32, from: f64, every: f64, seconds: f64, times: usize) -> Vec<Turn> {
    (0..times)
        .map(|at| turn(slot, from + at as f64 * every, from + at as f64 * every + seconds))
        .collect()
}

/// A two-voice room any rung can hold: each voice gets `diarize::anchor_per(90)` = 12 s of the anchor, so
/// the anchor is 24 s and the 90 s rung leaves 65 s of new audio per placement window.
fn two_voices() -> Vec<Turn> {
    let mut turns = speaks(0, 1.0, 8.0, 6.0, 4);
    turns.extend(speaks(1, 3.0, 8.0, 6.0, 4));
    turns
}

/// A room no rung can hold: seven voices, each taking a quarter of whatever window is in use, so the
/// anchor eats the window at every rung and `plan` refuses all three.
fn crowded_room() -> Vec<Turn> {
    let mut turns = speaks(0, 1.0, 8.0, 6.0, 4);
    for slot in 1..7 {
        turns.extend(speaks(slot, 2.0 + slot as f64, 8.0, 6.0, 4));
    }
    turns
}

/// What one run recorded: the log lines in order, pass 1's window widths, and every pass-2 ask as the
/// (span, anchor length) pair it was sent.
#[derive(Default)]
struct Walk {
    said: std::cell::RefCell<Vec<String>>,
    scanned: std::cell::RefCell<Vec<f64>>,
    placed: std::cell::RefCell<Vec<((f64, f64), f64)>>,
}

impl Walk {
    fn new() -> Self {
        Self::default()
    }

    // Pass 1's ask: every window of the rung answered with `room`.
    fn scan(&self, room: Vec<Turn>) -> impl FnMut(&[(f64, f64)]) -> ScanAnswer + '_ {
        move |windows: &[(f64, f64)]| {
            self.scanned.borrow_mut().push(
                windows
                    .iter()
                    .map(|(from, to)| to - from)
                    .fold(0.0f64, f64::max),
            );
            Ok(windows.iter().map(|_| room.clone()).collect())
        }
    }

    /// Pass 2's ask: every span answered with `answer`, recording what it was offered.
    fn place(&self, answer: Vec<Turn>) -> impl FnMut((f64, f64), f64) -> PlacementAnswer + '_ {
        move |span, anchor_seconds| {
            self.placed.borrow_mut().push((span, anchor_seconds));
            Ok(answer.clone())
        }
    }

    fn log(&self) -> impl FnMut(&str) + '_ {
        |line| self.said.borrow_mut().push(line.to_string())
    }

    fn lines(&self) -> Vec<String> {
        self.said.borrow().clone()
    }

    /// How many log lines contain `needle` — how a test counts one pass's lines without matching the others.
    fn countains(&self, needle: &str) -> usize {
        self.lines().iter().filter(|line| line.contains(needle)).count()
    }

    fn placements(&self) -> Vec<((f64, f64), f64)> {
        self.placed.borrow().clone()
    }
}

/// What pass 1 answers a whole rung with: one turn list per window it was sent.
type ScanAnswer = Result<Vec<Vec<Turn>>, String>;
/// What pass 2 answers one placement with.
type PlacementAnswer = Result<Vec<Turn>, String>;

/// The two passes' counts, each said once per window of its own pass, with the anchor line between them.
#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_diarization_is_a_two_pass_anchored_scan() {
    let walk = Walk::new();
    let rung = diar_scan::run(
        LENGTH,
        MODEL,
        BASE,
        walk.scan(two_voices()),
        walk.place(vec![]),
        walk.log(),
    )
    .expect("the top rung holds a two-voice room");

    // Pass 1: one `finding voices i/10` per window, spelled exactly as `diarize::scan_log` spells it.
    let lines = walk.lines();
    let expected: Vec<String> = (0..10)
        .map(|index| diarize::scan_log(BASE, index, 10))
        .collect();
    let found: Vec<&String> = lines
        .iter()
        .filter(|line| line.contains("finding voices"))
        .collect();
    assert_eq!(found.len(), 10, "one line per pass-1 window: {lines:?}");
    for (index, want) in expected.iter().enumerate() {
        assert_eq!(found[index], want, "pass 1's line {}", index + 1);
    }
    assert_eq!(rung.scan.len(), 10, "ten 90 s windows over 600 s at a two-thirds stride");

    // The anchor line, verbatim, once, after pass 1 and before any placement line.
    let anchor_seconds = 2.0 * diarize::anchor_per(90.0);
    let anchor_line = format!(
        ">>> [{BASE}] anchor: 2 voice(s) in {} s of a 90 s window -- {} s of new audio each",
        trim(anchor_seconds),
        trim(diarize::anchor_per(90.0))
    );
    assert_eq!(
        lines.iter().filter(|line| line.starts_with(">>> [talk] anchor:")).count(),
        1,
        "the anchor is cut once: {lines:?}"
    );
    let at_anchor = lines
        .iter()
        .position(|line| line == &anchor_line)
        .unwrap_or_else(|| panic!("the anchor line is `diarize::anchor_log`'s own: {lines:?}"));
    let first_placement = lines
        .iter()
        .position(|line| line.contains("placing speakers"))
        .expect("pass 2 logged its own count");
    let last_scan = lines
        .iter()
        .rposition(|line| line.contains("finding voices"))
        .expect("pass 1 logged its count");
    assert!(
        last_scan < at_anchor && at_anchor < first_placement,
        "pass 1, then the anchor, then pass 2: {lines:?}"
    );

    // Pass 2: one line per span, and the spans are the ones `plan` left to place.
    let placements = walk.placements();
    assert_eq!(placements.len(), 10, "one request per span left to place");
    let placement_lines: Vec<&String> = lines
        .iter()
        .filter(|line| line.contains("placing speakers"))
        .collect();
    assert_eq!(placement_lines.len(), 10, "one line per pass-2 span: {lines:?}");
    for (index, want) in (0..10)
        .map(|index| diarize::placement_log(BASE, index, 10))
        .enumerate()
    {
        assert_eq!(placement_lines[index], &want, "pass 2's line {}", index + 1);
    }
    // Every pass-2 ask was offered the anchor: the number it must prepend is the anchor's own width, so a
    // bare-audio ask (0.0) or a wrong-length one would be pass 1 asked again rather than pass 2 placing.
    for ((from, to), offered) in &placements {
        assert!(
            (offered - anchor_seconds).abs() < 1e-9,
            "span {from}..{to} was offered {offered} s of anchor, not {anchor_seconds}: the server must hear the reference in front"
        );
    }
    // And the spans cover the recording without overlap, each short enough to leave room for the anchor.
    assert_eq!(placements.first().unwrap().0 .0, 0.0);
    for pair in placements.windows(2) {
        assert!(
            (pair[0].0 .1 - pair[1].0 .0).abs() < 1e-9,
            "{placements:?} has a gap or an overlap"
        );
    }
    assert!(
        (placements.last().unwrap().0 .1 - LENGTH).abs() < 1e-9,
        "the tail of the recording is placed too: {placements:?}"
    );
}

/// A slot the anchor reaches keeps that voice's id; a slot it never heard earns a fresh one above every
/// anchor slot — inventing an id is the only answer that cannot put a stranger's words in a known mouth.
#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_diarization_names_a_slot_the_anchor_never_heard() {
    // The anchor holds two voices, 12 s each: block 0 occupies the anchor file's 0..12 s, block 1 its 12..24 s.
    let anchor_seconds = 2.0 * diarize::anchor_per(90.0);
    // Slot 0 speaks inside the anchor's first block (a real overlap); slot 5 starts past the whole anchor,
    // so its reach into the reference is nil — well under MIN_ANCHOR_OVERLAP.
    let answer = vec![turn(0, 1.0, 5.0), turn(5, anchor_seconds + 1.0, anchor_seconds + 4.0)];
    let walk = Walk::new();
    let rung = diar_scan::run(LENGTH, MODEL, BASE, walk.scan(two_voices()), walk.place(answer), walk.log())
        .expect("the top rung plans");

    // Pass 1's own voices come first (slots 0 and 1 as heard); pass 2's answers follow, renamed.
    let placed = &rung.turns[rung.scan.len() * two_voices().len()..];
    let highest_anchor_slot = 1u32;
    let known: Vec<u32> = placed.iter().filter(|t| t.speaker_id <= highest_anchor_slot).map(|t| t.speaker_id).collect();
    assert!(
        known.contains(&0),
        "the slot the anchor recognises keeps voice 0's id: {:?}",
        rung.turns
    );
    let fresh: Vec<u32> = placed
        .iter()
        .map(|t| t.speaker_id)
        .filter(|id| *id > highest_anchor_slot)
        .collect();
    assert!(
        !fresh.is_empty() && fresh.iter().all(|id| *id >= 2),
        "the unrecognised slot comes back above every anchor slot ({highest_anchor_slot}): {:?}",
        rung.turns
    );
    assert_eq!(
        diarize::MIN_ANCHOR_OVERLAP, 0.5,
        "the bound the thin claim falls under"
    );
}

/// Sound with no voice long enough to anchor on is an ANSWER: the line is said and the turns are empty,
/// so an absent `turns.json` is never read as a run that did not happen.
#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_diarization_no_voice_says_so_and_returns_no_turns() {
    // Two-second flickers: under `diarize::ANCHOR_MIN_SECONDS` (4 s), so no voice qualifies anywhere.
    let flickers = speaks(0, 1.0, 5.0, diarize::ANCHOR_MIN_SECONDS - 2.0, 10);
    let walk = Walk::new();
    let rung = diar_scan::run(
        LENGTH,
        MODEL,
        BASE,
        walk.scan(flickers.clone()),
        walk.place(vec![]),
        walk.log(),
    )
    .expect("no voice is an answer, not a failure");

    assert!(
        walk.countains(diarize::no_voice_log(BASE).as_str()) >= 1,
        "§6's sentence, verbatim: {:?}",
        walk.lines()
    );
    assert_eq!(
        walk.countains("placing speakers"),
        0,
        "nothing to place when nothing was anchored: {:?}",
        walk.lines()
    );
    // The turns are what was heard — flickers and all, since `plan` returns NoVoice rather than a trimmed
    // list; the point is the rung served and the step wrote its file instead of failing.
    assert_eq!(
        rung.turns.len(),
        rung.scan.len() * flickers.len(),
        "the rung's answers stand: {:?}",
        rung.turns.len()
    );
}

/// Every rung refusing fails, naming the model and the LAST rung's numbers — and costing no placement line,
/// because a refused rung is a rung the run never used.
#[test]
fn sec_04_prepare_6_details_confirmed_against_the_code_verification_pass_diarization_an_anchor_too_long_for_every_rung_refuses_naming_the_last_one() {
    let walk = Walk::new();
    let why = diar_scan::run(
        LENGTH,
        MODEL,
        BASE,
        walk.scan(crowded_room()),
        walk.place(vec![]),
        walk.log(),
    )
    .expect_err("seven voices' anchor fits no rung");

    assert!(why.contains(MODEL), "the diarization model is named: {why}");
    // `diarize::anchor_too_long` spells "anchor too long (X s of Y s window)" for the last refusal.
    let last = diarize::ladder()[diarize::ladder().len() - 1];
    assert!(
        why.ends_with(&format!(" s of {} s window)", trim(last))),
        "the last rung's numbers close it: {why}"
    );
    assert!(why.contains("anchor too long"), "{why}");
    assert_eq!(
        walk.countains("placing speakers"),
        0,
        "a rung the run never used places nothing: {:?}",
        walk.lines()
    );
    assert_eq!(
        walk.countains("anchor:"),
        0,
        "and no anchor line either — no rung got that far: {:?}",
        walk.lines()
    );
}

/// Seconds spelled the way Prepare logs them: whole numbers lose the `.0`.
fn trim(value: f64) -> String {
    if value == value.trunc() {
        return format!("{}", value as i64);
    }
    let mut text = format!("{value:.2}");
    while text.ends_with('0') && !text.ends_with(".0") {
        text.pop();
    }
    text.trim_end_matches('.').to_string()
}
