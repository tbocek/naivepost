//! F2.2 S5 — ▶✂ crosses a seam when the player reaches the clip's end, the second the render cuts at.
//!
//! The real tick loop ([`start_cut_preview_tick`]) on a GLib main loop, over a player whose second runs with the
//! wall clock. First rewrite: the tick jumped once the player was within one tick (100 ms) of the end, so up to
//! 100 ms of every kept clip never played in the preview while the render kept it.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use naivepost::cut::{Cut, Seg};
use naivepost::cut_play_leg::{start_cut_preview_tick, CutPreviewPage};

/// A player that plays from `start` with the wall clock, and from wherever the line is set when it jumps.
struct Player {
    cut_: Cut,
    origin: Cell<(Instant, f64)>,
    /// (player second just before the jump, where it jumped to)
    jumps: RefCell<Vec<(f64, f64)>>,
}

impl Player {
    fn now(&self) -> f64 {
        let (at, from) = self.origin.get();
        from + at.elapsed().as_secs_f64()
    }
}

struct Page(Rc<Player>);

impl CutPreviewPage for Page {
    fn cut_only(&self) -> bool {
        true
    }
    fn playhead(&self) -> Option<f64> {
        Some(self.0.now())
    }
    fn player_second(&self) -> Option<f64> {
        Some(self.0.now())
    }
    fn set_playhead(&self, at: f64) {
        let now = self.0.now();
        // A move of more than a tick is a seek: the player goes there.
        if (at - now).abs() > 0.2 {
            self.0.jumps.borrow_mut().push((now, at));
            self.0.origin.set((Instant::now(), at));
        }
    }
    fn cut(&self) -> Cut {
        self.0.cut_.clone()
    }
    fn say_clock(&self, _clock: f64, _ended: bool) {}
    fn volume(&self) -> f64 {
        1.0
    }
    fn arm_spare_at(&self, _at: f64) -> bool {
        false
    }
    fn cross_into_spare(&self) -> bool {
        false
    }
    fn drop_spare(&self) {}
    fn hold_still(&self, _hold: bool) -> bool {
        false
    }
    fn play_rate(&self, _rate: f64, _at: f64) -> bool {
        false
    }
    fn apply_gain(&self, _gain: f64, _at_secs: f64) {}
}

#[test]
fn f2_2_s5_the_preview_crosses_a_seam_at_the_clips_end() {
    let seg = |s: f64, e: f64| Seg {
        s,
        e,
        ..Default::default()
    };
    // Three seams, each landing at a different point between two ticks.
    let cut_ = Cut {
        segs: vec![seg(0.0, 10.0), seg(20.0, 20.437), seg(30.0, 30.271), seg(40.0, 41.0)],
        ..Default::default()
    };
    let player = Rc::new(Player {
        cut_,
        origin: Cell::new((Instant::now(), 9.537)),
        jumps: RefCell::new(Vec::new()),
    });
    // `glib::timeout_add_local` attaches to the global default context, so the loop is turned there.
    let context = glib::MainContext::default();
    let _owned = context.acquire().expect("the test owns the main context");
    start_cut_preview_tick(Rc::new(Page(player.clone())));
    let until = Instant::now() + Duration::from_millis(1800);
    while Instant::now() < until && player.jumps.borrow().len() < 3 {
        context.iteration(false);
        std::thread::sleep(Duration::from_millis(1));
    }
    let jumps = player.jumps.borrow().clone();
    assert_eq!(
        jumps.iter().map(|j| j.1).collect::<Vec<_>>(),
        [20.0, 30.0, 40.0],
        "every hole skipped, in order: {jumps:?}"
    );
    for ((at, _), end) in jumps.iter().zip([10.0, 20.437, 30.271]) {
        let early = (end - at) * 1000.0;
        // Never a tick early; late only by the timer's own slack.
        assert!(
            (-25.0..=2.0).contains(&early),
            "crossed {early:.1} ms before the clip ends at {end} (player at {at:.4}): {jumps:?}"
        );
    }
}
