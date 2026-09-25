# Native AI Video Editor for Post Processing (Naivepost)

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
upload text live on Produce, beside the render they belong to. Per tab, ▶ means
([§2](spec/11-flow-index.md)): Prepare runs Prepare; Cut runs Suggest until its
preview is started, then drives that preview until ⏹; Narrate writes and speaks
and greys out entirely when narration is off; Produce renders — and Narrate and
Produce refuse without a cut rather than being locked away.

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

1. **Optional voice separation** — a source flagged for it (✂) is decoded to 44.1 kHz
   stereo, sent to the separation model in pieces of at most 300 s cut at silences, and
   split into voice and everything else; the two halves replace it for everything below.
2. **Audio extraction and frames** — 16 kHz mono wav per source, and a JPEG out
   of the footage on a fixed quarter-second grid that restarts at every scene
   change, so a new slide's first picture is always among them. The scene pass
   looks at four frames a second over the whole video; frames are kept at the
   video's own size, and Freq only decides which of them the model sees.
3. **Speech to text** — every source transcribed, with word-level times. A long
   recording is asked about in even pieces (60 s for the qwen3 family, 300 s
   otherwise), each cut moved to a silence where one is nearby, and an
   out-of-memory answer halves the piece size down to 20 s; hearing nothing at all
   is a real answer — an empty transcript, not a failure. A file
   under two seconds — a recorder opened and closed again — is written up as
   silence without asking any server, and a video shorter than the frame
   interval keeps its one frame.
4. **Forced alignment** — a second pass that times each word to within about
   20 ms. This matters more than it sounds: the best transcriber here returns no
   word times at all, ASR timestamps elsewhere run a third of a second late, and
   every cut point in the app is placed on a word edge. The aligner is given the
   recording one piece at a time, and each piece carries the words the ASR heard
   in exactly those seconds (`asrchunks.json`) — never a guess at how many words
   a stretch of audio ought to hold. A forced aligner cannot decline the text it
   is handed, so a guess that runs high compresses real speech and leaves the
   rest of the recording with no words over it, which the cut then deletes as
   footage where nobody spoke.
5. **Diarization** — who spoke when, merged into the transcript. The window it
   asks for shrinks until the server can hold it (90 s, then 45, then 25): the
   model permits 90 s, but the buffer grows with the window and a machine with
   an LLM's weights pinned beside it may have no 2.9 GB hole to put one in.
6. **Describe** — the frames go to a vision model in small batches, each batch
   carrying the words heard in those seconds and a rolling summary of what has
   happened so far, so the answer is *what is happening* rather than *what is in
   this picture*. Output is one EVENT line per frame, stamped on that frame's
   second; a frame in which nothing changed says only `same` and is folded
   into the line before it when the log is read, so a change is dated to the
   second it happened and a brief still reads one line per thing that happened.
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
said goes in; only the mistakes in the saying come out. You stop the recording
when you stumble and say it again, so the mistakes are at the **joins**, and
that is what the model is asked about: one join at a time, the last words of
the take that was interrupted and the first words of the take that follows, as
they are written. It answers with two numbers — how many words come off each
side so the two run on as one sentence — and *nothing* else. Nought and nought
is a whole answer: a recording stopped to change a slide is not a stumble. The
side **before** the join is cut first and as far back as the repetition goes,
because what was said last is what you meant to keep.

Each removal becomes a cut whose edges are fenced by the surviving words on
either side: the cut can never touch a word that stays, and within that fence
the audio envelope picks the quietest moment to splice at. What survives is
written to `prepare/transcript/final.txt`, punctuated and readable — delete a
word there by hand, press Cut, and it is out of the video, with no model asked.

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

▶ asks the model for a cut. The toolbar has three ▶s of its own: plain ▶ plays
the recording, every second of it; ▶✂ plays the cut, removed stretches skipped,
with the clock on the finished video's time; ▶✂✂ reviews the cuts — ten seconds
of the finished video before each join and ten after it, one join after the
other, stopping after the last — so every splice is heard in one sitting
without watching the minutes between them.

