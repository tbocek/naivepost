// F4.5 Preview the cut with narration — spec/07-narrate.md F4.5, one test per step and per branch of its
// flowchart. ▶ runs the cut with the narration riding along: from the line if there is one, gaps skipped
// forward, a boundary held while a line still speaks, a missing take pausing the picture, a failed take sticky
// until its own ▶ retries it, sound as the cut says, and ⏹ stopping both players. Every rule is a plain
// function in naivepost::narrate_preview, so nothing here needs a display or a player.

use naivepost::cut::{Cut, Fx, Seg};
use naivepost::cut_hear;
use naivepost::narrate_preview as view;
use naivepost::narrate_preview::Take;
use naivepost::narrate_screen::AUDITION_LEAD_SECONDS;
use naivepost::narration::Entry;

#[allow(dead_code)] // every test binary compiles this whole module
mod common;
use common::{seg};

/// A line written against the clip `s..e`, starting `at` seconds into it.
fn clip(s: f64, e: f64, text: &str, at: f64) -> Entry {
    Entry { s, e, at, text: text.into(), ..Default::default() }
}

/// A line over the standing 10–20 clip.
fn line(at: f64, text: &str) -> Entry {
    clip(10.0, 20.0, text, at)
}

/// The session seconds a recording covers: everything both clips were shot from, before the cut removed the
/// stretch between them.
fn covered() -> Vec<(f64, f64)> {
    vec![(0.0, 45.0)]
}

/// S1: what ▶ and a click on the picture mean before anything is asked of a player.
#[test]
fn f4_5_s1_clicking_plays_from_the_line_else_the_cut_starts() {
    // No clips: the sentence is the whole answer, and it is that exact sentence.
    assert_eq!(view::start_point(&[], None, &covered()), view::Start::NoClips);
    assert_eq!(view::NOTHING_TO_PREVIEW, "nothing to preview yet \u{2014} cut some clips first");

    // Clips and no line chosen: the top of the cut.
    let clips = [seg(10.0, 20.0), seg(30.0, 40.0)];
    assert_eq!(view::start_point(&clips, None, &covered()), view::Start::Play(10.0));

    // A chosen line wins: "play from the line".
    assert_eq!(view::start_point(&clips, Some(35.0), &covered()), view::Start::Play(35.0));

    // A second no recording covers: nothing can be loaded, and toggling an empty pipeline would report
    // itself as playing while the frame stayed black.
    assert_eq!(view::start_point(&clips, None, &[(12.0, 30.0)]), view::Start::Uncovered { at: 10.0 });
    assert_eq!(view::NO_RECORDING_AT_START, "no recording covers the start of the cut");
}

