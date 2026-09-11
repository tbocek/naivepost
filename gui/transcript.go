package main

// Transcript: raw ASR into publishable, grounded text. Every source's absolute
// start comes from its filename or the session's own start (srcClock), so
// alignment is a lookup. The LLM fixes lines in blocks, grounded in the event
// log and what other sources heard at the same moment; timestamps and speakers
// pass through byte-identical, enforced; a block that fails twice keeps its
// original lines.
//
// prepare/transcript/
//   <video>/transcript.fixed.tsv + subtitles.srt   per video
//   <audio>/commentary.fixed.tsv                   per voice recording
//   offsets.tsv                                    video, audio, offset seconds
//   session.tsv / session.txt                      everything on one timeline;
//                                                  what the cut step reads

import (
	"errors"
	"fmt"
	"math"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"time"
)

const fixBlock = 25 // transcript lines per fixer request

// One paragraph or bullet per line, unwrapped: this is read in a wrapping text
// box, where a hard wrap at 80 columns only becomes a ragged second wrap. See
// describeSystem.
const fixSystem = `You clean up ASR transcript lines from a recorded session. They become subtitles, and they are the material the video edit is chosen from, so they have to stay faithful to what was actually said.

The lines under "Context around these lines" are for working out what a garbled line was, and nothing else: never copy them into a line, never let them put words in someone's mouth. They are often empty, which is normal. The speaker labels come from automatic diarisation and are sometimes plainly wrong; that is not yours to fix. A line that ends mid sentence stays one line -- the next continues it -- and a line you cannot make sense of keeps its original text.

What to fix.

- Every line is English or German. A line that looks like another language is a misrecognition: reconstruct the intended English or German from how it sounds and from what was happening. Never translate between English and German.
- Mixing the two is normal here: English game terms inside a German sentence, and the other way round. Keep the mix as spoken. It is not a mistake to tidy up.
- Repair mishearings from those surrounding lines. A phrase that means nothing by itself but sounds like something they say is on screen, or was just said, IS that thing. Names of games, items, places and players are what ASR gets wrong most, and the surrounding lines and the user context are where their spelling comes from.
- Remove stutter doubles ("I I" becomes "I") and bare fillers ("uh", "ähm") that are clearly disfluency. Keep repetition that is meant: "go go go" stays.
- ASR sometimes loops one phrase for a whole line, or invents subtitle credits ("Untertitel von ...", "Amara.org", "thanks for watching") over silence. Collapse a loop to one occurrence. Leave an invented credit alone unless the surrounding lines show what was really said.
- Punctuate and capitalise for readability: sentence case, commas and full stops where they help, a question mark where the voice is asking.
- Keep the speaker's words, register and swearing. Do not soften, censor, condense or improve anyone's phrasing. These are subtitles, not a rewrite.`

type seg4 struct {
	s, e      float64
	spk, text string
}

func loadSeg4(path string) []seg4 {
	b, err := os.ReadFile(path)
	if err != nil {
		return nil
	}
	var out []seg4
	for _, line := range strings.Split(string(b), "\n") {
		f := strings.Split(line, "\t")
		if len(f) < 4 {
			continue
		}
		var r seg4
		fmt.Sscanf(f[0], "%f", &r.s)
		fmt.Sscanf(f[1], "%f", &r.e)
		r.spk = f[2]
		r.text = strings.Join(f[3:], " ")
		out = append(out, r)
	}
	return out
}

// events.tsv is 3 columns: start, end, event
func loadEvents(path string) []tsvRow {
	b, err := os.ReadFile(path)
	if err != nil {
		return nil
	}
	var out []tsvRow
	for _, line := range strings.Split(string(b), "\n") {
		f := strings.Split(line, "\t")
		if len(f) < 3 {
			continue
		}
		var r tsvRow
		fmt.Sscanf(f[0], "%f", &r.s)
		fmt.Sscanf(f[1], "%f", &r.e)
		r.text = f[2]
		out = append(out, r)
	}
	return out
}

func srtStamp(t float64) string {
	if t < 0 {
		t = 0
	}
	h := int(t) / 3600
	m := (int(t) % 3600) / 60
	s := int(t) % 60
	ms := int((t-math.Floor(t))*1000 + 0.5)
	return fmt.Sprintf("%02d:%02d:%02d,%03d", h, m, s, ms)
}