From then on the cut is yours: drag to select, ＋ Add,
✕ to drop, ⌦ for whatever is in hand, Revert to go back to the suggestion.
**Trim a clip edge by dragging it** — with either button, on the pictures or on
the green bar above them — or ‹f and f› a frame at a time once it is in hand.
Wherever the pointer turns into a resize arrow, a drag there resizes: that is the
whole rule. The right button is for *moving* things instead, a scene along its
recording or a recording along the clock. Everything snaps to word edges,
silences and visual cuts, with the pointer showing what a press would grab.

You can also splice in cards, stills and sounds, switch which camera a scene is
shown from, silence a lane for one scene, and add effects — zoom, hold, speed,
volume, captions, drawings — each proposed by its own model pass after the cut
and each editable by hand.

### Narrate

An editable line per clip: what is said over it, in which voice, with what
emotion. ▶ writes the lines the cut does not have and speaks the ones not
already in the cache; rewriting every existing line too is a choice you turn on
(`P.policy.narrationRewrite`), never the default. A clip grows, or the speech
speeds up, when a line does not quite fit — both are logged. Turn the narration
off entirely and everything that exists to carry one disappears from the page.

### Produce

Renders the video. Every clip is encoded once from its own recording, joined by
stream copy, loudness-normalized over the whole thing so the joins do not pump.
Container, codec, quality, resolution, frame rate, audio bitrate, mono, frame
edges — all here.

▶ over an unchanged project costs nothing: the render keeps `produce/final.stamp`
beside the video, a hash of what it reads — the encoder settings (not where the
file goes), the segments, each narration line with its take's size and mtime, each
recording's path, size and mtime, the aspect, the voice and whether narration is on.
The same hash means the encode is skipped; the title, description, thumbnail and
upload record are not in it, so re-drawing a picture never makes a video stale.

**Subtitles** come from Prepare's aligned words, not from a second transcription
and not from reading the finished video back: the words that survive the cut,
in each clip's own seconds, spelled the way the **fixed** transcript spells
them — so the sentence in your context asking for the script's spelling of a
name or a number reaches the subtitles too, whichever speech model heard it. A
clip with no narration line under it captions its own speech; an inserted sound,
a freeze or a clip with no video gets no cues. A caption ends where the speaker
breathed, when two rows of 42 characters are full, or after six seconds on
screen; a gap shorter than 1.2 s holds the caption already up instead of going
blank, and words too brief to read are folded into the caption that follows. An `.srt` **and** a `.vtt` are written beside the video on every
render, one pair per language, whatever else you asked for — the video itself
can carry them burned into the picture, as a track inside the file, or not at
all. The `.vtt` is there because a browser reads no other subtitle format and
ignores the tracks muxed into the file; a `<video>` tag pointing at the poster
and every track is written out beside them, ready to paste into a page. A
`produce/final.html` is written with them — a bare `<video poster controls src
preload="none">`, its `poster=` the thumbnail as `final.jpg` and one `<track>`
per `.vtt`, your own language first and default — because a browser shows none
of the tracks muxed into the file. Open it over http rather than by
double-clicking: a `file://` page is refused every `<track>`. An mkv or an h265
says so in the log, since no browser plays them. **Translate**
takes the same cues into other languages — the track goes over in requests of
150 numbered lines, answered one line at a time, and a number that never comes
back is asked for once more by its original number before the line ships in the
source language with a warning naming it. A track is never dropped for one bad
line, and your own language is never translated into itself.

The **thumbnail** is either drawn by an image model from real frames of your
video, or — where your context asks for a picture the video already contains
("use the frame that shows the title slide") — the frame itself, cropped, with
the title printed on it locally. The title, the description and the thumbnail
instruction come from one model call that reads your context.

Under the picture, four runs cost different things: **⊙ Thumbnail only** ticked
redraws from the instruction as it stands — one drawing call, no text rewritten;
unticked, or **↻ reword**, that same text comes back from one model call for its
three answers and the picture stays as it is. Marking your own image as the
thumbnail copies it rather than referencing it, so it survives reopening, and
printing words on it costs no model call at all: they go over the drawn picture,
and the preview shows the text, width and position the export will use. **⤓**
offers a JPEG at the first quality that fits 2 MiB and never rescales it; **⤓
Save video** copies the render out without making it stale, and **↻ Transcode**
encodes a file you name with the settings on the row — no model call, and no
caption track, since a file from outside has no transcript beside it.

---

## What it needs

