//! Where each Prepare decision lives — `spec/12-decisions.md` §3, "Prepare".
//!
//! The ten rows of that section are the prototype's behind-the-model behaviours and where the rewrite
//! moves each. Almost all of the machinery already exists elsewhere in the crate — the retake marks,
//! the envelope placement, the seam tools, the transcript fixer, the describe batch, the tool loop —
//! so this module holds only the two pieces the audit needed that nothing else had: the fuzzy
//! word-suffix matcher that cuts a mark inside a line (S1), and the classifier that names a mark's
//! case in one sentence for the tool's answer (S3). Everything else is stated here as a routing row
//! pointing at the function that really makes the decision, so the table cannot claim a behaviour
//! this build does not have.
//!
//! The rule the whole section turns on is §1's: what the model is told. A trim it can see is a
//! result; a move it never sees must be mechanical, explainable and visible afterwards.

/// At least this share of the marked tail has to turn up again for the cut to count as a repeat.
///
/// §10 files this bound as `P.eng.repeatShare` (0.7, "fuzzy repeat match: share that must match"; prototype
/// 0.7), and §12 S1 gives the same number in words ("≥ 70 % of the tail matching"). `params::prepare()` rows
/// it from this constant. The share is applied as `ceil(share × tail_len)`, so a word is never counted in
/// halves: a 5-word tail's bar is 4 matched words (0.8 effective) and a 3-word tail must be found whole, i.e.
/// short tails clear a stricter bar than the stated 0.7.
pub const TAIL_MATCH_MIN: f64 = 0.7;

/// How many words of the later take may be stepped over while looking for the next tail word.
///
/// §10 files this bound as `P.eng.repeatSkip` (3, "fuzzy repeat match: words skipped"; prototype
/// `repeatSkip`, gui/retake.go), and §12 S1 gives the same number in words ("≤ 3 words skipped").
/// `params::prepare()` rows it from this constant. It bounds the step-over in [`suffix_match`] directly.
pub const TAIL_SKIP_MAX: usize = 3;

/// How many words may match imperfectly — heard differently, one edit away — before the run is refused.
///
/// §12 S1: "one edit allowed", again with no `P.` id.
pub const TAIL_EDIT_MAX: usize = 1;

/// What the matcher found: where the repeated tail starts in the later take, and how it got there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SuffixMatch {
    /// Index into the later take where the repeat begins.
    pub starts_at: usize,
    /// Tail words accounted for, exact and edited together.
    pub matched: usize,
    /// Later-take words stepped over without consuming a tail word.
    pub skipped: usize,
    /// Tail words that matched only imperfectly.
    pub edits: usize,
    /// `matched / tail.len()`, the share that had to clear [`TAIL_MATCH_MIN`].
    pub share: f64,
}

/// Whether two heard words are the same word.
///
/// Exact first, then a shared prefix or a single edit: ASR hears one word three ways across three
/// passes, and a matcher that demanded equality would refuse most real repeats. Short words are held
/// to exactness because "to" and "two" differ by one edit and are not the same word.
fn same_word(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    if a.chars().count() < 3 || b.chars().count() < 3 {
        return false;
    }
    a.starts_with(b) || b.starts_with(a) || one_edit(a, b)
}

/// One insertion, deletion or substitution apart.
fn one_edit(a: &str, b: &str) -> bool {
    let (long, short) = if a.chars().count() >= b.chars().count() {
        (a, b)
    } else {
        (b, a)
    };
    let long: Vec<char> = long.chars().collect();
    let short: Vec<char> = short.chars().collect();
    if long.len() > short.len() + 1 {
        return false;
    }
    let mut i = 0usize;
    let mut j = 0usize;
    let mut seen = false;
    while i < long.len() && j < short.len() {
        if long[i] == short[j] {
            i += 1;
            j += 1;
            continue;
        }
        if seen {
            return false;
        }
        seen = true;
        if long.len() > short.len() {
            // The extra letter belongs to the longer word: skip it and keep both cursers moving forward.
            i += 1;
        } else {
            i += 1;
            j += 1;
        }
    }
    !(seen && (long.len() - i > 0 || short.len() - j > 0))
}

