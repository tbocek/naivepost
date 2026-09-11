package main

// The window icon. Two things are needed: AddSearchPath so GTK finds the
// hicolor tree beside the binary, and appID as the icon name (freedesktop
// convention: icon and .desktop file are named after the application id).
//
// On Wayland the icon travels over xdg-toplevel-icon, which mutter does not
// implement (as of GNOME 50), so the shell only draws the icon of the .desktop
// file matched to the app id. Hence the desktop entry is written here, from
// the running binary's own path, so it is right by construction.

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"

	"github.com/diamondburned/gotk4/pkg/gtk/v4"
)

// appID is the application id, the icon name and the base name of both svgs.
// One string: see above.
const appID = "li.jos.naivepost"

// iconDirs is where the hicolor tree might be, best guess first: beside the
// binary (an installed or built copy: <somewhere>/gui/naivepost-gui with
// <somewhere>/gui/icons next to it), then under the naivepost root, which is
// where it is when the binary was moved but the checkout was not, and last the
// working directory, which is what `go run .` gives us.
func (a *App) iconDirs() []string {
	var dirs []string
	if exe, err := os.Executable(); err == nil {
		dirs = append(dirs, filepath.Join(filepath.Dir(exe), "icons"))
	}
	dirs = append(dirs, filepath.Join(a.root, "gui", "icons"))
	if wd, err := os.Getwd(); err == nil {
		dirs = append(dirs, filepath.Join(wd, "icons"))
	}
	return dirs
}

// setupIcons points the display's icon theme at the icons this build ships and
// gives the window their name; called from build, once the display exists.
// Best-effort, logged once when missing.
func (a *App) setupIcons() {
	theme := gtk.IconThemeGetForDisplay(gtk.BaseWidget(a.win).Display())
	if theme == nil {
		return
	}
	for _, dir := range a.iconDirs() {
		if st, err := os.Stat(dir); err == nil && st.IsDir() {
			theme.AddSearchPath(dir)
		}
	}
	switch file := a.iconFile(); {
	case theme.HasIcon(appID):
		// the default covers windows built later (the settings dialog), the
		// window's own is what the compositor is told about this one
		gtk.WindowSetDefaultIconName(appID)
		a.win.SetIconName(appID)
	case file == "":
		a.logf("icon: no %q in the icon theme -- looked in %v", appID, a.iconDirs())
		return
	default:
		// A picture the theme will not load -- a jpg, or a png dropped next to
		// the tree instead of into a size folder. The entry below points at the
		// file itself and the shell draws it from there, which under GNOME on
		// Wayland is the only half that ever reaches the screen anyway; what is
		// lost is the title bar on X11. Worth saying once, since the icon then
		// appears in one place and not the other for a reason nothing shows.
		a.logf("icon: %q is not in the icon theme -- %s is drawn from the desktop "+
			"entry instead", appID, file)
	}
	a.installDesktop()
}

// iconExts, best first, ranked by what each survives:
//
//	.svg   any size; right in a title bar and an app grid at once
//	.png   read by the icon theme, at the size declared by its hicolor folder
//	.jpg   read by the shell from the desktop entry, NOT by GTK's theme loader;
//	       leaves the title bar generic on X11 (see setupIcons)
var iconExts = []string{".svg", ".png", ".jpg", ".jpeg"}

// iconFile is the icon as a path, for whoever needs a file rather than a theme
// name -- which is the shell, reading a .desktop entry written by a checkout it
// knows nothing about. One folder at a time, best format within it: the folders
// are three guesses at the same tree, so the nearest one that has an icon at all
// is the tree this build means, whatever it keeps its icon as.
func (a *App) iconFile() string {
	for _, dir := range a.iconDirs() {
		for _, ext := range iconExts {
			if p := iconIn(dir, ext); p != "" {
				return p
			}
		}
	}
	return ""
}

// iconIn is the icon of one kind inside one icons/ folder: the theme's own
// place for it first -- scalable for a drawing, the largest pixel size for a
// picture, since the shell scales down better than it scales up -- and then the
// folder itself, for a file somebody simply dropped in beside the tree.
func iconIn(dir, ext string) string {
	if ext == ".svg" {
		if p := filepath.Join(dir, "hicolor", "scalable", "apps", appID+ext); exists(p) {
			return p
		}
	}
	best, px := "", -1
	found, _ := filepath.Glob(filepath.Join(dir, "hicolor", "*", "apps", appID+ext))
	for _, p := range found {
		if n := sizeDir(filepath.Base(filepath.Dir(filepath.Dir(p)))); n > px {
			best, px = p, n
		}
	}
	if best != "" {
		return best
	}
	if p := filepath.Join(dir, appID+ext); exists(p) {
		return p
	}
	return ""
}

// sizeDir reads the pixels out of a theme's size folder: "256x256" is 256, and
// "scalable" or "symbolic" is not a pixel size at all, which is 0 here -- last
// among sizes, still ahead of nothing.
func sizeDir(name string) int {
	n, _, ok := strings.Cut(name, "x")
	if !ok {
		return 0
	}
	px, err := strconv.Atoi(n)
	if err != nil {
		return 0
	}
	return px
}

// dataHome is XDG_DATA_HOME or the ~/.local/share the spec says to assume.
func dataHome() string {
	if d := os.Getenv("XDG_DATA_HOME"); filepath.IsAbs(d) {
		return d
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return ""
	}
	return filepath.Join(home, ".local", "share")
}

