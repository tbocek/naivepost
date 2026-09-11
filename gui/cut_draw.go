package main

import (
	"fmt"
	"math"
	"os/exec"
	"slices"
	"sort"
	"strconv"
	"strings"

	"github.com/diamondburned/gotk4/pkg/cairo"
	"github.com/diamondburned/gotk4/pkg/gdk/v4"
	"github.com/diamondburned/gotk4/pkg/gdkpixbuf/v2"
	"github.com/diamondburned/gotk4/pkg/glib/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// The playhead's own layer. GTK4 has no partial invalidation, so the cheap
// repaint has to be a cheap WIDGET: a transparent layer over both bands with
// nothing but the line, repainted ten times a second while the bands underneath
// repaint only on edits and scrolls (redrawTracks).

// lineOver lays the line's layer over the two bands. Transparent to the
// pointer as well as the eye: every hover, click and drag on the timeline
// belongs to the bands underneath, and a layer that could be a target would
// swallow all of them.
func (ed *cutEditor) lineOver(band gtk.Widgetter) *gtk.Overlay {
	ed.lineArea = gtk.NewDrawingArea()
	ed.lineArea.SetCanTarget(false)
	ed.lineArea.SetDrawFunc(func(_ *gtk.DrawingArea, cr *cairo.Context, _, h int) {
		ed.drawPlayline(cr, h)
	})
	over := gtk.NewOverlay()
	over.SetChild(band)
	over.AddOverlay(ed.lineArea)
	return over
}

// drawPlayline is the line itself, through both bands and the gap between
// them. The bands draw translated by the scroll; this layer does not scroll,
// so the view offset is subtracted here instead.
func (ed *cutEditor) drawPlayline(cr *cairo.Context, h int) {
	if !ed.hasPlay {
		return
	}
	x := ed.xOf(ed.playhead) - ed.viewX
	cr.SetSourceRGB(0.9, 0.15, 0.15)
	cr.SetLineWidth(2)
	cr.MoveTo(x, 0)
	cr.LineTo(x, float64(h))
	cr.Stroke()
}

// redrawLine repaints what the running clock alone moves: the line's layer and
// the framing overlay on the preview, cursor and camera with it -- the same
// trio redrawTracks settles, minus the bands. This is the playback tick's
// repaint; redrawTracks stays the answer for anything that changes what the
// bands show.
func (ed *cutEditor) redrawLine() {
	if ed.lineArea != nil {
		ed.lineArea.QueueDraw()
	}
	if ed.fxArea != nil {
		ed.fxArea.QueueDraw()
		ed.syncFxCursor()
		ed.syncPreviewZoom()
	}
}

// Timeline marks are drawn as paths, not text: cairo's toy text API does no
// font fallback and no stock font has all of ⏸ ⧉ ⊕ ▭ ❝ ♪, so they came out as
// boxes. Each mark is a few lines and arcs in a small square, in the current colour.

// fxMarkW is the square a mark is drawn in. Nine pixels beside a ten pixel
// font: the mark reads as a sibling of the words rather than as a picture
// stuck in front of them.
const fxMarkW = 9.0

// markPlate is plateText with a mark in front of the words -- the dark plate
// covering both, so a band's label stays legible over whatever the band's own
// fill is doing under it.
func markPlate(cr *cairo.Context, x, y float64, kind, s string) {
	e := cr.TextExtents(s)
	platePath(cr, x-3, y-11, fxMarkW+4+e.Width+6, plateH)
	cr.Fill()
	cr.SetSourceRGB(1, 1, 1)
	drawMark(cr, kind, x, y-10)
	cr.MoveTo(x+fxMarkW+4, y)
	cr.ShowText(s)
}

// drawMark paints one mark in the fxMarkW square whose top left corner is
// (x, y), in the colour and on the path state the caller has set up. An
// unknown kind draws nothing rather than a placeholder: a mark nobody
// recognises is worse than a label with no mark.
func drawMark(cr *cairo.Context, kind string, x, y float64) {
	m := fxMarkW
	cx, cy := x+m/2, y+m/2
	cr.SetLineWidth(1.2)
	switch kind {
	case "zoom":
		// a circle closing on a point: the camera moving in and back out
		cr.NewSubPath()
		cr.Arc(cx, cy, m/2-0.7, 0, 2*math.Pi)
		cr.Stroke()
		cr.MoveTo(cx-m/4, cy)
		cr.LineTo(cx+m/4, cy)
		cr.MoveTo(cx, cy-m/4)
		cr.LineTo(cx, cy+m/4)
		cr.Stroke()
	case "stay":
		// a frame that is put somewhere and left there
		cr.Rectangle(x+0.7, y+1.7, m-1.4, m-3.4)
		cr.Stroke()
	case "stop":
		// the two bars every pause button in the world wears
		cr.Rectangle(x+1.2, y+1, 2.3, m-2)
		cr.Rectangle(x+m-3.5, y+1, 2.3, m-2)
		cr.Fill()
	case "hush":
		// a stop that takes its sound with it: the same two bars, struck out
		drawMark(cr, "stop", x, y)
		cr.SetLineWidth(1.2)
		cr.MoveTo(x-0.4, y+m+0.4)
		cr.LineTo(x+m+0.4, y-0.4)
		cr.Stroke()
	case "speed":
		// two chevrons, the way every fast-forward points
		for _, dx := range []float64{0.6, 4.2} {
			cr.MoveTo(x+dx, y+1.4)
			cr.LineTo(x+dx+3, cy)
			cr.LineTo(x+dx, y+m-1.4)
		}
		cr.Stroke()
	case "text":
		// a T, which is what a title is
		cr.MoveTo(x+0.8, y+1.7)
		cr.LineTo(x+m-0.8, y+1.7)
		cr.MoveTo(cx, y+1.7)
		cr.LineTo(cx, y+m-1)
		cr.Stroke()
	case "svg":
		// a picture: a frame with a hill in it
		cr.Rectangle(x+0.7, y+1.2, m-1.4, m-2.4)
		cr.Stroke()
		cr.MoveTo(x+1.6, y+m-2)
		cr.LineTo(cx, cy-0.4)
		cr.LineTo(x+m-1.6, y+m-2)
		cr.ClosePath()
		cr.Fill()
	case "card":
		// two sheets, one laid over the other: something spliced in
		cr.Rectangle(x+0.7, y+2.6, m-3.3, m-3.9)
		cr.Stroke()
		cr.Rectangle(x+2.6, y+0.7, m-3.3, m-3.9)
		cr.Stroke()
	case "vol":
		// a speaker: a cone opening to the right, with a wave off it
		cr.MoveTo(x+0.8, y+2.4)
		cr.LineTo(x+0.8, y+m-2.4)
		cr.LineTo(x+2.6, y+m-2.4)
		cr.LineTo(x+4.6, y+m-0.8)
		cr.LineTo(x+4.6, y+0.8)
		cr.LineTo(x+2.6, y+2.4)
		cr.ClosePath()
		cr.Fill()
		cr.NewSubPath()
		cr.Arc(x+4.6, cy, 2.2, -1.0, 1.0)
		cr.Stroke()
	case "sound":
		// a note
		cr.NewSubPath()
		cr.Arc(x+2.4, y+m-2.2, 1.7, 0, 2*math.Pi)
		cr.Fill()
		cr.MoveTo(x+4.1, y+m-2.2)
		cr.LineTo(x+4.1, y+0.9)
		cr.LineTo(x+m-0.7, y+2.1)
		cr.Stroke()
	}
}

// sndSuffix is the tail on a speed's label: nothing for the default answer,
// a word or two for the four worth knowing without opening the form.
func sndSuffix(f cutFx) string {
	switch f.sound() {
	case sndPitch:
		return " pitched"
	case sndFx:
		return " · sound 1×"
	case sndScene:
		return " · sound 1× to the scene's end"
	case sndMute:
		return " silent"
	}
	return ""
}

// laneLabel is what one band on the effect lane says: a mark naming the kind
// and a few words of detail. The words go through cairo's toy text API (one
// font face, no fallback), so a label may only use characters every face has;
// anything a mark can say instead, a mark says. chars is the room in
// characters; only a title uses it.
func laneLabel(f cutFx, chars int) (mark, label string) {
	switch f.Kind {
	case "label":
		// the name IS the effect: there is nothing else about it to say, and
		// what it is called is what the hand is looking for on the lane
		return "label", laneWords(f.Text, chars)
	case "zoom":
		if f.Stay {
			return "stay", fmt.Sprintf("%.1fs", f.Dur)
		}
		return "zoom", fmt.Sprintf("%.1fs", f.Dur)
	case "speed":
		if f.frozenFx() {
			if f.sound() == sndMute {
				return "hush", fmt.Sprintf("%.1fs", f.Dur) // silent seconds
			}
			return "stop", fmt.Sprintf("%.1fs", f.Dur)
		}
		// the rate, and what the sound does when that is not the plain answer:
		// a ×4 whose sound is silent or off on its own clock is a different
		// effect to hear, and the bar is where that is read (cut_fxsound.go)
		return "speed", "×" + fxNum(f.Rate) + sndSuffix(f)
	case "text":
		return "text", laneWords(f.Text, chars)
	case "svg":
		return "svg", laneWords(svgName(f), chars)
	case "volume":
		// the percent, which is the whole of what this one does. No "%" in the
		// words: the mark beside them is a speaker, so the number can only be
		// a loudness, and every character costs room on a narrow band
		return "vol", gainPct(f.Gain)
	}
	return "", ""
}

// Thumbnails on the video rows: one frame every thumbStep frames, from the
// files Prepare extracted.
//
// Nothing is decoded inside a draw. A zoom changes which files are needed (the
// step is th*aspect/(pps*interval)), so a wheel notch used to stall on a
// screenful of JPEG decodes. A miss paints nothing and queues the file; a worker
// decodes the batch, hands it back on the GTK thread, and asks for one more
// draw. The cache holds cairo SURFACES: gdk_cairo_set_source_pixbuf converts on
// every paint; converted once on arrival, a paint is a memcpy.

// thumbPic is one frame, ready to paint: the surface and its width, which is
// whatever shape the source is at the row's height.
//
// A nil surface is a file that could not be read. It is kept in the cache
// exactly so it is not read again -- every draw would otherwise ask for it
// again, which is a decode attempt per frame per missing file.
type thumbPic struct {
	surf *cairo.Surface
	w    float64
}

// thumb is what drawTrack asks for. It never reads a file: the answer is
// either a picture that is ready or nothing at all, and asking for one that is
// not ready puts it on the list the loader drains.
func (ed *cutEditor) thumb(path string) *thumbPic {
	if p, ok := ed.thumbs[path]; ok {
		return p
	}
	if ed.thumbWant == nil {
		ed.thumbWant = map[string]bool{}
	}
	if !ed.thumbWant[path] {
		ed.thumbWant[path] = true
		ed.loadThumbs()
	}
	return nil
}

// thumbBatch is how many files one round of the loader hands back at a time.
// The whole point is that the page keeps moving, so a long list arrives in
// pieces: a screenful of pictures appears while the rest are still being read.
const thumbBatch = 6

// loadThumbs arms the worker, on an idle rather than here.
//
// On an idle because this is called from inside a draw: starting a goroutine
// is cheap, but the map it reads is the GTK thread's and the draw is not
// finished with it. One armed loader at a time -- what arrives while it runs
// is asked for again by the next draw, which is the same list one frame later.
func (ed *cutEditor) loadThumbs() {
	if ed.thumbBusy {
		return
	}
	ed.thumbBusy = true
	glib.IdleAdd(func() { ed.runThumbs() })
}

// runThumbs takes the list, reads the files off the GTK thread, and hands the
// pictures back to it.
//
// gen is the row height the batch was asked at. It travels with the work
// because setThumbH empties the cache and asks for everything again at another
// size, and a picture decoded for the old height would otherwise land in the
// new cache and be drawn at the wrong scale.
func (ed *cutEditor) runThumbs() {
	want := make([]string, 0, len(ed.thumbWant))
	for p := range ed.thumbWant {
		want = append(want, p)
	}
	ed.thumbWant = map[string]bool{}
	if len(want) == 0 {
		ed.thumbBusy = false
		return
	}
	h, gen := ed.thumbHt, ed.thumbGen
	go func() {
		done := make(map[string]*gdkpixbuf.Pixbuf, thumbBatch)
		flush := func() {
			if len(done) == 0 {
				return
			}
			batch := done
			done = make(map[string]*gdkpixbuf.Pixbuf, thumbBatch)
			glib.IdleAdd(func() { ed.tookThumbs(batch, gen) })
		}
		for _, p := range want {
			// the read, the decode and the scale, all of it here: this is the
			// work that used to happen inside drawTrack
			pb, err := gdkpixbuf.NewPixbufFromFileAtScale(p, -1, h, true)
			if err != nil {
				pb = nil // remembered as unreadable rather than tried again
			}
			done[p] = pb
			if len(done) >= thumbBatch {
				flush()
			}
		}
		flush()
		glib.IdleAdd(func() {
			ed.thumbBusy = false
			if len(ed.thumbWant) > 0 {
				ed.loadThumbs() // more was asked for while this ran
			}
		})
	}()
}

// tookThumbs puts a batch in the cache and asks for one draw for the lot.
//
// The pixbuf becomes a cairo surface here, on the GTK thread with the rest of
// the drawing machinery, and is not kept: what the band paints from is the
// surface, and holding both would be two copies of every frame in memory for
// no reader.
func (ed *cutEditor) tookThumbs(batch map[string]*gdkpixbuf.Pixbuf, gen int) {
	if gen != ed.thumbGen {
		return // asked for at a row height nobody is drawing any more
	}
	if ed.thumbs == nil {
		ed.thumbs = map[string]*thumbPic{}
	}
	for p, pb := range batch {
		ed.thumbs[p] = thumbSurface(pb)
	}
	ed.queueTracks()
}

// thumbSurface paints a pixbuf onto an image surface once, so that every draw
// after it is a copy rather than a conversion. A pixbuf that could not be read
// comes back as a picture with no surface, which is how the cache remembers
// that the file is not worth asking about again.
func thumbSurface(pb *gdkpixbuf.Pixbuf) *thumbPic {
	if pb == nil {
		return &thumbPic{}
	}
	w, h := pb.Width(), pb.Height()
	if w <= 0 || h <= 0 {
		return &thumbPic{}
	}
	surf := cairo.CreateImageSurface(cairo.FormatARGB32, w, h)
	cr := cairo.Create(surf)
	gdk.CairoSetSourcePixbuf(cr, pb, 0, 0)
	cr.Paint()
	surf.Flush()
	return &thumbPic{surf: surf, w: float64(w)}
}

// A file with several audio tracks (OBS desktop+mic, game+headset) gives one
// lane per chosen track, on the file's clock, indistinguishable downstream from
// a separate recorder. Which tracks are used is chosen on the Prepare row -- a
// fact about the FILE. Default is the first track alone, so old projects open
// unchanged.

// audTrack is one audio stream of a file as ffprobe lists them. The POSITION in
// the slice is the a:N index everything below uses -- ffprobe lists them in that
// order and ffmpeg counts them the same way, so no absolute stream index has to
// be carried around and no file's video streams can shift the numbering.
type audTrack struct {
	chans int    // 1 or 2, downmixed like ffprobeChannels: see lanes
	title string // the recorder's own name for it ("Mic/Aux"), or ""
}

// ffprobeTracks is every audio stream in a file, in a:0..a:N-1 order.
//
// Empty means the file has no sound, which is a real answer: a silent screen
// capture gets no lane rather than a strip of ground and a decode that can only
// fail. A probe that would not RUN is not that answer -- it is no answer -- and
// reports one mono track, which is the same guess this made when it was called
// ffprobeChannels and keeps a file the probe stumbled over in the session.
func ffprobeTracks(path string) []audTrack {
	out, err := exec.Command(ffTool("ffprobe"), "-v", "error", "-select_streams", "a",
		"-show_entries", "stream=channels:stream_tags=title", "-of", "csv=p=0", path).Output()
	if err != nil {
		return []audTrack{{chans: 1}}
	}
	return parseTracks(string(out))
}

// parseTracks reads that listing, one track per line. Split out from the probe
// so the answers it gives to a line it did not expect can be asked for directly
// -- the file that would produce one is a file nobody can conveniently write.
func parseTracks(out string) []audTrack {
	var tr []audTrack
	for _, ln := range strings.Split(out, "\n") {
		if ln = strings.TrimSpace(ln); ln == "" {
			continue
		}
		// "2" for a track with no name, "2,Desktop Audio" for one with. The
		// title is cut off LAST because it is the field that can hold a comma,
		// and a recorder's own name for a track is exactly where one turns up.
		num, title, _ := strings.Cut(ln, ",")
		n, err := strconv.Atoi(strings.TrimSpace(num))
		if err != nil || n < 1 {
			n = 1 // a stream ffprobe would not count is still a stream
		}
		tr = append(tr, audTrack{chans: min(2, n), title: strings.TrimSpace(title)})
	}
	return tr
}

// ffprobeChannels is how many lanes a recording gets: the first track's channel
// count, and zero when there is no audio at all. Kept as its own name because
// that is the question a separately recorded file asks -- one file, one track,
// how deep is its lane -- and asking it through the list is the same probe.
func ffprobeChannels(path string) int {
	tr := ffprobeTracks(path)
	if len(tr) == 0 {
		return 0
	}
	return tr[0].chans
}

// trackName: the file's own name for the first track, name + track number for
// the rest. The first stays BARE because lane names key cutSeg.Quiet,
// cutFile.Shift, cutFile.Rows and the waveform cache. Positional numbers, not
// the recorder's titles: titles can repeat, and a repeat is two lanes on one key.
func trackName(base string, n int) string {
	if n <= 0 {
		return base
	}
	return fmt.Sprintf("%s #%d", base, n+1) // 1-based, the way recorders count them
}

// wantTracks is which of a file's audio streams this session uses: the choice
// on its Prepare row, made safe against the file. Empty means the first track
// alone (what every older project means, so nothing migrates). Sorted,
// deduplicated, and dropped where it names a track the file has not got.
func wantTracks(sel []int, have int) []int {
	if have <= 0 {
		return nil
	}
	if len(sel) == 0 {
		return []int{0}
	}
	seen := map[int]bool{}
	var out []int
	for _, n := range sel {
		if n >= 0 && n < have && !seen[n] {
			seen[n] = true
			out = append(out, n)
		}
	}
	sort.Ints(out)
	return out
}

// srcLanes: the footage's own sound as lanes, one per chosen track. The FIRST
// track is the master (the preview's sound, drawn as a strip under the pictures,
// taken by the render off the footage input); every other one is a lane in the
// recorders' band. No sound, or first track not chosen: no master strip.
// want is the per-file choice keyed on path; unlisted files use track one.
func srcLanes(vids []tlVideo, want map[string][]int) []tlAudio {
	var out []tlAudio
	for _, v := range vids {
		tr := ffprobeTracks(v.path)
		for _, n := range wantTracks(want[v.path], len(tr)) {
			out = append(out, tlAudio{base: trackName(v.base, n), path: v.path,
				start: v.start, off: v.off, dur: v.dur, track: n,
				chans: tr[n].chans, master: n == 0})
		}
	}
	return out
}

// ownTrack says whether a video's own first track is one of this session's
// lanes -- the only thing a clip's base audio can come from ([0:a] in
// encodeClip). A row that asked for the second track and not the first gets
// it as a lane in the mix instead. True for a file nobody said anything about.
func ownTrack(want map[string][]int, path string) bool {
	sel := want[path]
	if len(sel) == 0 {
		return true
	}
	for _, n := range sel {
		if n == 0 {
			return true
		}
	}
	return false
}

// trackOf is a chosen track as ffmpeg names it in a filtergraph or -map: bare
// for the first, indexed for the rest. Bare and not "a:0" so a render nobody
// chose anything for logs the identical command it always did.
func trackOf(input int, track int) string {
	if track <= 0 {
		return fmt.Sprintf("%d:a", input)
	}
	return fmt.Sprintf("%d:a:%d", input, track)
}

// ---- the choice, on the Prepare row -----------------------------------------

// srcTracks is one source's audio streams, probed once per session and path:
// the list is rebuilt whole on every change, and an ffprobe per row per rebuild
// stalls the page. The one thing that changes a row's audio hands it a new
// path (separate.go), which is a new key.
func (s *sourceList) srcTracks(path string) []audTrack {
	if s.probed == nil {
		s.probed = map[string][]audTrack{}
	}
	tr, ok := s.probed[path]
	if !ok {
		tr = ffprobeTracks(path)
		s.probed[path] = tr
	}
	return tr
}

// setTrack turns one of a file's audio tracks on or off for this session. The
// last one on cannot be turned off: "this file, none of it" is the ✕ at the
// end of the row, and the stored form keeps one meaning -- what is listed is
// used, nothing listed is the first track (wantTracks).
func (s *sourceList) setTrack(i, n int, on bool) {
	if i < 0 || i >= len(s.items) {
		return
	}
	it := s.items[i]
	var next []int
	for _, t := range wantTracks(it.tracks, len(s.srcTracks(it.path))) {
		if t != n {
			next = append(next, t)
		}
	}
	if on {
		next = append(next, n)
		sort.Ints(next)
	}
	if len(next) == 0 {
		return // the last one standing; see above
	}
	s.items[i].tracks = next
	s.changed()
}

// trackLabel names one track the way the row shows it: its number, and the name
// the recorder gave it when it gave one -- OBS writes "Desktop Audio" and "Mic/
// Aux" into the container, and those are the words the person choosing is
// thinking in. The number is always there because the title is the part that
// can be missing, wrong, or the same on two tracks.
func trackLabel(n int, t audTrack) string {
	name := fmt.Sprintf("Track %d", n+1)
	if t.title != "" {
		name += " — " + t.title
	}
	if t.chans > 1 {
		return name + " (stereo)"
	}
	return name + " (mono)"
}

// trackButton is the row's control for a multi-track file: a menu holding one
// check per audio stream, and nil for the ordinary file with one, which has
// nothing to choose and would only get a button that cannot be wrong.
//
// It says how many are on over how many there are, because that is the fact a
// glance up the list is after -- which of these files is only half in the
// session -- and it is the fact the rest of the app is about to act on.
func (s *sourceList) trackButton(i int) *gtk.MenuButton {
	it := s.items[i]
	tr := s.srcTracks(it.path)
	if len(tr) < 2 {
		return nil
	}
	on := wantTracks(it.tracks, len(tr))
	box := gtk.NewBox(gtk.OrientationVertical, 2)
	margins(box, 6, 6, 6, 6)
	head := headLabel("Audio tracks in this file")
	box.Append(head)
	for n := range tr {
		n := n
		c := gtk.NewCheckButtonWithLabel(trackLabel(n, tr[n]))
		c.SetActive(slices.Contains(on, n))
		c.ConnectToggled(func() { s.setTrack(i, n, c.Active()) })
		box.Append(c)
	}
	why := gtk.NewLabel("Each track chosen gets a lane of its own in Cut, and is mixed\n" +
		"into the render like a separate recording. Untick one to leave it\n" +
		"out of the session entirely.")
	why.SetXAlign(0)
	why.SetMarginTop(4)
	why.AddCSSClass("dim-label")
	box.Append(why)
	pop := gtk.NewPopover()
	pop.SetChild(box)

	b := gtk.NewMenuButton()
	b.AddCSSClass("flat")
	b.SetPopover(pop)
	inner := gtk.NewBox(gtk.OrientationHorizontal, 2)
	inner.Append(gtk.NewImageFromIconName("media-playlist-consecutive-symbolic"))
	inner.Append(gtk.NewLabel(fmt.Sprintf("%d/%d", len(on), len(tr))))
	b.SetChild(inner)
	if len(on) < len(tr) {
		b.AddCSSClass("dim-label") // some of this file is not in the session
	}
	b.SetTooltipText(fmt.Sprintf("Audio tracks — this file holds %d, and %d of them are in\n"+
		"the session. Each one is a lane of its own in Cut and is mixed like a\n"+
		"separate recording.", len(tr), len(on)))
	return b
}
