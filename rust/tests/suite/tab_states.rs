//! §11-flow-index.md §2 Tab states — when each tab opens, and what the run bar's ▶ means there.

use naivepost::{cut, cut_screen, cut_screen::Preview, narrate_off, project::{Project, Source}, run, shell, tab_states};

const ITEM: &str = "§11-flow-index#2-tab-states";

/// A project with one source whose `footage` flag is as asked for. The only difference between the
/// cases below is that flag and the two run-time states (`has_cut`, the transport), so building the
/// project here keeps §2's columns the thing under test.
fn project(footage: bool, no_narration: bool) -> Project {
    let mut p = Project::default();
    p.no_narration = no_narration;
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
    assert_eq!(tab_states::opens(shell::Page::Prepare, &Project::default()), None);
    assert_eq!(
        tab_states::opens(shell::Page::Prepare, &project(false, false)),
        None,
        "Prepare opens with nothing filmed"
    );

    // Cut: "once a source is footage", and the reason names where to go.
    let no_footage = project(false, false);
    let reason = tab_states::opens(shell::Page::Cut, &no_footage).expect("Cut waits for footage");
    assert_eq!(reason, shell::CUT_LOCK);
    assert!(
        reason.contains("Add footage on the Prepare step first"),
        "the bounce says where the footage goes: {reason}"
    );
    assert_eq!(tab_states::opens(shell::Page::Cut, &project(true, false)), None);

    // Narrate and Produce: "always" — no footage, no cut, narration off or on.
    for page in [shell::Page::Narrate, shell::Page::Produce] {
        for (footage, off) in [(false, false), (true, false), (false, true)] {
            assert_eq!(
                tab_states::opens(page, &project(footage, off)),
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
    let plain = project(true, false);
    let states = [
        (false, run::Transport::default()),
        (true, run::Transport::default()),
        // A started-and-playing transport cannot take the button from Prepare: the page owns none.
        (true, run::Transport { playing: true, started: true }),
    ];
    for (has_cut, transport) in states {
        let (play, reason) = tab_states::play(shell::Page::Prepare, &plain, has_cut, transport);
        assert_eq!(play, tab_states::Play::Step(run::Step::Prepare), "cut={has_cut}");
        assert_eq!(reason, None, "Prepare never refuses");
    }
    // …because Prepare has no preview transport at all (§2 gives it none).
    assert_eq!(
        run::transport_for(shell::Page::Prepare, run::Transport { playing: true, started: true }),
        None
    );
}

// ---- S3: Cut's ▶ — Suggest until the preview started, then the transport ------------------

#[test]
fn sec_11_flow_index_2_tab_states_s3_cut_runs_suggest_until_the_preview_starts_then_drives_it() {
    assert_eq!(ITEM, "§11-flow-index#2-tab-states");
    let filmed = project(true, false);

    // Nothing started: ▶ suggests the cut.
    assert_eq!(
        tab_states::play(shell::Page::Cut, &filmed, false, run::Transport::default()).0,
        tab_states::Play::Step(run::Step::Suggest)
    );

    // Playing: ▶ pauses the preview rather than starting a job.
    assert_eq!(
        tab_states::play(
            shell::Page::Cut,
            &filmed,
            true,
            run::Transport { playing: true, started: true }
        )
        .0,
        tab_states::Play::Transport { resume: false }
    );

    // Started but parked: ▶ resumes it.
    assert_eq!(
        tab_states::play(
            shell::Page::Cut,
            &filmed,
            true,
            run::Transport { playing: false, started: true }
        )
        .0,
        tab_states::Play::Transport { resume: true }
    );

    // ⏹ cleared `started`: ▶ is the page's step again, which is what "until ⏹" means.
    assert_eq!(
        tab_states::play(shell::Page::Cut, &filmed, true, run::Transport::default()).0,
        tab_states::Play::Step(run::Step::Suggest)
    );

    // The two columns stay separate: a Cut page with no footage is refused at the TAB level, not by
    // ▶ — `play` has nothing to say about footage, because the tab was never entered.
    assert!(tab_states::opens(shell::Page::Cut, &project(false, false)).is_some());
    assert_eq!(
        tab_states::play(shell::Page::Cut, &project(false, false), false, run::Transport::default())
            .1,
        None,
        "▶ on Cut never refuses for missing footage; the tab lock already did"
    );

    // And the step this returns is `run::step`'s, so the two tables cannot drift.
    for page in shell::Page::all() {
        let (play, _) = tab_states::play(page, &project(true, false), true, run::Transport::default());
        assert_eq!(play, tab_states::Play::Step(run::step(page)), "{}", page.label());
    }
}

// ---- S4: Narrate's ▶ ----------------------------------------------------------------------

#[test]
fn sec_11_flow_index_2_tab_states_s4_narrate_writes_and_speaks_once_there_is_a_cut() {
    assert_eq!(ITEM, "§11-flow-index#2-tab-states");
    let told = project(true, false);

    // With a cut and no preview running: write and speak.
    assert_eq!(
        tab_states::play(shell::Page::Narrate, &told, true, run::Transport::default()).0,
        tab_states::Play::Step(run::Step::Narrate)
    );

    // Without one: §3's refusal, byte for byte, while the tab itself stayed open.
    let (play, reason) =
        tab_states::play(shell::Page::Narrate, &told, false, run::Transport::default());
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
            &told,
            true,
            run::Transport { playing: true, started: true }
        )
        .0,
        tab_states::Play::Transport { resume: false }
    );
    assert_eq!(
        tab_states::play(
            shell::Page::Narrate,
            &told,
            true,
            run::Transport { playing: false, started: true }
        )
        .0,
        tab_states::Play::Transport { resume: true }
    );
}

// ---- S5: Narrate greyed when narration is off ---------------------------------------------

#[test]
fn sec_11_flow_index_2_tab_states_s5_narration_off_greys_the_page_and_refuses_first() {
    assert_eq!(ITEM, "§11-flow-index#2-tab-states");
    let off = project(true, true);

    // The greying is F4.8's answer, unchanged — lines, preview and voice go grey; the tick does
    // not, because it is the way back out.
    let g = tab_states::greyed(shell::Page::Narrate, &off);
    assert_eq!(g, narrate_off::greyed(true));
    assert!(g.lines && g.preview && g.voice, "the three parts of the page grey out");
    assert!(!g.tick, "the tick stays pressable or the page cannot be escaped");

    // Narrated project: nothing greyed.
    assert_eq!(
        tab_states::greyed(shell::Page::Narrate, &project(true, false)),
        narrate_off::greyed(false)
    );
    for page in [shell::Page::Prepare, shell::Page::Cut, shell::Page::Produce] {
        assert_eq!(
            tab_states::greyed(page, &off),
            narrate_off::Greyed { lines: false, preview: false, voice: false, tick: false },
            "{} greys nothing",
            page.label()
        );
    }

    // Precedence: with narration off AND no cut, ▶ names the tick rather than the cut. There is no
    // voiced cut to point at in a video that will not be narrated.
    let (play, reason) =
        tab_states::play(shell::Page::Narrate, &off, false, run::Transport::default());
    assert_eq!(play, tab_states::Play::Refused);
    assert_eq!(reason.map(str::to_string), narrate_off::refuse_run(true));
    assert_ne!(reason, Some(cut::NO_CUT_YET), "the tick beats the cut");
}

// ---- S6: Produce's ▶ and the Cut preview-mode invariant -----------------------------------

#[test]
fn sec_11_flow_index_2_tab_states_s6_produce_renders_and_exactly_one_preview_mode_is_lit() {
    assert_eq!(ITEM, "§11-flow-index#2-tab-states");
    let told = project(true, false);

    // Produce: render once there is a cut, refuse by name before there is one.
    assert_eq!(
        tab_states::play(shell::Page::Produce, &told, false, run::Transport::default()).1,
        Some(cut::NO_CUT_YET)
    );
    assert_eq!(
        tab_states::play(shell::Page::Produce, &told, true, run::Transport::default()).0,
        tab_states::Play::Step(run::Step::Produce)
    );
    // Produce owns no preview transport, so a running player never holds its ▶.
    assert_eq!(
        run::transport_for(shell::Page::Produce, run::Transport { playing: true, started: true }),
        None
    );
    assert_eq!(
        tab_states::play(
            shell::Page::Produce,
            &told,
            true,
            run::Transport { playing: true, started: true }
        )
        .0,
        tab_states::Play::Step(run::Step::Produce),
        "a playing preview is not Produce's business"
    );

    // §2's "one lit, one ⏸": recording ▶, cut ▶✂, review ▶✂✂ — exactly one at a time.
    let recording = Preview { cut_only: false, reviewing: false };
    let cut_mode = Preview { cut_only: true, reviewing: false };
    let review = Preview { cut_only: false, reviewing: true };
    assert!(tab_states::exactly_one_preview_lit(&recording), "recording ▶ alone");
    assert!(tab_states::exactly_one_preview_lit(&cut_mode), "cut ▶✂ alone");
    assert!(tab_states::exactly_one_preview_lit(&review), "review ▶✂✂ alone");
    // Both ✂ flags set is not a state the page reaches: `play_cut_lit` deliberately goes dark under
    // a review (▶✂ and ▶✂✂ skip the same stretches, so only the tour lights a button), which is
    // what keeps §2's "one lit" true even there.
    assert!(
        tab_states::exactly_one_preview_lit(&Preview { cut_only: true, reviewing: true }),
        "a review lights ▶✂✂ alone even with cut_only left set"
    );
    assert!(!cut_screen::play_cut_lit(&Preview { cut_only: true, reviewing: true }));
    // ▶✂✂ plays P.policy.reviewPadSeconds (cut_review::REVIEW_PAD_SECONDS, 10 s) either side of
    // each join, and `cut_screen::can_review` (≥2 real clips) is what decides whether the button
    // is sensitive enough to reach that rule at all.
    assert_eq!(naivepost::cut_review::REVIEW_PAD_SECONDS, 10.0);
    assert!(cut_screen::play_cut_lit(&cut_mode));
    assert!(cut_screen::review_lit(&review));
    // Under a review, ▶✂ is not lit even though it skips the same stretches.
    assert!(!cut_screen::play_cut_lit(&review));
}
