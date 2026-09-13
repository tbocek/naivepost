package main

import (
	"context"
	"fmt"
	"math"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"

	"github.com/diamondburned/gotk4/pkg/gio/v2"
	"github.com/diamondburned/gotk4/pkg/glib/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
	"github.com/diamondburned/gotk4/pkg/pango"
)

// Workflow console for the naivepost pipeline: five steps, one per header tab,
// each gated on the previous one's output, with a shared run bar and log at
// the bottom. One folder per step under the project (prepare/{inputs,describe,
// transcript}, cut, narrate, produce); an older layout is moved once on open
// (migrateFolders). Runs resume where they stopped.
//
//	cd naivepost && ./gui/naivepost-gui

// steps is the pipeline in order; the tab row is this table. label is short
// (five sit in the header bar); icon stands alone when the bar runs out of
// room (headfit.go); wait names the step to finish rather than a number; help
// is the paragraph behind the header ⓘ.
var steps = []struct{ name, label, icon, tip, wait, help string }{
	{"prep", "Prepare", "view-list-symbolic", "The sources, their transcripts, their frames, and what the models make of them", "",
		"Add this session's files — footage, voice recordings, or one screen capture " +
			"that is both. Each row says what its file is for: the camera means frames " +
			"come out of it and it can be cut, and the microphone tags who is speaking, " +
			"1 being the voice the narration is spoken in.\n\n" +
			"Everything in the list is placed on one clock — the timestamp in the file " +
			"name, or the file's own time and length — which is what puts a separate " +
			"recording's words, waveform and sound where they belong against the footage. " +
			"The footage is the master: everything else is drawn, mixed and cut " +
			"against where it falls on the footage, and a recording is heard for the " +
			"part of it that was running while the footage was.\n\n" +
			"Language is what this session is spoken in, told to the speech-to-text " +
			"model: it belongs to the footage, so it is here and in the project rather " +
			"than in Settings, and each project carries its own.\n\n" +
			"One ▶ does the whole step, in the order it has to happen: everything in " +
			"the list is transcribed and a frame is pulled out of the footage every few " +
			"seconds, and then two model jobs read that — the frames are described, and " +
			"the raw transcripts are cleaned up and merged into one timeline covering " +
			"the whole session. The run bar names the job it is on and, after the colon, " +
			"the piece of work it is doing, counted against everything queued so far. " +
			"Which file that piece came from is in the log; hover the bar for how many " +
			"tasks are still waiting.\n\n" +
			"The box on the right is everything the models are told, one row of its " +
			"menu at a time. The first row is not a prompt: it is what you want the " +
			"editor to know about this session — who is in it, how names are spelled, " +
			"what has to end up in the video — and every request this project makes " +
			"carries it. Behind it, in the order the pipeline sends them, is every " +
			"system prompt in the app: this step's two, the cut and its three passes, " +
			"the narration, the upload text. They are here rather than on the pages " +
			"that send them because a prompt is written before the first run and then " +
			"left alone, while those pages are where the session's work happens — and " +
			"reading down the menu is reading the whole run. One wording each, kept on " +
			"this machine once you change it: a ✎ beside a name means what the model " +
			"reads there is not what shipped, and Reset puts the built-in back.\n\n" +
			"This is the long one, so the two run buttons mean different things here. ⏸ " +
			"parks it between requests and ▶ carries on from the same frame; ⏹ ends it, " +
			"and if the describing had started the next ▶ describes the session from the " +
			"beginning. Edit a prompt and it is ⏹ you want — a resumed run would describe " +
			"the rest of the footage under the new wording and leave the first half " +
			"under the old."},
	{"cut", "Cut", "edit-cut-symbolic", "Choose the clips the video is made of",
		"Add footage on the Prepare step first — the cut is laid out from the recordings",
		"The footage over the session timeline, with everything the cut keeps tinted " +
			"green over it, and a waveform lane per sound below. The row above the pictures " +
			"is the cut in bars: a green bar per kept stretch, the one the red line is in " +
			"drawn tall with handles on its ends and an ✕ that drops it — ⌦ does the " +
			"same to whatever is in hand, which is how the cards and the sounds go. A " +
			"selection is OF what it was drawn on: dragged across the pictures it is " +
			"footage, dragged across a waveform it is that recording's sound, and the " +
			"verbs read that. Which rows a SCENE is made of is said on the scene itself " +
			"— take one in hand and every row shows a mark at its left edge: a lens on " +
			"each camera row, lit on the one the scene is shown from, and a speaker on " +
			"each sound row, lit where the scene hears it. Press a lens to show that " +
			"scene from that camera, a speaker to switch that sound off for it. The rule " +
			"the inserts follow is that an insert replaces what it brings and nothing " +
			"else: laid over footage a clip takes the frames, and its form asks what " +
			"happens to the sound — its own, or the session's carrying on underneath. " +
			"Spliced in it replaces nothing at all, because it is time added to the " +
			"session rather than a stretch of it. ▶ below asks " +
			"the model to fill the ▶ target length from what Describe found; from then " +
			"on the cut is yours — drag to select, add, remove, and Revert goes back to " +
			"the suggestion. Once you have edited by hand ▶ says so rather than throwing " +
			"the edits away — Revert first if you want a fresh suggestion. The scroll " +
			"wheel zooms around the cursor.\n\nA left click " +
			"drops the red playhead; the clock under the transport buttons is where it " +
			"landed, in mm:ss.d of session time, and hovering it says which recording " +
			"that is, which frame, and whether the cut keeps it; the ⟦ in and out ⟧ marks " +
			"print their times the same way, in small type under their own buttons. Over " +
			"the picture and the toolbar the wheel steps frames instead of zooming — a " +
			"notch is a frame, five with Shift — and stepping or playing past the view's " +
			"edge scrolls the timeline to put the red line back in the middle.\n\nA clip's own " +
			"edges are trimmed rather than re-selected, and that is two buttons: the " +
			"right one picks a green border up — it turns white — and does nothing else, " +
			"so the choice keeps while your hand moves. Then move it, either by dragging " +
			"the white bar with the left button or a frame at a time with ‹f and f› (the " +
			"arrow keys do the same, Shift for five). The picture follows the edge the " +
			"whole way, so you trim against the frame rather than against a ruler, and " +
			"▶ plays from the edge rather than from the playhead. A left click clear of " +
			"the bar, or a right-click clear of any border, puts it down; the whole trim " +
			"is one Undo.\n\nA whole clip moves the same way, one level up: the right " +
			"button pressed on a clip away from its borders picks up the CLIP — outlined " +
			"in white — and a left drag then slides it with its length intact, snapping " +
			"flush to the clip either side of it; dropped flush against a neighbour from " +
			"the same camera the two become one clip again, which is how a | Split is " +
			"taken back. ‹f and f› nudge it a frame, it never " +
			"leaves the recording it was cut from or overlaps its neighbours, and the " +
			"whole slide is one Undo. Picking anything up leaves the playhead where it " +
			"is — a press near a border takes the border, one away from it takes the clip, " +
			"and neither moves the picture. The preview follows what you MOVE: drag or " +
			"nudge either of them and it lands on the frame you left it on.\n\nA source on Inputs that is not marked as footage gets its own " +
			"lanes under the cut, in blue, one per channel — left and right apart, because " +
			"they are often separate things, a mic on one and the game on the other. A " +
			"stereo file with the same signal on both sides is one lane marked L=R: a mic " +
			"in one input of an interface is one recording however it was written out. The " +
			"video is the master: the timeline is the footage's, and a separate recording is " +
			"placed on it by its own clock (the timestamp in its name, or the file's time and " +
			"length), so it sits where it actually was rather than at the left edge. Only the " +
			"stretch that was running while the footage ran is drawn, over its own lighter " +
			"ground — the minutes before you started the capture card are visibly not there " +
			"rather than stretched to fit. The waveforms appear a moment after the page does; " +
			"each is decoded once and kept.\n\n" +
			"The prompts this step sends — the cut, then the captions, the speed and " +
			"the effects, clip by clip — are on Prepare, in the box that holds every " +
			"prompt in the app. There is one wording each and it assumes nothing about " +
			"the footage: what KIND of video this is, how long it should run and what " +
			"has to be in it come from the User Context beside them, which every step " +
			"is sent and which outranks the wordings.\n\n" +
			"That column is where every form on this page opens — the insert's questions, " +
			"an effect's numbers — rather than in a window over the timeline, so the band " +
			"or the card a question is about stays on screen, and live, while it is " +
			"answered.\n\n" +
			"⧉ Insert drops a file into the cut at the playhead — a title card, a still, an " +
			"\"a few moments later\" clip, an SVG that animates itself. It shows in violet on " +
			"the track, the preview shows it while the red line is inside it — animated cards " +
			"animate — the footage under it gives way exactly as removing it would, and it is " +
			"never merged, trimmed or replaced by a later Suggest: a card is a file, not a " +
			"span. While a card is on the preview the session goes quiet, footage and " +
			"separate recordings alike, because that is the cut Produce makes; an inserted " +
			"video plays its own sound.\n\nThat is one of two modes, and the form asks " +
			"which — for every file, a sting as much as a card: over the footage, " +
			"as above, or between it. Between is what a card dropped at the playhead does, " +
			"since that is what Insert means; over the footage is what a card placed on a " +
			"marked selection does, since marking the seconds first is saying what they are " +
			"for. \"Insert between the footage\" cuts the video at that " +
			"point, plays the card, and carries on, so nothing filmed is lost and the video " +
			"gets longer by exactly the card. Where that shows is the \"cut\" figure under " +
			"the tracks, and in the finished video — not in the timeline, which is the " +
			"session's own clock and stays exactly as long as the recording however much " +
			"is cut out of it or spliced into it. Playing past one holds the footage where it " +
			"is, plays the card through, and lets it go again, which is what the finished " +
			"video does. An inserted card costs no session time, so it " +
			"is drawn as a violet marker hatched like the hole between two recordings " +
			"— the same picture, because it is the same thing: the footage stops here — as " +
			"wide as the card is long at the zoom you are at, so it grows with the clips " +
			"around it, and never narrower than a marker you can hit. Its name and length " +
			"are written beside it, and its seconds are typed in the form rather " +
			"than dragged. Right-click a card to hold it and ⧉ Insert becomes ✎ Edit: the " +
			"same form again, for what the card says, which mode it is in, and how long it " +
			"runs. Switching modes moves the footage back or out of the way to match.\n\n" +
			"A card that animates itself is rendered frame by frame, written either " +
			"way: SMIL (<animate>, <animateTransform>), or a <style> block with @keyframes " +
			"in it — opacity, transform and fill, with the delay and the easing the " +
			"stylesheet asks for. Whatever the file does not cover is drawn as it stands, " +
			"and anything Produce could not read says so in the log.\n\n" +
			"The project's assets folder starts with cards this app draws itself: tier.svg is " +
			"the S/A/B/C/D/F board, and s.svg, a.svg, b.svg, c.svg, d.svg, f.svg are single " +
			"letters that fly in. The board arrives with two of those letters flying onto it, " +
			"which is the one thing a still cannot show you and is what Just added below does. " +
			"They are filled in when you insert them — the board is the six tiers, S A B C D " +
			"F, and one line each: type \"Dust II, Mirage\" into Tier S and it is a red row " +
			"with two chips in it. A chip can be a " +
			"name, a logo, or both: \"Logo…\" beside a tier adds picture files to it, and " +
			"\"Dust II|logos/dust2.png\" is the name under the logo. A tier holds six; a " +
			"seventh is not drawn and the log says so. The last line, Just " +
			"added, is what the shot is about: the board you have already shown is put back up " +
			"quickly and these fly in on their own afterwards. A bare name (\"Mirage\") points " +
			"at an item already in a row above. \"D[1.1s]: logos/bla.svg, Test\" is the other " +
			"form — into row D, starting 1.1 seconds in (or 1100ms), the logo with Test under " +
			"it — and the list is comma separated, so several things can be sent in one after " +
			"another on a beat you choose. Leave the line empty and the whole board arrives at one " +
			"pace, as it always did. What you type is kept on the end of the path " +
			"(tier.svg?S=Dust II, Mirage&A=Nuke), so the same file is " +
			"a different board every time you place it and the cut says which. tier.svg is also " +
			"the file it is drawn from, and it is a board when you open it: every row and all " +
			"six places in it are written out with their own coordinates, and the {{holes}} in " +
			"them are what you typed and the timings the arrivals get — restyle that file and " +
			"every board drawn from it comes out restyled. " +
			"Your own SVG can " +
			"do the same: put {{name}} or {{name|default}} in it and Insert asks for each one, " +
			"and add <!-- Input: name | Label | what it is --> beside it to have the form ask " +
			"in your words rather than in the placeholder's. assets/CARDS.md is written into " +
			"the folder with the cards and says all of it — the canvas, the placeholders, and " +
			"how an animated one has to be written — for whoever, or whatever, writes the next one.\n\n" +
			"The aspect dropdown beside those buttons is for a video that is not the footage's " +
			"shape — 9:16 for a short, 1:1, 4:5 — and \"source\" is the absence of the choice: " +
			"nothing below applies until it is changed, and a cut that never touches it renders " +
			"exactly as it always did. With one chosen, the height set on Produce names the " +
			"output's height and the aspect names its width, and the video starts on the whole " +
			"frame, black either side, because throwing footage away is a choice you make, not " +
			"one made for you — until the first view: the moment one exists, the video is " +
			"framed on it from the very start, wherever on the timeline it was placed.\n\n" +
			"The four effects live in the ✚ Effect dropdown beside the aspect. " +
			"▭ View is that choice: jump anywhere, pick it, and draw on " +
			"the picture itself the region to show from there on. The rectangle keeps the cut's " +
			"aspect while you drag, snaps flush when you reach the full width or height of the " +
			"frame, and may be smaller — a crop, zoomed in — or larger, the frame with more " +
			"black around it. The form then asks the one number: 0 switches the camera at " +
			"that instant, seconds glide it over. Views chain — each one starts from wherever " +
			"the camera is at that moment, mid-glide included — and the camera stays until the " +
			"next one. ⊕ Zoom is a free drawing with a return ticket: any rectangle, not bound " +
			"to the aspect — the camera closes in on the smallest output-shaped window that " +
			"holds it — in over its own glide, held for its length, and back out over a separate " +
			"glide to wherever the views say the camera belongs. " +
			"⏩ Speed needs no drawing: it puts the seconds marked with in and out on a clock " +
			"of their own — type the rate, a fifth for slow motion or up to 100× to run " +
			"through the dead air of a long recording, the sound stretched or squeezed with " +
			"the picture and the pitch kept. ⏸ Stop holds the frame under " +
			"the playhead still for a time you type, which plays like a spliced card the " +
			"session never filmed.\n\nEvery effect is a mark on the lane under the picture band — orange " +
			"flags for views, teal brackets for zooms, rose bands for speed, rose also washed " +
			"over the footage that is off its own clock — and the marks answer to the timeline's own " +
			"verbs: the right button holds one (white) and on the held one opens its numbers, " +
			"a left drag slides it, ‹f and f› " +
			"nudge it a frame, ✎ Edit reopens its numbers, ⌦ deletes it, Esc puts it " +
			"down, and all of it shares the one Undo with the cut. A held view or zoom shows " +
			"its rectangle on the picture: draw beside it to re-frame it, or grab inside it " +
			"and slide it whole, its edges snapping onto the frame's top, left, right and " +
			"bottom as they come close; the right button on or inside it asks its times " +
			"again — a view's transition, a zoom's length and separate glides in and out — " +
			"except on the first view, which has none to ask. Paused, the preview draws " +
			"the camera as an outline over the full frame rather than cropping the picture — " +
			"you frame against everything the camera could see; playing, it zooms for real, " +
			"glides and all. It does not slow or " +
			"freeze; the \"cut\" figure under the tracks counts that added time, and " +
			"Produce renders it."},
	{"narrate", "Narrate", "audio-input-microphone-symbolic", "The narration, and the voice it is spoken in",
		"Finish Cut first — narration is written for the cut's clips",
		"▶ below is the initial fill: it writes the narration once (again only if the " +
			"cut moves) and speaks every line not cached yet. After that the lines are " +
			"yours: each box is \"[emotion] words\" with its start time beside it — edit " +
			"the words, the delivery, or when the line begins — a time inside another " +
			"clip moves the line to that clip, and one outside the cut is refused with " +
			"the line left where it was. ＋ beside the transport " +
			"adds a line at the paused second, 🗑 removes one, and an edited line is " +
			"simply re-spoken the first time it plays. The voice is cloned from a " +
			"sample, picked under the video; the slider and the picture play the cut, speaking each " +
			"line where it was placed, with the blue row following along. The bar is " +
			"the cut end to end — what the edit removed takes up no room on it, so " +
			"every point on it is video you keep, and the time beside it reads " +
			"\"session clock · how far into the finished video\". The cut is " +
			"the master: once it is rolling it keeps rolling, clip after clip, and " +
			"nothing moves the picture but you. Picking a line — clicking its row, or " +
			"its ▶ — starts three seconds before that line so you can watch it land, " +
			"and then the preview simply carries on down the cut. A clip the narration " +
			"left alone keeps its row and its ▶ too: that one plays the clip from the " +
			"top on its own audio, which is how you decide whether it wants a line. " +
			"A line whose time you retype into another clip leaves that row behind it, " +
			"empty. ⏪ and ⏩ " +
			"jump three seconds; stopped, the wheel over the slider steps one frame " +
			"at a time, which is the resolution a line's placement is judged at. The " +
			"playhead only lands on material the cut kept: dropped into a gap it " +
			"carries on the way it was going, to the head of the next clip or back " +
			"to the tail of the one behind.\n\nA take " +
			"is stable: the same words, delivery and voice always come back as the " +
			"same performance, so nothing you have approved changes behind you. " +
			"When a delivery is wrong rather than the words, the row's ↻ re-rolls " +
			"it — same line, a new draw — and plays the result.\n\nThe TTS blends eight base emotions — " +
			"happy, angry, sad, afraid, disgusted, melancholic, surprised, calm — " +
			"and maps whatever is in the [brackets] onto them. One or two of those " +
			"words work best; \"loud\" or \"fast\" is not an emotion and only dilutes " +
			"the match (anger already shouts, calm is already slow). Length lives in " +
			"the words themselves: more short exclamations, not stretched letters, " +
			"which get spelled out.\n\nPlain words are read by a small judge model, " +
			"which is forgiving but approximate. Write a weight and it is skipped, " +
			"the mix going to the engine exactly as asked: \"[angry=1]\" is pure " +
			"anger at full force, \"[happy=0.8, surprised=0.4]\" a blend you chose. " +
			"Weights run 0 to 1, and the difference is audible: \"[surprised]\" " +
			"is the judge's reading of the word, \"[surprised=1]\" the axis " +
			"itself at full force.\n\nBeside the eight there are named mixes of " +
			"them, which take a weight the same way: excited, ecstatic, playful, " +
			"proud, relieved, hopeful, tender, nostalgic, solemn, awed, alarmed, " +
			"horrified, desperate, confused, frustrated, bitter, contemptuous, " +
			"dismayed, heartbroken, ominous, tense. \"[excited=1]\" is happiness " +
			"with surprise mixed into it, which is what excitement sounds like " +
			"and what plain happiness does not. Any name none of these knows sends the " +
			"line back to the judge rather than guessing at an axis."},
	{"produce", "Produce", "applications-multimedia-symbolic", "Write the upload text, draw the thumbnail and render the video",
		"Finish Cut first — there is no cut to produce a video from",
		"Everything the upload needs, from one ▶: the first press writes the " +
			"title, the thumbnail instruction and the description; every press " +
			"draws the thumbnail; and then the final video is rendered — every " +
			"clip cut from its own recording, the narration laid over ducked game " +
			"audio, the whole thing loudness-normalized to -14 LUFS for YouTube. " +
			"Lines that have not been synthesized yet are spoken first.\n\nThe " +
			"model is not asked for the text again after the first run, however " +
			"much you edit the boxes: rewording the edit instruction or swapping " +
			"the images costs GPU time and no thinking. Deleting the publish folder " +
			"is what starts the text over; Suggest again, beside the title, " +
			"rewrites it without redrawing or rendering.\n\nThe thumbnail is " +
			"usually an edit of one of your own frames, which is what keeps it " +
			"recognizably this video rather than a stock illustration of the " +
			"genre. The first image in the row is the picture being edited; the " +
			"others are references the instruction can name by position — \"put " +
			"the ship from the second image behind them\" — and Make base " +
			"promotes one to the front. Empty the row and it is drawn from the " +
			"instruction alone. Write the instruction as instructions, not as a " +
			"description of a picture: say what to change, and everything you do " +
			"not mention stays as it is. The title is lettered into the picture " +
			"by the image model — four or five words survive being shrunk to a " +
			"phone's sidebar, and an empty title means no lettering.\n\nA " +
			"session's separate recordings are in the sound as well as in the " +
			"picture: whatever was running while a clip was running is mixed into " +
			"it, from the second of that recording the clip actually falls on — " +
			"the same placement the blue lanes on Cut are drawn from, with the " +
			"footage as the master. It joins the game audio rather than the " +
			"narration, so \"Game audio under voice\" ducks both together and " +
			"the spoken lines still sit on top. Cards are left silent: a card is " +
			"time added to the cut, not a moment of the session, so there is " +
			"nothing that was said under it."},
}

