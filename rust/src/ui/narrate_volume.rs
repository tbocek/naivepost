//! §07-narrate#1-screen (**6**) — the Narrate page's half of the ONE preview volume (F2.5 S6).

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut_hear;
use crate::ui::narrate_page_state::{NarratePage, PAGE as NARRATE_PAGE};
use crate::ui::window::{self};
use crate::ui::window_doors::find_widget_by_name;
use crate::ui::window_page_state::{WindowPage, PAGE as WINDOW_PAGE};

/// The widget name §1's **6** answers to, spelled once so the build, the wire and the mirror cannot
/// disagree about which scale on this page is the volume.
pub(crate) const WIDGET: &str = "narrate-volume";

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
pub(crate) fn ensure_slot(window: &adw::ApplicationWindow) {
    if scale(window).is_none() {
        return;
    }
    let owned = NARRATE_PAGE.with(
        |NarratePage {
             opened_by_narrate: flag,
             ..
         }| flag.get(),
    );
    let fresh = WINDOW_PAGE.with(
        |WindowPage {
             preview_volumes: slots,
             ..
         }| slots.borrow().is_empty(),
    );
    if !owned && !fresh {
        // The newest slot is the Cut page's (or an earlier window's): this window already shares it.
        return;
    }
    WINDOW_PAGE.with(
        |WindowPage {
             preview_volumes: slots,
             ..
         }| {
            let volume =
                std::rc::Rc::new(std::cell::RefCell::new(cut_hear::PreviewVolume::default()));
            // A slot of our own starts at the number the previous window was left at: one setting means a new
            // window does not reset it (§A.4's "the same setting wherever it is shown").
            volume.borrow_mut().set_percent(current_percent());
            slots.borrow_mut().push(volume);
        },
    );
    NARRATE_PAGE.with(
        |NarratePage {
             opened_by_narrate: flag,
             ..
         }| flag.set(true),
    );
}

/// F2.5 S6: a drag here is the same hand on the Cut page's slider — the number goes to
/// [`window::set_preview_volume`] and the widget decides no part of the rule, because the property is
/// owned by [`cut_hear::PreviewVolume`] and every preview reads that same value back.
pub(crate) fn wire(window: &adw::ApplicationWindow) {
    let Some(slider) = scale(window) else {
        return;
    };
    slider.connect_value_changed(move |scale| {
        window::set_preview_volume(scale.value());
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
    WINDOW_PAGE.with(
        |WindowPage {
             preview_volumes: slots,
             ..
         }| {
            slots
                .borrow()
                .last()
                .map(|volume| volume.borrow().percent())
                .unwrap_or(cut_hear::VOLUME_DEFAULT * 100.0)
        },
    )
}

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
