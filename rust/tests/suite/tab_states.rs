//! §11-flow-index.md §2 Tab states — when each tab opens, and what the run bar's ▶ means there.

use naivepost::{
    cut, cut_screen,
    cut_screen::Preview,
    project::{Project, Source},
    run, shell, tab_states,
};

const ITEM: &str = "§11-flow-index#2-tab-states";

/// A project with one source whose `footage` flag is as asked for. The only difference between the
/// cases below is that flag and the two run-time states (`has_cut`, the transport), so building the
/// project here keeps §2's columns the thing under test.
fn project(footage: bool) -> Project {
    let mut p = Project::default();
    p.sources.push(Source {
        path: "take.mp4".into(),
        footage,
        ..Default::default()
    });
    p
}

// ---- S1: when each tab opens --------------------------------------------------------------

#[test]
fn sec_11_flow_index_2_tab_states_s1_three_tabs_open_always_and_cut_waits_for_footage() {
    assert_eq!(ITEM, "§11-flow-index#2-tab-states");
    // Prepare: "always" — it is where the sources get added, so an empty project must open it.
    assert_eq!(
        tab_states::opens(shell::Page::Prepare, &Project::default()),
        None
    );
    assert_eq!(
        tab_states::opens(shell::Page::Prepare, &project(false)),
        None,
        "Prepare opens with nothing filmed"
    );

    // Cut: "once a source is footage", and the reason names where to go.
    let no_footage = project(false);
    let reason = tab_states::opens(shell::Page::Cut, &no_footage).expect("Cut waits for footage");
    assert_eq!(reason, shell::CUT_LOCK);
    assert!(
        reason.contains("Add footage on the Prepare step first"),
        "the bounce says where the footage goes: {reason}"
    );
    assert_eq!(tab_states::opens(shell::Page::Cut, &project(true)), None);

    // Narrate and Produce: "always" — with footage or without.
    for page in [shell::Page::Narrate, shell::Page::Produce] {
        for footage in [false, true] {
            assert_eq!(
                tab_states::opens(page, &project(footage)),
                None,
                "{} opens whatever the state",
                page.label()
            );
        }
    }
}

// ---- S2: Prepare's ▶ ----------------------------------------------------------------------

#[test]
fn sec_11_flow_index_2_tab_states_s2_prepare_runs_prepare_whatever_else_is_going_on() {
    assert_eq!(ITEM, "§11-flow-index#2-tab-states");
    let states = [
        (false, run::Transport::default()),
        (true, run::Transport::default()),
        // A started-and-playing transport cannot take the button from Prepare: the page owns none.
        (
            true,
            run::Transport {
                playing: true,
                started: true,
            },
        ),
    ];
    for (has_cut, transport) in states {
        let (play, reason) = tab_states::play(shell::Page::Prepare, has_cut, transport);
        assert_eq!(
            play,
            tab_states::Play::Step(run::Step::Prepare),
            "cut={has_cut}"
        );
        assert_eq!(reason, None, "Prepare never refuses");
    }
    // …because Prepare has no preview transport at all (§2 gives it none).
    assert_eq!(
        run::transport_for(
            shell::Page::Prepare,
            run::Transport {
                playing: true,
                started: true
            }
        ),
        None
    );
}

// ---- S3: Cut's ▶ — Suggest until the preview started, then the transport ------------------

#[test]
fn sec_11_flow_index_2_tab_states_s3_cut_runs_suggest_until_the_preview_starts_then_drives_it() {
    assert_eq!(ITEM, "§11-flow-index#2-tab-states");

    // Nothing started: ▶ suggests the cut.
    assert_eq!(
        tab_states::play(shell::Page::Cut, false, run::Transport::default()).0,
        tab_states::Play::Step(run::Step::Suggest)
    );

    // Playing: ▶ pauses the preview rather than starting a job.
    assert_eq!(
        tab_states::play(
            shell::Page::Cut,
            true,
            run::Transport {
                playing: true,
                started: true
            }
        )
        .0,
        tab_states::Play::Transport { resume: false }
    );

    // Started but parked: ▶ resumes it.
    assert_eq!(
        tab_states::play(
            shell::Page::Cut,
            true,
            run::Transport {
                playing: false,
                started: true
            }
        )
        .0,
        tab_states::Play::Transport { resume: true }
    );

    // ⏹ cleared `started`: ▶ is the page's step again, which is what "until ⏹" means.
    assert_eq!(
        tab_states::play(shell::Page::Cut, true, run::Transport::default()).0,
        tab_states::Play::Step(run::Step::Suggest)
    );

    // The two columns stay separate: a Cut page with no footage is refused at the TAB level, not by
    // ▶ — `play` has nothing to say about footage, because the tab was never entered.
    assert!(tab_states::opens(shell::Page::Cut, &project(false)).is_some());
    assert_eq!(
        tab_states::play(shell::Page::Cut, false, run::Transport::default()).1,
        None,
        "▶ on Cut never refuses for missing footage; the tab lock already did"
    );

    // And the step this returns is `run::step`'s, so the two tables cannot drift.
    for page in shell::Page::all() {
        let (play, _) = tab_states::play(page, true, run::Transport::default());
        assert_eq!(
            play,
            tab_states::Play::Step(run::step(page)),
            "{}",
            page.label()
        );
    }
}

// ---- S4: Narrate's ▶ ----------------------------------------------------------------------