/// S2: the tick follows the picture; gaps are skipped, a boundary is held, and a line's take decides what
/// happens when the picture reaches it.
#[test]
fn f4_5_s2_the_tick_follows_the_picture_and_the_narration_rides_along() {
    // P.eng.narrationMaxExtendSeconds
    let clips = [seg(10.0, 20.0), seg(30.0, 40.0)];

    // Inside a clip with a line whose wav exists: its audio starts at the offset the picture is at, so a
    // mid-line entry point does not restart the take from its first word.
    let entries = [line(2.0, "spoken")];
    let takes = [Take::Exists];
    assert_eq!(view::tick(14.5, &clips, &entries, &takes, None), view::Tick::Speak { line: 0, offset: 2.5 });

    // A line with no words is a deliberate silence: never offered to the tick, and so never spoken.
    let blank = [line(2.0, "   ")];
    assert_eq!(view::line_at(14.0, &blank), None);
    assert_eq!(view::tick(14.0, &clips, &blank, &[], None), view::Tick::Idle);

    // A line owns its seconds until the next spoken line on the same clip starts.
    let two = [line(2.0, "first"), line(6.0, "second")];
    assert_eq!(view::line_end(0, &two), 16.0);
    assert_eq!(view::line_end(1, &two), 20.0, "the last line runs to the clip's end");
    assert_eq!(view::line_at(17.0, &two), Some(1));

    // A gap between two clips is material the edit removed: skip forward rather than play it.
    assert_eq!(view::tick(25.0, &clips, &entries, &takes, None), view::Tick::SkipTo(30.0));
    // Past the last clip the cut is over, and both players stop.
    assert_eq!(view::tick(45.0, &clips, &entries, &takes, None), view::Tick::Pause);

    // A boundary is held while a line still speaks — up to the seconds the render may grow the clip by (the
    // preview never holds one the render would not).
    assert_eq!(view::MAX_EXTEND_SECONDS, 4.0);
    let speaking = Some(0usize);
    assert_eq!(view::tick(23.9, &clips, &entries, &takes, speaking), view::Tick::Hold);
    assert_eq!(view::tick(24.1, &clips, &entries, &takes, speaking), view::Tick::SkipTo(30.0));

    // A line with no take pauses the picture and is spoken first.
    assert_eq!(view::tick(14.0, &clips, &entries, &[Take::Missing], None), view::Tick::Synthesize { line: 0 });
    assert_eq!(view::synthesizing(2), "synthesizing line 3");
    // A take nobody has looked for at all counts as missing rather than existing.
    assert_eq!(view::tick(14.0, &clips, &entries, &[], None), view::Tick::Synthesize { line: 0 });

    // A take the server already refused runs the clip mute — and says so again every time it comes around.
    assert_eq!(view::tick(14.0, &clips, &entries, &[Take::Failed], None), view::Tick::Mute { line: 0 });
    assert_eq!(
        view::sticky_failed(2),
        "line 3 failed to synthesize \u{2014} see log; its \u{25b6} retries"
    );

    // The two failure sentences, and the dash each one keeps: they are different sentences in the same log,
    // and normalising one into the other would rewrite what a person reads.
    assert_eq!(view::failed_playing_on(2), "line 3 failed -- see log; playing on without it");
    assert_eq!(view::line_ready(2), "line 3 ready");
    assert!(view::sticky_failed(0).contains('\u{2014}'));
    assert!(!view::failed_playing_on(0).contains('\u{2014}'));
    assert!(view::failed_playing_on(0).contains("--"));

    // A hold can land mid-line, so a finished synthesis resumes at the line's own start — never where the
    // picture froze, which would start a shorter new wav past its end. A failure has nowhere new to land.
    assert_eq!(view::resume_after_synthesis(17.4, 12.0, true), 12.0);
    assert_eq!(view::resume_after_synthesis(17.4, 12.0, false), 17.4);
}

/// S3: what is heard is what will be rendered — the duck, the lanes as the cut has them, and the rate at the
/// second being sought.
#[test]
fn f4_5_s3_sound_equals_the_render_and_speed_applies_at_seeks() {
    // P.policy.gameVolume
    // The project's value is passed in; §10's 0.22 belongs to the project, not to this module.
    assert_eq!(view::duck(true, 0.22), 0.22);
    assert_eq!(view::duck(false, 0.22), 1.0, "nothing spoken, nothing ducked");

    // Lanes and hushes follow the cut: same scene, same answer as the owner gives it, so the preview cannot
    // disagree with the video it is supposed to be showing.
    let quiet = Seg { s: 10.0, e: 20.0, quiet: vec!["mic.mkv".into()], ..Default::default() };
    let cut = Cut { segs: vec![quiet.clone()], ..Default::default() };
    assert_eq!(view::lanes_heard(&cut, 15.0), cut_hear::hush(&quiet).to_vec());
    assert_eq!(view::lanes_heard(&cut, 25.0), Vec::<String>::new(), "a removed second hushes nothing");

    // Speed effects apply at seeks: the rate under the target, and the footage's own speed outside it.
    let fast = Fx { kind: "speed".into(), t: 12.0, dur: 4.0, rate: 2.0, ..Default::default() };
    assert_eq!(view::seek_rate(&[fast.clone()], 13.0), 2.0);
    assert_eq!(view::seek_rate(&[fast], 17.0), 1.0);
    assert_eq!(view::seek_rate(&[], 13.0), 1.0);
}

