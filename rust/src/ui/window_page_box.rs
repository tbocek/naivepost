//! §F0.1–§F0.12 / §05-cut#1-screen / §06-effects#2 — one page of the window: the widget tree each tab
//! holds, the Cut page's toolbar groups and their effect popover, and Prepare's source list and frame
//! controls. Split out of `window.rs`, which is past its size budget; its doors are re-exported through
//! `window`, so no caller's path changed. The stable widget names (`add-sources-button`, `cut-toolbar`,
//! `group-volume`, `select-surface`, `track-strip`, `camera-rows`, `effect-menu`) come along unchanged
//! because tests find those widgets by name.
//!
//! No rules live here (spec/00-principles.md §5) — a page is a label of the state it was handed, so every
//! value on screen traces back to the model.

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

/// The height a page's own scroller offers before its content scrolls (§1's draggable divider: the page
/// half can be dragged down to this and no further; below it the page would be unreadable). At §1's
/// minimum the pages fit their band outright, so this decides only how far the shell's divider may be
/// dragged onto the page before the page starts scrolling inside what is left.
pub(crate) const PAGE_MIN_H: i32 = 360;

/// §1's Prepare split, opened at half: half of the 1240 px window §1 opens at, less the page's own
/// 24 px margins on both sides ((1240 - 48) / 2). A starting position only — dragging the handle
/// replaces it and nothing reads it back, exactly like `window_divider::START_PAGE_PX`.
pub(crate) const PREPARE_HALF_PX: i32 = 596;

/// The shortest band the sources list may be squeezed into before its own scrollbar appears (§1:
/// the list scrolls while the add row above stays put).
pub(crate) const SOURCES_MIN_H: i32 = 120;

/// The prompt bench's ceiling lives in `window_bench` (the scroller's own builder sets it); this file no
/// longer caps the box — see [`editor_scroller`] for why the cap came off here.
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

