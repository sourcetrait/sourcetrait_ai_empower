use crate::*;
use crate::eye::tree::{size_label, tree, wrap};
use std::os::unix::fs::symlink;

/// Write a fixture file of `bytes` bytes.
fn write(path: &Path, bytes: usize) {
    fs::write(path, vec![b'x'; bytes]).expect("write fixture file");
}

#[test]
fn size_label_floors_to_the_largest_unit() {
    assert_eq!(size_label(0), "0");
    assert_eq!(size_label(1), "1b");
    assert_eq!(size_label(1023), "1023b");
    assert_eq!(size_label(1024), "1kb");
    assert_eq!(size_label(1536), "1kb"); // floor, not round
    assert_eq!(size_label(1024 * 1024 - 1), "1023kb");
    assert_eq!(size_label(1024 * 1024), "1mb");
    assert_eq!(size_label(32 * 1024 * 1024), "32mb");
    assert_eq!(size_label(1024 * 1024 * 1024), "1gb");
    assert_eq!(size_label(1024_u64.pow(4)), "1tb");
    assert_eq!(size_label(5 * 1024_u64.pow(4)), "5tb");
}

#[test]
fn wrap_backticks_only_names_with_spaces() {
    assert_eq!(wrap("file.txt"), "file.txt");
    assert_eq!(wrap("my file"), "`my file`");
    assert_eq!(wrap("a b c"), "`a b c`");
}

#[test]
fn renders_files_before_dirs_with_sizes_symlinks_and_default_git_ignore() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = fs::canonicalize(tmp.path()).expect("canonicalize root");

    write(&root.join("a.txt"), 0);
    write(&root.join("b.txt"), 2048);
    write(&root.join("sp ace.txt"), 5);
    fs::create_dir(root.join(".git")).unwrap();
    write(&root.join(".git").join("config"), 10);
    fs::create_dir(root.join("Zed")).unwrap();
    fs::create_dir(root.join("apple")).unwrap();
    write(&root.join("apple").join("inner.txt"), 40);
    fs::create_dir(root.join("empty")).unwrap();
    symlink("a.txt", root.join("link_f")).unwrap();
    symlink("apple", root.join("link_d")).unwrap();
    symlink("nowhere", root.join("link_broken")).unwrap();

    let r = root.display().to_string();
    let expected = [
        format!("{r}/"),
        " a.txt 0".to_string(),
        " b.txt 2kb".to_string(),
        " link_broken -> nowhere".to_string(),
        format!(" link_d -> {r}/apple/"),
        format!(" link_f -> {r}/a.txt 0"),
        " `sp ace.txt` 5b".to_string(),
        " Zed/".to_string(),
        " apple/".to_string(),
        "  inner.txt 40b".to_string(),
        " empty/".to_string(),
    ]
    .join("\n");

    assert_eq!(tree(&root, &[], &[]).unwrap(), expected);
}

#[test]
fn ignore_denies_and_regard_rescues() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = fs::canonicalize(tmp.path()).expect("canonicalize root");
    write(&root.join("keep.md"), 3);
    write(&root.join("skip.txt"), 3);
    write(&root.join("also.txt"), 3);

    let denied = tree(&root, &["*.txt".to_string()], &[]).unwrap();
    assert!(denied.contains("keep.md"));
    assert!(!denied.contains("skip.txt"));
    assert!(!denied.contains("also.txt"));

    let rescued = tree(&root, &["*.txt".to_string()], &["also.txt".to_string()]).unwrap();
    assert!(rescued.contains("also.txt"));
    assert!(!rescued.contains("skip.txt"));
}

#[test]
fn git_default_ignored_but_regardable() {
    let tmp = tempfile::tempdir().expect("temp dir");
    let root = fs::canonicalize(tmp.path()).expect("canonicalize root");
    fs::create_dir(root.join(".git")).unwrap();
    write(&root.join(".git").join("config"), 4);
    write(&root.join("real.txt"), 4);

    let default = tree(&root, &[], &[]).unwrap();
    assert!(!default.contains(".git"));
    assert!(default.contains("real.txt"));

    let regarded = tree(&root, &[], &["**/.git".to_string()]).unwrap();
    assert!(regarded.contains(".git/"));
    assert!(regarded.contains("config")); // its children still render
}
