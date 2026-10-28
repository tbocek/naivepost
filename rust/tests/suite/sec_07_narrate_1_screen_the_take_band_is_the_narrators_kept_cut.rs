//! §07-narrate#1-screen **10**: the take band of a narrator is everything that narrator says in the cut, end to
//! end: the stretches of every recording tagged with that narrator that the cut keeps (the Cut page's green bars),
//! in the cut's order. A take is stored in its own recording's seconds, under that recording's name, and shows on
//! the band where that second lies. The first rewrite showed the first tagged recording alone, whole, so a session
//! filmed in nineteen files offered half a minute of the narrator to pick from.

use std::collections::BTreeMap;
use std::path::PathBuf;

use naivepost::cut::{Cut, Seg};
use naivepost::narrate_data::Take;
use naivepost::narrate_tape;
use naivepost::timeline::Recording;

fn seg(s: f64, e: f64) -> Seg {
    Seg {
        s,
        e,
        ..Seg::default()
    }
}

/// Two narrated recordings one after another on the session clock (a at 0–30 s, b at 40–100 s) and a third, c,
/// at 100–120 s that nobody tagged. The cut keeps 5–10 of a, 50–60 and 70–75 of b, and 105–110 of c.
fn session() -> (Cut, Vec<Recording>, Vec<(String, PathBuf)>) {
    let cut = Cut {
        segs: vec![
            seg(5.0, 10.0),
            seg(50.0, 60.0),
            seg(70.0, 75.0),
            seg(105.0, 110.0),
        ],
        ..Cut::default()
    };
    let recordings = vec![
        Recording {
            base: "a".into(),
            start: 0.0,
            end: 30.0,
        },
        Recording {
            base: "b".into(),
            start: 40.0,
            end: 100.0,
        },
        Recording {
            base: "c".into(),
            start: 100.0,
            end: 120.0,
        },
    ];
    let narrated = vec![
        ("a".into(), PathBuf::from("/s/a.mkv")),
        ("b".into(), PathBuf::from("/s/b.mkv")),
    ];
    (cut, recordings, narrated)
}

#[test]
fn sec_07_narrate_1_screen_the_band_lays_every_kept_stretch_of_the_narrator_end_to_end() {
    let (cut, recordings, narrated) = session();
    let tape = narrate_tape::kept(&cut, &recordings, &narrated);
    let laid: Vec<(&str, f64, f64, f64)> = tape
        .0
        .iter()
        .map(|s| (s.base.as_str(), s.from, s.to, s.at))
        .collect();
    assert_eq!(
        laid,
        vec![("a", 5.0, 10.0, 0.0), ("b", 10.0, 20.0, 5.0), ("b", 30.0, 35.0, 15.0)],
        "each kept stretch in its recording's own seconds, laid after the one before; c is nobody's narration"
    );
    assert_eq!(tape.length(), 20.0);
    assert_eq!(tape.bases(), vec!["a".to_string(), "b".to_string()]);
    // A band span over a seam is a piece of each recording.
    let pieces: Vec<(&str, f64, f64, f64)> = tape
        .pieces((3.0, 8.0))
        .into_iter()
        .map(|(s, f, t, b)| (s.base.as_str(), f, t, b))
        .collect();
    assert_eq!(pieces, vec![("a", 8.0, 10.0, 3.0), ("b", 10.0, 13.0, 5.0)]);
}

#[test]
fn sec_07_narrate_1_screen_a_take_is_stored_in_its_recordings_seconds_and_shown_where_it_lies() {
    let (cut, recordings, narrated) = session();
    let tape = narrate_tape::kept(&cut, &recordings, &narrated);
    let mut stored: BTreeMap<String, Vec<Take>> = BTreeMap::new();
    // b's 12–15 s is on the band at 7–10; its 0–2 s is not kept, so it is not on the band.
    stored.insert(
        "b".into(),
        vec![Take { s: 0.0, e: 2.0 }, Take { s: 12.0, e: 15.0 }],
    );
    stored.insert("c".into(), vec![Take { s: 6.0, e: 9.0 }]);
    assert_eq!(
        tape.takes_on(&stored),
        vec![(7.0, 10.0)],
        "only what the band shows, where it shows it"
    );

    // A take dragged across the seam between a and b lands as one take in each, in their own seconds; b's take
    // outside the cut stays, b's old take inside the band is replaced by what the band now holds, c is not touched.
    let next = tape.store(&[(3.0, 8.0)], &stored);
    assert_eq!(next.get("a"), Some(&vec![Take { s: 8.0, e: 10.0 }]));
    assert_eq!(
        next.get("b"),
        Some(&vec![Take { s: 0.0, e: 2.0 }, Take { s: 10.0, e: 13.0 }])
    );
    assert_eq!(
        next.get("c"),
        None,
        "a recording the band does not show is not the band's to write"
    );
    // And what was stored shows back as the one span that was dragged.
    let mut all = stored.clone();
    all.extend(next);
    assert_eq!(tape.takes_on(&all), vec![(3.0, 8.0)]);
}
