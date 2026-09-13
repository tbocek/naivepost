package main

// A file with more than one audio track.
//
// The claim is that a further track is not a special case anywhere past the
// probe: it becomes a lane like a recorder that was running beside the camera,
// and everything the session does to a lane it does to this one. So most of
// what is pinned here is that the seams hold -- the naming that keeps two lanes
// of one file apart in four different maps, the clock they share with the
// pictures they were recorded with, the waveform cache that would otherwise
// hand the second lane the first one's picture -- and one end-to-end render,
// because whether the right stream came out of ffmpeg is not something any
// amount of reading the code can settle.

import (
	"encoding/json"
	"fmt"
	"os/exec"
	"path/filepath"
	"reflect"
	"strconv"
	"strings"
	"testing"
)

// twoTrack writes a video whose first audio track is a low tone from end to end
// and whose second is silent but for two seconds of a high one, four seconds in
// -- the two named the way OBS names its tracks.
//
// 300 Hz and 2500 Hz, which is a range chosen twice over: far enough apart that
// a highpass in a finished clip says which of them came out, and both low
// enough to survive the 8 kHz the waveforms are decoded at, so the same file
// settles the envelope as well as the render.
func twoTrack(t *testing.T, path string) {
	t.Helper()
	mustFFmpeg(t,
		"-f", "lavfi", "-t", "8", "-i", "testsrc=size=320x240:rate=30",
		"-f", "lavfi", "-t", "8", "-i", "sine=frequency=300:sample_rate=48000",
		"-f", "lavfi", "-t", "4", "-i", "anullsrc=r=48000:cl=mono",
		"-f", "lavfi", "-t", "2", "-i", "sine=frequency=2500:sample_rate=48000",
		"-f", "lavfi", "-t", "2", "-i", "anullsrc=r=48000:cl=mono",
		"-filter_complex", "[2:a][3:a][4:a]concat=n=3:v=0:a=1[t2]",
		"-map", "0:v", "-map", "1:a", "-map", "[t2]",
		"-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac",
		// stereo desktop, mono mic: the shape OBS actually writes, and the one
		// where "how wide is this lane" has a different answer per track
		"-ac:a:0", "2", "-ac:a:1", "1",
		"-metadata:s:a:0", "title=Desktop Audio", "-metadata:s:a:1", "title=Mic/Aux",
		path)
}

// ---- the probe --------------------------------------------------------------

// Every audio stream, in the order ffmpeg counts them, with the name the
// recorder gave it. The position in the list is the a:N index everything
// downstream maps and filters by, so a probe that reported them out of order or
// dropped one would put the wrong sound in the render and nothing before the
// render would notice.
func TestEveryAudioTrackInAFileIsFound(t *testing.T) {
	insertApp(t) // for the ffmpeg check alone
	dir := t.TempDir()

	both := filepath.Join(dir, "obs.mkv")
	twoTrack(t, both)
	got := ffprobeTracks(both)
	if len(got) != 2 {
		t.Fatalf("a two-track capture probed as %d track(s): %+v", len(got), got)
	}
	if got[0].title != "Desktop Audio" || got[1].title != "Mic/Aux" {
		t.Errorf("the tracks came back as %q and %q, want the names the recorder wrote",
			got[0].title, got[1].title)
	}
	if got[0].chans != 2 || got[1].chans != 1 {
		t.Errorf("the tracks probed %d and %d channels wide, want the 2 and 1 they were "+
			"written with", got[0].chans, got[1].chans)
	}
	// and the count on its own is the FIRST track's, which is the question a
	// file with one track is really asking
	if got := ffprobeChannels(both); got != 2 {
		t.Errorf("ffprobeChannels said %d for a capture whose first track is stereo", got)
	}

	// the ordinary file: one track, no name, and the same answer the channel
	// count on its own used to give
	one := filepath.Join(dir, "mic.wav")
	mustFFmpeg(t, "-f", "lavfi", "-t", "1", "-i", "sine=frequency=440:sample_rate=48000",
		"-ac", "2", "-c:a", "pcm_s16le", one)
	if got := ffprobeTracks(one); len(got) != 1 || got[0].chans != 2 || got[0].title != "" {
		t.Errorf("a plain stereo recording probed as %+v, want one unnamed stereo track", got)
	}
	if got := ffprobeChannels(one); got != 2 {
		t.Errorf("ffprobeChannels said %d for a stereo file", got)
	}

	// a video with no sound has no tracks, which is a real answer and the one
	// that keeps it off the band entirely
	silent := filepath.Join(dir, "silent.mp4")
	mustFFmpeg(t, "-f", "lavfi", "-t", "1", "-i", "testsrc=size=64x48:rate=10",
		"-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", silent)
	if got := ffprobeTracks(silent); len(got) != 0 {
		t.Errorf("a silent capture probed as %+v, want no tracks at all", got)
	}
	if got := ffprobeChannels(silent); got != 0 {
		t.Errorf("ffprobeChannels said %d for a silent capture, want 0", got)
	}
}

