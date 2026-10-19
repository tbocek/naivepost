//! The one integration test binary: every test file is a module under `suite/`, so a change relinks
//! one binary instead of one per file. `just test` still runs each file in a process of its own
//! (`all suite::<file>::`), as when each was its own binary: a widget test starts a whole application
//! and leaves process-wide state (cwd, environment, GTK, the app's own statics) behind.

#[allow(dead_code)] // the files use different subsets of the helpers
mod common;
mod suite;
