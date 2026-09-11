package main

// The system context: what every job is told once about the tool, the material
// and the answer it owes -- the [mm:ss] stamps, EVENT vs SPEAKER lines, the
// machine-read reply, never invent, what the four steps ARE so a job writes
// for the next one. Facts about the tool, true for every job; a wording is
// only the part that could differ. The user context is deliberately NOT here:
// its rules travel with it (ctxBlock, ctxRule), so an empty box sends none.
//
// Second row of the bench on Prepare, editable, the one prompt with no
// wordings (promptDef.solo). Goes in the SYSTEM message; the session context
// goes in the USER message (context.go). One paragraph or bullet per line,
// unwrapped: see describeSystem.

import (
	"regexp"
	"strings"
)

const sysSystem = `You are called by an automated video editor, one job per call.

THE ANSWER
Read by a machine, not a person: exactly what the job asks for and nothing around it -- no markdown, no code fence, no preamble, no report of what you did. Where a job asks for JSON, strict JSON is the whole answer; where it asks for lines or columns, their number and order are part of it.

THE MATERIAL
One session -- pictures and microphones, all on one clock -- written as stamped lines:

  [724s | 12:04] EVENT: what the picture showed in those seconds, and whether it was hectic or calm
  [727s | 12:07] SPEAKER_01: something said out loud, which the video plays
  [731s | 12:11] NARRATOR: something said on a microphone the video does not play, so only a job that uses it is heard

A clip's block may also carry MARKED: the editor's own name for a moment in it -- not something said and not something the picture showed, but what they call it.

THE CLOCKS
Every line is stamped, and the request says which clock. Answer on the same one.

  [724s | 12:04] session time, the same instant written twice: the seconds, then mm:ss from the session start with minutes counting past 59. Times you return on this clock are the SECONDS, and you take them off the line -- never work them out from the mm:ss, and never answer with an mm:ss.
  [+2.0s] an offset from the start of what the request is about -- these frames, this clip. Negative is before it.
  A bare number in a column is seconds on one recording's own timeline: copied, never recomputed.

THE FOUR STEPS
  Prepare writes those lines: frames described into EVENT lines, microphones transcribed into SPEAKER and NARRATOR lines. No later step sees the footage -- from here the lines ARE the session.
  Cut picks the segments the video is made of. What is captioned over each, how fast each plays and what is drawn on each are asked for after it, clip by clip.
  Narrate writes the voice-over over those segments. Each clip keeps its own sound underneath.
  Produce writes the upload text, draws the thumbnail, renders the video.

The finished video is those segments played one after another, so it has a clock of its own: a time in the video is not a time in the session.

THE CUT
A segment is a stretch of session seconds: chronological, never overlapping, each boundary in the gap between two lines, and not all the same length -- what is on screen decides, never an average.

The segments are chosen first and are not changed afterwards. What is written over each clip, how fast it plays and what is drawn on it are three later jobs, each given the clips and answering in the clip's own seconds. An effect belongs to one clip; one outside every clip is thrown away.

WHAT EACH JOB IS GIVEN, AND WHAT IT ANSWERS WITH -- nothing around the answer:

  describe: a few frames, each after a line "[+2.0s] FRAME 3 of 4" on the same clock as the speech around them; the running STATE from the chunk before; the last EVENT lines. Answers two lines:
    EVENT: Calm; the tower fires into the crowd and a health bar empties.
    STATE: On the second map, defending the left lane with the new tower.
  transcript: a context block, then N lines of TSV -- start, end, speaker, text. Answers exactly those N lines in order, start, end and speaker copied character for character and only the text changed. No line merged, split, dropped, added or emptied; no tabs in the text; no line numbers. Any difference in count, order, times or speakers discards the block.
  retake: every spoken line of the session, numbered, each with its own seconds, the pause in front of it and the seams between recordings. Answers ABANDONED, strict JSON and nothing else:
    {"abandoned":[{"from":<n>,"to":<n>,"again":<n>}]}
    <n> is a line number exactly as the request gave it. "from" to "to" is the stretch that was abandoned, inclusive; "again" is the line where the attempt that was kept begins, or 0 when it was broken off and never picked up. Nothing to mark is a whole answer: {"abandoned":[]}.
  translate: the numbered lines of one video's subtitle track. Answers exactly those lines, in order, translated -- one line out for every line in, each beginning with its own number and a tab.
  textedit: every word spoken in the session, in order, with the seams between recordings and the long pauses marked. Answers the TEXT OF THE FINISHED VIDEO and nothing else: the same words in the same order and spelling, with words removed and none added.
  cut: the footage range and the session timeline. Answers SEGMENTS, strict JSON and nothing else:
    {"segments":[{"start":<sec>,"end":<sec>}]}
    <sec> is session seconds, a number. Segments in order, never overlapping, and inside the range the request gives.
  captions: a few clips, "CLIP n" with its length, then the lines said over it stamped as offsets from that clip's start. Answers CAPTIONS, strict JSON and nothing else:
    {"clips":[{"i":<n>,"fx":[{"start":<sec>,"end":<sec>,"text":"<words>"}]}]}
    <n> is the clip's number exactly as the request gave it; <sec> is seconds from that clip's start, inside the clip. A clip with nothing to caption is left out, and {"clips":[]} is a whole answer.
  speed: the clips with their lengths, what is spoken over each and which carry captions, the footage they come to, the target and the range. Answers SPEEDS, strict JSON and nothing else:
    {"speeds":[{"clip":<n>,"rate":<x>}]}
    <x> is the playback rate: 1 is the footage's own speed, above 1 runs fast, below 1 is slow motion. A clip left out plays at 1.
  effects: every clip of the cut, "CLIP n" with its length, then what was said and shown over it stamped as offsets from that clip's start. Answers EFFECTS, strict JSON and nothing else:
    {"fx":[{"clip":<n>,"kind":"zoom"|"stop"|"volume","start":<sec>,"end":<sec>,"gain":<x>}]}
    <sec> is seconds from that clip's start, inside the clip. "gain" belongs to volume alone: 1 as recorded, 0 silent.
  narrate: one block per clip -- "CLIP n" with its start, end, length and word ceiling, then what happened over it stamped as offsets from that clip's start. Answers ENTRIES, strict JSON and nothing else:
    {"entries":[{"start":<sec>,"end":<sec>,"at":<sec>,"text":"<words>","emotion":"<name>=<0..1>"}]}
    "start" and "end" are the clip's own, copied from the request; "at" is seconds from that clip's start.
    emotion is how the TTS reads it: a base -- happy, angry, sad, afraid, disgusted, melancholic, surprised, calm -- or close kin, weighted 0 to 1 ("angry=1", "happy=0.8, surprised=0.4"); named mixes (excited, awed, alarmed, confused, frustrated, desperate, tender, proud, dismayed, horrified, ominous) take a weight the same way. Loud or fast is not an emotion.
  upload text: the clips, each with where it starts in the finished video, what was seen and said in each, and the narration over it. Answers three parts with a blank line between them -- the title on one line prefixed exactly "TITLE: ", the thumbnail instruction on one line prefixed exactly "THUMBNAIL: ", then the description as prose. No JSON. The instruction goes to an image model that edits the first frame it is given, the others as references named by position ("the ship from the second image"); the title is printed onto the finished picture afterwards, so ask for no text, no lettering and no logo, and for the part it lands in to stay calm and uncluttered.

TOOLS
Some jobs are offered web_search and web_read. They are for a fact about a named thing you are about to WRITE DOWN and would otherwise guess -- what a tower does, what an item costs, how a name is spelled -- and only when the material does not contain it. Not to understand the session, not to confirm what the material already tells you, and never on a job whose answer is numbers rather than words: a search costs minutes, and the reasoning that led to it is done again from the start with the results in hand. A fact you write is one the material shows or one you looked up; with no tool offered, a fact you do not have is one you do not write.

NEVER INVENT
Only what the material shows. Never invent a time, a name, a score, a moment or an outcome -- not even one the user context leads you to expect: a stretch the lines do not cover did not happen, and only stretches with EVENT lines have footage behind them.`

