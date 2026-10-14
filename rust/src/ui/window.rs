//! The window: renders a [`Project`]'s state and nothing else.
//!
//! No rules live here (spec/00-principles.md §5) — a page is a label of the
//! state it was handed, so every value on screen traces back to the model.

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use adw::prelude::*;
use gtk4 as gtk;

use crate::add_sources;
use crate::bench;
// F3.5 ▨ SVG: the page half of the drawing flow lives in `cut_svg`; these are the doors this shell calls.
// F3.3 ⏩ Speed and stop by hand: the page half of that flow lives in `cut_speed_hand`; these are the
// doors this shell calls.
use crate::ui::cut_speed_hand::{
    close_speed_form, press_speed_item, show_speed_form, speed_form_open, wire_band_esc,
    wire_speed_esc,
};
use crate::ui::cut_volume_hand::{
    close_volume_form, press_volume_item, show_volume_form, volume_form_open, wire_volume_esc,
};
use crate::ui::cut_text_hand::{
    close_text_form, press_aspect, press_text_item, refresh_aspect_readout, show_text_form,
    text_armed, text_drag_ended, text_form_open, wire_text_esc,
};
use crate::ui::cut_label_hand::{
    close_label_form, label_form_open, press_label_item, show_label_form, wire_label_esc,
};
use crate::ui::cut_folds::{
    fold_badges_box, page_recordings, press_fold_all, push_session_recordings, refresh_fold_badges,
};
use crate::ui::cut_cameras::refresh_camera_rows;
use crate::ui::cut_tracks;
use crate::ui::cut_form_frame;
use crate::ui::cut_picture;
use crate::ui::save_apply;
use crate::ui::rescan_apply;
use crate::ui::cut_cameras::hand_preview_back;
// F3.5 ▨ SVG: the page half of the drawing flow lives in `cut_svg`; these are the doors this shell calls.
use crate::ui::cut_svg::{
    close_svg_form, press_svg_item, show_svg_form, svg_armed, svg_drag_ended, svg_form_open,
    wire_svg_esc,
};
use crate::cut::{self, Cut};
use crate::fx_record;
use crate::cut_cards;
use crate::cut_hear;
use crate::fx_aspect;
use crate::fx_band;
use crate::fx_lane;
use crate::fx_zoom;
use crate::cut_insert;
use crate::cut_play;
use crate::cut_play_leg;
use crate::cut_review;
use crate::cut_select;
use crate::cut_delete;
use crate::cut_verbs;
use crate::cut_copy;
use crate::cut_line;
use crate::describe;
use crate::joins;
use crate::retakes;
use crate::cut_screen;
use crate::cut_trim;
use crate::hand_edit;
use crate::layout;
use crate::new_project;
use crate::open_project;
use crate::prepare;
use crate::prepare_run;
use crate::preview::{self, Player, Press};
use crate::project::MarkingPass;
use crate::project::Project;
use crate::rescan;
use crate::lucky;
use crate::run;
use crate::runqueue;
use crate::exchanges;
use crate::save_as;
use crate::policy;
use crate::ui::policy_form;
use crate::ui::settings;
use crate::sources::{self};
use crate::ui::source_rows;
use crate::startup;
use crate::narration::{self, Narration};
use crate::narrate_data;
use crate::narrate_preview;
use crate::narrate_off::{self};
// The strip's kept bars wear the page's own tints, read from the one module that spells them (§6's colours).
use crate::ui::cut_colours::{
    GROUND, INSERT_VIOLET, KEPT_BORDER, KEPT_TINT, KEPT_TINT_ALPHA, RULER_INK,
};
use crate::narrate_run::{self, JOBS, JOB_NARRATION, JOB_SPEAKING, SPEAKING_STATUS, STAGE_THINKING, Speak};
use crate::narrate_screen;
use crate::shell::{self, Move, Outcome, Page, Shell};
use crate::PAGES;

/// §1's badge **15** is a folder button beside the count; its tooltip names what the button opens.
/// The page-specific wording §1 mentions lives in `Shell::outputs`' own doc — the button itself is
/// one control for every page, so it gets one sentence here rather than four near-duplicates.
const OUTPUTS_FOLDER_TIP: &str = "Open this project's output folder";

pub const APP_ID: &str = "ch.bocek.naivepost";

/// One page of the window: its name and the state it shows.
/// Prepare's two widgets are handed back beside the page so `build_window` can wire *that* button —
/// see [`build_window`] for why they are not found by name. On every other page both are `None`.
fn page_box(
    page: &str,
    project: &Project,
    session: &Rc<RefCell<Project>>,
    status: &gtk::Label,
    window: &adw::ApplicationWindow,
) -> (gtk::Widget, Option<gtk::Button>, Option<gtk::CheckButton>) {
    let view = adw::ToolbarView::new();

    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 12);
    box_.set_margin_start(24);
    box_.set_margin_end(24);
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
        let add_ = gtk::Button::with_label(ADD_LABEL);
        add_.set_widget_name("add-sources-button");
        let copy_ = gtk::CheckButton::with_label(COPY_LABEL);
        copy_.set_widget_name("copy-into-project");
        // Tick on: the default is to bring the files inside the project, so the folder stays one
        // thing to move (F0.12 S2).
        copy_.set_active(!project.reference_sources);

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.set_widget_name("add-sources-row");
        row.append(&add_);
        row.append(&copy_);
        box_.append(&row);

        // §4's list itself. The rows are drawn from the project and every control on them forwards to
        // `sources.rs` — this page holds no rule of its own (spec/00-principles.md §5), so what a test
        // can read back is which rows exist, which controls they show, and what a press put on the
        // status line.
        let list = sources_list(&session, &status);
        let frame = gtk::Frame::new(None);
        frame.set_child(Some(&list));
        box_.append(&frame);

        // §1's badge **8**: Freq and Language under the list. Both are choices from a list rather
        // than free text — an interval off the stops would be sent to the model as a claim about
        // frames that were never picked.
        box_.append(&frame_controls(&session));

        // §1's badges **2**-**5**: the prompt bench, one heading row over one text box. The rows it
        // offers and every sentence on it come from `bench`; the box only forwards what is typed.
        let (heading, editor) = bench_box(&session, &bench_paths());
        box_.append(&heading);
        box_.append(&editor);

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
        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        toolbar.set_widget_name("cut-toolbar");
        toolbar.set_halign(gtk::Align::Start);
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
        toolbar.append(&transport);

        // Volume group (§1 item 9) — see F2.5 below for why one slider owns no copy of the number.
        let volume_group = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        volume_group.set_widget_name("group-volume");
        toolbar.append(&volume_group);


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
        toolbar.append(&verbs);

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
        toolbar.append(&effects);

        // History (§1 items 18-21) and zoom (item 22). Their sensitivity comes from
        // `cut_screen::history_buttons_enabled` through `refresh_history_buttons`, called from the
        // same refresh path the verbs use, so nothing here decides who is clickable.
        // History starts greyed: nothing to undo, redo, revert or clear on a page that has not been edited.
        let history = cut_tool_group("history", &cut_screen::HISTORY_BUTTONS, true);
        toolbar.append(&history);
        // Zoom starts live: −/+ act on any timeline immediately.
        let zoom = cut_tool_group("zoom", &cut_screen::ZOOM_BUTTONS, false);
        toolbar.append(&zoom);

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

    view.set_content(Some(&box_));
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
fn cut_tool_group(name: &str, tools: &[cut_screen::Tool], rests_insensitive: bool) -> gtk::Box {
    let group = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    group.set_widget_name(&format!("group-{name}"));
    group.set_halign(gtk::Align::Start);
    for tool in tools {
        let button = gtk::Button::with_label(tool.label);
        button.set_widget_name(tool.name);
        button.set_tooltip_text(Some(tool.tip));
        button.set_sensitive(!rests_insensitive);
        group.append(&button);
    }
    group
}

