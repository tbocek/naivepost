//! §11-flow-index.md §5 — directive A: the four homes, and which one each id belongs to.

use naivepost::{
    decision_homes::{self, Home},
    params::{self, Family},
    project::{self, Origin},
    project_settings, prompts, settings,
};

const ITEM: &str = "§11-flow-index#5-where-each-kind-of-decision-lives-directive-a";

/// Every id this build catalogues, from all five sections plus the project tab's rows.
fn catalogued() -> Vec<&'static str> {
    let mut ids: Vec<&'static str> = params::prepare()
        .into_iter()
        .chain(params::cut())
        .chain(params::effects())
        .chain(params::narrate())
        .chain(params::produce())
        .map(|p| p.id)
        .collect();
    ids.extend(project_settings::listed().into_iter().map(|p| p.id));
    ids
}

// ---- S1: the four homes -------------------------------------------------------------------

#[test]
fn sec_11_flow_index_5_where_each_kind_of_decision_lives_directive_a_s1_there_are_four_homes() {
    assert_eq!(ITEM, "§11-flow-index#5-where-each-kind-of-decision-lives-directive-a");
    // §5 lists exactly four bullets, so a fifth kind has to be added here and stated there.
    assert_eq!(Home::all().len(), 4);
    assert_eq!(
        Home::all().map(Home::label),
        [
            "editing policy",
            "the prompts",
            "machine settings",
            "engineering constants"
        ]
    );

    // Each home's description carries §5's own parenthetical words.
    assert!(Home::Policy.what_lives_here().contains("lengths"), "{}", Home::Policy.what_lives_here());
    assert!(Home::Policy.what_lives_here().contains("thresholds"));
    assert!(Home::Policy.what_lives_here().contains("budgets"));
    assert!(Home::Prompt.what_lives_here().contains("worded"));
    assert!(Home::Machine.what_lives_here().contains("binaries"));
    assert!(Home::Machine.what_lives_here().contains("servers"));
    assert!(Home::Engineering.what_lives_here().contains("caches"));
    assert!(Home::Engineering.what_lives_here().contains("waits"));

    // And each says where a person changes it — except the last, which says plainly that nobody does.
    for home in Home::all() {
        assert!(!home.where_you_change_it().is_empty(), "{} says nothing about editing it", home.label());
    }
    assert!(Home::Engineering.where_you_change_it().contains("fixed in code"));
    assert!(Home::Policy.where_you_change_it().contains("policy form"));
    assert!(Home::Prompt.where_you_change_it().contains("prompts/<key>.txt"));
    assert!(Home::Machine.where_you_change_it().contains("llm.conf"));
}

// ---- S2: what the video becomes → the editing policy ---------------------------------------

#[test]
fn sec_11_flow_index_5_where_each_kind_of_decision_lives_directive_a_s2_policy_is_derived_and_editable() {
    assert_eq!(ITEM, "§11-flow-index#5-where-each-kind-of-decision-lives-directive-a");
    // Lengths, thresholds and budgets are all policy (§10 §2), whatever their unit.
    for id in [
        "P.policy.targetLengthSeconds",   // a length
        "P.policy.minTakeSeconds",       // a threshold
        "P.policy.narrationMaxWords",    // a budget
        "P.policy.markingPass",          // what runs at all
        "P.policy.reviewPadSeconds",     // P.policy.reviewPadSeconds
    ] {
        assert_eq!(decision_homes::home_of(id), Some(Home::Policy), "{id} is policy");
        // …and `params` agrees with the router, so the two prefix tables cannot drift apart.
        assert_eq!(params::family(id), Family::Policy, "{id}'s family");
    }

    // Editable means F0.7 S4: the derivation fills a field the person has not touched and never
    // replaces one they have. Without that exception "derived from the User Context" would mean
    // "not editable", which is the opposite of what §5 claims.
    assert!(decision_homes::overridable_by_derivation(Origin::Model));
    assert!(decision_homes::overridable_by_derivation(Origin::Default));
    assert!(!decision_homes::overridable_by_derivation(Origin::User));
}