/// S4: a seek lands on the cut, because the seconds between two clips are not in the video at all. The
/// clips overlap here (two cameras over one session), which is what makes "the nearest boundary" the rule
/// rather than "the previous clip in the list".
#[test]
fn f4_5_s4_a_seek_in_a_gap_lands_on_the_cut() {
    let clips = [seg(10.0, 20.0), seg(30.0, 40.0)];

    assert_eq!(view::snap_seek(15.0, &clips, false), 15.0, "inside a clip: its own answer");
    assert_eq!(view::snap_seek(15.0, &clips, true), 15.0);

    // A gap target: back to the previous clip's end, forward to the next clip's start.
    assert_eq!(view::snap_seek(25.0, &clips, false), 20.0);
    assert_eq!(view::snap_seek(25.0, &clips, true), 30.0);

    // Outside the cut there is no boundary to snap to *in the direction asked*, so the target stands: nothing
    // starts before the first clip, and nothing lies ahead once the last one has ended.
    assert_eq!(view::snap_seek(5.0, &clips, false), 5.0);
    assert_eq!(view::snap_seek(5.0, &clips, true), 10.0, "the cut does start there");
    assert_eq!(view::snap_seek(45.0, &clips, false), 40.0, "and it did end there");
    assert_eq!(view::snap_seek(45.0, &clips, true), 45.0);

    // The nearest boundary wins in both directions, whatever order the segments arrived in.
    let shuffled = [seg(30.0, 40.0), seg(10.0, 20.0)];
    assert_eq!(view::snap_seek(25.0, &shuffled, false), 20.0);
    assert_eq!(view::snap_seek(25.0, &shuffled, true), 30.0);

    // A second inside a removed stretch that two clips bracket: going back finds the end behind it and going
    // forward the start ahead of it, not whichever clip happens to be listed first.
    let bracketed = [seg(0.0, 24.0), seg(10.0, 20.0), seg(26.0, 40.0)];
    assert_eq!(view::snap_seek(25.0, &bracketed, false), 24.0);
    assert_eq!(view::snap_seek(25.0, &bracketed, true), 26.0);
}

/// S5: the blue row follows the playhead, and picking a row drops the picture where the line can be seen to
/// land.
#[test]
fn f4_5_s5_the_row_follows_the_playhead_and_selecting_one_seeks_to_its_lead_in() {
    // P.eng.narrationRunInSeconds
    assert_eq!(AUDITION_LEAD_SECONDS, 3.0);

    let entries = [line(10.0, "spoken")];
    assert_eq!(view::selected_row(21.0, &entries), None);
    assert_eq!(view::selected_row(19.0, &entries), view::line_at(19.0, &entries));

    // Three seconds ahead of the line — and never back before the clip starts, where there is no video.
    assert_eq!(view::lead_in(0, &entries), 17.0);
    let early = [line(1.0, "spoken")];
    assert_eq!(view::lead_in(0, &early), 10.0, "the clip's start rather than a second before it");

    // Unless those seconds belong to the line before: then this one starts the preview instead of cutting
    // across another line that is still speaking.
    let two = [line(2.0, "first"), line(6.0, "second")];
    assert_eq!(view::lead_in(1, &two), 16.0);
    assert_eq!(view::selected_row(15.0, &two), Some(0), "and 15 s really is the first line's");
}

/// S6: ⏹ stops both players and hands ▶ back to the step.
#[test]
fn f4_5_s6_the_square_stops_both_players_and_hands_the_triangle_back() {
    let stopped = view::stop();
    assert!(stopped.picture_paused);
    assert!(stopped.voice_paused, "the narration riding along stops with the picture");
    assert!(!stopped.started);
    assert_eq!(stopped.play_button, view::PLAY_ICON);
    assert_eq!(view::PLAY_ICON, "\u{25b6}");
}

/// S2's last branch: a failed take stays mute until its own ▶ asks again.
#[test]
fn f4_5_s7_a_failed_line_is_sticky_per_wav_until_its_own_button_retries_it() {
    let mut failed = view::Failed::default();
    assert!(!failed.holds("a.wav"), "nothing has failed yet");

    failed.add("a.wav");
    assert!(failed.holds("a.wav"));
    failed.add("a.wav");
    assert!(failed.holds("a.wav"), "remembering it twice is still one failure");

    // The row's ▶ clears the failure first, so a second failure is noticed as a second failure.
    assert!(failed.retry("a.wav"));
    assert!(!failed.holds("a.wav"));
    assert!(!failed.retry("a.wav"), "nothing was left to clear");

    // Keyed on the take: re-editing a line gives it a new key, and that fresh take is not refused because an
    // older one failed.
    failed.add("a.wav");
    assert!(!failed.holds("b.wav"));
}

