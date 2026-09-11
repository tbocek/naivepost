package main

// The audio lanes under the cut: one pair for the footage's own sound, one per
// separate recording. The footage is the master; a recording is placed by the
// wall clock (srcClock) and only the part overlapping the footage is drawn, on
// its own lighter ground. The footage's own track is drawn too, so a laugh in
// two lanes either lines up or does not. Left and right are separate lanes
// (one player per side is common). Blue, because every other ink here means
// something else.

import (
	"bufio"
	"encoding/binary"
	"fmt"
	"io"
	"math"
	"os"
	"os/exec"
	"path/filepath"
	"sort"
	"strconv"
	"strings"

	"github.com/diamondburned/gotk4/pkg/cairo"
	"github.com/diamondburned/gotk4/pkg/glib/v2"
)

const (
	// The envelope's resolution. 10 ms is a hair under one pixel at the top
	// zoom (120 px/s), so the drawing never runs out of detail, and an hour of
	// stereo is 720 kB of it -- small enough to keep in memory and to cache on
	// disk without thinking about it.
	waveHz = 100.0
	// What the decode asks ffmpeg for. Peaks of a downsampled signal are a
	// little lower than the true ones, which does not matter for a picture, and
	// 8 kHz is a tenth of the bytes to push through the pipe.
	waveRate  = 8000
	waveLaneH = 30.0 // one channel's lane
	waveGap   = 4.0  // between one recording's lanes and the next's
	wavePad   = 3.0  // above and below the lanes as a whole
)

// tlAudio is a recording placed on the session timeline, as tlVideo is, minus
// the pictures. Not part of the timeline's geometry: relayout lays out the
// videos and the audio is drawn against that. The footage's own track is one
// too, master set -- side by side with it, the same shout is visibly in both
// lanes at the same x, which is what makes a lane readable.
type tlAudio struct {
	base   string
	path   string
	start  float64 // session time of the lane's first second
	off    float64 // the second of the FILE that lands there; see at
	dur    float64
	chans  int  // 1 or 2 lanes; more than two is downmixed to a stereo picture
	master bool // the footage's own track: heard by the preview already
	// which of the file's audio streams this lane is, a:N. Nought for a
	// recording, which is one file with one track, and for the first track of a
	// video; above nought for the rest of a multi-track capture, where the same
	// path is on several lanes and only this tells them apart (cut_tracks.go).
	track int
}

// at is the second of the file heard at session second t (tlVideo.at for
// sound). off is nought for a recording; a cut lane is a WINDOW partway into a
// file (cut_lane.go) and its sound is that same window.
func (au tlAudio) at(t float64) float64 { return t - au.start + au.off }

// soundAt says whether anything is audible at these seconds -- the capture's own
// track or a separate recording running under it. ed.auds is the right list to
// ask because it is both: masterLanes puts the footage's track in it, and only
// when the file actually has one.
func (ed *cutEditor) soundAt(t, dur float64) bool {
	for _, au := range ed.auds {
		if t < au.start+au.dur && t+dur > au.start {
			return true
		}
	}
	return false
}

// soundOpen is whether the insert form asks what this insert does to the
// sound: only for a picture insert (a sound insert IS the sound). Spliced
// between the footage the question is own sound or silent; laid over it, own
// sound or the session's carrying on. A copied stretch counts as having sound.
func (ed *cutEditor) soundOpen(path string, at, dur float64, m insMode) bool {
	if ed == nil || m.lane != "" || insKind(path) == "audio" {
		return false
	}
	if m.splice {
		return insHasSound(path)
	}
	return insHasSound(path) || ed.soundAt(at, dur)
}

// insHasSound is whether the insert brings a sound of its own to decide about.
func insHasSound(path string) bool {
	if _, ok := copySrc(path); ok {
		return true // a stretch of the session: picture and what was filmed with it
	}
	file, _ := insSplit(path)
	return hasAudioStream(file)
}

// loadWaves fetches an envelope for every lane without one, one goroutine
// each; a lane draws its ground until its wave lands. Asked whenever the lanes
// change (setLanes). Keyed by lane name: a copied shot is one file on two lanes.
func (ed *cutEditor) loadWaves() {
	if ed.audArea == nil {
		return // no band to draw them in: an editor built for a test
	}
	if ed.waves == nil {
		ed.waves = map[string]*waveform{}
	}
	a := ed.a
	for _, au := range ed.auds {
		if _, ok := ed.waves[au.base]; ok {
			continue
		}
		au := au
		go func() {
			wf, err := loadWave(a.waveCache(), au)
			glib.IdleAdd(func() {
				if err != nil {
					a.logf("no waveform for %s: %v", au.base, err)
					return
				}
				ed.waves[au.base] = wf
				ed.fitAudio()     // it may have come back on fewer lanes than the probe promised
				ed.fitSrc()       // ...and a master's collapse is a shallower ROW: the strip under
				ed.redrawTracks() // its pictures is row geometry now, not the band's
			})
		}()
	}
}

