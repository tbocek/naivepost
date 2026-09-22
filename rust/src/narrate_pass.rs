//! F4.2 The narration call (`spec/07-narrate.md` F4.2) — what the one long call is *asked*.
//!
//! SYSTEM = house rules + the "narrate" prompt, plus the speech addendum when the clips play what people
//! said out loud and the captions addendum for a captions-only voice. USER = User Context (+ the speech
//! rule) + `THE CLIPS AND WHAT IS KNOWN ABOUT EACH:` + one block per clip: its bounds, how long it plays,
//! how many words it affords, the transcript rows within ±[`CONTEXT_SECONDS`] of it, and the moments somebody
//! marked. This module builds those two strings and answers the arithmetic around them; nothing here talks
//! to a server or draws a widget.
//!
//! What this module does **not** own:
//! - the record of what was written — [`crate::narration`] holds the entries, their keys and their files;
//! - F4.3's fitting arithmetic (lead/gap/tail packing, growing the clip, sliding the schedule, speeding the
//!   speech). [`crate::narrate_screen`]'s doc already points at the render for that; a tool answer here says
//!   where a line lands and how many words it may have, and stops short of the packing numbers.
//! - the tool list — [`crate::tools::offered`] with [`crate::roles::Job::Narrate`] already lists
//!   `write_line`, `leave_silent`, `list_emotions`, `describe_insert` and `finish`;
//! - the emotion *recipes*. The eight weights a named blend mixes are the TTS side's (the prototype keeps
//!   them in `emoBlends`, `gui/narrate_tts.go`); F4.2 only asks whether a word is in the vocabulary at all.

use crate::cut::{Fx, Seg};
use crate::cut_captions;
use crate::fix_transcripts;
use crate::textfmt::SessionLine;

/// P.machine.narrationContextSeconds (§10's Machine table): how far outside a clip its block reaches for
/// transcript rows. A line just before the cut is the one the narration should answer to, and a line just
/// after it is what the next clip will react to, so the window runs both ways.
pub const CONTEXT_SECONDS: f64 = 4.0;

/// P.policy.narrationMinWords (§10's policy table): even a five-second clip can take a short line.
pub const MIN_WORDS: u32 = 8;

/// P.policy.narrationMaxWords: the cap binds past forty seconds, so a two-minute clip does not get two
/// minutes of talking.
pub const MAX_WORDS: u32 = 30;

/// P.policy.narrationWordsPerSecond: under a third of speaking rate, so there is room for the thought AND
/// its punchline and the words run out early in every clip.
pub const WORDS_PER_SECOND: f64 = 0.75;

/// F4.2 S2 (`USER … "THE CLIPS AND WHAT IS KNOWN ABOUT EACH:"`): §F4.2's own wording in caps, because it is
/// the line that separates what is known about a clip from what is being asked of the writer.
pub const CLIPS_HEADER: &str = "THE CLIPS AND WHAT IS KNOWN ABOUT EACH:";

/// F4.2 S2: what a clip with nothing over it says. Neutral on purpose — three passes read this brief, and
/// "say something general" is an instruction to one of them.
pub const NO_LINES: &str = "  (no lines over this clip: nothing said, nothing described)";

/// The speech rule, which rides under the User Context for the two jobs that decide what is spoken ("cut"
/// and "narrate") and nowhere else. Read the wrong way it is the worst answer this app gives — an aside to
/// the editor captioned into the video, or the video thrown away as asides — so it is said once, verbatim.
pub const CTX_SPEECH: &str = "The speech is content unless the user context above says otherwise: the speakers are in the video, and what they say is why a moment is worth keeping. Where the user context calls it directions (\"this part is boring\", \"speed this up\"), do what a direction asks at the second it asks and keep its words out of the video -- never caption them, and never keep a stretch just because it was spoken over. An instruction about a kind of stretch -- speed the dull parts up and show them instead of cutting them, caption each thing as it is named -- holds wherever such a stretch occurs. It decides segments too: a stretch to be shown fast has to be in the cut, with a speed effect over it, or there is nothing left to speed up.";

