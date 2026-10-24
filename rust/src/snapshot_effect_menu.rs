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
/// The gap between the button's bottom edge and the painted menu's top edge — one row of air, as GTK leaves it.
const PANEL_GAP_PX: f32 = 2.0;
/// The panel's own frame padding below its rows, so the last row is not flush with the border.
const PANEL_PAD_PX: i32 = 8;

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

/// Is this screen's SUBJECT the popover itself? It no longer is.
///
/// `spec/QUESTIONS.md`, §06 answer 3(b): "06-effect-menu is the Cut page with the '+ Effect' menu open under its
/// button, the toolbar and the tracks around it: the render shows the menu alone, stretched over the whole window…
/// the snapshot draws the window first and the open menu on top of it, where it opens and at its own size". So the
/// PAGE keeps `<name>.png` and [`paint_staged`] hangs the menu over it — the same reversal
/// `snapshot_prompt_picker::is_picker_subject` records for `04-prompt-picker`. The door stays (and stays asked) so
/// the naming rule has one source rather than a screen name hardcoded in `snapshot::run`.
pub fn is_menu_subject(_screen: &str) -> bool {
    false
}

thread_local! {
    static REQUESTED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// The menu panel's width over the page: [`MENU_SHOT_W`] is what a legible row needs, and it is also what keeps
/// the popup COMPACT — the whole-window sheet an earlier ask produced is what §2's picture does not show.
fn panel_width() -> i32 {
    MENU_SHOT_W
}

/// The panel's height: the header line plus every row's own measured natural height, never a guess. Measured off
/// the rows and not off the box because a `gtk::Box` that has never been allocated answers 0 there (the same lesson
/// `snapshot_prompt_picker::panel_height` records).
fn panel_height(list: &gtk::Widget) -> i32 {
    let rows: i32 = list
        .observe_children()
        .iter::<glib::Object>()
        .filter_map(Result::ok)
        .filter_map(|row| row.downcast::<gtk::Widget>().ok())
        .map(|row| row.measure(gtk::Orientation::Vertical, -1).3)
        .sum();
    let floor = list
        .first_child()
        .map(|row| row.measure(gtk::Orientation::Vertical, -1).3)
        .unwrap_or_default()
        .max(1);
    rows.max(floor) + PANEL_PAD_PX
}

/// Where the painted menu's left edge sits: GTK points a menu's own left edge at the button's centre, clamped
/// so the panel stays inside the window. `spec/img/06-effect-menu.png` draws exactly that — the rows hanging
/// under the middle of the ✚ Effect button, not flush with its left border.
pub fn panel_x(button_x: f32, button_w: f32, panel_w: i32, win_w: i32) -> f32 {
    (button_x + button_w / 2.0)
        .max(0.0)
        .min((win_w - panel_w).max(0) as f32)
}

/// Where the painted menu's top edge sits: one gap below the button, clamped inside the window.
pub fn panel_y(button_y: f32, button_h: f32, panel_h: i32, win_h: i32) -> f32 {
    (button_y + button_h + PANEL_GAP_PX)
        .max(0.0)
        .min((win_h - panel_h).max(0) as f32)
}

/// Paint the Cut page with the ✚ Effect menu open OVER it, into `out`: the window shot first, then the app's own
/// menu rows blitted where the popover would sit, at their own size.
///
/// This is what `spec/img/06-effect-menu.png` shows — the toolbar, ruler, green kept-scene bands and thumbnail
/// rows around a compact popup under ✚ Effect — and `spec/QUESTIONS.md` answer 3(b) asks for it in these words:
/// "the snapshot draws the window first and the open menu on top of it, where it opens and at its own size".
/// The precedent is [`crate::snapshot_prompt_picker::paint_open_over_window`], for the same technical reason: a
/// `gtk::Popover` lives on its own surface, so the window's render node cannot contain it and a whole-window shot
/// of the popped menu shows a page with no menu. The panel gets its own mapped top level only long enough to gain
/// a render node, sized from its OWN measure (never the window's ask, which is what once stretched six rows over
/// a 1343×900 frame).
/// What [`paint_staged`] reports: how many menu entries were painted, the panel's own size, and where
/// its top-left landed in the frame. Named because the bare tuple tripped `clippy::type_complexity`.
pub type MenuShot = (usize, (i32, i32), (i32, i32));

/// The menu's holder: its own top level, shown before the shot so GTK has laid it out by the time the
/// picture is taken (`snapshot_ready` waits for it by [`SHOT_MENU_NAME`]), instead of being shown inside the
/// shot and given a fixed number of loop turns.
pub struct StagedMenu {
    holder: gtk::Window,
    copy: gtk::Box,
    rows: usize,
    width: i32,
    height: i32,
}

/// The name of the staged menu's rows: what `--shot-on open:` waits for on this screen.
pub const SHOT_MENU_NAME: &str = "effect-shot-menu";

impl StagedMenu {
    pub fn holder(&self) -> gtk::Widget {
        self.holder.clone().upcast()
    }
}

/// Copies the ✚ Effect menu's rows into a holder of their own and shows it, sized from the panel's OWN
/// measure (never the window's ask, which is what once stretched six rows over a 1343×900 frame).
pub fn stage_open_over_window(window: &adw::ApplicationWindow) -> Result<StagedMenu, String> {
    let button = menu_button(window, "effect-button");
    let (copy, rows) = copied_rows(&button)?;
    install_shot_css();
    copy.add_css_class("effect-shot-menu");
    copy.set_widget_name(SHOT_MENU_NAME);
    let width = panel_width();
    // Filled before it is measured: a `gtk::Box` shrink-wraps to its labels' natural width, which is how a
    // "measured" panel once came out the width of the whole window.
    copy.set_halign(gtk::Align::Fill);
    copy.set_hexpand(true);
    let height = panel_height(copy.upcast_ref());
    let holder = gtk::Window::new();
    holder.set_decorated(false);
    holder.set_default_size(width, height);
    holder.set_child(Some(&copy));
    holder.present();
    // Its own top level, with its own frames: owed until GTK has given it a size.
    crate::ui::settle::when("the menu laid out in its holder", &holder, |holder| holder.width() > 0 && holder.height() > 0, |_| {});
    Ok(StagedMenu { holder, copy, rows, width, height })
}

/// Paints the Cut page with the staged menu OVER it, into `out`: the window first, then the menu where the
/// popover would sit, at its own size; then closes the holder. A holder not painted yet has no render node:
/// that is an error, and the holder stays open for another try.
pub fn paint_staged(window: &adw::ApplicationWindow, menu: &StagedMenu, out: &Path) -> Result<MenuShot, String> {
    let button = menu_button(window, "effect-button");
    let bounds = button
        .compute_bounds(window.upcast_ref::<gtk::Widget>())
        .ok_or("the ✚ Effect button has no box to hang the menu under")?;
    let (asked_w, asked_h) = crate::snapshot_face::asked_shot_wh();
    let (w, h) = (
        if asked_w > 0 { asked_w } else { window.width() },
        if asked_h > 0 { asked_h } else { window.height() },
    );
    let (x, y) = (
        panel_x(bounds.x(), bounds.width(), menu.width, w).floor(),
        panel_y(bounds.y(), bounds.height(), menu.height, h).floor(),
    );
    let surface = crate::snapshot_face::render_surface(window.upcast_ref(), w, h)?;
    let cr = cairo::Context::new(&surface).map_err(|err| err.to_string())?;
    let painted = crate::snapshot_face::render_surface(menu.copy.upcast_ref(), menu.width, menu.height).and_then(|panel| {
        cr.set_source_surface(&panel, x as f64, y as f64).map_err(|err| err.to_string())?;
        cr.paint().map_err(|err| err.to_string())
    });
    painted?;
    menu.holder.close();
    let mut file = std::fs::File::create(out).map_err(|err| err.to_string())?;
    surface.write_to_png(&mut file).map_err(|err| err.to_string())?;
    Ok((menu.rows, (menu.width, menu.height), (x as i32, y as i32)))
}

fn copied_rows(button: &gtk::MenuButton) -> Result<(gtk::Box, usize), String> {
    let source = button
        .popover()
        .and_then(|popover| popover.child())
        .and_then(|child| child.downcast::<gtk::Box>().ok())
        .ok_or("the ✚ Effect dropdown has no row box to paint")?;
    // The popover's child is the outer shell (header + `effect-menu` box); find the inner box by name rather than
    // assuming the child IS the rows.
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
    // The header first, as §2's picture draws it. Inside a popover GTK gives the label the menu row's own ink; on
    // a bare top level nothing does, so the words are spelled in `color` by `install_shot_css`.
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
    let mut entries = 0usize;
    for row in source_rows
        .observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|row| row.downcast::<gtk::Button>().ok())
    {
        let held = gtk::Button::with_label(row.label().unwrap_or_default().as_str());
        held.set_widget_name(row.widget_name().as_str());
        held.set_tooltip_text(row.tooltip_text().as_deref());
        // Left-aligned WITHIN the row, and the row itself fills the panel's width: `halign(Start)` on the button
        // collapsed each one to its own text and centred it, so the rows printed as fragments floating mid-box.
        held.set_halign(gtk::Align::Fill);
        let words = row.label().unwrap_or_default().to_string();
        held.set_child(Some(&row_label_left(&words)));
        copy.append(&held);
        entries += 1;
    }
    Ok((copy, entries))
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
