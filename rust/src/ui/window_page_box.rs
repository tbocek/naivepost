//! §F0.1–§F0.12 / §05-cut#1-screen / §06-effects#2: one page of the window — each tab's widget tree, the
//! Cut page's toolbar groups and effect popover, and Prepare's source list and frame controls. Widget
//! names (`add-sources-button`, `cut-toolbar`, `select-surface`, `track-strip`, `effect-menu`, …) are
//! found by name in tests. No rules live here (spec/00-principles.md §5).

use std::cell::RefCell;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::cut;
use crate::cut_hear;
use crate::cut_screen;
use crate::cut_select;
use crate::cut_trim;
use crate::ui::cut_form_frame;
use crate::ui::cut_tracks;

/// The least height a page's scroller offers (§1's draggable divider): below it the page would be unreadable.
pub(crate) const PAGE_MIN_H: i32 = 360;

/// §1's Prepare split opened at half: (1240 - 48) / 2. A starting position only, as `window_divider::START_PAGE_PX`.
pub(crate) const PREPARE_HALF_PX: i32 = 596;

pub(crate) const SOURCES_MIN_H: i32 = 120;

use crate::project::Project;
use crate::shell::Page;
use crate::sources;
use crate::ui::cut_lane_rows::cut_form_column;
use crate::ui::source_rows;
use crate::ui::window::{press_effect_item, session_reads, STRIP_HEIGHT, STRIP_WIDTH};
use crate::ui::window_bench::{bench_box, bench_paths};
use crate::ui::window_doors::find_widget_by_name;
use crate::ui::window_policy::main_window;
use crate::ui::window_project::{ADD_LABEL, COPY_LABEL};

