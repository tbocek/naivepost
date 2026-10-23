//! The `06-effect-menu` screen (§06-effects#2): the ✚ Effect dropdown OPEN, as `spec/img/06-effect-menu.png`
//! shows it. Split out of `snapshot.rs` (past the size another staging block fits in) and out of
//! `snapshot_effects.rs`, whose blocks all stage cut state; this one opens a widget, and painting it needs its
//! own top level for the same reason `snapshot_face::write_face_png` paints the run bar's ▶ apart from the
//! window: a popover's child is a separate surface, so the window's own render node does not contain it.
//!
//! Nothing here decides a rule. The rows are the app's — the buttons `ui` builds into the `effect-menu` box, in
//! the order that box holds them — and this module only pops the dropdown, reads them back off the widget tree,
//! and paints the same box the popover is showing.

use std::path::Path;

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut::EffectKind;
use crate::fx_lane;
use crate::ui;

/// How wide the painted menu shot is. Wide enough for the longest row (`🔊 Volume`) to sit inside its button with
/// the icon's padding, which the measure call above does not account for; the height IS measured.
const MENU_SHOT_W: i32 = 180;

/// Pop this window's real ✚ Effect dropdown and read its rows back off the app's own popover.
///
/// Returns the header row's text first, then one `"<name>: <label>"` per entry, in the order the `effect-menu`
/// box actually holds them — which is what makes this a check rather than a restatement: if a future edit
/// reordered the buttons, dropped one, or dropped the header §2's picture draws, the strings this returns change
/// and the caller's assert fires. The order the spec gives is
/// [`fx_lane::MENU_ORDER`]; the labels are `cut_screen::EFFECT_ITEMS`'s.
///
/// Public because `snapshot::run` → `snapshot_effects::paint` calls it for the `06-effect-menu` screen (that is
/// the program's path), and `tests/suite/sec_06_effects_2_staging.rs` calls it to prove the same door the shot uses.
pub fn stage(window: &adw::ApplicationWindow) -> Vec<String> {
    let button = menu_button(window, "effect-button");
    // The same state a click produces: `MenuButton::set_open` is what the button's own toggle does, so the
    // popover that appears is the app's, with the app's children — nothing is built for the shot.
    button.set_active(true);
    // Map the window and pump what the popover needs to lay its rows out, BEFORE they are read. A popover inside
    // a window that was never mapped produces no render node and no mapped children -- which is why the shot of
    // this screen has to do exactly this and nothing less (`snapshot_face::write_face_png` learned it the same
    // way for the run bar's glyph).
    window.present();
    // The rows go in `cut_effect_menu` while the page is being built, from this very window, so by now the
    // popover holds them; the map makes them real on screen.
    crate::snapshot_face::pump_context(64);
    assert!(
        button.is_active(),
        "snapshot: 06-effect-menu's dropdown would not open"
    );
    let popover = button
        .popover()
        .expect("snapshot: 06-effect-menu's ✚ Effect has no popover");
    // The popover's child is an outer box now: the header label first, then the six-row `effect-menu` box.
    let shell = popover
        .child()
        .and_then(|child| child.downcast::<gtk::Box>().ok())
        .expect("snapshot: 06-effect-menu's popover has no box");
    let mut rows: Vec<String> = Vec::new();
    for child in shell
        .observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
    {
        if child.widget_name() == "effect-menu-title" {
            let text = child
                .downcast_ref::<gtk::Label>()
                .map(|label| label.text().to_string())
                .unwrap_or_default();
            rows.push(format!("{}: {}", child.widget_name(), text));
        } else if child.widget_name() == "effect-menu" {
            let list = child.downcast::<gtk::Box>().expect("checked by name");
            rows.extend(
                list.observe_children()
                    .iter::<glib::Object>()
                    .flatten()
                    .filter_map(|row| row.downcast::<gtk::Button>().ok())
                    .map(|row| {
                        format!("{}: {}", row.widget_name(), row.label().unwrap_or_default())
                    }),
            );
        }
    }
    assert_eq!(
        rows.len(),
        fx_lane::MENU_ORDER.len() + 1,
        "snapshot: 06-effect-menu's popover drew {} rows, the menu lists {} effects plus its own header",
        rows.len(),
        fx_lane::MENU_ORDER.len()
    );
    rows
}

