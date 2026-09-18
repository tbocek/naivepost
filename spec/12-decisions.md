# 12 — What the model decides, and what the app decides

<!-- nav -->
[← 11 Flow index and UI flow map](11-flow-index.md) · [↑ Contents](README.md) · <span>end →</span>

**In depth:** [Prepare](#prepare) · [Cut and effects](#cut-and-effects) · [Narrate and Produce](#narrate-and-produce)
<!-- /nav -->

Directive B ([`00-principles.md` §4](00-principles.md#4-rewrite-directive-b--tools-first)): every model job finishes by calling tools. This chapter is its audit: per model job (twelve), which decisions the model makes, which the app makes **inside a tool** (model is told), which **after `finish`** (the walk-back, invisible by design), and which are a policy number or prompt. It also names, per job, every place the prototype silently overrules the model — the rewrite must move these.

Read with [`02-services.md` §3](02-services.md#3-tool-catalogue-rewrite-directive-b) (catalogue) and [`10-parameters.md` §2](10-parameters.md#2-editing-policy-project-derived-from-the-user-context-by-f07-else-defaults) (numbers).

## 1. The five homes of a decision

| home | the model … | example |
|---|---|---|
| **tool argument** | states it | which lines are an abandoned attempt |
| **tool result** | is told what the app made of its answer; may act on it in the same conversation | a segment's snapped edges and how far they moved |
| **`finish` answer** | is told what is still wrong with the whole; may fix it | fewer segments than the target allows; a clip with no line |
| **after `finish`** (the walk-back) | never sees it — so it must be mechanical, explainable in one sentence, visible on screen afterwards | placing a cut edge on the audio envelope |
| **policy / prompt** | neither: a number or wording, not a per-item judgement | the 40 % ceiling on marks; "clean it like a subtitler" |

Rule of thumb: **if the app changes what the model said, the model hears about it** — per item in the tool's answer, for the whole at `finish`. The walk-back keeps only what rule 2.1 puts there: *models propose, the machine places.*

## 2. Coverage per job

| job | the model decides | the app decides, and says so | after `finish`, unsaid | policy |
|---|---|---|---|---|
| policy [F0.7](03-shell.md#f07-derive-the-editing-policy-review--new) | which fields the context speaks to | the field is known and in range | – | the defaults |
| describe [F1.7](04-prepare.md#f17-describe-per-footage-source-chunks-of-ppolicydescribeframesperreq--4) | what is on each frame; the running state | which frame an event landed on | folding "same" rows when the file is read | frames per request, context windows |
| fix [F1.8](04-prepare.md#f18-fix-the-transcripts-blocks-of-ppolicyfixblocklines--25-lines) | the wording of a line | the line is in this block; no tab | building `session.tsv` | block size, context seconds |
| retakes [F1.9](04-prepare.md#f19-mark-retakes-gaming-style) | which lines were abandoned, and where re-said | the stretch really removed after trimming and edge placement; running share against the ceiling | – | pause, reach, min, ceiling, runs |
| joins [F1.10](04-prepare.md#f110-repair-the-joins-lecture-style) | how the two takes read as one | which words that removes; whether one stretch at the join | dedupe across the cut; `final.txt` | window, snap, noise, max, ceiling |
| cut [F2.14](05-cut.md#f214-suggest-a-cut) | which stretches are worth keeping, and why | the snapped edges, the marks inside a segment, the running footage total | closing 1.5 s holes, dead air, coalescing | target, window, min/max segments, snap tolerance |
| captions [F3.9](06-effects.md#f39-captions-proposed-by-the-model-after-the-cut) | the words and where they sit | the caption as clamped into its clip, with its fades | clamping to the cut as applied | batch, minimum length |
| speed [F3.10](06-effects.md#f310-speeds-proposed-by-the-model) | how fast a dull stretch may run | the rate after clamping, and the on-screen length | merging same-rate stretches | min/max rate, merge gap |
| decorations [F3.11](06-effects.md#f311-decorations-proposed-by-the-model) | which moments deserve a zoom, a stop, a level | the effect as placed; the box a zoom will use | clamping to the cut as applied | density (prompt), defaults |
| narrate [F4.2](07-narrate.md#f42-the-narration-call) | what is said over each clip, and with what feeling | where the line lands, how many words fit, whether it will be sped up to fit | the render's final packing | words per second, lead/gap/tail, extend, tempo |
| upload [F5.6](08-produce.md#f56-upload-text-and-thumbnail) | title, description, thumbnail instruction or frame | the frame taken and its second | printing the title onto the picture | brief bound, frames, long side |
| translate [F5.4](08-produce.md#f54-subtitles) | the translation of each numbered line | the line is numbered and not empty | re-wrapping and re-timing the cue | batch size, cue geometry |

## 3. Where the prototype overrules the model — and where that decision moves

Each entry: what the prototype does behind the model's back → where it belongs in the rewrite.

### Prepare

1. **A retake mark is trimmed to a fuzzy-matched word suffix** (≥ 70 % of the tail matching, ≤ 3 words skipped, one edit allowed): model marked whole lines, app cuts inside a line at its own boundary → `mark_abandoned`'s **result**: the stretch that will really go.
2. **Both edges of every mark are then moved onto the audio envelope** — the actual cut is decided in dBFS buckets → stays **after `finish`** (rule 2.2), but the placed seconds are in the same tool result, so the model sees them.
3. **A whole take has its `again` rewritten to 0; a rephrase is trimmed to the broken-off fragment; a refused mark is re-heard by a second ASR pass and sometimes resurrected** → the **result**, each with a one-sentence reason.
4. **Over the ceiling, every mark from all three runs is thrown away** → `finish`'s answer, with the running total, so the model can take marks back instead.
5. **The join pass's numbers are reverse-engineered from prose, then snapped up to 3 words to the join and extended to it; stretches ≤ 2 words elsewhere are re-read as respellings** → `drop_words` takes the counts as **arguments** and answers with the words that will go.
6. **`dedupeJoins` deletes words the model deliberately kept** → stays after `finish` (a rule about the cut, not the sentence); named in the log.
7. **A transcript block of 25 lines is thrown away whole when one row's time, speaker or tab does not match — and re-asked identically, twice, with no word about why** → `fix_line` validates **per line**; `flag_line` lets the model say "this speaker is wrong" (the prototype forbids fixing it and offers no way to report).
8. **Describe: a stamped line further than half an interval from any frame is discarded; an unparseable offset is dropped; an unlabelled reply is filed as the description; a multi-line answer is flattened; a batch's first frame is force-written "Calm; same view."** → `record_event` answers with the frame it landed on; `finish` names frames still empty.
9. **Two lines of speech context a side per source, and the model cannot tell "nobody spoke" from "your window was too small"** → `speech_around`.
10. **Retries never tell the model it is being re-asked**: the same messages go out again, so a rejected answer cannot be corrected → the tool loop *is* the correction (`09` [F6.1](09-llm-and-tools.md#2-tool-protocol-f61)); only a transport retry is silent.

### Cut and effects

11. **Every segment edge is moved up to five seconds** by a weighted contest the model never sees (silence midpoint 0.8, word edge 0.9, line edge 0.95, visual cut up to 1.0, distance penalty, outward bonus) → `add_segment`'s **result**: the snapped edges and how far each moved.
12. **Segments with no footage under an edge are dropped, and the model is then rejected for the resulting length** — corrected for arithmetic it got right → `add_segment` refuses the segment *when added*; `cut_status` keeps the running total honest.
13. **Silences over eight seconds are cut out of kept clips**, on the stated assumption the model will not → stays after `finish`; a deliberate long beat is a policy number (`P.policy.deadAirMaxSeconds`), not a silent rule. REVIEW: the cut brief SHOULD state the number, so the model can keep a beat by asking.
14. **Every zoom the model proposes is a centred punch-in at 60 % height**: the reply shape has no box and the pass sees no pictures — yet the prompt asks to zoom "onto the score, the face, the mistake" → `add_effect` takes a `box`; `get_frames(clip)` lets the pass look before aiming. The single widest gap between what the prompt asks and what the app accepts.
15. **A rate over 1 on a captioned clip is dropped in silence**, though the prompt already asked for it and the caption list came from another call → `set_clip_speed` **errors**, naming the caption.
16. **Kinds outside zoom/stop/volume, volume gains of exactly 1, captions under 0.3 s and empty texts are dropped without a word** → each is that item's error.
17. **A speed with no rate becomes 0.5; a stop with no span 2 s; a caption with no span 3 s; fades and ramps are invented** → stay app defaults (taste the prompt does not ask about), but the tool result states what was applied.
18. **The whole effect list is replaced, discarding hand-placed labels and zooms** → stays; the page's contract with the user (one Undo), not a model decision. Named in [`05-cut.md`](05-cut.md) [F2.14](05-cut.md#f214-suggest-a-cut) S3.
19. **Effects are clamped to the cut as applied and dropped under a second of survivor; only a count reaches the log** → stays after `finish` (the cut moved under them), but the count becomes a line the user can act on.
20. **Web tools are withdrawn after the first rejection, whatever it was, and thinking is switched off after an all-reasoning reply — both unannounced** → both stay the app's, but logged; a rewrite SHOULD tell the model the tools are gone rather than leave it looking.
21. **The target length is a regex over free text with no fallback**: "half an hour" silently becomes no target, changing the prompt, both count gates and the length gate at once → `P.policy.targetLengthSeconds`, derived by [F0.7](03-shell.md#f07-derive-the-editing-policy-review--new)'s `set_policy`, shown in the policy form.

### Narrate and Produce

22. **One clip skipped, one echoed bound more than half a second out, or entries out of clip order rejects the whole reply** — three times, then the run fails with nothing written: thirty-nine good clips lost for the fortieth → `write_line` takes one clip at a time; `finish` names clips still unanswered.
23. **Placement is overridden three ways at render time**: an overrunning line pushes every following line later, the whole schedule slides earlier, and speech is sped to P.policy.narrationMaxTempo — so the **last** line on a clip falls off, usually the punchline the prompt asked to land → `write_line`'s **result** says where the line lands and whether it will be squeezed; `finish` names clips that will not fit, so they can be rewritten shorter. The page already warns the user; only the model is kept in the dark.
24. **The hand-curated "this clip plays its own audio" list is wiped on every ▶** — the user's decisions destroyed by the next narration run → REVIEW ([`07-narrate.md`](07-narrate.md) [F4.1](07-narrate.md#f41--write-and-speak)): the silent list is the user's, not the model's, and MUST survive a rewrite of the lines.
25. **An emotion tag with a weight is sent at alpha 1, a bare one at 0.85** — same intent, two performances, neither number told to the model; **and one unknown name in a weighted tag discards the whole tag**, recognised parts included, re-routing the line to a different model → `write_line` answers with the emotion as resolved; `list_emotions` publishes the table.
26. **A `pos` outside five known words silently becomes bottom**, though the captions addendum asks for one on every entry → an error on `write_line`.
27. **Entries are re-sorted by (clip, offset) while out-of-order clips are rejected** — two opposite policies for one mistake class → one policy: the tool takes any order.
28. **An inserted card is described to the writer by file name alone** (`tier.svg?S=Dust II`), though the app rendered it and knows its text → `describe_insert`.
29. **The narration brief carries only `label` and `text` effects**: a zoom, an SVG burned over the picture, a volume mute — all invisible to the writer → pass every kind.
30. **The upload brief carries no effects and no clip rate**: a moment marked "the reveal" reaches the description writer as nothing; a 4× clip's stamps are on a clock it is not told about → pass the effects, print the rate.
31. **The word ceiling is advisory**: printed in the brief, never counted → `write_line` reports the count against it.
32. **A named thumbnail frame resolves to the nearest extracted frame at any distance** — "clip 3 +12" can land twenty seconds away; the log prints the frame's time, not the request's → `pick_frame` answers with the distance.
33. **Naming a frame clears the instruction; an unresolvable frame leaves neither** → two tools, each with its own answer; an error instead of a silent clearing.
34. **The title is printed onto the picture only if it was the first title to exist**; later rewrites leave the picture's words alone → REVIEW ([`08-produce.md`](08-produce.md) [F5.6](08-produce.md#f56-upload-text-and-thumbnail)).
35. **A title too long for its band overflows rather than being refused** — "four to seven words" is unenforced advice → `set_title` answers with the fitted size, or the warning.
36. **A missing `TITLE:` or `THUMBNAIL:` keeps the previous run's value**, attributing last run's answer to this one → deliberate (an empty box is easier to notice than a wrong line), now stated.
37. **Subtitle placement tags go into the translation as words, unchecked on the way back**, and **the model's line breaks are re-wrapped at 42 characters against the prompt's own instruction** → the tool answers with the wrapped line; the app holds the tag out of the text and puts it back.
38. **A line that cannot be translated is silently replaced with the original language** and shipped in the other language's track → kept (one bad line must not drop a whole track), but named in the log and in `finish`'s answer.

## 4. What stays out of the model's hands, on purpose

Each is mechanical, checkable and visible on screen afterwards; none is a judgement about the video:

- **Word times.** The aligner's, never the model's (rule 2.1); a model is never asked to compute a timestamp.
- **Where a cut lands between two known words.** The audio envelope chooses; the model chooses which words (rule 2.2).
- **The session clock**, source placement and hand shift corrections.
- **Edge snapping, hole closing, dead-air removal, coalescing, clamping** — the walk-back (rule 2.6). Runs after `finish` on every reply, model- or hand-made; the page shows what it did.
- **The render's arithmetic**: fitting, ducking, loudness, frame boxes, cue wrapping.
- **Which marking pass a style runs**, and whether a job is asked at all (the Lecture cut asks no model; the text decides).

## 5. Gaps this audit closed

Added to the catalogue ([`02-services.md` §3](02-services.md#3-tool-catalogue-rewrite-directive-b)): `get_frames` (promised in the principles chapter, never defined), `speech_around`, `flag_line`, `get_words`, `cut_status`, `list_emotions`, `describe_insert`, and a `box` argument on `add_effect`. Every job now names its read tools as well as write tools — before, only the policy job had one, so elsewhere the brief silently bounded what the model could be right about. Every write tool's result now carries what the app made of the item, not just "ok".

Four prototype behaviours are marked defects, not designs: the narration run wiping the user's silent list (§3.24, **MUST fix**), the subtitle placement tag travelling through the translation as a word (§3.37, **MUST fix**), the thumbnail title printed only once (§3.34, REVIEW), and the three disagreeing readings of a narration line's `at` on a sped-up clip ([`01-project-and-files.md` §4](01-project-and-files.md#4-narratenarrationjson), REVIEW).

<!-- nav -->
---
[← 11 Flow index and UI flow map](11-flow-index.md) · [↑ top](#12--what-the-model-decides-and-what-the-app-decides) · [↑ Contents](README.md) · <span>end →</span>
<!-- /nav -->