/// One page of the window. Prepare's two widgets are handed back so `build_window` wires *that* button; elsewhere both are `None`.
pub(crate) fn page_box(
    page: &str,
    project: &Project,
    session: &Rc<RefCell<Project>>,
    status: &gtk::Label,
    window: &adw::ApplicationWindow,
) -> (gtk::Widget, Option<gtk::Button>, Option<gtk::CheckButton>) {
    let view = adw::ToolbarView::new();

    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let page_scroller = gtk::ScrolledWindow::new();
    page_scroller.set_widget_name(&format!("page-scroll-{page}"));
    page_scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
    page_scroller.set_min_content_height(PAGE_MIN_H);
    // §1: "the page takes all the rest of the height". With both halves expanding the Paned split its leftover by
    // share and the bottom box painted over the page.
    page_scroller.set_vexpand(true);
    page_scroller.set_vexpand_set(true);
    // An `Automatic` hscroll bar makes the scroller's minimum width zero and the page collapses into a narrow
    // column (`tests/suite/window_shrinks.rs` measures the floor); the page must fit its width.
    page_scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
    // Not `propagate_natural_width`: it makes the page's full width the scroller's minimum, which
    // `tests/suite/window_shrinks.rs` forbids. Wide parts scroll inside themselves.
    page_scroller.set_child(Some(&box_));
    box_.set_margin_start(crate::ui::margins::PAGE_EDGE_PX);
    box_.set_margin_end(crate::ui::margins::PAGE_EDGE_PX);
    // 03-shell §1: 12 px from the header bar; 24 here plus the empty row and spacing read as a 36 px band.
    box_.set_margin_top(crate::ui::margins::PAGE_EDGE_PX);
    box_.set_margin_bottom(crate::ui::margins::PAGE_EDGE_PX);

    // Nothing is drawn here, but this empty row is `cut_page_layout::arrange`'s insertion anchor (it seats the
    // vertical paned after the first child). Hidden so it costs no row spacing.
    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    title_row.set_widget_name("page-anchor");
    title_row.set_visible(false);
    box_.append(&title_row);

    let mut prepare: Option<(gtk::Button, gtk::CheckButton)> = None;
    if page == "Prepare" {
        // §1: two columns split by a draggable divider opened at half: left = files and session, right = prompt bench.
        let left = gtk::Box::new(gtk::Orientation::Vertical, 12);
        left.set_widget_name("prepare-left");

        let add_ = gtk::Button::with_label(ADD_LABEL);
        add_.set_widget_name("add-sources-button");
        add_.set_tooltip_text(Some(sources::ADD_TIP));
        let copy_ = gtk::CheckButton::with_label(COPY_LABEL);
        copy_.set_widget_name("copy-into-project");
        copy_.set_tooltip_text(Some(sources::COPY_TIP));
        // Ticked: bring the files inside the project by default (F0.12 S2).
        copy_.set_active(!project.reference_sources);

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.set_widget_name("add-sources-row");
        row.append(&add_);
        row.append(&copy_);
        left.append(&row);

        // §1 gives the list its OWN scroller; otherwise a long list pushes the row under it out of the column
        // (`tests/suite/window_shrinks.rs`).
        let list = sources_list(session, status);
        let frame = gtk::Frame::new(None);
        frame.set_child(Some(&list));
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_widget_name("sources-scroll");
        scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
        scroll.set_min_content_height(SOURCES_MIN_H);
        scroll.set_vexpand(true);
        scroll.set_vexpand_set(true);
        scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
        scroll.set_hexpand(true);
        scroll.set_hexpand_set(true);
        scroll.set_child(Some(&frame));
        left.append(&scroll);

        // THE LEFT COLUMN HAS NO STRETCHING FILLER: an expanding spacer leaves the list at its minimum with a grey
        // band under the last row. The scroller takes the extra height instead.
        scroll.set_vexpand_set(true);
        // The Paned's child claims the width, not the widgets inside it.
        left.set_hexpand(true);
        left.set_hexpand_set(true);

        let (heading, bench_scroller) = bench_box(session, &bench_paths());
        let right = gtk::Box::new(gtk::Orientation::Vertical, 6);
        right.set_widget_name("prepare-right");
        right.set_vexpand(true);
        right.set_hexpand(true);
        right.set_hexpand_set(true);
        right.append(&heading);
        // The bench's own scroller (`window_bench`) keeps a long prompt from setting the page's minimum; `fill`
        // is what stretches it across a Paned half (§1: "one text box filling the rest of the column").
        right.append(&editor_scroller(&bench_scroller));

        // `shrink_*_child(true)` on BOTH halves keeps the window shrinkable to §1's 1240 px minimum.
        let split = gtk::Paned::new(gtk::Orientation::Horizontal);
        split.set_widget_name("prepare-split");
        split.set_wide_handle(false);
        split.set_shrink_start_child(true);
        split.set_shrink_end_child(true);
        // Without `hexpand` a Paned half keeps its natural width and "opened at half" holds only by accident.
        left.set_hexpand(true);
        right.set_hexpand(true);
        split.set_start_child(Some(&left));
        split.set_end_child(Some(&right));
        box_.append(&split);
        // §1: nothing scrolls as a whole at the minimum size; the split must take the leftover height or Prepare overflows its band.
        split.set_vexpand(true);
        split.set_position(PREPARE_HALF_PX);

        prepare = Some((add_, copy_));
    }

    // The User Context is not drawn here: §04 §1 puts it in the prompt bench.

    if page == Page::Narrate.label() {
        let narrate = crate::ui::narrate_page::build();
        narrate.set_vexpand(true);
        box_.append(&narrate);
    }

    if page == Page::Produce.label() {
        let produce = crate::ui::produce_page::build();
        // The split is the whole Produce page and must take the band's leftover height.
        produce.set_vexpand(true);
        box_.append(&produce);
    }

    if page == Page::Cut.label() {
        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, cut_screen::TOOLBAR_GAP_PX);
        toolbar.set_widget_name("cut-toolbar");
        // §A's one row of six groups is ~484 px wider than the bar at §1's floor; Adwaita's ~24 px button padding
        // is the overflow. `cut_screen::TOOLBAR_CSS` thins it.
        install_toolbar_css_once();
        // Fill, not Start: with `halign(Start)` the bar asks only for its groups' sum and the page beside it is dead plate.
        toolbar.set_halign(gtk::Align::Fill);
        toolbar.set_hexpand(true);
        let put_group = { |group: gtk::Box| toolbar.append(&group) };
        box_.insert_child_after(&toolbar, Some(&title_row));
        let mut previous: gtk::Widget = toolbar.clone().upcast();

        // Transport group (§1 items 5-8) from `cut_screen::TRANSPORT_BUTTONS`; starts live.
        let transport = cut_tool_group("transport", &cut_screen::TRANSPORT_BUTTONS, false);
        put_group(transport);

        let volume_group = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        volume_group.set_widget_name("group-volume");
        // An expanding group gets an EVEN slice of the slack, which scrolled Copy/Paste/Insert out of `group-verbs`.
        volume_group.set_hexpand(false);
        volume_group.set_halign(gtk::Align::Start);

        // F2.5 S6: `cut_hear::PreviewVolume` is the single loudness setting; this slider owns no copy. 0..100 here,
        // 0..1 underneath (`PreviewVolume::set_percent`). No icon: a themed one does not render headless.
        let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
        volume.set_widget_name("preview-volume");
        volume.set_tooltip_text(Some(cut_hear::VOLUME_TIP));
        volume.set_draw_value(false);
        volume.set_size_request(120, -1);
        volume.set_halign(gtk::Align::Start);
        volume.set_value(cut_hear::VOLUME_DEFAULT * 100.0);
        let volume_icon = gtk::Image::from_icon_name(cut_screen::VOLUME_ICON);
        volume_icon.set_widget_name("preview-volume-icon");
        volume_icon.set_tooltip_text(Some(cut_hear::VOLUME_TIP));
        let volume_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        volume_row.set_valign(gtk::Align::Center);
        volume_row.append(&volume_icon);
        volume_row.append(&volume);
        volume_row.set_halign(gtk::Align::Start);
        volume_row.set_size_request(-1, 24);
        // Do NOT unparent the slider from this row: `find_widget_by_name("preview-volume")` then returns None and
        // `cut_hear_mix_widgets` panics inside `connect_activate`, aborting the process with SIGABRT.
        volume_group.append(&volume_row);
        put_group(volume_group);
        let rule = gtk::Separator::new(gtk::Orientation::Vertical);
        rule.set_widget_name("toolbar-rule");
        rule.set_margin_top(2);
        rule.set_margin_bottom(2);
        toolbar.append(&rule);
        // Handlers go on in `build_window` after `set_content`: a handler on a widget outside the realized tree never fires.

        let surface_group = gtk::Box::new(gtk::Orientation::Vertical, 4);
        surface_group.set_widget_name("selection-row");
        // F2.6 S1: the placeholder drag surface is scoped `Surface::Ruler` (ground belonging to no row), which selects
        // the whole timeline's footage (`cut_select::ANY_ROW`).
        let surface = gtk::DrawingArea::new();
        surface.set_widget_name("select-surface");
        // A `DrawingArea` has no natural size and collapses to 0 px under `halign(Start)` unless asked.
        surface.set_size_request(240, 48);
        surface.set_halign(gtk::Align::Start);
        surface.set_tooltip_text(Some(cut_select::SURFACE_TIP));
        surface_group.append(&surface);

        let clear_ = gtk::Button::with_label(cut_select::CLEAR_LABEL);
        clear_.set_widget_name("clear-selection");
        clear_.set_tooltip_text(Some(cut_select::CLEAR_TIP));
        // Greyed at rest; `refresh_selection_readout` sets it from the live band.
        clear_.set_sensitive(false);
        clear_.set_halign(gtk::Align::Start);
        surface_group.append(&clear_);

        let readout = gtk::Label::new(Some(cut_select::READOUT_NONE));
        readout.set_widget_name("selection-readout");
        readout.set_xalign(0.0);
        readout.add_css_class("dim-label");
        surface_group.append(&readout);

        // Verbs start greyed (no selection); `refresh_verb_buttons` sets them on every draw, nudge and clear. Split stays
        // live at rest (it splits at the red line, `verb_buttons_state`). `insert-button` is lit by
        // `refresh_insert_button` once a line or selection exists, and reads "Edit" while a card is held (F2.12 S1/S6).
        let verbs = cut_tool_group("verbs", &cut_screen::VERB_BUTTONS, true);
        put_group(verbs);

        // A MenuButton popover with the six effects §A lists; headless fallback is a flat Box named `effect-menu` with the same buttons.
        let effect_button = gtk::MenuButton::new();
        effect_button.set_widget_name("effect-button");
        effect_button.set_label("\u{271a} Effect");
        effect_button.set_tooltip_text(Some(cut_screen::EFFECT_MENU_TIP));
        let effect_menu = cut_effect_menu(&main_window());
        effect_button.set_popover(Some(&effect_menu));
        let effects = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        effects.set_widget_name("group-effects");
        effects.set_hexpand(false);
        effects.set_halign(gtk::Align::Start);
        effects.append(&effect_button);
        put_group(effects);

        // Sensitivity comes from `cut_screen::history_buttons_enabled` via `refresh_history_buttons`. History starts greyed.
        let history = cut_tool_group("history", &cut_screen::HISTORY_BUTTONS, true);
        put_group(history);
        let zoom = cut_tool_group("zoom", &cut_screen::ZOOM_BUTTONS, false);
        put_group(zoom);

        let (form, insert_footer) =
            cut_form_frame::frame_parts(&cut_form_column().upcast::<gtk::Widget>());
        box_.insert_child_after(&form, Some(&previous));
        previous = form.upcast();

        // F2.12: the insert form's own box, because §A puts an open form INSTEAD of the idle readings; filled by
        // `show_insert_form` from a `cut_insert::FormPlan`.
        let insert_form = gtk::Box::new(gtk::Orientation::Vertical, 4);
        insert_form.set_widget_name("insert-form");
        insert_form.set_visible(false);
        box_.insert_child_after(&insert_form, Some(&previous));
        // Under its own form: inside the scrolling body the buttons fell below the painted frame (§05#1).
        box_.insert_child_after(
            &insert_footer,
            Some(&insert_form.clone().upcast::<gtk::Widget>()),
        );
        previous = insert_footer.upcast();

        // F3.1 S4: the zoom's form, hidden until a drag lands a box; filled by `show_zoom_form` from `fx_zoom::Form`.
        let zoom_form = gtk::Box::new(gtk::Orientation::Vertical, 4);
        // Tab-scoped name: `page_box` runs once per tab, so an unscoped name exists four times and `find_widget_by_name`
        // returns a hidden tab's box (`gtk_box_append: assertion 'gtk_widget_get_parent (child) == NULL' failed`).
        // Same for the other `*-form` holders, `fold-badges` and `camera-rows`.
        zoom_form.set_widget_name(&format!("zoom-form-{page}"));
        zoom_form.set_visible(false);
        box_.insert_child_after(&zoom_form, Some(&insert_form.upcast::<gtk::Widget>()));

        // F3.3 Speed form holder: empty and hidden until ⏩ Speed finds a band or a line; filled by `show_speed_form`.
        let speed_form = gtk::Box::new(gtk::Orientation::Vertical, 4);
        speed_form.set_widget_name(&format!("speed-form-{page}"));
        speed_form.set_visible(false);
        box_.insert_child_after(&speed_form, Some(&zoom_form.upcast::<gtk::Widget>()));

        // F3.4 Text form holder: hidden until ❝ Text is armed and a click or drag lands a box; filled by `show_text_form`.
        let text_form = gtk::Box::new(gtk::Orientation::Vertical, 4);
        text_form.set_widget_name(&format!("text-form-{page}"));
        text_form.set_visible(false);
        box_.insert_child_after(&text_form, Some(&speed_form.upcast::<gtk::Widget>()));
        // F3.5 drawing form holder: filled by `show_svg_form`.
        let svg_form = gtk::Box::new(gtk::Orientation::Vertical, 4);
        svg_form.set_widget_name(&format!("svg-form-{page}"));
        svg_form.set_visible(false);
        box_.insert_child_after(&svg_form, Some(&text_form.upcast::<gtk::Widget>()));
        // F3.6 Volume form holder: filled by `show_volume_form`.
        let volume_form = gtk::Box::new(gtk::Orientation::Vertical, 4);
        volume_form.set_widget_name(&format!("volume-form-{page}"));
        volume_form.set_visible(false);
        box_.insert_child_after(&volume_form, Some(&svg_form.upcast::<gtk::Widget>()));
        // F3.7 Label form holder: filled by `show_label_form`.
        let label_form = gtk::Box::new(gtk::Orientation::Vertical, 4);
        label_form.set_widget_name(&format!("label-form-{page}"));
        label_form.set_visible(false);
        box_.insert_child_after(&label_form, Some(&volume_form.upcast::<gtk::Widget>()));

        let strip = gtk::DrawingArea::new();
        strip.set_widget_name("track-strip");
        strip.set_size_request(STRIP_WIDTH, STRIP_HEIGHT);
        strip.set_halign(gtk::Align::Start);
        // `set_visible(false)` would remove the strip from the layout, so §1's "every part has pixels" check finds no
        // box and its gestures have no allocation. It stays in the layout at its size request and never draws.
        strip.set_opacity(0.0);
        strip.set_tooltip_text(Some(&format!(
            "{} / {}",
            cut_trim::TRIM_TIP,
            cut_trim::MOVE_TIP
        )));
        box_.insert_child_after(&surface_group, Some(&previous));
        previous = surface_group.upcast();
        // §05-cut#1-screen: `cut_tracks::install` builds the band, the red playhead layer and the scrollbar; draw
        // callbacks outlive this call, hence the clone.
        let held = window.clone();
        // Frames come from the folder the WINDOW works on, or in a headless shot the bench's fixture (`cut_picture::snapshot_frame_tree`).
        previous = cut_tracks::install(
            window,
            &box_,
            &previous,
            move || crate::ui::cut_view::band_scene_for(&held),
            || crate::ui::cut_picture::snapshot_frame_tree().or_else(|| session_reads().0),
        )
        .upcast();
        box_.insert_child_after(&strip, Some(&previous));

        // F2.10: a named row list under the strip's ruler (`spec/img/05-rows.png`), one row per coloured row from
        // `timeline::row_count`. F2.8's `track-strip` geometry is left exactly as drawn, since its widget test fires
        // into those press bands. Rebuilt by `refresh_camera_rows`.
        let cam_rows = gtk::Box::new(gtk::Orientation::Vertical, 2);
        cam_rows.set_widget_name("camera-rows");
        cam_rows.set_tooltip_text(Some(
            "one row per camera \u{2014} click its name plate to watch that row in the preview",
        ));
        box_.insert_child_after(&cam_rows, Some(&previous));
        // Not filled here: the window is not in the widget tree yet; `build_window` calls `refresh_camera_rows` after `set_content`.

        // §06-effects#2: preview frame, then the effects lane (`spec/img/06-lane.png`), built empty by
        // `cut_preview_panel::install`; `refresh_effects_lane` fills them.
        let _ = crate::ui::cut_preview_panel::install(&box_, &previous);
        // Re-order the widgets just built into §A/§1's order (preview | readings, toolbar, tracks, scrollbar).
        crate::ui::cut_page_layout::arrange(&box_, window);
    }

    view.set_content(Some(&page_scroller));
    // The page's half of `main-divider` is this `AdwToolbarView`, so the §1 expand must be set HERE, not on the inner scroller.
    view.set_vexpand(true);
    view.set_vexpand_set(true);
    // Only Prepare hands its widgets back; `build_window` wires them directly, since `find_widget_by_name` would reach an arbitrary page's copy.
    let (add_, copy_) = prepare.map_or((None, None), |(add_, copy_)| (Some(add_), Some(copy_)));
    (view.upcast(), add_, copy_)
}

