//! Spoken lines: heard once, whole, where they were put.

use naivepost::cut::Seg;
use naivepost::narrate_preview::{tick, Take, Tick};
use naivepost::narrate_screen::tag_moves_line;
use naivepost::narration::{lines_to_speak, Entry};
use naivepost::produce_exec::fit_spoken;
use naivepost::produce_render::{Clip, Placed};

fn entry(text: &str, pos: &str) -> Entry {
    Entry {
        s: 0.0,
        e: 10.0,
        text: text.into(),
        pos: pos.into(),
        ..Default::default()
    }
}

#[test]
fn a_line_already_speaking_is_not_started_again() {
    let segs = [Seg {
        s: 0.0,
        e: 10.0,
        ..Default::default()
    }];
    let entries = [entry("words", "")];
    let takes = [Take::Exists];
    assert!(matches!(
        tick(1.0, &segs, &entries, &takes, None),
        Tick::Speak { .. }
    ));
    assert_eq!(tick(1.1, &segs, &entries, &takes, Some(0)), Tick::Idle);
}

#[test]
fn a_caption_is_read_and_never_spoken() {
    let spoken = lines_to_speak(&[entry("said", ""), entry("read", "top"), entry("", "")]);
    assert_eq!(spoken.len(), 1);
    assert_eq!(spoken[0].text, "said");
}

#[test]
fn a_second_on_the_session_clock_lands_as_a_place_in_the_clip() {
    assert_eq!(tag_moves_line(100.0, 120.0, 110.0, 3.0), (10.0, false));
    assert_eq!(
        tag_moves_line(100.0, 120.0, 119.5, 3.0),
        (3.0, true),
        "the last second is refused"
    );
    assert_eq!(
        tag_moves_line(100.0, 120.0, 90.0, 3.0),
        (3.0, true),
        "outside the clip"
    );
}

fn clip(on_screen: f64, lines: &[(f64, f64)]) -> Clip {
    Clip {
        no: 1,
        on_screen,
        rate: 1.0,
        lines: lines
            .iter()
            .enumerate()
            .map(|(entry_index, (at, speech))| Placed {
                entry_index,
                at: *at,
                speech: *speech,
                tempo: 1.0,
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    }
}

#[test]
fn takes_longer_than_their_room_no_longer_overlap() {
    let mut clips = [clip(10.0, &[(5.0, 8.0), (6.0, 8.0)])];
    let logs = fit_spoken(&mut clips, &|_| 100.0);
    let lines = &clips[0].lines;
    let first_ends = lines[0].at + lines[0].speech / lines[0].tempo;
    assert!(lines[1].at >= first_ends, "{lines:?}");
    assert!(
        !logs.is_empty() || clips[0].on_screen > 10.0,
        "and it says so or grows: {logs:?}"
    );
}

#[test]
fn a_clip_grows_to_hear_a_line_out_and_keeps_room_it_does_not_need() {
    let mut clips = [clip(10.0, &[(8.0, 3.0)])];
    fit_spoken(&mut clips, &|_| 100.0);
    assert!(
        clips[0].on_screen > 10.0 || clips[0].lines[0].at + 3.0 <= 10.0,
        "{:?}",
        clips[0]
    );
    let mut fits = [clip(10.0, &[(1.0, 3.0)])];
    fit_spoken(&mut fits, &|_| 100.0);
    assert_eq!(fits[0].on_screen, 10.0);
    assert_eq!(fits[0].lines[0].at, 1.0);
}
