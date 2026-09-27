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

Pressing ▶ answers before it runs. A session with no sources says
`add at least one source` in the status line and nothing starts; two inputs whose
file names would land in the same `inputs/<name>` folder refuse with
`A and B have the same name — rename one`, because the second would overwrite the
first's whole record. Both refusals happen before the run is opened, so ⏸ never
appears for work that never began.

A previous run that died inside Describe is cleaned up first: its `events.tsv` and
`state.txt` are removed and the log says so, while the scaled frames are kept —
they are correct no matter where the description stopped, and re-extracting them
costs a minute per video for nothing.

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
   this picture*. What the model actually sees is not every picture on disk: each
   scene's first frame, then one every Freq seconds until the next change, so a new
   slide's opening picture is always in the batch even when it falls between two of
   them. Those copies are scaled to 896 px wide for sending while the frames the
   project keeps stay at the video's own size. A run that stopped mid-Describe
   resumes at the first unanswered chunk rather than re-describing what is already
   logged, and the pass ends with `event log complete (N chunks)`, naming how many
   of them came from the cache. A folder extracted frame-by-frame is refused rather
   than described — without a fixed interval there is nothing to stamp or resume on.
   Output is one EVENT line per frame, stamped on that frame's
   second; a frame in which nothing changed says only `same` and is folded
   into the line before it when the log is read, so a change is dated to the
   second it happened and a brief still reads one line per thing that happened.
7. **Fix** — the raw transcripts are cleaned into publishable text, grounded in
   the event log and in what other microphones heard at the same moment. Times
   and speaker labels pass through byte-identical, enforced.
8. **Merge** — everything above becomes one session timeline: every spoken line
   and every EVENT line on one clock.