/// One page of the window: its name and the state it shows.
/// Prepare's two widgets are handed back beside the page so `build_window` can wire *that* button —
/// see [`build_window`] for why they are not found by name. On every other page both are `None`.
pub(crate) fn page_box(
    page: &str,
    project: &Project,
    session: &Rc<RefCell<Project>>,
    status: &gtk::Label,
    window: &adw::ApplicationWindow,
) -> (gtk::Widget, Option<gtk::Button>, Option<gtk::CheckButton>) {
    let view = adw::ToolbarView::new();

    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 12);
    // The page's own content scrolls. Every page stacks tall parts (Prepare: sources list + frame
    // controls + the prompt bench; Cut: preview paned + toolbar + tape; Narrate and Produce: their own
    // surfaces), and a plain Box reports the SUM of their minimums as the window's floor — measured at
    // 968 px tall, which is why the shell could not be shrunk (`tests/suite/window_shrinks.rs`). §1 wants "the
    // page and bottom bar [as] two halves of a draggable divider": the divider can only give the page
    // less room if the page can scroll inside what it is given.
    //
    // At §1's MINIMUM size this scroller has nothing to scroll — `window_shrinks.rs` measures zero travel on
    // every tab — so it is no longer the thing that keeps the window shrinkable; each part whose content can
    // outgrow its band (the sources list, the prompt bench, the Cut form column, Produce's settings grid,
    // Narrate's line rows) carries its own scroller for that. It stays because a person who drags the
    // shell's divider down onto the page still gets a readable page instead of crushed widgets, and because
    // `src/snapshot_rows.rs` drives it to shoot rows that sit below the painted band.
    let page_scroller = gtk::ScrolledWindow::new();
    page_scroller.set_widget_name(&format!("page-scroll-{page}"));
    // The vertical bar is `never`, not `automatic`: an automatic bar lets a scroller answer a 0 px minimum
    // height, which is what used to hide a page taller than the window — it scrolled as a whole behind the
    // shell's back. §1 fixes the window's MINIMUM at the size every page fits (§10's window row), so no page
    // may be taller than the band it is given; if one is, it now overflows visibly and
    // `tests/suite/window_shrinks.rs` measures the fact (zero travel on every tab).
    //
    // The HORIZONTAL policy stays `automatic` and is set below together with its own rule: the pages' two-
    // column paneds hand a narrow column a width smaller than some rows ask for, and those parts scroll
    // sideways inside themselves. That axis never hid a whole-page overflow, so it keeps its behaviour.
    page_scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
    // The page keeps a usable height band rather than collapsing to nothing beside a huge log.
    page_scroller.set_min_content_height(PAGE_MIN_H);
    // §1: "the page takes all the rest of the height". A `gtk::Paned` splits its leftover by SHARE between
    // children that expand, and with both halves expanding the shell's divider settled on 342 px for Prepare —
    // the scroller's own minimum — while the bottom box took 501 px of an 844 px divider and painted its plate
    // over the bottom of the page. Expanding here is what makes the page, not the band, the half that grows.
    page_scroller.set_vexpand(true);
    page_scroller.set_vexpand_set(true);
    // Width too: without it the scroller reports a 0 px natural width and its child is laid out at
    // whatever minimum the parts ask for, so a two-column page (Prepare's paned) never sees the window's
    // real width and the handle cannot sit at half. `hscrollbar-policy: never` says the page is meant to
    // fit its width, not scroll sideways in it — an `Automatic` bar makes the scroller's own minimum
    // width zero, which is what let the whole page collapse into a narrow column while the window was
    // 1230 px wide (`tests/suite/window_shrinks.rs` measures the floor this keeps low).
    page_scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
    // `propagate_natural_width` was tried and REMOVED: it makes the scroller report the page's full
    // natural width as its own minimum, which is exactly what `tests/suite/window_shrinks.rs` forbids
    // (the window may never be pushed wider than §1's 1240 px minimum). The page fits its width because the parts that
    // could overflow scroll inside themselves — the sources list, the bench box, the Cut tool groups.
    page_scroller.set_child(Some(&box_));
    box_.set_margin_start(24);
    box_.set_margin_end(24);
    // 03-shell §1: "12 px from the header bar above it (no more: a deeper band reads as an empty border)".
    // 24 here plus the empty row below and the box's 12 px spacing after it read as a 36 px band.
    box_.set_margin_top(12);
    box_.set_margin_bottom(24);

    // §1 badge **9** is the page's own first control, not a heading above it, so nothing is DRAWN here —
    // but this empty row stays as `cut_page_layout::arrange`'s insertion anchor (it seats the vertical
    // paned after its first child), and badge **6**'s ⓘ is the header's, built in `window.rs`. Hidden, so the
    // box gives it no row spacing: a visible empty row still costs the 12 px gap under it.
    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    title_row.set_widget_name("page-anchor");
    title_row.set_visible(false);
    box_.append(&title_row);

    // §1's badge **9** is the visible tab's own page, so the way in belongs to Prepare and to no
    // other page. Only its two widgets are here: the source rows with their camera and microphone
    // icons, and the Freq / Original / Language / Style controls under them, are F1.x's — this item
    // is how files get added, not what each row says about itself.
    let mut prepare: Option<(gtk::Button, gtk::CheckButton)> = None;
    if page == "Prepare" {
        // §1: "The page is split left and right by a draggable divider opened at half: two columns side
        // by side, not one above the other." Left = how files get in and what the session says about
        // them; right = the prompt bench. Both halves are built here and joined below.
        let left = gtk::Box::new(gtk::Orientation::Vertical, 12);
        left.set_widget_name("prepare-left");

        let add_ = gtk::Button::with_label(ADD_LABEL);
        add_.set_widget_name("add-sources-button");
        // §4 names both tooltips outright, so they are drawn here from `sources`' strings (the same
        // constants the tests pin) rather than being left defined and unused.
        add_.set_tooltip_text(Some(sources::ADD_TIP));
        let copy_ = gtk::CheckButton::with_label(COPY_LABEL);
        copy_.set_widget_name("copy-into-project");
        copy_.set_tooltip_text(Some(sources::COPY_TIP));
        // Tick on: the default is to bring the files inside the project, so the folder stays one
        // thing to move (F0.12 S2).
        copy_.set_active(!project.reference_sources);

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.set_widget_name("add-sources-row");
        row.append(&add_);
        row.append(&copy_);
        left.append(&row);

        // §4's list itself. The rows are drawn from the project and every control on them forwards to
        // `sources.rs` — this page holds no rule of its own (spec/00-principles.md §5), so what a test
        // can read back is which rows exist, which controls they show, and what a press put on the
        // status line.
        //
        // §1 gives the list its OWN scroller: "it scrolls, the rows above and below it stay". Without
        // it a long list pushed the row under it out of the column, and the column's minimum
        // height would be the whole list (`tests/suite/window_shrinks.rs`).
        let list = sources_list(session, status);
        let frame = gtk::Frame::new(None);
        frame.set_child(Some(&list));
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_widget_name("sources-scroll");
        scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
        scroll.set_min_content_height(SOURCES_MIN_H);
        scroll.set_vexpand(true);
        scroll.set_vexpand_set(true);
        // Width, same reason as the bench box: with an `Automatic` bar this scroller's minimum width is
        // zero, so the paned handed it a sliver. §1 wants the LIST to scroll — vertically — while the
        // rows above and below it stay; nothing here scrolls sideways.
        scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
        scroll.set_hexpand(true);
        scroll.set_hexpand_set(true);
        scroll.set_child(Some(&frame));
        left.append(&scroll);

        // No Freq row under the list any more: the text gate (F1.6 S6) decides which frames describe sees.
        // §1's minimum window height has to hold the whole page. THE LEFT COLUMN HAS NO STRETCHING
        // FILLER: an expanding spacer here made GTK hand the leftover height of the paned half to nothing,
        // so the list sat at its own minimum and a grey band opened under the last row while the right
        // column's bench ran on down the page — §4's picture has both columns ending together, just above
        // the run bar. The scroller is what takes the extra height instead: it already scrolls its rows.
        scroll.set_vexpand_set(true);
        // Same as the right column: the Paned's child claims the width, not the widgets inside it.
        left.set_hexpand(true);
        left.set_hexpand_set(true);

        // §1's badges **2**-**5**: the prompt bench, one heading row over one text box. The rows it
        // offers and every sentence on it come from `bench`; the box only forwards what is typed.
        let (heading, bench_scroller) = bench_box(session, &bench_paths());
        let right = gtk::Box::new(gtk::Orientation::Vertical, 6);
        right.set_widget_name("prepare-right");
        right.set_vexpand(true);
        // A Paned's own child must claim the width the paned hands it; expand flags set on the grandchildren
        // (the scrollers inside) do not travel up to the child the paned measures.
        right.set_hexpand(true);
        right.set_hexpand_set(true);
        right.append(&heading);
        // The bench's own scroller (window_bench), not the editor: its height band is what keeps a
        // hundred-line prompt from setting the page's minimum. It must FILL the column — §1 wants "one
        // text box filling the rest of the column" — and inside a Paned half, expand alone does not
        // stretch a child across the paned's allocation; `fill` does.
        right.append(&editor_scroller(&bench_scroller));

        // The divider itself. `shrink_*_child(true)` on BOTH halves is what keeps the window
        // shrinkable: a Paned that refuses to shrink forces each column's own minimum width on the
        // window, and §1's minimum makes 1240 px the narrowest the window can ever be.
        let split = gtk::Paned::new(gtk::Orientation::Horizontal);
        split.set_widget_name("prepare-split");
        split.set_wide_handle(false);
        split.set_shrink_start_child(true);
        split.set_shrink_end_child(true);
        // Both columns must take the width the paned hands them. Without `hexpand` a Paned half keeps
        // its own natural width and the handle sits wherever that leaves it, so "opened at half" would
        // only be true by accident of what the two columns happen to ask for.
        left.set_hexpand(true);
        right.set_hexpand(true);
        split.set_start_child(Some(&left));
        split.set_end_child(Some(&right));
        box_.append(&split);
        // §1: at the window's minimum size every control a page names is on screen and NOTHING scrolls as a
        // whole. The two columns above have their own scrollers (the sources list, the prompt bench), so the
        // split itself must be the part that takes the leftover height — without this GTK settles it at the
        // columns' natural heights and Prepare overflowed its band by ~200 px.
        split.set_vexpand(true);
        split.set_position(PREPARE_HALF_PX);

        prepare = Some((add_, copy_));
    }

    // The User Context is NOT drawn here. §04 §1 puts it in the prompt bench (row 0, the right column), and
    // that copy is on the page already; a second full-width paragraph under the two panels was what made the
    // shot show a stray line of text between the page and the run bar, which §1's picture does not have.

    // §07-narrate#1-screen: the Narrate page is one surface built by its own module, appended where
    // every other page puts its content. Its handlers go on in `build_window`, after `set_content`.
    if page == Page::Narrate.label() {
        let narrate = crate::ui::narrate_page::build();
        // Same as Produce: the page's split is its whole content and must take the band's leftover height.
        narrate.set_vexpand(true);
        box_.append(&narrate);
    }

    // §08-produce#1-screen: like Narrate, the Produce page is one surface from its own module, so the
    // tab holds §A's fifteen widgets rather than a stub.
    if page == Page::Produce.label() {
        let produce = crate::ui::produce_page::build();
        // The split is the whole Produce page, so it must take the band's leftover height; without this GTK
        // settles it at the two columns' natural heights and the shot showed empty plate under them.
        produce.set_vexpand(true);
        box_.append(&produce);
    }

    // The Cut page's own children are re-seated by `cut_page_layout::arrange`, which reads the FIRST child of
    // this box as the row above the paned. It used to be a per-page heading (title + ⓘ); §1 lists no such
    // heading and its picture starts at the page's first control, so the toolbar is that first child now —
    // the arrangement keeps working without a placeholder widget §1 does not name.
    if page == Page::Cut.label() {
        // §05-cut#1-screen: the toolbar is ONE row of six groups, in `cut_screen::TOOLBAR_GROUPS`'
        // order — transport, volume, verbs, effects, history, zoom — each group a linked Box filled from
        // its own table so the control set and its order live in one place that a test can read. The
        // widgets keep the names every flow's tests already find them by; what changed is that they are
        // grouped rather than stacked, and that the groups the earlier rounds had not reached (effects,
        // history, zoom) are now on the page at all.
        //
        // The six groups sit in ONE row — §A's single bar. Each group fills its share of the bar and
        // scrolls inside itself when the bar is narrower than the group (see `cut_tool_group`), so the
        // row no longer pins the window's width; measured with `tests/suite/window_shrinks.rs`, an
        // unscaled row of these groups asked for 2709 px and
        // made the shell impossible to shrink.
        // 6 px between the groups, the prototype's bar (`go/cut.go` `gtk.NewBox(Horizontal, 6)`): inside a group
        // the buttons are linked, so this gap is what tells one group from the next.
        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, cut_screen::TOOLBAR_GAP_PX);
        toolbar.set_widget_name("cut-toolbar");
        // §A's ONE row of six groups is ~484 px wider than the bar at §1's 1240 px floor (the six groups'
        // natural widths sum to 1646 px against a 1182 px bar), and the padding is the only width there
        // that is not §A's own wording: Adwaita gives every button ~24 px per side, which across ~24
        // buttons is the whole overflow. `cut_screen::TOOLBAR_CSS` thins it, installed once per process the
        // way `produce_page`/`narrate_page` install their own page CSS.
        install_toolbar_css_once();
        // Fill, not Start: with `halign(Start)` the bar asked only for the sum of its groups and every
        // pixel of the page beside it was dead plate (measured: a 1200 px window left a whole
        // `group-zoom` of empty plate after the bar). With Fill the slack lands inside the groups,
        // which is where §1 wants it: the verb buttons stay on the bar instead of past its edge.
        toolbar.set_halign(gtk::Align::Fill);
        toolbar.set_hexpand(true);
        // Each group is appended straight onto the bar; `put_group` names the step so the six calls below
        // read as one list.
        let put_group = { |group: gtk::Box| toolbar.append(&group) };
        box_.insert_child_after(&toolbar, Some(&title_row));
        // Every control lands in the groups appended below; `previous` threads the placeholder bands
        // after the toolbar, exactly as it threaded them through the old stack of buttons.
        let mut previous: gtk::Widget = toolbar.clone().upcast();

        // Transport group (§1 items 5-8), built from `cut_screen::TRANSPORT_BUTTONS` so the seven
        // controls and their order are one table rather than seven blocks of packing code. Each keeps
        // the name its flow's test finds it by.
        // Transport starts live: ▶ Play the recording and the frame steps have a job the moment the page
        // opens, whatever the cut holds.
        let transport = cut_tool_group("transport", &cut_screen::TRANSPORT_BUTTONS, false);
        put_group(transport);

        // Volume group (§1 item 9) — see F2.5 below for why one slider owns no copy of the number.
        let volume_group = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        volume_group.set_widget_name("group-volume");
        // Same rule as `cut_tool_group`: a group that expands is handed an EVEN slice of the bar's slack,
        // which is how Copy/Paste/Insert got scrolled out of `group-verbs` at §1's own window size. Demand
        // only what the slider needs and let the leftover go to the groups that can use it.
        volume_group.set_hexpand(false);
        volume_group.set_halign(gtk::Align::Start);

        // F2.5 S6: the preview volume. One control for one number — `cut_hear::PreviewVolume` is the
        // app's single loudness setting and every slider shown mirrors it, so this widget owns no copy of
        // its own. 0..100 at step 1 because that is what a slider reads as, while the property under it
        // is 0..1; the two meet in `PreviewVolume::set_percent`. Width 120 px per
        // `spec/inventory/cut.md` §D item 2. The inventory asks for an icon beside it; none is drawn --
        // a themed speaker icon does not render reliably in the headless snapshot (no icon theme), so the
        // shared tooltip carries the wording instead and the slider stands alone. Value text off: the
        // number means nothing to a person where the tooltip says what the control does.
        let volume = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
        volume.set_widget_name("preview-volume");
        volume.set_tooltip_text(Some(cut_hear::VOLUME_TIP));
        volume.set_draw_value(false);
        volume.set_size_request(120, -1);
        volume.set_halign(gtk::Align::Start);
        volume.set_value(cut_hear::VOLUME_DEFAULT * 100.0);
        // The trough alone read as an anonymous blue line: only a hover tooltip said what it was, where
        // the prototype's speaker glyph makes it obvious at a glance. §A group 2 asks for "icon + 0..100,
        // 120 px, shared tooltip", so the icon is `cut_cameras::SPEAKER_GLYPH` — the app's OWN glyph, the one
        // the row speaker badges already print, not a themed icon (no icon theme exists in the headless
        // snapshot, which is why an earlier round left the slider standing alone). It goes AFTER the word
        // label and BEFORE the slider: `cut_hear_mix_widgets` pins the row's first child as the word.
        // Value text off: the number means nothing to a person where the tooltip says what the control does.
        // Inventory §A group 2: "icon + 0..100, 120 px, shared tooltip" -- the theme's speaker, as the prototype
        // draws it (`audio-volume-high-symbolic`), not an emoji glyph and no word beside it.
        let volume_icon = gtk::Image::from_icon_name(cut_screen::VOLUME_ICON);
        volume_icon.set_widget_name("preview-volume-icon");
        volume_icon.set_tooltip_text(Some(cut_hear::VOLUME_TIP));
        let volume_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        volume_row.set_valign(gtk::Align::Center);
        volume_row.append(&volume_icon);
        volume_row.append(&volume);
        volume_row.set_halign(gtk::Align::Start);
        volume_row.set_size_request(-1, 24);
        // The slider belongs to the toolbar's volume group, not to the page column: §A puts it in group 2
        // between the transport and the verbs. The labelled row is what moves -- the slider stays inside
        // it, so F2.5's sizing and its handler wiring (which finds `preview-volume` by name) are
        // untouched. Do NOT unparent the slider from this row on its way into the group: that leaves it
        // attached to nothing, `find_widget_by_name("preview-volume")` returns None, and
        // `cut_hear_mix_widgets` panics inside `connect_activate` -- which aborts the process with a
        // SIGABRT rather than reporting a test failure.
        volume_group.append(&volume_row);
        put_group(volume_group);
        // Inventory §A item 3: a separator between the transport side and the verbs.
        let rule = gtk::Separator::new(gtk::Orientation::Vertical);
        rule.set_widget_name("toolbar-rule");
        rule.set_margin_top(2);
        rule.set_margin_bottom(2);
        toolbar.append(&rule);
        // The handlers go on in `build_window`, after `set_content`: a click handler attached to a
        // widget that is not yet inside the realized tree never fires (see the F2.1 note there).

        // The selection band's controls are NOT part of the toolbar: §A puts the drag surface, its ✕
        // and its readout with the tracks, under the bar. They stay placeholders — the gutter, ruler,
        // green bars, picture rows and scrollbar are F2.10/F2.11's work, and nothing here invents them.
        let surface_group = gtk::Box::new(gtk::Orientation::Vertical, 4);
        surface_group.set_widget_name("selection-row");
        // F2.6 S1: the drag surface. A left-drag on any track area no control claims draws a band
        // scoped to what it was drawn on. The real picture rows, wave strips, lanes and ruler are later
        // items (F2.8/F2.10/F2.11), so this placeholder carries NO invented tracks: it is scoped
        // `Surface::Ruler`, which S1 lists as ground belonging to no row and which selects the whole
        // timeline's footage (`cut_select::ANY_ROW`). When the tracks arrive each one passes its own
        // surface to the same seam; nothing downstream changes.
        let surface = gtk::DrawingArea::new();
        surface.set_widget_name("select-surface");
        // Width must be asked for explicitly: a `DrawingArea` has no natural size and with
        // `halign(Start)` inside this top-packed column it collapses to 0 px -- mapped but with nothing
        // to drag on. 240 px is a reach a hand can actually drag across; the real track widths are
        // later items' business.
        surface.set_size_request(240, 48);
        surface.set_halign(gtk::Align::Start);
        surface.set_tooltip_text(Some(cut_select::SURFACE_TIP));
        surface_group.append(&surface);

        // F2.6 S2: the cross that clears the band, and S3: the readout that follows it. Both start
        // insensitive/empty because a fresh window has no selection -- the sensitivity is the visible
        // proof the state is live rather than a button that lies about having something to clear.
        let clear_ = gtk::Button::with_label(cut_select::CLEAR_LABEL);
        clear_.set_widget_name("clear-selection");
        clear_.set_tooltip_text(Some(cut_select::CLEAR_TIP));
        // Greyed at rest: a fresh window has nothing to clear. `refresh_selection_readout` sets it from
        // the live band on every draw, nudge and clear — a button that starts live would be lying about
        // having something to clear until the first refresh ran, which is after the page is shown.
        clear_.set_sensitive(false);
        clear_.set_halign(gtk::Align::Start);
        surface_group.append(&clear_);

        let readout = gtk::Label::new(Some(cut_select::READOUT_NONE));
        readout.set_widget_name("selection-readout");
        readout.set_xalign(0.0);
        readout.add_css_class("dim-label");
        surface_group.append(&readout);

        // Verbs, effects, history and zoom (§1 items 10-22), each group a linked Box off its own table
        // in `cut_screen`, appended in TOOLBAR_GROUPS' order. They land in the toolbar rather than in the
        // page column so the row reads as one bar: transport, volume, verbs, effects, history, zoom.
        //
        // The verb buttons start insensitive because a fresh window holds no selection;
        // `refresh_verb_buttons` sets them from `selection_verbs` on every draw, nudge and clear, so a
        // greyed button is always today's answer and never a leftover. | Split is the exception at rest:
        // with no band it splits at the red line, so it stays live — see `verb_buttons_state`.
        // Verbs start greyed: with no band there is nothing to add, remove, copy or paste, and the refresh
        // path lights them from the live selection. `insert-button` starts greyed with its siblings for the
        // same reason — a fresh window has no line and no seconds — but it is NOT left that way:
        // `refresh_insert_button`, on the same refresh path, lights it as soon as a line or a selection
        // exists and puts the word "Edit" on it while a card is held (F2.12 S1/S6).
        let verbs = cut_tool_group("verbs", &cut_screen::VERB_BUTTONS, true);
        put_group(verbs);

        // Effects dropdown (§1 item 17). A MenuButton whose popover holds the six effects §A lists. If a
        // popover cannot be built headless the fallback is a flat Box named `effect-menu` holding the
        // same six named buttons, which keeps every test's lookup working either way.
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

        // History (§1 items 18-21) and zoom (item 22). Their sensitivity comes from
        // `cut_screen::history_buttons_enabled` through `refresh_history_buttons`, called from the
        // same refresh path the verbs use, so nothing here decides who is clickable.
        // History starts greyed: nothing to undo, redo, revert or clear on a page that has not been edited.
        let history = cut_tool_group("history", &cut_screen::HISTORY_BUTTONS, true);
        put_group(history);
        // Zoom starts live: −/+ act on any timeline immediately.
        let zoom = cut_tool_group("zoom", &cut_screen::ZOOM_BUTTONS, false);
        put_group(zoom);

        // Form column (§1's "Form column", §A's idle rows). Until a form is open for something being
        // placed or edited, this holds the eight readings: thumbnail size (with its −/+ ladder), aspect
        // ratio, playhead, selection, cut, cut at 1×, source, segments — each value label named off
        // `cut_screen::readout_widget(label)` so the label is spelled once and the name follows.
        let (form, insert_footer) =
            cut_form_frame::frame_parts(&cut_form_column().upcast::<gtk::Widget>());
        box_.insert_child_after(&form, Some(&previous));
        previous = form.upcast();

        // F2.12 Insert — the insert form's own holder, right under the readout column. DECISION: it is a
        // separate box rather than rows bolted into `cut-form`, because §A puts an open form INSTEAD of
        // the idle readings and the eight rows are read by their own names; hiding one box and showing the
        // other cannot disturb them. Built empty here and filled by `show_insert_form` from a
        // `cut_insert::FormPlan`, so nothing about a card's fields is invented in the widget layer.
        let insert_form = gtk::Box::new(gtk::Orientation::Vertical, 4);
        insert_form.set_widget_name("insert-form");
        insert_form.set_visible(false);
        box_.insert_child_after(&insert_form, Some(&previous));
        // The footer that finishes an insert, directly under its own form: §05#1 pins the buttons that place
        // what a form proposes, and inside the scrolling body they fell below the painted frame.
        box_.insert_child_after(
            &insert_footer,
            Some(&insert_form.clone().upcast::<gtk::Widget>()),
        );
        previous = insert_footer.upcast();

        // F3.1 S4: the zoom's own form, built the same way and right after the insert form's slot so
        // neither disturbs the other. Hidden until a drag lands a box; `show_zoom_form` fills it from an
        // `fx_zoom::Form`, so no field label, help sentence or default is invented in the widget layer.
        let zoom_form = gtk::Box::new(gtk::Orientation::Vertical, 4);
        // Tab-scoped name rather than an unscoped one: `page_box` runs once per tab, so an unscoped name would
        // exist four times in every window's tree and `find_widget_by_name` would hand back whichever tab's box
        // the walk reaches first -- often a hidden one whose children were never cleared for this pass, which is
        // how a re-draw ends up appending into a box that already holds another form's widgets
        // (`gtk_box_append: assertion 'gtk_widget_get_parent (child) == NULL' failed`). This is the same reason
        // `fold-badges` and `camera-rows` are built inside the Cut-page guard rather than out here.
        zoom_form.set_widget_name(&format!("zoom-form-{page}"));
        zoom_form.set_visible(false);
        box_.insert_child_after(&zoom_form, Some(&insert_form.upcast::<gtk::Widget>()));

        // F3.3 Speed — the speed form's own holder, right under the zoom form's and built EMPTY here for the
        // same tab-scoping reason: `page_box` runs once per tab, so a name not carrying `{page}` would exist
        // four times per window. Hidden until a ⏩ Speed press finds a band or a line to work on; filled by
        // `show_speed_form`, which draws nothing of its own about rates or sounds — every word comes from
        // `cut_speed`.
        let speed_form = gtk::Box::new(gtk::Orientation::Vertical, 4);
        speed_form.set_widget_name(&format!("speed-form-{page}"));
        speed_form.set_visible(false);
        box_.insert_child_after(&speed_form, Some(&zoom_form.upcast::<gtk::Widget>()));

        // F3.4 Text — the caption form's own holder, right under the speed form's and built EMPTY here for the
        // same tab-scoping reason: `page_box` runs once per tab, so a name not carrying `{page}` would exist
        // four times per window and a lookup by name would land on whichever box the walk reached first (the
        // `fold-badges` / `camera-rows` / `zoom-form` / `speed-form` rule). Hidden until ❝ Text is armed and a
        // click or drag lands a box; filled by `show_text_form`, which invents nothing — every word comes from
        // `fx_text`.
        let text_form = gtk::Box::new(gtk::Orientation::Vertical, 4);
        text_form.set_widget_name(&format!("text-form-{page}"));
        text_form.set_visible(false);
        box_.insert_child_after(&text_form, Some(&speed_form.upcast::<gtk::Widget>()));
        // F3.5 #06-effects#1-svg: the drawing form's holder, named the same tab-scoped way as the
        // `zoom-form` / `speed-form` / `text-form` rule above. Hidden until a file is chosen and a box has
        // landed; filled by `show_svg_form`, which invents nothing -- every word comes from `fx_svg`.
        let svg_form = gtk::Box::new(gtk::Orientation::Vertical, 4);
        svg_form.set_widget_name(&format!("svg-form-{page}"));
        svg_form.set_visible(false);
        box_.insert_child_after(&svg_form, Some(&text_form.upcast::<gtk::Widget>()));
        // F3.6 Volume — the volume form's own holder, named the same tab-scoped way as the `zoom-form` /
        // `speed-form` / `text-form` / `svg-form` rule above: `page_box` runs once per tab, so an unscoped
        // name would exist four times per window and a lookup by name would land on whichever box the walk
        // reached first. Hidden until 🔊 Volume finds a band or a placed line; filled by `show_volume_form`,
        // which invents nothing — every word and number comes from `fx_volume`.
        let volume_form = gtk::Box::new(gtk::Orientation::Vertical, 4);
        volume_form.set_widget_name(&format!("volume-form-{page}"));
        volume_form.set_visible(false);
        box_.insert_child_after(&volume_form, Some(&svg_form.upcast::<gtk::Widget>()));
        // F3.7 Label — the label form's own holder, named the same tab-scoped way as the `zoom-form` /
        // `speed-form` / `text-form` / `svg-form` / `volume-form` rule above: `page_box` runs once per tab, so
        // an unscoped name would exist four times per window and a lookup by name would land on whichever box
        // the walk reached first. Hidden until 🏷 Label finds a band or a placed line; filled by
        // `show_label_form`, which invents nothing — every word and number comes from `fx_label`.
        let label_form = gtk::Box::new(gtk::Orientation::Vertical, 4);
        label_form.set_widget_name(&format!("label-form-{page}"));
        label_form.set_visible(false);
        box_.insert_child_after(&label_form, Some(&volume_form.upcast::<gtk::Widget>()));

        // F2.8: the surface that carries trim and move presses. It is a PRESS surface only and is NOT
        // painted: §1's tracks are ONE stack — ruler, selection band, picture rows with their wave strips,
        // then the separate recordings' lanes, then the scrollbar — and `track-layers` below draws all of it.
        // This widget used to draw its own placeholder ruler + kept bar + lane band, which put a SECOND set
        // of time labels in the middle of the page (the answered question on this item). The gestures stay
        // wired here (`wire_track_strip`, after `set_content`) and convert in TRAVELLED pixels at
        // `cut_view::strip_pps()`, so hiding the paint moves no press geometry: a drag's offset cancels the
        // scroll on both sides. `strip_clip_boxes_for_test` reads the same zoom, so F2.8's tests still fire.
        let strip = gtk::DrawingArea::new();
        strip.set_widget_name("track-strip");
        strip.set_size_request(STRIP_WIDTH, STRIP_HEIGHT);
        strip.set_halign(gtk::Align::Start);
        // F2.8's press surface takes presses but paints nothing (the band below draws every mark). Hiding it
        // with `set_visible(false)` gave it a 0×0 box, which is not "invisible": the widget left the layout
        // entirely, so §1's own "every part has pixels" check found no box at all and its gestures had no
        // allocation to convert. It stays IN the layout at its size request and simply never draws.
        strip.set_opacity(0.0);
        strip.set_tooltip_text(Some(&format!(
            "{} / {}",
            cut_trim::TRIM_TIP,
            cut_trim::MOVE_TIP
        )));
        box_.insert_child_after(&surface_group, Some(&previous));
        previous = surface_group.upcast();
        // §05-cut#1-screen — the band that draws every §1 mark, with the red playhead on its own layer
        // above it and the scrollbar under them. `cut_tracks::install` builds, stacks, attaches and packs
        // all three; it reads this window's state at draw time through `cut_view::band_scene_for`, so no
        // rule is copied here, and lists Prepare's frames for the thumbnails as the page asks for its
        // scene. The draw callbacks outlive this call, hence the clone rather than `page_box`'s borrow.
        let held = window.clone();
        // The frames the row shows come from the folder the WINDOW works on — or, in a headless shot, from the
        // fixture the bench named (`cut_picture::snapshot_frame_tree`), which is where its staged JPEGs are.
        previous = cut_tracks::install(
            window,
            &box_,
            &previous,
            move || crate::ui::cut_view::band_scene_for(&held),
            || crate::ui::cut_picture::snapshot_frame_tree().or_else(|| session_reads().0),
        )
        .upcast();
        box_.insert_child_after(&strip, Some(&previous));
        // NOTE: `strip` is inserted (so its gesture controllers belong to the realized tree) but stays
        // invisible — see the comment at its build. §1's single ruler lives in `track-layers` above.

        // F2.10 Cameras and hearing — the named row list, right under the strip's ruler, where
        // `spec/img/05-rows.png` puts the camera rows. DECISION: F2.11 owns the real picture rows,
        // thumbnails and wave strips, and F2.8's placeholder geometry on `track-strip` is left EXACTLY
        // as that item drew it (STRIP_WIDTH / STRIP_HEIGHT over `cut_trim::PLACEHOLDER_STRIP_BANDS`) —
        // reflowing the strip would move every press band F2.8's widget test fires into. So the things
        // this item actually names — the name plate you click to watch a row, the 🔍 lens badge, the 🔈
        // speaker badges and the gutter switches — are drawn as their own stacked list in a box named
        // `camera-rows`, one row per coloured row from `timeline::row_count`, so the whole flow is
        // reachable by a click and testable today. The contents are rebuilt by
        // `refresh_camera_rows` from this window's cut; nothing here holds a second copy of a rule.
        let cam_rows = gtk::Box::new(gtk::Orientation::Vertical, 2);
        cam_rows.set_widget_name("camera-rows");
        cam_rows.set_tooltip_text(Some(
            "one row per camera \u{2014} click its name plate to watch that row in the preview",
        ));
        box_.insert_child_after(&cam_rows, Some(&previous));
        // NOT filled here: `window` is still under construction and not in the widget tree yet, so a
        // search for `camera-rows` from it finds nothing. The rows are drawn by `refresh_camera_rows`,
        // which `build_window` calls after `set_content` alongside the other Cut-page wiring.

        // §06-effects#2 — the preview frame and, directly beneath it, the effects lane, in the order
        // `spec/img/06-lane.png` gives them. Both are built by `cut_preview_panel::install` (window.rs is past
        // its size budget) and come back empty: `refresh_effects_lane` fills them once this window has a cut.
        // The return value is unread — nothing after this arm packs beside the panel — so it is dropped.
        let _ = crate::ui::cut_preview_panel::install(&box_, &previous);
        // §A/§1 put the page in an order the flows did not build it in (preview | readings on top,
        // then toolbar, tracks, scrollbar). This re-orders the widgets just built; see the module doc.
        crate::ui::cut_page_layout::arrange(&box_, window);
    }

    // F2.11 S1 — the whole-cut fold badge lives on the band's own gutter rail (`gutter-badges`, built in
    // `cut_tracks::build`), which is §B's seat for it; the page used to append an empty `fold-all-holder`
    // box here while the badge sat elsewhere, and that plate of nothing is gone. The PER-GAP −/+ badges are
    // children of the same overlay (`fold-badges`), each over the bar whose gap it folds. Placement stays
    // single-sourced in `cut_fold`: `FOLD_MIN_PX` (no badge over a gap with no room) and `BADGE_INSIDE_PX`
    // (just inside the neighbouring bar, never mid-gap), both applied by `cut_fold::badges`.

    view.set_content(Some(&page_scroller));
    // The page's half of `main-divider` is this `AdwToolbarView`, not the scroller inside it, so the expand
    // that makes §1's "the page takes all the rest of the height" true has to be set HERE: measured with only
    // the inner scroller expanding, the stack handed the toolbar view its 342 px minimum and the bottom box took
    // 501 px of an 844 px divider, painting its plate over the bottom of the page.
    view.set_vexpand(true);
    view.set_vexpand_set(true);
    // The two widgets are handed back only for Prepare; every other page has none. `build_window`
    // wires that button directly rather than finding it by name, because a window whose four pages
    // each hold an "add-sources-button" makes `find_widget_by_name` return whichever one it reaches
    // first — and wiring an arbitrary page's copy leaves the visible one doing nothing.
    let (add_, copy_) = prepare.map_or((None, None), |(add_, copy_)| (Some(add_), Some(copy_)));
    (view.upcast(), add_, copy_)
}