// stepLocked reports whether a tab's prerequisites are missing. Prepare
// never is -- it is where the sources are added, so a project with nothing in
// it still has to be able to open it -- and an unknown name (there is none, but
// the lookup can fail) counts as open rather than as locked: refusing to show a
// page is the worse mistake.
func (a *App) stepLocked(i int) bool {
	if i < 0 || i >= len(steps) {
		return false
	}
	switch steps[i].name {
	case "cut":
		return a.cutLocked
	case "narrate":
		// choosing a voice lives on this tab and therefore waits for a cut,
		// which it does not need. That is deliberate: it belongs beside the lines
		// it will speak, and hearing a sample before there is anything to narrate
		// decides nothing you cannot decide again afterwards.
		return a.narrateLocked
	case "produce":
		return a.produceLocked
	}
	return false
}

func stepIndex(name string) int {
	for i, s := range steps {
		if s.name == name {
			return i
		}
	}
	return -1
}

// showStep moves to a page without going through the tab's click handler --
// for the bounce off a locked tab, and for sending a page whose prerequisites
// vanished back to the start.
func (a *App) showStep(name string) {
	// whatever was half-typed on the page being left is on disk before it is
	// left: the narration boxes write on a beat after the typing stops, and
	// clicking a tab is exactly the moment that beat has not come yet
	a.narr.flushSave()
	a.tabGuard = true
	for i, s := range steps {
		a.tabs[i].SetActive(s.name == name)
	}
	a.tabGuard = false
	a.stack.SetVisibleChildName(name)
	// the two groups on the shared bar are this page's
	a.inStack.SetVisibleChildName(name)
	a.outStack.SetVisibleChildName(name)
	a.updateRunControls() // ▶ ⏹ belong to the new page's playback now
	a.syncHelp()          // and so does the ⓘ
	// ...and so does ▶ itself, when the ticks beside it name one step: the tab
	// you walk to is the step you mean (followChainTick)
	a.followChainTick(name)
	// Cut's Inputs row lists what Suggest will be sent, and one of those things
	// is the context box on Describe -- the page you have usually just come
	// from. Refreshed on arrival rather than on every keystroke over there,
	// which would re-read the session timeline as you type.
	// ...and the tracks themselves, when the last run (or another project) moved
	// what they are drawn from. Only then: a rebuild probes every recording and
	// drops the undo history, which is not what a tab click should cost.
	if name == "cut" && a.ed != nil {
		if a.ed.stale || len(a.ed.vids) == 0 {
			a.updateCutInfo() // which does updateInputs itself
		} else {
			a.ed.updateInputs()
		}
	}
	// Narrate's row says the same thing about the narration, and one of the
	// things it counts is the cut -- which is the page you have just come from
	// and the one thing most likely to have changed under it.
	if name == "narrate" && a.narr != nil {
		a.narr.refit() // a clip dragged wider over there moves its line's slot here
		a.narr.updateInputs()
		a.narr.updateOut()
	}
	// ...and Produce reads both of those, plus the synthesis cache and the file
	// it wrote last time. It is the end of the chain, so everything upstream is
	// something that may have moved since it was last looked at.
	if name == "produce" {
		a.updateProduceInfo()
		// ...and the thumbnail half reads the folder it wrote into last time,
		// since the picture on the page is the file on disk rather than
		// something remembered
		if a.pub != nil {
			a.pub.refresh()
		}
	}
}

