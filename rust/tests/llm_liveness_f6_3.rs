//! F6.3 — `spec/09-llm-and-tools.md` §4, liveness and the gate, checked against `naivepost::llm_liveness`.
//!
//! §4 has no numbered steps, so its bullets are the steps here: s1 the watch (the stall, the heartbeat's three
//! shapes, the tail, the unstreamed ceiling), s2 slots, s3 how many at once, s4 what the prototype kept. No clock
//! and no threads: every moment is a `now` in whole seconds handed to the module, so 4m45s and 5m00s are both
//! just numbers and the boundary between them is an assertion rather than a wait.
//!
//! # Ids used
//!
//! `P.eng.llmStallMinutes` (5), `P.eng.llmWholeMinutes` (10), `P.machine.slots` (default 1, one count per model)
//! and `P.eng.llmTailChars` (90 shown / 360 kept). §4 cites the first three by name; the tail bound comes from
//! §10's row.

use naivepost::llm_liveness as live;
use naivepost::params;
use naivepost::produce_render as render;
use naivepost::settings::Slots;

const ITEM: &str = "F6.3";

/// A streamed call that started at t=0 and has said nothing.
fn silent() -> live::Watch {
    live::Watch::streamed("narrate", 0)
}

/// Every test names the item it covers; §4's bullets stand in for S-numbers, so the suffix says which one.
fn item(part: &str) -> String {
    format!("{ITEM} {part}")
}

// ---- s1: the stall ------------------------------------------------------------------

#[test]
fn f6_3_s1_a_streamed_call_is_given_up_for_silence_not_for_length() {
    assert!(item("silence, not length").starts_with(ITEM), "{ITEM}");
    // §4's whole point: a streamed call may run as long as it takes. Twenty minutes of tokens is not a failure;
    // five minutes of nothing is. The bound is P.eng.llmStallMinutes (5).
    assert_eq!(live::STALL_MINUTES, 5);

    // Alive at 4m45s — the guard looks four times per heartbeat, so this poll happens and says nothing alarming.
    let mut watch = silent();
    let before = watch.poll(285);
    assert!(!matches!(before, live::Poll::GaveUp { .. }), "{before:?} at 4m45s");
    // The next poll is 15 s later and it is the one that ends the call: POLL_SECONDS is what catches a stall on
    // the bound rather than up to a minute after it.
    assert_eq!(live::POLL_SECONDS, 15);
    let given_up = watch.poll(300);
    let live::Poll::GaveUp { log, blame } = given_up else { panic!("5m0s of silence must end it: {given_up:?}") };
    // Byte-exact §4 line — em dash before "giving up".
    assert_eq!(log, ">>> narrate: nothing for 5m0s \u{2014} giving up");
    // Nothing ever arrived, so the caller says so in §4's first shape.
    assert_eq!(blame, "nothing arrived in 5m0s");

    // A call that answered and then went quiet is reported differently: how long it had been going comes first,
    // because that is what tells a reader to blame the prompt's size rather than the connection. ASCII `--`.
    let mut spoke = live::Watch::streamed("publish", 0);
    spoke.answered_at(120, "{\"title\": \"x\"}", false);
    let later = spoke.poll(420); // 7 minutes in, 5 of them silent
    let live::Poll::GaveUp { log, blame } = later else { panic!("expected a give-up: {later:?}") };
    assert_eq!(log, ">>> publish: nothing for 5m0s \u{2014} giving up");
    assert_eq!(blame, "stopped answering after 7m0s -- nothing more for 5m0s");

    // Length alone never ends a streamed call: twenty minutes of steady tokens is healthy. `answered_at` is what a
    // reader loop does — a byte arrived *and* it meant something; the two facts stay separate because a keep-alive
    // proves life and writes nothing.
    let mut busy = live::Watch::streamed("suggest", 0);
    for minute in 1..=20 {
        busy.answered_at(minute * 60, "{\"segments\": []}", false);
        assert!(!matches!(busy.poll(minute * 60), live::Poll::GaveUp { .. }), "minute {minute}");
    }
}