// builtOnTheFly reports whether this binary is one the go tool made to run
// once: `go run .` and `go test` both build into a cache directory that is
// deleted on exit, and an entry pointing there launches nothing. Such a run
// gets no desktop entry rather than a broken one -- and the entry from the last
// real build is left alone rather than overwritten with a path that will not
// exist in a minute.
func builtOnTheFly(exe string) bool {
	return strings.Contains(exe, "/go-build")
}

// desktopEntry is what the shell reads. Icon is an absolute path so Exec, Path
// and Icon all point into this checkout and go stale together or not at all.
// StartupWMClass matches on X11 (WM_CLASS = app id); on Wayland the file NAME
// matches, which is why it must be the id. %f and MimeType together are the
// double-click: one without the other launches an empty session.
func desktopEntry(exe, dir, icon string) string {
	if icon == "" {
		icon = appID
	}
	return fmt.Sprintf(`[Desktop Entry]
Type=Application
Name=Naivepost
Comment=Workflow console for the Naivepost pipeline
Exec=%s %%f
Path=%s
Icon=%s
Terminal=false
MimeType=%s;
Categories=AudioVideo;Video;AudioVideoEditing;
StartupNotify=true
StartupWMClass=%s
`, desktopArg(exe), dir, icon, mimeType, appID)
}

// mimeType is what a project file IS to the desktop. A type of its own rather
// than text/plain or application/json: the point of the whole exercise is that
// double-clicking one opens Naivepost, and a file whose type is "some JSON" opens
// whatever the machine opens JSON with.
const mimeType = "application/x-naivepost-project"

// mimePackage declares that type and what it looks like -- one glob, the
// project extension. The database is what turns a file name into a type; the
// desktop entry above only says which types this program handles, so without
// this the association matches nothing.
func mimePackage() string {
	return fmt.Sprintf(`<?xml version="1.0" encoding="UTF-8"?>
<mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info">
  <mime-type type="%s">
    <comment>Naivepost project</comment>
    <glob pattern="*%s"/>
  </mime-type>
</mime-info>
`, mimeType, projExt)
}

// desktopArg quotes a program path the way the spec's Exec key wants it. Paths
// with spaces in them are the normal case for a checkout under a directory
// somebody named, and an unquoted one silently becomes two arguments.
func desktopArg(s string) string {
	if !strings.ContainsAny(s, ` "'\`+"\t`$") {
		return s
	}
	r := strings.NewReplacer(`\`, `\\`, `"`, `\"`, "`", "\\`", `$`, `\$`)
	return `"` + r.Replace(s) + `"`
}

// writeDesktop puts a file where the desktop looks -- the entry, or the mime
// package beside it -- and says whether it had to. Unchanged is the common case
// -- every start after the first -- and it must not touch the file, because
// what follows a write here is a database rebuild the desktop notices.
func writeDesktop(path, entry string) (bool, error) {
	if old, err := os.ReadFile(path); err == nil && string(old) == entry {
		return false, nil
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return false, err
	}
	if err := os.WriteFile(path, []byte(entry), 0o644); err != nil {
		return false, err
	}
	return true, nil
}

// installDesktop registers this build with the desktop, which under GNOME on
// Wayland is the only way the icon is ever drawn (see the top of this file).
//
// Best-effort and quiet when there is nothing to do: it is a side effect on the
// user's home directory, so the one time it happens it says so in the log, with
// the path, because a file written into someone's home by a program they only
// meant to run should not be a surprise found later.
func (a *App) installDesktop() {
	data := dataHome()
	if data == "" {
		return
	}
	exe, err := os.Executable()
	if err != nil {
		return
	}
	if p, err := filepath.EvalSymlinks(exe); err == nil {
		exe = p
	}
	if builtOnTheFly(exe) {
		return
	}
	path := filepath.Join(data, "applications", appID+".desktop")
	wrote, err := writeDesktop(path, desktopEntry(exe, a.root, a.iconFile()))
	if err != nil {
		a.logf("icon: could not write %s: %v", path, err)
		return
	}
	if wrote {
		a.logf("icon: wrote %s -- new windows take the icon from it", path)
	}
	a.installMime(data, wrote)
}

// installMime is the other half of the double-click: what a file of
// application/x-naivepost-project is called. Written into the user's data
// directory, best-effort, only when changed. The two update- commands rebuild
// the caches the desktop reads; off the GUI thread, since
// update-mime-database walks every package.
func (a *App) installMime(data string, entryWrote bool) {
	path := filepath.Join(data, "mime", "packages", appID+".xml")
	wrote, err := writeDesktop(path, mimePackage())
	if err != nil {
		a.logf("mime: could not write %s: %v", path, err)
		return
	}
	if !wrote && !entryWrote {
		return // both files are as this build left them last time
	}
	if wrote {
		a.logf("mime: wrote %s -- a %s file opens with Naivepost", path, projExt)
	}
	go func() {
		for _, c := range [][]string{
			{"update-mime-database", filepath.Join(data, "mime")},
			{"update-desktop-database", filepath.Join(data, "applications")},
		} {
			if out, err := exec.Command(c[0], c[1:]...).CombinedOutput(); err != nil {
				a.logfIdle("mime: %s failed: %v (%s)", c[0], err,
					strings.TrimSpace(string(out)))
			}
		}
	}()
}
