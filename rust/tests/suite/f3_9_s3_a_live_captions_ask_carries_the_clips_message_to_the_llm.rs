//! F3.9 S2/S3 over a socket: the ask that goes out is the spec's own message, and it carries the
//! tools §3.7 names for this job. One scenario per binary (`XDG_CONFIG_HOME`/cwd are process-wide).

#![allow(dead_code)]

#[path = "captions_wire_harness.rs"]
mod harness;

use harness::*;
use naivepost::captions_ask;

#[test]
fn f3_9_s3_a_live_captions_ask_carries_the_clips_message_to_the_llm() {
    let fixture = fixture("msg");
    let (clips, words) = two_clips_with_words();
    let llm = FakeLlm::start(vec![caption_json(&[(1, &[(0.4, 4.4, "The network is live.")])])]);
    let addr = point_at(&llm.url());

    let asked = captions_ask::ask(&fixture.tree, "Caption every spoken line.", &clips, &words, 1, &|| false);
    assert!(!asked.failed, "the ask went out and was answered: {:?}", asked.reason);
    assert_eq!(asked.calls.len(), 1, "one caption came back: {:?}", asked.calls);
    assert_eq!(asked.calls[0].clip, 1, "for clip 1");

    let seen = llm.next_seen();
    assert_eq!(seen.method, "POST", "a chat call is a POST");
    assert_eq!(seen.path, "/v1/chat/completions", "the LLM's chat path");
    // The message layout §F3.9 spells: header, one CLIP line with its length, offsets inside the clip.
    assert!(
        seen.body.contains("THE CLIPS, AND WHAT WAS SAID OVER EACH:"),
        "the clips header rode out to {addr}"
    );
    assert!(
        seen.body.contains("CLIP 1: 14.2 s long"),
        "clip 1 went with its own length: {}",
        &seen.body[..seen.body.len().min(400)]
    );
    assert!(
        seen.body.contains("[+0.4s] the network"),
        "the word line is an offset inside the clip, never a session second"
    );
    assert!(
        !seen.body.contains("[+14.6s]") && !seen.body.contains("[+16.2s]"),
        "no session seconds leaked into the offsets"
    );
    // The shipped captions prompt rode in the system message.
    assert!(
        seen.body.contains("You put words on screen over a cut that has already been chosen"),
        "the shipped captions wording rode out"
    );
    assert!(
        seen.body.contains("the swearing kept"),
        "the subtitler cleaning rule rode out with it"
    );
    // And the two tools this item names are offered to the model.
    assert!(
        seen.body.contains("add_caption"),
        "add_caption was offered: the body names the tool it wants back"
    );
    assert!(
        seen.body.contains("\"finish\""),
        "finish was offered so the reply can be closed"
    );
    // The timed row landed: every ask writes one, whatever the answer was.
    let rows = read(&fixture.tree.requests_tsv());
    assert!(
        rows.contains("captions"),
        "requests.tsv carries this job's row: {rows}"
    );
    assert!(
        rows.contains("llm") || rows.contains("LLM"),
        "and names the service it dialled: {rows}"
    );
}
