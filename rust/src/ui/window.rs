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
use crate::cut::{self, Cut};
use crate::cut_cam;
use crate::cut_hear;
use crate::cut_play;
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
use crate::project::Origin;
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
use crate::sources::{self, Control};
use crate::startup;
use crate::narration::{self, Narration};
use crate::shell::{self, Move, Outcome, Page, Shell};
use crate::PAGES;

/// §1's two readouts as the row draws them: each label is the word plus what `shell` computed, so
/// the prefix is the only text this file adds to those two rows.
const INPUTS_PREFIX: &str = "Inputs: ";
const OUTPUTS_PREFIX: &str = "Outputs: ";
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
        // path lights them from the live selection. `insert-button` rests greyed with its siblings on
        // purpose — with no line placed and no seconds chosen there is nothing to insert over, and
        // F2.12 lights it when a target exists; leaving it live because no test covers it would put a
        // working-looking control in a row of greyed ones.
        let verbs = cut_tool_group("verbs", &cut_screen::VERB_BUTTONS, true);
        toolbar.append(&verbs);

        // Effects dropdown (§1 item 17). A MenuButton whose popover holds the six effects §A lists. If a
        // popover cannot be built headless the fallback is a flat Box named `effect-menu` holding the
        // same six named buttons, which keeps every test's lookup working either way.
        let effect_button = gtk::MenuButton::new();
        effect_button.set_widget_name("effect-button");
        effect_button.set_label("\u{271a} Effect");
        effect_button.set_tooltip_text(Some(cut_screen::EFFECT_MENU_TIP));
        let effect_menu = cut_effect_menu();
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
        let form = cut_form_column();
        box_.insert_child_after(&form, Some(&previous));
        previous = form.upcast();



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
fn cut_effect_menu() -> gtk::Popover {
    let popover = gtk::Popover::new();
    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list.set_widget_name("effect-menu");
    for item in cut_screen::EFFECT_ITEMS {
        // `tip` carries the effect kind id here, not a sentence: it is what the row means to the cut,
        // and the label is what a person reads.
        let row = gtk::Button::with_label(item.label);
        row.set_widget_name(item.name);
        row.set_tooltip_text(Some(&format!("add a {} effect", item.tip)));
        list.append(&row);
    }
    popover.set_child(Some(&list));
    popover
}

/// The form column as the page sits idle (§1's "Form column", §A's "Idle rows"). A Grid named
/// `cut-form`: one row per reading, label in the left column and its value in the right, each value
/// named off `cut_screen::readout_widget` so a test reads the same eight rows the spec lists.
///
/// Two of the rows carry controls rather than a plain value: Thumbnails has the 🖼− / 🖼+ ladder
/// (40..160 px, one third at a step — `cut_screen::thumb_down`/`thumb_up`) and Aspect ratio has the
/// dropdown seeded with `ASPECT_DEFAULT` first. Both write through the seams below (`set_thumb_px`,
/// `set_aspect`) which repaint the row from the number they changed, so the readout is never a copy
/// that drifted from the control.
fn cut_form_column() -> gtk::Grid {
    let form = gtk::Grid::new();
    form.set_widget_name("cut-form");
    form.set_row_spacing(2);
    form.set_column_spacing(8);
    form.set_halign(gtk::Align::Start);
    // What the readings show for a page with no cut yet: zeros and the open-at thumbnail size. The
    // real values arrive with the rounds that own them; this fills the rows so none is blank.
    let cut_ = crate::cut::Cut::default();
    let rows = cut_screen::idle_readouts(
        cut_screen::THUMB_AT_OPEN,
        cut_screen::ASPECT_DEFAULT,
        0.0,
        None,
        &cut_,
        0.0,
    );
    for (index, row) in rows.iter().enumerate() {
        let name = cut_screen::readout_widget(row.label);
        let key = gtk::Label::new(Some(&format!("{}:", row.label)));
        key.set_xalign(0.0);
        key.add_css_class("dim-label");
        key.set_widget_name(&format!("{name}-key"));
        let value = if row.tip.is_empty() {
            // No control on this row: the value is the whole of it.
            let value = gtk::Label::new(Some(&row.value));
            value.set_xalign(0.0);
            value.upcast::<gtk::Widget>()
        } else {
            // Thumbnails: the ladder sits beside the number it drives.
            let line = gtk::Box::new(gtk::Orientation::Horizontal, 4);
            let minus = gtk::Button::with_label("\u{1f5bc}\u{2212}");
            minus.set_widget_name("thumb-minus");
            minus.set_tooltip_text(Some(row.tip));
            let plus = gtk::Button::with_label("\u{1f5bc}+");
            plus.set_widget_name("thumb-plus");
            plus.set_tooltip_text(Some(row.tip));
            let value = gtk::Label::new(Some(&row.value));
            value.set_xalign(0.0);
            line.append(&minus);
            line.append(&value);
            line.append(&plus);
            line.upcast::<gtk::Widget>()
        };
        value.set_widget_name(&name);
        form.attach(&key, 0, index as i32, 1, 1);
        form.attach(&value, 1, index as i32, 1, 1);
    }
    // The aspect row gets its dropdown next to the shape it reads. §A spells the row as a dropdown
    // rather than a number, so the default is listed first and stays what an unset project shows.
    if let Some(aspect_row) = cut_screen::IDLE_FORM_ROWS
        .iter()
        .position(|l| *l == "Aspect ratio")
    {
        let choice = gtk::DropDown::from_strings(&[
            cut_screen::ASPECT_DEFAULT,
            "4:3",
            "9:16",
            "1:1",
            "21:9",
        ]);
        choice.set_widget_name("aspect-choice");
        choice.set_tooltip_text(Some("the shape the finished video is cut to"));
        form.attach(&choice, 2, aspect_row as i32, 1, 1);
    }
    form
}

/// §4's list of rows, one per source file, drawn from the project and wired to [`crate::sources`].
///
/// The rows are named so a test can find them (`source-row-<index>`), and each control carries its own
/// widget name — the same reason Add's button is named: so a test presses it the way a user does,
/// rather than calling the rule behind it.
fn sources_list(session: &Rc<RefCell<Project>>, status: &gtk::Label) -> gtk::Widget {
    let list = gtk::ListBox::new();
    list.set_widget_name("sources-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    list.set_tooltip_text(Some(sources::LIST_TIP));
    // One hairline between rows, which is how §4's band reads; the frame around it is the page's.
    list.add_css_class("boxed-list");

    for index in 0..session.borrow().sources.len() {
        list.append(&source_row(session, status, index));
    }
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

/// A plain label beside a control, dimmed the way a field name is.
fn label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("dim-label");
    label
}

/// §1's badges **2**-**5**: the bench's heading row and its one text box. Everything the row says —
/// title, mark, Reset's liveness — is asked of [`bench`]; the widgets only forward.
thread_local! {
    /// The row the prompt bench opens on, when a caller says which one. `bench_box` reads this once,
    /// before its first paint; nothing in the app ever sets it, so the ordinary state is `None` =
    /// row 0, the User Context. It exists because `--snapshot 04-prompt-picker` has to show the
    /// picker OPEN on a prompt row (that is what `spec/img/04-prompt-picker.png` pictures) and the
    /// only other way would be to build a second bench for the snapshot — a copy of the page that
    /// could drift from the real one.
    static BENCH_OPEN_ROW: std::cell::RefCell<Option<usize>> =
        const { std::cell::RefCell::new(None) };
}

/// Ask the next bench built in this thread to open on `index`. Snapshot-only seam; see
/// [`BENCH_OPEN_ROW`].
pub fn set_bench_open_row(index: usize) {
    BENCH_OPEN_ROW.with(|slot| *slot.borrow_mut() = Some(index));
}

/// The row `bench_box` should start on: the requested one if it names a real row, else 0. A row
/// index off the end falls back rather than panicking, because a stale snapshot name must still draw
/// something rather than kill the run.
fn bench_open_row(count: usize) -> usize {
    BENCH_OPEN_ROW.with(|slot| {
        slot.borrow()
            .filter(|index| *index < count)
            .unwrap_or(0)
    })
}

fn bench_box(
    session: &Rc<RefCell<Project>>,
    paths: &Option<crate::settings::Paths>,
) -> (gtk::Box, gtk::TextView) {
    let mut bench_state = bench::Bench::new();
    // The snapshot seam above; `select` ignores an index off the end, so this is a no-op unless a
    // caller asked for a specific row.
    bench_state.select(bench_open_row(bench::ROWS.len()));
    let row = Rc::new(RefCell::new(bench_state));
    // Each handler below holds its own handle on the same answer to "where does this machine keep a
    // prompt", which is why it travels as an `Rc` rather than as a borrow of the caller's.
    let paths = Rc::new(paths.clone());

    let heading = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    heading.set_widget_name("bench-heading");

    let title = gtk::Label::new(None);
    title.set_widget_name("prepare-title");
    title.add_css_class("title-4");
    title.set_halign(gtk::Align::Start);
    heading.append(&title);

    let mark = gtk::Label::new(Some(bench::EDITED_MARK));
    mark.set_widget_name("bench-mark");
    mark.set_tooltip_text(Some(bench::EDITED_TIP));
    mark.add_css_class("dim-label");
    heading.append(&mark);

    // **3** the row picker: the thirteen rows in pipeline order, chosen from a menu. A DropDown is
    // GTK's own menu of strings, which is what §1 asks for and what carries the selected index.
    let titles: Vec<String> = bench::picker_rows()
        .iter()
        .map(|(_, title)| title.to_string())
        .collect();
    let picker = gtk::DropDown::from_strings(
        &titles
            .iter()
            .map(String::as_str)
            .collect::<Vec<&str>>(),
    );
    picker.set_widget_name("bench-picker");
    picker.set_halign(gtk::Align::Start);
    // The dropdown's own face must agree with the row the bench opened on, or the shot would read
    // "Describe prompt" beside a picker still showing "User Context". Set before `paint_bench` so
    // no notify fires against a half-built heading.
    picker.set_selected(u32::try_from(row.borrow().selected).unwrap_or(0));

    // **4** Reset, live only while this machine holds an edit.
    let reset = gtk::Button::with_label("Reset");
    reset.set_widget_name("bench-reset");
    reset.set_tooltip_text(Some("Restore the shipped wording"));
    heading.append(&picker);
    heading.append(&reset);

    // **5** the prompt or the User Context.
    let editor = gtk::TextView::new();
    editor.set_widget_name("bench-text");
    editor.set_monospace(true);
    editor.set_wrap_mode(gtk::WrapMode::WordChar);
    editor.set_vexpand(true);
    editor.set_top_margin(6);
    editor.set_bottom_margin(6);
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_child(Some(&editor));
    scroller.set_vexpand(true);

    // The box's contents are set when the row changes, never from its own `changed` handler: typing
    // stores text and storing must not retype the buffer out from under the cursor.
    let guard = Rc::new(RefCell::new(false));
    paint_bench(&row, &title, &mark, &reset, &editor, &guard, session, &paths);

    let session_for_picker = session.clone();
    let paths_for_picker = paths.clone();
    let row_for_picker = row.clone();
    let guard_for_picker = guard.clone();
    let title_for_picker = title.clone();
    let mark_for_picker = mark.clone();
    let reset_for_picker = reset.clone();
    let editor_for_picker = editor.clone();
    picker.connect_selected_notify(move |picker| {
        let index = picker.selected();
        if index == gtk::INVALID_LIST_POSITION {
            return;
        }
        row_for_picker.borrow_mut().select(usize::try_from(index).unwrap_or(0));
        paint_bench(
            &row_for_picker,
            &title_for_picker,
            &mark_for_picker,
            &reset_for_picker,
            &editor_for_picker,
            &guard_for_picker,
            &session_for_picker,
            &paths_for_picker,
        );
    });

    // Reset (**4**) puts the shipped wording back and forgets this machine's file; the buffer then
    // shows what is stored, which is why it is repainted rather than trusted to the click.
    let session_for_reset = session.clone();
    let paths_for_reset = paths.clone();
    let title_for_reset = title.clone();
    let mark_for_reset = mark.clone();
    let editor_for_reset = editor.clone();
    let reset_for_reset = reset.clone();
    let row_for_reset = row.clone();
    let guard_for_reset = guard.clone();
    reset.connect_clicked(move |_| {
        let current = row_for_reset.borrow().row().clone();
        let Some(paths) = paths_for_reset.as_ref() else { return };
        // User Context has no shipped wording to restore, and `bench::reset` says so with `None`.
        if let Ok(Some(_)) = bench::reset(paths, &current) {
            paint_bench(
                &row_for_reset,
                &title_for_reset,
                &mark_for_reset,
                &reset_for_reset,
                &editor_for_reset,
                &guard_for_reset,
                &session_for_reset,
                &paths_for_reset,
            );
        }
    });

    let buffer = editor.buffer();
    let session_for_text = session.clone();
    let paths_for_text = paths.clone();
    let mark_for_text = mark;
    let reset_for_text = reset;
    let row_for_text = row;
    buffer.connect_changed(move |buffer| {
        if *guard.borrow() {
            return;
        }
        let (start, end) = (buffer.bounds().0, buffer.bounds().1);
        let text = buffer.text(&start, &end, false).to_string();
        // Every keystroke is stored (§1): there is no Save button on this bench. The session takes
        // the text whatever this machine's settings can do with it — User Context lives there, and a
        // prompt that cannot be written must still not be thrown away.
        let current = row_for_text.borrow().row().clone();
        let mut held = session_for_text.borrow_mut();
        match paths_for_text.as_ref() {
            Some(paths) => {
                let _ = bench::store(paths, &mut held, &current, &text);
            }
            // With no config folder a prompt has nowhere to go; the User Context still lands on the
            // session, which is where it belongs.
            None if bench::is_context(&current) => {
                held.context = text.clone();
            }
            None => {}
        }
        let edited = match paths_for_text.as_ref() {
            Some(paths) => bench::edited(paths, &current),
            None => false,
        };
        mark_for_text.set_visible(bench::shows_mark(&current, edited));
        reset_for_text.set_visible(bench::shows_reset(&current, edited));
        reset_for_text.set_sensitive(bench::shows_reset(&current, edited));
        // F0.7's first door: editing the User Context re-derives the policy — but debounced, so a
        // paragraph typed costs one request rather than one per keystroke. Only the context row
        // triggers it; every other bench row is prompt wording, which no policy depends on.
        if bench::is_context(&current) {
            if let Some(window) = POLICY_WATCH_WINDOW.with(|w| w.borrow().clone()) {
                schedule_policy_derive(&window);
            }
        }
    });

    // The heading and the box are returned together; `page_box` stacks them in §1's order. The
    // scroller is dropped with its child still inside: GTK keeps what it was added to.
    drop(scroller);
    (heading, editor)
}

/// Where this machine keeps an edited prompt (§1's ✎). `None` — no config home at all — leaves the
/// bench showing rows with no mark rather than pretending to hold edits it cannot write.
fn bench_paths() -> Option<crate::settings::Paths> {
    crate::settings::from_environment()
}

/// Where settings are not: no folder at all, so every read finds nothing and every shipped wording
/// shows through. Used when this machine has no config home to write prompts into.
const NO_SETTINGS: crate::settings::Paths = crate::settings::Paths {
    config_dir: std::path::PathBuf::new(),
    data_dir: std::path::PathBuf::new(),
};

/// Draw one row of the bench: its title, whether this machine holds it, and its text.
fn paint_bench(
    row: &Rc<RefCell<bench::Bench>>,
    title: &gtk::Label,
    mark: &gtk::Label,
    reset: &gtk::Button,
    editor: &gtk::TextView,
    guard: &Rc<RefCell<bool>>,
    session: &Rc<RefCell<Project>>,
    paths: &Option<crate::settings::Paths>,
) {
    let held = row.borrow();
    title.set_text(&bench::title(held.row()));
    paint_bench_marks(mark, reset, paths.as_ref(), held.row());

    // The text is set here and only here, with the store handler parked: writing into a buffer
    // fires `changed`, and a `changed` that stores would repaint the buffer again.
    *guard.borrow_mut() = true;
    let buffer = editor.buffer();
    // With no config folder there is nothing this machine could hold, so the box shows the shipped
    // wording — which is what `bench::text` answers for a row nobody has touched.
    buffer.set_text(&bench::text(paths.as_ref().unwrap_or(&NO_SETTINGS), &session.borrow(), held.row()));
    *guard.borrow_mut() = false;
}

/// The mark and Reset, both live only while this machine holds the row's wording (§1).
fn paint_bench_marks(
    mark: &gtk::Label,
    reset: &gtk::Button,
    paths: Option<&crate::settings::Paths>,
    row: &bench::Row,
) {
    let edited = paths.is_some_and(|paths| bench::edited(paths, row));
    mark.set_visible(bench::shows_mark(row, edited));
    reset.set_visible(bench::shows_reset(row, edited));
    reset.set_sensitive(bench::shows_reset(row, edited));
}

/// A symbol control: an icon, flat so a row of them reads as one band rather than a row of boxes, and
/// carrying §4's key to the four symbols at the end of its tooltip.
fn icon_toggle(icon: &str, name: &str, tip: &str) -> gtk::ToggleButton {
    let button = gtk::ToggleButton::builder().icon_name(icon).build();
    button.set_widget_name(name);
    button.add_css_class("flat");
    button.set_tooltip_text(Some(&format!("{tip}{}", sources::ROW_KEY)));
    button
}

/// The same for a control that is pressed rather than ticked. `icon` empty leaves the face to be set by
/// the caller, which is how the narrator button shows the slot it holds.
fn icon_button(icon: &str, name: &str, tip: &str) -> gtk::Button {
    let button = if icon.is_empty() {
        gtk::Button::new()
    } else {
        gtk::Button::from_icon_name(icon)
    };
    button.set_widget_name(name);
    button.add_css_class("flat");
    button.set_tooltip_text(Some(&format!("{tip}{}", sources::ROW_KEY)));
    button
}

/// One row: the file's name, and the controls §4 gives it. A control that does not apply to this file
/// is absent rather than insensitive — an audio row has no footage tick to grey out.
fn source_row(session: &Rc<RefCell<Project>>, status: &gtk::Label, index: usize) -> gtk::ListBoxRow {
    let source = session.borrow().sources[index].clone();
    let name = sources::row_name(&source.path);

    let row = gtk::ListBoxRow::new();
    row.set_widget_name(&format!("source-row-{index}"));
    row.set_tooltip_text(Some(&source.path));

    let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 12);

    if sources::row_controls(&source, source.tracks.len().max(1)).contains(&Control::Footage) {
        let footage = icon_toggle("camera-video-symbolic", &format!("footage-{index}"), sources::FOOTAGE_TIP);
        footage.set_active(source.footage);
        footage.set_active(source.footage);
        footage.set_tooltip_text(Some(sources::FOOTAGE_TIP));
        wire_footage(&footage, Rc::clone(session), status, index, name.clone());
        box_.append(&footage);
    }

    let narrator = icon_button("", &format!("narrator-{index}"), &sources::NARRATOR_TIP);
    // The face is the slot, which is the one thing about this control a glance has to catch; slot 1 is
    // the narration's voice, so it is drawn as suggested (§4's "slot 1 highlighted").
    narrator.set_label(&source.narrator.to_string());
    if source.narrator == 1 {
        narrator.add_css_class("suggested-action");
    }
    wire_narrator(&narrator, Rc::clone(session), status, index, name.clone());
    box_.append(&narrator);

    let label = gtk::Label::new(Some(&name));
    label.set_widget_name(&format!("source-name-{index}"));
    label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    label.set_hexpand(true);
    label.set_xalign(0.0);
    box_.append(&label);

    let remove = icon_button("user-trash-symbolic", &format!("remove-{index}"), sources::REMOVE_TIP);
    wire_remove(&remove, Rc::clone(session), status, index);
    box_.append(&remove);

    row.set_child(Some(&box_));
    row
}

/// The 🎥 toggle. Each of the three handlers below takes the handles it needs by value, which is what
/// keeps this to one `Rc` clone per control instead of a shadowed local moved into the first closure
/// and missed by the next — the rule itself is [`sources::toggle_footage`], and the page only repeats
/// what it did.
fn wire_footage(
    footage: &gtk::ToggleButton,
    session: Rc<RefCell<Project>>,
    status: &gtk::Label,
    index: usize,
    name: String,
) {
    let status = status.clone();
    footage.connect_clicked(move |button| {
        if sources::toggle_footage(&mut session.borrow_mut(), index, button.is_active()) {
            status.set_text(&format!(
                "{name}: {}",
                if button.is_active() { "footage" } else { "not footage" }
            ));
        }
    });
}

/// The 🎤 narrator button. `face` is the button's own handle: `connect_clicked` takes the closure by
/// moving it, so the button cannot also be borrowed into it.
fn wire_narrator(
    narrator: &gtk::Button,
    session: Rc<RefCell<Project>>,
    status: &gtk::Label,
    index: usize,
    name: String,
) {
    let status = status.clone();
    let face = narrator.clone();
    narrator.connect_clicked(move |_| {
        let slot = sources::cycle_narrator(&mut session.borrow_mut(), index);
        // The button's face is the slot it just took, so the row repaints itself.
        face.set_label(&slot.to_string());
        face.set_tooltip_text(Some(&format!("{}{}", sources::slot_tip(slot), sources::ROW_KEY)));
        face.remove_css_class("suggested-action");
        if slot == 1 {
            face.add_css_class("suggested-action");
        }
        status.set_text(&format!("{name}: narrator {slot}"));
    });
}

/// The 🗑 button.
fn wire_remove(
    remove: &gtk::Button,
    session: Rc<RefCell<Project>>,
    status: &gtk::Label,
    index: usize,
) {
    let status = status.clone();
    remove.connect_clicked(move |_| {
        // The path is read before the list changes, so the sentence can name what left. Dropping a row
        // re-indexes every row below it, and these widgets are drawn once — redrawing the list belongs
        // to F0.9's live project state, so this page reports the press without pretending to repaint.
        let Some(path) = session.borrow().sources.get(index).map(|source| source.path.clone()) else {
            return;
        };
        if sources::remove(&mut session.borrow_mut(), index).is_some() {
            let left = session.borrow().sources.len();
            status.set_text(&sources::removal_status(&path, left));
        }
    });
}

