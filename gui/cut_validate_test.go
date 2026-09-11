package main

// What a cut answer is told about itself.
//
// One run's second attempt had three faults -- half its timestamps past the
// end of the recording (stamps read as decimals), the speed it meant written
// onto its segments where the parser dropped it, and a total nowhere near the
// target -- and was told one of them. The check is a function now, it says
// everything it found, and a rate on a segment is read as the speed effect it
// means.

import (
	"fmt"
	"regexp"
	"strconv"
	"strings"
	"testing"
)

func validEd(t *testing.T) *App {
	t.Helper()
	ed := newTestEd(t)
	ed.vids = []tlVideo{{base: "a", path: "a.mkv", start: 0, dur: 1695, interval: 4, fps: 30}}
	ed.relayout()
	ed.a.ed = ed
	return ed.a
}

// A rate on a segment is the speed effect it means: the same seconds, at that
// rate, counted into the total the model is judged by.
func TestASpeedWrittenOnASegmentIsTheEffectItMeans(t *testing.T) {
	a := validEd(t)
	reply := `{"segments":[{"start":0,"end":60},{"start":60,"end":300,"speed":4},{"start":300,"end":360},{"start":360,"end":600,"rate":4}],"fx":[]}`
	segs, fx, problem := a.checkCutReply(reply, 300, 1695, 1)
	if problem != "" {
		t.Fatalf("a cut that lands inside the window with its speeds counted was refused: %s", problem)
	}
	if len(segs) != 4 {
		t.Errorf("%d segments came back, want 4", len(segs))
	}
	speeds := 0
	for _, f := range fx {
		if f.Kind == "speed" && f.Rate == 4 {
			speeds++
		}
	}
	if speeds != 2 {
		t.Errorf("%d speed effects came back for two sped-up segments", speeds)
	}
	// 60 + 240/4 + 60 + 240/4 = 240 s of video, inside 180-450; read without
	// the speeds it would be 600 and refused
	if total := cutLen(applyFx(segs, fx)); total < 235 || total > 245 {
		t.Errorf("the total with the segments' own speeds is %.0f, want about 240", total)
	}
}

// Every fault at once, worst first, with the one conversion done on the
// model's own number.
func TestEveryFaultIsNamedInOneCorrection(t *testing.T) {
	a := validEd(t)
	reply := `{"segments":[{"start":0,"end":100},{"start":100,"end":232,"speed":4},{"start":232,"end":350},{"start":350,"end":1320}],` +
		`"fx":[{"kind":"text","start":2801,"end":2804,"text":"What we want"},{"kind":"text","start":10,"end":14,"text":"ok"}]}`
	// a 150 s target: 90 to 900 seconds of footage, and this keeps 1320
	_, _, problem := a.checkCutReply(reply, 150, 1695, 1)
	for _, want := range []string{
		"1 of your timestamps lie after the session ends at 1695 s",
		"2801 is not a second: a stamp [28:01] is mm*60+ss, 1681",
		"1320 s of footage, where",
		"where 90 to 900 is accepted",
	} {
		if !strings.Contains(problem, want) {
			t.Errorf("the correction does not say %q:\n%s", want, problem)
		}
	}
	// the cut is judged on footage now: how fast it plays is the speed pass's
	// answer and does not exist yet
	if m := regexp.MustCompile(`(\d+) s of footage`).FindStringSubmatch(problem); m == nil {
		t.Errorf("no footage total in the correction:\n%s", problem)
	} else if n, _ := strconv.Atoi(m[1]); n != 1320 {
		t.Errorf("the footage is reported as %d, want 1320", n)
	}
	// and the order is the order of the damage: the stamps before the total,
	// because the total is a consequence of them
	if i, j := strings.Index(problem, "timestamps"), strings.Index(problem, "s of footage"); i < 0 || j < i {
		t.Errorf("the footage is named before the fault underneath it:\n%s", problem)
	}
	// a number that does not read as a stamp gets no invented conversion
	if h := mmssHint(2875, 1695); h != "" {
		t.Errorf("2875 was read as a stamp: %q", h)
	}
	if h := mmssHint(99999, 1695); h != "" {
		t.Errorf("a number past any stamp got a hint: %q", h)
	}
}

