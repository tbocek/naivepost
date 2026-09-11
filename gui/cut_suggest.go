package main

// Suggest: the Cut page's ▶. The one path where the timeline arrives from a
// model -- the cut, then captions, speed and effects passes over it -- and
// every reply is walked back onto the timeline it claims to describe
// (clampFxToSegs, keepFilmed, rowsInSegs) before the page sees it.

import (
	"errors"
	"fmt"
	"math"
	"regexp"
	"sort"
	"strconv"
	"strings"

	"github.com/diamondburned/gotk4/pkg/glib/v2"
)

func (a *App) suggestClicked() {
	if a.busy() {
		return
	}
	// re-suggesting over an untouched suggestion is fine -- there is no human
	// work in it to lose. Over hand edits it is not, so say what to press.
	if len(a.ed.segs) > 0 && !sameCut(a.ed.segs, a.ed.base.segs) {
		// ▶ is the only way to run this step now, so this line is where people meet
		// Revert -- it has to name the button as it looks, not as a glyph it lost
		a.setStatus("you have hand edits — press Revert first for a fresh suggestion")
		return
	}
	rows := a.sessionRows()
	if len(rows) == 0 {
		a.setStatus("run Describe first — the suggestion reads the session timeline, and there is none")
		return
	}
	// the marks Prepare left on the timeline (retake.go): a stretch that was
	// said again is one line saying so, so the cut never chooses seconds the
	// speaker had already thrown away
	marks := a.loadRetakes()
	// a read to camera was cut in Prepare, as text: every filmed second minus
	// the marks. No model here -- there is nothing left for one to choose
	if a.videoStyleName() == styleRead {
		// ...unless the text was edited by hand since: then the text is the
		// edit, and the marks follow it (marksFromText)
		if m, ok := a.marksFromText(); ok {
			marks = m
		}
		a.cutByText(marks)
		return
	}
	session := sessionText(rows, a.narratorMic(), marks)
	// how long the finished video should be, as the user context names it:
	// "about 12 min", "a 90 s teaser". It was a box on the Cut page's toolbar,
	// which is a second place to say a thing the context already says -- and
	// the two disagreed for a week of runs, the box quietly winning while the
	// sentence beside it read 12 minutes. One place, and it is the one the
	// person writes in. Nothing named, and the default stands.
	// 0 is "no target", and it is the ordinary answer: a session whose context
	// names no length has no length, not a secret default one. It used to fall
	// back to 300 s and the request then told the model that 300 was "the
	// length named in the user context" -- which nobody had named. A script
	// read out in full came back cut to a third of itself, with the rest left
	// for a speed pass the same context had forbidden.
	target := 0.0
	if want, ok := ctxLength(a.sessionCtx()); ok {
		target = want
	} else {
		a.logf(">>> suggest: the user context names no length — everything worth keeping goes in")
	}
	// how long the session runs, which is the denominator the choosing half of
	// the bar counts against (see suggestCut)
	span := 0.0
	for _, r := range rows {
		span = math.Max(span, r.e)
	}

	a.startRun()
	a.saveProjectNow() // the run is a moment worth a file
	if target > 0 {
		a.logf(">>> suggest: target %.0f s — three calls: the cut, its captions, its effects", target)
	} else {
		a.logf(">>> suggest: no target length — three calls: the cut, its captions, its effects")
	}
	// Both calls are streamed, but the model thinks for minutes before the first
	// segment, so the bar pulses until the first finished segment's own fraction
	// stops it (same shape as publish). The queue's first word, not the bar's:
	// anything the queue says later would wipe it (showProg runs on an idle).
	a.qJob(trackSTT, "suggest", 1, 4)
	a.prog(trackSTT, 0, "thinking over the whole session")
	glib.TimeoutAdd(150, func() bool {
		if !a.running {
			return false
		}
		a.progMu.Lock()
		moving := a.progParts[trackSTT] > 0
		a.progMu.Unlock()
		if moving {
			return false
		}
		a.progress.Pulse()
		return true
	})
	go func() {
		a.logCtx("suggest")
		segs, fx, err := a.suggestCut(session, target, span)
		if err == nil {
			// the cut stands. Now what goes ON it, clip by clip: the captions,
			// then the decorations. Each pass sees the kept clips and nothing
			// else, answers in the clip's own seconds, and can fail on its own
			// without taking the cut with it.
			a.qJob(trackSTT, "suggest", 2, 4)
			caps := a.captionCut(segs, rows)
			// the speeds come after the captions and know about them: a
			// caption over a stretch at 4 is gone before it is read, and
			// which lines are captioned is the pass above's decision
			a.qJob(trackSTT, "suggest", 3, 4)
			a.prog(trackSTT, 0.85, "how fast each clip plays")
			fx = append(fx, caps...)
			fx = append(fx, a.speedCut(segs, caps, rows)...)
			a.qJob(trackSTT, "suggest", 4, 4)
			a.prog(trackSTT, 0.93, "the zooms and stops")
			fx = append(fx, a.decorateCut(segs, rows, fx)...)
		}
		glib.IdleAdd(func() {
			a.endRun()
			if err != nil {
				if !errors.Is(err, errStopped) {
					a.logf("suggest FAILED: %v", err)
				}
				a.progress.SetFraction(0)
				a.setStatus("suggest failed — see log")
				return
			}
			a.ed.pushUndo() // a suggestion is a proposal; Undo clears it again
			// A re-suggest replaces the old cut, never stacks on it -- but only
			// the footage half of it. The inserts were placed by hand, the model
			// was never told they exist, and an answer that does not mention them
			// is not an answer that dropped them.
			a.ed.segs = insertsOf(a.ed.segs)
			segs = joinSeams(segs, rows)
			for _, s := range segs {
				a.ed.segs = append(a.ed.segs, cutSeg{
					S: a.ed.snapEdge(s.S, true), E: a.ed.snapEdge(s.E, false)})
			}
			// ...and then the marks, last and over the top of the snapping.
			//
			// Hiding a stretch from the brief is not the same as keeping it out
			// of the cut: the model answers in RANGES, and a range that spans a
			// folded stretch keeps every second of it. One run came back with
			// 39.28-108.44 and 113.28-164.00 either side of a mark at
			// 106.16-111.36 -- it had clearly aimed at the seam and missed it by
			// two seconds, which is the whole of the doubled sentence.
			//
			// After the snap and not before it, because snapEdge moves an end
			// outward by up to five seconds to find a silence, and outward from
			// the border of a marked stretch is INTO it.
			if n := dropMarked(&a.ed.segs, marks); n > 0 {
				a.logf(">>> %d marked stretch(es) taken out of the cut — said twice, kept once", n)
			}
			// ...and the stretches where nothing was said at all, which are
			// nobody else's job (dropDeadAir)
			if gone := dropDeadAir(&a.ed.segs, a.ed.talk); gone > 0 {
				a.logf(">>> %s of silence taken out of the cut — nothing was said in it", mmss(gone))
			}
			a.ed.coalesce()
			// snapEdge and coalesce just moved the boundaries the effects
			// were placed against. The clamp is the guarantee -- whichever
			// pass proposed an effect, it lands inside the cut as applied,
			// or not at all.
			if len(fx) > 0 {
				kept := clampFxToSegs(fx, a.ed.segs)
				if n := len(fx) - len(kept); n > 0 {
					a.logf(">>> %d effect(s) pointed at footage the final cut does not keep — dropped", n)
				}
				fx = kept
			}
			// the effects the style proposed, on the page as if drawn by
			// hand. They REPLACE what was there: the moments they decorate
			// are the new suggestion's, and effects pinned to the old choice
			// would decorate stretches no longer in the video. A reply that
			// proposed none changes nothing here, and the pushUndo above
			// takes segments and effects back as one.
			if len(fx) > 0 {
				a.ed.fx = fx
				a.ed.dropFx()
			}
			a.ed.persist()
			a.ed.setBase() // from here on, Revert comes back to this suggestion
			a.progress.SetFraction(1)
			a.setStatus(fmt.Sprintf("suggested %d segments", len(segs)))
			// the length of the VIDEO this makes, effects included, which is
			// what the target was a target for: read straight off the
			// segments it would over-report every speed-up in the answer
			total := a.ed.cutLen()
			a.logf(">>> suggested %d segments, %d:%02d total",
				len(segs), int(total)/60, int(total)%60)
			if len(fx) > 0 {
				a.logf(">>> ...and %d effect(s): the speeds, the captions and the decorations", len(fx))
			}
		})
	}()
}

