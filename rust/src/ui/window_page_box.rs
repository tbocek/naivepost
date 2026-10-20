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
use crate::cut_screen;
use crate::ui::cut_form_frame;
use crate::cut_hear;
use crate::cut_select;
use crate::ui::cut_tracks;
use crate::cut_trim;
use crate::prepare;

/// The height a page's own scroller offers before its content scrolls (§1's draggable divider: the page
/// half can be dragged down to this and no further; below it the page would be unreadable).
pub(crate) const PAGE_MIN_H: i32 = 360;

/// §1's Prepare split, opened at half: half of the 1240 px window §1 opens at, less the page's own
/// 24 px margins on both sides ((1240 - 48) / 2). A starting position only — dragging the handle
/// replaces it and nothing reads it back, exactly like `window_divider::START_PAGE_PX`.
pub(crate) const PREPARE_HALF_PX: i32 = 596;

/// The shortest band the sources list may be squeezed into before its own scrollbar appears (§1:
/// the list scrolls while the add row above and the Freq row below stay put).
pub(crate) const SOURCES_MIN_H: i32 = 120;
use crate::project::Project;
use crate::sources;
use crate::ui::cut_lane_rows::cut_form_column;
use crate::ui::policy_form;
use crate::ui::source_rows;
use crate::ui::window_bench::{bench_box, bench_paths, label};
use crate::ui::window_doors::find_widget_by_name;
use crate::ui::window_policy::{main_window, wire_policy};
use crate::ui::window_project::{ADD_LABEL, COPY_LABEL};
use crate::ui::window_trim_strip::paint_track_strip;
use crate::ui::window::{press_effect_item, session_reads, STRIP_HEIGHT, STRIP_WIDTH};
use crate::shell::Page;

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
    let page_scroller = gtk::ScrolledWindow::new();
    page_scroller.set_widget_name(&format!("page-scroll-{page}"));
    page_scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    // The page keeps a usable height band rather than collapsing to nothing beside a huge log.
    page_scroller.set_min_content_height(PAGE_MIN_H);
    // Width too: without it the scroller reports a 0 px natural width and its child is laid out at
    // whatever minimum the parts ask for, so a two-column page (Prepare's paned) never sees the window's
    // real width and the handle cannot sit at half. `hscrollbar-policy: never` says the page is meant to
    // fit its width, not scroll sideways in it — an `Automatic` bar makes the scroller's own minimum
    // width zero, which is what let the whole page collapse into a narrow column while the window was
    // 1230 px wide (`tests/suite/window_shrinks.rs` measures the floor this keeps low).
    page_scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
    // `propagate_natural_width` was tried and REMOVED: it makes the scroller report the page's full
    // natural width as its own minimum, which is exactly what `tests/suite/window_shrinks.rs` forbids
    // (every page must let the window down to 1024x640). The page fits its width because the parts that
    // could overflow scroll inside themselves — the sources list, the bench box, the Cut tool groups.
    page_scroller.set_child(Some(&box_));
    box_.set_margin_start(24);    box_.set_margin_end(24);
    box_.set_margin_top(24);
    box_.set_margin_bottom(24);

    let title = gtk::Label::new(Some(page));
    title.add_css_class("title-1");
    title.set_halign(gtk::Align::Start);

    // F0.7 S5: each tab's ⓘ opens the policy form, beside the page title where a person looking for
    // "why is the pipeline doing this?" would look. Every page gets one and they all call the same
    // `press_policy`, so the form is reachable from here as well as from the run bar's ⚙ — two ways in,
    // one implementation. The name carries the page because four widgets sharing `tab-info-button` in
    // one window would make a lookup ambiguous (the reason `add-sources-button` is found by walking
    // rather than by name elsewhere).
    //
    // §1 lists ⓘ once at position **6** of the header bar, not once per page, and F0.1 S4 owns keeping
    // its tooltip synced to the visible tab ("current tab's label + help text"). That single header
    // widget is F0.1's round; until it exists this per-page ⓘ is what makes the form reachable from a
    // tab at all. When the header one lands, it calls the same `press_policy` and these become the
    // per-page shortcuts rather than being replaced by a second mechanism.
    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let info = gtk::Button::from_icon_name("help-about-symbolic");
    info.set_widget_name(&format!("tab-info-button-{page}"));
    info.set_tooltip_text(Some(policy_form::OPEN_TIP));
    info.set_valign(gtk::Align::Center);
    wire_policy(&info, window);
    title_row.append(&title);
    title_row.append(&info);
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
        // it a long list pushed the Freq/Language row out of the column, and the column's minimum
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

        // §1's badge **8**: Freq and Language under the list, in the same left column. Both are
        // choices from a list rather than free text — an interval off the stops would be sent to the
        // model as a claim about frames that were never picked.
        left.append(&frame_controls(session));
        // Same as the right column: the Paned's child claims the width, not the widgets inside it.
        left.set_hexpand(true);
        left.set_hexpand_set(true);

        // §1's badges **2**-**5**: the prompt bench, one heading row over one text box. The rows it
        // offers and every sentence on it come from `bench`; the box only forwards what is typed.
        let (heading, editor) = bench_box(session, &bench_paths());
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
        right.append(&editor_scroller(&editor));

        // The divider itself. `shrink_*_child(true)` on BOTH halves is what keeps the window
        // shrinkable: a Paned that refuses to shrink forces each column's own minimum width on the
        // window, and §1 lets the window down to 1024 px wide.
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
        split.set_position(PREPARE_HALF_PX);

        prepare = Some((add_, copy_));
    }

    let context = gtk::Label::new(Some(&project.context));
    context.set_wrap(true);
    context.set_xalign(0.0);
    context.add_css_class("body");
    box_.append(&context);

    // §07-narrate#1-screen: the Narrate page is one surface built by its own module, appended where
    // every other page puts its content. Its handlers go on in `build_window`, after `set_content`.
    if page == Page::Narrate.label() {
        box_.append(&crate::ui::narrate_page::build());
    }

    // §08-produce#1-screen: like Narrate, the Produce page is one surface from its own module, so the
    // tab holds §A's fifteen widgets rather than a stub.
    if page == Page::Produce.label() {
        box_.append(&crate::ui::produce_page::build());
    }


    // F2.1: the Cut page's own transport, first under the title rather than last in the box. The
    // page's content box is top-packed and does not stretch its children, so anything appended after
    // the context paragraph sits below a tall label and reads as an unrelated control; the spec puts
    // the transport at the head of the page, so it goes there.
    if page == Page::Cut.label() {
        // §05-cut#1-screen: the toolbar is ONE row of six groups, in `cut_screen::TOOLBAR_GROUPS`'
        // order — transport, volume, verbs, effects, history, zoom — each group a linked Box filled from
        // its own table so the control set and its order live in one place that a test can read. The
        // widgets keep the names every flow's tests already find them by; what changed is that they are
        // grouped rather than stacked, and that the groups the earlier rounds had not reached (effects,
        // history, zoom) are now on the page at all.
        //
        // The six groups sit in ONE row — §A's single bar. Each group scrolls inside itself when the bar
        // is narrower than the group (see `cut_tool_group`), so the row no longer pins the window's width;
        // measured with `tests/suite/window_shrinks.rs`, an unscaled row of these groups asked for 2709 px and
        // made the shell impossible to shrink.
        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        toolbar.set_widget_name("cut-toolbar");
        toolbar.set_halign(gtk::Align::Start);
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
        // the prototype's speaker glyph makes it obvious at a glance. A row with the word beside it puts
        // that back without a theme dependency, and the 24 px floor keeps it reading as a control next to
        // the ~34 px buttons instead of a hairline under them.
        let volume_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        volume_row.append(&gtk::Label::new(Some(cut_hear::VOLUME_LABEL)));
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
        let form = cut_form_frame::frame(&cut_form_column().upcast::<gtk::Widget>());
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
        box_.insert_child_after(
            &speed_form,
            Some(&zoom_form.upcast::<gtk::Widget>()),
        );

        // F3.4 Text — the caption form's own holder, right under the speed form's and built EMPTY here for the
        // same tab-scoping reason: `page_box` runs once per tab, so a name not carrying `{page}` would exist
        // four times per window and a lookup by name would land on whichever box the walk reached first (the
        // `fold-badges` / `camera-rows` / `zoom-form` / `speed-form` rule). Hidden until ❝ Text is armed and a
        // click or drag lands a box; filled by `show_text_form`, which invents nothing — every word comes from
        // `fx_text`.
        let text_form = gtk::Box::new(gtk::Orientation::Vertical, 4);
        text_form.set_widget_name(&format!("text-form-{page}"));
        text_form.set_visible(false);
        box_.insert_child_after(
            &text_form,
            Some(&speed_form.upcast::<gtk::Widget>()),
        );
        // F3.5 #06-effects#1-svg: the drawing form's holder, named the same tab-scoped way as the
        // `zoom-form` / `speed-form` / `text-form` rule above. Hidden until a file is chosen and a box has
        // landed; filled by `show_svg_form`, which invents nothing -- every word comes from `fx_svg`.
        let svg_form = gtk::Box::new(gtk::Orientation::Vertical, 4);
        svg_form.set_widget_name(&format!("svg-form-{page}"));
        svg_form.set_visible(false);
        box_.insert_child_after(
            &svg_form,
            Some(&text_form.upcast::<gtk::Widget>()),
        );
        // F3.6 Volume — the volume form's own holder, named the same tab-scoped way as the `zoom-form` /
        // `speed-form` / `text-form` / `svg-form` rule above: `page_box` runs once per tab, so an unscoped
        // name would exist four times per window and a lookup by name would land on whichever box the walk
        // reached first. Hidden until 🔊 Volume finds a band or a placed line; filled by `show_volume_form`,
        // which invents nothing — every word and number comes from `fx_volume`.
        let volume_form = gtk::Box::new(gtk::Orientation::Vertical, 4);
        volume_form.set_widget_name(&format!("volume-form-{page}"));
        volume_form.set_visible(false);
        box_.insert_child_after(
            &volume_form,
            Some(&svg_form.upcast::<gtk::Widget>()),
        );
        // F3.7 Label — the label form's own holder, named the same tab-scoped way as the `zoom-form` /
        // `speed-form` / `text-form` / `svg-form` / `volume-form` rule above: `page_box` runs once per tab, so
        // an unscoped name would exist four times per window and a lookup by name would land on whichever box
        // the walk reached first. Hidden until 🏷 Label finds a band or a placed line; filled by
        // `show_label_form`, which invents nothing — every word and number comes from `fx_label`.
        let label_form = gtk::Box::new(gtk::Orientation::Vertical, 4);
        label_form.set_widget_name(&format!("label-form-{page}"));
        label_form.set_visible(false);
        box_.insert_child_after(
            &label_form,
            Some(&volume_form.upcast::<gtk::Widget>()),
        );



        // F2.8: the strip that carries trim and move. A PLACEHOLDER standing in for the picture rows,
        // green bars and wave strips F2.10/F2.11 draw, scoped the way `select-surface` above is — no
        // invented tracks. Two hands on one strip: a press within `cut_trim::EDGE_GRAB_PX` of a clip
        // border drags that border (`// layout.edgeGrabPx`), and a right-press anywhere else moves what
        // is under the pointer. The gestures are attached in `wire_track_strip`, after `set_content`,
        // because a handler on a widget outside the realized tree never fires.
        let strip = gtk::DrawingArea::new();
        strip.set_widget_name("track-strip");
        strip.set_size_request(STRIP_WIDTH, STRIP_HEIGHT);
        strip.set_halign(gtk::Align::Start);
        // The strip paints itself from this window's cut — the ruler, the kept bar and its borders, or
        // `cut_trim::STRIP_EMPTY_HINT` when there is nothing to show. See `paint_track_strip`.
        strip.set_draw_func(move |_, cr, w, h| paint_track_strip(cr, w, h));
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
        previous = cut_tracks::install(
            window,
            &box_,
            &previous,
            move || crate::ui::cut_view::band_scene_for(&held),
            session_reads().0,
        )
        .upcast();
        box_.insert_child_after(&strip, Some(&previous));

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
        crate::ui::cut_page_layout::arrange(&box_);
    }

    // F2.11 S1 — the whole-cut fold badge's holder, beside the camera rows. THE CUT PAGE ONLY:
    // `page_box` runs once per tab (PAGES is four long), and a box named `fold-all-holder` on each of them
    // makes that name ambiguous — every lookup from `fold_badges_box` then lands on whichever page the
    // walk reaches first, which is how one page's refresh appends into another page's tree and trips
    // `gtk_box_append: assertion 'gtk_widget_get_parent (child) == NULL' failed`. Measured: 4 such boxes
    // per window before this guard, 1 after.
    //
    // DECISION (§A names no widget seat for the WHOLE-CUT badge): §B puts the fold-all badge in the 30 px
    // gutter, which today is painted rather than widgetled, so this button takes the nearest real seat the
    // page has — beside the rows, where a person looks for the tape's controls. The PER-GAP −/+ badges are
    // NOT here: they are children of the band's own overlay (`fold-badges`, built in `cut_tracks::build`)
    // so each sits over the bar whose gap it folds. Placement stays single-sourced in `cut_fold`:
    // `FOLD_MIN_PX` (no badge over a gap with no room) and `BADGE_INSIDE_PX` (just inside the neighbouring
    // bar, never mid-gap), both applied by `cut_fold::badges`, whose x each button is placed at.
    if page == Page::Cut.label() {
        let folds_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        folds_box.set_widget_name("fold-all-holder");
        folds_box.set_tooltip_text(Some(
            "\u{2212} folds a stretch the cut drops away \u{00b7} + on the seam opens it again",
        ));
        box_.append(&folds_box);
    }

    view.set_content(Some(&page_scroller));
    // The two widgets are handed back only for Prepare; every other page has none. `build_window`
    // wires that button directly rather than finding it by name, because a window whose four pages
    // each hold an "add-sources-button" makes `find_widget_by_name` return whichever one it reaches
    // first — and wiring an arbitrary page's copy leaves the visible one doing nothing.
    let (add_, copy_) = prepare.map_or((None, None), |(add_, copy_)| (Some(add_), Some(copy_)));
    (view.upcast(), add_, copy_)
}