/// S1 again, one floor up: what the composed step answers, as opposed to `start_point`'s verdict about the
/// second. The page stores exactly this `playing()` value, so a wrong answer here is a preview that lies
/// about itself in the state bar.
#[test]
fn f4_5_s8_the_composed_press_answers_pause_refusal_or_a_start_second() {
    let clips = [seg(10.0, 20.0), seg(30.0, 40.0)];

    // Already playing: pause, and nothing else is asked of the cut (no start second, no refusal).
    let paused = view::press_picture(true, &clips, Some(35.0), &covered());
    assert_eq!(paused, view::Pressed::Paused);
    assert!(!paused.playing(), "a pause leaves the preview not running -- the state stored must go false");
    assert_eq!(paused.refused(), None, "pausing is not a refusal");

    // No clips at all: the whole answer is the sentence, verbatim.
    let empty = view::press_picture(false, &[], None, &covered());
    assert_eq!(
        empty,
        view::Pressed::Refused(view::NOTHING_TO_PREVIEW.to_string())
    );
    assert_eq!(
        empty.refused(),
        Some("nothing to preview yet \u{2014} cut some clips first"),
        "S1's sentence is the string the status line gets"
    );
    assert!(!empty.playing());

    // A line under the playhead wins: start from ITS second, not from the top of the cut.
    let from_line = view::press_picture(false, &clips, Some(35.0), &covered());
    assert_eq!(from_line, view::Pressed::Playing { from: 35.0 });
    assert!(from_line.playing(), "this press leaves the preview running");
    assert_eq!(from_line.refused(), None);

    // No line chosen: the cut's own start, which is the first kept clip's `s`.
    let from_top = view::press_picture(false, &clips, None, &covered());
    assert_eq!(from_top, view::Pressed::Playing { from: 10.0 });

    // The cue lands off every covered span: refused, and with the recording sentence rather than the
    // empty-cut one -- the clips exist, it is the footage that does not reach there.
    let off_recording = view::press_picture(false, &clips, Some(35.0), &[(0.0, 12.0)]);
    assert_eq!(
        off_recording,
        view::Pressed::Refused(view::NO_RECORDING_AT_START.to_string())
    );
    assert_ne!(
        off_recording.refused(),
        Some(view::NOTHING_TO_PREVIEW),
        "an uncovered cue is not the same complaint as no clips"
    );
    assert!(!off_recording.playing(), "a refused cue must never set playing");

    // P.eng.narrationMaxExtendSeconds and the four-second boundary hold are S2's rule and belong to the
    // tick test above (`f4_5_s2_the_tick_follows_the_picture_and_the_narration_rides_along`, which pins
    // MAX_EXTEND_SECONDS == 4.0 and the Hold branch): a press never consults them, since the picture has
    // not moved yet when this answer is given.
}

/// S2's sticky failure, through the door F4.4's row ▶ actually calls: the retry clears first, so the
/// second attempt is allowed, while a key nobody retried keeps answering mute.
#[test]
fn f4_5_s9_retrying_a_take_clears_its_key_so_the_next_attempt_is_allowed() {
    let mut failed = view::Failed::default();
    failed.add("take-7.wav");
    failed.add("take-9.wav");

    // Never-retried keys stay sticky: the run past them speaks nothing, however many times it comes round.
    assert!(failed.holds("take-9.wav"));
    assert!(failed.holds("take-9.wav"), "sticky means sticky across repeat asks");

    // The retried key is cleared by its own press, so this attempt is allowed through.
    assert!(view::retry_line(&mut failed, "take-7.wav"), "there was a failure to clear");
    assert!(!failed.holds("take-7.wav"), "cleared, so the next synthesis may run");
    assert!(
        failed.holds("take-9.wav"),
        "clearing one take must not free the others -- stickiness is per wav"
    );

    // Pressing ▶ on a take that never failed answers false: the caller reads that as a first attempt, not a
    // retry, and does not claim to have rescued anything.
    assert!(!view::retry_line(&mut failed, "never-failed.wav"));
    // And a second retry of the rescued take also answers false: it is already free.
    assert!(!view::retry_line(&mut failed, "take-7.wav"));

    // Re-editing a line gives it a new key, so the freed take and the fresh one are different questions.
    failed.add("take-7.wav");
    assert!(failed.holds("take-7.wav"), "it failed again after the retry");
    assert!(!failed.holds("take-7-edited.wav"), "a new take is not punished for its predecessor");
}