// keepFilmed drops what cannot be shown: a segment with no recording at either
// end is time nobody has footage of. The suggestion goes
// through it, so neither can put a hole in the video.
func (a *App) keepFilmed(segs []cutSeg) []cutSeg {
	var out []cutSeg
	for _, s := range segs {
		// an insert brings its own picture, which is the entire point of it --
		// "no recording under it" is its normal state, not a fault
		if !s.isInsert() && a.ed.videoAt(s.S) == nil && a.ed.videoAt(s.E) == nil {
			continue
		}
		out = append(out, s)
	}
	return out
}

// minSuggestSegs is how few segments a suggestion may come back with before it
// is arithmetic nonsense rather than a choice. Four made sense when every cut
// was minutes long; a Short's count is however many beats its notes name, and
// rejecting a two-segment answer to a 25 s target burns attempts on a rule
// the target itself contradicts.
func minSuggestSegs(target float64) int {
	if n := 1 + int(target/30); n < 4 {
		return n
	}
	return 4
}

// maxSuggestSegs is the other end of the same question, and it exists because
// of one failure: an answer of 548 segments, the first five real moments and
// the rest a mechanical march of 24-second blocks every 32 seconds, running
// eleven times past the end of the session until the token ceiling chopped it
// mid-number. Nothing rejected it for its SHAPE -- the segments past the end
// were dropped for having no footage, the total then failed the length gate,
// and the reason the model was told was arithmetic when the reason was that it
// had stopped cutting and started counting.
//
// Generous: the wordings ask for about one segment per 20 seconds of target,
// and this is four times that with a floor, so a cut that really is made of
// many short moments is not refused. What it catches is the runaway, which
// misses by an order of magnitude and not by a few.
func maxSuggestSegs(target float64) int {
	if n := int(target / 5); n > 40 {
		return n
	}
	return 40
}

// ctxLength is a length the user context names for the finished video, in
// seconds, and whether it names one at all. "12min", "~15 min", "5 minutes",
// "90 s", "90 seconds": a number with a unit of time on it. A bare number is
// not a length -- "500 wins" and "level 2" are not durations -- and the first
// one with a unit is taken, because a context that names two is a context
// whose author knows which one they meant.
func ctxLength(ctx string) (float64, bool) {
	m := ctxLenRe.FindStringSubmatch(ctx)
	if m == nil {
		return 0, false
	}
	n, err := strconv.ParseFloat(m[1], 64)
	if err != nil || n <= 0 {
		return 0, false
	}
	if strings.HasPrefix(strings.ToLower(m[2]), "m") {
		n *= 60
	}
	return n, true
}

var ctxLenRe = regexp.MustCompile(`(?i)~?\b(\d+(?:[.,]\d+)?)\s*(min(?:ute)?s?|m|s|sec(?:ond)?s?)\b`)

// footageWindow is how much SESSION the cut may keep. The floor is the
// finished video's own -- footage under it cannot fill the target at any rate
// -- and the ceiling is what the speed pass could squeeze into the target at
// the fastest rate it uses, so a cut inside it is one the arithmetic can still
// land. Wide on purpose: which of those seconds are worth keeping is the cut's
// judgement, and how many of them run fast is a later call's.
func (a *App) footageWindow(target float64) (lo, hi float64) {
	lo, hi = a.suggestWindow(target)
	return lo, hi * maxSpeedRate
}

// maxSpeedRate is the fastest rate the speed pass is expected to use, and
// therefore how much footage a target can swallow. 4 and not 8: eight is for a
// minute of loading screen, not for a whole video, and a ceiling built on it
// would accept cuts nothing could make watchable.
const maxSpeedRate = 4.0