// sortLanes is the order the lanes are read in: the footage's own first
// whatever the clock says, because it is the thing every other lane is being
// compared to, then the rest by when they started. A master that sorted itself
// into the middle of the recordings would be one more lane to find first.
func sortLanes(auds []tlAudio) {
	sort.SliceStable(auds, func(i, j int) bool {
		if auds[i].master != auds[j].master {
			return auds[i].master
		}
		return auds[i].start < auds[j].start
	})
}

// waveform is the peak envelope: one byte per bucket per channel, the loudest
// sample in that bucket. Peak rather than RMS because what a cut is aimed at is
// where someone starts talking, and RMS rounds an onset off.
type waveform struct {
	hz    float64
	chans [][]uint8
}

// ---- how loud is drawn ------------------------------------------------------
//
// The height is a meter reading on the IEC 60268-18 scale (-70..0 dBFS), not
// an amplitude: linear puts everything but the loudest few dB on the floor.
// The envelope is a peak per 10 ms bucket, so a lane is filled from the
// bottom, not mirrored -- a level has no negative side.

// iecRaw is the standard's own curve: linear amplitude in, meter deflection
// out, with the knots exactly where IEC 60268-18 puts them.
func iecRaw(amp float64) float64 {
	if amp <= 0 {
		return 0
	}
	db := 20 * math.Log10(amp)
	switch {
	case db < -70:
		return 0
	case db < -60:
		return (db + 70) * 0.0025
	case db < -50:
		return (db+60)*0.005 + 0.025
	case db < -40:
		return (db+50)*0.0075 + 0.075
	case db < -30:
		return (db+40)*0.015 + 0.15
	case db < -20:
		return (db+30)*0.02 + 0.3
	}
	return math.Min(1, (db+20)*0.025+0.5)
}

// iecFloor is where the curve stands when the envelope holds the quietest
// thing it can hold. A bucket is one byte, so the smallest sound that is not
// silence is 1/255 of full scale -- about -48 dBFS -- while the curve's own
// bottom is -70. Left alone, the 22 dB between them would be a band of the
// lane no signal could ever land in, and a noise floor hovering around the
// byte's own floor would draw as a picket fence: nothing, then a fourteenth of
// the lane, then nothing.
var iecFloor = iecRaw(1.0 / 255)

// iecScale is the curve as this page uses it: the quietest bucket the envelope
// can hold sits on the floor of the lane, full scale reaches the top, and
// everything between is the meter curve stretched across that range.
func iecScale(amp float64) float64 {
	v := (iecRaw(amp) - iecFloor) / (1 - iecFloor)
	return math.Max(0, math.Min(1, v))
}

// peak is the loudest thing between two file-local times, 0..1. A drawn column
// covers many buckets when zoomed out and a fraction of one when zoomed in, and
// both have to answer: the first by taking the loudest of them (a peak envelope
// that averaged would fade out as you zoomed out), the second by taking the one
// it lands in.
func (wf *waveform) peak(ch int, from, to float64) float64 {
	if wf == nil || ch >= len(wf.chans) {
		return 0
	}
	buf := wf.chans[ch]
	i := int(from * wf.hz)
	j := int(math.Ceil(to * wf.hz))
	if j <= i {
		j = i + 1
	}
	if i < 0 {
		i = 0
	}
	if j > len(buf) {
		j = len(buf)
	}
	top := uint8(0)
	for ; i < j; i++ {
		if buf[i] > top {
			top = buf[i]
		}
	}
	return float64(top) / 255
}

// ---- getting one ------------------------------------------------------------

// waveCache is where the envelopes live between runs. Not under cut/, which
// is the cut's own folder and whose file count is reported to the user as what
// this step produced: an envelope is not an output, it is a picture of an input
// that would otherwise be decoded again on every start.
func (a *App) waveCache() string { return filepath.Join(a.outDir, "cache", "waves") }

