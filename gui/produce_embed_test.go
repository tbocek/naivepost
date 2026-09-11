package main

// The two files a web page needs, which are not the two a desktop player needs.

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// WebVTT is not "the .srt with a header": the stamps are written with a full
// stop and the three markup characters have to be escaped, or a cue carrying a
// spoken "R&D" comes out mangled and one carrying "<" loses the rest of its
// line. It is a separate file from the .srt because <track> parses WebVTT
// alone -- an .srt in a <track> is fetched, fails to parse, and shows nothing.
func TestWebVTTIsWebVTTAndNotAnSrt(t *testing.T) {
	got := vttText([]subCue{
		{s: 0.012, e: 5.132, text: "Welcome back. R&D <this>\nsecond row"},
		{s: 5.132, e: 10.732, text: "and on"},
	})
	if !strings.HasPrefix(got, "WEBVTT\n\n") {
		t.Fatalf("no WEBVTT header, so no player will read it:\n%s", got)
	}
	if !strings.Contains(got, "00:00:00.012 --> 00:00:05.132") {
		t.Errorf("the stamps are not WebVTT's:\n%s", got)
	}
	if strings.Contains(got, ",") {
		t.Errorf("an .srt comma survived into the WebVTT:\n%s", got)
	}
	if !strings.Contains(got, "R&amp;D &lt;this&gt;") {
		t.Errorf("the markup characters are not escaped, so the cue is read as tags:\n%s", got)
	}
	// the line break inside a cue is content: two rows of subtitle, not two cues
	if !strings.Contains(got, "&lt;this&gt;\nsecond row\n\n") {
		t.Errorf("a cue's second row was lost or split off:\n%s", got)
	}
	if n := strings.Count(got, "-->"); n != 2 {
		t.Errorf("%d cues in the file, want 2", n)
	}
}

// The tag is written to be pasted, so its shape is the shape somebody asked
// for: poster, controls, src, preload, then one self-closing <track> per
// language with the first defaulted.
func TestTheVideoTagIsTheOneYouPasteIn(t *testing.T) {
	got := embedHTML("news.mp4", "news.jpg", []subTrack{
		{code: "en", name: "English", path: "/out/news.vtt"},
		{code: "de", name: "German", path: "/out/news.de.vtt"},
	})
	want := `<video poster="news.jpg" controls src="news.mp4" preload="none">
 <track label="English" kind="subtitles" srclang="en" src="news.vtt" default />
 <track label="German" kind="subtitles" srclang="de" src="news.de.vtt" />
</video>
`
	if got != want {
		t.Errorf("the tag is\n%s\nwant\n%s", got, want)
	}
	// only the first is default: two defaults is two sets of subtitles drawn
	// over each other
	if strings.Count(got, " default") != 1 {
		t.Error("more than one track is marked default")
	}
	// the paths are RELATIVE: the file sits beside the video, and an absolute
	// path out of somebody's /mnt would break the moment the page is uploaded
	if strings.Contains(got, "/out/") {
		t.Error("the tag names a path on this machine rather than the file beside it")
	}
	// and with nothing drawn yet there is no poster attribute at all, rather
	// than one pointing at a file that is not there
	if strings.Contains(embedHTML("news.mp4", "", nil), "poster") {
		t.Error("a video with no thumbnail still claims a poster")
	}
}

// Which tracks the tag offers is read off the disk, not off the render: the
// press that writes it may have skipped the encode entirely, and then the
// subtitles beside the video are the ones the render before left.
func TestTheTagOffersTheTracksThatAreThere(t *testing.T) {
	dir := t.TempDir()
	out := filepath.Join(dir, "news.mp4")
	// news2.* is the render before this one, in the folder people actually
	// keep these in: a glob of "news*.vtt" would offer another video's
	// subtitles as this one's
	for _, n := range []string{"news.vtt", "news.de.vtt", "news.fr.vtt", "news.srt",
		"other.vtt", "news2.vtt", "news2.de.vtt"} {
		if err := os.WriteFile(filepath.Join(dir, n), []byte("WEBVTT\n\n"), 0o644); err != nil {
			t.Fatal(err)
		}
	}
	a := &App{}
	got := a.embedTracks(out)
	if len(got) != 3 {
		t.Fatalf("%d tracks, want the three .vtt files beside the video: %+v", len(got), got)
	}
	// the video's own language first -- it is the one marked default -- and
	// the translations after it, in an order that does not change between runs
	if got[0].code != "en" || got[0].name != "English" {
		t.Errorf("the first track is %+v, want the video's own language", got[0])
	}
	if got[1].code != "fr" || got[2].code != "de" {
		if got[1].name > got[2].name {
			t.Errorf("the translations are not in a stable order: %q then %q", got[1].name, got[2].name)
		}
	}
	for _, tr := range got {
		if strings.HasSuffix(tr.path, ".srt") || strings.Contains(tr.path, "other") ||
			strings.Contains(filepath.Base(tr.path), "news2") {
			t.Errorf("%s is not one of this video's subtitle tracks", tr.path)
		}
	}
	// and the same list is what a render CLEARS before it writes its own, so
	// a language that failed to translate this time does not leave last
	// time's beside the video -- and never touches the neighbour's
	for _, f := range subSideFiles(out) {
		if strings.Contains(filepath.Base(f), "news2") || strings.Contains(f, "other") {
			t.Errorf("%s would be deleted, and it belongs to another video", f)
		}
	}
	if !strings.Contains(funcBody(t, "produce.go", `func \(a \*App\) produce\(`),
		"for _, f := range subSideFiles(st.OutFile) {") {
		t.Error("a render no longer clears the subtitles of the render before it")
	}
}

