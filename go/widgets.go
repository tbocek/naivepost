package main

import (
	"context"
	"strings"

	"github.com/diamondburned/gotk4/pkg/gio/v2"
	"github.com/diamondburned/gotk4/pkg/glib/v2"
	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// The handful of GTK shapes this app builds over and over, made once.

// dimLabel is a left-aligned label in the dimmed foreground: the name in front
// of a control, the reading beside one.
func dimLabel(text string) *gtk.Label {
	l := gtk.NewLabel(text)
	l.SetXAlign(0)
	l.AddCSSClass("dim-label")
	return l
}

// headLabel is a left-aligned heading.
func headLabel(text string) *gtk.Label {
	l := gtk.NewLabel(text)
	l.SetXAlign(0)
	l.AddCSSClass("heading")
	return l
}

// wrapLabel is a left-aligned label that wraps, with a CSS class.
func wrapLabel(text, class string) *gtk.Label {
	l := gtk.NewLabel(text)
	l.SetXAlign(0)
	l.SetWrap(true)
	if class != "" {
		l.AddCSSClass(class)
	}
	return l
}

// flatIcon is a borderless icon button with a tooltip.
func flatIcon(icon, tip string, click func()) *gtk.Button {
	b := gtk.NewButtonFromIconName(icon)
	b.AddCSSClass("flat")
	b.SetTooltipText(tip)
	if click != nil {
		b.ConnectClicked(click)
	}
	return b
}

// margins sets all four margins at once.
func margins(w gtk.Widgetter, top, bottom, start, end int) {
	b := gtk.BaseWidget(w)
	b.SetMarginTop(top)
	b.SetMarginBottom(bottom)
	b.SetMarginStart(start)
	b.SetMarginEnd(end)
}

// linked is a row of buttons drawn as one segmented control.
func linked(ws ...gtk.Widgetter) *gtk.Box {
	b := gtk.NewBox(gtk.OrientationHorizontal, 0)
	b.AddCSSClass("linked")
	for _, w := range ws {
		b.Append(w)
	}
	return b
}

// modal is a dialog window over the main one: a heading, an optional line of
// explanation, the body, and the buttons right-aligned under it. It is
// hand-rolled because GtkAlertDialog's variadic constructor does not survive
// the binding. Shown by the caller once the body's focus is set.
func (a *App) modal(title, detail string, width int, body gtk.Widgetter, btns ...gtk.Widgetter) *gtk.Window {
	win := gtk.NewWindow()
	win.SetTransientFor(&a.win.Window)
	win.SetModal(true)
	win.SetTitle(title)
	win.SetDefaultSize(width, -1)

	row := gtk.NewBox(gtk.OrientationHorizontal, 8)
	row.SetHAlign(gtk.AlignEnd)
	row.SetMarginTop(8)
	for _, b := range btns {
		row.Append(b)
	}
	box := gtk.NewBox(gtk.OrientationVertical, 8)
	margins(box, 16, 16, 16, 16)
	if title != "" {
		box.Append(wrapLabel(title, "heading"))
	}
	if detail != "" {
		box.Append(wrapLabel(detail, "dim-label"))
	}
	if body != nil {
		box.Append(body)
	}
	box.Append(row)
	win.SetChild(box)
	return win
}

// closeButton is the Cancel every modal has.
func closeButton(win *gtk.Window, label string) *gtk.Button {
	b := gtk.NewButtonWithLabel(label)
	b.ConnectClicked(func() { win.Close() })
	return b
}

// extFilter is a file filter by extension; exts may carry a leading dot.
func extFilter(name string, exts ...string) *gtk.FileFilter {
	f := gtk.NewFileFilter()
	f.SetName(name)
	for _, e := range exts {
		f.AddSuffix(strings.TrimPrefix(e, "."))
	}
	return f
}

func (a *App) fileDialog(title, dir string, filt *gtk.FileFilter) *gtk.FileDialog {
	d := gtk.NewFileDialog()
	d.SetTitle(title)
	if dir != "" && exists(dir) {
		d.SetInitialFolder(gio.NewFileForPath(dir))
	}
	if filt != nil {
		fs := gio.NewListStore(gtk.GTypeFileFilter)
		fs.Append(filt.Object)
		d.SetFilters(fs)
	}
	return d
}

// pickFile asks for one file; ok is not called on dismiss.
func (a *App) pickFile(title, dir string, filt *gtk.FileFilter, ok func(string)) {
	d := a.fileDialog(title, dir, filt)
	d.Open(context.Background(), &a.win.Window, func(res gio.AsyncResulter) {
		if f, err := d.OpenFinish(res); err == nil && f != nil {
			ok(f.Path())
		}
	})
}

// pickFiles asks for any number of files.
func (a *App) pickFiles(title, dir string, filt *gtk.FileFilter, ok func([]string)) {
	d := a.fileDialog(title, dir, filt)
	d.OpenMultiple(context.Background(), &a.win.Window, func(res gio.AsyncResulter) {
		lm, err := d.OpenMultipleFinish(res)
		if err != nil || lm == nil {
			return
		}
		var paths []string
		for i := uint(0); i < lm.NItems(); i++ {
			if obj := lm.Item(i); obj != nil {
				paths = append(paths, (&gio.File{Object: obj}).Path())
			}
		}
		ok(paths)
	})
}

// pickFolder asks for a directory.
func (a *App) pickFolder(title, dir string, ok func(string)) {
	d := a.fileDialog(title, dir, nil)
	d.SelectFolder(context.Background(), &a.win.Window, func(res gio.AsyncResulter) {
		if f, err := d.SelectFolderFinish(res); err == nil && f != nil {
			ok(f.Path())
		}
	})
}

// saveAs asks where to write a file, starting at dir/name.
func (a *App) saveAs(title, dir, name string, filt *gtk.FileFilter, ok func(string)) {
	d := a.fileDialog(title, dir, filt)
	d.SetInitialName(name)
	d.Save(context.Background(), &a.win.Window, func(res gio.AsyncResulter) {
		if f, err := d.SaveFinish(res); err == nil && f != nil {
			ok(f.Path())
		}
	})
}

// editWait is how long a burst of typing has to stop before the work behind it
// runs. Long enough that a sentence is one run rather than forty, short enough
// that it has happened by the time you have looked up from the keyboard.
const editWait = 400

// debounce collects a burst of calls into one run of the work (typing). The
// wait is restarted by counting generations rather than cancelling the timer:
// a glib source must be removed on its own thread, and a stale timer that
// wakes, finds a newer generation and returns cannot go wrong.
type debounce struct {
	ms  uint // 0 is editWait, so a plain field needs no construction
	gen int
	// what is waiting to run, kept so it can be run early instead of waited
	// out (flush). nil when nothing is owed.
	owed func()
	// glib's timer, replaced in tests by a clock the test winds itself
	arm func(uint, func() bool)
}

// call runs f once, a beat after the LAST call of the burst it belongs to.
func (d *debounce) call(f func()) {
	d.gen++
	d.owed = f
	want := d.gen
	ms, arm := d.ms, d.arm
	if ms == 0 {
		ms = editWait
	}
	if arm == nil {
		arm = func(ms uint, fn func() bool) { glib.TimeoutAdd(ms, fn) }
	}
	arm(ms, func() bool {
		if d.gen == want { // no newer keystroke came in behind this one
			d.owed = nil
			f()
		}
		return false // one shot; the next call arms the next one
	})
}

// flush runs what is owed right now and leaves nothing armed behind it, for the
// moments where waiting is not allowed: leaving the page, closing the window,
// or starting the run that reads the file being written. Nothing owed is a
// no-op, so it is safe to put it wherever it belongs rather than only where
// something is known to be pending.
func (d *debounce) flush() {
	f := d.owed
	if f == nil {
		return
	}
	d.gen++ // every armed timer is stale from here
	d.owed = nil
	f()
}

// pending is whether work is owed. For the callers that have to know whether
// what is on screen has reached the disk yet.
func (d *debounce) pending() bool { return d.owed != nil }