#[test]
fn f6_3_s1_any_byte_off_the_wire_counts_keep_alives_included() {
    assert!(item("any byte counts").starts_with(ITEM), "{ITEM}");
    // §4: "any byte off the wire counts, including keep-alives the event parser never sees". A ping is proof the
    // server is there, which is the only thing the rule asks — so a call that produces no text for ten minutes but
    // keeps talking is alive, and `wrote` is not what resets the clock.
    let mut watch = live::Watch::streamed("describe", 0);
    for at in (45..=600).step_by(45) {
        watch.byte_arrived(at);
        assert!(!matches!(watch.poll(at), live::Poll::GaveUp { .. }), "keep-alive at {at}s");
    }
    // Ten minutes in with nothing but pings: still going. Any byte refreshes the minute's clock too — a keep-alive
    // is an event worth reporting on, and §4's line for it is "nothing yet", since no text has arrived.
    let line = watch.poll(600);
    assert_eq!(line, live::Poll::Heartbeat(">>> describe: nothing yet, 10m0s in".to_string()));
    // One poll later in the same minute says nothing again: a ping is not a heartbeat every fifteen seconds.
    assert_eq!(watch.poll(645), live::Poll::Idle);

    // Stop the pings and the same call dies five minutes later.
    let given_up = watch.poll(900);
    assert!(matches!(given_up, live::Poll::GaveUp { .. }), "{given_up:?}");
}

// ---- s1: the heartbeat --------------------------------------------------------------

#[test]
fn f6_3_s1_the_heartbeat_is_once_a_minute_in_three_shapes() {
    assert!(item("three shapes").starts_with(ITEM), "{ITEM}");
    // Two looks inside one minute say something once: the guard polls four times a heartbeat and a log that
    // repeated every fifteen seconds would be a scroll, not a signal.
    let mut watch = silent();
    assert_eq!(watch.poll(15), live::Poll::Idle);
    assert_eq!(watch.poll(30), live::Poll::Idle);
    let first = watch.poll(60);
    // Shape one: nothing at all yet.
    assert_eq!(first, live::Poll::Heartbeat(">>> narrate: nothing yet, 1m0s in".to_string()));
    assert_eq!(watch.poll(75), live::Poll::Idle, "the same minute says it once");

    // Shape two: bytes arrived and there is nothing worth quoting — reasoning alone is the model talking to itself,
    // so it only reaches the line when there is no answer at all (see the tail test). Sizes are spelled as the
    // prototype spells them.
    let mut sizes = live::Watch::streamed("suggest", 0);
    sizes.answered_at(60, "ok", true);
    // Reasoning alone is still counted; it is only quoted when there is no answer (see the tail test).
    assert_eq!(
        sizes.poll(120),
        live::Poll::Heartbeat(">>> suggest: 2 B thinking, 0 B reply, 2m0s in \u{2014} \"ok\"".to_string())
    );
    // A long answer carries its tail too, on the same line as the sizes.
    let mut big = live::Watch::streamed("suggest", 0);
    big.answered_at(60, &"t".repeat(1536), true);
    let line = big.poll(120);
    assert!(matches!(&line, live::Poll::Heartbeat(t) if t.starts_with(">>> suggest: 1.5 kB thinking, 0 B reply, 2m0s in")), "{line:?}");
    assert!(format!("{line:?}").contains("\u{2026}ttt"), "the tail is on the line: {line:?}");

    // Shape three: an unstreamed call has no bytes to show at all, so both sizes stay off the line entirely.
    let mut quiet = live::Watch::unstreamed("fix", 0);
    assert_eq!(quiet.poll(60), live::Poll::Heartbeat(">>> fix: nothing yet, 1m0s in".to_string()));
    for (bytes, want) in [(0usize, "0 B"), (900, "900 B"), (1023, "1023 B"), (1024, "1.0 kB"), (1536, "1.5 kB")] {
        assert_eq!(live::size_spelling(bytes), want, "{bytes} bytes");
    }

    // Shape three: the same plus the tail, em dash before the Go-quoted text.
    let mut tail = live::Watch::streamed("cut", 0);
    tail.answered_at(30, "{\"segments\": [{\"start\": 12.5,", false);
    // The first line comes a minute after the guard was armed, and says how long the call has been running.
    let line = tail.poll(90);
    let live::Poll::Heartbeat(line) = line else { panic!("a heartbeat: {line:?}") };
    assert!(line.starts_with(">>> cut: 0 B thinking, 29 B reply, 1m30s in \u{2014} "), "{line}");
    assert!(line.ends_with("\"{\\\"segments\\\": [{\\\"start\\\": 12.5,\""), "{line}");

    // Durations as §4 prints them (and deliberately not as §3's worded waits).
    for (seconds, want) in [(0u64, "0s"), (59, "59s"), (60, "1m0s"), (90, "1m30s"), (300, "5m0s"), (600, "10m0s")] {
        assert_eq!(live::duration_spelling(seconds), want, "{seconds}s");
    }
}