// syncHelp points the header ⓘ tooltip at the open page's help. Called from
// showStep, including the bounce off a locked tab.
func (a *App) syncHelp() {
	if a.helpInfo == nil {
		return
	}
	i := stepIndex(a.stack.VisibleChildName())
	if i < 0 {
		return
	}
	a.helpInfo.SetTooltipText(steps[i].label + "\n\n" + steps[i].help)
}

type App struct {
	root   string // naivepost directory (the scripts and project files live here)
	vidDir string // where the last video came from; where a chooser opens
	audDir string // same, for a recording -- neither is what the list is made of
	outDir string // where step outputs go; default root, project-settable

	win   *gtk.ApplicationWindow
	stack *gtk.Stack
	// one tab per entry in steps, in that order; they gray out with a hint
	tabs          []*gtk.ToggleButton
	tabGuard      bool          // suppresses re-entrant toggles while reverting
	cutLocked     bool          // no session timeline yet -- nothing to cut
	narrateLocked bool          // no cut yet -- nothing to narrate
	produceLocked bool          // no cut yet -- nothing to produce
	logExp        *gtk.Expander // collapsed until something actually runs
	helpInfo      *gtk.Image    // the ⓘ in the header bar, retooltipped per page by syncHelp
	log           *gtk.TextView
	linkTag       *gtk.TextTag      // paths in the log that open on a click (logPath)
	linkPaths     map[string]string // what a tagged path displays -> where it really is
	llmMu         sync.Mutex        // guards llmSeq; describe calls from worker goroutines
	llmSeq        int               // per-run counter naming the llm/ exchange files
	// whether this video has a narration at all. The Narrate page's own
	// checkbox writes it, the run refuses when it is set, and Produce hides
	// what only a narration needs (narrate.go, produce.go).
	narrOff bool
	// sources are referenced in place rather than copied into the project
	// (Project.RefSources); copyTick is the tick on Prepare that says so
	refSources bool
	refQuiet   bool // applyRefSources is setting the tick, not the user
	copyTick   *gtk.CheckButton
	// the run's own page: its file name, the finished calls' HTML in order, and
	// how many there have been. One page per run rather than one per call --
	// what happened is a thread through several calls, and a directory listing
	// is not a thread (llmlog.go).
	runName    string
	runSecs    []string
	runN       int
	status     *gtk.Label
	running    bool
	audioNoted string // the audio.cpp server already reported in the log
	ttsModel   string // the model id that server serves, asked for once
	// which aligner answered. A catalog entry is a claim rather than a working
	// model (alignModels) -- a server lists one whose family the engine it is
	// running was not built with -- so the run remembers the one that worked
	// instead of spending a failed request per source to learn it again.
	alignPick string

	// The Prepare page's own controls -- the settings a run reads off it,
	// which is everything on that page a runner needs and nothing it draws.
	srcList   *sourceList // the session's files, and what each one is for
	interval  *freqPick
	scalePick *gtk.DropDown
	// which pipeline this session runs through (textedit.go): the page's
	// dropdown, and the project's word for it before the page exists
	// the steps ▶ runs one after another (runchain.go)
	chainPick  *gtk.MenuButton
	chainTicks map[string]*gtk.CheckButton
	chain      []string // what is left of the chain under way
	chainSteps []string // ...and a project's answer, before the ticks exist
	// how long it is all taking: when the press was, when the step under way
	// started, which step that is, and what each finished one cost
	chainFrom  time.Time
	chainAt    time.Time
	chainStep  string
	chainRan   []string
	chainQuiet bool
	// set while the CHAIN is moving the pages itself (chainNext), so that the
	// move is not read as the hand walking to a tab (followChainTick)
	chainMoving bool

	stylePick  *gtk.DropDown
	videoStyle string     // which pipeline this session runs through; guarded by promptMu, like langTxt
	styleQuiet bool       // applyStyle is setting the dropdown, not the hand
	langEntry  *gtk.Entry // what the ASR is told this session is spoken in

	// One controller per page, in tab order (steps). Each is nil until its page
	// has been built, which is what every headless test is and also what the
	// first seconds of a session are -- so every reader checks.
	prep *preproc
	ed   *cutEditor
	narr *narrator
	prod *producer
	pub  *publisher

	// The Narrate page's voice picker, and the choice it holds. The id is
	// cached from narrate/voice.txt and guarded: since the narrator's name became
	// part of every step's input (tlLabel), it is read by the describe and
	// transcript workers as well as by the GUI thread.
	voicePick *voicePicker
	voiceMu   sync.Mutex
	voiceSel  string
	pitchSel  float64 // semitones the reference is shifted by, from narrate/pitch.txt
	pitchRead bool    // ...and whether that file has been read yet (0 is a real value)
	// the hand-picked voice-clone takes, by recording (narrate_take.go), under
	// the same lock and for the same reason: they are part of the cache key, so
	// they are read by whatever thread is about to speak a line.
	takesMap  map[string][]voiceTake
	takesRead bool

	progress *gtk.ProgressBar
	playBtn  *gtk.Button
	stopBtn  *gtk.Button
	// the visible step's Inputs line and Outputs group, each page adding its
	// own by step name. The two live side by side at the right of the shared
	// bottom bar: what the step reads, then what it has written.
	inStack  *gtk.Stack
	outStack *gtk.Stack

	// pipeline control: pause parks the runners at the next checkpoint, stop
	// kills the in-flight subprocesses; finished stages stay on disk either
	// way. STT and frame extraction run as parallel tracks (GPU vs CPU), so
	// several subprocesses can be live at once.
	ctlMu     sync.Mutex
	curCmds   map[*exec.Cmd]bool
	stopFlag  atomic.Bool
	pauseFlag atomic.Bool
	runCtx    context.Context // canceled by stop -- aborts in-flight LLM calls
	runCancel context.CancelFunc
	// ⏸ parks a run, ⏹ abandons one, and the difference has to be visible on
	// the next ▶: resume where it stopped, or start over. Only the describer
	// can tell them apart at all -- it is the one job that resumes per chunk --
	// so a stopped Describe + Transcript sets this, and the next run of that
	// step throws its half-written event logs away first. See resetDescribe.
	undRestart bool

	srcMu sync.Mutex
	// snapshot of the session's sources, taken on the GUI thread when a run
	// starts. selItems is the whole of it and the other three are what the
	// pipeline actually asks for, derived from it: the footage, then the rest,
	// and who was tagged as which narrator. They are kept together because a
	// run can rewrite the session -- splitting a voice off a recording turns
	// one row into two -- and every one of them has to say so at once.
	selItems []sourceItem
	selVid   []string
	selAud   []string
	selNarr  [narratorSlots]string
	// which audio tracks of each source the session uses, keyed on path. A path
	// this says nothing about uses its first track alone, so an ordinary
	// session's map is empty and every reader of it is a no-op (cut_tracks.go).
	selTracks map[string][]int

	// the editable system prompts. The views are the GUI thread's; promptTxt is
	// the copy a runner reads, kept current by the buffers' changed handler --
	// same rule as selVid/selAud, for the same reason.
	promptMu    sync.Mutex
	promptTxt   map[string]string
	promptViews map[string]*gtk.TextView
	// promptTxt is empty for a job whose shipped wording is what the model
	// reads; what is in it is this machine's own. promptDisk is what the
	// prompt folder is believed to hold, so the flush that mirrors one onto
	// the other writes only what changed (promptstore.go) -- GUI thread only,
	// like the views: it is written from the autosave tick.
	promptDisk map[string]string
	// the row above each box -- the "edited" mark and the Reset beside it --
	// and the flag that stops filling a box from reading as the user typing in
	// it. GUI thread only, so no lock (showPrompt).
	promptRows  map[string]promptRow
	promptQuiet bool
	// redraws the ✎ marks on the bench's menu when a project load or an edit
	// changes what the project is holding (prepedit.go)
	prepSync func()
	// every text box's heading row, so a row with a Reset button and a row with
	// a bare label are the same height and the boxes under them line up
	// (editorBody). GUI thread only, like the views.
	headGroup *gtk.SizeGroup
	// what langEntry says, for the runner to read; guarded like ctxTxt below and
	// for the same reason
	langTxt string
	// what the editor says about THIS session, typed on Prepare and read by
	// every step (context.go). Under promptMu for the same reason as the
	// prompts: the box belongs to the GUI thread, the string is what a runner
	// reads. Not in promptTxt -- it is not a prompt and it is stored in full,
	// whereas a prompt is stored only when it differs from the built-in.
	ctxTxt string
	// the bench's box while it is showing the context row, and nil while it is
	// showing a prompt (prepedit.go). Nil is normal, not a "page not built
	// yet": a project loaded while a prompt is on screen writes the cache
	// only, and the box rereads it on the next switch.
	ctxView *gtk.TextView

	// The project file and what was last written to it. projPath is never empty:
	// a session with no name of its own is root/session.naivepost, and outDir is
	// derived from whichever of the two it is (project.go). Both are the GUI
	// thread's. projLabel is projPath's name in the header bar; nil under the
	// tests, which build an App without a window. openPath is a file the desktop
	// handed us to open -- a double-click -- and is read once, by build().
	projPath  string
	projSaved []byte
	projLabel *gtk.Label
	openPath  string

	// the header bar and the parts of it that give up words when the window is
	// narrow (headfit.go). headBtns is the six icon buttons at the two ends --
	// they never change size, so what they take is simply subtracted; tabWords
	// is the word inside each tab, hidden as a group when only the icons fit.
	// tabWordsOn says which of the two states the row is in, because the row's
	// own measurement cannot tell us and both states have to be priced from
	// either one. All GUI-thread, like every widget here.
	head       *gtk.HeaderBar
	headBtns   []gtk.Widgetter
	tabRow     *gtk.Box
	tabWords   []*gtk.Label
	tabWordsOn bool
	headFitQ   bool
	headWatch  bool

	// one progress bar fed by both tracks: summed fractions, and the work each
	// of them still has queued (runqueue.go)
	progMu    sync.Mutex
	progParts [2]float64
	// where in the bar the running phase is drawn, for the one press that is
	// two steps (qPhase). Share 0 is the whole bar, which is every other press.
	progBase, progShare float64
	progQ               [2]qTrack
}

