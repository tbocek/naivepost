pub mod cut;
pub mod layout;
pub mod narration;
pub mod project;
pub mod publish;
pub mod requests;
pub mod snapshot;
pub mod textfmt;
pub mod wave;
pub mod ui;

/// The four pages, in the order the user works through them (spec/00-principles.md §1).
pub const PAGES: [&str; 4] = ["Prepare", "Cut", "Narrate", "Produce"];