// suggestWindow is how far a suggestion's total may drift from the target. A
// long cut is a wish (half over is fine); under a minute it is a promise, so
// the ceiling is a fifth over -- a 25-second target must not ship as 37. The
// floor is shared.
func (a *App) suggestWindow(target float64) (lo, hi float64) {
	if target <= shortTarget {
		return target * 0.6, target * 1.2
	}
	return target * 0.6, target * 1.5
}

// shortTarget is where a target stops being a wish and becomes a format. Under
// a minute is a Short, a teaser, a clip for a post: a length somebody promised
// somewhere, and 45 seconds of "30 second" video is a different thing from
// what was asked for. Minutes-long cuts land where the material lets them.
const shortTarget = 60.0

// sugFx is one effect as a cut style's reply spells it: a kind, the stretch of
// session seconds it covers, and the one number that kind needs. Every style
// asks for these; a reply without the list parses to none.
type sugFx struct {
	Kind       string
	Start, End float64
	Rate       float64
	// a pointer because 0 is a gain somebody can mean. Silence is the whole
	// reason a session says "do not use this audio", and as a plain float64
	// that instruction and a volume effect with no gain field at all arrive
	// here as the same 0 -- so obeying one meant obeying the other, and the
	// parser chose to obey neither. Absent is nil; explicit is a number.
	Gain *float64
	Text string
}

// speedGapMin is how close two speed stretches at the same rate may come
// before they are one stretch. A cut once came back with two ×4 runs a second
// apart -- a one-second island of normal speed between two fast ones, which
// on screen is a hiccup and on the lane is two badges jammed together. Under
// this many seconds of ordinary footage between them, the island goes too.
const speedGapMin = 4.0

// joinSpeeds folds speed effects at one rate that run into each other, or
// nearly, into one. The rest of the list is untouched and the order is kept.
func joinSpeeds(fx []cutFx) []cutFx {
	var speeds, rest []cutFx
	for _, f := range fx {
		if f.Kind == "speed" && f.Rate > 0 {
			speeds = append(speeds, f)
		} else {
			rest = append(rest, f)
		}
	}
	sort.SliceStable(speeds, func(i, j int) bool { return speeds[i].T < speeds[j].T })
	var out []cutFx
	for _, f := range speeds {
		if n := len(out); n > 0 && out[n-1].Rate == f.Rate &&
			f.T-(out[n-1].T+out[n-1].Dur) < speedGapMin {
			end := math.Max(out[n-1].T+out[n-1].Dur, f.T+f.Dur)
			out[n-1].Dur = end - out[n-1].T
			out[n-1].Tout = f.Tout // the fade out is the later stretch's
			continue
		}
		out = append(out, f)
	}
	return append(out, rest...)
}

// fxFromReply turns proposed effects into cutFx: the model chooses WHEN and
// WHICH KIND, the app's own defaults decide HOW. Nonsense entries are dropped,
// not fatal. svg cannot be asked for (a file on this machine).
// fxMaxProposed caps a reply's effects at the wording's rate over a cut longer
// than anyone makes.
const fxMaxProposed = 1000 // effectively none: how many effects a cut carries is the prompt's call, not this file's

func fxFromReply(in []sugFx) []cutFx { return fxFrom(in, fxMaxProposed) }

// fxFrom is fxFromReply with the cap as an argument: the reply's own effects
// are decorations and are held to fxMaxProposed; the speeds a cut is built
// from are not decorations and are held to nothing, or a cut of sixty
// sped-up stretches has thirty-six of them quietly running at 1.
func fxFrom(in []sugFx, cap int) []cutFx {
	var out []cutFx
	for _, f := range in {
		if cap > 0 && len(out) == cap {
			break
		}
		d := f.End - f.Start
		switch strings.ToLower(strings.TrimSpace(f.Kind)) {
		case "zoom":
			if d <= 0 {
				continue
			}
			g := math.Min(1, d/3) // the dialog's second of glide, shrunk to fit a short zoom
			out = append(out, cutFx{Kind: "zoom", T: f.Start, Dur: d,
				Cx: 0.5, Cy: 0.5, Hf: 0.6, Trans: g, Tout: g})
		case "speed":
			if d <= 0 {
				continue
			}
			rate := f.Rate
			if rate <= 0 {
				rate = 0.5 // an unnamed rate means slow-mo; 0 would be a freeze nobody asked for
			}
			rate, d = clampSpeed(rate, d)
			// the same fades a suggested zoom and caption get, on the same
			// terms: a second either side where the stretch is long enough to
			// spend it, and nothing where the render could not build the
			// stairs anyway -- which at a fast rate wants more footage than a
			// second, because a stair is measured on the finished video
			ramp := math.Min(1, d/4)
			if ramp < rampStep*math.Max(1, rate) {
				ramp = 0
			}
			out = append(out, cutFx{Kind: "speed", T: f.Start, Dur: d, Rate: rate,
				Trans: ramp, Tout: ramp})
		case "stop":
			// the held frame: the picture stands still at f.Start while the
			// footage runs on under it, which is a speed of 0 (frozenFx). It
			// is a kind of its own in the reply because a rate cannot say it
			// -- an omitted rate and a rate of 0 are the same JSON number,
			// and reading that as a freeze would hand a still to every reply
			// that forgot to name a speed
			if d <= 0 {
				d = 2 // long enough to read as a hold, short enough not to strand the viewer
			}
			fade := math.Min(0.3, d/4)
			out = append(out, cutFx{Kind: "speed", T: f.Start, Dur: d, Rate: 0,
				Trans: fade, Tout: fade})
		case "volume":
			if d <= 0 {
				continue
			}
			// a gain nobody named is a field left out, and there is nothing
			// to do about it; 1 is dropped for the plainer reason that it
			// does nothing. An explicit 0 is kept -- that is a stretch the
			// session wanted silent.
			if f.Gain == nil || *f.Gain == 1 {
				continue
			}
			// the same second either way that a suggested zoom and caption get
			ramp := math.Min(1, d/4)
			out = append(out, cutFx{Kind: "volume", T: f.Start, Dur: d,
				Gain: clampGain(*f.Gain), Trans: ramp, Tout: ramp})
		case "text":
			txt := strings.TrimSpace(f.Text)
			if txt == "" {
				continue // a caption with no words is nothing to draw
			}
			if d <= 0 {
				d = 3 // the dialog's own opening duration
			}
			fade := math.Min(0.3, d/4)
			out = append(out, cutFx{Kind: "text", T: f.Start, Dur: d,
				Trans: fade, Tout: fade, Text: txt})
		}
	}
	return out
}