// waveMagic changes when the format does, so an old cache is simply not read
// rather than read as something it is not. It has now changed twice without the
// layout changing, for the same reason both times: a stereo file whose two
// sides are the same signal is one channel (AWV3), and so is one whose two
// sides merely DRAW the same (AWV4, see sameLanes). A cache written before
// either would go on drawing the second lane forever, since it is keyed by the
// recording and the recording has not changed.
const waveMagic = "AWV4"

// loadWave is the cache in front of buildWave, keyed by the source's size and
// mtime (a re-recording under the same name is a different recording) and by
// the LANE's name, not the file's: two tracks of one .mkv share size and
// mtime, and a file-named cache would hand the second lane the first's
// envelope.
func loadWave(dir string, au tlAudio) (*waveform, error) {
	fi, err := os.Stat(au.path)
	if err != nil {
		return nil, err
	}
	cf := filepath.Join(dir, au.base+".wave")
	if wf, ok := readWave(cf, fi.Size(), fi.ModTime().Unix()); ok {
		return wf, nil
	}
	wf, err := buildWave(au.path, au.track, au.chans)
	if err != nil {
		return nil, err
	}
	if err := os.MkdirAll(dir, 0o755); err == nil {
		writeWave(cf, wf, fi.Size(), fi.ModTime().Unix())
	}
	return wf, nil
}

// header: magic, channels, hz, buckets, and the source's size and mtime
type waveHead struct {
	Chans uint8
	Hz    uint16
	Count uint32
	Size  int64
	Mtime int64
}

func readWave(file string, size, mtime int64) (*waveform, bool) {
	f, err := os.Open(file)
	if err != nil {
		return nil, false
	}
	defer f.Close()
	r := bufio.NewReader(f)
	magic := make([]byte, len(waveMagic))
	if _, err := io.ReadFull(r, magic); err != nil || string(magic) != waveMagic {
		return nil, false
	}
	var h waveHead
	if binary.Read(r, binary.LittleEndian, &h) != nil {
		return nil, false
	}
	if h.Size != size || h.Mtime != mtime || h.Chans == 0 || h.Chans > 2 || h.Hz == 0 {
		return nil, false // a different recording, or a format we do not read
	}
	wf := &waveform{hz: float64(h.Hz)}
	for c := 0; c < int(h.Chans); c++ {
		buf := make([]uint8, h.Count)
		if _, err := io.ReadFull(r, buf); err != nil {
			return nil, false
		}
		wf.chans = append(wf.chans, buf)
	}
	return wf, true
}

func writeWave(file string, wf *waveform, size, mtime int64) error {
	f, err := os.Create(file)
	if err != nil {
		return err
	}
	defer f.Close()
	w := bufio.NewWriter(f)
	w.WriteString(waveMagic)
	n := 0
	if len(wf.chans) > 0 {
		n = len(wf.chans[0])
	}
	if err := binary.Write(w, binary.LittleEndian, waveHead{
		Chans: uint8(len(wf.chans)), Hz: uint16(wf.hz), Count: uint32(n),
		Size: size, Mtime: mtime,
	}); err != nil {
		return err
	}
	for _, c := range wf.chans {
		w.Write(c)
	}
	return w.Flush()
}

