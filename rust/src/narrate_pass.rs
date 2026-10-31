//! F4.2 The narration call (`spec/07-narrate.md` F4.2) — what the one long call is *asked*.

use crate::cut::{Fx, Seg};
use crate::cut_captions;
use crate::fix_transcripts;
use crate::textfmt::SessionLine;

/// P.machine.narrationContextSeconds (§10's Machine table): how far outside a clip its block reaches for
/// transcript rows.
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

/// F4.2 S2: what a clip with nothing over it says.
pub const NO_LINES: &str = "  (no lines over this clip: nothing said, nothing described)";

/// The speech rule, which rides under the User Context for the two jobs that decide what is spoken ("cut"
/// and "narrate") and nowhere else.
pub const CTX_SPEECH: &str = "The speech is content unless the user context above says otherwise: the speakers are in the video, and what they say is why a moment is worth keeping. Where the user context calls it directions (\"this part is boring\", \"speed this up\"), do what a direction asks at the second it asks and keep its words out of the video -- never caption them, and never keep a stretch just because it was spoken over. An instruction about a kind of stretch -- speed the dull parts up and show them instead of cutting them, caption each thing as it is named -- holds wherever such a stretch occurs. It decides segments too: a stretch to be shown fast has to be in the cut, with a speed effect over it, or there is nothing left to speed up.";

/// F4.2 (`+ the speech addendum, when the clips play what people said out loud`): the premise the prompt
/// turns on once [`speech_heard`] says somebody is audible in the finished video.
pub const SPEECH_ADDENDUM: &str = "THIS VIDEO PLAYS WHAT PEOPLE SAID OUT LOUD. The lines marked SPEAKER are heard by the viewer in the speakers' own voices, so never say one back: set it up before it lands, or react after it. The NARRATOR lines are still yours -- nobody hears those unless you use them -- and so is every clip the speakers left alone.";

/// F4.2 (`+ the captions addendum, for a captions-only voice`): the same writer, writing lines nobody will
/// ever speak.
pub const CAPTIONS_ADDENDUM: &str = "THIS VIDEO HAS NO VOICE-OVER. Nobody speaks your lines: they are burned into the picture as captions and the viewer READS them. Everything above still holds -- the taste, the timing, the restraint -- with these changes:\n- Write for the eye, not the ear. Shorter still: a caption is read in the corner of the attention, and a line the viewer has to study is a line over footage they are missing.\n- \"emotion\" means nothing with no voice. Leave it \"\".\n- Give every entry with text a \"pos\": where the caption sits on the picture. \"bottom\" is the default and almost always right; \"top\" when the action or the game's own UI lives at the bottom of the frame; \"center\" only for a line that IS the moment, like a title card.\nSo each entry is {\"start\":<sec>,\"end\":<sec>,\"at\":<sec>,\"text\":\"...\",\"emotion\":\"\",\"pos\":\"bottom|top|center\"}.";

/// The heading of the User Context block: what it is, and how far it outranks the job's own rules.
pub const CTX_INTRO: &str = "USER CONTEXT -- written by the person who made this recording and is editing it. It outranks anything you infer from the material, and it outranks the rules of the job you were given wherever the two disagree; only the mechanics of the answer -- its shape, its clock, what may be invented -- are not its to change:";