#[test]
fn f6_3_s1_the_tail_collapses_then_cuts_and_never_splits_a_letter() {
    assert!(item("the tail").starts_with(ITEM), "{ITEM}");
    // Whitespace is collapsed before the line is written, so the tail the reader sees holds no runs of spaces —
    // and the cut keeps the LAST bytes, which is where a stuck model repeats itself.
    let padded = format!("{}{}{}", "z".repeat(300), " ".repeat(200), "b".repeat(40));
    let tail = live::tail_of(&padded);
    assert!(!tail.contains("  "), "runs collapsed: {tail:?}");
    assert!(tail.starts_with('\u{2026}'), "{tail:?}");
    // The LAST bytes are what is kept — that is where a stuck model repeats itself.
    assert!(tail.ends_with(&"b".repeat(40)), "{tail:?}");
    let body = &tail['\u{2026}'.len_utf8()..];
    assert_eq!(body.len(), live::TAIL_BYTES, "P.eng.llmTailChars is 90 shown");
    assert_eq!(live::TAIL_BYTES, 90);
    assert_eq!(live::TAIL_KEPT_BYTES, 360, "and 360 kept");

    // A cut landing inside a multi-byte character steps FORWARD to a boundary: half a letter prints as a
    // replacement character exactly where the interesting text is.
    let multibyte = "\u{00e9}".repeat(200); // 2 bytes each, so byte 90 from the end is mid-letter.
    let tail = live::tail_of(&multibyte);
    let body = tail.strip_prefix('\u{2026}').expect("a clipped tail");
    assert!(body.len() <= live::TAIL_BYTES, "{} bytes", body.len());
    assert_eq!(body.chars().count() * 2, body.len(), "every letter whole: {} bytes", body.len());

    // Short text is not clipped, so it carries no ellipsis — and a tail of nothing is no tail at all.
    assert_eq!(live::tail_of("the answer"), "the answer");
    assert_eq!(live::tail_of("   \n\t  "), "");

    // The answer is what gets quoted; the reasoning only when there is no answer yet.
    let mut watch = live::Watch::streamed("suggest", 0);
    watch.answered_at(30, "thinking about the cut", true);
    let reasoning_only = watch.poll(60); // the guard's first minute
    assert!(format!("{reasoning_only:?}").contains("thinking about the cut"), "{reasoning_only:?}");
    watch.answered_at(120, "the answer is here", false);
    let with_answer = watch.poll(120); // the next line is due a minute after that one, whatever bytes arrived
    let text = format!("{with_answer:?}");
    assert!(text.contains("the answer is here"), "{text}");
    assert!(!text.contains("thinking about the cut"), "the answer wins: {text}");

    // Go-quoting: the escapes that would break a log line; anything else is left as it stands, because the tail
    // exists to be read off a terminal, not to be re-parsed.
    let quoted = live::go_quote("a\"b\u{5c}c\nd");
    assert_eq!(quoted.as_bytes(), b"\"a\\\"b\\\\c\\nd\"");
}