/// Where the tail of what was said first comes again in the later take.
///
/// Earliest anchor, not longest run: the whole stutter goes, not the part of it that happens to
/// match best. The tail is looked for anywhere in the later take, because a retake does not always
/// resume at its first word; once a run is going, a word that does not match is stepped over rather
/// than ending it, since one word heard differently must not lose the repeat. Both bounds still hold
/// — [`TAIL_SKIP_MAX`] steps and [`TAIL_EDIT_MAX`] imperfect matches — and the run only counts when
/// at least [`TAIL_MATCH_MIN`] of the tail turned up.
///
/// This is the cut inside the line: the model marked whole lines, and the app trims to where the
/// repetition actually starts. The trimmed stretch is what `mark_abandoned` reports as `removed`, so
/// the model sees the seconds that will really go rather than the line bounds it sent.
pub fn suffix_match(tail: &[&str], later: &[&str]) -> Option<SuffixMatch> {
    if tail.is_empty() || later.is_empty() {
        return None;
    }
    for start in 0..later.len() {
        // The anchor has to be the tail's first word; only after that does the tolerance apply.
        if !same_word(tail[0], later[start]) {
            continue;
        }
        let counted = run_from(tail, later, start);
        if counted.matched as f64 >= (TAIL_MATCH_MIN * tail.len() as f64).ceil() {
            return Some(SuffixMatch {
                starts_at: start,
                ..counted
            });
        }
    }
    None
}

/// How much of the tail turns up in the later take starting at `from`, in order, within the bounds.
///
/// Steps over up to [`TAIL_SKIP_MAX`] later words per tail word looking for the next one, and allows
/// [`TAIL_EDIT_MAX`] imperfect matches across the whole run.
fn run_from(tail: &[&str], later: &[&str], from: usize) -> SuffixMatch {
    let mut j = from;
    let mut matched = 0usize;
    let mut edits = 0usize;
    for word in tail {
        let mut found = false;
        for step in 0..=TAIL_SKIP_MAX {
            let probe = j + step;
            if probe >= later.len() {
                break;
            }
            if later[probe] == *word {
                matched += 1;
                j = probe + 1;
                found = true;
                break;
            }
            if edits < TAIL_EDIT_MAX && same_word(word, later[probe]) {
                matched += 1;
                edits += 1;
                j = probe + 1;
                found = true;
                break;
            }
        }
        if !found {
            break;
        }
    }
    // Skips are the later-take positions consumed beyond the one each matched word needed.
    let skipped = (j - from).saturating_sub(matched);
    let share = matched as f64 / tail.len() as f64;
    SuffixMatch {
        starts_at: from,
        matched,
        skipped,
        edits,
        share,
    }
}

/// Why a mark ended up as it did — §12 S3's three cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarkCase {
    /// The take was abandoned entire and never picked up again, so `again` becomes 0.
    WholeTake,
    /// Only the broken-off fragment goes; what the speaker rephrased after it stays.
    RephraseFragment,
    /// The first pass refused the mark; a second ASR pass heard it and the mark is lifted.
    Resurrected,
}

/// Which of the three cases a mark is.
///
/// The second pass wins over everything: a refusal that a fresh listen contradicts is a refusal that
/// was wrong, and saying so is better than keeping a mark against audio that turns out to exist. With
/// no second-pass finding, a take whose replacement covers the whole thing is the whole-take case;
/// anything else is a fragment off the front of a rephrase.
pub fn classify(again_is_whole_take: bool, second_pass_found: bool) -> MarkCase {
    if second_pass_found {
        MarkCase::Resurrected
    } else if again_is_whole_take {
        MarkCase::WholeTake
    } else {
        MarkCase::RephraseFragment
    }
}