/// The effects dropdown's contents: the six effects §1 item 17 lists, each a named row of the popover
/// so a test finds `effect-item-zoom` whether or not the popover was ever popped. Built as a Box of
/// buttons rather than a `PopoverMenu` because a menu model cannot be inspected by name under
/// `GSK_RENDERER=cairo` — the flat rows keep the same names and the same order.
fn cut_effect_menu(window: &adw::ApplicationWindow) -> gtk::Popover {
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

/// §06-effects#1-record: record one effect of this kind on this window's cut.
///
/// One call to `fx_record::record_into` (which lays down a record carrying exactly the columns §1 gives
/// the kind), then the same save path `save_insert_cut` uses, then `record_edit` so F2.13's Undo covers
/// the addition — never `seed_review_cut`, which would publish without recording and leave the new bar
/// un-undoable. Returns the sentence the status line shows.
pub fn press_effect_item(window: &adw::ApplicationWindow, kind: cut::EffectKind) -> String {
    // F3.1 S1: a Zoom is NOT recorded here. The dropdown's ⊕ Zoom entry ARMS a drag — the box someone draws is
    // what becomes the zoom, through `press_zoom_item` and then `zoom_drag_ended`. Recording one on the click would
    // put a whole effect on the cut before anyone has said where, which is why this flow was unreachable from the UI.
    // Every other kind still records straight through, unchanged.
    if kind == cut::EffectKind::Zoom {
        return press_zoom_item(window);
    }
    // F3.3 S1: Speed is NOT recorded here either. ⏩ Speed opens a FORM — the band's own seconds, or a stop at
    // the line, with rate / sound / length / two fades / Curve still to be settled. The generic path below lays
    // down a fresh record of `NEW_EFFECT_SECONDS` width and no rate answer, which is why none of `cut_speed`'s
    // S1-S4 flow was reachable from the UI before this branch existed.
    if kind == cut::EffectKind::Speed {
        return press_speed_item(window);
    }
    // F3.4 S1: Text is NOT recorded here either. ❝ Text ARMS a gesture -- the box someone clicks or drags on
    // the picture is what becomes the caption, through `press_text_item` and then `text_drag_ended`. The
    // generic path below would lay down a record before anyone has said where the words go or WHAT they say,
    // which is why none of §F3.4's S1-S3 was reachable from the UI before this branch existed.
    if kind == cut::EffectKind::Text {
        return press_text_item(window);
    }
    // F3.5 S1: SVG is NOT recorded here either. The square-with-a-pen entry asks for the FILE first and only
    // then arms -- the chooser comes before the gesture, because a drawing with no ink cannot be drawn. The
    // generic path below would lay down a record before anyone has chosen a file or drawn a box, which is why
    // none of section F3.5's S1-S4 was reachable from the UI before this branch existed.
    if kind == cut::EffectKind::Svg {
        return press_svg_item(window);
    }
    // F3.6 S1: Volume is NOT recorded here either. 🔊 Volume opens a FORM — the band's own seconds, or two
    // from the line, with the percent still to be given. The generic path below lays down a record of
    // `NEW_EFFECT_SECONDS` width at whatever gain the struct defaults to, which is why none of §F3.6's S2-S5
    // (the 200 % default, the percent field, the 0.1 s floor, "heard even while paused") was reachable from
    // the UI before this branch existed. A volume has no box and nothing to arm: it works on SECONDS.
    if kind == cut::EffectKind::Volume {
        return press_volume_item(window);
    }
    // F3.7 S1: Label is NOT recorded here either. 🏷 Label opens a FORM — the band's own seconds, or two from
    // the line, with the NAME still to be given. The generic path below lays down a `NEW_EFFECT_SECONDS` record
    // with no name at all, and a label without a name is refused (§F3.7: "nothing is placed until then"), which
    // is why none of §F3.7's S2-S5 (the empty-name refusal, the 0.4 s floor, the tag that is drawn but never
    // rendered) was reachable from the UI before this branch existed.
    if kind == cut::EffectKind::Label {
        return press_label_item(window);
    }
    let mut cut_ = review_cut_of(window);
    // The red line, read the way Paste reads it: the live preview's playhead if one is going, else the
    // saved line position. No new position store — the line already has one owner.
    let at = paste_line(window);
    fx_record::record_into(&mut cut_, kind, at, fx_record::NEW_EFFECT_SECONDS);
    save_insert_cut(&cut_);
    record_edit(window, &cut_);
    // Draw the new bar in the same turn, so the lane never lags the thing that was just added.
    refresh_effects_lane(window);
    fx_record::recorded_status(kind)
}

// The §05-cut F3.1 zoom-by-hand arm group lives in `cut_zoom_hand` (window.rs is past its size budget);
// these are the doors the rest of the page and the other ui modules call, kept at the paths they had.
pub use crate::ui::cut_zoom_hand::{
    note_zoom_place, press_zoom_item, preview_picture_px, set_zoom_armed, zoom_armed,
    zoom_drag_ended, zoom_drag_ended_with_source, zoom_entry, zoom_form_box, zoom_form_open,
    PREVIEW_PANEL_H, PREVIEW_PANEL_W,
};
pub(crate) use crate::ui::cut_zoom_hand::{
    preview_panel, set_camera_layer_down, set_zoom_form, show_zoom_form, trim_seconds,
    zoom_field_row, LAST_ZOOM_WINDOW,
};




// The §05-cut effects-lane group lives in `cut_lane_rows` (window.rs is past its size budget); these are
// the doors the rest of the page and the other ui modules call, kept at the paths they had.
pub use crate::ui::cut_lane_rows::{cut_bar_width_px, effects_lane_box, refresh_effects_lane};
pub(crate) use crate::ui::cut_lane_rows::{
    cut_form_column, find_in_holder, preview_widget, set_class,
};


/// §4's list of rows, one per source file. The rows and their controls are `source_rows`'s (that file
/// is where the row rules live now that `window.rs` is past its size budget); this only builds the
/// container and hands it the handles a row needs.
fn sources_list(session: &Rc<RefCell<Project>>, status: &gtk::Label) -> gtk::Widget {
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

/// §1's badge **8**: the two frame controls under the list. Freq is a dropdown of the stops
/// ([`prepare::FREQ_STOPS`]) and Language an entry with the warning as its tooltip; both write
/// through to the session, so the page holds no rule of its own.
fn frame_controls(session: &Rc<RefCell<Project>>) -> gtk::Widget {
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

// The §1 bench, tabs and readouts group lives in `window_bench` (window.rs is past its size budget);
// these are the doors the rest of the page calls, kept at the paths they had.
pub use crate::ui::window_bench::{label, paint_progress, set_bench_open_row};
pub(crate) use crate::ui::window_bench::{
    bench_box, bench_paths, depth, has_label, paint_readouts, paint_readouts_for, paint_run_bar,
    paint_tabs, paint_tabs_for, refresh_cut_readouts, tab_buttons, LOG_OPEN_PX,
};

// The §05-cut selection / held-effect / live-form group lives in `window_selection` (window.rs is past its
// size budget); these are the doors the rest of the page and the other ui modules call, kept at their paths.
pub use crate::ui::window_selection::{
    apply_mix, clear_selection, clear_selection_button, drag_held_band, draw_selection,
    live_form_answer_is_undo, live_form_refused, nudge_held_effect, nudge_selection,
    open_effect_form, press_band_esc, press_edit_effect, press_effect_bar, press_fx_lane,
    preview_playhead, preview_volume, pressed_surface, remove_effect_at, remove_held_effect,
    select_surface, selection, selection_gesture, selection_readout, selection_surface,
    selection_verbs, set_preview_volume,
};
pub(crate) use crate::ui::window_selection::{
    close_any_effect_form, held_narration, placeholder_surface_bands, refresh_selection_readout,
    refresh_verb_buttons, set_pressed_surface, verb_buttons_state, HELD_NARRATION, LIVE_FORM,
    PREVIEW_VOLUMES, SELECT_GESTURES, SELECT_SURFACES, SELECTED,
};

// The §F2.8 trim/move/copy/paste + track-strip group lives in `window_trim_strip` (window.rs is past its
// size budget); these are the doors the rest of the page and the other ui modules call, kept at their paths.
pub use crate::ui::window_trim_strip::{
    copy_esc_controller, copy_hand, delete_key_controller, move_gesture, note_place, open_folds,
    paste_line, press_band, press_copy, press_lane, press_move, press_move_at, press_paste,
    press_position, press_row, press_scene, press_trim_border, right_press_took, set_press_band,
    set_press_scene, set_press_x, track_strip, trim_gesture, TRACK_REC_END, TRACK_STRIP_PPS,
};
pub(crate) use crate::ui::window_trim_strip::{
    draw_strip_labels, now_millis, paint_track_strip, refresh_copy_buttons, trim_boxes_for_strip,
    wire_copy_esc, wire_delete_keys, wire_track_strip,
};

// The §02 run-bar presses live in `window_run_press` and the three pass jobs in `cut_pass_gates`
// (window.rs is past its size budget); these are the doors the rest of the page calls, kept at their paths.
pub use crate::ui::cut_pass_gates::{
    CaptionBatch, captions_pass_button, decorations_pass_button, refresh_captions_gate, refresh_decorations_gate,
    refresh_speeds_gate, run_captions_pass, run_captions_pass_with_reply, speeds_pass_button,
    set_captions_pass, set_decorations_pass, set_speeds_pass,
};
pub use crate::ui::cut_pass_gates::SPEEDS_PASS_LABEL;
pub(crate) use crate::ui::cut_pass_gates::{wire_captions_pass, wire_decorations_pass, wire_speeds_pass};
pub(crate) use crate::ui::window_run_press::{
    wire_lucky, wire_play, wire_play_cut, wire_play_recording, wire_preview_volume, wire_stop,
};

/// The window's switching state and what it decided on arrival. `help_page` is read from the shell
/// by S4's own test; here only what is on screen.
#[derive(Debug, PartialEq)]
pub struct UiState {
    pub page: Page,
    pub status: String,
    /// F0.5 S1: whether the log expander is open — read from the widget, so a test checks the same
    /// thing the user sees rather than a flag that might not have been drawn.
    pub log_expanded: bool,
    /// F0.5 S2: how far the run's progress bar has got, 0 when idle.
    pub progress: f64,
}

/// Build the window showing `project`. `page` selects the visible tab.
pub fn build_window(app: &impl IsA<gtk::Application>, project: &Project, page: &str) -> adw::ApplicationWindow {

    let window = adw::ApplicationWindow::new(app);
    window.set_default_size(1400, 900);
    // §7's launch lines wait until there is a log to put them in.
    note_startup(&[]);

    // Where the window is and what it has to say: every rule behind a switch lives in shell.rs.
    let shell = Rc::new(RefCell::new(start_shell(page)));
    // S3's pending write lives inside this shell; publishing the handle is what lets the seam below
    // mark one owed. The Narrate page will call the same seam from its text view when F4.7 lands one.
    WINDOW_SHELLS.with(|shells| shells.borrow_mut().push(Rc::clone(&shell)));

    // S5 refits the narration lines when the page is *entered*, and S3 writes that refit out when
    // the page is *left* — two switches, so the lines have to survive between them. Reading them
    // afresh on every switch (what `session_reads` does) makes the refit land in a local that is
    // dropped at the end of the handler, and leaving Narrate then re-reads the pre-refit file: the
    // flush would write back what the disk already had. This is the window's copy; F4.7's text view
    // edits it too.
    let held_narration = Rc::new(RefCell::new(session_reads().2));
    HELD_NARRATION.with(|held| held.borrow_mut().push(Rc::clone(&held_narration)));

    // F0.7's record of which User Context this window's policy answers to, pushed beside the other
    // per-window slots so `derive_policy` reaches the newest one. Fresh: a window has derived nothing
    // yet, which is exactly what makes the first ▶ ask.
    POLICY_TRACKERS.with(|trackers| trackers.borrow_mut().push(Rc::new(RefCell::new(policy::Tracker::new()))));
    POLICY_WATCH_WINDOW.with(|watch| *watch.borrow_mut() = Some(window.clone()));

    // The window is handed an immutable `&Project` and holds no live project yet, so the flows that
    // change the session — Rescan (F0.11) and Add sources (F0.12) — work on one private copy shared
    // between them. F0.9's live project state replaces this; until then it is what keeps the session
    // each flow leaves behind for the next. Made before the pages so Prepare's rows can hold a handle
    // to it: a row that moved the session has to move the one copy every other flow reads.
    let session = Rc::new(RefCell::new(project.clone()));
    SESSION.with(|slots| slots.borrow_mut().push(Rc::clone(&session)));
    // F1.1: the ▶ handler asks `prepare_run` about the LIVE session rather than the build-time
    // `project` argument, so a source added or removed since the window opened is what gets asked
    // about. Published here for the same reason `SESSION` is.
    PLAY_SESSION.with(|slots| slots.borrow_mut().push(Rc::clone(&session)));

    // §2 S6's folder for THIS window, remembered before anything can move the cwd: the close hook writes
    // the session here rather than into `project_autosave::session_dir()`, which asks the process cwd at
    // close time — wrong once a flow has moved the session (F0.10's Save as, F0.11's Rescan) or a test has
    // pinned its cwd, and a close must never create or rewrite somebody else's folder.
    let session_folder = startup::session_dir(&std::env::current_dir().unwrap_or_default());
    SESSION_DIRS.with(|held| held.borrow_mut().push(session_folder.clone()));
    crate::project_autosave::install(Rc::clone(&session), session_folder);
    crate::project_autosave::start();
    // The folder this window has open: the working copy beside its launch root. A rename or an Open
    // moves it afterwards -- that is what `save_target` answers -- but a fresh window always starts on
    // its own root's session, whatever an earlier window in this process left behind.
    save_apply::set_open_dir(&startup::session_dir(&std::env::current_dir().unwrap_or_default()));

    // The status line: the shell's sentence, right-aligned in the bottom row (§1's "status line").
    // Also before the pages, for the same reason — a row's press reports itself there.
    let status = gtk::Label::new(Some(""));
    status.set_widget_name("status-line");
    status.set_xalign(1.0);
    status.set_hexpand(true);
    status.set_ellipsize(gtk::pango::EllipsizeMode::End);
    status.add_css_class("dim-label");
    status.set_margin_end(8);

    let stack = adw::ViewStack::new();
    let mut prepare_add: Option<(gtk::Button, gtk::CheckButton)> = None;
    for name in PAGES {
        let (child, add_, copy_) = page_box(name, project, &session, &status, &window);
        stack.add_titled(&child, Some(name), name);
        if let (Some(add_), Some(copy_)) = (add_, copy_) {
            prepare_add = Some((add_, copy_));
        }
    }
    if !page.is_empty() {
        stack.set_visible_child_name(page);
    }

    // The tab strip is the ViewStack's own title widget: a TabBar needs a TabView.
    let switcher = adw::ViewSwitcher::builder()
        .stack(&stack)
        .policy(adw::ViewSwitcherPolicy::Wide)
        .build();
    paint_tabs(&switcher, &shell.borrow(), project);

    // Where ▶ is decided: the run bar's own state, beside the shell's. Nothing about which of
    // pause / transport / start applies is worked out here — run.rs does that (F0.2).
    let bar = Rc::new(RefCell::new(run::RunBar::default()));
    // Published the same way `SESSION` is, so a test can read which step the bar holds instead of
    // inferring it from a button's icon. One window per test binary here, so `last()` is this one.
    RUN_BARS.with(|bars| bars.borrow_mut().push(Rc::clone(&bar)));
    WINDOW_BAR.with(|slots| slots.borrow_mut().push(Rc::clone(&bar)));

    // The children a run has out there, so ⏹ can reach them (F0.3 S3). Held here rather than inside
    // the bar because the flows that spawn register into it and the bar is what decides to stop;
    // nothing spawns yet, so this stays empty until the runner's round arrives.
    let procs = Rc::new(RefCell::new(run::Subprocesses::default()));

    // The tab row is a click; the stack is where that click is decided. `guard` keeps the bounce's
    // own write-back from re-entering this handler, which would otherwise recurse through two more
    // notify signals (the prototype does the same with tabGuard, gui/main.go:1209-1224).
    let guard = Rc::new(RefCell::new(false));

    // §1's badges **14** and **15**: the visible tab's `Inputs:` readout, then its `Outputs:`
    // count with the folder button beside it. They are built before the switch is wired because a
    // tab click repaints them, and they are window-level rather than page-level so a switch updates
    // one pair instead of four. Both texts come from `Shell::inputs` / `Shell::outputs`; nothing
    // here composes a sentence.
    let readout_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    readout_row.set_widget_name("readout-row");
    readout_row.set_margin_start(8);
    readout_row.set_margin_end(8);
    let inputs_readout = gtk::Label::new(None);
    inputs_readout.set_widget_name("inputs-readout");
    inputs_readout.set_xalign(0.0);
    inputs_readout.add_css_class("dim-label");
    readout_row.append(&inputs_readout);
    let outputs_folder = gtk::Button::from_icon_name("folder-symbolic");
    outputs_folder.set_widget_name("outputs-folder-button");
    outputs_folder.set_tooltip_text(Some(OUTPUTS_FOLDER_TIP));
    outputs_folder.set_valign(gtk::Align::Center);
    readout_row.append(&outputs_folder);
    let outputs_readout = gtk::Label::new(None);
    outputs_readout.set_widget_name("outputs-readout");
    outputs_readout.set_xalign(0.0);
    outputs_readout.set_hexpand(true);
    outputs_readout.add_css_class("dim-label");
    readout_row.append(&outputs_readout);

    wire_switching(
        &stack,
        &switcher,
        &status,
        &shell,
        &guard,
        project,
        &inputs_readout,
        &outputs_folder,
        &outputs_readout,
        &held_narration,
    );

    // S4 at startup: the row is drawn for the page the window opened on, not left blank until a tab
    // is clicked. Same read and same painter as the switch handler, so the two can never disagree.
    {
        let (tree, cut, narration) = session_reads();
        paint_readouts(
            &shell.borrow(),
            project,
            tree.as_ref(),
            &cut,
            &narration,
            &inputs_readout,
            &outputs_folder,
            &outputs_readout,
        );
    }

    // F0.5's bookkeeping, drawn: the bar that says how far the run has got and the log it writes
    // into. Both are window-level (built once here, not per page), so a name lookup cannot land on a
    // second copy the way a same-named page button could.
    let queue = Rc::new(RefCell::new(runqueue::Queue::new()));
    WINDOW_QUEUE.with(|slots| slots.borrow_mut().push(Rc::clone(&queue)));
    let exchange_log = Rc::new(RefCell::new(exchanges::RunLog::new()));

    // The progress bar sits between ⏹ and the status line: §2's bar reads play, stop, gears, then
    // the run's fraction and its words. Idle is 0 rather than hidden — a bar that vanishes makes the
    // row jump when a run starts.
    let progress = gtk::ProgressBar::new();
    progress.set_widget_name("run-progress");
    progress.set_hexpand(true);
    progress.set_valign(gtk::Align::Center);

    // §1: "the log expander, its header carrying the status line". The header is a row of two: the
    // word "Log" so the control is never an unlabelled caret (at rest the status is empty), and the
    // status label itself filling the rest. `state()` still reads the label by name, so every existing
    // assertion on `state().status` keeps working unchanged — only its parent moved.
    let log_view = gtk::TextView::new();
    log_view.set_widget_name("log-view");
    // §1: the run's exchange page is the log's one clickable line; everything else stays plain text.
    super::log_link::install(&log_view);
    log_view.set_editable(false);
    log_view.set_cursor_visible(false);
    log_view.set_monospace(true);
    log_view.set_wrap_mode(gtk::WrapMode::WordChar);
    log_view.set_vexpand(true);
    // §10 lists the log's heights only in prose ("log heights 220/110") and gives no `P.` id, so the
    // numbers go on the widget with this note rather than as invented parameter rows.
    log_view.set_height_request(LOG_OPEN_PX);

    let log_header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    log_header.set_widget_name("log-header");
    let log_title = gtk::Label::new(Some("Log"));
    log_title.add_css_class("heading");
    log_header.append(&log_title);
    log_header.append(&status);

    let log = gtk::Expander::new(Some("Log"));
    log.set_widget_name("log-expander");
    log.set_label_widget(Some(&log_header));
    log.set_child(Some(&log_view));
    log.set_expanded(false);

    // §2's run bar: ▶ at the left of the row above the log, ⏹ next to it (F0.3), then the "I'm
    // feeling lucky" gears (F0.4), then the progress bar F0.5 adds.
    let play = gtk::Button::from_icon_name(run::PLAY_ICON);
    play.set_widget_name("play-button");
    play.add_css_class("suggested-action");
    // ⏹ beside ▶, in the order §2's run bar reads: play, stop, then the status line.
    let stop_ = gtk::Button::from_icon_name(run::STOP_ICON);
    stop_.set_widget_name("stop-button");
    stop_.set_tooltip_text(Some(run::STOP_TOOLTIP));
    // The gears after ⏹: labelled rather than icon-only, because §03 names the control by its words and
    // the spec's run-bar image shows the phrase on the button.
    let lucky = gtk::Button::with_label(lucky::LUCKY_LABEL);
    lucky.set_widget_name("lucky-button");
    lucky.set_tooltip_text(Some(lucky::LUCKY_TOOLTIP));
    paint_run_bar(
        &play,
        &stop_,
        &bar.borrow(),
        shell.borrow().page,
        run::Transport::default(),
    );

    let run_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    run_row.set_widget_name("run-bar");
    run_row.append(&play);
    run_row.append(&stop_);
    run_row.append(&lucky);
    // F0.7's policy gear, right beside the "I'm feeling lucky" gears rather than in a toolbar of its own:
    // §03 puts both kinds of ⚙ on the same bar, and a second row of controls would read as a new area
    // of the window that no spec page drew.
    let policy_button = gtk::Button::from_icon_name("emblem-system-symbolic");
    policy_button.set_widget_name("policy-button");
    policy_button.set_tooltip_text(Some(policy_form::OPEN_TIP));
    run_row.append(&policy_button);
    wire_policy(&policy_button, &window);
    // F3.9's ✐ Captions lives on THIS bar, not on the Cut toolbar: §05 #1-screen pins that toolbar to six
    // declared groups (`cut_screen::TOOLBAR_GROUPS`), and F3.9 names no §1 item, so its control cannot join
    // that order without inventing a seventh group the inventory does not list. A caption pass is a JOB run
    // after the cut, which is what this row is for -- beside ▶ / ⏹ / 🎲 / ⚙, the other things you start.
    // Created once per window here (before any lookup) so re-entering the tab cannot stack a second button;
    // the widget NAME is unchanged from the toolbar attempt, so tests and snapshots find it where they always did.
    let captions = gtk::Button::with_label(CAPTIONS_PASS_LABEL);
    captions.set_widget_name("captions-pass-button");
    captions.set_tooltip_text(Some(
        "ask the model for captions over the kept clips \u{2014} \u{21b6} Undo takes the pass back",
    ));
    run_row.append(&captions);
    refresh_captions_gate(&window);
    wire_captions_pass(&captions, &window);
    // F3.10's ⏩ Speeds beside it, for the same reason: one call after the cut asking which stretches run
    // dull is a job, not one of §1's toolbar items, so it sits with the other jobs and NOT in `cut-toolbar`
    // (whose six groups §05 #1-screen pins).
    let speeds = gtk::Button::with_label(SPEEDS_PASS_LABEL);
    speeds.set_widget_name("speeds-pass-button");
    speeds.set_tooltip_text(Some(
        "ask the model which clips run fast \u{2014} \u{21b6} Undo takes the pass back",
    ));
    run_row.append(&speeds);
    refresh_speeds_gate(&window);
    wire_speeds_pass(&speeds, &window);
    // F3.11's ✨ Decorations beside those two, for the same reason: one call after the cut asking for zooms,
    // a stop and some volume is a job, not one of §1's toolbar items, so it sits with the other jobs and NOT
    // in `cut-toolbar` (whose six groups §05 #1-screen pins via `cut_screen::TOOLBAR_GROUPS`). Created once
    // per window here, before any lookup, so re-entering the tab cannot stack a second button.
    let decorations = gtk::Button::with_label(DECORATIONS_PASS_LABEL);
    decorations.set_widget_name("decorations-pass-button");
    decorations.set_tooltip_text(Some(
        "ask the model for zooms, a stop and some volume over the kept clips \u{2014} \u{21b6} Undo takes the pass back",
    ));
    run_row.append(&decorations);
    refresh_decorations_gate(&window);
    wire_decorations_pass(&decorations, &window);
    run_row.append(&progress);

    wire_play(
        &play,
        &stop_,
        &bar,
        &shell,
        &status,
        &progress,
        &log,
        &log_view,
        &queue,
        &exchange_log,
        &procs,
        project,
    );
    wire_stop(
        &stop_,
        &play,
        &bar,
        &shell,
        &status,
        &progress,
        &log,
        &log_view,
        &queue,
        &procs,
    );
    wire_lucky(
        &lucky,
        &bar,
        &shell,
        &stack,
        &guard,
        &status,
        &progress,
        &log,
        &log_view,
        &queue,
        &exchange_log,
        &procs,
        project.clone(),
    );

    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 0);
    // §1's header bar: **1** New · **2** Open · **3** Save · **8** Rescan. New is F0.8, Open F0.9,
    // Save F0.10, Rescan F0.11 and Settings F0.13; ⓘ (F0.1's S4) stays out for its own round.
    // The window holds one live project — `session` below — which is what makes Open possible: the
    // chooser hands over a folder and that single copy is replaced with what was read from it, so
    // every flow already sharing the handle reads the opened project rather than the one the launch
    // started with.
    let header = adw::HeaderBar::new();
    let new_ = gtk::Button::from_icon_name("document-new-symbolic");
    new_.set_widget_name("new-button");
    new_.set_tooltip_text(Some(NEW_TIP));
    header.pack_start(&new_);
    wire_new(&new_, &bar, &status, &session);

    // **2** Open, between New and Save as §1 reads the bar.
    let open_ = gtk::Button::from_icon_name("document-open-symbolic");
    open_.set_widget_name("open-button");
    open_.set_tooltip_text(Some(OPEN_TIP));
    header.pack_start(&open_);
    wire_open(&open_, &bar, &status, &session);

    let save_ = gtk::Button::from_icon_name("document-save-symbolic");
    save_.set_widget_name("save-button");
    save_.set_tooltip_text(Some(SAVE_TIP));
    header.pack_start(&save_);
    wire_save(&save_, &bar, &status, &session);

    // **8** sits at the RIGHT end of the bar, next to where Settings goes — `pack_end`, not
    // `pack_start`, so it does not join New and Save.
    let rescan_ = gtk::Button::from_icon_name("view-refresh-symbolic");
    rescan_.set_widget_name("rescan-button");
    rescan_.set_tooltip_text(Some(RESCAN_TIP));
    header.pack_end(&rescan_);
    wire_rescan(&rescan_, &status, &session, &shell);

    // F0.13's Settings button, packed after Rescan so Rescan stays rightmost as §1 reads the bar.
    // The dialog itself is built on the press, in `settings.rs`; nothing here decides anything.
    let settings_ = gtk::Button::from_icon_name("emblem-system-symbolic");
    settings_.set_widget_name("settings-button");
    settings_.set_tooltip_text(Some(SETTINGS_TIP));
    header.pack_end(&settings_);
    wire_settings(&settings_, &window);
    // §1's badge **6**: ⓘ, once, at header position 6 — read left to right the bar ends ⓘ ⚙ ⟳, so
    // it is packed after Settings (each `pack_end` lands left of the previous one). It opens the
    // same policy form as the run bar's gear through [`press_policy`]; its tooltip follows
    // `Shell::help_page`, which a switch syncs and a bounce leaves on the page still shown. The
    // four per-page ⓘ buttons stay for now: `cut_policy_form_widgets` presses
    // `tab-info-button-Cut`, and §1's header-only layout is not yet enforced.
    readout_row.append(&outputs_readout);

    // §1's badge **6**: ⓘ, once, at header position 6 — read left to right the bar ends ⓘ ⚙ ⟳, so
    // it is packed after Settings (each `pack_end` lands left of the previous one). It opens the
    // same policy form as the run bar's gear through [`press_policy`]; its tooltip follows
    // `Shell::help_page`, which a switch syncs and a bounce leaves on the page still shown. The
    // four per-page ⓘ buttons stay for now: `cut_policy_form_widgets` presses
    // `tab-info-button-Cut`, and §1's header-only layout is not yet enforced.
    let tab_info = gtk::Button::from_icon_name("help-about-symbolic");
    tab_info.set_widget_name("tab-info-button");
    tab_info.set_tooltip_text(Some(&shell.borrow().info_tip()));
    INFO_BUTTON.with(|slots| slots.borrow_mut().push(tab_info.clone()));
    header.pack_end(&tab_info);
    wire_policy(&tab_info, &window);

    // F0.12 lives on the Prepare page, so its handler gets the widgets that page built. They are
    // handed over rather than found by name: `page_box` runs once per tab, and a window with four
    // same-named buttons makes `find` return whichever one it reaches first — an arbitrary page's
    // button, wired to nothing or wired twice.
    if let Some((add_, copy_)) = prepare_add {
        wire_add(&window, &add_, &copy_, &bar, &status, &session);
    }

    box_.append(&header);
    box_.append(&switcher);
    box_.set_widget_name("page-tabs");
    box_.append(&stack);
    stack.set_vexpand(true);
    box_.append(&run_row);
    box_.append(&readout_row);
    // The log goes under the run bar, as §1 lists them. A `gtk::Paned` divider (the "two halves of a
    // draggable divider" line) is the window's own item, not F0.5's bookkeeping, so the box layout
    // stays and the expander simply takes its height back when collapsed.
    box_.append(&log);

    window.set_content(Some(&box_));

    // F2.1: this window's preview is registered only now that the content tree exists. A `Rc` clone of
    // a GTK widget handed out earlier keeps the ref floating outside the tree, and GTK then hands the
    // button's own strong reference back to the caller — which leaves the click handler never reached,
    // because nothing inside the realized window is the object the test fires. Registering after
    // `set_content` puts the wired button in the tree and makes the newest slot the live one.
    if let Some(play_) = play_recording_button(&window) {
        PREVIEW_PLAYERS.with(|slots| {
            slots.borrow_mut().push(Rc::new(std::cell::RefCell::new(Player::default())))
        });
        wire_play_recording(&play_, &window);
    }
    // F2.5 S6: this window's one preview volume, pushed the way the player slot is — after
    // `set_content`, so the slider being wired is the one inside the realized tree. Guarded by the
    // same lookup as the slider itself rather than a page check: only the Cut page builds one, so a
    // missing widget *is* "not this page", and every window still gets exactly one volume slot.
    if preview_volume_scale(&window).is_some() {
        PREVIEW_VOLUMES.with(|slots| {
            slots
                .borrow_mut()
                .push(Rc::new(std::cell::RefCell::new(cut_hear::PreviewVolume::default())))
        });
        // §05-cut#8 bullet 2: the stream volume goes to full ONCE, here, before any tick has asked for a
        // gain — through this app's own element, never the sound server's remembered stream volume.
        crate::ui::preview_sound::reset_stream_volume_once(&window);
        if let Some(scale) = preview_volume_scale(&window) {
            wire_preview_volume(&scale, &window);
        }
    }
    // F2.6: the drag surface and the cross are wired here for the same reason ▶ is — a handler on a
    // widget outside the realized tree never fires. The gesture's own press-to-drag slop is GTK's;
    // `cut_select::is_drag` is what the page would consult for its own slop rule, and the seam below
    // takes the drag's seconds directly so both paths meet in `draw_selection`.
    if select_surface(&window).is_some() {
        SELECTED.with(|slots| {
            slots
                .borrow_mut()
                .push(Rc::new(std::cell::RefCell::new(None::<cut_select::Selection>)))
        });
        SELECT_SURFACES.with(|slots| {
            slots
                .borrow_mut()
                .push(Rc::new(std::cell::RefCell::new(None::<cut_select::Surface>)))
        });
        wire_select_surface(&window);
        // F2.4 S1: the click that places the red line, on the same surface. The drag above selects;
        // this places and takes into hand. Both are needed — a page where only the drag fires has no way
        // for a press to move the line at all.
        wire_line_click_surface(&window);
        if let Some(cross) = clear_selection_button(&window) {
            wire_clear_selection(&cross, &window);
        }
        // The verb buttons exist on this page but were built insensitive. Nothing has been drawn yet, so
        // read the state once here to put them in the state the rules actually give a window with no
        // band — otherwise they would sit greyed until the first drag, and a greyed button that could
        // have been live is exactly the button that lies about the state.
        refresh_verb_buttons(&window);
    }
    // F2.2: ▶✂ is wired to the same player slot ▶ moves, because they are two views of one preview —
    // pressing one switches what the other would show, never a second transport.
    if let Some(cut_) = play_cut_button(&window) {
        wire_play_cut(&cut_, &window);
    }
    // F2.2 S5/S6: the tick loop that skips removed stretches, arms the spare pipeline ahead of each
    // jump and prints the cut's own clock. No-ops until ▶✂ makes the preview the plain cut.
    cut_play_leg::start_cut_preview_tick(&window, Rc::new(CutPreviewSlot(window.clone())));
    // F2.1 S3/S4: ▶'s own tick on the same cadence — no-ops until the recording preview is playing.
    crate::ui::cut_record_tick::start(Rc::new(window.clone()));
    // F2.3 S3/S4/S5: ▶✂✂'s tick, which feeds the line back into `cut_review::tick` so a review actually
    // advances from join to join. No-ops until a review is running.
    crate::ui::cut_review_play::start(Rc::new(window.clone()));
    // F2.3: this window's cut slot is registered here for the same reason the player slot is — after
    // `set_content`, so the button being wired is the one inside the realized tree. Seeded from the
    // project's own `cut/cut.json` when it has one, because that file IS the cut (§3): the strip paints
    // from this slot, so a project with cuts must show them and a project without one must show the
    // empty hint rather than a blank box. A missing or unreadable file is no cut today, which is what
    // `cut::load` already answers with `Cut::default`.
    // F2.4 S4: this window's line slot, registered with the others so the newest window is the live
    // one. The project root DOES reach the page -- `project_tree` above is what the cut was loaded
    // from -- so the saved position is restored here rather than starting at zero: `cut_line::restore`
    // keeps it only while some filmed span still covers it (the spans come from this same opened cut
    // through `timeline::kept_footage_recordings` + `filmed_runs`, so nothing new is invented).
    // One-shot per project open: it happens at build, never on re-entering the tab.
    let project_tree = layout::Tree::new(startup::session_dir(
        &std::env::current_dir().unwrap_or_default(),
    ))
    .ok();
    let opened_cut = project_tree
        .as_ref()
        .and_then(|tree| cut::load(tree).ok())
        .unwrap_or_default();
    REVIEW_CUTS.with(|slots| slots.borrow_mut().push(Rc::new(std::cell::RefCell::new(opened_cut.clone()))));
    // F2.13: this window's edit history, opened on the SAME cut that was just published, so the base
    // Revert returns to is what this page opened with (§2) rather than whatever a previous window left
    // behind. Done through `reopen_history_on` rather than a second push because the history may already
    // have been touched (a refresh ran during the build) -- replacing beats stacking another baseline.
    reopen_history_on(&window, &opened_cut);
    // F2.11/F2.10: this window's SESSION recordings, pushed beside the cut. The fold half reads these
    // rather than `kept_footage_recordings(&opened_cut)`: a dropped stretch only exists inside a run LONGER
    // than the union of the kept clips inside it. F2.10 asks that list of the OPEN PROJECT (stamped names,
    // hand shifts, probed lengths — [`crate::ui::cut_session`]), so a second camera that filmed the same
    // seconds gets its own row; what the cut implies is the fallback when nothing can be resolved.
    push_session_recordings(crate::ui::cut_session::session_or_kept(&opened_cut));
    // F2.4 S4 asks that a remembered line survives while some filmed span still covers it. The spans come from
    // the SESSION list this window was just given (`session_or_kept`: the open project's takes where ffprobe
    // answered, else what the opened cut implies) — a take keeps rolling past the last kept clip, and a line
    // out there is still on footage. Where nothing could be resolved that list IS the opened cut's own spans,
    // so the line and its tape cannot disagree about which folder they belong to.
    let runs = crate::timeline::filmed_runs(page_recordings().as_slice());
    if let Some(review_) = review_cuts_button(&window) {
        wire_review_cuts(&review_, &window);
    }
    // F2.3 S1 (and F2.2 S1): the cut slot above is filled only AFTER the Cut page's first refresh ran, so
    // both skipping buttons have their grey asked once more here — otherwise a project opened with one clip
    // would show them live until the first drag, which is a button lying about the state. The pairing lives
    // in the page half, so this file grows by nothing of its own beyond the call.
    crate::ui::cut_cut_play::paint_both_skipping_buttons(&window);
    // F3.9's ✐ Captions is wired where it is BUILT -- in the run-bar row above, next to the other jobs -- and
    // deliberately NOT appended into `cut-toolbar`: §05 #1-screen asserts that bar holds exactly the six
    // groups `cut_screen::TOOLBAR_GROUPS` declares, in that order, and F3.9 adds no §1 item to any of them.
    // Only the gate is refreshed here, because the policy may have moved since the page was built.
    refresh_captions_gate(&window);
    refresh_speeds_gate(&window);
    // F2.4 S4: this window's line slot, registered with the others so the newest window is the live
    // one. The project root DOES reach the page — `session_root` above is what the cut was loaded from —
    // so the saved position is restored here rather than starting at zero: `cut_line::restore` keeps it
    // only while some filmed span still covers it (the spans come from this same opened cut through
    // `timeline::kept_footage_recordings` + `filmed_runs`, so nothing new is invented). One-shot per
    // project open: it happens at build, never on re-entering the tab.
    let project_root = project_tree.as_ref().map(|tree| tree.dir().to_path_buf());
    if let Some(root) = project_root.clone() {
        LINE_ROOTS.with(|slots| slots.borrow_mut().push(root));
    }
    let restored = project_root
        .as_deref()
        .and_then(|root| cut_line::restore(root, &runs));
    if let Some(pos) = restored {
        log_line(&format!("line restored to {}", crate::tools::mm_ss(pos.t)));
    }
    LINE_STATES.with(|slots| {
        slots.borrow_mut().push(Rc::new(std::cell::RefCell::new((
            restored.unwrap_or(cut_line::LinePos { t: 0.0 }),
            cut_line::LineWriter::default(),
        ))))
    });
    // S4's other half: a restored line is CUED to that frame and scrolled into view, not merely logged.
    if let Some(pos) = restored {
        crate::ui::cut_picture::note_line_moved(&window, pos.t);
        crate::ui::cut_view::follow_playhead(&window, pos.t);
    }
    // F2.4: the four line-step buttons get their handlers now that they sit in the realized tree.
    for (name, _label, frames) in LINE_STEP_BUTTONS {
        if let Some(step) = line_step_button(&window, name) {
            wire_line_step(&step, &window, frames);
        }
    }
    wire_line_keys(&window);
    // F2.4 S4: closing this window saves the line where it stands, whatever the rate limit says.
    // `Propagation::Proceed` so the close still happens — same shape as policy_form.rs's own hook.
    {
        let closing = window.clone();
        window.connect_close_request(move |_| {
            flush_line_on_close(&closing);
            // §F4.7 (**Text**): closing the window flushes a half-typed line too, not only a tab switch.
            crate::ui::narrate_text::flush_owed(&closing);
            // §2 S6: and the project itself, which is what an interrupted session would lose — resolved
            // through this window's own folder rather than the process cwd, which a moved session or a
            // test that pinned its cwd can point somewhere else entirely.
            flush_session_on_close();
            // §6/§5: and any settings dialog still open — it is a separate top level, so its own close
            // hook never runs when the app comes down through this window.
            crate::ui::settings::flush_owed();
            glib::Propagation::Proceed
        });
    }
    // F2.7: the three verb buttons and ⌦ / Delete / BackSpace, wired after `set_content` like every
    // other control on this page.
    for (name, _, _) in cut_verbs::BUTTONS {
        if let Some(button) = line_step_button(&window, name) {
            let seam: fn(&adw::ApplicationWindow) -> cut_verbs::Outcome = match name {
                "add-button" => press_add,
                "split-button" => press_split,
                _ => press_remove,
            };
            wire_verb_button(&button, &window, seam);
        }
    }
    wire_delete_keys(&window);
    // F2.12: ⧉ Insert, wired the same way — after `set_content`, so the button being wired is the one
    // inside the realized tree. The button forwards and prints; every rule is in `cut_insert`.
    if let Some(button) = line_step_button(&window, "insert-button") {
        let win = window.clone();
        button.connect_clicked(move |_| {
            let status = press_insert_from_button(&win);
            if let Some(status_line) = find_status(win.upcast_ref()) {
                status_line.set_text(&status);
            }
            refresh_insert_button(&win);
        });
    }
    // F2.9: ⧉ Copy / ⧉ Paste / ⇲ Lane, wired the same way — after `set_content`, so the buttons being
    // wired are the ones inside the realized tree.
    for (name, seam) in [
        ("copy-button", press_copy as fn(&adw::ApplicationWindow) -> String),
        ("paste-button", press_paste),
        ("lane-button", press_lane),
    ] {
        if let Some(button) = line_step_button(&window, name) {
            let win = window.clone();
            button.connect_clicked(move |_| {
                let status = seam(&win);
                if let Some(status_line) = find_status(win.upcast_ref()) {
                    status_line.set_text(&status);
                }
                refresh_copy_buttons(&win);
            });
        }
    }
    // S2: Esc drops what is in hand, and only that key.
    wire_copy_esc(&window);
    // F3.1: the zoom's own drag on the preview panel, plus Esc releasing the arm. Wired after
    // `set_content` like every other control, so the panel being wired is the one inside the realized tree.
    wire_zoom_drag(&window);
    wire_zoom_esc(&window);
    // F3.3: Esc drops an open speed form too. Wired here (after `set_content`) rather than only inside
    // `show_speed_form`, so the key is claimed by the window that drew the form even before a test looks it up.
    wire_speed_esc(&window);
    // F3.6: Esc drops an open volume form, wired beside the speed form's for the same reason — the controller
    // must sit on the realized window, and it claims the key only when this window owns the form.
    wire_volume_esc(&window);
    // F3.7: Esc drops an open label form, wired beside the volume form's for the same reason — the controller
    // must sit on the realized window, and it claims the key only when this window owns the form.
    wire_label_esc(&window);
    // F3.4: Esc releases the caption arm / drops an open caption form. Wired here (after `set_content`) like
    // every other control on this page.
    wire_text_esc(&window);
    // F3.5: Esc releases the drawing arm / drops an open drawing form. Wired here (after `set_content`) like
    // every other control on this page.
    wire_svg_esc(&window);
    // F3.8 S3: Esc drops a HELD EFFECT. Wired last among the Esc controllers so the six per-kind form/arm
    // controllers keep their precedence and this one only claims what they left — a hold with no form open.
    wire_band_esc(&window);
    // F3.2: the Aspect ratio dropdown, wired by name after `set_content` for the same reason — the widget
    // the lookup finds must be the one in the realized tree.
    wire_aspect_choice(&window);
    // §05-cut#1-screen: the history group (Undo / Redo / Revert / Clear), the zoom pair and their
    // chords, wired after `set_content` like every other control on this page.
    wire_history_and_zoom(&window);
    wire_history_keys(&window);
    // F2.8: the trim and move gestures on `track-strip`, wired after `set_content` for the same reason.
    wire_track_strip(&window);
    // F2.10: fill `camera-rows` now that the page is inside the tree, so the search for it succeeds
    // and the plates, badges and switches a test fires by name are actually there.
    refresh_camera_rows(&window);
    // §06-effects#2: the lane's one row and the preview's frame are filled here for the same reason —
    // `page_box` builds both holders empty because the window is not in the tree yet at that point.
    refresh_effects_lane(&window);
    // F2.11: the gutter's fold-all badge, then the per-gap − / + badges and each row's ✕. Drawn here,
    // after `set_content`, because a button outside the realized tree is found by no name and fired by
    // no click.
    wire_fold_all(&window);
    refresh_fold_badges(&window);
    // §1 badge **31**: the bar takes its numbers now the page is in the tree and the session's length
    // is known; §1's three wheel gestures need their widgets realized to receive anything.
    repaint_scrollbar(&window);
    wire_wheel_gestures(&window);
    crate::ui::cut_view::attach_scrollbar(&window);
    // F2.4 S1/S1b: the painted band takes the line press too.
    crate::ui::window::wire_band_line_press(&window);
    // F2.6 S1/S2: and the selection drag, on the same band, reading the same painted geometry.
    crate::ui::cut_select_press::attach(&window);
    cut_form_frame::wire_close(&window);
    cut_picture::wire_click(&window);
    crate::ui::cut_preview_frame::install_draw(&window);
    // §2: land a suggestion — its cut AND the new base — through the page's own doors.
    crate::ui::cut_suggest_land::install(newest_review_cut, |window, landed| {
        publish_cut(landed);
        // note_change before suggested: Revert then keeps the model's work and drops only hand edits after it.
        cut_history(window).borrow_mut().note_change(landed);
        cut_history(window).borrow_mut().suggested(landed);
        refresh_history_buttons(window);
    });
    // §F2.14 S2b: the Suggest call's progress, painted (§S2b's "streaming progress counts segments"). One
    // reply's reading goes back through the page's one progress door, so the bar and the status row are drawn
    // exactly as every other beat draws them, with no second painting path.
    crate::ui::cut_suggest_progress::install(|window, progress| {
        let (Some(bar), Some(status)) = (
            crate::ui::window::progress_bar(window),
            crate::ui::window::find_status(window.upcast_ref()),
        ) else {
            return;
        };
        if progress.pulsing {
            bar.pulse();
        } else {
            bar.set_fraction(progress.fraction.clamp(0.0, 1.0));
        }
        status.set_text(&progress.line);
    });
    // §07-narrate#1-screen: the Narrate page's controls, wired after `set_content` like every other
    // control in this window, so the widgets being wired are inside the realized tree.
    crate::ui::narrate_page::wire(&window);
    crate::ui::produce_page::wire(&window);
    window
}

/// F2.11 S1 (`the gutter badge folds/unfolds all`): create `fold-all-button` in the gutter holder and
//  wire it to [`press_fold_all`]. The button lives beside the camera rows rather than painted into the
/// gutter's 30 px for the same reason the gap badges do — see `fold-badges` at the page build.
fn wire_fold_all(window: &adw::ApplicationWindow) {
    // Look the button up IN THE HOLDER rather than by a tree-wide name search. A window's `close()` does
    // not destroy its widgets while another window's tree is still around, so searching by name can hand
    // back a `fold-all-button` parented to a DIFFERENT window's box: this one would then look already
    // built and never be appended, leaving the second window with no gutter badge at all.
    let Some(holder) = fold_badges_box(window) else {
        return;
    };
    if holder_children_named(&holder, "fold-all-button") {
        return;
    }
    let button = gtk::Button::new();
    button.set_widget_name("fold-all-button");
    button.set_label("\u{2212}\u{25a4}");
    button.set_tooltip_text(Some("fold every stretch the cut drops \u{00b7} press again to open them all"));
    let win = window.clone();
    button.connect_clicked(move |_| {
        press_fold_all(&win);
    });
    holder.append(&button);
}

/// Does this holder already own a child with exactly this widget name? Scoped to the holder on purpose —
/// see [`wire_fold_all`] for what a tree-wide search costs.
fn holder_children_named(holder: &gtk::Box, name: &str) -> bool {
    holder
        .observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .any(|child| child.widget_name() == name)
}

/// The shell as the window starts: on the page named, or on Prepare for `""` and for a name that is
/// not one of the four (a page nobody asked for is not a reason to fail to open).
fn start_shell(page: &str) -> Shell {
    match Page::all().into_iter().find(|candidate| candidate.label() == page) {
        Some(page) => Shell { page, help_page: page, ..Shell::default() },
        None => Shell::default(),
    }
}

/// Where a tab click is decided: ask the shell, redraw from what it says, and put the strip back if
/// it refused. Split out of [`build_window`] so the handler has one job and can be driven directly.
fn wire_switching(
    stack: &adw::ViewStack,
    switcher: &adw::ViewSwitcher,
    status: &gtk::Label,
    shell: &Rc<RefCell<Shell>>,
    guard: &Rc<RefCell<bool>>,
    project: &Project,
    inputs_readout: &gtk::Label,
    outputs_folder: &gtk::Button,
    outputs_readout: &gtk::Label,
    held_narration: &Rc<RefCell<Narration>>,
) {
    let stack = stack.clone();
    let switcher = switcher.clone();
    let status = status.clone();
    let shell = shell.clone();
    let guard = guard.clone();
    let project = project.clone();
    let inputs_readout = inputs_readout.clone();
    let outputs_folder = outputs_folder.clone();
    let outputs_readout = outputs_readout.clone();
    let held_narration = held_narration.clone();
    stack.connect_visible_child_name_notify(move |stack| {
        if *guard.borrow() {
            return;
        }
        let shown = stack.visible_child_name().unwrap_or_default().to_string();
        let Some(target) = Page::all().into_iter().find(|page| page.label() == shown) else {
            return;
        };
        // S1-S5, all of it in shell.rs. The folder is read here rather than remembered: which
        // project is open is the window's business, and `session_dir` is the same stand-in Rescan
        // uses. A folder that is not a project leaves `tree` None — §1 wants the page to show what
        // it has rather than fail to open, so there is no error path out of the switch.
        let (tree, cut, _) = session_reads();
        // The lines themselves are the window's held copy, not a fresh read: S5 refits them on
        // arrival and S3 writes that refit on leaving, and the two are separate switches.
        let held = Rc::clone(&held_narration);
        // S3 asks whether a narration write is owed BEFORE the switch clears the flag: leaving the
        // tab writes what is half-typed even a beat early.
        let owed = shell.borrow().narration_pending.owe();

        let outcome = shell.borrow_mut().switch(
            target,
            Move::Click,
            &project,
            tree.as_ref(),
            &cut.segs,
            &mut held.borrow_mut().entries,
        );

        // S3: the half-typed lines reach disk before the page is left. Only a flush that was owed
        // writes, so a switch over an untouched narration costs no file.
        if owed {
            if let Some(tree) = tree.as_ref() {
                let _ = narration::save(&held.borrow(), tree);
            }
        }
        paint_tabs(&switcher, &shell.borrow(), &project);
        paint_readouts(
            &shell.borrow(),
            &project,
            tree.as_ref(),
            &cut,
            &held.borrow(),
            inputs_readout.upcast_ref(),
            &outputs_folder,
            outputs_readout.upcast_ref(),
        );
        status.set_text(&shell.borrow().status);

        if let Outcome::Bounced { .. } = outcome {
            // S2: the page did not move.
            *guard.borrow_mut() = true;
            stack.set_visible_child_name(shell.borrow().page.label());
            *guard.borrow_mut() = false;
        }
    });
}

/// The folder this window is working on, with its cut and narration read from it.
///
/// Read fresh every time rather than remembered: which project is open is the window's business, and
/// `session_dir` is the same stand-in Rescan, New and Save use, so the flows agree on what "this
/// project" means. A folder that is not a project leaves `tree` None — §1 wants the page to show
/// what it has rather than fail to open, so nothing here errors out. Both the tab switch (S3-S5) and
/// the first paint call this, so the row cannot disagree with itself between them.
pub(crate) fn session_reads() -> (Option<layout::Tree>, Cut, Narration) {
    let root = std::env::current_dir().unwrap_or_default();
    let dir = startup::session_dir(&root);
    let tree = if dir.join(crate::project::PROJECT_FILE).is_file() {
        layout::Tree::new(&dir).ok()
    } else {
        None
    };
    let cut = tree.as_ref().and_then(|tree| cut::load(tree).ok()).unwrap_or_default();
    let narration = tree
        .as_ref()
        .and_then(|tree| narration::load(tree).ok())
        .unwrap_or_default();
    (tree, cut, narration)
}






/// The New button's tooltip, §1's wording for badge **1**.
const NEW_TIP: &str = "New project \u{2014} name it, put it where you want it, and start over";

/// S3's confirmation, drawn from [`new_project`]'s strings and deciding nothing: the question, the
/// two paragraphs, and the two answers. "Start new…" is destructive because what it does cannot be
/// undone, and Cancel keeps the focus so a blind Enter does nothing (the prototype's `confirm`).
pub fn new_project_confirm(
    parent: Option<&adw::ApplicationWindow>,
    detail: &str,
) -> adw::MessageDialog {
    let dialog = adw::MessageDialog::new(parent, Some(new_project::QUESTION), Some(detail));
    // The × in the header bar answers exactly as Cancel does. `close-button` is adw::Window's own
    // property (the response its × produces is `close-response`, which names Cancel), so both are
    // set through the object rather than through a trait method this binding only has on Window.
    dialog.set_property("close-response", "cancel");
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("start", new_project::BUTTON);
    dialog.set_response_appearance("start", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog
}

/// Where a press of ＋ New goes: ask [`new_project::press`], then draw what it says. Every string the
/// user sees comes from that module, including the refusal's and the confirmation's. Whether the
/// session is empty comes from the live handle the pages mutate, so S2 asks only when there is
/// something to lose; a run under way comes from F0.2's [`run::RunBar`], which ▶ answers for too.
fn wire_new(new_: &gtk::Button, bar: &Rc<RefCell<run::RunBar>>, status: &gtk::Label, session: &Rc<RefCell<Project>>) {
    let bar = bar.clone();
    let status = status.clone();
    let session = session.clone();
    new_.connect_clicked(move |_| {
        let root = std::env::current_dir().unwrap_or_default();
        let open = startup::session_dir(&root);
        let empty = session.borrow().sources.is_empty();
        match new_project::press(bar.borrow().running.is_some(), empty, &open) {
            new_project::Gate::Refused { reason } => status.set_text(reason),
            new_project::Gate::Name => ask_new_project(&status, &root, &open, &session),
            new_project::Gate::Confirm { detail } => {
                let dialog = new_project_confirm(None, &detail);
                let status = status.clone();
                let root = root.clone();
                let open = open.clone();
                let session = session.clone();
                dialog.connect_response(None, move |dialog: &adw::MessageDialog, response| {
                    // Cancel is not pressing on: S3's whole point is that nothing was changed.
                    if response == "start" {
                        ask_new_project(&status, &root, &open, &session);
                    }
                    dialog.destroy();
                });
                dialog.present();
            }
        }
    });
}

/// S4: ask where the new project goes and what it is called. The name's date is taken here because
/// [`new_project`] is handed the day rather than reading a clock, which is what keeps its tests off
/// today; the folder chooser is the desktop's own sheet, so the folder it hands back *is* the new
/// project, and S5's refusal or S6's sentence goes on the status line.
fn ask_new_project(status: &gtk::Label, root: &Path, open: &Path, session: &Rc<RefCell<Project>>) {
    // `%F` is the ISO date, which is what the spec's default name is.
    let day = glib::DateTime::now_local()
        .and_then(|today| today.format("%F").map(|day| day.to_string()))
        .unwrap_or_else(|_| "new-project".to_string());
    let dir = open.parent().map(Path::to_path_buf).unwrap_or_else(|| root.to_path_buf());
    let dialog = gtk::FileDialog::builder()
        .title(new_project::TITLE)
        .accept_label("New project")
        .build();
    dialog.set_initial_folder(Some(&gio::File::for_path(&dir)));
    dialog.set_initial_name(Some(&new_project::free_name(&dir, &day)));

    let status = status.clone();
    let root = root.to_path_buf();
    let session = session.clone();
    dialog.select_folder(
        None::<&gtk::Window>,
        None::<&gio::Cancellable>,
        move |chosen: Result<gio::File, glib::Error>| {
            let Ok(file) = chosen else {
                return; // dismissed: nothing was asked for, so nothing changed
            };
            // The folder the chooser hands back *is* the project (01 §1), and its own name is what
            // S5 checked and S6 says in the status line.
            let path = file.path().unwrap_or_default();
            let named = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            match new_project::create(&root, &path, &named) {
                // S6: apply it, not just announce it — log, session swap, remembered pair, status.
                Ok(created) => crate::ui::new_apply::apply(&created, &session, &status),
                Err(reason) => status.set_text(&reason),
            }
        },
    );
}

/// Prepare's own two widgets (F0.12): the button that adds files, and the tick that decides whether
/// they come inside the project or stay where they are. The ellipsis is the single character §1's
/// screenshot shows, not three dots.
const ADD_LABEL: &str = "Add source files…";
const COPY_LABEL: &str = "copy into project";

/// The Rescan button's tooltip, §1's wording for badge **8**.
const RESCAN_TIP: &str = "Rescan inputs and outputs";

/// §1's wording for the Settings badge: the two servers the dialog is about, named in the tooltip
/// so the gear is never an unlabelled control.
const SETTINGS_TIP: &str = "Settings \u{2014} the LLM and audio.cpp endpoints";

pub use crate::ui::window_policy::{
    built_window, close_policy_forms, derive_policy, forget_policy_form, main_window,
    open_policy_form, policy_form_open, policy_is_current, press_policy, refresh_policy_form,
    reset_policy_to_defaults, session_policy, set_policy_sender_for_tests, set_session_context,
    set_session_policy_field,
};
pub(crate) use crate::ui::window_policy::{
    schedule_policy_derive, user_context, wire_policy, wire_settings, POLICY_TRACKERS,
    POLICY_WATCH_WINDOW,
};

/// Where a press of Rescan goes: [`rescan`] decides everything — which rows are gone, what was
/// re-read, what the status says. The button forwards and draws the answer (spec/00-principles.md §5).
///
/// No run refusal here: S1 drops what has vanished whether or not a run is on, so unlike Save this
/// handler never reads [`run::RunBar`].
fn wire_rescan(
    button: &gtk::Button,
    status: &gtk::Label,
    session: &Rc<RefCell<Project>>,
    shell: &Rc<RefCell<Shell>>,
) {
    let status = status.clone();
    // The window is handed an immutable `&Project` and holds no live project yet, so the scan works on
    // the private copy shared with Add sources, and [`rescan_apply`] writes the pruned list back to the
    // session file. Which folder is open is the same stand-in New and Save use, so all three agree.
    let root = std::env::current_dir().unwrap_or_default();
    let project = session.clone();
    let shell = shell.clone();
    button.connect_clicked(move |_| {
        let dir = startup::session_dir(&root);
        let tree = layout::Tree::new(&dir).ok();
        let found = rescan::rescan(&mut project.borrow_mut(), &root, &dir, tree.as_ref());
        // Every step the window takes with that answer -- the drop lines, the write-back, the vanished
        // page, all three refreshes and the status -- is `rescan_apply`'s, in the spec's order.
        let scanned = project.borrow().clone();
        rescan_apply::apply(
            &found,
            &scanned,
            &project,
            &dir,
            tree.as_ref(),
            &status,
            &shell,
        );
    });
}

/// Where a press of "Add source files…" goes: [`add_sources`] decides whether it may happen at all,
/// which kinds the chooser offers, what is copied and what is only referenced, and what the status
/// says. The button forwards (spec/00-principles.md §5).
///
/// The copy's bytes are drawn on F0.5's bar: a copy is a run of its own (F0.12 S0), so it reports
/// through the same bookkeeping ▶'s run uses ([`add_files`], which feeds `runqueue::Queue`) rather
/// than painting a second bar. The session that grows here is the window's private copy for the
/// same reason [`wire_rescan`] spells out — the pages render the `&Project` handed to
/// `build_window`, and a live project state is F0.9's.
fn wire_add(
    window: &adw::ApplicationWindow,
    button: &gtk::Button,
    copy_into: &gtk::CheckButton,
    bar: &Rc<RefCell<run::RunBar>>,
    status: &gtk::Label,
    session: &Rc<RefCell<Project>>,
) {
    let window = window.clone();
    let bar = bar.clone();
    let status = status.clone();
    let session = session.clone();
    // The tick is read when the chooser is answered, not when it opened.
    let copy_into = copy_into.clone();
    button.connect_clicked(move |_| {
        // S0 first: a copy is a run of its own, so no chooser opens while something else runs.
        if let Err(reason) = add_sources::press(bar.borrow().running.is_some()) {
            status.set_text(reason);
            return;
        }
        ask_add_sources(&window, &status, &session, &copy_into);
    });
}

/// S1's chooser: titled for adding, filtered to audio and video, and willing to take a whole card at
/// once. As in [`ask_save_project`] it is answered by nobody in a headless test, which is the point —
/// what a test asserts is that the press got this far.
///
/// GTK 4.10 replaced the file chooser with `gtk::FileDialog` and deprecated everything below it; the
/// replacement has no multiple-selection filter API that answers a *native* chooser, so this stays on
/// the old calls and takes their warnings here rather than silencing them crate-wide.
#[expect(
    deprecated,
    reason = "the 4.10 replacement cannot offer a filtered multi-select native chooser"
)]
fn ask_add_sources(
    window: &adw::ApplicationWindow,
    status: &gtk::Label,
    session: &Rc<RefCell<Project>>,
    copy_into: &gtk::CheckButton,
) {
    let chooser = gtk::FileChooserNative::builder()        .title(add_sources::CHOOSER_TITLE)
        .modal(true)
        .action(gtk::FileChooserAction::Open)
        .build();
    // The accept button says what it does. `add_choice` is not the three-label call it looks like —
    // that one adds an extra combo — so the buttons are set where they belong.
    chooser.set_accept_label(Some("Add"));
    chooser.set_cancel_label(Some("Cancel"));
    chooser.set_select_multiple(true);
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(add_sources::FILTER_NAME));
    for ext in add_sources::MEDIA_EXT {
        filter.add_pattern(&format!("*{ext}"));
    }
    chooser.add_filter(&filter);
    // F0.8 S6: a project created outside the root is where its footage almost certainly is, so the
    // chooser opens there; with nothing remembered the chooser keeps its own default.
    if let Some(dir) = crate::ui::new_apply::chooser_dir() {
        let _ = chooser.set_current_folder(Some(&gio::File::for_path(&dir)));
    }

    let _status = status.clone();
    let _session = session.clone();
    let _copy_into = copy_into.clone();
    // The same stand-in New, Save and Rescan use, so all four flows agree on which project is open.
    let _root = std::env::current_dir().unwrap_or_default();
    let window = window.clone();
    chooser.connect_response(move |chooser, response| {
        if response != gtk::ResponseType::Accept {
            return;
        }
        let files: Vec<std::path::PathBuf> = chooser
            .files()
            .iter::<gio::File>()
            .flatten()
            .filter_map(|file| file.path())
            .collect();
        // The same body a widget test drives, so the chooser and the seam cannot drift apart.
        add_files(&window, &files);
    });
}