func (a *App) suggestCut(session string, target, span float64) ([]cutSeg, []cutFx, error) {
	system := a.sysPrompt("cut")
	// With a target: the range, not just the number. Told "300 seconds" alone, a
	// model spends the call adding and dropping segments to hit it exactly and
	// runs out of room for the answer (once: 85 kB of arithmetic, no JSON). From
	// the same function the validator uses, so prompt and gate cannot drift.
	//
	// With none: say there is none. A length nobody asked for is not a weaker
	// instruction than a real one -- it is the instruction, and the model obeys
	// it by leaving out material the person wanted in.
	length := "NO TARGET LENGTH. The user context names none, so there is none: keep every " +
		"moment worth keeping and the video is as long as that comes to. Nothing is dropped to " +
		"reach a length, and nothing is kept to fill one."
	if target > 0 {
		lo, hi := a.footageWindow(target)
		length = fmt.Sprintf("TARGET LENGTH: %.0f seconds of finished video, which is the length "+
			"named in the user context. The dull stretches are played fast afterwards, so KEEP "+
			"between %.0f and %.0f seconds of footage, in at most %d segments. Stop at the "+
			"first set of moments that lands in that range.", target, lo, hi, maxSuggestSegs(target))
	}
	// ...and how long the session it is choosing from actually runs. Nothing
	// used to state where the session ENDS: three attempts in a row once came
	// back with segments marching to 28999 -- eight hours of a 28-minute
	// recording -- because a model that has lost its place has nothing in the
	// request to lose its place against. Both spellings here as on every
	// timeline line (stamp), since a clock read as a count of seconds is where
	// a run of plausible numbers turns into nonsense.
	user := a.ctxBlockFor("cut") + fmt.Sprintf("SESSION LENGTH: %.0f seconds, which the timeline "+
		"writes as %s. Every start and end you give is a number of SECONDS between 0 and "+
		"%.0f.\n\n%s\n\nSESSION TIMELINE:\n%s", span, mmss(span), span, length, session)
	msgs := []map[string]any{msg("system", system), msg("user", user)}
	// the web, for a caption that names a thing the timeline does not explain
	tools, ffx := a.webToolsFor("suggest")
	// The bar, while a call that takes minutes runs: segments counted as they
	// close, placed by where in the session they land (see jsonItemsDone).
	// Only ever forward -- a rejected attempt starts the count again, and a bar
	// that fell back to the first minute would read as work being undone rather
	// than redone.
	best := 0.0
	onText := func(s string) {
		n, through := jsonItemsDone(s, "segments")
		if n == 0 {
			return // still thinking, and the pulse says that better than a 0 does
		}
		f := float64(n) / suggestMaxSegs // no timeline to place them on: count
		if span > 0 && through > 0 {
			f = through / span
		}
		// never quite 0: a first segment two minutes into a two-hour session is
		// news, and reporting it as nothing would leave the pulse running
		f = math.Max(0.02, math.Min(1, f))
		if f <= best {
			return
		}
		best = f
		if span > 0 {
			a.prog(trackSTT, suggestChooseShare*f, "%d moments, at %s of %s",
				n, mmss(through), mmss(span))
		} else {
			a.prog(trackSTT, suggestChooseShare*f, "%d moments", n)
		}
	}
	// Thinking first, execute mode only after an attempt comes back empty
	// (thinkAgain): thinking calls answered in about ten minutes over three days
	// of runs; execute-mode ones answered in two and worse.
	think := true
	for try := 0; try < 3; try++ {
		if err := a.checkpoint(); err != nil {
			return nil, nil, err
		}
		reply, err := a.llmChatRetryTools("suggest", msgs, think, tools, a.webRunner("suggest", ffx), onText)
		if err != nil {
			return nil, nil, err
		}
		segs, fx, problem := a.checkCutReply(reply, target, span, try+1)
		if problem == "" {
			return segs, fx, nil
		}
		a.logfIdle(">>> suggest attempt %d rejected: %s", try+1, problem)
		// the search goes with the first refusal. The tools are there for a
		// caption that names something the timeline does not explain, which is
		// a detail of an answer -- and a model that has just failed to write
		// the answer spends the retry searching instead: one run went eight
		// rounds deep into a wiki and ran out of rounds without ever proposing
		// a segment. Everything the cut needs is in the timeline it was given.
		if tools != nil {
			a.logfIdle(">>> suggest: asking again without the web tools")
			tools = nil
		}
		if next := thinkAgain(think, reply); next != think {
			think = next
			a.logfIdle(">>> suggest: the model spent the whole call thinking and wrote " +
				"nothing — asking again with thinking off")
		}
		msgs = retryTurn(msgs, reply, problem)
	}
	return nil, nil, fmt.Errorf("no valid cut after 3 attempts")
}

