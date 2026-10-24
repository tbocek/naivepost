package main

// Where a recording sits on the wall clock.
//
// Nothing else on the Cut page is measured against anything else: the lanes,
// the waveforms, the merged transcript, the descriptions and the render all
// subtract the session's start from a source's start and draw the difference.
// So the placement is not a detail of one page -- it is the one number the
// whole session is built on, and it comes from one function.
//
// The rule these pin: a file that NAMES a moment is placed at it, and a file
// that names none is placed at the session's start -- the earliest moment
// anything else names, or 0:00 when nothing does. What it is emphatically NOT
// placed by is its own mtime, which is when the file was written: copy a card,
// re-encode, export or download and that is hours or weeks after the thing was
// shot, and a source dropped a week down the timeline is a row nobody can find.
// At the session's start it is at least on screen, next to the others, where
// the right drag can line it up by ear (cut_shift.go).

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

// stampedAt is a file name carrying a moment, and that moment.
func stampedAt(t *testing.T, y, mo, d, h, mi, s int) (string, float64) {
	t.Helper()
	when := time.Date(y, time.Month(mo), d, h, mi, s, 0, time.Local)
	name := "cam_" + when.Format("2006-01-02_15-04-05") + ".mkv"
	if got, ok := nameStamp(name); !ok || got != float64(when.Unix()) {
		t.Fatalf("%s does not read back as its own moment (%g, %v)", name, got, ok)
	}
	return name, float64(when.Unix())
}

// ---- a name is the only timestamp worth believing ---------------------------

func TestAFileThatNamesAMomentIsPlacedAtIt(t *testing.T) {
	cam, camT := stampedAt(t, 2026, 8, 8, 19, 55, 0)
	mic, micT := stampedAt(t, 2026, 8, 8, 19, 57, 30)

	at, zero := srcClock([]string{"/src/" + cam, "/src/" + mic})
	if at["/src/"+cam] != camT || at["/src/"+mic] != micT {
		t.Errorf("placed at %g and %g, want %g and %g",
			at["/src/"+cam], at["/src/"+mic], camT, micT)
	}
	// the session starts when the first of them started, not at the first in
	// the list and not at some average
	if zero != camT {
		t.Errorf("the session starts at %g, want the earlier source's %g", zero, camT)
	}
}

func TestAFileThatNamesNoMomentStartsWhereTheSessionDoes(t *testing.T) {
	cam, camT := stampedAt(t, 2026, 8, 8, 19, 55, 0)
	_, lateT := stampedAt(t, 2026, 8, 8, 20, 30, 0)
	late := "cam_" + time.Unix(int64(lateT), 0).Format("2006-01-02_15-04-05") + ".mkv"

	at, zero := srcClock([]string{"/src/" + cam, "/src/GX010042.MP4", "/src/" + late})
	if got := at["/src/GX010042.MP4"]; got != camT {
		t.Errorf("the unstamped file is at %g, want the session's start %g", got, camT)
	}
	// and it does not drag the start with it: zero is what the NAMES say, so
	// dropping an unstamped file into a session cannot move everything else
	if zero != camT {
		t.Errorf("the session starts at %g, want %g", zero, camT)
	}
}

func TestWhenNothingNamesAMomentTheSessionStartsAtNought(t *testing.T) {
	// no clock to join and no order worth guessing at: they all start together,
	// and the timeline reads 0:00 at the left edge like a single-file project
	at, zero := srcClock([]string{"/src/GX010042.MP4", "/src/rec.wav", "/src/a.mkv"})
	if zero != 0 {
		t.Errorf("the session starts at %g, want 0", zero)
	}
	for p, got := range at {
		if got != 0 {
			t.Errorf("%s is at %g, want 0", p, got)
		}
	}
}

func TestPlacingNothingAtAllIsNotACrash(t *testing.T) {
	at, zero := srcClock(nil)
	if len(at) != 0 || zero != 0 {
		t.Errorf("an empty session placed %v and starts at %g, want none and 0", at, zero)
	}
}

func TestThePlacementIsTheSameWhicheverOrderTheSourcesArriveIn(t *testing.T) {
	// the page holds its sources in arrival order, the render in the order they
	// were chosen, and the describe pass one video at a time. Same seconds, or
	// the waveform and the sound end up in two different spots
	cam, camT := stampedAt(t, 2026, 8, 8, 19, 55, 0)
	mic, _ := stampedAt(t, 2026, 8, 8, 19, 57, 30)
	loose := "/src/GX010042.MP4"

	one, z1 := srcClock([]string{"/src/" + cam, "/src/" + mic, loose})
	two, z2 := srcClock([]string{loose, "/src/" + mic, "/src/" + cam})
	if z1 != z2 || z1 != camT {
		t.Errorf("the session starts at %g one way round and %g the other, want %g", z1, z2, camT)
	}
	for p, got := range one {
		if two[p] != got {
			t.Errorf("%s is at %g one way round and %g the other", p, got, two[p])
		}
	}
}