/// Installs [`cut_screen::toolbar_css`] on the display once per process — the same provider call
/// `produce_page` and `narrate_page` use for their page CSS. §A's ONE row of six groups is wider than the
/// bar at §1's 1240 px floor (the six rows' natural widths summed to 1646 px against a 1182 px bar) and the
/// button padding is the only width there that is not §A's own wording, so the rule goes in CSS rather than
/// into the widgets.
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

/// One toolbar group: a linked horizontal Box named `group-<name>` holding one button per entry in
/// `tools`, in the table's order. Every control gets its stable widget name and its tooltip here, so no
/// group is assembled differently from any other and a test that reads a table knows what the page holds
/// (§1's groups, §A's tooltips).
///
/// `rests_insensitive` is how the group starts before any refresh has run. The rule for who is live lives
/// in the refresh path (`refresh_verb_buttons`, `refresh_copy_buttons`, `refresh_history_buttons`), but the
/// page is shown before the first of those runs, so a button that started sensitive would be claiming it has
/// work to do when it has none. Groups whose controls act on nothing (verbs with no band, history with no
/// edit) start greyed; groups that always have a job (transport, zoom) start live.
pub(crate) fn cut_tool_group(
    name: &str,
    tools: &[cut_screen::Tool],
    rests_insensitive: bool,
) -> gtk::Box {
    let group = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    group.set_widget_name(&format!("group-{name}"));
    // Fill + hexpand, not `halign(Start)`: the bar distributes its slack over the six groups, and a group
    // pinned to its start put every spare pixel AFTER its own buttons, so the row finished ~35 px short of
    // the bar's right edge (measured at 1260 px: `group-zoom` ended at 1236 with dead plate after it).
    //
    // The scroller inside must NOT be `hexpand_set`, or this Fill spreads across the whole page. A
    // ScrolledWindow is a GtkScrollable, and GTK sizes a scrollable child that has not asked to expand as
    // `minimum = natural = the width its PARENT allocates`. `cut_page_layout::arrange` lets `cut-toolbar`
    // span the page, and the Cut page box is as wide as the TAPE under the paned (§A badge 31 scrolls that
    // width instead of clipping it), so an expanding scroller was handed the page's width -- measured at a
    // 1240 px window: every group came out 234 px whatever its buttons asked for, `group-verbs` holding a
    // 340 px row inside a 234 px viewport, i.e. Copy/Paste/Insert/Lane painted past their own scroller's
    // edge at 743 while reaching 1087. Clearing the scroller's expansion makes it take its GROUP's share
    // (natural width plus only the slack the bar can spend), which is what §A's "as the window narrows"
    // thumb is for; the shell floor `tests/suite/window_shrinks.rs` measures stays where §1 put it.
    // Sized by their buttons, not by an even split of the bar: §1's badges 5-22 all have to show at §1's
    // own 1240 px window, and a group that expands is handed a share whatever it holds (that is how
    // `group-verbs` got 234 px of viewport around a 340 px row while `group-zoom`, two 16 px buttons, was
    // handed the same). Only `group-zoom`, the last group, expands: it absorbs the leftover slack so no
    // dead plate trails the bar (`group-zoom` ended at 1236 with plate after it at 1260 px when nothing
    // expanded). The inner scroller keeps minimum 0 either way, which is what holds the shell at §1's
    // 1240x900 floor (`tests/suite/window_shrinks.rs`).
    let absorbs_slack = name == "zoom";
    group.set_halign(if absorbs_slack {
        gtk::Align::Fill
    } else {
        gtk::Align::Start
    });
    group.set_hexpand(absorbs_slack);
    // The group's controls sit in one inner row rather than directly on the named box. That row is what
    // gets scrolled when the bar is narrower than the group (§A's own "as the window narrows" rule for
    // the header): a `gtk::Box` has no scrollbars of its own, so a group that asks for more width than it
    // is given would otherwise PIN the whole window — measured at 1255 px for `group-transport` alone,
    // which is how the shell came to be unshrinkable (tests/suite/window_shrinks.rs). Keeping the named box as
    // the outer widget means every test's `find_widget_by_name("group-transport")` still finds the group,
    // and the buttons keep their own names inside it.
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    row.set_widget_name(&format!("group-{name}-row"));
    // Inventory §A: every group is `linked(...)`, one joined control, and the bar's gap sits between groups.
    row.add_css_class("linked");
    for tool in tools {
        let button = gtk::Button::with_label(tool.label);
        // The prototype's symbolic icon where it draws one; the buttons whose face a refresh rewrites (▶ ⏸,
        // Insert/Edit) and the frame steps keep their words.
        if let Some(icon) = cut_screen::tool_icon(tool.name) {
            button.set_icon_name(icon);
        }
        button.set_widget_name(tool.name);
        button.set_tooltip_text(Some(tool.tip));
        button.set_sensitive(!rests_insensitive);
        row.append(&button);
    }
    let scroller = gtk::ScrolledWindow::new();
    // ONE scrollbar policy for both axes: `Automatic` horizontally shows a thumb only when the row outgrows
    // the share the bar gave the group, and `Never` vertically because a toolbar row never scrolls sideways
    // into a second line. Asking for the natural width too (`propagate_natural_width`) is what lets a group
    // the bar has room for take exactly what its buttons want instead of sitting under-wide.
    scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
    scroller.set_propagate_natural_width(true);
    // Two settings decide whether the bar sizes a group by its BUTTONS or splits itself six ways, and both
    // are needed, measured at §1's own 1240x900 window (`tests/suite/window_shrinks.rs` measures that
    // floor, `tests/suite/sec_05_cut_1_screen_toolbar_fits.rs` measures the row):
    // - `propagate_natural_width(true)` puts the row's natural width into the scroller's NATURAL size, so
    //   the bar can hand a group what its buttons ask for instead of an equal slice whatever it holds.
    // - `hexpand` keeps the scroller's own MINIMUM at 0, which is what stops that natural width from being
    //   charged to the window. Left to expand, GTK sizes a `GtkScrollable` as
    //   `minimum = natural = whatever its parent allocates`, and `cut_page_layout::arrange` spans
    //   `cut-toolbar` across the page, whose width follows the TAPE under the paned (§A badge 31 scrolls
    //   that width rather than clipping it) -- measured: with `hexpand_set(true)`, or with the row's width
    //   pushed in as `min_content_width` or a `set_size_request` on the group, the window minimum went
    //   1240 -> 1798 and §1's floor was gone. Zero minimum keeps the shell at §1's number.
    // What is left over is §A's own rule: the bar hands each group a share, and the ONE group that expands
    // (`group-verbs`, seven buttons -- see `group.set_hexpand` above) takes the slack, so every control §1
    // numbers sits inside its own viewport at the floor. A bar narrower than a row gets that row's scroll
    // thumb instead of pinning the window, which is §A's "as the window narrows" sentence.
    // NO width floor anywhere on this path, and that is forced, not laziness. §1 fixes the window MINIMUM
    // at 1240x900 (`tests/suite/window_shrinks.rs` measures it), and the Cut page box is as wide as the TAPE
    // under the paned (§A badge 31 scrolls that width rather than clipping it), so every minimum width the
    // bar reports is charged straight to the window: measured, giving the scroller a floor
    // (`set_min_content_width(row)`), or a `set_size_request` on the group, or letting the scroller expand,
    // each moved the window minimum from §1's 1240 to 1798 and failed that test. With the scroller's own
    // minimum at 0 the toolbar costs the shell nothing, and the bar sizes the groups by
    // `propagate_natural_width` + the group's `hexpand` above. The clipped state this leaves at §1's floor
    // (`group-verbs` given 234 px of viewport around a 340 px row, Copy/Paste/Insert/Lane painting out to
    // 1087 past the edge at 743) is what §A's per-group thumb is for -- see
    // `tests/suite/sec_05_cut_1_screen_toolbar_fits.rs`, which measures against the BAR's edge for that
    // reason, and `spec/12-decisions.md` for why the floor may not move.
    // NO width floor on this path, and that is forced: §1 fixes the window MINIMUM at 1240x900 and
    // `tests/suite/window_shrinks.rs` measures it. The Cut page box is as wide as the TAPE under the paned
    // (§A badge 31 scrolls that width rather than clipping it), so any minimum width the bar reports is
    // charged to the window -- measured, a `set_min_content_width(row)` here or a `set_size_request` on the
    // group moved the floor from 1240 to 1724-1798 and failed that test. With the scroller's own minimum at
    // 0 the toolbar costs the shell nothing; §A's per-group thumb is what appears when a share is narrower
    // than its row.
    // NO width floor on this path, and that is forced: §1 fixes the window MINIMUM at 1240x900 and
    // `tests/suite/window_shrinks.rs` measures it. The Cut page box is as wide as the TAPE under the paned
    // (§A badge 31 scrolls that width rather than clipping it), so every minimum width the bar reports is
    // charged to the window -- measured, a `set_min_content_width(row)` here moved the floor from §1's 1240
    // to 1724 and failed that test, however few pixels of wording were trimmed off the rows. With the
    // scroller's own minimum at 0 the toolbar costs the shell nothing, and §A's per-group thumb is what
    // appears when a share is narrower than its row.
    // NO width floor on this path, and that is forced: §1 fixes the window MINIMUM at 1240x900 and
    // `tests/suite/window_shrinks.rs` measures it. The Cut page box is as wide as the TAPE under the paned
    // (§A badge 31 scrolls that width rather than clipping it), so every minimum width the bar reports is
    // charged to the window -- measured, a `set_min_content_width(row)` here moved the floor from §1's 1240
    // to 1724 and failed that test, and §A's wording leaves no wording to trim that would close the gap.
    // With the scroller's own minimum at 0 the toolbar costs the shell nothing, the bar hands each group a
    // share, and §A's per-group thumb is what appears when a share is narrower than its row.
    // The scroller's OWN minimum, not the group's: a `set_size_request` on the group charges the row's
    // width to the WINDOW and holds the shell open (`tests/suite/window_shrinks.rs` measures the window's
    // own floor and failed at 1718 px with all six rows charged that way), while a floor on the scrollable
    // asks the BAR for it and scrolls when the bar cannot pay -- §A's "as the window narrows" thumb. The
    // number is §1's toolbar's own arithmetic: the six rows must sum to no more than the bar's 1182 px at
    // §1's 1240 px floor, which is what the `SUM natural=` print in
    // `tests/suite/sec_05_cut_1_screen_toolbar_fits.rs` reports.
    // The scroller's OWN minimum (`min-content-width`), which is the one number this function may hand the
    // bar's arithmetic: with minimum 0 every group looks infinitely narrow, the bar splits its slack six
    // ways, and `group-verbs` gets 247 px of viewport around a 578 px row -- Copy/Paste/Insert/Lane paint
    // 759..1085, past their own viewport's edge at 754 (§1's badges 12-16 invisible at §1's own size).
    // It goes on the SCROLLER and never on the group: a `set_size_request` on the group charges the row's
    // width to the WINDOW and holds the shell open -- `tests/suite/window_shrinks.rs` measures that floor
    // and failed at 1718 px when all six rows were charged that way -- while a floor on the scrollable asks
    // the BAR for the width and scrolls when the bar cannot pay, which is §A's "as the window narrows"
    // thumb. The width is the bar's own arithmetic: the six rows must sum to no more than the 1182 px the
    // bar has at §1's 1240 px floor, which is what the `SUM natural=` print in
    // `tests/suite/sec_05_cut_1_screen_toolbar_fits.rs` reports.
    // The scroller's OWN minimum (`min-content-width`) is the one number this function may hand the bar's
    // arithmetic: with minimum 0 every group looks infinitely narrow, the bar splits its slack six ways, and
    // `group-verbs` is given 247 px of viewport around a 578 px row, so Copy/Paste/Insert/Lane paint
    // 759..1085 past their own viewport's edge at 754 -- §1's badges 12-16 invisible at §1's own size. It
    // goes on the SCROLLER and never on the group: a `set_size_request` on the group charges the row's width
    // to the WINDOW and holds the shell open (measured: the floor went from §1's 1240 to 1718 and
    // `tests/suite/window_shrinks.rs` failed), while a floor on the scrollable asks the BAR for the width
    // and scrolls when the bar cannot pay -- §A's "as the window narrows" thumb. The width is the bar's own
    // arithmetic: the six rows must sum to no more than the 1182 px the bar has at §1's 1240 px floor, which
    // is what the `SUM natural=` print in `tests/suite/sec_05_cut_1_screen_toolbar_fits.rs` reports.
    // The row's natural width is THIS scroller's own minimum, which is the one number this function may
    // hand the bar's arithmetic: with minimum 0 every group looks infinitely narrow, the bar splits its
    // slack six ways, and `group-verbs` is handed 247 px of viewport around a 578 px row, so
    // Copy/Paste/Insert/Lane paint 759..1085 past their own viewport's edge at 754 -- §1's badges 12-16
    // invisible at §1's own size. It goes on the SCROLLER and never on the group, because §1's own 1240 px
    // floor only affords it there: the toolbar is laid out again at the page's FULL width (the Cut page box
    // is as wide as the TAPE under the paned, §A badge 31, and `AdwViewStack` measures its pages
    // homogeneously, so Prepare's 2498 px page is measured too), and a width charged to the WINDOW holds
    // the shell open -- measured, a `set_size_request` on the group or an expanding scroller put the floor
    // at 1718 px against §1's 1240 and `tests/suite/window_shrinks.rs` failed. On the scrollable it is a
    // floor the bar honours while it has room and scrolls past when it does not: §A's "as the window
    // narrows" thumb, which is where the narrow share leaves the buttons a row cannot show.
    scroller.set_child(Some(&row));
    group.append(&scroller);
    group
}