// Both files, for every track, beside the video: the .srt a desktop player
// reads and the .vtt a page reads. And the tag is written when the whole run
// is over, not at the end of the render -- the poster is the thumbnail, and
// that is drawn by the OTHER half of the same press.
func TestTheRenderWritesBothSubtitleFormatsAndThenTheTag(t *testing.T) {
	body := funcBody(t, "produce.go", `func \(a \*App\) produce\(`)
	if !strings.Contains(body, `os.WriteFile(stem+tail+".srt"`) ||
		!strings.Contains(body, `os.WriteFile(stem+tail+".vtt", []byte(vttText(t.cues))`) {
		t.Error("the render no longer writes both formats beside the video")
	}
	run := funcBody(t, "produce.go", `func \(a \*App\) produceRun\(words bool\) \{`)
	wait := strings.Index(run, "wg.Wait()")
	tag := strings.Index(run, "a.writeEmbed(st.OutFile, st.Codec)")
	if wait < 0 || tag < 0 || tag < wait {
		t.Errorf("the <video> tag is written before both halves are in (%d, %d) -- "+
			"the poster would be missing on the press that drew it", wait, tag)
	}
}

// A tag is written for every render, including the ones a browser cannot play
// — the video is still the deliverable, and mkv or h265 is a legitimate answer
// for a file that is going on a disk. What must not happen is finding out from
// a black rectangle on the page.
func TestATagForAFileNoBrowserPlaysSaysSo(t *testing.T) {
	if webUnplayable("/out/news.mp4", "h264") != "" {
		t.Error("the combination that plays everywhere is reported as a problem")
	}
	if webUnplayable("/out/news.webm", "vp9") != "" {
		t.Error("webm/vp9 is a page's own format and is reported as a problem")
	}
	if !strings.Contains(webUnplayable("/out/news.mkv", "h264"), "Matroska") {
		t.Error("an mkv is offered to a page without a word")
	}
	if !strings.Contains(webUnplayable("/out/news.mp4", "h265"), "Firefox") {
		t.Error("h265 is offered to a page without a word")
	}
	// and the container that forces its codec moves the dropdown with it, so
	// the page cannot read "webm / h264" while the encoder is told VP9
	sync := funcBody(t, "produce.go", `func \(p \*producer\) syncExt\(\)`)
	if !strings.Contains(sync, `setPick(p.codec, prodCodecs, "vp9")`) {
		t.Error("picking webm leaves a codec on the page that the render will override")
	}
	// ...and the one subtitle choice a container can refuse. webm carries no
	// srt and no mov_text, so "track in file" is a track that never gets
	// written and the page would be claiming one. mp4 and mkv keep the choice:
	// a browser ignores an in-band track, but every desktop player offers it.
	if !strings.Contains(sync, `p.subs.SetSelected(uint(subsIndex("none")))`) {
		t.Error("picking webm leaves \"track in file\" selected, which webm cannot carry")
	}
	if strings.Contains(sync, `"mp4"`) {
		t.Error("mp4 is being special-cased in the subtitle choice -- it carries mov_text " +
			"and the choice is a real one there")
	}
}

// An mp4 that is going on a page carries its index at the FRONT.
//
// The muxer's ordinary order is mdat then moov -- the table is only knowable
// once every frame is written -- and a browser can play nothing until it has
// that table: with byte ranges it fetches the tail first, without them it
// reads the whole file. A 90 MB download before the first frame is a <video>
// tag nobody waits for, and this app writes the tag.
//
// Only mp4: webm and mkv index as they go, and the flag is not theirs.
func TestAnMp4ForAPageIsWrittenFrontFirst(t *testing.T) {
	body := funcBody(t, "produce.go", `func \(a \*App\) produce\(`)
	i := strings.Index(body, `args = append(args, "-movflags", "+faststart+negative_cts_offsets")`)
	if i < 0 || !strings.Contains(body, `if st.Container == "mp4" {`) {
		t.Fatal("the mp4 mux no longer asks for faststart")
	}
	// the second half of that flag is about SYNC, not loading: without it the
	// muxer shifts the video track past its B-frame delay and writes an edit
	// list saying "start 66.67 ms in", and a player that ignores edit lists
	// runs the picture two frames behind the sound. Measured on a real render:
	// ffprobe -ignore_editlist 1 gave video start_time 0.066667 against audio
	// 0.000000, and 0.000000 for both once the offsets went in the sample
	// table (ctts v1) instead.
	if !strings.Contains(body, "negative_cts_offsets") {
		t.Error("the mp4 leans on its edit list for A/V sync")
	}
	// on the LAST ffmpeg of the render -- the one that writes st.OutFile --
	// and not on a clip encode, where it would cost a pass per clip for a file
	// nothing plays
	if j := strings.Index(body, "args = append(args, st.OutFile)"); j < i {
		t.Error("faststart is added after the output file, where ffmpeg reads it as an input option")
	}
	if strings.Contains(funcBody(t, "produce.go", `func \(a \*App\) encodeClip\(`), "faststart") {
		t.Error("every clip is being rewritten front-first, for files only the concat reads")
	}
}