// What the listing is read as, line by line -- including the lines a file
// nobody can conveniently write would produce. A stream ffprobe cannot count is
// still a stream: dropping it would renumber every track after it, so a:2 in
// the render would be the sound of a:3, and swallowing the whole line is the
// only way this can go wrong in a way that still produces a file.
func TestTheProbesListingIsReadTrackByTrack(t *testing.T) {
	for _, c := range []struct {
		what string
		out  string
		want []audTrack
	}{
		{"one unnamed track", "1\n", []audTrack{{chans: 1}}},
		{"named tracks, in order", "2,Desktop Audio\n1,Mic/Aux\n",
			[]audTrack{{2, "Desktop Audio"}, {1, "Mic/Aux"}}},
		{"a name with a comma in it", "1,Mic, close\n", []audTrack{{1, "Mic, close"}}},
		{"surround, downmixed to the two lanes anything is drawn in",
			"6,Game\n", []audTrack{{2, "Game"}}},
		{"a count that is not a number", "N/A,Aux\n", []audTrack{{1, "Aux"}}},
		{"a count of nought", "0\n", []audTrack{{1, ""}}},
		{"no audio at all", "", nil},
		{"blank lines between the tracks", "\n1\n\n2\n\n",
			[]audTrack{{1, ""}, {2, ""}}},
	} {
		if got := parseTracks(c.out); !reflect.DeepEqual(got, c.want) {
			t.Errorf("%s (%q) read as %+v, want %+v", c.what, c.out, got, c.want)
		}
	}

	// a probe that would not RUN is not the answer "no sound" -- that answer
	// takes the file off the band entirely, and a file the probe stumbled over
	// belongs in the session with the guess the old channel probe made
	if got := ffprobeTracks(filepath.Join(t.TempDir(), "not-a-file.mkv")); len(got) != 1 ||
		got[0].chans != 1 {
		t.Errorf("a file the probe could not open came back as %+v, want one mono track", got)
	}
}

// ---- which of them the session uses -----------------------------------------

// Empty means the first track alone. That is the whole of why nothing had to be
// migrated: a project written before any of this existed stores nothing here
// and means exactly what it always meant.
func TestSayingNothingMeansTheFirstTrackAlone(t *testing.T) {
	for _, c := range []struct {
		why  string
		sel  []int
		have int
		want []int
	}{
		{"an old project, or a file nobody chose for", nil, 3, []int{0}},
		{"a single-track file, whatever it says", nil, 1, []int{0}},
		{"a file with no sound has no lanes to choose", nil, 0, nil},
		{"the choice, as made", []int{0, 2}, 3, []int{0, 2}},
		{"the second track and not the first", []int{1}, 2, []int{1}},
		{"written out of order, read in order", []int{2, 0}, 3, []int{0, 2}},
		{"written twice", []int{1, 1}, 2, []int{1}},
		{"a track the file has not got any more", []int{0, 5}, 2, []int{0}},
		{"and one that never made sense", []int{-1, 1}, 2, []int{1}},
		{"every track named, of a file that lost them all", []int{0, 1}, 0, nil},
	} {
		got := wantTracks(c.sel, c.have)
		if len(got) != len(c.want) {
			t.Errorf("%s: %v of %d tracks came out as %v, want %v", c.why, c.sel, c.have, got, c.want)
			continue
		}
		for i := range got {
			if got[i] != c.want[i] {
				t.Errorf("%s: %v of %d tracks came out as %v, want %v",
					c.why, c.sel, c.have, got, c.want)
				break
			}
		}
	}
}

// The first track keeps the file's bare name, and it has to: a lane's name is
// the key in cutSeg.Quiet, cutFile.Shift, cutFile.Rows and the waveform cache,
// so numbering it would be every existing project opening with its silences and
// its clock corrections pointing at lanes that are not there.
func TestTheFirstTrackKeepsTheFilesOwnName(t *testing.T) {
	if got := trackName("game", 0); got != "game" {
		t.Errorf("the first track is called %q, want the file's own name", got)
	}
	if got := trackName("game", 1); got != "game #2" {
		t.Errorf("the second track is called %q, want it numbered the way a recorder counts", got)
	}
	if trackName("game", 1) == trackName("game", 2) {
		t.Error("two tracks of one file share a name — they share a map key too")
	}
}

// ---- the lanes --------------------------------------------------------------