/// The Save button's tooltip, §1's wording for badge **3**.
const SAVE_TIP: &str = "Save this project to a file";

/// The seam a widget test drives: the same body the chooser's callback calls, reached through the
/// window rather than by threading `bar`/`status`/`session` by hand. It decides nothing — it finds
/// the widgets this window built and forwards to [`open_folder`].
pub fn open_from(window: &adw::ApplicationWindow, picked: &Path) -> bool {
    let (Some(bar), Some(status), Some(session)) = (
        WINDOW_BAR.with(|slots| slots.borrow().last().cloned()),
        find_status(window.upcast_ref()),
        SESSION.with(|slots| slots.borrow().last().cloned()),
    ) else {
        panic!("the window has no run bar, status line or session");
    };
    open_folder(&bar, &status, &session, picked)
}

/// The seam a widget test drives for F0.12: the same body the "Add source files…" chooser's response
/// calls, reached through the window rather than by threading `bar`/`status`/`session`/`queue`/
/// `progress` by hand. It decides nothing — [`add_sources`] does — but it is where the copy's bytes
/// reach the bar.
///
/// A copy is a run of its own (F0.12 S0), so it borrows F0.5's bookkeeping instead of painting a
/// second bar: the copy owns one track, each file asked for is one task on it, and each file's byte
/// fraction inside that task is what `prog` reports. That is why the bar moves in proportion to the
/// bytes actually copied rather than to the number of rows added — a slow card is what the bar is
/// waiting on, and `add_sources::Progress` already carries those bytes.
///
/// Returns whether anything was added. Refused while another run is on (S0): the session is not
/// touched, so a refused press cannot half-apply.
pub fn add_files(window: &adw::ApplicationWindow, files: &[std::path::PathBuf]) -> bool {
    let (Some(bar), Some(status), Some(session), Some(queue), Some(progress)) = (
        WINDOW_BAR.with(|slots| slots.borrow().last().cloned()),
        find_status(window.upcast_ref()),
        SESSION.with(|slots| slots.borrow().last().cloned()),
        WINDOW_QUEUE.with(|slots| slots.borrow().last().cloned()),
        progress_bar(window),
    ) else {
        panic!("the window has no run bar, status line, session, queue or progress bar");
    };
    let copy = copy_into_project(window).is_some_and(|tick| tick.is_active());
    let root = std::env::current_dir().unwrap_or_default();
    let dir = startup::session_dir(&root);
    // S0 first, again at the seam: pressed while a run is on, it says the sentence and stops before
    // any byte moves. Nothing here clears a ▶ run's flag — ⏹ owns that.
    if let Err(reason) = add_sources::press(bar.borrow().running.is_some()) {
        status.set_text(reason);
        return false;
    }
    // Ours alone: the import marks itself as *the* run so a ▶ press during a copy is refused the
    // other way round, and only this call may take that flag away.
    bar.borrow_mut().running = Some(run::Run {
        step: run::Step::Prepare,
        paused: false,
        log_expanded: true,
        sources: run::snapshot_sources(&session.borrow()),
    });
    let added = add_copy(
        &mut session.borrow_mut(),
        &queue,
        &progress,
        &root,
        &dir,
        files,
        copy,
    );
    // End it through F0.5's own path, whatever the copy did: the import is over either way. The log
    // expander's state is put back by hand rather than left to `end_run`'s clear — that clears the
    // status line, which this flow needs for S5's sentence.
    bar.borrow_mut().running = None;

    match added {
        Ok(answer) => {
            status.set_text(&answer.status);
            // The added rows are drawn by the same door the build used, so an added row carries every
            // control §4 gives a row (the track menu included) rather than a shorter version of one.
            let total_rows = session.borrow().sources.len();
            if let Some(list) = find_widget_by_name(window.upcast_ref(), "sources-list")
                .and_then(|widget| widget.downcast::<gtk::ListBox>().ok())
            {
                // GTK's `ListBox` exposes its rows through the widget tree, so they are counted with
                // the same iterator the rest of this module walks widgets with: that count is where
                // the build-time drawing stopped, so everything past it is new.
                let drawn = list
                    .observe_children()
                    .iter::<glib::Object>()
                    .filter_map(|child| child.ok())
                    .filter(|child| child.downcast_ref::<gtk::ListBoxRow>().is_some())
                    .count();
                for index in drawn..total_rows {
                    list.append(&source_rows::row_at(
                        &source_rows::Rows {
                            session: Rc::clone(&session),
                            status: status.clone(),
                            list: list.clone(),
                        },
                        index,
                    ));
                }
            }
            answer.added > 0
        }
        // S0's abandonment: the log keeps the reason, the status line the sentence.
        Err(err) => {
            status.set_text(add_sources::NO_SOURCES_DIR);
            log_line(&err);
            false
        }
    }
}