/// F4.2 (`+ the speech addendum, when the clips play what people said out loud`): the premise the prompt
/// turns on once [`speech_heard`] says somebody is audible in the finished video.
pub const SPEECH_ADDENDUM: &str = "THIS VIDEO PLAYS WHAT PEOPLE SAID OUT LOUD. The lines marked SPEAKER are heard by the viewer in the speakers' own voices, so never say one back: set it up before it lands, or react after it. The NARRATOR lines are still yours -- nobody hears those unless you use them -- and so is every clip the speakers left alone.";

/// F4.2 (`+ the captions addendum, for a captions-only voice`): the same writer, writing lines nobody will
/// ever speak. Appended rather than shipped as a style, so a user-edited narrate prompt keeps working.
pub const CAPTIONS_ADDENDUM: &str = "THIS VIDEO HAS NO VOICE-OVER. Nobody speaks your lines: they are burned into the picture as captions and the viewer READS them. Everything above still holds -- the taste, the timing, the restraint -- with these changes:\n- Write for the eye, not the ear. Shorter still: a caption is read in the corner of the attention, and a line the viewer has to study is a line over footage they are missing.\n- \"emotion\" means nothing with no voice. Leave it \"\".\n- Give every entry with text a \"pos\": where the caption sits on the picture. \"bottom\" is the default and almost always right; \"top\" when the action or the game's own UI lives at the bottom of the frame; \"center\" only for a line that IS the moment, like a title card.\nSo each entry is {\"start\":<sec>,\"end\":<sec>,\"at\":<sec>,\"text\":\"...\",\"emotion\":\"\",\"pos\":\"bottom|top|center\"}.";

/// The heading of the User Context block: what it is, and how far it outranks the job's own rules. Only the
/// mechanics of the answer are not its to change, which is why the sentence says so before the text does.
const CTX_INTRO: &str = "USER CONTEXT -- written by the person who made this recording and is editing it. It outranks anything you infer from the material, and it outranks the rules of the job you were given wherever the two disagree; only the mechanics of the answer -- its shape, its clock, what may be invented -- are not its to change:";

/// F4.2 (`at most W words`): how many words a clip is offered — [`WORDS_PER_SECOND`] per second of what the
/// viewer watches, between the floor and the ceiling. The seconds are the on-screen ones, because that is
/// how long the line has to be spoken in: a clip at 2x affords half as many words as its session length.
pub fn word_ceiling(length: f64) -> u32 {
    let some = (length * WORDS_PER_SECOND) as i64;
    some.clamp(MIN_WORDS as i64, MAX_WORDS as i64) as u32
}

/// F4.2's heading needs the clip's own length: a spliced insert runs for its `dur`, footage at a rate runs
/// for its seconds divided by that rate, and everything else runs for the seconds between its bounds.
pub fn on_screen(seg: &Seg) -> f64 {
    if seg.dur > 0.0 {
        return seg.dur;
    }
    if seg.rate > 0.0 {
        return (seg.e - seg.s) / seg.rate;
    }
    seg.e - seg.s
}

/// F4.2 (`(X s of footage at Rx, Y s on screen)`): how long the clip is on screen and at what speed — the
/// two facts every brief's heading needs, and only one of which is its length. A clip at 4 runs a quarter of
/// the session seconds its lines are stamped in, so a line at `[+40s]` of it arrives ten seconds into what
/// the viewer watches; the rate has to be said somewhere, and this is that somewhere. A rate of 1 is the
/// footage's own clock and says nothing, so it is not mentioned.
pub fn plays(seg: &Seg) -> String {
    if seg.rate > 0.0 && seg.rate != 1.0 {
        // `{}` on a whole f64 prints "2", which is what "at 2x" reads as; the prototype's %g does the same.
        let rate = if seg.rate == seg.rate.trunc() {
            format!("{}", seg.rate as i64)
        } else {
            format!("{}", seg.rate)
        };
        return format!(
            "{:.0} s of footage at {}x, {:.0} s on screen",
            seg.e - seg.s,
            rate,
            on_screen(seg)
        );
    }
    format!("{:.0} s", on_screen(seg))
}