// One lane per chosen track, on the video's own clock. The first is the master
// -- the paired strip under the pictures, the sound the render takes off the
// footage input -- and every other is an ordinary lane in the band, which is
// what makes it behave like a recorder that was running beside the camera.
func TestEachChosenTrackIsALaneOfItsOwn(t *testing.T) {
	insertApp(t)
	dir := t.TempDir()
	path := filepath.Join(dir, "obs.mkv")
	twoTrack(t, path)
	v := tlVideo{base: "obs", path: path, start: 12, dur: 8}

	got := srcLanes([]tlVideo{v}, map[string][]int{path: {0, 1}})
	if len(got) != 2 {
		t.Fatalf("a two-track capture became %d lane(s): %+v", len(got), got)
	}
	if got[0].base != "obs" || !got[0].master || got[0].track != 0 {
		t.Errorf("the first lane is %+v, want the file's own name as the master track", got[0])
	}
	if got[1].base != "obs #2" || got[1].master || got[1].track != 1 {
		t.Errorf("the second lane is %+v, want a named, numbered lane of its own", got[1])
	}
	for _, au := range got {
		if au.path != path || au.start != 12 || au.dur != 8 {
			t.Errorf("%s sits at %g for %gs off %s, want the video's own clock",
				au.base, au.start, au.dur, filepath.Base(au.path))
		}
	}

	// nothing said: the first track alone, which is what every session that has
	// never heard of this gets
	if got := srcLanes([]tlVideo{v}, nil); len(got) != 1 || got[0].base != "obs" || !got[0].master {
		t.Errorf("a capture nobody chose for became %+v, want its first track alone", got)
	}

	// the second track and not the first: no master strip at all, rather than a
	// strip under the pictures playing a track the session was told to leave out
	got = srcLanes([]tlVideo{v}, map[string][]int{path: {1}})
	if len(got) != 1 || got[0].base != "obs #2" || got[0].master {
		t.Errorf("choosing the second track alone gave %+v, want that lane and no master", got)
	}
}

// A clip's own sound comes off the input its picture comes off, which is that
// file's first stream and nothing else. So a session that dropped the first
// track has to render the footage as having no sound of its own -- the track it
// did want arrives in the mix, like every other recording.
func TestOnlyTheFirstTrackCanBeTheFootagesOwnSound(t *testing.T) {
	for _, c := range []struct {
		why  string
		sel  []int
		want bool
	}{
		{"nobody chose anything", nil, true},
		{"the first track among others", []int{0, 1}, true},
		{"the first track alone", []int{0}, true},
		{"the second track instead of it", []int{1}, false},
		{"two further tracks and not the first", []int{1, 2}, false},
	} {
		if got := ownTrack(map[string][]int{"/x/obs.mkv": c.sel}, "/x/obs.mkv"); got != c.want {
			t.Errorf("%s: the footage's own sound is used = %v, want %v", c.why, got, c.want)
		}
	}
	// and a file the map says nothing about at all
	if !ownTrack(nil, "/x/game.mp4") {
		t.Error("a session with no track choices in it stopped taking the footage's own sound")
	}
}

// ffmpeg picks the first matching stream for a bare specifier, so "0:a" and
// "0:a:0" say the same thing to it -- but they do not write the same command,
// and every render made before any of this logged the bare one. Written out,
// an ordinary session's ffmpeg line is the identical line it always was.
func TestAnOrdinaryRenderNamesItsStreamsTheWayItAlwaysDid(t *testing.T) {
	if got := trackOf(0, 0); got != "0:a" {
		t.Errorf("the first track is named %q in the graph, want the bare specifier", got)
	}
	if got := trackOf(3, 2); got != "3:a:2" {
		t.Errorf("input 3's third track is named %q, want it indexed", got)
	}
}

// ---- the clock --------------------------------------------------------------

// A further track is glued to the pictures it was recorded with. Dragging the
// row to correct its clock has to take the track with it, or the correction is
// a desync nobody asked for -- and its own name still works as a name, so it
// can be nudged apart afterwards.
func TestDraggingARowTakesItsOtherTracksWithIt(t *testing.T) {
	ed := newTestEd(t)
	ed.vids = []tlVideo{{base: "obs", path: "/x/obs.mkv", start: 0, dur: 60}}
	ed.auds = []tlAudio{
		{base: "obs", path: "/x/obs.mkv", start: 0, dur: 60, master: true},
		{base: "obs #2", path: "/x/obs.mkv", start: 0, dur: 60, track: 1},
		{base: "mic", path: "/x/mic.wav", start: 0, dur: 60},
	}
	ed.slideSrc("obs", 4)
	if ed.vids[0].start != 4 {
		t.Fatalf("the pictures moved to %g, want 4", ed.vids[0].start)
	}
	if ed.auds[0].start != 4 || ed.auds[1].start != 4 {
		t.Errorf("the capture's tracks are at %g and %g, want both at 4 with the pictures",
			ed.auds[0].start, ed.auds[1].start)
	}
	if ed.auds[2].start != 0 {
		t.Errorf("a separate recording moved to %g — dragging one row moved another",
			ed.auds[2].start)
	}
	// and the track is still a lane, with a name a further drag is stored under
	ed.slideSrc("obs #2", -1.5)
	if ed.auds[1].start != 2.5 || ed.auds[0].start != 4 {
		t.Errorf("nudging the second track alone gave %g and %g, want 2.5 and 4 —"+
			" a track that cannot be moved apart is not a lane",
			ed.auds[1].start, ed.auds[0].start)
	}
}