// buildWave decodes the recording and keeps the loudest sample of every bucket.
// Nothing is held but the envelope: the samples go past in a pipe, which is why
// this can be pointed at an hour of 48 kHz stereo without asking what it costs.
func buildWave(path string, track, chans int) (*waveform, error) {
	if chans < 1 {
		chans = 1
	}
	if chans > 2 {
		chans = 2
	}
	args := []string{"-v", "error", "-i", path, "-vn"}
	// only for a track past the first: -vn already leaves ffmpeg picking the
	// best audio stream, which for every single-track file in the world is the
	// one meant, and adding a -map that says the same thing would change the
	// command every existing .wave in the cache was built by.
	if track > 0 {
		args = append(args, "-map", fmt.Sprintf("0:a:%d", track))
	}
	args = append(args, "-ac", strconv.Itoa(chans), "-ar", strconv.Itoa(waveRate),
		"-f", "s16le", "-c:a", "pcm_s16le", "-")
	cmd := exec.Command(ffTool("ffmpeg"), args...)
	out, err := cmd.StdoutPipe()
	if err != nil {
		return nil, err
	}
	var errBuf strings.Builder
	cmd.Stderr = &errBuf
	if err := cmd.Start(); err != nil {
		return nil, err
	}
	wf := &waveform{hz: waveHz, chans: make([][]uint8, chans)}
	per := int(waveRate / waveHz) // samples per bucket per channel
	r := bufio.NewReaderSize(out, 1<<16)
	buf := make([]byte, 2*chans*per)
	loudest, apart := 0, 0 // the peak of the whole file, and how far the sides get from each other
	for {
		n, err := io.ReadFull(r, buf)
		if n >= 2*chans {
			frames := n / (2 * chans)
			for c := 0; c < chans; c++ {
				top := 0
				for f := 0; f < frames; f++ {
					if v := abs16(s16(buf, f*chans+c)); v > top {
						top = v
					}
				}
				if top > loudest {
					loudest = top
				}
				// 32767 would put a full-scale peak one short of the top byte
				wf.chans[c] = append(wf.chans[c], uint8(min(255, top*255/32767)))
			}
			for f := 0; chans == 2 && f < frames; f++ {
				if d := abs16(s16(buf, 2*f) - s16(buf, 2*f+1)); d > apart {
					apart = d
				}
			}
		}
		if err != nil {
			break
		}
	}
	if err := cmd.Wait(); err != nil {
		tail := errBuf.String()
		if len(tail) > 200 {
			tail = tail[len(tail)-200:]
		}
		return nil, fmt.Errorf("ffmpeg: %w\n%s", err, tail)
	}
	if len(wf.chans[0]) == 0 {
		return nil, fmt.Errorf("%s: no audio came out of it", filepath.Base(path))
	}
	// Two sides carrying the same signal are one signal (a mono mic in a stereo
	// container). Not sample-exact: an encoder adds noise between the sides, so
	// the test is whether they ever differ by more than a hundredth of the file's
	// peak -- 40 dB down, invisible in a 30 px lane. A silent stereo file
	// collapses too.
	if chans == 2 && (apart*dualMonoRatio <= loudest || sameLanes(wf.chans[0], wf.chans[1])) {
		wf.chans = wf.chans[:1]
	}
	return wf, nil
}

// dualMonoRatio is how much quieter the difference between two channels has to
// be than the recording itself before they count as one signal.
const dualMonoRatio = 100

// sameLanes asks whether two envelopes draw the same PICTURE: mean difference
// within a byte, with a ceiling on any single bucket so one hard pan still
// counts as stereo. Looser than the sample test (which fails on one lossy
// transient) and an order of magnitude away from a mismatched mic pair.
const (
	laneSameAvg = 1.0 // bytes of envelope, meaned over the recording
	laneSameMax = 8   // ...and the most any one bucket may be out
)

func sameLanes(a, b []uint8) bool {
	if len(a) == 0 || len(a) != len(b) {
		return false
	}
	sum := 0
	for i := range a {
		d := int(a[i]) - int(b[i])
		if d < 0 {
			d = -d
		}
		if d > laneSameMax {
			return false
		}
		sum += d
	}
	return float64(sum)/float64(len(a)) < laneSameAvg
}

// s16 is sample i of an interleaved little-endian s16 buffer.
func s16(b []byte, i int) int { return int(int16(uint16(b[2*i]) | uint16(b[2*i+1])<<8)) }

func abs16(v int) int {
	if v < 0 {
		return -v
	}
	return v
}

// ---- drawing ----------------------------------------------------------------

// lanes is how many lanes one recording draws.
//
// The envelope decides, not the probe: a stereo file whose two sides are the
// same signal is one lane, and that is not something ffprobe can be asked --
// only the decode knows it. Until the envelope lands the probe is what there
// is, so a stereo recording briefly takes two lanes and settles into one when
// its wave arrives, which is also when the area is re-fitted around it.
func (ed *cutEditor) lanes(au tlAudio) int {
	if wf := ed.waves[au.base]; wf != nil && len(wf.chans) > 0 {
		return len(wf.chans)
	}
	return max(1, au.chans)
}

// sepAuds is what the band under the timeline holds: the separate recordings,
// the sound nobody filmed. A master -- some row's own track -- is drawn under
// that row's pictures instead (drawPairStrip), so every question about the
// band's own layout is a question about this list and not about ed.auds.
func (ed *cutEditor) sepAuds() []tlAudio {
	var out []tlAudio
	for _, au := range ed.auds {
		if !au.master {
			out = append(out, au)
		}
	}
	return out
}

// audioHeight is the widget's height, and 0 when there is nothing to show --
// which hides the area entirely rather than leaving an empty black strip under
// the cut saying nothing. With the masters paired under their rows that is the
// everyday state, not the odd one: a cameras-only session has no band at all.
func (ed *cutEditor) audioHeight() int {
	n := ed.audioLanes()
	if n == 0 {
		return 0
	}
	return int(float64(n)*waveLaneH + float64(len(ed.sepAuds())-1)*waveGap + 2*wavePad)
}