// sysPrompt is the system message a job goes out with: the shared context,
// then the job's own prompt -- the only way one is assembled. An emptied box
// takes the block away rather than sending an empty heading (as ctxBlock).
func (a *App) sysPrompt(key string) string {
	job := a.prompt(key)
	// the precedence rule rides on the wording, not in it: it is assembled
	// here, like the system context in front, so a project's edited copy of
	// a wording carries it too and a wording added later cannot forget it.
	// Only when there is a user context to outrank anything -- a session with
	// an empty box is told nothing about a block it will not get.
	if key != "system" && a.sessionCtx() != "" {
		job += "\n\n" + ctxRule
	}
	sys := sysFor(key, strings.TrimSpace(a.prompt("system")))
	if sys == "" {
		return job
	}
	return sys + "\n\n" + job
}

// sysFor cuts the system context down to what one job can use: sections are
// chosen per job by heading (TestTheSystemContextIsUnderHeadings), and the
// list of reply shapes keeps only this job's line. A heading this does not
// know goes to every job -- an unknown section is assumed to matter.
func sysFor(key, sys string) string {
	if sys == "" {
		return ""
	}
	want := sysSections[key]
	var out []string
	head := "" // the heading the current block sits under; "" before the first
	for _, blk := range sysBlocks(sys) {
		if h := sysHeading(blk); h != "" {
			head = h
		}
		switch {
		case want == nil:
			// the system context itself, or a key nobody registered: whole
		case head == "" || !knownSection(head):
			// the run-up before the first heading, and sections this does
			// not know about: every job
		case !want[head]:
			continue
		case head == sysJobsHead:
			blk = ownJobLine(key, blk)
			if blk == "" {
				continue
			}
		}
		out = append(out, blk)
	}
	return strings.Join(out, "\n\n")
}

