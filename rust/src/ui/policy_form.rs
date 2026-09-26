//! F0.7 S5 — the policy form: every field with its value, its source and the reason it has one.
//!
//! The shape is [`crate::settings`]'s (an `adw::Window` transient for the main one, a grid of labelled
//! rows built by one closure) because that dialog already solved the layout this page needs; what differs
//! is that nothing here is editable. The form **shows** the policy: who set each value and why, so a
//! proposal can be judged rather than merely read. Editing a field is a person's own action on the
//! project file and arrives with its own item — inventing a control that looks editable but writes
//! `source: user` behind a half-built path would be worse than not having it.
//!
//! Every row comes from [`policy::derived_fields`], read at build time off the live [`Policy`]. That is
//! deliberate: the same list drives validation in `policy::apply`, so a field cannot exist in the form
//! without being settable, or be settable without appearing here.

use adw::prelude::*;
use gtk4 as gtk;

use crate::policy::{self, CatalogueRow};
use crate::project::{CutMode, MarkingPass, Origin, Policy};

/// The widget-name prefix of every row part, so names are built from one place rather than scattered
/// literals: `policy-form`, `policy-row-<field>`, `policy-value-<field>`, `policy-source-<field>`,
/// `policy-reason-<field>`.
const NAME_PREFIX: &str = "policy";

/// The form's title, §03's label for the screen.
pub const TITLE: &str = "Editing policy";

/// The tooltip both openers carry — the ⚙ on the run bar and each tab's ⓘ say the same thing about
/// where they lead. A function rather than only a constant so a test can pin the two buttons against one
/// source of truth instead of retyping the string.
pub const OPEN_TIP: &str = "Editing policy \u{2014} every field, who set it and why";

/// [`OPEN_TIP`] as an accessor, for callers that should not care whether it is a const or a computed
/// string. Same bytes, one definition.
pub fn tip() -> &'static str {
    OPEN_TIP
}

/// The three sources, spelled once. A `format!("{origin:?}")` would print `Default`/`Model`/`User`
/// capitalised the way Rust spells variants; the form shows the lowercase words the project file uses,
/// which is what a person comparing the two should see.
pub fn origin_word(origin: Origin) -> &'static str {
    match origin {
        Origin::User => "user",
        Origin::Model => "model",
        Origin::Default => "default",
    }
}

/// The value as the form prints it. Each type gets its own arm so a new variant forces a decision here
/// rather than silently rendering `{:?}` of something nobody proofread.
pub fn value_word(policy: &Policy, field: &str) -> String {
    match field {
        "markingPass" => match policy.marking_pass.value {
            // P.policy.markingPass — named by what runs, so the word matches the flow it picks.
            MarkingPass::Joins => "joins".to_string(),
            MarkingPass::Retakes => "retakes".to_string(),
            MarkingPass::None => "none".to_string(),
        },
        "cutMode" => match policy.cut_mode.value {
            // P.policy.cutMode
            CutMode::Words => "words".to_string(),
            CutMode::Model => "model".to_string(),
        },
        "captionsPass" => pass_word(policy.captions_pass.value), // P.policy.captionsPass
        "speedPass" => pass_word(policy.speed_pass.value), // P.policy.speedPass
        "decorationsPass" => pass_word(policy.decorations_pass.value), // P.policy.decorationsPass
        // Unreachable through `derived_fields`; a hand-built name would show an empty cell rather than
        // panic, because a blank row on a form is a visible bug, not a crash.
        _ => String::new(),
    }
}

fn pass_word(on: bool) -> String {
    if on { "on".to_string() } else { "off".to_string() }
}

/// The reason column's width, in characters. §10 spells the longest shipped `because` in about thirty
/// characters ("three or four per five minutes"), so 36 wraps at a word rather than mid-sentence and
/// keeps the form's height near the picture's.
const BECAUSE_MAX_CHARS: i32 = 36;