// ---- the waveform -----------------------------------------------------------

// Two tracks of one file have the same size and the same modification time, so
// a cache keyed on the file would hand the second lane the first one's envelope
// and every check that guards against a stale picture would pass. Then the page
// would draw two identical waves and say, wrongly, that both tracks heard the
// same thing.
func TestTwoTracksOfOneFileGetTheirOwnWaveforms(t *testing.T) {
	insertApp(t)
	dir := t.TempDir()
	path := filepath.Join(dir, "obs.mkv")
	twoTrack(t, path)
	a := &App{outDir: t.TempDir()}

	first, err := loadWave(a.waveCache(), tlAudio{base: "obs", path: path, chans: 2})
	if err != nil {
		t.Fatal(err)
	}
	second, err := loadWave(a.waveCache(), tlAudio{base: "obs #2", path: path, track: 1, chans: 1})
	if err != nil {
		t.Fatal(err)
	}
	// the first track holds a tone from end to end and the second is silent for
	// its first four seconds, so the two envelopes disagree where it matters.
	// Read against each other rather than against a number: what the peak of a
	// tone comes to after an AAC encode and a resample to 8 kHz is the codec's
	// business, and the claim here is only about which stream was read.
	quiet, tone := loud(second, 0, 3), loud(second, 4.2, 1.5)
	if start := loud(first, 0, 3); start < 4*quiet+8 {
		t.Errorf("the first track's wave is flat at the start (%d against the second's "+
			"%d) — it holds a tone there and the second does not", start, quiet)
	}
	if tone < 4*quiet+8 {
		t.Errorf("the second track's wave reads %d where its tone is and %d where it is "+
			"silent — the decode did not reach a:1, or the cache handed this lane the "+
			"first track's envelope", tone, quiet)
	}
}

// loud is the peak of a waveform's first channel over a stretch of seconds, in
// the 0..255 the envelope is stored in.
func loud(wf *waveform, from, dur float64) int {
	top := 0
	for i := int(from * wf.hz); i < int((from+dur)*wf.hz) && i < len(wf.chans[0]); i++ {
		if v := int(wf.chans[0][i]); v > top {
			top = v
		}
	}
	return top
}

// ---- the row ----------------------------------------------------------------

// The last track on cannot be turned off. A source in the list is a source the
// session uses, and "this file, none of it" is not a third state to carry
// through every step -- it is the ✕ at the end of the row.
func TestTheLastTrackOnCannotBeSwitchedOff(t *testing.T) {
	insertApp(t)
	dir := t.TempDir()
	path := filepath.Join(dir, "obs.mkv")
	twoTrack(t, path)
	s := &sourceList{items: []sourceItem{{path: path, footage: true}}}

	s.setTrack(0, 1, true) // both
	if got := s.items[0].tracks; len(got) != 2 || got[0] != 0 || got[1] != 1 {
		t.Fatalf("turning the second track on gave %v, want both in order", got)
	}
	s.setTrack(0, 0, false) // the second alone
	if got := s.items[0].tracks; len(got) != 1 || got[0] != 1 {
		t.Fatalf("turning the first track off gave %v, want the second alone", got)
	}
	s.setTrack(0, 1, false) // and now there is nothing left to turn off
	if got := s.items[0].tracks; len(got) != 1 || got[0] != 1 {
		t.Errorf("the last track on was switched off, leaving %v — a row that uses none "+
			"of its file is a row that should have been removed", got)
	}
}

// The menu is only on a file that has something to choose. A recording with one
// track would get a button that cannot be wrong, which is a control that only
// costs a glance.
func TestOnlyAMultiTrackFileGetsAChoiceOnItsRow(t *testing.T) {
	if !strings.Contains(readSrc(t, "sources.go"), "if tb := s.trackButton(i); tb != nil {") {
		t.Error("the source row no longer asks trackButton for a track menu")
	}
	if !strings.Contains(readSrc(t, "cut_draw.go"), "if len(tr) < 2 {\n\t\treturn nil\n\t}") {
		t.Error("trackButton no longer refuses a file with one track — every ordinary " +
			"recording would grow a control it cannot use")
	}
	// and the label is the number, whatever the recorder called it: a title can
	// be missing, and it can be the same on two tracks
	if got := trackLabel(1, audTrack{chans: 2, title: "Mic/Aux"}); got != "Track 2 — Mic/Aux (stereo)" {
		t.Errorf("a named stereo track reads as %q", got)
	}
	if got := trackLabel(0, audTrack{chans: 1}); got != "Track 1 (mono)" {
		t.Errorf("an unnamed mono track reads as %q", got)
	}
}