/// The copy half of a press, with the bar fed from the bytes as they land. Split out so the seam
/// above stays about finding widgets and this stays about the run of its own.
fn add_copy(
    session: &mut Project,
    queue: &std::cell::RefCell<runqueue::Queue>,
    progress: &gtk::ProgressBar,
    root: &Path,
    dir: &Path,
    files: &[std::path::PathBuf],
    copy: bool,
) -> Result<add_sources::Added, String> {
    // One job on the first track; every file asked for is one task of it, so the tooltip counts the
    // pick rather than what survived (`added N of M` is the status line's job, not the bar's).
    queue.borrow_mut().job(TRACK_IMPORT, IMPORT_JOB, 0, 0);
    queue.borrow_mut().push(TRACK_IMPORT, files.len(), "file");
    // The folder a copy lands in has to exist before the first `.part` is renamed into it. `add`
    // makes the same folder afterwards, but the byte-by-byte report runs first (F0.12 S2).
    if copy {
        std::fs::create_dir_all(dir.join("sources"))
            .map_err(|_| add_sources::NO_SOURCES_DIR.to_string())?;
    }
    // The total is taken before the first byte moves, so the fraction never chases a growing sum.
    let total: u64 = files
        .iter()
        .map(|file| file.metadata().map(|meta| meta.len()).unwrap_or(0))
        .sum();
    let mut done_bytes = 0u64;
    for file in files {
        let size = file.metadata().map(|meta| meta.len()).unwrap_or(0);
        queue.borrow_mut().take(TRACK_IMPORT);
        if copy {
            // `add_sources`' own copy rule — `.part` then rename, same name + same size skipped, a
            // file already inside the project left where it is. Not reimplemented here.
            add_sources::copy_one(file, dir).map_err(|err| err.to_string())?;
        }
        done_bytes += size;
        // The bar means "how much of this import is on disk": the byte fraction, not the task count.
        // Three tiny files beside one long recording should not read as mostly copied.
        let fraction = if total == 0 {
            1.0
        } else {
            done_bytes as f64 / total as f64
        };
        let name = file
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        queue
            .borrow_mut()
            .prog(TRACK_IMPORT, fraction, &format!("copying {name}"));
        paint_import(progress, queue);
    }
    queue.borrow_mut().done(TRACK_IMPORT, 1.0);
    add_sources::add(session, root, dir, files, copy)
}

/// Draw the import on F0.5's bar. The fraction and the tooltip come from the queue exactly as
/// [`paint_progress`] takes them for a ▶ run; only the expander's body is left alone, since an
/// import writes no run lines of its own.
fn paint_import(progress: &gtk::ProgressBar, queue: &std::cell::RefCell<runqueue::Queue>) {
    let (_text, tip) = queue.borrow().text_and_tip();
    progress.set_fraction(queue.borrow().fraction());
    if !tip.is_empty() {
        progress.set_tooltip_text(Some(&tip));
    }
}

/// The import's own job name on the bar's line and tooltip.
const IMPORT_JOB: &str = "import";
/// The copy gets one track: the whole pick is a single run of its own, not work interleaved with a
/// ▶ run (which is precisely why S0 refuses while one is on).
const TRACK_IMPORT: usize = 0;

thread_local! {
    /// The run bar `build_window` created, published for the same reason as [`SESSION`]: so the
    /// seam in [`open_from`] can reach the copy this window actually uses.
    pub(crate) static WINDOW_BAR: std::cell::RefCell<Vec<Rc<std::cell::RefCell<run::RunBar>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    /// The F0.5 queue `build_window` created, published for the same reason as [`WINDOW_BAR`]: so
    /// the seam in [`add_files`] can drive the bar this window actually shows. A copy is a run of
    /// its own (F0.12 S0), so it reports through the bookkeeping rather than painting its own bar.
    pub(crate) static WINDOW_QUEUE: std::cell::RefCell<Vec<Rc<std::cell::RefCell<runqueue::Queue>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    /// §1's badge **6** — the header ⓘ, newest last, the same newest-slot arrangement as
    /// [`POLICY_FORMS`](crate::ui::window_policy) in the policy module. Held so a switch can repaint its tooltip from `Shell::help_page` without
    /// searching the widget tree, and so a test reads the tooltip of the button that is on screen.
    pub(crate) static INFO_BUTTON: std::cell::RefCell<Vec<gtk::Button>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    /// The live `Shell` of each window, newest last — the newest-slot rule every other window-level
    /// piece here uses. Held so the S3 pending-write seam can mark a write owed on the shell that is
    /// actually showing, instead of a copy the caller happens to have.
    pub(crate) static WINDOW_SHELLS: std::cell::RefCell<Vec<Rc<RefCell<Shell>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}



thread_local! {
    /// The live project handle `build_window` created, published so a test can read what Open put in
    /// place. One window per test binary here, so one slot is enough; the page widgets keep their own
    /// clone of the same `Rc`, which is why replacing the contents is visible everywhere.
    pub(crate) static SESSION: std::cell::RefCell<Vec<Rc<std::cell::RefCell<Project>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The Open button's tooltip, §1's wording for badge **2**.
const OPEN_TIP: &str = "Load a project \u{2014} sources, prompts and settings";

/// F2.4's four line-step buttons, in the spec's left-to-right order: `‹‹f ‹f f› ››`. Each entry is
/// (widget name, label, signed frames) — the sign is the direction and the magnitude the count, so a
/// button's own name cannot disagree with what it does. A `bool` could: wiring `shift=true` to
/// `line-step-back-five` reads as "five" while saying nothing about which way.
const LINE_STEP_BUTTONS: [(&str, &str, i64); 4] = [
    ("line-step-back-five", "\u{2039}\u{2039} f  step back 5 frames", -5),
    ("line-step-back", "\u{2039} f  step back 1 frame", -1),
    ("line-step-forward", "f \u{203a}  step forward 1 frame", 1),
    ("line-step-forward-five", "f \u{203a}\u{203a}  step forward 5 frames", 5),
];

/// The reason ▶ had nothing to play. Spec silent on a session with no filmed stretch; §0 asks for a
/// named, local reason rather than a silent button.
pub const NO_RECORDING_STATUS: &str = "nothing was filmed \u{2014} there is no recording to play";

/// One window's preview state, pushed beside [`SESSION`] so each window owns its own and the newest
/// one wins — the same arrangement `session_sources` reads.
thread_local! {
    static PREVIEW_PLAYERS: std::cell::RefCell<Vec<Rc<std::cell::RefCell<Player>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}







































/// One window's cut, held beside [`PREVIEW_PLAYERS`] with the same newest-slot rule.
///
/// The Cut page has no live cut model yet — that arrives with its own round — so this is what stands in
/// for it: `build_window` seeds a default (empty) cut and every press reads it back. A caller that DOES
/// have a cut hands it over with [`seed_review_cut`], which is the seam the cut-model round plugs into
/// instead of re-plumbing the button.
thread_local! {
    pub(crate) static REVIEW_CUTS: std::cell::RefCell<Vec<Rc<std::cell::RefCell<cut::Cut>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// One window's red line and the writer that throttles its saves, newest slot winning like
/// [`PREVIEW_PLAYERS`]. The writer lives beside the position because the rate limit is a property of
/// this window's moving line, not of the file.
thread_local! {
    pub(crate) static LINE_STATES: std::cell::RefCell<Vec<Rc<std::cell::RefCell<(cut_line::LinePos, cut_line::LineWriter)>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

type WindowFolders = std::cell::RefCell<Vec<std::path::PathBuf>>;

/// The project root this window's line file sits under, newest slot winning like [`REVIEW_CUTS`]. Held
/// beside [`LINE_STATES`] because the position without its root is a number with nowhere to be saved:
/// `restore` needs it on open and `flush_line_on_close` needs it on close. `SESSION_DIRS` is the same
/// shape for the folder §2 S6's close flush writes the session into — `project_autosave::session_dir()`
/// asks the process cwd, a different answer once a flow has moved the session or a test has pinned it.
thread_local! {
    static LINE_ROOTS: WindowFolders = const { std::cell::RefCell::new(Vec::new()) };
    static SESSION_DIRS: WindowFolders = const { std::cell::RefCell::new(Vec::new()) };
}

/// §2 S6 as the close hook wants it, resolved through THIS window: write the live session into the
/// folder that window was built for. Returns whether a file was written.
fn flush_session_on_close() -> bool {
    let Some(dir) = SESSION_DIRS.with(|held| held.borrow().last().cloned()) else {
        return false;
    };
    crate::project_autosave::flush_on_close(&live_project(), &dir)
}

/// What a frame step did (F2.4 S2/S3), so the caller paints the right thing and decides nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Step {
    /// Something was held: THAT moved by `frames` frames rather than the line.
    Nudged { frames: i64 },
    /// Nothing held: the recording under the line seeked to `t` and paused.
    SteppedTo { t: f64 },
    /// Nothing happened — S3's arrows with nothing held.
    Still,
}

/// The project root this window's line file lives under, or `None` when the page was built outside a
/// project. Newest slot wins like every other per-window accessor here.
pub fn line_root(window: &adw::ApplicationWindow) -> Option<std::path::PathBuf> {
    let _ = window;
    LINE_ROOTS.with(|slots| slots.borrow().last().cloned())
}

/// This window's red line position.
pub fn line_position(window: &adw::ApplicationWindow) -> cut_line::LinePos {
    let _ = window;
    LINE_STATES
        .with(|slots| slots.borrow().last().map(|slot| slot.borrow().0))
        .unwrap_or(cut_line::LinePos { t: 0.0 })
}

/// Set this window's line WITHOUT saving -- the test/setup entry point. Saving is a side effect of a
/// user moving the line, not of being told where it is, and a setup call that consumed the throttle
/// would make the next real move silently unwritable for a second.
pub fn set_line_position(window: &adw::ApplicationWindow, pos: cut_line::LinePos) {
    let _ = window;
    // F2.12 S1: a placed line IS a target for Insert, and this is where the page learns one exists.
    note_place(true);
    LINE_STATES.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            slot.borrow_mut().0 = pos;
        }
    });
    // F3.6 S5 (`applied in the preview even while paused`): a line that LANDED on a second, so that second's
    // loudness goes to sound now — a parked line inside a raised passage must be heard at that loudness or the
    // fades cannot be judged without playing. `note_place(true)` is what says "a person put the line down":
    // `set_line_position` is also the setup door (snapshots and tests park a line before any hand was there), and
    // a gain ask for a second nobody chose would be the app speaking for a user. While TRANSPORT is running the
    // tick hands the same number itself, so this asks only of a still frame and the two never both speak.
    if place_known() && !preview_player(window).transport.playing {
        crate::ui::cut_volume_hear::apply_paused_gain(window, pos.t);
    }
}

/// A user moved the line: store it and let [`cut_line::LineWriter`] decide whether that is worth a
/// save right now (at most one write a second, `cut_line::LINE_WRITE_MS`). Returns whether it wrote.
///
/// No project root reaches the page yet (the cut-model round owns it), so there is nowhere to write;
/// the throttle is still advanced so the rate rule holds the moment a root exists.
pub fn move_line_and_save(window: &adw::ApplicationWindow, pos: cut_line::LinePos, now_ms: u64) -> bool {
    // A root DOES reach the page (`line_root`), so a move writes through the writer, which keeps its own
    // one-second rule. With no root the rate rule still holds via the throttle stamp alone.
    let root = line_root(window);
    LINE_STATES.with(|slots| {
        let Some(slot) = slots.borrow().last().cloned() else {
            return false;
        };
        slot.borrow_mut().0 = pos;
        let Some(root) = root else {
            // No project open: keep the one-second rule on the stamp alone, for when a root exists.
            let owed = cut_line::may_write_line(slot.borrow().1.last_write_ms(), now_ms, false);
            if owed {
                slot.borrow_mut().1.note_throttled(now_ms);
            }
            return owed;
        };
        slot.borrow_mut().1.note_move(pos, &root, now_ms);
        true
    })
}

/// F2.4 S4: the close path. Writes the line through the one-second rate limit — `LineWriter::flush`
/// asks `may_write_line(.., closing: true)`, so a position reached 10 ms after the last save still lands
/// (the prototype's `lineSaveMs` / `cut_line::LINE_WRITE_MS` caps MOVES, not the close). Returns
/// whether bytes went out: `false` when this window has no root or no slot to write from.
pub fn flush_line_on_close(window: &adw::ApplicationWindow) -> bool {
    let Some(root) = line_root(window) else {
        return false;
    };
    LINE_STATES.with(|slots| {
        let Some(slot) = slots.borrow().last().cloned() else {
            return false;
        };
        let pos = line_position(window);
        // Bound to a `let` so the RefMut drops before `slot` does — as a bare tail expression its
        // temporary outlives the binding and the borrow checker refuses it.
        let wrote = slot.borrow_mut().1.flush(pos, &root, now_ms());
        wrote
    })
}

/// F2.4 S1 through the seam, with the row the pointer landed on: a press places the line, clears the
/// selection and watches THAT row exactly as [`cut_line::click_outcome_in_row`] says.
pub fn place_line_press(
    window: &adw::ApplicationWindow,
    hit: cut_line::Hit,
    now_ms: u64,
) -> cut_line::ClickOutcome {
    // F2.2 S2: a click on the picture is another way in that cannot switch the preview, so an empty
    // cut refuses it here rather than placing a line over nothing (nothing moves, clears or watches).
    let refused = hit.on_picture && cut_play_leg::other_way_in(&review_cut_of(window)).is_some();
    let outcome = if refused {
        cut_line::refused_click(hit.at)
    } else {
        cut_line::click_outcome_in_row(hit.at, hit.on_picture, hit.on_scene_picture, hit.playing, hit.sources, hit.gutter, hit.row)
    };
    if outcome.line_moved {
        move_line_and_save(window, cut_line::LinePos { t: outcome.line_at }, now_ms);
    }
    if outcome.clears_selection {
        clear_selection(window);
    }
    set_watched_row(outcome.watches);
    if outcome.takes_scene {
        let under = cut_delete::scene_under(&newest_review_cut().segs, outcome.line_at).cloned();
        set_held_clip(under);
    }
    outcome
}



/// F2.4 S2: ‹f / f› steps one frame (Shift five). With a hold, the HOLD moves; with nothing held the
/// recording under the line seeks and pauses.
pub fn step_the_line(
    window: &adw::ApplicationWindow,
    shift: bool,
    held: Option<cut_line::Held>,
    fps: f64,
    now_ms: u64,
) -> Step {
    let frames = cut_line::step_frames(shift);
    if held.is_some() {
        // Nudging the held edge/clip/effect needs the held item's own geometry, which belongs to the
        // cut-editing rounds; the returned frame count is what the caller acts on once that exists.
        return Step::Nudged { frames };
    }
    let from = line_position(window).t;
    let to = cut_line::step_line(from, frames, fps);
    move_line_and_save(window, cut_line::LinePos { t: to }, now_ms);
    // S2's "then pause": stepping is looking at a frame, not watching a run.
    pause_for_step(window);
    // And the frame follows: the preview shows the new second and the view keeps it on screen.
    cut_picture::note_line_moved(window, to);
    crate::ui::cut_view::follow_playhead(window, to);
    Step::SteppedTo { t: to }
}

/// F2.4 S3: ← / → step only while something is held; with nothing held they do nothing at all.
pub fn arrow_steps(
    window: &adw::ApplicationWindow,
    shift: bool,
    held: Option<cut_line::Held>,
    fps: f64,
    now_ms: u64,
) -> Step {
    if !cut_line::arrow_moves_line(held) {
        return Step::Still;
    }
    step_the_line(window, shift, held, fps, now_ms)
}

/// F2.4 S3: Space toggles the preview unless a text box has the focus.
///
/// F2.2 S2 adds the refusal in front of the toggle: Space is one of the ways in that CANNOT switch the
/// preview (only ▶✂ does that), so on an empty cut it says what to do instead of starting a player
/// with nothing to play. The sentence comes from `cut_play_leg::other_way_in`, i.e. from `cut_play`.
pub fn space_toggles(window: &adw::ApplicationWindow, text_focus: bool) -> bool {
    let toggle = cut_line::space_toggles_preview(text_focus);
    if toggle && !text_focus {
        // S2: an empty cut refuses this way in and leaves the transport where it was.
        if let Some(refusal) = cut_play_leg::other_way_in(&review_cut_of(window)) {
            if let Some(status) = find_status(window.upcast_ref()) {
                status.set_text(refusal);
            }
            return false;
        }
        if let Some(player) = live_player(window) {
            let mut held = player.borrow_mut();
            held.transport.playing = !held.transport.playing;
            held.transport.started = held.transport.playing || held.transport.started;
        }
    }
    toggle
}

/// Find one of the four named line-step buttons.
pub fn line_step_button(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), name)?.downcast().ok()
}

/// This window's key controller for the line (F2.4 S2/S3): ‹‹f ‹f f› ›› step one or five frames,
/// ← / → move only while something is held, Space toggles play.
pub fn line_key_controller(window: &adw::ApplicationWindow) -> Option<gtk::EventControllerKey> {
    let _ = window;
    LINE_KEYS.with(|cell| cell.borrow().clone())
}

thread_local! {
    static LINE_KEYS: std::cell::RefCell<Option<gtk::EventControllerKey>> =
        const { std::cell::RefCell::new(None) };
}

/// Wire one step button to its signed frame count. A button press holds nothing, so this is always
/// S2's seek-and-pause; the direction lives in `frames`, never inferred from the widget.
fn wire_line_step(button: &gtk::Button, window: &adw::ApplicationWindow, frames: i64) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        let _ = step_frames_of(&window, frames);
    });
}

/// Move the line by a signed number of frames and pause (F2.4 S2). The seam the buttons use; the rate
/// comes from [`crate::cut_fps`] (the real ffprobe over the footage file), not a constant.
pub fn step_frames_of(window: &adw::ApplicationWindow, frames: i64) -> Step {
    let from = line_position(window).t;
    let to = cut_line::step_line(from, frames, crate::cut_fps::session_fps(window));
    move_line_and_save(window, cut_line::LinePos { t: to }, now_ms());
    pause_for_step(window);
    // The frame the user sees after a step is Prepare's JPEG nearest the new second -- the same frame a
    // click cues -- and the view recentres if the step took the line off screen.
    cut_picture::note_line_moved(window, to);
    crate::ui::cut_view::follow_playhead(window, to);
    Step::SteppedTo { t: to }
}

/// S2's "then pause", all three ways something can play: the transport flag, ▶'s process, ▶✂'s pair.
fn pause_for_step(window: &adw::ApplicationWindow) {
    if let Some(player) = live_player(window) {
        player.borrow_mut().transport.playing = false;
    }
    if crate::play_back::playing_pid().is_some() {
        crate::play_back::halt();
    }
    if crate::cut_play_back::playing_pid().is_some() {
        crate::cut_play_back::halt();
    }
}
/// A monotone-ish millisecond clock for the write throttle. Coarse is fine: the rule it feeds only asks
/// whether a second has passed.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Add the keyboard controller to the window. Called after `set_content`, like every other `wire_*`.
fn wire_line_keys(window: &adw::ApplicationWindow) {
    let controller = gtk::EventControllerKey::new();
    let win = window.clone();
    controller.connect_key_pressed(move |_ctrl, key, _code, mods| {
        let shift = mods.contains(gtk::gdk::ModifierType::SHIFT_MASK);
        // S2/S3: the hold comes from the window's own held state, not a constant. With `None` hardcoded
        // here a real key press could never take S2's nudge branch, however much was in hand.
        let held = held_now();
        // F3.8 S2 (`Frame steps and ←/→ nudge a held effect unsnapped`): with an EFFECT in hand the arrows
        // move that record, not the line. The step is `cut_line::step_frames(shift)` frames converted to
        // seconds at the page's fps, and it lands exactly there: `nudge_held_effect` goes through
        // `effect_rules::nudge`, which never consults snap marks, so a nudged band stays precisely one frame
        // off the segment end it was beside. A clip or edge in hand keeps its existing path untouched.
        if held_effect().is_some() && matches!(key, gtk::gdk::Key::Left | gtk::gdk::Key::Right) {
            let frames = cut_line::step_frames(shift) as f64 / crate::cut_fps::session_fps(&win);
            let delta = if key == gtk::gdk::Key::Left { -frames } else { frames };
            let _ = nudge_held_effect(&win, delta);
            return glib::Propagation::Stop;
        }
        // One read of the session's rate per press: every branch below steps at the same number.
        let fps = crate::cut_fps::session_fps(&win);
        let response = match key {
            gtk::gdk::Key::Left => arrow_steps(&win, shift, held, fps, now_ms()),
            gtk::gdk::Key::Right => arrow_steps(&win, shift, held, fps, now_ms()),
            gtk::gdk::Key::comma => step_the_line(&win, shift, held, fps, now_ms()),
            gtk::gdk::Key::period => step_the_line(&win, shift, held, fps, now_ms()),
            gtk::gdk::Key::space => {
                // Space is not a line step, so it answers the propagation directly rather than with a
                // `Step`; handled means the focused widget never sees it.
                return if space_toggles(&win, false) {
                    glib::Propagation::Stop
                } else {
                    glib::Propagation::Proceed
                };
            }
            _ => return glib::Propagation::Proceed,
        };
        // Every handled key stops the default action; `Still` deliberately does not, so an unheld
        // arrow keeps doing whatever the focused widget would have done.
        if matches!(response, Step::Still) { glib::Propagation::Proceed } else { glib::Propagation::Stop }
    });
    window.add_controller(controller.clone());
    LINE_KEYS.with(|cell| *cell.borrow_mut() = Some(controller));
}

/// The Cut page's ▶ (F2.1), found by name the way [`play_button`] finds the run bar's.
pub fn play_recording_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "play-recording-button")?
        .downcast()
        .ok()
}

/// The Cut page's ▶✂ (F2.2), found by name the way [`play_recording_button`] finds ▶.
pub fn play_cut_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "play-cut-button")?
        .downcast()
        .ok()
}

/// The Cut page's ▶✂✂ (F2.3), found by name the way [`play_cut_button`] finds ▶✂.
pub fn review_cuts_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "review-cuts-button")?
        .downcast()
        .ok()
}

/// F2.8 S1: the strip's own cut, read back — the visible state a trim writes and the painter reads.
pub fn review_cut_segs(window: &adw::ApplicationWindow) -> Vec<cut::Seg> {
    let _ = window;
    newest_review_cut().segs
}

/// How many segments this window's cut holds right now — the read side of `seed_review_cut`, so a test can
/// see whether a gesture changed the list.
pub fn review_cut_segs_count(window: &adw::ApplicationWindow) -> usize {
    let _ = window;
    newest_review_cut().segs.len()
}

/// This window's shift correction (F2.8 S2), read straight off the cut the gesture wrote. A slide of a
/// recording or a row lands here, so a test can assert WHICH sources moved and by how much instead of
/// trusting the status sentence alone.
pub fn review_cut_shift(
    window: &adw::ApplicationWindow,
) -> std::collections::BTreeMap<String, f64> {
    let _ = window;
    newest_review_cut().shift.clone()
}

/// This window's lanes (F2.9 S3), by their stable read: ⇲ Lane adds one and cuts nothing, so a test
/// asserts the lane arrived while `review_cut_segs_count` stayed put.
pub fn review_lanes(window: &adw::ApplicationWindow) -> Vec<cut::Lane> {
    let _ = window;
    newest_review_cut().lanes
}

/// F2.13 S3: the whole cut this window holds, as a read seam. Clear takes the scenes and the effects off
/// and keeps everything recording-side, so a check has to read `fx`, `rows` and `nrows` too — the three
/// the narrower accessors above do not cover. Reading only: no rule lives here.
pub fn review_cut_of(window: &adw::ApplicationWindow) -> cut::Cut {
    let _ = window;
    newest_review_cut()
}

/// This window's preview as it currently stands — for a test that fired the real button and wants to
/// check the same state the logic test checks rather than a painted pixel.
pub fn preview_player(window: &adw::ApplicationWindow) -> Player {
    let _ = window;
    PREVIEW_PLAYERS.with(|slots| {
        slots
            .borrow()
            .last()
            .map(|player| *player.borrow())
            .unwrap_or_default()
    })
}

/// This window's live preview handle — the newest registered player, which is the one this window's
/// button moves. Shared by every accessor below so they all read and write the same object.
pub(crate) fn live_player(window: &adw::ApplicationWindow) -> Option<Rc<RefCell<Player>>> {
    let _ = window;
    PREVIEW_PLAYERS.with(|slots| slots.borrow().last().cloned())
}

/// Put the red line somewhere before a press, which is what placing the cursor on the timeline does in
/// the finished page. F2.1 S2 plays from here, so a test seeds it rather than depending on a click
/// landing on a track that this round does not draw.
pub fn set_playhead(window: &adw::ApplicationWindow, at: f64) {
    if let Some(player) = live_player(window) {
        player.borrow_mut().playhead = Some(at);
    }
    crate::ui::cut_view::follow_playhead(window, at);
    // The tick writes the playhead; the playing half reads the smoothed clock at paint time, so this is what
    // keeps the black mask and the camera layer from drifting off the picture between ticks.
    crate::ui::cut_preview_frame::repaint(window);
}

/// Stand the preview in one of its three modes (recording / ✂ cut / ▶✂✂ review), playing or not,
/// before a press — which is what the ✂ toggles and the transport itself do on the finished page.
pub fn set_preview_state(
    window: &adw::ApplicationWindow,
    cut_only: bool,
    reviewing: bool,
    transport: run::Transport,
) {
    if let Some(player) = live_player(window) {
        let mut held = player.borrow_mut();
        held.cut_only = cut_only;
        held.reviewing = reviewing;
        held.transport = transport;
    }
}

