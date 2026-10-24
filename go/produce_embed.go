package main

// The <video> tag, written beside the video.
//
// A browser plays none of the subtitle tracks muxed into the mp4 -- in-band
// text is not something Firefox or Chrome offer at all -- and <track> parses
// WebVTT alone, so the .srt that VLC reads is the wrong file for a page. What
// a page needs is the .vtt files the render writes beside the video and a tag
// pointing at them, and getting that tag right is five minutes of somebody's
// afternoon, every time. So it is written out with the video: open it to check
// the result, or paste the four lines into the page they belong in.

import (
	"fmt"
	"html"
	"image/jpeg"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// embedFile is where the tag goes: beside the video, named after it.
func embedFile(out string) string {
	return strings.TrimSuffix(out, filepath.Ext(out)) + ".html"
}

// posterFile is the still a <video> shows before it is played: the thumbnail,
// as a jpeg beside the video. A .jpg and not the thumbnail.png it comes from,
// because a poster is one frame of decoration on a page that already carries
// the video, and 200 kB of PNG is not worth it.
func posterFile(out string) string {
	return strings.TrimSuffix(out, filepath.Ext(out)) + ".jpg"
}

// webUnplayable says why a browser will not play this file, or "". The tag is
// written for every render, and a page is the one place where the container
// and codec chosen for a desktop player quietly stop working -- it is cheaper
// to read it in the log than to wonder at a black rectangle.
func webUnplayable(out, codec string) string {
	switch {
	case strings.EqualFold(filepath.Ext(out), ".mkv"):
		return "no browser plays Matroska — render to mp4 or webm for a page"
	case codec == "h265":
		return "Firefox plays no h265 at all, and the others only where the machine " +
			"decodes it in hardware — h264 is the one that plays everywhere"
	}
	return ""
}

// writeEmbed writes the poster and the tag. Called when the whole run is over,
// not at the end of the render: the thumbnail is drawn by the other half of
// that run (produceRun), and a tag written before it landed would have no
// poster in it. Never an error worth failing a render for -- the video and the
// subtitles are on disk either way.
func (a *App) writeEmbed(out, codec string) {
	if out == "" || !exists(out) {
		return
	}
	poster := ""
	if err := a.writePoster(out); err != nil {
		a.logfIdle("    produce: no poster for the <video> tag (%v)", err)
	} else {
		poster = filepath.Base(posterFile(out))
	}
	tracks := a.embedTracks(out)
	body := embedHTML(filepath.Base(out), poster, tracks)
	if err := os.WriteFile(embedFile(out), []byte(body), 0o644); err != nil {
		a.logfIdle("    produce: could not write %s (%v)", filepath.Base(embedFile(out)), err)
		return
	}
	a.logfIdle(">>> the <video> tag for it: %s", embedFile(out))
	if why := webUnplayable(out, codec); why != "" {
		a.logfIdle("    (this video is not one a page can play: %s)", why)
	}
	if len(tracks) > 0 {
		// the one that costs an afternoon: a <track> is fetched under the
		// same-origin rule, and file:// counts as a different origin for
		// every file. Opened from disk the video plays and the subtitles are
		// simply absent, with nothing on screen to say why.
		a.logfIdle("    (subtitles need it served over http -- opened as a file:// page " +
			"the browser refuses every <track>)")
	}
}

// writePoster is the thumbnail as a jpeg beside the video.
func (a *App) writePoster(out string) error {
	src := a.thumbFile()
	if !exists(src) {
		return fmt.Errorf("no thumbnail has been drawn yet")
	}
	img, err := pubDecode(src) // the thumbnail is a PNG; pubDecode takes any of them
	if err != nil {
		return err
	}
	f, err := os.Create(posterFile(out))
	if err != nil {
		return err
	}
	defer f.Close()
	return jpeg.Encode(f, img, &jpeg.Options{Quality: 90})
}

// embedTracks is the .vtt files that are actually beside the video, in the
// order a player should offer them: the video's own language first, the
// translations after it by name.
//
// Read off the DISK rather than taken from the render, because this runs after
// a press that may have skipped the encode entirely -- the subtitles from the
// render before are the subtitles this video has.
func (a *App) embedTracks(out string) []subTrack {
	stem := strings.TrimSuffix(out, filepath.Ext(out))
	own := a.asrLanguage()
	var rest []subTrack
	var first []subTrack
	for _, p := range subSideFiles(out) {
		if !strings.HasSuffix(p, ".vtt") || !exists(p) {
			continue
		}
		// "<stem>.vtt" is the video's own language; "<stem>.de.vtt" names its
		// own between two dots
		code := own
		if tail := strings.TrimSuffix(strings.TrimPrefix(p, stem), ".vtt"); tail != "" {
			code = strings.TrimPrefix(tail, ".")
		}
		t := subTrack{code: code, name: strings.ToUpper(code), path: p}
		if _, n, ok := subLangOf(code); ok {
			t.name = n
		}
		if p == stem+".vtt" {
			first = append(first, t)
			continue
		}
		rest = append(rest, t)
	}
	sort.Slice(rest, func(i, j int) bool { return rest[i].name < rest[j].name })
	return append(first, rest...)
}

// embedHTML is the tag itself. Bare -- no document around it, no styling --
// because it is written to be pasted into a page that has both, and it still
// opens in a browser on its own.
func embedHTML(video, poster string, tracks []subTrack) string {
	var b strings.Builder
	b.WriteString("<video")
	if poster != "" {
		fmt.Fprintf(&b, " poster=%q", html.EscapeString(poster))
	}
	fmt.Fprintf(&b, " controls src=%q preload=\"none\">\n", html.EscapeString(video))
	for i, t := range tracks {
		// default on the first alone: two default tracks is two sets of
		// subtitles drawn over each other in some players
		def := ""
		if i == 0 {
			def = " default"
		}
		fmt.Fprintf(&b, " <track label=%q kind=\"subtitles\" srclang=%q src=%q%s />\n",
			html.EscapeString(t.name), html.EscapeString(t.code),
			html.EscapeString(filepath.Base(t.path)), def)
	}
	b.WriteString("</video>\n")
	return b.String()
}