// ---- the project ------------------------------------------------------------

// The choice belongs to the file and not to the cut, so it is stored with the
// source. Omitted when it is the ordinary answer, which is what keeps a project
// written before this readable and unchanged.
func TestTheChosenTracksAreStoredWithTheSource(t *testing.T) {
	a := &App{root: "/home/x/naivepost"}
	items := []sourceItem{
		{path: "/home/x/naivepost/input_video/obs.mkv", footage: true, tracks: []int{0, 2}},
		{path: "/home/x/naivepost/input_video/cam.mp4", footage: true},
	}
	var stored []ProjectSource
	for _, it := range items {
		stored = append(stored, ProjectSource{
			Path: a.relToRoot(it.path), Footage: it.footage, Tracks: it.tracks})
	}
	b, err := json.Marshal(stored)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(string(b), `"tracks":[0,2]`) {
		t.Errorf("the chosen tracks are not in the project file: %s", b)
	}
	// the ordinary row says nothing at all, which is what keeps a project
	// written before any of this existed the same file it always was
	if strings.Count(string(b), `"tracks"`) != 1 {
		t.Errorf("a row that chose nothing still wrote a tracks field: %s", b)
	}
	back := a.projectSources(Project{Sources: stored})
	if len(back) != 2 {
		t.Fatalf("%d sources went in, %d came back", len(items), len(back))
	}
	for i := range items {
		if !sameSource(back[i], items[i]) {
			t.Errorf("source %d came back as %+v, want %+v", i, back[i], items[i])
		}
	}
	// the writer is a GUI-thread walk of the live list and cannot be called
	// headless, so what it puts in the record is pinned as text
	if !strings.Contains(readSrc(t, "project.go"), "SepVoice: it.sepVoice, Tracks: it.tracks,") {
		t.Error("the project writer no longer stores the row's chosen tracks")
	}
}

// Go will not compare a struct holding a slice, so sameSource spells every
// field out by hand -- and a field added later and forgotten there would be a
// change the project quietly does not notice it has to save.
//
// The row's fields are unexported, so this cannot reach for reflection to set
// them the way TestEverySegmentFieldCountsAsAChange does. It walks the type all
// the same and demands a mutation be written down for every field it finds: a
// field added later fails here for want of a line in the table, and every field
// already in it is really changed and really compared.
func TestEverySourceFieldCountsAsAChange(t *testing.T) {
	change := map[string]func(*sourceItem){
		"path":     func(it *sourceItem) { it.path = "x" },
		"footage":  func(it *sourceItem) { it.footage = true },
		"narrator": func(it *sourceItem) { it.narrator = 7 },
		"sepVoice": func(it *sourceItem) { it.sepVoice = true },
		"tracks":   func(it *sourceItem) { it.tracks = []int{7} },
	}
	// the tracks are compared in ORDER, which is the one place this is stricter
	// than a scene's silenced lanes: what the row saves is always sorted
	// (wantTracks), so a file holding them in another order is a file written
	// by something else and re-saving it the way this saves it is the point
	if sameSource(sourceItem{tracks: []int{0, 2}}, sourceItem{tracks: []int{2, 0}}) {
		t.Error("two rows holding the same tracks in different orders read as the same row")
	}

	base := sourceItem{}
	rt := reflect.TypeOf(base)
	for i := 0; i < rt.NumField(); i++ {
		name := rt.Field(i).Name
		mut, ok := change[name]
		if !ok {
			t.Fatalf("sourceItem grew a %s field and this test does not know how to "+
				"change one — add it here, and make sure sameSource reads it", name)
		}
		other := base
		mut(&other)
		if sameSource(base, other) {
			t.Errorf("changing %s left the two rows reading as the same one — sameSource "+
				"does not look at it, so nothing will notice it changed", name)
		}
	}
}

// ---- the render -------------------------------------------------------------

// renderTracks cuts the whole of a two-track capture into one clip with the
// given tracks chosen, and hands back the finished file. The session is built
// the way a run builds one -- sessionTracks off the snapshot, clipMixes off
// what that returned -- so what is being exercised is the real path from a tick
// on the Prepare row to a stream index in a filter graph.
func renderTracks(t *testing.T, a *App, dir, footage string, want []int, name string) string {
	t.Helper()
	a.selTracks = map[string][]int{footage: want}
	vids, recs, err := a.sessionTracks([]string{footage}, nil)
	if err != nil {
		t.Fatal(err)
	}
	if len(vids) != 1 {
		t.Fatalf("the session came out as %d video(s)", len(vids))
	}
	c := prodClip{video: &vids[0], local: 0, sessS: vids[0].start, length: 8, tempo: 1}
	c.mix = clipMixes(c, recs)
	st := defaultProdSettings()
	st.Preset, st.CRF, st.FPS, st.GameVol = "ultrafast", 32, 30, 1
	out := filepath.Join(dir, name)
	if err := a.encodeClip(c, out, "", st); err != nil {
		t.Fatal(err)
	}
	return out
}

