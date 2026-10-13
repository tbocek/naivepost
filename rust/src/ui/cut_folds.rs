//! F2.11 what the session filmed, and its folds drawn as buttons — the page half of the flow.
//!
//! Split out of `window.rs` unchanged: the rules live in [`crate::cut_fold`] and [`crate::timeline`],
//! and what is here is only the state a press leaves behind and the widgets that show it — the tape
//! list the badges are laid out from, the numbered −/+ badges and the two ✕. No decision of its own.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut::{self, Cut};
use crate::cut_fold;
use crate::cut_screen;
use crate::ui::cut_cameras::refresh_camera_rows;
use crate::ui::window::{
    self as window, find_status, find_widget_by_name, newest_review_cut, record_edit,
    seed_review_cut,
};

// --- F2.11: what the SESSION filmed ---------------------------------------------------------------------

/// The recordings this window lays its tape out from. NOT derived from the kept segments: a dropped
/// stretch only exists inside a run longer than the union of the clips kept inside it, and runs built
/// from those clips are exactly that union — so deriving them there would leave no head, no tail and no
/// hole, and a − badge could never be drawn (measured: `dropped_gaps(filmed_runs(
/// kept_footage_recordings(cut 10–30 + 40–60)), …)` answers `[]`).
///
/// Seeded at window build with what the loaded cut implies; [`set_session_recordings`] replaces it with
/// the takes' own spans once the rescan/`sources` round publishes them, which is this seam's future
/// input. Row COLOURING keeps reading `kept_footage_recordings` (F2.10's rows are the cameras the cut
/// actually shows), so only the fold half and the row-emptiness half ask here.
pub fn page_recordings() -> Vec<crate::timeline::Recording> {
    SESSION_RECORDINGS.with(|slots| slots.borrow().last().map(|slot| slot.borrow().clone()).unwrap_or_default())
}

/// Replace this window's session recordings with the ones the page should lay out. Newest slot only, so
/// an older window keeps its own view like every other per-window slot here.
///
/// This is also what DECLARES a session: a list written here was named by someone who knows the takes (an
/// open project's rescan, a flow laying out a hole it wants folded), so its heads, tails and holes are the
/// point of it and a later `seed_review_cut` must not trade them for what a cut implies.
pub fn set_session_recordings(recordings: &[crate::timeline::Recording]) {
    let held = SESSION_RECORDINGS.with(|slots| slots.borrow().last().cloned());
    match held {
        Some(slot) => *slot.borrow_mut() = recordings.to_vec(),
        None => SESSION_RECORDINGS.with(|slots| {
            slots.borrow_mut().push(Rc::new(RefCell::new(recordings.to_vec())))
        }),
    }
    SESSION_DECLARED.with(|flags| match flags.borrow().last() {
        Some(flag) => flag.set(true),
        None => flags.borrow_mut().push(std::cell::Cell::new(true)),
    });
}

pub(crate) fn push_session_recordings(recordings: Vec<crate::timeline::Recording>) {
    SESSION_RECORDINGS.with(|slots| {
        slots.borrow_mut().push(Rc::new(RefCell::new(recordings)));
    });
    // A window's first list is DERIVED from the cut it opened — a fallback, not a declaration.
    SESSION_DECLARED.with(|flags| flags.borrow_mut().push(std::cell::Cell::new(false)));
}

/// Has anyone DECLARED this window's session, or is the page still running on the spans its opened cut
/// implied? Only the second answer may be replaced when a new cut is seeded.
pub(crate) fn session_declared() -> bool {
    // Newest slot only: an older window's declaration is that window's business, like the list beside it.
    SESSION_DECLARED
        .with(|flags| flags.borrow().last().map(|flag| flag.get()))
        .unwrap_or(false)
}