func TestASourceListedTwiceIsStillPlacedOnce(t *testing.T) {
	// sourceStart appends the file it was asked about to the whole session,
	// which for a session source names it twice
	cam, camT := stampedAt(t, 2026, 8, 8, 19, 55, 0)
	at, zero := srcClock([]string{"/src/" + cam, "/src/" + cam})
	if len(at) != 1 || at["/src/"+cam] != camT || zero != camT {
		t.Errorf("placed %v starting at %g, want one entry at %g", at, zero, camT)
	}
}

// ---- it reads names, and nothing else ---------------------------------------

func TestPlacingASourceTouchesNoDisk(t *testing.T) {
	// what it replaced ran ffprobe and stat per file, which is why placing was
	// something only a runner could afford to ask. The lanes ask on a redraw
	cam, camT := stampedAt(t, 2026, 8, 8, 19, 55, 0)
	at, zero := srcClock([]string{"/nowhere/" + cam, "/nowhere/gone.mkv"})
	if at["/nowhere/"+cam] != camT || at["/nowhere/gone.mkv"] != camT || zero != camT {
		t.Errorf("files that do not exist placed at %v (start %g)", at, zero)
	}

	body := funcBody(t, "transcript.go", `func srcClock\(paths \[\]string\) `)
	for _, bad := range []string{"os.Stat", "ModTime", "ffprobe"} {
		if strings.Contains(body, bad) {
			t.Errorf("srcClock reaches for %s; it is asked on every redraw", bad)
		}
	}
}

func TestTheNameIsReadOffTheFileAndNotThePath(t *testing.T) {
	// a stamp is what the recorder wrote; a folder called 2019-01-01 is where
	// somebody filed it
	cam, camT := stampedAt(t, 2026, 8, 8, 19, 55, 0)
	at, _ := srcClock([]string{filepath.Join("/2019-01-01_00-00-00", cam)})
	if got := at[filepath.Join("/2019-01-01_00-00-00", cam)]; got != camT {
		t.Errorf("placed at %g by its folder, want %g from its name", got, camT)
	}
}

// ---- one file, asked about on its own ---------------------------------------

func TestAFileFromOutsideTheSessionIsPlacedByTheSameRule(t *testing.T) {
	cam, camT := stampedAt(t, 2026, 8, 8, 19, 55, 0)
	sting, stingT := stampedAt(t, 2026, 8, 8, 21, 10, 0)
	a := &App{selVid: []string{"/src/" + cam}}

	// a sting dropped on a cut lane is not a session source, and it is placed
	// exactly as one: its own stamp when it has one...
	if got := a.sourceStart("/other/" + sting); got != stingT {
		t.Errorf("the sting is at %g, want its own %g", got, stingT)
	}
	// ...and the session's start when it has none
	if got := a.sourceStart("/other/bumper.mp4"); got != camT {
		t.Errorf("the unstamped sting is at %g, want the session's start %g", got, camT)
	}
	// and asking about it changes nothing for the session itself
	if got := a.sourceStart("/src/" + cam); got != camT {
		t.Errorf("the camera moved to %g, want %g", got, camT)
	}
}

func TestTheSessionAlwaysHasAStart(t *testing.T) {
	// it used to answer MaxFloat64 for "could not place anything", and every
	// caller had to know that number. Now the rule itself covers the case
	a := &App{}
	if got := a.sessionZero(); got != 0 {
		t.Errorf("a session with no sources starts at %g, want 0", got)
	}
	a.selVid = []string{"/src/GX010042.MP4"}
	if got := a.sessionZero(); got != 0 {
		t.Errorf("a session where nothing names a moment starts at %g, want 0", got)
	}
	cam, camT := stampedAt(t, 2026, 8, 8, 19, 55, 0)
	a.selVid = append(a.selVid, "/src/"+cam)
	if got := a.sessionZero(); got != camT {
		t.Errorf("the session starts at %g, want %g", got, camT)
	}

	// the dead sentinel is gone from the one place that tested for it: a
	// publish run that bailed there would offer no thumbnails at all
	if body := funcBody(t, "publish.go", `func \(a \*App\) publishShots\(\) \[\]pubShot \{`); strings.Contains(body, "MaxFloat64") {
		t.Error("publishShots still bails on the old cannot-place sentinel")
	}
}

// ---- the steps that place sources all come through the one door -------------

func TestEveryStepPlacesItsSourcesTheSameWay(t *testing.T) {
	// the trap is a step that quietly does its own arithmetic: the waveform is
	// drawn by one and the sound is cut by another, and a disagreement of a few
	// seconds is a scene that plays the wrong words
	for _, c := range []struct{ file, head string }{
		{"cut.go", `func \(ed \*cutEditor\) reload\(\) error \{`},
		{"produce.go", `func \(a \*App\) sessionTracks\(vids, auds \[\]string\) `},
		{"transcript.go", `func \(a \*App\) fixTranscripts\(`},
		{"describe.go", `func \(a \*App\) commentary\(`},
		{"narrate.go", `func \(a \*App\) sessionZero\(\) float64 \{`},
	} {
		if body := funcBody(t, c.file, c.head); !strings.Contains(body, "srcClock(") {
			t.Errorf("%s: %s places its sources some other way", c.file, c.head)
		}
	}
	// and the frame names, which are the times the descriptions are written
	// against: the caller holds the session, so it hands the start down
	if body := funcBody(t, "pipeline.go", `func stampFrames\(fdir, video string, start, interval float64\) `); strings.Contains(body, "sourceStart") {
		t.Error("stampFrames places the video itself instead of being told where it is")
	}
	if src := readSrc(t, "pipeline.go"); strings.Count(src, "stampFrames(fdir, video, a.sourceStart(video), interval)") != 2 {
		t.Error("an extracted frame folder is stamped off some other clock than the session's")
	}
}