// The one ffmpeg has to settle: that ticking the second track really puts THAT
// stream in the finished clip, at the second it was recorded at.
//
// Everything upstream of the graph could be right and this still be wrong. A
// bare [N:a] label on a multi-track input takes the first stream without
// complaining, so a render that forgot the a:N would come out playing the
// capture's own sound twice -- a plausible-sounding file, and the wrong one.
// The second track is silent but for a tone in a band the first track has
// nothing in, so the finished clip is asked second by second which of them it
// is carrying.
func TestThePickedTrackIsTheOneThatComesOut(t *testing.T) {
	a := insertApp(t)
	dir := t.TempDir()
	footage := filepath.Join(dir, "obs_2026-01-01_10-00-00.mkv")
	twoTrack(t, footage)

	both := renderTracks(t, a, dir, footage, []int{0, 1}, "both.mp4")
	before, tone, after := highBand(t, both, 0.2, 3.4), highBand(t, both, 4.2, 1.6),
		highBand(t, both, 6.3, 1.5)
	t.Logf("both tracks — high band: %.1f dB before, %.1f during, %.1f after", before, tone, after)
	if tone < -40 {
		t.Errorf("nothing came through where the second track's tone is (%.1f dB) — "+
			"the further track is not in the mix", tone)
	}
	if tone-before < 20 || tone-after < 20 {
		t.Errorf("the tone is not where it was recorded: %.1f dB before, %.1f during, "+
			"%.1f after — the second track is in the clip at the wrong second", before, tone, after)
	}

	// and the default, which is every project written before any of this
	// existed: the first track alone, and the second one nowhere in the file.
	// The picture, the length and the capture's own sound are untouched by it.
	first := renderTracks(t, a, dir, footage, nil, "first.mp4")
	if got := highBand(t, first, 4.2, 1.6); got-tone > -20 {
		t.Errorf("the second track's tone is audible (%.1f dB against %.1f with it chosen) "+
			"in a render that asked for the first track alone", got, tone)
	}
	if d, err := ffprobeDur(first); err != nil || d < 7.5 || d > 8.6 {
		t.Errorf("the clip came out %.2f s long (err %v), want the picture's own 8", d, err)
	}
}

// lowBand is how loud a stretch of a file is BELOW 1 kHz, the mirror of
// highBand (produce_mix_test.go) and read the same way: a floor rather than an
// error for a stretch that decoded to silence.
func lowBand(t *testing.T, path string, ss, dur float64) float64 {
	t.Helper()
	out, err := exec.Command("ffmpeg", "-v", "info", "-ss", fmt.Sprintf("%.3f", ss),
		"-t", fmt.Sprintf("%.3f", dur), "-i", path,
		"-af", "lowpass=f=1000,lowpass=f=1000,volumedetect", "-f", "null", "-").CombinedOutput()
	if err != nil {
		t.Fatalf("measuring %s at %gs: %v\n%s", filepath.Base(path), ss, err, out)
	}
	m := meanVolRe.FindStringSubmatch(string(out))
	if m == nil {
		return -91
	}
	v, err := strconv.ParseFloat(m[1], 64)
	if err != nil {
		t.Fatalf("volumedetect said %q", m[1])
	}
	return v
}

// A row may untick the first track and keep a later one -- a capture whose
// track 1 is the game and whose track 0 is a monitor feed nobody wants -- and
// the clip then has to come out carrying the track that was ticked and not the
// one that was not.
//
// This is the one arrangement where the footage's own input contributes no
// sound at all. Its sound is read off [0:a], which is the file's FIRST stream
// and nothing else, so a render that took the input's audio whenever the file
// had any would put the unticked track under every clip -- audibly, and with
// the ticked one mixed on top of it, so the file would still play.
func TestUntickingTheFirstTrackTakesTheFootagesOwnSoundOff(t *testing.T) {
	a := insertApp(t)
	dir := t.TempDir()
	footage := filepath.Join(dir, "obs_2026-01-01_10-00-00.mkv")
	twoTrack(t, footage)

	second := renderTracks(t, a, dir, footage, []int{1}, "second.mp4")
	both := renderTracks(t, a, dir, footage, []int{0, 1}, "both.mp4")

	// the ticked track is there, where it was recorded
	quiet, tone := highBand(t, second, 0.2, 3.4), highBand(t, second, 4.2, 1.6)
	t.Logf("track 1 alone — high band %.1f dB quiet, %.1f dB tone; low band %.1f dB "+
		"against %.1f with both", quiet, tone, lowBand(t, second, 0.2, 3.4),
		lowBand(t, both, 0.2, 3.4))
	if tone-quiet < 20 {
		t.Errorf("the ticked track is not in the clip: %.1f dB where its tone is against "+
			"%.1f dB where it is silent", tone, quiet)
	}
	// and the unticked one is not, measured against the same clip rendered with
	// it ticked so this is a comparison and not a guess at what a tone weighs
	off, on := lowBand(t, second, 0.2, 3.4), lowBand(t, both, 0.2, 3.4)
	if off-on > -20 {
		t.Errorf("the unticked first track is audible (%.1f dB against %.1f with it "+
			"ticked) — the footage input's own sound was taken anyway", off, on)
	}
}