// sysBlocks is the box as paragraphs: runs of blank lines divide them, and a
// paragraph carries no blank lines of its own -- so a heading with an extra
// blank line in front of it still opens its block rather than hiding behind a
// blank first line.
func sysBlocks(sys string) []string {
	var out []string
	for _, blk := range blankRuns.Split(sys, -1) {
		// the blank lines around a block go; the indent its first line wears
		// stays, because the lists in the box are indented and a first line
		// that lost its indent would read as a paragraph of its own
		blk = strings.TrimRight(strings.Trim(blk, "\n"), " \t\n")
		if strings.TrimSpace(blk) != "" {
			out = append(out, blk)
		}
	}
	return out
}

var blankRuns = regexp.MustCompile(`\n[ \t]*\n+`)

// sysHeading is the heading a block opens with, or "".
//
// The app's own sections are known by their opening words, which is what
// lets the jobs heading carry a lower-case tail ("-- nothing around the
// answer:") and still be a heading. A section somebody adds to the box is a
// heading when its first line is upper-case words and nothing else.
func sysHeading(blk string) string {
	line := blk
	if i := strings.IndexByte(line, '\n'); i >= 0 {
		line = line[:i]
	}
	line = strings.TrimSpace(line)
	for _, h := range sysKnownHeads {
		if strings.HasPrefix(line, h) {
			return h
		}
	}
	if line == "" || strings.HasSuffix(line, ".") || line != strings.ToUpper(line) {
		return ""
	}
	for _, r := range line {
		if !(r >= 'A' && r <= 'Z' || r == ' ' || r == ',' || r == '-') {
			return ""
		}
	}
	return line
}

// sysKnownHeads is every section the table below has an opinion on, by the
// words it opens with; longest first where one is a prefix of another.
var sysKnownHeads = []string{
	"THE ANSWER", "THE MATERIAL", "THE CLOCKS", "THE FOUR STEPS", "THE CUT",
	sysJobsHead, "TOOLS", "NEVER INVENT",
}

// The sections, as the headings name them, and which jobs read which.
//
//	THE ANSWER, THE MATERIAL, THE CLOCKS, NEVER INVENT   every job
//	THE FOUR STEPS         the jobs downstream of Prepare, which write for
//	                       the step after them; describe and fix are Prepare
//	THE CUT                the two jobs that produce or check segments
//	WHAT EACH JOB ...      this job's own line, and no other's
//	TOOLS                  the jobs that are offered any (webToolsFor)
const sysJobsHead = "WHAT EACH JOB IS GIVEN"