// The frame slider snaps to these stops; a linear 0.1..5 s scale would cram
// all the useful low end into the first pixel. Index 0 keeps every frame.
var frameStops = []float64{0, 0.1, 0.2, 0.5, 1, 2, 3, 4, 5}
var frameStopLabels = []string{"each", "0.1", "0.2", "0.5", "1s", "2s", "3s", "4s", "5s"}

// where a session with nobody's opinion on it starts: one frame a second. Named
// rather than typed twice, since a new project has to land on the same stop the
// stepper builds itself at -- and index 0 is not it (that is every frame, which
// is gigabytes).
const defFrameStop = 4

// Frame size presets, no-resize first because that is the default and picking a
// size is the exception. Name is the identity and Label is only what the drop
// down shows: the name goes into the project file AND into the stamp that
// decides whether frames must be extracted again, so it has to stay put even
// when the wording moves. 896w is the width the vision model gets fed.
var scalePresets = []struct{ Name, Label, VF string }{
	// the label said "Resize", which on the closed button read as the
	// control's caption and in the open list as a nonsense first choice
	{"original", "Original", ""},
	{"896w (LLM)", "896w (LLM)", "scale=896:-2"},
	{"480p", "480p", "scale=-2:480"},
	{"720p", "720p", "scale=-2:720"},
	{"1080p", "1080p", "scale=-2:1080"},
}