#[test]
fn f6_3_s1_an_unstreamed_call_is_never_given_up_for_silence() {
    assert!(item("the ceiling").starts_with(ITEM), "{ITEM}");
    // §4: the guard also ticks for an unstreamed call, where X and Y stay 0 — it logs "nothing yet" every minute
    // and is never given up for silence. Only the ceiling ends it.
    let mut watch = live::Watch::unstreamed("publish", 0);
    for minute in 1..=9 {
        let line = watch.poll(minute * 60);
        assert_eq!(
            line,
            live::Poll::Heartbeat(format!(">>> publish: nothing yet, {minute}m0s in")),
            "minute {minute}"
        );
    }
    // Nine minutes of silence would have killed a streamed call four minutes ago.
    let over = watch.poll(600);
    let live::Poll::GaveUp { log, blame } = over else { panic!("the 10-minute ceiling ends it: {over:?}") };
    assert_eq!(log, ">>> publish: nothing for 10m0s \u{2014} giving up");
    // Not the stall's sentence: this call was never judged by silence.
    assert_eq!(blame, "no answer in 10m0s");
    assert_eq!(live::WHOLE_MINUTES, 10);

    // §4's note is why both rules exist: every call is streamed now, so the stall rule holds for all. The
    // prototype left two thinking jobs unstreamed — the upload text and textedit — where they sat under this
    // ceiling with no stall rule at all, which is the bug this item closes.
    assert!(live::STALL_MINUTES < live::WHOLE_MINUTES, "the stall must bite first when it can");
}

// ---- s2: slots ----------------------------------------------------------------------

#[test]
fn f6_3_s2_a_busy_model_queues_the_next_step_and_says_so_once() {
    assert!(item("slots").starts_with(ITEM), "{ITEM}");
    let mut gate = live::Gate::new("llm", 1);
    // Free: taken, and nothing to log — a run that never waits should not say anything about waiting.
    assert_eq!(gate.acquire("publish"), live::Acquisition::Taken);
    assert_eq!(gate.holders(), vec!["publish".to_string()]);

    // §4's line, byte-exact: ASCII `--`, the model, the count, the holders in brackets.
    let second = gate.acquire("narrate");
    assert_eq!(
        second,
        live::Acquisition::Waiting { log: ">>> narrate: waiting for llm -- all 1 slot(s) busy (publish)".to_string() }
    );
    // Said once. A caller polls this every few seconds, so repeating it would bury the log in one fact.
    assert_eq!(gate.acquire("narrate"), live::Acquisition::Waiting { log: String::new() });
    assert_eq!(gate.waiting(), vec!["narrate".to_string()]);

    // Release hands the slot to whoever waited longest — first come first served, without the waiter having to
    // ask again to be noticed.
    gate.release("publish");
    assert_eq!(gate.holders(), vec!["narrate".to_string()]);
    assert!(gate.waiting().is_empty());

    // Three arrive while it is busy and leave in the order they arrived.
    let mut queue = live::Gate::new("llm", 1);
    queue.acquire("publish");
    for step in ["narrate", "suggest", "transcript"] {
        assert!(matches!(queue.acquire(step), live::Acquisition::Waiting { .. }), "{step}");
    }
    assert_eq!(queue.waiting(), ["narrate", "suggest", "transcript"]);
    let mut left = Vec::new();
    for _ in 0..4 {
        let holder = queue.holders().first().cloned().expect("someone is always held until the end");
        if holder != "publish" {
            left.push(holder.clone());
        }
        queue.release(&holder);
    }
    assert_eq!(left, ["narrate", "suggest", "transcript"], "first come first served");

    // A second wait is news again — once the step has actually been served in between. The flag belongs to a step,
    // so re-asking while still queued stays quiet (§3: "say it once"); a *later* call that queues behind someone
    // else is a different wait and gets its own line.
    let mut twice = live::Gate::new("llm", 1);
    twice.acquire("publish");
    assert!(!matches!(twice.acquire("narrate"), live::Acquisition::Waiting { log } if log.is_empty()));
    assert!(matches!(twice.acquire("narrate"), live::Acquisition::Waiting { log } if log.is_empty()), "the second ask in the same queue says nothing");
    twice.release("publish"); // narrate was next in line and is now on the wire
    assert_eq!(twice.holders(), ["narrate".to_string()]);
    // narrate finishes its call and asks again later; by then describe holds the slot, so this is a new wait.
    twice.release("narrate");
    assert_eq!(twice.acquire("describe"), live::Acquisition::Taken);
    let again = twice.acquire("narrate");
    assert!(matches!(&again, live::Acquisition::Waiting { log } if !log.is_empty()), "{again:?}");
}