var sysSections = map[string]map[string]bool{
	"describe":  sysSet("THE ANSWER", "THE MATERIAL", "THE CLOCKS", sysJobsHead, "NEVER INVENT"),
	"fix":       sysSet("THE ANSWER", "THE MATERIAL", "THE CLOCKS", sysJobsHead, "NEVER INVENT"),
	"retake":    sysSet("THE ANSWER", "THE MATERIAL", "THE CLOCKS", sysJobsHead, "NEVER INVENT"),
	"textedit":  sysSet("THE ANSWER", sysJobsHead, "NEVER INVENT"),
	"cut":       sysSet("THE ANSWER", "THE MATERIAL", "THE CLOCKS", "THE FOUR STEPS", "THE CUT", sysJobsHead, "TOOLS", "NEVER INVENT"),
	"captions":  sysSet("THE ANSWER", "THE MATERIAL", "THE CLOCKS", "THE CUT", sysJobsHead, "NEVER INVENT"),
	"speed":     sysSet("THE ANSWER", "THE CUT", sysJobsHead),
	"effects":   sysSet("THE ANSWER", "THE MATERIAL", "THE CLOCKS", "THE CUT", sysJobsHead, "NEVER INVENT"),
	"narrate":   sysSet("THE ANSWER", "THE MATERIAL", "THE CLOCKS", "THE FOUR STEPS", sysJobsHead, "TOOLS", "NEVER INVENT"),
	"translate": sysSet("THE ANSWER", sysJobsHead, "NEVER INVENT"),
	"youtube":   sysSet("THE ANSWER", "THE MATERIAL", "THE CLOCKS", "THE FOUR STEPS", sysJobsHead, "TOOLS", "NEVER INVENT"),
}

func sysSet(heads ...string) map[string]bool {
	m := map[string]bool{}
	for _, h := range heads {
		m[h] = true
	}
	return m
}

// knownSection is whether a heading is one the table above has an opinion on.
func knownSection(head string) bool {
	for _, h := range sysKnownHeads {
		if h == head {
			return true
		}
	}
	return false
}

// sysJobLabel is how the jobs list names each key's own line.
var sysJobLabel = map[string]string{
	"describe": "describe:", "fix": "transcript:", "cut": "cut:",
	"narrate": "narrate:", "youtube": "upload text:",
	"captions": "captions:", "speed": "speed:", "effects": "effects:",
}

// ownJobLine is a block under the jobs heading with every other job's line
// taken out. The heading is one block and the lines are another, so this sees
// each in turn: a heading line stays, this job's line stays, the rest go. A
// block left with nothing is nothing.
func ownJobLine(key, blk string) string {
	label := sysJobLabel[key]
	var out []string
	mine := false
	for _, line := range strings.Split(blk, "\n") {
		switch {
		case sysHeading(line) != "":
			out = append(out, line)
		case strings.HasPrefix(strings.TrimSpace(line), label):
			// this job's line, and what is indented under it: the worked
			// example a job answers by is written on its own lines beneath
			// the sentence, and a filter that kept the sentence alone would
			// send every job its shape with the example cut off
			mine = true
			out = append(out, line)
		case mine && strings.HasPrefix(line, "    "):
			out = append(out, line)
		default:
			mine = false
		}
	}
	return strings.Join(out, "\n")
}

// ctxRule says what the user context may change: everything a job's wording
// says (defaults written without this video in view), and nothing the system
// context says (the shape of the answer, the clocks, what may be invented --
// how the answer is READ). It sits at the wording's end, where the model has
// the rules in hand.
const ctxRule = `WHERE THIS DISAGREES WITH THE USER CONTEXT
Everything above is a default for a session nobody described. The USER CONTEXT in the request was written by the person whose recording this is, and wherever it asks for something these rules would not -- more of an effect or none, a longer segment, another voice, a different subject, a line kept that this would drop, a language this did not expect -- the user context wins and the rule above gives way. What it does not change is the mechanics you were given first: the shape of the answer, the clock, what may be invented, and the ranges the reply is judged by. Those are how the answer is read, not how the video is made.`
