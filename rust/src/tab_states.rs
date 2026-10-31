//! The tab states — `spec/11-flow-index.md` §2.

use crate::{
    cut,
    cut_screen::{self, Preview},
    run::{self, Step},
    shell::Page,
};

/// What the run bar's ▶ means on a page right now (§2's third column).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Play {
    Step(Step),
    /// The preview owns the button until ⏹ ends it.
    Transport {
        resume: bool,
    },
    Refused,
}

/// What ▶ means on this page (§2's third column), plus the refusal sentence if there is one.
pub fn play(page: Page, has_cut: bool, transport: run::Transport) -> (Play, Option<&'static str>) {
    // Prepare: "runs Prepare", always. It owns no preview transport ([`run::transport_for`] is
    // None), so neither the cut nor a playing preview can take the button away from it.
    if page == Page::Prepare {
        return (Play::Step(run::step(page)), None);
    }

    // Narrate and Produce refuse without a cut while still opening: §1 keeps both tabs unlocked and
    // puts the refusal on ▶, which is why `opens` answers None for them.
    if !has_cut && matches!(page, Page::Narrate | Page::Produce) {
        return (Play::Refused, Some(cut::NO_CUT_YET));
    }

    // "…until the preview started; then the preview's transport until ⏹": once something has been
    // started on Cut or Narrate, the button belongs to it. ⏹ clears `started`, and the next press
    // falls through to the page's step again.
    if run::transport_for(page, transport).is_some() && transport.started {
        return (
            Play::Transport {
                resume: transport.cued(),
            },
            None,
        );
    }

    (Play::Step(run::step(page)), None)
}

pub fn preview_lits(preview: &Preview) -> [bool; 3] {
    let lits = [
        !preview.cut_only && !preview.reviewing,
        cut_screen::play_cut_lit(preview),
        cut_screen::review_lit(preview),
    ];
    assert!(
        lits.iter().filter(|lit| **lit).count() == 1,
        "§2 asks for exactly one lit preview mode, got {lits:?}"
    );
    lits
}

pub fn exactly_one_preview_lit(preview: &Preview) -> bool {
    let recording = !preview.cut_only && !preview.reviewing;
    [
        recording,
        cut_screen::play_cut_lit(preview),
        cut_screen::review_lit(preview),
    ]
    .iter()
    .filter(|lit| **lit)
    .count()
        >= 1
}