/// Build the form for this policy. `parent` is the main window, so the form rides with it and closes
/// when dismissed.
pub fn build(parent: Option<&gtk::Window>, policy: &Policy) -> adw::Window {
    let window = adw::Window::new();
    window.set_title(Some(TITLE));
    window.set_modal(true);
    window.set_transient_for(parent);
    window.set_default_width(780);

    let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    root.set_widget_name(&format!("{NAME_PREFIX}-form"));
    root.set_margin_start(18);
    root.set_margin_end(18);
    root.set_margin_top(12);
    root.set_margin_bottom(12);

    let heading = gtk::Label::new(Some(TITLE));
    heading.add_css_class("title-3");
    heading.set_halign(gtk::Align::Start);
    root.append(&heading);

    // One line saying what the whole page is, because the fields alone do not explain that a `model`
    // row came from the User Context and a `user` row will never be touched again.
    let note = gtk::Label::new(Some(
        "Derived from the User Context. A field you set yourself is never overwritten by a later derivation.",
    ));
    note.add_css_class("dim-label");
    note.set_wrap(true);
    note.set_halign(gtk::Align::Start);
    root.append(&note);

    let grid = gtk::Grid::new();
    grid.set_row_spacing(8);
    grid.set_column_spacing(12);
    grid.set_hexpand(true);
    root.append(&grid);

    // Column captions, so the four columns read as four answers instead of loose labels. Lowercase as
    // `img/03-policy-form.svg` spells them.
    for (caption, col) in [("field", 1i32), ("value", 2), ("source", 3), ("because", 4)] {
        let head = gtk::Label::new(Some(caption));
        // Named so a test reads the four captions rather than scraping the grid's children.
        head.set_widget_name(&format!("{NAME_PREFIX}-caption-{caption}"));
        head.add_css_class("heading");
        head.set_halign(gtk::Align::Start);
        grid.attach(&head, col, 0, 1, 1);
    }

    let mut row = 1i32;
    for entry in policy::derived_fields() {
        add_row(&grid, policy, entry, row);
        wrap_row(&grid, entry.field, row);
        row += 1;
    }

    // The picture's footer: this page is the whole §2 catalogue, of which F0.7 derives the five above.
    let footer = gtk::Label::new(Some(
        "... every field of 10-parameters.md \u{a7}2",
    ));
    footer.set_widget_name(&format!("{NAME_PREFIX}-footer"));
    footer.add_css_class("dim-label");
    footer.set_halign(gtk::Align::Start);
    root.append(&footer);

    // The two buttons the picture puts bottom-right. `Reset to defaults` goes through
    // `policy::reset_to_defaults` and repaints in place rather than rebuilding, so a person sees the
    // fields move back without the window flickering shut.
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    // `Re-derive` is F0.7's third door — "on demand from the policy form". Without it a person who
    // just edited the User Context and opened ⚙ has to wait for the debounce or press ▶ to see the
    // answer.
    let redrive = gtk::Button::with_label("Re-derive");
    redrive.set_widget_name(&format!("{NAME_PREFIX}-redrive-button"));
    let reset = gtk::Button::with_label("Reset to defaults");
    reset.set_widget_name(&format!("{NAME_PREFIX}-reset-button"));
    let close = gtk::Button::with_label("Close");
    close.set_widget_name(&format!("{NAME_PREFIX}-close-button"));
    buttons.append(&redrive);
    buttons.append(&reset);
    buttons.append(&close);
    root.append(&buttons);

    wire_buttons(&window, &redrive, &reset, &close);

    window.set_content(Some(&root));
    window
}

/// One field: its name and id, its value, its source, and the reason beside them.
///
/// Four columns, as `img/03-policy-form.svg` draws them — one grid row per field. The reason used to
/// sit under its row at `(1, row + 1)` while the caller advanced `row` by one, which stacked every
/// field's name on top of the previous field's reason; the picture has always had four columns.
fn add_row(grid: &gtk::Grid, policy: &Policy, entry: CatalogueRow, row: i32) {
    let name = gtk::Label::new(Some(entry.field));
    name.set_halign(gtk::Align::Start);
    name.set_tooltip_text(Some(entry.id));
    grid.attach(&name, 1, row, 1, 1);

    let value = gtk::Label::new(Some(&value_word(policy, entry.field)));
    value.set_widget_name(&format!("{NAME_PREFIX}-value-{}", entry.field));
    value.set_halign(gtk::Align::Start);
    value.add_css_class("title-4");
    grid.attach(&value, 2, row, 1, 1);

    let origin = field_origin(policy, entry.field);
    let source = gtk::Label::new(Some(origin_word(origin)));
    source.set_widget_name(&format!("{NAME_PREFIX}-source-{}", entry.field));
    source.set_halign(gtk::Align::Start);
    // A model-derived value is the one a person may want to overrule, so it is the one worth dimming
    // less than the rest; `default` and `user` are both settled and read quieter.
    if origin != Origin::Default {
        source.add_css_class("accent");
    }
    grid.attach(&source, 3, row, 1, 1);

    // The reason is the fourth column, wrapped: it is a sentence and a column of them would turn into
    // unreadable ribbons. Blank for a default and for a hand-set field: neither has a reason to justify
    // itself (§S3 stores `because` only for what the model set).
    let reason_text = reason_of(policy, entry.field).unwrap_or_else(|| match origin {
        Origin::Model => "(no reason given)".to_string(),
        Origin::User => "set by you".to_string(),
        Origin::Default => "the default".to_string(),
    });
    let reason = gtk::Label::new(Some(&reason_text));
    reason.set_widget_name(&format!("{NAME_PREFIX}-reason-{}", entry.field));
    reason.set_halign(gtk::Align::Start);
    reason.set_wrap(true);
    reason.set_max_width_chars(BECAUSE_MAX_CHARS);
    reason.add_css_class("dim-label");
    grid.attach(&reason, 4, row, 1, 1);
}

