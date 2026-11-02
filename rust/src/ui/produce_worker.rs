//! §03-shell §1 for the Produce page: its run (F5.1) and its six presses (F5.7) walk on a worker, never on the
//! window thread. What a walk reads off the page is taken on the window thread first ([`Carried`]) and loaded
//! again on the worker's own thread, because the page's state, the scripted spawner and the scripted draw are
//! thread-locals there. The first rewrite ran the render, the model call and sd.cpp's poll inside the click.

use crate::produce_translate::SpawnSlot;
use crate::ui::produce_languages::DrawSlot;
use crate::ui::produce_page::ProduceState;
use crate::ui::produce_page_state::{ProducePage, PAGE};

/// What a Produce walk carries from the window thread to its worker.
pub(crate) struct Carried {
    state: ProduceState,
    spawn: Option<SpawnSlot>,
    draw: Option<(DrawSlot, Vec<u8>)>,
}

impl Carried {
    /// Take the page's state and the scripted seams a test loaded, on the window thread.
    pub(crate) fn take() -> Self {
        Carried {
            state: crate::ui::produce_page::read_state(),
            spawn: crate::produce_translate::spawn_for_test(),
            draw: crate::ui::produce_languages::carried_draw(),
        }
    }

    /// Load them again on the worker's thread, so every leg reads what the press found.
    pub(crate) fn load(self) {
        crate::ui::produce_page::set_state(self.state);
        if let Some(spawn) = self.spawn {
            crate::produce_translate::set_spawn_for_test(spawn);
        }
        crate::ui::produce_languages::load_draw(self.draw);
    }
}

/// Whether a page press is on its worker now.
pub fn busy() -> bool {
    PAGE.with(|page| page.busy.get())
}

/// Run one page press on a worker: `work` gets the page as the press found it and answers on the worker;
/// `land` takes the answer on the window thread. `button` is the widget pressed, greyed while the walk runs
/// (the press's busy face). A press made while another is walking is ignored and answers `false`.
pub(crate) fn press<T: Send + 'static>(
    button: Option<gtk4::Button>,
    work: impl FnOnce() -> T + Send + 'static,
    land: impl FnOnce(T) + 'static,
) -> bool {
    use gtk4::prelude::*;
    if busy() {
        return false;
    }
    PAGE.with(|ProducePage { busy: flag, .. }| flag.set(true));
    if let Some(button) = button.as_ref() {
        button.set_sensitive(false);
    }
    let carried = Carried::take();
    crate::ui::background::run(
        "produce-press",
        move || {
            carried.load();
            work()
        },
        move |answer| {
            PAGE.with(|ProducePage { busy: flag, .. }| flag.set(false));
            if let Some(button) = button.as_ref() {
                button.set_sensitive(true);
            }
            land(answer);
        },
    );
    true
}