#[test]
fn f6_3_s2_every_model_has_its_own_count_and_the_default_is_one() {
    assert!(item("slots per model").starts_with(ITEM), "{ITEM}");
    // P.machine.slots: default 1, one count per model. All seven of §4's list — the LLM, each audio.cpp model and
    // the image model — start at one request at a time.
    let defaults = Slots::default();
    for (name, count) in [
        ("llm", defaults.llm),
        ("asr", defaults.asr),
        ("diar", defaults.diar),
        ("align", defaults.align),
        ("tts", defaults.tts),
        ("sep", defaults.sep),
        ("sd", defaults.sd),
    ] {
        assert_eq!(count, live::DEFAULT_SLOTS, "{name}");
        assert_eq!(count, 1, "P.machine.slots default: {name}");
    }

    // Two models on one server are two slot counts (§5): a full LLM gate says nothing about the ASR gate. The
    // Settings text says why that is not twice the hardware — "two models on one GPU share it however this is
    // set" — so sharing a machine must not merge the counts, and here it cannot: they are separate gates.
    let mut llm = live::Gate::new("llm", 2);
    llm.acquire("publish");
    llm.acquire("narrate");
    assert!(matches!(llm.acquire("suggest"), live::Acquisition::Waiting { .. }), "the LLM is full");
    let mut asr = live::Gate::new("asr", 2);
    assert_eq!(asr.acquire("transcribe"), live::Acquisition::Taken, "audio.cpp has its own slots");

    // §5's spin range, 1 to 16: Settings keeps what was typed, so a caller clamps when it builds a gate. A stale
    // conf of 0 would make a gate nothing can ever enter.
    assert_eq!(live::clamped_slots(0), live::SLOTS_MIN);
    assert_eq!(live::clamped_slots(4), 4);
    assert_eq!(live::clamped_slots(live::SLOTS_MAX), live::SLOTS_MAX);
    assert_eq!(live::clamped_slots(40), live::SLOTS_MAX);
    // A gate built from a wild count still admits someone.
    let mut zero = live::Gate::new("llm", 0);
    assert_eq!(zero.acquire("publish"), live::Acquisition::Taken);

    // Two boxes pointing at one machine: worth a warning, never a merged count.
    // One address, two boxes: worth the warning.
    assert!(live::shares_a_server("http://127.0.0.1:8731", "http://127.0.0.1:8731/v1"));
    assert!(!live::shares_a_server("llama:8080", "http://llama:8080/"), "a bare host reads as https, which is a different address");
    assert!(live::shares_a_server("https://llama:8080/", "http://llama:8080".replace("http", "https").as_str()));
    // Different host or port: not claimed as shared — and two ports on one machine are not either, because that
    // would need DNS to know, and a warning that fires on every install teaches nobody to read it.
    // Different hosts are not shared, and neither is one host on two ports — that is two servers by agreement.
    assert!(!live::shares_a_server("http://llama:8080", "http://audiocpp:8765"));
    assert!(!live::shares_a_server("http://127.0.0.1:8731", "http://127.0.0.1:8765"));
    // The same host and port is, whatever path each box adds afterwards: one server wearing two prefixes.
    assert!(live::shares_a_server("http://host:1234/sd", "http://host:1234/v1"));
    assert!(!live::shares_a_server("", "http://host:80/sd"), "an empty box is no server");
}

