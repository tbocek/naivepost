package main

// The source list, headless -- render() no-ops without a box, so everything the
// rows do is reachable from a test. What is being pinned here is the thing the
// merge was for: one file, one row, one transcript, and a role that says what
// the file is rather than which folder it came out of.

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"
	"time"
)

// mkSources writes empty files under a temp dir and returns their paths.
func mkSources(t *testing.T, names ...string) (dir string, paths []string) {
	t.Helper()
	dir = t.TempDir()
	for _, n := range names {
		p := filepath.Join(dir, n)
		if err := os.WriteFile(p, []byte("x"), 0o644); err != nil {
			t.Fatal(err)
		}
		paths = append(paths, p)
	}
	return dir, paths
}

// The default roles are the guess the old two folders made for you, and adding
// the same file twice must not put it in twice: the two ways in overlap, and a
// file listed twice is transcribed twice and lands twice in the timeline.
func TestAddingSourcesGuessesTheRoleAndNeverDuplicates(t *testing.T) {
	dir, paths := mkSources(t, "clip.mkv", "voice.flac", "notes.txt")
	s := &sourceList{}

	if n := s.add(paths...); n != 2 {
		t.Fatalf("added %d of three files, want the two playable ones", n)
	}
	if s.items[0].path != paths[0] || !s.items[0].footage {
		t.Errorf("a video arrived as %+v, want footage", s.items[0])
	}
	if s.items[1].footage {
		t.Errorf("a recording arrived as footage: %+v", s.items[1])
	}
	if n := s.add(paths[0]); n != 0 || len(s.items) != 2 {
		t.Errorf("re-adding a file added %d rows; the list is now %+v", n, s.items)
	}
	// a whole folder is how a session usually arrives, and it overlaps whatever
	// was picked by hand first
	if n := s.addDir(dir); n != 0 {
		t.Errorf("adding the folder the files came from added %d more", n)
	}
}

// One file is one row even when it is two things -- that is the merge. split()
// is where the pipeline reads it, and appending its two halves has to be the
// session exactly once, or a screen recording that is also a voice is described
// and transcribed twice.
func TestSplitIsEverySourceExactlyOnce(t *testing.T) {
	_, paths := mkSources(t, "cam.mkv", "screen.mkv", "voice.flac")
	s := &sourceList{}
	s.add(paths...)
	s.setFootage(1, false) // the screen capture, kept only for what is on it

	foot, rest := s.split()
	if len(foot) != 1 || foot[0] != paths[0] {
		t.Errorf("footage = %v, want only the camera", foot)
	}
	if len(rest) != 2 || rest[0] != paths[1] || rest[1] != paths[2] {
		t.Errorf("the rest = %v, want the screen capture and the recording, in list order", rest)
	}
	seen := map[string]int{}
	for _, p := range append(append([]string{}, foot...), rest...) {
		seen[p]++
	}
	if len(seen) != len(paths) {
		t.Fatalf("the two halves hold %d distinct files, want %d", len(seen), len(paths))
	}
	for p, n := range seen {
		if n != 1 {
			t.Errorf("%s appears %d times in what the pipeline runs", filepath.Base(p), n)
		}
	}
}

// Footage means frames come out of it, and an audio file has none to give: the
// toggle must refuse, and so must a hand-edited project -- Prepare would
// otherwise point ffmpeg's frame extraction at a flac and fail half-way in.
func TestOnlyAVideoCanBeFootage(t *testing.T) {
	_, paths := mkSources(t, "voice.flac", "clip.mkv")
	s := &sourceList{}
	s.add(paths...)

	s.setFootage(0, true)
	if s.items[0].footage {
		t.Error("a flac became footage")
	}
	s.setFootage(1, false) // the direction that IS a choice: a video, sound only
	if s.items[1].footage {
		t.Error("a video could not give its frames up")
	}
	s.setFootage(1, true)
	if !s.items[1].footage {
		t.Error("a video could not take its frames back")
	}

	s.load([]sourceItem{{path: paths[0], footage: true}})
	if s.items[0].footage {
		t.Error("a hand-edited project made a flac footage")
	}
}