/// The seam F2.1's button calls: [`preview::press_recording`] decides which of switch-to-the-
/// recording, pause or play-from-the-red-line the press means; this forwards it and paints only what
/// the answer says.
///
/// `runs` are the filmed stretches ([`crate::timeline::filmed_runs`]) — needed because a session
/// with nothing filmed has nothing to start at, and starting at 0 would pretend otherwise.
///
/// The player's own `Transport` is deliberately NOT pushed into the shell's [`run::RunBar`]: the bar
/// tracks a *run* (`Run { step, paused, .. }`), not a preview transport, and folding one into the
/// other would mean changing its API for a state it has no rule about. The two stay separate until a
/// round owns both.
pub fn press_play_recording(window: &adw::ApplicationWindow, runs: &[(f64, f64)]) -> Press {
    let Some(status) = find_status(window.upcast_ref()) else {
        panic!("the window has no status line");
    };
    let Some(player) = PREVIEW_PLAYERS.with(|slots| slots.borrow().last().cloned()) else {
        panic!("the window has no preview player");
    };
    let pressed = preview::press_recording(&mut player.borrow_mut(), runs);
    match pressed {
        // S1: the clock just changed meaning, so the sentence that says so goes where a press's
        // answer is read.
        Press::SwitchedToRecording { .. } => status.set_text(preview::RECORDING_STATUS),
        // S2: playing and pausing need no sentence — the bar's own progress owns the line while a
        // preview runs, and a pause leaves the previous one standing.
        Press::Playing { .. } | Press::Paused => {}
        Press::NoFootage => status.set_text(NO_RECORDING_STATUS),
    }
    pressed
}



/// The placeholder surface's pixels-per-second (F2.6). Named here rather than inlined so the widget
/// test converts its drag with the SAME number the handler uses -- two copies of 4.0 in two files are
/// how a drag assertion starts disagreeing with the code it tests.
pub const SELECT_SURFACE_PPS: f64 = cut_screen::ZOOM_AT_OPEN;

/// F2.4 S1: the click gesture that places the red line, newest window winning like [`SELECT_GESTURES`].
/// A test emits `released` on this and reads back what the rule stored.
pub fn line_click_gesture(window: &adw::ApplicationWindow) -> Option<gtk::GestureClick> {
    let _ = window;
    LINE_CLICKS.with(|cell| cell.borrow().clone())
}

thread_local! {
    static LINE_CLICKS: std::cell::RefCell<Option<gtk::GestureClick>> =
        const { std::cell::RefCell::new(None) };
}

/// Attach a left-button click to this window's `select-surface` so a real press places the line.
fn wire_line_click_surface(window: &adw::ApplicationWindow) {
    let Some(area) = select_surface(window) else { return };
    let gesture = gtk::GestureClick::new();
    // Left button only: the right button belongs to trim/move (F2.8) and must not move the line.
    gesture.set_button(1);
    let win = window.clone();
    gesture.connect_released(move |_g, _n_press, x, y| {
        crate::ui::cut_line_press::press_strip(&win, x, y, now_ms());
    });
    area.add_controller(gesture.clone());
    LINE_CLICKS.with(|cell| *cell.borrow_mut() = Some(gesture));
}

/// Attach the same press to the PAINTED band, where the geometry the rule reads is.
pub fn wire_band_line_press(window: &adw::ApplicationWindow) {
    let Some(content) = window.content() else { return };
    for name in ["track-band", "track-playhead"] {
        if let Some(band) = find_widget_by_name(&content, name) {
            crate::ui::cut_line_press::attach(window, &band);
        }
    }
}

thread_local! {
    /// The gesture attached to the painted band, kept so a widget test can fire it the toolkit's way.
    static BAND_LINE_CLICKS: std::cell::RefCell<Option<gtk::GestureClick>> =
        const { std::cell::RefCell::new(None) };
}
/// Store the band's line gesture (called by [`crate::ui::cut_line_press::attach`]).
pub(crate) fn store_band_line_gesture(gesture: gtk::GestureClick) {
    BAND_LINE_CLICKS.with(|cell| *cell.borrow_mut() = Some(gesture));
}

/// The millisecond clock the line throttle reads, for the page half that places a line.
pub fn line_now_ms() -> u64 {
    now_ms()
}

/// This window's line-placing gesture on the painted band. Test-only reader: the page attaches the gesture
/// itself (`cut_line_press::attach`) and never asks for it back, so a name saying so keeps the program path
/// honest about what calls this.
pub fn band_line_gesture_for_test(window: &adw::ApplicationWindow) -> Option<gtk::GestureClick> {
    let _ = window;
    BAND_LINE_CLICKS.with(|cell| cell.borrow().clone())
}

/// Attach the drag gesture to this window's `select-surface` and remember it. S1's scope is chosen once at
/// the press and the band is drawn FROM THE SECOND THE HAND LANDED ON to the second under it (never from 0:
/// a drag that always started at the head of the tape could not select a stretch in the middle of a
/// session, which is the only thing a selection is for). Scope resolution stays `cut_select::draw`'s.
fn wire_select_surface(window: &adw::ApplicationWindow) {
    let Some(area) = select_surface(window) else { return };
    let gesture = gtk::GestureDrag::new();
    let win = window.clone();
    // F2.6 S1: the press decides the scope, once. Which band the y fell in (ruler / picture row / wave
    // strip / lane / selection band / effects lane) is answered by `cut_select::surface_at` -- a pure
    // function over the drawn bands -- and remembered, so the drag below never re-decides it. The begin x
    // goes with it: that second is the band's start.
    gesture.connect_drag_begin(move |_g, sx, sy| {
        let (bands, rows) = placeholder_surface_bands();
        let surface = cut_select::surface_at(sy, &bands, rows);
        set_pressed_surface(surface);
        crate::ui::cut_select_press::set_drag_begin(sx / SELECT_SURFACE_PPS);
        // The effects lane refuses a selection and puts a held effect down instead — at the press, once,
        // so dragging across it cannot put the same effect down on every update.
        if surface == cut_select::Surface::FxLane {
            press_fx_lane(&win);
        }
    });
    let fx_win = window.clone();
    gesture.connect_drag_update(move |_g, x, y| {
        let surface = pressed_surface(&fx_win).unwrap_or_else(|| {
            let (bands, rows) = placeholder_surface_bands();
            cut_select::surface_at(y, &bands, rows)
        });
        // The effects lane takes nothing back: it puts a held effect down and draws no band, so the
        // selection that was there stays exactly as it was.
        if surface == cut_select::Surface::FxLane {
            return;
        }
        // Pixels to seconds at the page's current zoom; until the tracks exist the placeholder runs at
        // the open-at zoom so a drag still lands on real session seconds.
        let from = crate::ui::cut_select_press::drag_begin().unwrap_or(0.0);
        draw_selection(
            &fx_win,
            surface,
            crate::ui::cut_select_press::recording_for_surface(&fx_win, surface, y).as_deref(),
            from,
            x / SELECT_SURFACE_PPS,
        );
    });
    area.add_controller(gesture.clone());
    SELECT_GESTURES.with(|cell| *cell.borrow_mut() = Some(gesture));
}

/// Wire the ✕ to [`clear_selection`] and nothing else: the rule for what clearing means (and that it
/// clears only the selection, never the cut) lives in `cut_select::clear`.
/// Wire the ✕ to [`cut_verb_press::press_badge`] with the `selection` badge and nothing else: the rule for
/// what clearing means (and that it clears only the selection, never the cut) lives in `cut_select::clear`,
/// and the dispatch of a ✕ lives in the one badge door every ✕ on this page fires through.
fn wire_clear_selection(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        crate::ui::cut_verb_press::press_badge(&window, crate::cut_verbs::Badge::Selection, 0);
    });
}

/// F2.7: wire one verb button to its seam. The callback holds no rule — it presses, prints the answer
/// and leaves the state where [`cut_verbs`] said to leave it.
fn wire_verb_button(
    button: &gtk::Button,
    window: &adw::ApplicationWindow,
    press: fn(&adw::ApplicationWindow) -> cut_verbs::Outcome,
) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        press(&window);
    });
}

/// The doors moved to [`crate::ui::cut_verb_press`] (this file is past its size budget); naming them here
/// is what the button wires below use, so no copy of a verb door lives in two files.
use crate::ui::cut_verb_press::{press_add, press_delete_key, press_remove, press_split};

/// F3.12 S1–S5: hold this window's effects to the cut as it will actually play, and say what went.
///
/// The rule lives in [`crate::cut_clamp`]; this seam only reads the page, runs the pass over the newest
/// review cut (the scenes AFTER snapping, dead-air removal, mark removal and coalescing — which is the whole
/// reason the pass exists), and writes back what survives. It is reached from the Cut page through
/// [`crate::ui::cut_verb_press::report_verb`], so every verb that rewrites the segment list clamps the
/// effect list against it in the same step; `suggest::apply` runs the same function on the model's own
/// answer.
///
/// A no-op writes NOTHING at all — not even a status-free history entry: `record_edit` feeds
/// `History::note_change`, but a clamp that changed nothing must not consume an Undo step or repaint the lane.
pub fn clamp_effects_to_cut(window: &adw::ApplicationWindow) -> String {
    let before = newest_review_cut();
    let clamped = crate::cut_clamp::clamp_to_cut(&before);
    // Nothing dropped and nothing trimmed: the effects already sit inside the footage the cut keeps.
    if clamped.dropped == 0 && clamped.kept == before.fx {
        return "nothing to clamp \u{2014} every effect sits inside the footage the cut keeps".to_string();
    }
    let mut cut_ = before.clone();
    cut_.fx = clamped.kept.clone();
    // Through `record_edit`, NEVER `publish_cut` alone: one ↶ takes the clamp back exactly as it takes any
    // other edit back (§F3.12's "same Undo step as the segments").
    record_edit(window, &cut_);
    refresh_effects_lane(window);
    let said = if clamped.dropped > 0 {
        // S5's line goes to the log, where the count is read with the rest of the run; the status line says
        // the same thing shorter. Which effect went is NOT said (§12 decision 19).
        log_line(&crate::cut_clamp::log_line(clamped.dropped));
        format!(
            "effects clamped to the cut \u{2014} {} dropped",
            clamped.dropped
        )
    } else {
        format!(
            "effects trimmed to the cut \u{2014} {} kept",
            cut_.fx.len()
        )
    };
    if let Some(status_line) = find_status(window.upcast_ref()) {
        status_line.set_text(&said);
    }
    said
}

/// Write this window's cut's segment list, keeping everything else about the cut (lanes, shifts, rows,
/// folds) exactly as it was — the write side of [`newest_review_cut`], used by the verb doors.
pub(crate) fn set_review_segs(window: &adw::ApplicationWindow, segs: &[cut::Seg]) {
    let _ = window;
    REVIEW_CUTS.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            slot.borrow_mut().segs = segs.to_vec();
        }
    });
}

/// What ⌦ would take first: the effect in hand. Set by the effects-lane rounds when a person picks one
/// up; a seam today so the delete order is testable before those pixels exist.
///
/// `None` puts the effect down and nothing else ([`crate::ui::cut_hand::drop_effect`]): the door that deleted
/// the effect does not also decide what the next ⌦ takes.
pub fn set_held_effect(effect: Option<cut::Fx>) {
    match effect {
        Some(fx) => {
            crate::ui::cut_hand::pick_up(crate::ui::cut_hand::Hand::Effect(fx));
        }
        None => crate::ui::cut_hand::clear_all(),
    };
}

/// Read side of [`set_held_effect`].
pub fn held_effect() -> Option<cut::Fx> {
    crate::ui::cut_hand::held_effect()
}

/// What ⌦ takes next: the clip in hand — `the only way to remove a spliced card` (§J rule 8). Same
/// seam arrangement as the held effect: real drags set it from F2.8/F2.9 onward.
pub fn set_held_clip(seg: Option<cut::Seg>) {
    match seg {
        Some(seg) => {
            crate::ui::cut_hand::pick_up(crate::ui::cut_hand::Hand::Clip(seg));
        }
        None => crate::ui::cut_hand::clear_all(),
    };
}

/// Read side of [`set_held_clip`]: the hand holds a WHOLE clip, as opposed to one of its edges.
pub fn held_clip() -> Option<cut::Seg> {
    crate::ui::cut_hand::held_clip()
}

/// The segment in hand either way — whole clip or held edge — for the doors that move a segment without asking
/// which of the two the hand took.
pub fn held_segment() -> Option<cut::Seg> {
    crate::ui::cut_hand::held_clip_or_edge()
}

/// F2.4 S1b: put a picked clip EDGE in the hand (the 12 px reach, `// layout.lineReachPx`).
pub fn set_held_edge(seg: Option<cut::Seg>) {
    match seg {
        Some(seg) => {
            crate::ui::cut_hand::pick_up(crate::ui::cut_hand::Hand::Edge(seg));
        }
        None => crate::ui::cut_hand::clear_all(),
    };
}

/// Read side of [`set_held_edge`].
pub fn held_edge() -> Option<cut::Seg> {
    crate::ui::cut_hand::held_edge()
}

/// F2.4 S2/S3: what the line is holding right now, in the order the spec steps them — edge first, then
/// the whole clip, then an effect. This is what a frame-step or arrow press asks; with the hardcoded
/// `None` that stood here before, a real key press could never take the nudge branch at all.
pub fn held_now() -> Option<cut_line::Held> {
    crate::ui::cut_hand::held_kind()
}

/// F2.4 S1b: turn what the first left press picked into the hold it means. Edge goes to the edge slot so
/// a frame step moves only that border; a border or whole-clip pick takes the clip.
pub fn apply_pick(pick: cut_line::PressPick, seg: Option<cut::Seg>) {
    match pick {
        cut_line::PressPick::Edge => set_held_edge(seg),
        cut_line::PressPick::Border | cut_line::PressPick::Clip => set_held_clip(seg),
    }
}

/// F2.4 S1: which picture row this window watches. Set by a picture-band click through
/// [`place_line_press`]; read by whatever opens a row on the watched one.
pub fn watched_row() -> Option<usize> {
    WATCHED_ROW.with(|cell| *cell.borrow())
}

/// Write side of [`watched_row`].
pub fn set_watched_row(row: Option<usize>) {
    WATCHED_ROW.with(|cell| *cell.borrow_mut() = row);
}

thread_local! {
    /// F2.12 S1: whether this session has been told where its red line stands. `None` means no line was
    /// ever placed, which is the state that makes "click the timeline where the insert goes first" true.
    pub(crate) static INSERT_PLACE_KNOWN: std::cell::RefCell<Option<bool>> =
        const { std::cell::RefCell::new(None) };
    /// F2.4 S1: which picture row this window is watching. Only a picture-band click sets it, and only
    /// while nothing plays with sources loaded — `cut_line::watches_row` owns that; this just remembers.
    static WATCHED_ROW: std::cell::RefCell<Option<usize>> =
        const { std::cell::RefCell::new(None) };
}

// --- F2.8: trim and move on the track strip -------------------------------------------------------------


pub use crate::ui::cut_view::{band_clip_boxes_for_test, strip_clip_boxes_for_test, strip_pps};





















// --- F2.9: copy, paste and lane --------------------------------------------------------------------------









// --- F2.12: insert a card, still, video or sound ---------------------------------------------------------

/// The insert form's holder, wherever this window put it. `None` on a page that drew none (Prepare, Narrate,
/// Produce) — every insert control lives inside this box, so one lookup finds the whole form and nothing
/// outside it.
fn insert_form_box(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    find_widget_by_name(window.upcast_ref(), "insert-form")?
        .downcast::<gtk::Box>()
        .ok()
}

/// The holder that is DRAWING a form right now, as opposed to the empty ones the other tabs carry (see
/// `page_box`: an unscoped name exists once per tab, and a tree-wide lookup can land on a hidden one). A
/// snapshot of the form asks this rather than the name, so the shot cannot be of an empty box.
pub fn visible_insert_form(window: &adw::ApplicationWindow) -> Option<gtk::Widget> {
    fn walk(root: &gtk::Widget) -> Option<gtk::Widget> {
        if root.widget_name() == "insert-form"
            && root.is_visible()
            && root.observe_children().n_items() > 0
        {
            return Some(root.clone());
        }
        for child in root.observe_children().iter::<glib::Object>() {
            let Ok(child) = child else { continue };
            let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
            if let Some(found) = walk(&widget) {
                return Some(found);
            }
        }
        None
    }
    walk(window.upcast_ref())
}

/// What the open form stands for, so Apply can place the file the chooser returned at the second the page
/// read when Insert was pressed. Held per window because two windows may each have an insert half-finished;
/// the newest one is the one whose buttons were just clicked.
#[derive(Debug, Clone)]
struct InsertOpen {
    path: String,
    at: f64,
    seconds: f64,
}

thread_local! {
    static INSERT_OPEN: std::cell::RefCell<Option<InsertOpen>> =
        const { std::cell::RefCell::new(None) };
}

/// The form the page is showing, if any — what a widget test reads to assert the same state the logic test
/// checked ([`cut_insert::FormPlan`] plus where the insert will land).
pub fn insert_open(window: &adw::ApplicationWindow) -> Option<(String, f64, f64)> {
    let _ = window;
    INSERT_OPEN.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|open| (open.path.clone(), open.at, open.seconds))
    })
}

/// F2.12 S4: draw the form from a [`cut_insert::FormPlan`]. Every label, every conditional control and the
/// opening length come out of the plan; this only turns them into widgets with stable names.
///
/// The three radios are a group by construction: GTK lets one of several `RadioButton`s share a group, and
/// the LANE radio is only ever created when the plan says video, so a non-video file cannot be put on a lane
/// even by a stray click. The sound tick is drawn only when the plan says there is a sound to answer for, and
/// greyed while LANE is chosen — a lane cuts nothing, so there is no sound underneath to keep or drop yet.
fn show_insert_form(
    window: &adw::ApplicationWindow,
    plan: &cut_insert::FormPlan,
    path: &str,
    at: f64,
    fields: Vec<cut_insert::Field>,
) {
    let Some(holder) = insert_form_box(window) else {
        return;
    };
    // Clear the previous form out of THE HOLDER, never by a tree-wide name search (see the fold badges for
    // why: a closed window's widgets stay parented somewhere).
    let stale: Vec<gtk::Widget> = holder
        .observe_children()
        .iter::<glib::Object>()
        .flatten()
        .filter_map(|child| child.downcast::<gtk::Widget>().ok())
        .collect();
    for old in stale {
        holder.remove(&old);
    }

    let heading = gtk::Label::new(Some(&format!(
        "{} \u{2014} {}",
        cut_insert::edit_verb(cut_insert::hold(held_segment().as_ref())),
        cut_insert::base_name(path)
    )));
    heading.set_widget_name("insert-heading");
    heading.set_xalign(0.0);
    holder.append(&heading);

    // One entry per declared card field, in the order the card asked (S4). A logo field gets its Logo\u{2026}
    // picker beside the entry rather than instead of it: the typed value survives a re-open either way.
    for field in &fields {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let key = gtk::Label::new(Some(&field.label));
        key.set_widget_name(&format!("insert-label-{}", field.key));
        key.set_xalign(0.0);
        row.append(&key);
        let entry = gtk::Entry::new();
        entry.set_widget_name(&format!("insert-field-{}", field.key));
        entry.set_text(&field.value);
        entry.set_hexpand(true);
        if !field.hint.is_empty() {
            entry.set_tooltip_text(Some(&field.hint));
        }
        row.append(&entry);
        if field.logo {
            let picker = gtk::Button::with_label("Logo\u{2026}");
            picker.set_widget_name(&format!("insert-logo-{}", field.key));
            picker.set_tooltip_text(Some("pick a mark to sit beside the title"));
            row.append(&picker);
        }
        holder.append(&row);
    }

    // The three modes (§G's labels verbatim), BETWEEN first as the default for no selection. GTK 4 has no
    // `RadioButton` type: a toggle in a group is a `CheckButton` drawn as a radio and grouped by
    // `set_group`, which is what makes "exactly one mode" true rather than a convention.
    let between = gtk::CheckButton::with_label(cut_insert::BETWEEN_LABEL);
    between.set_widget_name("insert-mode-between");
    let over = gtk::CheckButton::with_label(cut_insert::OVER_LABEL);
    over.set_widget_name("insert-mode-over");
    over.set_group(Some(&between));
    holder.append(&between);
    holder.append(&over);
    // The LANE radio is drawn for video only; `lane_offered` answers that, so a still or a sound cannot be
    // put on a row of its own even by a stray click.
    if plan.lane_shown {
        let lane = gtk::CheckButton::with_label(cut_insert::LANE_LABEL);
        lane.set_widget_name("insert-mode-lane");
        lane.set_group(Some(&between));
        holder.append(&lane);
    }
    match plan.mode {
        cut_insert::Mode::Between => between.set_active(true),
        cut_insert::Mode::Over => over.set_active(true),
        cut_insert::Mode::Lane => {
            if let Some(lane) = find_widget_by_name(window.upcast_ref(), "insert-mode-lane")
                .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
            {
                lane.set_active(true);
            } else {
                between.set_active(true);
            }
        }
    }

    // The sound tick: shown only when there is a sound to answer for, greyed while LANE is chosen.
    if plan.sound_shown {
        let tick = gtk::CheckButton::with_label(plan.sound_label);
        tick.set_widget_name("insert-sound-tick");
        tick.set_sensitive(!matches!(plan.mode, cut_insert::Mode::Lane));
        holder.append(&tick);
    }

    // Seconds, opening at the plan's length (S3).
    let sec_row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let sec_key = gtk::Label::new(Some("Seconds"));
    sec_key.set_xalign(0.0);
    sec_row.append(&sec_key);
    let seconds = gtk::Entry::new();
    seconds.set_widget_name("insert-seconds");
    seconds.set_width_chars(6);
    seconds.set_text(&format!("{:.1}", plan.seconds));
    seconds.set_tooltip_text(Some(&format!(
        "at least {:.1} s \u{2014} shorter keeps the length it opened with",
        cut_select::MIN_SCENE_SECONDS
    )));
    sec_row.append(&seconds);
    holder.append(&sec_row);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let apply = gtk::Button::with_label(if cut_insert::hold(held_segment().as_ref()) {
        "Apply"
    } else {
        "Insert it"
    });
    apply.set_widget_name("insert-apply-button");
    let cancel = gtk::Button::with_label("Cancel");
    cancel.set_widget_name("insert-cancel-button");
    buttons.append(&apply);
    buttons.append(&cancel);
    holder.append(&buttons);

    INSERT_OPEN.with(|cell| {
        *cell.borrow_mut() = Some(InsertOpen {
            path: path.to_string(),
            at,
            seconds: plan.seconds,
        })
    });
    holder.set_visible(true);

    let win = window.clone();
    apply.connect_clicked(move |_| {
        let status = press_insert_apply(&win);
        if let Some(status_line) = find_status(win.upcast_ref()) {
            status_line.set_text(&status);
        }
        refresh_copy_buttons(&win);
    });
    let win = window.clone();
    cancel.connect_clicked(move |_| {
        let status = close_insert_form(&win);
        if let Some(status_line) = find_status(win.upcast_ref()) {
            status_line.set_text(&status);
        }
    });
    // Switching the mode re-draws the tick's sentence, which is the only thing the change touches on the
    // form itself (what it does to the footage is decided at Apply, by `cut_insert::place`).
    for radio in [&between, &over] {
        let win = window.clone();
        radio.connect_toggled(move |button| {
            if button.is_active() {
                refresh_insert_tick(&win);
            }
        });
    }
    if let Some(lane) = find_widget_by_name(window.upcast_ref(), "insert-mode-lane")
        .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
    {
        let win = window.clone();
        lane.connect_toggled(move |button| {
            if button.is_active() {
                refresh_insert_tick(&win);
            }
        });
    }
}

/// Read the mode the radios hold right now. No lane widget means no lane was offered, so the answer is never
/// `Lane` for a still, a card or a sound.
fn insert_mode_now(window: &adw::ApplicationWindow) -> cut_insert::Mode {
    let radio = |name: &str| {
        find_widget_by_name(window.upcast_ref(), name)
            .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
    };
    if radio("insert-mode-lane").is_some_and(|b| b.is_active()) {
        cut_insert::Mode::Lane
    } else if radio("insert-mode-over").is_some_and(|b| b.is_active()) {
        cut_insert::Mode::Over
    } else {
        cut_insert::Mode::Between
    }
}

/// Re-label and re-enable the sound tick after a mode change: the sentence comes from
/// [`cut_insert::tick_label`], the greying from the fact that a LANE has nothing under it.
fn refresh_insert_tick(window: &adw::ApplicationWindow) {
    let Some(tick) = find_widget_by_name(window.upcast_ref(), "insert-sound-tick")
        .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
    else {
        return;
    };
    let mode = insert_mode_now(window);
    tick.set_label(Some(cut_insert::tick_label(mode == cut_insert::Mode::Between)));
    tick.set_sensitive(!matches!(mode, cut_insert::Mode::Lane));
}

/// Hide the form and forget what it stood for. Returns the line the page prints.
pub fn close_insert_form(window: &adw::ApplicationWindow) -> String {
    INSERT_OPEN.with(|cell| *cell.borrow_mut() = None);
    if let Some(holder) = insert_form_box(window) {
        holder.set_visible(false);
    }
    "the insert was cancelled".to_string()
}