// checkCutReply reads one answer to the cut and says everything wrong with it
// -- one fault per sentence, worst first -- or hands back the cut. Everything,
// not the first thing: there are only three attempts, and an answer with three
// faults gets one round to fix them all.
func (a *App) checkCutReply(reply string, target, span float64, attempt int) ([]cutSeg, []cutFx, string) {
	var out struct {
		// speed and rate on a SEGMENT are the same thing said where a model
		// keeps saying it: a stretch that runs at 4 is one segment with one
		// number on it, which is exactly what the wording asks for and not
		// quite the shape the reply names. It is read as the speed effect it
		// means rather than thrown away -- thrown away, the arithmetic the
		// model did with it was right and the total it was told was wrong.
		Segments []struct{ Start, End, Speed, Rate float64 } `json:"segments"`
		Fx       []sugFx                                     `json:"fx"`
	}
	if problem := jsonReply(reply, &out); problem != "" {
		return nil, nil, problem
	}
	// a rate on a segment: 1 is the ordinary rate and says nothing, and any
	// other is a speed effect over the whole segment. They are kept APART from
	// the reply's own effects -- they are how the cut is built, not decoration
	// on it, so they are neither counted against the effect ceiling nor cut
	// off by fxMaxProposed, which once dropped every one of sixty and had the
	// total counted at the footage's own length
	var segSpeeds []sugFx
	for _, s := range out.Segments {
		if r := math.Max(s.Speed, s.Rate); r > 0 && math.Abs(r-1) > 1e-9 {
			segSpeeds = append(segSpeeds, sugFx{Kind: "speed", Start: s.Start, End: s.End, Rate: r})
		}
	}
	var probs []string

	// timestamps past the end of the recording, segments and effects alike.
	// Said first, because it is the fault underneath the others: a cut whose
	// seconds run past the end fails the length gate too, and being told the
	// total sends the next attempt to rebalance an answer whose real problem
	// is that it read the timeline's stamps as decimals. That is what these
	// numbers are, every time -- 2804 for the line stamped [28:04] -- so the
	// message does the one conversion the model got wrong, on its own number.
	past, first := 0, 0.0
	for _, s := range out.Segments {
		if span > 0 && s.Start >= span {
			if past == 0 {
				first = s.Start
			}
			past++
		}
	}
	for _, f := range append(append([]sugFx(nil), out.Fx...), segSpeeds...) {
		if span > 0 && f.Start >= span {
			if past == 0 {
				first = f.Start
			}
			past++
		}
	}
	if past > 0 {
		probs = append(probs, fmt.Sprintf("%d of your timestamps lie after the session ends at %.0f s%s",
			past, span, mmssHint(first, span)))
	}

	if len(out.Segments) < minSuggestSegs(target) {
		probs = append(probs, fmt.Sprintf("fewer than %d segments", minSuggestSegs(target)))
	} else if n := maxSuggestSegs(target); len(out.Segments) > n {
		// said as a shape problem, because that is what it is: an answer
		// this long is a model that stopped choosing moments
		probs = append(probs, fmt.Sprintf("%d segments, which is not a cut -- keep it under %d, "+
			"and use a speed effect over one long segment where a stretch has to be "+
			"shown but not watched", len(out.Segments), n))
	}

	var segs []cutSeg
	for _, s := range out.Segments {
		if s.End <= s.Start {
			probs = append(probs, "a segment ends before it starts")
			break
		}
		segs = append(segs, cutSeg{S: s.Start, E: s.End})
	}
	// only video-backed time counts, and it is counted after the drop: a
	// suggestion that spent half its length on stretches nobody filmed is
	// short, and being told the number it actually landed on is what makes
	// the next attempt aim elsewhere
	asked := len(segs)
	segs = a.keepFilmed(segs)
	if n := asked - len(segs); n > 0 {
		a.logfIdle(">>> suggest attempt %d: %d segment(s) dropped for having no footage", attempt, n)
	}
	// on FOOTAGE, not on finished length: how fast it plays is the speed
	// pass's answer and does not exist yet. The ceiling is what that pass
	// could compress into the target at the fastest ordinary rate, so a cut
	// this side of it is one the arithmetic can still land.
	fx := joinSpeeds(append(fxFrom(segSpeeds, 0), fxFromReply(out.Fx)...))
	raw := 0.0
	for _, s := range segs {
		raw += s.E - s.S
	}
	// only against a target the user named: with none there is no range, and a
	// cut that keeps the whole of a script is the right answer rather than an
	// answer that missed one
	if lo, hi := a.footageWindow(target); target > 0 && (raw < lo || raw > hi) {
		probs = append(probs, fmt.Sprintf("%.0f s of footage, where %.0f to %.0f is accepted "+
			"(the dull stretches are played fast afterwards, which is what makes the "+
			"upper end reachable)", raw, lo, hi))
	}
	if len(probs) > 0 {
		return nil, nil, strings.Join(probs, "; ")
	}
	return segs, fx, ""
}

// mmssHint is the conversion a stamp-read-as-a-decimal got wrong, done on the
// model's own number: 2804 came from [28:04], which is 28*60+4. Empty when the
// number does not read as a stamp at all, rather than a hint that is itself a
// guess.
func mmssHint(n, span float64) string {
	mm, ss := int(n)/100, int(n)%100
	if ss >= 60 || mm <= 0 {
		return ""
	}
	secs := mm*60 + ss
	if float64(secs) > span {
		return ""
	}
	return fmt.Sprintf(" -- %.0f is not a second: a stamp [%02d:%02d] is mm*60+ss, %d",
		n, mm, ss, secs)
}

// clampFxToSegs holds a proposed effect to the cut as it will actually play:
// inside a footage segment, or gone. It has to run against the segments AS
// APPLIED -- after snapEdge, after coalesce -- because every
// one of those moves boundaries after the effects were chosen, and a rule
// enforced any earlier is enforced against a cut that no longer exists.
// Inserts do not count as home: they bring their own picture, and an effect
// proposed off the timeline was never about one.
func clampFxToSegs(fx []cutFx, segs []cutSeg) []cutFx {
	var out []cutFx
	for _, f := range fx {
		t0, t1 := f.fxSpan()
		// a point -- a bare view -- has no width to overlap with; being
		// inside a segment is the whole question
		if t1 <= t0 {
			for _, s := range segs {
				if !s.isInsert() && s.S <= t0 && t0 <= s.E {
					out = append(out, f)
					break
				}
			}
			continue
		}
		// the footage segment this effect shares the most time with
		best, overlap := -1, 0.0
		for i, s := range segs {
			if s.isInsert() {
				continue
			}
			if o := math.Min(t1, s.E) - math.Max(t0, s.S); o > overlap {
				best, overlap = i, o
			}
		}
		if best < 0 {
			continue // no kept footage under it at all
		}
		s := segs[best]
		if t0 >= s.S && t1 <= s.E {
			out = append(out, f) // already inside: exactly as proposed
			continue
		}
		was := t1 - t0 // the band before the cut took a piece off it
		nt0, nt1 := math.Max(t0, s.S), math.Min(t1, s.E)
		if nt1-nt0 < 1 {
			// under a second left is not the effect that was meant: most of
			// the moment it decorated is not in the video
			continue
		}
		f.T, f.Dur = nt0, nt1-nt0
		switch f.Kind {
		case "zoom", "text", "svg", "volume":
			// nothing kind-specific: the band is the band, and the fades
			// inside it are shrunk with it below
		case "speed":
			// a rate has to stay playable over the band it was trimmed to.
			// A stop is not a rate and gets none: clampSpeed would hand the
			// frozen frame one, which is a stop that quietly became slow
			// motion because the cut moved under it.
			if !f.frozenFx() {
				f.Rate, f.Dur = clampSpeed(f.Rate, f.Dur)
			}
		default:
			continue // a kind with no span to trim
		}
		trimFades(&f, was)
		out = append(out, f)
	}
	return out
}