// Accepts the stamps recorders write: OBS 2026-08-08 19-55-15, Quest
// ...-20260808-195900-0, phone VID_20250814_213311, dashcam 20250814213311,
// ShadowPlay 2025.08.14 - 21.33.11.03, QuickTime "2026-08-08 at 7.55.15 PM",
// ISO 2026-08-08T19:55:15. Always year first (day/month order is a guess that
// misplaces by weeks); century pinned to 19/20; parseStamp rejects month 13
// and hour 25. A single-digit hour must bring its own separators.
var tsRe = regexp.MustCompile(`((?:19|20)\d{2})[-._]?(\d{2})[-._]?(\d{2})` +
	`(?:\s?[aA][tT]\s|[-_T. ]{0,3})` +
	`(?:(\d{2})[-.:_]?(\d{2})[-.:_]?(\d{2})|(\d)[-.:_](\d{2})[-.:_](\d{2}))` +
	`(?:\s?([aApP][mM]))?`)

// epochRe is the last resort: bare unix seconds, which is what an iPhone
// screen recording (RPReplay_Final1723456789.mp4) carries. Ten digits bounded
// by non-digits, pinned to 2017..2033 so an arbitrary number has to look like
// the present decade before it counts.
var epochRe = regexp.MustCompile(`(?:\A|\D)(1[5-9]\d{8})(?:\D|\z)`)

func parseStamp(m []string) (float64, error) {
	h, mn, sc := m[4], m[5], m[6]
	if h == "" { // the single-digit-hour layout matched instead
		h, mn, sc = "0"+m[7], m[8], m[9]
	}
	t, err := time.ParseInLocation("20060102-150405",
		m[1]+m[2]+m[3]+"-"+h+mn+sc, time.Local)
	if err != nil {
		return 0, err
	}
	// a 12-hour stamp names its half of the day
	if ap := m[10]; ap != "" {
		switch hr := t.Hour(); {
		case (ap[0] == 'p' || ap[0] == 'P') && hr < 12:
			t = t.Add(12 * time.Hour)
		case (ap[0] == 'a' || ap[0] == 'A') && hr == 12:
			t = t.Add(-12 * time.Hour)
		}
	}
	return float64(t.Unix()), nil
}

// nameStamp reads the wall clock out of a file name, if it carries one. It is
// the whole of what this app knows about when a recording happened: the Inputs
// page asks it to warn about names that carry none, and srcClock asks it to
// place the file. Every candidate the pattern finds gets a chance, so a
// digit run that only resembles a date (a resolution, a serial) cannot mask a
// real stamp sitting after it.
func nameStamp(name string) (float64, bool) {
	for _, m := range tsRe.FindAllStringSubmatch(name, -1) {
		if t, err := parseStamp(m); err == nil {
			return t, true
		}
	}
	if m := epochRe.FindStringSubmatch(name); m != nil {
		if n, err := strconv.Atoi(m[1]); err == nil {
			return float64(n), true
		}
	}
	return 0, false
}

// stampFmt is the human half of what tsRe reads back, and it is what an output
// file naming a moment is called: 2026-08-08_19-59-00. Seconds are the whole
// resolution -- a frame every three seconds or thirty a second, the name says
// which second it belongs to and stampSeq separates the ones that share one.
const stampFmt = "2006-01-02_15-04-05"

// stampName renders a wall-clock instant, floored to the second it falls in.
// Local time, because that is what the recorders put in their own file names
// and what parseStamp reads them back as.
func stampName(unix float64) string {
	return time.Unix(int64(math.Floor(unix)), 0).Format(stampFmt)
}

// stampSeq names a series of instants, numbering the ones landing in the same
// second: 19-59-00, then 19-59-00-1, 19-59-00-2. The first frame of a second
// keeps the bare name, which plain string sort then gets backwards ('-' sorts
// before '.') -- read such a folder back with sortStamped, not sort.Strings.
type stampSeq struct{ used map[string]int }

func (s *stampSeq) name(unix float64, ext string) string {
	if s.used == nil {
		s.used = map[string]int{}
	}
	base := stampName(unix)
	n := s.used[base]
	s.used[base]++
	if n > 0 {
		base = fmt.Sprintf("%s-%d", base, n)
	}
	return base + ext
}