/// F2.12 S5: Apply / Insert it. Reads the form's own widgets, hands the answers to
/// [`cut_insert::FormAnswer::from_form`] and [`cut_insert::place`], saves the cut through the same door the
/// folds use, and returns [`cut_insert::Placed::status`] (or the refusal). No rule lives here.
pub fn press_insert_apply(window: &adw::ApplicationWindow) -> String {
    let Some(open) = INSERT_OPEN.with(|cell| cell.borrow().clone()) else {
        return cut_insert::NO_LINE_YET.to_string();
    };
    let fallback = open.seconds;
    let text = find_widget_by_name(window.upcast_ref(), "insert-seconds")
        .and_then(|w| w.downcast::<gtk::Entry>().ok())
        .map(|entry| entry.text().to_string())
        .unwrap_or_default();
    let silent = find_widget_by_name(window.upcast_ref(), "insert-sound-tick")
        .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
        .is_some_and(|tick| tick.is_active());
    let answer = cut_insert::FormAnswer::from_form(insert_mode_now(window), &text, fallback, silent);
    let mut cut_ = newest_review_cut();
    match cut_insert::place(&mut cut_, &open.path, open.at, answer.mode, answer.seconds, answer.silent) {
        Ok(placed) => {
            record_edit(window, &cut_);
            save_insert_cut(&cut_);
            INSERT_OPEN.with(|cell| *cell.borrow_mut() = None);
            if let Some(holder) = insert_form_box(window) {
                holder.set_visible(false);
            }
            placed.status
        }
        Err(refusal) => refusal,
    }
}

/// Write the mutated cut where it was read from. Same path as `save_folds`: `cut::save` writes the whole cut,
/// and the segments an insert added are fields of it. A project with no tree keeps the insert in memory only.
pub(crate) fn save_insert_cut(cut_: &Cut) {
    // §7's one writer: the session resolution and the call to `cut::save` are `cut_save`'s, not this door's.
    let _ = crate::cut_save::write_cut(cut_);
}

/// A landed cut is written by the flow that landed it — `cut_suggest_words_run` for the words cut,
/// its model twin for a model cut — because only that flow knows which session folder it read.
/// Write `cut_` to this window's own cut.json — the door every editing press uses, exposed so a check can
/// start from a file on disk rather than only from an in-memory cut.
pub fn save_seed_for_test(window: &adw::ApplicationWindow, cut_: &Cut) {
    let _ = window;
    save_insert_cut(cut_);
}

/// F2.12 S1 + S2: ⧉ Insert. The placement question is answered FIRST, before any dialog opens, because a
/// chooser the user cancels should not have cost them a refusal they did not ask for — and because the file
/// that comes back has to land where the timeline read at the press.
///
/// With a line or a selection this seeds `assets/` with the built-in cards (so the folder never opens empty),
/// then opens the file chooser titled for the scope of the seconds. Headless there is nobody to answer the
/// chooser, so nothing more happens here; the seam a widget test drives past the chooser is
/// [`insert_chosen`], which is what the chooser's callback calls.
pub fn press_insert(window: &adw::ApplicationWindow) -> String {
    // S6 first: a held card is edited, not replaced. A card that went away while its dialog was open says so
    // rather than opening a form for something that is no longer there.
    if let Some(held) = held_segment() {
        let cut_ = newest_review_cut();
        if held.is_insert() && cut_insert::find_card(&cut_, &held.ins, held.s).is_none() {
            return cut_insert::GONE.to_string();
        }
    }
    // S1: the placement question is answered BEFORE any chooser opens — a cancelled dialog should not have
    // cost the user a refusal they did not ask for, and the file that comes back has to land where the
    // timeline read at the press.
    let band = selection(window);
    let placed = place_known().then(|| line_position(window).t);
    let at = match cut_insert::needs_place(placed, band.as_ref()) {
        Ok(at) => at,
        Err(refusal) => return refusal.to_string(),
    };
    let root = std::env::current_dir().unwrap_or_default();
    let dir = startup::session_dir(&root);
    let assets = cut_insert::chooser_dir(&dir);
    cut_insert::seed_cards(&assets, &cut_cards::seeds());
    let title = cut_insert::chooser_title(band.as_ref());
    let sound_only = matches!(
        band.as_ref().map(|b| &b.scope),
        Some(cut_select::Scope::Sound { .. })
    );
    open_insert_chooser(window, title, &assets, sound_only, at);
    format!("{} \u{2014} choose the file", title)
}

/// Open the native file chooser over `dir`. Kept apart from [`press_insert`] so the decision (where, which
/// title, which filter) is testable without a display: a headless run simply never answers the dialog.
fn open_insert_chooser(
    window: &adw::ApplicationWindow,
    title: &str,
    dir: &std::path::Path,
    sound_only: bool,
    at: f64,
) {
    let chooser = gtk::FileChooserNative::builder()
        .title(title)
        .modal(true)
        .action(gtk::FileChooserAction::Open)
        .build();
    let _ = chooser.set_current_folder(Some(&gio::File::for_path(dir)));
    let filter = gtk::FileFilter::new();
    if sound_only {
        filter.set_name(Some("sounds"));
        for ext in ["mp3", "wav", "ogg", "oga", "flac", "m4a", "aac", "opus"] {
            filter.add_pattern(&format!("*.{}", ext));
        }
    } else {
        filter.set_name(Some("clips, images, animations and sounds"));
        for ext in [
            "mp4", "mkv", "mov", "webm", "avi", "svg", "svgz", "png", "jpg", "jpeg", "mp3", "wav",
            "ogg", "flac", "m4a",
        ] {
            filter.add_pattern(&format!("*.{}", ext));
        }
    }
    chooser.add_filter(&filter);
    let win = window.clone();
    chooser.connect_response(move |chooser, response| {
        if response != gtk::ResponseType::Accept {
            return;
        }
        let Some(file) = chooser.file().and_then(|f| f.path()) else {
            return;
        };
        // The same body a widget test drives, so the chooser and the seam cannot drift apart.
        insert_chosen(&win, &file, at);
    });
    chooser.show();
}

/// F2.12 S3 + S4: the file came back from the chooser — build the plan and draw the form.
///
/// `file_seconds` is NOT probed here: reading a video's duration means an ffprobe call, and the container
/// this runs in has neither ffmpeg nor a reason to spawn one per click. The plan therefore falls back to
/// [`cut_insert::DEFAULT_SECONDS`] (P.policy.insertDefaultSeconds = 4) unless the caller passes a length it
/// already knows, which is exactly what a still and an un-probed file get anyway.
pub fn insert_chosen(window: &adw::ApplicationWindow, file: &std::path::Path, at: f64) -> String {
    insert_chosen_with_length(window, file, at, None)
}

/// [`insert_chosen`] with the length the caller already has (a probe, or a card's own animation). Split out
/// so a test can hand in a length and see S3's precedence — selection seconds beat the file's, and both beat
/// the default — without a media probe.
pub fn insert_chosen_with_length(
    window: &adw::ApplicationWindow,
    file: &std::path::Path,
    at: f64,
    file_seconds: Option<f64>,
) -> String {
    let raw = file.to_string_lossy().to_string();
    let band = selection(window);
    let animated = svg_is_animated(file);
    // S3: the file's own length comes from the real probe (ffprobe), which is also what S7's render is asked
    // against. `None` — a still, an unreadable file, a machine with no ffprobe — falls back to
    // `P.policy.insertDefaultSeconds`, which is the whole point of the fallback rather than a failure.
    let probed = crate::ui::cut_card_preview::probe_card_seconds(window, &raw);
    // S3's order: a selection's seconds beat the file's, and the file's beat the default. For a card "the file's
    // own length" is its last moving moment (§5), which no probe can answer — ffprobe reads an SVG as one frame.
    // `offer_seconds` is that door; it reads the document only when there is nothing else to ask.
    let file_seconds = file_seconds
        .or(probed)
        .or_else(|| crate::produce_cards::offer_seconds(&raw));
    let plan = cut_insert::form_plan(&raw, band.as_ref(), file_seconds, animated, false, false, false);
    // The card's holes are read off the document; a non-SVG gets no fields and the door ignores the argument.
    let mut fields: Vec<cut_insert::Field> = Vec::new();
    if plan.kind == cut_insert::Kind::Svg {
        if let Ok(doc) = std::fs::read_to_string(file) {
            fields = cut_insert::card_fields(doc.as_bytes());
            for field in fields.iter_mut() {
                if let Some(value) = plan.values.iter().find_map(|(key, value)| {
                    key.eq_ignore_ascii_case(&field.key)
                        .then_some(value.clone())
                }) {
                    field.value = value;
                }
            }
        }
    }
    show_insert_form(window, &plan, &raw, at, fields);
    cut_form_frame::set_pinned(window, cut_form_frame::open_form_title(window).as_deref());
    format!(
        "{} \u{2014} {:.1} s, {} \u{2014} the form is open",
        cut_insert::base_name(&raw),
        plan.seconds,
        mode_word(plan.mode)
    )
}

/// Whether an SVG animates, read off the document: a `@keyframes` block or an `<animate>` tag. An SVG with
/// neither is drawn as a still, so it has no length of its own (see [`cut_insert::default_length`]).
fn svg_is_animated(file: &std::path::Path) -> bool {
    crate::ui::cut_card_preview::svg_is_animated(file)
}

/// The mode said back as one word, for the status line.
fn mode_word(mode: cut_insert::Mode) -> &'static str {
    match mode {
        cut_insert::Mode::Between => "spliced between the footage",
        cut_insert::Mode::Over => "over the footage",
        cut_insert::Mode::Lane => "on a lane of its own",
    }
}

/// F2.12 S6: set Insert's sensitivity and its word. Runs from the same refresh path as the verb and copy
/// buttons, so a greyed Insert is always today's answer: with no line and no selection there is nowhere for
/// an insert to go, and while a card is held the button edits that card rather than choosing a new file.
pub fn refresh_insert_button(window: &adw::ApplicationWindow) {
    if let Some(button) = line_step_button(window, "insert-button") {
        // A target means a SECOND someone chose: a placed red line, or a band whose length clears the floor
        // every other verb holds a scene to. `line_position` answers 0.0 both for "the line is on frame
        // zero" and for "no line was ever placed", so the page keeps its own answer beside it rather than
        // reading a false yes out of the default.
        let has_target = place_known() || selection(window).is_some();
        let live = match selection(window) {
            Some(band) => band.length() >= cut_select::MIN_SCENE_SECONDS,
            None => has_target,
        };
        button.set_sensitive(live);
        // F3.8 S1: `Insert becomes ✎ Edit` when an EFFECT is held too, not only a card. §A.9 says it in the
        // same breath as "band → hold", so the held-effect case joins the held-card one here rather than being
        // a second rule somewhere else. The word still comes from `cut_insert::edit_verb`, never retyped.
        let editing = cut_insert::hold(held_segment().as_ref()) || held_effect().is_some();
        button.set_label(cut_insert::edit_verb(editing));
    }
}

/// Whether this session has been told where its red line stands. Read back by
/// [`refresh_insert_button`] and by [`press_insert`]: `false` is a page that has never had a line placed,
/// which is what makes S1's refusal reachable.
fn place_known() -> bool {
    INSERT_PLACE_KNOWN.with(|cell| cell.borrow().unwrap_or(false))
}


/// S1: ⧉ Insert's wire. The button forwards here and prints what comes back, so no rule sits in the callback.
fn press_insert_from_button(window: &adw::ApplicationWindow) -> String {
    press_insert(window)
}



/// F2.8 S1: the strip's height — the ruler row plus the kept-bar row, one number the size request and
/// the painter both read so they cannot drift apart.
pub const STRIP_HEIGHT: i32 = crate::ui::cut_trim_press::STRIP_H as i32;

/// F2.8 S1: a ruler mark every 30 s at the open zoom is a mark every 120 px, which is what
/// `spec/img/05-trim.png` prints (`0:00`, `0:30`, `1:00`). Zoomed far in the spacing would crowd, but
/// the strip's zoom is fixed until F2.10/F2.11 own the real rows, so one constant covers it.
pub const RULER_EVERY_SECONDS: f64 = 30.0;

/// F2.8 S1: how wide the placeholder strip is drawn. The spec's picture runs the whole session across
/// the window; this strip is a placeholder for the rows F2.10/F2.11 own, and its width has to stay a
/// placeholder too — but wide enough that the ruler's first marks AND the clips they measure are both on
/// screen. At [`TRACK_STRIP_PPS`] 480 px covers two minutes of the session, which is where the three
/// seeded clips live; a narrower strip cut the third tick off at 240 px and left a label-less bar.
pub const STRIP_WIDTH: i32 = 480;





// F3.10 S1–S5, the pass itself, lives in [`crate::ui::cut_speeds_pass`]: window.rs is past its size budget,
// and the pass needs none of this file's state — only the doors it already exports.





/// F3.11's label. As with ✐ Captions and ⏩ Speeds, the spec's flowchart starts at a model call and names no
/// control: a named button is the decision taken in its spirit, so the pass can be started and its gate seen.
pub const DECORATIONS_PASS_LABEL: &str = "\u{2728} Decorations";

// F3.11 S1–S5, the decorations pass itself, lives in [`crate::ui::cut_effects_pass`]: window.rs is
// past its size budget, and the pass needs none of this file's state — only the doors it exports.

// --- F4.1 ▶ Write and speak -----------------------------------------------------------------
//
// The run's decisions are `crate::narrate_run`'s; what is here gathers them off this window's session,
// forwards them, and paints. Nothing in this block decides anything: the refusal order, which clips the
// one call covers, whether a line is spoken, and every sentence that ends up on screen all come from
// `narrate_run`. The narration call itself arrives through a closure for the same reason
// `run_captions_pass_with_reply` exists — a test drives the real route with a scripted answer.

/// S1 alone: does this press of ▶ get refused, and with which sentence? Reads three of the four gates
/// off the project folder; busy comes from the run bar rather than the project, so it is asked through
/// [`narrate_bar_busy`] and handed to [`narrate_run::refuse`], which owns the order.
pub fn refuse_narrate_run(project: &Project) -> Option<String> {
    refuse_narrate_run_with_busy(project, narrate_bar_busy())
}

/// The same four gates with `busy` handed in. Needed because the shell's ▶ forwards HERE, after
/// `runqueue::start_run` has already put this press's own run in the bar: reading that as "a run is
/// under way" would refuse the press with S1's PAUSING sentence for the run it just opened. The
/// handler's pre-press check (`narrate_asked` above) is where busy is honestly answered.
pub fn refuse_narrate_run_with_busy(project: &Project, busy: bool) -> Option<String> {
    let tree = narrate_session_tree();
    let has_cut = tree
        .as_ref()
        .and_then(|tree| cut::load(tree).ok())
        .is_some_and(|cut_| !cut_.segs.is_empty());
    let has_timeline = tree
        .as_ref()
        .is_some_and(|tree| tree.session_tsv().is_file());
    narrate_run::refuse(busy, has_cut, project.no_narration, has_timeline)
}

/// Whether this window's run bar counts as busy — the same question [`run::controls`] answers when it
/// picks ⏸ for the button face, read here so the refusal and the button cannot disagree.
fn narrate_bar_busy() -> bool {
    RUN_BARS
        .with(|bars| bars.borrow().last().cloned())
        .is_some_and(|bar| {
            let bar = bar.borrow();
            run::controls(&bar.running, None).icon == run::PAUSE_ICON
        })
}

/// This session's project folder, resolved the way every other flow resolves it, so "this project" means
/// one thing app-wide. `None` when no project is open.
pub fn narrate_session_tree() -> Option<layout::Tree> {
    let dir = startup::session_dir(&std::env::current_dir().unwrap_or_default());
    layout::Tree::new(&dir).ok()
}

/// F4.1 S1–S7 on this window. With no scripted reply this is the LIVE path: ▶ telephones the address
/// the Settings hold through `narrate_live`, over real HTTP, three attempts at most (§F4.2).
pub fn press_narrate_run(window: &adw::ApplicationWindow) -> String {
    // The scripted reply lives in a thread-local so the shell's own ▶ can reach it too: the real
    // button's handler calls THIS function, and a test that wants the written-and-spoken case drives
    // the button rather than the seam. Empty means "ask for real", which is what the app does.
    let reply = NARRATE_SCRIPT.with(|cell| cell.borrow().clone());
    if reply.is_empty() {
        return run_narrate_on(window, live_narrate_reply);
    }
    run_narrate_on(window, move |_tree, clips| {
        Ok(reply
            .iter()
            .filter_map(|(clip, at, text, emotion)| {
                let (start, end) = *clips.get((*clip as usize).checked_sub(1)?)?;
                Some(narrate_run::Written {
                    start,
                    end,
                    at: *at,
                    text: text.clone(),
                    emotion: emotion.clone(),
                })
            })
            .collect())
    })
}

/// The live narration leg: with no scripted reply, ▶ comes here (§F4.2's call over real HTTP). All the
/// gathering and driving lives in `narrate_ask`, so this file stays a shell.
fn live_narrate_reply(
    tree: &layout::Tree,
    bounds: &[(f64, f64)],
) -> Result<Vec<narrate_run::Written>, String> {
    crate::narrate_ask::run(tree, &live_project(), bounds)
}

/// One narration reply handed to the shell's ▶ by a test: `(clip number from 1, clip-relative second,
/// text, emotion)`, the same shape `press_narrate_run_with_reply` takes. Set it, then click
/// `play-button`; the handler reads it here instead of telephoning F4.2's endpoint, which headless has
/// none. Clearing it puts ▶ back to the no-model behaviour.
/// Feed `press_narrate_run` a scripted narration reply instead of telephoning F4.2, and let it stand
/// until the next call replaces it. A test that wants the no-model behaviour passes an empty slice.
pub fn set_narrate_script(reply: &[(u32, f64, &str, &str)]) {
    NARRATE_SCRIPT.with(|cell| {
        cell.replace(
            reply
                .iter()
                .map(|(clip, at, text, emotion)| (*clip, *at, text.to_string(), emotion.to_string()))
                .collect(),
        )
    });
}

/// Whether a scripted narration reply is loaded — exported so a test can assert the button really is
/// reading the script rather than falling through to the no-model path.
pub fn narrate_script_loaded() -> bool {
    !NARRATE_SCRIPT.with(|cell| cell.borrow().is_empty())
}

/// F4.1's body for THIS window and the live session's project. Both doors — the seam above and the
/// shell's ▶ — come through here, so the two cannot drift apart.
fn run_narrate_on<FReply>(window: &adw::ApplicationWindow, reply: FReply) -> String
where
    FReply: FnMut(
        &layout::Tree,
        &[(f64, f64)],
    ) -> Result<Vec<narrate_run::Written>, String>,
{
    let project = live_project();
    // A bar already holding a run means the press arriving here opened it (the shell's ▶ forwards
    // after `start_run`), so it must not be read as "busy" — see `refuse_narrate_run_with_busy`.
    let opened_by_this_press = bar_is_running();
    let reason = if opened_by_this_press {
        refuse_narrate_run_with_busy(&project, false)
    } else {
        refuse_narrate_run(&project)
    };
    if let Some(reason) = reason {
        say_on_status(window, &reason);
        return reason;
    }
    // A press that came through `run::RunBar::press` opened a run before arriving here; nothing
    // headless streams to close it, so this call closes it at its own end. Three writes follow the
    // close, because the handler paints from the bar AFTER this returns and `end_run` leaves both
    // `running` and `status` empty: put the sentence back on the label, repaint ▶ from the idle bar,
    // and redraw the progress area so the stage the run painted is not wiped by that later paint.
    let said = run_narrate_with(window, &project, reply);
    if opened_by_this_press {
        clear_narrate_bar();
        set_status_line(window, &said);
        repaint_play_button(window);
    } else {
        set_status_line(window, &said);
    }
    said
}

/// Repaint ▶ and ⏹ from the bar's current state after a narrate run closed it, so neither button is
/// left showing the face of a run that has ended.
fn repaint_play_button(window: &adw::ApplicationWindow) {
    let Some(bar) = RUN_BARS.with(|bars| bars.borrow().last().cloned()) else { return };
    let drawn = run::controls(&bar.borrow().running, None);
    if let Some(play) = play_button(window) {
        play.set_icon_name(drawn.icon);
        play.set_tooltip_text(Some(drawn.tooltip));
    }
    // ⏹'s sensitivity comes from the same call, as `paint_run_bar` draws them together: with the run
    // closed there is nothing left to end, so it greys out again.
    if let Some(stop_) = stop_button(window) {
        stop_.set_sensitive(drawn.stop_sensitive);
    }
}

/// Whether this window's run bar currently holds a run — the same slot [`clear_narrate_bar`] empties.
fn bar_is_running() -> bool {
    RUN_BARS.with(|bars| bars.borrow().last().cloned())
        .is_some_and(|bar| bar.borrow().running.is_some())
}

/// Write one sentence on this window's status line without logging it (the run already logged it).
/// Put this window's status row to `said` — the one door every page uses.
pub fn set_status_line(window: &adw::ApplicationWindow, said: &str) {
    if let Some(status_line) = find_status(window.upcast_ref()) {
        status_line.set_text(said);
    }
}

/// Put this window's run bar back to idle after a narrate run that ran to its end in one go, through
/// the one close door F0.5 S4 gives every run (running off, controls, the audio unload handed over).
fn clear_narrate_bar() {
    crate::ui::run_close::close_last_narrate_run();
}

/// The live project this window runs under — the newest slot, the same rule `session_policy` reads.
/// Public to the crate so the Produce page's F5.1 seam reads the session rather than a widget copy of it.
pub(crate) fn live_project() -> Project {
    PLAY_SESSION
        .with(|slots| slots.borrow().last().cloned())
        .map(|held| held.borrow().clone())
        .unwrap_or_default()
}

/// The shared body of both doors. Private so they cannot drift apart. Steps land in §F4.1's order, and
/// each one's wording comes from `narrate_run`, never from a string written here.
fn run_narrate_with<FReply>(
    window: &adw::ApplicationWindow,
    project: &Project,
    mut reply: FReply,
) -> String
where
    FReply: FnMut(
        &layout::Tree,
        &[(f64, f64)],
    ) -> Result<Vec<narrate_run::Written>, String>,
{
    let Some(tree) = narrate_session_tree() else {
        let said = narrate_run::failed_status("narration");
        say_on_status(window, &said);
        return said;
    };
    let cut_ = cut::load(&tree).unwrap_or_default();
    let existing = narration::load(&tree).unwrap_or_default();
    let voice = narrate_data::read_voice(&tree);
    let rewrite = project.policy.narration_rewrite.value;
    // The clips this run writes for are the VISIBLE SEGMENTS of the cut in play order. NOT
    // `cut_play::kept_runs`: that MERGES touching segments for preloading, so three adjoining clips
    // would read as one and every clip number above 1 the model named would fall outside the batch.
    let mut clips: Vec<(f64, f64)> = cut_
        .segs
        .iter()
        .filter(|seg| seg.e > seg.s)
        .map(|seg| (seg.s, seg.e))
        .collect();
    clips.sort_by(|a, b| a.0.total_cmp(&b.0));
    let plan = narrate_run::plan_run(rewrite, &clips, &existing, &voice);

    // S2: pull half-typed rows into the record, then keep the previous generation before anything
    // overwrites it. Both happen before the save so the file on disk matches what the run believes.
    let mut record = pull_half_typed(window, &existing);
    if let Err(error) = narrate_data::keep_previous(&tree) {
        log_line(&format!("!!! could not keep narration.prev.json -- {error}"));
        let said = narrate_run::failed_status("narration");
        say_on_status(window, &said);
        return said;
    }
    // S3: save the project, open the two jobs, and put the reason on the log BEFORE the call goes out.
    if let Err(error) = crate::project::save(project, tree.dir()) {
        log_line(&format!("!!! could not save the project -- {error}"));
        let said = narrate_run::failed_status("narration");
        say_on_status(window, &said);
        return said;
    }
    log_line(&plan.log);
    NARRATE_QUEUE.with(|queue| {
        let mut queue = queue.borrow_mut();
        queue.reset();
        queue.phase(0.0, 0.5);
        queue.job(0, JOB_NARRATION, 1, JOBS);
        queue.push(0, plan.clips.len().max(1), "clip");
        queue.prog(0, 0.0, STAGE_THINKING);
        queue.job(1, JOB_SPEAKING, 2, JOBS);
    });
    set_narrate_stage(window, STAGE_THINKING);

    let written = match reply(&tree, &plan.clips) {
        Ok(lines) => lines,
        Err(error) => {
            log_line(&format!("!!! narrate: the call failed -- {error}"));
            let said = narrate_run::failed_status("narration");
            say_on_status(window, &said);
            return said;
        }
    };
    if written.is_empty() {
        // No line came back: nothing is saved, nothing is spoken, and the run says so plainly.
        let said = narrate_run::Outcome::NothingWritten.status();
        say_on_status(window, &said);
        return said;
    }

    // S4: fold the written lines in (the silent list survives inside `fold_written`), save, publish.
    record = narrate_run::fold_written(&record, &written);
    if let Err(error) = narration::save(&record, &tree) {
        log_line(&format!("!!! could not write narration.json -- {error}"));
        let said = narrate_run::failed_status("narration");
        say_on_status(window, &said);
        return said;
    }
    if let Some(held) = HELD_NARRATION.with(|held| held.borrow().last().cloned()) {
        *held.borrow_mut() = record.clone();
    }
    // The bar's stage follows the clips as they close; headless they all arrive with the one reply, so
    // this is the final count in one step.
    set_narrate_stage(window, &narrate_run::writing(written.len(), plan.clips.len()));
    log_line(&narrate_run::written_log(written.len()));
    NARRATE_QUEUE.with(|queue| queue.borrow_mut().done(0, 1.0));

    // S5: a captions-only voice stops here — the lines exist and nothing is spoken.
    if plan.captions_only {
        let said = narrate_run::captions_only_done(written.len());
        say_on_status(window, &said);
        return said;
    }

    // S6: the speaking pass. F4.4 owns the POST /v1/audio/speech request; `narrate_tts` builds the
    // request, the options and the take path but makes no call yet, so this seam reports what WOULD be
    // synthesized and counts the cache honestly -- the cached half is a real file check, so a warm
    // project really does report zero work.
    say_on_status(window, SPEAKING_STATUS);
    // F4.8: the lines this pass may speak come through `narrate_off::lines_to_speak`, the one seam the
    // tick owns — with narration off it answers empty, so nothing is synthesized behind the tick's back,
    // and a blank line stays a deliberate silence that is never spoken.
    let to_speak = narrate_off::lines_to_speak(live_project().no_narration, &record.entries);
    NARRATE_QUEUE
        .with(|queue| queue.borrow_mut().push(1, to_speak.len().max(1), "line"));
    let speaks = narrate_run::speak_pass(&voice, to_speak.as_slice(), |entry| {
        let key = narration::key_for(entry, Some(&voice));
        tree.tts_wav(&narration::tts_file(&key)).is_file()
    });
    for (index, speak) in speaks.iter().enumerate() {
        if *speak == Speak::Synthesize {
            log_line(&narrate_preview::synthesizing(index));
        }
    }
    let (spoken, cached) = narrate_run::tally(&speaks);
    log_line(&narrate_run::spoken_log(spoken, cached));

    // S7: the ending goes on the STATUS LINE, which is where the prototype never put it.
    let said = narrate_run::Outcome::Spoken {
        written: written.len(),
        spoken,
    }
    .status();
    say_on_status(window, &said);
    said
}

