//! The tab states — `spec/11-flow-index.md` §2.
//!
//! §2's table asks two questions of each of the four tabs: *when it opens*, and *what the run bar's
//! ▶ means there*. Both answers exist already, one rule per module; this module only puts the two
//! columns side by side so a caller (the run bar, the tab row) asks one question and gets one
//! answer. Nothing here decides anything twice:
//!
//! | column | who actually owns the rule |
//! |---|---|
//! | when it opens | [`crate::shell::lock`] |
//! | what ▶ runs | [`crate::run::step`] + [`crate::run::transport_for`] |
//! | refuses without a cut | [`crate::cut::NO_CUT_YET`] / [`crate::cut::exists`] |
//! | greyed when narration is off | [`crate::narrate_off::greyed`] |
//! | the three preview modes | [`crate::cut_screen`] (`play_cut_lit`, `review_lit`) |
//!
//! Keeping the aggregation separate rather than folding it into `shell` or `run` is what lets a test
//! ask "what does ▶ do on Narrate with no cut and narration off" without a window and without a
//! filesystem — see [`play`].

use crate::{
    cut,
    cut_screen::{self, Preview},
    narrate_off,
    project::Project,
    run::{self, Step},
    shell::{self, Page},
};

/// What the run bar's ▶ means on a page right now (§2's third column).
///
/// `Transport { resume }` is the preview's own transport: `resume` true when it is cued (started
/// but paused), false when it is playing and ▶ would pause it — the same shape as
/// [`run::Pressed::ToggledTransport`], which is what a press turns into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Play {
    /// Start the page's step: Prepare runs Prepare, Cut runs Suggest, [`Step::Narrate`] is
    /// "write and speak", [`Step::Produce`] is "render".
    Step(Step),
    /// The preview owns the button until ⏹ ends it.
    Transport { resume: bool },
    /// ▶ has nothing to start here, and says why — the reason comes back as the second half of
    /// [`play`]'s answer so no caller invents its own words.
    Refused,
}

/// §2's refusal sentence for Narrate with narration off. F4.8 spells it in a `String` because the
/// module also builds refusals from project text; here it is one fixed sentence, so it lives as a
/// constant rather than being leaked at every call.
pub const NARRATION_OFF: &str = "this video has no narration \u{2014} tick Narration at the top of this page to write one";

/// Whether the tab opens, `None` meaning it does (§2's second column: "always" for three of them).
///
/// Only Cut waits, and its reason is [`shell::CUT_LOCK`] — the sentence a click bounces with and a
/// hover shows. Narrate and Produce open always even with no footage and no cut: their refusal lives
/// in [`play`], because the voice picker and the render settings are useful before any cut exists.
pub fn opens(page: Page, project: &Project) -> Option<&'static str> {
    shell::lock(page, project)
}

/// What ▶ means on this page (§2's third column), plus the refusal sentence if there is one.
///
/// `has_cut` is the caller's answer from [`cut::exists`] — a bool rather than a tree, so this rule
/// reads no filesystem and a test can set the state directly. Returns the play meaning and, for the
/// two refusals, the sentence the status line shows; `(Play::Refused(..), reason)` pairs are kept
/// together because the bar needs both and a caller must not pick its own words.
pub fn play(
    page: Page,
    project: &Project,
    has_cut: bool,
    transport: run::Transport,
) -> (Play, Option<&'static str>) {
    // Prepare: "runs Prepare", always. It owns no preview transport ([`run::transport_for`] is
    // None), so neither the cut nor a playing preview can take the button away from it.
    if page == Page::Prepare {
        return (Play::Step(run::step(page)), None);
    }

    // Narrate with narration off refuses before the missing-cut check. §2 lists the greying as the
    // state and F4.8 stops the run at its own start with the sentence that names the way back; a
    // video with no narration will never have a voiced cut either, so naming the tick beats naming
    // a cut that would not be spoken anyway. (§2 does not order these two refusals.)
    if page == Page::Narrate && project.no_narration {
        assert_eq!(
            Some(NARRATION_OFF.to_string()),
            narrate_off::refuse_run(true),
            "the constant here and F4.8's sentence must stay one sentence"
        );
        return (Play::Refused, Some(NARRATION_OFF));
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

/// Which parts of the tab's page are greyed (§2: "greyed when narration is off").
///
/// Only Narrate greys, and only through [`narrate_off::greyed`] — whose `tick` field is always
/// false, which is what keeps the tab escapable: turning narration back on must stay pressable on
/// the page that greys everything else. The other three pages never grey.
pub fn greyed(page: Page, project: &Project) -> narrate_off::Greyed {
    match page {
        Page::Narrate => narrate_off::greyed(project.no_narration),
        Page::Prepare | Page::Cut | Page::Produce => narrate_off::Greyed {
            lines: false,
            preview: false,
            voice: false,
            tick: false,
        },
    }
}

/// §2's "one lit, one ⏸": the three Cut preview modes are recording ▶, cut ▶✂ and review ▶✂✂, and
/// exactly one of them is lit at a time.
///
/// The individual rules are [`cut_screen::play_cut_lit`] and [`cut_screen::review_lit`]; the
/// recording is not a flag of its own but what is left when neither ✂ mode is on (see
/// [`crate::preview`]). So a state with both ✂ flags set lights ▶✂✂ only — which is still one —
/// while the broken states are the ones where a rule stops lighting anything: hence *at least* one
/// lit for every combination, rather than an exact count that would never catch the bug it guards.
pub fn exactly_one_preview_lit(preview: &Preview) -> bool {
    let recording = !preview.cut_only && !preview.reviewing;
    [recording, cut_screen::play_cut_lit(preview), cut_screen::review_lit(preview)]
        .iter()
        .filter(|lit| **lit)
        .count()
        >= 1
}