/// F4.2 (`CLIP n: a–b (…, at most W words -- fewer is better, none is fine)`): one clip's heading, with the
/// dash between the bounds an en dash because that is what the prototype prints and what a reader of the log
/// expects to see. The clip number is 1-based: it is what the model sends back as `clip`.
pub fn clip_heading(n: u32, seg: &Seg) -> String {
    format!(
        "CLIP {n}: {:.1}\u{2013}{:.1} ({}, at most {} words -- fewer is better, none is fine)",
        seg.s,
        seg.e,
        plays(seg),
        word_ceiling(on_screen(seg))
    )
}

/// The asset's last path component, query kept: "tier.svg" says nothing about what is on the card and
/// "tier.svg?S=Dust II,Mirage" says all of it, which is exactly what a narrator needs to read. A `project:`
/// prefix is part of how a path is *stored*, not part of its name (§1's one path rule), so it goes too — the
/// writer is shown a file name, never a scheme.
fn asset_name(ins: &str) -> &str {
    let name = ins.rsplit('/').next().unwrap_or(ins);
    name.strip_prefix("project:").unwrap_or(name)
}

/// F4.2 S2 (`with its lines within ±P.machine.narrationContextSeconds as "[+Ns] LABEL: text"`), plus the
/// moments somebody marked. Seconds are offsets from the clip's own start, so the narration can follow the
/// clip instead of arriving at it; a line just outside the clip goes negative or past the end, which is
/// exactly what it is. A label contributes its words and nothing else — it changes nothing in the video, it
/// exists only to be read here: the editor's own word for a moment, which no transcript row can contain.
pub fn brief(segs: &[Seg], rows: &[SessionLine], fx: &[Fx], narrator: &str) -> String {
    let mut out = String::new();
    for (i, seg) in segs.iter().enumerate() {
        out.push('\n');
        out.push_str(&clip_heading(i as u32 + 1, seg));
        out.push('\n');
        // An insert has no footage under it and no transcript over it, so the lines around it would describe
        // whatever it covered — the one thing the viewer will not be looking at. Say what it is instead.
        if seg.is_insert() {
            out.push_str(&format!(
                "  (not footage: a graphic or clip inserted here, {:?}. Narrate what is ON it, or say \
                 nothing -- there is nothing from the session to describe, and inventing one would caption \
                 the wrong picture)\n",
                asset_name(&seg.ins)
            ));
            continue;
        }
        let mut said = 0usize;
        for row in rows {
            if row.end <= seg.s - CONTEXT_SECONDS || row.start >= seg.e + CONTEXT_SECONDS {
                continue;
            }
            out.push_str(&format!(
                "  [{:+.0}s] {}: {}\n",
                row.start - seg.s,
                fix_transcripts::label(asset_name(&row.source), &row.who, narrator),
                row.text
            ));
            said += 1;
        }
        if said == 0 {
            out.push_str(NO_LINES);
            out.push('\n');
        }
        for effect in fx {
            let (from, to) = effect.spans();
            let text = effect.text.trim();
            if text.is_empty() || to <= seg.s || from >= seg.e {
                continue;
            }
            // Only a label and a caption are worth reading here: a zoom, a speed or a volume says something
            // about the footage that the writer has no way to speak to.
            let tag = match effect.kind.as_str() {
                "label" => "MARKED",
                "text" => "CAPTION",
                _ => continue,
            };
            out.push_str(&format!("  [{:+.0}s] {tag}: {text}\n", from - seg.s));
        }
    }
    out
}