/// F4.2 (`at most W words`): how many words a clip is offered — [`WORDS_PER_SECOND`] per second of what the
/// viewer watches, between the floor and the ceiling.
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
/// two facts every brief's heading needs, and only one of which is its length.
pub fn plays(seg: &Seg) -> String {
    if seg.rate > 0.0 && seg.rate != 1.0 {
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
/// "tier.svg?S=Dust II,Mirage" says all of it, which is exactly what a narrator needs to read.
fn asset_name(ins: &str) -> &str {
    let name = ins.rsplit('/').next().unwrap_or(ins);
    name.strip_prefix("project:").unwrap_or(name)
}

fn insert_words(seg: &Seg) -> Vec<String> {
    let path = crate::cut_cards::split_path(&seg.ins);
    match std::fs::read_to_string(&path.file) {
        Ok(raw) => {
            let card = crate::produce_cards::draw(&seg.ins, &raw);
            crate::cut_cards::card_words(&card.drawn)
        }
        Err(_) => Vec::new(),
    }
}

/// F4.2 S2 (`with its lines within ±P.machine.narrationContextSeconds as "[+Ns] LABEL: text"`), plus the
/// moments somebody marked.
pub fn brief(segs: &[Seg], rows: &[SessionLine], fx: &[Fx], narrator: &str) -> String {
    let mut out = String::new();
    for (i, seg) in segs.iter().enumerate() {
        out.push('\n');
        out.push_str(&clip_heading(i as u32 + 1, seg));
        out.push('\n');
        if seg.is_insert() {
            out.push_str(&format!(
                "  (not footage: a graphic or clip inserted here, {:?}. Narrate what is ON it, or say \
                 nothing -- there is nothing from the session to describe, and inventing one would caption \
                 the wrong picture)\n",
                asset_name(&seg.ins)
            ));
            // §12 #28: the file name alone was the whole description, and a name like `tier.svg?S=Dust II`
            // says nothing about what the viewer sees.
            for said in insert_words(seg) {
                out.push_str(&format!("  ON THE CARD: {said}\n"));
            }
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
            if to <= seg.s || from >= seg.e {
                continue;
            }
            // \u00a712 #29: every kind is read here, not only a label and a caption.
            let Some((tag, words)) = effect_tag(effect) else {
                continue;
            };
            out.push_str(&format!("  [{:+.0}s] {tag}: {words}\n", from - seg.s));
        }
    }
    out
}

/// F4.2's premise: does the finished video play anything anybody said?
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
/// keep their lanes) says both.
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
pub fn user_message(
    context: &str,
    segs: &[Seg],
    rows: &[SessionLine],
    fx: &[Fx],
    narrator: &str,
) -> String {
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

pub const EMOTION_KIN: [(&str, &[&str]); 8] = [
    (
        "happy",
        &[
            "happy",
            "happiness",
            "joy",
            "joyful",
            "cheerful",
            "glad",
            "delighted",
            "upbeat",
            "pleased",
        ],
    ),
    (
        "angry",
        &[
            "angry",
            "anger",
            "mad",
            "furious",
            "fury",
            "rage",
            "annoyed",
            "irritated",
            "indignant",
        ],
    ),
    (
        "sad",
        &[
            "sad",
            "sadness",
            "unhappy",
            "sorrow",
            "sorrowful",
            "hurt",
            "crying",
            "tearful",
        ],
    ),
    (
        "afraid",
        &[
            "afraid",
            "fear",
            "fearful",
            "scared",
            "frightened",
            "terrified",
            "nervous",
            "anxious",
            "panicked",
        ],
    ),
    (
        "disgusted",
        &["disgusted", "disgust", "revolted", "repulsed", "grossed"],
    ),
    (
        "melancholic",
        &[
            "melancholic",
            "melancholy",
            "low",
            "gloomy",
            "down",
            "depressed",
            "depression",
            "wistful",
            "weary",
        ],
    ),
    (
        "surprised",
        &[
            "surprised",
            "surprise",
            "shocked",
            "amazed",
            "astonished",
            "startled",
            "stunned",
        ],
    ),
    // The eighth base is spelled three ways by the people writing it: `calm` is what §1 calls it, `natural`
    // and `neutral` are what a model sends.
    (
        "calm",
        &[
            "natural",
            "neutral",
            "calm",
            "deadpan",
            "flat",
            "plain",
            "even",
            "relaxed",
            "matter-of-fact",
        ],
    ),
];

pub fn list_emotions() -> String {
    let mut lines: Vec<String> = EMOTION_KIN
        .iter()
        .map(|(base, kin)| {
            let words = kin
                .iter()
                .copied()
                .filter(|word| *word != *base)
                .collect::<Vec<_>>();
            format!("{base}: {}", words.join(", "))
        })
        .collect();
    lines.push(String::new());
    lines.push(BLENDS_HEAD.to_string());
    for blend in crate::narrate_tts::BLENDS.iter() {
        lines.push(crate::narrate_tts::blend_recites(blend));
    }
    lines.join("\n")
}

const BLENDS_HEAD: &str = "named blends, as their recipes over the eight bases:";

/// §3.8's `pos?`: the three placements Produce can burn, normalised to what the record stores.
pub fn normalise_pos(pos: &str) -> Option<&'static str> {
    match pos.trim().to_lowercase().as_str() {
        "top" => Some("top"),
        "center" | "centre" | "middle" => Some("center"),
        "bottom" => Some(""),
        _ => None,
    }
}

/// §F4.2's `at` clamped inside the clip, never its last second: a line that starts at the final second has
/// nowhere to be spoken, and a negative offset belongs to the clip before.
pub fn at_offset(at: f64, length: f64) -> f64 {
    at.clamp(0.0, (length - 1.0).max(0.0))
}

/// §3.8's `emotion`: is every word of this tag in the vocabulary?
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
        let known = EMOTION_KIN
            .iter()
            .any(|(base, kin)| *base == lower.as_str() || kin.iter().any(|word| *word == lower));
        if !known {
            return false;
        }
    }
    true
}

