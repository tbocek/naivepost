//! The four tab buttons (and every other `-symbolic` control) are drawn from the desktop's icon
//! theme: `gui/icons` ships the app icon only. On a machine whose theme cannot resolve those names
//! GTK draws the broken-icon picture in every button — which is what a release showed and a checkout
//! never does, because the checkout runs where a theme is installed.
//!
//! So the names are spelled once ([`icons::NAMED_SYMBOLIC_ICONS`]) and checked against a tree the way
//! GTK looks them up ([`icons::missing_in_for_test`]), without a display.

use std::fs;
use std::path::{Path, PathBuf};

use crate::common;
use naivepost::icons;

/// The icon tree the checkout ships: the app icon and nothing else.
fn repo_icons() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../gui/icons")
}

/// One theme folder of a fake tree, with its `index.theme` and the icons named below.
fn theme(root: &Path, name: &str, folders: &[(&str, &[&str])]) -> PathBuf {
    let theme = root.join(name);
    let directories: Vec<String> = folders.iter().map(|(dir, _)| dir.to_string()).collect();
    let mut index = format!(
        "[Icon Theme]\nName={name}\nDirectories={}\n",
        directories.join(";")
    );
    for (dir, icons) in folders {
        let path = theme.join(dir);
        fs::create_dir_all(&path).unwrap();
        index.push_str(&format!("[{dir}]\nSize=16\nContext=symbolic\n"));
        for icon in *icons {
            fs::write(path.join(format!("{icon}.svg")), "<svg/>").unwrap();
        }
    }
    fs::write(theme.join("index.theme"), index).unwrap();
    theme
}

#[test]
fn every_name_the_interface_asks_for_is_a_symbolic_name() {
    // The list is the app's whole ask of the theme; a stray non-symbolic name in it would be a
    // different lookup rule (a full-colour icon is found in `apps/`, not `symbolic/`).
    assert!(icons::NAMED_SYMBOLIC_ICONS.contains(&"view-list-symbolic"));
    assert!(icons::NAMED_SYMBOLIC_ICONS.contains(&"applications-multimedia-symbolic"));
    assert!(
        icons::NAMED_SYMBOLIC_ICONS
            .iter()
            .all(|name| name.ends_with("-symbolic")),
        "{:?}",
        icons::NAMED_SYMBOLIC_ICONS
    );
}

#[test]
fn the_names_are_the_ones_the_code_asks_for() {
    // The list is only worth having if it is the code's own ask. Walked off `src/` rather than
    // trusted: the release bug was a list nobody kept honest.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let asked = symbolic_names_in(&root);
    assert!(
        !asked.is_empty(),
        "the walk found no symbolic name in src/, so it proves nothing"
    );
    let mut missing: Vec<&str> = asked
        .iter()
        .filter(|name| !icons::NAMED_SYMBOLIC_ICONS.contains(&name.as_str()))
        .map(|name| name.as_str())
        .collect();
    missing.sort();
    assert!(
        missing.is_empty(),
        "widgets ask for {missing:?}, which nothing checks"
    );
}

/// Every `"...-symbolic"` string literal under `dir`, recursively, deduplicated and sorted.
fn symbolic_names_in(dir: &Path) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for entry in fs::read_dir(dir).expect("src/ is readable") {
        let path = entry.expect("an entry").path();
        if path.is_dir() {
            found.extend(symbolic_names_in(&path));
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let text = fs::read_to_string(&path).expect("a readable .rs file");
        for word in text.split('"').skip(1) {
            // Odd-indexed pieces are inside a string literal; `"-symbolic"` never appears in a
            // comment or a macro here, so no other spelling of a name is counted.
            if word.ends_with("-symbolic") && !word.contains(' ') {
                found.push(word.to_string());
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

#[test]
fn a_tree_with_no_theme_resolves_none_of_them() {
    // The app-icon tree alone: `hicolor/` holds `ch.bocek.naivepost`, which is not one of these names,
    // and it carries no `index.theme`, so it is not even a theme. This is what the AppImage used to
    // ship, and why every tab drew the broken-icon picture.
    assert_eq!(icons::themes(&repo_icons()), vec![] as Vec<PathBuf>);
    assert_eq!(
        icons::missing_in_for_test(&repo_icons()).len(),
        icons::NAMED_SYMBOLIC_ICONS.len()
    );
}

#[test]
fn a_theme_resolves_the_names_it_holds_and_only_those() {
    let root = common::temp_dir_tagged("one-theme");
    let icons_dir = root.join("icons");
    let held: Vec<&str> = icons::NAMED_SYMBOLIC_ICONS
        .iter()
        .copied()
        .skip(1)
        .collect();
    theme(&icons_dir, "Adwaita", &[("symbolic/16x16", &held)]);
    // One name is missing from the theme: the list alone answers which button stays broken.
    assert_eq!(
        icons::missing_in_for_test(&icons_dir),
        vec![icons::NAMED_SYMBOLIC_ICONS[0]]
    );

    // Adding it where GTK also looks — under `apps/` of a listed folder — closes the list.
    let loose = icons::NAMED_SYMBOLIC_ICONS[0];
    theme(
        &icons_dir,
        "hicolor",
        &[("symbolic/16x16/apps", &[loose]), ("scalable/apps", &[])],
    );
    assert!(
        icons::missing_in_for_test(&icons_dir).is_empty(),
        "{:?}",
        icons::missing_in_for_test(&icons_dir)
    );
}

#[test]
fn a_folder_without_an_index_theme_is_not_a_theme() {
    // GTK reads `index.theme` to know a theme exists and which folders it keeps; a bare folder of
    // svgs would answer "found" here and still draw nothing on screen.
    let root = common::temp_dir_tagged("no-index");
    let icons_dir = root.join("icons");
    let all: Vec<&str> = icons::NAMED_SYMBOLIC_ICONS.to_vec();
    let bare = icons_dir.join("NotATheme");
    fs::create_dir_all(bare.join("symbolic/16x16")).unwrap();
    for name in &all {
        fs::write(
            bare.join("symbolic/16x16").join(format!("{name}.svg")),
            "<svg/>",
        )
        .unwrap();
    }
    assert_eq!(icons::themes(&icons_dir), vec![] as Vec<PathBuf>);
    assert_eq!(icons::missing_in_for_test(&icons_dir).len(), all.len());
}

#[test]
fn the_tree_beside_the_binary_is_where_a_package_carries_its_theme() {
    // An AppImage puts its theme beside the executable, so the launch can name it without a display
    // or `$XDG_DATA_DIRS`; this pins that the same search order `found` uses is what themes() walks.
    let root = common::temp_dir_tagged("bundled");
    let bin = root.join("usr/bin/naivepost");
    fs::create_dir_all(bin.parent().unwrap()).unwrap();
    fs::write(&bin, "not really an exe").unwrap();
    let icons_dir = bin.parent().unwrap().join("icons");
    theme(&icons_dir, "Adwaita", &[("symbolic/16x16", &[])]);
    assert_eq!(
        icons::bundled_themes(&bin, &root),
        vec![icons_dir.join("Adwaita")]
    );
    assert_eq!(
        icons::missing_in_for_test(&icons_dir).len(),
        icons::NAMED_SYMBOLIC_ICONS.len()
    );
}
