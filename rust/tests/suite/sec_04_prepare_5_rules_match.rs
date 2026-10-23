//! §04-prepare.md §5 bullet 3, its second half: *"'prefer the later saying' is the direction of the
//! match, not a request to the model"*.
//!
//! The clause has two halves and they live in different places. The DIRECTION is arithmetic — when the same
//! words were said twice around a join and the answer keeps one copy, the app charges the removal to BEFORE
//! and reads the kept copy as AFTER (`joins::later_saying_wins`, the tie-break in `split_answer`, which
//! `answer_join` uses on every real ▶ answer). The NOT-A-REQUEST half is about the prompt: nothing the app
//! sends may hand that choice to the model, because a model asked which copy it prefers answers differently
//! every run, and the cut would move between runs of the same project.
//!
//! So one test drives the direction through `answer_join` with an answer whose repeated middle could be
//! attributed either way, and one reads both texts the app actually sends — the shipped file and this flow's
//! own request — for the absence of a question. No window, no socket.

use naivepost::joins;
use naivepost::prompts;
use naivepost::tools::textedit::{self, Side};

/// Phrases that would put the choice in the model's hands. None of them appears in any text the app sends:
/// the rule is stated as the app's own doing ("BEFORE gives way first", "what was said last is what the
/// speaker meant to keep"), which is the spec's wording and states a decision rather than asking for one.
const ASKS_THE_MODEL: [&str; 4] = [
    "prefer the later",
    "which copy",
    "you choose which",
    "tell me which",
];

fn words(text: &str) -> Vec<String> {
    text.split(' ').map(str::to_string).collect()
}

/// The direction, through the door a real answer arrives at. BEFORE ends "...i say it" and AFTER starts
/// "say it again": two words were said twice and the answer holds one copy of them, so every reading must
/// leave exactly those two out — which is the tie §5 settles. The LATER saying wins: the kept copy belongs to
/// BEFORE's tail (the copy the speaker went on with), so the removal falls on AFTER's head instead of
/// reaching back into BEFORE.
#[test]
fn sec_04_prepare_5_rules_b3_the_direction_of_the_match_keeps_the_later_saying() {
    let before = words("so then i say it");
    let after = words("say it again but longer");
    // The answer keeps BEFORE whole and one copy of the doubled pair: `so then i say it` stands, and AFTER's
    // head "say it" is what the join left out. That is the tie §5 settles — the kept copy belongs to the
    // LATER saying, so the removal falls on AFTER's head rather than reaching back into BEFORE.
    let reply = r#"{"joined":"so then i say it again but longer"}"#;

    let refs_before: Vec<&str> = before.iter().map(String::as_str).collect();
    let refs_after: Vec<&str> = after.iter().map(String::as_str).collect();
    let mut join = textedit::Join::new(&refs_before, &refs_after);
    let outcome = joins::answer_join(&mut join, reply, &before, &after);
    let applied_before = match outcome {
        joins::Outcome::Applied { before, after } => {
            assert_eq!(
                before, 0,
                "BEFORE stands whole — the earlier saying was not the one dropped: {outcome:?}"
            );
            assert_eq!(
                after, 2,
                "the doubled pair came off AFTER's head: {outcome:?}"
            );
            after
        }
        other => panic!("a repaired join is an Applied, not {other:?}"),
    };
    assert_eq!(
        applied_before, 2,
        "two words went, and they were AFTER's head"
    );
    // Which side the tool was told about is the same fact from its side: `answer_join` hands the counts to
    // `tool:textedit`, so a reading that charged BEFORE would show up as BEFORE's tail taken instead.
    assert!(
        join.words_taken(Side::Before).is_empty(),
        "BEFORE gave up nothing, which is the whole direction of the match"
    );
    assert_eq!(
        join.dropped_count(Side::After),
        2,
        "the doubled pair came off AFTER, nearest the seam"
    );
    assert_eq!(
        join.words_taken(Side::After),
        vec!["say".to_string(), "it".to_string()],
        "and they are exactly the repeated words, taken from AFTER's head"
    );
}

/// The NOT-A-REQUEST half, on both texts the model can ever see: the shipped prompt file and this flow's own
/// request. Neither may ask the model which copy to keep.
#[test]
fn sec_04_prepare_5_rules_b3_the_direction_is_not_asked_of_the_model() {
    let shipped = prompts::shipped_text("textedit").expect("textedit ships wording");
    for phrase in ASKS_THE_MODEL {
        assert!(
            !shipped.to_lowercase().contains(phrase),
            "the shipped textedit prompt asks the model ({phrase:?}); §5 says the direction is the app's"
        );
    }
    // What IS there is the spec's own statement of the rule, as the app's doing. Pinned so a rewrite that
    // quietly turns it into a question gets caught by the pair.
    assert!(
        shipped.contains("BEFORE gives way first"),
        "the shipped wording states the direction itself"
    );
    assert!(
        shipped.contains("what was said last is what the speaker meant to keep"),
        "and names why: the later saying is the one meant"
    );

    // The per-join request the flow builds: the join's number, the two sides labelled, the answer shape —
    // and no request to choose between copies.
    let request = joins::request(
        "",
        3,
        7,
        &words("the take trailed off"),
        &words("and the next begins"),
    );
    assert!(request.contains("JOIN 3 of 7"), "{request}");
    assert!(
        request.contains("BEFORE (the end of the take that was interrupted)"),
        "{request}"
    );
    assert!(
        request.contains("AFTER (the beginning of the take that follows)"),
        "{request}"
    );
    assert!(request.contains("{\"joined\":\"...\"}"), "{request}");
    for phrase in ASKS_THE_MODEL {
        assert!(
            !request.to_lowercase().contains(phrase),
            "the join request asks the model ({phrase:?}): {request}"
        );
    }
    // Asking whether thinking is on is a different question and stays the flow's own (§F1.10).
    assert!(
        joins::thinking_on(),
        "P.machine.textEditThinking: this job thinks"
    );
}