// The floors and ceilings still hold, and an empty or truncated answer is
// still named as such rather than parsed.
func TestTheShapeGatesStillHold(t *testing.T) {
	a := validEd(t)
	if _, _, p := a.checkCutReply("", 300, 1695, 1); !strings.Contains(p, "no answer at all") {
		t.Errorf("an empty answer was not named: %q", p)
	}
	if _, _, p := a.checkCutReply(`{"segments":[{"start":0,"end":20`, 300, 1695, 1); !strings.Contains(p, "stopped in the middle") {
		t.Errorf("a truncated answer was not named: %q", p)
	}
	if _, _, p := a.checkCutReply(`{"segments":[{"start":0,"end":300}],"fx":[]}`, 300, 1695, 1); !strings.Contains(p, "fewer than") {
		t.Errorf("one segment was not refused: %q", p)
	}
	var b strings.Builder
	b.WriteString(`{"segments":[{"start":0,"end":60},{"start":60,"end":120},{"start":120,"end":180},{"start":180,"end":240}],"fx":[`)
	for i := 0; i < 40; i++ {
		if i > 0 {
			b.WriteString(",")
		}
		b.WriteString(`{"kind":"text","start":1,"end":2,"text":"x"}`)
	}
	b.WriteString("]}")
	if _, fx, p := a.checkCutReply(b.String(), 300, 1695, 1); p != "" || len(fx) != 40 {
		t.Errorf("forty captions were refused or dropped (%d kept): %q", len(fx), p)
	}
	// the loop hands the reply to the check and takes the cut from it
	src := readSrc(t, "cut_suggest.go")
	for _, want := range []string{
		"segs, fx, problem := a.checkCutReply(reply, target, span, try+1)",
		"return segs, fx, nil",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("cut_suggest.go no longer contains %q", want)
		}
	}
	// and how fast a clip plays has a shape of its own, in the pass that
	// answers it: the cut's reply is the segments alone
	sys := readSrc(t, "syscontext.go")
	if !strings.Contains(sys, `{"speeds":[{"clip":<n>,"rate":<x>}]}`) {
		t.Error("the system context does not show the speed pass what to answer")
	}
	if strings.Contains(sys, `"end":232,"speed":4`) {
		t.Error("the cut's own shape carries a speed again, which is a later pass's answer")
	}
}

// A rate of 1 on a segment is the ordinary rate and not an effect, and the
// speeds a cut is built from are never capped or counted as decoration. One
// answer with sixty segments -- half at 1, half at 4 -- was refused as "91
// effects, a subtitle track" with its total counted at the footage's own
// length: the ones came in as effects, and the fours were cut off by the
// proposal cap before they could be counted.
func TestSegmentSpeedsAreStructureNotDecoration(t *testing.T) {
	a := validEd(t)
	var b strings.Builder
	b.WriteString(`{"segments":[`)
	for i := 0; i < 60; i++ {
		if i > 0 {
			b.WriteString(",")
		}
		rate := 1
		if i%2 == 1 {
			rate = 4
		}
		fmt.Fprintf(&b, `{"start":%d,"end":%d,"speed":%d}`, i*24, i*24+24, rate)
	}
	b.WriteString(`],"fx":[{"kind":"text","start":2,"end":6,"text":"one caption"}]}`)
	_, fx, problem := a.checkCutReply(b.String(), 720, 1695, 1)
	// 30 segments of 24 s at 4, every one of them past the proposal cap, and
	// all of them counted: 720 + 720/4 = 900, inside 432-1080
	if problem != "" {
		t.Fatalf("refused: %s", problem)
	}
	speeds := 0
	for _, f := range fx {
		if f.Kind == "speed" {
			speeds++
		}
	}
	if speeds != 30 {
		t.Errorf("%d speed effects survived for 30 sped-up segments (fxMaxProposed is %d)", speeds, fxMaxProposed)
	}
}

// Two fast stretches a second apart are one. A cut once came back with ×4 runs
// at 470-507 and 508-540 -- a one-second island of normal speed between them,
// a hiccup on screen and two badges jammed together on the lane.
func TestSpeedsASecondApartAreOneSpeed(t *testing.T) {
	fx := joinSpeeds([]cutFx{
		{Kind: "text", T: 5, Dur: 3, Text: "a caption between them stays"},
		{Kind: "speed", T: 508, Dur: 32, Rate: 4},
		{Kind: "speed", T: 470, Dur: 37, Rate: 4}, // ends 507: one second before the next
		{Kind: "speed", T: 600, Dur: 20, Rate: 4}, // 60 s later: its own stretch
		{Kind: "speed", T: 621, Dur: 10, Rate: 2}, // a second later but another rate: kept apart
		{Kind: "speed", T: 700, Dur: 10, Rate: 4},
		{Kind: "speed", T: 713, Dur: 10, Rate: 4}, // three seconds later: joined
		{Kind: "speed", T: 800, Dur: 10, Rate: 4},
		{Kind: "speed", T: 815, Dur: 10, Rate: 4}, // five seconds later: not joined
	})
	var got [][2]float64
	for _, f := range fx {
		if f.Kind == "speed" {
			got = append(got, [2]float64{f.T, f.T + f.Dur})
		}
	}
	want := [][2]float64{{470, 540}, {600, 620}, {621, 631}, {700, 723}, {800, 810}, {815, 825}}
	if len(got) != len(want) {
		t.Fatalf("got %v, want %v", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Errorf("speed %d is %v, want %v", i, got[i], want[i])
		}
	}
	if fx[len(fx)-1].Kind != "text" {
		t.Error("the caption did not survive the join")
	}
	if !strings.Contains(readSrc(t, "cut_suggest.go"), "fx := joinSpeeds(append(fxFrom(segSpeeds, 0), fxFromReply(out.Fx)...))") {
		t.Error("the cut's speeds are not joined before they are counted or kept")
	}
	if !strings.Contains(readSrc(t, "cut_suggest.go"), "joinSpeeds(fxFrom(in, 0))") {
		t.Error("the speed pass no longer folds two nearby fast stretches into one")
	}
}