// ---- the passes after the cut ----------------------------------------------
//
// Cut first, then captions, speed and effects over the kept clips in the clips'
// own seconds (offsets from the clip's start, never session stamps). One reply
// for all of it ran out of tokens or wrote nothing.

// captionBatch is how many clips one captions request carries. Five is a few
// paragraphs in and a few dozen lines out -- small enough that a bad answer
// costs a minute, not a run.
const captionBatch = 5

// captionCut is the captions pass: every spoken line over the kept clips as a
// text effect, batch by batch. A batch that fails twice is skipped and said
// so; the cut stands either way.
func (a *App) captionCut(segs []cutSeg, rows []tsvRow) []cutFx {
	var out []cutFx
	narr := a.narratorMic()
	system := a.sysPrompt("captions")
	batches := (len(segs) + captionBatch - 1) / captionBatch
	for b := 0; b*captionBatch < len(segs); b++ {
		if err := a.checkpoint(); err != nil {
			return out
		}
		lo, hi := b*captionBatch, min(len(segs), (b+1)*captionBatch)
		batch := segs[lo:hi]
		a.prog(trackSTT, 0.7+0.2*float64(b)/float64(batches), "captions, clips %d–%d of %d", lo+1, hi, len(segs))
		brief := clipBriefsWith(batch, rows, nil, narr, func(i int, s cutSeg) string {
			return fmt.Sprintf("CLIP %d: %.0f s long", lo+i+1, s.E-s.S)
		})
		user := a.ctxBlockFor("captions") + "THE CLIPS, AND WHAT WAS SAID OVER EACH:\n" + brief
		msgs := []map[string]any{msg("system", system), msg("user", user)}
		for try := 0; try < 2; try++ {
			reply, err := a.llmChatRetryOn("captions", msgs, false, nil)
			if err != nil {
				a.logfIdle("!!! captions: clips %d–%d: %v", lo+1, hi, err)
				break
			}
			fx, problem := captionsFromReply(batch, lo, reply)
			if problem == "" {
				out = append(out, fx...)
				a.logfIdle(">>> captions: clips %d–%d: %d caption(s)", lo+1, hi, len(fx))
				break
			}
			a.logfIdle(">>> captions: clips %d–%d rejected: %s", lo+1, hi, problem)
			if try == 1 {
				a.logfIdle("!!! captions: clips %d–%d skipped -- the cut stands without them", lo+1, hi)
			}
			msgs = retryTurn(msgs, reply, problem)
		}
	}
	return out
}

// captionsFromReply reads one batch's answer into text effects on the session
// clock. Clip numbers are as the brief gave them (first is number first+1);
// seconds are offsets from the clip's start and are held inside the clip. A
// clip the reply does not mention has no captions, which is an answer.
func captionsFromReply(batch []cutSeg, first int, reply string) ([]cutFx, string) {
	var out struct {
		Clips []struct {
			I  int `json:"i"`
			Fx []struct {
				Start, End float64
				Text       string
			} `json:"fx"`
		} `json:"clips"`
	}
	if p := jsonReply(reply, &out); p != "" {
		return nil, p
	}
	var fx []cutFx
	for _, c := range out.Clips {
		k := c.I - first - 1
		if k < 0 || k >= len(batch) {
			return nil, fmt.Sprintf("clip %d is not one of the clips given (%d to %d)", c.I, first+1, first+len(batch))
		}
		seg := batch[k]
		for _, e := range c.Fx {
			txt := strings.TrimSpace(e.Text)
			if txt == "" {
				continue
			}
			st, en := math.Max(0, e.Start), math.Min(seg.E-seg.S, e.End)
			if en-st < 0.3 {
				continue // a caption nobody could read, or one entirely past the clip
			}
			d := en - st
			fade := math.Min(0.3, d/4)
			fx = append(fx, cutFx{Kind: "text", T: seg.S + st, Dur: d, Trans: fade, Tout: fade, Text: txt})
		}
	}
	return fx, ""
}

// decorateCut is the effects pass: zooms, stops and volume in one request over
// every kept clip; a failed answer leaves the cut plain. It is told the
// captions (CAPTION lines) and each clip's rate, since both change the answer.
func (a *App) decorateCut(segs []cutSeg, rows []tsvRow, done []cutFx) []cutFx {
	if len(segs) == 0 {
		return nil
	}
	if err := a.checkpoint(); err != nil {
		return nil
	}
	rate := make([]float64, len(segs))
	for _, f := range done {
		if f.Kind != "speed" || f.Rate <= 0 {
			continue
		}
		t0, t1 := f.fxSpan()
		for i, s := range segs {
			if t1 > s.S && t0 < s.E {
				rate[i] = f.Rate
			}
		}
	}
	brief := clipBriefsWith(segs, rows, done, a.narratorMic(), func(i int, s cutSeg) string {
		h := fmt.Sprintf("CLIP %d: %.0f s long", i+1, s.E-s.S)
		if rate[i] > 0 && rate[i] != 1 {
			h += fmt.Sprintf(", plays at %gx", rate[i])
		}
		return h
	})
	user := a.ctxBlockFor("effects") + "THE CLIPS, AND WHAT WAS SAID AND SHOWN OVER EACH:\n" + brief
	msgs := []map[string]any{msg("system", a.sysPrompt("effects")), msg("user", user)}
	for try := 0; try < 2; try++ {
		reply, err := a.llmChatRetryOn("effects", msgs, false, nil)
		if err != nil {
			a.logfIdle("!!! effects: %v", err)
			return nil
		}
		fx, problem := decorationsFromReply(segs, reply)
		if problem == "" {
			a.logfIdle(">>> effects: %d decoration(s)", len(fx))
			return fx
		}
		a.logfIdle(">>> effects rejected: %s", problem)
		msgs = retryTurn(msgs, reply, problem)
	}
	a.logfIdle("!!! effects: no usable answer -- the cut stands without them")
	return nil
}