Four HTTP servers, all local by default — an empty Server box in Settings
means the loopback address below. Compose files for all of them are in
`../cpp/`; the writing model's default port is
[halogen-flash-server](https://github.com/peonist-ai/halogen-flash-server)'s own.

| what | default | serves |
|---|---|---|
| an OpenAI-compatible LLM | `http://127.0.0.1:8731` | every text and vision job |
| audio.cpp | `http://127.0.0.1:8765` | speech-to-text, alignment, diarization, separation, TTS |
| sd.cpp | `http://127.0.0.1:1234` | the thumbnail |
| ffmpeg / ffprobe | on `PATH` | every frame, cut and encode |

Settings are one bash-sourceable file, `~/.config/naivepost/llm.conf`: server
URLs, API keys, and which model id to ask each server for. Model *weights* are
never named here — that is the servers' business (`cpp/config-llamacpp.ini`,
`cpp/config-audiocpp.json`). The same file holds how many requests each server
may take at once (`LLM_SLOTS`, the five audio.cpp `…_SLOTS` and `SD_SLOTS`, all
absent meaning 1), which subtitle languages to offer as a `code:tag:name` list in
`SUBTITLE_LANGUAGES`, and the projects it remembers. What a value resolves to runs
dialog box → this file → an old `llm.conf` left beside the videos (picked up once
into the config folder, its old copy left where it is and never read again) → the
built-in default; below the box, `NAIVEPOST_TTS_URL` moves the audio server for this
app alone and `AUDIOCPP_SERVER` for any frontend reading it, and `SD_SERVER` the
drawing server — while the writing model has no environment override at all, so a
stale export cannot outrank what the dialog shows.

The vision model has to be able to read images, or Describe is the one job that
cannot run.

Which step calls which server, what waits for what, and what could overlap:
[SERVICES.md](SERVICES.md).

## Build and run

```sh
cd gui && go build && ./gui
```

Go 1.26, GTK4 via gotk4. No libadwaita.