thread_local! {
    /// F2.11: one recording list per window, newest last like [`REVIEW_CUTS`]. Cloned OUT of the borrow
    /// before any write (see `remember_fold_gaps`) so a refresh re-entering here cannot double-borrow.
    static SESSION_RECORDINGS: std::cell::RefCell<Vec<Rc<RefCell<Vec<crate::timeline::Recording>>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// One flag per [`SESSION_RECORDINGS`] slot: was that list DECLARED (someone named the real takes) or is
    /// it the derived fallback a window build leaves behind? Pushed in lockstep with the list above.
    static SESSION_DECLARED: std::cell::RefCell<Vec<std::cell::Cell<bool>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

// --- F2.11: folds, drawn as buttons -------------------------------------------------------------------

/// The − glyph (fold a dropped stretch away) spelled once, so a badge and its tooltip cannot drift.
const FOLD_GLYPH: &str = "\u{2212}";
/// The + glyph (open the seam again).
const UNFOLD_GLYPH: &str = "+";
/// The ✕ that takes an emptied row or a cut lane off the page (§A's gutter).
pub(crate) const CROSS_GLYPH: &str = "\u{2715}";

/// This window's holder for the whole-cut fold badge (`fold-all-button`), by its stable name.
pub fn fold_badges_box(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    // Scoped to THIS window's own content rather than searched from the window down. `find_widget_by_name`
    // walks whatever tree it is handed and returns the FIRST name match, and a closed window's widgets are
    // not destroyed while another window lives — so searching from the window can return a box belonging
    // to a previous check's window, whose children then collide with this one's appends
    // (`gtk_box_append: assertion 'gtk_widget_get_parent (child) == NULL' failed`). Walking from
    // `window.content()` cannot reach a sibling window's tree at all.
    let content = window.content()?;
    find_widget_by_name(&content, "fold-all-holder")?
        .downcast()
        .ok()
}

/// The gap badge `index` folds, newest window first — what `press_fold_gap` forwards to the rule with,
/// and what a test reads back to see WHICH gap a numbered button means.
pub fn folded_gap_at(window: &adw::ApplicationWindow, index: usize) -> Option<(f64, f64)> {
    let _ = window;
    FOLD_TABLES.with(|slots| {
        slots
            .borrow()
            .last()
            .and_then(|table| table.borrow().get(index).copied())
    })
}

/// The gaps this window currently wears a badge over, in tape order.
pub fn fold_gaps(window: &adw::ApplicationWindow) -> Vec<(f64, f64)> {
    let _ = window;
    FOLD_TABLES.with(|slots| slots.borrow().last().map(|t| t.borrow().clone()).unwrap_or_default())
}

/// The band's own badge holder, scoped to THIS window exactly as [`fold_badges_box`] is (a closed window's
/// tree lives on, and a name search from the window can land in it). It is the empty `fold-badges` box laid
/// OVER the painted band inside `track-layers`, so a per-gap badge sits at its x AMONG the bars §B puts it
/// beside; the gutter's whole-cut `fold-all-button` stays in the other holder (`fold_badges_box`).
pub fn fold_overlay(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    let content = window.content()?;
    find_widget_by_name(&content, "fold-badges")?
        .downcast()
        .ok()
}

/// The ONE span the fold badges are placed on: the filmed runs cut at this cut's folds, at the zoom the band
/// paints at (`cut_view::pps`, the number `scene_from` hands the painter), after the gutter. Badge x and
/// painted x cannot drift because both ask this same span.
pub fn band_fold_span(cut_: &Cut) -> crate::timeline::Span {
    crate::timeline::Span::new(
        crate::timeline::cells(
            &crate::timeline::filmed_runs(&page_recordings()),
            &cut_.folds,
        ),
        // The zoom the BADGE HOLDER is drawn at: `cut_view::pps()` is the real band's, which lives in a
        // scrolled viewport whose width nothing can measure before it is mapped (a snapshot asks for 4096 px
        // and gets a wider fit back), while the badge row shares the strip's fixed 480 px geometry.
        // `strip_pps()` is that strip's own door — the same one F2.8's press geometry uses — so a badge's x
        // means the same second as the bar it sits over, at whatever zoom each surface settled on.
        window::strip_pps(),
        cut_screen::GUTTER_PX,
    )
}

/// Redraw the fold badges from this window's own cut: one `fold-button-<i>` per gap `cut_fold::badges`
/// says may wear one, labelled `−` while the gap is open and `+` once folded. Nothing is decided here —
/// the list, the minimum width (`cut_fold::FOLD_MIN_PX`) and the placement inside the neighbouring bar
/// (`cut_fold::BADGE_INSIDE_PX`) all come from `cut_fold::badges`, and each press runs
/// `cut_fold::toggle_fold` and prints what comes back.
///
/// Like `refresh_camera_rows`, the badges are rebuilt rather than patched: the number of gaps changes with
/// every cut edit, so a reused child would keep a stale label under a stable name.
pub fn refresh_fold_badges(window: &adw::ApplicationWindow) {
    // Two holders matter: the gutter box, which keeps ONLY the whole-cut `fold-all-button` (§B puts that one
    // in the gutter), and the band's overlay, which carries the per-gap badges over their own bars.
    let holder = fold_badges_box(window);
    let Some(band) = fold_overlay(window) else {
        return;
    };
    let cut_ = newest_review_cut();
    // The SESSION's tape, not the kept segments — see [`page_recordings`] for why the derived list can
    // never contain a dropped stretch.
    let recordings = page_recordings();
    let runs = crate::timeline::filmed_runs(&recordings);
    let span = band_fold_span(&cut_);
    let shown = cut_fold::badges(&cut_.folds, &runs, &cut_.segs, &span);
    // Remember WHICH gap each numbered button stands for, so the press needs no second arithmetic.
    remember_fold_gaps(shown.iter().map(|badge| badge.gap).collect());

    // Take away every badge this pass did not re-use: a gap that moved off, shrank under `FOLD_MIN_PX` or
    // was removed with its clip must not keep a live button at its old x. Searched in BOTH holders, so a
    // leftover in the gutter box from before this move cannot shadow the real one by name.
    let mut stale: Vec<gtk::Widget> = Vec::new();
    if let Some(gutter) = holder.as_ref() {
        stale.extend(
            gutter
                .observe_children()
                .iter::<glib::Object>()
                .flatten()
                .filter_map(|child| child.downcast::<gtk::Widget>().ok())
                .filter(|child| child.widget_name().starts_with("fold-button-")),
        );
    }
    stale.extend(
        band.observe_children()
            .iter::<glib::Object>()
            .flatten()
            .filter_map(|child| child.downcast::<gtk::Widget>().ok())
            .filter(|child| child.widget_name().starts_with("fold-button-")),
    );
    for old in stale {
        if let Some(parent) = old.parent().and_then(|node| node.downcast::<gtk::Box>().ok()) {
            parent.remove(&old);
        }
    }
    for (index, badge) in shown.iter().enumerate() {
        let button = gtk::Button::new();
        button.set_widget_name(&format!("fold-button-{}", index + 1));
        button.set_label(match badge.folded {
            true => UNFOLD_GLYPH,
            false => FOLD_GLYPH,
        });
        button.set_tooltip_text(Some(&format!(
            "{} {}",
            if badge.folded { "unfold" } else { "fold" },
            cut_fold::fold_status(badge.gap).replace("folded ", "").as_str()
        )));
        // Over the bar, at the x the rule answered: a child of a top-left-pinned row, pushed right by its
        // own margin, so the badge's left edge IS `badge.x` in band pixels.
        button.set_halign(gtk::Align::Start);
        button.set_valign(gtk::Align::Start);
        button.set_margin_start(badge.x.round() as i32);
        let win = window.clone();
        button.connect_clicked(move |_| {
            press_fold_gap(&win, index);
        });
        band.append(&button);
    }
}

thread_local! {
    /// F2.11 S1: the gaps this window's numbered badges stand for, newest window last like
    /// [`REVIEW_CUTS`]. A button carries its INDEX, not a copy of the gap, so the rule is asked about
    /// the same stretch the drawing showed — two copies of a gap could otherwise disagree after an edit.
    #[expect(clippy::type_complexity, reason = "one badge table per window, newest last")]
    static FOLD_TABLES: std::cell::RefCell<Vec<Rc<RefCell<Vec<(f64, f64)>>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

fn remember_fold_gaps(gaps: Vec<(f64, f64)>) {
    FOLD_TABLES.with(|slots| {
        // The slot is looked up and cloned out of the borrow BEFORE writing into it. Held across the
        // write, the outer `borrow()` collides with a nested refresh (a press inside a draw re-enters
        // here) and panics on the double borrow.
        let held = slots.borrow().last().cloned();
        match held {
            Some(table) => *table.borrow_mut() = gaps,
            None => slots.borrow_mut().push(Rc::new(RefCell::new(gaps))),
        }
    });
}

/// This window's cut as the page sees it, folds included — what a widget test reads back instead of
/// reaching into a thread-local. Same newest-slot rule as [`review_cut_segs`].
pub fn review_fold_rows(window: &adw::ApplicationWindow) -> cut::Cut {
    let _ = window;
    newest_review_cut()
}

/// F2.11 S1 (`\u{2212} in such a gap folds it to a seam \u00b7 + on the seam unfolds`): the seam a
/// badge forwards to. Saves the cut afterwards, because a fold is a view but IS kept in `cut.json`.
#[expect(clippy::question_mark, reason = "no badge at that index means nothing to fold, and no status line")]
pub fn press_fold_gap(window: &adw::ApplicationWindow, index: usize) -> Option<String> {
    let Some(gap) = folded_gap_at(window, index) else {
        return None;
    };
    let mut cut_ = newest_review_cut();
    let said = cut_fold::toggle_fold(&mut cut_, gap);
    seed_review_cut(window, &cut_);
    save_folds(&cut_);
    refresh_fold_badges(window);
    if let Some(status) = find_status(window.upcast_ref()) {
        status.set_text(&said);
    }
    Some(said)
}

/// F2.1 S3: ▶ ran into a folded seam, so it opens — the badge's fold rule through the playback path.
///
/// Not [`press_fold_gap`]: that one takes its gap from the badge layout, which is built only over gaps the
/// CUT drops, and under ▶ every second plays, cuts and all, so the line can run into a fold the cut keeps.
/// A fold opened by playback spends no Undo step (F2.11: a fold is saved and never undone), so this writes
/// through `seed_review_cut` + `save_folds`, exactly as a badge press persists it, minus the history entry.
pub fn unfold_fold_for_playback(window: &adw::ApplicationWindow, index: usize) -> Option<String> {
    let mut cut_ = newest_review_cut();
    let [start, _] = *cut_.folds.get(index)?;
    cut_.folds.remove(index);
    seed_review_cut(window, &cut_);
    save_folds(&cut_);
    refresh_fold_badges(window);
    // §B's sentence for THIS event ("unfolded m:ss — ▶ ran into it"), which is preview's wording, not the
    // badge's "unfolded X – Y (N s)": the person was watching, not folding.
    Some(crate::preview::unfolded_status(start))
}

/// F2.11 S1 (`the gutter badge folds/unfolds all`): `fold-all-button`'s seam. `cut_fold::fold_all`
/// answers which way it went by reading the cut's own list, so this passes the gaps and adds nothing.
pub fn press_fold_all(window: &adw::ApplicationWindow) -> String {
    let cut_now = newest_review_cut();
    let recordings = page_recordings();
    let runs = crate::timeline::filmed_runs(&recordings);
    let gaps = cut_fold::dropped_gaps(&runs, &cut_now.segs);
    let segs = cut_now.segs.clone();
    let mut cut_ = cut_now;
    let said = cut_fold::fold_all(&mut cut_, &gaps, &runs, &segs);
    seed_review_cut(window, &cut_);
    save_folds(&cut_);
    refresh_fold_badges(window);
    if let Some(status) = find_status(window.upcast_ref()) {
        status.set_text(&said);
    }
    said
}

/// Write the folds back where they were read from. `cut::save` writes the whole cut, and `folds` is a
/// field of it, so a toggle lands in `cut/cut.json` on the same path the page loaded from. A project
/// with no tree (nothing saved yet) keeps its folds in memory only — there is no file to write into.
pub(crate) fn save_folds(cut_: &Cut) {
    // §7's one writer: a fold toggle lands in `cut/cut.json` through `cut_save`, the same door every edit uses.
    let _ = crate::cut_save::write_cut(cut_);
}

/// F2.11 S2 (`An emptied bottom row stays until its \u{2715}`): the row cross's seam. Whether the row
/// may go is `cut_fold::kill_row`'s answer, including "not while it holds footage" and "never the last
/// row"; this only forwards and prints.
pub fn press_row_cross(window: &adw::ApplicationWindow, row: usize) -> Option<String> {
    let cut_ = newest_review_cut();
    // The SAME spans the row list colours with, so "is this row empty?" is asked of one answer rather than
    // two — `refresh_camera_rows` drops a lane's span once its ✕ took the lane, and asking here against the
    // raw session list would still count that phantom row as occupied and refuse a cross the page just drew
    // live. One lane too short or too long flips which row counts as the emptied bottom one.
    let recordings = crate::ui::cut_cameras::row_spans(&cut_);
    let placed = crate::timeline::rows_for(&recordings, &cut_);
    let mut cut_ = cut_;
    let said = cut_fold::kill_row(&mut cut_, row, &recordings, &placed)?;
    record_edit(window, &cut_);
    save_folds(&cut_);
    refresh_camera_rows(window);
    refresh_fold_badges(window);
    if let Some(status) = find_status(window.upcast_ref()) {
        status.set_text(&said);
    }
    Some(said)
}

/// F2.11 S2 (`a cut lane's \u{2715} removes the lane, its pins, shift, pictures and sound`): the lane
/// cross's seam. All four things go inside `cut_fold::remove_lane`; this forwards and repaints.
pub fn press_lane_cross(window: &adw::ApplicationWindow, name: &str) -> String {
    let mut cut_ = newest_review_cut();
    let said = cut_fold::remove_lane(&mut cut_, name);
    record_edit(window, &cut_);
    save_folds(&cut_);
    refresh_camera_rows(window);
    refresh_fold_badges(window);
    if let Some(status) = find_status(window.upcast_ref()) {
        status.set_text(&said);
    }
    said
}