/// F4.2's premise: does the finished video play anything anybody said? A spoken line survives when the scene
/// covering it keeps its lane, and the narrator's own microphone never survives — that is the whole point of
/// having one. `source` arrives as a `session.tsv` path or `project:`-relative source and `narrator` is that
/// same spelling trimmed to the name the mic is known by, so both are compared on their last path component:
/// an insert hears nothing either — it replaces the footage that carried the line. This is what decides
/// whether [`SPEECH_ADDENDUM`] rides on the prompt.
pub fn speech_heard(segs: &[Seg], rows: &[SessionLine], narrator: &str) -> bool {
    for row in rows {
        let source = asset_name(&row.source);
        if row.is_event() || (!narrator.is_empty() && source.contains(narrator)) {
            continue;
        }
        if segs.iter().any(|seg| {
            !seg.is_insert() && row.end > seg.s && row.start < seg.e && seg.hears(source)
        }) {
            return true;
        }
    }
    false
}

/// F4.2 S1 (`SYSTEM house rules + the "narrate" prompt + …`): the two addenda ride on the end, each on its
/// own, so a project that both plays speech and has no voice-over (a captions-only voice over clips that
/// keep their lanes) says both. The prompt arrives as [`crate::settings::prompt_text`] answered it — this
/// module does not read files, so the house rules and the "narrate" wording are the caller's to fetch.
pub fn system(prompt: &str, speech: bool, captions_only: bool) -> String {
    let mut out = prompt.to_string();
    if speech {
        out.push_str("\n\n");
        out.push_str(SPEECH_ADDENDUM);
    }
    if captions_only {
        out.push_str("\n\n");
        out.push_str(CAPTIONS_ADDENDUM);
    }
    out
}

/// F4.2 S2 (`USER User Context (+ the speech rule) + "THE CLIPS…" + per clip …`): the whole second message.
/// An empty context adds nothing at all, not an empty block — a header over nothing would read as a note the
/// writer should still obey — and the speech rule is only ever about a context that exists.
pub fn user_message(context: &str, segs: &[Seg], rows: &[SessionLine], fx: &[Fx], narrator: &str) -> String {
    let mut out = String::new();
    if !context.trim().is_empty() {
        out.push_str(CTX_INTRO);
        out.push('\n');
        out.push_str(context.trim());
        out.push_str("\n\n");
        out.push_str(CTX_SPEECH);
        out.push_str("\n\n");
    }
    out.push_str(CLIPS_HEADER);
    out.push_str(&brief(segs, rows, fx, narrator));
    out
}

// --- What the tools answer (§02-services.md §3.8) ---------------------------------------------------------

/// The eight bases with the words that mean one of them, in [`crate::narrate_screen::EMOTIONS`]' order (the
/// prototype's `emoBases`, `gui/narrate_tts.go`). The lists are deliberately generous: someone writing
/// "furious" or "deadpan" means one of these eight, and refusing them would send the line down the slower
/// path for a spelling. What lives here is only the *vocabulary* — the eight weights a word mixes stay with
/// the TTS side, as do the named blends' recipes.
pub const EMOTION_KIN: [(&str, &[&str]); 8] = [
    ("happy", &["happy", "happiness", "joy", "joyful", "cheerful", "glad", "delighted", "upbeat", "pleased"]),
    ("angry", &["angry", "anger", "mad", "furious", "fury", "rage", "annoyed", "irritated", "indignant"]),
    ("sad", &["sad", "sadness", "unhappy", "sorrow", "sorrowful", "hurt", "crying", "tearful"]),
    ("afraid", &["afraid", "fear", "fearful", "scared", "frightened", "terrified", "nervous", "anxious", "panicked"]),
    ("disgusted", &["disgusted", "disgust", "revolted", "repulsed", "grossed"]),
    ("melancholic", &["melancholic", "melancholy", "low", "gloomy", "down", "depressed", "depression", "wistful", "weary"]),
    ("surprised", &["surprised", "surprise", "shocked", "amazed", "astonished", "startled", "stunned"]),
    // The eighth base is spelled three ways by the people writing it: `calm` is what §1 calls it, `natural`
    // and `neutral` are what a model sends.
    ("calm", &["natural", "neutral", "calm", "deadpan", "flat", "plain", "even", "relaxed", "matter-of-fact"]),
];