/// Installs [`cut_screen::toolbar_css`] once per process, like `produce_page` and `narrate_page`: the button padding is what overflows §1's floor.
fn install_toolbar_css_once() {
    static LOADED: std::sync::Once = std::sync::Once::new();
    LOADED.call_once(|| {
        let provider = gtk::CssProvider::new();
        provider.load_from_string(&cut_screen::toolbar_css());
        gtk::style_context_add_provider_for_display(
            &gtk::gdk::Display::default().expect("a display to style"),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });
}

/// One toolbar group: a linked Box named `group-<name>` with one button per entry of `tools`, each carrying its stable name and tooltip.
/// `rests_insensitive`: the page is shown before the first refresh, so groups acting on nothing start greyed.
pub(crate) fn cut_tool_group(
    name: &str,
    tools: &[cut_screen::Tool],
    rests_insensitive: bool,
) -> gtk::Box {
    let group = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    group.set_widget_name(&format!("group-{name}"));
    // Only `group-zoom`, the last group, expands and absorbs the slack; the rest are sized by their buttons. An even split
    // of the bar left Copy/Paste/Insert/Lane past their viewport. Fill, not `halign(Start)`, so no dead plate trails the bar.
    // The scroller inside must NOT be `hexpand_set`: GTK sizes a non-expanding scrollable as min = natural = its parent's allocation,
    // and the Cut page box is as wide as the TAPE.
    let absorbs_slack = name == "zoom";
    group.set_halign(if absorbs_slack {
        gtk::Align::Fill
    } else {
        gtk::Align::Start
    });
    group.set_hexpand(absorbs_slack);
    // The controls sit in an inner row that scrolls when the bar is narrower than the group; a bare `gtk::Box` would
    // PIN the window (tests/suite/window_shrinks.rs). The outer box keeps the name `group-<name>`.
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    row.set_widget_name(&format!("group-{name}-row"));
    row.add_css_class("linked");
    for tool in tools {
        let button = gtk::Button::with_label(tool.label);
        if let Some(icon) = cut_screen::tool_icon(tool.name) {
            button.set_icon_name(icon);
        }
        button.set_widget_name(tool.name);
        button.set_tooltip_text(Some(tool.tip));
        button.set_sensitive(!rests_insensitive);
        row.append(&button);
    }
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
    scroller.set_propagate_natural_width(true);
    // No width floor (`set_min_content_width`, or `set_size_request` on the group): the Cut page box is as wide as
    // the TAPE, so any minimum the bar reports is charged to the window and moved its floor from §1's 1240 to ~1800
    // (`tests/suite/window_shrinks.rs`). Minimum 0 plus `propagate_natural_width` sizes a group by its buttons; a
    // narrower share gets §A's per-group scroll thumb (`tests/suite/sec_05_cut_1_screen_toolbar_fits.rs`).
    scroller.set_child(Some(&row));
    group.append(&scroller);
    group
}

/// The effects dropdown's contents: the six effects §1 item 17 lists as named rows, as a Box of buttons since
/// a menu model cannot be inspected by name under `GSK_RENDERER=cairo`.
pub(crate) fn cut_effect_menu(window: &adw::ApplicationWindow) -> gtk::Popover {
    let popover = gtk::Popover::new();
    // The first row is the menu's name (`spec/img/06-effect-menu.png`: `✚ Effect ✓` above the six entries).
    let shell = gtk::Box::new(gtk::Orientation::Vertical, 0);
    shell.set_widget_name("effect-menu-popover");
    // A Label, not a Button: the header names the menu and answers nothing.
    let title = gtk::Label::new(Some(cut_screen::EFFECT_MENU_TITLE));
    title.set_widget_name("effect-menu-title");
    title.set_halign(gtk::Align::Start);
    title.set_margin_start(8);
    title.set_margin_top(4);
    title.set_margin_bottom(2);
    shell.append(&title);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list.set_widget_name("effect-menu");
    for item in cut_screen::EFFECT_ITEMS {
        let row = gtk::Button::with_label(item.label);
        row.set_widget_name(item.name);
        row.set_tooltip_text(Some(&format!("add a {} effect", item.tip)));
        let owner = window.clone();
        let kind_id = item.tip.to_string();
        row.connect_clicked(move |row| {
            // Close on a pick: left open, the modal popover swallows the drag a picked ⊕ Zoom or ❝ Text waits for.
            if let Some(popover) = row
                .ancestor(gtk::Popover::static_type())
                .and_then(|w| w.downcast::<gtk::Popover>().ok())
            {
                popover.popdown();
            }
            let Some(kind) = cut::Fx::parse_kind(&kind_id) else {
                return;
            };
            let say = press_effect_item(&owner, kind);
            if let Some(status) = find_widget_by_name(owner.upcast_ref(), "status-line")
                .and_then(|w| w.downcast::<gtk::Label>().ok())
            {
                status.set_text(&say);
            }
        });
        list.append(&row);
    }
    shell.append(&list);
    popover.set_child(Some(&shell));
    popover
}

/// §4's list of rows, one per source file; the row rules are in `source_rows`.
pub(crate) fn sources_list(session: &Rc<RefCell<Project>>, status: &gtk::Label) -> gtk::Widget {
    let list = gtk::ListBox::new();
    list.set_widget_name("sources-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    list.set_tooltip_text(Some(sources::LIST_TIP));
    list.add_css_class("boxed-list");
    source_rows::append_all(&source_rows::Rows {
        session: Rc::clone(session),
        status: status.clone(),
        list: list.clone(),
    });
    list.upcast()
}

/// The bench's scroller, sized for the column (the view keeps its name `bench-text` inside it).
fn editor_scroller(scroller: &gtk::ScrolledWindow) -> gtk::Widget {
    // Fill the column (§1) with no `max_content_height` or `propagate_natural_height`: a cap stopped the box growing, so the
    // bench ended ~460 px down while the page band ran on. `BENCH_MIN_H` (from `bench_box`) is the only height rule.
    scroller.set_vexpand(true);
    scroller.set_vexpand_set(true);
    // An `Automatic` hscroll policy makes the minimum WIDTH zero, so a narrow Paned half sheds width and the bench goes unreadably thin.
    scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroller.set_hexpand(true);
    scroller.set_hexpand_set(true);
    scroller.clone().upcast::<gtk::Widget>()
}