/// `write_line(clip, at, text, emotion, pos?)` — §3.8's "ok with **the line as it will play** … and the
/// emotion resolved into its eight weights; error when the clip is not one given, `pos` is not
/// top/center/bottom, or the emotion is not in the vocabulary".
pub fn write_line(
    clips: &[Seg],
    clip: u32,
    at: f64,
    text: &str,
    emotion: &str,
    pos: &str,
) -> String {
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
    // \u00a712 #23 / \u00a7 F4.2's flow box: the answer carries "whether it will be squeezed", in the render's
    // own logged sentence, so a long line can be rewritten here instead of sped up silently later.
    let mut body = serde_json::json!({
        "clip": clip,
        "at": placed,
        "seconds_on_screen": screen,
        "words": text.split_whitespace().count(),
        "at_most_words": word_ceiling(screen),
        "emotion": emotion.trim(),
        "weights": crate::narrate_tts::weights_of(emotion),
        "pos": place,
        "text": text.trim(),
    });
    if let Some(squeeze) = pack_warning(screen, placed, text.trim().chars().count(), &[], clip) {
        body["squeeze"] = serde_json::json!(squeeze);
    }
    crate::tools::ok(&body)
}

/// The clip's whole run of lines packed, the lead and the gap behind the line above included: F4.3's
/// arithmetic, kept short on purpose because the render's own copy is the real one (`produce_render::fit`,
/// whose `MAX_TEMPO` is quoted below) and this is only what the TOOL can say before any render runs.
pub fn pack_warning(
    screen: f64,
    at: f64,
    chars: usize,
    before: &[(f64, usize)],
    clip: u32,
) -> Option<String> {
    let speech_of = |count: usize| {
        crate::narrate_screen::page_speech_seconds(
            count,
            crate::narrate_screen::SPEECH_CHARS_PER_SECOND,
        )
    };
    let speech: f64 = before
        .iter()
        .map(|(_, count)| speech_of(*count))
        .sum::<f64>()
        + speech_of(chars);
    // Pack in clip order, as the render does: each line starts after the one above it finishes + gap + tail.
    let mut placed: Vec<(f64, f64)> = before
        .iter()
        .map(|(line_at, line_chars)| (*line_at, speech_of(*line_chars)))
        .chain(std::iter::once((at, speech_of(chars))))
        .collect();
    placed.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut end = 0.0_f64;
    for (line_at, seconds) in placed {
        let start = line_at
            .max(crate::produce_render::LEAD_SECONDS)
            .max(end + crate::produce_render::GAP_SECONDS + crate::produce_render::TAIL_SECONDS);
        end = start + seconds;
    }
    if end + crate::produce_render::TAIL_SECONDS <= screen {
        return None;
    }
    let needed = (end + crate::produce_render::TAIL_SECONDS) / screen.max(f64::MIN_POSITIVE);
    let tempo = needed.clamp(1.0, crate::produce_render::MAX_TEMPO);
    Some(crate::produce_render::sped_up_log(
        clip as usize,
        speech,
        screen,
        tempo,
    ))
}