/// S2: read the row boxes back into the record, so a half-typed line that was never committed still
/// reaches the file the run saves. Empty boxes are left exactly as they were -- clearing a box is not
/// the same act as deleting a line (F4.7's 🗑 is), so an empty box never erases an entry. Rows are read
/// by name (`line-text-<n>`, one per entry in the page's sorted order) rather than from a remembered
/// handle, because `refresh` rebuilds the rows whole. The box is write-only: `parse_box` reads a `@N`
/// out of the tag, and nothing here prints a second back into it -- the time field owns the number.
pub fn pull_half_typed(window: &adw::ApplicationWindow, existing: &Narration) -> Narration {
    let mut record = existing.clone();
    for index in 0..record.entries.len() {
        let Some(found) = find_widget_by_name(window.upcast_ref(), &format!("line-text-{index}")) else {
            continue;
        };
        let Ok(entry_box) = found.downcast::<gtk::Entry>() else {
            continue;
        };
        let typed = entry_box.text().to_string();
        if typed.trim().is_empty() {
            continue;
        }
        let parsed = narrate_screen::parse_box(&typed);
        let entry = &mut record.entries[index];
        match parsed.role {
            // A placement tag makes the line a caption and clears the emotion, so no entry carries both.
            narrate_screen::BoxRole::Caption => {
                entry.pos = if parsed.tag.is_empty() {
                    "bottom".to_string()
                } else {
                    parsed.tag
                };
                entry.emotion = String::new();
            }
            _ => {
                entry.emotion = parsed.tag;
            }
        }
        entry.text = parsed.body;
    }
    record.sort();
    record
}

/// What the bar's label reads right now, and where it is painted. Kept in thread-locals so the doors can
/// report the sequence without having captured the builder's locals, and so a test can read the stages
/// back instead of trusting a comment.
fn set_narrate_stage(window: &adw::ApplicationWindow, stage: &str) {
    NARRATE_STAGE.with(|cell| cell.replace(stage.to_string()));
    let fraction = NARRATE_QUEUE.with(|queue| queue.borrow().fraction());
    let tip = NARRATE_QUEUE.with(|queue| queue.borrow().text_and_tip().1);
    if let Some(bar) = find_widget_by_name(window.upcast_ref(), "run-progress")
        .and_then(|w| w.downcast::<gtk::ProgressBar>().ok())
    {
        bar.set_fraction(fraction.clamp(0.0, 1.0));
        bar.set_text(Some(stage));
        bar.set_show_text(true);
        if !tip.is_empty() {
            bar.set_tooltip_text(Some(&tip));
        }
    }
}

/// The stage text this run last put on the bar.
pub fn narrate_stage() -> String {
    NARRATE_STAGE.with(|cell| cell.borrow().clone())
}

/// Put the narration run's bar face on screen without running the run: `narration 1/2` open, two of
/// four clips closed. Only the snapshot needs this -- headless there is no model to answer, but §07's
/// screen shows a page that has just been written, and an idle bar would misrepresent it. Paints
/// through the same `set_narrate_stage` the run itself uses, so the shot cannot show a state the run
/// never produces.
pub fn show_narrate_stage(window: &adw::ApplicationWindow, written: usize, of: usize) {
    NARRATE_QUEUE.with(|queue| {
        let mut queue = queue.borrow_mut();
        queue.reset();
        queue.phase(0.0, 0.5);
        queue.job(0, JOB_NARRATION, 1, JOBS);
        queue.push(0, of.max(1), "clip");
        queue.prog(0, if of == 0 { 0.0 } else { written as f64 / of as f64 }, &narrate_run::writing(written, of));
        queue.job(1, JOB_SPEAKING, 2, JOBS);
    });
    set_narrate_stage(window, &narrate_run::writing(written, of));
}

thread_local! {
    /// The narration run's own view of the bar: `narration 1/2` then `speaking 2/2`, each owning half
    /// (§F4.1 S3/S6). Held here so the two doors cannot drift apart.
    static NARRATE_QUEUE: std::cell::RefCell<runqueue::Queue> =
        std::cell::RefCell::new(runqueue::Queue::new());
    /// A scripted F4.1 narration reply for the shell's ▶ — see `set_narrate_script`.
    static NARRATE_SCRIPT: std::cell::RefCell<Vec<(u32, f64, String, String)>> =
        std::cell::RefCell::new(Vec::new());
    static NARRATE_STAGE: std::cell::RefCell<String> =
        std::cell::RefCell::new(String::new());
}

/// Put a sentence on this window's status line, the widget every other door writes, and log it too.
fn say_on_status(window: &adw::ApplicationWindow, said: &str) {
    if let Some(status_line) = find_status(window.upcast_ref()) {
        status_line.set_text(said);
    }
    log_line(said);
}




/// F2.3: the Cut page's ▶✂✂ forwards to [`press_review_cuts`], reading the cut this window holds.
fn wire_review_cuts(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        let _ = press_review_cuts(&window, &newest_review_cut());
    });
}

/// Where a press of Open goes. [`open_project`] decides everything — which folder the pick names,
/// whether it is an old single-file project, what the read produced and what had to be dropped; this
/// only asks for a folder, then draws the answer.
fn wire_open(
    open_: &gtk::Button,
    bar: &Rc<RefCell<run::RunBar>>,
    status: &gtk::Label,
    session: &Rc<RefCell<Project>>,
) {
    let bar = bar.clone();
    let status = status.clone();
    let session = session.clone();
    open_.connect_clicked(move |_| {
        // The chooser cannot be answered in a headless test, so what a test asserts is that the press
        // got here and that every rule behind it lives in `open_project` (see
        // `tests/open_project_widgets.rs`, which drives `open_folder` directly).
        ask_open_project(&bar, &status, &session);
    });
}

/// S1: the folder chooser, titled for opening. Folder-select rather than file-select because a
/// project *is* a folder (01 §1); a user who picks `naivepost.json` inside one still gets that
/// folder, which is [`open_project::folder_for`]'s rule rather than the chooser's.
fn ask_open_project(
    bar: &Rc<RefCell<run::RunBar>>,
    status: &gtk::Label,
    session: &Rc<RefCell<Project>>,
) {
    let dialog = gtk::FileDialog::builder()
        .title(open_project::TITLE)
        .accept_label("Open")
        .build();

    let bar = bar.clone();
    let status = status.clone();
    let session = session.clone();
    dialog.select_folder(
        None::<&gtk::Window>,
        None::<&gio::Cancellable>,
        move |chosen: Result<gio::File, glib::Error>| {
            let Ok(file) = chosen else {
                return; // dismissed: nothing was asked for, so nothing changed
            };
            let picked = file.path().unwrap_or_default();
            open_folder(&bar, &status, &session, &picked);
        },
    );
}

/// S1 → S4 for one picked path, with no chooser in the way. This is the body the chooser's callback
/// calls, and the seam a widget test drives so the assertions land on real state rather than on a
/// dialog nobody answers.
pub fn open_folder(
    bar: &Rc<RefCell<run::RunBar>>,
    status: &gtk::Label,
    session: &Rc<RefCell<Project>>,
    picked: &Path,
) -> bool {
    match open_project::open(picked, |path| path.is_file(), |path| path.exists()) {
        Err(failure) => {
            log_line(&failure.log);
            status.set_text(failure.status);
            false
        }
        Ok(applied) => {
            // S3: paths settled before the read; migration first, it reads what the swap consumes.
            let migration = open_project::finish(&applied);
            let lines = applied.lines.clone();
            let opened_project = applied.project.clone();
            *session.borrow_mut() = opened_project.clone();
            for line in &lines {
                log_line(line);
            }
            // S4: old folder names move, and each move is logged.
            for line in &migration.lines {
                log_line(line);
            }
            for line in &migration.failures {
                log_line(line);
            }
            // S4's last three steps live in `ui::open_apply`, which owns their order and wording.
            crate::ui::open_apply::finish(&applied, &opened_project, status);
            // A run reading the old project would now be reading files that are not its own, so the
            // stop flag is raised rather than letting it carry on over swapped sources.
            let _ = bar.borrow();
            true
        }
    }
}

thread_local! {
    /// What this launch has opened, per root — the pairs settings writes as `PROJECT_<n>_ROOT` /
    /// `PROJECT_<n>_FILE`. Held here rather than written out because persisting belongs to
    /// `settings`' writer, which owns quoting and the whole-store render.
    static REMEMBERED: std::cell::RefCell<std::collections::BTreeMap<String, String>> =
        const { std::cell::RefCell::new(std::collections::BTreeMap::new()) };
}

/// Put an opened project into the window's remembered list AND into `llm.conf`, the one conf door.
pub(crate) fn remember_opened(root: &Path, file: &Path) -> Result<open_project::Remembered, String> {
    let paths = crate::settings::from_environment().ok_or("no settings folder to write into")?;
    REMEMBERED.with(|list| {
        let mut projects = list.borrow_mut();
        crate::machine_store::persist_project(&paths, &mut projects, root, file)
            .map(|_| open_project::remember(&mut projects, root, file))
    })
}
/// Where a press of Save goes: [`save_as::press`] decides whether it may happen at all, and the
/// chooser only names the project. [`save_apply`] says every sentence and moves the open-folder memory.
///
/// Save reads the LIVE session (the handle [`wire_new`] and [`wire_open`] swap), not the `&Project`
/// the window was built from, so what reaches disk is tonight's work.
fn wire_save(
    save_: &gtk::Button,
    bar: &Rc<RefCell<run::RunBar>>,
    status: &gtk::Label,
    session: &Rc<RefCell<Project>>,
) {
    let bar = bar.clone();
    let status = status.clone();
    let session = session.clone();
    save_.connect_clicked(move |_| {
        if let Err(reason) = save_as::press(bar.borrow().running.is_some()) {
            save_apply::refused(reason, &status);
            return;
        }
        // The folder this window took on: a Save's rename or an Open's pick, whichever it was.
        let open = save_apply::save_target();
        ask_save_project(&status, &session, &open);
    });
}

/// S2: the dialog that names the project. `select_folder` rather than `save_folder` because a
/// project is a folder (01 §1) and a file chooser would insist on a file; the name typed in the
/// sheet is what S3 renames to, so it starts as the open folder's own name.
fn ask_save_project(status: &gtk::Label, session: &Rc<RefCell<Project>>, open: &Path) {
    let dialog = gtk::FileDialog::builder()
        .title(save_as::TITLE)
        .accept_label("Save")
        .build();
    if let Some(dir) = open.parent() {
        dialog.set_initial_folder(Some(&gio::File::for_path(dir)));
    }
    dialog.set_initial_name(open.file_name().and_then(|name| name.to_str()));

    let status = status.clone();
    let open = open.to_path_buf();
    let session = session.clone();
    dialog.select_folder(
        None::<&gtk::Window>,
        None::<&gio::Cancellable>,
        move |chosen: Result<gio::File, glib::Error>| {
            let Ok(file) = chosen else {
                return; // dismissed: nothing was asked for, so nothing changed
            };
            let named = file
                .path()
                .unwrap_or_default()
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            // The decided save itself belongs to `save_apply` -- the sentences, the log, and moving
            // the open-folder memory with the folder that moved. A test drives THAT door, because no
            // headless run can answer the desktop's sheet.
            save_apply::save_named(&session, &status, &open, &named).ok();
        },
    );
}

thread_local! {
    /// The window's log lines, standing in for F0.5's log expander: `>>> moved the output folder
    /// to …` and the `!!!` refusals have nowhere to be drawn until that row exists, and a test has
    /// to be able to see that they were said at all.
    static LOGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// The launch's own log lines (§7's desktop registration), held until a window exists to read them.
static PENDING_LOGS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// §7's launch lines: `main` produces them while there is still no log to write into, so they wait in
/// [`PENDING_LOGS`] and the first [`build_window`] drains them through [`note_startup`].
pub fn note_startup(lines: &[String]) {
    if let Ok(mut pending) = PENDING_LOGS.lock() {
        pending.extend(lines.iter().cloned());
    }
    for line in PENDING_LOGS.lock().map(|mut p| p.drain(..).collect::<Vec<_>>()).unwrap_or_default() {
        log_line(&line);
    }
}

pub(crate) fn log_line(line: &str) {
    LOGS.with(|logs| logs.borrow_mut().push(line.to_string()));
}

/// Empty the log sinks. Only a test binary asks: nothing clears the shell's log while a session runs.
pub fn clear_logs_for_test() {
    LOGS.with(|logs| logs.borrow_mut().clear());
}

/// The one line a finished run leaves about its audio housekeeping. Called from the unload thread,
/// so it goes through the same log sink as everything else; `log_line` itself stays `pub(crate)`
/// because only this crate's own flows are allowed to write to the user's log.
pub fn note_unload(said: &str) {
    // Written straight to the shared buffer: this runs on the unload's own thread, whose thread-local
    // sink no reader ever sees.
    crate::ui::run_close::log(&format!("{}{said}", crate::ui::run_close::UNLOAD_PREFIX));
}

/// Every line the window has logged this session, oldest first.
///
/// The thread-local is THE record: every reader counts lines in it (a migration note checked as
/// `before + 1` counts exactly what it added). §S4's unload answers on its own thread and does NOT join
/// this record -- its news lives in [`crate::ui::run_close::SHARED_LOG`] and on the status row, because a
/// sentence written after the close returned would land after the run's last word (§2).
pub fn window_logs() -> Vec<String> {
    LOGS.with(|logs| logs.borrow().clone())
}




























thread_local! {
    /// The run bar `build_window` created, published so a test can read the step it holds. Same
    /// reason as [`SESSION`]: the bar lives inside the window's closure and nothing else can reach it.
    pub(crate) static RUN_BARS: std::cell::RefCell<Vec<Rc<std::cell::RefCell<crate::run::RunBar>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    /// The live session the newest window works on, handed to the ▶ handler so F1.1's refusal asks
    /// about what the page holds now rather than what `build_window` was called with.
    pub(crate) static PLAY_SESSION: std::cell::RefCell<Vec<Rc<std::cell::RefCell<Project>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}















// --- §05-cut#1-screen: the history and zoom groups -----------------------------------------------
//
// The four history buttons and the −/+ pair live in the toolbar; the rules they consult are
// `cut::History` (the stack, its depth bound `P.layout.undoDepth`) and `cut_screen`'s zoom ladder.
// What this section adds is the door from a press to those rules and nothing else: each function takes
// the window, calls one rule, prints what came back, and repaints from the answer.

thread_local! {
    /// This window's edit history (§2), newest slot last like [`REVIEW_CUTS`] so the newest window is
    /// the live one. Opened on whatever cut the page was built with, which §2 makes both the first
    /// state and the base Revert returns to.
    static CUT_HISTORIES: std::cell::RefCell<Vec<Rc<std::cell::RefCell<cut::History>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The TEST-FACING door for "the page has been edited". Every mutating press in the app records through
/// [`record_edit`], which publishes, records and refreshes in one step; this is the narrower seam a test
/// uses when it wants to say "treat what is on screen as an edit" without going through a widget -- which
/// is why `tests/cut_screen_toolbar_widgets.rs` calls it after a Split. It pushes unconditionally
/// (`History::push`), where the app-side door asks first whether anything actually changed.
///
/// A real ＋ Add lights Undo WITHOUT any call to this function: `report_verb`'s Applied branch calls
/// `record_edit`, so the button's state comes from the app's own recording rather than from a caller
/// remembering to say so. Do not use this to make a button light in production code.
pub fn note_edit(window: &adw::ApplicationWindow) {
    let history = cut_history(window);
    history.borrow_mut().push(&newest_review_cut());
    refresh_history_buttons(window);
}

/// This window's history, opened on the cut it currently shows if none was opened yet.
fn cut_history(_window: &adw::ApplicationWindow) -> Rc<std::cell::RefCell<cut::History>> {
    let held = CUT_HISTORIES.with(|slot| slot.borrow().last().cloned());
    if let Some(held) = held {
        return held;
    }
    let fresh = Rc::new(std::cell::RefCell::new(cut::History::open(&newest_review_cut())));
    CUT_HISTORIES.with(|slot| slot.borrow_mut().push(fresh.clone()));
    fresh
}

/// F2.13: re-open this window's edit history on the cut a caller is about to show, so the base Revert
/// returns to is that cut and not whatever was newest when the history happened to be touched first.
/// Used by `build_window` at project open (against the cut read from `cut/cut.json`) and by tests that
/// seed a cut into an already-built window — both are "this is what the page opens with".
/// F2.13: re-open this window's edit history on the cut a caller is about to show, so the base Revert
/// returns to is that cut and not whatever was newest when the history happened to be touched first.
/// Used by `build_window` at project open (against the cut read from `cut/cut.json`) and by tests that
/// seed a cut into an already-built window — both are "this is what the page opens with".
pub fn reopen_history_on(window: &adw::ApplicationWindow, cut_: &cut::Cut) {
    let _ = window;
    CUT_HISTORIES.with(|slot| {
        let fresh = Rc::new(std::cell::RefCell::new(cut::History::open(cut_)));
        let mut slot = slot.borrow_mut();
        if let Some(latest) = slot.last_mut() {
            *latest = fresh;
        } else {
            slot.push(fresh);
        }
    });
}

/// Undo (§1 item 18): take the page back one state. Returns the line for the status bar — either what
/// the page went back to, or the fact that there was nothing behind it.
pub fn press_undo(window: &adw::ApplicationWindow) -> String {
    let history = cut_history(window);
    let back = history.borrow_mut().undo();
    match back {
        Some(snapshot) => {
            let mut cut_ = newest_review_cut();
            snapshot.restore(&mut cut_);
            publish_cut(&cut_);
            // The lane and the preview refresh with it: a bar left on screen for an effect Undo took away
            // is a lie about what this cut holds.
            refresh_effects_lane(window);
            // ...and so do the timeline readings: Undo changes the segment count, so a `Segments:` row that
            // still shows the undone answer would be the same kind of lie.
            refresh_cut_readouts(window);
            // S1: the sentence is `cut::undone`'s, not composed here.
            let said = cut::undone(cut_.segs.len());
            log_line(&said);
            said
        }
        None => {
            let said = "nothing to undo \u{2014} you are at the state this page opened with".to_string();
            log_line(&said);
            said
        }
    }
}

/// Redo (§1 item 19): put back what Undo took away.
pub fn press_redo(window: &adw::ApplicationWindow) -> String {
    let history = cut_history(window);
    let forward = history.borrow_mut().redo();
    match forward {
        Some(snapshot) => {
            let mut cut_ = newest_review_cut();
            snapshot.restore(&mut cut_);
            publish_cut(&cut_);
            refresh_effects_lane(window);
            refresh_cut_readouts(window);
            // S1: one walk sentence for both directions (spec F2.13 gives the pair a single line).
            let said = cut::undone(cut_.segs.len());
            log_line(&said);
            said
        }
        None => {
            let said = "nothing to redo \u{2014} a new edit threw that branch away".to_string();
            log_line(&said);
            said
        }
    }
}

/// Revert (§1 item 20): drop everything added or removed by hand and go back to the base — the last
/// suggestion, or what the page opened with when there has been none.
pub fn press_revert(window: &adw::ApplicationWindow) -> String {
    let history = cut_history(window);
    let already_base = history.borrow().base_is_the_screen(&newest_review_cut());
    if already_base {
        let said = cut::NOTHING_TO_REVERT.to_string();
        log_line(&said);
        return said;
    }
    // What was on screen BEFORE the restore, so the sentence can say how much hand-made work went away.
    let had = newest_review_cut();
    let snapshot = history.borrow_mut().revert();
    let mut cut_ = had.clone();
    snapshot.restore(&mut cut_);
    publish_cut(&cut_);
    refresh_effects_lane(window);
    refresh_cut_readouts(window);
    let said = cut::reverted(snapshot.segs.len(), had.segs.len());
    log_line(&said);
    said
}

/// Clear (§1 item 21): every kept stretch and every effect off the timeline, recordings as loaded.
/// It is itself an edit, so Undo brings them back — which is why the result is pushed rather than
/// simply written.
pub fn press_clear_cut(window: &adw::ApplicationWindow) -> String {
    let had = newest_review_cut();
    if had.segs.is_empty() && had.fx.is_empty() {
        let said = cut::NOTHING_TO_CLEAR.to_string();
        log_line(&said);
        return said;
    }
    let cleared = crate::cut::cleared(&had);
    let history = cut_history(window);
    history.borrow_mut().push(&cleared);
    publish_cut(&cleared);
    refresh_effects_lane(window);
    let said = cut::cleared_message(had.segs.len(), had.fx.len());
    log_line(&said);
    said
}

/// Write a cut back as the page's current one, so the readouts, the verbs and every later flow see it.
fn publish_cut(cut_: &cut::Cut) {
    REVIEW_CUTS.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            *slot.borrow_mut() = cut_.clone();
        }
    });
}

/// F2.13 S4: the ONE door every mutating press writes through. It publishes the new cut, records it on
/// the history, and refreshes the four history buttons from the stack — so a button lighting up always
/// means a snapshot is behind it, never that some caller remembered to say so.
///
/// Recording goes through [`cut::History::note_change`] rather than `push`: several doors write things
/// that are not edits along with the cut (a fold toggle persists `folds`, which `Snapshot` deliberately
/// omits so Undo can never unfold the page under someone; the watched row is not a cut field at all),
/// and comparing the snapshot against the state on screen is what keeps those off the stack. A door whose
/// write changed none of the seven editable things therefore leaves Undo exactly as greyed as it was.
///
/// The history's OWN writes (`press_undo`, `press_redo`, `press_revert`) use plain [`publish_cut`]: they
/// move the pointer or reset the base, and recording them would put the undo itself on the undo stack.
pub(crate) fn record_edit(window: &adw::ApplicationWindow, cut_: &cut::Cut) {
    publish_cut(cut_);
    // Whether the write was an edit at all is the history's own question (`note_change`), not the door's:
    // a drag released at the second it was already at changes nothing and therefore takes no step.
    cut_history(window).borrow_mut().note_change(cut_);
    refresh_history_buttons(window);
    // Every edit changes the timeline, and four of the form column's readings answer from it. That column is
    // built ONCE by `cut_form_column` for an empty cut, so without this the page kept saying `Segments: 0`
    // beside landed scenes until something else happened to repaint it.
    refresh_cut_readouts(window);
}

/// − / + (§1 item 22): one step of the zoom ladder over THIS page's view, its floor where the whole
/// session fits (`cut_view::floor_pps`) rather than the placeholder strip's constant, so a session of any
/// length stops at “all of it on screen”.
pub fn press_zoom(window: &adw::ApplicationWindow, inward: bool) -> String {
    // The ladder's floor is the strip's fit (§A), not the full-page band's — see `zoom_surface_width`.
    let width = crate::ui::cut_view::zoom_surface_width(window);
    let (next, _) = crate::ui::cut_view::zoom_step(
        inward,
        &crate::timeline::filmed_runs(&page_recordings()),
        crate::ui::cut_view::extents(width).0,
        crate::ui::cut_view::view_px(width),
    );
    set_preview_pps(next);
    repaint_scrollbar(window);
    let said = format!("zoom {:.1} px a second", next);
    log_line(&said);
    said
}

/// Repaint badge **31** and the band beside it from the live view. Lives in `cut_view`, which owns the bar.
fn repaint_scrollbar(window: &adw::ApplicationWindow) {
    crate::ui::cut_view::refresh_page_scrollbar(window);
}