The rewrite lives in `rust/` (Rust, GTK4 with libadwaita), built against the
spec in `spec/`. It builds and runs tests, one spec item at a time; so far that
is the project's file layout and the `cut/cut.json`, `narrate/narration.json`
and `produce/publish/publish.json`, the machine files of §7, the session clock
of §8, the text formats of §6 — the transcripts, `session.tsv`, `events.tsv`,
`retakes.tsv`, `final.txt`, the ASR sidecars, `.frames` and `scenes.tsv`, the
`AWV4` waveform cache and `requests.tsv` — and the four servers of
[02-services.md §1](spec/02-services.md): their loopback defaults, what each
settings box falls back to, which path and timeout every kind of call has, and
which aligner a server's own model list offers — and the model roles of
[02-services.md §2](spec/02-services.md): which of the fourteen jobs asks which
server, thinking on or off, and the numbers that follow (a 60 s ASR chunk for
the qwen3 family and 300 s elsewhere, halved to 20 s on an out-of-memory answer;
90 → 45 → 25 s diarization windows; four frames a vision call; three cut
attempts, the web withdrawn after the first). Its `cut/cut.json` reader and
writer follow [06-effects.md §1](spec/06-effects.md): one effect record for all
six kinds, and old files read forward on load — a `view` becomes a zoom, a
per-effect `mute` becomes `snd: "mute"`, and a whole-cut `sound` is spread scene
by scene into `quiet` — none of those legacy keys ever written again. Its
narration file keeps the spoken takes reusable across versions: the cache key
`25e0.85|[roll#][voice|]text|emotion` names one take, its filename the first 8
bytes of that key's SHA-1 and its seed the next three, so a line re-speaks
itself only when the words, the delivery, the voice or a re-roll changed. Its
publish file is the upload text and the thumbnail state in one record — the
same one `naivepost.json` embeds — where the first frame is the base the image
model edits and the rest are references; the file's existence is what "the text
is written" means, so deleting the folder is how that step is started over. Its
machine files — `llm.conf` at 0600 with the remembered `PROJECT_<n>_ROOT`/`_FILE`
pairs sorted so an unchanged save is byte-identical, the legacy `settings.json`
read once and never written again, the voices folder, an edited prompt kept only
while it differs from the shipped text — answer in one order everywhere: the
dialog box, then this file, then the environment, then the built-in default. Its
session clock places every source by the timestamp its own file name carries —
the earliest is second zero, an unstamped file sits at the start rather than at
its mtime and warns, and a frame is named by its exact second
(`2026-08-08_19-55-15.250`), never by its position in a sorted list. And the
two tool passes of [02-services.md §3](spec/02-services.md): the model-chosen
cut (§3.6 — `add_segment`, `remove_segment`, `set_speed`, `cut_status`,
`finish_cut`) answers with what a segment became rather than with a yes, and
the clips pass (§3.7 — `add_caption`, `set_clip_speed`, `add_effect`,
`get_frames`) clamps into the clip's own seconds and reports the fade, the box
and the rate it actually used; a refusal names the model's own number in mm:ss.
And what happens when a server is missing ([02-services.md
§4](spec/02-services.md)): narration still gets its cuts from the waveform when
the aligner is down, a video whose render or publish was forgotten by the server
says so instead of hanging, and a forgotten job can be retried while any other
failure has to start over. And the details §5 confirms against the code: every
audio path in a request body is one an upload returned, the speech body carries
the project language rather than a hard-coded `"en"`, sd.cpp's four waits are
15/60/30 s at 1 s/10 s, and each Settings row — ten buttons and "Test All" —
keeps its own verdict while the file is written 600 ms after the last keystroke
instead of on a Save button. What those buttons do when pressed
([F0.13](spec/03-shell.md)): each reads what its box holds at that moment rather
than the file still catching up, spins, then marks ✓ or ✗ with the verdict as its
tooltip and mirrors the line into the main log as "settings: …"; "Test All" runs
every row and reports each, so one failure never hides the rest. The pass rules
are that strict per server — the LLM test is one completion of sixteen tokens with
thinking off and quotes the answer and the wait; vision shows a generated 48 px red
square and only passes if the model says red; ffmpeg is checked as one build holding
the seven filters and four encoders the steps shell out to, with a missing ffprobe
reported before a missing filter; firefox "off" is a pass while a live firefox has to
be driven headless; audio.cpp answers with its health and names the voice it will
narrate with, and having no aligner is a success because cut points then come off the
waveform; sd.cpp names its loaded weights, and a port answering only `/v1/models` is
called out as not sd-server. The model list itself is `GET /v1/models` on a 15 s budget —
faster than any completion, because a list loads no weights and a slow one means the address
is wrong rather than the model being cold — and it fills the Model dropdown in the server's
own order, where Use copies the picked id into Model only if that exact string was listed:
ids share prefixes between vision and text variants of one base name, so a partial match
would write an id whose completions fail for a reason nobody can see. And what every model call leaves behind
([§7](spec/03-shell.md)): one HTML page per run under `llm/`, named
`MMDD-HHMMSS-<first step>.html` after whatever started the run, each call a
numbered section carrying the model, whether it was thinking or executing, how
long it took, the messages with their images inline, the reply, its reasoning
folded away, the tool calls and their results, and a note when a reply was cut
off at the model's token limit or never came. While a call is still waiting for
its answer the page says so — `reply pending` in that line, standing in for the
duration there is not yet — and it fills in as the streamed reply arrives inside
an open `<pre>`, so opening the file mid-run shows how far the model has got. A
run ends at a queue reset: the next call starts a fresh page named after its own
first step, and a call made with no run open gets a page of its own rather than
going unrecorded. The log gets two lines per call —
what went out in kB and images, what came back in how long — plus once per run
the line naming `llm/<page>`. Keeping that page can never fail the call it
records: a problem turns into a log line instead. And no request is ever too big
([§5](spec/09-llm-and-tools.md)): one prompt may carry at most 120 000 characters
(`P.machine.promptMaxChars`), counted as characters rather than bytes so a Cyrillic
transcript isn't charged twice for its encoding; a job with more to say is split up —
captions five clips per request, subtitle translations 150 lines
(`P.machine.translateBatch`) — or leaves the rest for the model to read back through
`get_lines` / `get_events` instead of being uploaded whole. A request over the cap is
refused before it goes on the wire, and the refusal names both ways out. The size that
went out is reported in the log only, never on the exchange page. And the details §8 confirms the
same way ([§8](spec/03-shell.md)): every subprocess is logged quoted, as a shell would
take it, before it runs, and a failure repeats the command with the last 400 characters of
its output; opening a `naivepost.json` opens its folder, and a project that used to write
somewhere else says which folder it writes into now and that the old one is untouched, with
old numbered step folders moved into the new layout one line per move and anything it cannot
move left exactly where it was; a cleared voices-folder or model-id box goes back to the
shipped default while every other setting is written as typed, empty included; the Settings
dialog's own log stays collapsed until a test fails, which opens it; a missing audio model is
answered with the catalogue that server does serve, plus the exact install command when the id
is a shipped default; and a file whose header carries no duration — a recorder killed
mid-write — is measured by decoding once and remembered. And switching tabs
([F0.1](spec/03-shell.md)): a locked tab is greyed rather than disabled, so a
click reaches it and bounces off with the reason on the status line — "Add
footage on the Prepare step first" — while a page whose prerequisites vanish
while open goes back to Prepare in silence; arriving flushes the narration's
400 ms autosave, then refits its lines to the cut, rebuilds the cut if Prepare's
output moved, or refreshes Produce's readouts and publish panel. And what one
means ([F0.2](spec/03-shell.md)): a run under way first, so the same button shows
⏸ and says "pausing after the current stage…" rather than starting anything; then
the page's own preview, but only once it has been started — Prepare and Produce
have none, so their ▶ runs the step while something plays; otherwise the sources
are snapshotted as they stand and that one step runs, with no "all done" line for
a single step. The window draws that ▶ at the left of its bottom row; ⏹ and the
"I'm feeling lucky" gears come with their own items. And which project a launch
opens ([F0.6](spec/03-shell.md)): the file the desktop handed over — the first
only, and even when it cannot be read, since a double-click asks for *that*
project; else what `llm.conf` remembers for this folder while it is still on
disk, an unmounted drive falling through without being forgotten; else
`session.naivepost` beside the folder; else a blank session at that path, which
writes nothing until the session is first saved. And ＋ New in the header bar
([F0.8](spec/03-shell.md)): refused while a run works, asking first when the
session has anything in it — naming what goes back to empty and saying the open
project stays on disk as it is — then defaulting the name to today's date with
`-2`, `-3` while taken and `.naivepost` on the end, refusing a name that is a
project already, and writing only `naivepost.json`: every output of the project
you left is untouched. And Save ([F0.10](spec/03-shell.md)), refused while a run
is on: the same name rewrites `naivepost.json`, a new name renames the whole
folder — never a copy, so the transcripts, frames and renders keep the name —
and that rename is itself the check, failing onto a folder that already has work
in it with the files left where they are and the log saying where. And ＋ Add source
files… on Prepare ([F0.12](spec/03-shell.md)): refused while a run works, since a copy is
a run of its own; offering audio and video only, and either copying each pick into the
project's `sources/` — through a `.part` renamed at the end, so an interrupted copy never
looks like a finished file, and skipping one that is already there at the same size — or
referencing it where it lies. A video arrives as footage, the first untagged recording
becomes the narrator only if nobody holds that slot, and the status counts what went in:
"added 2 source(s)", "added 1 of 3 — the rest were already in", "already in the session —
nothing added". And ⟳ Rescan, at the right end of the header bar
([F0.11](spec/03-shell.md)): every source whose
file has gone is dropped from the session and named in the log by the path the row
shows; narrator slot 1 goes to the first surviving recording, footage only if none
is left, and only when its holder was the file that went — a slot you untagged
yourself comes back untagged. The cut and narration are then re-read from disk, so
a file deleted since the last scan reads back empty rather than remembered, and the
status says "rescanned". Those rows are the sources list (§4): one per source file,
named by its file name and carrying the stored path as its tooltip. 🎥 footage is
offered only where there are frames to take, and off means the file is only listened to;
🎤 cycles the free narrator slots 1-4 and back to none, slot 1 being the voice the
narration is spoken in and going to the first untagged recording, footage last, whenever
nothing holds it; ⚠ marks a name with no timestamp in it, since such a file starts where
the session does; a file holding two or more audio streams gets a track menu whose face
reads `<on>/<total>`, and the last ticked track cannot be unticked; ✂ is dead on a file
that is already half of a split; 🗑 takes the row off the session and leaves the file on
disk. Two files left with one name stop ▶ until one is renamed, and a project hand-edited
into a contradiction — two rows in one slot, or footage promised out of an audio file — is
cleaned as it loads, one log line per change. Language and the frame cadence (Freq) sit
under the list and write straight through to the session; Style is not in this rewrite —
the policy sets it from the User Context — and frames are kept at the video's own size.

