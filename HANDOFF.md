# Handoff — naivepost (was aitools/autocut)

Written at the end of a long session so the next editor starts where it stopped.
Read `Readme.md` first for what the app *does*; this is what *changed* and why.

## Where things are

| | |
|---|---|
| **`/home/draft/git/naivepost`** | the repo to work in. One commit, remote `git@github.com:tbocek/naivepost.git`, **not pushed**. 242 files. build + vet + `go test ./...` clean. |
| `/home/draft/git/aitools/autocut` | the original. **Still there, untouched, not deleted** — 6 modified files uncommitted (the merged ▶ button). Decide whether to delete it. |
| `/home/draft/git/aitools/cpp` | the model-server stack. **Stayed behind.** `naivepost/Readme.md` points at `../cpp/`, which is now outside the repo. |
| `/mnt/rec/01-news.autocut` | the real test project. **Will not open under the new name** — see below. |

### The rename was a clean break, deliberately

Everything: module `naivepost/gui`, binary `naivepost-gui`, app id `li.jos.naivepost`,
icon files, MIME type `application/x-naivepost-project`, env keys `NAIVEPOST_*`.
Including the two that touch data:

```
projects  <name>.autocut/autocut.json  ->  <name>.naivepost/naivepost.json
settings  ~/.config/autocut/llm.conf   ->  ~/.config/naivepost/llm.conf
```

**No migration code exists** — the user chose that. An existing `.autocut` project
must be renamed by hand (folder *and* the json inside it), and Settings re-entered
once. Do not add migration unless asked.

## The one big architectural change: Lecture style

A **Video style** dropdown on Prepare picks which *pipeline* runs (`textedit.go`).
Stored as `"style"` in the project; blank = Gaming, so old projects are unchanged.

**Gaming** — the old behaviour: the cut model reads the session timeline and
chooses the moments worth keeping.

**Lecture** — a read to camera. Built this session, and it is the thing to
understand before changing anything near the cut:

1. The model is shown **every spoken word in order** (bare, lowercase, one line
   per breath, `SEAM` between recordings, `(pause 4.6s)` for long silences).
2. It answers with **the text of the finished video, removing words only**.
3. That answer is matched back against the word stream **from the end backwards**
   (`keepMask`). This is not a detail: where something was said twice, a forward
   match pairs the kept words with the *earlier* (abandoned) saying every time.
   Backwards it takes the later one, so *"prefer the second take"* is the shape of
   the match rather than a request to the model.
4. `dedupeJoins` then drops a word left on both sides of a cut (the model kept
   `…project and | and the more…`); the match cannot see that one, since both were kept.
5. Dropped runs become marks; `placeEdges` sets their edges.
6. **Cut runs no model at all** — every filmed second minus the marks minus dead air.
7. The answer is written to `prepare/transcript/final.txt`. **Edit it by hand,
   press Cut, and the marks are rebuilt from the file** (`marksFromText`, gated on
   mtime vs `retakes.tsv`). This is the intended editing loop.

## Rules the code holds to — break these and the bugs come back

Each of these was a real failure this session. The tests pin them with the reason
in the comment.

- **A model is never asked for a timestamp it must compute.** Words, line numbers,
  or clip-relative seconds only. A cut once read the marker `[01:45]` as 145 s
  instead of 105 and deleted five seconds of good speech. Timeline stamps now read
  `[105s | 01:45]` — seconds *and* clock — and every SPEAKER line carries its **end**
  too (`stampSpan`), because a model asked to guess where a line ended once cut four
  seconds into a sentence.
- **The words fence every cut; the envelope only chooses where inside the fence.**
  The aligner is accurate to ~20 ms. A breath is *sound*, so an envelope asked
  "where does the sound before this word stop" answers *after the breath* — that was
  every deep breath heard at a join, and `state of the art` coming out as `state of`.
  `endAfter`/`startBefore` follow the word's own tail past the aligned edge (a
  trailing s the aligner clipped), then take the quietest moment of the gap, never
  crossing a word that stays.
- **The aligner is never handed silence.** It spreads the text it is given across
  the audio it is given and cannot decline: one recording ran 13.5 s past the last
  word and its final four words came back at 30.9 s of a 31.2 s file. `soundSpan`
  shrinks each window to the part with sound in it. Note the trap it fell into
  first: silencedetect calls the file 31.253312 long and ffprobe 31.253313, so a
  silence tested for *reaching the end* fails by a microsecond — it subtracts the
  silences instead.
- **The aligner answers bare lowercase.** It hands back what it *matched*, not what
  it was given. `dressWords` restores case and punctuation from `transcript.txt`,
  which is what the aligner was handed, word for word.