/// The label [`crate::fx_lane::MENU_ORDER`] implies for each kind, spelled the way the popover spells it.
///
/// A lookup against the table the page builds from, not a second copy of it: the strings come from
/// `cut_screen::EFFECT_ITEMS`, matched by the kind id each entry carries in its `tip`, so this cannot drift from
/// what the dropdown actually draws.
pub fn labels_in_menu_order() -> Vec<String> {
    fx_lane::MENU_ORDER
        .iter()
        .map(|kind| {
            let id = effect_id(kind);
            crate::cut_screen::EFFECT_ITEMS
                .iter()
                .find(|item| item.tip == id)
                .unwrap_or_else(|| panic!("{id} has no dropdown entry"))
                .label
                .to_string()
        })
        .collect()
}

/// The dropdown entry's kind id for an [`EffectKind`] — the word `cut::Fx::parse_kind` accepts, which is what
/// each row's `tip` holds. Written here because `EffectKind::parse` is private to `cut` and this is the reverse
/// direction of the same table.
fn effect_id(kind: &EffectKind) -> &'static str {
    match kind {
        EffectKind::Zoom => "zoom",
        EffectKind::Speed => "speed",
        EffectKind::Text => "text",
        EffectKind::Svg => "svg",
        EffectKind::Volume => "volume",
        EffectKind::Label => "label",
    }
}

/// Ask `snapshot::run` to paint THIS screen's shot from the dropdown's own box instead of from the window.
///
/// A `gtk::Popover` lives on its own surface, so the window's render node does not contain it: a whole-window
/// shot of the popped menu shows the page with no menu in it. The frame-taking step reads this flag and calls
/// [`write_png`] with the button, exactly as `03-runbar-busy` asks for the run bar's ▶ on its own.
///
/// Deliberately NOT a parameter of `snapshot_effects::paint`: that one call site is one of five module entry
/// points `snapshot::run` invokes before the pixels are taken, and threading a return value through it for one
/// screen would move four other modules' signatures for no gain. Process-wide and set-once, like the snapshot
/// staging every other screen here already relies on.
pub fn note_shot_sidecar() {
    REQUESTED.with(|flag| flag.set(true));
}

/// Has [`note_shot_sidecar`] been called? `snapshot::run` asks once, when it chooses what to paint.
pub fn shot_sidecar_wanted() -> bool {
    REQUESTED.with(|flag| flag.get())
}

/// Is this screen's SUBJECT the popover itself? §06-effects#2's picture (`spec/img/06-effect-menu.png`)
/// shows the six labelled rows, not the page behind them, so the menu takes `<name>.png` and the page goes
/// beside it -- the same reading `03-policy-form`'s `subject` arm already applies to a dialog it pictures.
pub fn is_menu_subject(screen: &str) -> bool {
    screen == "06-effect-menu"
}

