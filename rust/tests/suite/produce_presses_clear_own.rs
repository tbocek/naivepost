//! F5.7: the order of ↻ over the thumbnail. The draw itself is the run's walk; its record is proved in
//! `f5_7_where_it_runs_a_press_returns_busy_then_lands.rs`.

use std::cell::RefCell;
use std::rc::Rc;

use naivepost::produce_presses::{self as presses, Legs};
use naivepost::produce_runs as runs;

type Calls = Rc<RefCell<Vec<&'static str>>>;

fn legs(calls: &Calls) -> Legs {
    let cleared = calls.clone();
    Legs {
        has_cut: Box::new(|| true),
        has_video: Box::new(|| true),
        has_picture: Box::new(|| true),
        rendering: Box::new(|| false),
        words_of: Box::new(|_| Vec::new()),
        instruction: Box::new(|| "a wide shot".to_string()),
        print: Box::new(|_, _| Ok(())),
        encode: Box::new(|_| Ok(())),
        copy: Box::new(|_| Ok(())),
        jpeg_sizes: Box::new(|_| vec![1]),
        was_stopped: Box::new(|| false),
        clear_own: Box::new(move || {
            cleared.borrow_mut().push("clear");
            Ok(())
        }),
        count_prints: true,
    }
}

fn press(legs: &Legs, calls: &Calls, drawn: Result<(), String>) -> presses::Outcome {
    let draw = calls.clone();
    presses::press_redraw_thumbnail(
        legs,
        move || {
            draw.borrow_mut().push("draw");
            drawn
        },
        |_| {},
    )
}

#[test]
fn a_refused_redraw_neither_draws_nor_clears() {
    let calls = Calls::default();
    let mut no_cut = legs(&calls);
    no_cut.has_cut = Box::new(|| false);
    assert!(press(&no_cut, &calls, Ok(())).refused);
    let mut no_words = legs(&calls);
    no_words.instruction = Box::new(|| "  ".to_string());
    assert!(press(&no_words, &calls, Ok(())).refused);
    assert!(calls.borrow().is_empty());
}

#[test]
fn a_failed_draw_keeps_the_chosen_picture() {
    let calls = Calls::default();
    let out = press(&legs(&calls), &calls, Err("server down".to_string()));
    assert_eq!(*calls.borrow(), ["draw"]);
    assert_eq!(out.error.as_deref(), Some("server down"));
    assert_eq!(out.status, runs::ending("the thumbnail", "", false, false));
}

#[test]
fn a_finished_draw_hands_the_picture_back() {
    let calls = Calls::default();
    let out = press(&legs(&calls), &calls, Ok(()));
    assert_eq!(*calls.borrow(), ["draw", "clear"]);
    assert_eq!(out.sd_calls, 1);
    assert_eq!(out.status, runs::REDRAW_DONE);
}