/// §1's three wheel gestures, attached to the widgets they belong to. The ground names live here because
/// this is where those widgets are named; `cut_view::attach_wheel` does the attaching.
fn wire_wheel_gestures(window: &adw::ApplicationWindow) {
    crate::ui::cut_view::attach_wheel(
        window,
        &[
            ("group-transport", cut_screen::WheelOver::Transport),
            ("preview-panel", cut_screen::WheelOver::Preview),
            ("track-strip", cut_screen::WheelOver::Tracks),
        ],
        |win, frames| {
            let _ = step_frames_of(win, frames);
        },
    );
}
thread_local! {
    /// The page's pixels-per-second. Seeded at the placeholder strip's zoom so the toolbar's −/+ start
    /// from where the page actually draws; F2.10/F2.11 move the real tracks onto this same number.
    static PREVIEW_PPS: std::cell::RefCell<f64> = const { std::cell::RefCell::new(0.0) };
}

fn set_preview_pps(pps: f64) {
    PREVIEW_PPS.with(|cell| *cell.borrow_mut() = pps);
}

/// Wire the four history buttons and the two zoom buttons. Same shape as the verb wiring: find by name,
//  click forwards to the seam, the status line gets the sentence, and the sensitivity is refreshed
/// from the rule rather than remembered.
fn wire_history_and_zoom(window: &adw::ApplicationWindow) {
    for (name, seam) in [
        ("undo-button", press_undo as fn(&adw::ApplicationWindow) -> String),
        ("redo-button", press_redo),
        ("revert-button", press_revert),
        ("clear-cut-button", press_clear_cut),
    ] {
        if let Some(button) = line_step_button(window, name) {
            let win = window.clone();
            button.connect_clicked(move |_| {
                let status = seam(&win);
                if let Some(status_line) = find_status(win.upcast_ref()) {
                    status_line.set_text(&status);
                }
                refresh_history_buttons(&win);
            });
        }
    }
    for (name, inward) in [("zoom-out-button", false), ("zoom-in-button", true)] {
        if let Some(button) = line_step_button(window, name) {
            let win = window.clone();
            button.connect_clicked(move |_| {
                let status = press_zoom(&win, inward);
                if let Some(status_line) = find_status(win.upcast_ref()) {
                    status_line.set_text(&status);
                }
            });
        }
    }
    // The thumbnail ladder (§A's "Thumbnails [🖼− 🖼+]") sits in the FORM column beside the number it
    // drives, so its handler repaints that one label rather than a status line.
    for (name, bigger) in [("thumb-minus", false), ("thumb-plus", true)] {
        if let Some(button) = line_step_button(window, name) {
            let win = window.clone();
            button.connect_clicked(move |_| {
                let px = press_thumb(&win, bigger);
                if let Some(status_line) = find_status(win.upcast_ref()) {
                    status_line.set_text(&format!("thumbnails {px} px"));
                }
            });
        }
    }
}

thread_local! {
    /// The page's thumbnail height in px. Seeded at `THUMB_AT_OPEN` because that is what the form
    /// column drew, so the first click steps from the number on screen rather than from zero.
    static THUMB_PX: std::cell::RefCell<u32> = const { std::cell::RefCell::new(0) };
}

/// 🖼− / 🖼+ (§A): one third of a step, clamped 40..160 by `cut_screen::thumb_down`/`thumb_up`, and
/// the `cut-readout-thumbnails` label is repainted from the new number so the readout can never drift
/// from the control that owns it. Returns the new height.
pub fn press_thumb(_window: &adw::ApplicationWindow, bigger: bool) -> u32 {
    let now = THUMB_PX.with(|cell| {
        let held = cell.borrow().clone();
        let next = if bigger {
            cut_screen::thumb_up(held.max(cut_screen::THUMB_AT_OPEN))
        } else {
            cut_screen::thumb_down(held.max(cut_screen::THUMB_AT_OPEN))
        };
        *cell.borrow_mut() = next;
        next
    });
    if let Some(label) = find_widget_by_name(
        _window.upcast_ref(),
        &cut_screen::readout_widget("Thumbnails"),
    )
    .and_then(|w| w.downcast::<gtk::Label>().ok())
    {
        label.set_text(&format!("{now} px"));
    }
    now
}

/// Ctrl+Z / Ctrl+Shift+Z / Ctrl+Y (§1 items 18-19's chords). One controller on the window, following
/// [`wire_line_keys`]' shape: the key decides, the shift decides which of the two directions, and an
/// unheld letter is left to whatever widget has focus.
fn wire_history_keys(window: &adw::ApplicationWindow) {
    let controller = gtk::EventControllerKey::new();
    let win = window.clone();
    controller.connect_key_pressed(move |_ctrl, key, _code, mods| {
        let ctrl = mods.contains(gtk::gdk::ModifierType::CONTROL_MASK);
        let shift = mods.contains(gtk::gdk::ModifierType::SHIFT_MASK);
        if !ctrl {
            return glib::Propagation::Proceed;
        }
        let status = match key {
            // Z alone undoes; Z with Shift redoes, and so does Y — §1 spells both, because both hands
            // reach for one of them and a person should not have to remember which editor they opened.
            gtk::gdk::Key::z | gtk::gdk::Key::Z if !shift => press_undo(&win),
            gtk::gdk::Key::z | gtk::gdk::Key::Z if shift => press_redo(&win),
            gtk::gdk::Key::y | gtk::gdk::Key::Y => press_redo(&win),
            _ => return glib::Propagation::Proceed,
        };
        if let Some(status_line) = find_status(win.upcast_ref()) {
            status_line.set_text(&status);
        }
        refresh_history_buttons(&win);
        glib::Propagation::Stop
    });
    window.add_controller(controller.clone());
    HISTORY_KEYS.with(|cell| *cell.borrow_mut() = Some(controller));
}

thread_local! {
    static HISTORY_KEYS: std::cell::RefCell<Option<gtk::EventControllerKey>> =
        const { std::cell::RefCell::new(None) };
}

/// This window's history key controller, newest window winning like [`line_key_controller`].
pub fn history_key_controller(window: &adw::ApplicationWindow) -> Option<gtk::EventControllerKey> {
    let _ = window;
    HISTORY_KEYS.with(|cell| cell.borrow().clone())
}

/// F3.1 S2: this window's zoom drag, newest-window rule as everywhere else here (`SELECT_GESTURES`), so a
/// widget test can fire `drag-begin`/`drag-end` on the real gesture instead of calling the seam directly.
pub fn zoom_gesture(window: &adw::ApplicationWindow) -> Option<gtk::GestureDrag> {
    let _ = window;
    ZOOM_GESTURES.with(|cell| cell.borrow().clone())
}

thread_local! {
    static ZOOM_GESTURES: std::cell::RefCell<Option<gtk::GestureDrag>> =
        const { std::cell::RefCell::new(None) };
}

/// F3.1 S2: the drag on the preview that draws the box. The gesture carries no rule: it converts its own
/// press position and offset into two corners and hands them to [`zoom_drag_ended`], which is the same seam
/// the widget test drives. Left button only — the right button belongs to trim/move (F2.8).
fn wire_zoom_drag(window: &adw::ApplicationWindow) {
    let Some(panel) = preview_panel(window).and_then(|w| w.downcast::<gtk::Widget>().ok()) else {
        return;
    };
    let gesture = gtk::GestureDrag::new();
    gesture.set_button(1);
    let _win = window.clone();
    // `drag-begin` carries the ABSOLUTE press position; `drag-end` carries the OFFSET from it (the F2.8
    // lesson), so the release corner is start + offset and both corners are known only across the two signals.
    let start = std::rc::Rc::new(std::cell::Cell::new((0.0f64, 0.0f64)));
    let start_begin = start.clone();
    gesture.connect_drag_begin(move |_g, x, y| {
        start_begin.set((x, y));
    });
    let start_update = start.clone();
    let win_update = window.clone();
    gesture.connect_drag_update(move |_g, dx, dy| {
        let (sx, sy) = start_update.get();
        crate::ui::cut_preview_frame::note_box(Some((sx, sy, dx, dy)), &win_update);
    });
    let win_end = window.clone();
    gesture.connect_drag_end(move |_g, dx, dy| {
        let (sx, sy) = start.get();
        // DISPATCH BY ARMED ENTRY -- the one structural choice here that the spec does not dictate. This panel
        // carries ONE left-button drag gesture (F3.1's); adding a second button-1 gesture for F3.4 would put
        // two grab handlers on one widget fighting over the same sequence, and which one wins depends on GTK's
        // internal capture order rather than on what the user armed. So the single gesture asks who is waiting:
        // a caption arm takes the press first (its box is drawn on the output frame), and only an unarmed press
        // falls through to the zoom flow. A no-travel press arrives here too and is read as a CLICK inside
        // `text_drag_ended_with_source`, which is also where a press with nothing to place at falls back to the
        // play/pause toggle (`fx_text::press_without_travel_toggles_play`).
        // ...and a DRAWING arm is asked before the caption one. Both draw against the output frame, but the
        // svg arm is the more specific state: it only exists once a file has actually been chosen, so an svg
        // press that fell through to the text flow would drop the ink someone just picked. Order: svg, text,
        // then the zoom flow for an unarmed press.
        let say = if svg_armed() {
            svg_drag_ended(&win_end, (sx, sy), (sx + dx, sy + dy))
        } else if text_armed() {
            text_drag_ended(&win_end, (sx, sy), (sx + dx, sy + dy))
        } else {
            zoom_drag_ended(&win_end, (sx, sy), (sx + dx, sy + dy))
        };
        if let Some(status_line) = find_status(win_end.upcast_ref()) {
            status_line.set_text(&say);
        }
        crate::ui::cut_preview_frame::note_box(None, &win_end);
    });
    panel.add_controller(gesture.clone());
    ZOOM_GESTURES.with(|cell| *cell.borrow_mut() = Some(gesture));
}

/// F3.2: the Aspect ratio dropdown's wire. `connect_selected_notify` is the `notify::selected` handler GTK
/// raises when the popover's own choice lands (and the one a test raises with `set_selected`), so this fires
/// on the user's pick and on nothing else. The callback converts the index to the string it stands for and
/// prints what [`press_aspect`] returns — no rule, no sentence of its own.
fn wire_aspect_choice(window: &adw::ApplicationWindow) {
    let Some(widget) = find_widget_by_name(window.upcast_ref(), "aspect-choice") else {
        return;
    };
    let Ok(choice) = widget.downcast::<gtk::DropDown>() else {
        return;
    };
    let win = window.clone();
    choice.connect_selected_notify(move |picker| {
        // The index is the only thing the signal carries; the string it stands for comes from the same
        // `ASPECTS` list the dropdown was built from, so the two cannot disagree about what index 2 means.
        if picker.selected() == gtk::INVALID_LIST_POSITION {
            return;
        }
        let picked = fx_aspect::ASPECTS
            .get(picker.selected() as usize)
            .copied()
            .unwrap_or(fx_aspect::SOURCE);
        let say = press_aspect(&win, picked);
        if let Some(status_line) = find_status(win.upcast_ref()) {
            status_line.set_text(&say);
        }
    });
}

/// F3.1 S1: Esc releases the arm without drawing a box ("same entry again disarms" has a keyboard twin, and
/// §cut.md's key list drops the arm rather than leaving someone stuck with a lowered camera layer). Claims
/// ONLY Escape while armed; every other key, and Escape with nothing armed, returns `Proceed`, so typing in an
/// entry field is untouched exactly as `wire_copy_esc` leaves it.
/// Is this the window whose zoom form is live? Every zoom write goes through here so a closed window's leftover
/// handler cannot draw into, or clear, the form another window is showing -- the failure shape behind
/// `gtk_box_append: assertion 'gtk_widget_get_parent (child) == NULL' failed` across checks.
pub(crate) fn zoom_form_owner(window: &adw::ApplicationWindow) -> bool {
    LAST_ZOOM_WINDOW.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|w: &adw::ApplicationWindow| w.as_ptr() == window.as_ptr())
            .unwrap_or(false)
    })
}


pub fn press_zoom_apply(window: &adw::ApplicationWindow) -> String {
    if !zoom_form_owner(window) {
        return "no zoom form on this page \u{2014} drag a box first".to_string();
    }
    let Some(stored) = zoom_form_open() else {
        return "no zoom to apply \u{2014} drag a box first".to_string();
    };
    // Read the six fields back. An entry that does not parse keeps what the form already held rather than
    // becoming zero, so a half-typed number cannot silently place a 0-second camera move.
    let read = |name: &str, keep: f64| -> f64 {
        zoom_entry(name)
            .map(|entry| entry.text().trim().to_string())
            .and_then(|text| text.parse::<f64>().ok())
            .unwrap_or(keep)
    };
    let stay = zoom_check("zoom-end-stay-on-it").is_some_and(|check| check.is_active());
    let form = fx_zoom::Form {
        at: stored.at,
        dur: read("zoom-field-length", stored.dur),
        stay,
        trans: read("zoom-field-fade-in", stored.trans),
        tout: read("zoom-field-fade-out", stored.tout),
        curve: fx_zoom::CURVE_CHOICES[0].to_string(),
        // The box came from the drag; it is carried through untouched, never re-derived here.
        cx: stored.cx,
        cy: stored.cy,
        hf: stored.hf,
        row: stored.row,
    };
    let effect = match fx_zoom::apply(&form) {
        Ok(effect) => effect,
        // S4's floor: refuse with the module's own sentence and leave the form open to be corrected.
        Err(reason) => {
            // F3.8 S5: a refusal wrote nothing, so it gives the visit's Undo entry back rather than spending it.
            live_form_refused();
            return reason;
        }
    };
    let mut cut_ = newest_review_cut();
    // A zoom placed by a drag is new: one push of the applied record. `fx_record::record_into` is not the
    // path here because it lays down a fresh record of its own width, and this one's width was just typed.
    cut_.fx.push(effect.clone());
    save_insert_cut(&cut_);
    // `record_edit`, NOT `seed_review_cut`: the addition must sit on the history so F2.13's ↶ takes it back.
    live_form_answer_is_undo(window);
    record_edit(window, &cut_);
    refresh_effects_lane(window);
    set_zoom_form(None);
    if let Some(holder) = zoom_form_box(window) {
        holder.set_visible(false);
    }
    fx_zoom::placed_status(effect.t, effect.stay)
}

/// Read one of the ending checks back from THIS window's tree.
fn zoom_check(name: &str) -> Option<gtk::CheckButton> {
    LAST_ZOOM_WINDOW.with(|cell| {
        cell.borrow()
            .as_ref()
            .and_then(|w| find_widget_by_name(w.upcast_ref(), name))
            .and_then(|w| w.downcast::<gtk::CheckButton>().ok())
    })
}

/// S4: Cancel drops the form and changes nothing on the cut — no record pushed, no history written, so ↶
/// still points where it did before the box was drawn.
pub fn press_zoom_cancel(window: &adw::ApplicationWindow) -> String {
    set_zoom_form(None);
    if let Some(holder) = zoom_form_box(window) {
        holder.set_visible(false);
    }
    "left as it was \u{2014} no zoom placed".to_string()
}

pub fn press_zoom_esc(window: &adw::ApplicationWindow) -> Option<String> {
    if !zoom_armed() {
        return None;
    }
    set_zoom_armed(false);
    set_camera_layer_down(window, false);
    // §A.1 spells the refusal and the arm words but no exact disarm sentence, so this reuses the one
    // `press_zoom_item` gives for taking the arm off — one wording, never two near-duplicates.
    Some("zoom released \u{2014} no box was drawn".to_string())
}

fn wire_zoom_esc(window: &adw::ApplicationWindow) {
    let controller = gtk::EventControllerKey::new();
    let win = window.clone();
    controller.connect_key_pressed(move |_ctrl, key, _code, _mods| {
        if key != gtk::gdk::Key::Escape {
            return glib::Propagation::Proceed;
        }
        match press_zoom_esc(&win) {
            Some(said) => {
                if let Some(status_line) = find_status(win.upcast_ref()) {
                    status_line.set_text(&said);
                }
                glib::Propagation::Stop
            }
            None => glib::Propagation::Proceed,
        }
    });
    window.add_controller(controller);
}

/// Which of the four history buttons may be pressed right now, from the stack's own counts. Called
/// from [`refresh_selection_readout`] so a greyed history button is today's answer along with every
/// other greyed control on the page, never a leftover from the last draw.
pub(crate) fn refresh_history_buttons(window: &adw::ApplicationWindow) {
    let history = cut_history(window);
    let held = history.borrow();
    let depth = held.depth();
    // `depth - at - 1` is what lies ahead of the pointer; `at` is private, so the two counts come from
    // the questions the stack answers rather than from its insides.
    let undo_left = usize::from(held.can_undo());
    let redo_left = usize::from(held.can_redo());
    let ever_edited = depth > 1 || !held.base_is_the_screen(&newest_review_cut());
    let live = cut_screen::history_buttons_enabled(undo_left, redo_left, ever_edited);
    for (tool, is_live) in cut_screen::HISTORY_BUTTONS.iter().zip(live) {
        if let Some(button) = line_step_button(window, tool.name) {
            button.set_sensitive(is_live);
        }
    }
}

/// This window's preview-volume slider, by its stable name (F2.5 S6). The seam a test fires and the
/// page reads; `None` on a page that drew none.
pub fn preview_volume_scale(window: &adw::ApplicationWindow) -> Option<gtk::Scale> {
    find_widget_by_name(window.upcast_ref(), "preview-volume")?
        .downcast()
        .ok()
}

/// F2.2 S5: this window's side of the ▶✂ tick loop, over the SAME player and cut slots the buttons use.
struct CutPreviewSlot(adw::ApplicationWindow);

impl cut_play_leg::CutPreviewPage for CutPreviewSlot {
    fn cut_only(&self) -> bool { preview_player(&self.0).cut_only }
    fn playhead(&self) -> Option<f64> { preview_player(&self.0).playhead }
    fn set_playhead(&self, at: f64) { set_playhead(&self.0, at) }
    fn cut(&self) -> Cut { review_cut_of(&self.0) }
    fn volume(&self) -> f64 { preview_volume(&self.0) }
    // S5's six doors forward to `cut_cut_play`, the F2.2 page half: `window.rs` is past its size budget.
    fn arm_spare_at(&self, at: f64) -> bool { crate::ui::cut_cut_play::preroll_spare(&self.0, at) }
    fn cross_into_spare(&self) -> bool { crate::ui::cut_cut_play::swap_spare(&self.0) }
    fn drop_spare(&self) { crate::ui::cut_cut_play::release_spare(); }
    fn hold_still(&self, hold: bool) -> bool { crate::ui::cut_cut_play::freeze(hold) }
    fn play_rate(&self, rate: f64, at: f64) -> bool { crate::ui::cut_cut_play::hand_rate(&self.0, rate, at) }
    fn apply_gain(&self, gain: f64, at_secs: f64) { crate::ui::cut_cut_play::hand_gain(&self.0, gain, at_secs); }
    fn say_clock(&self, clock: f64, ended: bool) {
        if let Some(status) = find_status(self.0.upcast_ref()) {
            status.set_text(&cut_play_leg::clock_line(clock, ended));
        }
        // §D: the ✂ clock carries its own explanation as a tooltip: cut time, total, session second.
        let len = cut_play_leg::kept_length(&review_cut_of(&self.0));
        crate::ui::cut_cut_play::set_clock_tooltip(&self.0, clock, len, preview_playhead(&self.0));
    }
}


/// The seam F2.2's ▶✂ calls: [`cut_play::pressed`] decides whether the press switches to the cut,
/// ends a review or toggles play/pause; this forwards it and paints only what the answer says.
///
/// `segs` are the cut's own segments ([`crate::cut::Cut::segs`]). Like ▶'s `runs`, they are not yet
/// reachable from the window — the Cut page holds no live cut model until its own round — so an empty
/// list is what the button knows today, which is exactly the S1 case: nothing to skip to, refused.
pub fn press_play_cut(window: &adw::ApplicationWindow, cut: &cut::Cut) -> cut_play::Pressed {
    let Some(status) = find_status(window.upcast_ref()) else {
        panic!("the window has no status line");
    };
    let Some(player) = live_player(window) else {
        panic!("the window has no preview player");
    };
    let pressed = cut_play::pressed(&mut player.borrow_mut(), cut);
    // F2.10 S3: ▶ is what takes the preview back from a watched row. The release runs FIRST and its
    // sentence wins over `cut_play`'s own, because "▶ plays the cut — <row> no longer watched" answers
    // the click the user just made; a press that released nothing leaves the status as `cut_play`
    // answered it. (It used to run before the match below and be overwritten by it.)
    let released = hand_preview_back(window);
    if let Some(back) = &released {
        status.set_text(back);
    }
    // The outer `status` is the status Label, so each arm binds its own name and paints from it. A row
    // release already printed above keeps the line; otherwise this press's own sentence goes there.
    match &pressed {
        // S2/S3: both change what the clock means or what is playing, so both get their sentence. Both also
        // need a process making sound from the line the press just snapped to: ▶✂ owns that player, so the
        // press starts it (`cut_play_back::cue_clips` has no other program door).
        cut_play::Pressed::SwitchedToCut { status: line, .. } => {
            crate::ui::cut_cut_play::cue_running(window, preview_playhead(window));
            if released.is_none() {
                status.set_text(line)
            }
        }
        cut_play::Pressed::ReviewEnded { status: ended } => {
            crate::ui::cut_cut_play::cue_running(window, preview_playhead(window));
            if released.is_none() {
                status.set_text(*ended)
            }
        }
        cut_play::Pressed::Refused(line) => status.set_text(line),
        // S4: the toggle needs no sentence — the button's own face carries it. A pause takes the cut's
        // processes back with it (the player and the prerolled spare); a resume re-cues at the second the
        // press snapped to, which `Toggle::Playing` names rather than leaving this seam to guess.
        cut_play::Pressed::Toggled(cut_play::Toggle::Paused) => crate::ui::cut_cut_play::pause_media(),
        cut_play::Pressed::Toggled(cut_play::Toggle::Playing { from }) => {
            crate::ui::cut_cut_play::cue_running(window, *from);
        }
    }
    pressed
}


/// Put this window's cut where ▶✂✂ can read it — the seam the cut-model round plugs into. Until then
/// `build_window` seeds an empty one and a press answers S1's refusal.
pub fn seed_review_cut(window: &adw::ApplicationWindow, cut_: &cut::Cut) {
    let _ = window;
    REVIEW_CUTS.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            *slot.borrow_mut() = cut_.clone();
        }
    });
    // F2.10/F2.11: the row list, the fold badges and ⇲ Lane's "is anything rolling at m:ss" all read this
    // page's session spans, so a cut seeded after the window opened moves them — until someone declares the
    // real takes, what the page holds is only what the cut it OPENED implied, and the newer fact is this one.
    // A session DECLARED through `set_session_recordings` is never rewritten: its heads, tails and holes are
    // what fold badges and row-emptiness are asked against, and no kept clip can imply them.
    if !crate::ui::cut_folds::session_declared() {
        crate::ui::cut_folds::set_session_recordings(&crate::ui::cut_session::session_or_kept(cut_));
    }
}

/// The newest cut this window holds, or an empty one when nothing has been seeded.
pub(crate) fn newest_review_cut() -> cut::Cut {
    REVIEW_CUTS
        .with(|slots| slots.borrow().last().map(|slot| slot.borrow().clone()))
        .unwrap_or_default()
}

/// The seam F2.3's ▶✂✂ calls: [`cut_review::pressed`] decides whether the press starts the review from
/// the red line, refuses for want of a join, or pauses-and-ends a running one; this forwards it and hands
/// the answer to [`cut_review_play::answer_press`], which owns what each arm does to the page. Every
/// sentence comes from `cut_review` itself.
pub fn press_review_cuts(window: &adw::ApplicationWindow, cut_: &cut::Cut) -> cut_review::Pressed {
    let Some(status_line) = find_status(window.upcast_ref()) else {
        panic!("the window has no status line");
    };
    let Some(player) = live_player(window) else {
        panic!("the window has no preview player");
    };
    let pressed = cut_review::pressed(&mut player.borrow_mut(), cut_);
    crate::ui::cut_review_play::answer_press(window, &status_line, &pressed, cut_);
    pressed
}

/// F3.9's label. The spec's flowchart starts at "after the cut" and names no control, so nothing existed to
/// wire the pass to: a named button is the decision taken in its spirit — the pass has to be startable, and its
/// gate (P.policy.captionsPass) has to be visible rather than inferred from a silent grey bar.
pub const CAPTIONS_PASS_LABEL: &str = "\u{2710} Captions";

// The §1 widget doors live in `window_doors` (window.rs is past its size budget); these are the same names,
// re-exported under the paths every other module and test already reaches them through.
pub use crate::ui::window_doors::{
    state, tab_tooltip, tab_dimmed, tab_button, inputs_readout, outputs_readout, outputs_folder_button, mark_narration_owed, tab_info_button, save_button, save_tooltip, rescan_button, find_source_widget, open_button, session_handle, session_sources, run_queue, open_tooltip, progress_bar, log_expander, progress_tooltip, rescan_tooltip, add_sources_button, copy_into_project, bench_language, session_freq, running_step, marking_pass, set_marking_pass, new_button, new_tooltip, play_button, stop_button, lucky_button, play_tooltip, find_status,

};
pub(crate) use crate::ui::window_doors::{find_switcher, find_widget_by_name};