✂ on a row is a wish, spent by ▶. The recording is decoded to 44.1 kHz stereo and sent to
the separation model in pieces of at most 300 s, each cut moved to a silence rather than
into a word; the voice is the `vocals` or `voice` stem and the rest is `instrumental`, or
every other stem mixed with `amix normalize=0`. Both halves are written to
`stems/<base>.split-voice.wav` and `<base>.split-novoice.wav` — `.mkv` for a video, its
picture copied — keeping the timestamp in the name, and the row becomes two: the rest
keeps 🎥 footage, the voice takes the 🎤 slot, the wish is cleared and the project saved.
A stem set with no voice, or with no other half, stops the run with the reason; when both
halves already exist the log says so and nothing is redone. The log also reports the mean
level of the mix and of each half, warning when the rest sits 10 dB or more under the mix.

Each source is then read once into `prepare/inputs/<source>/`: its audio down to mono
16 kHz, the ASR asked unless `words.json` says it was already done, an aligner used when
one is served (and a failed one is only a warning where the ASR timed its own words — a
hard stop naming that model where nothing else can), and diarization asked unless
`turns.json` exists. Words become segments broken at a speaker change, a pause over 0.7 s
or 12 s, into `transcript.tsv` for Cut and `transcript.srt` for you. A take under two
seconds is written up as silence with no server asked, and the log ends "N segments".

