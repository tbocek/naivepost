//! ⏸ holds a walk at its checkpoint, and ⏹ gets out of it.

use std::cell::Cell;

use naivepost::run_stages::park_while;

#[test]
fn not_paused_goes_straight_on() {
    assert!(park_while(|| false, || false));
}

#[test]
fn a_pause_holds_until_it_is_lifted() {
    let looks = Cell::new(0);
    let carried_on = park_while(
        || {
            looks.set(looks.get() + 1);
            looks.get() < 3
        },
        || false,
    );
    assert!(carried_on);
    assert_eq!(looks.get(), 3);
}

#[test]
fn a_stop_ends_a_pause() {
    assert!(!park_while(|| true, || true));
}