/// The tab row, drawn from [`Shell`].
///
/// A locked page's tab is greyed and carries the lock reason as its tooltip; an unlocked one carries
/// its own description (§03-shell.md §1: "Locked tab: greyed, not disabled; tooltip = the reason").
/// Nothing here decides which of those two applies — [`shell::lock`] does.
///
/// `adw::ViewSwitcher` builds its buttons itself and offers no handle to them (its API is
/// `stack`/`policy` alone), so they are found by walking the switcher's descendants for the label
/// each button draws — the page word, which is unique to one tab. Reaching the button rather than
/// the label matters: a tooltip on the label would vanish with the label when the bar narrows to
/// icons, and greying only the text would leave the button looking live.
fn paint_tabs(switcher: &adw::ViewSwitcher, shell: &Shell, project: &Project) {
    let buttons = tab_buttons(switcher.upcast_ref());
    for page in Page::all() {
        let reason = shell::lock(page, project);
        // The page shown is never greyed, even where it is locked: §1's "prerequisites vanish while
        // open" case leaves a locked page on screen for the moment before it is sent back.
        let dimmed = reason.is_some() && shell.page != page;
        let tooltip = match reason {
            Some(reason) if dimmed => reason,
            _ => page.tip(),
        };
        let matching = buttons
            .iter()
            .filter(|button| has_label(button, page.label()))
            .max_by_key(|button| depth(button));
        if let Some(button) = matching {
            button.set_tooltip_text(Some(tooltip));
            if dimmed {
                button.add_css_class("dim-label");
            } else {
                button.remove_css_class("dim-label");
            }
        }
    }
}

/// Every clickable inside `root`, deepest last.
fn tab_buttons(root: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut out = Vec::new();
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
        if widget.downcast_ref::<gtk::Button>().is_some() {
            out.push(widget.clone());
        }
        out.extend(tab_buttons(&widget));
    }
    out
}

fn has_label(widget: &gtk::Widget, text: &str) -> bool {
    descendants(widget)
        .iter()
        .filter_map(|child| child.downcast_ref::<gtk::Label>())
        .any(|label| label.text() == text)
}

/// How deep a widget sits under its ancestor: the tab button holding the label is the deepest one,
/// while its rows and the switcher itself hold it too.
fn depth(widget: &gtk::Widget) -> usize {
    descendants(widget).len()
}