#[test]
fn f6_3_s2_the_wait_is_cancellable_and_is_not_the_watch_s_business() {
    assert!(item("queue is not a stall").starts_with(ITEM), "{ITEM}");
    // ⏹ during a queue.
    let mut gate = live::Gate::new("llm", 1);
    gate.acquire("publish");
    gate.acquire("narrate");
    assert!(gate.cancel_waiting("narrate"), "there was a wait to cancel");
    assert!(!gate.cancel_waiting("narrate"), "and there is not one now");
    assert!(gate.waiting().is_empty());
    // With nobody left waiting the gate is simply busy: a new asker joins the queue rather than being turned away,
    // and gets the slot when the holder gives it up.
    assert!(matches!(gate.acquire("suggest"), live::Acquisition::Waiting { .. }));
    gate.release("publish");
    assert_eq!(gate.holders(), vec!["suggest".to_string()]);

    // §4: "the watch starts once the request is on the wire", so three minutes in a queue are never three minutes
    // of "nothing yet" — and certainly not a stall. The caller builds the Watch at the moment it is Taken.
    let mut gate = live::Gate::new("llm", 1);
    gate.acquire("publish");
    let queued_at = 0u64;
    assert!(matches!(gate.acquire("narrate"), live::Acquisition::Waiting { .. }));
    // ... three minutes pass, with the step polling and waiting ...
    // The first ask named the queue; these do not repeat it.
    for at in [60, 120, 180] {
        assert!(matches!(gate.acquire("narrate"), live::Acquisition::Waiting { log } if log.is_empty()), "at {at}");
    }
    // The hand-off happens as the slot comes free, and the queued step learns of it on its next ask: being told
    // "still waiting" after its turn arrived would leave a slot nobody used. It is admitted silently — §3 speaks
    // about queueing, not about starting.
    gate.release("publish");
    assert_eq!(gate.acquire("narrate"), live::Acquisition::Taken);
    let taken_at = 180;
    assert!(taken_at - queued_at >= 3 * 60, "the queue lasted three minutes");

    // The watch starts here. Its first heartbeat is a minute after the wire, not after the ask: a poll at the
    // moment it was taken, and one two minutes later, say nothing about the queue.
    let mut watch = live::Watch::streamed("narrate", taken_at);
    assert_eq!(watch.poll(taken_at), live::Poll::Idle);
    assert_eq!(watch.poll(taken_at + 59), live::Poll::Idle);
    assert_eq!(watch.poll(taken_at + 60), live::Poll::Heartbeat(">>> narrate: nothing yet, 1m0s in".to_string()));
    // And the stall clock is measured from the wire too: five minutes of silence after being taken ends it, and
    // the three queued minutes are not part of it.
    assert!(!matches!(watch.poll(taken_at + 240), live::Poll::GaveUp { .. }), "queueing time is not silence");
}

// ---- s3: how many at once -----------------------------------------------------------

#[test]
fn f6_3_s3_independent_requests_may_use_every_slot_dependent_ones_never() {
    assert!(item("in flight").starts_with(ITEM), "{ITEM}");
    // Slots are per model and shared by whoever asks, so a step may only use all of them when its own requests
    // do not depend on each other.
    assert_eq!(live::in_flight(true, 4), 4);
    assert_eq!(live::in_flight(false, 4), 1);
    assert_eq!(live::in_flight(true, 1), 1, "one slot is one at a time either way");
    // The clamp applies here too: a wild count does not mean an unbounded fan-out.
    assert_eq!(live::in_flight(true, 0), 1);
    assert_eq!(live::in_flight(true, 99), live::SLOTS_MAX as usize);

    // Independent (§4's list): describe chunks of different videos, fix blocks, the retake pool's runs, subtitle
    // translations, one TTS line per slot. Translations are the clearest — German does not wait for French.
    assert_eq!(live::in_flight(true, 3), 3, "subtitle translations: one per slot");

    // Dependent, whatever N is: the join pass (F1.10), whose every window leaves out what the joins before it
    // took — asking about window 3 before window 2 answered asks about the wrong footage.
    assert_eq!(live::in_flight(false, 8), 1, "the join pass stays in order at any slot count");
    // And a tool conversation's rounds: round 2 is built from what round 1's tools returned.
    assert_eq!(live::in_flight(false, 8), 1, "a tool conversation has no second round to send yet");
}

// ---- s4: what the prototype kept ----------------------------------------------------