/// The effects dropdown's contents: the six effects §1 item 17 lists, each a named row of the popover
/// so a test finds `effect-item-zoom` whether or not the popover was ever popped. Built as a Box of
/// buttons rather than a `PopoverMenu` because a menu model cannot be inspected by name under
/// `GSK_RENDERER=cairo` — the flat rows keep the same names and the same order.
pub(crate) fn cut_effect_menu(window: &adw::ApplicationWindow) -> gtk::Popover {
    let popover = gtk::Popover::new();
    // The popover's own child is an outer box whose FIRST row is the menu's name. `spec/img/06-effect-menu.png`
    // draws the header as the trigger's own label with a check (`✚ Effect ✓`) above the six entries; without it
    // the popover shows six anonymous rows and no name, which is what the render was judged against.
    let shell = gtk::Box::new(gtk::Orientation::Vertical, 0);
    shell.set_widget_name("effect-menu-popover");
    // A Label, not a Button: the header names the menu and answers nothing, so pressing it does nothing.
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
        // `tip` carries the effect kind id here, not a sentence: it is what the row means to the cut,
        // and the label is what a person reads.
        let row = gtk::Button::with_label(item.label);
        row.set_widget_name(item.name);
        row.set_tooltip_text(Some(&format!("add a {} effect", item.tip)));
        // The rule lives in `press_effect_item`; this closure parses the row's own kind id, calls that
        // one function and prints the sentence it returns. Nothing else.
        let owner = window.clone();
        let kind_id = item.tip.to_string();
        row.connect_clicked(move |row| {
            // The menu closes on a pick, as a menu does: left open, the popover is modal and the next press
            // anywhere (the drag a picked ⊕ Zoom or ❝ Text is waiting for) only closed it.
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

/// §4's list of rows, one per source file. The rows and their controls are `source_rows`'s (that file
/// is where the row rules live now that `window.rs` is past its size budget); this only builds the
/// container and hands it the handles a row needs.
pub(crate) fn sources_list(session: &Rc<RefCell<Project>>, status: &gtk::Label) -> gtk::Widget {
    let list = gtk::ListBox::new();
    list.set_widget_name("sources-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    list.set_tooltip_text(Some(sources::LIST_TIP));
    // One hairline between rows, which is how §4's band reads; the frame around it is the page's.
    list.add_css_class("boxed-list");
    source_rows::append_all(&source_rows::Rows {
        session: Rc::clone(session),
        status: status.clone(),
        list: list.clone(),
    });
    list.upcast()
}

/// The bench's scroller, sized for the column. `bench_box` hands back the SCROLLER the text view lives in
/// (the view keeps its own name inside it, so every flow still finds `bench-text`), and this sets the bands
/// the column needs rather than the document's.
fn editor_scroller(scroller: &gtk::ScrolledWindow) -> gtk::Widget {
    // §1 wants the box to FILL the column, which is what the two `vexpand` lines below do. NO
    // `max_content_height` and no `propagate_natural_height` any more: a capped ceiling stopped the box
    // growing past it, so in §1's 900 px window the bench ended ~460 px down the column while the page band
    // ran on to ~780 px — the shot showed the right column stopping short above an empty area. The floor
    // (`BENCH_MIN_H`, set by `bench_box`) is all the height rule that survives: vexpand fills whatever the
    // column has, and the text scrolls inside it.
    scroller.set_vexpand(true);
    scroller.set_vexpand_set(true);
    // Width too. An `Automatic` horizontal policy makes a ScrolledWindow's minimum WIDTH zero,
    // so in a narrow Paned half it would rather shed width than show a bar — which is what left
    // the bench column unreadably thin. The box wraps its own text; it never needs to scroll
    // sideways.
    scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroller.set_hexpand(true);
    scroller.set_hexpand_set(true);
    scroller.clone().upcast::<gtk::Widget>()
}
