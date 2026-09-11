# Native AI Video Editor for Post Processing, in short: Naivepost

A desktop video editor for sessions you have already recorded, where the
tedious half of the work is done by models running on your own machine.

You give it the files a session left behind — camera footage, a separate
microphone recording, a screen capture — and it transcribes them, works out
what is on screen, proposes a cut, writes and speaks a narration over it,
renders the video, and writes the title, description, subtitles and thumbnail
you upload with. Every one of those is a proposal you can overrule; none of
them is a black box you have to accept whole.

It is a GTK4 application in Go (`gui/`, ~100k lines), talking to four local
HTTP servers over the network — nothing is compiled in, nothing calls out to a
cloud service unless you point it at one.

---

## What it actually does

Four tabs, left to right, in the order the work happens. Each has one **▶** that
runs that step; **▶▶** beside it runs several in a row. The thumbnail and the
upload text live on Produce, beside the render they belong to.

### Prepare

Add the session's files. Each row says what its file is *for*: a camera icon
means frames come out of it and it can be cut; a microphone tags who is
speaking, voice 1 being the one the narration is spoken in. One file can be
both.

Everything is placed on **one clock**, taken from the timestamp in the filename
or the file's own creation time and length. That is what puts a separate
recorder's words, waveform and sound where they belong against the footage. The
footage is the master: a recording is heard for exactly the part of it that was
running while the footage was.

Pressing ▶ then does, in order:

1. **Optional voice separation** — a source flagged for it is split into voice
   and everything else, and the two halves replace it for everything below.
2. **Audio extraction and frames** — 16 kHz mono wav per source, and a JPEG out
   of the footage every *n* seconds (the Freq control; 1 s is typical).
3. **Speech to text** — every source transcribed, with word-level times.
4. **Forced alignment** — a second pass that times each word to within about
   20 ms. This matters more than it sounds: ASR timestamps run a third of a
   second late on average, and every cut point in the app is placed on a word
   edge.
5. **Diarization** — who spoke when, merged into the transcript.
6. **Describe** — the frames go to a vision model in small batches, each batch
   carrying the words heard in those seconds and a rolling summary of what has
   happened so far, so the answer is *what is happening* rather than *what is in
   this picture*. Output is one EVENT line per few seconds.
7. **Fix** — the raw transcripts are cleaned into publishable text, grounded in
   the event log and in what other microphones heard at the same moment. Times
   and speaker labels pass through byte-identical, enforced.
8. **Merge** — everything above becomes one session timeline: every spoken line
   and every EVENT line on one clock.
9. **Mark what was said twice** — see *Video style* below.

Every stage is skipped if its output is already on disk, so a re-run resumes
rather than starting over. ⏸ parks the run between requests; ⏹ ends it.

The right-hand pane is **every prompt the app will send**, one menu row at a
time. The first row is not a prompt: it is what you want the editor to know
about this session — who is in it, how names are spelled, what has to end up in
the video, how long it should be — and every request this project makes carries
it. Behind it, in pipeline order, are all twelve system prompts. A ✎ beside a
name means yours differs from the shipped one; Reset restores it.

### Video style

A dropdown on Prepare that chooses **which pipeline runs**, not which wording is
used:

**Lecture** — a read to camera, where the speech *is* the video. Everything you
said goes in; only the mistakes in the saying come out. A model is shown every
word that was spoken, in order, with the seams between recordings and the long
pauses marked, and answers with **the text of the finished video, removing words
only** — never adding, never rewording. That answer is matched back against the
word stream *from the end backwards*, so where something was said twice the
**later** saying is the one kept, by construction rather than by instruction.
Every dropped run becomes a cut whose edges are fenced by the surviving words on
either side: the cut can never touch a word that stays, and within that fence
the audio envelope picks the quietest moment to splice at. The answer is written
to `prepare/transcript/final.txt` — delete a word there by hand, press Cut, and
it is out of the video, with no model asked.

**Gaming** — a session where the interesting moments have to be *chosen*. The
cut model reads the whole session timeline and answers with the segments worth
keeping, to a target length if the context names one.

Both styles describe the frames, because the thumbnail is picked by what is on
one.

### Cut

The session on a timeline: thumbnails per camera row, a waveform lane per sound
below, everything the cut keeps tinted green. Time nobody filmed takes no width
at all, so two recordings meet with a striped amber border each.

The page opens as soon as there is footage — you can lay the session out, see
where takes fall against each other, shift a recording by hand and listen,
before anything has been transcribed.

▶ asks the model for a cut. From then on it is yours: drag to select, ＋ Add,
✕ to drop, ⌦ for whatever is in hand, Revert to go back to the suggestion.
Trimming a clip edge is the right button (pick the border up — it turns white)
then the left, or ‹f and f› a frame at a time. Everything snaps to word edges,
silences and visual cuts, with the pointer showing what a press would grab.

You can also splice in cards, stills and sounds, switch which camera a scene is
shown from, silence a lane for one scene, and add effects — zoom, hold, speed,
volume, captions, drawings — each proposed by its own model pass after the cut
and each editable by hand.

### Narrate