impl MarkCase {
    /// The one sentence the tool answers with.
    ///
    /// Written as the reply, not as prose about the reply: a model reading this acts on it, so it says
    /// what happened to the mark rather than what the code did.
    pub fn reason(&self) -> &'static str {
        match self {
            MarkCase::WholeTake => {
                "the whole take was dropped and never picked up again, so again is 0"
            }
            MarkCase::RephraseFragment => {
                "only the broken-off fragment goes; the rephrased rest stays"
            }
            MarkCase::Resurrected => {
                "a second listening heard this stretch, so the mark is lifted"
            }
        }
    }
}

/// One row of the Prepare audit: the behaviour, the home it moved to, and the code that owns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ruled {
    /// The prototype behaviour, close to §12's wording.
    pub what: &'static str,
    /// Where the decision lives now, in §1's terms.
    pub home: &'static str,
    /// The function that actually makes it.
    pub lives_in: &'static str,
}

/// The ten rows of §12 §3 Prepare, in order.
///
/// Kept beside the matcher and the classifier rather than in a doc comment so the pairing of a
/// behaviour with its owner is checkable: a row naming a function that no longer exists fails a test
/// rather than drifting quietly.
pub fn audit() -> [Ruled; 10] {
    [
        Ruled {
            what: "a retake mark is trimmed to a fuzzy-matched word suffix",
            home: "mark_abandoned's result: the stretch that will really go",
            lives_in: "prepare_decisions::suffix_match, reported as tools::retakes::Marks::mark_abandoned's `removed`",
        },
        Ruled {
            what: "both edges of every mark are moved onto the audio envelope",
            home: "after finish (rule 2.2), with the placed seconds in the same tool result",
            lives_in: "edges::place_edges",
        },
        Ruled {
            what: "a whole take rewrites its `again` to 0, a rephrase trims to the fragment, a refused mark is re-heard",
            home: "the result, each with a one-sentence reason",
            lives_in: "prepare_decisions::classify + MarkCase::reason",
        },
        Ruled {
            what: "over the ceiling every mark from all three runs is thrown away",
            home: "finish's answer, with the running total, so marks can be taken back",
            lives_in: "tools::retakes::Marks::finish + Marks::unmark (P.machine.retakeCeil)",
        },
        Ruled {
            what: "the join pass's counts are snapped to the join and small stretches read as respellings",
            home: "drop_words takes the counts as arguments and answers with the words that go",
            lives_in: "tools::textedit::Join::drop_words (P.machine.seamSnapWords, P.machine.seamNoiseWords)",
        },
        Ruled {
            what: "dedupeJoins deletes words the model deliberately kept",
            home: "after finish -- a rule about the cut, not the sentence -- named in the log",
            lives_in: "tools::textedit::dedupe_note",
        },
        Ruled {
            what: "one bad row threw away a block of 25 lines, re-asked twice with no word why",
            home: "fix_line validates per line; flag_line lets the model report a wrong speaker",
            lives_in: "tools::fix::fix_line + tools::fix::flag_line",
        },
        Ruled {
            what: "describe discarded drifted stamps, dropped unparseable offsets, flattened multi-line answers, force-wrote the first frame",
            home: "record_event answers with the frame it landed on; finish names frames still empty",
            lives_in: "tools::describe::Batch::record_event + Batch::finish + place_offset",
        },
        Ruled {
            what: "two lines of speech context a side, indistinguishable from nobody having spoken",
            home: "speech_around answers with every overlapping line, capped by nothing",
            lives_in: "tools::describe::speech_around",
        },
        Ruled {
            what: "retries never told the model it was being re-asked",
            home: "the tool loop is the correction (F6.1); only a transport retry is silent",
            lives_in: "tool_loop::tool_result + tool_loop::retry_without_tools",
        },
    ]
}