// readStamp is the inverse: the second a name claims, and which frame of that
// second it is. Names carrying no stamp -- f000001.jpg from before frames were
// timestamped, a stray file -- report !ok and are left to sort by name.
func readStamp(path string) (sec float64, sub int, ok bool) {
	base := filepath.Base(path)
	m := tsRe.FindStringSubmatch(base)
	loc := tsRe.FindStringIndex(base)
	if m == nil || loc == nil {
		return 0, 0, false
	}
	t, err := parseStamp(m)
	if err != nil {
		return 0, 0, false
	}
	rest := strings.TrimSuffix(base[loc[1]:], filepath.Ext(base))
	if rest == "" {
		return t, 0, true
	}
	n, err := strconv.Atoi(strings.TrimPrefix(rest, "-"))
	if err != nil || !strings.HasPrefix(rest, "-") {
		return 0, 0, false
	}
	return t, n, true
}

// sortStamped puts timestamped output files back in the order they were made.
func sortStamped(paths []string) {
	sort.Slice(paths, func(i, j int) bool {
		as, an, aok := readStamp(paths[i])
		bs, bn, bok := readStamp(paths[j])
		switch {
		case !aok || !bok:
			return paths[i] < paths[j]
		case as != bs:
			return as < bs
		default:
			return an < bn
		}
	})
}

// srcClock puts sources on one wall clock and says where second nought falls.
// A file that NAMES a moment is placed at it. One that does not goes at the
// session's start -- not its mtime, which is when it was copied or exported --
// where the right drag can line it up by ear (cut_shift.go). Nothing named:
// all start together at 0:00. This is the only door for lanes, transcript,
// describe, frame names and render, and it is handed the WHOLE session, since
// a shorter list can put zero elsewhere. Names only, no stat or ffprobe.
func srcClock(paths []string) (map[string]float64, float64) {
	at := make(map[string]float64, len(paths))
	zero := math.Inf(1)
	for _, p := range paths {
		if t, ok := nameStamp(filepath.Base(p)); ok {
			at[p], zero = t, math.Min(zero, t)
		}
	}
	if math.IsInf(zero, 1) {
		zero = 0 // nothing named a moment: the session starts at 0:00
	}
	for _, p := range paths {
		if _, ok := at[p]; !ok {
			at[p] = zero
		}
	}
	return at, zero
}

// sourceStart is where one file sits on that same clock, for the steps that
// hold a path and not the list. It IS srcClock, asked about the whole session
// with this file in it -- so a file the session does not contain, a sting on a
// cut lane say, is placed by the same rule as one it does.
func (a *App) sourceStart(path string) float64 {
	vids, auds := a.snappedSources()
	all := append(append(append([]string{}, vids...), auds...), path)
	at, _ := srcClock(all)
	return at[path]
}

// one source with everything known about its place in the world
type src struct {
	path, base string
	start      float64 // wall clock, seconds since epoch
	dur        float64
	isVideo    bool
	rows       []seg4   // fixed transcript, its own timeline
	events     []tsvRow // videos only
}