func (a *App) frameScale() (name, vf string) {
	i := int(a.scalePick.Selected())
	if i < 0 || i >= len(scalePresets) {
		i = 0
	}
	return scalePresets[i].Name, scalePresets[i].VF
}

func (a *App) setFrameScale(name string) {
	for i, p := range scalePresets {
		if p.Name == name || p.Label == name {
			a.scalePick.SetSelected(uint(i))
			return
		}
	}
}

// defLanguage is what a session is assumed to be in when nobody says. It was a
// setting on this machine (llm.conf) and is now a field on the project, for the
// reason setup.go's header gives: the stack is the same every night, the
// footage is not.
const defLanguage = "en"

// projectLanguage is the box's text as the project file stores it: what was
// typed, trimmed, and "" when nothing was -- an empty box means the default,
// and writing "en" into every project would freeze today's default into files
// that never asked for it (same rule as an unedited prompt).
func (a *App) projectLanguage() string {
	a.promptMu.Lock()
	defer a.promptMu.Unlock()
	return strings.TrimSpace(a.langTxt)
}

// asrLanguage is the same value with the default filled in: what Prepare puts in
// the request. Callable from a runner's goroutine, which is the whole reason
// the string is cached beside the widget rather than read off it -- the box is
// the GUI thread's (same rule as sessionCtx).
func (a *App) asrLanguage() string {
	if s := a.projectLanguage(); s != "" {
		return s
	}
	return defLanguage
}

func (a *App) setLanguage(s string) {
	a.promptMu.Lock()
	a.langTxt = s
	a.promptMu.Unlock()
	// the subtitle translations are offered in every language but this one,
	// and this is where it changes (syncLangs)
	a.syncSubLangs()
}

// applyLanguage loads a project's language into the box as well as the cache.
// GUI thread only, like applySessionCtx and for the same reason.
func (a *App) applyLanguage(s string) {
	a.setLanguage(s)
	if a.langEntry != nil {
		a.langEntry.SetText(s)
	}
}

func (a *App) frameInterval() float64 { return frameStops[a.interval.i] }

func (a *App) setFrameInterval(v float64) { a.interval.set(nearestStop(v)) }

// nearestStop maps seconds onto the stop table -- a project may store a value
// from a build whose table was different, and typing is free-form.
func nearestStop(v float64) int {
	best, bd := 4, math.MaxFloat64 // default 1 s
	for i, s := range frameStops {
		if d := math.Abs(s - v); d < bd {
			best, bd = i, d
		}
	}
	return best
}

// freqPick is the frame-frequency stepper: one stop visible at a time, - / +
// walking the table, and the text editable by hand. Hand-rolled rather than a
// GtkSpinButton because the spin button re-reads its own display as a plain
// number before every step -- and "each" and "1s" are not numbers, so every
// click first corrupted the value it was about to step.
type freqPick struct {
	box   *gtk.Box
	entry *gtk.Entry
	i     int
}

func newFreqPick() *freqPick {
	f := &freqPick{}
	f.entry = gtk.NewEntry()
	f.entry.SetWidthChars(4)
	f.entry.SetMaxWidthChars(4)
	f.entry.SetAlignment(0.5)
	f.entry.SetTooltipText("Seconds between frames — type 0.1 to 5, or 'each' for every frame")
	f.entry.ConnectActivate(f.parse)
	// clicking elsewhere commits like Enter does: a half-typed value silently
	// kept on screen would not be the value a run then uses
	fc := gtk.NewEventControllerFocus()
	fc.ConnectLeave(f.parse)
	f.entry.AddController(fc)

	minus := gtk.NewButtonFromIconName("list-remove-symbolic")
	minus.ConnectClicked(func() { f.set(f.i - 1) })
	plus := gtk.NewButtonFromIconName("list-add-symbolic")
	plus.ConnectClicked(func() { f.set(f.i + 1) })

	f.box = gtk.NewBox(gtk.OrientationHorizontal, 0)
	f.box.AddCSSClass("linked") // one control, three parts
	f.box.Append(f.entry)
	f.box.Append(minus)
	f.box.Append(plus)
	f.set(defFrameStop)
	return f
}

// set clamps to the table -- past either end the buttons simply stop moving.
func (f *freqPick) set(i int) {
	if i < 0 {
		i = 0
	}
	if i >= len(frameStops) {
		i = len(frameStops) - 1
	}
	f.i = i
	f.entry.SetText(frameStopLabels[i])
}

// parse commits a typed value: seconds with or without the s, or "each"/"0"
// for every frame, landing on the nearest stop. Anything unreadable snaps the
// display back to the value still in force.
func (f *freqPick) parse() {
	t := strings.TrimSuffix(strings.ToLower(strings.TrimSpace(f.entry.Text())), "s")
	if t == "each" {
		f.set(0)
		return
	}
	v, err := strconv.ParseFloat(t, 64)
	if err != nil || v < 0 {
		f.set(f.i)
		return
	}
	f.set(nearestStop(v))
}

// Where each step writes: folders named for their steps (step1/..step6/ once;
// migrateFolders moves them). Prepare's three jobs are under its name --
// prepare/inputs/, prepare/describe/, prepare/transcript/.
func (a *App) prepareDir() string    { return filepath.Join(a.outDir, "prepare") }
func (a *App) inputsDir() string     { return filepath.Join(a.prepareDir(), "inputs") }
func (a *App) describeDir() string   { return filepath.Join(a.prepareDir(), "describe") }
func (a *App) transcriptDir() string { return filepath.Join(a.prepareDir(), "transcript") }
func (a *App) narrateDir() string    { return filepath.Join(a.outDir, "narrate") }
func (a *App) produceDir() string    { return filepath.Join(a.outDir, "produce") }

// framesDir is where the first half of Prepare leaves one video's frames:
// the describer reads them and the Cut page waits for them, so the path is
// named once.
func (a *App) framesDir(base string) string {
	return filepath.Join(a.inputsDir(), "frames", base)
}

// canCut is what the Cut page needs: frames out of a source marked as footage
// -- not session.tsv. A silent capture or a session nobody wants described is
// cut by hand; what Describe adds is text ON the timeline, and having none is
// an empty track, not a locked page.
// canCut is whether there is anything to lay on the tracks: one source marked
// as footage, and nothing more.
//
// It used to be "somebody has extracted frames", which locked the page until
// Prepare had run. But a recording is a lane before it is a transcript: its
// length, its shape and its sound are in the file itself, and laying the
// session out -- seeing where the takes fall against each other, shifting one
// by hand, hearing it -- is work that comes BEFORE describing anything. The
// frames are the pictures drawn on the lane, and a lane without them is a lane
// with no pictures on it, not a page that cannot be opened.
//
// What still needs Prepare is the SUGGESTION, and suggestClicked says so in
// the one place it is true: "run Describe first -- the suggestion reads the
// session timeline, and there is none".
func (a *App) canCut() bool {
	vids, _ := a.snapSources()
	return len(vids) > 0
}

func main() {
	a := &App{curCmds: map[*exec.Cmd]bool{}}
	wd, _ := os.Getwd()
	if _, err := os.Stat(filepath.Join(wd, "input_video")); err != nil {
		// started elsewhere: the binary lives in <root>/gui
		exe, _ := os.Executable()
		wd = filepath.Dir(filepath.Dir(exe))
	}
	a.root = wd
	a.vidDir = filepath.Join(wd, "input_video")
	a.audDir = filepath.Join(wd, "input_audio")
	// A session always has a file, before anyone saves one (see projExt). Set
	// here rather than through setProject, which refreshes pages that do not
	// exist yet; from build() on, setProject is the only thing that moves it.
	a.projPath = filepath.Join(wd, workName)
	a.outDir = dataDir(a.projPath)

	// HandlesOpen is the double-click. Without the flag gio treats a file
	// argument as an error and refuses to start; with it, the desktop's "open
	// with Naivepost" arrives at ConnectOpen and a bare launch still goes to
	// ConnectActivate. GApplication is single-instance, so a double-click while
	// a window is up is forwarded to this same process rather than starting a
	// second one -- which is why open loads into the window that exists instead
	// of building another.
	app := gtk.NewApplication(appID, gio.ApplicationHandlesOpen)
	app.ConnectActivate(func() {
		a.build(app)
		a.startHangWatch() // a stuck main loop writes its own stacks (hangwatch.go)
	})
	app.ConnectOpen(func(files []gio.Filer, _ string) {
		var path string
		if len(files) > 0 {
			path = files[0].Path() // one window, one project: the rest are ignored
		}
		if a.win == nil {
			a.openPath = path
			a.build(app)
			return
		}
		if path != "" {
			a.loadProjectFrom(path)
		}
		a.win.Present()
	})
	os.Exit(app.Run(os.Args))
}