fn descendants(root: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut out = Vec::new();
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        if let Ok(widget) = child.downcast::<gtk::Widget>() {
            out.push(widget.clone());
            out.extend(descendants(&widget));
        }
    }
    out
}

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
    wire_new(&new_, &bar, &status);

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
    wire_save(&save_, &bar, &status, project);

    // **8** sits at the RIGHT end of the bar, next to where Settings goes — `pack_end`, not
    // `pack_start`, so it does not join New and Save.
    let rescan_ = gtk::Button::from_icon_name("view-refresh-symbolic");
    rescan_.set_widget_name("rescan-button");
    rescan_.set_tooltip_text(Some(RESCAN_TIP));
    header.pack_end(&rescan_);
    wire_rescan(
        &rescan_,
        &status,
        &session,
        &shell,
        &switcher,
        &inputs_readout,
        &outputs_folder,
        &outputs_readout,
    );

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
    let runs = crate::timeline::filmed_runs(&crate::timeline::kept_footage_recordings(&opened_cut));
    REVIEW_CUTS.with(|slots| slots.borrow_mut().push(Rc::new(std::cell::RefCell::new(opened_cut))));
    if let Some(review_) = review_cuts_button(&window) {
        wire_review_cuts(&review_, &window);
    }
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
    // §05-cut#1-screen: the history group (Undo / Redo / Revert / Clear), the zoom pair and their
    // chords, wired after `set_content` like every other control on this page.
    wire_history_and_zoom(&window);
    wire_history_keys(&window);
    // F2.8: the trim and move gestures on `track-strip`, wired after `set_content` for the same reason.
    wire_track_strip(&window);
    // F2.10: fill `camera-rows` now that the page is inside the tree, so the search for it succeeds
    // and the plates, badges and switches a test fires by name are actually there.
    refresh_camera_rows(&window);
    window
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
fn session_reads() -> (Option<layout::Tree>, Cut, Narration) {
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

/// S4's readout row, drawn from the shell. The two texts come from `Shell::inputs` and
/// `Shell::outputs` alone — this function only prefixes and sets labels, so a sentence on that row
/// always traces back to a rule in `shell.rs`. The folder button is insensitive while there is no
/// project folder to open, which is the visible proof the row knows what it is counting.
fn paint_readouts(
    shell: &Shell,
    project: &Project,
    tree: Option<&layout::Tree>,
    cut: &Cut,
    narration: &Narration,
    inputs_label: &gtk::Label,
    outputs_folder: &gtk::Button,
    outputs_label: &gtk::Label,
) {
    inputs_label.set_text(&format!("{INPUTS_PREFIX}{}", shell.inputs(tree, project, cut, narration)));
    // §1 badge **6**: the Inputs row's tooltip is per-file. Prepare has one; every other page's
    // row clears it rather than keeping a stale per-source list from the tab just left.
    match shell.inputs_tip(tree, project) {
        Some(tip) => {
            inputs_label.set_tooltip_text(Some(&tip));
            inputs_label.set_sensitive(true);
        }
        None => inputs_label.set_tooltip_text(None::<&str>),
    }
    outputs_label.set_text(&format!("{OUTPUTS_PREFIX}{}", shell.outputs(tree)));
    outputs_folder.set_sensitive(tree.is_some());
    // §1: "Outputs: folder button and count" — the button points at the page's own output folder,
    // which is the directory `Shell::outputs` just counted.
    if let Some(tree) = tree {
        // Prepare's tooltip goes further than "open this folder": §1 asks that it name the three
        // subfolders the count covers, so the number says what is inside it.
        let page_tip = if shell.page == crate::shell::Page::Prepare {
            format!(" ({})", prepare::OUTPUTS_TIP)
        } else {
            String::new()
        };
        outputs_folder.set_tooltip_text(Some(&format!(
            "{OUTPUTS_FOLDER_TIP}: {}{page_tip}",
            shell.output_dir(tree).display()
        )));
    }
    // S4's ⓘ sync: the header button describes the page `help_page` names, which a switch sets and
    // a bounce leaves alone. Repainting it here is what makes "sync ⓘ" happen on the same tick as the
    // readouts rather than in a second pass over the window.
    if let Some(info) = INFO_BUTTON.with(|slots| slots.borrow().last().cloned()) {
        info.set_tooltip_text(Some(&shell.info_tip()));
    }
}

/// The run bar as it should be drawn right now: the icon and tooltip [`run::controls`] says, and
/// nothing else. It decides nothing — ▶ is one button that becomes ⏸ rather than a second button
/// beside it, and which of the two applies is F0.2's precedence, settled in run.rs.
/// The run bar's icon and tooltip are the whole of what F0.2 puts on screen. `transport` is the
/// visible page's preview, which only Cut and Narrate have — [`run::transport_for`] answers `None`
/// for the other two, and that is why their ▶ runs the step while a preview plays.
fn paint_run_bar(
    play: &gtk::Button,
    stop_: &gtk::Button,
    bar: &run::RunBar,
    page: Page,
    transport: run::Transport,
) {
    let drawn = run::controls(&bar.running, run::transport_for(page, transport));
    play.set_icon_name(drawn.icon);
    play.set_tooltip_text(Some(drawn.tooltip));
    // ⏹ is sensitive whenever there is something to end — a run, a playing preview, or one parked
    // part way through. `controls` already decides that; this only draws it.
    stop_.set_sensitive(drawn.stop_sensitive);
}

/// §10's prose log heights: 220 px open, 110 px collapsed. No `P.` id exists for them, so they are
/// bare constants here with the note rather than invented parameter rows.
const LOG_OPEN_PX: i32 = 220;

/// Draw F0.5's two widgets from the bookkeeping state: the bar's fraction and tooltip come from the
/// queue, the expander's openness from the run's `log_expanded`, and its body from the lines logged
/// so far. No decisions here — [`runqueue`] decided them, this paints them.
fn paint_progress(
    progress: &gtk::ProgressBar,
    log: &gtk::Expander,
    log_view: &gtk::TextView,
    queue: &runqueue::Queue,
    expanded: bool,
) {
    let (_text, tip) = queue.text_and_tip();
    progress.set_fraction(queue.fraction());
    // A run with nothing queued yet leaves the standing tooltip rather than blanking it (§2).
    if !tip.is_empty() {
        progress.set_tooltip_text(Some(&tip));
    }
    log.set_expanded(expanded);
    if expanded {
        let mut body = String::new();
        for line in window_logs() {
            body.push_str(&line);
            body.push('\n');
        }
        let buffer = log_view.buffer();
        buffer.set_text(&body);
        // The newest line is the one worth seeing.
        let mut end = buffer.end_iter();
        log_view.scroll_to_iter(&mut end, 0.0, false, 0.0, 1.0);
    }
}

/// Where a press of ▶ is decided: ask [`run::RunBar::press`], then redraw the button and put the
/// bar's sentence on the status line. Same shape as [`wire_switching`] — one handler, one job.
///
/// The transport is `Transport::default()` because no page has a preview model yet (the Cut review
/// and the Narrate voice sample arrive with their own pages' rounds), so nothing is ever playing to
/// be toggled; ⏹, which would end a preview, is F0.3.
fn wire_play(
    play: &gtk::Button,
    stop_: &gtk::Button,
    bar: &Rc<RefCell<run::RunBar>>,
    shell: &Rc<RefCell<Shell>>,
    status: &gtk::Label,
    progress: &gtk::ProgressBar,
    log: &gtk::Expander,
    log_view: &gtk::TextView,
    queue: &Rc<RefCell<runqueue::Queue>>,
    exchange_log: &Rc<RefCell<exchanges::RunLog>>,
    project: &Project,
) {
    let bar = bar.clone();
    let shell = shell.clone();
    let status = status.clone();
    let project = project.clone();
    let stop_ = stop_.clone();
    let progress = progress.clone();
    let log = log.clone();
    let log_view = log_view.clone();
    let queue = queue.clone();
    let exchange_log = exchange_log.clone();
    play.connect_clicked(move |play| {
        // F1.1 S1: Prepare refuses at its own start, BEFORE the bar is opened — a run that started
        // and was then sorry would leave ⏸/⏹ showing for work that never began. So the refusal is
        // asked first and short-circuits here; every other page (and every non-refused press) falls
        // through to `press`, whose precedence F0.2 owns.
        let prepare_asked = run::step(shell.borrow().page) == run::Step::Prepare;
        let live = PLAY_SESSION.with(|slots| {
            slots
                .borrow()
                .last()
                .map(|held| held.borrow().clone())
                .unwrap_or_else(|| project.clone())
        });
        if prepare_asked {
            if let Some((log, sentence)) = prepare_run::refuse(&live) {
                log_line(&log);
                status.set_text(&sentence);
                paint_run_bar(play, &stop_, &bar.borrow(), shell.borrow().page, run::Transport::default());
                return;
            }
        }

        let pressed = bar.borrow_mut().press(shell.borrow().page, run::Transport::default(), &project);
        // A refusal has to survive to the end of this handler: `run::RunBar::press` leaves its own
        // status empty when a run starts (the run is meant to overwrite it), and the tail below
        // copies that empty string onto the label. Writing the refusal earlier would erase it.
        let mut refusal: Option<String> = None;
        // Whether this press may open a run. F1.1's refusals set it false so `start_run` is skipped
        // and the bar never shows a run that was refused before starting.
        let mut started = true;
        // F0.5 S1: a press that opened a run also opened the bookkeeping — fresh cancel context,
        // empty queue, model log closed, log expander open. A pause or a transport toggle is not a
        // new run, so only `Started` goes through `start_run`.
        if matches!(pressed, run::Pressed::Started { .. }) {
            // F0.7's second door: a context nobody has derived from yet is derived before the step
            // reads it, so ▶ never runs on a policy that lags what the person just typed. Only when
            // the tracker says so — an unchanged context costs nothing, and the LLM cache makes a
            // repeated identical request free besides.
            let context = user_context(&main_window());
            if POLICY_TRACKERS
                .with(|trackers| trackers.borrow().last().cloned())
                .is_some_and(|tracker| tracker.borrow().needs_derive(&context))
            {
                let derived = derive_policy(&main_window());
                if !derived.used_defaults {
                    log_line(&format!("policy: {}", derived.status()));
                }
            }
            // F1.12 S2: the Cut step first asks whether `final.txt` was hand-edited since the marks
            // were written. The rule is entirely in `hand_edit::before_cut`; this handler only runs
            // it for the page that owns the marks and forwards what it says. Spec silent on a lucky
            // all-steps run (F0.4), so it is checked here too — the same press, the same question.
            if run::step(shell.borrow().page) == run::Step::Suggest {
                let dir = startup::session_dir(&std::env::current_dir().unwrap_or_default());
                if let Ok(tree) = layout::Tree::new(&dir) {
                    let sources: Vec<String> = project
                        .sources
                        .iter()
                        .map(|source| source.path.clone())
                        .collect();
                    if let Some(outcome) = hand_edit::before_cut(&tree, &sources) {
                        for line in &outcome.logs {
                            log_line(line);
                        }
                        if outcome.refused {
                            // The refusal sentence goes where a press's answer is read — held until
                            // after the tail's own status write, which would otherwise blank it.
                            refusal = Some(
                                outcome
                                    .logs
                                    .last()
                                    .map(|line| line.trim_start_matches('!').trim_start().to_string())
                                    .unwrap_or_else(|| "the edit was refused".to_string()),
                            );
                        }
                    }
                }
            }
            // F1.1 S1-S4: Prepare's own start flow, run BEFORE the bookkeeping opens a run on it.
            // A refusal never becomes a run at all — `start_run` is not reached and the status line
            // carries the sentence instead. Every rule lives in `prepare_run`; this block only finds
            // the tree the window works in, prints what came back, and remembers a refusal for the
            // tail below (which would otherwise blank the line with the bar's empty status).
            if run::step(shell.borrow().page) == run::Step::Prepare {
                let dir = startup::session_dir(&std::env::current_dir().unwrap_or_default());
                // Ask about the live session, not the argument this window was built with: sources get
                // added and removed after that call, and a refusal about stale data is a wrong refusal.
                let asked = PLAY_SESSION.with(|slots| {
                    slots
                        .borrow()
                        .last()
                        .map(|held| held.borrow().clone())
                        .unwrap_or_else(|| project.clone())
                });
                match layout::Tree::new(&dir)
                    .ok()
                    .map(|tree| (prepare_run::begin(&asked, &tree), tree))
                {
                    Some((prepare_run::Start::Refused { log, status }, _)) => {
                        log_line(&log);
                        refusal = Some(status);
                        started = false;
                    }
                    Some((prepare_run::Start::SaveFailed { error }, _)) => {
                        log_line(&format!("!!! could not save the project -- {error}"));
                        refusal = Some(prepare_run::failed_status(&error));
                        started = false;
                    }
                    // The opening lines go out whether or not a runner follows them. F1.7's Describe
                    // stage now speaks here too (see below), so the log is no longer the last word;
                    // what still has no runner behind it are the stages of F1.2/F1.3/F1.8-F1.10, which
                    // is why the bar sits at zero rather than pretending — and why ⏹ still has
                    // something to stop here.
                    Some((prepare_run::Start::Started { lines, .. }, tree)) => {
                        for line in &lines {
                            log_line(line);
                        }
                        // F1.7's Describe stage speaks here too: one forwarder per footage source,
                        // which PLANS what would be sent and prints it. Nothing is spawned and no
                        // vision server is contacted — the requests themselves arrive with the runner
                        // rounds; what lands now is the plan the person can read while ▶ is pressed.
                        let freq = asked.interval;
                        for source in &asked.sources {
                            if !source.footage {
                                continue;
                            }
                            // The lane Describe files a source's work under is its file name minus the
                            // extension — the same rule `prepare_run::lane` and `frames.rs` use, so
                            // this stage reads exactly the folder F1.6 wrote.
                            let lane = source
                                .path
                                .rsplit('/')
                                .next()
                                .unwrap_or(&source.path)
                                .split_once('.')
                                .map(|(stem, _)| stem.to_string())
                                .unwrap_or_else(|| source.path.clone());
                            for line in describe::stage_log_for(&tree, &lane, freq, &[]) {
                                log_line(&line);
                            }
                        }
                        // F1.13's session word list is built FIRST, before either marking pass reads
                        // it: retakes and joins both dress their words off this one list, so a pass
                        // that ran without it would be matching against spellings nothing wrote. The
                        // gate lives inside the call — under MarkingPass::None it returns nothing at
                        // all, and a list already on disk is resumed, not re-glued. No model is asked.
                        for line in crate::word_list::press_word_list(
                            &tree,
                            &asked,
                            crate::policy::marking_pass_of(&asked.policy),
                        ) {
                            log_line(&line);
                        }
                        // F1.9's retake marking pass speaks here as well, through ONE entry point and
                        // only when the policy names this pass: under Joins or None the call returns
                        // nothing at all, so one press never speaks twice for two different passes.
                        // Marks come from the model, which is not contacted here; what lands without a
                        // server is S1's too-few-lines answer and the empty marks file it owes Cut.
                        for line in retakes::press_marking(
                            &tree,
                            &asked,
                            crate::policy::marking_pass_of(&asked.policy),
                        ) {
                            log_line(&line);
                        }
                        // F1.10's join repair joins the same press through ONE entry point, gated the
                        // same way: under Retakes or None it returns nothing at all, so one press
                        // never speaks for two passes. The answers are the textedit model's and no
                        // server is contacted here; what lands without one is S1's too-few-words
                        // answer, S2's no-seam sentence, and the two files the pass owes Cut.
                        for line in joins::press_joins(
                            &tree,
                            &asked,
                            crate::policy::marking_pass_of(&asked.policy),
                        ) {
                            log_line(&line);
                        }
                    }
                    // No project folder yet: nothing to clear and nothing to save into, so the press
                    // starts the run on the empty session like any other page does.
                    None => (),
                }
            }
            if !started {
                paint_run_bar(play, &stop_, &bar.borrow(), shell.borrow().page, run::Transport::default());
                status.set_text(refusal.as_deref().unwrap_or(""));
                return;
            }
            runqueue::start_run(
                &mut bar.borrow_mut(),
                &mut queue.borrow_mut(),
                &mut exchange_log.borrow_mut(),
                run::step(shell.borrow().page),
                run::snapshot_sources(&project),
            );
        }
        paint_run_bar(
            play,
            &stop_,
            &bar.borrow(),
            shell.borrow().page,
            run::Transport::default(),
        );
        status.set_text(&bar.borrow().status);
        paint_progress(
            &progress,
            &log,
            &log_view,
            &queue.borrow(),
            bar.borrow().running.as_ref().is_some_and(|run| run.log_expanded),
        );
        // Last write wins on the label: F1.12's refusal is why ▶ did not do what it was pressed for,
        // so it goes on screen after the bar's own (empty) status rather than under it.
        if let Some(reason) = refusal {
            status.set_text(&reason);
        }
    });
}

/// Where a press of ⏹ is decided: ask [`run::RunBar::press_stop`], then redraw both buttons and put
/// the bar's sentence on the status line. The handler forwards and decides nothing
/// (spec/00-principles.md §5) — which of S1/S2/S3 applies, and what each one stops, is run.rs's rule.
///
/// `in_describe` is `false` here because the shell does not know which stage is running; F1.7's own
/// round passes the real value when it drives Describe (S5 arms "describe from the start").
fn wire_stop(
    stop_: &gtk::Button,
    play: &gtk::Button,
    bar: &Rc<RefCell<run::RunBar>>,
    shell: &Rc<RefCell<Shell>>,
    status: &gtk::Label,
    progress: &gtk::ProgressBar,
    log: &gtk::Expander,
    log_view: &gtk::TextView,
    queue: &Rc<RefCell<runqueue::Queue>>,
    procs: &Rc<RefCell<run::Subprocesses>>,
) {
    let bar = bar.clone();
    let shell = shell.clone();
    let status = status.clone();
    let procs = procs.clone();
    let play = play.clone();
    let progress = progress.clone();
    let log = log.clone();
    let log_view = log_view.clone();
    let queue = queue.clone();
    stop_.connect_clicked(move |stop_| {
        let stopped = bar.borrow_mut().press_stop(
            shell.borrow().page,
            run::Transport::default(),
            false,
            &mut procs.borrow_mut(),
        );
        // Repaint both: stopping a run changes ▶'s face as well as ⏹'s sensitivity.
        paint_run_bar(
            &play,
            stop_,
            &bar.borrow(),
            shell.borrow().page,
            run::Transport::default(),
        );
        // Only write a sentence when there is one: S2's "nothing more" must leave an existing status
        // alone rather than blanking it.
        if !stopped.status().is_empty() {
            status.set_text(stopped.status());
        }
        paint_progress(
            &progress,
            &log,
            &log_view,
            &queue.borrow(),
            bar.borrow().running.as_ref().is_some_and(|run| run.log_expanded),
        );
    });
}

/// Where a press of the "I'm feeling lucky" gears is decided: ask [`lucky::Chain::start`] whether the
/// bar will allow it, then walk the chain one step at a time. The handler forwards and decides nothing
/// (spec/00-principles.md §5) — S1's refusal, S3's opening line, S4's skips and S6's end sentence all
/// come from `lucky.rs`.
///
/// Each step is handed over by switching to its page synchronously through the same `stack`/`guard`
/// path F0.1 uses (a step's own work belongs to the widgets of the page it runs on), logging
/// `>>> run: <Name>`, and then reporting the outcome back with [`lucky::Chain::next`]. **The real
/// outcomes arrive with F1.1/F2.14/F4.1/F5.1's rounds** — those step bodies are not implemented yet,
/// so every step reports [`lucky::StepOutcome::Declined`] here. What this item pins is the wire and the
/// state machine, not the work behind each step; when a flow lands it replaces that one call with its
/// own outcome and the chain needs no other change.
fn wire_lucky(
    lucky_button: &gtk::Button,
    bar: &Rc<RefCell<run::RunBar>>,
    shell: &Rc<RefCell<Shell>>,
    stack: &adw::ViewStack,
    guard: &Rc<RefCell<bool>>,
    status: &gtk::Label,
    progress: &gtk::ProgressBar,
    log: &gtk::Expander,
    log_view: &gtk::TextView,
    queue: &Rc<RefCell<runqueue::Queue>>,
    exchange_log: &Rc<RefCell<exchanges::RunLog>>,
    procs: &Rc<RefCell<run::Subprocesses>>,
    project: Project,
) {
    let lucky_button = lucky_button.clone();
    let bar = bar.clone();
    let shell = shell.clone();
    let stack = stack.clone();
    let guard = guard.clone();
    let status = status.clone();
    let procs = procs.clone();
    let progress = progress.clone();
    let log = log.clone();
    let log_view = log_view.clone();
    let queue = queue.clone();
    let exchange_log = exchange_log.clone();
    lucky_button.connect_clicked(move |button| {
        // S1: busy is read from the same bar state ▶ and ⏹ answer for, so the three buttons cannot
        // disagree about whether something is going.
        let busy = bar.borrow().running.is_some() || bar.borrow().stop_flag;
        let (mut chain, opening) = match lucky::Chain::start(busy) {
            Ok(started) => started,
            Err(refusal) => {
                status.set_text(refusal);
                return;
            }
        };
        log_line(&opening);

        // F0.5 S1: the chain is a run like any other, so it opens the bookkeeping the same way —
        // fresh cancel context, empty queue, model log closed, log expanded.
        runqueue::start_run(
            &mut bar.borrow_mut(),
            &mut queue.borrow_mut(),
            &mut exchange_log.borrow_mut(),
            run::step(shell.borrow().page),
            run::snapshot_sources(&project),
        );
        paint_progress(&progress, &log, &log_view, &queue.borrow(), true);

        // S2: the gears turn while the chain runs. Animation itself is cosmetic; what matters is that
        // the button cannot be pressed a second time mid-chain, which S1 would also refuse.
        button.set_sensitive(false);
        button.add_css_class("lucky-running");

        loop {
            let advance = chain.next(lucky::StepOutcome::Declined);
            match advance {
                lucky::Advance::Run { page, name } => {
                    // F0.1's synchronous move: show the page so the step has its widgets. `guard`
                    // keeps the stack's own write-back from re-entering the tab handler.
                    *guard.borrow_mut() = true;
                    stack.set_visible_child_name(page.label());
                    *guard.borrow_mut() = false;
                    shell.borrow_mut().page = page;
                    log_line(&lucky::step_line(name));
                }
                lucky::Advance::Skipped { line } => {
                    log_line(&line);
                    // A skip closes no running step; keep walking.
                }
                lucky::Advance::End { line, status: short } => {
                    log_line(&line);
                    status.set_text(&short);
                    // F0.5 S4: the chain ended, so its bookkeeping ends with it and the bar stops.
                    runqueue::end_run(&mut bar.borrow_mut(), None);
                    queue.borrow_mut().reset();
                    break;
                }
                lucky::Advance::Idle => break,
            }
            // A Declined outcome never blocks, but a stop flag set from elsewhere (⏹ during the chain)
            // must end it rather than spin.
            if bar.borrow().stop_flag {
                let stopped = chain.next(lucky::StepOutcome::Stopped);
                if let lucky::Advance::End { line, status: short } = stopped {
                    log_line(&line);
                    status.set_text(&short);
                    runqueue::end_run(&mut bar.borrow_mut(), None);
                    queue.borrow_mut().reset();
                }
                break;
            }
        }

        // The chain is over either way: give the button back. F0.5 S4 closes its bookkeeping too —
        // running off, so nothing downstream thinks a run still stands; the log's own state is kept
        // as it was painted (open), because that is where the end line and any refusal were read.
        runqueue::end_run(&mut bar.borrow_mut(), None);
        queue.borrow_mut().reset();
        button.set_sensitive(true);
        button.remove_css_class("lucky-running");
        paint_progress(
            &progress,
            &log,
            &log_view,
            &queue.borrow(),
            log.is_expanded(),
        );
        let _ = procs.borrow();
    });
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
/// user sees comes from that module, including the refusal's and the confirmation's.
///
/// The session is never reported empty here because the window has no source list to ask — its
/// project is the one it was built with, and the pages that own sources arrive with their own rounds.
/// A run under way is read from F0.2's [`run::RunBar`], which is the same state ▶ answers for.
fn wire_new(new_: &gtk::Button, bar: &Rc<RefCell<run::RunBar>>, status: &gtk::Label) {
    let bar = bar.clone();
    let status = status.clone();
    new_.connect_clicked(move |_| {
        let root = std::env::current_dir().unwrap_or_default();
        let open = startup::session_dir(&root);
        match new_project::press(bar.borrow().running.is_some(), false, &open) {
            new_project::Gate::Refused { reason } => status.set_text(reason),
            new_project::Gate::Name => ask_new_project(&status, &root, &open),
            new_project::Gate::Confirm { detail } => {
                let dialog = new_project_confirm(None, &detail);
                let status = status.clone();
                let root = root.clone();
                let open = open.clone();
                dialog.connect_response(None, move |dialog: &adw::MessageDialog, response| {
                    // Cancel is not pressing on: S3's whole point is that nothing was changed.
                    if response == "start" {
                        ask_new_project(&status, &root, &open);
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
fn ask_new_project(status: &gtk::Label, root: &Path, open: &Path) {
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
                Ok(created) => status.set_text(&created.status),
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

/// Where a press of Rescan goes: [`rescan`] decides everything — which rows are gone, what was
/// re-read, what the status says. The button forwards and draws the answer (spec/00-principles.md §5).
///
/// No run refusal here: S1 drops what has vanished whether or not a run is on, so unlike Save this
/// handler never reads [`run::RunBar`].
/// F0.13: the gear opens the Settings dialog over this window. Nothing is decided here — the rows,
/// their badges and Test All live in [`settings`], and every pass rule lives in [`crate::checks`].
fn wire_settings(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        settings::open_with(&window, settings::no_probe());
    });
}

/// F0.7 S5: where a press of the policy control goes — build the form off this window's live policy and
/// show it. The button decides nothing: [`policy_form::build`] reads every row from
/// [`policy::derived_fields`] plus the [`Project`]'s `Policy`, so what shows is the current state and
/// not a snapshot someone remembered to refresh.
///
/// A form already open is raised rather than duplicated: pressing ⚙ twice should not put two copies of
/// one screen on the desk.
pub fn press_policy(window: &adw::ApplicationWindow) {
    // F0.7's door into the form also closes the staleness gap: if this window has never derived from
    // its User Context, derive now so the first thing the form paints is what the model said rather
    // than a default nobody chose. The tracker makes this free on every later open.
    let context = user_context(window);
    if POLICY_TRACKERS
        .with(|trackers| trackers.borrow().last().cloned())
        .is_some_and(|tracker| tracker.borrow().needs_derive(&context))
    {
        derive_policy(window);
    }
    if let Some(existing) = POLICY_FORMS.with(|forms| forms.borrow().last().cloned()) {
        // Rebuild the rows in place rather than only raising the old window: the policy may have changed
        // since this form was opened, and a form that shows last-opened values is exactly the staleness
        // S5 exists to avoid.
        refresh_policy_form(&existing);
        existing.present();
        return;
    }
    let Some(session) = SESSION.with(|slots| slots.borrow().last().cloned()) else {
        return;
    };
    // Read at open time, through the same handle Open/Rescan write, so the form cannot drift from the
    // project it describes.
    let policy = session.borrow().policy.clone();
    let form = policy_form::build(Some(window.upcast_ref()), &policy);
    POLICY_FORMS.with(|forms| forms.borrow_mut().push(form.clone()));
    form.present();
}

/// Whether this window has a policy form showing.
pub fn policy_form_open(_window: &adw::ApplicationWindow) -> bool {
    POLICY_FORMS.with(|forms| !forms.borrow().is_empty())
}

/// The open policy form, newest first — the same handle `press_policy` raised. Published so a test can
/// read the labels inside it without walking GTK's application window list (which needs a running
/// `GApplication` registration this harness does not have).
pub fn open_policy_form() -> Option<adw::Window> {
    POLICY_FORMS.with(|forms| forms.borrow().last().cloned())
}

/// Re-read the live policy into an already-open form's labels, in place.
///
/// The row widgets are found by their stable names rather than kept in a struct, so a rebuild needs no
/// bookkeeping and cannot hold a stale handle — the same reason the buttons are looked up per press.
/// the form is showing. Public so `policy_form`'s Re-derive button repaints through exactly the same
/// path a policy change already uses.
pub fn refresh_policy_form(form: &adw::Window) {
    let Some(session) = SESSION.with(|slots| slots.borrow().last().cloned()) else {
        return;
    };
    let policy = session.borrow().policy.clone();
    for entry in policy::derived_fields() {
        if let Some(label) =
            find_widget_by_name(form.upcast_ref(), &format!("policy-value-{}", entry.field))
        {
            if let Ok(label) = label.downcast::<gtk::Label>() {
                label.set_text(&policy_form::value_word(&policy, entry.field));
            }
        }
        if let Some(label) =
            find_widget_by_name(form.upcast_ref(), &format!("policy-source-{}", entry.field))
        {
            if let Ok(label) = label.downcast::<gtk::Label>() {
                label.set_text(policy_form::origin_word(policy_form::field_origin(&policy, entry.field)));
            }
        }
        if let Some(label) =
            find_widget_by_name(form.upcast_ref(), &format!("policy-reason-{}", entry.field))
        {
            if let Ok(label) = label.downcast::<gtk::Label>() {
                label.set_text(&policy_form::reason_word(&policy, entry.field));
            }
        }
    }
}

/// The live policy this window's session holds — the same `Rc` Open/Rescan write, read through the
/// newest slot like [`session_sources`] does. Published so a test can set a field's origin to `User`
/// and watch what the form then prints, rather than poking the thread-local from outside.
pub fn session_policy(window: &adw::ApplicationWindow) -> crate::project::Policy {
    let _ = window;
    SESSION
        .with(|slots| slots.borrow().last().cloned())
        .map(|project| project.borrow().policy.clone())
        .unwrap_or_default()
}

/// Put a value, source and reason on one policy field of this window's live session.
///
/// A test seam for S4/S5: it goes through `policy::apply`, so what lands is exactly what the derivation
/// path would have written, and the caller only overrides the origin afterwards to stand in for "a
/// person set this by hand".
pub fn set_session_policy_field(
    window: &adw::ApplicationWindow,
    field: &str,
    value: &str,
    because: &str,
    origin: Origin,
) -> policy::Applied {
    let _ = window;
    let Some(project) = SESSION.with(|slots| slots.borrow().last().cloned()) else {
        return policy::Applied::Refused("this window holds no session".to_string());
    };
    let proposal = policy::Proposal {
        field: field.to_string(),
        value: value.to_string(),
        because: because.to_string(),
    };
    let outcome = policy::apply(&mut project.borrow_mut().policy, &proposal);
    // Only after a Set does the origin override make sense: refusing first and stamping `User` onto an
    // untouched field would show the form something that never happened.
    if outcome == policy::Applied::Set {
        stamp_origin(&mut project.borrow_mut().policy, field, origin);
    }
    outcome
}

/// Move one field's `source` to `origin` without touching its value or reason.
fn stamp_origin(policy: &mut crate::project::Policy, field: &str, origin: Origin) {
    match field {
        "markingPass" => policy.marking_pass.origin = origin,
        "cutMode" => policy.cut_mode.origin = origin,
        "captionsPass" => policy.captions_pass.origin = origin,
        "speedPass" => policy.speed_pass.origin = origin,
        "decorationsPass" => policy.decorations_pass.origin = origin,
        _ => {}
    }
}

/// Reset the live session's policy to the §2 defaults and repaint every open form.
///
/// The form asks for this; it does not decide anything. `policy::reset_to_defaults` owns the rule,
/// this only reaches the newest session (the one Save writes) and refreshes what is showing, so a
/// reset cannot leave a stale clone behind that overwrites the reset on the next save.
pub fn reset_policy_to_defaults() {
    let Some(session) = SESSION.with(|slots| slots.borrow().last().cloned()) else {
        return;
    };
    policy::reset_to_defaults(&mut session.borrow_mut().policy);
    POLICY_FORMS.with(|forms| {
        for form in forms.borrow().iter() {
            refresh_policy_form(form);
        }
    });
}

/// Drop one form from the open-forms slot, so `policy_form_open` stops reporting a window that is on
/// its way out. Called from the form's own close path: the thread-local would otherwise keep a closed
/// window alive and make `press_policy` raise something invisible.
pub fn forget_policy_form(form: &adw::Window) {
    // The index is found with the list only briefly borrowed, and removal goes through a queued idle
    // rather than happening inside the close signal: `close()` runs the delete path synchronously and
    // anything that reads this slot from there (a repaint, a re-entrant press) would meet a borrow held
    // across the signal and panic "RefCell already borrowed".
    let index = POLICY_FORMS.with(|forms| {
        forms
            .borrow()
            .iter()
            .position(|open| std::ptr::eq(open.as_ptr(), form.as_ptr()))
    });
    let Some(index) = index else { return };
    glib::idle_add_local(move || {
        POLICY_FORMS.with(|forms| {
            let mut forms = forms.borrow_mut();
            if index < forms.len() {
                forms.remove(index);
            }
        });
        glib::ControlFlow::Break
    });
}

thread_local! {
    /// Who answers a derivation's request, newest last — the same newest-slot rule as
    /// [`WINDOW_SHELLS`]. Injected rather than hard-wired to the HTTP layer so the whole of F0.7 S2-S4
    /// is testable with no server: a check installs a stub here and drives the real ⚙.
    static POLICY_SENDERS: std::cell::RefCell<
        Vec<Rc<std::cell::RefCell<dyn FnMut(&policy::DerivationRequest, &[String]) -> Vec<policy::Proposal>>>>,
    > = const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    /// Per-window record of which User Context the policy was derived from (F0.7). Newest last.
    static POLICY_TRACKERS: std::cell::RefCell<Vec<Rc<RefCell<policy::Tracker>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    /// The pending debounced derivation, so a burst of keystrokes replaces one timer instead of
    /// queueing one per letter. `policy::DEBOUNCE_MS` is the wait; without this slot every keystroke
    /// would fire its own request and the last few would race.
    static POLICY_DEBOUNCE: std::cell::RefCell<Vec<glib::SourceId>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Answer a derivation with nothing but a refusal naming why.
///
/// §00's "failure is specific and local": with no model answering, the policy keeps exactly what it
/// had and the status line says who was not there, rather than pretending a derivation happened or
/// silently falling back to defaults over a context that asked for something else.
fn unanswered_model(
    _req: &policy::DerivationRequest,
    refusals: &[String],
) -> Vec<policy::Proposal> {
    if !refusals.is_empty() {
        // A second round with nobody listening: say nothing new, so the loop's cap ends it quickly.
        return Vec::new();
    }
    // An unknown field name is refused by `apply`, which is how "nothing was set" reaches the caller
    // as a sentence rather than as a silent zero.
    vec![policy::Proposal {
        field: "llm".to_string(),
        value: "unreachable".to_string(),
        because: "the editing-policy derivation could not reach the model".to_string(),
    }]
}

/// The newest window this process built. The run-bar handler is wired before the handle is in scope at
/// the call site, so it reads the window back from [`POLICY_WATCH_WINDOW`] — the same newest-slot rule
/// every other per-window piece here uses.
pub fn main_window() -> adw::ApplicationWindow {
    POLICY_WATCH_WINDOW
        .with(|watch| watch.borrow().clone())
        .expect("a policy request happens inside a built window")
}

thread_local! {
    /// The window the bench's User Context editor belongs to, newest last. The bench is built before
    /// the window handle reaches `wire_*`, so its keystroke hook reads the window back from here —
    /// the same newest-slot rule as [`POLICY_TRACKERS`].
    static POLICY_WATCH_WINDOW: std::cell::RefCell<Option<adw::ApplicationWindow>> =
        const { std::cell::RefCell::new(None) };
}

/// Install the answer used by later derivations in this process (a test seam). Newest wins, so a
/// check can put its own model in front of whatever `build_window` installed.
pub fn set_policy_sender_for_tests(
    sender: impl FnMut(&policy::DerivationRequest, &[String]) -> Vec<policy::Proposal> + 'static,
) {
    POLICY_SENDERS.with(|senders| {
        senders
            .borrow_mut()
            .push(Rc::new(std::cell::RefCell::new(sender)))
    });
}

/// The window's User Context text, read through the session the bench writes.
fn user_context(window: &adw::ApplicationWindow) -> String {
    let _ = window;
    SESSION
        .with(|slots| slots.borrow().last().cloned())
        .map(|session| session.borrow().context.clone())
        .unwrap_or_default()
}

/// The window's User Context text, as the bench and `derive_policy` read it.
pub fn set_session_context(window: &adw::ApplicationWindow, text: &str) {
    let _ = window;
    if let Some(session) = SESSION.with(|slots| slots.borrow().last().cloned()) {
        session.borrow_mut().context = text.to_string();
    }
}

/// Whether this window's policy still matches its User Context (F0.7's "changed" question, published
/// so a widget test asserts the same fact the ▶ path acted on).
pub fn policy_is_current(window: &adw::ApplicationWindow, context: &str) -> bool {
    let _ = window;
    POLICY_TRACKERS
        .with(|trackers| trackers.borrow().last().cloned())
        .is_some_and(|tracker| !tracker.borrow().needs_derive(context))
}

/// Derive this window's editing policy from its User Context now (F0.7 S1-S4), and drop the marks a
/// changed `markingPass` invalidates.
///
/// Thin on purpose: every rule is `policy::derive`'s. This reads the two things the rule needs (the
/// context, the live policy), sends them through the installed answer, records the result on the
/// tracker, and lets the caller print `Derived::status()` where it belongs.
pub fn derive_policy(window: &adw::ApplicationWindow) -> policy::Derived {
    let context = user_context(window);
    let Some(session) = SESSION.with(|slots| slots.borrow().last().cloned()) else {
        return policy::Derived {
            used_defaults: true,
            ..policy::Derived::default()
        };
    };
    let before = session.borrow().policy.clone();
    let mut derived = {
        let mut policy = session.borrow().policy.clone();
        let out = match POLICY_SENDERS.with(|senders| senders.borrow().last().cloned()) {
            Some(sender) => policy::derive(&mut policy, &context, &mut *sender.borrow_mut()),
            None => policy::derive(&mut policy, &context, &mut unanswered_model),
        };
        session.borrow_mut().policy = policy;
        out
    };
    // Record even a defaults-only derivation: an empty context IS an answer, and remembering it is
    // what stops every ▶ from re-asking about a context nobody intends to fill.
    if let Some(tracker) = POLICY_TRACKERS.with(|trackers| trackers.borrow().last().cloned()) {
        tracker.borrow_mut().mark_derived(&context);
    }
    // §03: a changed markingPass invalidates the marks, so Prepare re-runs its marking pass instead
    // of resuming off output made under a different pass.
    let dropped = policy::invalidated_marks(&before, &session.borrow().policy);
    if !dropped.is_empty() {
        let dir = startup::session_dir(&std::env::current_dir().unwrap_or_default());
        if let Ok(tree) = layout::Tree::new(&dir) {
            for name in dropped {
                let path = if name == "final.txt" {
                    tree.final_txt()
                } else {
                    tree.retakes_tsv()
                };
                let _ = std::fs::remove_file(path);
            }
        }
    }
    // Nothing refused and nothing landed but the model was reached: say so, rather than reporting a
    // derivation of zero fields as if it were news.
    let _ = &mut derived;
    derived
}

/// Queue one debounced derivation (F0.7's "debounce"). Called from the User Context editor on every
/// keystroke; a pending one is cancelled first, so typing a paragraph costs one request.
fn schedule_policy_derive(window: &adw::ApplicationWindow) {
    let held = window.clone();
    POLICY_DEBOUNCE.with(|pending| {
        if let Some(id) = pending.borrow_mut().pop() {
            id.remove();
        }
        let id = glib::timeout_add_local(
            std::time::Duration::from_millis(policy::DEBOUNCE_MS),
            move || {
                POLICY_DEBOUNCE.with(|p| p.borrow_mut().clear());
                let derived = derive_policy(&held);
                if let Some(status) = find_widget_by_name(held.upcast_ref(), "status-line")
                    .and_then(|w| w.downcast::<gtk::Label>().ok())
                {
                    status.set_text(&derived.status());
                }
                POLICY_FORMS.with(|forms| {
                    for form in forms.borrow().iter() {
                        refresh_policy_form(form);
                    }
                });
                glib::ControlFlow::Break
            },
        );
        pending.borrow_mut().push(id);
    });
}

/// test between rounds: the thread-local outlives a window, and a leftover form would make
/// `press_policy`'s raise-instead-of-rebuild branch fire when the check wants a fresh build.
pub fn close_policy_forms() {
    // Drained BEFORE any `close()` runs: closing now fires the form's own `close-request`, which calls
    // `forget_policy_form`, and mutating this list while it is mutably borrowed would panic.
    let open: Vec<adw::Window> = POLICY_FORMS.with(|forms| forms.borrow_mut().drain(..).collect());
    for form in open {
        form.close();
    }
}

/// F0.7: the run bar's policy gear, wired after the widgets exist. Its sibling in the header is each
/// tab's ⓘ; both call the same [`press_policy`], so the form is reachable from the ⚙ AND from ⓘ with
/// one implementation behind them.
fn wire_policy(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let window = window.clone();
    button.connect_clicked(move |_| press_policy(&window));
}

fn wire_rescan(
    button: &gtk::Button,
    status: &gtk::Label,
    session: &Rc<RefCell<Project>>,
    shell: &Rc<RefCell<Shell>>,
    switcher: &adw::ViewSwitcher,
    inputs_readout: &gtk::Label,
    outputs_folder: &gtk::Button,
    outputs_readout: &gtk::Label,
) {
    let status = status.clone();
    // The window is handed an immutable `&Project` and holds no live project yet, so the scan works
    // on the private copy shared with Add sources. F0.9's live project state replaces this; until
    // then it is what keeps the pruned list for whatever flow reads the session next. Which folder is
    // open is the same stand-in New and Save use, so all three flows agree.
    let root = std::env::current_dir().unwrap_or_default();
    let project = session.clone();
    let shell = shell.clone();
    let switcher = switcher.clone();
    let inputs_readout = inputs_readout.clone();
    let outputs_folder = outputs_folder.clone();
    let outputs_readout = outputs_readout.clone();
    button.connect_clicked(move |_| {
        let dir = startup::session_dir(&root);
        let tree = layout::Tree::new(&dir).ok();
        let found = rescan::rescan(&mut project.borrow_mut(), &root, &dir, tree.as_ref());
        for line in &found.logs {
            log_line(line);
        }
        // §1's third case: a page whose prerequisites vanished while it was open goes back to Prepare
        // in silence. The scan just rewrote `project`, so the lock is asked of THAT list — asking the
        // immutable copy `build_window` was handed would answer about sources that are already gone.
        let scanned = project.borrow().clone();
        if shell::lock(shell.borrow().page, &scanned).is_some() {
            let (tree, cut, mut narration) = session_reads();
            // The scan rewrote the folder, so the window's held lines are re-seeded from it here:
            // a refit over narration that no longer belongs to this project would write someone
            // else's words back over the ones just scanned.
            if let Some(held) = HELD_NARRATION.with(|held| held.borrow().last().cloned()) {
                *held.borrow_mut() = narration.clone();
            }
            let page = shell.borrow().page;
            shell.borrow_mut().switch(
                page,
                Move::Vanished,
                &scanned,
                tree.as_ref(),
                &cut.segs,
                &mut narration.entries,
            );
            paint_tabs(&switcher, &shell.borrow(), &scanned);
            paint_readouts(
                &shell.borrow(),
                &scanned,
                tree.as_ref(),
                &cut,
                &narration,
                &inputs_readout,
                &outputs_folder,
                &outputs_readout,
            );
        }
        status.set_text(found.status);
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

    let status = status.clone();
    let session = session.clone();
    let copy_into = copy_into.clone();
    // The same stand-in New, Save and Rescan use, so all four flows agree on which project is open.
    let root = std::env::current_dir().unwrap_or_default();
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
            // The list drew its rows once at build time, so the new ones are appended here rather
            // than the list being rebuilt — rows already on screen keep their own state (S3).
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
                    list.append(&source_row(&session, &status, index));
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
    static WINDOW_BAR: std::cell::RefCell<Vec<Rc<std::cell::RefCell<run::RunBar>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    /// The F0.5 queue `build_window` created, published for the same reason as [`WINDOW_BAR`]: so
    /// the seam in [`add_files`] can drive the bar this window actually shows. A copy is a run of
    /// its own (F0.12 S0), so it reports through the bookkeeping rather than painting its own bar.
    static WINDOW_QUEUE: std::cell::RefCell<Vec<Rc<std::cell::RefCell<runqueue::Queue>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    /// §1's badge **6** — the header ⓘ, newest last, the same newest-slot arrangement as
    /// [`POLICY_FORMS`]. Held so a switch can repaint its tooltip from `Shell::help_page` without
    /// searching the widget tree, and so a test reads the tooltip of the button that is on screen.
    static INFO_BUTTON: std::cell::RefCell<Vec<gtk::Button>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    /// The live `Shell` of each window, newest last — the newest-slot rule every other window-level
    /// piece here uses. Held so the S3 pending-write seam can mark a write owed on the shell that is
    /// actually showing, instead of a copy the caller happens to have.
    static WINDOW_SHELLS: std::cell::RefCell<Vec<Rc<RefCell<Shell>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    /// The narration this window holds, newest last — the same newest-slot rule as [`WINDOW_SHELLS`].
    /// S5's refit and S3's flush are two different switches over one set of lines, so the lines live
    /// here rather than in whatever a single switch happened to read from disk.
    static HELD_NARRATION: std::cell::RefCell<Vec<Rc<RefCell<Narration>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    /// The policy form this window has open (F0.7 S5), newest last. Held so a second press on ⚙ or ⓘ
/// raises the form that is already there instead of stacking a third copy of the same screen, and so
/// `policy_form_open` can answer whether one is showing without searching the widget tree.
static POLICY_FORMS: std::cell::RefCell<Vec<adw::Window>> = const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    /// The live project handle `build_window` created, published so a test can read what Open put in
    /// place. One window per test binary here, so one slot is enough; the page widgets keep their own
    /// clone of the same `Rc`, which is why replacing the contents is visible everywhere.
    static SESSION: std::cell::RefCell<Vec<Rc<std::cell::RefCell<Project>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The Open button's tooltip, §1's wording for badge **2**.
const OPEN_TIP: &str = "Load a project \u{2014} sources, prompts and settings";

/// F2.1's own ▶ on the Cut page: the label and the tooltip §F2.1 S2 gives ("play from the red line ·
/// every second plays, cuts and all"). Kept apart so the widget test can pin the wording against the
/// spec rather than against whatever was typed into the button.
const RECORD_PLAY_LABEL: &str = "\u{25b6} Play the recording";

/// F2.2's label. The glyph pair is the button's whole identity in §A (▶ vs ▶✂), so it leads the
/// words rather than sitting alone as an icon a person has to memorise.
const PLAY_CUT_LABEL: &str = "\u{25b6}\u{2702} Play the cut";

/// F2.3's label — the third of the transport group, spelled with two scissors so it cannot be
/// mistaken for ▶✂ at a glance.
const REVIEW_CUTS_LABEL: &str = "\u{25b6}\u{2702}\u{2702} Review every cut";

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
const RECORD_PLAY_TIP: &str =
    "Play the recording from the red line \u{2014} every second of it, cuts and all";

/// The reason ▶ had nothing to play. Spec silent on a session with no filmed stretch; §0 asks for a
/// named, local reason rather than a silent button.
pub const NO_RECORDING_STATUS: &str = "nothing was filmed \u{2014} there is no recording to play";

/// One window's preview state, pushed beside [`SESSION`] so each window owns its own and the newest
/// one wins — the same arrangement `session_sources` reads.
thread_local! {
    static PREVIEW_PLAYERS: std::cell::RefCell<Vec<Rc<std::cell::RefCell<Player>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The app's ONE preview volume (F2.5 S6), held per window beside [`PREVIEW_PLAYERS`] with the same
/// newest-slot rule. It is a `PreviewVolume` and not a bare number because the type is what keeps the
/// slider's 0..100 and the property's 0..1 from drifting apart, and because the tooltip promises every
/// slider shows the same setting — two stored copies would be two answers.
thread_local! {
    static PREVIEW_VOLUMES: std::cell::RefCell<Vec<Rc<std::cell::RefCell<cut_hear::PreviewVolume>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// This window's selection band (F2.6), newest-slot rule as everywhere else here. `None` is no band:
/// the page then reads the empty marks and every verb that needs a selection is off.
thread_local! {
    static SELECTED: std::cell::RefCell<Vec<Rc<std::cell::RefCell<Option<cut_select::Selection>>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The surface the current band was drawn on, kept beside [`SELECTED`] so a resize or a move stays on
/// the ground it started from. S1 scopes a band by what it was drawn on, and a later drag on a lane
/// must not silently turn a footage selection into a sound one (or the reverse) mid-adjust.
thread_local! {
    static SELECT_SURFACES: std::cell::RefCell<Vec<Rc<std::cell::RefCell<Option<cut_select::Surface>>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// F2.6 S1: a left-drag from `from` to `to` on `surface` selected what it was drawn on. The rule is
/// [`cut_select::draw`]'s alone — including its refusal for the effects lane, which answers `None` and
/// leaves this window with no band rather than a half-made one. Returns the same answer it stored.
pub fn draw_selection(
    window: &adw::ApplicationWindow,
    surface: cut_select::Surface,
    recording: Option<&str>,
    from: f64,
    to: f64,
) -> Option<cut_select::Selection> {
    let band = cut_select::draw(surface, recording, from, to);
    SELECTED.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            *slot.borrow_mut() = band.clone();
        }
    });
    SELECT_SURFACES.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            *slot.borrow_mut() = Some(surface);
        }
    });
    refresh_selection_readout(window);
    band
}

/// F2.6: this window's band, or `None`. Read by the readout, the verbs and every later flow that acts
/// on a selection — all of them through this one value, so none can hold a stale copy.
pub fn selection(window: &adw::ApplicationWindow) -> Option<cut_select::Selection> {
    let _ = window;
    SELECTED.with(|slots| slots.borrow().last().and_then(|s| s.borrow().clone()))
}

/// The surface the current band came from, defaulting to the ruler when nothing has been drawn yet.
pub fn selection_surface(window: &adw::ApplicationWindow) -> cut_select::Surface {
    let _ = window;
    SELECT_SURFACES
        .with(|slots| slots.borrow().last().and_then(|s| *s.borrow()))
        .unwrap_or(cut_select::Surface::Ruler)
}

/// F2.6 S2: the cross. Routes through [`cut_select::clear`] and returns whether there was anything to
/// clear, which is also what turns the button insensitive again — a press on nothing reports nothing.
pub fn clear_selection(window: &adw::ApplicationWindow) -> bool {
    let had = selection(window).is_some();
    SELECTED.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            *slot.borrow_mut() = cut_select::clear(selection(window));
        }
    });
    refresh_selection_readout(window);
    had
}

/// F2.6 S2: drag one part of the band to `to` (session seconds) at `pps` pixels-per-second. The snap
/// marks are rebuilt each call from this window's cut, its lanes and the playhead, because the borders a
/// handle may snap to change as the cut changes — a cached list would go stale against an edit.
///
/// `Part::Outside` moves nothing: a press clear of the band starts a new selection instead, which is
/// S1's job, not a nudge.
pub fn nudge_selection(window: &adw::ApplicationWindow, part: cut_select::Part, to: f64, pps: f64) {
    let Some(band) = selection(window) else { return };
    let (cut_, lanes) = selection_snap_inputs(window);
    let marks = cut_select::snap_marks(&cut_, &lanes, preview_playhead(window));
    let moved = match part {
        cut_select::Part::Start => cut_select::resize(&band, false, to, &marks, pps),
        cut_select::Part::End => cut_select::resize(&band, true, to, &marks, pps),
        cut_select::Part::Middle => cut_select::move_band(&band, to, &marks, pps),
        cut_select::Part::Outside => return,
    };
    SELECTED.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            *slot.borrow_mut() = Some(moved);
        }
    });
    refresh_selection_readout(window);
}

/// F2.6 S4: the verbs' state for the band there is now — Add/Split/Remove greyed for a sound selection,
/// Copy and Insert re-aimed at it. The ＋ Add / | Split / － Remove buttons F2.7 added read THIS answer
/// (`refresh_verb_buttons`), so the page's greying and the logic tests' assertions cannot drift apart;
/// ⧉ Copy / Insert are still not drawn (F2.9/F0.6 rounds).
pub fn selection_verbs(window: &adw::ApplicationWindow) -> cut_select::Verbs {
    let (cut_, _) = selection_snap_inputs(window);
    // "is there a recording to cut": a cut with no segment has nothing to split, which is the only
    // thing S4 asks of the page beyond the band itself.
    let has_footage = !cut_.segs.is_empty();
    cut_select::verbs(selection(window).as_ref(), has_footage)
}

/// What this window can offer the snap-mark builder. The cut model round owns where a real `Cut` comes
/// from; until then the page's own review-cut slot stands in, so `nudge_selection` snaps against real
/// borders whenever one has been seeded and against nothing otherwise.
fn selection_snap_inputs(window: &adw::ApplicationWindow) -> (cut::Cut, Vec<cut::Lane>) {
    let cut_ = REVIEW_CUTS.with(|slots| {
        slots
            .borrow()
            .last()
            .map(|slot| slot.borrow().clone())
            .unwrap_or_default()
    });
    let lanes = cut_.lanes.clone();
    (cut_, lanes)
}

/// F2.7: which of the three buttons is live right now, as (add, split, remove).
///
/// | Split stays live with no band on purpose: F2.6's `verbs()` answers `false` for everything when
/// there is no selection, but F2.7 gives a band-less Split a real job — one border at the red line,
/// right half in hand — so greying it would take away the verb the spec says works there. An empty case
/// explains itself through the refusal sentence instead. With a band up, `verbs()` decides all three,
/// which is what greys them for a recording's sound.
pub fn verb_buttons_state(window: &adw::ApplicationWindow) -> (bool, bool, bool) {
    let verbs = selection_verbs(window);
    let split_live = match selection(window) {
        Some(_) => verbs.split,
        None => true,
    };
    (verbs.add, split_live, verbs.remove)
}

/// F2.7: set the three verb buttons from this window's live state, so a greyed button is always today's
/// answer and never a leftover from an earlier band. Runs from `refresh_selection_readout`, which every
/// draw, nudge and clear already calls — one refresh path for the whole selection row.
fn refresh_verb_buttons(window: &adw::ApplicationWindow) {
    let (add, split, remove) = verb_buttons_state(window);
    let sensitivity = [add, split, remove];
    for ((name, _, _), live) in cut_verbs::BUTTONS.iter().zip(sensitivity) {
        if let Some(button) = line_step_button(window, name) {
            button.set_sensitive(live);
        }
    }
    // ＋ Add wears its sound warning as a tooltip while a sound band makes it the wrong button.
    if let Some(button) = line_step_button(window, "add-button") {
        let tip = cut_verbs::add_tip_now(selection(window).as_ref())
            .unwrap_or_else(|| cut_verbs::BUTTONS[0].2.to_string());
        button.set_tooltip_text(Some(&tip));
    }
}

/// F2.7 S3: write the band into the readout and set the cross's sensitivity. The readout IS the band's
/// own marks (`cut_select::marks`), not a second copy, so the two cannot disagree with the handle being
/// dragged. The tent-of-a-second form comes from `preview::clock`, the Cut page's one clock face.
fn refresh_selection_readout(window: &adw::ApplicationWindow) {
    let band = selection(window);
    let text = match cut_select::marks(band.as_ref()) {
        Some((start, end)) => format!(
            "{} {} \u{2013} {}",
            cut_select::READOUT_PREFIX,
            preview::clock(Some(start)),
            preview::clock(Some(end))
        ),
        None => cut_select::READOUT_NONE.to_string(),
    };
    if let Some(label) = find_widget_by_name(window.upcast_ref(), "selection-readout") {
        if let Ok(label) = label.downcast::<gtk::Label>() {
            label.set_text(&text);
        }
    }
    if let Some(button) = find_widget_by_name(window.upcast_ref(), "clear-selection") {
        if let Ok(button) = button.downcast::<gtk::Button>() {
            button.set_sensitive(band.is_some());
        }
    }
    refresh_verb_buttons(window);
    // F2.9: the copy buttons ride the same refresh path, so a band drawn, nudged or cleared updates them
    // without a second place to remember.
    refresh_copy_buttons(window);
    // §05-cut#1-screen: and so do the four history buttons — one refresh path sets every greyed
    // control on the page, from the rule that owns each of them.
    refresh_history_buttons(window);
    // F2.10: and the camera rows, whose badges, plates and gutter switches read the same live cut on
    // every draw, nudge and clear — a badge showing a state the next press contradicts is a lie.
    refresh_camera_rows(window);
}


/// The playhead the snap marks are built around (F2.6 S2 lists the line among the snap targets).
fn preview_playhead(window: &adw::ApplicationWindow) -> f64 {
    live_player(window)
        .map(|player| player.borrow().playhead.unwrap_or(0.0))
        .unwrap_or(0.0)
}

/// This window's drag gesture, so a test can fire it the way GTK does. `None` on a page that drew none.
pub fn selection_gesture(window: &adw::ApplicationWindow) -> Option<gtk::GestureDrag> {
    let _ = window;
    SELECT_GESTURES.with(|cell| cell.borrow().clone())
}

thread_local! {
    static SELECT_GESTURES: std::cell::RefCell<Option<gtk::GestureDrag>> =
        const { std::cell::RefCell::new(None) };
}

/// F2.6 S1: what ground the current drag started on, or `None` when no press has been remembered yet.
pub fn pressed_surface(window: &adw::ApplicationWindow) -> Option<cut_select::Surface> {
    let _ = window;
    SELECT_SURFACES.with(|slots| slots.borrow().last().and_then(|s| s.borrow().clone()))
}

/// F2.6 S1: remember the surface a press landed on for this window's drag.
fn set_pressed_surface(surface: cut_select::Surface) {
    SELECT_SURFACES.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            *slot.borrow_mut() = Some(surface);
        }
    });
}

/// F2.6 S1: a press on the effects lane -- the only ground that refuses a selection. With an effect in
/// hand the press PUTS IT DOWN (that is what the lane is for); with nothing held it does nothing. Either
/// way NO band is drawn, so the existing selection stays exactly as it was. Callable without a widget so
/// the rule is testable on its own; the gesture handler just calls this when `surface_at` says `FxLane`.
pub fn press_fx_lane(window: &adw::ApplicationWindow) -> cut_select::FxLanePress {
    let _ = window;
    let answer = cut_select::fx_lane_press(held_effect().is_some());
    match answer {
        cut_select::FxLanePress::PutsHeldEffectDown => {
            set_held_effect(None);
            log_line("effect put down on the effects lane");
        }
        cut_select::FxLanePress::Nothing => {}
    }
    answer
}

/// The surface the placeholder track area draws today, and how many rows of it exist. Until F2.8/F2.10/
/// F2.11 own the real surfaces this answers the placeholder geometry (`cut_select::PLACEHOLDER_BANDS`)
/// with one camera row -- the same single row the page shows -- so every band is reachable by y.
fn placeholder_surface_bands() -> (cut_select::SurfaceBands, usize) {
    (cut_select::PLACEHOLDER_BANDS, 1)
}

/// The recording name a wave-strip or lane selection is scoped to. `draw` takes `Option<&str>` and refuses
/// (returns `None`) for a sound scope with no name, so the page must name one: until F2.11 owns the lane
/// names this answers the placeholder camera the single drawn row stands for, matching
/// `timeline::kept_footage_recordings`' `cam<N>` naming rather than inventing a second scheme.
fn recording_for_surface(surface: cut_select::Surface) -> Option<&'static str> {
    match surface {
        cut_select::Surface::WaveStrip | cut_select::Surface::Lane => Some("cam0"),
        _ => None,
    }
}

/// This window's ✕ Clear selection button (F2.6 S2), looked up by its stable name.
pub fn clear_selection_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "clear-selection")?
        .downcast()
        .ok()
}