// ---- S3: how a job is worded → the prompts --------------------------------------------------

#[test]
fn sec_11_flow_index_5_where_each_kind_of_decision_lives_directive_a_s3_wording_lives_in_the_prompts() {
    assert_eq!(ITEM, "§11-flow-index#5-where-each-kind-of-decision-lives-directive-a");
    for key in prompts::KEYS {
        assert_eq!(decision_homes::home_of(key), Some(Home::Prompt), "{key} is wording");
    }
    // tools.md sits in prompts/ but is tool descriptions, not a prompt — and being a bare word with
    // no prefix rule to catch it, it names no home at all rather than borrowing one.
    assert!(!prompts::is_known("tools"));
    assert_eq!(decision_homes::home_of("tools"), None);

    // The two vocabularies do not collide: no prompt key is also a parameter id, so an id can never
    // be both "how a job is worded" and "what the video becomes".
    let ids = catalogued();
    assert!(!ids.is_empty(), "the sweep needs ids to check against");
    for key in prompts::KEYS {
        assert!(
            !ids.contains(&key),
            "prompt key {key} also appears as a parameter id \u{2014} two homes for one name"
        );
    }
}

// ---- S4: which servers and binaries → machine settings --------------------------------------

#[test]
fn sec_11_flow_index_5_where_each_kind_of_decision_lives_directive_a_s4_servers_and_binaries_are_machine_settings() {
    assert_eq!(ITEM, "§11-flow-index#5-where-each-kind-of-decision-lives-directive-a");
    // Every conf key is a server, a model id, a folder or a binary path — read off the store itself.
    let keys = settings::Conf::key_names();
    assert_eq!(keys.len(), 15, "§10 §1's fifteen settings");
    for key in &keys {
        assert_eq!(
            decision_homes::home_of(key),
            Some(Home::Machine),
            "{key} is a machine setting"
        );
    }
    // The named ones §5 means by "servers and binaries".
    for key in ["LLM_SERVER", "AUDIOCPP_VOICES", "FFMPEG", "FIREFOX", "SD_SERVER"] {
        assert!(keys.contains(&key), "{key} missing from the conf keys");
    }

    // Slot counts are conf keys too, and route through the same check rather than the dotted rule.
    assert_eq!(settings::Slots::KEY_NAMES.len(), 7);
    for key in settings::Slots::KEY_NAMES {
        assert!(decision_homes::is_machine_setting(key), "{key} is a conf key");
        assert_eq!(decision_homes::home_of(key), Some(Home::Machine));
    }

    // Both routes agree on what §10 spells out: the rowed `P.machine.*` ids are machine settings as
    // surely as the conf keys they are configured by. P.machine.describeFramesPerReq
    for id in ["P.machine.describeFramesPerReq", "P.machine.fixBlockLines", "P.machine.retakeRuns"] {
        assert_eq!(decision_homes::home_of(id), Some(Home::Machine), "{id}");
    }
}

// ---- S5: looks, waits and caches → engineering constants -------------------------------------

#[test]
fn sec_11_flow_index_5_where_each_kind_of_decision_lives_directive_a_s5_look_waits_and_caches_are_engineering() {
    assert_eq!(ITEM, "§11-flow-index#5-where-each-kind-of-decision-lives-directive-a");
    for id in [
        "P.eng.llmStallMinutes",   // P.eng.llmStallMinutes — a wait
        "P.eng.llmWholeMinutes",   // another wait
        "P.eng.frameGridSeconds",  // how frames are cut up
        "layout.waveCacheMagic",   // a cache format
        "layout.undoDepth",        // how the app behaves
        "layout.zoomStep",         // how the app looks
        "llm.backoffSeconds",      // a wait, unrowed by §10
        "preview.playTickMs",      // a wait
        "narrate.autosaveMs",      // a debounce
    ] {
        assert_eq!(decision_homes::home_of(id), Some(Home::Engineering), "{id} is fixed in code");
    }

    // Bare `machine.jpegQuality` is engineering, not a machine setting: the bare prefix names the
    // rule that sizes the value, while only a `P.`-spelled `P.machine.*` row is a Settings entry.
    assert_eq!(decision_homes::home_of("machine.jpegQuality"), Some(Home::Engineering));
    assert!(!decision_homes::is_machine_setting("machine.jpegQuality"));
    // And the rowed spelling of a machine setting really is different from that bare prefix.
    assert_ne!(
        decision_homes::home_of("machine.jpegQuality"),
        decision_homes::home_of("P.machine.thumbnailLongSide")
    );
}

