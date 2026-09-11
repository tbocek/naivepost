package main

// Describe: each footage source's frames (inputs/frames/<v>/) go to the vision
// LLM in small batches with the words heard around those seconds and a rolling
// state, so each batch describes what is HAPPENING. Output:
// prepare/describe/<video>/events.tsv, resumable per chunk. Page: prep.go.

import (
	"errors"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

const framesPerReq = 4 // frames per vision request

// recentEvents is how much of the previous work rides along with each request:
// the last few EVENT lines, as text. No earlier frames are ever re-sent -- the
// model sees framesPerReq images and reads about everything before them. The
// page states both numbers, so they are named rather than typed twice.
const recentEvents = 3

// The primer holds no game knowledge: what is specific to this footage goes in
// the prompt box, which replaces this wholesale. One paragraph per line, long
// on purpose -- it is read in a wrapping text box, and a hard wrap wraps twice.
const describeSystem = `You describe screen-recorded footage for a video editor.

You will never see these frames or any earlier ones again: your two lines are your only memory, so write them for a reader who has seen nothing.

What to write, in this order of importance.
1. What CHANGES across the frames: movement, an action and what it causes, something arriving or gone. The frames are a span of time, not a picture. If nothing meaningful changes, say so in a few words rather than padding.
2. How it moves: hectic (fast turning, violent or continuous motion, most of the picture different from one frame to the next), calm and steady, or in between. Say which even when nothing else happens -- the cut is chosen on pace as much as on content.
3. On-screen text -- names, scores, counters, menus, subtitles -- read and used. Once something has a name, keep using that name, so the same thing reads the same way across the whole log. Where the user context names a person, a place or a thing, use its name for it.

What the speech is for. It comes from more than one microphone, and whoever is talking may be describing something you cannot see, remembering, or talking about nothing on screen. Speech is a claim, not evidence: where speech and frames disagree, the frames win, and a line may refer to something before or after the moment it is spoken. Lines under a "context" heading are for orientation only -- never describe something that appears only there.

What not to write. Nothing you were not shown or told: no genre, title, place or character assumed. No "appears to" or "seems to" -- if you cannot tell what something is, say how it looks and move on. No mention of frames, images, chunks, or yourself.

The two lines:
EVENT: what happens in these seconds and how hectic or calm it is -- present tense, concrete, specific. Up to 35 words when something happens; when nothing meaningful changes, the pace and a few words, twelve at most -- "Calm; same view, the tower keeps firing" is a whole line. The cut reads hundreds of these in one go and chooses by what CHANGES, so a long line about nothing is a long line in the way. Do not restate the STATE.
STATE: the running state after these seconds, at most 50 words: where this is, what is being done, who else is present, the ongoing goal. Carry forward what is still true, drop what has stopped being true, keep it readable on its own.

Both labels, every time, including when nothing happened. Written out, when something does:

EVENT: Hectic; the red car spins at the hairpin, clips the barrier and stops across the track.
STATE: Lap 3 of 5, the red car last after the spin, yellow flags at the hairpin.

...and when nothing does, which is most of the frames you will see:

EVENT: Calm; same view, the driver keeps talking.
STATE: Lap 4 of 5, the red car last, the track clear again.

Both examples are invented and none of it is in the session you are given.`

type tsvRow struct {
	s, e float64
	spk  string // SPEAKER_nn, or EVENT for a line describing the screen
	text string
	// which recording this came off, as the merged timeline names it (blank in
	// a single recording's own transcript). It decides whether the viewer will
	// ever hear the line: the render takes its audio from the footage, so a
	// line off a separate microphone is in the transcript and not in the video.
	src string
}

// tlLabel is who a line belongs to, in the one vocabulary every step uses:
//
//	EVENT       what the picture showed
//	NARRATOR    the narrator's own microphone, which the video never plays
//	SPEAKER_nn  a voice the video does play
//
// narr is narratorMic, blank when nobody is exempt.
func tlLabel(r tsvRow, narr string) string {
	switch {
	case r.spk == "EVENT":
		return "EVENT"
	case narr != "" && r.src == narr:
		return "NARRATOR"
	case r.spk == "":
		return "SPEAKER"
	}
	return r.spk
}

// sessionText renders the merged timeline as the cut reads it: one line each,
// stamped, then label, then text. Built from session.tsv at request time, so a
// change here reaches old projects.
//
// The stamp says the same instant TWICE -- seconds, then mm:ss -- because the
// answer is in seconds and the reasoning is in minutes, and the step between
// them was being taken by hand three hundred times a run. Once was enough to
// lose five seconds of a video: a cut read the marker [01:45] as 145 seconds
// rather than 105, dropped a sentence that was never a stumble, and kept the
// stumble it had been told about. Nothing needs converting now.
//
// A stretch marked abandoned (retake.go) comes out as ONE line saying so, with
// everything inside it -- what was said and what was on screen -- left out. It
// is not hidden: the line says the seconds are there and were said again, which
// is what stops a model choosing a moment inside them, and the transcript still
// holds every word.
//
// It says ALREADY REMOVED because it is: dropMarked subtracts the stretch from
// whatever the cut answers, on the word. Told only that the stretch was "not
// kept", a model helpfully ends its segment in front of it -- and its own
// boundary is a whole second coarser than the mark, so every marker cost a
// word off the end of the sentence before it ("...the previous public state
// of", "...through apps, not").
func sessionText(rows []tsvRow, narr string, marks []retake) string {
	var b strings.Builder
	done := map[int]bool{}
	for _, r := range rows {
		if i := retakeAt(marks, r); i >= 0 {
			if !done[i] {
				done[i] = true
				m := marks[i]
				again := ""
				if m.Again > 0 {
					again = ", said again at " + stamp(m.Again)
				}
				fmt.Fprintf(&b, "%s (abandoned attempt to %s%s -- already removed, read straight past it)\n",
					stamp(m.S), stamp(m.E), again)
			}
			continue
		}
		fmt.Fprintf(&b, "%s %s: %s\n", stampSpan(r.s, r.e), tlLabel(r, narr), r.text)
	}
	return b.String()
}

// stampSpan is a line's stamp: where it starts AND where it ends. The end is
// the half a cut needs and the half the line never carried. Told only where a
// line begins, a model guesses where it ends from the length of the text and
// the picture lines around it, and one guess ended a segment four seconds
// inside a sentence of the script -- an EVENT line said he "finishes the point
// and pauses" at 737, the sentence ran to 744, and the description won.
func stampSpan(s, e float64) string {
	a, b := int(s), int(math.Ceil(e))
	return fmt.Sprintf("[%ds-%ds | %02d:%02d]", a, b, a/60, a%60)
}

// stamp is one instant as a timeline line wears it: the seconds a model answers
// with, then the mm:ss it reads the session's shape by. Truncated, not rounded,
// so a stamp never names a second the line had not reached.
func stamp(t float64) string {
	n := int(t)
	return fmt.Sprintf("[%ds | %02d:%02d]", n, n/60, n%60)
}

// How much speech rides along with a chunk of frames, and how far from it a
// line may have been said. The two caps apply independently: whichever binds
// first wins.
const (
	ctxSegs   = 2    // context segments on each side
	ctxWindow = 10.0 // seconds between the chunk and a context segment
)

// electSpeech splits a recording's transcript into what was said during a
// chunk of frames and a little either side. The three sets are disjoint and
// cover every segment: anything overlapping the chunk is during, IN FULL; the
// rest is before or after and kept only if close and near the front of its
// queue.
func electSpeech(rows []tsvRow, chunkStart, chunkEnd float64) (before, during, after []tsvRow) {
	for _, r := range rows {
		switch {
		case r.e > chunkStart && r.s < chunkEnd:
			during = append(during, r)
		case r.e <= chunkStart:
			if chunkStart-r.e <= ctxWindow {
				before = append(before, r)
			}
		default: // r.s >= chunkEnd
			if r.s-chunkEnd <= ctxWindow {
				after = append(after, r)
			}
		}
	}
	// nearest the chunk first, so the two that survive the cap are the two
	// closest to it and not the two that happen to come first in the file
	sort.Slice(before, func(i, j int) bool { return before[i].e > before[j].e })
	sort.Slice(after, func(i, j int) bool { return after[i].s < after[j].s })
	before = before[:min(len(before), ctxSegs)]
	after = after[:min(len(after), ctxSegs)]
	// and back into reading order for the block itself
	sort.Slice(before, func(i, j int) bool { return before[i].s < before[j].s })
	sort.Slice(during, func(i, j int) bool { return during[i].s < during[j].s })
	return before, during, after
}

// speechSrc is one recording's transcript, already shifted onto the frames'
// own clock, and what to call it in the block. More than one because the
// footage's own audio is rarely the only microphone in the session: the person
// recording is usually on a separate track, saying what they are doing while
// they do it, which is the best evidence there is for what a chunk is about.
type speechSrc struct {
	label string
	rows  []tsvRow
}

// spoken is one elected line and which recording it came off.
type spoken struct {
	tsvRow
	label string
}

// speechBlock is what the model is told was said around these frames. All
// three sections are always emitted, empty included, so "nobody spoke" is not
// read as speech dropped. Times are seconds from the chunk's first frame,
// signed, the same clock the frames are labelled on. Each source is elected
// separately and merged in time order, so the two-a-side cap is per speaker.
// Segments stay one per line, text untouched: the ASR's pauses carry meaning.
func speechBlock(srcs []speechSrc, narr string, chunkStart, chunkEnd float64) string {
	var before, during, after []spoken
	tag := func(dst *[]spoken, rows []tsvRow, src string) {
		for _, r := range rows {
			r.src = src // a single recording's transcript does not carry it
			*dst = append(*dst, spoken{tsvRow: r, label: tlLabel(r, narr)})
		}
	}
	for _, s := range srcs {
		b, d, a := electSpeech(s.rows, chunkStart, chunkEnd)
		tag(&before, b, s.label)
		tag(&during, d, s.label)
		tag(&after, a, s.label)
	}
	var b strings.Builder
	section := func(head, empty string, segs []spoken) {
		b.WriteString(head + "\n")
		if len(segs) == 0 {
			b.WriteString(empty + "\n")
			return
		}
		sort.SliceStable(segs, func(i, j int) bool { return segs[i].s < segs[j].s })
		for _, r := range segs {
			fmt.Fprintf(&b, "[%+.1fs] %s: %s\n", r.s-chunkStart, r.label, r.text)
		}
	}
	section("--- context before (do not describe) ---", "(none)", before)
	section("--- spoken during these frames ---", "(no speech during these frames)", during)
	section("--- context after (do not describe) ---", "(none)", after)
	return strings.TrimRight(b.String(), "\n")
}

// eventState reads one describe reply into its two lines.
//
// Lenient about the first label and only the first. A run of six chunks in one
// recording wrote the description straight out and labelled the STATE
// correctly underneath it -- the words were right and the "EVENT:" in front of
// them was missing -- and the whole reply was then filed as the event: a
// sixty-word line about nothing, with the state repeated inside it, in the
// brief the cut reads. What comes before the STATE line is the event, which is
// what it plainly was.
//
// The wording asks for both labels and shows them (describeSystem). This is the
// floor under that, not a second opinion about it: an example makes the model
// right more often, and cannot make it right always.
func eventState(reply string) (event, state string) {
	rest, anchored := reply, false
	// the state first: its label is what says where the event ends when the two
	// came back on one line, which is how they came back
	if i := strings.Index(rest, "STATE:"); i >= 0 {
		state = flatten(rest[i+len("STATE:"):])
		rest, anchored = rest[:i], true
	}
	if i := strings.Index(rest, "EVENT:"); i >= 0 {
		rest, anchored = rest[i+len("EVENT:"):], true
	}
	event = flatten(rest)
	// leniency needs an anchor. Words in front of a STATE line are the event
	// that lost its label; words with no label anywhere near them are a reply
	// that came back in some shape nobody asked for, and saying so in the log
	// beats filing a refusal as a description of the footage.
	if event == "" || !anchored {
		event = "(no event line: " + flatten(reply) + ")"
	}
	return event, state
}

// flatten is one line of whatever it is given: events.tsv is a line per event
// with tabs between its fields, so a description that came back on two lines
// would otherwise be two rows, the second of them nonsense.
func flatten(s string) string {
	return strings.Join(strings.Fields(strings.ReplaceAll(s, "\t", " ")), " ")
}

// loadTSVRows reads both timeline files: a recording's transcript (start, end,
// speaker, text) and the session timeline (start, end, RECORDING, speaker,
// text, incl. EVENT lines). Text is the LAST column -- column 4 of the five is
// the label, and reading it as text once fed Narrate the word "SPEAKER_00".
func loadTSVRows(path string) []tsvRow {
	b, err := os.ReadFile(path)
	if err != nil {
		return nil
	}
	var out []tsvRow
	for _, line := range strings.Split(string(b), "\n") {
		f := strings.Split(line, "\t")
		if len(f) < 4 {
			continue
		}
		var r tsvRow
		fmt.Sscanf(f[0], "%f", &r.s)
		fmt.Sscanf(f[1], "%f", &r.e)
		// the merged file has the recording's name in between; everything after
		// the speaker is the line itself, so a tab in it keeps the whole line
		// rather than its tail
		if len(f) > 4 {
			r.src, r.spk, r.text = f[2], f[3], strings.Join(f[4:], "\t")
		} else {
			r.spk, r.text = f[2], strings.Join(f[3:], "\t")
		}
		out = append(out, r)
	}
	return out
}

// ---- run --------------------------------------------------------------------

// Every source in the session takes part: each video is described on its own
// timeline, and every (video, voice) pair gets its own alignment offset.
type videoPlan struct {
	base     string
	video    string // absolute path
	dir      string // prepare/describe/<base>
	frames   []string
	interval float64
	scale    string
	chunks   int
}

func (a *App) planVideo(video, descDir string) (*videoPlan, error) {
	base := baseName(video)
	fdir := a.framesDir(base)
	ents, err := os.ReadDir(fdir)
	if err != nil {
		return nil, fmt.Errorf("no frames for %s -- run Prepare", base)
	}
	p := &videoPlan{base: base, video: video, dir: filepath.Join(descDir, base)}
	for _, e := range ents {
		// every .jpg in here is a frame: they are named for the second they were
		// shot in now, and f000001.jpg only in folders extracted before that.
		// The dotted marker file is the one thing to skip.
		if !e.IsDir() && strings.HasSuffix(e.Name(), ".jpg") && !strings.HasPrefix(e.Name(), ".") {
			p.frames = append(p.frames, filepath.Join(fdir, e.Name()))
		}
	}
	sortStamped(p.frames)
	if len(p.frames) == 0 {
		return nil, fmt.Errorf("frame folder is empty: %s", fdir)
	}
	if b, err := os.ReadFile(filepath.Join(fdir, ".interval")); err == nil {
		parts := strings.SplitN(strings.TrimSpace(string(b)), "|", 2)
		fmt.Sscanf(parts[0], "%f", &p.interval)
		if len(parts) > 1 {
			p.scale = parts[1]
		}
	}
	if p.interval <= 0 {
		return nil, fmt.Errorf("%s was extracted as every-frame; describe needs a fixed interval — rerun Prepare with e.g. 1s", base)
	}
	p.chunks = (len(p.frames) + framesPerReq - 1) / framesPerReq
	return p, nil
}

// commentary puts every other recording's words on this video's clock
// (srcClock). The prompt trusts words over pictures, so a wrong offset is
// believed; an unstamped recording therefore goes at the session start, the
// one guess visible and correctable on Cut by the right drag. Offsets are
// logged: when a description is about the wrong thing, this is the number.
func (a *App) commentary(video string, audios []string) []speechSrc {
	if len(audios) == 0 {
		return nil
	}
	// the WHOLE session, not just this video and these recordings: the session's
	// start is the earliest moment anything in it names, and a clock built from
	// two files can put its zero somewhere the Cut page never would. Then an
	// unstamped camera would hear the same recording at one offset here and
	// another one there.
	vids, auds := a.snappedSources()
	all := append(append(append([]string{}, vids...), auds...), video)
	at, _ := srcClock(append(all, audios...))
	vidStart := at[video]
	var out []speechSrc
	for _, aud := range audios {
		base := baseName(aud)
		st := at[aud]
		rows := loadTSVRows(a.transcriptPath(base))
		if len(rows) == 0 {
			continue
		}
		off := st - vidStart
		for i := range rows {
			rows[i].s += off
			rows[i].e += off
		}
		out = append(out, speechSrc{label: base, rows: rows})
		a.logfIdle(">>> [%s] hearing %s alongside it, starting %.1f s in", baseName(video), base, off)
	}
	return out
}

// The describer describes every footage source. span is how much of the progress bar
// this job owns -- all of it when Describe runs on its own, half when the
// fixer runs after it on the same page.
func (a *App) describeAll(videos, audios []string, span float64) error {
	descDir := a.describeDir()
	if err := os.MkdirAll(descDir, 0o755); err != nil {
		return err
	}
	var plans []*videoPlan
	total := 0
	for _, v := range videos {
		p, err := a.planVideo(v, descDir)
		if err != nil {
			return err
		}
		plans = append(plans, p)
		total += p.chunks
	}
	// the queue: one task per chunk of frames, over every recording. Nothing
	// here is countable before the frames are on disk, which is why it is
	// filled at the top of the job rather than when the run started.
	a.qPush(trackDescribe, total, "chunk")
	// this step ONLY describes; the fixer reads these event logs afterwards
	done := 0
	for _, p := range plans {
		if err := a.describeVideo(p, a.commentary(p.video, audios), done, total, span); err != nil {
			return err
		}
		done += p.chunks
	}
	// chunks that resume skip their progress call, so a video that was already
	// described would leave the bar where it started -- claim the share here
	a.qDone(trackDescribe, span)
	return nil
}

// resetDescribe drops every source's event log and rolling STATE so the next
// run describes from t=0. It keeps .llmframes (scaled pixels, minutes of
// ffmpeg) and prepare/transcript (the fixer never resumes). Every folder under
// prepare/describe/ goes, not just the selected sources -- a deselected
// recording's log is exactly the stale half-run this removes.
func (a *App) resetDescribe() error {
	ents, err := os.ReadDir(a.describeDir())
	if err != nil {
		return nil // nothing described yet is already at the start
	}
	var cleared []string
	for _, e := range ents {
		if !e.IsDir() {
			continue
		}
		gone := false
		for _, f := range []string{"events.tsv", "state.txt"} {
			switch err := os.Remove(filepath.Join(a.describeDir(), e.Name(), f)); {
			case err == nil:
				gone = true
			case !errors.Is(err, os.ErrNotExist):
				return err
			}
		}
		if gone {
			cleared = append(cleared, e.Name())
		}
	}
	if len(cleared) > 0 {
		a.logf(">>> stopped last time — describing from the start again: %s (scaled frames kept)",
			strings.Join(cleared, ", "))
	}
	return nil
}

// ---- describe ---------------------------------------------------------------

func (a *App) describeVideo(p *videoPlan, comm []speechSrc, chunkOff, chunkTotal int, span float64) error {
	if err := os.MkdirAll(p.dir, 0o755); err != nil {
		return err
	}
	// this video's own audio first, then everyone else's microphone
	speech := append([]speechSrc{{
		label: p.base,
		rows:  loadTSVRows(filepath.Join(a.inputsDir(), p.base, "transcript.tsv")),
	}}, comm...)
	narr := a.narratorMic()
	evPath := filepath.Join(p.dir, "events.tsv")
	statePath := filepath.Join(p.dir, "state.txt")

	// The model needs LLM-sized images; frames may be stored bigger. Scaled
	// copies are cached per frame, so resume and re-runs pay scaling once.
	needScale := true
	switch p.scale {
	case "896w (LLM)", "480p":
		needScale = false
	}
	llmDir := filepath.Join(p.dir, ".llmframes")
	if needScale {
		if err := os.MkdirAll(llmDir, 0o755); err != nil {
			return err
		}
	}
	scaledFrame := func(src string) (string, error) {
		if !needScale {
			return src, nil
		}
		dst := filepath.Join(llmDir, filepath.Base(src))
		if !exists(dst) {
			if err := a.runCmd(ffTool("ffmpeg"), "-v", "error", "-y", "-i", src,
				"-vf", "scale=896:-2", "-q:v", "4", dst); err != nil {
				return "", err
			}
		}
		return dst, nil
	}

	// resume: chunks already described are keyed by their start time; the
	// recent-events window picks up from the end of the existing log
	done := map[string]bool{}
	var recent []tsvRow
	if b, err := os.ReadFile(evPath); err == nil {
		for _, l := range strings.Split(string(b), "\n") {
			f := strings.Split(l, "\t")
			if len(f) >= 3 && f[0] != "" {
				done[f[0]] = true
				r := tsvRow{spk: "EVENT", text: f[2]}
				fmt.Sscanf(f[0], "%f", &r.s)
				recent = append(recent, r)
				if len(recent) > recentEvents {
					recent = recent[1:]
				}
			}
		}
	}
	state := "Recording just started."
	if b, err := os.ReadFile(statePath); err == nil && len(b) > 0 {
		state = strings.TrimSpace(string(b))
	}

	cached := 0
	for c := 0; c < p.chunks; c++ {
		if err := a.checkpoint(); err != nil {
			return err
		}
		lo := c * framesPerReq
		hi := min(lo+framesPerReq, len(p.frames))
		// two ends, and they are different on purpose: the frames run from t0
		// to tLast, and that is what the model is shown and what the speech is
		// elected against. The event is logged as covering t0 to t1, one
		// interval further, because the last frame stands for the interval
		// that follows it and the cut reads these spans as time.
		t0 := float64(lo) * p.interval
		tLast := float64(hi-1) * p.interval
		t1 := float64(hi) * p.interval
		key := fmt.Sprintf("%.2f", t0)
		// taken whatever becomes of it: a chunk described by an earlier run is
		// one this run is done with, and a queue that only counted the ones it
		// worked would sit at 1/300 through a resumed session
		a.qTake(trackDescribe)
		if done[key] {
			continue
		}
		a.prog(trackDescribe, span*float64(chunkOff+c)/float64(chunkTotal), "")

		// past history rides along twice: the rolling STATE (what is true) and
		// the last events (what just happened) -- together they let the model
		// describe motion and consequences, not disconnected stills
		// the session context leads, ahead of the state and the pictures: it is
		// what the names in this footage mean, and a describer that reads it
		// after the frames has already guessed at them
		text := a.ctxBlockFor("describe") + fmt.Sprintf("STATE so far: %s\nFrames cover t=%.0fs to t=%.0fs, %g s apart.", state, t0, tLast, p.interval)
		if len(recent) > 0 {
			// on the frames' clock like everything else in this request: these
			// used to carry their absolute time in the video, which is the one
			// number on the page measured from somewhere else
			text += "\nJust before this:\n"
			for _, r := range recent {
				text += fmt.Sprintf("[%+.0fs] EVENT: %s\n", r.s-t0, r.text)
			}
			text = strings.TrimRight(text, "\n")
		}
		text += "\n" + speechBlock(speech, narr, t0, tLast)
		// Each frame gets a line of its own in front of it, on the same signed
		// clock the speech is on. Without them the images arrive as an unlabelled
		// run and the only thing carrying their order is their position in the
		// array -- so "[+2.1s] he opens it" could not be tied to a picture, and
		// which of the four was first was something the model had to assume.
		content := []any{txtPart(text)}
		for i, f := range p.frames[lo:hi] {
			sf, err := scaledFrame(f)
			if err != nil {
				return err
			}
			part, err := imgPart(sf)
			if err != nil {
				return err
			}
			content = append(content, txtPart(fmt.Sprintf("[%+.1fs] FRAME %d of %d",
				float64(i)*p.interval, i+1, hi-lo)), part)
		}
		// the same frames, the same state, the same wording: the same answer
		// (llmcache.go). The pictures are inside content as data URLs, so a
		// frame that changed by one pixel is a different question.
		sys := a.sysPrompt("describe")
		ask := askKey(sys, content)
		reply, hit := a.cachedReply("describe", ask)
		if hit {
			cached++
		} else {
			var err error
			reply, err = a.llmChatRetry("describe", []map[string]any{
				msg("system", sys), msg("user", content),
			}, false)
			if err != nil {
				if errors.Is(err, errStopped) {
					return errStopped
				}
				return fmt.Errorf("describe %s t=%.0f: %w", p.base, t0, err)
			}
			a.keepReply("describe", ask, reply)
		}

		event, newState := eventState(reply)
		if newState != "" {
			state = newState
		}
		f, err := os.OpenFile(evPath, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o644)
		if err != nil {
			return err
		}
		fmt.Fprintf(f, "%s\t%.2f\t%s\n", key, t1, event)
		f.Close()
		if err := os.WriteFile(statePath, []byte(state+"\n"), 0o644); err != nil {
			return err
		}
		recent = append(recent, tsvRow{s: t0, spk: "EVENT", text: event})
		if len(recent) > recentEvents {
			recent = recent[1:]
		}
	}
	if cached > 0 {
		a.logfIdle(">>> [%s] event log complete (%d chunks, %d answered from the cache)",
			p.base, p.chunks, cached)
	} else {
		a.logfIdle(">>> [%s] event log complete (%d chunks)", p.base, p.chunks)
	}
	return nil
}