thread_local! {
    static REQUESTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Paint the dropdown's own row box into `out`, on its own mapped top level.
///
/// The precedent is [`crate::snapshot_face::write_face_png`]: a widget whose real home is a separate surface (a
/// popover, an icon in a 1334 px window) gives no legible pixels inside the whole-window shot, so it is shown
/// where it can be read. A COPY of the box, not the popover's child — reparenting a widget out of a mapped window
/// leaves it with no render node, which is what made an earlier attempt print "window produced no render node".
/// The copy carries each row's own label and name because those ARE the thing this screen records.
pub fn write_png(button: &gtk::MenuButton, out: &Path) -> Result<(), String> {
    let source = button
        .popover()
        .and_then(|popover| popover.child())
        .and_then(|child| child.downcast::<gtk::Box>().ok())
        .ok_or("the ✚ Effect dropdown has no row box to paint")?;
    // The popover's child is now the outer shell (header + `effect-menu` box); the copy below flattens it back
    // into one column of rows, so find the inner box by name rather than assuming the child IS the six rows.
    let mut list: Option<gtk::Box> = None;
    let mut header: Option<String> = None;
    for child in source
        .observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
    {
        if child.widget_name() == "effect-menu" {
            list = child.downcast::<gtk::Box>().ok();
        } else if let Some(label) = child.downcast_ref::<gtk::Label>() {
            header = Some(label.text().to_string());
        }
    }
    let source_rows = list.ok_or("the ✚ Effect popover holds no `effect-menu` box")?;
    let copy = gtk::Box::new(gtk::Orientation::Vertical, 0);
    copy.set_widget_name("effect-menu");
    copy.set_homogeneous(false);
    // The header first, as §2's picture draws it. Inside a popover GTK gives the label the menu row's own ink;
    // on this bare top level nothing does, so the words are spelled in `color` from the same reason the rows
    // below need their box background spelled — see the note on [`install`] next to them.
    if let Some(words) = header {
        let head = gtk::Label::new(Some(&words));
        head.set_widget_name("effect-menu-title");
        head.set_halign(gtk::Align::Start);
        head.set_margin_start(8);
        head.set_margin_top(4);
        head.set_margin_bottom(2);
        head.add_css_class("effect-shot-head");
        copy.append(&head);
    }
    for row in source_rows
        .observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|row| row.downcast::<gtk::Button>().ok())
    {
        let held = gtk::Button::with_label(row.label().unwrap_or_default().as_str());
        held.set_widget_name(row.widget_name().as_str());
        held.set_tooltip_text(row.tooltip_text().as_deref());
        // Left-aligned WITHIN the row, and the row itself fills the shot's width: `halign(Start)` on the button
        // collapsed each one to its own text and centred it in the box, so the rows printed as six fragments
        // floating in the middle instead of the menu's left-aligned rows.
        held.set_halign(gtk::Align::Fill);
        let words = row.label().unwrap_or_default().to_string();
        let inner = row_label_left(&words);
        held.set_child(Some(&inner));
        copy.append(&held);
    }
    let holder = gtk::Window::new();
    holder.set_decorated(false);
    // The shot is a snapshot of this box, not of a live window (see [`crate::snapshot_face::write_png`]), so GTK's
    // theme never paints the ground or the glyphs behind it: without these two rules the rows came out as ink on
    // transparent and printed as grey words on black. The colours are the popover's own — the same light ground and
    // near-black text §2's menu shows.
    install_shot_css();
    copy.add_css_class("effect-shot-menu");
    // Sized from the rows' own natural height (measured, never guessed) and wide enough that the longest row's
    // text sits fully inside its button: a `measured + 12` width is what clipped `⊕ Zoom` to `Zoom` in an earlier
    // attempt, because the icon and its padding are not in a button's text width. `measure` answers
    // (min_w, min_h, nat_w, nat_h).
    let height = copy.measure(gtk::Orientation::Vertical, -1).3;
    holder.set_default_size(MENU_SHOT_W, height + 12);
    holder.set_child(Some(&copy));
    holder.present();
    // The map must happen before the frame is taken; `write_png` reads the widget's allocated size.
    crate::snapshot_face::pump_context(64);
    let result = crate::snapshot_face::write_png(copy.upcast_ref(), out);
    holder.close();
    result
}

/// One menu row's words, pushed to the left edge of whatever box holds them — how §2's popover prints them.
fn row_label_left(words: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(words));
    label.set_halign(gtk::Align::Start);
    label.set_hexpand(true);
    label.set_margin_start(10);
    label
}

/// Has [`install_shot_css`] run? One provider for the process; a snapshot paints one screen.
static SHOT_CSS_LOADED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Spell the menu shot's own ground and ink, because a snapshotted box gets no theme painting behind it.
fn install_shot_css() {
    if SHOT_CSS_LOADED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let provider = gtk::CssProvider::new();
    provider.load_from_string(
        ".effect-shot-menu { background-color: #f6f5f4; color: #1c1c1c; }\n\
         .effect-shot-head { font-weight: bold; color: #1c1c1c; }",
    );
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("a display to style"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

/// This window's ✚ Effect `MenuButton`, for the frame-taking step in `snapshot::run`, which is a different
/// call from [`stage`] and so cannot carry the button back in a local.
pub fn effect_button() -> gtk::MenuButton {
    menu_button(&ui::window::main_window(), "effect-button")
}

/// Find this window's `MenuButton` by name. The same walk `tests/suite/effect_menu_record_widgets.rs` uses: the
/// toolbar's effect trigger is a `MenuButton`, so the plain-Button lookup in `tests/common` cannot reach it.
fn menu_button(window: &adw::ApplicationWindow, name: &str) -> gtk::MenuButton {
    fn walk(node: &gtk::Widget, name: &str, out: &mut Option<gtk::MenuButton>) -> bool {
        if node.widget_name() == name && node.downcast_ref::<gtk::MenuButton>().is_some() {
            out.replace(node.clone().downcast().expect("checked above"));
            return true;
        }
        if let Some(child) = node.first_child() {
            let mut cursor = Some(child);
            while let Some(current) = cursor {
                if walk(&current, name, out) {
                    return true;
                }
                cursor = current.next_sibling();
            }
        }
        false
    }
    let mut found = None;
    walk(window.upcast_ref(), name, &mut found);
    found.unwrap_or_else(|| panic!("snapshot: the Cut page drew no MenuButton named {name}"))
}
