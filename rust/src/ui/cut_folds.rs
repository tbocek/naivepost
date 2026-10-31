//! F2.11 what the session filmed, and its folds drawn as buttons — the page half of the flow.

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut::Cut;
use crate::cut_fold;
use crate::cut_screen;
use crate::ui::cut_cameras::refresh_camera_rows;
use crate::ui::window::{
    self as window, find_status, find_widget_by_name, newest_review_cut, record_edit,
    seed_review_cut,
};

// --- F2.11: what the SESSION filmed ---------------------------------------------------------------------

pub fn page_recordings() -> Vec<crate::timeline::Recording> {
    SESSION_RECORDINGS.with(|slots| {
        slots
            .borrow()
            .last()
            .map(|slot| slot.borrow().clone())
            .unwrap_or_default()
    })
}

pub fn set_session_recordings(recordings: &[crate::timeline::Recording]) {
    let held = SESSION_RECORDINGS.with(|slots| slots.borrow().last().cloned());
    match held {
        Some(slot) => *slot.borrow_mut() = recordings.to_vec(),
        None => SESSION_RECORDINGS.with(|slots| {
            slots
                .borrow_mut()
                .push(Rc::new(RefCell::new(recordings.to_vec())))
        }),
    }
    // The borrow of the list ends before the push: matched on directly, `flags.borrow()` lived
    // through the arms and the `None` arm's `borrow_mut` panicked "already borrowed" whenever no
    // window had declared a slot yet.
    let declared = SESSION_DECLARED.with(|flags| {
        flags.borrow().last().map(|flag| {
            flag.set(true);
        })
    });
    if declared.is_none() {
        SESSION_DECLARED.with(|flags| flags.borrow_mut().push(std::cell::Cell::new(true)));
    }
    // The Narrate page reads these recordings too: its preview's still (the one painted before they
    // arrived was looked up in the stand-ins the cut implied, which have no frames, and stayed
    // dark) and its take band's tape.
    crate::ui::narrate_picture::repaint_landed();
    crate::ui::narrate_page::recordings_landed();
}

pub(crate) fn push_session_recordings(recordings: Vec<crate::timeline::Recording>) {
    SESSION_RECORDINGS.with(|slots| {
        slots.borrow_mut().push(Rc::new(RefCell::new(recordings)));
    });
    SESSION_DECLARED.with(|flags| flags.borrow_mut().push(std::cell::Cell::new(false)));
}

pub(crate) fn session_declared() -> bool {
    SESSION_DECLARED
        .with(|flags| flags.borrow().last().map(|flag| flag.get()))
        .unwrap_or(false)
}

thread_local! {
    /// F2.11: one recording list per window, newest last like [`REVIEW_CUTS`].
    static SESSION_RECORDINGS: std::cell::RefCell<Vec<Rc<RefCell<Vec<crate::timeline::Recording>>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static SESSION_DECLARED: std::cell::RefCell<Vec<std::cell::Cell<bool>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

// --- F2.11: folds, drawn as buttons -------------------------------------------------------------------

const FOLD_GLYPH: &str = "\u{2212}";
const UNFOLD_GLYPH: &str = "+";
/// The ✕ that takes an emptied row or a cut lane off the page (§A's gutter).
pub(crate) const CROSS_GLYPH: &str = "\u{2715}";

pub fn fold_badges_box(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    // Scoped to THIS window's own content rather than searched from the window down.
    let content = window.content()?;
    find_widget_by_name(&content, "fold-all-holder")?
        .downcast()
        .ok()
}

pub fn folded_gap_at(index: usize) -> Option<(f64, f64)> {
    FOLD_TABLES.with(|slots| {
        slots
            .borrow()
            .last()
            .and_then(|table| table.borrow().get(index).copied())
    })
}

/// The gaps this window currently wears a badge over, in tape order.
pub fn fold_gaps() -> Vec<(f64, f64)> {
    FOLD_TABLES.with(|slots| {
        slots
            .borrow()
            .last()
            .map(|t| t.borrow().clone())
            .unwrap_or_default()
    })
}

/// The band's own badge holder, scoped to THIS window exactly as [`fold_badges_box`] is (a closed
/// window's tree lives on, and a name search from the window can land in it).
pub fn fold_overlay(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    let content = window.content()?;
    find_widget_by_name(&content, "fold-badges")?
        .downcast()
        .ok()
}

/// The ONE span the fold badges are placed on: the filmed runs cut at this cut's folds, at the zoom
/// the band paints at (`cut_view::pps`, the number `scene_from` hands the painter), after the
/// gutter. Badge x and painted x cannot drift because both ask this same span.
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

pub fn refresh_fold_badges(window: &adw::ApplicationWindow) {
    // Two holders matter: the gutter box, which keeps ONLY the whole-cut `fold-all-button` (§B puts
    // that one in the gutter), and the band's overlay, which carries the per-gap badges over their
    // own bars.
    let holder = fold_badges_box(window);
    let Some(band) = fold_overlay(window) else {
        return;
    };
    let cut_ = newest_review_cut();
    // The SESSION's tape, not the kept segments — see [`page_recordings`] for why the derived list
    // can never contain a dropped stretch.
    let recordings = page_recordings();
    let runs = crate::timeline::filmed_runs(&recordings);
    let span = band_fold_span(&cut_);
    let shown = cut_fold::badges(&cut_.folds, &runs, &cut_.segs, &span);
    remember_fold_gaps(shown.iter().map(|badge| badge.gap).collect());

    // Take away every badge this pass did not re-use: a gap that moved off, shrank under
    // `FOLD_MIN_PX` or was removed with its clip must not keep a live button at its old x.
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
        if let Some(parent) = old
            .parent()
            .and_then(|node| node.downcast::<gtk::Box>().ok())
        {
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
            cut_fold::fold_status(badge.gap)
                .replace("folded ", "")
                .as_str()
        )));
        button.set_halign(gtk::Align::Start);
        button.set_valign(gtk::Align::Start);
        button.set_margin_start(badge.x.round() as i32);
        // §B puts the −/+ badges IN the selection band, not above it: `fold-badges` is pinned to the band's
        // top, so the ruler's height is paid for here or every badge floats over the ruler.
        button.set_margin_top(crate::cut_trim::RULER_H as i32 + 1);
        let win = window.clone();
        button.connect_clicked(move |_| {
            press_fold_gap(&win, index);
        });
        band.append(&button);
    }
}