/// S2's resume halves: where the picture goes when a synthesis ends, and the two sentences that tell the
// person which half they are in.
#[test]
fn f4_5_s10_resume_lands_on_the_line_after_success_and_on_frozen_after_failure() {
    // Success resumes at the line's own start, not at the second the picture froze: a hold can land
    // mid-line, and starting a shorter take at the stale offset would begin it past its end.
    assert_eq!(view::resume_after_synthesis(17.4, 16.0, true), 16.0);
    // Failure has no new audio to land on, so the picture carries on from where it stopped.
    assert_eq!(view::resume_after_synthesis(17.4, 16.0, false), 17.4);
    // The two halves really do differ -- if they collapsed to one value the failure path would silently
    // replay the line instead of playing on without it.
    assert_ne!(
        view::resume_after_synthesis(17.4, 16.0, true),
        view::resume_after_synthesis(17.4, 16.0, false)
    );

    // The two sentences are different strings too, and keep their own punctuation: `failed_playing_on` is
    // written with two hyphens, `sticky_failed` with an em dash and its own ▶.
    let played_on = view::failed_playing_on(6);
    let sticky = view::sticky_failed(6);
    assert_eq!(played_on, "line 7 failed -- see log; playing on without it");
    assert!(played_on.contains("--"), "two hyphens, as the prototype writes them");
    assert!(!played_on.contains('\u{2014}'), "not the sticky sentence's em dash");
    assert_eq!(
        sticky,
        "line 7 failed to synthesize \u{2014} see log; its \u{25b6} retries"
    );
    assert!(sticky.contains('\u{2014}'), "the em dash belongs to this one");
    assert!(sticky.contains('\u{25b6}'), "and it names the row's own button as the way out");
    assert_ne!(played_on, sticky, "one is said once, the other every time round");
    // Row numbers are 1-based in both: index 6 is spoken of as line 7.
    assert!(played_on.starts_with("line 7 ") && sticky.starts_with("line 7 "));
}

/// S6: what ⏹ reports -- both players stopped, the triangle returned, and the sentence that says so.
#[test]
fn f4_5_s11_stop_reports_both_players_stopped_and_play_returned() {
    let stopped = view::stop();
    assert!(stopped.picture_paused, "the picture stopped");
    assert!(stopped.voice_paused, "the narration riding along stopped with it");
    assert!(!stopped.started, "nothing is left cued: the preview is ended, not parked");
    assert_eq!(stopped.play_button, view::PLAY_ICON, "\u{25b6} is handed back, not kept");

    // Both really stopped: the ownership sentence is handed out.
    assert_eq!(view::hand_play_back(stopped), Some(view::HANDED_BACK));
    assert_eq!(
        view::HANDED_BACK,
        "\u{25b6} is the step's again \u{2014} the preview stopped"
    );

    // A Stopped that still has a player going must NOT announce itself as stopped: the bar reads `None` as
    // "do not hand ▶ back", so a half-stopped preview cannot be reported as ended.
    let voice_running = view::Stopped { voice_paused: false, ..stopped };
    assert_eq!(view::hand_play_back(voice_running), None);
    let picture_running = view::Stopped { picture_paused: false, ..stopped };
    assert_eq!(view::hand_play_back(picture_running), None);
    let still_cued = view::Stopped { started: true, ..stopped };
    assert_eq!(
        view::hand_play_back(still_cued),
        None,
        "a preview parked part-way is not 'stopped', so ▶ stays the preview's"
    );
}