// audioLanes is how many waveform lanes the band holds: the separate
// recordings' channels, and only theirs -- a master's wave lives under its
// own row of pictures now, not down here.
func (ed *cutEditor) audioLanes() int {
	n := 0
	for _, au := range ed.sepAuds() {
		n += ed.lanes(au)
	}
	return n
}

// audAtY is the recording whose lanes sit at y in the audio area, by base name,
// or "" with none at all. Total, deliberately: a press on the pad above a lane
// or the hair between two must not quietly mean "the pictures".
func (ed *cutEditor) audAtY(y float64) string {
	auds := ed.sepAuds()
	if len(auds) == 0 {
		return ""
	}
	top := wavePad
	for _, au := range auds {
		if top += float64(ed.lanes(au)) * waveLaneH; y < top {
			return au.base
		}
		top += waveGap
	}
	return auds[len(auds)-1].base
}

// audLaneSpan is the top and bottom of one recording's lanes, and whether it
// has any -- the other half of audAtY, for drawing on the recording a
// selection is of rather than across all of them.
func (ed *cutEditor) audLaneSpan(base string) (float64, float64, bool) {
	top := wavePad
	for _, au := range ed.sepAuds() {
		h := float64(ed.lanes(au)) * waveLaneH
		if au.base == base {
			return top, top + h, true
		}
		top += h + waveGap
	}
	return 0, 0, false
}