// ---- S6: exhaustive over known ids, disjoint, and unknown ids name nothing --------------------

#[test]
fn sec_11_flow_index_5_where_each_kind_of_decision_lives_directive_a_s6_every_known_id_has_exactly_one_home() {
    assert_eq!(ITEM, "§11-flow-index#5-where-each-kind-of-decision-lives-directive-a");
    let mut seen: std::collections::HashMap<&'static str, Home> = std::collections::HashMap::new();
    let mut ids: Vec<&'static str> = catalogued();
    ids.extend(prompts::KEYS.iter().copied());
    ids.extend(settings::Conf::key_names());
    ids.extend(settings::Slots::KEY_NAMES.iter().copied());

    for id in ids {
        let home = decision_homes::home_of(id).unwrap_or_else(|| panic!("{id} has no home"));
        // The catalogued lists repeat an id across sections on purpose (one constant, two sections
        // naming it), so a clash is only a clash when the two answers differ.
        match seen.get(id) {
            Some(previous) if *previous != home => {
                panic!("{id} landed in two homes: {previous:?} and {home:?}")
            }
            _ => drop(seen.insert(id, home)),
        }
    }
    // All four homes are actually populated by the sweep — an empty home would mean directive A's
    // fourth kind is unreachable in practice.
    for home in Home::all() {
        assert!(
            seen.values().any(|v| *v == home),
            "no known id lives in {}",
            home.label()
        );
    }
    // Policy and Machine are both reached from the catalogued ids alone (not just from prompts/conf),
    // so the split is real across §10 rather than only across the extra lists.
    let from_params: Vec<&str> = seen
        .iter()
        .filter(|(id, _)| id.starts_with("P."))
        .map(|(id, _)| *id)
        .collect();
    assert!(from_params.iter().any(|id| seen[id] == Home::Policy));
    assert!(from_params.iter().any(|id| seen[id] == Home::Machine));
    assert!(from_params.iter().any(|id| seen[id] == Home::Engineering));

    // An id the app does not know names no home, rather than inventing one.
    assert_eq!(decision_homes::home_of(""), None);
    assert_eq!(decision_homes::home_of(".."), None);
    assert_eq!(decision_homes::home_of("not-a-parameter"), None);
    assert_eq!(decision_homes::home_of(".leadingDot"), None);
    assert_eq!(decision_homes::home_of("trailingDot."), None);

    // A `P.unknown.*` id inherits Engineering: §10 has no section for that prefix, so there is no
    // home that could claim it. That is why a new home means spelling a new prefix in §10 first —
    // until then every unsectioned value falls to the one kind that needs no section.
    assert_eq!(decision_homes::home_of("P.unknown.thing"), Some(Home::Engineering));
    assert_eq!(params::family("P.unknown.thing"), Family::Other, "unsectioned, per params");
    // §3's project-tab controls fall the same way: the person picks a value, but the list of values
    // and the meaning of each are this build's code.
    assert_eq!(decision_homes::home_of("P.project.frameInterval"), Some(Home::Engineering));
    assert_eq!(params::family("P.project.frameInterval"), Family::Project);
    // Project-typed storage is `project::Field`, whose origin is what gates the derivation.
    assert_eq!(project::Origin::default(), Origin::Default);
}