// Somebody has to be the narrator -- it is the voice Produce speaks in -- so a
// first session should not have to be told that the one recording in it is the
// one to clone. A mic track is the better guess than a screen capture, which
// holds everyone at once.
func TestTheFirstRecordingIsTaggedAsTheNarrator(t *testing.T) {
	_, paths := mkSources(t, "clip.mkv", "voice.flac")
	s := &sourceList{}
	s.add(paths[0])
	if s.narratorPath(1) != paths[0] {
		t.Errorf("a session of one video tagged %q, want the video itself", s.narratorPath(1))
	}
	s.add(paths[1])
	if got := s.narratorPath(1); got != paths[0] {
		t.Errorf("adding a recording moved the tag to %q; a tag already given stays given", got)
	}

	// a session that starts with a recording tags that one, not the footage
	fresh := &sourceList{}
	fresh.add(paths[1], paths[0])
	if got := fresh.narratorPath(1); got != paths[1] {
		t.Errorf("narrator 1 = %q, want the recording", got)
	}
	if got := fresh.items[1].narrator; got != 0 {
		t.Errorf("the footage was tagged %d as well; one tag is one person", got)
	}
}

// A slot is a person, so two rows cannot both be narrator 2 -- Produce would
// pick one of them silently. Clicking steps over the taken slots and off the
// end goes back to untagged.
func TestCyclingStepsOverTakenSlotsAndBackToNone(t *testing.T) {
	_, paths := mkSources(t, "me.flac", "mate.flac", "third.flac")
	s := &sourceList{}
	s.add(paths...)
	if s.items[0].narrator != 1 {
		t.Fatalf("the first recording is %+v, want narrator 1", s.items[0])
	}

	// row 1 walks the free slots: 1 is taken, so it starts at 2
	for _, want := range []int{2, 3, 4, 0, 2} {
		s.cycleNarrator(1)
		if got := s.items[1].narrator; got != want {
			t.Fatalf("cycling reached %d, want %d", got, want)
		}
	}
	// and row 2 cannot land on what row 1 holds
	s.cycleNarrator(2)
	if got := s.items[2].narrator; got != 3 {
		t.Errorf("the third recording landed on %d, want 3 -- 1 and 2 are taken", got)
	}
	if s.narratorPath(2) != paths[1] {
		t.Errorf("narrator 2 = %q, want it still held by the row that claimed it", s.narratorPath(2))
	}
	// the tag is a role, not a kind of file: untagged is still transcribed
	_, rest := s.split()
	if len(rest) != 3 {
		t.Errorf("the pipeline sees %d recordings, want all three", len(rest))
	}
}

// Removing throws a row out of the session and nothing else: the file stays on
// disk, because a project is not a folder. If the narrator leaves with it,
// somebody else has to become the narrator, or Produce has nobody to speak as.
func TestRemovingARowLeavesTheFileAndFindsANewNarrator(t *testing.T) {
	_, paths := mkSources(t, "me.flac", "mate.flac")
	s := &sourceList{}
	s.add(paths...)

	s.remove(0)
	if len(s.items) != 1 || s.items[0].path != paths[1] {
		t.Fatalf("the list is %+v, want only the second recording", s.items)
	}
	if !exists(paths[0]) {
		t.Error("removing a row deleted the file")
	}
	if got := s.narratorPath(1); got != paths[1] {
		t.Errorf("narrator 1 = %q after the narrator was removed, want the remaining recording", got)
	}
	s.remove(5) // out of range: a stale row index from a rebuilt list
	if len(s.items) != 1 {
		t.Errorf("an impossible index changed the list: %+v", s.items)
	}
}