// What the render is handed for a multi-track capture: the further tracks as
// recordings, and the first one NOT among them.
//
// The first track is already in the render, read off the footage's own input
// ([0:a], encodeClip). Handing it over a second time as a recording would put
// the same sound in the mix twice -- the clip still plays, and the capture is
// simply louder than everything else in it for no reason anyone can see.
//
// The clocks are the other half. A further track is glued to the pictures it
// was recorded with, so it has to carry whatever correction that row was
// dragged to, and it has a name of its own so it can then be nudged apart from
// them. Both at once here: the row is dragged 5 s and the track 3 s more, and
// what the render gets has to be 3 s off its own pictures.
func TestTheRenderGetsTheFurtherTracksAndNotTheFirstOne(t *testing.T) {
	a := insertApp(t)
	dir := t.TempDir()
	footage := filepath.Join(dir, "obs_2026-01-01_10-00-00.mkv")
	twoTrack(t, footage)
	a.selTracks = map[string][]int{footage: {0, 1}}

	vids, recs, err := a.sessionTracks([]string{footage}, nil)
	if err != nil {
		t.Fatal(err)
	}
	if len(vids) != 1 || len(recs) != 1 {
		t.Fatalf("the render was handed %d video(s) and %d recording(s), want 1 and 1: %+v",
			len(vids), len(recs), recs)
	}
	if want := trackName(vids[0].base, 1); recs[0].base != want {
		t.Errorf("the recording came out as %q, want %q — the footage's own first track "+
			"is being mixed in on top of the input it is already read from",
			recs[0].base, want)
	}
	if recs[0].track != 1 {
		t.Errorf("the recording says it is track %d, want 1", recs[0].track)
	}

	// dragged: the row by 5 s, and the track by 3 s more under its own name
	a.ed = &cutEditor{shift: map[string]float64{vids[0].base: 5, trackName(vids[0].base, 1): 3}}
	vids, recs, err = a.sessionTracks([]string{footage}, nil)
	if err != nil {
		t.Fatal(err)
	}
	if len(recs) != 1 {
		t.Fatalf("the render was handed %d recording(s) after the drag", len(recs))
	}
	if off := recs[0].start - vids[0].start; off < 2.99 || off > 3.01 {
		t.Errorf("the further track sits %.2f s off its own pictures, want 3 — it either "+
			"missed the row's own correction or its own nudge on top of it", off)
	}
}

// ---- a copied shot of a multi-track file ------------------------------------

// A cut lane is a second look at a row of footage, and it brings that row's
// sound with it (laneAudios, cut_lane.go). "That row's sound" is the file's
// FIRST track and nothing else -- the one the render reads off the footage's
// own input -- and a multi-track capture puts several lanes on one path, so the
// question of which of them a copied shot carries only exists here.
func TestACopiedShotCarriesTheFilesFirstTrack(t *testing.T) {
	row := tlVideo{base: "obs", path: "/x/obs.mkv", start: 100, off: 5, dur: 20}

	// the further track listed FIRST, and with a different width, so taking the
	// wrong one is a wrong answer and not the same answer arrived at twice
	src := []tlAudio{
		{base: "obs #2", path: "/x/obs.mkv", chans: 1, track: 1},
		{base: "obs", path: "/x/obs.mkv", chans: 2, master: true},
	}
	got := laneAudios([]tlVideo{row}, src)
	if len(got) != 1 {
		t.Fatalf("a row of footage came out with %d sound lane(s), want 1", len(got))
	}
	if got[0].chans != 2 || !got[0].master {
		t.Errorf("the copied shot sounds %d channels wide (master %v), want 2 and true — "+
			"it took a further track of the file rather than the one its pictures came with",
			got[0].chans, got[0].master)
	}

	// and a row whose first track the session was told to leave out has no
	// sound of its own to copy: the file is in the session, so the answer is
	// known and it is "none" -- not a reason to go and probe the file again
	only2 := []tlAudio{{base: "obs #2", path: "/x/obs.mkv", chans: 1, track: 1}}
	if got := laneAudios([]tlVideo{row}, only2); len(got) != 0 {
		t.Errorf("a row whose first track is unticked was given %d sound lane(s): %+v — "+
			"the render puts nothing there, so the page would be drawing a lane that is "+
			"not in the finished video", len(got), got)
	}

	// and neither does a silent capture, which reaches the same answer by the
	// other road: it HAS a lane in the session and that lane is nought deep
	silent := []tlAudio{{base: "obs", path: "/x/obs.mkv", chans: 0, master: true}}
	if got := laneAudios([]tlVideo{row}, silent); len(got) != 0 {
		t.Errorf("a copied shot of a silent capture was given %d sound lane(s): %+v — "+
			"a strip of ground with a decode behind it that can only fail", len(got), got)
	}
}

