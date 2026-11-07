//! §04-prepare.md §5 bullet 3, its second half: *"'prefer the later saying' is the direction of the
//! match, not a request to the model"*.
//!
//! The clause has two halves. The DIRECTION is the app's rule, stated in what it sends ("BEFORE gives way
//! first ... what was said last is what the speaker meant to keep"), and an answer names its side outright:
//! the quote it gives is `drop_before` or `drop_after` (§3.5), so the app never guesses which copy of a doubled
//! saying was meant (the first rewrite matched a joined text backwards and had to). The NOT-A-REQUEST half is
//! about the prompt: nothing the app sends may hand the choice to the model as a question, because a model
//! asked which copy it prefers answers differently every run.
//!
//! So one test drives an answer whose doubled pair could go either way and checks the side it names is the
//! side that loses the words, and one reads both texts the app sends for the absence of a question.

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
/// "say it again": two words were said twice. The answer names the side: quoting AFTER's head takes it from
/// AFTER and BEFORE stands whole; the same words quoted as BEFORE's tail take them from BEFORE instead.
#[test]
fn sec_04_prepare_5_rules_b3_the_direction_of_the_match_keeps_the_later_saying() {
    let before = words("so then i say it");
    let after = words("say it again but longer");
    let refs_before: Vec<&str> = before.iter().map(String::as_str).collect();
    let refs_after: Vec<&str> = after.iter().map(String::as_str).collect();

    let mut join = textedit::Join::new(&refs_before, &refs_after);
    let reply = r#"{"drop_before":"","drop_after":"say it"}"#;
    let outcome = joins::answer_join(&mut join, reply, &before, &after);
    assert_eq!(
        outcome,
        joins::Outcome::Applied {
            before: 0,
            after: 2
        },
        "the doubled pair comes off the side the answer named"
    );
    assert!(
        join.words_taken(Side::Before).is_empty(),
        "BEFORE gave up nothing"
    );
    assert_eq!(
        join.words_taken(Side::After),
        vec!["say".to_string(), "it".to_string()],
        "and they are exactly the repeated words, taken from AFTER's head"
    );

    let mut join = textedit::Join::new(&refs_before, &refs_after);
    let reply = r#"{"drop_before":"say it","drop_after":""}"#;
    assert_eq!(
        joins::answer_join(&mut join, reply, &before, &after),
        joins::Outcome::Applied {
            before: 2,
            after: 0
        },
        "the same words quoted as BEFORE's tail come off BEFORE"
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
        "the take trailed off [CUT] and the next begins",
        &words("the take trailed off"),
        &words("and the next begins"),
        None,
    );
    assert!(request.contains("JOIN 3 of 7"), "{request}");
    assert!(
        request.contains("BEFORE (the end of the take that was interrupted)"),
        "{request}"
    );
    assert!(
        request.contains("AFTER (the whole take that follows, up to where the next one starts)"),
        "{request}"
    );
    assert!(
        request.contains("{\"drop_before\":\"...\",\"drop_after\":\"...\"}"),
        "{request}"
    );
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

/// A quote that starts (BEFORE) or ends (AFTER) inside a word-list entry holding several words takes that entry
/// whole: it is timed as one and cannot be cut in two. 09-news join 13: BEFORE ended "with that. The last article
/// is the following here." with "that. The" one entry; the answer quoted from "The" and was refused twice, so the
/// sentence stayed in twice. A quote that matches no ending at all is still refused.
#[test]
fn sec_04_prepare_5_rules_b3_a_quote_inside_an_entry_of_several_words_takes_it_whole() {
    let before: Vec<String> = [
        "we",
        "land",
        "with",
        "that. The",
        "last",
        "article",
        "is",
        "here.",
    ]
    .iter()
    .map(|w| w.to_string())
    .collect();
    let after = words("the last article is here this author found");
    let refs_before: Vec<&str> = before.iter().map(String::as_str).collect();
    let refs_after: Vec<&str> = after.iter().map(String::as_str).collect();

    let mut join = textedit::Join::new(&refs_before, &refs_after);
    let reply = r#"{"drop_before":"The last article is here.","drop_after":""}"#;
    assert_eq!(
        joins::answer_join(&mut join, reply, &before, &after),
        joins::Outcome::Applied {
            before: 5,
            after: 0
        },
        "the entry the quote starts inside goes with it"
    );

    let shared: Vec<String> = ["this", "author found", "it"]
        .iter()
        .map(|w| w.to_string())
        .collect();
    let refs_shared: Vec<&str> = shared.iter().map(String::as_str).collect();
    let mut join = textedit::Join::new(&refs_before, &refs_shared);
    let reply = r#"{"drop_before":"","drop_after":"this author"}"#;
    assert_eq!(
        joins::answer_join(&mut join, reply, &before, &shared),
        joins::Outcome::Applied {
            before: 0,
            after: 2
        },
        "and on AFTER, the entry the quote ends inside"
    );

    let mut join = textedit::Join::new(&refs_before, &refs_after);
    let reply = r#"{"drop_before":"The first article is here.","drop_after":""}"#;
    assert!(
        matches!(
            joins::answer_join(&mut join, reply, &before, &after),
            joins::Outcome::Refused(_)
        ),
        "a quote that is not how BEFORE ends is still refused"
    );
}
