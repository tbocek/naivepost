package main

// Cut: the session timeline. Thumbnails with the kept stretches tinted green,
// the waveform lanes under them. Time nobody filmed takes no width at all, so
// two recordings meet with a border each; folded gaps likewise (cut_fold.go).
// cut/cut.json holds the segments in session seconds.

import (
	"context"
	"encoding/json"
	"fmt"
	"image"
	_ "image/jpeg" // frames are jpeg; the decoder registers itself with image.Decode
	_ "image/png"
	"math"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/diamondburned/gotk4/pkg/cairo"
	"github.com/diamondburned/gotk4/pkg/gdk/v4"
	"github.com/diamondburned/gotk4/pkg/gio/v2"
	"github.com/diamondburned/gotk4/pkg/glib/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

const (
	rulerH = 18 // tick zone on top of the picture track
	// the selection band between the ruler and the thumbnails (drawSelBand). 22,
	// not 16: the green bar wears a 14 px plated ✕ (drawKillBadge) that has to
	// sit inside the row.
	selBandH = 22
	laneGap  = 3   // px between two cameras' rows of the picture band
	snapTol  = 5.0 // seconds the Add edges may move to find a better cut point
	// how far inside its own pictures a recording's border is drawn: half the
	// line, so the two pixels of it lie on the footage and none on the seam
	srcEdgeIn = 1.0
	// ...and how deep into them the stripes behind it reach (srcEdgeMark).
	// Wide enough for a diagonal to read as a diagonal, narrow enough that two
	// of them are a mark on the footage rather than a band across it.
	srcEdgeW = 9.0
	talkPad  = 0.2 // ...and how close to a word still counts as inside it
	minSegLn = 1.0 // segments shorter than this are dropped when editing
	undoDeep = 50  // how many edits back Undo reaches
	edgeGrab = 6.0 // px either side of a clip edge that hovers and trims it
	// how far the pointer must travel before a press on something held is a
	// DRAG of it rather than a click on it. Nobody double-clicks without
	// moving the mouse a pixel or two, and every one of those pixels used to
	// be a move: the second press landed on a clip the first had taken in
	// hand, the wiggle slid it, and the picture jumped to the clip's start to
	// show where it had gone. A few px of stillness is what a click is.
	dragSlop = 4.0
	// px either side of the HELD edge that a press takes hold of it by. Wider
	// than edgeGrab because by then you are aiming at something you can see --
	// the white bar -- and because missing costs more: a press that lands clear
	// of it drops the edge and starts a selection over the clip you were in the
	// middle of trimming.
	edgeMove = 12.0
	// the smallest a spliced insert's marker is drawn: it is a POINT on the
	// session timeline (see cutSeg) but has a length (spliceSpan), and zoomed out
	// a two-second card would be a few px of violet. About the size of the hatched
	// hole between two recordings, on purpose.
	splicePx = 22.0
	// px a dragged clip snaps to its neighbour within. Small: it has to be
	// reachable by hand and it must not swallow a deliberate one-second hole
	// between two clips.
	snapPx = 8.0
	// live scrubbing while an edge is dragged. A flushing accurate seek decodes
	// from the previous keyframe, and a drag fires one motion event per frame of
	// the UI; asking gstreamer for sixty of those a second gets a picture that
	// lags the mouse by more the longer you drag. One every scrubEvery keeps up,
	// and the drag's end always seeks exactly where it landed.
	scrubEvery = 90 * time.Millisecond

	// How the run bar is shared between Suggest's three calls. Choosing gets
	// the most because it is the long one -- the whole timeline, thought over,
	// and asked again up to three times -- where the captions and the effects
	// see only the kept clips and answer in a minute.
	suggestChooseShare = 0.7
	// the most segments the cut prompt asks for. Only a fallback denominator,
	// for a reply whose segments have no readable end to place them by.
	suggestMaxSegs = 20.0
)

// One paragraph or bullet per line, unwrapped: see describeSystem.
//
// Written for a ~27B local model: one narrow job, every rule load-bearing, the
// job as a procedure rather than as taste. There is one wording; what kind of
// video a session is belongs in the user context, which outranks it (ctxRule).
const cutSystem = `You choose the moments a video is cut from. The recording is a session of something happening -- a game, a build, a lesson, a conversation, a drive -- and the USER CONTEXT says which, when it says anything at all.

Work in this order.

1. Work out what this session is before you choose anything. The user context usually says it in its first sentence; with none, the first minutes of the timeline do -- what the speakers say they are doing, and what the EVENT lines keep showing. Cut for THAT, not for what a session like it usually contains. This is for you, not for the answer: the answer is segments and nothing else.

2. List what the user context names: every thing, moment, person or part. Each one gets a segment where it happens on the timeline. Find the place by the speech and the EVENT lines around it, and take the whole beat -- the setup, the thing itself, and what came of it, even when they are minutes apart. These come first and are never dropped for length.

3. Fill the rest of the target with what would be missed if it were gone: the thing working, the thing failing, the point being made, the answer to a question asked earlier. What people responded to outranks a competent stretch nobody said anything about. Take something from every part of the session, not only the start.

4. Shape the whole. The first segment establishes what this is: wherever the speakers say what they are doing or what they are after. Set busy stretches (EVENT says hectic) against calm ones. Finish on something that reads as an ending -- the result, the verdict, the last word.

Each segment starts a beat before the first word you want and ends after the reaction to it. About one segment per 20 seconds of target length, never fewer than two.

Every line is stamped with the seconds it STARTS and the seconds it ENDS: [733s-745s | 12:13] runs from 733 to 745. A SPEAKER line is a sentence or a phrase being said for the whole of that range, so a boundary inside it cuts a sentence in half -- the stretch you drop begins after a line's end and finishes before the next line's start, never between the two numbers of one line. The EVENT lines describe the picture and say nothing reliable about when speech stops: where an EVENT line and a SPEAKER line disagree about whether someone is talking, the SPEAKER line is right.

A line reading (abandoned attempt ...) has ALREADY been taken out of the video for you, to the word. Read it as though it were not there: the line before it and the line after it are one continuous sentence.

This does not change your job. You still choose which stretches the video keeps and which it drops, here as everywhere else, and a session with no target length still has silences, dead ends and stretches worth nothing in it that it is YOUR job to leave out. Answering with segments that run end to end over the whole session is not a cut. All the marker means is that you never have to aim a boundary at one: that particular cut is already made, more exactly than a boundary of yours can be.` + cutReply

// fxRules is the effects pass's wording (reply shape: cutReply); the user
// context leads because "speed up the boring parts" is about segments too.
// speedSystem is the speed pass's wording: a rate per clip from its own lines,
// after the captions pass (a captioned clip stays at 1), with no target -- how
// much to speed up is the user context's to say.
const speedSystem = `You decide how fast each clip of a finished cut plays. The clips are chosen and are not yours to change; you answer with a rate for the clips that do not play at 1, and nothing else.

Under each clip are its lines: what was said, and what the frames showed, at the seconds they happened. Read them for dullness -- a stretch with no lines is silence, a run of lines describing the same thing is the same thing going on and on, and either is a candidate. A line marked CAPTION is words already on screen.

Speed is for the stretches nobody would sit through at 1: the walk back, the loading screen, the same action done again, the wait for something to happen. Where the USER CONTEXT says how much to speed up -- a share of the video, a length it should play down to, a rate for the dull parts -- that is the brief, and you choose which clips deliver it. Where it says nothing, speed only what would bore, and leave the rest at 1; a cut with nothing dull in it gets no rates at all.

- The longer the dull stretch, the higher the rate: a few seconds at 2, a minute at 4 to 8, ten minutes of nothing up to 100. A short clip at 20 is a flicker, and a long one at 2 is still a long wait.
- A clip with something spoken over it plays at 1. The video plays that sound, and speech at 4 is noise. Go past this only when the user context asks for more than the silent clips can give, and then take the clips that say least.
- A clip with captions on it plays at 1 whatever else is true: words on screen at 4 are gone before they are read. The request says which clips carry them.
- Below 1 is slow motion, and it costs seconds instead of saving them: 0.5 on a SHORT clip that is the moment the video is about -- the impact, the reveal, the disaster -- and nowhere else. Never on a clip tens of seconds long.
- A clip is one rate from end to end. Half a clip fast is two clips, and the cut is not yours to change.

Answer with SPEEDS.`

// fxRules is the wording of the effects pass, the third call: it sees the kept
// clips and what was said over each, and answers with zooms, stops and volume
// pinned to clips. Nothing to choose, no arithmetic.
//
// One paragraph or bullet per line, unwrapped: see describeSystem.
const fxRules = `You decorate a cut that has already been chosen. The clips are in front of you, with what was said and shown over each; you add the effects that make a moment land, and nothing else -- the segments are not yours to change, and captions and speed are written elsewhere.

Your three kinds. zoom punches in on the centre of the seconds it covers. stop holds the picture still while the sound runs on. volume sets how loud those seconds are, 1 as recorded, 0 silent.

- Few and deliberate: three or four across five minutes of finished video, each with a reason you could say out loud. Not one on every clip, and not none.
- That count is a DEFAULT, and the USER CONTEXT outranks it. Asked for more, write more; asked for none, write none.
- Pick the kind by what the moment needs, not by variety. Something important on screen and easy to miss -> zoom onto it. The one beat everything else was leading to -> stop. Sound that does not sit right against the rest -> volume.
- A zoom runs two to four seconds, onto the score, the face, the mistake, while the speech is about it. One stop per video, a second or two on the beat the video is about. volume for a stretch recorded too quiet or too loud, for ducking a background under a line that matters, and for muting seconds the user context says are not to be heard: 1 as recorded, 0 silent.
- Every effect names its clip and its seconds FROM THAT CLIP'S START, inside the clip: a zoom at 0 to 3 of clip 4 starts where clip 4 does.
- The captions are already written and are marked CAPTION: do not zoom past words the viewer is reading. A heading saying the clip plays at a rate is a stretch that was kept to be skipped through -- nothing to zoom onto and nothing to stop on.

Answer with EFFECTS.`

// effectsSystem is fxRules under the name the bench's conventions expect: a
// wording ends in System, and the registry test counts them by that.
const effectsSystem = fxRules

// captionSystem is the wording of the captions pass: the second call, clip by
// clip, once the cut stands. Every line said over a clip becomes a text effect
// in the clip's own seconds -- which is exactly the job that broke the single
// call every time it was asked there. Two hundred captions over a whole
// session is an answer no reply can hold and a reasoning no call can finish;
// ten captions over one clip is a paragraph.
//
// The default is every line, because the words on screen were what the person
// asked for. What the USER CONTEXT says about them -- fewer, cleaner, only the
// verdicts, none at all -- outranks the default, as everywhere.
//
// One paragraph or bullet per line, unwrapped: see describeSystem.
const captionSystem = `You put words on screen over a cut that has already been chosen. You are given a few clips, each with the lines spoken over it stamped in seconds from that clip's start. The clips are not yours to change.

Whether there are captions at all is the USER CONTEXT's call, and it is the only thing that decides it. Said nothing about them, caption nothing: answer with an empty list. Where it asks -- every spoken line, only the verdicts, only what is named on screen, one line per clip -- write exactly that and no more.

- A caption starts when its line starts and ends when it ends, in seconds from the clip's own start, and stays inside the clip: a line running past the end is captioned up to the end.
- Clean the words as a subtitler would: no ehm, no ehh, no stutters ("I I" is "I"), no repeated words, sentence case, the swearing kept. The speaker's own words otherwise, never a paraphrase.
- A line that is not about the video -- an aside to the editor, "cut this part" -- is never captioned. The user context says which lines are directions.

Answer with CAPTIONS.`

// cutReply is the end of every cut wording: where a segment may start and end,
// the length arithmetic, the reply shape (read by suggestParse, so spelled here
// only) and the check to run before answering. The tolerance it asks for is
// tighter than the one the run accepts (suggestWindow).
const cutReply = `

A segment ends on the payoff, never just before it, and a moment that only makes sense because of an earlier one takes that one too or neither. Segments run from about 8 seconds to a minute, longer where a stretch has to be shown but not watched -- keep such a stretch whole, as ONE segment: it is played fast afterwards, not cut into pieces with the dull seconds left out.

Answer with SEGMENTS, and nothing else in the reply. How fast each plays, what is captioned over it and what is drawn on it are asked for afterwards, clip by clip, once the cut stands.

Check before you answer: every segment has an EVENT line inside it, every start is later than the end before it, everything the user context names is in, and -- if you were given a range -- the footage they come to, end minus start added up, lands in it. Anywhere inside it is right; do not trim towards its middle. Given no range, there is nothing to add up: keep what is worth keeping and stop.`

// cutSeg is one piece of the finished video: a stretch of session S..E, or an
// insert (Ins) that plays in that slot. An overwriting insert costs the session
// seconds it covers; a spliced one sits at a point (S == E) and runs for Dur.
// length() is the one place the two spellings meet; splitSpliced is what the
// render walks. coalesce never merges an insert and keepFilmed never drops one.
type cutSeg struct {
	S float64 `json:"s"`
	E float64 `json:"e"`
	// the asset, absolute or relative to the project root. Empty for footage,
	// which is nearly every segment, so an ordinary cut.json is unchanged.
	Ins string `json:"ins,omitempty"`
	// how long a SPLICED insert runs. Only a spliced one has it -- an insert
	// that replaces footage runs for exactly the footage it replaces -- so an
	// ordinary cut.json is unchanged by this too.
	Dur float64 `json:"dur,omitempty"`
	// playback rate for footage: 0.5 is half speed. 0 or 1 is normal, so an
	// ordinary cut.json is unchanged by this as well. Only produceSegs writes
	// rates -- the editor's own segments never carry one.
	Rate float64 `json:"rate,omitempty"`
	// where an inserted SOUND starts inside its file. Nought for a file
	// chosen from disk, which plays from its own beginning, and the copied
	// second for a stretch of a lane copied out of the session -- which is
	// the whole of what makes a copy of sound different from a file.
	Ss float64 `json:"ss,omitempty"`
	// this insert brings no sound of its own (askInsertParams). Read by mode:
	// SPLICED, the insert plays silent; OVERWRITING, the footage underneath keeps
	// being heard and only the picture is replaced.
	Mute bool `json:"mute,omitempty"`
	// which row of the picture band this scene's PICTURE comes from. Nought on a
	// one-camera session. The row, not the file: a row is a camera and a camera is
	// several files; which FILE is a question about a second (pickVideoOn).
	Cam int `json:"cam,omitempty"`
	// for a sound laid over the footage: which recording it was put in place
	// OF. Empty is the answer a selection scoped to picture-and-sound gives --
	// the file stands in for everything audible, which is what overwriting the
	// sound has always meant. Named -- the selection was drawn in that lane --
	// it stands in for that one recording
	// and the rest keep playing under it. Meaningless on anything but a sound
	// insert, and an ordinary cut.json is unchanged by it.
	Lane string `json:"lane,omitempty"`
	// which lanes this scene does NOT hear, by the name every audio row on the
	// page already carries: a camera's own sound is its recording's name, a
	// separate recording is its file's. Both kinds go in one list because the
	// page shows them as one kind of thing -- a row with a waveform -- and a
	// scene that is "my voice only" has to be able to say so about both.
	//
	// The SILENT ones rather than the audible ones, for two reasons. A cut
	// written before this, and a scene nobody has touched, both come out as the
	// empty list, which is every lane playing -- what the render has always
	// done. And a lane that appears later (another source split) arrives
	// audible everywhere rather than silent everywhere, which is the answer
	// that can be heard and corrected rather than the one that is missed.
	Quiet []string `json:"quiet,omitempty"`
	// this clip STARTS at a border somebody made on purpose: | Split put it
	// there (cut_split.go). Two clips of one camera that touch are otherwise
	// one clip, and coalesce joins them the moment anything rearranges the
	// list -- which is right for two selections that turned out to meet, and
	// wrong for a border drawn deliberately to give a stretch its own camera,
	// its own sound or its own place in the order. The flag is what tells the
	// two apart. Nothing else sets it, so an ordinary cut.json is unchanged.
	Split bool `json:"split,omitempty"`
	// which SCENE this piece came from, stamped by the render's planning
	// (produceSegs), never stored. One scene becomes several pieces (splices, rate
	// boundaries), and only the sound asks whether two are the same scene
	// (cut_fxsound.go).
	Scene int `json:"-"`
}

// laneQuiet is the one reading of that list, so the page, the preview and the
// render cannot disagree about what a scene hears.
func laneQuiet(quiet []string, base string) bool {
	for _, q := range quiet {
		if q == base {
			return true
		}
	}
	return false
}

// hears is the question every caller actually asks.
func (s cutSeg) hears(base string) bool { return !laneQuiet(s.Quiet, base) }

func (s cutSeg) isInsert() bool { return s.Ins != "" }

// spliced is "the footage is not replaced here, it is cut open and continues
// after the card". Dur rather than S == E as the test: a footage segment can be
// squeezed to nothing by an edit and would then answer yes to the other one.
func (s cutSeg) spliced() bool { return s.Ins != "" && s.Dur > 0 }

// length is how long this clip runs in the finished video: a spliced card
// runs for its own Dur, slowed footage runs longer than the footage it shows.
func (s cutSeg) length() float64 {
	if s.Dur > 0 {
		return s.Dur
	}
	if s.Rate > 0 {
		return (s.E - s.S) / s.Rate
	}
	return s.E - s.S
}

type tlVideo struct {
	base     string
	path     string
	start    float64 // session time of this video's t=0
	wall     float64 // the same instant on the wall clock, for naming outputs
	dur      float64
	interval float64
	fps      float64
	frames   []string
	w, h     int     // pixel size, for the framing overlay's arithmetic
	pxOrigin float64 // display x of the video's left edge (at current zoom)
	lane     int     // which row of the picture band this one is drawn on
	// the second of the FILE this row starts at. Nought for a recording, which
	// is shown whole from its first frame; a cut lane is a window on a file and
	// may open partway into it (cut_lane.go).
	off float64
}

// at is the second of v's file that session-second t shows, and it is the
// question every -ss and every seek on this page is asking. Two things stand
// between the two clocks: where the recording sits in the session (start), and,
// on a lane the cut put there, how far into the file the row begins (off).
func (v *tlVideo) at(t float64) float64 { return t - v.start + v.off }

// sessionAt is the same sum read the other way: the session second at which this
// file's second local is on screen.
func (v *tlVideo) sessionAt(local float64) float64 { return v.start + local - v.off }

// ffprobeFPS reads the average frame rate; r_frame_rate lies on VFR captures.
func ffprobeFPS(path string) float64 {
	out, err := exec.Command(ffTool("ffprobe"), "-v", "error", "-select_streams", "v:0",
		"-show_entries", "stream=avg_frame_rate", "-of", "csv=p=0", path).Output()
	if err != nil {
		return 30
	}
	var num, den float64
	fmt.Sscanf(strings.TrimSpace(string(out)), "%f/%f", &num, &den)
	if den > 0 && num/den >= 1 && num/den <= 240 {
		return num / den
	}
	return 30
}

type cutEditor struct {
	a    *App
	vids []tlVideo
	segs []cutSeg
	// the separate recordings, drawn as waveform lanes under the cut. They are
	// not part of the timeline's geometry -- the footage is the master, and an
	// audio recording that starts before it or runs on after it is simply drawn
	// where it overlaps (see cut_audio.go).
	auds  []tlAudio
	waves map[string]*waveform // per recording, filled in by a background decode

	// the tracks are built from what Prepare wrote, and that is a
	// snapshot taken at reload time -- so anything that writes into those
	// folders, or changes which project's folders they are, leaves this page
	// showing the past. stale says so, and the page catches up when it is next
	// looked at (see refreshCut). Rebuilding on every arrival instead would
	// re-probe every recording for a tab click and throw away the undo history
	// for nothing.
	stale   bool
	pending bool // a catch-up is already queued for the end of this turn

	pps    float64 // pixels per second (zoom)
	lastX  float64 // cursor x, for zoom centering
	totalW float64
	// the stretches of the session that got filmed, and where each is drawn;
	// rebuilt by relayout. filmed is the runs (merged across cameras); spans is
	// those runs cut into the CELLS the page is laid out in (a folded gap splits a
	// run, cut_fold.go). "What did the cameras cover" is runs(); "where is this
	// second drawn" is xOf, which walks the cells.
	spans  []tlSpan
	filmed []tlSpan
	// which dropped stretches are folded, in session seconds. Kept with the
	// cut (cutFile.Folds) because a fold is about a gap between two of its
	// segments, and matched back to those gaps by overlap rather than by
	// identity, so a trim that moves a gap's edge keeps its fold.
	folds [][2]float64
	// the fold badge under the pointer, or -1 (cut_fold.go)
	foldHov int
	// ...and whether the fold-all control in the gutter has it (cut_gutter.go)
	foldAllHov bool
	laneN      int // how many rows the picture band is stacked into (at least 1)
	// nRows is the row count the band holds on to even when the HIGHEST rows
	// are empty. An empty row between two full ones survives relayout because
	// the pins hold the gap open; an empty row at the bottom has nothing under
	// it and would fold up the moment a drag vacated it -- before its ✕ could
	// ever be pressed (killRow). moveRow raises this floor, closeRow lowers
	// it, and 0 means no floor: the footage alone says how many rows there are.
	nRows int

	sel struct {
		t0, t1 float64
		active bool
		// which sound the selection is OF, by recording base name, or "" when
		// it was dragged on the pictures. The span is a stretch of session
		// time either way; what the verbs make of it is not. On the pictures
		// ⧉ Copy takes the footage and ⧉ Insert puts a card in the cut; in a
		// lane the same two buttons take that lane's sound and lay a sound
		// over those seconds, because a selection means the thing it was
		// drawn on.
		aud string
		// ...and, when it was dragged on the pictures, whether it is about the
		// picture ALONE: the frames without the sound filmed with them. Three
		// what a selection is OF is what it was drawn on: the pictures, or a
		// lane's wave. See selSnd (cut_audio.go)
		// for why that stopped being the same sentence as "these seconds".
		// Meaningless while aud names a recording, and cleared with it.
		pic bool
		// ...and, when it was dragged on the pictures, WHICH camera's row it
		// was dragged on. The green says what will be shown, so a selection
		// has to know which picture it is offering: ＋ Add on the second row
		// keeps the second camera for those seconds and takes them off the
		// first. Nought on a one-camera session, where there is only one row
		// to have drawn it on.
		lane int
	}
	// the copied selection, in hand: where the footage it plays again starts,
	// and for how long. Not yet in the cut -- ⧉ Paste is what places it -- so
	// it lives here and not in segs, and it outlives the selection it was
	// taken from: clearing the band does not empty the hand.
	copyFrom float64
	copyLen  float64
	copyOn   bool
	// which recording the copy in hand is OF, empty for footage. Kept beside
	// the seconds rather than read off the selection at paste time, for the
	// same reason the seconds are: the band may have been cleared or redrawn
	// somewhere else entirely by then, and the hand still holds what it took.
	copyAud string
	// ...and which camera's row those seconds were taken off. A copy plays
	// footage again, and with two cameras rolling "the footage at 4:10" is two
	// different pictures: without this the paste would come out as whichever
	// recording happened to be first in the list. Read at the same moment as
	// the seconds, for the same reason.
	copyCam int
	// the selection band's own hold, the same shape as the edge's and the
	// effect's: whether the band is in hand, and whether the pointer is over
	// it. Its row is cut_selband.go.
	selOn       bool
	selHov      bool
	bandHov     bool // the pointer is over the green bar's clip, ends included
	bandKillHov int  // ...and the bar whose ✕ it is on, which lights red; -1 for none
	// wheel zooming, coalesced: the deltas that arrived since the last frame
	// and whether a frame is already booked to apply them. See the scroll
	// controller in buildCut for why one wheel gesture is many events.
	zoomPend  float64
	zoomBook  bool
	scrollMut bool   // syncScroll is setting the adjustment; its value-changed is not a scroll
	selCur    string // the cursor name the source area last asked for
	audCur    string // ...and the lanes
	thumbHt   int    // thumbnail height; the 🔍 buttons change it
	srcHt     int    // the height the source area was last asked for; see fitSrc
	playhead  float64
	hasPlay   bool
	// the row the preview is WATCHING, plus one; 0 for the cut's own answer,
	// so a zero editor answers to the cut. Inside a kept scene the preview
	// shows the scene's camera (camAt), which leaves no way to see what
	// another camera saw at the same second -- and that is most of how a
	// scene gets stolen for it. A click on a row asks exactly that, and ▶
	// withdraws it: playback is the cut's (toggle).
	monRow    int
	player    *Player
	playVideo *tlVideo // which recording the preview is playing
	// the preview has been started and not stopped, which is what makes the run
	// bar its transport. Same rule as narrate and produce: a recording merely
	// LOADED -- which clicking the timeline does, just to show the frame there
	// -- must not take ▶ away from suggesting, which is what ▶ means here.
	started bool

	markIn, markOut float64 // editor-style in/out points, session time
	hasIn, hasOut   bool

	// the clip edge a press near a border has picked up: which segment, which
	// side of it, and whether this hold has moved anything yet. The undo snapshot
	// is taken on the first move, so picking an edge up and putting it back down
	// is not an edit. edgeOn rather than an index-or-minus-one because a zero value
	// has to mean "nothing held", and 0 is a perfectly good segment.
	edgeOn    bool
	edgeSeg   int
	edgeEnd   bool
	edgeDirty bool
	// the border the pointer is over, which is the one a left press would take
	// hold of. Held as its time rather than as an index because that is all the
	// picture of it needs, and because a hover is answered on every motion
	// event: two fields to compare against beats working out afresh which clip
	// the highlight belonged to.
	edgeHovOn bool
	edgeHovT  float64
	lastScrub time.Time // when the preview last followed a dragged edge

	// the whole clip a double click has picked up, held the same way and for
	// the same reason: an edge is what you grab near a border, and a clip is what
	// you grab anywhere else on one. A drag then slides it, keeping its
	// length, which is the edit that has no other spelling here -- "this scene,
	// four seconds later" used to be two edge drags that had to agree.
	segOn    bool
	segSel   int
	segDirty bool

	// The tracks are NOT a wide widget in a scrolled window (see drawTrack):
	// they are exactly as wide as the space they have, and this adjustment is
	// the window onto the timeline that they draw.
	hadj         *gtk.Adjustment
	hbar         *gtk.Scrollbar
	viewX, viewW float64          // scroll offset and width of that window, in timeline px
	srcArea      *gtk.DrawingArea // the footage, with the cut tinted green over it
	// the recorders' band: waveform lanes for the sound nobody filmed -- every
	// recording that is not some row's own track. A camera's sound is drawn
	// under its own pictures in srcArea instead (drawPairStrip), so in the
	// common cameras-only session this band is not there at all.
	audArea *gtk.DrawingArea
	// the red line's own transparent layer over both bands, so the playback
	// tick has something thin to repaint (cut_playline.go); lineIdx is the
	// scene the green bar stood on when the bands were last painted, the one
	// other thing on them the running clock alone can move
	lineArea  *gtk.DrawingArea
	lineIdx   int
	total     *gtk.Label // the cut's length, and the three readings under it
	totalRaw  *gtk.Label // ...the same cut with no speed effects on it
	totalSrc  *gtk.Label
	totalSegs *gtk.Label
	clock     *gtk.Label // the red line's time in numbers, beside the transport
	marks     *gtk.Label // the two marks in numbers, under the buttons that set them

	target *gtk.Entry
	inputs *gtk.Label // what this page reads, and what Suggest is sent
	out    *gtk.Label // what cut/ holds, the same line every other page shows

	// the form column beside the video (cut_form.go): its heading, the words
	// it shows when it is empty, the form in it and who to tell when that form
	// is taken out. formBox nil means no page has been built, which is what
	// every headless test is.
	formBox     *gtk.Box
	formHead    *gtk.Box
	formFoot    *gtk.Box // pinned under the scroller: the form's buttons live here
	formTitle   *gtk.Label
	formIdle    *gtk.Box
	formCur     gtk.Widgetter
	formFootCur gtk.Widgetter
	formGone    func()
	// the live effect form: which one is up (so a keystroke waiting out its
	// debounce cannot land on the form that replaced it) and which effect its
	// answers have already been written onto. See fxWin and fxLiveOk.
	fxLiveCur *fxLive
	fxLiveOn  *cutFx
	formArm   string // the kind whose "now draw it" note the column is holding

	// the frames on the video rows, ready to paint (cut_thumbs.go). Filled by
	// a worker rather than by the draw that wanted them: what a zoom needs is
	// a screenful of files nobody has read yet, and reading them where they
	// are drawn is what made the wheel lag.
	thumbs    map[string]*thumbPic
	thumbWant map[string]bool // asked for by a draw, not yet read
	thumbBusy bool            // a loader is running; the next draw asks again
	thumbGen  int             // bumped by setThumbH: work for the old height is dropped
	// the insert currently under the playhead, and its rendered frames. Nil
	// until the playhead first lands on one (cut_insview.go).
	film *insFilm
	// a spliced card playing with the footage stopped, and the insert whose own
	// sound the preview is playing. Both cut_insview.go's.
	hold    insHold
	cardSnd string
	scores  map[string][]float64 // per video: visual change per frame
	gaps    map[string][]float64 // per video: session-time speech-gap points
	// every stretch anybody was talking in, session time, in order. The gaps
	// above are the silences BETWEEN these; this is the other half of the same
	// reading, and what answers "is a word sounding here" (talking).
	talk [][2]float64
	// and the words themselves, timed by the aligner where there is one
	// (align.go). What a cut inside a phrase is placed by: the boundary
	// between two words, which no silence marks.
	words []srcWord

	// the two hand-made corrections to the timeline (cut_shift.go): how many
	// seconds each source's clock was out, and the rows as they were when the
	// first correction froze them. Both keyed by source base, both saved.
	shift map[string]float64
	rows  map[string]int
	// the rows the cut put on the band itself: copied or inserted material that
	// no recording is behind, and that Prepare has never heard of
	// (cut_lane.go). laneHov is the one whose ✕ is under the pointer.
	cutLanes []cutLane
	laneHov  string
	// badgeHov is the speaker or lens under the pointer, which lights blue
	// while it is (cut_hear.go)
	badgeHov badgeID
	// rowHov is the empty row whose ✕ is under the pointer, -1 for none
	// (cut_lane.go: an emptied row wears the same badge a cut lane does)
	rowHov int

	undo []cutState // one snapshot per edit; every edit is reversible
	redo []cutState // what Undo took back, so Redo can put it in again
	base cutState   // the cut at the last checkpoint; Revert returns to this

	// the camera and the clock (cut_fx.go): the cut's aspect ratio ("" is
	// the source's own), the effects, and which one is currently held.
	// Held the same way a clip is -- one thing held at a time, so taking hold
	// of an effect drops a held clip or edge and the other way round.
	aspect string
	fx     []cutFx
	// the preview plays the CUT rather than the recording: the stretches the
	// edit removed are skipped instead of played through, so ▶ shows what the
	// finished video will run. A view mode -- nothing here is saved, and the
	// track still draws every second of the session underneath.
	cutOnly    bool
	cutPlayBtn *gtk.Button // the second ▶: plays the CUT (cutOnly follows it)
	// the play/pause half of that button's face. An image and not the label,
	// because the label was two glyphs and the pause one falls to another font
	// -- see syncCutPlay.
	cutPlayIcon *gtk.Image
	// the clip a gap was last skipped to, so a jump that cannot be made is not
	// attempted again on every tick. -1 is "not in a gap"; see skipGap.
	jumped int
	fxOn   bool
	fxSel  int
	// which effect the pointer is over, and whether it is over one at all.
	// Purely a drawing matter -- nothing is held until it is pressed -- but it
	// is what makes a lane of shoulder-to-shoulder markers clickable.
	fxHovOn bool
	fxHov   int
	// the effect whose ✕ the pointer is on, or -1 (cut_fxkill.go)
	fxKillHov int
	// this hold has moved its effect; the undo snapshot is taken on the first
	// move, so lifting an effect and putting it back is not an edit
	fxDirty bool
	// an effect is being dragged along its lane right now. The line follows a
	// dragged effect, and syncFxHold must not read that as the line walking
	// away from it -- see there.
	fxMoving bool
	// the three layers of the finished picture over the preview -- the camera,
	// a stop's frozen frame, the mask and the titles -- and the smoothed clock
	// they are drawn on. Embedded, not held, because this page had all of it
	// first and every ed.fxArea / ed.livePlayhead() / ed.syncPreviewZoom() in
	// the file still means what it did; what changed is that the Narrate
	// preview now runs the SAME code (cut_fxscreen.go) instead of its own
	// second opinion about the same render.
	fxScreen
	// what the next drag on the video draws: "zoom", "text" or "svg" while
	// one of the effect buttons is armed, "" when the video is just a picture.
	fxArm string
	// the drawing an armed "svg" is waiting to place -- chosen before the
	// drag, because a box means nothing until you know what goes in it
	fxSrc string
	// the drawings already rasterized for the preview, by file (fxsvg.go)
	svgs map[string]*fxSVG
	// the pointer's current shape over the overlay, remembered so the motion
	// handler only touches the cursor when it actually changes
	fxCursor string
	aspectDD *gtk.DropDown // the toolbar's aspect choice
	aspectMu bool          // the dropdown is being set by code, not by hand

	undoBtn, redoBtn, revertBtn *gtk.Button
	playBtn                     *gtk.Button // ▶/⏸ for the preview; drawn by syncPlayIcons
	insBtn                      *gtk.Button // insert a file, or edit the card in hand
	pasteBtn                    *gtk.Button // put the copy in hand down (syncInsertBtn)
	// ⇲ Lane, which is on the bar only while a copy of footage is in hand: it
	// is the other place a copy can go (cut_lane.go), and a permanent button
	// for it would be greyed out for the whole of every session that never
	// takes one.
	laneBtn *gtk.Button
	copyBtn *gtk.Button // ⧉ Copy, greyed until there is a selection to take
	// ＋ Add: the footage's verb, greyed while the selection is a sound's (see
	// syncSelBtns).
	addBtn *gtk.Button
	// | Split, between them: the span kept AND cut free of what it lay in.
	// Greyed by Add's own rule, because it is the same kind of verb about the
	// same kind of selection (cut_split.go).
	splitBtn *gtk.Button
	// － Remove, the same span the other way round. It stood beside ＋ Add
	// once, guessed what it was aimed at, and was taken off the bar for it;
	// this one is the selection's verb and nothing else's (cut_selrm.go), so
	// it can cut a hole in a scene, which the green bar's ✕ cannot.
	remBtn *gtk.Button
	// ✗ Clear: the whole cut off the timeline at once. Beside Undo and Revert
	// (cut_clear.go).
	clearBtn *gtk.Button
}

// ---- data ------------------------------------------------------------------

// mmss is how this page says a duration. It was three identical closures in
// three functions before something outside them needed it too.
func mmss(t float64) string { return fmt.Sprintf("%d:%02d", int(t)/60, int(t)%60) }

func (a *App) cutDir() string  { return filepath.Join(a.outDir, "cut") }
func (a *App) cutPath() string { return filepath.Join(a.cutDir(), "cut.json") }

// cutFile is cut.json, whole: one shape for reload, persist and the render.
// Shift and Rows are the timeline's own corrections; they are this project's,
// not the files', and every step re-derives the placement from them.
type cutFile struct {
	Segs   []cutSeg `json:"segs"`
	Aspect string   `json:"aspect,omitempty"`
	Fx     []cutFx  `json:"fx,omitempty"`
	// READ ONLY, and kept only so an old project still opens the way it was
	// left: one lane the whole cut was heard on. Nothing writes it -- reload
	// spreads it across the scenes (migrateSound, cut_hear.go) and the next
	// save leaves the field out, which is what makes the move a one-way door.
	Sound string `json:"sound,omitempty"`
	// per source base: seconds its clock was out, as dragged by hand
	Shift map[string]float64 `json:"shift,omitempty"`
	// per source base: the row it sat on when the first drag froze the rows
	Rows map[string]int `json:"rows,omitempty"`
	// the rows that are not recordings: copied or inserted material given a
	// band of its own for the cut to reach (cut_lane.go)
	Lanes []cutLane `json:"lanes,omitempty"`
	// how many rows the band keeps even while the highest stand empty: a row
	// vacated by a drag waits for its ✕ across a restart too (cutEditor.nRows)
	NRows int `json:"nrows,omitempty"`
	// the dropped stretches that are folded away on the page (cut_fold.go).
	// A fold changes nothing about the video and is here all the same: it is
	// about the gaps between THESE segments, and a view of a long session that
	// had to be folded again on every open would be folded once and never
	// again. The render reads this file and ignores the field.
	Folds [][2]float64 `json:"folds,omitempty"`
}

// reload rebuilds the timeline from the current selection + step outputs.
func (ed *cutEditor) reload() error {
	a := ed.a
	vids, auds := a.snapSources()
	if len(vids) == 0 {
		return fmt.Errorf("nothing to cut — no source on the Prepare step is marked as footage")
	}
	type st struct {
		path  string
		start float64
	}
	// same zero convention as session.tsv: the earliest moment any source
	// names, and 0:00 when none of them names one (srcClock)
	var all []st
	paths := append(append([]string{}, vids...), auds...)
	at, zero := srcClock(paths)
	for _, p := range paths {
		all = append(all, st{p, at[p]})
	}
	ed.vids = nil
	ed.film = nil // another project's card is not this one's
	for _, s := range all[:len(vids)] {
		// no frames is not a reason to refuse the page: a recording is a lane
		// before it is a transcript, and everything the lane is made of --
		// where it starts, how long it runs, what shape it is, what it sounds
		// like -- is in the file. The frames are the pictures drawn ON the
		// lane, and without them the lane is drawn without pictures
		// (frameRange already answers nothing for a row that has none).
		p, err := a.planVideo(s.path, a.describeDir())
		if err != nil {
			p = &videoPlan{base: baseName(s.path), video: s.path}
		}
		dur, _ := ffprobeDur(s.path)
		vw, vh, _ := ffprobeSize(s.path)
		ed.vids = append(ed.vids, tlVideo{
			base: p.base, path: s.path, start: s.start - zero, wall: s.start, dur: dur,
			interval: p.interval, fps: ffprobeFPS(s.path), frames: p.frames, w: vw, h: vh,
		})
	}
	sort.Slice(ed.vids, func(i, j int) bool { return ed.vids[i].start < ed.vids[j].start })

	// Every sound in the session gets a lane, the footage's own first. It is the
	// master and it is the one already coming out of the speakers, and it is
	// exactly what a separate recording has to be read against: two waveforms
	// with the same shout in the same column is the page saying, without a word,
	// that the two clocks agree. Its lane is its own video's stretch of the
	// timeline, so there is no placing to do -- it starts where the video does.
	//
	// Nothing is decoded here: what this needs is where each one sits and how
	// many lanes it has, and the envelopes arrive later (below) without holding
	// the page up.
	//
	// One lane per track the Prepare row asked for, and not one per file: a
	// capture with the game on one track and a headset on the other is two
	// recordings that happen to share a container, and this is where they stop
	// being one (cut_tracks.go).
	ed.auds = srcLanes(ed.vids, a.snappedTracks())
	for _, s := range all[len(vids):] {
		dur, _ := ffprobeDur(s.path)
		ed.auds = append(ed.auds, tlAudio{
			base: baseName(s.path), path: s.path, start: s.start - zero,
			dur: dur, chans: max(1, ffprobeChannels(s.path)),
		})
	}
	sortLanes(ed.auds)

	// cut state; the undo history belongs to the cut that produced it
	ed.segs = nil
	ed.undo = nil
	ed.edgeOn = false
	ed.jumped = -1 // the clip a gap was skipped to belonged to the last cut
	ed.fx = nil
	ed.fxOn = false
	ed.cutLanes = nil // another cut's own rows are not on this band
	ed.nRows = 0      // nor its empty rows
	ed.folds = nil    // nor which of its gaps were folded away
	ed.setAspect("")
	ed.syncButtons()
	var c cutFile
	if b, err := os.ReadFile(a.cutPath()); err == nil {
		if json.Unmarshal(b, &c) == nil {
			ed.segs = c.Segs
			ed.fx = migrateFx(c.Fx)
			// a cut written when the sound was one choice for the whole
			// project, said the way this one says it (cut_hear.go)
			if segs, note := migrateSound(ed.segs, c.Sound, ed.vids, ed.auds); note != "" {
				ed.segs = segs
				ed.a.logf("cut: %s", note)
			}
			ed.shift, ed.rows = c.Shift, c.Rows
			ed.nRows = c.NRows
			ed.folds = c.Folds
			ed.setAspect(c.Aspect)
		}
	}
	// the hand-made corrections go on before anything is measured: the gaps
	// below, the rows and the spans in relayout, and every x on the page are
	// all read off these starts
	ed.applyShift()
	// and then the rows the cut put on the band itself, because that is what
	// they are: Prepare settled what was RECORDED, and this settles what
	// the cut has to reach for (cut_lane.go). After the corrections and not
	// before, because setLanes places them itself and applyShift would
	// otherwise move them a second time. Their waveform lanes come with them,
	// windowed to the rows the way the pictures are, which is why this runs
	// before loadWaves below rather than after it.
	ed.setLanes(c.Lanes)
	ed.setBase() // what is on disk now is the checkpoint this session edits from

	// speech-gap candidates: midpoints of silence between anything anyone
	// says, per video, in session time -- Add prefers cutting there
	ed.gaps = map[string][]float64{}
	var speech [][2]float64
	for _, s := range all {
		base := baseName(s.path)
		rows := loadSeg4(filepath.Join(a.transcriptDir(), base, "transcript.fixed.tsv"))
		if rows == nil {
			rows = loadSeg4(filepath.Join(a.transcriptDir(), base, "commentary.fixed.tsv"))
		}
		if rows == nil {
			rows = loadSeg4(filepath.Join(a.inputsDir(), base, "transcript.tsv"))
		}
		for _, r := range rows {
			speech = append(speech, [2]float64{s.start - zero + r.s, s.start - zero + r.e})
		}
	}
	sort.Slice(speech, func(i, j int) bool { return speech[i][0] < speech[j][0] })
	ed.talk = speech
	// the words, on the session clock, for the edges that fall inside a phrase
	ed.words = a.sessionWords(paths)
	for vi := range ed.vids {
		v := &ed.vids[vi]
		var pts []float64
		last := v.start
		for _, sp := range speech {
			if sp[0] > last && sp[0] < v.start+v.dur {
				pts = append(pts, (last+sp[0])/2) // silence midpoint
			}
			if sp[1] > last {
				last = sp[1]
			}
		}
		ed.gaps[v.base] = pts
	}

	ed.loadWaves()

	// visual-change scores in the background; snapping works without them
	// (speech gaps only) until they land
	if ed.scores == nil {
		ed.scores = map[string][]float64{}
	}
	for _, v := range ed.vids {
		if _, ok := ed.scores[v.base]; ok {
			continue
		}
		v := v
		go func() {
			sc := frameChangeScores(v.frames)
			glib.IdleAdd(func() { ed.scores[v.base] = sc })
		}()
	}
	ed.relayout()
	ed.updateInputs() // the recordings just changed, and so did their lengths
	return nil
}

// frameChangeScores diffs consecutive frames at postage-stamp size; local
// maxima are scene-change candidates.
//
// The decoding is Go's own and not GdkPixbuf's, deliberately: this runs on a
// worker goroutine (a long recording is hundreds of frames and the caller must
// not stall the window), and every pixbuf is a GObject whose reference
// bookkeeping gotk4 finishes back on the main loop. Making and dropping
// hundreds of them off the main thread raced that bookkeeping and corrupted the
// heap -- GLib said so by the hundred lines (g_atomic_rc_box_release_full:
// assertion 'real_box->magic == G_BOX_MAGIC' failed) before the abort landed in
// an unrelated finalizer. image/jpeg touches nothing but Go memory, so it is
// safe anywhere. Thumbnails on screen (ed.thumb) stay on pixbufs: those are
// made on the main thread, where they belong.
func frameChangeScores(frames []string) []float64 {
	out := make([]float64, len(frames))
	var prev []byte
	for i, f := range frames {
		px := framePostage(f)
		if px == nil {
			continue
		}
		if prev != nil && len(prev) == len(px) {
			sum := 0
			for j := 0; j < len(px); j++ {
				d := int(px[j]) - int(prev[j])
				if d < 0 {
					d = -d
				}
				sum += d
			}
			out[i] = float64(sum) / float64(len(px))
		}
		prev = append(prev[:0], px...)
	}
	return out
}

// postage size: what a scene change has to survive being shrunk to before we
// call it one. Small enough that camera noise and a wobbling hand average out,
// big enough that a person walking through the shot moves a box or two.
const postW, postH = 24, 14

// framePostage reads one frame down to a postW x postH grid of brightness,
// each cell the average of the whole box of source pixels under it, so a moved
// edge changes a cell instead of falling between two sampled points. It returns
// nil for anything it cannot read -- a half-written frame, a format the
// stdlib does not know -- and the caller then leaves that frame's score at zero.
func framePostage(file string) []byte {
	f, err := os.Open(file)
	if err != nil {
		return nil
	}
	defer f.Close()
	img, _, err := image.Decode(f)
	if err != nil {
		return nil
	}
	b := img.Bounds()
	if b.Dx() <= 0 || b.Dy() <= 0 {
		return nil
	}
	sum := make([]int, postW*postH)
	cnt := make([]int, postW*postH)
	// jpeg decodes to YCbCr, whose Y plane already is the brightness -- reading
	// it straight is worth it here, where the loop runs over every pixel of
	// every frame of the recording
	yc, _ := img.(*image.YCbCr)
	for y := b.Min.Y; y < b.Max.Y; y++ {
		row := (y - b.Min.Y) * postH / b.Dy() * postW
		for x := b.Min.X; x < b.Max.X; x++ {
			k := row + (x-b.Min.X)*postW/b.Dx()
			if yc != nil {
				sum[k] += int(yc.Y[yc.YOffset(x, y)])
			} else {
				r, g, bl, _ := img.At(x, y).RGBA()
				sum[k] += int((r + 2*g + bl) / 4 >> 8)
			}
			cnt[k]++
		}
	}
	px := make([]byte, postW*postH)
	for k := range px {
		if cnt[k] > 0 {
			px[k] = byte(sum[k] / cnt[k])
		}
	}
	return px
}

// ---- geometry --------------------------------------------------------------

// tlSpan is one filmed stretch of the session and where it is drawn. The axis
// is TIME, not the recordings laid end to end: two cameras rolling through the
// same minute share one x. Unfilmed time collapses to nothing.
type tlSpan struct {
	t0, t1 float64 // the session seconds this run covers
	px     float64 // where t0 is on the timeline
	// this stretch is FOLDED: drawn at no width at all however long it is, so
	// the footage either side of it meets (cut_fold.go). A filmed run is cut
	// into cells at its folded gaps, so a span is a run or a piece of one.
	fold bool
}

// dur is the run's length in seconds.
func (s tlSpan) dur() float64 { return s.t1 - s.t0 }

// timeSpans is that union: the filmed runs, in order, none touching.
func timeSpans(vids []tlVideo) []tlSpan {
	raw := make([]tlSpan, 0, len(vids))
	for _, v := range vids {
		if v.dur > 0 {
			raw = append(raw, tlSpan{t0: v.start, t1: v.start + v.dur})
		}
	}
	// ed.vids is sorted by start, but a lane shifted in time by hand is not,
	// and the merge below is only right on a sorted list
	sort.Slice(raw, func(i, j int) bool { return raw[i].t0 < raw[j].t0 })
	var out []tlSpan
	for _, r := range raw {
		if n := len(out); n > 0 && r.t0 <= out[n-1].t1 {
			out[n-1].t1 = math.Max(out[n-1].t1, r.t1)
			continue // this one carries on where the run so far had got to
		}
		out = append(out, r)
	}
	return out
}

// runs is the filmed stretches, laid out or not. relayout caches them with
// their pixel origins, and everything that draws reads that cache; anything
// that only wants the TIMES -- what got dropped, what a selection can cover --
// asks here instead, so it still answers on an editor that has no widgets and
// has never been laid out.
func (ed *cutEditor) runs() []tlSpan {
	if len(ed.filmed) > 0 {
		return ed.filmed
	}
	return timeSpans(ed.vids)
}

// assignLanes is which row each recording is drawn on: one camera's files share
// a row, a camera rolling through another gets its own. Greedy interval
// colouring in start order. pin (cutFile.Rows) holds rows chosen by hand, so a
// drag that ends an overlap does not collapse two cameras onto one row.
func assignLanes(vids []tlVideo, pin map[string]int) int {
	ord := make([]int, len(vids))
	for i := range ord {
		ord[i] = i
	}
	// ed.vids is sorted by start on load, but a row shifted in time by hand is
	// not, and colouring out of order would stack two recordings that do not
	// overlap
	sort.SliceStable(ord, func(a, b int) bool { return vids[ord[a]].start < vids[ord[b]].start })
	var rows [][][2]float64 // per row: the stretches already on it
	grow := func(r int) {
		for len(rows) <= r {
			rows = append(rows, nil)
		}
	}
	free := func(r int, s, e float64) bool {
		for _, iv := range rows[r] {
			if s < iv[1]-1e-9 && e > iv[0]+1e-9 {
				return false
			}
		}
		return true
	}
	put := func(v *tlVideo, r int) {
		grow(r)
		v.lane = r
		rows[r] = append(rows[r], [2]float64{v.start, v.start + v.dur})
	}
	// the written-down rows first and in full, so the greedy pass below sees
	// every row they claim, whichever end of the session they claim it at
	for _, i := range ord {
		if r, ok := pin[vids[i].base]; ok && r >= 0 {
			put(&vids[i], r)
		}
	}
	for _, i := range ord {
		v := &vids[i]
		if r, ok := pin[v.base]; ok && r >= 0 {
			continue // already placed above
		}
		// the lowest row that is already finished by the time this one begins.
		// Greedy in start order is optimal for interval colouring, and on a
		// session with no pins and no shifts it puts one camera's files back
		// on one row exactly as it always did.
		lane := len(rows)
		for r := range rows {
			if free(r, v.start, v.start+v.dur) {
				lane = r
				break
			}
		}
		put(v, lane)
	}
	return max(1, len(rows))
}

func (ed *cutEditor) relayout() {
	ed.laneN = max(assignLanes(ed.vids, ed.rows), ed.nRows)
	// a camera unplugged between two visits takes its row with it, and a
	// selection still pointing at that row would keep footage nobody can see
	if ed.sel.lane >= ed.laneN {
		ed.sel.lane = 0
	}
	ed.layoutPx()
	if ed.srcArea != nil {
		// height only: the width is whatever the page gives us. The +8 is the
		// picture band's own breathing room; the lane below it is where the
		// camera and clock effects live (cut_fx.go).
		ed.fitSrc()
		ed.fitAudio()
		ed.fitSelAud() // a reload may have taken the recording the selection was of
		ed.syncScroll()
		ed.redrawTracks()
	}
	ed.updateTotal()
}

// layoutPx is the half of relayout that is arithmetic: where every run and
// every recording sits in timeline px at the current zoom. On its own it is
// what a zoom needs, and nothing a zoom does not.
func (ed *cutEditor) layoutPx() {
	ed.filmed = timeSpans(ed.vids)
	ed.spans = ed.cells()
	// the black strip in front of second zero, which the switches at the left
	// of every band stand in rather than on the footage (cut_gutter.go)
	x := gutterPx
	for i := range ed.spans {
		ed.spans[i].px = x
		x += ed.spanW(ed.spans[i])
	}
	ed.totalW = x
	if len(ed.spans) == 0 {
		ed.totalW = 0 // nothing loaded: no tape, and so no strip in front of it
	}
	// a recording is contiguous and the runs are the union of all of them, so
	// each file sits inside exactly one run, and x is linear in t inside a run:
	// a file's origin is just its start read off the map. Still kept as a field
	// because the thumbnails are walked by frame index, not by second.
	for i := range ed.vids {
		ed.vids[i].pxOrigin = ed.xOf(ed.vids[i].start)
	}
}

// picTop is where the picture band starts: under the ruler's clock and the
// selection band, in that order. With more than one camera it is
// the top of the FIRST row; the rest are stacked under it (laneTop).
func (ed *cutEditor) picTop() float64 { return ed.fxLaneTop() + ed.fxLaneHeight() }

// laneH is one row of the picture band: a thumbnail and its border.
func (ed *cutEditor) laneH() float64 { return float64(ed.thumbHt) + 4 }

// laneTop is where row i starts. Not a stride any more: every row above it is
// its pictures AND the wave strip paired under them (pairH), and a row with
// sound is deeper than a row without.
func (ed *cutEditor) laneTop(i int) float64 {
	t := ed.picTop()
	for j := 0; j < i; j++ {
		t += ed.laneH() + ed.pairH(j) + laneGap
	}
	return t
}

// pairH is how deep row i's wave strip is: one waveform lane per channel of
// the row's own sound, and nothing at all for a row whose footage has none --
// a silent screen capture is a row of pictures, not a row over an empty band
// pretending it recorded something. Two sources sharing a row share the strip,
// so it is as deep as the deepest of them needs.
func (ed *cutEditor) pairH(i int) float64 {
	h := 0.0
	for _, v := range ed.vids {
		if v.lane != i {
			continue
		}
		if au := ed.pairAud(v.base); au != nil {
			h = math.Max(h, float64(ed.lanes(*au))*waveLaneH)
		}
	}
	return h
}

// pairAud is the sound drawn under this row's pictures: the footage's own
// track (masterLanes, laneAudios), which pairs with the pictures because
// footage is picture and the sound filmed with it in one piece. A separate
// recorder is nobody's pair and keeps its lane in the band below.
func (ed *cutEditor) pairAud(base string) *tlAudio {
	if au := ed.audByBase(base); au != nil && au.master {
		return au
	}
	return nil
}

// picBottom is where the whole stack of rows ends. Everything that is about the
// CUT rather than about one camera -- the green, the scrim, the markers, the
// playhead's own band -- is drawn from picTop to here, so it reads as one thing
// across every row it crosses.
func (ed *cutEditor) picBottom() float64 {
	last := max(0, ed.laneN-1)
	return ed.laneTop(last) + ed.laneH() + ed.pairH(last)
}

// rowNameVid is the recording a row's name plate speaks for: the one under the
// view's left edge, and -- when the edge is over a gap or before the row's
// first file -- the first one in view. Nil for a row with nothing on screen,
// which is a row with nothing to name.
func (ed *cutEditor) rowNameVid(row int, vx0, vx1 float64) *tlVideo {
	var first *tlVideo
	for i := range ed.vids {
		v := &ed.vids[i]
		if v.lane != row {
			continue
		}
		x0, x1 := v.pxOrigin, v.pxOrigin+v.dur*ed.pps
		if x1 < vx0 || x0 > vx1 {
			continue // off screen entirely
		}
		if x0 <= ed.viewX && ed.viewX < x1 {
			return v // under the left edge: the one the eye is on
		}
		if first == nil || x0 < first.pxOrigin {
			first = v
		}
	}
	return first
}

// hitPics is whether a y of the source area is on the picture band -- the
// thumbnails and the green over them. The two rows above it and the effects
// lane below it are their own objects with their own rules, so "is this press
// about the cut itself" is a question worth having one answer to.
func (ed *cutEditor) hitPics(y float64) bool {
	return y >= ed.picTop() && y < ed.picBottom()
}

// segTop is where a scene is drawn: its own camera's row. Clamped, because a
// cut.json can name a row this session has not got -- a camera unplugged since
// it was written -- and a scene drawn off the bottom of the band is a scene
// nobody can see to fix.
func (ed *cutEditor) segTop(s cutSeg) float64 { return ed.laneTop(ed.segRow(s)) }

// segRow is that row's number, which the wave strip under it is asked for too
// (pairH): the green that says "this scene is in the video" covers the
// pictures AND the sound filmed with them.
func (ed *cutEditor) segRow(s cutSeg) int {
	return min(max(0, s.Cam), max(0, ed.laneN-1))
}

// laneAt is which camera's row of PICTURES a y is on, or -1 for the thin
// space between two rows, for a row's wave strip (pairAt) and for anything off
// the stack.
func (ed *cutEditor) laneAt(y float64) int {
	for i := 0; i < max(1, ed.laneN); i++ {
		if t := ed.laneTop(i); y >= t && y < t+ed.laneH() {
			return i
		}
	}
	return -1
}

// pairAt is the sound half of the same question: which row's wave strip a y is
// on, or -1.
func (ed *cutEditor) pairAt(y float64) int {
	for i := 0; i < max(1, ed.laneN); i++ {
		if t := ed.laneTop(i) + ed.laneH(); y >= t && y < t+ed.pairH(i) {
			return i
		}
	}
	return -1
}

// pairAudAt is WHOSE sound that strip is at timeline-x px: two sources sharing
// a row each bring the stretch under their own pictures, so the answer is the
// one under the pointer -- or the nearest along the row, audAtY's rule, so a
// press on the stretch of the row neither of them covers is a miss and not a
// void.
func (ed *cutEditor) pairAudAt(px, y float64) string {
	row := ed.pairAt(y)
	if row < 0 {
		return ""
	}
	best, dist := "", math.Inf(1)
	for _, v := range ed.vids {
		if v.lane != row || ed.pairAud(v.base) == nil {
			continue
		}
		d := 0.0
		if x0, x1 := v.pxOrigin, v.pxOrigin+v.dur*ed.pps; px < x0 {
			d = x0 - px
		} else if px > x1 {
			d = px - x1
		}
		if d < dist {
			best, dist = v.base, d
		}
	}
	return best
}

// fitSrc gives the source-track area the height it currently needs. Not a
// constant any more: the effects lane is as deep as the effects in it pile up
// (fxRows), so adding an effect over an existing one makes the area taller and
// removing it gives the room back.
//
// The +8 is the picture band's own breathing room.
func (ed *cutEditor) fitSrc() {
	if ed.srcArea == nil {
		return
	}
	// picBottom already counts the effects lane: it is above the pictures now,
	// so the whole stack is measured from the top of it (fxLaneTop)
	h := int(ed.picBottom()) + 4
	if h == ed.srcHt {
		return // SetSizeRequest during a draw is how you get a resize loop
	}
	ed.srcHt = h
	ed.srcArea.SetSizeRequest(-1, h)
}

// fitAudio gives the lane area the height its lanes need: a fixed height per
// lane, none at all without lanes. Called again as each envelope lands, since
// a stereo file with identical sides collapses to one lane.
func (ed *cutEditor) fitAudio() {
	if ed.audArea == nil {
		return
	}
	if ah := ed.audioHeight(); ah > 0 {
		ed.audArea.SetSizeRequest(-1, ah)
		ed.audArea.SetVisible(true)
	} else {
		ed.audArea.SetVisible(false)
	}
}

// syncScroll points the scrollbar at the timeline as it now is. It is also
// where the bar disappears: a bar that cannot move is a bar that says there is
// something off to the right, and at the zoom floor there is not.
func (ed *cutEditor) syncScroll() {
	if ed.hadj == nil {
		return
	}
	// five writes, and every one of them can emit value-changed -- a new
	// upper re-clamps the value, a new page size re-clamps it again -- each of
	// which used to be a full redraw. The handler sits this out; whoever
	// called relayout draws once when the layout is settled.
	ed.scrollMut = true
	ed.hadj.SetUpper(ed.totalW)
	ed.hadj.SetPageSize(ed.viewW)
	ed.hadj.SetStepIncrement(ed.viewW / 8)
	ed.hadj.SetPageIncrement(ed.viewW * 0.9)
	ed.hadj.SetValue(ed.hadj.Value()) // re-clamps against the new upper
	ed.scrollMut = false
	ed.viewX = ed.hadj.Value()
	ed.hbar.SetVisible(ed.totalW > ed.viewW+0.5)
}

// setOff scrolls to a timeline x; the adjustment does the clamping.
func (ed *cutEditor) setOff(x float64) {
	if ed.hadj == nil {
		ed.viewX = math.Max(0, x)
		return
	}
	ed.hadj.SetValue(x)
}

func (ed *cutEditor) setThumbH(h int) {
	ed.thumbHt = max(40, min(160, h))
	// the pictures are cached at the old height, and so is whatever a loader
	// is holding half-read: the generation is what tells the two apart when it
	// comes back (cut_thumbs.go)
	ed.thumbs, ed.thumbWant = map[string]*thumbPic{}, nil
	ed.thumbGen++
	ed.relayout()
}

// setPlayhead drops the red line and cues the preview there. Whatever the
// player was doing continues: paused stays paused (showing the new frame),
// playing keeps playing from the new spot.
func (ed *cutEditor) setPlayhead(t float64) {
	ed.playhead = t
	ed.reLive(t) // the live clock is re-based with the line; see livePlayhead
	ed.hasPlay = true
	// a card holding the footage is holding it for the line, and the line has
	// just been put somewhere else
	ed.cancelHold()
	ed.syncFxHold() // and an effect being aimed is being aimed at THIS frame
	ed.showTime()
	ed.syncSelBtns()  // | Split cuts at the line, and now there is one
	ed.syncPlayGain() // the line may have landed inside a volume effect
	if v := ed.videoAt(t); v != nil && ed.player != nil {
		wasPlaying := ed.player.playing
		// before the seek, never after: a rate only takes hold at a seek, and
		// this is the seek. Setting it afterwards would need a second one.
		ed.player.SetRate(fxPreviewRateAt(ed.fx, t))
		same := ed.playVideo == v
		if !same {
			ed.playVideo = v
			// which recordings are under THIS piece of footage, and by how far
			// their clocks differ from its own -- both change with the file, so
			// they are settled before the file is
			ed.player.SetMix(ed.mixUnder(v))
		}
		// and which of them this scene hears, before either line below tells
		// them to play: a lane the scene silences is not started at all rather
		// than started and hushed (Player.applyMute). showInsert settles it
		// again at the bottom of this function, which is where every OTHER
		// path reaches it -- but by then these pipelines are already running,
		// and a lane switched off is not to be heard for that moment either.
		ed.syncHush()
		if same {
			ed.player.SeekTo(v.at(t)) // same file: cheap in-place seek
		} else {
			ed.player.PlaySegment(v.path, v.at(t), -1, wasPlaying)
		}
	}
	// and if the line landed inside a card, the card is what the preview shows,
	// whatever the footage under it is doing
	ed.showInsert()
	ed.redrawTracks()
}

// mixUnder is the separate recordings to play under v, each with its offset
// relative to v's 0. The footage's own master track is not in it (the preview
// already plays it); a further track of the same file is, as its own pipeline
// (mixTrack.track). delta includes both offs -- a cut lane opens partway into
// its file.
func (ed *cutEditor) mixUnder(v *tlVideo) []mixTrack {
	var out []mixTrack
	for _, au := range ed.auds {
		if au.master || (au.path == v.path && au.track == 0) {
			continue
		}
		if au.start+au.dur <= v.start || au.start >= v.start+v.dur {
			continue
		}
		out = append(out, mixTrack{base: au.base, path: au.path,
			delta: (v.start - v.off) - (au.start - au.off),
			lo:    au.off, hi: au.off + au.dur, track: au.track})
	}
	return out
}

// showTime prints the red line's time (mm:ss.d, as the edge readouts and
// Narrate). Pushed rather than drawn: three paths move the playhead, and the
// line may be scrolled out of view while the time is not.
func (ed *cutEditor) showTime() {
	if ed.clock == nil {
		return
	}
	// Under ▶✂ the readout is the cut's own clock -- how far into the FINISHED
	// video -- in the same format; the tooltip says which clock. An empty cut has
	// no reading (everything maps to 0:00.0), so the session clock stays until
	// there is a cut.
	t, tip := ed.playhead, ed.playheadTip()
	if ed.cutOnly && len(ed.segs) > 0 {
		// through the effects, not over them: this clock claims to be the
		// finished video's, and a ×2 halves the seconds the video spends on
		// the footage the line is walking through. Read straight off the
		// segments it drifted further from the picture with every effect --
		// the preview was already playing at the effect's rate (syncPlayRate)
		// while the number counted session seconds.
		t = ed.cutPos(ed.playhead)
		tip = fmt.Sprintf("%s into the cut, of %s — the finished video's own clock "+
			"(the ▶✂ preview), the speed effects included. Session time here is %s.",
			mmss(t), mmss(ed.cutLen()), playheadClock(ed.playhead, ed.hasPlay))
	}
	ed.clock.SetText(playheadClock(t, ed.hasPlay))
	ed.clock.SetTooltipText(tip)
}

// playheadClock is the toolbar's reading of the playhead, and the dashes are
// exactly as wide as a time so that placing the line for the first time does not
// shove the rest of the bar sideways.
func playheadClock(t float64, has bool) string {
	if !has {
		return "--:--.-"
	}
	return fmtClock(t)
}

// showMarks keeps the small line under ⟦ in and out ⟧ reading the two marks.
// Pushed like the clock above it, because two paths change a mark -- setting
// one and clearing both -- and a readout only one of them updates is right
// after ⟦ in and silently stale after ✕, which is the worse failure.
func (ed *cutEditor) showMarks() {
	if ed.marks == nil {
		return
	}
	ed.marks.SetText(marksClock(ed.markIn, ed.markOut, ed.hasIn, ed.hasOut))
}

// marksClock is the small print under the in/out buttons: both marks in the
// mm:ss.d spelling the clock beside them uses, and dashes while a mark is
// unset, exactly as wide as a time, so setting one never changes the line's
// width and the bar never twitches.
func marksClock(in, out float64, hasIn, hasOut bool) string {
	return playheadClock(in, hasIn) + " – " + playheadClock(out, hasOut)
}

// playheadTip is the long form of the same answer, for the hover: where the line
// falls inside the recording it is over (which is the number ffmpeg and the
// player think in, and it is not the session time the label shows), which frame
// that is, and whether the cut currently keeps it.
func (ed *cutEditor) playheadTip() string {
	if !ed.hasPlay {
		return "No playhead yet — left-click a track to place the red line"
	}
	where := "in the gap between recordings"
	if v := ed.videoAt(ed.playhead); v != nil {
		where = fmt.Sprintf("%s at %s", filepath.Base(v.path), fmtClock(v.at(ed.playhead)))
		if v.fps > 0 {
			where += fmt.Sprintf(", frame %d", int(math.Round(v.at(ed.playhead)*v.fps)))
		}
	}
	kept := "cut away"
	if ed.inCut(ed.playhead) {
		kept = "kept"
	}
	return fmt.Sprintf("The red line: %.2f s into the session — %s — %s here",
		ed.playhead, where, kept)
}

// frameStep pauses and nudges the preview by whole frames -- or, while a clip
// edge or a whole clip is held, that. ‹f on a boundary you have just picked up
// can only mean one thing, and it is not "move the playhead somewhere else".
func (ed *cutEditor) frameStep(n int) {
	// A hold that has evaporated -- undone, re-cut, the project swapped out
	// from under the page -- must not swallow the press. Each nudge says
	// whether it moved anything, and when none of them did, the button means
	// what it says on its face and the line moves.
	if ed.edgeOn && ed.nudgeEdge(n) {
		return
	}
	if ed.segOn && ed.nudgeSeg(n) {
		return
	}
	if ed.fxOn && ed.nudgeFx(n) {
		return
	}
	if ed.playVideo == nil || ed.player == nil {
		ed.a.setStatus("click a track first to place the playhead")
		return
	}
	v := ed.playVideo
	ed.cancelHold() // stepping is a hand on the line, the same as clicking it
	ed.player.Pause()
	local := math.Max(v.off, math.Min(v.off+v.dur, v.at(ed.playhead)+float64(n)/v.fps))
	ed.playhead = v.sessionAt(local)
	ed.reLive(ed.playhead) // a hand on the line: the live clock comes with it
	// the rate before the seek, never after -- it only takes hold at one, and
	// this is the seek. The same bargain setPlayhead makes.
	ed.player.SetRate(fxPreviewRateAt(ed.fx, ed.playhead))
	ed.player.SeekTo(local)
	ed.showTime()
	ed.revealPlayhead() // a step must never move the line somewhere you cannot see
	ed.showInsert()     // stepping through a card steps through the card
	ed.redrawTracks()
}

// revealOff is where the view must scroll for a playhead at timeline x: kept
// where it is while x is on screen, centered on x once it is not. Centered
// rather than nudged just inside the edge, because a line brought back to the
// very edge is one more step from leaving again.
func revealOff(x, viewX, viewW float64) (float64, bool) {
	if x >= viewX && x <= viewX+viewW {
		return viewX, false
	}
	return x - viewW/2, true
}

// revealPlayhead scrolls the timeline so the red line is on screen. Stepping
// and playback both move the line without moving the view, so either could
// walk it silently off the page -- and a transport whose subject is somewhere
// off screen is a transport you operate blind.
func (ed *cutEditor) revealPlayhead() {
	if !ed.hasPlay || ed.viewW <= 0 {
		return
	}
	if off, out := revealOff(ed.xOf(ed.playhead), ed.viewX, ed.viewW); out {
		ed.setOff(off)
	}
}

// wheelFrames is the transport on the wheel: a notch is a frame, five with
// Shift, exactly the arrow keys' spelling -- and like ‹f and f› it nudges a
// held edge, clip or effect instead of the line. One controller per widget,
// because a controller cannot be in two places.
func (ed *cutEditor) wheelFrames() *gtk.EventControllerScroll {
	sc := gtk.NewEventControllerScroll(gtk.EventControllerScrollVertical)
	sc.ConnectScroll(func(_, dy float64) bool {
		if dy == 0 {
			return false
		}
		n := 1
		if sc.CurrentEventState()&gdk.ShiftMask != 0 {
			n = 5
		}
		if dy < 0 {
			n = -n
		}
		ed.frameStep(n)
		return true
	})
	return sc
}

// playTick is how often the page reads the player's clock. Ten times a second
// is more than a red line and a clock face need and less than a moving camera
// wants -- see livePlayhead, which is how the second one is paid for without
// redrawing the timeline sixty times a second.
const playTick = 100

// liveClock is the extrapolation as arithmetic, shared with the Narrate preview
// (narrate_fxview.go) so the two cannot drift. Returns the clock now and the
// high-water mark to keep; the caller stores the mark.
func liveClock(playhead, posT float64, posAt time.Time, liveMax, rate float64, playing bool) (now, mark float64) {
	if !playing || posAt.IsZero() {
		return playhead, playhead // nothing to smooth; re-arm on the line itself
	}
	d := time.Since(posAt).Seconds()
	if d < 0 {
		d = 0
	}
	span := float64(playTick) / 1000
	if d > span {
		d = span
	}
	if v := posT + d*rate; v > liveMax {
		liveMax = v
	}
	if hi := posT + span*rate; liveMax > hi {
		liveMax = hi // a whole tick of headroom, and not one second more
	}
	return liveMax, liveMax
}

// syncPlayGain puts the preview at the loudness the volume effects give the
// second under the line, on top of whatever the slider says (SetFxGain). The
// preview's half of a volume effect, and the reason it is beside syncPlayRate
// rather than inside it: a gain needs no seek to take hold, so it can be set
// while the picture is stopped, which is what makes scrubbing across a boosted
// stretch sound like the boosted stretch.
func (ed *cutEditor) syncPlayGain() {
	if ed.player != nil {
		ed.player.SetFxGain(fxGainAt(ed.fx, ed.playhead))
	}
}

func (ed *cutEditor) syncPlayRate() {
	if ed.player != nil {
		ed.player.SetRateNow(fxPreviewRateAt(ed.fx, ed.playhead))
	}
}

// skipGap jumps the line over the stretch between two clips in the cut-only
// preview and reports whether it did (setPlayhead has then moved everything).
// Re-entry is guarded twice, as on Narrate: setPlayhead's seek drops
// player.playing until the preroll, and jumped covers a second no recording
// covers.
func (ed *cutEditor) skipGap() bool {
	if !ed.cutOnly || ed.player == nil || len(ed.segs) == 0 {
		ed.jumped = -1
		return false
	}
	cur, next := gapAt(ed.segs, ed.playhead)
	if cur >= 0 {
		ed.jumped = -1
		return false
	}
	switch {
	case next < 0:
		// past the last clip: the finished video has ended, and stopping here
		// is what that looks like
		ed.jumped = -1
		ed.player.Pause()
		ed.a.updateRunControls()
	case next != ed.jumped:
		ed.jumped = next
		ed.setPlayhead(ed.playable(ed.segs[next].S))
	default:
		// jumped here already and the line is STILL in the gap. Playback used
		// to sit here for good: the guard is there so a seek that has not
		// landed yet is not fought tick after tick, but a seek that can never
		// land is not a seek in flight. It happens when the clip starts where
		// nobody filmed -- setPlayhead finds no recording, leaves the player
		// running on the old file, and the position it reads back is inside
		// this same gap again. Ask once more, from the first second there is
		// footage for, and if there is none, stop rather than pretend to play.
		if to := ed.playable(ed.segs[next].S); ed.videoAt(to) != nil {
			ed.setPlayhead(to)
		} else {
			ed.jumped = -1
			ed.player.Pause()
			ed.a.updateRunControls()
		}
	}
	return true
}

// playable is t, or the first second at or after it that a recording covers.
// A cut may name a second nobody filmed -- the model chooses from a timeline
// where unfilmed time is written down as plainly as the rest -- and the player
// has nothing to seek to there.
func (ed *cutEditor) playable(t float64) float64 {
	if ed.videoAt(t) != nil {
		return t
	}
	best := math.Inf(1)
	for i := range ed.vids {
		if v := &ed.vids[i]; v.start >= t && v.start < best {
			best = v.start
		}
	}
	if math.IsInf(best, 1) {
		return t
	}
	return best
}

// walkOn carries playback from the end of one recording to the next.
//
// A row is several files in a line and the timeline is one clock over all of
// them, so the end of a file is not the end of anything the page shows: ▶ ran
// out of stream and the line stopped in the hatched strip between two
// recordings, with the rest of the session still to the right of it.
//
// The strip is not played through. Nobody filmed those minutes -- there is
// nothing there to watch -- and the page does not even draw them to scale
// (cut_fold.go), so the line lands on the next recording's first frame and
// keeps going, which is what the same press already does when the cut runs onto
// another camera (followPlayback).
//
// Only after an END. A pause is a decision and stays one, and a stream nobody
// started has nowhere to walk on to.
func (ed *cutEditor) walkOn() bool {
	if ed.player == nil || !ed.player.ended || ed.hold.on {
		return false
	}
	t, v, ok := ed.nextPlay()
	if !ok {
		return false // the last recording has played out: that IS the end
	}
	if v == ed.playVideo && t <= ed.playhead {
		return false // the same file at the same second is not somewhere to go
	}
	ed.jumped = -1
	ed.setPlayhead(t)
	ed.cutOnlySnap() // ▶✂ carries on at the next KEPT clip, not the next frame
	if !ed.player.playing {
		// setPlayhead cues the player where it finds it, and it found it
		// stopped: what ended was the file, not the press
		ed.player.Toggle()
	}
	return true
}

// nextPlay is where that is: the second the line is already on when another
// camera is still rolling through it, and otherwise the first frame of the next
// recording on the timeline. false past the last one.
//
// The recording that just ended is never the answer, whatever the clock says.
// Two files written back to back -- a camera stopped and started again, a
// recorder splitting by size -- meet at one second, and asking which recording
// that second belongs to has two answers; the one we have just played out is
// not the one to play next.
func (ed *cutEditor) nextPlay() (float64, *tlVideo, bool) {
	t := ed.playhead
	if v := ed.videoAt(t); v != nil && v != ed.playVideo {
		return t, v, true
	}
	var best *tlVideo
	for i := range ed.vids {
		v := &ed.vids[i]
		if v == ed.playVideo || v.start < t-0.01 {
			continue
		}
		if best == nil || v.start < best.start {
			best = v
		}
	}
	if best == nil {
		return 0, nil, false
	}
	return math.Max(best.start, t), best, true
}

// cutOnlySnap puts the line on kept material before the picture starts, so a ▶
// pressed with the line standing in a gap does not open on a frame the cut
// throws away. The tick would move it a moment later anyway; this is only so
// that moment is never on screen.
func (ed *cutEditor) cutOnlySnap() {
	if !ed.cutOnly || len(ed.segs) == 0 {
		return
	}
	if cur, next := gapAt(ed.segs, ed.playhead); cur < 0 && next >= 0 {
		ed.setPlayhead(ed.segs[next].S)
	}
}

// followPlayback keeps the red line on the player's clock while it runs;
// on pause the queries stop and the line simply stays put.
// syncPlayRate puts the preview on the clock the footage under the line runs
// on, so a slowed stretch is slow to watch and not just rose-tinted on the
// track. Called as the line moves under playback -- a rate change with no seek
// on the way to carry it, which is the one case SetRate deliberately leaves to
// its caller. SetRateNow asks the running pipeline for an instant rate change
// and falls back to a flushing seek-in-place only where that is refused
// (player.go has the story), so a ramp's stairs no longer each cost a hitch.
func (ed *cutEditor) followPlayback() bool {
	// ...except while a spliced card is playing, when there is no clock to
	// follow: the footage is held and the card runs on the wall clock instead
	if ed.hold.on {
		ed.tickHold()
		return true
	}
	// the recording under the line has run out with the session still going:
	// the line walks on to the next one rather than stopping in the strip
	// between two files (walkOn)
	if ed.walkOn() {
		return true
	}
	if ed.player == nil || !ed.player.playing || ed.playVideo == nil {
		ed.syncPreviewZoom() // playback may have just stopped; the live zoom goes with it
		return true
	}
	if pos, ok := ed.player.Position(); ok {
		was := ed.playhead
		ed.playhead = ed.playVideo.sessionAt(pos)   // off included: a cut lane's window starts partway in
		ed.posT, ed.posAt = ed.playhead, time.Now() // the camera's clock; see livePlayhead
		ed.syncFxHold()                             // ▶ walks the line off whatever was picked up
		ed.showTime()
		// ▶ plays the dropped stretches too, so a seam the line has walked
		// into comes open and stays open (cut_fold.go). ▶✂ never enters one.
		if !ed.cutOnly {
			ed.walkFold()
		}
		// a card comes up as the line reaches it and goes as the line leaves,
		// and while it is up this is what advances it frame by frame -- unless
		// the line has just run into a card the footage is cut open for, which
		// stops the footage here and plays the card before either goes on
		if s := ed.splicedCrossed(was, ed.playhead); s != nil {
			ed.startHold(s)
		} else {
			ed.showInsert()
		}
		// after the card check above, so a card standing at a clip boundary
		// still plays: a held card stops this tick at the top, and the skip is
		// never reached while one is up
		if ed.skipGap() {
			return true // setPlayhead did the rest of this
		}
		// the cut has run onto another camera: load the other file. A visible hiccup
		// at every camera change, accepted -- smooth playback would need a second
		// prerolled pipeline per row. Not while a card is up: the picture is the
		// card's, and re-cueing underneath would jog the sound for nothing.
		if card, _ := ed.cardNow(); card == nil || card.audioIns() {
			if v := ed.videoAt(ed.playhead); v != nil && v != ed.playVideo {
				ed.setPlayhead(ed.playhead)
				return true
			}
		}
		ed.syncPlayRate()   // the line has crossed into or out of a speed effect
		ed.syncPlayGain()   // and into or out of a volume one
		ed.revealPlayhead() // playback runs the line off the view; recenter and follow
		// the bands' pixels only change when the green bar walks onto another
		// scene; every other tick moves nothing but the line's own layer
		if ed.bandClipIdx() != ed.lineIdx {
			ed.redrawTracks()
		} else {
			ed.redrawLine()
		}
	}
	return true // keep the timer alive
}

func (ed *cutEditor) xOf(t float64) float64 {
	for _, s := range ed.spans {
		if t <= s.t1 {
			if t < s.t0 {
				return s.px // in an unfilmed stretch: the next run's edge
			}
			if s.fold {
				return s.px // a folded cell has no width: every second in it is the seam
			}
			return s.px + (t-s.t0)*ed.pps
		}
	}
	return ed.totalW
}

func (ed *cutEditor) tAt(x float64) float64 {
	if len(ed.spans) == 0 {
		return 0 // nothing loaded: every x on an empty track is time zero
	}
	for _, s := range ed.spans {
		if x < s.px {
			return s.t0 // left of the first run: the gutter, which is not tape
		}
		w := ed.spanW(s)
		if s.fold && x <= s.px+w {
			// a seam is one x standing for the whole gap; a press on it is
			// its first second, which is where the footage stops being kept
			return s.t0
		}
		// half-open on the right, because unfilmed time takes no width: the x
		// where one recording stops is the x where the next one starts, and it
		// reads as the second the LATER one begins. A press there is a press
		// on the take you can see there.
		if x < s.px+w {
			return s.t0 + (x-s.px)/ed.pps
		}
	}
	return ed.spans[len(ed.spans)-1].t1
}

// ontoFilm is the nearest second to t that a recording covers, in the direction
// the edge is facing: a start moves FORWARD to where the next take begins, an
// end moves BACK to where the last one stopped, so an edge never crosses the
// footage it belongs to on its way out of a hole.
func (ed *cutEditor) ontoFilm(t float64, isStart bool) float64 {
	best, dist := t, math.Inf(1)
	for i := range ed.vids {
		v := &ed.vids[i]
		c := v.start
		if !isStart {
			c = v.start + v.dur
		}
		if isStart && c < t || !isStart && c > t {
			continue
		}
		if d := math.Abs(c - t); d < dist {
			best, dist = c, d
		}
	}
	if math.IsInf(dist, 1) {
		// nothing that way: the other way is better than a second of nothing
		for i := range ed.vids {
			v := &ed.vids[i]
			for _, c := range []float64{v.start, v.start + v.dur} {
				if d := math.Abs(c - t); d < dist {
					best, dist = c, d
				}
			}
		}
	}
	return best
}

// tAtView is the same for an x on the widget, which is a window onto the
// timeline scrolled viewX px along it.
func (ed *cutEditor) tAtView(x float64) float64 { return ed.tAt(x + ed.viewX) }

// thumbStep is how many frames apart the thumbnails on a row stand: one
// thumbnail's WIDTH, in frames, for a row th px tall drawn at pps.
//
// The width is whatever shape this source is. Assuming 16:9 for every source
// left the row striped on a 4:3 capture, on a phone held upright, on anything
// else odd -- the step was measured for a picture wider than the one that got
// drawn, and what showed through between two frames was the band's own ground.
// A source that has not said what shape it is keeps the old assumption, which
// is right for most cameras and no worse than what it replaced.
//
// Rounded DOWN on purpose: too small a step overlaps by a hair, too large a
// step is a stripe, and only one of those can be seen.
func (v *tlVideo) thumbStep(th, pps float64) int {
	ar := 16.0 / 9
	if v.w > 0 && v.h > 0 {
		ar = float64(v.w) / float64(v.h)
	}
	if v.interval <= 0 || pps <= 0 {
		return 1 // no frames to step between: frameRange answers nothing anyway
	}
	return max(1, int(th*ar/(pps*v.interval)))
}

// frameRange is the half-open range of frames to paint for px x0..x1, in
// strides of step. The start is snapped to a stride from the ROW's first frame,
// not the file's: thumbnails do not reshuffle on scroll, and a lane opening
// partway into a file still starts with a picture.
func (v *tlVideo) frameRange(pps, x0, x1 float64, step int) (first, last int) {
	// a row with no frames to walk, or none it could tell apart: an editor
	// built for a test, a source whose frames have not been extracted yet.
	// perFrame would be nought and every index below would come out as whatever
	// a division by it makes of them
	if len(v.frames) == 0 || v.interval <= 0 || step <= 0 {
		return 0, 0
	}
	perFrame := pps * v.interval // px of timeline per frame
	// counted from where the file's OWN first frame would be drawn, which is
	// where the row starts for a recording and off seconds left of it for a
	// lane showing a file from partway in (cut_lane.go)
	org := v.pxOrigin - v.off*pps
	// the row's own first frame: nought for a recording, the window's start for
	// a lane. Never before it, nor past the window's end -- a lane cut from a
	// recording borrows that recording's whole frame folder, and the frames
	// either side of its window are another row's picture.
	lo := int(v.off / v.interval)
	first = max(lo, int((x0-org)/perFrame))
	first -= (first - lo) % step
	last = min(len(v.frames), int((x1-org)/perFrame)+1)
	if v.dur > 0 {
		last = min(last, int((v.off+v.dur)/v.interval)+1)
	}
	if last <= first {
		return 0, 0 // the window is off one end of this recording
	}
	return first, last
}

// pickVideo is the recording playing at session-second t, or nil when t falls
// in no recording -- the gap between two of them, or past the end of the last.
//
// Half-open, [start, start+dur), so a second on the seam between two recordings
// belongs to the one starting there. Closing the far end would hand it to the
// one ENDING there instead, and a preview cued at second `dur` of a file is
// cued one frame past its last: the seam is the one place the answer is worth
// getting right, and the answer everyone wants there is "the next clip".
//
// A free function over a slice rather than a method, because the render walks a
// snapshot of the timeline with no editor behind it (produce.go), and the
// preview and the render have to agree about which side of a seam a second is
// on or a cut made in one plays back as the other.
func pickVideo(vids []tlVideo, t float64) *tlVideo {
	for i := range vids {
		if t >= vids[i].start && t < vids[i].start+vids[i].dur {
			return &vids[i]
		}
	}
	return nil
}

// pickVideoOn is pickVideo for one camera: the recording on row cam running at
// t, or nil. Falls back to whatever was rolling when the row has nothing there
// -- an old cut.json says row 0 for everything -- rather than render a hole.
func pickVideoOn(vids []tlVideo, cam int, t float64) *tlVideo {
	if v := videoOn(vids, cam, t); v != nil {
		return v
	}
	return pickVideo(vids, t)
}

// videoOn is the same question without the fallback: the recording on row cam
// at t, and nil when that row was not rolling.
func videoOn(vids []tlVideo, cam int, t float64) *tlVideo {
	for i := range vids {
		if vids[i].lane == cam && t >= vids[i].start && t < vids[i].start+vids[i].dur {
			return &vids[i]
		}
	}
	return nil
}

// videoAt is the recording the PAGE is showing at t -- which on a session shot
// on one camera is simply the one that was rolling, and on several is the one
// the cut chose. Everything that asks a question about the frame at t goes
// through here: what the preview cues, how many frames per second to step by,
// which file a still is pulled from.
func (ed *cutEditor) videoAt(t float64) *tlVideo { return pickVideoOn(ed.vids, ed.camAt(t), t) }

// cutVideoOn is the recording the FINISHED VIDEO shows at t: the scene's camera,
// nil in a gap. videoAt is the Cut page's own question (what it is showing,
// watchRow included) and is not for the render or its previews.
func cutVideoOn(segs []cutSeg, vids []tlVideo, t float64) *tlVideo {
	for _, s := range segs {
		if t >= s.S && t < s.E {
			return pickVideoOn(vids, s.Cam, t)
		}
	}
	return nil
}

func (ed *cutEditor) cutVideoAt(t float64) *tlVideo { return cutVideoOn(ed.segs, ed.vids, t) }

// videoShown is videoAt without the charity: the recording on the watched row
// at t, and nil when that row has nothing there. The standstill preview asks
// this one (showInsert), because a paused preview borrowing another camera's
// frame looks exactly like the row HAVING that footage -- the one thing a
// glance at the preview is for. Playback keeps the fallback: a black picture
// with running sound helps nobody, and the master pipeline is the clock.
func (ed *cutEditor) videoShown(t float64) *tlVideo { return videoOn(ed.vids, ed.camAt(t), t) }

// camAt is which camera's row the preview shows at t.
//
// The row a click asked to WATCH comes first (monRow): the kept scene's
// answer below is always the scene's own camera, which left no way at all to
// see what another camera saw at the same second -- and that is most of how a
// scene gets stolen for it. Clamped rather than trusted, since rows can go
// away between the click and the asking.
//
// Then the kept scene's, when t is in one -- that is the whole of what the
// green means. In a stretch the cut throws away there is no scene to ask, and
// every row's thumbnails are on the page at once, so the answer is the row the
// hand was last on: drag a selection across the second camera and the preview
// follows the second camera, before ＋ Add has been pressed and whether or not
// it ever is.
func (ed *cutEditor) camAt(t float64) int {
	if m := ed.watchRow(); m >= 0 {
		return m
	}
	for _, s := range ed.segs {
		if !s.isInsert() && t >= s.S && t < s.E {
			return s.Cam
		}
	}
	return ed.sel.lane
}

// watchRow is the row the preview is watching, or -1 when it answers to the
// cut. The ONE place monRow is read back: clamped, because rows can go away
// between the click and the asking, and a preview, an outline and a status
// line that clamp separately are three chances to disagree about the same
// stale watch.
func (ed *cutEditor) watchRow() int {
	if ed.monRow <= 0 || len(ed.vids) == 0 {
		return -1
	}
	return min(ed.monRow-1, max(0, ed.laneN-1))
}

// monStatus says when the preview and the cut disagree on purpose: the line
// stands in a kept scene, the scene shows one camera, and the preview is
// watching another because a click asked it to. Said only then -- a click on
// the scene's own row changes nothing worth a sentence.
func (ed *cutEditor) monStatus() {
	m := ed.watchRow()
	if m < 0 {
		return
	}
	for _, s := range ed.segs {
		if !s.isInsert() && ed.playhead >= s.S && ed.playhead < s.E && s.Cam != m {
			ed.a.setStatus(fmt.Sprintf("watching camera %d — the cut shows camera %d here; "+
				"▶ plays the cut", m+1, s.Cam+1))
			return
		}
	}
}

// ---- editing ---------------------------------------------------------------

// snapEdge moves a rough edge to the best nearby cut point: strongest visual
// change or a speech gap, with a bias outward so sloppy selections keep the
// action whole instead of clipping it.
func (ed *cutEditor) snapEdge(t float64, isStart bool) float64 {
	v := ed.videoAt(t)
	if v == nil {
		// nobody filmed this second. It used to be left where it was, and a
		// clip that BEGINS there is a clip the player cannot open: it seeks to
		// a file that is not under the line, finds nothing, and playback stops
		// dead in the gap (skipGap). The cut sees a timeline where the minutes
		// between two takes are written down as plainly as the rest, so it
		// will name one; the page has to answer with the nearest second there
		// is footage for.
		return ed.ontoFilm(t, isStart)
	}
	best, bestScore := t, 0.35 // a candidate must beat "just leave it"
	try := func(c, score float64) {
		d := math.Abs(c - t)
		if d > snapTol || c < v.start || c > v.start+v.dur {
			return
		}
		score -= 0.4 * d / snapTol
		if (isStart && c <= t) || (!isStart && c >= t) {
			score += 0.3
		}
		if score > bestScore {
			best, bestScore = c, score
		}
	}
	for _, g := range ed.gaps[v.base] {
		try(g, 0.8)
	}
	// ...and the word boundaries themselves, which beat a silence midpoint
	// because they ARE the thing a midpoint is a guess at. Timed by the
	// aligner in Prepare (align.go): 0.02 s from the sound where the ASR's own
	// stamps sit 0.29 s behind it.
	//
	// This is what takes the aligner from a retake feature to a property of
	// the page: every edge placed here is placed with it -- the ends of a
	// suggested segment, and the selection you draw by hand with ＋ Add.
	for _, w := range ed.wordEdges(t) {
		try(w, 0.9)
	}
	// ...and above those, the ends and starts of whole LINES -- a sentence, a
	// phrase -- because a boundary on a word edge in the middle of a sentence
	// is exact and still wrong. Only just above: the distance term decides,
	// so a boundary a breath away from a line's end goes to the line's end,
	// and one four seconds away stays on its word.
	for _, sp := range ed.talk {
		if isStart {
			try(sp[0], 0.95)
		} else {
			try(sp[1], 0.95)
		}
	}
	// ...and only then the pictures. A frame candidate can score up to 1.0
	// against a speech gap's 0.8, and frames sit on the extraction interval --
	// one second on a talking head -- so a visual peak at a whole second beats
	// the pause beside it and takes the cut into the middle of a word. That is
	// where "…multiple wallet makers at" came from: the end snapped to a frame
	// at 350.00 while the start of the next clip found the pause at 350.76, and
	// the word "once" fell into the hole between them.
	//
	// Where nobody is talking they are still the best answer there is, which is
	// most of a screen capture and all of a silent one.
	if sc := ed.scores[v.base]; sc != nil && !ed.talking(t) {
		mean := 0.0
		for _, s := range sc {
			mean += s
		}
		mean /= float64(len(sc) + 1)
		i0 := int(v.at(t-snapTol) / v.interval)
		i1 := int(v.at(t+snapTol) / v.interval)
		for i := max(1, i0); i <= min(len(sc)-1, i1); i++ {
			if sc[i] > 2*mean && sc[i] >= sc[i-1] {
				try(v.sessionAt(float64(i)*v.interval), math.Min(1, sc[i]/(4*mean)))
			}
		}
	}
	return best
}

// wordEdges is the ends of words near session second t, in session time: where
// one word stops and the next has not started. The gap between two words is the
// only place inside a phrase a cut can go, and it is a place a silence midpoint
// cannot find -- there is no silence between "art" and "and", only a closure.
//
// Both edges of every word in reach, since which of the two a cut wants depends
// on which side of it the footage is kept: try() picks by distance and by the
// side the caller is on.
func (ed *cutEditor) wordEdges(t float64) []float64 {
	var out []float64
	for _, w := range ed.words {
		if w.e < t-snapTol || w.s > t+snapTol {
			continue
		}
		out = append(out, w.s, w.e)
	}
	return out
}

// talking is whether anybody was speaking at session second t, with a little
// either side: a cut a fifth of a second from a word is a cut in that word as
// far as the ear is concerned.
func (ed *cutEditor) talking(t float64) bool {
	for _, sp := range ed.talk {
		if t >= sp[0]-talkPad && t <= sp[1]+talkPad {
			return true
		}
	}
	return false
}

// rangePieces is what Add would keep out of the stretch t0..t1: a selection may
// span several recordings and the hole between them, so it comes apart into one
// piece per recording, with the slivers too short to be a scene dropped.
//
// It is split out from addRange because the button needs the answer before it
// commits: an empty list means Add would change nothing, and the press has to
// say so rather than push an undo step over a cut that never moved.
func (ed *cutEditor) rangePieces(t0, t1 float64) []cutSeg {
	if t1 < t0 {
		t0, t1 = t1, t0
	}
	t0 = ed.snapEdge(t0, true)
	t1 = ed.snapEdge(t1, false)
	var out []cutSeg
	// over the runs rather than the recordings: two cameras that both saw a
	// minute are one minute of cut, not the same minute kept twice. Which of
	// them is SHOWN is the row the selection was drawn on, carried here.
	for _, sp := range ed.runs() {
		s := math.Max(t0, sp.t0)
		e := math.Min(t1, sp.t1)
		if e-s >= minSegLn {
			out = append(out, cutSeg{S: s, E: e, Cam: ed.sel.lane})
		}
	}
	return out
}

// addRange keeps t0..t1, and says whether doing so took the seconds off
// another camera -- which is a thing the hand did not ask for by name and has
// to be told about.
func (ed *cutEditor) addRange(t0, t1 float64) bool {
	pieces := ed.rangePieces(t0, t1)
	kept := ed.keptLen()
	for _, p := range pieces {
		ed.stealSpan(p.S, p.E, p.Cam) // one camera at a time
	}
	// measured in seconds and not in scenes: taking the middle out of one scene
	// leaves two, so the count can go UP while footage went away
	stole := ed.keptLen() < kept-1e-9
	ed.segs = append(ed.segs, pieces...)
	ed.coalesce()
	ed.persist()
	return stole
}

// keptLen is how many seconds of the session the scenes cover between them.
func (ed *cutEditor) keptLen() float64 {
	d := 0.0
	for _, s := range ed.segs {
		d += s.E - s.S
	}
	return d
}

// camName is how a camera is referred to in a sentence: the row's first
// recording, which is the name written on its own band. Falls back to the row
// number when the row is empty, which is a session that has just lost a camera.
func (ed *cutEditor) camName(lane int) string {
	for _, v := range ed.vids {
		if v.lane == lane {
			return v.base
		}
	}
	return fmt.Sprintf("row %d", lane+1)
}

// insDefault is how long an insert runs when nothing in the file says. A card
// nobody reads in four seconds is a card that was too wordy to be a card.
const insDefault = 4.0

// addInsert drops a file onto the timeline at t. The cut is a sequence, so the
// footage under it gives way: the segment is split around it, as Remove would,
// and the insert takes the seconds between. Undoable. Landing in a gap between
// recordings costs nothing.
func (ed *cutEditor) addInsert(path string, t, dur float64, mute bool) {
	ed.layOver(cutSeg{S: t, E: t + dur, Ins: path, Mute: mute, Cam: ed.sel.lane})
}

// addSound lays a stretch of a sound file over the footage at t: for dur
// seconds what is heard is the file from ss; the picture is untouched and the
// video no longer. Returns how many stretches of footage it landed on (see
// layOverSound).
func (ed *cutEditor) addSound(path string, t, dur, ss float64, lane string) int {
	return ed.layOverSound(cutSeg{S: t, E: t + dur, Ins: path, Ss: ss, Lane: lane})
}

// layOver is the cut taking one insert over the footage, with the undo step
// and the save both ways of arriving here owe. Two ways arrive: a file chosen
// from disk and a stretch of a lane copied out of the session. What differs
// between them is what is IN the segment, never what the cut does with it, so
// the doing is written once.
func (ed *cutEditor) layOver(s cutSeg) {
	if s.E-s.S < minSegLn {
		s.E = s.S + insDefault
	}
	ed.pushUndo()
	ed.removeSpan(s.S, s.E)
	ed.segs = append(ed.segs, s)
	ed.coalesce()
	ed.persist()
}

// under this a piece is not worth a segment of its own: the sound would be a
// blink, and the footage either side of it would have been split for nothing.
const sndMinLn = 0.05

// layOverSound lays a sound over the footage without moving the picture. The
// span is cut to what the cut already keeps and one sound goes over each piece
// with its own offset, so it never puts dropped footage back or displaces a
// card. Splits are exact (no minimum-scene guard). Returns how many pieces.
func (ed *cutEditor) layOverSound(s cutSeg) int {
	if s.E-s.S < minSegLn {
		s.E = s.S + insDefault
	}
	out, n := make([]cutSeg, 0, len(ed.segs)+2), 0
	for _, f := range ed.segs {
		t0, t1 := math.Max(f.S, s.S), math.Min(f.E, s.E)
		if f.isInsert() || t1-t0 < sndMinLn {
			out = append(out, f)
			continue
		}
		if t0 > f.S {
			out = append(out, cutSeg{S: f.S, E: t0})
		}
		// Ss walks with the piece: the second of the file this piece begins at
		// is as far into it as the piece is into the span asked for, so two
		// pieces either side of a hole are two parts of one sound and not the
		// same opening seconds played twice. Lane does not walk -- every piece
		// stands in for the same recording, because the lane was named once
		// for the whole span and a hole in the footage is no reason to change
		// its mind.
		out = append(out, cutSeg{S: t0, E: t1, Ins: s.Ins, Ss: s.Ss + t0 - s.S, Lane: s.Lane})
		if t1 < f.E {
			out = append(out, cutSeg{S: t1, E: f.E})
		}
		n++
	}
	if n == 0 {
		return 0
	}
	ed.pushUndo()
	ed.segs = out
	ed.coalesce()
	ed.persist()
	return n
}

// addSplice drops a file BETWEEN the footage: the clip is cut open at t, the
// card runs for dur, the footage picks up where it left off. It takes no
// session time, so it is stored as a point (S == E) with its length in Dur.
func (ed *cutEditor) addSplice(path string, t, dur float64, mute bool, cam int) {
	if dur < minSegLn {
		dur = insDefault
	}
	ed.pushUndo()
	ed.segs = append(ed.segs, cutSeg{S: t, E: t, Ins: path, Dur: dur, Mute: mute, Cam: cam})
	ed.coalesce()
	ed.persist()
}

// A copied selection is an insert whose "file" is the session itself:
// copy:SECONDS, the footage second it plays from, length in Dur. It gets every
// behaviour a card has and costs one case at render time (see produce). No file
// can shadow the scheme: insert paths are project-relative and no admitted
// suffix contains ":".
const copyScheme = "copy:"

// copySrc is the footage second a copy starts at, and whether ins is one.
func copySrc(ins string) (float64, bool) {
	if !strings.HasPrefix(ins, copyScheme) {
		return 0, false
	}
	t, err := strconv.ParseFloat(ins[len(copyScheme):], 64)
	return t, err == nil && t >= 0
}

func (s cutSeg) isCopy() bool {
	_, ok := copySrc(s.Ins)
	return ok
}

// audioIns says this insert is sound alone: an audio file placed on the cut.
// Its picture is the session's own, which is why its marker is drawn in the
// audio lanes and not in the picture band.
func (s cutSeg) audioIns() bool {
	return s.isInsert() && insKind(s.Ins) == "audio"
}

// The two readings of Mute, which is one flag because it is one sentence --
// this insert brings no sound of its own -- and two behaviours because the mode
// already says what is underneath it.
//
// keepsSoundUnder: the cut was NOT opened for it, so the footage it covers is
// still there being heard. Only the picture is replaced, and the render takes
// the sound from the recording instead of from the file (produce.go, the isInsert
// case, which puts it in prodClip.snd exactly where an audio insert's file
// would go).
//
// playsSilent: the cut WAS opened for it, so there is nothing underneath to
// keep and no sound anywhere in the slot. clipInput reports the input as having
// no audio and the silence comes from anullsrc, the same way a held frame's
// does.
func (s cutSeg) keepsSoundUnder() bool { return s.isInsert() && s.Mute && !s.spliced() }
func (s cutSeg) playsSilent() bool     { return s.isInsert() && s.Mute && s.spliced() }

// insName is what an insert is called on the track and in the status line: the
// file's name, or, for a copy, the footage it plays again -- a copy has no file
// to name, and the seconds are how the eye finds the original.
func insName(s cutSeg) string {
	if from, ok := copySrc(s.Ins); ok {
		return "copy of " + mmss(from)
	}
	return insBase(s.Ins)
}

// applyInsert puts the Edit dialog's answer into the cut -- words, mode, length
// -- as one undo step. Not a field assignment: going over the footage takes
// seconds out of it, going between hands them back.
func (ed *cutEditor) applyInsert(i int, ins string, m insMode) {
	if i < 0 || i >= len(ed.segs) || !ed.segs[i].isInsert() {
		return
	}
	if m.dur < minSegLn {
		m.dur = insDefault
	}
	ed.pushUndo()
	card := ed.segs[i]
	card.Ins = ins
	// the dialog asks this one whenever it is a live question (insMode.askMute);
	// otherwise m.mute came back exactly as the card handed it over
	card.Mute = m.mute
	if m.splice {
		if !card.spliced() {
			ed.returnFootage(i)
		}
		card.E, card.Dur = card.S, m.dur
		ed.segs[i] = card
	} else {
		card.E, card.Dur = card.S+m.dur, 0
		ed.putOver(i, card)
	}
	ed.coalesce()
	ed.persist()
	ed.reholdSeg(card)
}

// setSpliced switches a card between the modes and changes nothing else.
func (ed *cutEditor) setSpliced(i int, on bool) {
	if i < 0 || i >= len(ed.segs) || ed.segs[i].spliced() == on {
		return
	}
	s := ed.segs[i]
	ed.applyInsert(i, s.Ins, insMode{splice: on, dur: s.length()})
}

// returnFootage gives the clip before card i the seconds the card was covering,
// so that when it stops covering them the footage is whole again. The clip after
// it then touches this one and coalesce makes them one clip, which is what they
// were before the card was placed.
//
// A card that took nothing -- one dropped in the hole between two recordings --
// finds no clip ending where it starts, and nothing is given back.
func (ed *cutEditor) returnFootage(i int) {
	card := ed.segs[i]
	for j := range ed.segs {
		p := &ed.segs[j]
		if p.isInsert() || math.Abs(p.E-card.S) > 0.05 {
			continue
		}
		hi := card.E
		if v := ed.videoAt(p.S); v != nil {
			hi = math.Min(hi, v.start+v.dur) // never past the end of the file
		}
		if hi > p.E {
			p.E = hi
		}
		return
	}
}

// putOver seats card i over the footage in its own seconds: whatever is under it
// gives way, the same surgery addInsert does. Written as a removal because that
// is what it is -- the card is lifted out of the list first, or removeSpan would
// drop it along with the footage, an insert being a file and not a span.
func (ed *cutEditor) putOver(i int, card cutSeg) {
	rest := make([]cutSeg, 0, len(ed.segs))
	rest = append(rest, ed.segs[:i]...)
	ed.segs = append(rest, ed.segs[i+1:]...)
	ed.removeSpan(card.S, card.E)
	ed.segs = append(ed.segs, card)
}

// indexOfSeg finds a clip in the list again by what it is: an insert is its file
// at its own start time, and nothing else in a cut is both.
func (ed *cutEditor) indexOfSeg(want cutSeg) int {
	for i, s := range ed.segs {
		if s.Ins == want.Ins && math.Abs(s.S-want.S) < 0.001 {
			return i
		}
	}
	return -1
}

// reholdSeg takes hold of a clip again after the list has been rearranged under
// it -- found by what it is, not by where it was, since coalesce sorts and
// renumbers. An edit made from the toolbar should leave the thing being edited
// still held, or the next edit needs the card picked up a second time.
func (ed *cutEditor) reholdSeg(want cutSeg) {
	if i := ed.indexOfSeg(want); i >= 0 {
		ed.segOn, ed.segSel, ed.segDirty = true, i, false
	}
	ed.syncInsertBtn()
	ed.redrawTracks()
}

// removeSpan is removeRange without the bookkeeping -- the same surgery, so
// that a caller doing several things at once pushes one undo and saves once.
func (ed *cutEditor) removeSpan(t0, t1 float64) {
	var out []cutSeg
	for _, s := range ed.segs {
		if s.E <= t0 || s.S >= t1 { // untouched
			out = append(out, s)
			continue
		}
		if s.isInsert() {
			// an insert is a file, not a span: it is dropped whole or not at all.
			// Trimming one to a fraction of its length would leave a clip nobody
			// asked for, showing part of a card.
			continue
		}
		// the halves are the same scene shortened, not new ones: they keep
		// whatever it said about itself, and what it says now is which camera
		// it shows
		if s.S < t0 && t0-s.S >= minSegLn {
			h := s
			h.E = t0
			out = append(out, h)
		}
		if s.E > t1 && s.E-t1 >= minSegLn {
			h := s
			h.S = t1
			out = append(out, h)
		}
	}
	ed.segs = out
}

// stealSpan takes t0..t1 away from the scenes on every row BUT cam: the newer
// green wins, which is how painting camera B over camera A's green switches
// camera. Inserts are left alone -- an insert is not a camera.
func (ed *cutEditor) stealSpan(t0, t1 float64, cam int) {
	var out []cutSeg
	for _, s := range ed.segs {
		if s.isInsert() || s.Cam == cam || s.E <= t0 || s.S >= t1 {
			out = append(out, s)
			continue
		}
		if s.S < t0 && t0-s.S >= minSegLn {
			h := s
			h.E = t0
			out = append(out, h)
		}
		if s.E > t1 && s.E-t1 >= minSegLn {
			h := s
			h.S = t1
			out = append(out, h)
		}
	}
	ed.segs = out
}

func (ed *cutEditor) removeRange(t0, t1 float64) {
	if t1 < t0 {
		t0, t1 = t1, t0
	}
	ed.removeSpan(t0, t1)
	ed.coalesce()
	ed.persist()
}

// ---- moving a clip edge by hand ---------------------------------------------
//
// The green borders are handles: hovering highlights one, a press picks it up,
// and ‹f/f› then move it a frame at a time instead of the playhead. Both put the
// picture on the frame the edge cuts on.

// edgeAt is the clip edge nearest a point of the timeline, within edgeGrab px:
// the segment's index and which side of it. The waveform lanes answer to it as
// the picture band does -- a cut point is a time, and every band is the same
// timeline seen a different way.
func (ed *cutEditor) edgeAt(px float64) (int, bool, bool) {
	// the borders on the press's own side first, then the rest. Folded, one clip's
	// end and the next one's start are the SAME x (cut_fold.go), so nearest-wins
	// could never take the right-hand start; the clip the press is inside is the
	// clip it means.
	for _, inside := range []bool{true, false} {
		seg, end, near := -1, false, edgeGrab
		for i, s := range ed.segs {
			// A spliced card has no borders to trim: it sits at one point of the
			// footage and its length is its own, typed in the dialog. Both its edges
			// are that one x -- the middle of the marker you press to take hold of
			// the card -- so answering with an edge here would hand you a border of a
			// clip with no length instead of the card you pressed, and the border
			// wins over the clip.
			if s.spliced() {
				continue
			}
			x0, x1 := ed.xOf(s.S), ed.xOf(s.E)
			if d := math.Abs(x0 - px); d < near && (!inside || px >= x0) {
				seg, end, near = i, false, d
			}
			if d := math.Abs(x1 - px); d < near && (!inside || px <= x1) {
				seg, end, near = i, true, d
			}
		}
		if seg >= 0 {
			return seg, end, true
		}
	}
	return -1, false, false
}

// grabEdge picks up the edge under a timeline x, and says whether it found one.
func (ed *cutEditor) grabEdge(px float64) bool {
	seg, end, ok := ed.edgeAt(px)
	if !ok {
		return false
	}
	ed.edgeOn, ed.edgeSeg, ed.edgeEnd, ed.edgeDirty = true, seg, end, false
	ed.segOn, ed.fxOn = false, false // one thing is held at a time, and this is now it
	ed.syncInsertBtn()
	side := "start"
	if end {
		side = "end"
	}
	// what is in hand, and the one thing about it that cannot be seen: which
	// button moves it. The rest -- the frame keys, ⌦, that a click elsewhere
	// puts it down -- was a paragraph printed on every press, and a status bar
	// is one line wide (fxStatus).
	ed.a.setStatus(fmt.Sprintf("clip %d's %s picked up at %s — right-drag to trim",
		seg+1, side, fmtClock(ed.edgeTime())))
	ed.redrawTracks()
	return true
}

// edgeHeld is whether a clip border is in hand.
func (ed *cutEditor) edgeHeld() bool { return ed.edgeOn && ed.edgeSeg < len(ed.segs) }

// onHeldEdge says whether a timeline x is close enough to the held edge to take
// hold of it. This is what makes a left drag mean "move this" rather than "start
// a new selection": the press has to land on the bar, not merely somewhere on a
// page that happens to have an edge held.
func (ed *cutEditor) onHeldEdge(px float64) bool {
	if !ed.edgeHeld() {
		return false
	}
	return math.Abs(ed.xOf(ed.edgeTime())-px) <= edgeMove
}

func (ed *cutEditor) dropEdge() {
	if !ed.edgeOn {
		return
	}
	ed.edgeOn = false
	ed.redrawTracks()
}

// edgeTime is where the held edge is now, or 0 when nothing is held.
func (ed *cutEditor) edgeTime() float64 {
	if !ed.edgeHeld() {
		return 0
	}
	if ed.edgeEnd {
		return ed.segs[ed.edgeSeg].E
	}
	return ed.segs[ed.edgeSeg].S
}

// clampEdge is how far an edge may travel: never so far that its own clip is
// shorter than minSegLn, never onto the neighbouring clip, and never out of the
// recording it was cut from (lo..hi). The cut is the input to every step after
// this one, so this arithmetic sits on its own where it can be tested rather
// than inside a mouse handler.
func clampEdge(segs []cutSeg, i int, end bool, t, lo, hi float64) float64 {
	s := segs[i]
	if end {
		if i+1 < len(segs) {
			hi = math.Min(hi, segs[i+1].S)
		}
		return math.Min(math.Max(t, s.S+minSegLn), hi)
	}
	if i > 0 {
		lo = math.Max(lo, segs[i-1].E)
	}
	return math.Max(math.Min(t, s.E-minSegLn), lo)
}

// moveEdgeTo puts the held edge at a session time, as far as it may go. live
// says the mouse is still down, and then the cut is only redrawn: writing
// cut.json (and re-reading the folder it is in, and re-gating two tabs) on
// every motion event is a lot of work for a version of the cut that exists for
// sixteen milliseconds. The drag's end writes the one that matters.
func (ed *cutEditor) moveEdgeTo(t float64, live bool) {
	if !ed.edgeHeld() {
		ed.edgeOn = false
		return
	}
	s := &ed.segs[ed.edgeSeg]
	lo, hi := math.Inf(-1), math.Inf(1)
	if v := ed.videoAt((s.S + s.E) / 2); v != nil {
		lo, hi = v.start, v.start+v.dur
	}
	t = clampEdge(ed.segs, ed.edgeSeg, ed.edgeEnd, t, lo, hi)
	if (ed.edgeEnd && t == s.E) || (!ed.edgeEnd && t == s.S) {
		return // against a stop: not an edit, and not worth an undo step
	}
	// one undo entry for the whole hold, not one per mouse move: a drag is a
	// single act, and fifty of them would be the entire history
	if !ed.edgeDirty {
		ed.pushUndo()
		ed.edgeDirty = true
	}
	if ed.edgeEnd {
		s.E = t
	} else {
		s.S = t
	}
	if live {
		ed.updateTotal()
		ed.redrawTracks()
		return
	}
	ed.persist()
}

// edgeFPS is the frame rate of the recording the held edge falls in, which is
// what a "frame" means for it. 30 when there is nothing there to ask.
func (ed *cutEditor) edgeFPS() float64 {
	if v := ed.videoAt(ed.edgeTime()); v != nil && v.fps > 0 {
		return v.fps
	}
	return 30
}

// edgeFrame is the frame the held edge is judged by. An end edge is a frame
// short of itself: the boundary's own frame is the first one the cut does NOT
// keep, and what you are looking at is the last one it does.
func (ed *cutEditor) edgeFrame() float64 {
	t := ed.edgeTime()
	if ed.edgeEnd {
		t -= 1 / ed.edgeFPS()
	}
	return t
}

// showEdge puts the preview on that frame, so the edge is moved against the
// picture rather than against a ruler. live is a drag still in progress, and
// then it is thinned to scrubEvery -- the seeks would otherwise queue up behind
// the mouse and the picture would arrive after the drag had ended.
func (ed *cutEditor) showEdge(live bool) {
	if !ed.edgeHeld() {
		return
	}
	if live {
		if time.Since(ed.lastScrub) < scrubEvery {
			return
		}
		ed.lastScrub = time.Now()
	}
	ed.setPlayhead(ed.edgeFrame())
}

// edgeStatus reads the whole clip out, because moving one border is how you get
// a clip of the length you wanted and the length is the thing you are watching.
func (ed *cutEditor) edgeStatus() {
	if !ed.edgeHeld() {
		return
	}
	s := ed.segs[ed.edgeSeg]
	ed.a.setStatus(fmt.Sprintf("clip %d: %s – %s (%s)", ed.edgeSeg+1,
		fmtClock(s.S), fmtClock(s.E), ed.spanSecs(s.S, s.E)))
}

// nudgeEdge moves the held edge by whole frames and shows the frame it lands
// on. False means there was no edge to move after all (see frameStep).
func (ed *cutEditor) nudgeEdge(n int) bool {
	if !ed.edgeHeld() {
		ed.edgeOn = false
		return false
	}
	ed.moveEdgeTo(ed.edgeTime()+float64(n)/ed.edgeFPS(), false)
	ed.showEdge(false)
	ed.edgeStatus()
	return true
}

// ---- moving a whole clip by hand ---------------------------------------------
//
// The same gesture as an edge, aimed one level up. A press near a border picks
// that border up; a DOUBLE click anywhere else on a clip picks up the whole
// clip, and a drag then slides it with its length intact. It is a double one
// because a single press over the footage is already how the playhead is put
// somewhere. That is the edit the page had no spelling for: "this scene, but
// four seconds later" was two edge drags that had to agree with each other to the
// frame, and if they disagreed the clip changed length instead of moving.
//
// A dragged clip snaps to the clip either side of it, so "put these two
// together" is a gesture rather than an arithmetic exercise -- and it stops
// there, because clips may not overlap and a clip may not leave the recording it
// was cut from: its frames are that file's frames, and sliding it into the next
// recording would show footage nobody selected.

// spliceSpan is the marker of a spliced card in view px: from the splice point,
// as wide as the card's seconds at this zoom. Every hit test and draw of the
// marker goes through here.
func (ed *cutEditor) spliceSpan(s cutSeg) (float64, float64) {
	x := ed.xOf(s.S)
	return x, x + math.Max(splicePx, s.Dur*ed.pps)
}

// segSpan is where a clip is on the timeline: its own two borders, or the marker
// for a spliced card, which has no borders of its own.
func (ed *cutEditor) segSpan(s cutSeg) (float64, float64) {
	if s.spliced() {
		return ed.spliceSpan(s)
	}
	return ed.xOf(s.S), ed.xOf(s.E)
}

// segAtPx is the clip under a point of the timeline, or -1. Searched from the
// top down -- inserts are painted over the footage, so an insert dropped inside
// a kept clip is the thing you are pointing at, not the clip behind it.
func (ed *cutEditor) segAtPx(px float64) int {
	for i := len(ed.segs) - 1; i >= 0; i-- {
		x0, x1 := ed.segSpan(ed.segs[i])
		if px >= x0 && px <= x1 {
			return i
		}
	}
	return -1
}

// segOnGreen is the scene a point of the picture band is actually DRAWN on:
// segAtPx's second, then the row (segTop) -- the two differ where the cut
// shows one camera and the pointer is on another. segAtPx alone is right for
// anything measured along the clock, wrong for "what did I press ON".
func (ed *cutEditor) segOnGreen(px, y float64) int {
	i := ed.segAtPx(px)
	if i < 0 {
		return -1
	}
	if t := ed.segTop(ed.segs[i]); y < t || y >= t+ed.laneH() {
		return -1
	}
	return i
}

// grabSeg picks up the whole clip under a timeline x.
func (ed *cutEditor) grabSeg(px float64) bool {
	i := ed.segAtPx(px)
	if i < 0 {
		return false
	}
	ed.edgeOn, ed.fxOn = false, false // one thing is held at a time, and this is now it
	ed.segOn, ed.segSel, ed.segDirty = true, i, false
	s := ed.segs[i]
	what := fmt.Sprintf("clip %d (%s – %s)", i+1, fmtClock(s.S), fmtClock(s.E))
	if s.isInsert() {
		what = fmt.Sprintf("%s at %s", insBase(s.Ins), fmtClock(s.S))
	}
	ed.a.setStatus(what + " picked up — right-drag to move")
	ed.syncInsertBtn()
	ed.redrawTracks()
	return true
}

func (ed *cutEditor) dropSeg() {
	if !ed.segOn {
		return
	}
	ed.segOn = false
	ed.syncInsertBtn()
	ed.redrawTracks()
}

// heldSeg is the clip being held, or nil.
func (ed *cutEditor) heldSeg() *cutSeg {
	if !ed.segOn || ed.segSel >= len(ed.segs) {
		return nil
	}
	return &ed.segs[ed.segSel]
}

// onHeldSeg is whether a left press lands on the held clip, which is what makes
// the drag a move rather than a new selection. Same rule as onHeldEdge: the
// press has to land on the thing, not merely on a page that has one held.
func (ed *cutEditor) onHeldSeg(px float64) bool {
	s := ed.heldSeg()
	if s == nil {
		return false
	}
	x0, x1 := ed.segSpan(*s)
	return px >= x0 && px <= x1
}

// clampSeg is where a clip may sit: inside the recording it was cut from, clear
// of its neighbours, snapped flush when close. lo/hi bound the recording; segs
// are in timeline order, i is the clip moved; snap is seconds counting as
// touching -- a px tolerance at the current zoom.
func clampSeg(segs []cutSeg, i int, t, lo, hi, snap float64) float64 {
	s := segs[i]
	ln := s.E - s.S
	// the neighbours: the nearest clip before and after in time, which is what
	// "the next area" means to a hand that can see them
	before, after := math.Inf(-1), math.Inf(1)
	for j, o := range segs {
		if j == i {
			continue
		}
		if o.E <= s.S && o.E > before {
			before = o.E
		}
		if o.S >= s.E && o.S < after {
			after = o.S
		}
	}
	lo, hi = math.Max(lo, before), math.Min(hi, after)
	if math.Abs(t-lo) <= snap {
		t = lo // flush against what comes before
	} else if math.Abs(t+ln-hi) <= snap {
		t = hi - ln // ...or against what comes after
	}
	if t+ln > hi {
		t = hi - ln
	}
	if t < lo {
		t = lo
	}
	return t
}

// moveSegTo slides the held clip so that it starts at t. live is a drag still in
// progress, and then the cut is only redrawn -- the same rule moveEdgeTo works
// by, and for the same reason: cut.json is written once, when the hand stops.
func (ed *cutEditor) moveSegTo(t float64, live bool) {
	s := ed.heldSeg()
	if s == nil {
		ed.segOn = false
		return
	}
	lo, hi := math.Inf(-1), math.Inf(1)
	// footage may not leave its own recording; an insert is a file and belongs
	// to none, so it may go anywhere the clips around it leave room
	if !s.isInsert() {
		if v := ed.videoAt((s.S + s.E) / 2); v != nil {
			lo, hi = v.start, v.start+v.dur
		}
	}
	t = clampSeg(ed.segs, ed.segSel, t, lo, hi, snapPx/math.Max(ed.pps, 0.001))
	if t == s.S {
		return // against a stop: not an edit, and not worth an undo step
	}
	if !ed.segDirty {
		ed.pushUndo() // one entry for the whole drag, as with an edge
		ed.segDirty = true
	}
	s.E, s.S = t+(s.E-s.S), t
	if live {
		ed.updateTotal()
		ed.redrawTracks()
		return
	}
	ed.persist()
}

// showSeg puts the preview on the held clip's first frame, so a clip is moved
// against the picture it will start on. Throttled while dragging, exactly as
// showEdge is, and for the same reason.
func (ed *cutEditor) showSeg(live bool) {
	s := ed.heldSeg()
	if s == nil {
		return
	}
	if live {
		if time.Since(ed.lastScrub) < scrubEvery {
			return
		}
		ed.lastScrub = time.Now()
	}
	ed.setPlayhead(s.S)
}

func (ed *cutEditor) segStatus() {
	s := ed.heldSeg()
	if s == nil {
		return
	}
	ed.a.setStatus(fmt.Sprintf("clip %d: %s – %s (%s)", ed.segSel+1,
		fmtClock(s.S), fmtClock(s.E), ed.spanSecs(s.S, s.E)))
}

// nudgeSeg moves the held clip by whole frames, the same way ‹f and f› move a
// held edge.
func (ed *cutEditor) nudgeSeg(n int) bool {
	s := ed.heldSeg()
	if s == nil {
		ed.segOn = false
		return false
	}
	fps := 30.0
	if v := ed.videoAt(s.S); v != nil && v.fps > 0 {
		fps = v.fps
	}
	ed.moveSegTo(s.S+float64(n)/fps, false)
	ed.showSeg(false)
	ed.segStatus()
	return true
}

// what a press took hold of, which is what the gesture that made it has to do
// next: trim, move, or start a selection.
const (
	pickNone = iota
	pickEdge
	pickSeg
)

// pickAt is what a press at a timeline point means, in the order that matters:
// the held edge first (wider tolerance, edgeMove), then any border, then -- with
// clips true, which is the double click -- the clip. Picking something up never
// moves the red line; only a drag or ‹f/f› does.
func (ed *cutEditor) pickAt(px float64, clips bool) int {
	switch {
	case ed.onHeldEdge(px):
		ed.edgeStatus() // already yours, and the bar is what you aimed at
		return pickEdge
	case ed.grabEdge(px):
		return pickEdge
	case !clips:
		return pickNone
	case ed.onHeldSeg(px):
		ed.segStatus() // it is already yours; here is what you are holding
		return pickSeg
	case ed.grabSeg(px):
		return pickSeg
	}
	ed.dropEdge()
	ed.dropSeg()
	return pickNone
}

// redrawTracks repaints every band of the timeline. One call rather than a
// QueueDraw per area at each of a dozen call sites: the bands are one picture of
// one cut, and they have not stayed the same set -- the lanes arrived long after
// the footage, and the second picture band left again.
func (ed *cutEditor) redrawTracks() {
	if ed.srcArea == nil {
		return
	}
	ed.queueTracks()
	// the framing overlay is a view of the same state -- where the camera is
	// at the playhead, what is held -- and its pointer-grabbing follows the
	// same state, so both are settled here rather than at every call site
	if ed.fxArea != nil {
		ed.syncFxCursor()
		ed.syncPreviewZoom()
	}
}

// queueTracks is the drawing half of redrawTracks: every band asked to paint,
// and nothing about the preview. A pan and a zoom come through here directly,
// because all they changed is where things are drawn -- and syncPreviewZoom
// writes a transform and a size request onto the preview widget, which is a
// relayout of the largest widget on the page for a wheel notch that did not
// touch it.
func (ed *cutEditor) queueTracks() {
	if ed.srcArea == nil {
		return
	}
	ed.fitSrc() // the effects lane is as deep as the effects pile up
	ed.srcArea.QueueDraw()
	if ed.audArea != nil {
		ed.audArea.QueueDraw()
	}
	// the line layer scrolls and zooms with the bands, and the green bar the
	// bands were painted with is remembered so the playback tick can tell a
	// line that moved from a line that moved ONTO another scene (redrawLine)
	ed.lineIdx = ed.bandClipIdx()
	if ed.lineArea != nil {
		ed.lineArea.QueueDraw()
	}
	if ed.fxArea != nil {
		ed.fxArea.QueueDraw()
	}
}

// cutState is everything Undo and Revert put back: the segments, the effects,
// the aspect, and the corrections made to the timeline itself. One snapshot,
// not four -- an edit that changed the camera and an edit that changed the cut
// undo the same way, and an undo that restored the segments but left the
// effects would un-mix two lists the user edited as one page.
type cutState struct {
	segs   []cutSeg
	fx     []cutFx
	aspect string
	// where the sources were, and which rows they were on (cut_shift.go). A
	// right drag is an edit like any other and has to come back the same way,
	// and the rows come with it because they were frozen BY a drag: an undo
	// that put the seconds back and left the rows frozen would leave the
	// project pinned to a shape it no longer has.
	shift map[string]float64
	rows  map[string]int
	// and the rows the cut itself put on the band. Adding one is an edit, so
	// taking it back is an undo like any other (cut_lane.go).
	lanes []cutLane
	// and the floor under the row count, or ✕ on an emptied bottom row would
	// be an edit Undo cannot take back (cutEditor.nRows)
	nRows int
}

func (ed *cutEditor) snapshot() cutState {
	return cutState{append([]cutSeg(nil), ed.segs...), append([]cutFx(nil), ed.fx...), ed.aspect,
		copyShift(ed.shift), copyRows(ed.rows), append([]cutLane(nil), ed.cutLanes...), ed.nRows}
}

func (ed *cutEditor) restore(st cutState) {
	ed.segs = st.segs
	ed.fx = st.fx
	ed.dropFx() // whatever was held may not exist in the restored list
	// the sources are moved back before anything is measured against them,
	// and only when they actually moved: relayout is not free, and every
	// ordinary undo would otherwise pay for a feature it did not use
	if !sameShift(ed.shift, st.shift) || !sameRows(ed.rows, st.rows) ||
		!sameCutLanes(ed.cutLanes, st.lanes) || ed.nRows != st.nRows {
		for b := range ed.shift {
			if _, ok := st.shift[b]; !ok {
				ed.slideSrc(b, -ed.shift[b])
			}
		}
		for b, d := range st.shift {
			ed.slideSrc(b, d-ed.shift[b])
		}
		// a copy in hand is a row NUMBER and a session second, and it is the
		// one thing here the snapshot does not hold: taken while the band had
		// one shape and pasted after it was put back into another, it would
		// splice footage off a camera nobody chose. Undo drops the selection
		// and the marks for the same reason -- what the hand was holding was
		// held against a cut that is no longer the cut.
		if !sameRows(ed.rows, st.rows) {
			ed.copyOn = false
			ed.syncInsertBtn()
		}
		ed.shift, ed.rows = copyShift(st.shift), copyRows(st.rows)
		ed.nRows = st.nRows
		// after the corrections, because the cut's own rows are rebuilt from
		// what cut.json says rather than moved, and where they land is the
		// correction the shift map has just been put back to
		ed.setLanes(st.lanes)
		sortLanes(ed.auds)
		ed.relayout()
	}
	ed.setAspect(st.aspect)
}

// pushUndo snapshots the cut before an edit. Every path that changes segs, fx
// or the aspect goes through here first, so Add, Remove, Suggest and every
// effect edit are all reversible -- pressing Add is a try, not a commitment.
func (ed *cutEditor) pushUndo() {
	ed.undo = append(ed.undo, ed.snapshot())
	if len(ed.undo) > undoDeep {
		ed.undo = ed.undo[len(ed.undo)-undoDeep:]
	}
	// a fresh edit forks history: what Undo took back no longer leads to this
	// cut, and a Redo that grafted it on anyway would interleave two histories
	ed.redo = nil
	ed.syncButtons()
}

func (ed *cutEditor) undoLast() {
	if len(ed.undo) == 0 {
		ed.a.setStatus("nothing to undo")
		return
	}
	ed.redo = append(ed.redo, ed.snapshot())
	ed.restore(ed.undo[len(ed.undo)-1])
	ed.undo = ed.undo[:len(ed.undo)-1]
	ed.sel.active = false
	ed.clearMarks()
	ed.persist()
	ed.syncButtons()
	ed.a.setStatus(fmt.Sprintf("undone — %d segment(s) left", len(ed.segs)))
}

// redoLast is undoLast run backwards: the cut Undo stepped away from goes back
// on, and the step itself goes back on the undo pile -- appended raw, because
// pushUndo would clear the very stack this is walking.
func (ed *cutEditor) redoLast() {
	if len(ed.redo) == 0 {
		ed.a.setStatus("nothing to redo")
		return
	}
	ed.undo = append(ed.undo, ed.snapshot())
	ed.restore(ed.redo[len(ed.redo)-1])
	ed.redo = ed.redo[:len(ed.redo)-1]
	ed.sel.active = false
	ed.clearMarks()
	ed.persist()
	ed.syncButtons()
	ed.a.setStatus(fmt.Sprintf("redone — %d segment(s)", len(ed.segs)))
}

// setBase marks the current cut as what Revert returns to: whatever was on disk
// when the page loaded, or whatever Suggest produced. Everything after that is
// the user's own delta.
func (ed *cutEditor) setBase() {
	ed.base = ed.snapshot()
	ed.syncButtons()
}

func sameCut(a, b []cutSeg) bool {
	if len(a) != len(b) {
		return false
	}
	for i := range a {
		if !sameSeg(a[i], b[i]) {
			return false
		}
	}
	return true
}

// sameSeg is == for a struct holding a slice (Quiet). Every field is named by
// hand; TestEverySegmentFieldCountsAsAChange fails on any field this forgets.
func sameSeg(a, b cutSeg) bool {
	return a.S == b.S && a.E == b.E && a.Ins == b.Ins && a.Dur == b.Dur &&
		a.Rate == b.Rate && a.Ss == b.Ss && a.Mute == b.Mute &&
		a.Cam == b.Cam && a.Lane == b.Lane && a.Split == b.Split &&
		sameQuiet(a.Quiet, b.Quiet)
}

// sameQuiet compares the two as SETS, not as lists. Which lane the user
// silenced first is not part of what the cut sounds like, and Revert lighting
// up because two toggles were pressed in the other order would be a lie about
// there being an unsaved change.
func sameQuiet(a, b []string) bool {
	if len(a) != len(b) {
		return false
	}
	for _, q := range a {
		if !laneQuiet(b, q) {
			return false
		}
	}
	return true
}

func sameState(a, b cutState) bool {
	if !sameCut(a.segs, b.segs) || a.aspect != b.aspect || len(a.fx) != len(b.fx) {
		return false
	}
	if !sameShift(a.shift, b.shift) || !sameCutLanes(a.lanes, b.lanes) {
		return false // a lane dragged into place, or put there, is worth Reverting
	}
	for i := range a.fx {
		if a.fx[i] != b.fx[i] {
			return false
		}
	}
	return true
}

func (ed *cutEditor) syncButtons() {
	if ed.undoBtn != nil {
		ed.undoBtn.SetSensitive(len(ed.undo) > 0)
	}
	if ed.redoBtn != nil {
		ed.redoBtn.SetSensitive(len(ed.redo) > 0)
	}
	if ed.revertBtn != nil {
		ed.revertBtn.SetSensitive(!sameState(ed.snapshot(), ed.base))
	}
	ed.syncInsertBtn()
	ed.syncPlayBtn()
}

// syncPlayBtn greys ▶✂ out when there is nothing it could honestly play: its
// preview is the finished video, an empty cut IS an empty video, and with no
// clips there are no gaps to skip either -- it would run the whole recording
// against its face's one promise. Plain ▶ never greys: the recording is always
// there to play. Synced from setCutOnly and from syncButtons, which every edit
// passes through, so adding the first clip wakes the button up again.
func (ed *cutEditor) syncPlayBtn() {
	if ed.cutPlayBtn == nil {
		return
	}
	ed.cutPlayBtn.SetSensitive(len(ed.segs) > 0)
}

// syncInsertBtn tells the Insert button which of its two jobs it is doing. Held
// card: it opens that card, so it says Edit. Anything else: it puts a new one in
// at the playhead.
func (ed *cutEditor) syncInsertBtn() {
	if ed == nil || ed.insBtn == nil {
		return
	}
	// what the copy in hand would be put down as, on the button that puts it
	// there. Grey with nothing in hand, which is most of the time.
	if ed.pasteBtn != nil {
		ed.pasteBtn.SetSensitive(ed.copyOn)
		switch {
		case !ed.copyOn:
			ed.pasteBtn.SetTooltipText("put a copy down at the red line — ⧉ takes one first")
		case ed.copyAud != "":
			ed.pasteBtn.SetTooltipText(fmt.Sprintf("lay the copied sound (%.1f s of %s, %s – %s) "+
				"over the footage at the red line: the picture runs on and the video stays "+
				"exactly as long. Esc drops the copy",
				ed.copyLen, ed.copyAud, mmss(ed.copyFrom), mmss(ed.copyFrom+ed.copyLen)))
		default:
			ed.pasteBtn.SetTooltipText(fmt.Sprintf("splice the copied footage (%s – %s, %.1f s) into "+
				"the cut at the red line: the cut is opened there, those seconds play again, and "+
				"the video gets longer by them. Esc drops the copy",
				mmss(ed.copyFrom), mmss(ed.copyFrom+ed.copyLen), ed.copyLen))
		}
	}
	if ed.laneBtn != nil {
		// footage only: a copied SOUND has no picture to put on a row, and the
		// lane it stands in for is already chosen (cutSeg.Lane). Grey rather
		// than hidden, like the paste beside it: a button that comes and goes
		// moves every button after it, and a bar whose contents shift as you
		// work is one you cannot aim at.
		ed.laneBtn.SetSensitive(ed.copyOn && ed.copyAud == "")
		if !ed.copyOn {
			ed.laneBtn.SetTooltipText("put a copy on a row of its own — ⧉ takes one first")
		}
		if ed.copyOn {
			ed.laneBtn.SetTooltipText(fmt.Sprintf("put the copied footage (%s – %s, %.1f s) "+
				"on a row of its own starting at the red line, instead of splicing it into "+
				"the cut. Nothing is cut to it yet: select on the new row and press ＋ Add, "+
				"the same way you would cut to a second camera",
				mmss(ed.copyFrom), mmss(ed.copyFrom+ed.copyLen), ed.copyLen))
		}
	}
	if s := ed.heldSeg(); s != nil && s.isInsert() {
		ed.insBtn.SetIconName("document-edit-symbolic")
		ed.insBtn.SetTooltipText("change the held card — what it says, and whether it plays " +
			"over the footage (overwrite) or between it (insert)")
		return
	}
	if f := ed.heldFx(); f != nil {
		ed.insBtn.SetIconName("document-edit-symbolic")
		ed.insBtn.SetTooltipText("change the held effect — " + f.fxLabel())
		return
	}
	ed.insBtn.SetIconName("insert-object-symbolic")
	ed.insBtn.SetTooltipText("put a file in the cut at the playhead — a video sting, a still, " +
		"or an SVG that animates itself. A selected region gives it its length; " +
		"otherwise the file's own. With the selection drawn in a lane's own wave " +
		"it offers sounds instead, and lays one over those seconds without moving the " +
		"picture. A card (tier.svg, s.svg … in assets) or any SVG " +
		"with {{holes}} in it asks what to put on it first. Right-click a card on " +
		"the track to hold it, and this becomes Edit.")
}

// droppedSpans is the session time this cut throws away, as stretches: the
// holes between kept clips plus what hangs off either end of each recording.
// Built on demand for the cut preview's scrim. Inserts neither open nor close
// a hole (a spliced card has S == E, an overwriting one sits in kept footage).
func (ed *cutEditor) droppedSpans() [][2]float64 {
	var out [][2]float64
	for _, sp := range ed.runs() {
		t := sp.t0
		for _, s := range ed.segs {
			if s.isInsert() || s.E <= t || s.S >= sp.t1 {
				continue
			}
			if s.S > t {
				out = append(out, [2]float64{t, math.Min(s.S, sp.t1)})
			}
			t = math.Max(t, s.E)
		}
		if t < sp.t1 {
			out = append(out, [2]float64{t, sp.t1})
		}
	}
	return out
}

// segAt returns the index of the kept scene covering t, or -1.
func (ed *cutEditor) segAt(t float64) int {
	for i, s := range ed.segs {
		if t >= s.S && t < s.E {
			return i
		}
	}
	return -1
}

func (ed *cutEditor) coalesce() {
	// this is where the segment list is rearranged wholesale -- sorted, merged,
	// renumbered -- so a held edge or a held clip, which are indexes into it, have
	// to let go
	ed.edgeOn, ed.segOn = false, false
	sort.Slice(ed.segs, func(i, j int) bool { return ed.segs[i].S < ed.segs[j].S })
	var out []cutSeg
	film := -1 // where the last stretch of footage went, which is what merges
	for _, s := range ed.segs {
		// An insert never merges (it is a file, not seconds); the merge is into
		// the last FOOTAGE, since a spliced card takes no session time. Scenes of
		// different cameras never merge -- the seam is the switch -- and a border
		// | Split made is kept until a drag rejoins the two (mergeDropped).
		if !s.isInsert() && !s.Split && film >= 0 && s.Cam == out[film].Cam &&
			s.S <= out[film].E+mergeTol && allSpliced(out[film+1:]) {
			if s.E > out[film].E {
				out[film].E = s.E
			}
			continue
		}
		if !s.isInsert() {
			film = len(out)
		}
		out = append(out, s)
	}
	ed.segs = out
}

// allSpliced is whether everything here takes no session time -- which is to
// say, whether two clips either side of it are still next to each other.
func allSpliced(segs []cutSeg) bool {
	for _, s := range segs {
		if !s.spliced() {
			return false
		}
	}
	return true
}

// inserts are the cut's non-footage items, kept aside while the footage is
// replaced wholesale -- which is what a suggestion and an audit both do. They
// were placed by hand and no model was told they exist, so a run that came back
// without them has not decided against them, it never saw them.
func insertsOf(segs []cutSeg) []cutSeg {
	var out []cutSeg
	for _, s := range segs {
		if s.isInsert() {
			out = append(out, s)
		}
	}
	return out
}

// splitSpliced is the cut as a sequence of clips to render: every spliced
// insert cuts the footage it sits in and the halves come out either side. Not
// what the timeline stores -- there the footage is one clip with a splice
// point in it. Every step after this reads the cut through produceSegs.
func splitSpliced(segs []cutSeg) []cutSeg {
	ordered := append([]cutSeg(nil), segs...)
	sort.SliceStable(ordered, func(i, j int) bool {
		if ordered[i].S != ordered[j].S {
			return ordered[i].S < ordered[j].S
		}
		// a splice at the very start of a clip goes before it, not after: the
		// card is what you meant to see first
		return ordered[i].spliced() && !ordered[j].spliced()
	})
	var out []cutSeg
	for _, s := range ordered {
		n := len(out)
		if !s.spliced() || n == 0 {
			out = append(out, s)
			continue
		}
		// the clip it lands in is the one before it, since the list is in
		// order -- and it only cuts anything if it lands strictly inside
		prev := out[n-1]
		if prev.isInsert() || s.S <= prev.S || s.S >= prev.E {
			out = append(out, s)
			continue
		}
		tail := prev
		out[n-1].E, tail.S = s.S, s.S
		out = append(out, s, tail)
	}
	return out
}

func filmedOf(segs []cutSeg) []cutSeg {
	var out []cutSeg
	for _, s := range segs {
		if !s.isInsert() {
			out = append(out, s)
		}
	}
	return out
}

func (ed *cutEditor) persist() {
	// keyed, because cutFile.Sound is read on load and never written: an old
	// project's whole-cut choice is migrated into the scenes once (migrateSound)
	// and the field goes out of the file on the very next save
	// what is stored is the gaps as they now are: a fold is matched to a gap
	// by overlap, and this is where that reading is written back (syncFolds)
	ed.syncFolds()
	b, _ := json.MarshalIndent(cutFile{Segs: ed.segs, Aspect: ed.aspect, Fx: ed.fx,
		Shift: ed.shift, Rows: ed.rows, Lanes: ed.cutLanes, NRows: ed.nRows,
		Folds: ed.folds}, "", "  ")
	os.MkdirAll(filepath.Dir(ed.a.cutPath()), 0o755)
	if err := os.WriteFile(ed.a.cutPath(), append(b, '\n'), 0o644); err != nil {
		ed.a.logf("save cut: %v", err)
	}
	ed.updateTotal()
	ed.updateOut() // the file on disk just changed size
	// Narrate and Produce are gated on cut.json existing, and this is the only
	// place it comes into existence. Without this the tabs stayed grey after a
	// perfectly good cut and woke up only on a rescan or a restart, which looks
	// exactly like the cut not having worked.
	ed.a.updateGates()
	ed.syncButtons() // every edit changes whether there is a delta to revert
	// an edit can put a card under the playhead or take one away -- dropping an
	// insert, removing it, undoing either -- and the preview says which of those
	// happened before the line is moved again
	ed.showInsert()
	ed.redrawTracks()
}

// rawLen is the same cut with the speed effects taken off: the footage kept as
// filmed. "How much have I kept" and "how long is the video" are both asked.
func (ed *cutEditor) rawLen() float64 {
	sum := 0.0
	for _, s := range ed.segs {
		if s.Dur > 0 {
			sum += s.Dur // a card runs for its own length, whatever is around it
			continue
		}
		sum += math.Max(0, s.E-s.S)
	}
	return sum
}

// cutLen is how long the finished video is: a spliced card's own length,
// slowed footage as it plays, everything else the footage under it -- not the
// timeline's span.
func (ed *cutEditor) cutLen() float64 {
	sum := 0.0
	for _, s := range ed.fxSegs() {
		sum += s.length() // a spliced card takes no session time and still runs
	}
	return sum
}

// fxSegs is the cut as the render runs it: spliced cards cut out, every clip
// carrying its rate (splitSpliced + applyFx, produceSegs' own pipe). Every
// number this page prints about TIME goes through here -- a clip's session
// seconds stopped being the answer when speed effects existed.
func (ed *cutEditor) fxSegs() []cutSeg { return applyFx(splitSpliced(ed.segs), ed.fx) }

// cutPos is where session time t falls on the finished video's clock, effects
// and all. The reading the ▶✂ preview is asked for.
func (ed *cutEditor) cutPos(t float64) float64 { return cutPos(ed.fxSegs(), t) }

// runLen is how long session seconds t0..t1 run in the finished video (a
// selection or a held clip). A stop runs at ×1 here, as the render treats it:
// the footage under the still plays on, so a stop costs no time.
func (ed *cutEditor) runLen(t0, t1 float64) float64 {
	if t1 <= t0 {
		return 0
	}
	out, at := 0.0, t0
	for _, st := range rateSpans(ed.fx) {
		lo, hi := math.Max(st.t0, t0), math.Min(st.t1, t1)
		if hi <= lo {
			continue
		}
		out += math.Max(0, lo-at) // seconds no effect covers run at ×1
		out += (hi - lo) / st.applied()
		at = hi
	}
	return out + math.Max(0, t1-at)
}

// spanSecs is how a stretch's length is written in a status line: its own
// seconds, and what it comes to in the video when an effect makes those two
// different. Both, because both are being asked about -- the first is the
// footage you are pointing at, the second is what it costs the video -- and a
// cut with no effects in it reads exactly as it always did.
func (ed *cutEditor) spanSecs(t0, t1 float64) string {
	raw, run := t1-t0, ed.runLen(t0, t1)
	if math.Abs(raw-run) < 0.05 {
		return fmt.Sprintf("%.1f s", raw)
	}
	return fmt.Sprintf("%.1f s, %.1f s in the video", raw, run)
}

// updateTotal fills the three readings at the foot of the quiet column: how
// long the cut runs, how much footage it was cut from, and how many pieces it
// is in. One line each, in the column the playhead and the selection are read
// in -- they were one line of three facts separated by dots, which is a
// sentence in a column of readings.
func (ed *cutEditor) updateTotal() {
	if ed.total == nil {
		return
	}
	sum, src := ed.cutLen(), 0.0
	for _, v := range ed.vids {
		src += v.dur
	}
	ed.total.SetText(mmss(sum))
	ed.totalRaw.SetText(mmss(ed.rawLen()))
	ed.totalSrc.SetText(mmss(src))
	segs := strconv.Itoa(len(ed.segs))
	// counted separately because they are not segments of the session: two of
	// the "segments" being cards is the difference between a five-minute cut of
	// footage and a five-minute cut with a minute of graphics in it
	if n := len(insertsOf(ed.segs)); n > 0 {
		segs += fmt.Sprintf(", %d inserted", n)
	}
	ed.totalSegs.SetText(segs)
}

// updateInputs says what this page works from: the recordings on the tracks,
// and the session timeline Suggest is sent -- every line anyone said, merged
// with every recording's event log.
func (ed *cutEditor) updateInputs() {
	if ed == nil || ed.inputs == nil {
		return
	}
	src := 0.0
	var names []string
	for _, v := range ed.vids {
		src += v.dur
		names = append(names, fmt.Sprintf("%s  %s", mmss(v.dur), v.base))
	}
	// short enough to read in one glance: each input is named and counted, and
	// what each one MEANS is the tooltip's (see narrate.go's own row)
	line := fmt.Sprintf("%s · %s", plural(len(ed.vids), "video"), mmss(src))
	detail := strings.Join(names, "\n")
	if len(names) == 0 {
		line, detail = "nothing to cut — no source on Inputs is marked as footage", ""
	}
	// the separate recordings are not footage and are not part of the timeline's
	// geometry, but they are on the page, and a lane that starts in the middle
	// of the tracks and stops before the end is only explicable if this row says
	// why: each is placed by its own wall clock, and only the stretch that
	// overlaps the footage is drawn.
	if len(ed.auds) > 0 {
		sep := 0
		for _, au := range ed.auds {
			if !au.master {
				sep++
			}
		}
		line += " · +" + plural(sep, "recording")
		detail += "\n\nEvery sound in the session, placed by its own clock — only the part running while the footage ran is drawn, and all of it is what the preview plays:"
		for _, au := range ed.auds {
			kind := "mono"
			if au.chans >= 2 {
				// a stereo file with one signal in it is said as such: it is why
				// it is drawn on one lane, and the row is where that is explained
				kind = "L/R"
				if ed.lanes(au) < 2 {
					kind = "L=R"
				}
			}
			what := fmt.Sprintf("from %s into the session", mmss(au.start))
			if au.master {
				what = "the footage's own track"
			}
			detail += fmt.Sprintf("\n%s  %s  %s, %s", mmss(au.dur), au.base, kind, what)
		}
	}

	rows := loadTSVRows(filepath.Join(ed.a.transcriptDir(), "session.tsv"))
	switch {
	case len(rows) == 0:
		line += " · no timeline"
	default:
		speech, events := 0, 0
		for _, r := range rows {
			if r.spk == "EVENT" {
				events++
			} else {
				speech++
			}
		}
		// the COUNT is not information: nothing on this page is decided by 688
		// rather than 700. That the timeline is there at all is, so the row
		// speaks only when it is missing -- which is the state that stops
		// Suggest.
		detail += fmt.Sprintf("\n\n%d lines: %d spoken, %d what was on screen, all of it sent with Suggest",
			speech+events, speech, events)
		// the same string the request will carry, so the size is the real one
		detail += fmt.Sprintf("\n\nprepare/transcript/session.txt — %d kB, sent whole with the cut prompt",
			(len(sessionText(rows, ed.a.narratorMic(), ed.a.loadRetakes()))+512)/1024)
	}
	// the context rides along with every request this page makes -- in the
	// tooltip, not on the row: the box is on the page before this one and the
	// word "context" alone says nothing about what is in it.
	if c := ed.a.sessionCtx(); c != "" {
		detail += "\n\nSession context (Describe), sent with Suggest and the audit:\n" + c
	}
	ed.inputs.SetText(line)
	ed.inputs.SetTooltipText(strings.TrimSpace(detail))
}

// updateOut says what is on disk, which is not what ed.total says: the total is
// the cut in the editor, and until it is persisted the two differ.
func (ed *cutEditor) updateOut() {
	if ed == nil || ed.out == nil {
		return
	}
	ed.out.SetText(summarizeOutputs(ed.a.cutDir()))
}

// ---- drawing ---------------------------------------------------------------

// srcEdgeMark paints one end of a recording ON its own pictures: a band of
// amber diagonals with the border line down its outer side. dir is +1 for a
// beginning and -1 for an end, so the band always lies inside the footage it
// belongs to and never over the take next door.
//
// Striped and not a plain line, because a line only says WHERE. What happened
// here is that the camera stopped, and the minutes it was off take no width on
// this timeline at all -- so if the mark does not say it, nothing does. Two
// takes that meet are two striped bands back to back, and that reads as a
// break in the footage the way two plain lines never did.
//
// room is how much of this recording there is to draw on; a take narrower than
// two bands gets what fits rather than a band over its neighbour.
func srcEdgeMark(cr *cairo.Context, x, dir, top, h, room float64) {
	w := math.Min(srcEdgeW, math.Max(1, room/2))
	cr.Save()
	cr.Rectangle(math.Min(x, x+dir*w), top, w, h)
	cr.Clip()
	// through the stripes, because what is under them is the frame this take
	// begins on and covering it is a worse trade than a fainter mark
	cr.SetSourceRGBA(0.9, 0.7, 0.2, 0.7)
	cr.SetLineWidth(1.5)
	for dy := -w; dy < h; dy += 5 {
		cr.MoveTo(x, top+dy+w)
		cr.LineTo(x+dir*w, top+dy)
		cr.Stroke()
	}
	cr.Restore()
	// and the edge itself solid, so the boundary is still a boundary: the
	// stripes say what happened, this says exactly where
	cr.SetSourceRGB(0.9, 0.7, 0.2)
	cr.SetLineWidth(2)
	cr.MoveTo(x, top)
	cr.LineTo(x, top+h)
	cr.Stroke()
}

// hatchStrokes paints "the footage stops here": dashed yellow diagonals,
// clipped to the band. Marks only and no ground of their own, because the one
// thing they mark is already painted something -- the splice marker is violet
// first, since it says two things at once, and hatching drawn under that
// violet would be tinted by it until it was no longer the same marks.
func hatchStrokes(cr *cairo.Context, x, w, top, h float64) {
	cr.Save()
	defer cr.Restore()
	cr.Rectangle(x, top, w, h)
	cr.Clip()
	cr.SetSourceRGB(0.45, 0.4, 0.3)
	cr.SetLineWidth(1)
	// the stroke runs corner to corner, so its length is the diagonal; a fifth
	// of that, with as much again of gap, is the dash
	cr.SetDash([]float64{h * math.Sqrt2 / 5, h * math.Sqrt2 / 5}, 0)
	for dx := x - h; dx < x+w; dx += 6 {
		cr.MoveTo(dx, top+h)
		cr.LineTo(dx+h, top)
		cr.Stroke()
	}
}

// plateText draws a label on its own dark ground: no single ink is readable
// over thumbnails, and DIFFERENCE fails on mid-grey.
func plateText(cr *cairo.Context, x, y float64, s string) {
	e := cr.TextExtents(s)
	platePath(cr, x-3, y-11, e.Width+6, plateH)
	cr.Fill()
	cr.SetSourceRGB(1, 1, 1)
	cr.MoveTo(x, y)
	cr.ShowText(s)
}

// plateH is how tall a plate is, plateR how round its corners are. Rounded,
// where bands stay square: a band is a MEASUREMENT aimed at by its ends and
// can be three px wide; a plate is a label nobody aims at.
const (
	plateH = 14.0
	plateR = 3.0
)

// platePath lays the plate's outline and sets its ink, ready to be filled. The
// radius gives way on a plate too small to hold it -- a two-pixel label with
// three-pixel corners is a lozenge -- so what is drawn is always a plate.
func platePath(cr *cairo.Context, x, y, w, h float64) {
	cr.SetSourceRGBA(0, 0, 0, 0.66)
	r := math.Min(plateR, math.Min(w, h)/2)
	cr.NewSubPath()
	cr.Arc(x+w-r, y+r, r, -math.Pi/2, 0)
	cr.Arc(x+w-r, y+h-r, r, 0, math.Pi/2)
	cr.Arc(x+r, y+h-r, r, math.Pi/2, math.Pi)
	cr.Arc(x+r, y+r, r, math.Pi, 3*math.Pi/2)
	cr.ClosePath()
}

// drawTrack paints the footage with the kept stretches tinted green. The widget
// is the window onto the timeline, never the timeline (an hour at top zoom is
// wider than a cairo surface can be): everything is in timeline coordinates
// under a Translate, and every loop is culled to the view first.
func (ed *cutEditor) drawTrack(cr *cairo.Context, w, h int) {
	th := float64(ed.thumbHt)
	top := ed.picTop()
	// how deep the whole stack of camera rows is. Everything that is about the
	// CUT and not about one camera is drawn across all of it: one green band
	// down every row, not a separate one per row that could be read as saying
	// the cameras were kept separately.
	bandH := ed.picBottom() - top
	// background
	cr.SetSourceRGB(0.13, 0.13, 0.13)
	cr.Rectangle(0, 0, float64(w), float64(h))
	cr.Fill()

	// what is on screen, in timeline px. The margin is for the things that
	// start left of the edge and reach into view: a thumbnail, a tick's label.
	const margin = 80
	vx0, vx1 := ed.viewX-margin, ed.viewX+float64(w)
	cr.Save()
	cr.Translate(-ed.viewX, 0)
	defer cr.Restore()

	for _, v := range ed.vids {
		if v.pxOrigin > vx1 || v.pxOrigin+v.dur*ed.pps < vx0 {
			continue // this recording is off screen entirely
		}
		lt := ed.laneTop(v.lane)        // this camera's row
		step := v.thumbStep(th, ed.pps) // thumbnails, only the ones in view
		// per CELL, not per recording: a folded gap inside this row's footage
		// is 32 px standing for minutes, so the frames on either side of it
		// are two runs of pictures at two origins, and none at all belongs in
		// the middle (cut_fold.go). With nothing folded this is one cell and
		// one pass, exactly as before.
		for _, cell := range ed.cellsOf(v.start, v.start+v.dur) {
			cv := v
			// where this recording's frame nought WOULD be drawn if the cell
			// carried on backwards: the origin frameRange counts indices from
			cv.pxOrigin = cell.px - (cell.t0-v.start)*ed.pps
			cx0, cx1 := math.Max(vx0, cell.px), math.Min(vx1, cell.px+ed.spanW(cell))
			first, last := cv.frameRange(ed.pps, cx0, cx1, step)
			for i := first; i < last; i += step {
				t := v.sessionAt(float64(i) * v.interval)
				// ready, or nothing: a frame that has not been read yet is asked
				// for and the band's own ground stands in for it until it lands
				// (cut_thumbs.go). Nothing is decoded from inside a draw.
				pic := ed.thumb(v.frames[i])
				if pic == nil || pic.surf == nil {
					continue
				}
				x := ed.xOf(t)
				// never wider than the row it belongs to, nor than the cell:
				// a lane's last thumbnail is a whole frame's worth of a file
				// the row stops partway through, and the last one before a
				// fold must not spill across the seam
				w := math.Min(pic.w, float64(step)*v.interval*ed.pps)
				w = math.Min(w, ed.xOf(v.start+v.dur)-x)
				w = math.Min(w, cell.px+ed.spanW(cell)-x)
				if w <= 0 {
					continue
				}
				cr.SetSourceSurface(pic.surf, x, lt+2)
				cr.Rectangle(x, lt+2, w, th)
				cr.Fill()
			}
		}

		// where this recording begins and ends, drawn just INSIDE its own
		// pictures rather than between them (srcEdgeMark).
		//
		// Between them the mark is about the GAP, and the gap is the one thing
		// here that is not footage: two takes with a stretch nobody filmed in
		// between came out as a band of hatching with a line down each side of
		// it -- the emptiest part of the page wearing the loudest mark on it.
		// So unfilmed time is laid out at no width at all and the borders are
		// on the pictures: two takes that meet are two bordered pictures
		// touching, and what shows is the border, not the space.
		//
		// Its name is not a place on the tape at all: that is pinned, below.
		x0, x1 := v.pxOrigin+srcEdgeIn, ed.xOf(v.start+v.dur)-srcEdgeIn
		srcEdgeMark(cr, x0, 1, lt, ed.laneH(), x1-x0)
		// ...and the far end only where the two marks would be the same two
		// pixels: a recording drawn narrower than its own borders is one
		// border, and drawing it twice only thickens it
		if x1-x0 > 3*srcEdgeIn {
			srcEdgeMark(cr, x1, -1, lt, ed.laneH(), x1-x0)
		}

		if au := ed.pairAud(v.base); au != nil {
			// and the row's own sound directly under its pictures, edge to
			// edge: the pair is one piece of footage seen twice
			ed.drawPairStrip(cr, v, *au, lt+ed.laneH(), vx0, vx1)
		}
	}

	// The rows' names, pinned where the recorders' band pins its own (laneNameX)
	// rather than scrolling with the tape. Each names the recording under the
	// view's left edge -- over a gap, the first one in view.
	cr.SetFontSize(10)
	for r := 0; r < max(1, ed.laneN); r++ {
		v := ed.rowNameVid(r, vx0, vx1)
		if v == nil {
			continue
		}
		name := v.base
		if v.off > 0 {
			// which seconds of the file this row is showing. A lane cut from a
			// recording is that recording's name over again, and this is the
			// only thing on the page that says which part of it (cut_lane.go)
			name += fmt.Sprintf(" from %s", mmss(v.off))
		}
		if d := ed.shiftOf(v.base); d != 0 {
			// a camera moved by hand looks exactly like a camera whose file
			// says it started there, and the difference is the whole of what
			// the right button did (cut_shift.go)
			name += fmt.Sprintf(" %+.2f s", d)
		}
		plateText(cr, ed.viewX+laneNameX, ed.laneTop(r)+12, name)
	}

	// ruler
	stepS := tickStep(ed.pps)
	cr.SetFontSize(9)
	for _, sp := range ed.spans {
		if sp.fold {
			continue // a whole gap in 32 px: every tick in it lands on the same pixel
		}
		if sp.px > vx1 || sp.px+sp.dur()*ed.pps < vx0 {
			continue
		}
		from := math.Max(sp.t0, sp.t0+(vx0-sp.px)/ed.pps)
		to := math.Min(sp.t1, sp.t0+(vx1-sp.px)/ed.pps)
		t0 := math.Ceil(from/stepS) * stepS
		for t := t0; t < to; t += stepS {
			x := ed.xOf(t)
			cr.SetSourceRGB(0.6, 0.6, 0.6)
			cr.MoveTo(x, float64(rulerH))
			cr.LineTo(x, float64(rulerH)-5)
			cr.Stroke()
			cr.MoveTo(x+2, float64(rulerH)-7)
			cr.ShowText(fmt.Sprintf("%d:%02d", int(t)/60, int(t)%60))
		}
	}

	// in/out markers: solid lines with flag triangles, same visual weight as
	// the yellow file boundaries so they are actually findable
	if ed.hasIn {
		x := ed.xOf(ed.markIn)
		cr.SetSourceRGB(0.15, 0.85, 0.25)
		cr.SetLineWidth(3)
		cr.MoveTo(x, top)
		cr.LineTo(x, top+bandH)
		cr.Stroke()
		cr.MoveTo(x, top)
		cr.LineTo(x+9, top)
		cr.LineTo(x, top+9)
		cr.ClosePath()
		cr.Fill()
	}
	if ed.hasOut {
		x := ed.xOf(ed.markOut)
		cr.SetSourceRGB(0.92, 0.12, 0.12)
		cr.SetLineWidth(3)
		cr.MoveTo(x, top)
		cr.LineTo(x, top+bandH)
		cr.Stroke()
		cr.MoveTo(x, top)
		cr.LineTo(x-9, top)
		cr.LineTo(x, top+9)
		cr.ClosePath()
		cr.Fill()
	}

	// the state overlay: everything the cut keeps, tinted green. What is left
	// untinted is what the cut drops -- which is the whole of what the second
	// band used to say, said here against the footage it refers to.
	for _, s := range ed.segs {
		if s.isInsert() && !(s.audioIns() && !s.spliced()) {
			// violet, below: green means "this footage is kept". The one insert
			// that keeps its footage is a sound laid over a selection -- the
			// picture runs on under it -- so the tint stays for that one.
			continue
		}
		x0, x1 := ed.xOf(s.S), ed.xOf(s.E)
		if x1 < vx0 || x0 > vx1 {
			continue
		}
		// on the scene's OWN row and no other. The green is what will be shown,
		// and with two cameras up that is a claim about one of them -- a tint
		// down both rows would say the finished video shows two pictures at
		// once. One camera, one row, and this is the whole band again.
		st, lh := ed.segTop(s), ed.laneH()
		cr.SetSourceRGBA(0.2, 0.8, 0.3, 0.30)
		cr.Rectangle(x0, st, x1-x0, lh)
		cr.Fill()
		// ...and over the row's OWN SOUND on the strip under its pictures: a kept
		// scene takes the sound filmed with it, so the strip says "kept" too.
		// Fainter than over the thumbnails (as drawAudio is): a waveform IS the
		// reading, and a heavy wash takes it with it.
		ph := ed.pairH(ed.segRow(s))
		if ph > 0 {
			cr.SetSourceRGBA(0.2, 0.8, 0.3, 0.16)
			cr.Rectangle(x0, st+lh, x1-x0, ph)
			cr.Fill()
		}
		// hard green edges, boundary-marker style, down the pictures and the
		// strip together -- the cut's borders run through the sound as they do
		// in the band below
		cr.SetSourceRGB(0.15, 0.85, 0.25)
		cr.SetLineWidth(2)
		for _, x := range []float64{x0, x1} {
			cr.MoveTo(x, st)
			cr.LineTo(x, st+lh+ph)
			cr.Stroke()
		}
	}

	// While the preview is the cut (▶✂) the dropped stretches are dimmed rather
	// than merely left
	// untinted. In that mode they are not "footage the cut does not keep", they
	// are the seconds ▶ is about to jump over -- and a mode that changes what
	// plays has to be visible on the band that says what plays. The inserts
	// below draw over this, which is right: they are kept.
	if ed.cutOnly {
		cr.SetSourceRGBA(0.04, 0.04, 0.05, 0.62)
		for _, g := range ed.droppedSpans() {
			x0, x1 := ed.xOf(g[0]), ed.xOf(g[1])
			if x1 < vx0 || x0 > vx1 {
				continue
			}
			cr.Rectangle(x0, top, x1-x0, bandH)
		}
		cr.Fill()
	}

	// inserts, over the overlay. Violet rather than a
	// shade of the green: an insert is not footage that was kept, it is footage
	// that is not there, and the two must not be told apart by brightness. The
	// file's name is written into the band because that is the only thing on
	// this page that says WHICH card is at 12:30 -- there is no thumbnail under
	// it to recognize, the track behind it is whatever the insert covered.
	for _, s := range ed.segs {
		if !s.isInsert() || s.audioIns() {
			// a sound-only insert is marked in the audio lanes, where the thing
			// it changes lives; the picture band it leaves alone
			continue
		}
		// A spliced card costs the footage nothing, so it owns no session time
		// and has no span of the timeline to be drawn across. What it has is a
		// POINT where the footage is cut open and a length of its own, and
		// spliceSpan is that length drawn at this zoom, STARTING at the point --
		// the card begins where the red line was when it was placed, and the
		// marker grows rightward with the zoom the way the card runs.
		x0, x1 := ed.segSpan(s)
		if x1 < vx0 || x0 > vx1 {
			continue
		}
		cr.SetSourceRGBA(0.55, 0.35, 0.9, 0.55)
		cr.Rectangle(x0, top, x1-x0, bandH)
		cr.Fill()
		if s.spliced() {
			// hatched over the violet, so the marker says both things at once: a
			// card goes in here, and the footage stops for it
			hatchStrokes(cr, x0, x1-x0, top, bandH)
		}
		cr.SetSourceRGB(0.75, 0.6, 1)
		cr.SetLineWidth(2)
		for _, x := range []float64{x0, x1} {
			cr.MoveTo(x, top)
			cr.LineTo(x, top+bandH)
			cr.Stroke()
		}
		cr.SetFontSize(10)
		switch {
		case s.spliced():
			// The length comes with the name here and nowhere else: a spliced card
			// does not stretch between two borders you can read off the ruler.
			// Beside the marker while the marker is a marker, inside it once the
			// zoom has made it wide enough to write in -- text that starts at the
			// same place either way, so it does not appear to jump as you zoom.
			tx := x1 + 4
			if x1-x0 > 90 {
				tx = x0 + 4
			}
			markPlate(cr, tx, top+th-2, "card", fmt.Sprintf("%s  %.1fs", insName(s), s.Dur))
		case x1-x0 > 24:
			// the file, not the parameters: a filled-in tier board's parameters
			// are longer than the clip they would be written across
			markPlate(cr, x0+4, top+th-2, "card", insName(s))
		}
	}

	// A sound-only insert is marked on the SOUND: the recorders' band below
	// when the session has one (drawAudio, which also says why the picture
	// band leaves these alone), and every row's wave strip here always --
	// with the cameras' waves paired under their pictures, a session with no
	// separate recorder has no other band to say "these seconds were placed".
	heldSnd := ed.heldSeg()
	for i := range ed.segs {
		s := ed.segs[i]
		if !s.audioIns() {
			continue
		}
		x0, x1 := ed.segSpan(s)
		if x1 < vx0 || x0 > vx1 {
			continue
		}
		named := false
		for r := 0; r < max(1, ed.laneN); r++ {
			ph := ed.pairH(r)
			if ph <= 0 {
				continue
			}
			// named once, on the first strip that can carry it: the same
			// mark on three rows saying the same file three times is noise
			ed.sndInsMark(cr, s, x0, x1, ed.laneTop(r)+ed.laneH(), ph, heldSnd == &ed.segs[i], !named)
			named = true
		}
	}

	// slowed or frozen stretches, tinted rose over the footage itself: the
	// effect belongs to the frames under it, and the lane below carries its
	// handle. The same hue as its lane marker, so the two read as one thing.
	for _, f := range ed.fx {
		if f.Kind != "speed" {
			continue
		}
		x0, x1 := ed.fxSpanPx(f)
		if x1 < vx0 || x0 > vx1 {
			continue
		}
		cr.SetSourceRGBA(0.92, 0.42, 0.6, 0.18)
		cr.Rectangle(x0, top, x1-x0, bandH)
		cr.Fill()
	}

	// What each scene does with the sound on its own camera, on the strips under
	// the pictures -- and which row it is shown from, on the rows (drawCamBadges).
	// After the green, not before: "silences its camera" has to be the answer
	// you see over "is in the video".
	ed.drawSilences(cr, ed.pairSilences(), vx0, vx1)
	ed.drawCamBadges(cr, vx0, vx1)
	ed.drawHearBadges(cr, ed.hearBadgesSrc(), vx0, vx1)
	// the ✕ that takes a whole row away, over everything else the picture band
	// draws: a control the inserts or the cut preview's dimming could paint
	// over would be a control that is there on some frames and not others.
	// The one that drops a scene is not here -- it is on the green bar in the
	// selection row (drawSelBand), which is drawn just below.
	ed.drawLaneKill(cr, vx0, vx1)
	// the black strip in front of second zero, drawn over the picture band and
	// under the two rows below -- nothing on the tape may reach into it, and
	// the switches above stand in it (cut_gutter.go). The bands get their own
	// pass because they are drawn after this one.
	ed.drawGutter(cr, top, bandH)
	ed.drawPairSwitches(cr)
	ed.drawRowKill(cr)

	// the two rows that speak for the whole cut, over the pictures rather than
	// under them now: the green bar under the clock and the effects lane under
	// it (cut_fx.go). Drawn after the band so the hairline that closes the
	// group lands on the pictures' own top edge rather than beneath it.
	ed.drawSelBand(cr, vx0, vx1)
	ed.drawFoldBadges(cr, vx0, vx1)
	ed.drawFxLane(cr, vx0, vx1)
	ed.drawFxKill(cr, vx0, vx1)
	// the same strip across the two bands, and the one control that belongs to
	// the whole page rather than to a row: fold every dropped stretch away, or
	// bring them all back (cut_gutter.go)
	ed.drawGutter(cr, float64(rulerH), ed.picTop()-float64(rulerH))
	ed.drawFoldAll(cr)

	// the clip a double click has picked up, outlined in white. The edge marker
	// below says which BORDER is about to move; this says which whole clip is,
	// and they are the same gesture at two scales, so they are the same ink.
	if s := ed.heldSeg(); s != nil && !s.audioIns() {
		// ...unless the held clip is a sound: its marker is in the lanes, and
		// the outline goes where the marker is (drawAudio)
		x0, x1 := ed.segSpan(*s)
		st := ed.segTop(*s)
		cr.SetSourceRGBA(1, 1, 1, 0.9)
		cr.SetLineWidth(2)
		cr.Rectangle(x0+1, st+1, x1-x0-2, ed.laneH()-2)
		cr.Stroke()
	}

	// selection rubber band
	if ed.sel.active {
		a, b := ed.sel.t0, ed.sel.t1
		if b < a {
			a, b = b, a
		}
		x0, x1 := ed.xOf(a), ed.xOf(b)
		// on the row it was drawn on: the blue says which picture ＋ Add is
		// about to keep, so it has to be over that picture
		cr.SetSourceRGBA(0.3, 0.55, 0.9, 0.45)
		lt, lh := ed.laneTop(ed.sel.lane), ed.laneH()
		cr.Rectangle(x0, lt, x1-x0, lh)
		cr.Fill()
		// ...and its two ENDS drawn as ends, with a grip at mid-height: a
		// press on either takes that end from here exactly as it does from
		// the band, and a hand cannot aim at an edge that is only where a
		// wash stops. Lit while the pointer is on one (selHov), as the band
		// lights its own grips.
		cr.SetSourceRGBA(0.45, 0.7, 1, 0.95)
		cr.SetLineWidth(2)
		if ed.selHov {
			cr.SetLineWidth(3)
		}
		for _, x := range []float64{x0, x1} {
			cr.MoveTo(x, lt)
			cr.LineTo(x, lt+lh)
			cr.Stroke()
			cr.Rectangle(x-3, lt+lh/2-7, 6, 14)
			cr.Fill()
		}
	}

	// which sound is in hand, said on the wave itself: a selection drawn on a
	// row's own strip wears its second wash there, exactly as one drawn in a
	// separate recorder's lane wears it in the band below (drawAudio)
	if ed.sel.active && ed.selSnd() {
		for _, v := range ed.vids {
			if v.base != ed.sel.aud {
				continue
			}
			if au := ed.pairAud(v.base); au != nil {
				x0, x1 := ed.selSpanPx()
				sy, sh := ed.laneTop(v.lane)+ed.laneH(), float64(ed.lanes(*au))*waveLaneH
				cr.SetSourceRGBA(0.3, 0.55, 0.9, 0.34)
				cr.Rectangle(x0, sy, x1-x0, sh)
				cr.Fill()
				cr.SetSourceRGB(0.62, 0.82, 1)
				for _, x := range []float64{x0, x1} {
					cr.Rectangle(x-1.5, sy, 3, sh)
				}
				cr.Fill()
			}
		}
	}

	// the row the preview is watching (camAt), outlined across the pair --
	// the strip is the same footage. Dashed, so it cannot be read as the held
	// clip's solid white; gone the moment ▶ takes the preview back to the cut.
	if m := ed.watchRow(); m >= 0 {
		lt := ed.laneTop(m)
		cr.SetSourceRGBA(0.95, 0.95, 1, 0.6)
		cr.SetLineWidth(1.5)
		cr.SetDash([]float64{5, 4}, 0)
		cr.Rectangle(ed.viewX+1, lt+0.75, float64(w)-2, ed.laneH()+ed.pairH(m)-1.5)
		cr.Stroke()
		cr.SetDash(nil, 0)
	}

	// the border under the pointer: a soft white bar with a halo behind it, over
	// the green one it belongs to. Not the held marker's ink -- no heads, and
	// half the strength -- because it is an offer rather than a state: this is
	// the border the next press would take, and it goes away when the pointer
	// does. See hoverEdge.
	if ed.edgeHovOn {
		x := ed.xOf(ed.edgeHovT)
		cr.SetSourceRGBA(1, 1, 1, 0.16)
		cr.SetLineWidth(9)
		cr.MoveTo(x, top)
		cr.LineTo(x, top+bandH)
		cr.Stroke()
		cr.SetSourceRGBA(1, 1, 1, 0.6)
		cr.SetLineWidth(3)
		cr.MoveTo(x, top)
		cr.LineTo(x, top+bandH)
		cr.Stroke()
	}

	// the clip edge that is held: white, wider than the green border it sits on,
	// with a head each way to say that it moves. Drawn last, so nothing painted
	// over it can hide what is about to change under the next ‹f.
	if ed.edgeOn && ed.edgeSeg < len(ed.segs) {
		x := ed.xOf(ed.edgeTime())
		st, lh := ed.segTop(ed.segs[ed.edgeSeg]), ed.laneH()
		cr.SetSourceRGB(1, 1, 1)
		cr.SetLineWidth(3)
		cr.MoveTo(x, st)
		cr.LineTo(x, st+lh)
		cr.Stroke()
		for _, d := range []float64{-1, 1} {
			cr.MoveTo(x, st+lh/2-5)
			cr.LineTo(x+7*d, st+lh/2)
			cr.LineTo(x, st+lh/2+5)
			cr.ClosePath()
			cr.Fill()
		}
	}
}

func (ed *cutEditor) inCut(t float64) bool {
	for _, s := range ed.segs {
		if t >= s.S && t < s.E {
			return true
		}
	}
	return false
}

func tickStep(pps float64) float64 {
	for _, s := range []float64{1, 2, 5, 10, 30, 60, 120, 300, 600} {
		if s*pps >= 70 {
			return s
		}
	}
	return 1200
}

// The run bar drives the preview through these; see transport in pipeline.go.
func (ed *cutEditor) playing() bool { return ed.player != nil && ed.player.Playing() }
func (ed *cutEditor) cued() bool    { return ed.player != nil && ed.player.Cued() }

// playAs is both play buttons. Each one is ▶ for its own idea of what the
// preview IS -- ▶ the recording, ▶✂ the cut -- so each press delivers exactly
// what the face it landed on promises. Pressing the button whose preview is
// already running pauses it; pressing the other one switches the preview over,
// and if something was playing it plays on as the other thing rather than
// stopping -- switching is why you pressed a play button and not ⏸.
func (ed *cutEditor) playAs(cut bool) {
	if ed.cutOnly != cut {
		ed.setCutOnly(cut)
		if ed.playing() {
			return
		}
	}
	ed.toggle()
}

// setCutOnly switches what the preview IS, with everything that follows from
// that: the gap-skip guard, the dimming, the clock's meaning and the ▶✂ lamp.
// The two play buttons are the only hands on it.
func (ed *cutEditor) setCutOnly(cut bool) {
	if ed.cutOnly == cut {
		return
	}
	ed.cutOnly = cut
	ed.jumped = -1
	ed.syncPlayBtn() // an empty cut is nothing to play; see that function
	if cut {
		// the mode promises kept material, so it delivers some immediately
		// rather than at the next tick
		ed.cutOnlySnap()
		if len(ed.segs) == 0 {
			ed.a.setStatus("preview is the cut — and the cut is empty, so ▶✂ has " +
				"nothing to play until a clip is added")
		} else {
			ed.a.setStatus("preview is the cut — the clock reads the finished video")
		}
	} else {
		ed.a.setStatus("preview is the recording again — everything plays, cuts and all")
	}
	ed.showTime()
	ed.a.syncPlayIcons() // both ▶s redraw: whose preview this is just changed
	ed.redrawTracks()
}

// syncCutPlay draws the ▶✂ button: a pause face while its preview is the one
// running, and lit for as long as the preview is the cut at all -- the lamp
// the old toggle button's pressed state used to be.
//
// The play/pause half is the stock ICON, and the ✂ beside it is the label. It
// was one label of two glyphs, "▶✂" and "⏸✂", because no stock icon says
// "play, but the cut" -- and swapping those two glyphs moved the whole page.
// ⏸ (U+23F8) is in a different font from ▶ (U+25B6) on an ordinary Linux
// desktop: Pango falls back to the emoji face for it, which is taller, so the
// button grew a few px on every press, the toolbar row grew with it, and the
// timeline under it stepped down and back up as the preview started and
// stopped. An icon is the same size in both states, and the ✂ never changes.
func (ed *cutEditor) syncCutPlay() {
	if ed.cutPlayBtn == nil || ed.cutPlayIcon == nil {
		return
	}
	if ed.playing() && ed.cutOnly {
		ed.cutPlayIcon.SetFromIconName("media-playback-pause-symbolic")
		ed.cutPlayBtn.SetTooltipText("pause the cut preview")
	} else {
		ed.cutPlayIcon.SetFromIconName("media-playback-start-symbolic")
		ed.cutPlayBtn.SetTooltipText("play the CUT instead of the recording: the removed " +
			"stretches are skipped, so this runs the finished video. The clock reads the " +
			"cut's own time while it does. Changes nothing that is saved.")
	}
	if ed.cutOnly {
		ed.cutPlayBtn.AddCSSClass("suggested-action")
	} else {
		ed.cutPlayBtn.RemoveCSSClass("suggested-action")
	}
}

func (ed *cutEditor) toggle() {
	// ▶ is grey in this state (syncPlayBtn), but the button is not the only way
	// in -- a click on the picture and the run bar land here too, and every way
	// in has to refuse for the same reason
	if ed.cutOnly && len(ed.segs) == 0 {
		ed.a.setStatus("the cut is empty — add a clip to play it, or press ▶ " +
			"to play the recording instead")
		return
	}
	if ed.player == nil {
		return
	}
	if !ed.playing() {
		// ▶ plays the CUT: a preview that was watching one row on a click's
		// orders stops watching it. The scenes name their own cameras from
		// here (camAt), and the branches below must load the scene's file,
		// not the watched one's.
		ed.monRow = 0
		ed.redrawTracks() // the dashed outline goes with it
	}
	// ▶ starts where the red line is, both buttons, whatever is in hand. Under
	// ✂ only, a line in a dropped stretch moves to the next clip (cutOnlySnap).
	if !ed.playing() && ed.cutOnly {
		ed.cutOnlySnap()
	}
	// what this scene hears, settled before the transport moves rather than in
	// the showInsert below it: ▶ starts the recordings (syncMix), and a lane
	// this scene silences is one ▶ must not start at all
	ed.syncHush()
	ed.player.Toggle()
	// the black "no footage on this row" frame comes and goes with the
	// standstill (showInsert), and pausing is a standstill nothing else
	// re-settles: playback's own ticks stop with it
	ed.showInsert()
	ed.started = ed.started || ed.player.Playing()
	ed.a.updateRunControls()
}

func (ed *cutEditor) stop() {
	if ed.player != nil {
		ed.player.Stop()
	}
	ed.started = false // ⏹ hands ▶ back to the step's own job, suggesting
}

// ---- page ------------------------------------------------------------------

func (a *App) buildCut() gtk.Widgetter {
	ed := &cutEditor{a: a, pps: 4, thumbHt: 64, jumped: -1, rowHov: -1, fxKillHov: -1,
		bandKillHov: -1, foldHov: -1, thumbs: map[string]*thumbPic{}}
	a.ed = ed
	if p, err := NewPlayer(); err == nil {
		ed.player = p // the preview above the tracks; independent of Review's
		p.OnState = a.updateRunControls
		p.OnError = a.playerErr("the cut preview")
		p.OnLog = func(s string) { a.logf("%s", s) }
		glib.TimeoutAdd(playTick, ed.followPlayback)
	} else {
		a.logf("cut preview player: %v", err)
	}

	// Suggesting is this page's long job, and every other page's long job is
	// the run bar's ▶. There is nothing here for it any more: the length it
	// aims at is a sentence in the user context on Prepare ("about 12 min"),
	// beside everything else the run is told.
	// Not the blue one. suggested-action is the page's primary verb, and this
	// is not it -- ▶ on the run bar is -- so the colour was saying "press this
	// first" about a button that cannot be pressed at all until seconds are
	// marked. The bar is a row of equal verbs, and which of them is live says
	// what to do next (syncSelBtns).
	// Icons, like the transport and the undo group either side of them. Five
	// words on five buttons is a third of the bar spent saying what a symbol
	// says -- and these five are the page's own verbs, pressed all session,
	// which is exactly the set a hand learns by position. What each one is
	// stays in the tooltip and in the status line the press writes.
	ed.addBtn = gtk.NewButtonFromIconName("list-add-symbolic")
	add := ed.addBtn
	add.ConnectClicked(func() { a.addSelClicked() })
	// the same selection, cut out of what it lies in rather than kept or
	// dropped: the third thing that can be done to a span (cut_split.go).
	ed.splitBtn = gtk.NewButtonFromIconName("edit-cut-symbolic")
	ed.splitBtn.ConnectClicked(func() { a.splitSelRange() })
	// the same selection, dropped instead of kept. Beside Add because they are
	// one pair, and greyed by the same rule -- see cut_selrm.go for why a
	// remove is back on the bar at all.
	ed.remBtn = gtk.NewButtonFromIconName("list-remove-symbolic")
	ed.remBtn.ConnectClicked(func() { a.removeSelRange() })
	// Copy takes the selected seconds in hand rather than acting on the cut:
	// while a copy is held, Insert reads ⧉ Paste and splices those seconds in
	// again at the red line. Greyed until there is a selection, because a copy
	// IS the selection taken in hand, and with nothing selected the press
	// could only explain itself.
	ed.copyBtn = gtk.NewButtonFromIconName("edit-copy-symbolic")
	ed.copyBtn.ConnectClicked(func() { a.copyClicked() })
	ed.syncSelBtns()
	// One button, two jobs, because they are the same job seen from either end:
	// with nothing held it puts a card in, and with a card held it opens that
	// card. A second button that is greyed out unless you happen to be holding an
	// insert would say the same thing and take up the bar saying it.
	// Paste is its own button, greyed when there is nothing in hand.
	//
	// It used to BE the Insert button, relabelled while a copy was held: one
	// press meaning "choose a file" or "put the copy down" depending on a
	// state nothing on the bar showed. That was a trade for width, and the
	// width is not the price any more -- these are icons. A button that is
	// there and grey says both things at once: this is where a copy goes, and
	// you are not holding one.
	ed.pasteBtn = gtk.NewButtonFromIconName("edit-paste-symbolic")
	ed.pasteBtn.SetSensitive(false)
	ed.pasteBtn.ConnectClicked(func() { a.pasteCopy() })

	ed.insBtn = gtk.NewButtonFromIconName("insert-object-symbolic")
	ins := ed.insBtn
	ins.ConnectClicked(func() { a.insertClicked() })
	// the second thing a copy of footage can be. Paste puts it back into the
	// cut in sequence; this puts it on a row of its own, beside the cameras, so
	// the green can choose between the two. It comes and goes with the copy --
	// there is nothing it could mean with nothing in hand.
	// down, into a row of its own: not the ＋ two buttons along, which keeps
	// footage in the cut. Two identical icons in one group is one icon
	ed.laneBtn = gtk.NewButtonFromIconName("go-down-symbolic")
	ed.laneBtn.ConnectClicked(func() { a.pasteLane() })
	ed.syncInsertBtn() // its label and tooltip depend on what is held
	// Undo and Revert are icons, not words. They were the two widest buttons in
	// the bar and they are both the kind of control you reach for by shape --
	// Undo has a keyboard shortcut people already know, and Revert is a rare
	// deliberate act, not something scanned for. The glyphs they used to carry
	// (↶ and ↺) were nearly the same picture; the theme's undo arrow and
	// revert-to-saved icon are not.
	ed.revertBtn = gtk.NewButtonFromIconName("document-revert-symbolic")
	ed.revertBtn.SetTooltipText("Revert edits — drop everything you added or removed by hand and go back to " +
		"the last suggestion — or, if you have not suggested yet, to the cut this page opened with")
	ed.revertBtn.SetSensitive(false)
	ed.revertBtn.ConnectClicked(func() { a.revertClicked() })
	// Clear, beside Revert because they are the same kind of verb -- throw work
	// away -- and different about which work: Revert goes back to the last
	// suggestion, this one goes back to nothing at all. One Undo brings it
	// back, which is what makes it a button rather than a question.
	ed.clearBtn = gtk.NewButtonFromIconName("edit-clear-all-symbolic")
	ed.clearBtn.SetTooltipText("Clear: take every kept stretch and every effect off the " +
		"timeline, leaving the recordings as they were loaded (↶ Undo brings them back)")
	ed.clearBtn.ConnectClicked(func() { ed.clearCut() })
	ed.undoBtn = gtk.NewButtonFromIconName("edit-undo-symbolic")
	ed.undoBtn.SetTooltipText("Undo — take back the last Add, Remove or Suggest (Ctrl+Z)")
	ed.undoBtn.SetSensitive(false)
	ed.undoBtn.ConnectClicked(func() { ed.undoLast() })
	ed.redoBtn = gtk.NewButtonFromIconName("edit-redo-symbolic")
	ed.redoBtn.SetTooltipText("Redo — put back what Undo took (Ctrl+Shift+Z)")
	ed.redoBtn.SetSensitive(false)
	ed.redoBtn.ConnectClicked(func() { ed.redoLast() })
	// The playhead's time, printed. Monospaced ("numeric") so the digits do
	// not dance under ‹f/f› -- a readout that reflows on every frame is one
	// you cannot read while stepping. In the quiet column with the page's
	// other readings (cut_form.go): it was small print under the transport,
	// which is a caption on a control rather than a number to read.
	ed.clock = gtk.NewLabel("")
	ed.clock.AddCSSClass("numeric")
	ed.clock.SetWidthChars(8) // "--:--.-" and "59:59.9" both fit; the column never twitches
	ed.clock.SetXAlign(0)
	ed.showTime() // opens as "--:--.-", not as a blank gap in the bar

	// What the cut comes to. It was the small print under the view buttons at
	// the right end of the toolbar, where it was the first thing the bar cut
	// off -- "cut 6:39 · source 28:15 · 9 segm…" -- and it is not a control:
	// it is read between edits, like the panel it now lives in.
	ed.total, ed.totalRaw = idleRead(), idleRead()
	ed.totalSrc, ed.totalSegs = idleRead(), idleRead()

	// Two pairs that both step something up and down, so they must not look
	// alike: one zooms the timeline, the other sizes the thumbnails drawn on it.
	// They used to be a bare +/− and a pair of magnifiers, which is backwards --
	// a magnifier IS the zoom icon, and the thing being made bigger in the other
	// pair is a picture. So: the theme's zoom icons for the timeline, and a
	// picture with a sign for the thumbnails. Different nouns, not two spellings
	// of the same one.
	sized := func(sign, tip string, click func()) *gtk.Button {
		row := gtk.NewBox(gtk.OrientationHorizontal, 1)
		row.Append(gtk.NewImageFromIconName("image-x-generic-symbolic"))
		row.Append(gtk.NewLabel(sign))
		b := gtk.NewButton()
		b.SetChild(row)
		b.SetTooltipText(tip)
		b.ConnectClicked(click)
		return b
	}
	thumbMinus := sized("−", "smaller thumbnails on the tracks", func() { ed.setThumbH(ed.thumbHt * 3 / 4) })
	thumbPlus := sized("+", "larger thumbnails on the tracks", func() { ed.setThumbH(ed.thumbHt * 4 / 3) })

	zoomOut := gtk.NewButtonFromIconName("zoom-out-symbolic")
	zoomOut.SetTooltipText("zoom the timeline out — it stops where the whole session is on screen " +
		"(the scroll wheel does the same, around the cursor)")
	zoomOut.ConnectClicked(func() { ed.zoomStep(1 / 1.25) })
	zoomIn := gtk.NewButtonFromIconName("zoom-in-symbolic")
	zoomIn.SetTooltipText("zoom the timeline in, around the middle of what is on screen " +
		"(the scroll wheel does the same, around the cursor)")
	zoomIn.ConnectClicked(func() { ed.zoomStep(1.25) })

	// The camera and the clock (cut_fx.go). The dropdown is the shape of the
	// finished video; the three buttons put effects in. They sit with the
	// editing controls because that is what they are -- each one changes what
	// Produce makes, is saved in cut.json, and answers to Undo.
	ed.aspectDD = gtk.NewDropDownFromStrings(fxAspects)
	ed.aspectDD.SetTooltipText("the shape of the finished video — source is the footage's own, " +
		"9:16 is a vertical short. The whole frame fits inside it (bars either side) until " +
		"▭ View frames a region; the outline on the preview is what the finished video shows")
	ed.aspectDD.NotifyProperty("selected", func() {
		if ed.aspectMu {
			return // set by code (reload, undo), not a choice being made
		}
		if i := int(ed.aspectDD.Selected()); i >= 0 && i < len(fxAspects) {
			ed.aspectChanged(fxAspects[i])
		}
	})
	// The effects behind one dropdown -- a menu of verbs, not a state:
	// picking one fires it and the control snaps back to its label, so the
	// notify below re-enters once with 0 and leaves.
	fxKinds := []string{"✚ Effect", "⊕ Zoom", "❝ Text", "▨ SVG", "⏩ Speed", "🔊 Volume", "🏷 Label"}
	fxDD := gtk.NewDropDownFromStrings(fxKinds)
	fxDD.SetTooltipText("put an effect in: ⊕ Zoom frames what the video shows — drag a box on " +
		"the preview and say how long; when its seconds are up the camera either pulls back " +
		"to where it was or stays on the region, which is how a widescreen recording is turned " +
		"into a vertical short. ❝ Text writes words over the picture for a few seconds in " +
		"a box you draw, ▨ SVG lays a drawing of yours over it the same way — it does not cut " +
		"the video, an insert is the one that does. ⏩ Speed puts a stretch on a clock of its " +
		"own — slowed to a quarter, " +
		"up to 100× to run through dead air, or ×0 to stop the picture on one frame while the " +
		"footage runs on underneath. On the preview " +
		"the box under the pointer can be dragged and its border resized; click its mark in " +
		"the lane below the track for its numbers. 🔊 Volume is the one that changes nothing " +
		"you can see: the seconds it covers are played louder or quieter than they were " +
		"recorded, anywhere from silent to ten times, which is how a passage nobody can " +
		"hear is rescued without touching the rest. 🏷 Label is the one that changes " +
		"nothing at all: it names a moment in your own words, and that name goes into the " +
		"brief the narration writer is given — so your notes on Prepare can say what to do " +
		"when it arrives")
	fxDD.NotifyProperty("selected", func() {
		i := int(fxDD.Selected())
		if i <= 0 {
			return
		}
		fxDD.SetSelected(0)
		switch i {
		case 1:
			ed.armFx("zoom")
		case 2:
			ed.armFx("text")
		case 3:
			a.svgClicked()
		case 4:
			a.speedClicked()
		case 5:
			a.volumeClicked()
		case 6:
			a.labelClicked()
		}
	})

	// one button that is ▶ or ⏸ depending on the preview, like every other play
	// button in the app (syncPlayIcons in pipeline.go keeps it drawn)
	ed.playBtn = gtk.NewButtonFromIconName("media-playback-start-symbolic")
	ed.playBtn.SetTooltipText("play or pause the preview at the playhead")
	ed.playBtn.ConnectClicked(func() { ed.playAs(false) })
	// with something held -- a clip edge, a whole clip, an effect (click its
	// mark) -- these move that instead of the playhead. Said on every one of
	// them, because that is the state you are in when you look at them.
	prev5 := gtk.NewButtonWithLabel("‹‹f")
	prev5.SetTooltipText("back 5 frames (pauses) — or whatever is held, 5 frames")
	prev5.ConnectClicked(func() { ed.frameStep(-5) })
	prevF := gtk.NewButtonWithLabel("‹f")
	prevF.SetTooltipText("previous frame (pauses) — or whatever is held, one frame")
	prevF.ConnectClicked(func() { ed.frameStep(-1) })
	nextF := gtk.NewButtonWithLabel("f›")
	nextF.SetTooltipText("next frame (pauses) — or whatever is held, one frame")
	nextF.ConnectClicked(func() { ed.frameStep(+1) })
	next5 := gtk.NewButtonWithLabel("f››")
	next5.SetTooltipText("forward 5 frames (pauses) — or whatever is held, 5 frames")
	next5.ConnectClicked(func() { ed.frameStep(+5) })
	// The second ▶. There used to be a "✂ Cut only" toggle here: a MODE, which
	// ▶ then obeyed -- so playing the cut was two buttons in the right order,
	// and a ▶ that sometimes played the recording and sometimes the cut. Two
	// play buttons say it in one press each: ▶ the recording, every second of
	// it, which is what you want while deciding where the cuts go; ▶✂ the cut
	// -- gaps jumped, effects on, the clock on the cut's own time, what
	// Produce will make. Whichever ran last still colors the page (dimming,
	// clock), and the ▶✂ face stays lit while the preview is the cut.
	// Nothing about it is saved.
	//
	// Its face is the stock play/pause icon with a ✂ beside it, rather than a
	// label of two glyphs: see syncCutPlay for what the label cost.
	ed.cutPlayIcon = gtk.NewImageFromIconName("media-playback-start-symbolic")
	face := gtk.NewBox(gtk.OrientationHorizontal, 2)
	face.Append(ed.cutPlayIcon)
	face.Append(gtk.NewLabel("✂"))
	ed.cutPlayBtn = gtk.NewButton()
	ed.cutPlayBtn.SetChild(face)
	ed.cutPlayBtn.ConnectClicked(func() { ed.playAs(true) })
	ed.syncCutPlay() // opens with its tooltip and face in the recording state

	// The selection in numbers. It used to sit under its own ⟦ in / out ⟧ / ✕
	// buttons, but the band made those a second way of doing what a drag
	// already does -- and a pair of marks set by button could exist with no
	// selection under them, which left Add refusing while the readout showed a
	// range. The numbers are the half worth keeping, so they moved in with the
	// buttons that CONSUME a selection instead (see the bar below).
	ed.marks = gtk.NewLabel("")
	ed.marks.AddCSSClass("numeric")
	ed.marks.SetXAlign(0)
	ed.marks.SetTooltipText("the selection, in session time")
	ed.showMarks() // opens as dashes, not as a blank sliver under the buttons

	formPane := ed.buildForm()

	// The bar in linked groups rather than twenty equal buttons: each group reads
	// as one object and the bar fits a laptop. Left to right is the order of the
	// work -- move the playhead, mark, change the cut. Readings and set-once
	// controls live in the column beside the video (cut_form.go).
	rule := func() *gtk.Separator {
		s := gtk.NewSeparator(gtk.OrientationVertical)
		s.SetMarginTop(2)
		s.SetMarginBottom(2)
		return s
	}

	bar := gtk.NewBox(gtk.OrientationHorizontal, 6)
	// the wheel over the bar steps frames, so a hand hovering the transport
	// never has to land on one exact button to scrub
	bar.AddController(ed.wheelFrames())
	bar.Append(linked(ed.playBtn, ed.cutPlayBtn, prev5, prevF, nextF, next5))
	// how loud the preview is, next to the two ▶s that use it -- the run bar
	// at the bottom of the window has one too, and both are the same number
	// (volumeCtl). Here as well as there because this is the page a cut is
	// listened to on, and reaching past the timeline to a slider on the status
	// bar is a long way to go to turn the game down
	bar.Append(volumeCtl())
	bar.Append(rule())
	bar.Append(linked(add, ed.splitBtn, ed.remBtn, ed.copyBtn, ed.pasteBtn, ins, ed.laneBtn))
	bar.Append(fxDD)
	bar.Append(linked(ed.undoBtn, ed.redoBtn, ed.revertBtn, ed.clearBtn))
	// The bar is verbs only: everything on it changes the cut. The prompts are
	// on Prepare (prepedit.go); the view controls and every reading are in the
	// form column (cut_form.go), except the zoom, which is pressed all session.
	bar.Append(linked(zoomOut, zoomIn))
	// every line of the column reads the same way: what it is, then what it
	// says. The buttons are a reading too -- they say how big the pictures on
	// the tracks are -- and a pair of them with no name was the one row you had
	// to recognise by its icons.
	ed.formIdle.Append(idleRow("Thumbnails", linked(thumbMinus, thumbPlus)))
	// the shape of the finished video, with them: it is chosen once, out of
	// three answers, and it was the only dropdown on a bar of verbs
	ed.formIdle.Append(idleRow("Aspect ratio", ed.aspectDD))
	// ...and every reading the bar used to print under a group of buttons.
	// A number under a button is a caption on a control; a number in a column
	// of numbers is something to read. The bar is verbs now, and this is what
	// they are doing.
	ed.formIdle.Append(idleRow("Playhead", ed.clock))
	ed.formIdle.Append(idleRow("Selection", ed.marks))
	// the cut as the video plays it, and the same cut with the speed effects
	// taken off (rawLen): a target missed by 90 s was either 90 s too much
	// footage or 90 s not sped up, and one number cannot say which
	ed.formIdle.Append(idleRow("Cut", ed.total))
	ed.formIdle.Append(idleRow("Cut at 1×", ed.totalRaw))
	ed.formIdle.Append(idleRow("Source", ed.totalSrc))
	ed.formIdle.Append(idleRow("Segments", ed.totalSegs))

	ed.srcArea = gtk.NewDrawingArea()
	ed.srcArea.SetDrawFunc(func(_ *gtk.DrawingArea, cr *cairo.Context, w, h int) {
		ed.drawTrack(cr, w, h)
	})
	ed.audArea = gtk.NewDrawingArea()
	ed.audArea.SetDrawFunc(func(_ *gtk.DrawingArea, cr *cairo.Context, w, h int) {
		ed.drawAudio(cr, w, h)
	})
	ed.audArea.SetVisible(false) // until reload finds a separate recording
	// No tooltip on either band, and none on anything else drawn on the
	// timeline.
	//
	// They were here to advertise the right button, which has no hover state
	// of its own -- and a tooltip over a timeline is a paragraph laid across
	// the thing it is describing. It arrives on a pause the hand did not mean
	// to make (reading a waveform, lining a border up by eye), it covers the
	// lane's name, the frames and the seconds under it, and it goes away only
	// when the pointer moves, which is the moment you were trying not to move
	// it. Anything worth saying about the tracks is said IN them -- the
	// highlighted border, the lit badge, the cursor -- or in the status line
	// under them, which is a row that exists to be read and covers nothing.
	ed.srcArea.SetHExpand(true)
	ed.audArea.SetHExpand(true)
	// the tracks are as wide as the page, so their width IS the view width
	ed.srcArea.ConnectResize(func(w, h int) {
		ed.viewW = float64(w)
		ed.syncScroll()
		// the fit-the-window zoom moves with the window: widen the page while
		// fully zoomed out and the timeline has to grow with it, or a scrollbar
		// comes back for the empty strip beside it
		if m := ed.minPps(); ed.pps < m {
			ed.pps = m
			glib.IdleAdd(ed.relayout) // never resize from inside an allocation
		}
	})

	// The lanes answer to the mouse exactly as the picture band does: the wheel
	// zooms, a click places the playhead, a drag selects, and a press near a
	// border picks that border up. They are a view of the same timeline, and a
	// band you can see a cut point in but not click on is a band that makes you
	// aim at the thumbnails instead.
	for _, area := range []*gtk.DrawingArea{ed.srcArea, ed.audArea} {
		area := area
		area.SetFocusable(true) // so Del/Ctrl+Z reach the page after a click
		// wheel zooms around the cursor; Shift+wheel (and a trackpad's sideways
		// swipe) pans, which used to be the scrolled window's job
		motion := gtk.NewEventControllerMotion()
		motion.ConnectMotion(func(x, y float64) { ed.lastX = x })
		area.AddController(motion)
		scroll := gtk.NewEventControllerScroll(gtk.EventControllerScrollBothAxes)
		scroll.ConnectScroll(func(dx, dy float64) bool {
			if scroll.CurrentEventState()&gdk.ShiftMask != 0 {
				dx, dy = dy, 0
			}
			if dx != 0 {
				ed.setOff(ed.viewX + dx*ed.viewW/8)
			}
			if dy != 0 {
				ed.zoomWheel(dy)
			}
			return true
		})
		area.AddController(scroll)
		// The left button says WHICH SECONDS, and that is all: a drag is a selection
		// wherever it is pressed, a click puts the red line there and takes what it
		// landed on in hand. Trimming and sliding are the right button's (the slide
		// gesture below).
		drag := gtk.NewGestureDrag()
		var dragStartX, dragStartY float64
		var hadSel bool
		var selT0, selT1 float64
		var selPart int    // which part of the selection band this drag has, if any
		var fxPart int     // ...and which part of the effect's band
		var grabAt float64 // where in the held clip the press landed
		drag.ConnectDragBegin(func(x, y float64) {
			area.GrabFocus()
			dragStartX, dragStartY = x, y
			// a press in the effects lane is about the effect under it: it
			// picks that effect up if it was not already in hand, and the
			// drag then slides it -- the same deal a held clip gets one band
			// up, except that an effect needs no separate right-click first.
			// A marker is a few px wide; making the hand say "this one" twice
			// before it may move it is a tax on the only thing you can do to
			// it here.
			// a press in the selection band is about the selection: its ends
			// move that end, its middle moves the whole of it, its ✕ throws it
			// away, and a press on the empty part of the row starts a new one
			// exactly as a press on the pictures does. Like the effects lane,
			// no separate right-click first: there is one object in this row,
			// and making the hand name it twice is a tax on the only thing
			// there is to do here.
			selPart = selNone
			// the fold-all control, in the black strip at the head of the tape
			// (cut_gutter.go). Before the band's own answers: it stands in
			// front of second zero, where none of them reach.
			if area == ed.srcArea && ed.foldAllAt(x+ed.viewX, y) {
				ed.toggleFoldAll()
				return
			}
			if area == ed.srcArea && ed.hitSelBand(y) {
				// the − that folds a dropped stretch away and the + that
				// brings it back, between the two bars they hold apart
				// (cut_fold.go). Asked first: the badge sits where a press
				// would otherwise land on one of those bars' ends.
				if i := ed.foldBadgeAt(x+ed.viewX, y); i >= 0 {
					ed.toggleFold(i, x+ed.viewX)
					return
				}
				if selPart = ed.selPartAt(x + ed.viewX); selPart == selKill {
					ed.killSel()
					selPart = selNone
					return
				}
				if selPart != selNone {
					ed.holdSel(selPart)
					a, _ := ed.selSpan()
					grabAt = ed.tAtView(x) - a
					return
				}
				// clear of the blue, the GREEN bar's ✕ is the one thing it answers to this
				// button; trimming and sliding are the right button's. A press that goes
				// nowhere still takes the clip in hand at the release, with the picture
				// band's.
				if i := ed.bandKillAt(x + ed.viewX); i >= 0 {
					ed.killSeg(i) // the page's one "drop that scene"
					return
				}
				ed.dropSel() // clear of it: this is a new selection
			}
			if area == ed.srcArea && ed.fxHitLane(y) {
				// the ✕ at a band's right end, asked before the band: the same
				// press would otherwise pick the effect up (cut_fxkill.go)
				if i := ed.fxKillAt(x+ed.viewX, y); i >= 0 {
					ed.killFx(i)
					return
				}
				if i := ed.fxIndexAt(x+ed.viewX, y); i >= 0 {
					if !ed.fxOn || ed.fxSel != i {
						ed.holdFx(i)
					}
					// a press on an end of a band that has ends changes that
					// end; anywhere else on it slides the whole thing
					fxPart = ed.fxPartAt(i, x+ed.viewX)
					ed.fxMoving = true
					grabAt = ed.tAtView(x) - ed.heldFx().T
				} else {
					ed.dropFx() // a press on the empty lane puts it down
				}
				return
			}
			// a press ON an end of the blue selection, from the pictures or
			// the sound under them, takes that end -- the same grip the
			// selection row gives it, and asked BEFORE every badge and switch
			// below: where a grip is drawn under the pointer, that is what the
			// press means, and a badge that happens to share the pixels does
			// not get to make it a fresh selection. Only the ends: a press
			// inside the blue still starts a new selection, since drawing one
			// inside another is a thing a hand does, and sliding the whole
			// band is the row's job. The gutter's controls are not the tape
			// and keep their answer (cut_gutter.go).
			if !ed.gutterCtl(x+ed.viewX, y) {
				if part := ed.selPartNear(x+ed.viewX, selGripPicsPx); part == selStart || part == selEnd {
					selPart = part
					ed.holdSel(selPart)
					a, _ := ed.selSpan()
					grabAt = ed.tAtView(x) - a
					return
				}
			}
			// a green border under the press: the drag trims it (hovering
			// highlighted it first, so one button is enough). Lane badges are
			// asked before anything else -- they exist only while a scene is in
			// hand -- and the permanent lane switch before a scene's badge.
			if area == ed.audArea {
				if base := ed.laneSwitchAt(x+ed.viewX, y); base != "" {
					ed.toggleLaneAll(base)
					return
				}
			}
			// the same switch for the sound filmed with the pictures, on the
			// paired strip under them: one per camera row, and asked here for
			// the reason above
			if area == ed.srcArea {
				if bases := ed.pairSwitchAt(x+ed.viewX, y); len(bases) > 0 {
					ed.toggleLanesAll(bases, pairSwitchName(bases))
					return
				}
			}
			if base := ed.hearAt(x+ed.viewX, y, area == ed.srcArea); base != "" {
				ed.toggleHear(base)
				return
			}
			// the same column, on the picture rows: which camera the scene is
			// shown from (cut_cam.go). Asked with the sound's badges and for
			// the reason they are asked here -- while a scene is in hand,
			// pressing one of its marks is the only thing the press can mean
			if area == ed.srcArea {
				if r := ed.camBadgeAt(x+ed.viewX, y); r >= 0 {
					ed.setSegCam(r)
					return
				}
			}
			// the ✕ badges the picture band carries, asked before the borders
			// they can overlap: a press on one would otherwise be read as
			// taking hold of a clip edge to trim it. Dropping a SCENE is not
			// among them any more -- that ✕ is on the green bar in the
			// selection row, and it is answered with the rest of that row's
			// parts (bandClipPartAt, above).
			if area == ed.srcArea && ed.hitPics(y) {
				// a cut lane's own ✕ sits at the row's start (cut_lane.go)
				if name := ed.laneKillAt(x+ed.viewX, y); name != "" {
					ed.killLane(name)
					return
				}
				// an empty row's ✕ can share ground with nothing: the row
				// wearing it has no footage, so no lane badge either
				if r := ed.rowKillAt(x+ed.viewX, y); r >= 0 {
					ed.killRow(r)
					return
				}
			}
			ed.dropEdge() // any other left click puts a held edge or clip down
			ed.dropSeg()
			ed.dropSel()
			hadSel, selT0, selT1 = ed.sel.active, ed.sel.t0, ed.sel.t1
			ed.sel.t0 = ed.tAtView(x)
			ed.sel.t1 = ed.sel.t0
			ed.sel.active = true
			// a selection drawn in a lane is a selection of THAT sound, and
			// the press is the only place it can be said: from here on the
			// selection is a span of session time like any other, and which
			// band drew it is not something the span remembers by itself.
			ed.sel.aud = ""
			if area == ed.audArea {
				ed.sel.aud = ed.audAtY(y)
			}
			// ...and a selection drawn on the pictures is a selection of the
			// CAMERA it was drawn on, said in the same place and for the same
			// reason. A press in the thin space between two rows keeps the row
			// the last one used rather than jumping to the first: it is a miss,
			// not a change of mind.
			if area == ed.srcArea {
				if l := ed.laneAt(y); l >= 0 {
					ed.sel.lane = l
				} else if l := ed.pairAt(y); l >= 0 {
					// drawn on the wave strip under a row: that row's camera,
					// and the selection is of that footage's SOUND -- the
					// strip is the lane the recording used to have below
					ed.sel.lane = l
					ed.sel.aud = ed.pairAudAt(x+ed.viewX, y)
				}
			}
			ed.syncSelBtns()
		})
		drag.ConnectDragUpdate(func(ox, oy float64) {
			if ed.fxMoving {
				// nothing has moved and the pointer has barely left the press: still a
				// CLICK, which opens an effect's numbers at the release. Without the guard
				// the slide snapped the band to the nearest boundary (snapFxSpan) on the
				// way to opening it.
				if !ed.fxDirty && math.Abs(ox) < dragSlop && math.Abs(oy) < dragSlop {
					return
				}
				if fxPart == fxStart || fxPart == fxEnd {
					ed.resizeFxTo(fxPart == fxEnd, ed.tAtView(dragStartX+ox))
				} else if f := ed.heldFx(); f != nil {
					// the whole band slides, so both its ends are offered to
					// the cuts and the other effects (snapFxSpan)
					t0, t1 := f.fxSpan()
					ed.moveFxTo(ed.snapFxSpan(ed.tAtView(dragStartX+ox)-grabAt, t1-t0), true)
				}
				ed.showFx(true) // the picture comes with it, as with a clip
				return
			}
			switch selPart {
			case selWhole:
				ed.moveSelTo(ed.tAtView(dragStartX+ox) - grabAt)
				return
			case selStart, selEnd:
				ed.resizeSelTo(selPart == selEnd, ed.tAtView(dragStartX+ox))
				return
			}
			ed.sel.t1 = ed.tAtView(dragStartX + ox)
			ed.syncSelMarks() // the readout under Add follows the drag live
			ed.syncSelBtns()
		})
		drag.ConnectDragEnd(func(ox, oy float64) {
			_, _, _ = hadSel, selT0, selT1
			if ed.fxMoving {
				ed.fxMoving, fxPart = false, fxWhole
				moved := ed.fxDirty
				if ed.fxDirty {
					ed.persist()
					ed.fxDirty = false
				}
				ed.fxStatus()
				// a press that did not move the effect is a click, and a click
				// opens its numbers -- on an idle, not inside the gesture, and
				// only unmoved: the form looks the effect up by its numbers at
				// the press (updateFx)
				if !moved && math.Abs(ox) < dragSlop && math.Abs(oy) < dragSlop {
					glib.IdleAdd(func() { ed.a.editFx() })
				}
				return
			}
			if selPart != selNone {
				part := selPart
				selPart = selNone
				ed.holdSel(part) // the status line, with the numbers as they now are
				return
			}
			if math.Abs(ox) >= 5 || math.Abs(oy) >= 5 {
				return // a real drag: the new selection stands
			}
			// a press without movement is a CLICK: cue the playhead. The
			// selection dies with it, readout included -- a reading that
			// outlived its band would show a range Add then refuses to act
			// on. Only the marks: clearMarks would also drop a held effect,
			// which a click on the footage deliberately leaves in hand.
			ed.sel.active = false
			ed.hasIn, ed.hasOut = false, false
			ed.showMarks()
			ed.syncSelBtns()
			// a click on a row also says which camera the preview shows (monRow, not
			// sel.lane: watching is a choice only a click makes). A click elsewhere
			// moves the line and changes no minds. Not while playing: ▶ promised the
			// cut, and followPlayback re-cues from camAt every tick.
			if area == ed.srcArea && !ed.playing() && len(ed.vids) > 0 {
				if l := ed.laneAt(dragStartY); l >= 0 {
					ed.monRow = l + 1
				} else if l := ed.pairAt(dragStartY); l >= 0 {
					ed.monRow = l + 1
				}
			}
			// ...unless the press was on a control in the gutter, which is
			// not the tape and has no second to cue to (cut_gutter.go)
			if !ed.gutterCtl(dragStartX+ed.viewX, dragStartY) {
				ed.setPlayhead(ed.tAtView(dragStartX))
				ed.monStatus()
			}
			// ...and a click ON THE GREEN takes that scene in hand, as the same click on
			// the green bar does: one object drawn in two rows. Last, so the scene's own
			// account stands; clear of the green, the drop above (dropSeg) is what the
			// click meant.
			if area == ed.srcArea && ed.hitPics(dragStartY) {
				if px := dragStartX + ed.viewX; ed.segOnGreen(px, dragStartY) >= 0 {
					ed.grabSeg(px) // the same scene: segOnGreen asked segAtPx for it
				}
			}
			// ...and the same click on the bar that stands for that scene, one
			// row up. The bar answers the hand for the whole cut now, so a
			// press on any of them takes that clip -- which is what the press
			// on its ends and middle used to do before the right button took
			// the dragging (bandClipPartAt).
			if area == ed.srcArea && ed.hitSelBand(dragStartY) {
				if i, part := ed.bandClipPartAt(dragStartX + ed.viewX); part != selNone && part != selKill {
					ed.holdBandClip(i, selWhole)
				}
			}
		})
		area.AddController(drag)

		// The right button MOVES what is under it: on the green a scene or its
		// border; inside a selection every scene in it; clear of the green the
		// recordings themselves (cut_shift.go).
		slide := gtk.NewGestureDrag()
		slide.SetButton(gdk.BUTTON_SECONDARY)
		var slideSrcs []string           // what this drag moves; empty = the green
		var slideFrom map[string]float64 // their corrections at the press
		var slideSegs []cutSeg           // ...or the cut at the press
		var slideWhat string
		var slideD float64                   // the gesture so far, in seconds
		var slideEdges, slideTargs []float64 // what moves, and what it lands on
		var slideX0, slideY0 float64         // where the press landed: the row under it, and the seconds
		// the cut's own drags, which used to be the left button's: a border
		// being trimmed, or a whole scene being slid along the recording
		var trimming, moving bool
		var slideGrab float64 // where in the held clip the press landed
		var slideRows bool    // this drag may change rows (it moves sources on the picture band)
		var slideOn, slideTimeOn bool
		var foldShutList []foldGap // seams opened for the hold, put back on release
		slide.ConnectDragBegin(func(x, y float64) {
			area.GrabFocus()
			slideSrcs, slideFrom, slideSegs = nil, nil, nil
			slideD, slideOn, slideTimeOn = 0, false, false
			foldShutList = nil
			trimming, moving = false, false
			slideX0, slideY0 = x, y
			a0, a1 := ed.selSpan()
			t := ed.tAtView(x)
			px := x + ed.viewX
			// The cut's own green first: left says WHICH SECONDS, right moves
			// what is under it. Below the selection's case, which is the same
			// verb over every scene inside it.
			green := func() bool {
				if area != ed.srcArea {
					return false
				}
				switch {
				case ed.hitSelBand(y):
					// the bar stands for the clip, so its ends are that clip's
					// borders and its middle is the clip (bandClipPartAt)
					i, part := ed.bandClipPartAt(px)
					if part == selNone {
						return false
					}
					// the ✕ is in the middle of the bar now (cut_selband.go),
					// and this button has no remove: over it a press means the
					// clip, like any other press on a bar's middle.
					if part == selKill {
						part = selWhole
					}
					ed.holdBandClip(i, part)
					if part == selWhole {
						moving, slideGrab = true, ed.tAtView(x)-ed.segs[i].S
					} else {
						trimming = true
					}
					return true
				case ed.hitPics(y):
					// a border first, by the same few px the highlight under
					// the pointer has been offering all along (hoverEdge)
					if ed.onHeldEdge(px) || ed.grabEdge(px) {
						trimming = true
						return true
					}
					if ed.segOnGreen(px, y) < 0 {
						return false // plain footage: the recordings move instead
					}
					if !ed.onHeldSeg(px) {
						ed.grabSeg(px)
					}
					if s := ed.heldSeg(); s != nil {
						moving, slideGrab = true, ed.tAtView(x)-s.S
						return true
					}
				}
				return false
			}
			switch {
			case area == ed.audArea:
				// a lane has no green of its own to move, and one recording
				// out of step with the rest is the whole reason this exists
				slideSrcs = []string{ed.audAtY(y)}
				slideWhat = "the recording"
			case area == ed.srcArea && ed.pairAt(y) >= 0:
				// on the wave strip: the one recording under the pointer, not
				// the whole row -- the per-recording drag the old audio band
				// had lives here now. Its pictures come with it: one base,
				// one correction (cut_shift.go).
				slideSrcs = []string{ed.pairAudAt(x+ed.viewX, y)}
				slideWhat = slideSrcs[0]
			case ed.sel.active && ed.sel.aud == "" && t >= a0 && t < a1 &&
				(ed.hitPics(y) || ed.hitSelBand(y)):
				slideSegs = append([]cutSeg(nil), ed.segs...)
				slideWhat = "the selected scenes"
				foldShutList = ed.foldOpen(a0, a1, px) // same rule as one scene, over all of them
			case green():
				// what is in hand can be dragged across a seam, so the seams
				// around it come open for as long as the button is down: the
				// drag then runs on the ordinary timeline, where a pixel is a
				// pixel (cut_fold.go)
				if s := ed.heldSeg(); s != nil {
					foldShutList = ed.foldOpen(s.S, s.E, px)
				} else if trimming {
					foldShutList = ed.foldOpen(t, t, px)
				}
				return // a scene or a border in hand; the update drags it
			default:
				l := ed.laneAt(y)
				if l < 0 {
					l = ed.sel.lane // the hair between two rows is a miss, not a choice
				}
				slideSrcs = ed.laneSrcs(l)
				slideWhat = fmt.Sprintf("camera %d", l+1)
			}
			if len(slideSrcs) == 1 && slideSrcs[0] == "" {
				slideSrcs = nil // an empty lane row: nothing under the pointer
			}
			slideFrom = copyShift(ed.shift)
			slideEdges, slideTargs = ed.slideSnapSet(slideSrcs, slideSegs != nil)
			// only sources on the picture band have a row to move to: a
			// separate recording's lane is the recorder's, not a row, and the
			// green names rows rather than sitting on one
			slideRows = slideSrcs != nil && area == ed.srcArea
		})
		// The drag is read in pixels over the zoom, not through tAtView: an
		// unfilmed stretch is drawn as one fixed hatch however many minutes it
		// stands for, so two x a hair apart across one can be a quarter of an
		// hour apart in time. Correcting a clock by "however long that gap
		// happens to be" is not a correction anyone asked for.
		slide.ConnectDragUpdate(func(ox, oy float64) {
			// the cut's own drags, before anything that moves recordings:
			// nothing has moved yet and the pointer has barely left where it
			// was pressed, so this is still a click and a click does not drag
			// what it landed on
			if trimming || moving {
				if (trimming && !ed.edgeDirty) || (moving && !ed.segDirty) {
					if math.Abs(ox) < dragSlop && math.Abs(oy) < dragSlop {
						return
					}
				}
				if trimming {
					ed.moveEdgeTo(ed.tAtView(slideX0+ox), true)
					ed.showEdge(true) // the picture comes with it
					return
				}
				ed.moveSegTo(ed.tAtView(slideX0+ox)-slideGrab, true)
				ed.showSeg(true)
				return
			}
			if ed.pps <= 0 || (slideSrcs == nil && slideSegs == nil) {
				return
			}
			d := ox / ed.pps
			// ...but it does snap: within a few pixels of an edge of the
			// dragged material meeting a still one -- the selection's border
			// above it, another recording's start, a scene's edge -- the drag
			// lands exactly on it (slideSnap). Same reach at every zoom.
			d = slideSnap(d, slideEdges, slideTargs, snapPx/math.Max(ed.pps, 0.001))
			// The same drag moves between rows: the pointer standing on
			// another row that has room is the ask (moveRow). Worked out
			// before the gates below because a purely vertical drag is a drag
			// too, and has to open the gesture -- but never the TIME gate,
			// which stays sideways-only: with the hand moving straight up the
			// snap would otherwise be free to yank the part sideways onto
			// whatever alignment happens to be in reach.
			to := -1
			if slideRows {
				if r := ed.rowAt(slideY0 + oy); ed.rowFits(slideSrcs, r) {
					to = r
				}
			}
			slideTimeOn = slideTimeOn || math.Abs(ox) >= 3
			if !slideTimeOn && to < 0 {
				return // a right click, not yet a drag
			}
			if !slideOn {
				ed.pushUndo() // the whole gesture is one step back, rows and seconds both
				slideOn = true
			}
			if slideTimeOn {
				slideD = d
				if slideSrcs != nil {
					ed.shiftTo(slideSrcs, slideFrom, d)
				} else if ed.slideGreen(slideSegs, d) {
					ed.redrawTracks()
				}
				ed.a.setStatus(shiftLabel(slideWhat, d))
			}
			if to >= 0 && ed.moveRow(slideSrcs, to) {
				ed.a.setStatus(fmt.Sprintf("%s moved to row %d — its kept scenes came along", slideWhat, to+1))
			}
		})
		slide.ConnectDragEnd(func(ox, oy float64) {
			// the seams this hold opened go back, anchored on the column the
			// press landed in so the page does not jump out from under the
			// hand that just let go (cut_fold.go). Deferred: the branches
			// below return early, and every one of them ends the same hold.
			defer func() {
				ed.foldShut(foldShutList, slideX0+ed.viewX)
				foldShutList = nil
			}()
			if trimming {
				trimming = false
				// a border trimmed out until it meets the next clip closes the
				// gap between them, and two kept stretches with nothing
				// between them are one stretch: the same join a clip dragged
				// against its neighbour gets (cut_split.go). This is the
				// commoner way to ask for it -- the gap is closed by extending
				// what is kept, where sliding a clip moves the footage.
				merged := ed.edgeDirty && ed.mergeTouching(ed.edgeSeg)
				if ed.edgeDirty {
					ed.persist() // the drag is over: this is the cut that goes on disk
					if !merged {
						// the picture lands exactly where the edge did,
						// throttling or no throttling, so what you trimmed to
						// is what is on screen and the next ‹f is judged
						// against it. Only when something actually moved: a
						// press that merely picked the border up is a choice,
						// and a choice does not move the red line (pickAt).
						ed.showEdge(false)
					}
				}
				if merged {
					return // the border is gone, and the join said so
				}
				ed.edgeStatus()
				return
			}
			if moving {
				moving = false
				// a press that went nowhere is a CLICK, and a right click on
				// a scene means "this one": it is in hand, and the status
				// says what is in hand. The red line is not moved -- putting
				// it somewhere is the left button's, and the whole point of
				// two buttons is that neither does the other's job.
				if !ed.segDirty && math.Abs(ox) < dragSlop && math.Abs(oy) < dragSlop {
					ed.segStatus() // a right click on a scene is "this one"
					return
				}
				// dragged up against the clip beside it, the two are one clip
				// again: the drop is the join (cut_split.go). Asked before the
				// write, so what goes on disk is the merged cut, and it says
				// its own sentence -- there is no held clip left to report on.
				merged := ed.segDirty && ed.mergeDropped()
				if ed.segDirty {
					ed.persist()
					ed.segDirty = false
					// the picture lands where the clip did, so what you moved
					// it to is what is on screen. Only when it actually MOVED,
					// the rule the held edge above already keeps: the second
					// press of a double click lands on a clip that is already
					// in hand and ends a drag that went nowhere, and putting
					// the line on the clip's start there is the page yanking
					// the picture away from the frame that was clicked.
					ed.showSeg(false)
				}
				if merged {
					return
				}
				ed.segStatus()
				return
			}
			if !slideOn {
				return
			}
			slideOn = false
			ed.persist()
			// the frame under the red line is different footage now, or the
			// same footage at a different second, and a preview still showing
			// the old one is the page disagreeing with itself
			ed.setPlayhead(ed.playhead)
			ed.a.setStatus(shiftLabel(slideWhat, slideD))
		})
		area.AddController(slide)

		// The second click of a double click takes the whole clip: a single
		// press is how the red line is placed, so it cannot also pick things
		// up. On the green the drag's release already took the scene; this is
		// for dropped footage and cards (pickAt).
		pick := gtk.NewGestureClick()
		pick.SetButton(gdk.BUTTON_PRIMARY)
		pick.ConnectPressed(func(n int, x, y float64) {
			if n < 2 {
				return
			}
			if ed.segOnGreen(x+ed.viewX, y) >= 0 {
				return // the single click has it; a second one must not re-take it
			}
			area.GrabFocus()
			if area == ed.srcArea && ed.fxHitLane(y) {
				return // the lane's own gesture, and it is on the single press
			}
			if area == ed.srcArea && !ed.hitPics(y) {
				return
			}
			ed.pickAt(x+ed.viewX, true)
		})
		area.AddController(pick)

		// Hovering says what a press would take hold of, and says it in the band
		// itself as well as in the pointer: the effect markers sit shoulder to
		// shoulder and the narrowest one wins (see fxIndexAt), and a clip border
		// is two px of green among a lot of other green, so "the one under the
		// pointer" is not always the one the eye would have guessed.
		// Highlighting the answer removes the guess -- and on the borders it is
		// what lets a single press mean trim without ever surprising anyone.
		hover := gtk.NewEventControllerMotion()
		if area == ed.srcArea {
			hover.ConnectMotion(func(x, y float64) { ed.hoverTracks(x, y) })
			hover.ConnectLeave(func() { ed.hoverTracks(-1, -1) })
		} else {
			hover.ConnectMotion(func(x, y float64) { ed.hoverLanes(x, y) })
			hover.ConnectLeave(func() { ed.hoverLanes(-1, -1) })
		}
		area.AddController(hover)
	}

	// The scrollbar is ours rather than a scrolled window's, because a scrolled
	// window would want a child as wide as the whole timeline (see drawTrack).
	// Hidden when it cannot move: a bar at the zoom floor is a bar that says
	// there is more session off to the right when there is not.
	ed.hadj = gtk.NewAdjustment(0, 0, 0, 1, 1, 0)
	ed.hadj.ConnectValueChanged(func() {
		ed.viewX = ed.hadj.Value()
		if ed.scrollMut {
			return // syncScroll is writing it, and draws once itself when it must
		}
		// a pan or a zoom moves where things are drawn and nothing else: the
		// preview's camera layer depends on the playhead and the effects, not
		// on the view, so it is not re-synced for every pixel of scrolling
		ed.queueTracks()
	})
	ed.hbar = gtk.NewScrollbar(gtk.OrientationHorizontal, ed.hadj)
	ed.hbar.SetVisible(false)

	band := gtk.NewBox(gtk.OrientationVertical, 4)
	band.Append(ed.srcArea)
	band.Append(ed.audArea) // the recorders' band: the sound nobody filmed
	tracks := gtk.NewBox(gtk.OrientationVertical, 4)
	tracks.Append(ed.lineOver(band)) // the red line, on a layer of its own
	tracks.Append(ed.hbar)
	tracks.SetVExpand(true)
	tracks.SetVAlign(gtk.AlignStart) // the tracks are their own height; the rest is air

	bottom := gtk.NewBox(gtk.OrientationVertical, 8)
	bottom.SetMarginTop(6)
	bottom.SetMarginStart(12)
	bottom.SetMarginEnd(12)
	bottom.SetMarginBottom(8)
	bottom.Append(bar)
	bottom.Append(tracks)

	// What this step wrote and a way into the folder. ed.total above says what
	// the editor holds; this says what is actually saved, which is the thing
	// the next step reads. The group rides the shared bottom bar (outStack in
	// main.go) rather than the page: every step answers this same question,
	// so it is asked in one place.
	openOut := gtk.NewButtonFromIconName("folder-open-symbolic")
	openOut.SetTooltipText("cut/ — the cut, as cut.json")
	openOut.ConnectClicked(func() { a.openFolder(a.cutDir()) })
	ed.out = gtk.NewLabel("")
	outRow := gtk.NewBox(gtk.OrientationHorizontal, 8)
	outRow.Append(openOut)
	outRow.Append(ed.out)
	a.outStack.AddNamed(outRow, "cut")
	ed.updateOut()

	// Ctrl+Z and Del on the page. Bubble phase on purpose: the notes box and
	// the target entry see the key first and keep their own editing behaviour.
	keys := gtk.NewEventControllerKey()
	keys.ConnectKeyPressed(func(keyval, keycode uint, state gdk.ModifierType) bool {
		switch {
		// space is ▶/⏸, the BUTTON's own verb (toggle): the cut under ▶✂, the
		// recording otherwise, from the red line. Bubble phase, like the rest of this
		// switch: a text box or focused button that took the key keeps it.
		case keyval == gdk.KEY_space && state&(gdk.ControlMask|gdk.AltMask) == 0:
			ed.toggle()
		case keyval == gdk.KEY_z && state&gdk.ControlMask != 0:
			ed.undoLast()
		case (keyval == gdk.KEY_Z || keyval == gdk.KEY_y) && state&gdk.ControlMask != 0:
			ed.redoLast()
		case keyval == gdk.KEY_Delete || keyval == gdk.KEY_BackSpace:
			a.removeSelClicked()
		// ← and → are the frame buttons for the hand that is already on the
		// mouse, and they exist ONLY while an edge or a clip is held: unheld
		// they are the focus keys GTK expects them to be. Bubble phase still
		// gives the notes box and the lists their own scrolling first.
		case (ed.edgeOn || ed.segOn || ed.fxOn) && (keyval == gdk.KEY_Left || keyval == gdk.KEY_Right):
			n := 1
			if state&gdk.ShiftMask != 0 {
				n = 5
			}
			if keyval == gdk.KEY_Left {
				n = -n
			}
			ed.frameStep(n)
		case (ed.edgeOn || ed.segOn || ed.fxOn || ed.selOn || ed.copyOn || ed.fxArm != "") && keyval == gdk.KEY_Escape:
			ed.dropEdge()
			ed.dropSeg()
			ed.dropFx()
			ed.dropSel()
			ed.copyOn = false
			ed.syncInsertBtn()
			ed.fxArm = ""
			ed.syncFxCursor()
			ed.syncPreviewZoom() // an armed view/zoom had the live layer down
		default:
			return false
		}
		return true
	})
	bottom.AddController(keys)

	// Video and forms side by side on top, timeline across the full width
	// below. The picture is 16:9 and a form is a column of labelled rows, so
	// they want opposite shapes -- stacked, the column ate the height the tracks
	// needed and the space beside the video stayed empty. The tracks are the one
	// thing that wants the whole width, so they get it.
	top := gtk.NewPaned(gtk.OrientationHorizontal)
	top.SetEndChild(formPane)
	top.SetShrinkEndChild(false)
	if ed.player != nil {
		ed.player.Picture.SetVExpand(true)
		ed.player.Picture.SetSizeRequest(-1, 160)
		// clicking the video itself also toggles; the ▶/⏸ button lives in the bar
		click := gtk.NewGestureClick()
		click.ConnectReleased(func(n int, x, y float64) { ed.toggle() })
		ed.player.Picture.AddController(click)
		// a frame + breathing room, so the video is not glued to its neighbors.
		// Between the two, the framing overlay: the rectangle that says what a
		// vertical (or zoomed) cut of this picture will show (cut_fxview.go).
		vframe := videoFrame(ed.buildFxOverlay())
		vframe.SetMarginTop(8)
		vframe.SetMarginStart(12) // the window's edge
		vframe.SetMarginEnd(6)    // ...and the handle's, which the form column matches
		vframe.SetMarginBottom(6)
		top.SetStartChild(vframe)
	} else {
		top.SetStartChild(gtk.NewBox(gtk.OrientationVertical, 0)) // no preview: the forms have the row
	}
	top.SetPosition(660)

	// What this page reads, on the shared bottom bar beside what it has
	// written (inputsLabel). The question it answers is the one asked just
	// before pressing Suggest -- is everything in here, and does the model get
	// to hear as well as see -- and it was answerable only by opening
	// session.txt.
	ed.inputs = inputsLabel()
	a.inStack.AddNamed(ed.inputs, "cut")
	ed.updateInputs()

	// which half of the page matters depends on whether you are cutting or
	// tuning what Suggest is told, so the divider is the user's
	pane := gtk.NewPaned(gtk.OrientationVertical)
	pane.SetStartChild(top)
	pane.SetEndChild(bottom)
	pane.SetPosition(380)
	pane.SetVExpand(true)
	// the tracks are never squeezed away. A paned shrinks both children below
	// their minimum by default, and with the log open on a short window the
	// 380 px above left the bottom half a few pixels of thumbnail at the edge
	// of the screen -- which reads as "the timeline is gone", not as "the
	// timeline is small". The picture above has a floor of its own (160 px)
	// and gives way first.
	pane.SetShrinkEndChild(false)
	pane.SetResizeStartChild(true)
	pane.SetResizeEndChild(false)

	page := gtk.NewBox(gtk.OrientationVertical, 4)
	page.Append(pane)
	// and the empty timeline is laid out from the start: the ruler, the
	// and an empty row are a page with no cut yet, which is a real state, and
	// the band has no height until relayout gives it one (see clearTracks)
	ed.relayout()
	return page
}

// zoomStep is what the + and − in the bar do: the wheel's own step, but around
// the middle of what is on screen rather than around a cursor that is up on the
// button and not over the timeline at all. Zooming about the view's center is
// also what keeps a click-click-click on + heading somewhere: whatever you
// centered stays centered.
func (ed *cutEditor) zoomStep(factor float64) { ed.zoomAt(ed.viewW/2, factor) }

// zoomWheel banks a wheel delta and applies it once on the next idle. A
// touchpad delivers a notch as ten or twenty fractional deltas, and each one
// used to be a whole relayout and redraw -- the lag. Banked, they are one
// factor, drawn once.
func (ed *cutEditor) zoomWheel(dy float64) {
	ed.zoomPend += dy
	if ed.zoomBook {
		return
	}
	ed.zoomBook = true
	glib.IdleAdd(func() {
		ed.zoomBook = false
		dy := ed.zoomPend
		ed.zoomPend = 0
		if dy != 0 {
			ed.zoomAt(ed.lastX, math.Pow(1.25, -dy))
		}
	})
}

// zoomAt zooms about a point of the VIEW (a cursor position, or its middle),
// keeping whatever is under that point under it afterwards.
func (ed *cutEditor) zoomAt(viewX, factor float64) {
	t := ed.tAtView(viewX)
	pps := math.Max(ed.minPps(), math.Min(120, ed.pps*factor))
	if pps == ed.pps {
		return // against a stop: nothing to lay out and nothing to draw
	}
	ed.pps = pps
	// the pixels, the scrollbar, one draw. Not relayout: that is for a change
	// of what is ON the timeline and re-syncs the preview widget's camera
	// layer too, and a zoom changes only where things are drawn.
	ed.layoutPx()
	ed.syncScroll()
	ed.setOff(ed.xOf(t) - viewX)
	// ...and the draw, rather than leaving it to setOff: the adjustment only
	// fires value-changed when the value MOVES, and a zoom anchored against
	// either end clamps to the same offset. The pixels changed and nothing
	// repainted them until the pointer moved.
	ed.queueTracks()
	ed.updateTotal()
}

// minPps is the zoom at which the whole session fits the window, the floor.
// The holes between filmed runs are drawn at a fixed width, so they come off
// the width the footage may use -- otherwise the fully zoomed-out timeline
// was wider than its window by every hole in it.
func (ed *cutEditor) minPps() float64 {
	// the gutter comes off the width the footage may use, exactly as the holes
	// do: it is drawn at a fixed width and does not shrink with the zoom
	// (cut_gutter.go)
	return fitPps(ed.viewW-gutterPx, ed.filmedDur())
}

// fitPps is that floor without a widget in the way: the zoom at which dur
// seconds come to exactly view pixels, the rounding in relayout included. The
// runs cost nothing to lie between, so only their total length counts.
func fitPps(view, dur float64) float64 {
	if view <= 0 || dur <= 0 {
		return 0 // no allocation yet, or nothing loaded: no width to fit into
	}
	return math.Max(0, (view-1)/dur) // -1: relayout rounds the width up
}

// sessEnd is the far end of the session: the moment the last recording stops.
// Past it is time nobody filmed, so nothing on the timeline may be dragged out
// there. A clip and an edge are already held to their own recording; an effect
// is held to this, which is the same rule one recording wider.
func (ed *cutEditor) sessEnd() float64 {
	// the LATEST end, not the last recording's: sorted by start, the file that
	// begins last is not necessarily the one that stops last -- a camera that
	// rolled the whole session outlasts the one switched on halfway through it
	end := 0.0
	for _, v := range ed.vids {
		end = math.Max(end, v.start+v.dur)
	}
	return end
}

// filmedDur is how much of the session got filmed at all: the runs added up,
// NOT the recordings added up. Two cameras rolling through the same minute are
// one minute of timeline between them, and a zoom fitted to the sum of the
// files would fit the tracks into half the window.
func (ed *cutEditor) filmedDur() float64 {
	d := 0.0
	for _, s := range ed.runs() {
		d += s.dur()
	}
	return d
}

// updateCutInfo (re)loads the editor when its inputs exist. It is the ONLY
// thing that fills this page -- buildCut makes an empty one -- so anything
// that changes what Prepare wrote has to end up here, or the tracks go on
// showing a session that is over. refreshCut is how the runs say so.
func (a *App) updateCutInfo() {
	if a.ed == nil {
		return
	}
	a.ed.updateOut()    // true even with no timeline to load: the folder is the folder
	a.ed.updateInputs() // and so is what is missing, which is the useful part here
	a.ed.stale = false  // whatever the tracks show after this, it is what is on disk
	if !a.canCut() {
		a.ed.clearTracks()
		return
	}
	if err := a.ed.reload(); err != nil {
		a.logf("cut editor: %v", err)
		a.ed.clearTracks() // a half-built timeline is worse than an empty one
	}
}

// refreshCut brings the Cut page up to date with what a run just wrote: now
// if it is on screen, otherwise on the way in. Describe writes the timeline
// the page is gated on, and nothing else rebuilt the tracks.
func (a *App) refreshCut() {
	if a.ed == nil {
		return
	}
	a.ed.stale = true
	if a.stack == nil || a.stack.VisibleChildName() != "cut" {
		return // it will catch up on the way in
	}
	// on screen, so it has to catch up now -- but not once per caller. Opening a
	// project says "the sources changed" three times on its way through
	// applyProject, and a rebuild is three ffprobes per recording. The idle pass
	// folds them into the one that matters, the last.
	if a.ed.pending {
		return
	}
	a.ed.pending = true
	glib.IdleAdd(func() {
		a.ed.pending = false
		if a.ed.stale {
			a.updateCutInfo()
		}
	})
}

// clearTracks empties the timeline. For the project that was swapped out from
// under the page: without it, opening another project whose folder holds no
// session of its own leaves the previous one's recordings drawn on the tracks,
// which is the most convincing wrong thing this page can show.
//
// It lays the empty timeline out too, rather than returning early when there
// is nothing to clear. The band has no height of its own -- fitSrc gives it
// one, and only relayout calls fitSrc -- so an editor that was never laid out
// is a page with no tracks on it at all, not a page with empty tracks. That
// was what a project with frames but no cut showed: the ruler and
// the empty row are what "no cut yet" looks like, and they need the relayout
// as much as a full cut does.
func (ed *cutEditor) clearTracks() {
	ed.vids, ed.segs, ed.undo, ed.redo, ed.base = nil, nil, nil, nil, cutState{}
	ed.fx, ed.fxOn, ed.fxArm = nil, false, ""
	ed.setAspect("")
	ed.sel.active = false
	ed.hasPlay = false
	ed.clearMarks()
	ed.syncButtons()
	ed.relayout() // which redraws every band and re-counts the total
}

func (ed *cutEditor) clearMarks() {
	ed.hasIn, ed.hasOut = false, false
	ed.showMarks()
	// an edge, a clip or an effect held over an Undo or a Revert points into
	// the old cut
	ed.edgeOn, ed.segOn, ed.fxOn = false, false, false
	ed.syncInsertBtn()
	ed.syncSelBtns()
}

func (a *App) addSelClicked() {
	ed := a.ed
	if !ed.sel.active || len(ed.vids) == 0 {
		a.setStatus("drag a region on a track first")
		return
	}
	// Add keeps FOOTAGE, and a sound-scoped selection is not about footage. The
	// button is greyed for this; the guard is for every other way in.
	if ed.sel.aud != "" {
		a.setStatus(fmt.Sprintf("＋ Add keeps footage — the selection is %s's sound", ed.sel.aud))
		return
	}
	// a selection lying in the gap between two recordings, or one shorter than
	// a scene, adds nothing at all. Saying "added" over a cut that did not move
	// -- and leaving an undo step that undoes nothing -- is the one status line
	// that cannot be trusted afterwards, so measure first and say what happened.
	if len(ed.rangePieces(ed.sel.t0, ed.sel.t1)) == 0 {
		a.setStatus(fmt.Sprintf("nothing to add: %.2f s selected, a scene is %.0f s or more",
			math.Abs(ed.sel.t1-ed.sel.t0), minSegLn))
		return
	}
	ed.pushUndo()
	stole := ed.addRange(ed.sel.t0, ed.sel.t1)
	ed.sel.active = false
	ed.clearMarks()
	// with two cameras up, "added" is only half of what happened: the seconds
	// came off whatever camera had them, and a switch nobody was told about is
	// a switch that reads as footage going missing
	switch {
	case stole && ed.laneN > 1:
		a.setStatus(fmt.Sprintf("added on %s, and taken off the other camera — "+
			"↶ Undo (Ctrl+Z) takes it back", ed.camName(ed.sel.lane)))
	case ed.laneN > 1:
		a.setStatus(fmt.Sprintf("added on %s — ↶ Undo (Ctrl+Z) takes it back",
			ed.camName(ed.sel.lane)))
	default:
		a.setStatus("added — ↶ Undo (Ctrl+Z) takes it back")
	}
}

// copyClicked takes the selected stretch of footage in hand. Nothing happens
// to the cut: the selection is measured, remembered, and the Insert button
// turns into ⧉ Paste. The selection itself stays on the band -- taking a copy
// is reading, not editing.
func (a *App) copyClicked() {
	ed := a.ed
	if !ed.sel.active {
		a.setStatus("select a stretch of the pictures or of a lane first — " +
			"⧉ Copy takes the selection in hand")
		return
	}
	t0 := math.Min(ed.sel.t0, ed.sel.t1)
	ln := math.Abs(ed.sel.t1 - ed.sel.t0)
	if ln < minSegLn {
		a.setStatus(fmt.Sprintf("the selection is %.2f s — under %.0f s there is nothing worth copying", ln, minSegLn))
		return
	}
	ed.copyFrom, ed.copyLen, ed.copyOn = t0, ln, true
	// what was drawn on is what is copied: a lane's sound from a lane, footage
	// -- picture and what was filmed with it -- from the pictures. Silencing a
	// pasted stretch is the pasted clip's own question, asked of it by its form
	// (soundOpen), rather than a scope set before the copy was taken.
	ed.copyAud, ed.copyCam = ed.sel.aud, ed.sel.lane
	ed.syncInsertBtn()
	if ed.copyAud != "" {
		a.setStatus(fmt.Sprintf("copied %.1f s of %s (%s – %s) — click where it goes, then ⧉ Paste",
			ln, ed.copyAud, mmss(t0), mmss(t0+ln)))
		return
	}
	a.setStatus(fmt.Sprintf("copied %s – %s (%.1f s) — click where it goes, then ⧉ Paste",
		mmss(t0), mmss(t0+ln), ln))
}

// pasteCopy splices the copied footage into the cut at the red line, as a copy
// segment (see copyScheme): the cut is opened at that point, the copied seconds
// play again, and the footage carries on from the very next frame. Pasting
// consumes the copy -- the button goes back to ⧉ Insert -- because a copy that
// stayed in hand would leave the file chooser unreachable behind a label that
// never changes back; the selection is still on the band, and ⧉ Copy takes it
// again for another paste of the same seconds.
func (a *App) pasteCopy() {
	ed := a.ed
	if !ed.hasPlay {
		a.setStatus("click the timeline where the copy goes first")
		return
	}
	if ed.copyAud != "" {
		a.pasteSound()
		return
	}
	was := ed.cutLen()
	ed.addSplice(fmt.Sprintf("%s%.3f", copyScheme, ed.copyFrom), ed.playhead, ed.copyLen,
		false, ed.copyCam)
	ed.copyOn = false
	ed.syncInsertBtn()
	a.setStatus(fmt.Sprintf("pasted %.1f s from %s at %s — the cut is %s, was %s",
		ed.copyLen, mmss(ed.copyFrom), mmss(ed.playhead), mmss(ed.cutLen()), mmss(was)))
}

// pasteLane is the other place a copy can go: not back into the cut in
// sequence, but onto a row of its own beside the cameras, so the green can
// choose between the two (cut_lane.go).
//
// It is the same seconds of the same file either way. What differs is what the
// timeline then says about them: spliced, they play after the footage they were
// taken from and the video is longer by them; on a lane they play INSTEAD of
// whatever else was rolling, and the video is exactly as long as it was. A
// second angle on a session shot with one camera is this, and nothing else on
// this page can say it.
//
// Nothing is cut to the new row. A lane that arrived already green would have
// made the choice the lane exists to offer.
func (a *App) pasteLane() {
	ed := a.ed
	if !ed.copyOn || ed.copyAud != "" {
		return // the button is only on the bar while footage is in hand
	}
	if !ed.hasPlay {
		a.setStatus("click the timeline where the new lane starts first")
		return
	}
	v := pickVideoOn(ed.vids, ed.copyCam, ed.copyFrom)
	if v == nil {
		a.setStatus(fmt.Sprintf("nothing is rolling at %s any more", mmss(ed.copyFrom)))
		return
	}
	// the FILE second, which is what a lane is a window on: the copy was taken
	// at a session second, and the two differ by wherever that recording sits
	name := ed.addLane(v.path, v.at(ed.copyFrom), ed.playhead, ed.copyLen)
	if name == "" {
		a.setStatus("that copy is too short to be a lane of its own")
		return
	}
	ed.copyOn = false
	ed.syncInsertBtn()
	ed.sel.active = false
	ed.clearMarks()
	a.setStatus(fmt.Sprintf("%.1f s from %s is now the %s lane, starting at %s",
		ed.copyLen, mmss(ed.copyFrom), name, mmss(ed.playhead)))
	ed.redrawTracks()
}

// pasteSound lays the copied sound over the footage at the red line. Sound
// alone, so the picture is left exactly as it was: those seconds keep their
// frames, the video does not get longer by a single one, and the only thing
// that changes is what is heard. That is the other half of ⧉ Paste -- a copy
// of footage is spliced in and lengthens the video, a copy of sound is laid
// over it and does not -- and which of the two this is was settled by the band
// the selection was drawn on, not asked again here.
func (a *App) pasteSound() {
	ed := a.ed
	au := ed.audByBase(ed.copyAud)
	if au == nil {
		a.setStatus(fmt.Sprintf("%s is not in the session any more — the copied sound has "+
			"nowhere to come from", ed.copyAud))
		return
	}
	// where in the FILE those seconds are. A selection that began before the
	// recording did starts the sound where the LANE does -- the file's own
	// beginning for a recording, the window's for a cut lane: there is nothing
	// earlier on that lane to play, and refusing the paste over a second of
	// lead-in nobody selected on purpose would be the worse answer.
	ss := au.at(math.Max(ed.copyFrom, au.start))
	at := ed.playhead
	// the copy stays in hand when it had nowhere to go: the paste did not
	// fail so much as miss, and the answer to missing is to move the red line
	// and press again, not to go and copy the same seconds a second time
	n := ed.addSound(a.storePath(au.path), at, ed.copyLen, ss, ed.copyAud)
	if n == 0 {
		a.setStatus(fmt.Sprintf("the cut keeps no footage at %s — a sound needs a picture under it", mmss(at)))
		return
	}
	ed.copyOn = false
	ed.syncInsertBtn()
	over := "the footage"
	if n > 1 {
		// it crossed a hole in the cut, and saying so is the difference between
		// a puzzling second marker in the lanes and an expected one
		over = fmt.Sprintf("%d stretches of footage", n)
	}
	a.setStatus(fmt.Sprintf("laid %.1f s of %s over %s at %s", ed.copyLen, ed.copyAud, over, mmss(at)))
}

// audByBase is the recording with this base name, or nil when the session no
// longer has it -- which a copy taken before a reload can find.
func (ed *cutEditor) audByBase(base string) *tlAudio {
	for i := range ed.auds {
		if ed.auds[i].base == base {
			return &ed.auds[i]
		}
	}
	return nil
}

// syncSelBtns greys the verbs that have nothing to act on and says why in the
// tooltip: a selection drawn on a wave is sound, and Add/Split/Remove act on
// footage.
func (ed *cutEditor) syncSelBtns() {
	if ed == nil {
		return
	}
	// A verb is live when it has something to act on, grey when not. snd is the
	// exception said in words: a selection on a WAVE is seconds of sound, and Add,
	// Split and Remove act on footage -- the tooltip says why the button is grey.
	snd := ed.sel.active && ed.sel.aud != ""
	on := ed.sel.active && ed.sel.aud == ""
	long := on && math.Abs(ed.sel.t1-ed.sel.t0) >= minSegLn
	if ed.copyBtn != nil {
		ed.copyBtn.SetSensitive(ed.sel.active && math.Abs(ed.sel.t1-ed.sel.t0) >= minSegLn)
	}
	if ed.addBtn != nil {
		// a scene has a floor: a selection under it adds nothing, and a button
		// that reports "nothing to add" is a button that should have been grey
		ed.addBtn.SetSensitive(long)
		tip := "keep the selected region (Undo takes it back)"
		switch {
		case snd:
			tip = "＋ Add keeps footage, and this selection is " + ed.sel.aud + "'s sound"
		case !ed.sel.active:
			tip = "drag a region on a track, then ＋ Add keeps it"
		case !long:
			tip = fmt.Sprintf("the selection is under %.0f s — too short to keep as a scene", minSegLn)
		}
		ed.addBtn.SetTooltipText(tip)
	}
	if ed.splitBtn != nil {
		// the one verb that works with nothing selected: it cuts at the red
		// line instead, so it needs a line or a selection and not both
		ed.splitBtn.SetSensitive(!snd && (on || ed.hasPlay))
		tip := "cut the selected region free: a border at each end, nothing removed, " +
			"so those seconds become a scene of their own. With nothing selected it " +
			"cuts once, at the red line (Undo takes it back)"
		switch {
		case snd:
			tip = "| Split cuts footage, and this selection is " + ed.sel.aud + "'s sound"
		case !on && !ed.hasPlay:
			tip = "click a track to put the red line somewhere, then | Split cuts there"
		}
		ed.splitBtn.SetTooltipText(tip)
	}
	if ed.remBtn != nil {
		ed.remBtn.SetSensitive(on)
		tip := "drop the selected region — through the middle of a scene it " +
			"leaves two, one either side (Undo takes it back)"
		switch {
		case snd:
			tip = "－ Remove drops footage, and this selection is " + ed.sel.aud + "'s sound"
		case !ed.sel.active:
			tip = "drag a region on a track, then － Remove drops it"
		}
		ed.remBtn.SetTooltipText(tip)
	}
}

// insertClicked drops a file into the cut at the playhead: a sting, a still,
// a diagram, an animated tier list -- things a session does not contain. The
// playhead, not the selection: an insert replaces footage rather than choosing
// it. A selected region lends its length.
func (a *App) insertClicked() {
	ed := a.ed
	// the same button opens a held card instead of choosing a new file. Holding
	// one is a statement about what you are working on, and "insert another card
	// at the playhead" is not what anyone means while holding one.
	if s := ed.heldSeg(); s != nil && s.isInsert() {
		a.editInsert()
		return
	}
	// and a held effect the same way: while one is held, the button is its Edit
	if ed.heldFx() != nil {
		a.editFx()
		return
	}
	if !ed.hasPlay && !ed.sel.active {
		a.setStatus("click the timeline where the insert goes first")
		return
	}
	at, want := ed.playhead, 0.0
	// the selection is read HERE and not in the callback, beside the seconds
	// and for the same reason: the chooser is a window the hand can reach
	// around, and the file that comes back has to be placed the way the
	// selection read when the button was pressed.
	lane := ""
	if ed.sel.active {
		at = math.Min(ed.sel.t0, ed.sel.t1)
		want = math.Abs(ed.sel.t1 - ed.sel.t0)
		lane = ed.sel.aud
	}

	// what the chooser admits follows what the selection was drawn on. A
	// selection in a lane is about sound, and offering it a tier card there
	// would be offering to put a picture where the hand pointed at a waveform.
	// Footage, and no selection at all, are offered everything -- what an
	// insert does to the sound is the form's question now, asked of the file
	// that actually comes back (soundOpen).
	title, name, exts := "Insert a clip, image, animation or sound",
		"Video, image, SVG or audio", insExts
	if ed.sel.active && ed.selSnd() {
		title, name, exts = "Insert a sound over the selected seconds", "Audio", audExts
	}
	a.pickFile(title, a.insertDir(), extFilter(name, exts...), func(path string) {
		// Which mode a file arrives in follows the gesture that placed it, and
		// the button is called Insert: a card dropped at the playhead is put
		// BETWEEN the footage, so the video gets longer by it and nothing
		// filmed is lost. A card placed over a SELECTION is the other one --
		// marking seconds and then putting a card there is a sentence that
		// says what those seconds are for -- and it is the selection that gave
		// it its length, so the two answers stay together.
		m := insMode{dur: want, splice: want < minSegLn, lane: lane}
		if m.dur < minSegLn {
			m.dur = a.insertLength(path)
		}
		// what the tick opens on: a file with no sound of its own replaces no
		// sound, so the session carries on under it -- which is the rule the
		// page has always followed ("an insert replaces what it brings, and
		// nothing else"). A file that brought sound arrives bringing it.
		m.mute = !insHasSound(path)
		m.askMute = ed.soundOpen(path, at, m.dur, m)
		// a card is a picture with holes in it, and the holes are the whole
		// point of one: ask before placing it rather than dropping an empty
		// board on the timeline and leaving the filling to a path typed by hand.
		// A file with no holes is asked about too -- how it sits in the cut and
		// how long it runs are questions about a video sting as much as about a
		// card, and a sting placed without being asked is a sting that can only
		// ever overwrite.
		fields, _ := insFields(path)
		a.askInsertParams("Insert", path, fields, m, func(q svgQuery, m insMode) {
			a.placeInsert(path+q.suffix(), at, m)
		})
	})
}

// What the insert chooser admits. audExts is the sound half on its own,
// because a selection drawn in a lane is offered only those. Both lists are
// the extensions insKind sorts by, so what the chooser lets in is exactly what
// the render knows how to ask ffmpeg for.
var audExts = []string{"mp3", "wav", "ogg", "oga", "flac", "m4a", "aac", "opus"}

// picExts is the picture half, offered on its own to a selection scoped to the
// picture alone.
var picExts = []string{"mp4", "mkv", "mov", "webm", "avi", "m4v",
	"png", "jpg", "jpeg", "webp", "bmp", "gif", "svg"}

var insExts = append(append([]string{}, picExts...), audExts...)

// insMode is how an insert sits in the cut: over the footage or between it, and
// for how long. The two are asked for together because they are one decision --
// a card that costs no footage has nothing to take its length from, so the
// seconds have to be said rather than dragged.
type insMode struct {
	splice bool
	dur    float64
	// whether it brings its own sound. The dialog's one tick, in whichever of
	// its two readings the mode is in (askInsertParams): silent when the
	// footage is cut open for it, the session carrying on underneath when it
	// is laid over. See cutSeg.Mute, which is the field it becomes.
	mute bool
	// which recording a sound is being put in place of: the lane the selection
	// was drawn in, read before the chooser opened. Not a dialog question --
	// the hand said it by pointing at that waveform. See cutSeg.Lane.
	lane string
	// a third way for it to sit in the cut, and the only one that adds a ROW
	// rather than a scene: the file goes on a band of its own and the cut
	// reaches it with the green, like a second camera nobody filmed with
	// (cut_lane.go). Video only -- a row is footage, and a still on one would
	// be a card wearing a camera's clothes.
	asLane bool
	// whether mute is a live question for this insert at all, which is what
	// decides if the dialog shows the tick: a picture insert that either
	// brings a sound of its own or lands over seconds that have one. Both
	// answers are then honest readings and only the hand knows which was
	// meant. See cutEditor.soundOpen, which is the whole of the condition.
	askMute bool
}

// placeInsert puts a chosen file in the cut. The path may carry a card's
// parameters, which are kept with it: the file is made relative to the project
// so it survives a move, and the parameters are not a path and are not touched.
func (a *App) placeInsert(ins string, at float64, m insMode) {
	if m.dur < minSegLn {
		m.dur = a.insertLength(ins)
	}
	file, q := insSplit(ins)
	rel := a.storePath(file) + q.suffix()
	was := a.ed.cutLen()
	how := "over the footage — drag its edges to retime it"
	switch {
	case m.asLane:
		// a row, not a scene. Nothing is added to the cut here on purpose: the
		// point of a lane is that the green chooses between it and the cameras
		// beside it, and a lane that arrived already green would have taken
		// that choice (cut_lane.go)
		name := a.ed.addLane(file, 0, at, m.dur)
		if name == "" {
			a.setStatus(fmt.Sprintf("%s is too short to be a lane of its own",
				filepath.Base(file)))
			return
		}
		how = fmt.Sprintf("on a lane of its own (%s) — select on that row and press ＋ Add "+
			"to cut to it; its ✕ takes the row away again", name)
	case m.splice:
		a.ed.addSplice(rel, at, m.dur, m.mute, a.ed.sel.lane)
		how = "between the footage, which is cut open for it"
		if m.mute {
			how = "between the footage, which is cut open for it, and silent — " +
				"the selection was scoped to the picture alone"
		}
	case insKind(file) == "audio":
		// a sound goes in through its own door, because it is the one insert
		// that must leave the picture exactly as it found it (layOverSound)
		n := a.ed.addSound(rel, at, m.dur, 0, m.lane)
		if n == 0 {
			a.setStatus(fmt.Sprintf("the cut keeps no footage at %s — %s is a sound, and one needs a picture under it",
				mmss(at), filepath.Base(file)))
			return
		}
		how = "over the footage, which keeps its frames — drag its edges to retime it"
		if n > 1 {
			how = fmt.Sprintf("over %d stretches of footage, which keep their frames", n)
		}
	default:
		a.ed.addInsert(rel, at, m.dur, m.mute)
		if m.mute {
			how = "over the picture only, and what is heard under it runs on — " +
				"drag its edges to retime it"
		}
	}
	a.ed.sel.active = false
	a.ed.clearMarks()
	// The length of the finished video, said here because this is the one edit
	// whose effect on it cannot be read off the timeline: the timeline is the
	// session's clock and stays exactly as long as the recording, while a card
	// spliced into it makes the VIDEO longer by its own seconds.
	a.setStatus(fmt.Sprintf("%s inserted at %s for %.1f s, %s — the cut is now %s (was %s) "+
		"— ↶ Undo takes it back", filepath.Base(file), mmss(at), m.dur, how,
		mmss(a.ed.cutLen()), mmss(was)))
}

// editInsert opens the held card: what is written on it, whether it plays over
// the footage or between it, and how long it runs. It is the same dialog that
// places one, because those are the same three questions -- and it opens even
// for a card with nothing written on it, since a video sting still has a mode
// and a length.
func (a *App) editInsert() {
	ed := a.ed
	held := ed.heldSeg()
	if held == nil || !held.isInsert() {
		return
	}
	was := *held
	before := ed.cutLen()
	file, q := insSplit(was.Ins)
	path := a.loadPath(file) + q.suffix()
	if was.isCopy() {
		path = was.Ins // not a file: the dialog asks only its mode and seconds
	}
	fields, _ := insFields(path) // no fields is a dialog of mode and seconds
	em := insMode{splice: was.spliced(), dur: was.length(), mute: was.Mute, lane: was.Lane}
	em.askMute = ed.soundOpen(path, was.S, em.dur, em)
	a.askInsertParams("Save", path, fields, em,
		func(q svgQuery, m insMode) {
			// the card is found again rather than remembered: the dialog does
			// not hold the timeline still, and coalesce renumbers
			i := ed.indexOfSeg(was)
			if i < 0 {
				a.setStatus("that card is no longer in the cut")
				return
			}
			ed.applyInsert(i, file+q.suffix(), m)
			how := "over the footage"
			if m.splice {
				how = "between the footage, which is cut open for it"
			}
			a.setStatus(fmt.Sprintf("%s — %.1f s, %s — the cut is now %s (was %s)",
				insBase(was.Ins), m.dur, how, mmss(ed.cutLen()), mmss(before)))
		})
}

// insertDir is where the insert chooser opens: an assets folder beside the
// project, since a card reused across sessions lives with the project rather
// than with any recording. Opening the chooser is also when the built-in cards
// are put there -- there is no other moment where a card is what the user is
// after, and a folder that opens empty teaches that there is nothing to insert.
func (a *App) insertDir() string {
	dir := filepath.Join(a.root, "assets")
	if a.root == "" || !exists(a.root) {
		return a.outDir
	}
	wrote, err := writeSVGCards(dir)
	if err != nil {
		a.logf(">>> assets: %v", err)
		if !exists(dir) {
			return a.outDir
		}
	}
	if len(wrote) > 0 {
		a.logf(">>> wrote the built-in cards to %s: %s", dir, strings.Join(wrote, ", "))
	}
	return dir
}

// askInsertParams fills a card in before it is placed. One entry per parameter,
// and the card says what those are: a tier board asks for its six tiers by name
// and for what has just landed on one of them, and an SVG somebody else wrote
// asks for whatever it declares.
func (a *App) askInsertParams(verb, path string, fields []svgField, m insMode, ok func(svgQuery, insMode)) {
	form := a.cutForm()
	if form == nil {
		return // no page, so no column and no button that could have been pressed
	}
	// a file with nothing to fill in still has the two questions below it, and
	// telling someone about key=value for a video sting is an answer to a
	// question they did not ask
	subText := "What is on the card. It is kept with the insert as " +
		"name.svg?key=value, so the same file serves every session."
	if len(fields) == 0 {
		subText = "How this sits in the cut, and how long it runs."
	}
	sub := gtk.NewLabel(subText)
	sub.SetXAlign(0)
	sub.SetWrap(true)
	sub.AddCSSClass("dim-label")

	grid := gtk.NewGrid()
	grid.SetRowSpacing(6)
	grid.SetColumnSpacing(10)

	var entries []*gtk.Entry
	var done func()

	// the card as the dialog stands: every entry's text under the key it was
	// asked for. An empty row is still a row -- an empty D tier is a statement --
	// but an empty caption is nothing at all and is left unsaid.
	cur := func() svgQuery {
		var q svgQuery
		for i, f := range fields {
			if v := strings.TrimSpace(entries[i].Text()); v != "" || f.Keep {
				q = append(q, svgParam{f.Key, v})
			}
		}
		return q
	}
	// How the card sits in the cut: over the footage (costs the seconds it runs)
	// or between it (cuts the clip open, lengthens the video by the card). Two
	// lines rather than a tick: it is what the card DOES to the footage, not an
	// option.
	between := gtk.NewCheckButtonWithLabel(
		"Insert BETWEEN the footage — the video gets longer by the card, nothing filmed is lost")
	over := gtk.NewCheckButtonWithLabel(
		"Play OVER the footage — the card replaces those seconds (the same as Remove)")
	own := gtk.NewCheckButtonWithLabel(
		"Put it on a LANE of its own — a row of the band to cut to, and nothing is cut yet")
	over.SetGroup(between) // one group is a set of radio buttons
	own.SetGroup(between)
	// only footage can be a row. A row is a recording as far as everything
	// downstream is concerned -- the render cuts stretches of it, the preview
	// seeks in it -- and a card is not a recording, it is a picture the cut
	// puts over one.
	own.SetVisible(insKind(path) == "video")
	between.SetActive(m.splice)
	over.SetActive(!m.splice)
	between.SetTooltipText("The footage is cut at this point, the card plays, and the footage " +
		"carries on with the very next frame. The finished video is longer by exactly the card, " +
		"and its sound is silent under the card and resumes where it stopped.")
	over.SetTooltipText("The card is on screen instead of those seconds of session, which are " +
		"gone from the cut exactly as Remove would take them. The video is no longer than it was.")
	own.SetTooltipText("The file becomes a new row of the picture band, starting at the red " +
		"line, as though a camera nobody set up had been rolling there. Nothing is added to " +
		"the cut by this: select on the new row and press ＋ Add to cut to it, the same way " +
		"you would cut between two cameras. Its ✕ takes the row away again.")
	// what this insert does to the sound: one flag (cutSeg.Mute), read the way
	// the MODE makes true, so the tick says that sentence. A tick, because unlike
	// over-versus-between this is a preference: neither answer eats footage.
	keep := gtk.NewCheckButtonWithLabel("")
	keep.SetActive(m.mute)
	keep.SetVisible(m.askMute)
	// nothing is underneath a row: a lane brings its own picture and its own
	// sound and covers nothing until the cut says so
	syncKeep := func() {
		keep.SetSensitive(!own.Active())
		if between.Active() {
			keep.SetLabel("Play it SILENT — the insert's own sound is not used")
			keep.SetTooltipText("The footage is cut open for this insert, so there is " +
				"nothing underneath it to hear. Ticked, it runs silent; unticked, it " +
				"brings whatever sound it has of its own.")
			return
		}
		keep.SetLabel("Keep the sound running under it — only the picture is replaced")
		keep.SetTooltipText("Ticked, the picture is replaced and everything that was " +
			"audible in those seconds — the capture's own track, every separate " +
			"recording, and this file's own sound if it has one — is decided in " +
			"favour of the session: what was playing carries on underneath. " +
			"Unticked, the insert brings its own sound, or silence if it has none.")
	}
	between.ConnectToggled(syncKeep)
	over.ConnectToggled(syncKeep)
	own.ConnectToggled(syncKeep)
	syncKeep()

	secs := gtk.NewEntry()
	secs.SetText(strings.TrimSuffix(fmt.Sprintf("%.1f", m.dur), ".0"))
	secs.SetMaxWidthChars(5)
	secs.SetWidthChars(5)
	secs.SetInputPurpose(gtk.InputPurposeNumber)
	secs.SetTooltipText("how long the card runs. An inserted card has no edges on the " +
		"timeline to drag, so this is where its length is said.")
	secLbl := gtk.NewLabel("Seconds")
	secLbl.SetXAlign(1)
	secBox := gtk.NewBox(gtk.OrientationHorizontal, 6)
	secBox.Append(secLbl)
	secBox.Append(secs)
	secBox.SetHAlign(gtk.AlignStart)

	// what the two controls say now, with a length that is never zero: a card of
	// no seconds is not a shorter card, it is one nobody ever sees
	mode := func() insMode {
		// lane rides through untouched: which recording a sound insert stands
		// in for is said by the lane the selection was drawn in, before the
		// chooser opened, and nothing in this window asks it again. mute is
		// the tick above, in whichever of its two readings the mode is in.
		out := insMode{splice: between.Active(), asLane: own.Active(), dur: m.dur,
			mute: m.mute, lane: m.lane, askMute: m.askMute}
		if v, err := strconv.ParseFloat(strings.TrimSpace(secs.Text()), 64); err == nil && v >= minSegLn {
			out.dur = v
		}
		if m.askMute && !out.asLane {
			out.mute = keep.Active()
		}
		return out
	}
	done = func() {
		q, md := cur(), mode()
		form.hideForm()
		ok(q, md)
	}
	build := func(fs []svgField) {
		fields, entries = fs, make([]*gtk.Entry, len(fs))
		for i, f := range fs {
			lbl := gtk.NewLabel(f.Label)
			lbl.SetXAlign(1)
			e := gtk.NewEntry()
			e.SetText(f.Val)
			e.SetHExpand(true)
			if f.Hint != "" {
				e.SetPlaceholderText(f.Hint)
				lbl.SetTooltipText(f.Hint)
			}
			entries[i] = e
			grid.Attach(lbl, 0, i, 1, 1)
			grid.Attach(e, 1, i, 1, 1)
			if f.Logo {
				pick := gtk.NewButtonWithLabel("Logo…")
				pick.SetTooltipText("add image files to this tier — a chip is a name, " +
					"a logo, or Name|logo.png for both")
				pick.ConnectClicked(func() { a.pickLogos(&a.win.Window, e) })
				grid.Attach(pick, 2, i, 1, 1)
			}
			e.ConnectActivate(done)
		}
	}
	build(fields)

	secs.ConnectActivate(done)
	insert := gtk.NewButtonWithLabel(verb)
	insert.AddCSSClass("suggested-action")
	insert.ConnectClicked(func() { done() })
	cancel := gtk.NewButtonWithLabel("Cancel")
	cancel.ConnectClicked(func() { form.hideForm() })

	// pinned under the column's scroller, not at the foot of the form: six
	// questions are taller than the panel, and the button that answers them
	// had scrolled off the bottom of it
	btns := gtk.NewBox(gtk.OrientationHorizontal, 8)
	btns.SetHAlign(gtk.AlignEnd)
	btns.Append(cancel)
	btns.Append(insert)

	box := gtk.NewBox(gtk.OrientationVertical, 8)
	box.Append(sub)
	box.Append(grid)
	box.Append(gtk.NewSeparator(gtk.OrientationHorizontal))
	box.Append(between)
	box.Append(over)
	box.Append(own)
	box.Append(keep)
	box.Append(secBox)
	form.showFormFoot(verb+" "+filepath.Base(path), box, btns, nil)
	if len(entries) > 0 {
		entries[0].GrabFocus()
	}
}

// pickLogos adds image files to a list of items, as bare paths, which is a chip
// that is only its logo. Type a name and a bar in front of one to have both.
//
// A logo inside the project is written relative to it and nothing else --
// no project: prefix here, because this path is not read back by loadPath but
// by the card being drawn, which looks beside itself and then in the folder
// above, and that folder is the project (cardLogo). One outside is absolute:
// a card is baked into frames somewhere else entirely, so there is nothing
// else for a relative name to be relative to.
func (a *App) pickLogos(parent *gtk.Window, e *gtk.Entry) {
	d := gtk.NewFileDialog()
	d.SetTitle("Logos for this tier")
	d.SetInitialFolder(gio.NewFileForPath(a.insertDir()))
	filt := gtk.NewFileFilter()
	filt.SetName("Images")
	for _, ext := range []string{"png", "jpg", "jpeg", "webp", "gif", "bmp", "svg"} {
		filt.AddSuffix(ext)
	}
	filters := gio.NewListStore(gtk.GTypeFileFilter)
	filters.Append(filt.Object)
	d.SetFilters(filters)
	d.OpenMultiple(context.Background(), parent, func(res gio.AsyncResulter) {
		list, err := d.OpenMultipleFinish(res)
		if err != nil || list == nil {
			return // dismissed
		}
		items := splitLabels(e.Text())
		for i := uint(0); i < list.NItems(); i++ {
			obj := list.Item(i)
			if obj == nil {
				continue
			}
			f := &gio.File{Object: obj}
			if p := f.Path(); p != "" {
				rel, _ := a.projRel(p)
				items = append(items, rel)
			}
		}
		e.SetText(strings.Join(items, ", "))
	})
}

// insertLength is how long a file wants to be on screen: a video's own length,
// an animation's own length, and a fixed few seconds for a still, which has no
// opinion. Only a default -- the edges are draggable like any other clip's.
func (a *App) insertLength(path string) float64 {
	switch insKind(path) {
	case "video", "audio":
		file, _ := insSplit(path)
		if d, err := ffprobeDur(file); err == nil && d > 0 {
			return d
		}
	case "svg":
		// the card as it will be rendered, parameters and all: a board of eight
		// tiers takes longer to arrive than a board of three, and the length
		// offered here has to be the length of the card actually inserted
		b, _, err := insSVG(path)
		if err != nil {
			break
		}
		if svgHasCSSAnimation(b) && !svgAnimated(b) {
			a.logf(">>> %s: a CSS animation with no @keyframes in the file — drawn as a still",
				insBase(path))
		}
		if root, err := parseSVG(b); err == nil {
			if d := svgDuration(root); d > 0 {
				return d
			}
		}
	}
	return insDefault
}

// revertClicked throws away the hand-made delta and nothing else. Undoing ten
// Adds one at a time is not a workflow, but neither is nuking a suggestion you
// wanted to keep: this returns to the checkpoint -- the last suggestion, or the
// cut the page opened with -- and is itself one ↶ Undo away from coming back.
func (a *App) revertClicked() {
	ed := a.ed
	if sameState(ed.snapshot(), ed.base) {
		a.setStatus("nothing to revert — the cut is as it was")
		return
	}
	was := len(ed.segs)
	ed.pushUndo()
	ed.restore(cutState{append([]cutSeg(nil), ed.base.segs...),
		append([]cutFx(nil), ed.base.fx...), ed.base.aspect,
		copyShift(ed.base.shift), copyRows(ed.base.rows),
		append([]cutLane(nil), ed.base.lanes...), ed.base.nRows})
	ed.sel.active = false
	ed.clearMarks()
	ed.persist()
	switch {
	case len(ed.base.segs) == 0:
		a.setStatus(fmt.Sprintf("reverted — %s gone, the cut is empty", plural(was, "hand-made segment")))
	default:
		a.setStatus(fmt.Sprintf("reverted to the %d segment(s) of the last suggestion "+
			"(↶ Undo brings your edits back)", len(ed.base.segs)))
	}
}

// removeSelClicked drops the selected stretch or, when nothing is selected, the
// single scene under the playhead: clicking a green scene and pressing Remove
// should work without having to rubber-band it first. It never fails silently --
// a button that does nothing and says nothing reads as a missing button.
func (a *App) removeSelClicked() {
	ed := a.ed
	switch i := -1; {
	case ed.heldFx() != nil:
		// same rule as a held clip: what is held is what you are working on
		ed.removeHeldFx()
	case ed.heldSeg() != nil:
		// what is held is what you are working on, and a spliced card cannot be
		// removed any other way: it has no span to select and it is under the
		// playhead at one instant only
		s := *ed.heldSeg()
		ed.pushUndo()
		rest := make([]cutSeg, 0, len(ed.segs))
		rest = append(rest, ed.segs[:ed.segSel]...)
		ed.segs = append(rest, ed.segs[ed.segSel+1:]...)
		ed.dropSeg()
		ed.persist()
		what := fmt.Sprintf("the scene at %s", mmss(s.S))
		if s.isInsert() {
			what = insBase(s.Ins)
		}
		a.setStatus(fmt.Sprintf("removed %s (%.0f s) — ↶ Undo takes it back", what, s.length()))
	case ed.sel.active && ed.sel.aud != "":
		// the selection is a sound's, and ⌦ drops FOOTAGE. Falling through to
		// "the scene under the playhead" would be worse than refusing: it
		// would remove something nobody pointed at.
		a.setStatus(fmt.Sprintf("⌦ drops footage — the selection is %s's sound", ed.sel.aud))
	case ed.sel.active:
		before := len(ed.segs)
		ed.pushUndo()
		ed.removeRange(ed.sel.t0, ed.sel.t1)
		ed.sel.active = false
		ed.clearMarks()
		a.setStatus(fmt.Sprintf("removed — %d segment(s), was %d", len(ed.segs), before))
	case ed.hasPlay:
		if i = ed.segAt(ed.playhead); i < 0 {
			a.setStatus("the playhead is not on a kept scene — click a green one, or drag a region")
			return
		}
		s := ed.segs[i]
		ed.pushUndo()
		ed.segs = append(ed.segs[:i], ed.segs[i+1:]...)
		ed.persist()
		a.setStatus(fmt.Sprintf("removed the scene at %s (%.0f s) — ↶ Undo takes it back",
			mmss(s.S), s.E-s.S))
	default:
		a.setStatus("nothing selected — click a kept scene, or drag a region on a track")
	}
}