Frames come out of a video in two ffmpeg passes. A scene pass reads the whole video at four
frames a second and calls any frame scoring over 1.0 a change, merging changes nearer than half
a second into the first and writing `scenes.tsv`; then each scene gives its own first frame plus
one every quarter second until the next change, so the grid restarts where the picture does and
the Cut timeline never runs out of pictures. Frames are named by their exact second on the file's
own clock (`YYYY-MM-DD_HH-MM-SS.mmm.jpg`), kept at the video's own size, and extracted in
parallel — a quarter of the machine's cores, two to eight workers. The `.frames` marker holds the
grid and the threshold, so a source whose frames were already made at both is skipped with a line
saying so; a take shorter than one grid step keeps its first frame alone.

The aligner's pass over that audio keeps its pieces at 60 s: the ASR's own chunks when
their word counts agree with their word times, otherwise windows cut at silences with the
text shared out by how much of each has speech in it. Each window is trimmed to its sound
plus a quarter second either side, split at the quietest moment nearest its middle when
still too long, and halved on an out-of-memory answer down to 15 s. A name in the box is
obeyed — one not served for alignment skips alignment for the run rather than quietly
using another model, and cut points then come off the waveform; with the box empty, every
model declared for `align` is tried and the first to answer serves the rest of the run.
`words.aligned.json` is written once at the end, and if 10 s or more — and 8 % or more —
of the talking has no word over it, the log says the times are wrong and the cut will drop
that footage.