// ---- a lane's own clock -----------------------------------------------------

func TestAStingOnALaneIsNamedForItsOwnMoment(t *testing.T) {
	// wall is the wall clock of the file's second NOUGHT -- produce names the
	// clips it cuts off it (stampName(c.video.wall + c.local)) -- and a stamp
	// in the name is exactly that. So where the window opens does not come into
	// it: the same file windowed twice must name the same moments both times
	sting, stingT := stampedAt(t, 2026, 8, 8, 21, 10, 0)
	a := &App{}
	cam := tlVideo{base: "a", path: "/f/a.mp4", start: 0, wall: 1000, dur: 100}
	got := a.laneVideos([]cutLane{
		{Name: "top", Src: "/other/" + sting, At: 10, Off: 0, Dur: 5},
		{Name: "tail", Src: "/other/" + sting, At: 40, Off: 30, Dur: 5},
	}, []tlVideo{cam})
	if len(got) != 2 {
		t.Fatalf("laneVideos drew %v", got)
	}
	for _, v := range got {
		if v.wall != stingT {
			t.Errorf("%s: second nought is %g, want the file's own %g", v.base, v.wall, stingT)
		}
	}

	// a file naming no moment keeps the place the row was put at, which is the
	// only thing anyone knows about it
	un := a.laneVideos([]cutLane{{Name: "bump", Src: "/other/bumper.mp4", At: 10, Off: 4, Dur: 5}},
		[]tlVideo{cam})
	if len(un) != 1 || un[0].wall != 1000+10-4 {
		t.Errorf("the unstamped sting is at %v, want second nought at %g", un, 1000.0+10-4)
	}
}

func TestALaneOntoASessionSourceKeepsTheRecordingsClock(t *testing.T) {
	// a copied shot is one recording seen twice, and a corrected clock lives on
	// the recording: reading the name again would undo the right drag
	sting, _ := stampedAt(t, 2026, 8, 8, 21, 10, 0)
	a := &App{}
	src := tlVideo{base: "s", path: "/other/" + sting, start: 0, wall: 1234, dur: 100}
	got := a.laneVideos([]cutLane{{Name: "copy", Src: "/other/" + sting, At: 10, Dur: 5}},
		[]tlVideo{src})
	if len(got) != 1 || got[0].wall != 1234 {
		t.Errorf("the copy is at %v, want the recording's own 1234", got)
	}
}

// ---- what the Inputs page promises about a name -----------------------------

func TestTheRowWarningSaysWhatActuallyHappens(t *testing.T) {
	// the warning is the only place a user is told the rule before it bites, so
	// it must describe the rule that is in force
	src := readSrc(t, "sources.go")
	if strings.Contains(src, "by its file date") {
		t.Error("the unstamped-source warning still promises placement by file date")
	}
	i := strings.Index(src, "No timestamp in the file name")
	if i < 0 {
		t.Fatal("the unstamped-source warning is gone")
	}
	if tip := src[i:min(len(src), i+600)]; !strings.Contains(tip, "session does") {
		t.Error("the warning does not say the file starts where the session does")
	}
}

// ---- the describe pass hears what the Cut page shows -------------------------

func TestTheDescriberPlacesARecordingWhereTheWholeSessionDoes(t *testing.T) {
	// commentary holds one video and the voices, and the temptation is to put
	// just those on a clock. That clock's zero is the earliest of THEM -- so an
	// unstamped camera would land on the recorder's own start here and on the
	// session's start everywhere else, and the words would be laid against the
	// wrong frames by exactly the difference
	cam, camT := stampedAt(t, 2026, 8, 8, 19, 55, 0)
	mic, micT := stampedAt(t, 2026, 8, 8, 19, 57, 30)
	a := &App{outDir: t.TempDir()}
	a.selVid = []string{"/src/" + cam, "/src/GX010042.MP4"} // the second names no moment
	a.selAud = []string{"/src/" + mic}

	dir := filepath.Join(a.inputsDir(), baseName(mic))
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(dir, "transcript.tsv"),
		[]byte("0.00\t2.00\tSPEAKER_00\thello\n"), 0o644); err != nil {
		t.Fatal(err)
	}

	got := a.commentary("/src/GX010042.MP4", a.selAud)
	if len(got) != 1 || len(got[0].rows) != 1 {
		t.Fatalf("the describer heard %v", got)
	}
	// the camera is at the session's start, and the recorder started 2:30 later
	if want := micT - camT; got[0].rows[0].s != want {
		t.Errorf("the first line is heard %g s in, want %g", got[0].rows[0].s, want)
	}
}