#[test]
fn f6_3_s4_one_slot_reproduces_the_prototype_and_a_step_behind_itself_is_silent() {
    assert!(item("one at a time").starts_with(ITEM), "{ITEM}");
    // The prototype held the LLM to one request at a time in code; P.machine.slots defaults to 1, so the shipped
    // default is that behaviour with a knob on top of it.
    let mut gate = live::Gate::new("llm", live::DEFAULT_SLOTS);
    assert_eq!(gate.acquire("publish"), live::Acquisition::Taken);
    assert!(matches!(gate.acquire("narrate"), live::Acquisition::Waiting { .. }), "one at a time");

    // The prototype's other rule, kept: only when the holder was a *different* step does anything get said. A
    // step queued behind its own earlier call waits silently — naming the holder would name the asker, and
    // "publish is waiting for publish" says less than nothing.
    let mut same = live::Gate::new("llm", 1);
    assert_eq!(same.acquire("publish"), live::Acquisition::Taken);
    assert_eq!(same.acquire("publish"), live::Acquisition::Waiting { log: String::new() });
    // Two slots, both held by one step, and a third step still gets a line that names the holder once rather
    // than twice.
    let mut two = live::Gate::new("llm", 2);
    two.acquire("describe");
    two.acquire("fix");
    let waiting = two.acquire("narrate");
    let live::Acquisition::Waiting { log } = waiting else { panic!("three asks for two slots: {waiting:?}") };
    assert_eq!(log, ">>> narrate: waiting for llm -- all 2 slot(s) busy (describe, fix)");

    // Produce runs its translation after the encodes so the encoder never idles behind the gate — that ordering
    // is produce_render's own (§S7), reused rather than restated here. The gate has no opinion about it, which is
    // why nothing in this module mentions encode order.
    assert!(render::TRANSLATE_FRACTION > render::JOIN_FRACTION, "translation sits after the encodes");
    assert_eq!(render::translate_log(42, "de"), ">>> subtitles: translating 42 lines into de");
}

// ---- s5: the ids -------------------------------------------------------------------

#[test]
fn f6_3_s5_the_three_numbers_are_catalogued_and_have_one_home() {
    assert!(item("the ids").starts_with(ITEM), "{ITEM}");
    // §10's rows quoted: 5 / 10 / 1, and one `P.machine.slots` row covering all seven models.
    let stall = params::find("P.eng.llmStallMinutes").expect("catalogued");
    assert_eq!(stall.spelled, "5");
    assert_eq!(stall.from, "llm_liveness::STALL_MINUTES");
    assert_eq!(params::family("P.eng.llmStallMinutes"), params::Family::Eng);

    let whole = params::find("P.eng.llmWholeMinutes").expect("catalogued");
    assert_eq!(whole.spelled, "10");
    assert_eq!(whole.from, "llm_liveness::WHOLE_MINUTES");
    assert_eq!(params::family("P.eng.llmWholeMinutes"), params::Family::Eng);

    let slots = params::find("P.machine.slots").expect("catalogued");
    assert_eq!(slots.spelled, "1");
    assert_eq!(slots.from, "llm_liveness::DEFAULT_SLOTS");
    assert_eq!(params::family("P.machine.slots"), params::Family::Machine);

    // The module reads them by name: no second copy of either bound where a change would miss it.
    let source = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/llm_liveness.rs")).expect("the module");
    let body = |name: &str| -> String {
        // The free function, not the method of the same name where there are two.
        let at = source
            .find(&format!("pub fn {name}("))
            .unwrap_or_else(|| panic!("{name}'s body"));
        let rest = &source[at..];
        // A function's own closing brace sits at four spaces; anything nested closes deeper than that.
        let end = rest.find("\n    }").unwrap_or(rest.len());
        rest[..end].to_string()
    };
    let poll = body("poll");
    assert!(poll.contains("STALL_MINUTES * 60") && poll.contains("WHOLE_MINUTES * 60"), "{poll}");
    for literal in ["300", "600", "(5)", "(10)"] {
        assert!(!poll.contains(literal), "{literal} in poll is a second copy of a bound: {poll}");
    }
    let blame = body("blame");
    assert!(blame.contains("STALL_MINUTES * 60"), "{blame}");
    // The tail bound too: `tail_of` reads TAIL_BYTES, never 90.
    let tail = body("tail_of");
    assert!(tail.contains("TAIL_BYTES"), "{tail}");
    assert!(!tail.contains("90"), "a literal 90 in tail_of: {tail}");
}