/// `leave_silent(clip)` — ok, and nothing else.
pub fn leave_silent(clip: u32) -> String {
    crate::tools::ok(&serde_json::json!({ "clip": clip, "silent": true }))
}

/// §3.8's `get_lines(from, to)`: how many rows one read may hand back.
pub const GET_LINES_MAX_ROWS: usize = 40;

/// `get_lines(from, to)` — §3.8: "the transcript around a clip, beyond the brief's
/// ±P.machine.narrationContextSeconds".
pub fn get_lines(rows: &[SessionLine], anchor: f64, from: f64, to: f64, narrator: &str) -> String {
    let mut hits: Vec<&SessionLine> = rows
        .iter()
        .filter(|row| row.start < to && row.end > from)
        .collect();
    hits.sort_by(|a, b| a.start.total_cmp(&b.start));
    let truncated = hits.len() > GET_LINES_MAX_ROWS;
    hits.truncate(GET_LINES_MAX_ROWS);
    if hits.is_empty() {
        return crate::tools::ok(&serde_json::json!({
            "from": from,
            "to": to,
            "lines": [],
            "note": format!(
                "nothing said between {:+.0}s and {:+.0}s of this clip",
                from - anchor,
                to - anchor
            ),
        }));
    }
    let lines: Vec<String> = hits
        .iter()
        .map(|row| {
            format!(
                "[{:+.0}s] {}: {}",
                row.start - anchor,
                fix_transcripts::label(asset_name(&row.source), &row.who, narrator),
                row.text
            )
        })
        .collect();
    crate::tools::ok(&serde_json::json!({
        "from": from,
        "to": to,
        "lines": lines,
        "truncated": truncated,
    }))
}

/// `{}` on a whole f64 prints "2", which is what "at 2x" reads as; a trailing tail of decimals is not wanted
/// in a brief line.
pub fn rate_words(rate: f64) -> String {
    if rate == rate.trunc() {
        format!("{}", rate as i64)
    } else {
        format!("{rate}")
    }
}

/// \u00a712 #29: the narration brief passes EVERY effect kind \u{2014} "the narration brief carries only `label`
/// and `text` effects: a zoom, an SVG burned over the picture, a volume mute \u{2014} all invisible to the
/// writer \u{2192} pass every kind".
pub fn effect_tag(effect: &Fx) -> Option<(&'static str, String)> {
    let text = effect.text.trim();
    match effect.kind.as_str() {
        "label" => (!text.is_empty()).then(|| ("MARKED", text.to_string())),
        "text" => (!text.is_empty()).then(|| ("CAPTION", text.to_string())),
        "zoom" => Some((
            "ZOOM",
            if text.is_empty() {
                "the picture is punched in here".to_string()
            } else {
                text.to_string()
            },
        )),
        "speed" | "stop" => Some(if effect.rate == 0.0 {
            ("STOP", "the picture holds here".to_string())
        } else {
            (
                "SPEED",
                format!("the footage runs at {}x here", rate_words(effect.rate)),
            )
        }),
        "volume" => Some((
            "VOLUME",
            format!(
                "the sound runs at {}x of its own level here",
                rate_words(effect.gain)
            ),
        )),
        "svg" => Some((
            "GRAPHIC",
            if text.is_empty() {
                "a graphic is burned over the picture".to_string()
            } else {
                text.to_string()
            },
        )),
        other => Some((
            "EFFECT",
            format!(
                "{other}{}",
                if text.is_empty() {
                    String::new()
                } else {
                    format!(": {text}")
                }
            ),
        )),
    }
}