// The other side of it: a file that was never a source -- an insert dropped in
// from outside the session -- has no lane to read an answer off, and that is
// the one case worth opening the file for.
func TestAnInsertFromOutsideTheSessionIsStillProbed(t *testing.T) {
	insertApp(t) // for the ffmpeg check alone
	dir := t.TempDir()
	outside := filepath.Join(dir, "clip.mkv")
	twoTrack(t, outside)

	row := tlVideo{base: "clip", path: outside, start: 0, dur: 8}
	got := laneAudios([]tlVideo{row}, []tlAudio{{base: "mic", path: "/x/mic.wav", chans: 1}})
	// its first track, which is stereo in this fixture -- read off the file
	// itself, because there was no lane to read it off
	if len(got) != 1 || got[0].chans != 2 {
		t.Fatalf("an insert from outside the session came out as %+v, want one stereo "+
			"lane read off the file itself", got)
	}
}

// The run works from a snapshot of the list taken on the GUI thread, because
// the list is a widget and the pipeline is not on that thread (snapItems). The
// track choice has to be in that snapshot or every step after Prepare reads
// nothing and quietly falls back to the first track of everything -- a render
// that is missing a whole lane and never says so.
func TestTheTrackChoiceIsInTheRunsSnapshot(t *testing.T) {
	a := &App{}
	a.snapItems([]sourceItem{
		{path: "/x/obs.mkv", footage: true, tracks: []int{0, 2}},
		{path: "/x/cam.mp4", footage: true},
	})
	got := a.snappedTracks()
	if !reflect.DeepEqual(got["/x/obs.mkv"], []int{0, 2}) {
		t.Errorf("the run was told %v about the multi-track file, want [0 2]", got["/x/obs.mkv"])
	}
	// only the rows that answered: a session with no multi-track file in it
	// hands the pipeline an empty map, and nothing downstream has to tell "the
	// first track" from "no answer" a second time
	if _, ok := got["/x/cam.mp4"]; ok || len(got) != 1 {
		t.Errorf("the snapshot came out as %v, want the answering row alone", got)
	}
	// and a copy of it, not the row's own slice: the page goes on being edited
	// while the run reads this
	got["/x/obs.mkv"][0] = 9
	if a.selItems[0].tracks[0] != 0 {
		t.Errorf("writing to the snapshot reached back into the row: %v", a.selItems[0].tracks)
	}
}

// One probe per file, not one per question.
//
// Opening the Cut step asks four things about every recording -- how long, how
// big, how fast, how many tracks -- and each used to be its own ffprobe
// process, started on the GUI thread. A session of 31 takes is 93 processes in
// a row: measured at 45 ms each, over four seconds, against a three-second hang
// watchdog that duly wrote a stack dump while the window sat there. One call
// per file answers all four; the answer is kept under the file's identity, so
// walking back to the tab costs nothing.
func TestOneProbePerFileAnswersEveryQuestion(t *testing.T) {
	src := readSrc(t, "ffprobe.go")
	for _, want := range []string{
		"format=duration:stream=codec_type,width,height,avg_frame_rate,channels",
		"fileMark(path)", // the key: a file still being written is asked again
		"probeCache[key]",
	} {
		if !strings.Contains(src, want) {
			t.Errorf("ffprobe.go no longer contains %q", want)
		}
	}
	// and the four readers go through it rather than starting their own
	for _, c := range []struct{ file, fn string }{
		{"pipeline.go", `func ffprobeDur\(`},
		{"pipeline.go", `func ffprobeSize\(`},
		{"cut.go", `func ffprobeFPS\(`},
		{"cut_draw.go", `func ffprobeTracks\(`},
	} {
		body := funcBody(t, c.file, c.fn)
		if !strings.Contains(body, "ffprobeInfo(") {
			t.Errorf("%s does not ask the shared probe", c.fn)
		}
		// the one exception: a file whose header carries no duration is
		// decoded to find out, and that stays where it was
		if strings.Contains(body, "ffTool(\"ffprobe\")") {
			t.Errorf("%s still starts its own ffprobe", c.fn)
		}
	}
	// a probe that FAILED is not an empty answer: a silent capture really has
	// no audio stream, a file ffprobe cannot open says nothing
	if !strings.Contains(readSrc(t, "cut_draw.go"), "if info := ffprobeInfo(path); info.ok {") {
		t.Error("ffprobeTracks cannot tell a silent file from an unreadable one")
	}
}