// The fixer fixes every source's transcript and writes the merged session
// timeline. span is this job's share of the progress bar; see the describer.
func (a *App) fixTranscripts(videos, audios []string, span float64) error {
	trDir := a.transcriptDir()
	if err := os.MkdirAll(trDir, 0o755); err != nil {
		return err
	}

	// place every source on the wall clock
	var srcs []*src
	all := append(append([]string{}, videos...), audios...)
	at, _ := srcClock(all)
	for _, p := range all {
		dur, _ := ffprobeDur(p)
		srcs = append(srcs, &src{path: p, base: baseName(p), start: at[p], dur: dur,
			isVideo: len(videos) > 0 && contains(videos, p)})
	}
	var offTsv strings.Builder
	for _, v := range srcs {
		if !v.isVideo {
			continue
		}
		for _, au := range srcs {
			if au.isVideo {
				continue
			}
			off := v.start - au.start
			fmt.Fprintf(&offTsv, "%s\t%s\t%.2f\n", v.base, au.base, off)
			a.logfIdle(">>> offset: %s starts %.1f s into %s", v.base, off, au.base)
		}
	}
	if err := os.WriteFile(filepath.Join(trDir, "offsets.tsv"), []byte(offTsv.String()), 0o644); err != nil {
		return err
	}

	// load raw material
	for _, s := range srcs {
		s.rows = loadSeg4(filepath.Join(a.inputsDir(), s.base, "transcript.tsv"))
		if s.isVideo {
			s.events = loadEvents(filepath.Join(a.describeDir(), s.base, "events.tsv"))
		}
	}

	// context provider: everything any source shows or says inside a window
	// of one source's timeline, mapped through the wall clock
	// ...labelled the way every other step labels a timeline line (tlLabel),
	// with the recording named after it when it is not the one being cleaned
	narr := a.narratorMic()
	ctxFor := func(of *src, t0, t1 float64) string {
		var b strings.Builder
		w0 := of.start + t0 - 5
		w1 := of.start + t1 + 5
		for _, s := range srcs {
			if s == of {
				continue
			}
			for _, ev := range s.events {
				if s.start+ev.e > w0 && s.start+ev.s < w1 {
					fmt.Fprintf(&b, "EVENT (%s): %s\n", s.base, ev.text)
				}
			}
			for _, r := range s.rows {
				if s.start+r.e > w0 && s.start+r.s < w1 {
					who := tlLabel(tsvRow{spk: r.spk, src: s.base}, narr)
					fmt.Fprintf(&b, "%s (%s): %s\n", who, s.base, r.text)
				}
			}
		}
		if of.isVideo { // its own events ground its own audio too
			for _, ev := range of.events {
				if ev.e > t0-5 && ev.s < t1+5 {
					fmt.Fprintf(&b, "EVENT: %s\n", ev.text)
				}
			}
		}
		return b.String()
	}

	// fix everything, block by block
	total := 0
	for _, s := range srcs {
		total += (len(s.rows) + fixBlock - 1) / fixBlock
	}
	a.qPush(trackFix, total, "block")
	done := 0
	for _, s := range srcs {
		fixed, err := a.fixRows(s, ctxFor, &done, total, span)
		if err != nil {
			return err
		}
		s.rows = fixed
		dir := filepath.Join(trDir, s.base)
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return err
		}
		var tsv, srt strings.Builder
		for i, r := range fixed {
			fmt.Fprintf(&tsv, "%.2f\t%.2f\t%s\t%s\n", r.s, r.e, r.spk, r.text)
			fmt.Fprintf(&srt, "%d\n%s --> %s\n[%s] %s\n\n", i+1, srtStamp(r.s), srtStamp(r.e), r.spk, r.text)
		}
		name := "commentary.fixed.tsv"
		if s.isVideo {
			name = "transcript.fixed.tsv"
		}
		if err := os.WriteFile(filepath.Join(dir, name), []byte(tsv.String()), 0o644); err != nil {
			return err
		}
		if s.isVideo {
			if err := os.WriteFile(filepath.Join(dir, "subtitles.srt"), []byte(srt.String()), 0o644); err != nil {
				return err
			}
		}
	}

	// merged session timeline: the cut step's single source of truth
	zero := math.MaxFloat64
	for _, s := range srcs {
		zero = math.Min(zero, s.start)
	}
	type row struct {
		g, ge    float64
		src, spk string
		text     string
	}
	var rows []row
	for _, s := range srcs {
		for _, r := range s.rows {
			rows = append(rows, row{s.start - zero + r.s, s.start - zero + r.e, s.base, r.spk, r.text})
		}
		for _, ev := range s.events {
			rows = append(rows, row{s.start - zero + ev.s, s.start - zero + ev.e, s.base, "EVENT", ev.text})
		}
	}
	sort.Slice(rows, func(i, j int) bool { return rows[i].g < rows[j].g })
	// session.tsv is the machine copy and keeps every column, including which
	// recording each line came off. session.txt is the readable one, and it is
	// written exactly as the cut step will hand it to the model (sessionText),
	// so what you read is what it reads.
	var stv strings.Builder
	var tl []tsvRow
	for _, r := range rows {
		fmt.Fprintf(&stv, "%.2f\t%.2f\t%s\t%s\t%s\n", r.g, r.ge, r.src, r.spk, r.text)
		tl = append(tl, tsvRow{s: r.g, e: r.ge, src: r.src, spk: r.spk, text: r.text})
	}
	if err := os.WriteFile(filepath.Join(trDir, "session.tsv"), []byte(stv.String()), 0o644); err != nil {
		return err
	}
	a.logfIdle(">>> session timeline: %d rows across %d source(s)", len(rows), len(srcs))
	// ...and then the one question that needs the whole session in one stream
	// and the text already fixed: which of it was said twice (retake.go). Last
	// in Prepare, so everything after it -- the cut, its captions, the
	// narration, the toolbar -- reads the marks rather than asking again.
	marks, err := a.findMarks(tl)
	if err != nil {
		if errors.Is(err, errStopped) {
			return err
		}
		// a marking, not the step: the timeline is written and usable, and a
		// pass that could not run leaves it exactly as unmarked as it was
		a.logfIdle("!!! retakes: %v -- the timeline stands unmarked", err)
	}
	// session.txt is what the cut reads, marks and all, so what you open is
	// what it opens (sessionText)
	if err := os.WriteFile(filepath.Join(trDir, "session.txt"),
		[]byte(sessionText(tl, a.narratorMic(), marks)), 0o644); err != nil {
		return err
	}
	a.qDone(trackFix, span)
	return nil
}