/// `describe_insert(clip)` — §3.8: "what is actually on an inserted card: its text, parameters, length.
pub fn describe_insert(clips: &[Seg], clip: u32) -> String {
    let Some(index) = (clip as usize).checked_sub(1) else {
        return crate::tools::error(&format!(
            "clip {clip} is not one of the {} clips in this edit",
            clips.len()
        ));
    };
    let Some(seg) = clips.get(index) else {
        return crate::tools::error(&format!(
            "clip {clip} is not one of the {} clips in this edit",
            clips.len()
        ));
    };
    if seg.ins.is_empty() {
        return crate::tools::error(&format!(
            "clip {clip} is footage, not an insert -- describe_insert answers about a card; \
             this clip's own transcript is in the brief"
        ));
    }
    let path = crate::cut_cards::split_path(&seg.ins);
    // §12 #39 ("the file's full path AND its ARGUMENTS"): the arguments come back spelled as the cut wrote them,
    // because they are what fills the card's holes — the writer cannot know `?S=` is a hole unless it is shown.
    let params: Vec<String> = path
        .args
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect();
    // Where the app put it, in the two spellings the cut uses: an absolute seconds-from-clip-start `offset=`, or
    // a named `at=` position.
    let placed = match path.args.iter().find(|(key, _)| key == "offset") {
        Some((_, seconds)) => format!("{seconds}s from the clip's start"),
        None => match path.args.iter().find(|(key, _)| key == "at") {
            Some((_, where_)) => format!("the {where_} of the frame"),
            None => "the head of the clip".to_string(),
        },
    };
    // §12 #28: the card's WORDS, read out of the document the app would draw — through the same door the
    // render goes (`produce_cards::draw`: a stamped card is redrawn from its arguments, an unstamped one has
    // its holes filled), then the `<text>` elements picked out of the result.
    let words = match std::fs::read_to_string(&path.file) {
        Ok(raw) => {
            let card = crate::produce_cards::draw(&seg.ins, &raw);
            crate::cut_cards::card_words(&card.drawn)
        }
        Err(_) => Vec::new(),
    };
    // #38: a card is not footage.
    let still = crate::produce_cards::is_card(&path.file) || seg.dur > 0.0;
    crate::tools::ok(&serde_json::json!({
        "clip": clip,
        "insert": path.file,
        "still": still,
        "params": params,
        "says": words,
        "placed_at": placed,
        "seconds_on_screen": on_screen(seg),
    }))
}

/// `finish` — §3.8: "names the clips with no answer and those whose lines will not fit, so they can be
/// rewritten shorter instead of squeezed by the render".
pub fn finish(answered: &[u32], unfitted: &[u32]) -> String {
    let total = answered
        .iter()
        .chain(unfitted.iter())
        .max()
        .copied()
        .unwrap_or(0);
    let mut missing: Vec<u32> = (1..=total).filter(|n| !answered.contains(n)).collect();
    let mut bad: Vec<u32> = unfitted.to_vec();
    bad.sort_unstable();
    bad.dedup();
    missing.sort_unstable();
    bad.retain(|n| !missing.contains(n));
    if missing.is_empty() && bad.is_empty() {
        return crate::tools::ok(&serde_json::json!({ "done": true }));
    }
    let mut parts: Vec<String> = Vec::new();
    if !missing.is_empty() {
        parts.push(format!(
            "no line and no silence for clip(s) {}",
            missing
                .iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !bad.is_empty() {
        parts.push(format!(
            "line(s) that will not fit, rewrite shorter rather than let the render squeeze them: clip(s) {}",
            bad.iter()
                .map(|n| n.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    crate::tools::error(&parts.join("; "))
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

/// Only ever forward.
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