// A source that quietly stopped being in the list is how a render comes out
// missing an angle nobody can account for -- so pruning names what it dropped,
// and drops nothing else.
func TestPruningDropsOnlyWhatIsGoneAndSaysSo(t *testing.T) {
	_, paths := mkSources(t, "clip.mkv", "voice.flac")
	s := &sourceList{}
	s.add(paths...)
	if gone := s.prune(); len(gone) != 0 {
		t.Fatalf("pruning a list of files that are all there dropped %v", gone)
	}
	if err := os.Remove(paths[1]); err != nil {
		t.Fatal(err)
	}
	gone := s.prune()
	if len(gone) != 1 || gone[0] != paths[1] {
		t.Fatalf("pruning reported %v, want the file that was removed", gone)
	}
	if len(s.items) != 1 || s.items[0].path != paths[0] {
		t.Errorf("the list is %+v, want the file that is still there", s.items)
	}
	if s.narratorPath(1) != paths[0] {
		t.Error("the narrator left with the pruned file and nobody replaced them")
	}
}

// load is a project being opened, and a project file is something you can edit
// by hand. Two rows seated in one slot is the case that must not survive it:
// the first keeps the slot, the second loses the tag rather than the list
// silently answering with either one.
func TestLoadingRejectsTwoPeopleInOneSlot(t *testing.T) {
	_, paths := mkSources(t, "a.flac", "b.flac", "c.flac")
	s := &sourceList{}
	s.load([]sourceItem{
		{path: paths[0], narrator: 2},
		{path: paths[1], narrator: 2},
		{path: paths[2], narrator: 9},
	})
	if got := s.items[0].narrator; got != 2 {
		t.Errorf("the first claim on slot 2 came back as %d", got)
	}
	if got := s.items[2].narrator; got != 0 {
		t.Errorf("a slot that does not exist survived as %d", got)
	}
	// the second claim lost the slot, and since that file named itself as
	// somebody and nobody in this project was narrator 1, it becomes narrator 1
	// -- filling the empty slot, never taking an occupied one
	if got := s.narratorPath(1); got != paths[1] {
		t.Errorf("narrator 1 = %q, want the row whose duplicate claim was cleared", got)
	}
	if got := s.narratorPath(2); got != paths[0] {
		t.Errorf("narrator 2 = %q, want the row that claimed it first", got)
	}
}

// The mic button cycles a row through the slots and back around to none -- a
// project where the user looked at the tag and turned it off. That choice is
// stored, and load used to run autoTag over it anyway, which cannot tell "the
// user untagged everyone" from "nobody was ever asked" -- so the tag came back
// on every restart. Roles come back exactly as stored; load re-tags only when
// it had to strip a broken tag itself.
func TestAnUntaggedProjectComesBackUntagged(t *testing.T) {
	_, paths := mkSources(t, "screen.mp4", "b.flac")
	s := &sourceList{}
	s.load([]sourceItem{
		{path: paths[0], footage: true},
		{path: paths[1]},
	})
	for i, it := range s.items {
		if it.narrator != 0 {
			t.Errorf("row %d came back as narrator %d, want the stored none", i, it.narrator)
		}
	}
}

// autoTag fills the empty slot 1; it must never move somebody who is already
// somebody. Re-casting the narration -- and freeing the slot they were in --
// out from under an explicit tag is not something a list should do on its own.
func TestAutoTaggingNeverMovesAnExplicitTag(t *testing.T) {
	_, paths := mkSources(t, "mate.flac", "me.flac")
	s := &sourceList{}
	s.load([]sourceItem{{path: paths[0], narrator: 3}})
	if got := s.items[0].narrator; got != 3 {
		t.Fatalf("the only row was re-tagged from 3 to %d", got)
	}
	if s.narratorPath(1) != "" {
		t.Errorf("slot 1 was filled by %q, want it left empty", s.narratorPath(1))
	}
	// ...and the next row that arrives is the one that takes it
	s.add(paths[1])
	if got := s.narratorPath(1); got != paths[1] {
		t.Errorf("narrator 1 = %q, want the recording that had no tag", got)
	}
}