/// One toolbar group: a linked horizontal Box named `group-<name>` holding one button per entry in
/// `tools`, in the table's order. Every control gets its stable widget name and its tooltip here, so
/// no group is assembled differently from any other and a test that reads a table knows what the page
/// holds (§1's groups, §A's tooltips).
///
/// `rests_insensitive` is how the group starts before any refresh has run. The rule for who is live
/// lives in the refresh path (`refresh_verb_buttons`, `refresh_copy_buttons`,
/// `refresh_history_buttons`), but the page is shown before the first of those runs, so a button that
/// started sensitive would be claiming it has work to do when it has none. Groups whose controls act on
/// nothing (verbs with no band, history with no edit) start greyed; groups that always have a job
/// (transport, zoom) start live.
pub(crate) fn cut_tool_group(name: &str, tools: &[cut_screen::Tool], rests_insensitive: bool) -> gtk::Box {
    let group = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    group.set_widget_name(&format!("group-{name}"));
    group.set_halign(gtk::Align::Start);
    // The group's controls sit in one inner row rather than directly on the named box. That row is what
    // gets scrolled when the bar is narrower than the group (§A's own "as the window narrows" rule for
    // the header): a `gtk::Box` has no scrollbars of its own, so a group that asks for more width than it
    // is given would otherwise PIN the whole window — measured at 1255 px for `group-transport` alone,
    // which is how the shell came to be unshrinkable (tests/suite/window_shrinks.rs). Keeping the named box as
    // the outer widget means every test's `find_widget_by_name("group-transport")` still finds the group,
    // and the buttons keep their own names inside it.
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    row.set_widget_name(&format!("group-{name}-row"));
    for tool in tools {
        let button = gtk::Button::with_label(tool.label);
        button.set_widget_name(tool.name);
        button.set_tooltip_text(Some(tool.tip));
        button.set_sensitive(!rests_insensitive);
        row.append(&button);
    }
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
    scroller.set_propagate_natural_width(true);
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
        row.connect_clicked(move |_| {
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
    popover.set_child(Some(&list));
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

/// The bench's text view wrapped in the scroller that gives it a height band. `bench_box` returns the
/// text view itself (every flow finds `bench-text` by name and reads its buffer); this walks up to the
/// `ScrolledWindow` it sits in so the column can size the SCROLLER rather than the document.
fn editor_scroller(editor: &gtk::TextView) -> gtk::Widget {
    let scroller = editor
        .parent()
        .and_then(|scroller| scroller.downcast::<gtk::ScrolledWindow>().ok());
    match scroller {
        Some(scroller) => {
            // §1 wants the box to FILL the column, so the band that bounds it is lifted to "no ceiling"
            // and only its 160 px floor survives: a Paned half gives what it has, and a max height there
            // would leave blank plate under a prompt that has more to show.
            scroller.set_max_content_height(-1);
            scroller.set_vexpand(true);
            scroller.set_vexpand_set(true);
            // Width too. An `Automatic` horizontal policy makes a ScrolledWindow's minimum WIDTH zero,
            // so in a narrow Paned half it would rather shed width than show a bar — which is what left
            // the bench column unreadably thin. The box wraps its own text; it never needs to scroll
            // sideways.
            scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
            scroller.set_hexpand(true);
            scroller.set_hexpand_set(true);
            scroller.upcast::<gtk::Widget>()
        }
        // Built without its scroller (no caller does today): the view alone still belongs on the page.
        None => editor.clone().upcast(),
    }
}

/// §1's badge **8**: the two frame controls under the list. Freq is a dropdown of the stops
/// ([`prepare::FREQ_STOPS`]) and Language an entry with the warning as its tooltip; both write
/// through to the session, so the page holds no rule of its own.
pub(crate) fn frame_controls(session: &Rc<RefCell<Project>>) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.set_widget_name("frame-controls");

    row.append(&label("Freq"));
    let freq = gtk::DropDown::from_strings(
        &prepare::FREQ_STOPS
            .iter()
            .map(|secs| prepare::freq_label(*secs))
            .collect::<Vec<_>>()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
    );
    freq.set_widget_name("freq-control");
    freq.set_selected(
        prepare::FREQ_STOPS
            .iter()
            .position(|stop| *stop == session.borrow().interval)
            .map(|index| index as u32)
            .unwrap_or(gtk::INVALID_LIST_POSITION),
    );
    let held = session.clone();
    freq.connect_selected_notify(move |picker| {
        // `INVALID_LIST_POSITION` is the empty selection GTK gives before a choice exists; anything
        // off the stops is `set_freq`'s refusal to make, not this handler's decision.
        let index = picker.selected();
        if index == gtk::INVALID_LIST_POSITION {
            return;
        }
        let secs = prepare::FREQ_STOPS[usize::try_from(index).unwrap_or(0)];
        prepare::set_freq(&mut held.borrow_mut(), secs);
    });
    row.append(&freq);

    row.append(&label("Language"));
    let language = gtk::Entry::new();
    language.set_widget_name("language-entry");
    language.set_tooltip_text(Some(prepare::LANGUAGE_TIP));
    // The code as it will be used, so an empty box shows the "en" it means rather than a blank.
    language.set_text(&prepare::language_label(&session.borrow().language));
    let held = session.clone();
    language.connect_changed(move |entry| {
        held.borrow_mut().language = entry.text().to_string();
    });
    row.append(&language);

    row.upcast()
}
