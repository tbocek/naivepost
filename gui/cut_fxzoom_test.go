package main

// The live-zoom layer's transform. GskTransform is refcounted and every
// gsk_transform_* call takes over the reference it was chained onto, while the
// Go value it was chained onto keeps a finalizer that will unref it again --
// so the obvious chained spelling hands out frees for memory GTK still holds,
// and does it once per tick of a playing preview. These pin the shape that
// balances, because nothing at run time complains until the heap is already
// gone.

import (
	"math"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestZoomTransformIsTranslateThenScale(t *testing.T) {
	const s, tx, ty = 2.5, -120.0, 37.5
	xx, yx, xy, yy, dx, dy := zoomTransform(s, tx, ty).To2D()
	for _, c := range []struct {
		name string
		got  float32
		want float64
	}{
		{"xx", xx, s}, {"yy", yy, s},
		{"yx", yx, 0}, {"xy", xy, 0},
		{"dx", dx, tx}, {"dy", dy, ty},
	} {
		if math.Abs(float64(c.got)-c.want) > 1e-4 {
			t.Errorf("%s is %g, want %g", c.name, c.got, c.want)
		}
	}
	// what the chain meant, spelled out: a point of the picture lands where
	// scaling about the origin and then shifting by (tx,ty) puts it
	px, py := 100.0, 40.0
	wantX, wantY := s*px+tx, s*py+ty
	gotX := float64(xx)*px + float64(xy)*py + float64(dx)
	gotY := float64(yx)*px + float64(yy)*py + float64(dy)
	if math.Abs(gotX-wantX) > 1e-3 || math.Abs(gotY-wantY) > 1e-3 {
		t.Fatalf("(%g,%g) maps to (%g,%g), want (%g,%g)", px, py, gotX, gotY, wantX, wantY)
	}
}

// A transform built from nothing consumes nothing, so the one value that comes
// back is the only one owing an unref -- and SetChildTransform takes it
// transfer-none. Chaining instead would put the double free back.
func TestTheZoomTransformIsNotChained(t *testing.T) {
	b, err := os.ReadFile("cut_fxview.go")
	if err != nil {
		t.Fatal(err)
	}
	src := string(b)
	fn := src[strings.Index(src, "func zoomTransform("):]
	fn = fn[:strings.Index(fn, "\n}")]
	if !strings.Contains(fn, "var identity *gsk.Transform") {
		t.Fatal("zoomTransform no longer starts from the identity (a nil transform)")
	}
	for _, consuming := range []string{".Translate(", ".Scale(", ".Rotate(", ".Skew(", ".Invert("} {
		if strings.Contains(fn, consuming) {
			t.Fatalf("zoomTransform chains %s, which consumes the transform it is called on "+
				"while Go still holds a finalizer that unrefs it", consuming)
		}
	}
	// and nowhere else in the package either
	files, err := filepath.Glob("*.go")
	if err != nil {
		t.Fatal(err)
	}
	for _, f := range files {
		if f == "cut_fxzoom_test.go" {
			continue // this file names the bad spelling in order to forbid it
		}
		bb, err := os.ReadFile(f)
		if err != nil {
			t.Fatal(err)
		}
		for i, line := range strings.Split(string(bb), "\n") {
			if strings.HasPrefix(strings.TrimSpace(line), "//") {
				continue // a comment may quote it; zoomTransform's does
			}
			if strings.Contains(line, "gsk.NewTransform()") {
				t.Errorf("%s:%d builds a GskTransform by chaining; see zoomTransform", f, i+1)
			}
		}
	}
}
