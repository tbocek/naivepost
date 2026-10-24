//! §07-narrate#1-screen (**6**) — the Narrate page's half of the ONE preview volume (F2.5 S6).
//!
//! The slider in §1's transport row is a second face of [`cut_hear::PreviewVolume`], not a second
//! setting: `spec/inventory/narrate.md` §A describes it as "shared with every preview" and carries the
//! Cut page's own tooltip, so a drag here has to move the same number the Cut page's slider moves and
//! every preview reads back. Three doors, all thin: [`ensure_slot`] gives a window that builds this
//! page a slot to hold the one number, [`wire`] hands a drag to `ui::set_preview_volume` and nothing
//! else, [`mirror`] puts the shared number back on the widget after something else moved it.
//!
//! Split out of `narrate_page.rs`, which is at its size budget; the page re-exports what it calls.

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut_hear;
use crate::ui::window::{self, PREVIEW_VOLUMES};
use crate::ui::window_doors::find_widget_by_name;

/// The widget name §1's **6** answers to, spelled once so the build, the wire and the mirror cannot
/// disagree about which scale on this page is the volume.
pub(crate) const WIDGET: &str = "narrate-volume";

thread_local! {
    /// Whether THIS thread's newest volume slot was opened by the Narrate page rather than by the Cut
    /// page. A slot is pushed per window and read as "the newest", so the page that pushed last owns it;
    /// the guard below only ever pushes when the newest slot belongs to nobody or to this page, which is
    /// how a window holding both sliders keeps exactly ONE slot (§A.4's one number needs one holder).
    static OPENED_BY_NARRATE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// This page's volume control, if this window has one. Only the Narrate page builds it, so `None` is
/// "not this page" — the same reading `preview_volume_scale` gives the Cut page's slider.
pub(crate) fn scale(window: &adw::ApplicationWindow) -> Option<gtk::Scale> {
    find_widget_by_name(window.upcast_ref(), WIDGET)?
        .downcast()
        .ok()
}

/// Give this window a slot for the one number. Called from `build_window` after `set_content`, beside
/// the Cut page's own push: without a slot, `ui::set_preview_volume` finds none, mints a throwaway and
/// the drag is silently lost — which is why the Narrate page needed this door at all.
///
/// The guard reads the SLOT stack rather than "does this window have both sliders": every window builds
/// all five pages into one tree, so both sliders exist everywhere and the only thing that tells the two
/// pushes apart is who pushed last. That is also how a window keeps exactly ONE slot (§A.4's one number
/// needs one holder).
pub(crate) fn ensure_slot(window: &adw::ApplicationWindow) {
    if scale(window).is_none() {
        return;
    }
    let owned = OPENED_BY_NARRATE.with(|flag| flag.get());
    let fresh = PREVIEW_VOLUMES.with(|slots| slots.borrow().is_empty());
    if !owned && !fresh {
        // The newest slot is the Cut page's (or an earlier window's): this window already shares it.
        return;
    }
    PREVIEW_VOLUMES.with(|slots| {
        let volume = std::rc::Rc::new(std::cell::RefCell::new(cut_hear::PreviewVolume::default()));
        // A slot of our own starts at the number the previous window was left at: one setting means a new
        // window does not reset it (§A.4's "the same setting wherever it is shown").
        volume.borrow_mut().set_percent(current_percent());
        slots.borrow_mut().push(volume);
    });
    OPENED_BY_NARRATE.with(|flag| flag.set(true));
}

/// F2.5 S6: a drag here is the same hand on the Cut page's slider — the number goes to
/// [`window::set_preview_volume`] and the widget decides no part of the rule, because the property is
/// owned by [`cut_hear::PreviewVolume`] and every preview reads that same value back.
pub(crate) fn wire(window: &adw::ApplicationWindow) {
    let Some(slider) = scale(window) else {
        return;
    };
    let window = window.clone();
    slider.connect_value_changed(move |scale| {
        window::set_preview_volume(&window, scale.value());
    });
}

/// The slider at rest: the shared scale (0..100, step 1). Its value is this slot's own number —
/// [`ensure_slot`] has already pushed it and copied in what was last set, so a fresh page shows the one
/// volume rather than a default of its own.
pub(crate) fn build() -> gtk::Scale {
    let slider = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    slider.set_widget_name(WIDGET);
    slider.set_draw_value(false);
    slider.set_size_request(90, -1);
    // The shared sentence, verbatim from `cut_hear`: two wordings would be two settings, and §A says this
    // slider is "shared with every preview".
    slider.set_tooltip_text(Some(cut_hear::VOLUME_TIP));
    slider.set_value(current_percent());
    slider
}

/// The one number as this widget's position: whatever the newest slot holds, or the shared default when
/// this thread has never opened one (a headless page build).
pub(crate) fn current_percent() -> f64 {
    PREVIEW_VOLUMES.with(|slots| {
        slots
            .borrow()
            .last()
            .map(|volume| volume.borrow().percent())
            .unwrap_or(cut_hear::VOLUME_DEFAULT * 100.0)
    })
}

/// Put the shared number on BOTH faces when either differs from it — at build, and again on every
/// repaint. Writing a scale emits `value-changed`, so an equal value is left alone: a refresh that
/// re-set what was already there would read as a phantom drag (the same reason
/// [`crate::ui::narrate_picker::follow`] only replaces rows that differ). The shell builds all five pages
/// into one tree, so one mirror keeps both faces showing the one number.
pub(crate) fn mirror(window: &adw::ApplicationWindow) {
    let percent = current_percent();
    for slider in [scale(window), crate::ui::preview_volume_scale(window)]
        .into_iter()
        .flatten()
    {
        if (slider.value() - percent).abs() > f64::EPSILON {
            slider.set_value(percent);
        }
    }
}