// ---- state -----------------------------------------------------------------

// loadMeta reads inputs/meta.env: the primary video and recording of the last
// run, and the frame settings it ran with. Its existence is also the marker
// that Prepare has run at all -- either of the two files may be missing from a
// legitimate session, so the keys are not that marker.
func (a *App) loadMeta() map[string]string {
	m := map[string]string{}
	b, err := os.ReadFile(filepath.Join(a.outDir, "inputs", "meta.env"))
	if err != nil {
		return m
	}
	for _, line := range strings.Split(string(b), "\n") {
		if k, v, ok := strings.Cut(line, "="); ok {
			m[k] = v
		}
	}
	return m
}

// snapSources caches the session's sources for background runners, which must
// never touch the list widget. Split by role: the footage, then everything
// else; every source is in exactly one of the two.
func (a *App) snapSources() (vids, auds []string) {
	if a.srcList == nil {
		return a.snappedSources()
	}
	return a.snapItems(append([]sourceItem(nil), a.srcList.items...))
}

// snapItems takes one list of sources as the run's snapshot and hands back the
// two path lists the pipeline works from. It goes through a sourceList of its
// own rather than reading the fields: what counts as footage and who holds a
// narrator slot are that type's rules, and the run must not answer either
// question differently from the page.
func (a *App) snapItems(items []sourceItem) (vids, auds []string) {
	l := sourceList{items: items} // the rules, without the widget
	vids, auds = l.split()
	var narr [narratorSlots]string
	for n := 1; n <= narratorSlots; n++ {
		narr[n-1] = l.narratorPath(n)
	}
	// only the rows that answered, so the map is empty for every session that
	// has no multi-track file in it and nothing downstream has to tell "the
	// first track" from "no answer" a second time
	tracks := map[string][]int{}
	for _, it := range items {
		if len(it.tracks) > 0 {
			tracks[it.path] = append([]int(nil), it.tracks...)
		}
	}
	a.srcMu.Lock()
	a.selItems, a.selVid, a.selAud, a.selNarr, a.selTracks = items, vids, auds, narr, tracks
	a.srcMu.Unlock()
	return
}

// snappedTracks is the per-file track choice from the run's snapshot, for the
// background work that must not touch the list widget. Nil before any snapshot
// has been taken, which reads as "nobody chose anything" and is the truth.
func (a *App) snappedTracks() map[string][]int {
	a.srcMu.Lock()
	defer a.srcMu.Unlock()
	return a.selTracks
}

func (a *App) snappedSources() (vids, auds []string) {
	a.srcMu.Lock()
	defer a.srcMu.Unlock()
	return a.selVid, a.selAud
}

// snappedItems is the whole snapshot, for the one thing that rewrites it.
func (a *App) snappedItems() []sourceItem {
	a.srcMu.Lock()
	defer a.srcMu.Unlock()
	return append([]sourceItem(nil), a.selItems...)
}

// sepWanted is every source still waiting for its voice to be lifted off, from
// the snapshot -- so the runner can ask without touching a widget.
func (a *App) sepWanted() []string {
	a.srcMu.Lock()
	defer a.srcMu.Unlock()
	l := sourceList{items: a.selItems}
	return l.sepVoiceWanted()
}

// narratorPath is the recording tagged with slot n, from the snapshot -- so a
// runner can ask who narrator 2 is without touching a widget.
func (a *App) narratorPath(n int) string {
	if n < 1 || n > narratorSlots {
		return ""
	}
	a.srcMu.Lock()
	defer a.srcMu.Unlock()
	return a.selNarr[n-1]
}

// voiceSource is the recording the narration is cloned from: narrator 1, or --
// for a session nobody has tagged -- the same guess the pipeline made before
// the tags existed, the first recording, and failing that the first source at
// all, which is the single-video session.
func (a *App) voiceSource() string {
	if p := a.narratorPath(1); p != "" {
		return p
	}
	vids, auds := a.snappedSources()
	if len(auds) > 0 {
		return auds[0]
	}
	if len(vids) > 0 {
		return vids[0]
	}
	return ""
}

// ---- UI --------------------------------------------------------------------

