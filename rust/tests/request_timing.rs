//! §09-llm-and-tools#10-every-request-timed-new — timing every request that leaves the machine.
//!
//! `requests.tsv` (§6) keeps one line per outside call: eighteen columns, four services (`llm`,
//! `audio`, `image`, `web`) and five outcomes (`ok`, `cache`, `error <status or reason>`,
//! `stalled`, `cancelled`). This exercises the half of it that is *numbers*: the gate wait of §4, the
//! first byte, the time on the wire, the thinking share, the cache row that has no time at all, the
//! retry as its own row, and the per-service line a run ends with.
//!
//! Every timestamp is a literal epoch millisecond passed in, so nothing sleeps and no clock is read;
//! the writes go through `requests::record` and come back through `requests::read`. Column indices
//! are derived from `requests::HEADER` rather than hard-coded, so a column moving shows up here as a
//! failure rather than as a silently-wrong assertion.

use std::path::PathBuf;

use naivepost::layout::Tree;
use naivepost::request_timing as rt;
use naivepost::requests::{self, Outcome, Request, Service};

const ITEM: &str = "§09-llm-and-tools#10-every-request-timed-new";

/// A project folder inside a throwaway root (same shape as `tests/text_formats_sidecars.rs`).
fn project(tag: &str) -> (PathBuf, Tree) {
    let root = std::env::temp_dir().join(format!(
        "naivepost-request-timing-{}-{tag}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let dir = root.join("demo.naivepost");
    std::fs::create_dir_all(&dir).unwrap();
    (root, Tree::new(&dir).unwrap())
}

/// The index of a column in `requests.tsv`, by the name §6 gives it.
fn column(name: &str) -> usize {
    requests::HEADER
        .split('\t')
        .position(|field| field == name)
        .unwrap_or_else(|| panic!("no {name} column in {}", requests::HEADER))
}

/// A row whose identity fields say what it is; timings are filled by the function under test.
///
/// No time on the wire: a bare base row knows nothing about how long anything took, which is what
/// lets the cache-row tests tell "never went out" apart from "went out and was instant".
fn base(service: Service, step: &str, kind: &str) -> Request {
    Request {
        started: "2026-09-24 10:00:00.000".to_string(),
        run: "0924-100000-prepare.html".to_string(),
        step: step.to_string(),
        job: kind.to_string(),
        service,
        model: "qwen-vl".to_string(),
        kind: kind.to_string(),
        sent_bytes: Some(4096),
        images: Some(2),
        received_bytes: Some(512),
        tokens_in: Some(900),
        tokens_out: Some(40),
        ..Default::default()
    }
}

/// The same row with a known time on the wire, for the tallies that need one.
fn worked(service: Service, step: &str, kind: &str, on_wire_s: f64) -> Request {
    Request {
        on_wire_s: Some(on_wire_s),
        first_byte_s: Some((on_wire_s / 2.0).min(on_wire_s)),
        ..base(service, step, kind)
    }
}

#[test]
fn sec_09_llm_and_tools_10_every_request_timed_new_s0_the_item_under_test_is_named() {
    // The round's id, kept in a string so it survives grepping and cannot be a stale comment.
    assert_eq!(ITEM, "§09-llm-and-tools#10-every-request-timed-new");
}

#[test]
fn sec_09_llm_and_tools_10_every_request_timed_new_s1_the_four_timings_are_captured() {
    // §6's header still carries the four timing columns this module fills, under those names.
    assert_eq!(requests::HEADER.split('\t').count(), 18, "{}", requests::HEADER);
    for name in ["wait_s", "first_byte_s", "on_wire_s", "thinking_s"] {
        assert!(column(name) < 18, "{name} is not a column");
    }

    // Queued 4 s behind the gate (§4), out at t=100000, first byte 1.5 s later, done 20 s after
    // going out, of which 3 s was thinking.
    let mut timer = rt::Timer::queued(4000, 100_000);
    timer.first_byte(101_500);
    timer.finished(120_000);
    timer.thought(3000);

    let row = rt::finish_row(&base(Service::Llm, "cut", "chat"), &timer, Outcome::Ok, 1);
    assert_eq!(row.wait_s, Some(4.0));
    assert_eq!(row.first_byte_s, Some(1.5));
    assert_eq!(row.on_wire_s, Some(20.0));
    assert_eq!(row.thinking_s, Some(3.0));
    // Identity survived the trip through finish_row.
    assert_eq!(row.step, "cut");
    assert_eq!(row.service, Service::Llm);

    // And the written row spells them with the two decimals §6 asks for.
    let written = requests::write_line(&row);
    let fields: Vec<&str> = written.trim_end().split('\t').collect();
    assert_eq!(fields[column("wait_s")], "4.00");
    assert_eq!(fields[column("first_byte_s")], "1.50");
    assert_eq!(fields[column("on_wire_s")], "20.00");
    assert_eq!(fields[column("thinking_s")], "3.00");
}

#[test]
fn sec_09_llm_and_tools_10_every_request_timed_new_s2_every_outcome_leaves_a_row_behind() {
    // §10 lists four endings -- succeeded, failed, given up by the stall watch, cancelled by the
    // stop button -- plus the cache hit, which s4 covers. All five spellings round-trip.
    let cases: Vec<(Outcome, &str)> = vec![
        (Outcome::Ok, "ok"),
        (Outcome::Error("500".into()), "error 500"),
        (Outcome::Error("connection refused".into()), "error connection refused"),
        (Outcome::Stalled, "stalled"),
        (Outcome::Cancelled, "cancelled"),
    ];
    let mut timer = rt::Timer::sent(100_000);
    timer.finished(103_000);

    for (outcome, spelling) in cases {
        let row = rt::finish_row(&base(Service::Audio, "transcribe", "asr"), &timer, outcome.clone(), 1);
        let line = requests::write_line(&row);
        let fields: Vec<&str> = line.trim_end().split('\t').collect();
        assert_eq!(fields.len(), 18, "{line}");
        // The outcome is the second-to-last field, before the attempt number.
        assert_eq!(fields[column("outcome")], spelling, "{line}");
        // And it reads back as the same variant, not as a string that merely looks alike.
        let back = requests::parse_line(&line).expect("parses back");
        assert_eq!(back.outcome, outcome, "{line}");
        assert_eq!(back.attempt, 1);
    }
}

#[test]
fn sec_09_llm_and_tools_10_every_request_timed_new_s3_a_retry_is_a_line_of_its_own() {
    // A first try has never been attempted; each retry counts up off the last row.
    assert_eq!(rt::next_attempt(None), 1, "attempts start at 1, not 0");
    let first = rt::finish_row(
        &base(Service::Llm, "narrate", "chat"),
        &{
            let mut t = rt::Timer::queued(1000, 200_000);
            t.finished(260_000);
            t
        },
        Outcome::Error("timeout".into()),
        rt::next_attempt(None),
    );
    let second = rt::finish_row(
        &first,
        &{
            let mut t = rt::Timer::queued(500, 300_000);
            t.first_byte(301_000);
            t.finished(340_000);
            t
        },
        Outcome::Ok,
        rt::next_attempt(Some(&first)),
    );
    assert_eq!(first.attempt, 1);
    assert_eq!(second.attempt, 2);

    // Both land in the file: the second did not replace the first.
    let (_root, tree) = project("retry");
    requests::record(&tree, &first).unwrap();
    requests::record(&tree, &second).unwrap();
    let rows = requests::read(&tree).unwrap();
    assert_eq!(rows.len(), 2, "one row per attempt");
    assert_eq!(
        rows.iter().map(|r| r.attempt).collect::<Vec<_>>(),
        vec![1, 2]
    );
    // Same request both times -- only the attempt number and the timings differ.
    for field in [
        |r: &Request| r.step.clone(),
        |r: &Request| r.job.clone(),
        |r: &Request| r.model.clone(),
        |r: &Request| r.kind.clone(),
    ] {
        assert_eq!(field(&rows[0]), field(&rows[1]));
    }
    // The failed first try kept its error, so the file says why there were two.
    assert_eq!(rows[0].outcome, Outcome::Error("timeout".into()));
    assert_eq!(rows[1].outcome, Outcome::Ok);
}

#[test]
fn sec_09_llm_and_tools_10_every_request_timed_new_s4_a_cache_hit_has_no_time_on_the_wire() {
    let row = rt::cached_row(&base(Service::Llm, "describe", "chat"), 1);
    assert_eq!(row.outcome, Outcome::Cache);
    // Empty, not zero: an empty field says this never went out, "0.00" would say it went out and came
    // back instantly. That distinction is the whole reason cache rows are counted at all.
    assert_eq!(row.on_wire_s, None);
    assert_eq!(row.first_byte_s, None);
    assert_eq!(row.wait_s, None, "a hit is answered before the gate, so it never queued");
    assert_eq!(row.thinking_s, None);
    // What the cache knows about the reply stays: sizes and tokens describe the answer, not the trip.
    assert_eq!(row.received_bytes, Some(512));
    assert_eq!(row.tokens_out, Some(40));

    let written = requests::write_line(&row);
    let fields: Vec<&str> = written.trim_end().split('\t').collect();
    for name in ["wait_s", "first_byte_s", "on_wire_s", "thinking_s"] {
        assert_eq!(
            fields[column(name)],
            "",
            "{name} must be empty on a cache row, not 0.00"
        );
    }
    assert_eq!(fields[column("outcome")], "cache");

    // And such a row contributes no time to a tally while still being counted.
    let tallies = rt::tally(std::slice::from_ref(&row));
    assert_eq!(tallies.len(), 1);
    assert_eq!(tallies[0].count, 1);
    assert_eq!(tallies[0].cached, 1);
    assert_eq!(tallies[0].on_wire_s, 0.0);
    assert_eq!(tallies[0].wait_s, 0.0);
}

#[test]
fn sec_09_llm_and_tools_10_every_request_timed_new_s5_rows_are_appended_and_never_rewritten() {
    // §10: "Nothing in the file is ever rewritten; a line is written before the reply is used, so a
    // step that then fails still leaves its requests behind."
    let (_root, tree) = project("append");
    let path = tree.requests_tsv();
    let mut timer = rt::Timer::sent(400_000);
    timer.finished(401_000);

    let three: Vec<Request> = (0..3)
        .map(|i| {
            rt::finish_row(
                &base(Service::Llm, &format!("step{i}"), "chat"),
                &timer,
                Outcome::Ok,
                1,
            )
        })
        .collect();
    for row in &three {
        requests::record(&tree, row).unwrap();
    }
    let before = std::fs::read_to_string(&path).unwrap();
    let before_lines: Vec<&str> = before.lines().collect();
    assert_eq!(before_lines.len(), 4, "header + 3 rows");

    // A fourth arrives -- including one that failed, which is the case that matters: the row is there
    // even though nothing usable came of the call.
    let mut stalled = rt::Timer::sent(500_000);
    stalled.finished(800_000);
    requests::record(
        &tree,
        &rt::finish_row(&base(Service::Web, "youtube", "read"), &stalled, Outcome::Stalled, 3),
    )
    .unwrap();

    let after = std::fs::read_to_string(&path).unwrap();
    let after_lines: Vec<&str> = after.lines().collect();
    assert_eq!(after_lines.len(), 5, "header + 4 rows");
    // Byte-for-byte: the earlier rows were not touched by the later write.
    for i in 0..4 {
        assert_eq!(after_lines[i], before_lines[i], "line {i} changed");
    }
    assert!(after.starts_with(&before), "the old content is a prefix of the new");
    assert!(after_lines[4].contains("stalled"));
    // And reading the file gives all four back.
    assert_eq!(requests::read(&tree).unwrap().len(), 4);
}

/// The set §10's example sentence describes: 57 llm calls totalling 2h13m on the wire with 4m
/// waiting, 38 audio calls totalling 6m10s, 3 web calls totalling 12s.
///
/// Built by loop so the counts really are 57 / 38 / 3 and the totals really are 7980 / 370 / 12 s
/// (240 s of llm waiting spread over its rows), letting s7 compare against the spec's own line
/// exactly rather than against a shrunken stand-in.
fn spec_example_rows() -> Vec<Request> {
    let mut rows: Vec<Request> = Vec::new();
    // 57 llm rows: 140 s each on the wire = 7980 s = 2h13m; one of them carrying the 240 s wait.
    for i in 0..57 {
        let wait = if i == 0 { Some(240.0) } else { None };
        rows.push(Request {
            wait_s: wait,
            ..worked(Service::Llm, "cut", "chat", 140.0)
        });
    }
    // 38 audio rows totalling 370 s = 6m10s: ten at 30 s and twenty-eight at 2.5 s.
    for i in 0..38 {
        let wire = if i < 10 { 30.0 } else { 70.0 / 28.0 };
        rows.push(worked(Service::Audio, "transcribe", "asr", wire));
    }
    // 3 web rows: 4 s each = 12 s.
    for _ in 0..3 {
        rows.push(worked(Service::Web, "youtube", "read", 4.0));
    }
    rows
}

#[test]
fn sec_09_llm_and_tools_10_every_request_timed_new_s6_rows_are_tallied_per_service() {
    let tallies = rt::tally(&spec_example_rows());
    // Only the services used, in OUTSIDE_SERVICES' fixed order -- image was never touched.
    assert_eq!(
        tallies.iter().map(|t| t.service).collect::<Vec<_>>(),
        vec![Service::Llm, Service::Audio, Service::Web]
    );
    let near = |a: f64, b: f64| (a - b).abs() < 1e-9;
    assert_eq!(tallies[0].count, 57);
    assert!(near(tallies[0].on_wire_s, 7980.0), "2h13m: {}", tallies[0].on_wire_s);
    assert!(near(tallies[0].wait_s, 240.0), "4m: {}", tallies[0].wait_s);
    assert_eq!(tallies[1].count, 38);
    assert!(near(tallies[1].on_wire_s, 370.0), "6m10s: {}", tallies[1].on_wire_s);
    assert!(near(tallies[1].wait_s, 0.0));
    assert_eq!(tallies[2].count, 3);
    assert!(near(tallies[2].on_wire_s, 12.0));

    // Cache rows count but add no time.
    let mixed = vec![
        worked(Service::Image, "thumbnail", "image", 30.0),
        rt::cached_row(&base(Service::Image, "thumbnail", "image"), 1),
    ];
    let mixed_tallies = rt::tally(&mixed);
    assert_eq!(mixed_tallies.len(), 1);
    assert_eq!(mixed_tallies[0].count, 2, "the cached row is counted");
    assert_eq!(mixed_tallies[0].cached, 1);
    assert!(mixed_tallies[0].on_wire_s > 0.0, "only the real row adds time");

    // No rows at all: no entries, not four zero ones.
    assert!(rt::tally(&[]).is_empty());
}

#[test]
fn sec_09_llm_and_tools_10_every_request_timed_new_s7_the_run_ends_with_one_line_per_service() {
    // §10's own sentence, quoted exactly.
    let expected = ">>> requests: llm 57 in 2h 13m on the wire, 4m waiting for a slot; \
                   audio 38 in 6m 10s; web 3 in 12s";
    let line = rt::summary_line(&rt::tally(&spec_example_rows()));
    assert_eq!(line, expected, "\ngot:      {line}\nexpected: {expected}");

    // Zero wait gets no clause: printing it everywhere buries the one service that waited.
    let no_wait = rt::tally(&vec![Request {
        on_wire_s: Some(12.0),
        ..base(Service::Web, "youtube", "read")
    }]);
    let said = rt::summary_line(&no_wait);
    assert_eq!(said, ">>> requests: web 1 in 12s on the wire");
    assert!(!said.contains("waiting"), "{said}");

    // Duration spellings: exact minutes match llm_liveness::duration_spelling's "Xm0s", sub-minute
    // is bare seconds, hours drop the minutes when they are zero.
    let spelled = |wire: f64| {
        let t = rt::tally(&vec![Request {
            on_wire_s: Some(wire),
            ..base(Service::Audio, "align", "align")
        }]);
        rt::summary_line(&t)
    };
    assert!(spelled(360.0).contains("in 6m on the wire"), "{}", spelled(360.0));
    assert!(spelled(12.0).ends_with("in 12s on the wire"), "{}", spelled(12.0));
    assert!(spelled(7200.0).contains("2h on the wire"), "{}", spelled(7200.0));
    assert!(spelled(7260.0).contains("2h 1m"), "{}", spelled(7260.0));
    assert!(spelled(0.0).contains("0s on the wire"), "{}", spelled(0.0));

    // Nothing sent outside: reads as nothing, not as an empty sentence after the prefix.
    assert_eq!(rt::summary_line(&[]), ">>> requests: none");
}

#[test]
fn sec_09_llm_and_tools_10_every_request_timed_new_s8_the_wait_share_is_what_slots_are_chosen_by() {
    // P.machine.slots -- the count these numbers inform -- is held by llm_liveness and is NOT
    // re-declared here; this module states the ratio and the comparison, never the threshold.
    let heavy = rt::ServiceTally {
        service: Service::Llm,
        count: 57,
        on_wire_s: 7980.0,
        wait_s: 240.0,
        cached: 0,
    };
    let share = rt::wait_share(&heavy).expect("there was time spent");
    assert!((share - 240.0 / 8220.0).abs() < 1e-9, "{share}");
    assert!(share < 0.03 && share > 0.029, "~0.029: {share}");

    // Nothing spent either way: no judgement available, which is not the same as "no waiting".
    let idle = rt::ServiceTally {
        service: Service::Web,
        count: 1,
        on_wire_s: 0.0,
        wait_s: 0.0,
        cached: 1,
    };
    assert_eq!(rt::wait_share(&idle), None);
    // All waiting, no work: the share is the whole of it.
    let queued = rt::ServiceTally {
        service: Service::Audio,
        count: 2,
        on_wire_s: 0.0,
        wait_s: 60.0,
        cached: 0,
    };
    assert!((rt::wait_share(&queued).expect("waited") - 1.0).abs() < 1e-9);

    // The decision itself, with the threshold supplied by the caller.
    assert!(rt::wants_more_slots(0.5, 0.2));
    assert!(!rt::wants_more_slots(0.1, 0.2));
    // Exactly at the threshold is advice, not a demand.
    assert!(!rt::wants_more_slots(0.2, 0.2));

    // Only the four outside services exist, so subprocess work (ffmpeg, ffprobe -- logged as the
    // commands they ran, per §10) has no home in this file.
    assert_eq!(
        rt::OUTSIDE_SERVICES,
        [Service::Llm, Service::Audio, Service::Image, Service::Web]
    );
    assert_eq!(rt::OUTSIDE_SERVICES.len(), 4);
    // The header's service column accepts exactly these four words and nothing naming a subprocess.
    for word in ["llm", "audio", "image", "web"] {
        assert!(requests::HEADER.contains("service"));
        assert!(rt::OUTSIDE_SERVICES.iter().any(|s| format!("{s:?}").to_lowercase().contains(word)));
    }
    for absent in ["ffmpeg", "ffprobe", "local", "subprocess"] {
        assert!(
            !rt::OUTSIDE_SERVICES.iter().any(|s| format!("{s:?}").to_lowercase() == absent),
            "{absent} must not be a timed service"
        );
    }
}