// decorationsFromReply reads the effects pass: each named to a clip, offsets
// from that clip's start, held inside it. The kinds the pass owns and no
// others -- a speed or a caption here is written elsewhere and is dropped.
func decorationsFromReply(segs []cutSeg, reply string) ([]cutFx, string) {
	var out struct {
		Fx []struct {
			Clip       int
			Kind       string
			Start, End float64
			Gain       *float64
		} `json:"fx"`
	}
	if p := jsonReply(reply, &out); p != "" {
		return nil, p
	}
	var in []sugFx
	for _, e := range out.Fx {
		k := e.Clip - 1
		if k < 0 || k >= len(segs) {
			return nil, fmt.Sprintf("clip %d is not one of the clips given (1 to %d)", e.Clip, len(segs))
		}
		kind := strings.ToLower(strings.TrimSpace(e.Kind))
		if kind != "zoom" && kind != "stop" && kind != "volume" {
			continue
		}
		seg := segs[k]
		st, en := math.Max(0, e.Start), math.Min(seg.E-seg.S, e.End)
		if en <= st {
			continue
		}
		in = append(in, sugFx{Kind: kind, Start: seg.S + st, End: seg.S + en, Gain: e.Gain})
	}
	return fxFrom(in, 0), ""
}

// ---- the speed pass ---------------------------------------------------------
//
// A rate per clip, asked once the cut and its captions stand: a rate on a
// captioned clip is dropped here (words at 4 are unreadable). It is handed no
// target -- how much to speed up is the user context's to say (speedSystem);
// the cut's length is the cut pass's gate (footageWindow).

// speedCut asks for a rate per clip and returns the speed effects. A failure
// leaves the cut at 1 throughout, which is a longer video and a whole one.
func (a *App) speedCut(segs []cutSeg, caps []cutFx, rows []tsvRow) []cutFx {
	if len(segs) == 0 {
		return nil
	}
	if err := a.checkpoint(); err != nil {
		return nil
	}
	capped := make([]int, len(segs)) // how many captions each clip carries
	for _, f := range caps {
		for i, s := range segs {
			if f.T >= s.S && f.T < s.E {
				capped[i]++
			}
		}
	}
	// ...and the clip's own lines: what was said and shown, at the seconds they
	// happened. Two counts (lines spoken, captions) could not tell a silent walk
	// back from a silent final approach; the lines can.
	raw := 0.0
	for _, s := range segs {
		raw += s.E - s.S
	}
	brief := clipBriefsWith(segs, rows, caps, a.narratorMic(), func(i int, s cutSeg) string {
		h := fmt.Sprintf("CLIP %d: %.0f s long", i+1, s.E-s.S)
		if capped[i] > 0 {
			h += fmt.Sprintf(", %d caption(s) -- runs at 1", capped[i])
		}
		return h
	})
	user := a.ctxBlockFor("speed") + fmt.Sprintf("FOOTAGE: %.0f seconds over %d clips.\n\n"+
		"THE CLIPS, AND WHAT WAS SAID AND SHOWN OVER EACH:\n%s", raw, len(segs), brief)
	msgs := []map[string]any{msg("system", a.sysPrompt("speed")), msg("user", user)}
	for try := 0; try < 2; try++ {
		reply, err := a.llmChatRetryOn("speed", msgs, false, nil)
		if err != nil {
			a.logfIdle("!!! speed: %v", err)
			return nil
		}
		fx, problem := speedsFromReply(segs, capped, reply)
		if problem == "" {
			// no length gate: "no clip runs fast" is a right answer to a cut
			// with nothing dull in it, and the length is the cut pass's
			a.logfIdle(">>> speed: %d clip(s) run fast — %s of video from %s of footage",
				len(fx), mmss(cutLen(applyFx(segs, fx))), mmss(raw))
			return fx
		}
		a.logfIdle(">>> speed rejected: %s", problem)
		msgs = retryTurn(msgs, reply, problem)
	}
	a.logfIdle("!!! speed: no usable answer — every clip plays at 1")
	return nil
}

// speedsFromReply reads a rate per clip into speed effects over whole clips.
//
// A rate ABOVE 1 on a captioned clip is dropped: the words on it would be
// unreadable, and that rule is not the model's to weigh against the
// arithmetic. Below 1 is slow motion and is kept -- it costs the target
// seconds rather than saving them, which is the arithmetic's problem and is
// caught by the length gate, and a caption over a slowed clip is on screen
// longer rather than shorter.
//
// What is SAID over a clip is not enforced here, only asked for (speedSystem):
// the user context can ask for more speed than the silent clips can give, and
// the wording says what to take then.
func speedsFromReply(segs []cutSeg, capped []int, reply string) ([]cutFx, string) {
	var out struct {
		Speeds []struct {
			Clip int
			Rate float64
		} `json:"speeds"`
	}
	if p := jsonReply(reply, &out); p != "" {
		return nil, p
	}
	var in []sugFx
	for _, sp := range out.Speeds {
		i := sp.Clip - 1
		if i < 0 || i >= len(segs) {
			return nil, fmt.Sprintf("clip %d is not one of the clips given (1 to %d)", sp.Clip, len(segs))
		}
		if sp.Rate <= 0 || math.Abs(sp.Rate-1) < 0.01 {
			continue // 1 is the footage's own speed and says nothing
		}
		if sp.Rate > 1 && capped[i] > 0 {
			// words on screen: this clip runs at 1, whatever was asked for.
			// Only the fast half of the rule -- a caption over a clip in slow
			// motion is on screen longer, not shorter.
			continue
		}
		in = append(in, sugFx{Kind: "speed", Start: segs[i].S, End: segs[i].E, Rate: sp.Rate})
	}
	return joinSpeeds(fxFrom(in, 0)), ""
}