// drawAudio paints every lane. Same discipline as drawTrack: timeline
// coordinates with the view translated under them, and every loop cut down to
// the columns actually on screen first, so an afternoon of recording costs the
// same per frame as a minute of it.
func (ed *cutEditor) drawAudio(cr *cairo.Context, w, h int) {
	fh := float64(h)
	cr.SetSourceRGB(0.13, 0.13, 0.13)
	cr.Rectangle(0, 0, float64(w), fh)
	cr.Fill()
	auds := ed.sepAuds()
	if len(auds) == 0 || len(ed.vids) == 0 {
		return
	}
	vx0, vx1 := ed.viewX, ed.viewX+float64(w)

	cr.Save()
	cr.Translate(-ed.viewX, 0)
	y := wavePad
	for _, au := range auds {
		wf := ed.waves[au.base]
		for ch := 0; ch < ed.lanes(au); ch++ {
			ed.drawLane(cr, au, wf, ch, y, vx0, vx1)
			y += waveLaneH
		}
		y += waveGap
	}
	// the black strip at the head of the tape; the switches standing in it are
	// drawn last, over the names (cut_fold.go)
	ed.drawGutter(cr, 0, fh)
	// What the cut keeps, in green, as over the thumbnails: sound is chosen here,
	// so the band has to show the cut. Fainter than the tint on the pictures -- a
	// waveform IS the reading, and a heavy wash would take it with it.
	for _, s := range ed.segs {
		if s.isInsert() && !(s.audioIns() && !s.spliced()) {
			continue // violet below; the sound laid over running footage keeps its green
		}
		x0, x1 := ed.xOf(s.S), ed.xOf(s.E)
		if x1 < vx0 || x0 > vx1 {
			continue
		}
		cr.SetSourceRGBA(0.2, 0.8, 0.3, 0.16)
		cr.Rectangle(x0, 0, x1-x0, fh)
		cr.Fill()
		cr.SetSourceRGB(0.15, 0.85, 0.25)
		cr.SetLineWidth(2)
		for _, x := range []float64{x0, x1} {
			cr.MoveTo(x, 0)
			cr.LineTo(x, fh)
			cr.Stroke()
		}
	}
	// and which lanes each of them hears, grey where it does not. Every scene,
	// not only the one in hand: the switch on the name plate is these answers
	// written to all of them at once, and a press on it that changed nothing
	// visible but its own plate looked like a state of its own (cut_hear.go).
	ed.drawSilences(cr, ed.laneSilences(), vx0, vx1)
	// and under the ▶✂ preview the dropped stretches are dimmed rather than merely left
	// untinted, the picture band's rule: in that mode they are the seconds ▶
	// jumps over, and a lane that still showed them at full brightness would
	// be offering sound that is never heard.
	if ed.cutOnly {
		cr.SetSourceRGBA(0.04, 0.04, 0.05, 0.62)
		for _, g := range ed.droppedSpans() {
			x0, x1 := ed.xOf(g[0]), ed.xOf(g[1])
			if x1 < vx0 || x0 > vx1 {
				continue
			}
			cr.Rectangle(x0, 0, x1-x0, fh)
		}
		cr.Fill()
	}
	// A sound-only insert is marked here and nowhere else: the picture band is
	// about what is seen, and these seconds look no different there -- the
	// footage keeps its frames (or holds one, spliced). The sound is what was
	// placed, so the lanes carry the violet, the hatching for a splice, and the
	// held outline, exactly as the picture band does for a card.
	held := ed.heldSeg()
	for i := range ed.segs {
		s := ed.segs[i]
		if !s.audioIns() {
			continue
		}
		x0, x1 := ed.segSpan(s)
		if x1 < vx0 || x0 > vx1 {
			continue
		}
		ed.sndInsMark(cr, s, x0, x1, 0, fh, held == &ed.segs[i], true)
	}
	// the selection, over everything it covers. Drawn here and not only in its
	// own row because a selection made in a lane is a selection of that
	// SOUND: the span is session time and crosses every lane, so every lane
	// is tinted, and the one recording it is of is tinted again on top. That
	// second wash is the whole answer to "which sound is in hand" -- ⧉ Copy
	// takes that one, and nothing else on the page says which it is.
	if ed.sel.active {
		x0, x1 := ed.selSpanPx()
		if x1 >= vx0 && x0 <= vx1 {
			cr.SetSourceRGBA(0.3, 0.55, 0.9, 0.16)
			cr.Rectangle(x0, 0, x1-x0, fh)
			cr.Fill()
			if y0, y1, ok := ed.audLaneSpan(ed.sel.aud); ok {
				cr.SetSourceRGBA(0.3, 0.55, 0.9, 0.34)
				cr.Rectangle(x0, y0, x1-x0, y1-y0)
				cr.Fill()
			}
			cr.SetSourceRGB(0.62, 0.82, 1)
			for _, x := range []float64{x0, x1} {
				cr.Rectangle(x-1.5, 0, 3, fh)
			}
			cr.Fill()
		}
	}
	// the border under the pointer, then the one being held. The lanes answer to
	// the mouse as the picture band does, so they have to show the same offer:
	// a cut point you can see through the sound but cannot tell you are about to
	// grab is a cut point you trim by accident.
	if ed.edgeHovOn {
		x := ed.xOf(ed.edgeHovT)
		cr.SetSourceRGBA(1, 1, 1, 0.45)
		cr.SetLineWidth(3)
		cr.MoveTo(x, 0)
		cr.LineTo(x, fh)
		cr.Stroke()
	}
	if ed.edgeOn && ed.edgeSeg < len(ed.segs) {
		x := ed.xOf(ed.edgeTime())
		cr.SetSourceRGB(1, 1, 1)
		cr.SetLineWidth(2)
		cr.MoveTo(x, 0)
		cr.LineTo(x, fh)
		cr.Stroke()
	}
	// last inside the translation, so the badges lie over the waves rather
	// than under them: they are what the hand is aiming at while a scene is
	// in hand, and a control a waveform can bury is not one (cut_hear.go)
	ed.drawHearBadges(cr, ed.hearBadgesAud(), vx0, vx1)
	cr.Restore()

	// the names last and un-translated: a mixer strip's labels stay where they
	// are while the tape moves past, and a lane scrolled far from the start of
	// its recording would otherwise be an anonymous blue smear
	cr.SetFontSize(9)
	y = wavePad
	for _, au := range auds {
		n := ed.lanes(au)
		for ch := 0; ch < n; ch++ {
			name := au.base + " " + laneName(ch, n, au.chans)
			if d := ed.shiftOf(au.base); d != 0 {
				// a corrected clock is a fact about the project that is
				// otherwise invisible: the lane simply sits where it sits, and
				// nothing distinguishes "the recorder was two seconds out" from
				// "this is where the file says it starts"
				name += fmt.Sprintf(" · %+.2f s", d)
			}
			plateText(cr, laneNameX, y+12, name)
			y += waveLaneH
		}
		y += waveGap
	}

	// and the switches over the names: a name is a label and a switch is a
	// button, and the button is the one a label may not bury. Back in the
	// tape's own coordinates, which is where the gutter they stand in is.
	cr.Save()
	cr.Translate(-ed.viewX, 0)
	ed.drawLaneSwitches(cr)
	cr.Restore()
}

// laneName says which channel a lane is: lanes of them, out of chans recorded.
// A mono recording says so rather than calling its one channel "L", which would
// imply a missing R, and a stereo recording drawn on one lane says "L=R" rather
// than "mono" -- it is not a mono file, it is a stereo file with one signal in
// it, and a lane that called itself mono would look like the page had lost a
// channel somewhere.
func laneName(ch, lanes, chans int) string {
	if lanes >= 2 {
		if ch == 0 {
			return "L"
		}
		return "R"
	}
	if chans >= 2 {
		return "L=R"
	}
	return "mono"
}

