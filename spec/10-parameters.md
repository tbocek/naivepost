# 10 — Parameters

<!-- nav -->
[← 09 The LLM client, tools, gate, cache, log](09-llm-and-tools.md) · [↑ Contents](README.md) · [11 Flow index and UI flow map →](11-flow-index.md)
<!-- /nav -->

Every prototype constant, with meaning and proposed home ([`00-principles.md` §3](00-principles.md#3-rewrite-directive-a--no-implicit-behaviour)). Values are the prototype's; a "measured" note marks where its comments cite a measurement. REVIEW every home and default.

## 1. Machine settings (Settings dialog / llm.conf)

LLM server (empty = `http://127.0.0.1:8731`), LLM model, LLM key; audio.cpp server (8765), key, voices folder (/mnt/models/audiocpp/voices; Flatpak: data home), ASR model (nemotron-asr), diarization model (sortformer-diar), TTS model (index-tts2), separation model (bs-roformer), aligner (none → prefer qwen3-aligner); ffmpeg path (empty = PATH; ffprobe beside it); firefox path or "off"; sd.cpp server (1234), key; remembered last project per root. Proposed additions: subtitle languages list (en/eng/English, de/deu/German, fr/fra/French), P.machine.slots (REVIEW, new; one count per model — LLM, each audio.cpp model, image model — default 1: requests to that model on the wire at once, the rest wait their turn; [09 §4](09-llm-and-tools.md#4-liveness-and-the-gate-f63); prototype: the LLM fixed at one in code, the others unlimited), P.machine.describeFrameWidth (REVIEW, new; 896 — the width every frame is scaled to before it goes to the vision model, height by aspect: a 16:9 frame is 896×504. A setting of the model, not of the edit: Qwen-VL reads images in 28-px patches and 896 = 32 × 28, so a 16:9 frame is 576 patches with nothing padded; Gemma 3 rescales to 896×896 anyway; Claude, GPT and LLaVA resize or tile to their own sizes, and 896 wide sits inside what they take. Prototype: fixed in code, `scale=896:-2`), and two bounds (REVIEW, new): P.machine.promptMaxChars (120 000, most one request may carry, [`09-llm-and-tools.md` §5](09-llm-and-tools.md#5-context-budgets-review--new)) and P.machine.briefMaxChars (120 000, the upload brief).

## 2. Editing policy (project; derived from the User Context by F0.7; else defaults)

One row per parameter; chapters cite the first column. Prototype constant named for cross-checking ([`inventory/`](inventory)).

| parameter | default | meaning | prototype constant |
|---|---|---|---|
| P.policy.targetLengthSeconds | 0 (none) | finished length the cut aims at | ctxLength regex over the context |
| P.policy.shortTargetSeconds | 60 | a target up to this is a format (footage window narrows to ×[0.6, 1.2]); above it a wish | shortTarget |
| P.policy.minTakeSeconds | 2.0 | a shorter recording is a start/stop: written up as silence, no server asked | shortTake |
| P.policy.minSceneSeconds | 1.0 | shortest stretch worth suggesting, keeping or copying | minSegLn |
| P.policy.minPieceSeconds | 0.04 | shortest remainder a removal may leave (about a frame) | minPieceLn |
| P.policy.minClipSeconds | 0.5 | shortest clip the render makes; also the speed clamp floor | minClipLn |
| P.policy.snapToleranceSeconds | 5.0 | how far a suggested edge may move to a word edge / silence / visual cut | snapTol |
| P.policy.talkPadSeconds | 0.2 | how close to a word still counts as inside it | talkPad |
| P.policy.seamMaxSeconds | 1.5 | a hole the model left between segments closes when somebody talked in it | seamMax |
| P.policy.deadAirMaxSeconds | 8.0 | a longer silence inside a clip is cut out | deadAirMax |
| P.policy.deadAirKeepSeconds | 0.5 | the beat left where dead air was cut | deadAirKeep |
| P.policy.suggestMinSegments | min(1 + ⌊target/30⌋, 4) | fewer is refused | minSuggestSegs |
| P.policy.suggestMaxSegments | max(⌊target/5⌋, 40) | more is refused | maxSuggestSegs |
| P.policy.suggestFallbackSegments | 20 | denominator for a reply whose segments have no readable end (progress only) | cut.go suggestMaxSegs |
| P.policy.footageWindow | target × [0.6, 1.2] (≤ shortTargetSeconds) else [0.6, 1.5]; ceiling × maxSpeedRate | accepted footage for a target | suggestWindow, footageWindow |
| P.policy.maxSpeedRate | 4.0 | fastest dull footage may play to meet a target | maxSpeedRate |
| P.policy.maxProposedEffects | 1000 | cap on effects per cut reply (effectively none: the prompt decides) | fxMaxProposed |
| P.policy.speedGapSeconds | 4.0 | same-rate stretches nearer than this merge | speedGapMin |
| P.policy.captionBatch | 5 | clips per caption request | captionBatch |
| P.policy.captionMinSeconds | 0.3 | a shorter caption is dropped | caption floor |
| P.policy.effectMinSurvivingSeconds | 1.0 | a clamped effect band shorter than this is dropped | clampFxToSegs |
| P.policy.minRate | 0.05 | speed effect clamp, floor | fxMinRate |
| P.policy.maxRate | 100 | speed effect clamp, ceiling | fxMaxRate |
| P.policy.rampStepSeconds | 0.6 | one stair of a speed ramp on screen (measured: what renders) | rampStep |
| P.policy.maxGain | 10 | volume effect ceiling (playbin's own) | fxMaxGain |
| P.policy.effectDefaultSeconds | zoom/text/svg 3; stop/speed/volume/label 2 | length of an effect placed by hand |  |
| P.policy.effectDefaultFades | zoom 1; text/svg 0.3; volume 0.25; stop 0.5 | fade of an effect placed by hand |  |
| P.policy.effectMinSeconds | 0.1 | shortest an effect band may be dragged down to | cut_fx.go 0.1 |
| P.policy.effectMinSelectionSeconds | 0.2 | timeline needed under the band before ⏩ Speed treats it as a chosen stretch | cut_fx.go 0.2 |
| P.policy.aspectStaySeconds | 1.0 | length of the staying zoom an aspect change places | cut_fx.go 1.0 |
| P.policy.suggestedZoomHeight | 0.6 | a proposed zoom's height fraction, centred | fxFrom |
| P.policy.soundDipSeconds | 0.15 | fade down into a sound close and back out (short enough not to hear, long enough not to click) | sndDip |
| P.policy.soundMinPieceSeconds | 0.05 | under this a sound-over piece is not worth a segment of its own | sndMinLn |
| P.policy.reviewPadSeconds | 10 | ▶✂✂ plays this much before and after each join | reviewPad |
| P.policy.insertDefaultSeconds | 4 | length of a still/card with no length of its own | insDefault |
| P.policy.retakePauseSeconds | 1.5 | a pause worth drawing in the retake brief (measured: 139/141 gaps cluster 0.7–1.1 s) | retakePause |
| P.policy.retakeRuns | 3 | identical retake calls pooled | retakeRuns |
| P.policy.retakeReachSeconds | 180 | max gap between an attempt and its replacement | retakeReach |
| P.policy.retakeMinSeconds | 0.3 | a shorter removal is a breath, not an attempt | retakeMin |
| P.policy.retakeCeil | 0.4 | refused when more than this share of speech is called abandoned | retakeCeil |
| P.policy.retakeFragmentSeconds | 6.0 | longest line still a broken-off fragment (measured: tails under 2 s, longest 5) | retakeFrag |
| P.policy.againReachSeconds | 25 | how much of the later take is read for the repeat | againReach |
| P.policy.repeatSkip | 3 | fuzzy repeat match: words skipped | repeatSkip |
| P.policy.repeatShare | 0.7 | fuzzy repeat match: share that must match | 0.7 |
| P.policy.dressReachWords | 8 | how far ahead in the transcript a word is sought when dressing aligner output | retake.go dressReach |
| P.policy.respellRunReachWords | 6 | max distance between two spellings of one stretch before the re-dressing walk gives up | subwords.go runReach |
| P.policy.wordPadSeconds | 0.08 | room a cut leaves a word | wordPad |
| P.policy.edgeReachSeconds | 0.8 | envelope-only edge placement: search reach | retake_edge |
| P.policy.edgePadSeconds | 0.05 | envelope-only edge placement: pad | retake_edge |
| P.policy.lateStampSeconds | 0.6 | envelope-only edge placement: how late a stamp may be | retake_edge |
| P.policy.envelopeWindowSeconds | 4 | envelope searched each side of a stamp (±400 buckets) | retake_edge.go 400 |
| P.policy.edgeTailDB | 12 | word-fenced edge placement: tail threshold | retake_edge |
| P.policy.edgeTailMaxSeconds | 0.25 | word-fenced edge placement: longest tail | retake_edge |
| P.policy.troughReachSeconds | 0.4 | word-fenced edge placement: trough search | retake_edge |
| P.policy.seamReachWords | 140 | words shown each side of a join (measured: failures at 63 and 71 abandoned words with 70) | seamReach |
| P.policy.seamMaxWords | 40 | most words a join may remove | seamMaxWords |
| P.policy.seamCeil | 0.6 | most of the shown words a join may remove | seamCeil |
| P.policy.seamSnapWords | 3 | at-the-join tolerance (measured: refusals 4→2) | seamSnap |
| P.policy.seamRetries | 1 | a refused join is asked again this many times (new 2026-09-18) | seamRetries |
| P.policy.strayWordRatio | 10 | a word whose loudest moment is under 1/N of its recording's median word is "on no sound" (F1.13; measured: 1 of 18 155 words in three lectures) | strayRatio |
| P.policy.strayWordGapSeconds | 1.0 | …and it must start this long after the word before it | strayGapSeconds |
| P.policy.seamNoiseWords | 2 | stretches this short elsewhere are respellings (measured: 5→2) | seamNoise |
| P.policy.joinReachWords | 3 | dedupe across a cut | joinReach |
| P.policy.keepReachWords | 200 | backward match look-back | keepReach |
| P.policy.textEditThinking | on | thinking for the join pass (measured: 15/29 → 20/29 joins right, ~2 min a join) |  |
| P.policy.describeFramesPerReq | 4 | frames per vision request | framesPerReq |
| P.policy.describeRecentEvents | 3 | previous EVENT lines carried along | recentEvents |
| P.policy.describeCtxSegs | 2 | speech context: segments per side per source | ctxSegs |
| P.policy.describeCtxWindowSeconds | 10 | speech context: window per side per source | ctxWindow |
| P.policy.sceneThreshold | 1.0 | scdet score, on the 4-per-second stream, over which a frame starts a new scene (measured: 6 of 8 slide changes found; the source-rate pass found 3) | new |
| P.policy.sceneMinGapSeconds | 0.5 | scene changes nearer than this merge into the first | new |
| P.policy.fixBlockLines | 25 | transcript lines per fixer request | fixBlock |
| P.policy.fixContextSeconds | 5 | cross-source grounding window | ±5 s |
| P.policy.fixTries | 2 | attempts per block | try < 2 |
| P.policy.asrSampleRate | 16000 | the rate ASR and diarization are trained on; everything resampled to it | sampleRate |
| P.policy.asrChunkQwenSeconds | 60 | ASR chunk for qwen3 | asrChunkQwen |
| P.policy.asrChunkMaxSeconds | 300 | ASR chunk for other models | asrChunkMax |
| P.policy.asrChunkMinSeconds | 20 | shortest ASR chunk | asrChunkMin |
| P.policy.asrCutSeekSeconds | 20 | how far a chunk cut slides to a silence | asrCutSeek |
| P.policy.silenceThresholdDB | −35 | silencedetect threshold | asrQuietDB |
| P.policy.silenceMinSeconds | 0.4 | silencedetect minimum | asrQuietMin |
| P.policy.mergeGapSeconds | 0.7 | transcript segment building: gap that ends a segment |  |
| P.policy.mergeMaxSeconds | 12 | transcript segment building: longest segment |  |
| P.policy.mergeMaxWordSeconds | 2 | transcript segment building: longest word |  |
| P.policy.mergeNearSeconds | 1 | transcript segment building: near join |  |
| P.policy.diarWindowsSeconds | 90, 45, 25 | window ladder (measured: 90 passes, 150 fails) | diarWins |
| P.policy.diarHopShare | 2/3 | pass 1's stride as share of the window (each voice need only be seen once) | diarHopShare |
| P.policy.anchorPerSeconds | 12 | seconds of each voice in the anchor, at the 90 s window (a quarter of the window at the lower rungs) | anchorPer |
| P.policy.anchorMinSeconds | 4 | speech that makes a slot count as a voice | anchorMin |
| P.policy.anchorCutSeconds | 0.3 | shortest stretch of one voice worth cutting into the anchor | anchorCut |
| P.policy.minAnchorOverlap | 0.5 | anchor-block overlap to claim a slot | minAnchorOv |
| P.policy.turnGapSeconds | 0.5 | gap that ends a turn | diarTurnGap |
| P.policy.alignChunkMaxSeconds | 60 | alignment window | alignChunkMax |
| P.policy.alignChunkMinSeconds | 15 | shortest alignment window | alignChunkMin |
| P.policy.alignCutSeekSeconds | 4 | how far an alignment cut slides to a silence | alignCutSeek |
| P.policy.alignSoundPadSeconds | 0.25 | sound kept around an alignment piece | alignSoundPad |
| P.policy.alignClipMaxSeconds | 120 | sanity ceiling on a returned second: past it, the number was not seconds | alignClipMax |
| P.policy.alignBareWarnSeconds | 10 | voiced audio under no word before a warning | alignBareWarn |
| P.policy.alignBareShare | 0.08 | share of voiced audio under no word before a warning | alignBareShare |
| P.policy.frameWorkersMinFrames | 4 | frames per worker below which extraction is one ffmpeg | pipeline.go workers*4 |
| P.policy.sepChunkMaxSeconds | 300 | separation chunk | sepChunkMax |
| P.policy.sepCutSeekSeconds | 20 | how far a separation cut slides to a silence | sepCutSeek |
| P.policy.sepLopsidedDB | 10 | a stem this much quieter is dropped | sepLopsidedDB |
| P.policy.narrationMinWords | 8 | per-clip word floor | narrBudget |
| P.policy.narrationMaxWords | 30 | per-clip word ceiling | narrBudget |
| P.policy.narrationWordsPerSecond | 0.75 | words a clip second affords | narrBudget |
| P.policy.narrationLeadSeconds | 0.3 | line packing: lead | narrLead |
| P.policy.narrationGapSeconds | 0.3 | line packing: gap between lines | narrGap |
| P.policy.narrationTailSeconds | 0.2 | line packing: tail | narrTail |
| P.policy.narrationMaxExtendSeconds | 4 | fitting: most a clip is extended for its line | maxExtend |
| P.policy.narrationMaxTempo | 1.25 | fitting: fastest a line is spoken | maxTempo |
| P.policy.narrationContextSeconds | 4 | transcript rows joining a clip's brief | ±4 s |
| P.policy.narrationRunInSeconds | 3 | audition lead-in | narrRunIn |
| P.policy.speechCharsPerSecond | 15 (8..28) | spoken-length estimate: default, floor, ceiling | speechRate |
| P.policy.ttsLanguage | project language | REVIEW: prototype hard-coded "en" |  |
| P.policy.emotionAlpha | 0.85 | TTS judge path | emoAlpha |
| P.policy.refMinTakeSeconds | 5 | automatic voice reference: shortest take | refMinLen |
| P.policy.refPadSeconds | 2 | automatic voice reference: pad | refPad |
| P.policy.refWantSeconds | 14 | automatic voice reference: length wanted | refWant |
| P.policy.refTakeMax | 3 | automatic voice reference: takes joined | refTakeMax |
| P.policy.refMinWordsPerSecond | 1.5 | automatic voice reference: a slower take is not speech | refMinRate |
| P.policy.refSampleRate | 48000 | voice reference write rate (above it ffmpeg writes WAVE_FORMAT_EXTENSIBLE, which the server refuses) | narrate_voice.go 48000 |
| P.policy.takeMinSeconds | 0.4 | shortest hand-picked take | takeMin |
| P.policy.pitchRangeSemitones | 6 |  | pitchRange |
| P.policy.refLoudness | I −16, TP −1.5, LRA 7 | reference levelling | refLoud |
| P.policy.narratorSlots | 4 |  | narratorSlots |
| P.policy.gameVolume | 0.22 | game audio under the narration | GameVol |
| P.policy.loudness | I −14, TP −1.5, LRA 11 | final mix | loudFlt |
| P.policy.clipLimiter | −1 dBFS (0.891) | per-clip ceiling | clipCeil |
| P.policy.laneMinMixSeconds | 0.1 | shortest lane overlap worth an ffmpeg input | produce.go 0.1 |
| P.policy.subtitleBreakSeconds | 0.6 | cue building: a pause that breaks a cue | subBreak |
| P.policy.subtitleRowChars | 42 | cue building: characters per row | subRowChars |
| P.policy.subtitleMaxSeconds | 6 | cue building: longest cue | subMax |
| P.policy.subtitleHoldSeconds | 1.2 | cue building: hold | subHold |
| P.policy.subtitleMinSeconds | 0.8 | cue building: shortest cue | subMin |
| P.policy.translateBatch | 150 | lines per translation request — REVIEW: new; prototype sent all |  |
| P.policy.publishFrames | 3 | first-run frames | defPubFrames |
| P.policy.publishMaxFrames | 8 | row cap | maxPubFrames |
| P.policy.thumbnailLongSide | 1280 |  | pubLongSide |
| P.policy.titleBand | {0.5, 0.25, 1, 0.4} |  | pubTitleBox |
| P.policy.thumbnailJPEGMax | 2 MiB |  | pubJPEGMax |
| P.policy.blurSigma | 0.02·height, min 4 | frame-edge blur | blurSigma |
| P.policy.keepSwearing | true | caption cleaning rule (from the prompt) | captionSystem |
| P.policy.decorationDensity | "three or four per five minutes" | from the effects prompt | fxRules |
| P.policy.markingPass | retakes | which Prepare marking pass runs: joins ([F1.10](04-prepare.md#f110-repair-the-joins)), retakes ([F1.9](04-prepare.md#f19-mark-retakes)) or none — from the User Context | styleRead/styleMoments |
| P.policy.cutMode | model | how Suggest builds the cut: words (derived from the marked text, no model) or model | styleRead/styleMoments |
| P.policy.captionsPass | on | whether [F3.9](06-effects.md#f39-captions-proposed-by-the-model-after-the-cut) is called at all | new |
| P.policy.speedPass | on | whether [F3.10](06-effects.md#f310-speeds-proposed-by-the-model) is called at all | new |
| P.policy.decorationsPass | on | whether [F3.11](06-effects.md#f311-decorations-proposed-by-the-model) is called at all | new |

## 3. Project settings (tab controls)

P.project.frameInterval (Freq: seconds between frames sent to the describe model, counted from each scene change; stops 0.25, 0.5, 1, 2, 3, 4, 5; default 1 — prototype: the extraction interval, stops 0, 0.1, 0.2, 0.5, 1 … 5), language (en), narration on/off, copy sources (on), the User Context; encoder settings (mp4, h264, CRF 24, veryslow, 1080, 30 fps, 128 kbit/s, subtitles none, no languages, VFR off, mono off, blurred edges on); publish state; aspect (source).

## 4. Prompts

system, describe, fix, retake, textedit, cut, captions, speed, effects, narrate, translate, youtube ([`prompts/`](prompts)); new: policy. Thinking on for textedit, cut, narrate, youtube; off elsewhere.

## 5. Engineering constants (fixed in code)

Those the chapters cite by name:

| constant | value | meaning |
|---|---|---|
| P.eng.llmToolRounds | 8 | tool rounds per call (`09` [F6.1](09-llm-and-tools.md#2-tool-protocol-f61)) |
| P.eng.llmAttempts | 3 | validation attempts per job (cut, narrate, upload text) |
| P.eng.llmStallMinutes | 5 | a streamed call with no byte for this long is given up |
| P.eng.llmWholeMinutes | 10 | ceiling on an unstreamed call |
| P.eng.llmTailChars | 90 (360 kept) | heartbeat tail shown / kept of a streaming tail |
| P.eng.searchHits | 8 | hits per web query |
| P.eng.preloadLeadSeconds | 3 | preview: how far ahead the next clip is opened (tolerance 0.01 s) |
| P.eng.frameGridSeconds | 0.25 | extraction grid, restarted at every scene change: at the deepest zoom (240 px/s) one frame is 60 px, so the picture band has no black gaps (new) |
| P.eng.audiocppUnloadSeconds | 20 | the only deadline on an audio.cpp call (`unload_all_models`) |

The rest, by area:
Window 1240×740; log heights 220/110; autosave 2 s; settings debounce 600 ms; typing debounce 400 ms; hang watchdog 200 ms / 3 s; tabGap 6, headSlack 90, name 28 chars; progress pulse 150 ms; checkpoint poll 200 ms; error tail 400 chars; log dir listing cap 12; frame extraction workers clamp(CPU/4, 2, 8), frame count round(length/interval), a lone last frame folded into the chunk before it, quality -q:v 4; ffprobe cache by path+size+mtime, fps window 1..240; timeline: ruler 18, selection band 22, effects row 26, lane gap 3, wave lane 30, gutter 30, thumb height 40..160 (64 at open, steps ×4/3 and ÷3/4), zoom 4 → 240 px/s (4 at open), zoom step 1.25, fold gutter 30 px with fold/kill/hear/selection minimum widths derived from the grab reaches, selection band ground 0.16 and floor 0.04 s, tick steps, pan viewW/8, grab reaches 6/8/9/10/12 px, drag slop 4, scrub throttle 90 ms, tick 100 ms, preload lead 3 s, preload tolerance 0.01 s, rate seek gap 250 ms, thumb batch 6, card preview 8 fps / 960 px / 48 textures, svg preview 512 px, waveform 200 buckets/s at 8 kHz, dual-mono ratio 100, line save 1 s, scrollbar gear 40 px, undo depth 50, colours; effect bars: label sizing 10 word / 5 number chars, text height 62, ends become handles from 30 px, row packing gives width from 0.4 s, hold slack 1/24 s; Narrate take band: ruler 12, lane 56, max 200 px/s, click slop 3 px; publish page: icon 22 px, box min 28, mono line 20; tier card: 6 slots, top 180, row 140, box 124, chips 338/244/230/108, pad 14; dual-mono: envelope average ≤ 1.0 and max ≤ 8 bytes also read as one lane; progress shares: Prepare inputs 0.3 (separation 0.1 of it), Suggest choose 0.7, Narrate write 0.5; text metrics 0.58 / 1.25 / 0.95 / 7 pt / 12 lines, edge dilation 0.08 / 0.85 / 16; card canvas 1920×1080 and timings, bake 25 fps; sd.cpp timeouts 15/60/30/1/10 s; web search 8 hits / 6000 chars / 15 s render / 45 s call / 15 s boot; LLM tokens 65536/8192, tool rounds 8, backoff 5 s–4 min, stall 5 min, whole 10 min, heartbeat 1 min, tail 90 chars, preview 110 chars; cache and log formats; TTS cache key format (frozen: changing it re-speaks every project).

<!-- nav -->
---
[← 09 The LLM client, tools, gate, cache, log](09-llm-and-tools.md) · [↑ top](#10--parameters) · [↑ Contents](README.md) · [11 Flow index and UI flow map →](11-flow-index.md)
<!-- /nav -->