// seamMax is the widest hole that can be a rounding artifact rather than an
// edit. The timeline is stamped in whole seconds, so the finest cut a reply can
// express is one -- and "this stretch carries on" comes out as ...86], [87...
// with whatever fell in that second falling out of the video.
const seamMax = 1.5

// joinSeams closes the holes a reply did not mean to open.
//
// One run asked for 21 holes. Eleven were exactly one second, and eight of
// those had a WORD in them -- "new algorithms.", "deprecated.", "once,",
// "that wallet", "global North" -- each of them the middle of a sentence the
// cut kept both sides of. They are not edits; they are the model saying the
// stretch continues in the only vocabulary it has.
//
// Ten of the eleven were closed anyway, by luck: both edges usually snap to the
// same pause and coalesce joins what touches. The one where they did not lost
// the word "once". This is that repair done on purpose, before the snapping
// rather than as a side effect of it.
//
// A short hole with nothing but silence in it is left alone: that one may well
// be a trim, and the seconds it drops are seconds nobody spoke in.
func joinSeams(segs []cutSeg, rows []tsvRow) []cutSeg {
	if len(segs) < 2 {
		return segs
	}
	out := []cutSeg{segs[0]}
	for _, s := range segs[1:] {
		last := &out[len(out)-1]
		if s.S-last.E > 0 && s.S-last.E <= seamMax && wordsBetween(rows, last.E, s.S) {
			last.E = s.E
			continue
		}
		out = append(out, s)
	}
	return out
}

// wordsBetween is whether anybody was talking between two seconds of the
// session -- the EVENT lines are the picture and say nothing about that.
func wordsBetween(rows []tsvRow, t0, t1 float64) bool {
	for _, r := range rows {
		if r.spk != "EVENT" && r.e > t0 && r.s < t1 {
			return true
		}
	}
	return false
}

// dropMarked takes every marked stretch out of the cut, cutting the scenes that
// span one in two, and says how many it acted on.
//
// The marks are facts about the material (retake.go): those seconds were said
// again, and no arrangement of segments may keep them. What is left too short
// to be a scene goes with them -- a sliver either side of a stumble is not a
// shot, it is the frames the stumble was wearing.
// dropDeadAir takes the long silences out of the clips, and returns how much
// went. Nothing else does.
//
// A retake is found by the words that are said twice, and a man who stops
// talking for half a minute and then carries on has said nothing twice: there
// is no repeat to find. The cut will not do it either. Asked with no target
// length its whole editorial act turns out to be leaving out what the markers
// name -- one run answered with five segments running end to end across the
// entire session, which is not a cut at all -- so a session's dead ends come
// through it untouched. One recording here held 31 seconds of nobody speaking
// in the middle of a sentence, another 13 at the end of a take, and both were
// in the finished video.
//
// A beat is LEFT where each one was (deadAirKeep). Cutting from the last word
// straight to the next is a jump; a breath is what the pause was before it
// went on too long.
func dropDeadAir(segs *[]cutSeg, talk [][2]float64) float64 {
	var out []cutSeg
	gone := 0.0
	for _, s := range *segs {
		if s.isInsert() {
			out = append(out, s)
			continue
		}
		at := s.S
		for _, q := range quietWithin(talk, s.S, s.E) {
			if q[1]-q[0] < deadAirMax {
				continue
			}
			e, next := q[0]+deadAirKeep/2, q[1]-deadAirKeep/2
			if e > at {
				out = append(out, cutSeg{S: at, E: e, Cam: s.Cam, Quiet: s.Quiet})
			}
			gone += next - math.Max(at, e)
			at = next
		}
		if s.E > at {
			out = append(out, cutSeg{S: at, E: s.E, Cam: s.Cam, Quiet: s.Quiet})
		}
	}
	var keep []cutSeg
	for _, s := range out {
		if s.isInsert() || s.E-s.S >= minSegLn {
			keep = append(keep, s)
		}
	}
	*segs = keep
	return gone
}

// quietWithin is the stretches of t0..t1 with no speech in them: the
// complement of talk, which is every line of the transcript on the session
// clock and is already sorted (loadSources).
func quietWithin(talk [][2]float64, t0, t1 float64) [][2]float64 {
	var out [][2]float64
	at := t0
	for _, sp := range talk {
		if sp[1] <= at || sp[0] >= t1 {
			continue
		}
		if sp[0] > at {
			out = append(out, [2]float64{at, math.Min(sp[0], t1)})
		}
		if at = math.Max(at, sp[1]); at >= t1 {
			return out
		}
	}
	return append(out, [2]float64{at, t1})
}

const (
	// silence past this inside a clip is not a pause, it is a man who has
	// stopped talking. A BACKSTOP, and a high one: the pass measures the gap
	// between transcribed words, and a breath or a click of the tongue is
	// audible without being a word, so between two sentences of a read to
	// camera that gap runs 2.5 to 3.7 s as a matter of course. Set at 2.5 it
	// cut a session into twenty-eight pieces. The stretches that were plainly
	// mistakes ran 5 to 31 s -- and every one of those a retake mark now
	// covers, so what is left for this to find is the one nobody said
	// anything twice around, and that one is long.
	deadAirMax = 8.0
	// ...and what is left of one where it goes.
	deadAirKeep = 0.5
)

func dropMarked(segs *[]cutSeg, marks []retake) int {
	hit := 0
	for _, m := range marks {
		to := m.To
		if to < m.E {
			to = m.E // a mark written before the edges were placed
		}
		var out []cutSeg
		cut := false
		for _, s := range *segs {
			if s.isInsert() || s.E <= m.S || s.S >= to {
				out = append(out, s)
				continue
			}
			cut = true
			if s.S < m.S-0.001 {
				out = append(out, cutSeg{S: s.S, E: m.S, Cam: s.Cam, Quiet: s.Quiet})
			}
			if s.E > to+0.001 {
				out = append(out, cutSeg{S: to, E: s.E, Cam: s.Cam, Quiet: s.Quiet})
			}
		}
		if cut {
			hit++
		}
		*segs = out
	}
	var keep []cutSeg
	for _, s := range *segs {
		if s.isInsert() || s.E-s.S >= minSegLn {
			keep = append(keep, s)
		}
	}
	*segs = keep
	return hit
}