An editable line per clip: what is said over it, in which voice, with what
emotion. ▶ writes the lines the cut does not have and speaks the ones not
already in the cache. A clip grows, or the speech speeds up, when a line does
not quite fit — both are logged. Turn the narration off entirely and everything
that exists to carry one disappears from the page.

### Produce

Renders the video. Every clip is encoded once from its own recording, joined by
stream copy, loudness-normalized over the whole thing so the joins do not pump.
Container, codec, quality, resolution, frame rate, audio bitrate, mono, frame
edges — all here.

**Subtitles** come from Prepare's aligned words, not from a second transcription
and not from reading the finished video back: the words that survive the cut,
in each clip's own seconds, with their written spelling restored from the
transcript. They can be burned in, muxed as a track, written as an `.srt`
beside the video, or left out. **Translate** takes the same cues into other
languages — one call per language for the whole track, read back by line number
so a dropped line can never shift the rest onto the wrong seconds.

The **thumbnail** is either drawn by an image model from real frames of your
video, or — where your context asks for a picture the video already contains
("use the frame that shows the title slide") — the frame itself, cropped, with
the title printed on it locally. The title, the description and the thumbnail
instruction come from one model call that reads your context.

---

## What it needs

Four HTTP servers, all local by default. Compose files for all of them are in
`../cpp/`.

| what | default | serves |
|---|---|---|
| an OpenAI-compatible LLM | `http://127.0.0.1:9001` | every text and vision job |
| audio.cpp | `http://127.0.0.1:8765` | speech-to-text, alignment, diarization, separation, TTS |
| sd.cpp | `http://127.0.0.1:1234` | the thumbnail |
| ffmpeg / ffprobe | on `PATH` | every frame, cut and encode |

Settings are one bash-sourceable file, `~/.config/naivepost/llm.conf`: server
URLs, API keys, and which model id to ask each server for. Model *weights* are
never named here — that is the servers' business (`cpp/config-llamacpp.ini`,
`cpp/config-audiocpp.json`).

The vision model has to be able to read images, or Describe is the one job that
cannot run.

## Build and run

```sh
cd gui && go build && ./gui
```

Go 1.26, GTK4 via gotk4. No libadwaita.

## A project on disk

A project is a **folder** ending in `.naivepost`, holding everything the session
produced. It can be moved, copied or zipped whole.

```
<name>.naivepost/
  naivepost.json                    the project: sources, settings, context, prompts
  sources/                        the footage, if you asked for it to be copied in
  prepare/
    inputs/<source>/              voice16k.wav, transcript.{txt,tsv,srt},
                                  words.json, words.aligned.json, turns.json
    inputs/frames/<source>/       one JPEG per interval, named for its second
    describe/<source>/events.tsv  what was on screen, per few seconds
    transcript/
      <source>/transcript.fixed.tsv + subtitles.srt
      session.tsv / session.txt   everything on one clock — what the cut reads
      retakes.tsv                 the stretches that were said twice
      final.txt                   Lecture only: the words of the finished video
  cache/
    llm/<step>/                   model answers, keyed by the exact request
    waves/, edges/                waveform envelopes
  cut/cut.json                    the segments, in session seconds, plus effects
  narrate/narration.json          the lines, their voices and their placements
  produce/
    clips/                        per-clip encodes, the working .srt, translations
    final.<container>             the upload
  publish/
    thumbnail.png                 the picture with the words on it
    thumbnail-plain.png           the picture as drawn
    description.txt               the description
  llm/                            a readable transcript of every model call
```

`cache/llm` is keyed on the exact request, images included, so re-running a step
whose inputs have not changed costs nothing. Change a prompt and the cache for
that step misses, which is what you want.

## Design rules the code holds to

These are the decisions everything else follows from.

- **Models propose; the machine places.** A model is never asked for a
  timestamp it would have to compute. It answers in words, or line numbers, or
  clip-relative seconds — and the app turns that into a cut using the aligner's
  word times. Where a model was given the arithmetic, it got it wrong: one run
  read the marker `[01:45]` as 145 seconds instead of 105 and removed five
  seconds of good speech.
- **Every cut lands on a word edge.** The aligner is accurate to about 20 ms;
  the audio envelope is only ever allowed to choose *where between two known
  words* a splice falls, never which words it touches. An envelope cannot tell
  a breath from a word, and letting it try is what used to clip syllables.
- **Nothing is deleted, only marked.** A retake, a false start, a stretch the
  cut drops — the transcript keeps every word, and the marks are a file you can
  read and edit.
- **A step's output is its resume marker.** Every stage checks whether its own
  file exists before doing the work, so a run that was stopped, failed or ran
  out of memory resumes from where it stopped.
- **Failure is specific and local.** A pass that cannot run says so at the
  point it failed, naming the model and the reason, and the step carries on
  where it can. A language that fails to translate costs that language and
  nothing else.
- **One place per answer.** A thing you tell the editor is said once, in the
  context box, and reaches every job from there — rather than in a settings
  field that can quietly disagree with it.

## Tests

```sh
cd gui && go test ./...
```

About 24 seconds, no network, no GPU, no servers. Much of the suite pins
*reasons* rather than outputs — the wording a prompt has to contain, the order
two things happen in, the fact that a particular mistake stayed fixed — because
every one of those was a bug once, and the comment beside the assertion says
which.