// drawLane paints one channel of one recording across the footage it overlaps.
//
// Per video rather than straight across the timeline, because the timeline is
// not one continuous clock: time nobody filmed is not laid out at all, so a
// column walked blindly across the width would put one recording's samples
// over the stretch before it started rolling.
func (ed *cutEditor) drawLane(cr *cairo.Context, au tlAudio, wf *waveform, ch int, y, vx0, vx1 float64) {
	for _, v := range ed.vids {
		ed.drawWaveSpan(cr, au, v, wf, ch, y, vx0, vx1, false)
	}
}

// sndInsMark paints one sound-only insert's marker over one strip of wave --
// the violet, the splice hatching, the edges, the held outline, and (asked
// once per insert) the name plate. One painter for the recorders' band and
// the rows' paired strips: a placed sound shows in both places, and two
// hand-copied blocks are two chances for them to stop saying the same thing.
func (ed *cutEditor) sndInsMark(cr *cairo.Context, s cutSeg, x0, x1, y, h float64, held, named bool) {
	cr.SetSourceRGBA(0.55, 0.35, 0.9, 0.45)
	cr.Rectangle(x0, y, x1-x0, h)
	cr.Fill()
	if s.spliced() {
		// the same sentence the picture band's hatching says for a card:
		// the footage stops for this
		hatchStrokes(cr, x0, x1-x0, y, h)
	}
	cr.SetSourceRGB(0.75, 0.6, 1)
	cr.SetLineWidth(2)
	for _, x := range []float64{x0, x1} {
		cr.MoveTo(x, y)
		cr.LineTo(x, y+h)
		cr.Stroke()
	}
	if held {
		cr.SetSourceRGBA(1, 1, 1, 0.9)
		cr.Rectangle(x0+1, y+1, x1-x0-2, h-2)
		cr.Stroke()
	}
	if !named {
		return
	}
	// named at the bottom of the strip: the plates own the top left
	cr.SetFontSize(10)
	switch {
	case s.spliced():
		tx := x1 + 4
		if x1-x0 > 90 {
			tx = x0 + 4
		}
		markPlate(cr, tx, y+h-6, "sound", fmt.Sprintf("%s  %.1fs", insName(s), s.Dur))
	case x1-x0 > 24:
		markPlate(cr, x0+4, y+h-6, "sound", insName(s))
	}
}

// drawPairStrip paints a row's own sound under its pictures: every channel of
// the footage's track, windowed exactly as the pictures are, in the dim voice.
// Over this one video only -- the strip belongs to the row's footage, not to
// the session, and two sources sharing a row each bring the stretch under
// their own pictures.
func (ed *cutEditor) drawPairStrip(cr *cairo.Context, v tlVideo, au tlAudio, y, vx0, vx1 float64) {
	wf := ed.waves[au.base]
	for ch := 0; ch < ed.lanes(au); ch++ {
		ed.drawWaveSpan(cr, au, v, wf, ch, y, vx0, vx1, true)
		y += waveLaneH
	}
}

// drawWaveSpan paints the stretch of one channel of one recording that
// overlaps one piece of footage. dim is the paired strip's voice: the same
// wave turned down, plateless, edge to edge under the thumbnails, so it reads
// as the row's shadow rather than a row of its own.
func (ed *cutEditor) drawWaveSpan(cr *cairo.Context, au tlAudio, v tlVideo, wf *waveform, ch int, y, vx0, vx1 float64, dim bool) {
	// the overlap of this recording with this piece of footage, in session
	// time
	t0 := math.Max(au.start, v.start)
	t1 := math.Min(au.start+au.dur, v.start+v.dur)
	if t1 <= t0 {
		return // this recording was not running while this one was
	}
	// ...then in px, one CELL at a time: a folded gap inside the overlap is
	// 32 px standing for minutes, so the wave either side of it is two pieces
	// at two origins and there is none to draw in between (cut_fold.go). With
	// nothing folded this is one cell and one pass, as it always was.
	for _, cell := range ed.cellsOf(t0, t1) {
		cv := v
		cv.pxOrigin = cell.px - (cell.t0-v.start)*ed.pps
		ct0, ct1 := math.Max(t0, cell.t0), math.Min(t1, cell.t1)
		ed.drawWaveCell(cr, au, cv, wf, ch, y, ct0, ct1,
			math.Max(vx0, cell.px), math.Min(vx1, cell.px+ed.spanW(cell)), dim)
	}
}