// Sources come from anywhere now, and every step names a source's output folder
// after the file's stem -- so two files with the same stem quietly share one
// transcript, and the likeliest pair is a camera take and its own sound file.
// The run refuses; the list itself still holds both, because they ARE two files.
func TestSourcesThatWouldShareAnOutputFolderAreFound(t *testing.T) {
	_, paths := mkSources(t, "clip.mkv", "voice.flac")
	s := &sourceList{}
	s.add(paths...)
	if x, y := s.clash(); x != "" {
		t.Fatalf("two differently named files clashed: %s, %s", x, y)
	}

	// the camera's own sound take, beside it
	_, same := mkSources(t, "clip.flac")
	s.add(same[0])
	x, y := s.clash()
	if x != paths[0] || y != same[0] {
		t.Fatalf("clash = (%q, %q), want the two files that are both inputs/clip", x, y)
	}
	if len(s.items) != 3 {
		t.Errorf("the list dropped one of them: %+v", s.items)
	}
}

// The stamps recorders actually write, and the digit runs that only look like
// one. The row warning and sourceStart both stand on nameStamp, so what this
// table accepts is exactly what is placed on the session clock -- a format
// missing here is a file silently placed by its mtime instead.
func TestTimestampsAreReadFromTheNamesRecordersWrite(t *testing.T) {
	want := time.Date(2026, 8, 8, 19, 55, 15, 0, time.Local)
	for _, name := range []string{
		"2026-08-08_19-55-15.flac",                          // what our own outputs are called
		"2026-08-08 19-55-15.mkv",                           // OBS's default, and Windows Game Bar's
		"Replay_2026-08-08_19-55-15.mkv",                    // ...and its replay buffer
		"com.AnotherAxiom.GorillaTag-20260808-195515-0.mp4", // a Quest capture
		"20260808-195515.mp4",                               // compact recorder stamp
		"VID_20260808_195515.mp4",                           // a phone
		"PXL_20260808_195515123.mp4",                        // a phone that appends milliseconds
		"DJI_20260808195515_0001_D.mp4",                     // no separator at all
		"rec 2026.08.08_19.55.15.wav",                       // dotted, from a handheld recorder
		"Desktop 2026.08.08 - 19.55.15.03.DVR.mp4",          // ShadowPlay, frame counter and all
		"Screen Recording 2026-08-08 at 19.55.15.mov",       // QuickTime
		"Screen Recording 2026-08-08 at 7.55.15 PM.mov",     // ...on a 12-hour clock
		"2026-08-08T19:55:15.mkv",                           // ISO by hand
		"1920x1080_20260808_195515.mkv",                     // a resolution before the real stamp
		"clip_2026-08-08_19-55-15_fix.mkv",                  // trailing junk after it
		fmt.Sprintf("RPReplay_Final%d.mp4", want.Unix()),    // an iPhone: bare unix seconds
	} {
		got, ok := nameStamp(name)
		if !ok {
			t.Errorf("%s: no stamp found", name)
			continue
		}
		if got != float64(want.Unix()) {
			t.Errorf("%s: read as %s, want %s", name,
				time.Unix(int64(got), 0).Format(stampFmt), want.Format(stampFmt))
		}
	}
	for _, name := range []string{
		"voice.flac",              // nothing at all
		"GX010042.MP4",            // a GoPro counter, not a date
		"IMG_20260808_WA0012.jpg", // a date without a time cannot place an hours-long session
		"clip_1920x1080_60fps.mkv",
		"86402026080821.mp4",  // digits that swallow a would-be stamp
		"20263108-195515.mkv", // looks like a stamp, but there is no month 31
		"20260808-256060.mkv", // ...and no hour 25, even read around the 2
		"take_1234567890.mp4", // ten digits, but 2009 is not this decade
	} {
		if got, ok := nameStamp(name); ok {
			t.Errorf("%s: read as %s, want no stamp",
				name, time.Unix(int64(got), 0).Format(stampFmt))
		}
	}

	// the other half of the 12-hour clock: 12 AM is midnight, not noon
	mid := time.Date(2026, 8, 8, 0, 55, 15, 0, time.Local)
	if got, ok := nameStamp("Screen Recording 2026-08-08 at 12.55.15 AM.mov"); !ok || got != float64(mid.Unix()) {
		t.Errorf("12.55.15 AM read as %v (ok=%v), want %s", got, ok, mid.Format(stampFmt))
	}
}