func (a *App) build(app *gtk.Application) {
	// activate fires again when a second launch forwards to this instance;
	// building twice would spawn extra windows that start playing on their own
	if a.win != nil {
		a.win.Present()
		return
	}
	a.win = gtk.NewApplicationWindow(app)
	a.win.SetTitle("Naivepost")
	// the few styles of our own: the no-timestamp flag on a source row, and the
	// settings dialog's test verdicts. Plain GTK only promises the semantic
	// warning/success classes on a handful of widgets, so the colors are stated
	// here rather than hoped for from the theme.
	css := gtk.NewCSSProvider()
	css.LoadFromData(".stamp-warn { color: #e5a50a; } " +
		".test-ok { color: #26a269; } .test-bad { color: #c01c28; } " +
		// the bars a preview's aspect leaves over, in the color every other
		// player puts there instead of the page background (see videoFrame)
		".videoframe { background-color: #101010; } " +
		// a slider's own reading, in the colour of a reading. The theme draws
		// the value over the handle and the numbers under the marks in a
		// dimmed foreground, which on a live control is the app's own way of
		// saying "greyed out" -- so the one number a slider exists to report
		// looked like a setting that could not be changed.
		"scale value, scale marks label { color: @theme_fg_color; } " +
		// the boxes you type in get the corner the entries and buttons have: every
		// one is a scrolled window with the frame class (editorFrame), which the
		// theme draws square. The timeline stays square: a band is a measurement
		// (platePath).
		".frame { border-radius: 6px; } .frame textview, .frame textview text { border-radius: 6px; } " +
		// ...and one font in everything you type in. The boxes set it on
		// themselves (SetMonospace); an entry has no such switch, so the one
		// line above a monospace box -- a title, a server, a model id, a
		// number in an effect form -- came out in the proportional font, and
		// the two rows of one form disagreed about what typed text looks
		// like. Said once, here, for every entry in the app.
		"entry, entry text { font-family: monospace; }")
	gtk.StyleContextAddProviderForDisplay(gtk.BaseWidget(a.win).Display(), css,
		gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
	// fits a 1366x768 laptop with room for the panel: every page is either
	// scrolled or split by a divider, so a bigger screen is worth more space
	// but no page depends on having it
	a.win.SetDefaultSize(1240, 740)

	head := gtk.NewHeaderBar()
	a.head = head
	// Symbols, like everything else in this bar; the tooltip says what the icon
	// means. New, then Open, then Save: the order every application uses.
	newP := gtk.NewButtonFromIconName("document-new-symbolic")
	newP.SetTooltipText("New project — name it, put it where you want it, and start over")
	newP.ConnectClicked(a.newProjectDialog)
	loadP := gtk.NewButtonFromIconName("document-open-symbolic")
	loadP.SetTooltipText("Load a project — sources, prompts and settings")
	loadP.ConnectClicked(a.loadProjectDialog)
	saveP := gtk.NewButtonFromIconName("document-save-symbolic")
	saveP.SetTooltipText("Save this project to a file")
	saveP.ConnectClicked(a.saveProjectDialog)
	head.PackStart(newP)
	head.PackStart(loadP)
	head.PackStart(saveP)
	// Which project this session is written to: the whole path when the bar has
	// room, the file name when it costs the tabs their words (fitHeader).
	// Ellipsized so a long name cannot push the centred tabs off centre.
	a.projLabel = gtk.NewLabel("")
	a.projLabel.SetEllipsize(pango.EllipsizeEnd)
	a.projLabel.AddCSSClass("dim-label")
	a.projLabel.SetMarginStart(6)
	head.PackStart(a.projLabel)
	a.showProject()
	// every step needs a rescan after files change on disk, so it lives here
	rescan := gtk.NewButtonFromIconName("view-refresh-symbolic")
	rescan.SetTooltipText("Rescan inputs and outputs")
	rescan.ConnectClicked(a.rescanAll)
	head.PackEnd(rescan)
	setup := gtk.NewButtonFromIconName("preferences-system-symbolic")
	setup.SetTooltipText("Settings — the LLM and audio.cpp endpoints")
	setup.ConnectClicked(a.setupDialog)
	head.PackEnd(setup)
	// what this step does, hovered rather than clicked (syncHelp): a mark that
	// looks like every other ⓘ in the app has to behave like them
	info := gtk.NewImageFromIconName("help-about-symbolic")
	info.SetMarginStart(6)
	info.SetMarginEnd(6)
	a.helpInfo = info
	head.PackEnd(info)
	// what the bar spends on things other than the tabs and the project name
	a.headBtns = []gtk.Widgetter{newP, loadP, saveP, rescan, setup, info}
	a.win.SetTitlebar(head)

	// run controls exist BEFORE the pages: page builders refresh their info
	// texts during construction, and those touch the shared progress bar
	// ▶ is play and pause both -- it draws itself from what is under way, and
	// what is under way may be a run or this page's playback. See transport.
	a.playBtn = gtk.NewButtonFromIconName("media-playback-start-symbolic")
	a.playBtn.AddCSSClass("suggested-action")
	a.playBtn.SetTooltipText("Run the ticked steps — or resume what is paused")
	a.playBtn.ConnectClicked(a.playClicked)
	a.stopBtn = gtk.NewButtonFromIconName("media-playback-stop-symbolic")
	a.stopBtn.SetTooltipText("Stop the run or the playback — ⏸ is what parks one to carry on later")
	a.stopBtn.SetSensitive(false)
	a.stopBtn.ConnectClicked(a.stopClicked)
	a.progress = gtk.NewProgressBar()
	// no text on the bar. It drew its line above the trough, so an idle page
	// carried a sentence over an empty bar at the far LEFT of the window --
	// "prepared (1 frame set(s))", "nothing running" -- while everything else
	// this bar says lives at its right. The bar is a fraction now, and what
	// the run is doing goes to the status line down there with it (showProg).
	a.progress.SetShowText(false)
	// the bar reads "describe 1/2: chunk 4/12": the job, which of the run's jobs
	// it is, and where the work queue has got to. Which file is in the log --
	// the tooltip a run replaces this one with counts the tasks instead
	a.progress.SetTooltipText("The run: the job, which of the run's jobs it is, and the task it is on")
	a.progress.SetHExpand(true)
	a.progress.SetVAlign(gtk.AlignCenter)

	// What the visible step has written -- how many files, newest when, and a
	// way into the folder. Every step answers that, and each used to answer it
	// on a row of its own at the page's bottom edge: five copies of the same
	// line, each costing its page a line of height. The answer lives once now,
	// at the right end of the shared bottom bar, and follows the visible tab
	// the way ▶ and ⏹ do. Built here with the other run controls because the
	// pages fill it as they are built.
	a.outStack = gtk.NewStack()
	a.outStack.SetHhomogeneous(false) // prep's three folders must not set the width for every tab
	// ...and what it READS, beside it. That line was a row at the top of every
	// page: four copies of the same heading, each costing its page a line of
	// height above the work, and each answering a question about the run --
	// which is what this bar is for. Inputs left of Outputs, in the order the
	// step happens.
	a.inStack = gtk.NewStack()
	a.inStack.SetHhomogeneous(false)

	a.stack = gtk.NewStack()
	a.stack.SetTransitionType(gtk.StackTransitionTypeCrossfade)
	// each page asks for its own size, not for the largest page's. Homogeneous
	// (the GTK default) means the timeline on the cut step sets the floor for
	// every other page too, and the whole window inherits it -- so a page that
	// would fit a small screen on its own is clipped because a page you are not
	// looking at would not.
	a.stack.SetHhomogeneous(false)
	a.stack.SetVhomogeneous(false)
	a.stack.SetVExpand(true)
	a.stack.AddNamed(a.buildPrep(), "prep")
	a.stack.AddNamed(a.buildCut(), "cut")
	a.stack.AddNamed(a.buildNarrate(), "narrate")
	a.stack.AddNamed(a.buildProduce(), "produce")

	// Tabs, not a sidebar down the left. The steps are a fixed five, so a list
	// spent 170px of width on five rows and a column of empty space under them;
	// as a group of toggle buttons they fit the header bar, which is already
	// there -- the page gets the width back and gives up no height for it.
	// Hand-rolled rather than a GtkStackSwitcher, because a tab has to be able
	// to gray out and say what is missing, which the stock widget cannot do.
	tabRow := gtk.NewBox(gtk.OrientationHorizontal, 0)
	tabRow.AddCSSClass("linked") // one segmented control, not five loose buttons
	a.tabRow, a.tabWordsOn = tabRow, true
	for i, st := range steps {
		// icon and word, the word being the part that goes when the bar runs
		// short (fitHeader). The icon is always drawn, so the row never
		// changes how many things are in it -- only how wide they are, which
		// is what keeps a tab in the same place across a resize.
		i, b := i, gtk.NewToggleButton()
		row := gtk.NewBox(gtk.OrientationHorizontal, tabGap)
		row.Append(gtk.NewImageFromIconName(st.icon))
		word := gtk.NewLabel(st.label)
		row.Append(word)
		b.SetChild(row)
		a.tabWords = append(a.tabWords, word)
		b.SetTooltipText(st.tip)
		if i > 0 {
			b.SetGroup(a.tabs[0]) // radio behaviour: some page is always current
		}
		b.ConnectToggled(func() {
			// the other half of every switch is a tab going inactive, and the
			// bounce below re-toggles two more -- only the one being entered acts
			if a.tabGuard || !b.Active() {
				return
			}
			if a.stepLocked(i) {
				a.showStep(a.stack.VisibleChildName()) // bounce: the page did not move
				// the same sentence the tooltip carries: a click is how you find
				// out the tab is closed, so it must not answer with less than
				// hovering it would have
				a.setStatus(steps[i].wait)
				return
			}
			a.showStep(steps[i].name)
		})
		a.tabs = append(a.tabs, b)
		tabRow.Append(b)
	}
	head.SetTitleWidget(tabRow)
	a.watchHeadWidth()

	// shared log + status across all pages: one bottom row, the status text
	// living in the expander header so nothing reserves empty space
	var logScroll *gtk.ScrolledWindow
	a.log, logScroll = newLogPane(220)
	a.status = gtk.NewLabel("")
	a.status.SetXAlign(1) // status lives right-aligned in the free header space
	a.status.SetHExpand(true)
	a.status.SetEllipsize(pango.EllipsizeEnd)
	a.status.AddCSSClass("dim-label")
	a.status.SetMarginEnd(8)
	logLbl := gtk.NewLabel("Log")
	head2 := gtk.NewBox(gtk.OrientationHorizontal, 12)
	head2.SetHExpand(true)
	head2.Append(logLbl)
	head2.Append(a.status)
	a.logExp = gtk.NewExpander("")
	a.logExp.SetLabelWidget(head2)
	a.logExp.SetChild(logScroll)
	a.logExp.SetMarginStart(8)
	a.logExp.SetMarginEnd(8)
	a.logExp.SetMarginTop(2)
	a.logExp.SetMarginBottom(2)

	// the shared bottom bar: run controls act on the visible step
	ctlRow := gtk.NewBox(gtk.OrientationHorizontal, 8)
	margins(ctlRow, 4, 2, 8, 8)
	// ▶ and the menu of what it runs are one control (buildChainMenu), so ▶
	// is appended by it rather than here
	ctlRow.Append(a.buildChainMenu())
	ctlRow.Append(a.stopBtn)
	// No volume slider on this bar. It was here for one page: Produce, which
	// used to watch its own result and had no transport of its own to hang a
	// slider off. Produce no longer plays anything -- the finished file is
	// opened in whatever plays videos on this machine -- so the bar's slider
	// was a control over silence on every page in the app. The two pages that
	// do play have their own beside their own ▶ (cut.go, narrate.go), and
	// those are still one number between them (volumeCtl, SetPreviewVolume).
	ctlRow.Append(a.progress)
	// No Improve button here. It asked the model why a step decided what it
	// did and offered edits to the prompts that would change it -- taken out
	// with its prompt and its cards; the prompts are edited on Prepare.
	// the one Outputs heading in the app; the group behind it is the visible
	// page's own, swapped by showStep
	inLbl := gtk.NewLabel("Inputs:")
	inLbl.AddCSSClass("heading")
	ctlRow.Append(inLbl)
	ctlRow.Append(a.inStack)
	outLbl := gtk.NewLabel("Outputs:")
	outLbl.AddCSSClass("heading")
	ctlRow.Append(outLbl)
	ctlRow.Append(a.outStack)

	bottom := gtk.NewBox(gtk.OrientationVertical, 0)
	// a hard edge above the control/log rows, so the page ending there reads as
	// a status bar rather than as widgets stopping mid-air
	bottom.Append(gtk.NewSeparator(gtk.OrientationHorizontal))
	bottom.Append(ctlRow)
	bottom.Append(a.logExp)

	// the first page is chosen here rather than beside the tabs it moves,
	// because showStep dresses this bar for the page it opens -- the Outputs
	// group and the volume slider are both on the row built just above
	a.showStep("prep")

	// The log against the page above it, on a divider. How much log you want is
	// a per-moment question -- all of it while a run talks, none of it while
	// cutting -- and it was a fixed 220 px that the page had to live around.
	// Only the page takes the window's extra height; the log keeps whatever it
	// was dragged to, and cannot be dragged over the run controls.
	outer := gtk.NewPaned(gtk.OrientationVertical)
	outer.SetStartChild(a.stack)
	outer.SetEndChild(bottom)
	outer.SetResizeStartChild(true)
	outer.SetResizeEndChild(false)
	outer.SetShrinkEndChild(false)
	// Two halves, and both are needed. The expander and its scroller have to be
	// told to fill what the divider gives them, or the log sits at its natural
	// height under an empty stretch of window (same trick as the settings
	// dialog). And collapsing it hands the height back rather than leaving the
	// divider parked where a log nobody can see used to be -- position(-1) is
	// how a GtkPaned is told to forget a dragged position.
	logGrow := func() {
		on := a.logExp.Expanded()
		logScroll.SetVExpand(on)
		a.logExp.SetVExpand(on)
		if !on {
			outer.SetPosition(-1)
		}
	}
	a.logExp.NotifyProperty("expanded", logGrow)
	logGrow()
	a.win.SetChild(outer)
	// after the log exists, so that a theme that cannot find the icon says so
	// somewhere the user will look rather than only on stderr
	a.setupIcons()

	// The config left the session folder; a machine that has been cutting for
	// months still has it there. Also after the log, because the one thing it
	// does that is worth seeing is the line saying where the file went.
	a.migrateConf()
	a.loadGlobalPrompts()

	// Pick up where the last session left off. A file handed over by the desktop
	// comes first -- a double-click is somebody asking for THAT project, not for
	// whatever was open last -- then the named project that was open when the
	// last session ended, and only failing that the working copy. Opening the
	// working copy unconditionally was the old behaviour, and it silently undid
	// the Save that named a variant: you saved jan-video, quit, and came back to
	// the session you had saved it to get away from.
	switch {
	case a.openPath != "":
		a.loadProjectFrom(a.openPath)
	case a.lastProject() != "":
		a.loadProjectFrom(a.lastProject())
	case exists(a.projPath):
		a.loadProjectFrom(a.projPath)
	case exists(filepath.Join(a.root, "project.json")):
		// The working copy from before a project was a file you double-click.
		// Its contents are this session and its name is the one a session has
		// now (projectName); project.json is left on disk exactly as it was.
		// What it wrote is NOT moved: it went into the root, which also holds
		// the checkout, and no rename could tell one from the other.
		a.loadProjectFrom(filepath.Join(a.root, "project.json"))
	}

	a.updateGates()
	a.updateCutInfo()
	a.updateProduceInfo()
	a.startAutosave() // from here on the project file follows the window
	a.win.SetVisible(true)
}

// updateGates greys the tabs whose prerequisites are missing and puts the
// reason where the description usually is; a locked tab bounces back. Greyed
// rather than insensitive, since an insensitive button gets no hover.
func (a *App) updateGates() {
	if len(a.tabs) == 0 {
		return
	}
	// the cut works on the footage and its frames, which is the first half of
	// Prepare's output. Describe's session timeline is what it prints on
	// the tracks and what the suggestion reads, and a session can be cut by
	// hand without it
	a.cutLocked = !a.canCut()
	// ...and nothing else is locked. Narrate and Produce used to wait for a
	// cut to exist, which is true of RUNNING them and not of opening them:
	// the resolution, the container, which languages the subtitles are
	// translated into and whether there is a narration at all are answers you
	// give BEFORE the run, and a tab that cannot be opened is a setting that
	// cannot be reached until the thing it governs has already happened.
	// Their own ▶ refuses with "no cut yet" (narrateRun, produceRun), which
	// is the honest place for that to be said.
	a.narrateLocked, a.produceLocked = false, false
	for i, s := range steps {
		w := gtk.BaseWidget(a.tabs[i].Child()) // the label, so the button keeps its frame
		if a.stepLocked(i) {
			w.AddCSSClass("dim-label")
			a.tabs[i].SetTooltipText(s.wait)
		} else {
			w.RemoveCSSClass("dim-label")
			a.tabs[i].SetTooltipText(s.tip)
		}
	}
	if a.stepLocked(stepIndex(a.stack.VisibleChildName())) {
		a.showStep("prep")
	}
}

func (a *App) rescanAll() {
	// the list is the session, so a rescan cannot re-read it from a folder --
	// what it can do is notice that a source is no longer on disk. Say which:
	// a row that quietly disappeared is how a render comes out missing an angle.
	for _, gone := range a.srcList.prune() {
		a.logf("!!! dropped %s -- it is no longer there", gone)
	}
	a.updateGates() // which re-reads what is on disk
	a.prep.refresh()
	a.updateCutInfo()
	a.updateNarrateInfo()
	a.updateProduceInfo()
	a.setStatus("rescanned")
}

// updateNarrateInfo re-reads the narration from disk. It was the one step a
// rescan skipped: delete narrate/ and the page went on showing the narration it
// had in memory and an Outputs line counting files that were no longer there.
// Nothing is lost by re-reading -- every edit on that page is written as it is
// typed -- and after a rescan the folder is the answer, including when the
// answer is that there is nothing in it.
func (a *App) updateNarrateInfo() {
	n := a.narr
	if n == nil || n.list == nil {
		return // page not built yet
	}
	n.load()
	n.rebuildRows()
	n.updateInputs()
	n.updateOut()
}

// ---- helpers ---------------------------------------------------------------

// setStatus is the answer to a click -- what an edit did, or why nothing -- in
// the log expander's header, held until the next press. Not where a run
// reports (the bar does), and it does not repeat the header bar's project.
func (a *App) setStatus(s string) {
	if a.status == nil {
		return // headless (tests): the status line is the window's
	}
	a.status.SetText(s)
}

// newLogPane is what every log looks like: read-only monospace, wrapping, in a
// framed scroller. Shared by the run log and the settings test log; minHeight
// is the one thing they disagree on.
func newLogPane(minHeight int) (*gtk.TextView, *gtk.ScrolledWindow) {
	tv := gtk.NewTextView()
	tv.SetEditable(false)
	tv.SetCursorVisible(false) // read-only: a blinking caret in it is a lie
	tv.SetMonospace(true)
	tv.SetWrapMode(gtk.WrapWordChar) // paths and ffmpeg lines are long
	sw := gtk.NewScrolledWindow()
	sw.SetChild(tv)
	sw.SetMinContentHeight(minHeight)
	sw.AddCSSClass("frame")
	return tv, sw
}

// logf writes one line to the run log and the terminal. A log line reports: a
// name, a count, a size, a failure. Explanations belong on the page, in a
// tooltip or in the source, and per-task progress on the bar (prog).
func (a *App) logf(format string, args ...any) {
	fmt.Fprintf(os.Stderr, format+"\n", args...) // mirror to the launching terminal
	if a.log == nil {
		return // called before the window exists: stderr is the whole log
	}
	buf := a.log.Buffer()
	end := buf.EndIter()
	buf.Insert(end, fmt.Sprintf(format, args...)+"\n")
	mark := buf.CreateMark("", buf.EndIter(), false)
	a.log.ScrollToMark(mark, 0, false, 0, 1)
	buf.DeleteMark(mark)
}

func (a *App) logfIdle(format string, args ...any) {
	glib.IdleAdd(func() { a.logf(format, args...) })
}

// A watchdog for the GTK thread: a heartbeat on the main loop, watched by its
// own goroutine. When the beat is hangStall late, every goroutine's stack goes
// to hang-<time>.txt beside the settings and to stderr -- the Go frames above
// the blocked C call say which seek or state change never came back. One dump
// per hang.

const (
	hangBeat  = 200 * time.Millisecond // how often the main loop says it is alive
	hangStall = 3 * time.Second        // how late a beat has to be to count as a hang
)

// startHangWatch installs the heartbeat and the watcher. Called once the main
// loop exists; harmless before the window does.
func (a *App) startHangWatch() {
	var last atomic.Int64
	last.Store(time.Now().UnixNano())
	glib.TimeoutAdd(uint(hangBeat/time.Millisecond), func() bool {
		last.Store(time.Now().UnixNano())
		return true
	})
	go func() {
		dumped := false
		for range time.Tick(hangBeat) {
			late := time.Since(time.Unix(0, last.Load()))
			if late < hangStall {
				dumped = false
				continue
			}
			if dumped {
				continue
			}
			dumped = true
			a.dumpHang(late)
		}
	}()
}

// dumpHang writes every goroutine's stack. Not through the log: the log is a
// widget on the thread that is stuck.
func (a *App) dumpHang(late time.Duration) {
	buf := make([]byte, 1<<20)
	n := runtime.Stack(buf, true)
	head := fmt.Sprintf("naivepost: the GTK thread has not answered for %s -- every goroutine's stack follows\n\n", late.Round(100*time.Millisecond))
	fmt.Fprint(os.Stderr, head)
	os.Stderr.Write(buf[:n])
	dir := configDir()
	if dir == "" {
		return
	}
	p := filepath.Join(dir, "hang-"+time.Now().Format("0102-150405")+".txt")
	os.WriteFile(p, append([]byte(head), buf[:n]...), 0o600)
	fmt.Fprintf(os.Stderr, "\nnaivepost: written to %s\n", p)
}