// drawWaveCell is that painter for one cell: the stretch [t0,t1] of one
// channel of one recording, clipped to the px window, at an origin px is
// linear from.
func (ed *cutEditor) drawWaveCell(cr *cairo.Context, au tlAudio, v tlVideo, wf *waveform, ch int,
	y, t0, t1, vx0, vx1 float64, dim bool) {

	bot := y + waveLaneH - 1 // the meter's zero, a hair inside the lane
	full := waveLaneH - 2    // and how far up full scale reaches
	alpha := 1.0
	if dim {
		alpha = 0.55
	}
	x0 := math.Max(v.pxOrigin+(t0-v.start)*ed.pps, vx0)
	x1 := math.Min(v.pxOrigin+(t1-v.start)*ed.pps, vx1)
	if x1 <= x0 {
		return // off screen
	}
	// the ground says where the recording IS, which is the other half of
	// only drawing the relevant part: an empty lane and a lane of silence
	// are different things and have to look different
	cr.SetSourceRGBA(0.16, 0.17, 0.2, alpha)
	cr.Rectangle(x0, y, x1-x0, waveLaneH)
	cr.Fill()
	// the baseline sits under the fill rather than through it: it is the
	// meter's zero, and on a silent stretch it is the only thing saying the
	// recorder was still running
	cr.SetSourceRGBA(0.35, 0.6, 1, 0.35*alpha)
	cr.SetLineWidth(1)
	cr.MoveTo(x0, math.Round(bot)+0.5)
	cr.LineTo(x1, math.Round(bot)+0.5)
	cr.Stroke()
	if wf == nil {
		return // still being decoded; the ground already says it is here
	}
	// one filled column per pixel, standing up from the baseline
	spp := 1 / ed.pps // seconds per pixel
	hgts := make([]float64, 0, int(x1-math.Floor(x0))+1)
	cr.SetSourceRGBA(0.29, 0.62, 1, alpha)
	for x := math.Floor(x0); x < x1; x++ {
		at := au.timeAt(v, ed.pps, x)
		// the envelope is a linear peak and the lane is a meter: iecScale
		// is the whole difference between a row of spikes over a flat line
		// and a picture of where the sound is
		p := iecScale(wf.peak(ch, at, at+spp))
		h := 0.0
		if p > 0 {
			h = math.Max(1, p*full)
			cr.Rectangle(x, bot-h, 1, h)
		}
		hgts = append(hgts, h)
	}
	cr.Fill()
	// and a darker cap along the top of the fill. A column is one pixel
	// wide, so at any zoom worth looking at the lane is a solid block of
	// blue whose only shape is its skyline; drawn in the same ink as the
	// block it sits on, that skyline is exactly where the eye stops being
	// able to see it.
	cr.SetSourceRGBA(0.09, 0.27, 0.52, alpha)
	for i, h := range hgts {
		if h >= 3 { // below that the cap would eat the column it caps
			cr.Rectangle(math.Floor(x0)+float64(i), bot-h, 1, 1)
		}
	}
	cr.Fill()
}

// timeAt is the recording's own time at a timeline x, through the video that x
// belongs to. The audio's clock and the footage's clock differ by exactly where
// each of them started, which is what start holds and what makes this a
// subtraction rather than an alignment problem.
func (au tlAudio) timeAt(v tlVideo, pps, x float64) float64 {
	return au.at(v.start + (x-v.pxOrigin)/pps)
}

// selSnd is the selection scoped to one recording's sound -- drawn in a lane,
// or on the wave strip under a row's pictures, which is the only place that
// choice is made now (the press that starts the drag says it, cut.go). The
// strip that used to be able to change it afterwards is gone: what a
// selection is OF is what it was drawn on, and which rows a SCENE is made of
// is said on the scene itself (cut_cam.go, cut_hear.go).
func (ed *cutEditor) selSnd() bool { return ed.sel.aud != "" }

// fitSelAud keeps the selection's scope pointing at things the session still
// has: the lanes are rebuilt from what is on disk, a recording can go away
// between one reload and the next, and a selection pointing at one nobody has
// is a selection whose every verb would miss. So it comes back to the footage.
func (ed *cutEditor) fitSelAud() {
	if ed.sel.aud != "" && ed.audByBase(ed.sel.aud) == nil {
		ed.sel.aud = ""
		ed.syncSelBtns()
	}
}