#[test]
fn sec_11_flow_index_2_tab_states_s4_narrate_writes_and_speaks_once_there_is_a_cut() {
    assert_eq!(ITEM, "§11-flow-index#2-tab-states");

    // With a cut and no preview running: write and speak.
    assert_eq!(
        tab_states::play(shell::Page::Narrate, true, run::Transport::default()).0,
        tab_states::Play::Step(run::Step::Narrate)
    );

    // Without one: §3's refusal, byte for byte, while the tab itself stayed open.
    let (play, reason) = tab_states::play(shell::Page::Narrate, false, run::Transport::default());
    assert_eq!(play, tab_states::Play::Refused);
    assert_eq!(reason, Some(cut::NO_CUT_YET));
    assert_eq!(
        reason.unwrap(),
        "no cut yet \u{2014} build one on the Cut step first",
        "the sentence is §3's, em dash included"
    );

    // A started preview owns the button here too, playing or cued.
    assert_eq!(
        tab_states::play(
            shell::Page::Narrate,
            true,
            run::Transport {
                playing: true,
                started: true
            }
        )
        .0,
        tab_states::Play::Transport { resume: false }
    );
    assert_eq!(
        tab_states::play(
            shell::Page::Narrate,
            true,
            run::Transport {
                playing: false,
                started: true
            }
        )
        .0,
        tab_states::Play::Transport { resume: true }
    );
}

// ---- S5: Narrate has no narration switch -------------------------------------------------

/// There is no narration switch (§07 §1; first rewrite: a tick that greyed the page and refused ▶). Narrate's ▶
/// refuses only for want of a cut, in the cut's own words.
#[test]
fn sec_11_flow_index_2_tab_states_s5_narrate_refuses_only_without_a_cut() {
    assert_eq!(ITEM, "§11-flow-index#2-tab-states");
    let (play, reason) = tab_states::play(shell::Page::Narrate, false, run::Transport::default());
    assert_eq!(play, tab_states::Play::Refused);
    assert_eq!(reason, Some(cut::NO_CUT_YET));
    assert_eq!(
        tab_states::play(shell::Page::Narrate, true, run::Transport::default()),
        (tab_states::Play::Step(run::Step::Narrate), None),
        "with a cut, ▶ writes and speaks"
    );
}

// ---- S6: Produce's ▶ and the Cut preview-mode invariant -----------------------------------

#[test]
fn sec_11_flow_index_2_tab_states_s6_produce_renders_and_exactly_one_preview_mode_is_lit() {
    assert_eq!(ITEM, "§11-flow-index#2-tab-states");

    // Produce: render once there is a cut, refuse by name before there is one.
    assert_eq!(
        tab_states::play(shell::Page::Produce, false, run::Transport::default()).1,
        Some(cut::NO_CUT_YET)
    );
    assert_eq!(
        tab_states::play(shell::Page::Produce, true, run::Transport::default()).0,
        tab_states::Play::Step(run::Step::Produce)
    );
    // Produce owns no preview transport, so a running player never holds its ▶.
    assert_eq!(
        run::transport_for(
            shell::Page::Produce,
            run::Transport {
                playing: true,
                started: true
            }
        ),
        None
    );
    assert_eq!(
        tab_states::play(
            shell::Page::Produce,
            true,
            run::Transport {
                playing: true,
                started: true
            }
        )
        .0,
        tab_states::Play::Step(run::Step::Produce),
        "a playing preview is not Produce's business"
    );

    // §2's "one lit, one ⏸": recording ▶, cut ▶✂, review ▶✂✂ — exactly one at a time.
    let recording = Preview {
        cut_only: false,
        reviewing: false,
    };
    let cut_mode = Preview {
        cut_only: true,
        reviewing: false,
    };
    let review = Preview {
        cut_only: false,
        reviewing: true,
    };
    assert!(
        tab_states::exactly_one_preview_lit(&recording),
        "recording ▶ alone"
    );
    assert!(
        tab_states::exactly_one_preview_lit(&cut_mode),
        "cut ▶✂ alone"
    );
    assert!(
        tab_states::exactly_one_preview_lit(&review),
        "review ▶✂✂ alone"
    );
    // Both ✂ flags set is not a state the page reaches: `play_cut_lit` deliberately goes dark under
    // a review (▶✂ and ▶✂✂ skip the same stretches, so only the tour lights a button), which is
    // what keeps §2's "one lit" true even there.
    assert!(
        tab_states::exactly_one_preview_lit(&Preview {
            cut_only: true,
            reviewing: true
        }),
        "a review lights ▶✂✂ alone even with cut_only left set"
    );
    assert!(!cut_screen::play_cut_lit(&Preview {
        cut_only: true,
        reviewing: true
    }));
    // ▶✂✂ plays P.policy.reviewPadSeconds (cut_review::REVIEW_PAD_SECONDS, 10 s) either side of
    // each join, and `cut_screen::can_review` (≥2 real clips) is what decides whether the button
    // is sensitive enough to reach that rule at all.
    assert_eq!(naivepost::cut_review::REVIEW_PAD_SECONDS, 10.0);
    assert!(cut_screen::play_cut_lit(&cut_mode));
    assert!(cut_screen::review_lit(&review));
    // Under a review, ▶✂ is not lit even though it skips the same stretches.
    assert!(!cut_screen::play_cut_lit(&review));
}
