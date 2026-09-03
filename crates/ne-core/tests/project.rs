//! Project file round-tripping and path handling.

use ne_core::project::{addr_key, parse_addr_key, Project, FORMAT_VERSION};
use std::path::{Path, PathBuf};

fn tmpdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sith-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn address_keys_round_trip() {
    assert_eq!(addr_key(2, 0x1A40), "02:1A40");
    assert_eq!(parse_addr_key("02:1A40"), Some((2, 0x1A40)));
    assert_eq!(parse_addr_key("10:0000"), Some((10, 0)));
    assert_eq!(parse_addr_key("nonsense"), None);
}

#[test]
fn annotations_survive_a_save_and_load() {
    let dir = tmpdir("roundtrip");
    let bin = dir.join("GAME.EXE");
    std::fs::write(&bin, b"not really an executable").unwrap();
    let file = dir.join("game.sith");

    let mut p = Project::new("game");
    p.path = Some(file.clone());
    {
        let notes = p.notes_mut(&bin, "GAME");
        notes.set_name(2, 0x225C, "MainWndProc");
        notes.set_comment(2, 0x225C, "handles WM_PAINT");
        notes.toggle_bookmark(2, 0x225C);
        notes.bits32 = vec![6, 7];
    }
    p.save(&file).unwrap();
    assert!(!p.dirty, "saving clears the dirty flag");

    let loaded = Project::load(&file).unwrap();
    assert_eq!(loaded.format_version, FORMAT_VERSION);
    assert_eq!(loaded.name, "game");
    let notes = loaded.notes_for(&bin, "GAME").expect("notes are found again");
    assert_eq!(notes.name_at(2, 0x225C), Some("MainWndProc"));
    assert_eq!(notes.comment_at(2, 0x225C), Some("handles WM_PAINT"));
    assert!(notes.is_bookmarked(2, 0x225C));
    assert_eq!(notes.bits32, vec![6, 7]);
}

#[test]
fn binary_paths_are_stored_relative_to_the_project() {
    let dir = tmpdir("relative");
    let bin = dir.join("GAME.EXE");
    std::fs::write(&bin, b"x").unwrap();
    let file = dir.join("game.sith");

    let mut p = Project::new("game");
    p.path = Some(file.clone());
    p.notes_mut(&bin, "GAME").set_name(1, 0x10, "start");
    p.save(&file).unwrap();

    let text = std::fs::read_to_string(&file).unwrap();
    assert!(
        text.contains("\"GAME.EXE\""),
        "the path should be stored relative:\n{text}"
    );
    assert!(
        !text.contains(dir.to_str().unwrap()),
        "no absolute path should leak into the file"
    );

    // Moving the whole folder must not break the reference.
    let moved = tmpdir("relative-moved");
    let moved_bin = moved.join("GAME.EXE");
    let moved_file = moved.join("game.sith");
    std::fs::copy(&bin, &moved_bin).unwrap();
    std::fs::copy(&file, &moved_file).unwrap();
    let loaded = Project::load(&moved_file).unwrap();
    assert_eq!(loaded.resolve(&loaded.binaries[0].path), moved_bin);
}

#[test]
fn a_moved_binary_is_matched_by_module_name() {
    let dir = tmpdir("bymodule");
    let file = dir.join("p.sith");
    let mut p = Project::new("p");
    p.path = Some(file.clone());
    p.notes_mut(Path::new("/original/GAME.EXE"), "GAME")
        .set_name(1, 0x20, "start");
    p.save(&file).unwrap();

    let loaded = Project::load(&file).unwrap();
    // A different path, but the same module: the notes should still be found.
    let notes = loaded
        .notes_for(Path::new("/somewhere/else/GAME.EXE"), "GAME")
        .expect("matched by module name");
    assert_eq!(notes.name_at(1, 0x20), Some("start"));
}

#[test]
fn clearing_a_name_removes_the_entry() {
    let mut p = Project::new("p");
    let bin = Path::new("/x/A.EXE");
    p.notes_mut(bin, "A").set_name(1, 0x10, "thing");
    assert_eq!(p.annotation_count(), 1);
    p.notes_mut(bin, "A").set_name(1, 0x10, "   ");
    assert_eq!(p.annotation_count(), 0, "a blank name is removed, not stored");
}

#[test]
fn bookmarks_toggle() {
    let mut p = Project::new("p");
    let bin = Path::new("/x/A.EXE");
    assert!(p.notes_mut(bin, "A").toggle_bookmark(3, 0x100));
    assert!(p.notes_for(bin, "A").unwrap().is_bookmarked(3, 0x100));
    assert!(!p.notes_mut(bin, "A").toggle_bookmark(3, 0x100));
    assert!(!p.notes_for(bin, "A").unwrap().is_bookmarked(3, 0x100));
}

#[test]
fn empty_entries_are_pruned_on_save() {
    let dir = tmpdir("prune");
    let file = dir.join("p.sith");
    let mut p = Project::new("p");
    p.path = Some(file.clone());
    let _ = p.notes_mut(Path::new("/x/EMPTY.EXE"), "EMPTY");
    p.notes_mut(Path::new("/x/USED.EXE"), "USED")
        .set_name(1, 0, "start");
    p.prune();
    assert_eq!(p.binaries.len(), 1);
    assert_eq!(p.binaries[0].module, "USED");
}

#[test]
fn a_newer_format_version_is_refused_rather_than_misread() {
    let dir = tmpdir("version");
    let file = dir.join("future.sith");
    std::fs::write(
        &file,
        r#"{"format_version": 9999, "name": "x", "binaries": []}"#,
    )
    .unwrap();
    let err = Project::load(&file).unwrap_err();
    assert!(
        err.to_string().contains("newer"),
        "the error should say why: {err}"
    );
}