func contains(list []string, s string) bool {
	for _, v := range list {
		if v == s {
			return true
		}
	}
	return false
}

// fixRows runs one source's lines through the LLM in blocks. Validation is
// strict: same line count, byte-identical start/end/speaker; a block failing
// twice keeps its original lines, loudly.
func (a *App) fixRows(s *src, ctxFor func(*src, float64, float64) string,
	done *int, total int, span float64) ([]seg4, error) {
	cached := 0

	system := a.sysPrompt("fix")
	var out []seg4
	nblocks := (len(s.rows) + fixBlock - 1) / fixBlock
	for b := 0; b < nblocks; b++ {
		if err := a.checkpoint(); err != nil {
			return nil, err
		}
		a.qTake(trackFix)
		a.prog(trackFix, span*float64(*done)/float64(total), "")
		*done++

		lo := b * fixBlock
		hi := min(lo+fixBlock, len(s.rows))
		blk := s.rows[lo:hi]
		var lines []string
		for _, r := range blk {
			lines = append(lines, fmt.Sprintf("%.2f\t%.2f\t%s\t%s", r.s, r.e, r.spk, r.text))
		}
		user := a.ctxBlockFor("fix") + fmt.Sprintf(`Context around these lines:
%s
Transcript lines to clean (%d lines, return exactly %d):
%s`, ctxFor(s, blk[0].s, blk[len(blk)-1].e), len(blk), len(blk), strings.Join(lines, "\n"))

		ok := false
		for try := 0; try < 2 && !ok; try++ {
			// the same block, the same context, the same wording: the same
			// answer (llmcache.go). Only the first attempt is cached -- a
			// second attempt is asked BECAUSE the first came back unusable, and
			// serving it from the file would repeat the failure for ever.
			ask := ""
			var reply string
			if try == 0 {
				ask = askKey(system, user)
				if r, hit := a.cachedReply("transcript", ask); hit {
					reply, cached = r, cached+1
				}
			}
			var err error
			if reply == "" {
				reply, err = a.llmChatRetry("transcript", []map[string]any{
					msg("system", system), msg("user", user),
				}, false)
			}
			if err != nil {
				if errors.Is(err, errStopped) {
					return nil, errStopped
				}
				return nil, fmt.Errorf("fix %s block %d: %w", s.base, b+1, err)
			}
			var got []seg4
			for _, l := range strings.Split(reply, "\n") {
				f := strings.Split(l, "\t")
				if len(f) < 4 {
					continue
				}
				var r seg4
				fmt.Sscanf(f[0], "%f", &r.s)
				fmt.Sscanf(f[1], "%f", &r.e)
				r.spk = f[2]
				r.text = strings.Join(f[3:], " ")
				got = append(got, r)
			}
			if len(got) == len(blk) {
				match := true
				for i := range got {
					if math.Abs(got[i].s-blk[i].s) > 0.01 || math.Abs(got[i].e-blk[i].e) > 0.01 ||
						got[i].spk != blk[i].spk {
						match = false
						break
					}
				}
				if match {
					// kept only when it passed: a block whose answer was
					// refused is one the next run has to ask about again
					if ask != "" {
						a.keepReply("transcript", ask, reply)
					}
					out = append(out, got...)
					ok = true
				}
			}
		}
		if !ok {
			a.logfIdle(">>> [%s] block %d/%d failed validation, keeping original lines", s.base, b+1, nblocks)
			out = append(out, blk...)
		}
	}
	if cached > 0 {
		a.logfIdle(">>> [%s] %d block(s) answered from the cache", s.base, cached)
	}
	return out, nil
}

// findMarks is the pass that says which seconds go, by style: a read to camera
// is edited as text (textedit.go), a session has its retakes found (retake.go).
func (a *App) findMarks(tl []tsvRow) ([]retake, error) {
	if a.videoStyleName() == styleRead {
		return a.findTextEdit(tl)
	}
	return a.findRetakes(tl)
}