9. **One word list for the session** — before anything is marked, the aligned (or,
   where there was no aligner, the recogniser's) word timings are glued into words on
   the session clock and saved once. A word put on almost no sound *and* landing well
   after the word before it — under a tenth of its recording's median loudness, more
   than a second late — is taken as placed on the wrong breath and timed at that
   previous word's end, so a clip ending there does not carry the silence with it;
   each one is named in the log. The list then carries the raw transcript's case and
   punctuation and the fix pass's spelling over the recogniser's bare words. Times
   stop changing here: from this point only how a word is written changes, never when
   it is, which is what lets the marks, the joins, `final.txt` and the captions all
   read the same words.
10. **Mark what was said twice** — the app asks whether any part of the talk is
   a retake of something said earlier, and marks those stretches abandoned
   rather than deleting them. Fewer than four spoken lines and there is nothing
   to compare, so nothing is marked — but the empty marks file is still written,
   because its presence is what tells the cut stage this question was asked. The
   brief the model is given numbers every line and flags each pause of 1.5 s or
   more, which is where a retake usually starts. The same question is put three
   times and the answers pooled and deduped, since one pass hears some repeats
   and misses others. A mark shorter than 0.3 s is treated as a breath and
   dropped, and a mark is trimmed back to the words the later take actually
   repeats rather than swallowing the sentence before it. If more than 40 % of
   the speech would be called abandoned, the whole thing is refused — that is a
   misreading, not an edit — and the refusal is the only line in the log.

Every stage is skipped if its output is already on disk, so a re-run resumes
rather than starting over. ⏸ parks the run between requests; ⏹ ends it.

The right-hand pane is **every prompt the app will send**, one menu row at a
time. The first row is not a prompt: it is what you want the editor to know
about this session — who is in it, how names are spelled, what has to end up in
the video, how long it should be — and every request this project makes carries
it. Behind it, in pipeline order, are all twelve system prompts. A ✎ beside a
name means yours differs from the shipped one; Reset restores it. Nothing is
saved by a button: every keystroke goes to disk as you type it.

Along the bottom of the window, `Inputs:` counts what Prepare has read — frames
to the describe model and transcript lines to the fixer — with the same
arithmetic broken down per file on hover; `Outputs: Prepare:` counts what the
page has written, its folder button naming the three subfolders the count
covers. Both refresh when you switch tabs.

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

A join's answer is checked before anything is removed. It must leave out one
stretch, next to the join: an answer that cuts two separate stretches, or more
than forty words, or more than sixty percent of what it was shown, is refused
and the stumble stays in, with the reason on the log. A refused join is asked
once more; if that fails too, nothing is removed there. And when a join takes
out every word of a recording, that recording is not silently dropped — it is
flagged yellow on the Cut page as gone whole, worth a look.

**Gaming** — a session where the interesting moments have to be *chosen*. The
cut model reads the whole session timeline and answers with the segments worth
keeping, to a target length if the context names one.

Both styles describe the frames, because the thumbnail is picked by what is on
one.

### Cut

The session on a timeline: thumbnails per camera row, a waveform lane per sound
below, everything the cut keeps tinted green. Time nobody filmed takes no width
at all, so two recordings meet with a striped amber border each.

One toolbar runs across the page in six groups, left to right: transport (▶ the
recording, ▶✂ the cut, ▶✂✂ every cut, and the frame steps), the preview volume,
the verbs (＋ Add, | Split, － Remove, ⧉ Copy, ⧉ Paste, Insert, ⇲ Lane), the
**✚ Effect** dropdown, history (↶ Undo `Ctrl+Z`, ↷ Redo `Ctrl+Shift+Z` or
`Ctrl+Y`, Revert, ✗ Clear) and zoom (**− +**). Each press scales by a quarter-step
and stops at both ends — in at 240 px a second, out where the whole session
fits, so you can never scroll past the end of your own recording. The **Thumbnails**
row in the form column carries its own 🖼− / 🖼+ ladder between 40 and 160 px, and
the page opens at 64 px.

Picking an entry under **✚ Effect** adds that kind of effect at the red line with its kind's own
default: a bare caption lands in the lower third, a drawing in the middle, a volume on the whole bed
until you point it at one lane. It starts two seconds wide — drag it to the length you want — and
**↶ Undo** takes it back.

**⊕ Zoom** is the one entry that does not add anything when you pick it — it waits for you to draw
the box. The picture's camera layer steps aside so you can see the whole source, and you drag a
rectangle wherever the close-up should be; let go and the form opens, titled with the second it
belongs to, where you say how long the move lasts and how it ends: **Pull back** for a passing
close-up that opens out again on its own, or **Stay on it** for a reframing that keeps that region
from there on (that is how a vertical short is cut out of widescreen footage). A camera that stays has
no way back, so its fade-out is greyed out rather than left for you to get wrong. Press Esc before
drawing anything and nothing happens at all. Every other effect still lands straight away.

**Aspect ratio** in the form column is the shape of the finished video: the footage's own, or 9:16,
1:1, 4:5 and 16:9. Picking a shape nobody has framed yet does two things at once — it stores that
shape and puts a whole-frame zoom at the very start, so the video has a framing from its first frame
instead of opening on bars. If the lane already holds a zoom that stays inside the new shape, your
zooms are left alone and keep deciding what shows; the shape still changes. One **↶ Undo** takes the
shape and the starting zoom back together. The outline drawn on the preview is what the finished video
shows, not a guide line.

**⏩ Speed** works on whatever you marked, as long as the mark is at least a fifth of a second — then the
effect's seconds are your seconds and it opens at half speed. With no mark but the red line down, it
becomes a two-second stop right there. The form asks six things: the rate, what the sound does, how
long, the fade in, the fade out, and the curve. **×0 means a hold**, not a very slow clip: the picture
stands still while the clock runs, and the footage underneath keeps running normally. If you marked a
stretch too short for a rate to fit without going under what the render can cut, the **rate gives way
instead of your mark moving** — your seconds stay where you put them. Under the sound choice a note tells
you what that answer costs: how many seconds the sound ends up behind the picture, and that going back in
sync skips those seconds. Esc or Cancel leaves everything alone. One **↶ Undo** takes the whole effect
back. Where a fast stretch and a slow one overlap, they don't fight — the two rates average across each
span, so overlapping effects blend rather than one winning outright.

Below the camera rows, the **effects lane** shows one row per group of effects that overlap in time,
each kind in its own colour so a zoom never looks like a caption, and a staying zoom is coloured apart
from a passing one. The lane keeps its height even when nothing is on it: an empty lane is the ordinary
state, not a collapsed one. In the paused preview everything outside the camera rect is dimmed, the
overlays draw at the see-through-ness their fades give them, and the one you are holding draws full with
a dashed violet outline so you can tell it apart from what is already placed. While a delayed sound
answer is still running behind the picture, a plate reads `sound 2.0 s behind` (or `ahead`) on the bar —
but only where the bar is wide enough to hold the words; a narrow bar says nothing rather than truncating.

History answers for what you changed on the page. **↶ Undo** walks back one step and
says how many kept stretches are left; **↷ Redo** walks forward again, and `Ctrl+Z`,
`Ctrl+Shift+Z` and `Ctrl+Y` all reach the same walk as the buttons do. **Revert** goes
back to the last suggestion — or, if nothing has been suggested this session, to what the
page opened with — and your own edits come back with Undo afterwards. When there is
nothing to go back to, both of them say so plainly instead of pretending to move: "nothing
to undo", "nothing to revert". **✗ Clear** takes every kept stretch and every effect off
in one step, and leaves your recordings, rows, shifts and lanes exactly where you put
them; it refuses when there is nothing on the page to clear. The walk holds its last 50
states, so a long session loses its earliest steps first, never the whole history.

The page opens as soon as there is footage — you can lay the session out, see
where takes fall against each other, shift a recording by hand and listen,
before anything has been transcribed.

▶ asks the model for a cut — or, when your policy says **cut by the words**, builds it from your marks
with no model at all. It refuses before asking anything when it cannot do the job: while another run is
going, while you have hand edits ("press Revert first for a fresh suggestion"), or before Describe has
produced a session timeline. With a target length it aims the cut at that window; with none it keeps what
is worth keeping, and it only asks for the captions, speed and decoration passes your policy switched on.
Whatever comes back lands as one Undo, keeps the inserts you placed by hand, and replaces the effects with
the ones that still sit on footage the cut actually keeps.

The page also carries its own **▶ Play the recording** control: it plays from the red line and lets
every second through, cuts and all. Pressing it again pauses; ⏹ ends what it started. Reopening the project
brings the red line back to where you left it, read from `cut/line.json` beside the cut — unless the
footage no longer covers that second, in which case the page simply starts at the beginning. And
closing the window always saves the line, even a moment after the last automatic save,
so the position you closed on is the position you open on. Beside it sits **▶✂ Play the cut**, which
plays the finished video instead: removed stretches are skipped, the line jumps to
the next clip's first playable second and stops past the last one, speed effects
hold their flat rate, a stop shows its still and volume applies — and the clock
reads the cut's own time, not the session's. Pressing it while the preview is the
recording switches over and snaps the line onto kept material ("preview is the cut —
the clock reads the finished video"); pressing it during a review ends the review
and carries on as the plain cut. With no clips there is nothing to skip to, so the
button is greyed and every other way in (Space, a click on the picture, the run
bar) answers "the cut is empty — add a clip to play it, or press ▶ to play the
recording instead". And **▶✂✂ Review every cut** hears every splice in one sitting without watching the
minutes between them: ten seconds of the finished video before each join and ten after it, one join
after the other, pausing with "reviewed all N cuts" once the last has been heard. It starts wherever
the red line stands — inside a join's window it plays on, between windows it seeks to the next run-up,
past the last join it wraps to the first — and it ends if you move the line by hand ("the line was
moved — the cut review is over; ▶✂✂ starts it again"). With fewer than two clips there is no join to
hear, so it says "nothing to review — a cut needs two clips to have a join between them". Each of the
three buttons wears ⏸ only while its own thing runs; pressing another switches the preview without
stopping it, and exactly one is lit at a time.

Under them sits the **preview volume** slider, labelled and 0–100. It is one setting for the whole app: every place
it is shown mirrors the same number, and it changes nothing that gets rendered — the render mixes what
each scene hears at recorded levels, so how loud your preview was is never in the file. What you hear
per second follows the scene under the red line: every overlapping sound (a camera's own, a separate
recording, an extra track) plays unless that scene silences it, a silenced lane is never started rather
than played quietly, and a muted picture goes quiet by mute, not by stopping anything. Volume effects
and speed over the line raise the gain and set the rate as you play over them.

Below that is where you **select**: a left-drag draws a band scoped to whatever ground it was drawn on —
that row's footage, or one recording's sound — and the `Selection:` readout under it follows the band live.
Where the drag STARTS is what decides what you get: pressing on a picture row selects that camera's footage,
pressing on the ruler selects the whole timeline's footage, and pressing on the effects lane takes no
selection at all — it puts down the effect you were holding. Once started, a drag keeps that ground even if
the pointer wanders across another band, so the readout never changes its mind about what you drew. The band's
ends resize, its middle moves, and **✕ Clear selection** takes the selection away and nothing else; the cut
keeps every clip. Ends snap to clip borders, recording ends, effect ends and the red line when released
near one. selection of *sound* rather than pictures greys ＋ Add, | Split and － Remove and points ⧉ Copy
at the sound instead, which comes back as a laid-over lane rather than a cut. Until the tracks themselves
are drawn this drag runs on a placeholder
strip above where they will sit, so it selects across the whole session rather than one row.

Under the readout sit those three verbs. **＋ Add** keeps the selected stretch as scenes — one per filmed
run, both ends snapped to a word edge, a silence or a visual cut within five seconds — and takes that span
off every other camera; the status line says whether anything was taken from another camera, and
↶ Undo (Ctrl+Z) gives it back. **| Split** puts a border at each end of the band and removes nothing:
the band stays up so you can see what you asked for, and the right half is marked so no automatic pass
joins it back. With nothing selected, | Split cuts once at the red line and hands you the second half.
**— Remove** drops exactly the selection; remainders of at least a frame survive either side. Every one of
them answers in words when it cannot act — a band under one second is too short to add, a sound selection
cannot be added to, split or removed, and a stretch the cut already keeps none of has nothing to drop.

**⌦ / Delete / BackSpace** works on whatever is in hand, in this order: a held effect, then a held clip
(the only way to remove a spliced card), then the selection, then the scene under the red line — and if
none of those is there it says so rather than guessing.

**⧉ Copy** takes the selected stretch into hand, where it stays until you use it or let go of it. A band
under a second is refused — there is nothing worth copying that short — and taking a copy leaves the band on
screen, because copying is reading, not editing. **⧉ Paste** puts what is in hand at the red line. Copied
*footage* comes in as a spliced `copy:<seconds>` card and the video gets longer, with the status line
naming both totals: what the cut is now and what it was. Copied *sound* is laid over the kept footage as
one piece per kept stretch, and refused outright when the cut keeps no picture under it rather than being
hung in silence. Either way the paste consumes the copy, so the same seconds cannot be pasted twice.
**⇲ Lane** gives the copy a row of its own — `Copied`, then `Copied-2`, `Copied-3` past every name already
taken — starting at the red line with nothing cut to it; ＋ Add is what lays scenes on that row later. It
refuses a line no recording is still rolling at. **Esc** drops whatever is in hand, and only that key, so
typing anywhere else is untouched.

The toolbar has three ▶s of its own: plain ▶ plays
the recording, every second of it; ▶✂ plays the cut, removed stretches skipped,
with the clock on the finished video's time; ▶✂✂ reviews the cuts — ten seconds
of the finished video before each join and ten after it, one join after the
other, stopping after the last — so every splice is heard in one sitting
without watching the minutes between them.

From then on the cut is yours: drag to select, ＋ Add,
✕ to drop, ⌦ for whatever is in hand, Revert to go back to the suggestion.
**Trim a clip edge by dragging it** — with either button, on the pictures or on
the green bar above them — or ‹f and f› a frame at a time once it is a clip in hand.
The strip you drag on shows the session's ruler along the top, marked every whole
second with its time, and under it the kept stretches of the cut as one green bar
with a visible border where each clip starts and ends. A press takes a border when
it lands within 6 px of it; that is how near you have to aim.
The right button is for *moving* things instead, a scene along its
recording or a recording along the clock. Everything snaps to word edges,
silences and visual cuts, with the pointer showing what a press would grab.

Trimming stops where the cut would break: an end won't go below a second of scene, past the next clip, or
past the recording's end, and a start won't go above a second short or before the previous clip. Drop a
border flush against a neighbour (within about a frame) and the two join back into one scene — same camera
only, never a card — with **↶ Undo** putting the border back; otherwise the clip simply lands on the edge
you left it at, and the status line reads "clip N: 00:40 – 01:24 (43.5 s)".

What the right button takes depends on where you press, in this order: a recording on the recorders' band,
then the wave strip under the pointer, then the marked scenes if you pressed inside a footage selection
(unless you hit a border, which trims), then one scene sliding along its own recording with its length kept
and clear of its neighbours, and failing all of that the whole camera row along the clock — which is how a
shift correction between two devices gets applied. A row change says "<what> moved to row 3 — its kept
scenes came along"; anything else says "<what> moved +1.23 s", or "<what> is back where it started" if you
ended where you began. Folds in the way of the drag open while it runs and refold after. One undo covers the
whole gesture, however many times the mouse moved, and a right click never moves the red line.

What a right-press grabs depends on where it lands: on the recorders' band or on a wave strip it takes that
one recording, inside a footage selection it slides the scenes you selected, and on the green bar or a clip
it slides that scene along its own recording with its length kept, snapping flush against a neighbour it came
within 8 px of. Press on a border instead and either mouse button trims it — the border wins over the slide,
which is why a slide only happens away from one.

You can also splice in cards, stills and sounds, switch which camera a scene is
shown from, silence a lane for one scene, and add effects — zoom, hold, speed,
volume, captions, drawings — each proposed by its own model pass after the cut
and each editable by hand.

Each camera of the session gets its own coloured row, and two recordings that overlap in time never share
one: dragging an overlapping recording's pin gives it a row of its own, and every other clip on it moves
with it. Clicking a row makes the preview watch that camera — the picture comes from there until you press
▶, which hands playback back to the cut itself. The row's name plate carries the shift correction between
its clock and the cut's (so `cam1 -19.00 s` reads as "camera 2 sits 19 seconds earlier"), and while a
row is watched the status line says so once when the red line reaches a scene kept from another camera.
On each row, the 🔍 badge picks which camera shows the scene under the red line; the 🔈 badge silences
that microphone for this scene only, and the gutter switch beside it silences the same lane for the whole
cut. Silencing a lane takes it out of what you hear but never out of the cut.

Time the cut throws away can be folded out of the way too. Press **−** over a stretch the cut drops — a hole
between two kept clips, or the head or tail of what was filmed — and it collapses to a seam, the clips either
side meeting where the gap used to be. Press **+** on that seam and it opens again. The gutter badge does every
gap at once: one press folds them all, the next unfolds them all. A gap too narrow to fit the badge gets none,
so you are never offered a control wider than the thing it folds. Folding changes nothing about the cut — it is
a view, not an edit, so undo leaves it alone and nothing measured with it moves — but the page remembers it:
reopen the project and the same stretches are still folded. An emptied bottom row stays on screen until you press
its ✕, and a cut lane's ✕ takes the lane, its pin, its shift, its pictures and its sound with it.

**Insert** puts something the session never recorded into the cut — a sting, a still, a tier board, a song.
Click where it goes (a red line or a marked band answers "where"; with neither, Insert says so rather than
guessing) and the chooser opens in `assets/`, where the built-in cards are written the first time you ask.
Three ways to place it: **BETWEEN** the footage cuts the clip open for the card, so the video grows by the
card's own seconds while costing no session time; **OVER** takes exactly the seconds it runs, the same as
－ Remove, and only the picture changes if you leave the sound running under it; **LANE** is for video only
and cuts nothing — it adds a row you can later cut to. A sound has no third way: it is laid over the kept
footage, one piece per stretch, each resuming where that piece stands, and it refuses where there is no
picture under it. A card's own holes come out of the file, so the form asks for each declared field by name
(with a Logo… picker where the card wants a mark), and what you type rides on the end of the path, which
makes one file a different card every time. Hold a card on the track — right-click or double click — and
Insert becomes **Edit**, re-opening that card's form; if it left the cut while the dialog was open, it says
so plainly instead of editing a ghost.

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
`AWV4` waveform cache and `requests.tsv` — the Settings dialog of
[03-shell.md §5](spec/03-shell.md), opened from the header bar's gear: rows for
the LLM server, key and model with its vision check and Fetch models, ffmpeg and
firefox, the audio.cpp endpoint with its four model rows and the forced aligner,
and the sd.cpp server. Each Test button reads what its box holds at that moment,
marks ✓ or ✗ with the verdict as its tooltip, and mirrors its line into the main
log as `settings: …`; "Test All" runs all six kinds and reports each one without
stopping at a failure — and the four servers of
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
output moved, or refreshes Produce's readouts and publish panel. Those two rows —
the page's `Inputs:` count and its `Outputs:` count with the folder button beside
it — sit under the bottom bar on every tab, not only on Prepare, and they are
redrawn each time you switch. The ⓘ that opens the editing-policy form is now one
button in the title bar, to the left of ⚙ ⟳, and its tooltip names the tab you are
looking at. And what one
means ([F0.2](spec/03-shell.md)): a run under way first, so the same button shows
⏸ and says "pausing after the current stage…" rather than starting anything; then
the page's own preview, but only once it has been started — Prepare and Produce
have none, so their ▶ runs the step while something plays; otherwise the sources
are snapshotted as they stand and that one step runs, with no "all done" line for
a single step. The window draws that ▶ at the left of its bottom row, with ⏹
beside it: one press stops the page's preview and the run together, ending on
"stopping…", and a subprocess killed by that stop is reported as stopped rather
than as a failure. Right of ⏹ sits **I'm feeling lucky** ([F0.4](spec/03-shell.md)):
one press runs every step in page order — Prepare → Cut → Narrate → Produce — each
switching to its own page and logging `>>> run: ‹Name›`. Narrate skips itself when the
video has no narration, and Cut skips itself when the cut has hand edits, which are
kept; the press refuses if a run is already going. It closes with what it cost —
`all done in 2m 15s`, or `stopped after …` with how many steps were left undone.
Every run, whichever button started it, keeps its own bookkeeping
([F0.5](spec/03-shell.md)): the bar fills while it works and reads
`‹job› ‹phase›: ‹task› n/m`, the status line carries the same words, the log
opens by itself when the run starts and stays open after it ends so the last line
is still there to read, and the audio models are unloaded quietly at the end of
every run — even one that used no audio.
And which project a launch
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
nothing added". The copy counts on the run bar as it goes — the bar means how much of the import is
on disk, so it moves with the bytes rather than with the rows — and each accepted file appears in the
sources list straight away, beside the rows that were already there. And ⟳ Rescan, at the right end of the header bar
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

**The editing policy** ([F0.7](spec/03-shell.md)) is what replaces that missing Style
dropdown, and you see it by pressing ⚙ on the bottom bar or ⓘ in the title bar.
The form lists all five fields, each with its value, who set it, and why.
Who set it is one of three words: `default`, `model`, or `user`.
With no User Context written, every field sits at its default — retakes marking, a model-chosen cut, and captions, speeds and decorations all on.
Write some context and the app asks the model (thinking off) to set only the fields your words actually speak to, storing each with the reason it gave, which is the sentence printed under that row.
A field you set yourself is never overwritten by a later derivation: it keeps your value and reads `set by you`, whatever the model proposes next.
An empty context stops the derivation entirely rather than inventing a style out of nothing.
These five switches decide which flows run at all: the marking pass picks retakes, join repair or no marking in Prepare; the cut mode picks the words-derived cut or the model-chosen one; and the three pass switches turn captions, speeds and decorations off so those jobs are never asked.
Change the marking pass and the marks already made go stale — `retakes.tsv` and `final.txt` hold what the previous pass decided — so Prepare runs the new pass instead of trusting them.
Re-opening the form re-reads the live policy, so what you see is what the project holds now, not when you last looked.
The reason sits in its own fourth column beside the value and the source, under a note that the page is the whole §2 catalogue.
Three buttons sit at the bottom: **Re-derive** asks the model again right now, **Reset to defaults** puts every field back at the shipped default — still open to a later derivation, not frozen as yours — and **Close** dismisses the form.
A new context is derived from on its own: about half a second after you stop typing in the User Context, before ▶ reads the policy, or whenever you press Re-derive; if the model is unreachable nothing changes and the status line says which one was missing.

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
jumping forward onto the next sound. The envelopes are read from `cache/waves/<take>.wave`, checked
against that recording's own size and modification time: a cache made before the file was replaced is
thrown away rather than trusted, and a take with no cache at all falls back to the word times, so a
missing envelope never fails the run.

The text itself is open to you: `prepare/transcript/final.txt` is edited in any text editor — there is no
editor in the app — and the next Cut ▶ notices it is newer than the marks and remakes them from what you
left in it, asking no model. A word you typed that nobody spoke is left out with a warning, since there is
no sound for it, and an edit that removes more than 40 % of the words is refused as not an edit: nothing
is marked, the marks that were there stay, and the refusal is what the status line says after the press.
Either way the remake happens once — the remade `retakes.tsv` is newer than the text from then on, so a
later ▶ finds no edit to make. The word times this works from are read per source out of
`prepare/inputs/<base>/words.json`, keyed on the recording's base name without its extension. Join
marks you leave in the file (`|cut 3|`, `|cut|`) are read as marks, never as words, so leaving them in
or deleting them gives the same cut; a word nobody spoke counts on top of what you deleted rather than
standing in for one, so it is dropped with a warning and does not by itself tip the edit past the 40 %
refusal.

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
    word_list.json                  the session's one word list: every word glued onto
                                  the session clock, with a word put on almost no
                                  sound moved to the end of the word before it and
                                  the fix pass's spelling printed over the recogniser's.
                                  Times never change after this — only how a word is
                                  written — so retakes, joins, `final.txt` and the
                                  captions all read the same words. Written once; a
                                  re-run keeps it rather than rebuilding.
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
both recipes are how the app is seen here. A screen that is really a dialog —
`03-new-confirm`, `03-policy-form`, `03-settings` — paints the dialog itself
rather than the window behind it, matching its spec image; the settings dialog
shows blank badge cells there because nothing has been probed in a snapshot run.

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