thread_local! {
    /// F2.11 S1: the gaps this window's numbered badges stand for, newest window last like
    /// [`REVIEW_CUTS`].
    #[expect(clippy::type_complexity, reason = "one badge table per window, newest last")]
    static FOLD_TABLES: std::cell::RefCell<Vec<Rc<RefCell<Vec<(f64, f64)>>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

fn remember_fold_gaps(gaps: Vec<(f64, f64)>) {
    FOLD_TABLES.with(|slots| {
        // The slot is looked up and cloned out of the borrow BEFORE writing into it.
        let held = slots.borrow().last().cloned();
        match held {
            Some(table) => *table.borrow_mut() = gaps,
            None => slots.borrow_mut().push(Rc::new(RefCell::new(gaps))),
        }
    });
}

/// F2.11 S1 (`\u{2212} in such a gap folds it to a seam \u00b7 + on the seam unfolds`): the seam a
/// badge forwards to. Saves the cut afterwards, because a fold is a view but IS kept in `cut.json`.
#[expect(
    clippy::question_mark,
    reason = "no badge at that index means nothing to fold, and no status line"
)]
pub fn press_fold_gap(window: &adw::ApplicationWindow, index: usize) -> Option<String> {
    let Some(gap) = folded_gap_at(index) else {
        return None;
    };
    press_fold_at(window, gap)
}

/// F2.11 S1: fold or open one gap, named by its seconds -- the door the band's own −/+ badge
/// presses (`cut_band_marks`), which reads the gap off the badge it drew rather than off a numbered
/// button.
pub fn press_fold_at(window: &adw::ApplicationWindow, gap: (f64, f64)) -> Option<String> {
    let mut cut_ = newest_review_cut();
    let said = cut_fold::toggle_fold(&mut cut_, gap);
    seed_review_cut(&cut_);
    save_folds(&cut_);
    refresh_fold_badges(window);
    if let Some(status) = find_status(window.upcast_ref()) {
        status.set_text(&said);
    }
    Some(said)
}

/// F2.1 S3: ▶ ran into a folded seam, so it opens — the badge's fold rule through the playback
/// path.
pub fn unfold_fold_for_playback(window: &adw::ApplicationWindow, index: usize) -> Option<String> {
    let mut cut_ = newest_review_cut();
    let [start, _] = *cut_.folds.get(index)?;
    cut_.folds.remove(index);
    seed_review_cut(&cut_);
    save_folds(&cut_);
    refresh_fold_badges(window);
    // §B's sentence for THIS event ("unfolded m:ss — ▶ ran into it"), which is preview's wording,
    // not the badge's "unfolded X – Y (N s)": the person was watching, not folding.
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
    seed_review_cut(&cut_);
    save_folds(&cut_);
    refresh_fold_badges(window);
    if let Some(status) = find_status(window.upcast_ref()) {
        status.set_text(&said);
    }
    said
}

pub(crate) fn save_folds(cut_: &Cut) {
    // §7's one writer: a fold toggle lands in `cut/cut.json` through `cut_save`, the same door
    // every edit uses.
    crate::ui::window_logs::save_cut_or_say(cut_);
}

/// F2.11 S2 (`An emptied bottom row stays until its \u{2715}`): the row cross's seam.
pub fn press_row_cross(window: &adw::ApplicationWindow, row: usize) -> Option<String> {
    let cut_ = newest_review_cut();
    // The SAME spans the row list colours with, so "is this row empty?" is asked of one answer
    // rather than two.
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

/// F2.11 S2 (`a cut lane's \u{2715} removes the lane, its pins, shift, pictures and sound`): the
/// lane cross's seam.
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

pub(crate) fn sync_page_folds(cut: &mut crate::cut::Cut) {
    let mut spans = page_recordings();
    spans.extend(crate::timeline::kept_footage_recordings(cut));
    let runs = crate::timeline::filmed_runs(&spans);
    crate::cut_fold::sync_folds(cut, &runs);
}