/// The reset and close buttons. Both act on the newest session's policy rather than on the copy this
/// form was built from: the session is what Save writes, so a reset that only moved a stale clone
/// would look applied and be lost.
fn wire_buttons(
    window: &adw::Window,
    redrive: &gtk::Button,
    reset: &gtk::Button,
    close: &gtk::Button,
) {
    let form = window.clone();
    close.connect_clicked(move |_| {
        // `close()` runs the delete path, which unmaps the window right away; the slot that lets
        // `press_policy` raise this same form instead of building a second one is dropped in the
        // `close-request` handler below, because GTK emits that signal synchronously and would be
        // re-entrant if the list were mutated from `close-request`'s own default handler.
        form.close();
    });
    let forget = window.clone();
    window.connect_close_request(move |_| {
        crate::ui::forget_policy_form(&forget);
        glib::Propagation::Proceed
    });
    let refresh = window.clone();
    redrive.connect_clicked(move |_| {
        // S2-S4 now, through the same seam ▶ uses; then repaint so the three columns show what the
        // derivation just decided rather than what was on screen when the form opened.
        let derived = crate::ui::derive_policy(&crate::ui::main_window());
        crate::ui::refresh_policy_form(&refresh);
        let _ = derived;
    });
    reset.connect_clicked(move |_| {
        crate::ui::reset_policy_to_defaults();
    });
}

/// Read the stored `because` for a field by name, so the form needs no knowledge of which arm holds it.
fn reason_of(policy: &Policy, field: &str) -> Option<String> {
    let because = match field {
        "markingPass" => policy.marking_pass.because.clone(),
        "cutMode" => policy.cut_mode.because.clone(),
        "captionsPass" => policy.captions_pass.because.clone(),
        "speedPass" => policy.speed_pass.because.clone(),
        "decorationsPass" => policy.decorations_pass.because.clone(),
        _ => None,
    };
    because.filter(|text| !text.trim().is_empty())
}

/// The reason line shown for a field: the stored `because`, or what the source says when there is none.
/// Public so the in-place refresh in `window::refresh_form` prints exactly what a fresh build would.
pub fn reason_word(policy: &Policy, field: &str) -> String {
    let origin = field_origin(policy, field);
    reason_of(policy, field).unwrap_or_else(|| match origin {
        Origin::Model => "(no reason given)".to_string(),
        Origin::User => "set by you".to_string(),
        Origin::Default => "the default".to_string(),
    })
}

/// The origin of a field by name — the same lookup `policy::apply` does, read here for display.
pub fn field_origin(policy: &Policy, field: &str) -> Origin {
    match field {
        "markingPass" => policy.marking_pass.origin,
        "cutMode" => policy.cut_mode.origin,
        "captionsPass" => policy.captions_pass.origin,
        "speedPass" => policy.speed_pass.origin,
        "decorationsPass" => policy.decorations_pass.origin,
        _ => Origin::Default,
    }
}

/// Give the row a widget of its own to name.
///
/// `gtk::Grid` has no row object, so the row is carried by a CSS-free marker box attached beside the
/// cells at column 0 — the one column nothing else uses. Naming that box is what makes
/// `policy-row-markingPass` findable without rebuilding the grid as nested boxes, which would lose the
/// column alignment the three answers depend on.
/// Give the row a widget of its own to name.
///
/// `gtk::Grid` has no row object, so the row is carried by a CSS-free marker box attached beside the
/// cells at column 0 — the one column nothing else uses. Naming that box is what makes
/// `policy-row-markingPass` findable without rebuilding the grid as nested boxes, which would lose the
/// column alignment the four answers depend on. It asks for zero width: the row's identity is the
/// point, and a marker with a width of its own shoves the field names right out of their column.
fn wrap_row(grid: &gtk::Grid, field: &str, row: i32) {
    let marker = gtk::Box::new(gtk::Orientation::Vertical, 0);
    marker.set_widget_name(&format!("{NAME_PREFIX}-row-{field}"));
    marker.set_size_request(0, 1);
    grid.attach(&marker, 0, row, 1, 1);
}