- **One request's memory grows with the audio in it.** 45 s aligns on this machine,
  60 s does not. `alignChunkMax` 30 s, and `asrChunk()` is per-model (Qwen3 60 s,
  Nemotron 300 s) because Qwen3 allocates ~1.5 GB per 20 s.
- **Every stage's output is its resume marker.** A stopped or failed run resumes.
- **One place per answer.** Things you tell the editor go in the context box and
  reach every job from there.

## What else changed this session

**Cut page.** Unfilmed time takes no width (`gapPx` gone); recordings meet with
striped amber borders inset on their own pictures (`srcEdgeMark`). `tAt` is
half-open so a seam reads as the take that *starts* there. Boundaries can never
land in unfilmed time (`ontoFilm`) — a clip starting at 203.5 in a 4.5 s hole froze
playback dead, because `setPlayhead` found no recording and `skipGap`'s guard never
retried. The page now opens on **footage alone**, before Prepare. Selection ends can
be grabbed from the pictures, and the grip is drawn and asked about *before* every
badge on that band.

**Retakes.** Three pooled runs of the pass (`retakeRuns`), verified individually and
merged (`mergeMarks`). `keepRetakes` no longer refuses overlapping answers — that
gate silently emptied the whole pool. `tailFragments` accepts a rephrase (nothing
repeats, but the mark ends its take in fragments). `goesWith` measures what is
*removed*, not the words — a one-word false start followed by 18 s of silence was
being refused as "too short to cut cleanly".

**Subtitles.** Built from the **aligned words that survive the cut**, not transcript
rows — 11 of 81 rows were partly cut and showed words the video does not play, twice.
`tidyCues` holds a line across gaps under 1.2 s (5–80 ms gaps were one blank frame
each). Translation (`translate.go`): English/German/French, the spoken language
filtered out via `asrLanguage()` *not* `projectLanguage()` (the box is empty by
default and shows `en` as a placeholder). Answers read back **by line number**.

**Produce/Publish.** Subtitles work with no narration. The thumbnail can be a frame
*picked* from the video: the upload call answers `frame: <seconds>` when the context
asks for one, and `takeFrameAt` crops it and sets `Own` (words printed locally, never
redrawn). Title box is the upper **half** of the picture. Narrate and Produce tabs
are always reachable — only Cut waits for its input.

**Run bar.** One split control `[ ▶ | Prepare ▾ ]` (`runchain.go`). ▶ runs the
**ticked steps**, wherever you are — it no longer means four different things on four
pages. Default: Prepare alone. Narrate skips itself with no narration, Cut skips
itself over hand edits (keeping them). Prints `all done in 41m 18s — Prepare 32m 04s,
Cut 3m 51s, …`. **Uses `showStep`, not `SetVisibleChildName`** — that was a real bug:
the chain reached Cut with an editor that had never loaded the session.

**Infrastructure.** `freeAudioModels` unloads models when a run ends (they were
resident forever). `cache/llm/<step>` keyed on the exact request. Aligner registered
as a task, with a Settings box.

## Open items

1. **Push the repo** — the user does this, not you.
2. **Delete `aitools/autocut`?** Not done. 6 files uncommitted there.
3. **`cpp/` is outside the repo** — the Readme's `../cpp/` is a dangling path.
4. **Word edges drawn on the waveform** — proposed and wanted, not built: ticks at
   each aligned word start/end on the lanes, past a zoom threshold, so hand cuts
   snap to something visible. `ed.words` is already loaded there.
5. **Halogen** (`cpp/docker-compose.yml`) — the user removed the profile guard and
   the `${HALOGEN_VERSION}` indirection. Their call.
6. Subtitle flicker in **Totem 43.2** is a player bug (pop-os/pop#3441), not ours.
   VLC needed `vlc-plugin-freetype`, now installed. The files were always correct.

## How this user works

- **Verify, don't assert.** Measure against the real project before claiming a cause.
  Several confident wrong diagnoses this session were caught by doing that one step
  later than it should have been. If you don't know, say so.
- **Never run tests that hit the LLM/TTS servers.** `go build`, `go vet`,
  `go test ./...` are offline and take ~24 s.
- **Never use the `gh` CLI.** WebFetch/curl for GitHub.
- Tests pin *reasons*: the wording a prompt must contain, the order two things
  happen in, the fact a specific bug stayed fixed. Write them that way — the comment
  beside the assertion says which bug it was.
- Comments explain *why*, with the concrete failure named. Match that density.
- Ask before outward-facing or irreversible actions.