/// `list_emotions(–)` (§3.8: "the eight bases with their kin words"): the table above, one line per base, so
/// the writer is never asked to guess a spelling it has not been shown.
pub fn list_emotions() -> String {
    EMOTION_KIN
        .iter()
        .map(|(base, kin)| {
            let words = kin.iter().copied().filter(|word| *word != *base).collect::<Vec<_>>();
            format!("{base}: {}", words.join(", "))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// §3.8's `pos?`: the three placements Produce can burn, normalised to what the record stores. `"bottom"` is
/// the default and therefore stored as nothing at all — the same bytes a line with no placement gets. A word
/// outside these five is refused rather than read as bottom: silently moving a caption the writer meant for
/// the top of the frame is not something it can notice, and §3.8 says so in as many words ("error when …
/// `pos` is not top/center/bottom"). No placement at all — the empty string — is `None` too, and its caller
/// treats that as the default; only a word that names some other corner of the picture is worth answering.
pub fn normalise_pos(pos: &str) -> Option<&'static str> {
    match pos.trim().to_lowercase().as_str() {
        "top" => Some("top"),
        "center" | "centre" | "middle" => Some("center"),
        "bottom" => Some(""),
        _ => None,
    }
}

/// §F4.2's `at` clamped inside the clip, never its last second: a line that starts at the final second has
/// nowhere to be spoken, and a negative offset belongs to the clip before. The caller passes the clip's own
/// on-screen length, since that — not the session seconds — is the clock the writer was shown.
pub fn at_offset(at: f64, length: f64) -> f64 {
    at.clamp(0.0, (length - 1.0).max(0.0))
}

/// §3.8's `emotion`: is every word of this tag in the vocabulary? Empty is known — a line with no emotion is
/// what a captions-only project writes and what a plain reading asks for. Otherwise each comma-separated part
/// must name a base or one of its kin, with an optional `=weight` whose value has to parse as a number (a
/// weight that is not a number is not a weight). The named blends are the TTS side's business; here they are
/// simply words, and refusing them is what tells the writer to ask for a base instead.
pub fn emotion_known(emotion: &str) -> bool {
    if emotion.trim().is_empty() {
        return true;
    }
    for part in emotion.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let (name, weight) = match part.split_once('=') {
            Some((name, weight)) => (name.trim(), Some(weight.trim())),
            None => (part, None),
        };
        if let Some(value) = weight {
            if value.parse::<f64>().is_err() {
                return false;
            }
        }
        let lower = name.to_lowercase();
        let known = EMOTION_KIN.iter().any(|(base, kin)| {
            *base == lower.as_str() || kin.iter().any(|word| *word == lower)
        });
        if !known {
            return false;
        }
    }
    true
}

/// `write_line(clip, at, text, emotion, pos?)` — §3.8's "ok with **the line as it will play** … and the
/// emotion resolved into its eight weights; error when the clip is not one given, `pos` is not
/// top/center/bottom, or the emotion is not in the vocabulary". Refusals come in that order because each is
/// about a different argument, and the model gets one sentence per call.
///
/// What the answer does *not* carry is where the line ends up after packing: the lead, the gap behind the
/// line above, whether the clip will be grown or the speech sped up. That arithmetic is F4.3's and belongs
/// to the render; a tool answer says what this call decided (the clamped offset, the ceiling it must stay
/// under) so a long line can be rewritten shorter instead of squeezed later.
pub fn write_line(clips: &[Seg], clip: u32, at: f64, text: &str, emotion: &str, pos: &str) -> String {
    if let Some(problem) = cut_captions::clip_outside(clip, 1, clips.len() as u32) {
        return crate::tools::error(&problem);
    }
    let Some(place) = normalise_pos(pos).or((pos.trim().is_empty()).then_some("")) else {
        return crate::tools::error(&format!(
            "pos {pos:?} is not a placement -- say \"bottom\" (the default), \"top\" or \"center\""
        ));
    };
    if !emotion_known(emotion) {
        return crate::tools::error(&format!(
            "emotion {:?} is not in the vocabulary -- ask for one of the eight bases \
             ({}) or their kin, or leave it empty",
            emotion,
            crate::narrate_screen::EMOTIONS.join(", ")
        ));
    }
    let seg = &clips[(clip - 1) as usize];
    let screen = on_screen(seg);
    let placed = at_offset(at, screen);
    crate::tools::ok(&serde_json::json!({
        "clip": clip,
        "at": placed,
        "seconds_on_screen": screen,
        "words": text.split_whitespace().count(),
        "at_most_words": word_ceiling(screen),
        "emotion": emotion.trim(),
        "pos": place,
        "text": text.trim(),
    }))
}

/// `leave_silent(clip)` — ok, and nothing else. Silence has to be *said*: a clip with no entry at all is a
/// reply the caller rejects, while this is a clip the writer looked at and left alone.
pub fn leave_silent(clip: u32) -> String {
    crate::tools::ok(&serde_json::json!({ "clip": clip, "silent": true }))
}

// --- The call's own flow (F4.2's last paragraph) ------------------------------------------------------------

/// How many times the one call is made before the run gives up with nothing written.
pub fn attempts() -> u32 {
    crate::roles::LLM_ATTEMPTS
}

/// Whether the NEXT attempt may still think: thinking stops after an attempt that spent the whole call
/// thinking and wrote nothing, and it never comes back on — a model that answered once was not held back by
/// the reasoning budget.
pub fn thinking_after(thinking: bool, wrote_anything: bool) -> bool {
    thinking && wrote_anything
}

/// What is logged when thinking is switched off mid-run, so the log says why the next attempt reads shorter.
pub fn thinking_off_log() -> String {
    ">>> narrate: the model spent the whole call thinking and wrote nothing \u{2014} asking again with thinking off"
        .to_string()
}

/// What is logged between two attempts: the reason this reply was unusable, said back before it is asked
/// for once more.
pub fn rejected_log(attempt: u32, problem: &str) -> String {
    format!(">>> narration attempt {attempt} rejected: {problem}")
}

/// The bar while the one long call runs: clips counted as they close, out of the clips asked about.
pub fn progress(written: u32, total: u32) -> String {
    format!("writing {written}/{total} clips")
}

/// Only ever forward. A retry starts the count again, and a bar that fell back to 1/9 would read as work
/// being undone rather than redone.
pub fn progress_forward(best: u32, seen: u32) -> u32 {
    best.max(seen)
}

/// Never served from the reply cache: the same brief must be rewritten when the user edits the prompt or
/// moves a cut, and the expensive part is the voice, not the words — the TTS cache is the cache.
pub fn served_from_cache() -> bool {
    false
}

/// Every clip empty is not taste but a model that refused the job, and it fails the run rather than writing
/// a silent video.
pub fn no_line_fault() -> String {
    "every clip came back with no line at all".to_string()
}

/// A clip nobody answered: silence has to be said, so an absent entry is rejected the same way an empty one
/// is — naming the clip and its bounds, which is what the reply was asked to echo.
pub fn missing_clip_fault(n: u32, s: f64, e: f64) -> String {
    format!("clip {n} ({s:.1}-{e:.1}) got no entry")
}
