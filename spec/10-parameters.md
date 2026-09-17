# 10 — Parameters

Every value the prototype held as a constant, with its meaning and proposed home (`00-principles.md` §3). Values are the prototype's; a "measured" note records where the prototype's comments cite a measurement. REVIEW every home and default.

## 1. Machine settings (Settings dialog / llm.conf)
LLM server (empty = http://127.0.0.1:8731), LLM model, LLM key; audio.cpp server (8765), key, voices folder (/mnt/models/audiocpp/voices; Flatpak: data home), ASR model (nemotron-asr), diarization model (sortformer-diar), TTS model (index-tts2), separation model (bs-roformer), aligner (none → prefer qwen3-aligner); ffmpeg path (empty = PATH; ffprobe beside it); firefox path or "off"; sd.cpp server (1234), key; remembered last project per root. Proposed additions: subtitle languages list (en/eng/English, de/deu/German, fr/fra/French), style table, prompt max chars.

## 2. Editing policy (project; derived from the User Context by F0.7; else defaults)

| field | default | meaning | prototype constant |
|---|---|---|---|
| targetLengthSeconds | 0 (none) | finished length the cut aims at | ctxLength regex over the context |
| minTakeSeconds | 2.0 | a recording shorter than this is a start/stop, written up as silence, no server asked | shortTake |
| minSceneSeconds | 1.0 | shortest stretch worth suggesting, keeping or copying | minSegLn |
| minPieceSeconds | 0.04 | shortest remainder a removal may leave (about a frame) | minPieceLn |
| minClipSeconds | 0.5 | shortest clip the render makes; also the speed clamp floor | minClipLn |
| snapToleranceSeconds | 5.0 | how far a suggested edge may move to a word edge / silence / visual cut | snapTol |
| talkPadSeconds | 0.2 | how close to a word still counts as inside it | talkPad |
| seamMaxSeconds | 1.5 | a hole the model left between segments is closed when somebody talked in it | seamMax |
| deadAirMaxSeconds | 8.0 | a silence inside a clip longer than this is cut out | deadAirMax |
| deadAirKeepSeconds | 0.5 | the beat left where dead air was cut | deadAirKeep |
| suggestMinSegments | min(1 + target/30, 4) | fewer is refused | minSuggestSegs |
| suggestMaxSegments | max(target/5, 40) | more is refused | maxSuggestSegs |
| footageWindow | target × [0.6, 1.2] (≤ 60 s) else [0.6, 1.5], × [1, maxSpeedRate] | accepted footage for a target | suggestWindow, footageWindow |
| maxSpeedRate | 4.0 | how fast dull footage may be played when meeting a target | maxSpeedRate |
| speedGapSeconds | 4.0 | same-rate stretches nearer than this merge | speedGapMin |
| captionBatch | 5 | clips per caption request | captionBatch |
| captionMinSeconds | 0.3 | a shorter caption is dropped | caption floor |
| effectMinSurvivingSeconds | 1.0 | a clamped effect band shorter than this is dropped | clampFxToSegs |
| minRate / maxRate | 0.05 / 100 | speed effect clamp | fxMinRate/fxMaxRate |
| rampStepSeconds | 0.6 | one stair of a speed ramp on screen (measured: what renders) | rampStep |
| maxGain | 10 | volume effect ceiling (playbin's own) | fxMaxGain |
| effect default lengths | zoom/text/svg 3 s; stop/speed/volume/label 2 s | | |
| effect default fades | zoom 1; text/svg 0.3; volume 0.25; stop 0.5 | | |
| suggestedZoomHeight | 0.6 | a proposed zoom's height fraction, centred | fxFrom |
| reviewPadSeconds | 10 | ▶✂✂ plays this much before and after each join | reviewPad |
| insertDefaultSeconds | 4 | length of a still/card with no length of its own | insDefault |
| retakePauseSeconds | 1.5 | a pause worth drawing in the retake brief (measured: 139/141 gaps cluster 0.7–1.1 s) | retakePause |
| retakeRuns | 3 | identical retake calls pooled | retakeRuns |
| retakeReachSeconds | 180 | max gap between an attempt and its replacement | retakeReach |
| retakeMinSeconds | 0.3 | a shorter removal is a breath, not an attempt | retakeMin |
| retakeCeil | 0.4 | more than this share of speech called abandoned is refused | retakeCeil |
| retakeFragmentSeconds | 6.0 | longest line still a broken-off fragment (measured: tails under 2 s, longest 5) | retakeFrag |
| againReachSeconds | 25 | how much of the later take is read for the repeat | againReach |
| repeatSkip / repeatShare | 3 / 0.7 | fuzzy repeat match | repeatSkip, 0.7 |
| wordPadSeconds | 0.08 | room a cut leaves a word | wordPad |
| seamReachWords | 140 | words shown each side of a join (measured: failures at 63 and 71 abandoned words with 70) | seamReach |
| seamMaxWords / seamCeil | 40 / 0.6 | most a join may remove | seamMaxWords, seamCeil |
| seamSnapWords / seamNoiseWords | 3 / 2 | at-the-join tolerance; stretches this short elsewhere are respellings (measured: refusals 4→2, 5→2) | seamSnap, seamNoise |
| joinReachWords / keepReachWords | 3 / 200 | dedupe across a cut; backward match look-back | joinReach, keepReach |
| textEditThinking | on | thinking for the join pass (measured: 15/29 → 20/29 joins right, ~2 min a join) | |
| edgeReach / edgePad / lateStamp | 0.8 / 0.05 / 0.6 s | envelope-only edge placement | retake_edge |
| edgeTailDB / edgeTailMax / troughReach | 12 dB / 0.25 s / 0.4 s | word-fenced edge placement | retake_edge |
| describeFramesPerReq | 4 | frames per vision request | framesPerReq |
| describeRecentEvents | 3 | previous EVENT lines carried along | recentEvents |
| describeCtxSegs / describeCtxWindow | 2 / 10 s | speech context per side per source | ctxSegs, ctxWindow |
| describeFrameWidth | 896 | frame width sent to the vision model | scale=896:-2 |
| fixBlockLines | 25 | transcript lines per fixer request | fixBlock |
| fixContextSeconds | 5 | cross-source grounding window | ±5 s |
| fixTries | 2 | | |
| asr chunk (qwen3 / other / min) | 60 / 300 / 20 s | | asrChunkQwen/Max/Min |
| asrCutSeekSeconds | 20 | how far a chunk cut slides to a silence | asrCutSeek |
| silenceThresholdDB / silenceMinSeconds | −35 / 0.4 | silencedetect | asrQuietDB/Min |
| mergeGap / mergeMaxLen / mergeMaxWord / mergeNear | 0.7 / 12 / 2 / 1 s | transcript segment building | |
| diarWindows | 90, 45, 25 s | window ladder (measured: 90 passes, 150 fails) | diarWins |
| anchorPerSeconds / anchorMin / anchorCut / minAnchorOverlap / turnGap | 12 / 4 / 0.3 / 0.5 / 0.5 s | diarization anchoring | |
| alignChunkMax / Min / CutSeek / SoundPad | 60 / 15 / 4 / 0.25 s | alignment windows | |
| alignBareWarnSeconds / alignBareShare | 10 s / 0.08 | warning threshold | |
| sepChunkMax / sepLopsidedDB | 300 s / 10 dB | separation | |
| narrationMinWords / MaxWords / WordsPerSecond | 8 / 30 / 0.75 | per-clip word ceiling | narrBudget |
| narrationLead / Gap / Tail | 0.3 / 0.3 / 0.2 s | line packing | narrLead/Gap/Tail |
| narrationMaxExtend / MaxTempo | 4 s / 1.25 | fitting | maxExtend, maxTempo |
| narrationContextSeconds | 4 | transcript rows joining a clip's brief | ±4 s |
| narrationRunInSeconds | 3 | audition lead-in | narrRunIn |
| speechCharsPerSecond (default, min, max) | 15, 8, 28 | spoken-length estimate | speechRate |
| ttsLanguage | project language | REVIEW: prototype hard-coded "en" | |
| emotionAlpha | 0.85 | TTS judge path | emoAlpha |
| refMinTake / refPad / refWant / refTakeMax / refMinWordsPerSecond | 5 s / 2 s / 14 s / 3 / 1.5 | automatic voice reference | |
| takeMinSeconds | 0.4 | shortest hand-picked take | takeMin |
| pitchRangeSemitones | 6 | | pitchRange |
| refLoudness | I −16, TP −1.5, LRA 7 | reference levelling | refLoud |
| narratorSlots | 4 | | narratorSlots |
| gameVolume | 0.22 | game audio under the narration | GameVol |
| loudness | I −14, TP −1.5, LRA 11 | final mix | loudFlt |
| clipLimiter | −1 dBFS (0.891) | per-clip ceiling | clipCeil |
| subtitleBreak / RowChars / MaxSeconds / Hold / Min | 0.6 s / 42 / 6 s / 1.2 s / 0.8 s | cue building | subBreak… |
| translateBatch | 150 lines | REVIEW: new; prototype sent all | |
| publishFrames / publishMaxFrames | 3 / 8 | first-run frames; row cap | defPubFrames, maxPubFrames |
| thumbnailLongSide | 1280 | | pubLongSide |
| titleBand | {0.5, 0.25, 1, 0.4} | | pubTitleBox |
| thumbnailJPEGMax | 2 MiB | | pubJPEGMax |
| briefMaxChars | 120 000 | REVIEW: new bound on the upload brief | |
| blurSigma | 0.02·height, min 4 | frame-edge blur | blurSigma |
| keepSwearing | true | caption cleaning rule (from the prompt) | captionSystem |
| decorationDensity | "three or four per five minutes" | from the effects prompt | fxRules |
| styles | Lecture (joins, text cut), Gaming (retakes, model cut) | | styleRead/styleMoments |

## 3. Project settings (tab controls)
Frame interval (stops 0, 0.1, 0.2, 0.5, 1, 2, 3, 4, 5; default 1), frame scale (original, 896w (LLM), 480p, 720p, 1080p), language (en), style, chain ticks (Prepare), narration on/off, copy sources (on), the User Context; encoder settings (mp4, h264, CRF 24, veryslow, 1080, 30 fps, 128 kbit/s, subtitles none, no languages, VFR off, mono off, blurred edges on); publish state; aspect (source).

## 4. Prompts
system, describe, fix, retake, textedit, cut, captions, speed, effects, narrate, translate, youtube (`prompts/`); new: policy. Thinking on for textedit, cut, narrate, youtube; off elsewhere.

## 5. Engineering constants (fixed in code)
Window 1240×740; log heights 220/110; autosave 2 s; settings debounce 600 ms; typing debounce 400 ms; hang watchdog 200 ms / 3 s; tabGap 6, headSlack 90, name 28 chars; progress pulse 150 ms; checkpoint poll 200 ms; error tail 400 chars; log dir listing cap 12; frame extraction workers clamp(CPU/4, 2, 8), quality -q:v 4; ffprobe cache by path+size+mtime, fps window 1..240; timeline: ruler 18, selection band 22, effects row 26, lane gap 3, wave lane 30, gutter 30, thumb height 40..160, zoom 4 → 240 px/s, zoom step 1.25, tick steps, pan viewW/8, grab reaches 6/8/9/10/12 px, drag slop 4, scrub throttle 90 ms, tick 100 ms, preload lead 3 s, preload tolerance 0.01 s, rate seek gap 250 ms, thumb batch 6, card preview 8 fps / 960 px / 48 textures, svg preview 512 px, waveform 200 buckets/s at 8 kHz, dual-mono ratio 100, line save 1 s, scrollbar gear 40 px, undo depth 50, colours; text metrics 0.58 / 1.25 / 0.95 / 7 pt / 12 lines, edge dilation 0.08 / 0.85 / 16; card canvas 1920×1080 and timings, bake 25 fps; sd.cpp timeouts 15/60/30/1/10 s; web search 8 hits / 6000 chars / 15 s render / 45 s call / 15 s boot; LLM tokens 65536/8192, tool rounds 8, backoff 5 s–4 min, stall 5 min, whole 10 min, heartbeat 1 min, tail 90 chars, preview 110 chars; cache and log formats; TTS cache key format (frozen: changing it re-speaks every project).