/// This window's Selection readout label (F2.6 S3).
pub fn selection_readout(window: &adw::ApplicationWindow) -> Option<gtk::Label> {
    find_widget_by_name(window.upcast_ref(), "selection-readout")?
        .downcast()
        .ok()
}

/// This window's drag surface (F2.6 S1).
pub fn select_surface(window: &adw::ApplicationWindow) -> Option<gtk::DrawingArea> {
    find_widget_by_name(window.upcast_ref(), "select-surface")?
        .downcast()
        .ok()
}

/// F2.5 S6: set this window's one preview volume from a slider's percent (0..100) and push the result
/// into the live player, so every preview of this window shares the one number. Returns the gain written
/// (`0..=1`), which is what the caller would read back — the clamp lives in [`cut_hear::clamp_volume`],
/// never here.
pub fn set_preview_volume(window: &adw::ApplicationWindow, percent: f64) -> f64 {
    let _ = window;
    let volume = PREVIEW_VOLUMES.with(|slots| {
        slots
            .borrow()
            .last()
            .cloned()
            .unwrap_or_else(|| Rc::new(std::cell::RefCell::new(cut_hear::PreviewVolume::new())))
    });
    volume.borrow_mut().set_percent(percent);
    let gain = volume.borrow().value();
    if let Some(player) = live_player(window) {
        player.borrow_mut().volume = Some(gain);
    }
    gain
}

/// F2.5 S6: the one number, as a gain — what every preview multiplies its sound by, and the value
/// [`cut_hear::mix_at`] is handed. A window that never showed a slider reads the default full.
pub fn preview_volume(window: &adw::ApplicationWindow) -> f64 {
    let _ = window;
    PREVIEW_VOLUMES.with(|slots| {
        slots
            .borrow()
            .last()
            .map(|v| v.borrow().value())
            .unwrap_or(cut_hear::VOLUME_DEFAULT)
    })
}

/// F2.5: ask the hearing rules what this tick means and store the answer on this window's player.
///
/// The page calls this instead of computing anything: `cut_hear::mix_at` decides which lanes start,
/// whether the footage's own sound is muted and how loud and fast to play, at this window's one
/// preview volume. Nothing here re-decides a part of that answer; it only lands `footage_muted` where a
/// later round's pipeline can read it.
pub fn apply_mix(
    window: &adw::ApplicationWindow,
    segs: &[cut::Seg],
    lanes: &[cut::Lane],
    fx: &[cut::Fx],
    t: f64,
) -> cut_hear::Mix {
    let mix = cut_hear::mix_at(segs, lanes, fx, t, preview_volume(window));
    if let Some(player) = live_player(window) {
        player.borrow_mut().footage_muted = mix.footage_muted;
    }
    mix
}