Those words are then cleaned in blocks of 25 lines, each block asked with five seconds on
either side from every other recording plus this source's own on-screen events as grounding.
A bad line is refused by name and the model is told so on the second try; out of tries that
block keeps what the ASR heard and the rest goes on, so one unreadable stretch never costs
the source. A row nobody can make out is flagged for you rather than guessed at. Each source
leaves `transcript.fixed.tsv` (a recorder's words go to `commentary.fixed.tsv`) with
`subtitles.srt` over its video, and everything merges onto one clock into `session.tsv` and
`session.txt` — the marked stretches of an abandoned take folded into one line each, so what
you open is what the cut reads.

Where a mark's cut actually lands is read off the waveform, not from the aligner's numbers: with word
times the words fence the edge and the sound picks the quietest moment inside the fence — the last word
that stays followed up to 0.25 s past its own end for a trailing consonant, then the quietest point
within 0.4 s; without them the envelope alone places it, reaching back up to 0.8 s and leaving 0.05 s at
the cut, and a stamp with more than half a second of quiet before it stands where it is rather than
jumping forward onto the next sound.

The text itself is open to you: `prepare/transcript/final.txt` is edited in any text editor — there is no
editor in the app — and the next Cut ▶ notices it is newer than the marks and remakes them from what you
left in it, asking no model. A word you typed that nobody spoke is left out with a warning, since there is
no sound for it, and an edit that removes more than 40 % of the words is refused as not an edit, leaving
nothing marked.

What Prepare leaves behind is the whole record of one read: `prepare/inputs/meta.env` first, whose presence
is what says the sources were read at all — `INTERVAL` (Freq) and `SCALE`, always `native` so frames keep
the video's own size, plus the footage file with its base name and the same pair for narrator 1, each only
when it resolves. Alongside it go that source's mono 16 kHz audio, its transcripts in three forms, its word
times plain and aligned, its diarized turns, its scene list and `.frames` marker, its per-frame rows and
describe checkpoint, and the scaled frames sent to the model; then the merged `session.tsv`, `session.txt`,
`offsets.tsv`, `retakes.tsv` and `final.txt`. Folders are 0755 and files 0644, and a source path is stored
by the one rule everything else uses — inside the project as `project:`, under the root relative to it,
absolute nowhere else — so a renamed project still finds its sources.

Beside the list is the prompt bench: one heading row — the row's title, a ✎ mark reading
"edited — kept in your settings", a row picker, Reset — over one text box. Thirteen rows
in pipeline order, User Context first and the twelve system prompts behind it. The User
Context is stored on the project and its box carries the project's note; every prompt is
stored per machine under the settings folder's `prompts/<key>.txt` at 0600. Every keystroke
is stored, so there is no Save button. ✎ and Reset appear only while this machine holds that
row's wording, and Reset deletes the file and restores the shipped wording; a machine with no
settings folder shows no mark and cannot store a prompt, but still edits the User Context.
A thirteenth prompt, `policy`, drives F0.7's derivation of the editing policy from the User
Context — asked with thinking off, and edited through the policy form rather than this bench.

What a model actually reads is not the whole of these files: each request sends the system prompt cut
to the sections its job can use (`rust/src/prompt_assembly.rs`), then that job's own wording, then
the precedence rule when the project has a User Context to outrank. A section you add to the system
prompt under a heading nobody has heard of reaches every job; one added under a heading some jobs are
not given reaches only those that are.

To ship it, `./release.sh` tags the next version (v0.1, v0.2, ...) and pushes
the tag; GitHub Actions (`.github/workflows/release.yml`) then runs the tests,
builds a Flatpak and an AppImage and attaches both to the release. The same
builds run locally with `./release.sh flatpak` (flatpak-builder needed, from
`ch.bocek.naivepost.yml`) and `./release.sh appimage` (docker or podman
needed, built in a Debian testing container); both land in `dist/`. Prefer the
Flatpak where Flatpak exists: the GNOME runtime already carries GTK4, the GTK4
video sink, GStreamer and an ffmpeg with h264, so nothing is bundled by hand
and ffmpeg is inside the sandbox. The four servers are not in either; the app
reaches them over HTTP wherever you run them. The AppImage also expects what
any desktop has: an icon theme, GL, the X11/Wayland and text libraries. Inside
Flatpak the app writes no desktop entry of its own and offers the model no web
search.

One package. The pipeline files -- transcribe, align, describe, fix, translate,
the LLM and image clients -- touch no widget, and `gui/seam.go` says exactly
what they may take from the application: an interface `*App` satisfies, plus
the handful of fields read directly. A test reads those files and fails on
anything past it, so a pipeline function that reaches for a page is a red test
rather than a race that happens to work.

## A project on disk

A project is a **folder** ending in `.naivepost`, holding everything the session
produced. It can be moved, copied or zipped whole.

```
<name>.naivepost/
  naivepost.json                    the project: sources, settings, context, prompts
  sources/                        the footage, if you asked for it to be copied in
  prepare/
    inputs/<source>/              voice16k.wav, transcript.{txt,tsv,srt},
                                  words.json, asrchunks.json,
                                  words.aligned.json, turns.json
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
    clips/                        per-clip encodes, the working .srt, translations,
                                  and the concat list the join reads
    final.<container>             the upload
    final.stamp                   what the video is up to date with
    final.jpg                     the poster: the thumbnail as a jpeg
    final.html                    the <video> tag, tracks and all
  publish/
    thumbnail.png                 the picture with the words on it
    thumbnail-plain.png           the picture as drawn
    description.txt               the description
  llm/                            a readable transcript of every model call
```

`cache/llm/<step>/` is keyed on the exact request — the system prompt, the user
text, the rolling state, the speech and context it was grounded in, the images,
and which draw of a pooled call this was — so re-running a step whose inputs have
not changed costs nothing. Change a prompt and the cache for that step misses,
which is what you want. The key also holds the model id and whether thinking was
asked for, so switching models in Settings asks again rather than replaying the
previous model's answers. Only an answer worth keeping is stored: an empty reply,
a partial translation or a repaired-after-refusal fix never is, and a cache that
will not write makes the step slower, never fail. Five steps use it — describe,
the transcript fixer, joins, retakes and subtitle translation; the cut, the
narration and the upload text do not. A cached answer is served before the
model's slot is taken and before the exchange page opens, so a resumed run's
`llm/` page shows only the calls actually made (each hit still gets its own
`requests.tsv` line, marked `cache`).

Every request that leaves the machine is timed into that same file — the LLM, audio.cpp, the image
server and the web tools' searches and reads, one line each appended when it ends however it ended:
`ok`, `cache`, `error <status or reason>`, `stalled` from the stall watch, or `cancelled` by the
stop button. A retry is a line of its own with the next attempt number, and a cache hit's line has no
time on the wire at all, which is how the file shows what a re-run saved. Nothing in it is rewritten,
and the line goes down before the reply is used, so a step that fails on the content afterwards still
leaves its requests behind. Local work is not in there: ffmpeg and the other subprocesses are logged
as the commands they ran. At the end of a run the log says it per service — `>>> requests: llm 57 in
2h 13m on the wire, 4m waiting for a slot; audio 38 in 6m 10s; web 3 in 12s` — and that split of
waiting against working is what slot counts get chosen by, rather than a guess about how loaded a box
is.

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
- **A number lives in one constant.** Every tuning value Prepare reads — the retake and seam family, describe's context sizes, the chunk and silence and diarization bounds, the frame grid and worker counts — is declared once, in the module whose rule uses it, with its `P.*` id from the parameters table in its doc comment. `rust/src/params.rs` lists them all for reading; it holds no numbers itself, so a catalogue row cannot go stale against the code. The project's own tab settings are catalogued the same way by `params::project_settings()`: Freq and the rest of that page's controls read their defaults from the structs that store them, not from a copy. Directive A sorts those homes into four kinds — editing policy for what the video becomes, the prompts for how a job is worded, machine settings for which servers and binaries run, and engineering constants for how the app looks, waits and caches — and `rust/src/decision_homes.rs` keeps that split enforced. Around every model call there are five further places a decision can sit — the tool argument, the tool's answer, the `finish` answer, the walk-back after it, and the policy or prompt — separated by what the model is told rather than where the code runs; `rust/src/decision_audit.rs` holds that table, and its rule of thumb is that if the app changes what the model said, the model hears about it: per item in the tool's answer, for the whole cut at `finish`.
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

From `rust/`, tests need a display, so they run under a virtual one:

```sh
cd rust && just test    # xvfb-run, GSK_RENDERER=cairo
```

`just snapshot <screen>` renders one screen to `rust/shots/<screen>.png` the
same way, where `<screen>` is named after a spec image (`03-window`, `03-sources`,
`04-prepare`, `05-cut`, ...) and the window is built from the fixture project at
`rust/fixtures/demo.naivepost`. There is no host display in this container, so
both recipes are how the app is seen here.

`just build` (or `cargo build --release`) produces the standalone binary at
`rust/target/release/naivepost`, and prints the triple it is building for first.
There is nothing to choose per machine: the libc follows the toolchain, so the same
recipe links musl on Alpine and glibc on a Debian/Ubuntu host — only the system
packages differ by name (`gtk4.0-dev libadwaita-dev` on Alpine,
`libgtk-4-dev libadwaita-1-dev` on Debian/Ubuntu). The binary is not static: it
links that libc plus GTK 4, libadwaita and cairo, so run it on a matching system
rather than copying it across. Against a real display it opens the project in the
current folder; inside the container there is nothing to draw to, so use
`just snapshot` to see it.
