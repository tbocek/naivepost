package main

// What ffprobe knows about a file, asked once.
//
// Every probe here used to be its own ffprobe process, and the page that opens
// the Cut step asks four things about every recording: how long it runs, how
// big its picture is, how fast it plays, how many tracks it has. A session of
// thirty-one takes is a hundred processes started one after another on the GUI
// thread, and the thread that starts them is the thread that draws -- so the
// window stops answering and the hang watch writes a stack (main.go,
// startHangWatch). Measured: 45 ms a probe, 93 probes, four and a bit seconds
// against a three-second watchdog.
//
// So: one call per file, answering everything, and the answer kept under the
// file's identity -- its path, size and modification time (fileMark). A
// recording still being written to disk changes size and is asked again; one
// that has not moved is never asked twice, and walking back to the Cut tab
// costs nothing at all.

import (
	"encoding/json"
	"fmt"
	"os/exec"
	"strconv"
	"strings"
	"sync"
)

// probeInfo is one file as ffprobe describes it. Zero values are "not known":
// a file with no video stream has no size and no frame rate, which is a real
// answer about a microphone recording rather than a failure.
type probeInfo struct {
	dur    float64
	w, h   int
	fps    float64
	tracks []audTrack
	// whether ffprobe answered at all. Not the same as an empty answer: a
	// silent capture HAS no audio stream and its no tracks is the truth,
	// where a file ffprobe could not open says nothing about anything and
	// the callers fall back to what they always did.
	ok bool
}

var (
	probeMu    sync.Mutex
	probeCache = map[string]probeInfo{}
)

// ffprobeInfo is everything about one file, from one process, remembered.
func ffprobeInfo(path string) probeInfo {
	key := path + " " + fileMark(path)
	probeMu.Lock()
	got, ok := probeCache[key]
	probeMu.Unlock()
	if ok {
		return got
	}
	info := probeFile(path)
	probeMu.Lock()
	probeCache[key] = info
	probeMu.Unlock()
	return info
}

// probeFile is the process itself. JSON rather than csv: this asks about the
// format and about every stream at once, and the shape of a csv answer depends
// on which of them a file happens to have.
func probeFile(path string) probeInfo {
	out, err := exec.Command(ffTool("ffprobe"), "-v", "error",
		"-show_entries", "format=duration:stream=codec_type,width,height,avg_frame_rate,channels:stream_tags=title",
		"-of", "json", path).Output()
	if err != nil {
		return probeInfo{}
	}
	var got struct {
		Format struct {
			Duration string `json:"duration"`
		} `json:"format"`
		Streams []struct {
			CodecType string `json:"codec_type"`
			Width     int    `json:"width"`
			Height    int    `json:"height"`
			AvgRate   string `json:"avg_frame_rate"`
			Channels  int    `json:"channels"`
			Tags      struct {
				Title string `json:"title"`
			} `json:"tags"`
		} `json:"streams"`
	}
	if json.Unmarshal(out, &got) != nil {
		return probeInfo{}
	}
	info := probeInfo{ok: true}
	fmt.Sscanf(strings.TrimSpace(got.Format.Duration), "%f", &info.dur)
	for _, s := range got.Streams {
		switch s.CodecType {
		case "video":
			if info.w == 0 && s.Width > 0 && s.Height > 0 {
				info.w, info.h = s.Width, s.Height
				info.fps = rateOf(s.AvgRate)
			}
		case "audio":
			n := s.Channels
			if n < 1 {
				n = 1 // a stream ffprobe would not count is still a stream
			}
			info.tracks = append(info.tracks, audTrack{chans: min(2, n), title: strings.TrimSpace(s.Tags.Title)})
		}
	}
	return info
}

// rateOf reads ffprobe's "30000/1001" into frames a second, or 0 for an answer
// no picture has: 0/0 for a stream that never moved, a rate no camera shoots.
func rateOf(s string) float64 {
	num, den, ok := strings.Cut(strings.TrimSpace(s), "/")
	if !ok {
		return 0
	}
	n, err1 := strconv.ParseFloat(num, 64)
	d, err2 := strconv.ParseFloat(den, 64)
	if err1 != nil || err2 != nil || d <= 0 {
		return 0
	}
	if r := n / d; r >= 1 && r <= 240 {
		return r
	}
	return 0
}