/// One window's cut, held beside [`PREVIEW_PLAYERS`] with the same newest-slot rule.
///
/// The Cut page has no live cut model yet — that arrives with its own round — so this is what stands in
/// for it: `build_window` seeds a default (empty) cut and every press reads it back. A caller that DOES
/// have a cut hands it over with [`seed_review_cut`], which is the seam the cut-model round plugs into
/// instead of re-plumbing the button.
thread_local! {
    static REVIEW_CUTS: std::cell::RefCell<Vec<Rc<std::cell::RefCell<cut::Cut>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// One window's red line and the writer that throttles its saves, newest slot winning like
/// [`PREVIEW_PLAYERS`]. The writer lives beside the position because the rate limit is a property of
/// this window's moving line, not of the file.
thread_local! {
    static LINE_STATES: std::cell::RefCell<Vec<Rc<std::cell::RefCell<(cut_line::LinePos, cut_line::LineWriter)>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The project root this window's line file sits under, newest slot winning like [`REVIEW_CUTS`]. Held
/// beside [`LINE_STATES`] because the position without its root is a number with nowhere to be saved:
/// `restore` needs it on open and `flush_line_on_close` needs it on close.
thread_local! {
    static LINE_ROOTS: std::cell::RefCell<Vec<std::path::PathBuf>> =
        const { std::cell::RefCell::new(Vec::new()) };
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
    LINE_STATES.with(|slots| {
        if let Some(slot) = slots.borrow().last() {
            slot.borrow_mut().0 = pos;
        }
    });
}

/// A user moved the line: store it and let [`cut_line::LineWriter`] decide whether that is worth a
/// save right now (at most one write a second, `cut_line::LINE_WRITE_MS`). Returns whether it wrote.
///
/// No project root reaches the page yet (the cut-model round owns it), so there is nowhere to write;
/// the throttle is still advanced so the rate rule holds the moment a root exists.
pub fn move_line_and_save(window: &adw::ApplicationWindow, pos: cut_line::LinePos, now_ms: u64) -> bool {
    let _ = window;
    LINE_STATES.with(|slots| {
        let Some(slot) = slots.borrow().last().cloned() else {
            return false;
        };
        slot.borrow_mut().0 = pos;
        let wrote = cut_line::may_write_line(slot.borrow().1.last_write_ms(), now_ms, false);
        if wrote {
            slot.borrow_mut().1.note_throttled(now_ms);
        }
        wrote
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

/// F2.4 S1 through the seam: a press on the tracks places the line, clears the selection and watches
/// a row exactly as [`cut_line::click_outcome`] says — the page reports pixels and modifiers, this
/// applies the rule and stores the result.
pub fn place_line_from_click(
    window: &adw::ApplicationWindow,
    on_picture: bool,
    on_scene_picture: bool,
    playing: bool,
    sources: bool,
    gutter: bool,
    at: f64,
    now_ms: u64,
) -> cut_line::ClickOutcome {
    let outcome = cut_line::click_outcome(at, on_picture, on_scene_picture, playing, sources, gutter);
    if outcome.line_moved {
        move_line_and_save(window, cut_line::LinePos { t: outcome.line_at }, now_ms);
    }
    // S1's other two effects land here too, because this is the one place the rule is applied. The
    // selection model exists (F2.6), so a track press really does clear it; and the watched row is
    // stored so the page knows which picture row a later ▶ should open on.
    if outcome.clears_selection {
        clear_selection(window);
    }
    set_watched_row(outcome.watches);
    // "the scene under the click taken in hand": only when `takes_scene` says the pointer was on that
    // scene's own picture, and then it is the scene under the NEW line, from this window's cut.
    if outcome.takes_scene {
        let under = cut_delete::scene_under(&newest_review_cut().segs, outcome.line_at).cloned();
        set_held_clip(under);
    }
    outcome
}

/// F2.4 S1b: what a first left press picks up at the 12 px reach (`// layout.lineReachPx`,
/// `cut_line::EDGE_REACH_PX`) — edge, then border, then the whole clip.
pub fn pick_with_reach(window: &adw::ApplicationWindow, edge_px: f64, inside: bool) -> cut_line::PressPick {
    let _ = window;
    cut_line::first_press_pick(edge_px, inside)
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
    if let Some(player) = live_player(window) {
        player.borrow_mut().transport.playing = false;
    }
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
pub fn space_toggles(window: &adw::ApplicationWindow, text_focus: bool) -> bool {
    let toggle = cut_line::space_toggles_preview(text_focus);
    if toggle && !text_focus {
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

/// Move the line by a signed number of frames and pause (F2.4 S2). The seam the buttons use; the
/// keyboard path goes through `step_the_line`, which first asks whether a hold should be nudged.
pub fn step_frames_of(window: &adw::ApplicationWindow, frames: i64) -> Step {
    let from = line_position(window).t;
    let to = cut_line::step_line(from, frames, DEFAULT_FPS);
    move_line_and_save(window, cut_line::LinePos { t: to }, now_ms());
    if let Some(player) = live_player(window) {
        player.borrow_mut().transport.playing = false;
    }
    Step::SteppedTo { t: to }
}

/// The frame rate the line steps at when no project has said otherwise. The spec gives no `P.*` row for
/// it; 25 fps is what the prototype's session runs at, and a wrong rate moves the line by a wrong but
/// still sub-second amount rather than breaking anything.
const DEFAULT_FPS: f64 = 25.0;

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
        let response = match key {
            gtk::gdk::Key::Left => arrow_steps(&win, shift, held, DEFAULT_FPS, now_ms()),
            gtk::gdk::Key::Right => arrow_steps(&win, shift, held, DEFAULT_FPS, now_ms()),
            gtk::gdk::Key::comma => step_the_line(&win, shift, held, DEFAULT_FPS, now_ms()),
            gtk::gdk::Key::period => step_the_line(&win, shift, held, DEFAULT_FPS, now_ms()),
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
fn live_player(window: &adw::ApplicationWindow) -> Option<Rc<RefCell<Player>>> {
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

/// F2.1: the Cut page's ▶ forwards to [`press_play_recording`], which forwards to
/// [`preview::press_recording`] and paints. The handler takes the window it was wired on, cloned into
/// the closure the way [`wire_settings`] does, so `build_window` needs nothing threaded through.
fn wire_play_recording(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        // The filmed stretches come from Prepare's output (`timeline::filmed_runs` over the
        // session's recordings), which belongs to the Cut page's own round — no `Recording` model
        // is reachable from here yet — so an empty list is what the window knows today, and the seam
        // takes real spans from any caller that does have them.
        let _ = press_play_recording(&window, &[]);
    });
}

/// F2.5 S6: hand the slider's number to [`set_preview_volume`] and nothing else. The widget decides no
/// part of the rule — it does not clamp, scale or remember — because the one volume is owned by
/// `cut_hear::PreviewVolume` and every preview reads that same value back.
fn wire_preview_volume(scale: &gtk::Scale, window: &adw::ApplicationWindow) {
    let window = window.clone();
    scale.connect_value_changed(move |slider| {
        set_preview_volume(&window, slider.value());
    });
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
///
/// The surface is the picture band's stand-in today (F2.8/F2.10 draw the real rows), so every click on
/// it counts as a picture-band click: `on_picture` is true and the gutter never answers through here.
/// Whether the pointer was on a scene's own PICTURE — the thing that decides if the hand closes — is
/// read from this window's cut at the clicked second and handed to the rule, which decides; nothing is
/// decided in this handler. Seconds come from `SELECT_SURFACE_PPS`, the same number the drag uses, so
/// the two gestures cannot disagree about where a pixel is in time.
fn wire_line_click_surface(window: &adw::ApplicationWindow) {
    let Some(area) = select_surface(window) else { return };
    let gesture = gtk::GestureClick::new();
    // Left button only: the right button belongs to trim/move (F2.8) and must not move the line.
    gesture.set_button(1);
    let win = window.clone();
    gesture.connect_released(move |_g, _n_press, x, _y| {
        let at = x / SELECT_SURFACE_PPS;
        let cut_ = newest_review_cut();
        // "the scene under the click" means its picture, so ask whether a kept scene covers this second.
        let on_scene_picture = cut_delete::scene_under(&cut_.segs, at).is_some();
        place_line_from_click(
            &win,
            true,
            on_scene_picture,
            false,
            true,
            false,
            at,
            now_ms(),
        );
        // S1b: the same press also picks up by the wider 12 px reach. The distance to the nearer clip
        // border is measured off the drawn geometry (`cut_trim::clip_boxes` at the same pps), so the
        // reach is the one the page shows rather than a number invented here.
        let boxes = cut_trim::clip_boxes(&cut_.segs, SELECT_SURFACE_PPS);
        let x_px = x;
        let nearest = boxes.iter().enumerate().map(|(i, b)| {
            let d = (x_px - b.x).min((x_px - (b.x + b.w)).abs());
            (d, i)
        }).min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        if let Some((edge_px, i)) = nearest {
            let inside = {
                let b = &boxes[i];
                x_px >= b.x && x_px <= b.x + b.w
            };
            let pick = cut_line::first_press_pick(edge_px, inside);
            let seg = cut_.segs.get(i).cloned();
            apply_pick(pick, seg);
        }
    });
    area.add_controller(gesture.clone());
    LINE_CLICKS.with(|cell| *cell.borrow_mut() = Some(gesture));
}

/// Attach the drag gesture to this window's `select-surface` and remember it. The handler forwards the
/// drag's end x (converted to seconds at the page's pixels-per-second) to [`draw_selection`] and
/// decides nothing about scope — that is `cut_select::draw`'s call from the surface up.
fn wire_select_surface(window: &adw::ApplicationWindow) {
    let Some(area) = select_surface(window) else { return };
    let gesture = gtk::GestureDrag::new();
    let win = window.clone();
    // F2.6 S1: the press decides the scope, once. Which band the y fell in (ruler / picture row / wave
    // strip / lane / selection band / effects lane) is answered by `cut_select::surface_at` -- a pure
    // function over the drawn bands -- and remembered, so the drag below never re-decides it.
    gesture.connect_drag_begin(move |_g, _sx, sy| {
        let (bands, rows) = placeholder_surface_bands();
        set_pressed_surface(cut_select::surface_at(sy, &bands, rows));
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
            press_fx_lane(&fx_win);
            return;
        }
        // Pixels to seconds at the page's current zoom; until the tracks exist the placeholder runs at
        // the open-at zoom so a drag still lands on real session seconds.
        draw_selection(
            &win,
            surface,
            recording_for_surface(surface),
            0.0,
            x / SELECT_SURFACE_PPS,
        );
    });
    area.add_controller(gesture.clone());
    SELECT_GESTURES.with(|cell| *cell.borrow_mut() = Some(gesture));
}

/// Wire the ✕ to [`clear_selection`] and nothing else: the rule for what clearing means (and that it
/// clears only the selection, never the cut) lives in `cut_select::clear`.
fn wire_clear_selection(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        clear_selection(&window);
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

/// F2.7 ⌦: a held effect, then a held clip, then the selection, then the scene under the line — in that
/// order, decided by [`cut_verbs::delete_verb`]. The two "held" values come from the seams below: the
/// real drag rounds (F2.8/F2.9/F0.6) will set them when a person actually picks something up, and until
/// then a test can set them here so the ORDER is checkable now rather than asserted later.
pub fn press_delete_key(window: &adw::ApplicationWindow) -> cut_verbs::Outcome {
    let segs = newest_review_cut().segs;
    let band = selection(window);
    let scene = held_scene_under_line(window);
    let outcome = cut_verbs::delete_verb(
        held_effect().as_ref(),
        held_clip().as_ref(),
        band.as_ref(),
        scene.as_ref(),
        &segs,
    );
    report_verb(window, &outcome);
    outcome
}

/// F2.7 ＋ Add: keep the band as scenes.
pub fn press_add(window: &adw::ApplicationWindow) -> cut_verbs::Outcome {
    let (cut_, _) = selection_snap_inputs(window);
    let band = selection(window);
    // No snap candidates of the page's own yet: word edges and silences arrive with the aligner round,
    // so an empty list means the ends stay where they were drawn rather than inventing points.
    let outcome = cut_verbs::add(band.as_ref(), &cut_.segs, &[]);
    report_verb(window, &outcome);
    outcome
}

/// F2.7 | Split: a border at each end of the band, or one at the red line when there is none.
pub fn press_split(window: &adw::ApplicationWindow) -> cut_verbs::Outcome {
    let (cut_, _) = selection_snap_inputs(window);
    let band = selection(window);
    let outcome = cut_verbs::split(band.as_ref(), preview_playhead(window), &cut_.segs);
    report_verb(window, &outcome);
    outcome
}

/// F2.7 － Remove: drop exactly the band.
pub fn press_remove(window: &adw::ApplicationWindow) -> cut_verbs::Outcome {
    let (cut_, _) = selection_snap_inputs(window);
    let band = selection(window);
    let outcome = cut_verbs::remove(band.as_ref(), &cut_.segs);
    report_verb(window, &outcome);
    outcome
}

/// Print the verb's sentence and honour what the answer says about the band. A refusal changes nothing at
/// all — no status-free silence, and no edit either, which is the point of refusing.
fn report_verb(window: &adw::ApplicationWindow, outcome: &cut_verbs::Outcome) {
    if let Some(status_line) = find_status(window.upcast_ref()) {
        status_line.set_text(outcome.status());
    }
    match outcome {
        cut_verbs::Outcome::Applied {
            segs,
            keeps_selection,
            into_hand,
            ..
        } => {
            // Only a verb that produced segments rewrites the cut; ⌦ on a held card reports through the
            // status and leaves the list to the round that owns removal of inserts.
            if !segs.is_empty() {
                REVIEW_CUTS.with(|slots| {
                    if let Some(slot) = slots.borrow().last() {
                        slot.borrow_mut().segs = segs.clone();
                    }
                });
            }
            if !keeps_selection {
                clear_selection(window);
            }
            if *into_hand {
                VERB_HAND.with(|cell| *cell.borrow_mut() = selection(window));
            }
        }
        cut_verbs::Outcome::Refused(_) => {}
    }
}

/// The scene under the red line, or `None` where the cut keeps nothing — [`cut_delete::scene_under`]'s
/// answer, so ⌦ asks the same question every other ⌦-adjacent rule asks.
fn held_scene_under_line(window: &adw::ApplicationWindow) -> Option<cut::Seg> {
    let cut_ = newest_review_cut();
    cut_delete::scene_under(&cut_.segs, preview_playhead(window)).cloned()
}

/// What ⌦ would take first: the effect in hand. Set by the effects-lane rounds when a person picks one
/// up; a seam today so the delete order is testable before those pixels exist.
pub fn set_held_effect(effect: Option<cut::Fx>) {
    HELD_EFFECT.with(|cell| *cell.borrow_mut() = effect);
}

/// Read side of [`set_held_effect`].
pub fn held_effect() -> Option<cut::Fx> {
    HELD_EFFECT.with(|cell| cell.borrow().clone())
}

/// What ⌦ takes next: the clip in hand — `the only way to remove a spliced card` (§J rule 8). Same
/// seam arrangement as the held effect: real drags set it from F2.8/F2.9 onward.
pub fn set_held_clip(seg: Option<cut::Seg>) {
    HELD_CLIP.with(|cell| *cell.borrow_mut() = seg);
}

/// Read side of [`set_held_clip`].
pub fn held_clip() -> Option<cut::Seg> {
    HELD_CLIP.with(|cell| cell.borrow().clone())
}

/// F2.4 S1b: put a picked clip EDGE in the hand (the 12 px reach, `// layout.lineReachPx`).
pub fn set_held_edge(seg: Option<cut::Seg>) {
    HELD_EDGE.with(|cell| *cell.borrow_mut() = seg);
}

/// Read side of [`set_held_edge`].
pub fn held_edge() -> Option<cut::Seg> {
    HELD_EDGE.with(|cell| cell.borrow().clone())
}

/// F2.4 S2/S3: what the line is holding right now, in the order the spec steps them — edge first, then
/// the whole clip, then an effect. This is what a frame-step or arrow press asks; with the hardcoded
/// `None` that stood here before, a real key press could never take the nudge branch at all.
pub fn held_now() -> Option<cut_line::Held> {
    if held_edge().is_some() {
        Some(cut_line::Held::Edge)
    } else if held_clip().is_some() {
        Some(cut_line::Held::Clip)
    } else if held_effect().is_some() {
        Some(cut_line::Held::Effect)
    } else {
        None
    }
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
/// [`place_line_from_click`]; read by whatever opens a row on the watched one.
pub fn watched_row() -> Option<usize> {
    WATCHED_ROW.with(|cell| *cell.borrow())
}

/// Write side of [`watched_row`].
pub fn set_watched_row(row: Option<usize>) {
    WATCHED_ROW.with(|cell| *cell.borrow_mut() = row);
}

// --- F2.10: cameras and hearing, the seams the badges and rows call ------------------------------------

/// F2.10 S3: this window's watch, newest slot last like [`REVIEW_CUTS`] so one window's click cannot
/// answer another's ▶. The rule itself is [`cut_cam::Watch`]; this only remembers which one is live.
thread_local! {
    static CAM_WATCHES: std::cell::RefCell<Vec<Rc<RefCell<cut_cam::Watch>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// F2.10: a named `gtk::ToggleButton` in this window — the lens badge, a speaker badge or a gutter
/// switch. `line_step_button` only downcasts to a plain Button, so the toggle half needs its own finder;
/// it is the same walk, and the same stable names the page drew.
pub fn toggle_button(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::ToggleButton> {
    find_widget_by_name(window.upcast_ref(), name)?.downcast().ok()
}

/// F2.10 S3: this window's [`cut_cam::Watch`], created on first ask so a page that was never clicked
/// still has an empty one to hand to ▶.
pub fn camera_watch(window: &adw::ApplicationWindow) -> Rc<RefCell<cut_cam::Watch>> {
    let _ = window;
    CAM_WATCHES.with(|slots| {
        let mut slots = slots.borrow_mut();
        if let Some(latest) = slots.last() {
            return latest.clone();
        }
        let fresh = Rc::new(RefCell::new(cut_cam::Watch::default()));
        slots.push(fresh.clone());
        fresh
    })
}

/// The session second the red line stands on — the `t` every F2.10 sentence is about.
///
/// This is [`line_position`]'s number and nothing else: the line's own state, restored from
/// `cut/line.json` at open and moved by F2.4's steps. No second line position is invented here, so the
/// status says the same moment the ruler draws.
fn line_second(window: &adw::ApplicationWindow) -> f64 {
    line_position(window).t
}

/// The scene index the line stands in, as F2.10 reads it: a KEPT scene (footage, not an insert), since
/// both the lens badge and the speaker badge are asked about footage. `None` when the line sits over a
/// card or over nothing.
fn kept_scene_at_line(window: &adw::ApplicationWindow) -> Option<usize> {
    let cut_ = newest_review_cut();
    let at = line_second(window);
    cut_
        .segs
        .iter()
        .position(|seg| seg.ins.is_empty() && seg.s <= at && at < seg.e)
}

/// F2.10 S3 (`if the line is in a kept scene shown from another row, status says ONCE …`): the seam a
/// row's name plate forwards to. [`cut_cam::click_row`] owns the whole decision, including
/// whether the "the cut shows camera M here" sentence is owed; this prints what comes back.
pub fn press_watch_row(window: &adw::ApplicationWindow, row: i32) -> Option<String> {
    let cut_ = newest_review_cut();
    let at = line_second(window);
    let said = {
        let watch = camera_watch(window);
        let mut watch = watch.borrow_mut();
        cut_cam::click_row(&mut watch, row, &cut_.segs, Some(at))
    };
    // Keep F2.4's slot in step: whatever the picture band click sets, a row click means the same thing
    // about which row this window is looking at.
    set_watched_row(Some(row.max(0) as usize));
    if let Some(line) = &said {
        if let Some(status) = find_status(window.upcast_ref()) {
            status.set_text(line);
        }
    }
    // The dashed outline moves with the watch, so the rows are rebuilt rather than merely queued.
    refresh_camera_rows(window);
    said
}

// --- F2.10: drawing the rows --------------------------------------------------------------------------

/// The 🔍 glyph, spelled once so the badge and its tooltip cannot drift apart. (The headless container
/// has no icon theme, so — as with F2.5's "preview volume" word for a speaker icon — the glyph is drawn
/// as text rather than looked up.)
pub const LENS_GLYPH: &str = "\u{1f50d}";
/// The 🔈 glyph for a lane's speaker badge, spelled once for the same reason.
pub const SPEAKER_GLYPH: &str = "\u{1f508}";

/// Is a lane heard in the scene under the line? An unlisted lane is heard (`spec/inventory/cut.md` §A
/// rule 10: `quiet` lists the silent ones), and with no kept scene under the line there is nothing to
/// be silent in, so the badge reads heard and greyed rather than lying about a scene that isn't there.
fn lane_heard_in_line_scene(cut_: &Cut, lane: &str, scene: Option<usize>) -> bool {
    match scene {
        Some(index) => cut_
            .segs
            .get(index)
            .map(|seg| seg.hears(lane))
            .unwrap_or(true),
        None => true,
    }
}

/// Rebuild `camera-rows` from this window's own cut. One row per coloured row of the timeline
/// (`timeline::row_count`, which is the greedy colouring's answer plus `Cut::nrows`' floor — no second
/// colouring here), each holding, in this order: the clickable name plate (F2.10 S3), the lens badge
/// (S1), one speaker badge per lane (S2) and that lane's gutter switch (S2). Every widget takes its
/// state from the same functions the logic tests call, and every press goes through the same
/// `press_*` seam a test fires, so the drawing cannot disagree with the rule.
///
/// Called from `refresh_selection_readout` (which every draw, nudge and clear already calls) and after
/// each of this item's presses. Rebuilding rather than patching keeps the watched row's dashed outline
/// STATE: it is added when the watch says so, never painted on luck.
pub fn refresh_camera_rows(window: &adw::ApplicationWindow) {
    let Some(rows_box) = camera_rows_box(window) else {
        return;
    };
    let cut_ = newest_review_cut();
    let recordings = crate::timeline::kept_footage_recordings(&cut_);
    let placed = crate::timeline::rows_for(&recordings, &cut_);
    let total = crate::timeline::row_count(&recordings, &cut_);
    // The rows this window has already drawn, keyed by their widget name. A row is built ONCE and then
    // only re-styled: `find_widget_by_name` matches on GTK's *widget name*, which every control here
    // carries (`set_widget_name`), so a rebuild that appended a second `watch-row-1` would leave the
    // first one — the widget a test actually finds — frozen at its old label. Growing the list to the
    // row count and never shrinking it also keeps §B's emptied bottom row on screen until its ✕ lands
    // with F2.11, rather than dropping a row the user can still click.
    let lanes: Vec<String> = cut_.lanes.iter().map(|lane| lane.name.clone()).collect();
    let scene = kept_scene_at_line(window);
    let at = line_second(window);
    let watched = watched_row();

    let drawn = rows_box.observe_children().n_items() as usize;
    // The guard is set for the whole pass below: every `set_active` in it is a STATE PUSH, not a press.
    CAMERA_REFRESH.with(|cell| cell.set(true));
    // The dashed outline the spec's **4** asks for has to be painted, not merely named: GTK draws no
    // class of its own, so a `watched-row` class with no CSS behind it is invisible. A frame around the
    // row box is what reads as "this row is the one the preview shows", and it is drawn here rather
    // than in a stylesheet because the crate installs no CSS provider at all (nothing else in `src/`
    // calls `CssProvider`), and adding a display-wide provider would restyle every other page.
    let watched_box = gtk::CssProvider::new();
    let _ = watched_box.load_from_data(".watched-row { border: 2px dashed #3a63c8; }");
    gtk::StyleContext::add_provider_for_display(
        &gtk::gdk::Display::default().expect("a display to style"),
        &watched_box,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    for index in 0..total.max(drawn) {
        let row = index as i32;
        // Reuse the row this window already drew rather than appending a second copy: `find_widget_by_name`
        // matches GTK's widget name, so a duplicate `camera-row-1` would leave the FIRST one — the
        // widget a test finds by that name — frozen at its old label while a hidden twin carried the
        // new state. Building once and re-styling is what keeps the on-screen plate and the rule the
        // same object.
        let holder = match find_widget_by_name(rows_box.upcast_ref(), &format!("camera-row-{}", index + 1))
            .and_then(|found| found.downcast::<gtk::Box>().ok())
        {
            Some(existing) => existing,
            None => {
                let fresh = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                fresh.set_widget_name(&format!("camera-row-{}", index + 1));
                rows_box.append(&fresh);
                fresh
            }
        };
        if watched == Some(index) {
            // **4** in `spec/img/05-rows.png`: the watched row is outlined, not filled. BOTH branches
            // run on every pass, so the outline can move OFF a row as well as onto one.
            holder.add_css_class("watched-row");
        } else {
            holder.remove_css_class("watched-row");
        }
        // The holder is reused so one row answers to one name; its CONTROLS, however, have to be made
        // fresh each pass (`gtk::Box::append` refuses a child that already has a parent, and the
        // labels, glyphs and toggle faces all carry this pass's state). Drain the old set first, then
        // build — which is why the block below appends without checking.
        while let Some(old) = holder.first_child() {
            holder.remove(&old);
        }

        // The name plate: what lies on this row, which part of that file it shows, and the shift
        // correction the page was opened with (**2**'s "−19.00 s"). A row with nothing on it still
        // gets its plate, named by `cut_cam::row_name`'s fallback, because an empty bottom row is a
        // real row until its ✕ (§B) and an unnamed row cannot be watched back.
        let occupant = recordings
            .iter()
            .zip(&placed)
            .find(|(_, placed_row)| **placed_row as i32 == row);
        let (plate, base) = match occupant {
            Some((rec, _)) => {
                let shift = cut_.shift.get(&rec.base).copied().unwrap_or(0.0);
                (cut_cam::name_plate(&rec.base, shift, 0.0), rec.base.clone())
            }
            None => (cut_cam::row_name(row, &[], &[]), String::new()),
        };
        let plate_button = gtk::Button::with_label(&plate);
        plate_button.set_widget_name(&format!("watch-row-{}", index + 1));
        plate_button.set_tooltip_text(Some(&format!(
            "watch {} in the preview \u{2014} \u{25b6} hands the preview back to the cut",
            if base.is_empty() {
                plate.as_str()
            } else {
                base.as_str()
            }
        )));
        let win = window.clone();
        plate_button.connect_clicked(move |_| {
            press_watch_row(&win, row);
        });
        holder.append(&plate_button);

        // S1's lens badge: lit when the scene under the line is shown FROM this row. Sensitive only
        // where the question has an answer — a card at the line owns no camera, so asking it which
        // lens it uses would write a number nothing reads.
        let lens = gtk::ToggleButton::new();
        lens.set_widget_name(&format!("lens-badge-{}", index + 1));
        lens.set_label(LENS_GLYPH);
        lens.set_sensitive(scene.is_some());
        lens.set_active(scene.map(|i| cut_.segs[i].cam).unwrap_or(-1) == row);
        lens.set_tooltip_text(Some(&format!(
            "{LENS_GLYPH} {}",
            match scene {
                Some(i) => format!(
                    "the scene at {} is shown from {} now",
                    cut_hear::scene_clock(cut_.segs[i].s),
                    if base.is_empty() {
                        plate.as_str()
                    } else {
                        base.as_str()
                    }
                ),
                None => "no footage scene at the line \u{2014} nothing to show from a row".to_string(),
            }
        )));
        let win = window.clone();
        lens.connect_toggled(move |badge| {
            // A `set_active` from the refresh is not a press: without this guard the refresh would
            // re-enter the very press that asked for it and never settle.
            if refreshing() {
                return;
            }
            // The button's own face is set here; the RULE runs in `press_lens_row`. A press that the
            // rule refuses (an insert reached despite the greying) puts the face back rather than
            // leaving a lit badge over an unchanged cut.
            if press_lens_row(&win, row).is_none() {
                badge.set_active(false);
            }
        });
        holder.append(&lens);

        // S2: one speaker badge per lane, plus that lane's gutter switch beside it.
        for lane in lanes.iter().cloned() {
            let heard = lane_heard_in_line_scene(&cut_, &lane, scene);
            // The lane name each closure below owns its own copy of, so no closure borrows a loop
            // variable that dies at the next row.
            let badge = gtk::ToggleButton::new();
            badge.set_widget_name(&format!("speaker-badge-{lane}"));
            badge.set_label(SPEAKER_GLYPH);
            // Pressed = silent, so the badge reads as struck-through sound when the scene does not hear.
            badge.set_active(!heard);
            badge.set_sensitive(scene.is_some());
            badge.set_tooltip_text(Some(&cut_hear::hush_status(
                lane.as_str(),
                heard,
                scene.map(|i| cut_.segs[i].s).unwrap_or(at),
            )));
            let win = window.clone();
            let badge_lane = lane.clone();
            badge.connect_toggled(move |toggled| {
                if refreshing() {
                    return;
                }
                if press_speaker_badge(&win, &badge_lane).is_none() {
                    toggled.set_active(false);
                }
            });
            holder.append(&badge);

            let gutter = gtk::ToggleButton::new();
            gutter.set_widget_name(&format!("gutter-switch-{lane}"));
            gutter.set_label("\u{25ac}");
            // OFF (not pressed) when the lane is silent EVERYWHERE, on while any scene still hears it —
            // the same reading `cut_hear::toggle_lane_all` acts on, so the switch never claims a state
            // the next press contradicts.
            gutter.set_active(cut_hear::lane_is_heard_anywhere(&cut_, &[lane.as_str()]));
            gutter.set_tooltip_text(Some(&format!("{lane} for the whole cut")));
            let win = window.clone();
            gutter.connect_toggled(move |switch| {
                if refreshing() {
                    return;
                }
                press_gutter_switch(&win, lane.as_str());
                // The switch's own face is the whole-cut answer, which `toggle_lane_all` just wrote;
                // re-read it so a half-silenced row that went fully silent shows exactly that.
                let fresh = newest_review_cut();
                switch.set_active(cut_hear::lane_is_heard_anywhere(&fresh, &[lane.as_str()]));
            });
            holder.append(&gutter);
        }
        // NOT re-appended: `holder` is already inside `rows_box` — it was put there when first built.
        // Appending it again is the `gtk_box_append: assertion 'gtk_widget_get_parent (child) == NULL'
        // failed` that shows up on every refresh after the first.
    }
    CAMERA_REFRESH.with(|cell| cell.set(false));
}

thread_local! {
    /// F2.10: set while [`refresh_camera_rows`] is pushing state INTO the widgets, so a `set_active`
    /// made by the refresh cannot be mistaken for a user press and re-enter the press that asked for the
    /// refresh. Without this the chain `press_* -> refresh -> set_active -> connect_toggled -> press_*`
    /// never ends.
    static CAMERA_REFRESH: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Is a camera-row refresh in progress right now? Every toggle handler starts with this and returns if set.
fn refreshing() -> bool {
    CAMERA_REFRESH.with(|cell| cell.get())
}

/// F2.10 S1 (`🔍 lens badge: which row its picture comes from`): the seam the badge forwards to. The
/// scene keeps its own seconds and only changes which row its picture is read off, so the edit is the
/// `cam` field and nothing else — [`cut_cam::show_scene_from`] refuses an insert for exactly that
/// reason, and a refusal leaves this window's cut untouched rather than half-applied.
pub fn press_lens_row(window: &adw::ApplicationWindow, row: i32) -> Option<String> {
    let scene = kept_scene_at_line(window)?;
    let mut cut_ = newest_review_cut();
    let recordings = crate::timeline::kept_footage_recordings(&cut_);
    let rows = crate::timeline::rows_for(&recordings, &cut_);
    let said = cut_cam::show_scene_from(&mut cut_, scene, row, &recordings, &rows)?;
    seed_review_cut(window, &cut_);
    if let Some(status) = find_status(window.upcast_ref()) {
        status.set_text(&said);
    }
    // The lit badge follows the cut, so the rows are rebuilt from the new state rather than queued.
    refresh_camera_rows(window);
    Some(said)
}

/// F2.10 S2 (`🔈 speaker badge per lane: does this scene hear that lane`): the seam a badge forwards
/// to. With no kept scene under the line there is no scene to be silent in, so nothing is printed and
/// nothing changes.
pub fn press_speaker_badge(window: &adw::ApplicationWindow, lane: &str) -> Option<String> {
    let scene = kept_scene_at_line(window)?;
    let mut cut_ = newest_review_cut();
    let said = cut_hear::toggle_heard(&mut cut_, scene, lane)?;
    seed_review_cut(window, &cut_);
    if let Some(status) = find_status(window.upcast_ref()) {
        status.set_text(&said);
    }
    refresh_camera_rows(window);
    Some(said)
}

/// F2.10 S2 (`the gutter switch toggles a lane for the whole cut`): the seam the gutter forwards to.
/// The switch stands for every recording sharing that lane's name, so `lanes` is that name once and
/// `name` is what the status calls it — the same string, because today one switch speaks for one
/// recording. [`cut_hear::toggle_lane_all`] answers even when there is nothing to change ("… is in no
/// scene yet"), so this always has a sentence to print.
pub fn press_gutter_switch(window: &adw::ApplicationWindow, lane: &str) -> String {
    let mut cut_ = newest_review_cut();
    let said = cut_hear::toggle_lane_all(&mut cut_, &[lane], lane);
    seed_review_cut(window, &cut_);
    if let Some(status) = find_status(window.upcast_ref()) {
        status.set_text(&said);
    }
    refresh_camera_rows(window);
    said
}

/// F2.10 S3: ▶ hands the preview back — the seam [`hand_preview_back`] is reached through, kept separate
/// so a test can count how many times the watch was released without a real ▶.
pub fn hand_preview_back(window: &adw::ApplicationWindow) -> Option<String> {
    let was = {
        let watch = camera_watch(window);
        let mut watch = watch.borrow_mut();
        watch.play_hands_back()
    };
    if !was {
        return None;
    }
    // The row's own name comes from this window's recordings, so the sentence names a camera rather
    // than a number; `row_name` falls back to "row N" when nothing is laid out yet.
    let cut_ = newest_review_cut();
    let recordings = crate::timeline::kept_footage_recordings(&cut_);
    let rows = crate::timeline::rows_for(&recordings, &cut_);
    let named = watched_row()
        .map(|row| cut_cam::row_name(row as i32, &recordings, &rows))
        .unwrap_or_else(|| "the watched row".to_string());
    set_watched_row(None);
    refresh_camera_rows(window);
    Some(format!("\u{25b6} plays the cut \u{2014} {named} no longer watched"))
}

/// F2.10: the box holding this window's camera rows, by its stable name — `None` on a page that drew
/// none. Every F2.10 press repaints through it, and Task 2 of this item builds its contents here.
pub fn camera_rows_box(window: &adw::ApplicationWindow) -> Option<gtk::Box> {
    find_widget_by_name(window.upcast_ref(), "camera-rows")?
        .downcast()
        .ok()
}

thread_local! {
    static HELD_EFFECT: std::cell::RefCell<Option<cut::Fx>> =
        const { std::cell::RefCell::new(None) };
    static HELD_CLIP: std::cell::RefCell<Option<cut::Seg>> =
        const { std::cell::RefCell::new(None) };
    /// F2.4 S1b: the clip edge a first left press picked up within `layout.lineReachPx` (12 px).
    /// Kept apart from the whole-clip hold because a frame step must move the edge, not the clip.
    static HELD_EDGE: std::cell::RefCell<Option<cut::Seg>> =
        const { std::cell::RefCell::new(None) };
    /// F2.4 S1: which picture row this window is watching. Only a picture-band click sets it, and only
    /// while nothing plays with sources loaded — `cut_line::watches_row` owns that; this just remembers.
    static WATCHED_ROW: std::cell::RefCell<Option<usize>> =
        const { std::cell::RefCell::new(None) };
    /// The right half a band-less Split put in the hand (F2.7 S2). The paste round reads it.
    static VERB_HAND: std::cell::RefCell<Option<cut_select::Selection>> =
        const { std::cell::RefCell::new(None) };
}

/// This window's ⌦ controller, so a test can see it is attached. `None` on a page that drew none.
pub fn delete_key_controller(window: &adw::ApplicationWindow) -> Option<gtk::EventControllerKey> {
    let _ = window;
    DELETE_KEYS.with(|cell| cell.borrow().clone())
}

// --- F2.8: trim and move on the track strip -------------------------------------------------------------

/// Pixels-per-second for the placeholder strip — the same zoom `select-surface` runs at, so a pixel the
/// hand travels means the same number of seconds on both strips.
pub const TRACK_STRIP_PPS: f64 = cut_screen::ZOOM_AT_OPEN;

/// This window's `track-strip` (F2.8), by its stable name.
pub fn track_strip(window: &adw::ApplicationWindow) -> Option<gtk::DrawingArea> {
    find_widget_by_name(window.upcast_ref(), "track-strip")?
        .downcast()
        .ok()
}

/// The left-button drag that trims a border, so a test can fire it the way GTK does.
pub fn trim_gesture(window: &adw::ApplicationWindow) -> Option<gtk::GestureDrag> {
    let _ = window;
    TRIM_GESTURES.with(|cell| cell.borrow().clone())
}

/// The right-button drag that moves. Same reason for existing as [`trim_gesture`].
pub fn move_gesture(window: &adw::ApplicationWindow) -> Option<gtk::GestureDrag> {
    let _ = window;
    MOVE_GESTURES.with(|cell| cell.borrow().clone())
}

thread_local! {
    /// The trim drag in flight: which clip's which border the press took, and when it last scrubbed.
    /// `None` means the press fell where no border was in reach, so the drag does nothing at all.
    static TRIM_DRAGS: std::cell::RefCell<Option<TrimDrag>> =
        const { std::cell::RefCell::new(None) };
    static TRIM_GESTURES: std::cell::RefCell<Option<gtk::GestureDrag>> =
        const { std::cell::RefCell::new(None) };
    static MOVE_GESTURES: std::cell::RefCell<Option<gtk::GestureDrag>> =
        const { std::cell::RefCell::new(None) };
    /// F2.8 S2: where the move drag was PRESSED, (x, y) in strip px, newest window first. The scope has to
    /// be fixed at the press — `wave_strip` and `on_border` are read from this, not re-derived on each
    /// update, so a pointer wandering off the band mid-drag cannot change what was picked up.
    static MOVE_PRESSED: std::cell::RefCell<Vec<Rc<std::cell::RefCell<(f64, f64)>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// The folds opened for the drag in flight, newest window first. Refolded when the gesture ends;
    /// until the fold surface exists this only records what the rule asked to open.
    static OPEN_FOLDS: std::cell::RefCell<Vec<Rc<std::cell::RefCell<Vec<usize>>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// What the right button was pressed on, where the page knows it: a recording on the recorders' band, or
/// the scene under the pointer. Both are seams standing in until the real track rounds (F2.10/F2.11) set
/// them from an actual press position, exactly as F2.7's held-effect/held-clip seams do.
pub fn set_press_band(recording: Option<String>) {
    PRESS_BAND.with(|cell| *cell.borrow_mut() = recording);
}

/// Read side of [`set_press_band`].
pub fn press_band() -> Option<String> {
    PRESS_BAND.with(|cell| cell.borrow().clone())
}

/// The scene index a press fell on, if it fell on one.
pub fn set_press_scene(scene: Option<usize>) {
    PRESS_SCENE.with(|cell| *cell.borrow_mut() = scene);
}

/// Read side of [`set_press_scene`].
pub fn press_scene() -> Option<usize> {
    PRESS_SCENE.with(|cell| cell.borrow().clone())
}

thread_local! {
    static PRESS_BAND: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
    static PRESS_SCENE: std::cell::RefCell<Option<usize>> =
        const { std::cell::RefCell::new(None) };
}

/// F2.8 S1: drag one clip's border to `to` (session seconds). [`cut_trim::clamp_edge`] decides where it
/// may go — a scene's worth of length, the next clip, the recording's end — this writes that answer into
/// the window's cut and prints the sentence it earned. No rule lives here.
pub fn press_trim_border(
    window: &adw::ApplicationWindow,
    index: usize,
    border: cut_trim::Border,
    to: f64,
) -> f64 {
    let mut cut_ = newest_review_cut();
    let clamped = cut_trim::clamp_edge(&cut_.segs, index, border, to, 0.0, TRACK_REC_END);
    match border {
        cut_trim::Border::Start => {
            if let Some(seg) = cut_.segs.get_mut(index) {
                seg.s = clamped;
            }
        }
        cut_trim::Border::End => {
            if let Some(seg) = cut_.segs.get_mut(index) {
                seg.e = clamped;
            }
        }
    }
    // S1 on release: a neighbour landed against within a frame joins back into one scene, and the
    // sentence says so because putting that border back is the thing a person may want most.
    let status = if cut_trim::merge_pair(&cut_.segs, index).is_some() {
        let joined = cut_trim::merge(&mut cut_.segs, index);
        match joined {
            Some((held, _)) => cut_.segs.get(held).map(|seg| {
                cut_trim::joined_status(seg.s, seg.e)
            }).unwrap_or_default(),
            None => String::new(),
        }
    } else {
        cut_.segs
            .get(index)
            .map(|seg| cut_trim::trim_status(index + 1, seg.s, seg.e))
            .unwrap_or_default()
    };
    seed_review_cut(window, &cut_);
    if !status.is_empty() {
        if let Some(status_line) = find_status(window.upcast_ref()) {
            status_line.set_text(&status);
        }
    }
    clamped
}

/// The recording's own span the placeholder strip stands inside. A placeholder number for a placeholder
/// track: the real bounds come with the tracks (F2.10/F2.11), and until then a trim needs *some* wall to
/// stop at rather than none, which would let a clip be dragged off the end of a session.
pub const TRACK_REC_END: f64 = 3600.0;

/// F2.8 S2/S3: a right-drag moved. [`cut_trim::right_gesture`] answers the whole gesture; this applies
/// the answer — the shift map, the row change, the folds to open — and prints the sentence.
///
/// S3 holds here absolutely: nothing on this path touches the red line. `cut_trim::right_press_moves_line()`
/// is `false`, and there is deliberately no `set_playhead` / `move_line_and_save` call anywhere in this
/// function or in the button-3 gesture that reaches it, so the page cannot drift into moving the line
/// while the right hand is doing arithmetic on the clock.
/// F2.8 S2: the boxes `track-strip` paints for this window's cut, in the same px space a press arrives in.
/// Read from the newest review cut so a press and a paint never disagree about where a clip is.
fn trim_boxes_for_strip() -> Vec<cut_trim::Box_> {
    let cut_ = newest_review_cut();
    cut_trim::clip_boxes(&cut_.segs, TRACK_STRIP_PPS)
}

/// F2.8 S1/S2: what a right-press at `press_x` takes. A border grabbed within [`cut_trim::EDGE_GRAB_PX`]
/// of a DRAWN box belongs to S1 ("either button drags that border") and must be trimmed, not slid — but
/// only once the hand has actually travelled: an unmoved press stays a click (S3), which trims nothing.
/// Anything else falls through to the move rule.
fn right_press_takes_border(press_x: f64, travel_px: f64) -> Option<(usize, cut_trim::Border)> {
    if travel_px.abs() < cut_select::DRAG_SLOP_PX {
        return None;
    }
    cut_trim::border_at(&trim_boxes_for_strip(), press_x)
}

/// F2.8 S2: where this window's move press landed in x, in strip px. The widget path stores it at
/// `drag-begin`; until F2.10/F2.11 own the real rows a caller may also prime it directly (the same kind of
/// seam `set_press_scene` is), which is how a test places a press mid-scene rather than on a border.
pub fn set_press_x(x: f64) {
    // Read the old pair first and drop that borrow before writing: holding it across the second
    // `borrow_mut` on the same RefCell panics with "RefCell already borrowed".
    let old = MOVE_PRESSED.with(|cell| cell.borrow().last().map(|s| *s.borrow()));
    if let Some((_, y)) = old {
        MOVE_PRESSED.with(|cell| {
            if let Some(slot) = cell.borrow().last() {
                *slot.borrow_mut() = (x, y);
            }
        });
    }
}

/// Read side of [`set_press_x`] — the stored press position for the newest window.
pub fn press_position() -> (f64, f64) {
    MOVE_PRESSED.with(|cell| cell.borrow().last().map(|s| *s.borrow()).unwrap_or((0.0, 0.0)))
}

/// F2.8 S2/S3: the older door, seam-only. Kept for callers (and tests) that supply no press position: it
/// answers with whatever `set_press_band` primed and nothing else — no band, no border — so a call here can
/// still never move the line without travel. The widget path goes through [`press_move_at`] instead.
pub fn press_move(
    window: &adw::ApplicationWindow,
    travel_px: f64,
    row_change: bool,
    d_seconds: f64,
) -> cut_trim::Gesture {
    press_move_with_override(
        window,
        press_band(),
        None,
        false,
        travel_px,
        row_change,
        d_seconds,
    )
}

/// F2.8 S2: what a right-press at (`press_x`, `press_y`) took, exactly as the widget computes it — the
/// same door, with the answer returned instead of applied. Kept separate from [`press_move_at`] so a test
/// can assert WHICH branch the wire chose without trusting the status sentence alone.
///
/// The recording NAME comes from `set_press_band` (until F2.10/F2.11 own the lane names); WHICH band was
/// hit comes from the press y through `cut_trim::press_targets` over `cut_trim::PLACEHOLDER_STRIP_BANDS`.
/// That is what makes the flowchart's wave-strip branch reachable from a real right-press with nothing
/// primed but the name.
pub fn right_press_took(
    press_x: f64,
    press_y: f64,
) -> (Option<String>, Option<String>, bool) {
    let boxes = trim_boxes_for_strip();
    cut_trim::press_targets(press_y, press_x, &boxes, TRACK_STRIP_PPS, press_band().as_deref())
}

/// F2.8 S2: the move door that knows WHERE the press landed. Same rule as [`press_move`], but its
/// `recorders_band` / `wave_strip` / `on_border` come from the press position through
/// [`right_press_took`], not from literals. `press_scene`/`press_row` remain overrides for the scene/row
/// questions; those seams are all that is left standing in, because the page has no real rows to read them
/// off yet.
pub fn press_move_at(
    window: &adw::ApplicationWindow,
    press_x: f64,
    press_y: f64,
    travel_px: f64,
    d_seconds: f64,
) -> cut_trim::Gesture {
    let (recorders_band, wave_strip, on_border) = right_press_took(press_x, press_y);
    press_move_with_override(window, recorders_band, wave_strip, on_border, travel_px, false, d_seconds)
}

/// F2.8 S2/S3: the whole move rule with its three inputs supplied. Both doors land here: the widget's
/// position-driven [`press_move_at`] and the older seam-only [`press_move`], which passes whatever seams
/// were primed. A press that never travelled stays a click and never moves the line
/// ([`cut_trim::right_press_moves_line`]).
fn press_move_with_override(
    window: &adw::ApplicationWindow,
    recorders_band: Option<String>,
    wave_strip: Option<String>,
    on_border: bool,
    travel_px: f64,
    row_change: bool,
    d_seconds: f64,
) -> cut_trim::Gesture {
    // The scene seam only answers when this press took neither a band nor a selection away from a border:
    // the flowchart asks those questions BEFORE it asks about a scene.
    let scene = if recorders_band.is_some() || wave_strip.is_some() || on_border {
        None
    } else {
        let inside_selection = selection(window)
            .map(|b| b.length() >= crate::cut_select::MIN_SECONDS)
            .unwrap_or(false);
        if inside_selection {
            None
        } else {
            press_scene()
        }
    };
    press_move_core(
        window,
        recorders_band,
        wave_strip,
        on_border,
        scene,
        travel_px,
        row_change,
        d_seconds,
    )
}

/// F2.8 S2: the move rule with every input supplied, including which scene (if any) the press fell on.
fn press_move_core(
    window: &adw::ApplicationWindow,
    recorders_band: Option<String>,
    wave_strip: Option<String>,
    on_border: bool,
    scene: Option<usize>,
    travel_px: f64,
    row_change: bool,
    d_seconds: f64,
) -> cut_trim::Gesture {
    let cut_ = newest_review_cut();
    let band = selection(window);
    let band_name = recorders_band.clone();
    let selected: Vec<usize> = match (&band, &cut_) {
        (Some(band), cut_) => cut_
            .segs
            .iter()
            .enumerate()
            .filter(|(_, seg)| seg.ins.is_empty() && seg.e > band.start && seg.s < band.end)
            .map(|(index, _)| index)
            .collect(),
        (None, _) => Vec::new(),
    };
    let sources: Vec<String> = cut_
        .shift
        .keys()
        .cloned()
        .chain(band_name.clone())
        .collect::<std::collections::BTreeSet<String>>()
        .into_iter()
        .collect();
    let press = cut_trim::Press {
        recorders_band: recorders_band.as_deref(),
        wave_strip: wave_strip.as_deref(),
        selection: band.as_ref().map(|b| (b.start, b.end)),
        selected,
        inside_selection: band
            .as_ref()
            .map(|b| crate::cut_select::MIN_SECONDS <= b.length())
            .unwrap_or(false),
        on_border,
        scene,
        row: press_row(window),
    };
    let gesture = cut_trim::right_gesture(
        &press,
        travel_px,
        row_change,
        d_seconds,
        &cut_.segs,
        0.0,
        TRACK_REC_END,
        TRACK_STRIP_PPS,
        &cut_.folds,
        &cut_.shift,
        &sources,
        false,
    );
    apply_gesture(window, &gesture);
    gesture
}

/// Write a gesture's answer into this window's cut. A click writes nothing at all — S3's unmoved press
/// leaves the project exactly as it was, including its status line.
fn apply_gesture(window: &adw::ApplicationWindow, gesture: &cut_trim::Gesture) {
    let cut_trim::Gesture::Slid {
        status,
        shift,
        row,
        open,
        ..
    } = gesture
    else {
        return;
    };
    let mut cut_ = newest_review_cut();
    cut_.shift = shift.clone();
    if let Some(row) = row {
        // Rows are counted from 1 in the sentence and stored per source; the placeholder records the
        // hand's target so the next draw reads it back.
        cut_.nrows = (*row as i32) + 1;
    }
    seed_review_cut(window, &cut_);
    OPEN_FOLDS.with(|slots| {
        // The slot is created on first use rather than at window build: nothing else on this page needs
        // it, and `seed_review_cut` shows a seam can write the newest slot without one. Pushed when
        // empty so the newest window's folds are always the last entry, like SELECTED and REVIEW_CUTS.
        let mut slots = slots.borrow_mut();
        if slots.is_empty() {
            slots.push(Rc::new(std::cell::RefCell::new(Vec::new())));
        }
        if let Some(slot) = slots.last() {
            *slot.borrow_mut() = open.clone();
        }
    });
    if let Some(status_line) = find_status(window.upcast_ref()) {
        status_line.set_text(status);
    }
}

/// The row a right-press would land on. Until the rows are drawn the strip sits on row 0; the seam keeps
/// the page from inventing a row number of its own.
fn press_row(_window: &adw::ApplicationWindow) -> usize {
    0
}

/// The folds this window's drag in flight asked to open, newest write last. Read by the fold surface when
/// it exists; exposed now so a test can assert the rule's answer reached the page.
pub fn open_folds(window: &adw::ApplicationWindow) -> Vec<usize> {
    let _ = window;
    OPEN_FOLDS.with(|slots| {
        slots
            .borrow()
            .last()
            .map(|slot| slot.borrow().clone())
            .unwrap_or_default()
    })
}

// --- F2.9: copy, paste and lane --------------------------------------------------------------------------

thread_local! {
    /// The copy in hand (F2.9 S1), newest window first's rule not needed: one copy app-wide, because a
    /// second one would leave the person unsure which ⧉ Paste answers.
    static COPY_HAND: std::cell::RefCell<Option<cut_copy::Hand>> =
        const { std::cell::RefCell::new(None) };
    static COPY_ESC_KEYS: std::cell::RefCell<Option<gtk::EventControllerKey>> =
        const { std::cell::RefCell::new(None) };
}

/// What is in hand right now — the read side of [`press_copy`], so a test can assert the hand without
/// reaching into thread-locals.
pub fn copy_hand() -> Option<cut_copy::Hand> {
    COPY_HAND.with(|cell| cell.borrow().clone())
}

/// This window's Esc controller (F2.9 S2), so a test can see it is attached. `None` on a page that drew none.
pub fn copy_esc_controller(window: &adw::ApplicationWindow) -> Option<gtk::EventControllerKey> {
    let _ = window;
    COPY_ESC_KEYS.with(|cell| cell.borrow().clone())
}

/// F2.9 S1: ⧉ Copy. [`cut_copy::copy`] decides everything — whether there is a band, whether it is a
/// second long, what it is a stretch of; this only reads the band, keeps the answer if it took one, and
/// hands back the sentence to print. A refusal leaves no hand, so ⧉ Paste stays greyed.
pub fn press_copy(window: &adw::ApplicationWindow) -> String {
    let taken = cut_copy::copy(selection(window).as_ref());
    match &taken {
        cut_copy::Take::Taken(hand) => {
            COPY_HAND.with(|cell| *cell.borrow_mut() = Some(hand.clone()));
            cut_copy::copied_status(hand)
        }
        cut_copy::Take::TooShort(say) => say.clone(),
        cut_copy::Take::NothingSelected(say) => (*say).to_string(),
    }
}

/// The line the copy lands on. The player's own playhead when one is running, otherwise the stored line:
/// both are the same number while nothing plays, and the player is what the eye is watching.
fn paste_line(window: &adw::ApplicationWindow) -> f64 {
    live_player(window)
        .map(|player| player.borrow().playhead.unwrap_or_else(|| line_position(window).t))
        .unwrap_or_else(|| line_position(window).t)
}

/// F2.9 S2: ⧉ Paste at the red line. [`cut_copy::paste`] decides footage-vs-sound, splices or lays, and
/// refuses with its own sentence; this supplies the three things only the page knows — the line, the
/// copied recording's project path, and the file second the span starts at — then writes the mutated cut
/// back and mirrors the hand so a refusal keeps the copy.
pub fn press_paste(window: &adw::ApplicationWindow) -> String {
    let at = paste_line(window);
    let mut hand = copy_hand();
    // The copied recording's path: a sound names itself by base, and until F2.10/F2.11 own the row/path
    // mapping the base IS the project-relative path the cut uses, so an unmatched name passes through
    // unchanged rather than inventing one.
    let sources = session_sources(window);
    let path = match hand.as_ref().and_then(|h| h.recording_public()) {
        Some(recording) => sources
            .iter()
            .find(|source| source_base(source) == recording)
            .cloned()
            .unwrap_or_else(|| recording.to_string()),
        None => String::new(),
    };
    // Session→file seconds for the copied span. No offset helper exists yet — the aligner round brings the
    // two clocks together — so the span's own start, clamped at zero, is what the lane/file is asked for.
    let file_seconds = hand.as_ref().map(|h| h.from.max(0.0)).unwrap_or(0.0);
    let mut cut_ = newest_review_cut();
    let outcome = cut_copy::paste(&mut cut_, &mut hand, Some(at), &path, file_seconds, &sources);
    seed_review_cut(window, &cut_);
    COPY_HAND.with(|cell| *cell.borrow_mut() = hand);
    match outcome {
        Ok(status) => status,
        Err(refusal) => refusal,
    }
}

/// The base name of a project path — what a `Scope::Sound { recording }` names.
fn source_base(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(path)
        .to_string()
}

/// F2.9 S3: ⇲ Lane. [`cut_copy::lane_start`] says whether the line may open a row there and
/// [`cut_copy::lane`] builds it; this names it and files it. NOTHING is cut to the new row — no `Seg` is
/// added here, because a lane that arrived already green would be a cut nobody made; ＋ Add lays scenes on
/// it later. Name: `Copied`, then `Copied-2`, `Copied-3`… past every row already taken, since
/// `Lane::name` keys `Cut::rows` and every scene's `quiet` list.
pub fn press_lane(window: &adw::ApplicationWindow) -> String {
    let Some(hand) = copy_hand() else {
        return "nothing is in hand \u{2014} \u{29c9} Copy takes the selection first".to_string();
    };
    let at = paste_line(window);
    let mut cut_ = newest_review_cut();
    // The session's filmed runs, from the recordings the page draws its rows from. Until F2.11 owns them,
    // the kept footage spans stand in: they are what was filmed and kept, which is the honest answer.
    let filmed: Vec<(f64, f64)> = cut_copy::footage_stretches(&cut_, 0.0, f64::MAX);
    match cut_copy::lane_start(Some(at), &hand, &filmed) {
        cut_copy::LaneStart::Refusal(say) => say,
        cut_copy::LaneStart::Start(at) => {
            let taken: Vec<String> = cut_.lanes.iter().map(|lane| lane.name.clone()).collect();
            let name = cut_copy::lane_name("Copied", &taken);
            let source = hand
                .source_public()
                .map(|src| src.to_string())
                .unwrap_or_else(|| session_sources(window).first().cloned().unwrap_or_default());
            let file_seconds = hand.from.max(0.0);
            let Some(lane) = cut_copy::lane(&hand, &source, file_seconds, at, name.clone()) else {
                return cut_copy::too_short().to_string();
            };
            cut_.lanes.push(lane);
            seed_review_cut(window, &cut_);
            cut_copy::lane_status(hand.length, hand.from, &name, at)
        }
    }
}

/// S2: Esc drops the copy. Claims ONLY Escape — every other key returns `Proceed`, so typing in an entry
/// field is untouched, exactly as the ⌦ controller does for its two keys.
fn wire_copy_esc(window: &adw::ApplicationWindow) {
    let controller = gtk::EventControllerKey::new();
    let win = window.clone();
    controller.connect_key_pressed(move |_ctrl, key, _code, _mods| {
        if key == gtk::gdk::Key::Escape {
            COPY_HAND.with(|cell| *cell.borrow_mut() = None);
            if let Some(status_line) = find_status(win.upcast_ref()) {
                status_line.set_text(cut_copy::DROPPED);
            }
            refresh_copy_buttons(&win);
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(controller.clone());
    COPY_ESC_KEYS.with(|cell| *cell.borrow_mut() = Some(controller));
}

/// F2.9: set the three copy buttons from live state. Copy is live with a band a second or longer; Paste
/// and Lane only while a hand is held. Runs from `refresh_selection_readout`, which every draw, nudge and
/// clear already calls, so a greyed button is always today's answer.
fn refresh_copy_buttons(window: &adw::ApplicationWindow) {
    let copy_live = selection(window)
        .map(|band| band.length() >= cut_select::MIN_SCENE_SECONDS)
        .unwrap_or(false);
    let held = copy_hand().is_some();
    if let Some(button) = line_step_button(window, "copy-button") {
        button.set_sensitive(copy_live);
    }
    for name in ["paste-button", "lane-button"] {
        if let Some(button) = line_step_button(window, name) {
            button.set_sensitive(held);
        }
    }
}

/// F2.8 S1: the strip's height — the ruler row plus the kept-bar row, one number the size request and
/// the painter both read so they cannot drift apart.
pub const STRIP_HEIGHT: i32 = (cut_trim::RULER_H + cut_trim::BAR_H) as i32;

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

/// Paint `track-strip` from this window's own cut. The callback reads state and draws it: every mark comes
/// from `cut_trim::ruler_ticks`, `cut_trim::clip_boxes` and `cut_trim::border_positions` at
/// [`TRACK_STRIP_PPS`], so the border the eye sees is the same pixel the hand grabs. An empty cut draws
/// the frame plus [`cut_trim::STRIP_EMPTY_HINT`] rather than a blank box, which would read as broken.
fn paint_track_strip(cr: &cairo::Context, width: i32, height: i32) {
    let w = width.max(1) as f64;
    let h = height.max(1) as f64;
    let cut_ = newest_review_cut();

    // Ground first, so every mark after it sits on something.
    cr.set_source_rgb(0.95, 0.95, 0.95);
    cr.rectangle(0.0, 0.0, w, h);
    let _ = cr.fill();

    // The ruler row: whole-second ticks with their labels, then the kept bar under them.
    let ticks = cut_trim::ruler_ticks(TRACK_REC_END, TRACK_STRIP_PPS, RULER_EVERY_SECONDS);
    cr.set_source_rgb(0.45, 0.45, 0.45);
    for (x, _) in &ticks {
        if *x <= w {
            cr.rectangle(*x, 0.0, 1.0, cut_trim::RULER_H);
            let _ = cr.fill();
        }
    }
    draw_strip_labels(cr, &ticks, w);

    let boxes = cut_trim::clip_boxes(&cut_.segs, TRACK_STRIP_PPS);
    if boxes.is_empty() {
        cr.select_font_face("Sans", cairo::FontSlant::Normal, gtk::cairo::FontWeight::Normal);
        cr.set_font_size(11.0);
        cr.set_source_rgb(0.45, 0.45, 0.45);
        let _ = cr.move_to(6.0, cut_trim::RULER_H + cut_trim::BAR_H * 0.6);
        let _ = cr.show_text(cut_trim::STRIP_EMPTY_HINT);
        return;
    }

    let top = cut_trim::RULER_H;
    for b in &boxes {
        // Violet for an insert, the kept green for footage — the two colours the inventory lists, told
        // apart because only one of them may be trimmed.
        if b.insert {
            cr.set_source_rgba(0.55, 0.35, 0.75, 0.55);
        } else {
            cr.set_source_rgba(0.2, 0.8, 0.3, 0.3);
        }
        cr.rectangle(b.x, top, b.w.max(1.0), cut_trim::BAR_H);
        let _ = cr.fill();
    }
    // Borders last and darker, so they read on top of the fill: these are the pixels a press takes.
    cr.set_source_rgb(0.11, 0.42, 0.16);
    for (x, _, _) in cut_trim::border_positions(&boxes) {
        cr.rectangle(x - 0.5, top, 1.5, cut_trim::BAR_H);
        let _ = cr.fill();
    }
}

/// Draw the ruler's labels onto the strip's cairo context. Cairo text is best-effort in the headless
/// container (no font config beyond what GTK brings up), so a failure here must never take the strip down
/// with it: every call's result is ignored and the tick marks stay visible regardless.
fn draw_strip_labels(cr: &cairo::Context, ticks: &[(f64, String)], w: f64) {
    cr.set_source_rgb(0.35, 0.35, 0.35);
    for (x, label) in ticks {
        if *x <= w {
            let _ = cr.move_to(x + 2.0, 11.0);
            let _ = cr.show_text(label);
        }
    }
}

/// The trim drag in flight, as [`wire_track_strip`] holds it between the press and the release.
struct TrimDrag {
    /// The segment whose border is being dragged — the index into this window's cut.
    index: usize,
    /// Which of its two borders the press took.
    border: cut_trim::Border,
    /// When the last scrub happened, for `cut_trim::scrubs`' throttle.
    scrubbed_ms: Option<u64>,
}

/// Milliseconds since an arbitrary origin, for the scrub throttle. Monotonic enough for a 90 ms gate; the
/// app never compares these across a restart.
fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Attach the two gestures to `track-strip`: the left button trims a border, the right button moves.
/// Registered after `set_content` like every other control here, and kept in thread-locals so a test can
/// fire them the way GTK does.
fn wire_track_strip(window: &adw::ApplicationWindow) {
    let Some(strip) = track_strip(window) else { return };

    let trimmer = gtk::GestureDrag::new();
    // The border a drag chases, seeded from the press position over the drawn layout. Until F2.10/F2.11
    // own the real rows, this strip's own geometry (`cut_trim::clip_boxes` at `TRACK_STRIP_PPS`) is what
    // the pointer is pointing at — so the hand grabs exactly the pixel the eye saw, and no rule about
    // where a border may go lives here: `press_trim_border` asks `cut_trim::clamp_edge` for that.
    trimmer.connect_drag_begin(move |_g, px, _py| {
        let boxes = cut_trim::clip_boxes(&newest_review_cut().segs, TRACK_STRIP_PPS);
        let taken = cut_trim::border_at(&boxes, px);
        TRIM_DRAGS.with(|cell| {
            *cell.borrow_mut() = taken.map(|(index, border)| TrimDrag {
                index,
                border,
                scrubbed_ms: None,
            })
        });
    });
    let win_upd = window.clone();
    let strip_upd = strip.clone();
    let strip_end = strip.clone();
    trimmer.connect_drag_update(move |_g, x, _y| {
        TRIM_DRAGS.with(|cell| {
            let Some(mut drag) = cell.borrow_mut().take() else { return };
            // S1: the picture scrubs live, but at most every `SCRUB_MS` — an accurate seek per mouse-move
            // event is a pipeline that never stops flushing. `// preview.scrubMs`
            let now = now_millis();
            if !cut_trim::scrubs(drag.scrubbed_ms, now) {
                *cell.borrow_mut() = Some(drag);
                return;
            }
            drag.scrubbed_ms = Some(now);
            press_trim_border(&win_upd, drag.index, drag.border, x / TRACK_STRIP_PPS);
            *cell.borrow_mut() = Some(drag);
        });
        // The border moved, so the strip has to be repainted or the hand drags something invisible.
        strip_upd.queue_draw();
    });
    let win_end = window.clone();
    trimmer.connect_drag_end(move |_g, x, _y| {
        // Release lands the final edge and prints the sentence it earned (joined-into-one-scene when a
        // neighbour came within a frame), through the same seam as every tick above.
        let done = TRIM_DRAGS.with(|cell| cell.borrow_mut().take());
        if let Some(drag) = done {
            press_trim_border(&win_end, drag.index, drag.border, x / TRACK_STRIP_PPS);
        }
        TRIM_DRAGS.with(|cell| *cell.borrow_mut() = None);
        strip_end.queue_draw();
    });
    strip.add_controller(trimmer.clone());
    TRIM_GESTURES.with(|cell| *cell.borrow_mut() = Some(trimmer));

    // Button 3 is GTK's secondary button — the right hand, whose drags move and never trim.
    let mover = gtk::GestureDrag::new();
    mover.set_button(3);
    // F2.8 S2: the press fixes what the drag takes, so its x AND y are remembered here — `wave_strip` and
    // `on_border` have to come from where the pointer actually landed, not from a seam primed beforehand.
    // Newest-slot rule like `SELECT_SURFACES`, so one window's press cannot answer another's drag.
    let pressed_begin = Rc::new(std::cell::RefCell::new((0.0f64, 0.0f64)));
    MOVE_PRESSED.with(|cell| cell.borrow_mut().push(pressed_begin.clone()));
    let pressed_upd = pressed_begin.clone();
    // A `drag-begin` carries the ABSOLUTE press position; a `drag-update`/`drag-end` carry the OFFSET from
    // it. So the begin stores the absolute pair, and an update's own numbers are added to it to get where
    // the pointer is now. A caller that primed the x seam directly (`set_press_x`) already supplied the
    // absolute press, so its stored 0.0 begin adds nothing and the seam answer stands.
    mover.connect_drag_begin(move |_g, x, y| {
        // A `drag-begin` carries the ABSOLUTE press position; updates carry offsets from it. Each axis is
        // taken from the begin UNLESS a caller primed it directly (`set_press_x`, F2.10/F2.11's seam), in
        // which case the primed value stands -- hence the per-axis 0.0 "not primed" test rather than a
        // whole-pair one: a test can prime x and still let the real y through.
        let (px, py) = *pressed_begin.borrow();
        *pressed_begin.borrow_mut() = (if px == 0.0 { x } else { px }, if py == 0.0 { y } else { py });
    });
    let win = window.clone();
    mover.connect_drag_update(move |_g, x, _y| {
        // The press position was stored at the begin above; the update carries only the offset.
        // The stored pair is the press itself when a caller primed it; add nothing to it. Reading it here
        // rather than recomputing keeps the scope fixed at the press even if the pointer wanders.
        let (press_x, press_y) = *pressed_upd.borrow();
        // S1's "either button drags that border": if the press landed within EDGE_GRAB_PX of a drawn
        // clip border, the RIGHT button trims it rather than starting a slide. That is why S2 reads
        // "...the scene slides ... unless on a border" — the border question is answered first, by the
        // same `border_at` the left button uses, so the two sentences cannot contradict each other.
        if let Some((index, border)) = right_press_takes_border(press_x, x) {
            press_trim_border(&win, index, border, (press_x + x) / TRACK_STRIP_PPS);
            if let Some(strip) = track_strip(&win) {
                strip.queue_draw();
            }
            return;
        }
        // Otherwise the right hand moves: seconds travelled since the press, at the strip's zoom.
        press_move_at(&win, press_x, press_y, x, x / TRACK_STRIP_PPS);
    });
    strip.add_controller(mover.clone());
    MOVE_GESTURES.with(|cell| *cell.borrow_mut() = Some(mover));
}

thread_local! {
    static DELETE_KEYS: std::cell::RefCell<Option<gtk::EventControllerKey>> =
        const { std::cell::RefCell::new(None) };
}

/// Add the ⌦ / Delete / BackSpace controller to the window. It claims ONLY those two keys: every other
/// keypress is passed on untouched, so this cannot swallow typing in an entry field.
fn wire_delete_keys(window: &adw::ApplicationWindow) {
    let controller = gtk::EventControllerKey::new();
    let win = window.clone();
    controller.connect_key_pressed(move |_ctrl, key, _code, _mods| {
        if matches!(key, gtk::gdk::Key::Delete | gtk::gdk::Key::BackSpace) {
            press_delete_key(&win);
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(controller.clone());
    DELETE_KEYS.with(|cell| *cell.borrow_mut() = Some(controller));
}

/// This window's preview-volume slider, by its stable name (F2.5 S6). The seam a test fires and the
/// page reads; `None` on a page that drew none.
pub fn preview_volume_scale(window: &adw::ApplicationWindow) -> Option<gtk::Scale> {
    find_widget_by_name(window.upcast_ref(), "preview-volume")?
        .downcast()
        .ok()
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
        // S2/S3: both change what the clock means or what is playing, so both get their sentence.
        cut_play::Pressed::SwitchedToCut { status: line, .. } => {
            if released.is_none() {
                status.set_text(line)
            }
        }
        cut_play::Pressed::ReviewEnded { status: ended } => {
            if released.is_none() {
                status.set_text(*ended)
            }
        }
        cut_play::Pressed::Refused(line) => status.set_text(line),
        // S4: the toggle needs no sentence — the button's own face carries it.
        cut_play::Pressed::Toggled(_) => {}
    }
    pressed
}

/// F2.2: the Cut page's ▶✂ forwards to [`press_play_cut`], taking the window cloned into the
/// closure exactly as [`wire_play_recording`] does.
fn wire_play_cut(button: &gtk::Button, window: &adw::ApplicationWindow) {
    let window = window.clone();
    button.connect_clicked(move |_| {
        // The cut this press plays is the one this window holds — `seed_review_cut`'s slot, read back
        // through `newest_review_cut`. F2.10 S3 hangs on that: ▶ must release a watched row from the
        // same cut the page drew, so pressing with an empty stand-in would refuse instead of handing
        // the preview back.
        let _ = press_play_cut(&window, &newest_review_cut());
    });
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
}

/// The newest cut this window holds, or an empty one when nothing has been seeded.
fn newest_review_cut() -> cut::Cut {
    REVIEW_CUTS
        .with(|slots| slots.borrow().last().map(|slot| slot.borrow().clone()))
        .unwrap_or_default()
}

/// The seam F2.3's ▶✂✂ calls: [`cut_review::pressed`] decides whether the press starts the review from
/// the red line, refuses for want of a join, or pauses-and-ends a running one; this forwards it and
/// paints only what the answer says. Every sentence comes from `cut_review` itself.
pub fn press_review_cuts(window: &adw::ApplicationWindow, cut_: &cut::Cut) -> cut_review::Pressed {
    let Some(status_line) = find_status(window.upcast_ref()) else {
        panic!("the window has no status line");
    };
    let Some(player) = live_player(window) else {
        panic!("the window has no preview player");
    };
    let pressed = cut_review::pressed(&mut player.borrow_mut(), cut_);
    match &pressed {
        // S4: the status names which join out of how many is being heard.
        cut_review::Pressed::Started { status, .. } => status_line.set_text(status),
        // S5: pausing ends the review, and says so with the module's own short line.
        cut_review::Pressed::PausedAndEnded { status } => status_line.set_text(*status),
        // S1: fewer than two clips, in `refused`'s own words.
        cut_review::Pressed::Refused(reason) => status_line.set_text(reason),
    }
    pressed
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
            // S3: the paths were settled before the read, so the swap below happens against the
            // folder just opened. Replacing the shared copy IS the page refresh: Prepare's rows read
            // this same handle, so they show the opened project without being rebuilt here. The
            // migration runs first, because it reads the applied value the swap consumes.
            let migration = open_project::finish(&applied);
            let remembered = REMEMBERED.with(|list| {
                open_project::remember(
                    &mut list.borrow_mut(),
                    &applied.root,
                    &open_project::project_file_in(&applied.out),
                )
            });
            // The lines are taken out before the project is moved into the session, so `applied` is
            // not read after that point.
            let lines = applied.lines.clone();
            let opened = applied
                .root
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "project opened".to_string());
            *session.borrow_mut() = applied.project;
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
            // S4: remember it for this root. Data-level — writing `llm.conf` is settings' job, and
            // the pair is recorded on the window's own list rather than reaching for `$HOME`.
            log_line(&format!(">>> opened {}", remembered.file));
            // A run reading the old project would now be reading files that are not its own, so the
            // stop flag is raised rather than letting it carry on over swapped sources.
            let _ = bar.borrow();
            let opened = applied
                .root
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_else(|| "project opened".to_string());
            status.set_text(&opened);
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

/// What this launch recorded as opened, keyed by root.
pub fn remembered_projects() -> std::collections::BTreeMap<String, String> {
    REMEMBERED.with(|list| list.borrow().clone())
}

/// Where a press of Save goes: [`save_as::press`] decides whether it may happen at all, and the
/// chooser only names the project. Every sentence the user reads comes from [`save_as`].
///
/// As in [`wire_new`], the open project is the working copy beside the root until F0.9 keeps the
/// window's own project path; a run under way is F0.2's [`run::RunBar`] state.
fn wire_save(save_: &gtk::Button, bar: &Rc<RefCell<run::RunBar>>, status: &gtk::Label, project: &Project) {
    let bar = bar.clone();
    let status = status.clone();
    let project = project.clone();
    save_.connect_clicked(move |_| {
        if let Err(reason) = save_as::press(bar.borrow().running.is_some()) {
            status.set_text(reason);
            log_line(reason);
            return;
        }
        ask_save_project(&status, &project, &startup::session_dir(&std::env::current_dir().unwrap_or_default()));
    });
}

/// S2: the dialog that names the project. `select_folder` rather than `save_folder` because a
/// project is a folder (01 §1) and a file chooser would insist on a file; the name typed in the
/// sheet is what S3 renames to, so it starts as the open folder's own name.
fn ask_save_project(status: &gtk::Label, project: &Project, open: &Path) {
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
    let project = project.clone();
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
            match save_as::save_as(&project, &open, &named) {
                Ok(saved) => {
                    for line in &saved.logs {
                        log_line(line);
                    }
                    status.set_text(&saved.status);
                }
                // The `!!!` sentence is a log line as much as the status line's: it says where the
                // files still are, which is what F0.5's log will be for.
                Err(reason) => {
                    status.set_text(&reason);
                    log_line(&reason);
                }
            }
        },
    );
}

thread_local! {
    /// The window's log lines, standing in for F0.5's log expander: `>>> moved the output folder
    /// to …` and the `!!!` refusals have nowhere to be drawn until that row exists, and a test has
    /// to be able to see that they were said at all.
    static LOGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

pub(crate) fn log_line(line: &str) {
    LOGS.with(|logs| logs.borrow_mut().push(line.to_string()));
}

/// Every line the window has logged this session, oldest first.
pub fn window_logs() -> Vec<String> {
    LOGS.with(|logs| logs.borrow().clone())
}

/// The switching state as the window currently shows it — for the tests that drive a real click.
pub fn state(window: &adw::ApplicationWindow) -> UiState {
    let (Some(switcher), Some(status)) = (find_switcher(window.upcast_ref()), find_status(window.upcast_ref()))
    else {
        panic!("the window has no tab row");
    };
    let page = switcher
        .stack()
        .and_then(|stack| stack.visible_child_name().map(|name| name.to_string()))
        .unwrap_or_default();
    UiState {
        page: Page::all()
            .into_iter()
            .find(|candidate| candidate.label() == page)
            .unwrap_or(Page::Prepare),
        status: status.text().to_string(),
        log_expanded: log_expander(window).is_some_and(|expander| expander.is_expanded()),
        progress: progress_bar(window)
            .map(|bar| bar.fraction())
            .unwrap_or(0.0),
    }
}

/// The tooltip the tab button for `page` carries, which is how a test reads §1's "tooltip = the
/// reason" without reaching inside the switcher itself.
pub fn tab_tooltip(window: &adw::ApplicationWindow, page: Page) -> Option<String> {
    tab_button(window, page)?.tooltip_text().map(|text| text.to_string())
}

/// Whether the tab for `page` is greyed (§1's "greyed, not disabled" — the class, never `set_sensitive`).
pub fn tab_dimmed(window: &adw::ApplicationWindow, page: Page) -> bool {
    tab_button(window, page)
        .is_some_and(|button| button.has_css_class("dim-label"))
}

/// The tab button drawing `page`'s word, so a test can click it the way a user does.
pub fn tab_button(window: &adw::ApplicationWindow, page: Page) -> Option<gtk::Button> {
    let switcher = find_switcher(window.upcast_ref())?;
    tab_buttons(switcher.upcast_ref())
        .into_iter()
        .filter(|button| has_label(button, page.label()))
        .max_by_key(|button| depth(button))?
        .downcast()
        .ok()
}

/// §1's badge **14** — the visible tab's `Inputs:` readout, by its stable name.
pub fn inputs_readout(window: &adw::ApplicationWindow) -> Option<gtk::Label> {
    find_widget_by_name(window.upcast_ref(), "inputs-readout")?
        .downcast()
        .ok()
}

/// §1's badge **15** — the `Outputs:` count beside its folder button.
pub fn outputs_readout(window: &adw::ApplicationWindow) -> Option<gtk::Label> {
    find_widget_by_name(window.upcast_ref(), "outputs-readout")?
        .downcast()
        .ok()
}

/// §1's badge **15** — the folder button that goes with the Outputs count.
pub fn outputs_folder_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "outputs-folder-button")?
        .downcast()
        .ok()
}

/// Mark this window's narration write as owed — the state a half-typed line leaves behind.
///
/// S3 owns the decision whether leaving the tab writes (`shell::Pending::owe`); this only raises
/// the flag, through the same type the switch reads. The Narrate page has no text view yet, so that
/// page's typing cannot raise it — this is the seam F4.7 will call, and what `switch_tab_widgets`
/// drives in its place.
pub fn mark_narration_owed(window: &adw::ApplicationWindow) {
    let _ = window;
    if let Some(shell) = WINDOW_SHELLS.with(|shells| shells.borrow().last().cloned()) {
        // `now` is irrelevant here: leaving the tab writes whatever is owed, even a beat early.
        shell.borrow_mut().narration_pending.touched(std::time::Duration::from_secs(0));
    }
}

/// The narration this window is holding — newest slot wins, the same rule [`tab_info_button`] uses,
/// so a test or a flow reaches the lines of the window that was built last rather than one left over
/// from an earlier window in the same process. A window with nothing held reads as empty narration,
/// which is what a project with no `narration.json` shows anyway.
fn held_narration() -> Narration {
    HELD_NARRATION
        .with(|held| held.borrow().last().cloned())
        .map(|held| held.borrow().clone())
        .unwrap_or_default()
}

/// §1's badge **6** — the header ⓘ (not the per-page copies), newest slot wins so a test reads the
/// button this window built rather than one left over from an earlier window in the same process.
pub fn tab_info_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    let _ = window;
    INFO_BUTTON.with(|slots| slots.borrow().last().cloned())
}

/// The switcher inside the window: it is the one thing in here with a stack of its own.
fn find_switcher(root: &gtk::Widget) -> Option<adw::ViewSwitcher> {
    if let Some(switcher) = root.downcast_ref::<adw::ViewSwitcher>() {
        return Some(switcher.clone());
    }
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
        if let Some(found) = find_switcher(&widget) {
            return Some(found);
        }
    }
    None
}

/// The Save button, named so a test can press it the way a user does.
pub fn save_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "save-button")?
        .downcast()
        .ok()
}

/// The tooltip Save carries, which is how a test reads §1's wording for badge **3**.
pub fn save_tooltip(window: &adw::ApplicationWindow) -> Option<String> {
    save_button(window)?.tooltip_text().map(|text| text.to_string())
}

/// The Rescan button, named so a test can press it the way a user does.
pub fn rescan_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "rescan-button")?
        .downcast()
        .ok()
}

/// One widget from Prepare's sources list, found by the name §4's rows are built with — `sources-list`,
/// `source-row-<i>`, `footage-<i>`, `narrator-<i>`, `remove-<i>`, `source-name-<i>`. One accessor for
/// the whole list rather than six, because the row count is the project's business: a test asking for
/// `remove-7` of a two-row session gets `None`, which is the honest answer.
pub fn find_source_widget(window: &adw::ApplicationWindow, name: &str) -> Option<gtk::Widget> {
    find_widget_by_name(window.upcast_ref(), name)
}

/// The Open button (F0.9), found the same way as [`play_button`].
pub fn open_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "open-button")?
        .downcast()
        .ok()
}

/// The sources of the project this window currently holds. `build_window` keeps its own `Rc`, so the
/// handle is published here at build time for a test to read back — the same copy the Prepare page's
/// rows mutate, not a snapshot of it.
/// The live session — the last window's handle, which is the one this check just opened into. Each
/// `build_window` publishes its own, so reading the newest avoids mixing in earlier windows' copies.
pub fn session_sources(window: &adw::ApplicationWindow) -> Vec<String> {
    let _ = window;
    SESSION.with(|slots| {
        slots
            .borrow()
            .last()
            .map(|project| {
                project
                    .borrow()
                    .sources
                    .iter()
                    .map(|source| source.path.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    })
}

/// The tooltip Open carries, which is how a test reads §1's wording for badge **2**.
pub fn open_tooltip(window: &adw::ApplicationWindow) -> Option<String> {
    open_button(window)?.tooltip_text().map(|text| text.to_string())
}

/// The run's progress bar (F0.5 S2), found the same way as [`play_button`]. Window-level, so a name
/// lookup cannot land on a per-page copy.
pub fn progress_bar(window: &adw::ApplicationWindow) -> Option<gtk::ProgressBar> {
    find_widget_by_name(window.upcast_ref(), "run-progress")?
        .downcast()
        .ok()
}

/// The log expander (F0.5 S1), whose header carries the status line and whose body holds the run's
/// lines. `is_expanded()` is what `state().log_expanded` reports.
pub fn log_expander(window: &adw::ApplicationWindow) -> Option<gtk::Expander> {
    find_widget_by_name(window.upcast_ref(), "log-expander")?
        .downcast()
        .ok()
}

/// The tooltip the progress bar carries — where F0.5 puts the counting ("task 4 of 12, 8 waiting").
pub fn progress_tooltip(window: &adw::ApplicationWindow) -> Option<String> {
    progress_bar(window)?.tooltip_text().map(|text| text.to_string())
}

/// The tooltip Rescan carries, which is how a test reads §1's wording for badge **8**.
pub fn rescan_tooltip(window: &adw::ApplicationWindow) -> Option<String> {
    rescan_button(window)?.tooltip_text().map(|text| text.to_string())
}

/// Prepare's "Add source files…" button, named so a test can press it the way a user does.
pub fn add_sources_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "add-sources-button")?
        .downcast()
        .ok()
}

/// The "copy into project" tick beside it (F0.12 S2), which is what the press reads to decide
/// between a copy and a reference.
pub fn copy_into_project(window: &adw::ApplicationWindow) -> Option<gtk::CheckButton> {
    find_widget_by_name(window.upcast_ref(), "copy-into-project")?
        .downcast()
        .ok()
}

/// The ASR code the page's Language box holds (§1's badge **10**) — how a test sees that typing
/// reached [`Project::language`] rather than stopping at the widget.
pub fn bench_language(window: &adw::ApplicationWindow) -> Option<String> {
    let entry = find_widget_by_name(window.upcast_ref(), "language-entry")?
        .downcast::<gtk::Entry>()
        .ok()?;
    Some(entry.text().to_string())
}

/// Freq as the live session holds it, read off `Project::interval` rather than off the dropdown's
/// selected index. A widget test needs the SAME field `prepare::set_freq` writes, otherwise it
/// would only prove the dropdown remembers its own selection.
pub fn session_freq(window: &adw::ApplicationWindow) -> f64 {
    let _ = window;
    SESSION.with(|slots| {
        slots
            .borrow()
            .last()
            .map(|project| project.borrow().interval)
            .unwrap_or(naivepost_interval_default())
    })
}

thread_local! {
    /// The run bar `build_window` created, published so a test can read the step it holds. Same
    /// reason as [`SESSION`]: the bar lives inside the window's closure and nothing else can reach it.
    static RUN_BARS: std::cell::RefCell<Vec<Rc<std::cell::RefCell<crate::run::RunBar>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

thread_local! {
    /// The live session the newest window works on, handed to the ▶ handler so F1.1's refusal asks
    /// about what the page holds now rather than what `build_window` was called with.
    static PLAY_SESSION: std::cell::RefCell<Vec<Rc<std::cell::RefCell<Project>>>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// The step the run bar is holding, if any — read off `RunBar::running` rather than off a button's
/// face. A widget test needs the same field `runqueue::start_run` writes and F1.1's refusal skips,
/// otherwise it would only prove that ⏹ looked sensitive.
pub fn running_step(window: &adw::ApplicationWindow) -> Option<crate::run::Step> {
    let _ = window;
    RUN_BARS.with(|bars| {
        bars.borrow()
            .last()
            .and_then(|bar| bar.borrow().running.as_ref().map(|run| run.step))
    })
}

/// What a project with no opinion carries, reached through the type rather than retyped here so the
/// two cannot drift apart.
fn naivepost_interval_default() -> f64 {
    crate::project::Project::default().interval
}

/// The marking pass the LIVE session runs under — the same field `▶` reads when it decides whether
/// F1.9's retake pass or F1.10's joins pass speaks. Read from `PLAY_SESSION`, not from a window
/// widget: the policy has no control of its own on this page (it lives in the gear), so the session
/// is the truth and a test that read anything else would be reading a copy.
pub fn marking_pass(_window: &adw::ApplicationWindow) -> MarkingPass {
    PLAY_SESSION
        .with(|slots| slots.borrow().last().cloned())
        .map(|session| session.borrow().policy.marking_pass.value)
        .unwrap_or(MarkingPass::Retakes)
}

/// Move the live session's marking pass, as saving a project that names Joins would.
///
/// This is the seam a person's saved project arrives through: `build_window` publishes the argument
/// into `PLAY_SESSION`, and F0.7's derive door may then replace it. A test that wants to know which
/// pass a press will run changes it HERE, after the window exists, because changing the build-time
/// argument can be overwritten before the handler ever sees it.
pub fn set_marking_pass(window: &adw::ApplicationWindow, pass: MarkingPass) {
    let _ = window;
    if let Some(session) = PLAY_SESSION.with(|slots| slots.borrow().last().cloned()) {
        session.borrow_mut().policy.marking_pass.value = pass;
    }
}



/// The New button, named so a test can press it the way a user does.
pub fn new_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "new-button")?
        .downcast()
        .ok()
}

/// The tooltip ＋ New carries, which is how a test reads §1's wording for badge **1**.
pub fn new_tooltip(window: &adw::ApplicationWindow) -> Option<String> {
    new_button(window)?.tooltip_text().map(|text| text.to_string())
}

/// The ▶ button, named rather than searched for by icon: the icon changes to ⏸ while a run works,
/// and an unnamed busy button would then be findable no more.
pub fn play_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "play-button")?.downcast().ok()
}

/// The ⏹ button (F0.3), found the same way as [`play_button`].
pub fn stop_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "stop-button")?.downcast().ok()
}

/// The "I'm feeling lucky" button (F0.4), found the same way as [`play_button`].
pub fn lucky_button(window: &adw::ApplicationWindow) -> Option<gtk::Button> {
    find_widget_by_name(window.upcast_ref(), "lucky-button")?.downcast().ok()
}

/// The tooltip ▶ carries, which is how a test reads §2's wording without reaching into the button.
pub fn play_tooltip(window: &adw::ApplicationWindow) -> Option<String> {
    play_button(window)?.tooltip_text().map(|text| text.to_string())
}

/// The first widget under `root` carrying `name`, found the same walk as [`find_status`].
pub(crate) fn find_widget_by_name(root: &gtk::Widget, name: &str) -> Option<gtk::Widget> {
    if root.widget_name() == name {
        return Some(root.clone());
    }
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
        if let Some(found) = find_widget_by_name(&widget, name) {
            return Some(found);
        }
    }
    None
}

/// The status line, named rather than searched for by text: an empty label is the ordinary state and
/// would match any label in the tree. Public so a widget test can read the sentence a press printed —
/// the page's one output channel, and the thing F2.7 pins.
pub fn find_status(root: &gtk::Widget) -> Option<gtk::Label> {
    if root.widget_name() == "status-line" {
        return root.downcast_ref::<gtk::Label>().cloned();
    }
    for child in root.observe_children().iter::<glib::Object>() {
        let Ok(child) = child else { continue };
        let Ok(widget) = child.downcast::<gtk::Widget>() else { continue };
        if let Some(found) = find_status(&widget) {
            return Some(found);
        }
    }
    None
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

/// Record that the page has been edited, so Undo/Redo/Revert/Clear have something to answer about.
/// The verb doors print and write segments but do not own the history (§2's snapshots are the edit
/// record); this is the seam that pushes one, and the same one `note_edit` in the test stands for.
pub fn note_edit(window: &adw::ApplicationWindow) {
    let history = cut_history(window);
    history.borrow_mut().push(&newest_review_cut());
    refresh_history_buttons(window);
}

/// This window's history, opened on the cut it currently shows if none was opened yet.
fn cut_history(window: &adw::ApplicationWindow) -> Rc<std::cell::RefCell<cut::History>> {
    let held = CUT_HISTORIES.with(|slot| slot.borrow().last().cloned());
    if let Some(held) = held {
        return held;
    }
    let fresh = Rc::new(std::cell::RefCell::new(cut::History::open(&newest_review_cut())));
    CUT_HISTORIES.with(|slot| slot.borrow_mut().push(fresh.clone()));
    fresh
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
            let said = format!(
                "\u{21b6} back to the previous state \u{2014} {} segment(s), {} \u{2014} Redo puts it forward again",
                cut_.segs.len(),
                crate::tools::mm_ss(cut_screen::cut_seconds(&cut_))
            );
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
            let said = format!(
                "\u{21a7} forward again \u{2014} {} segment(s), {}",
                cut_.segs.len(),
                crate::tools::mm_ss(cut_screen::cut_seconds(&cut_))
            );
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
        let said = "nothing to revert \u{2014} the cut is as it was".to_string();
        log_line(&said);
        return said;
    }
    let snapshot = history.borrow_mut().revert();
    let mut cut_ = newest_review_cut();
    snapshot.restore(&mut cut_);
    publish_cut(&cut_);
    let said = format!(
        "back to where this page started \u{2014} {} segment(s). \u{21b6} Undo cannot reach the hand edits \
         you just dropped: they are gone",
        cut_.segs.len()
    );
    log_line(&said);
    said
}

/// Clear (§1 item 21): every kept stretch and every effect off the timeline, recordings as loaded.
/// It is itself an edit, so Undo brings them back — which is why the result is pushed rather than
/// simply written.
pub fn press_clear_cut(window: &adw::ApplicationWindow) -> String {
    let had = newest_review_cut();
    if had.segs.is_empty() && had.fx.is_empty() {
        let said = "nothing to clear \u{2014} the timeline is already empty".to_string();
        log_line(&said);
        return said;
    }
    let cleared = crate::cut::cleared(&had);
    let history = cut_history(window);
    history.borrow_mut().push(&cleared);
    publish_cut(&cleared);
    let said = format!(
        "cleared {} segment(s) and {} effect(s) \u{2014} the recordings stay as they were loaded, and \
         \u{21b6} Undo brings them back",
        had.segs.len(),
        had.fx.len()
    );
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

/// − / + (§1 item 22): one step of the zoom ladder. The floor is where the whole session fits, so
/// zooming out stops there rather than at a number that would leave the timeline in a corner.
pub fn press_zoom(window: &adw::ApplicationWindow, inward: bool) -> String {
    let pps = preview_pps();
    let next = if inward {
        cut_screen::zoom_up(pps, TRACK_STRIP_PPS)
    } else {
        cut_screen::zoom_down(pps, TRACK_STRIP_PPS)
    };
    set_preview_pps(next);
    let said = format!("zoom {:.1} px a second", next);
    log_line(&said);
    said
}

thread_local! {
    /// The page's pixels-per-second. Seeded at the placeholder strip's zoom so the toolbar's −/+ start
    /// from where the page actually draws; F2.10/F2.11 move the real tracks onto this same number.
    static PREVIEW_PPS: std::cell::RefCell<f64> = const { std::cell::RefCell::new(0.0) };
}

fn preview_pps() -> f64 {
    PREVIEW_PPS.with(|cell| {
        let held = *cell.borrow();
        if held > 0.0 { held } else { TRACK_STRIP_PPS }
    })
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

/// Which of the four history buttons may be pressed right now, from the stack's own counts. Called
/// from [`refresh_selection_readout`] so a greyed history button is today's answer along with every
/// other greyed control on the page, never a leftover from the last draw.
fn refresh_history_buttons(window: &adw::ApplicationWindow) {
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
