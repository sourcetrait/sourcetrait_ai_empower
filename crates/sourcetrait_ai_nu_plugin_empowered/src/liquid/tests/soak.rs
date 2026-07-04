use crate::*;
use crate::liquid::soak::{PROTECTED_DEFAULT, Replace, soak_dir_replace};

/// An empty fill record; fixtures use plain / trivially-rendered files.
fn empty_fill() -> nu::Record {
    nu::Record::new()
}

/// A Replace ctx whose tmp base + retire graveyard live under `tmp`.
fn ctx(force: bool, tmp: &Path) -> Replace<'_> {
    Replace { force, tmp_base: tmp, user: "test", protected: PROTECTED_DEFAULT }
}

/// Write a file, creating parents.
fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// A source dir holding one plain file `a.txt` = "new".
fn source(root: &Path) -> PathBuf {
    let src = root.join("src");
    write(&src.join("a.txt"), "new");
    src
}

/// Whether a file named `name` exists directly under some `<tmp>/retired/soak/*`.
fn retired_contains(tmp: &Path, name: &str) -> bool {
    let graveyard = tmp.join("retired").join("soak");
    let Ok(dirs) = fs::read_dir(&graveyard) else {
        return false;
    };
    dirs.flatten().any(|dir| dir.path().join(name).exists())
}

#[test]
fn replace_into_absent_target_creates_it() {
    let tmp = tempfile::tempdir().unwrap();
    let src = source(tmp.path());
    let to = tmp.path().join("out");
    soak_dir_replace(&src, &to, &empty_fill(), &ctx(false, tmp.path())).unwrap();
    assert_eq!(fs::read_to_string(to.join("a.txt")).unwrap(), "new");
}

#[test]
fn replace_into_empty_target_swaps() {
    let tmp = tempfile::tempdir().unwrap();
    let src = source(tmp.path());
    let to = tmp.path().join("out");
    fs::create_dir(&to).unwrap();
    soak_dir_replace(&src, &to, &empty_fill(), &ctx(false, tmp.path())).unwrap();
    assert_eq!(fs::read_to_string(to.join("a.txt")).unwrap(), "new");
}

#[test]
fn non_protected_without_force_errors_and_leaves_target() {
    let tmp = tempfile::tempdir().unwrap();
    let src = source(tmp.path());
    let to = tmp.path().join("out");
    write(&to.join("old.txt"), "old");
    let result = soak_dir_replace(&src, &to, &empty_fill(), &ctx(false, tmp.path()));
    assert!(result.is_err());
    assert_eq!(fs::read_to_string(to.join("old.txt")).unwrap(), "old");
    assert!(!to.join("a.txt").exists());
}

#[test]
fn force_replaces_non_protected_and_retires_old() {
    let tmp = tempfile::tempdir().unwrap();
    let src = source(tmp.path());
    let to = tmp.path().join("out");
    write(&to.join("old.txt"), "old");
    soak_dir_replace(&src, &to, &empty_fill(), &ctx(true, tmp.path())).unwrap();
    assert_eq!(fs::read_to_string(to.join("a.txt")).unwrap(), "new");
    assert!(!to.join("old.txt").exists());
    assert!(retired_contains(tmp.path(), "old.txt"));
}

#[test]
fn protected_git_preserved_inline_without_force() {
    let tmp = tempfile::tempdir().unwrap();
    let src = source(tmp.path());
    let to = tmp.path().join("out");
    write(&to.join(".git").join("HEAD"), "ref: main");
    soak_dir_replace(&src, &to, &empty_fill(), &ctx(false, tmp.path())).unwrap();
    assert_eq!(fs::read_to_string(to.join("a.txt")).unwrap(), "new");
    assert_eq!(fs::read_to_string(to.join(".git").join("HEAD")).unwrap(), "ref: main");
}

#[test]
fn protected_repo_also_preserved() {
    let tmp = tempfile::tempdir().unwrap();
    let src = source(tmp.path());
    let to = tmp.path().join("out");
    write(&to.join(".repo").join("marker"), "x");
    soak_dir_replace(&src, &to, &empty_fill(), &ctx(false, tmp.path())).unwrap();
    assert!(to.join("a.txt").exists());
    assert_eq!(fs::read_to_string(to.join(".repo").join("marker")).unwrap(), "x");
}

#[test]
fn protected_plus_non_protected_needs_force_then_preserves() {
    let tmp = tempfile::tempdir().unwrap();
    let src = source(tmp.path());
    let to = tmp.path().join("out");
    write(&to.join(".git").join("HEAD"), "ref: main");
    write(&to.join("old.txt"), "old");
    assert!(soak_dir_replace(&src, &to, &empty_fill(), &ctx(false, tmp.path())).is_err());
    assert!(to.join("old.txt").exists());
    assert!(!to.join("a.txt").exists());
    soak_dir_replace(&src, &to, &empty_fill(), &ctx(true, tmp.path())).unwrap();
    assert_eq!(fs::read_to_string(to.join(".git").join("HEAD")).unwrap(), "ref: main");
    assert_eq!(fs::read_to_string(to.join("a.txt")).unwrap(), "new");
    assert!(!to.join("old.txt").exists());
}

#[test]
fn build_failure_leaves_target_untouched() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    write(&src.join("bad.liquid"), "{{ unclosed");
    let to = tmp.path().join("out");
    write(&to.join(".git").join("HEAD"), "ref: main");
    write(&to.join("keep.txt"), "keep");
    let result = soak_dir_replace(&src, &to, &empty_fill(), &ctx(true, tmp.path()));
    assert!(result.is_err());
    assert_eq!(fs::read_to_string(to.join(".git").join("HEAD")).unwrap(), "ref: main");
    assert_eq!(fs::read_to_string(to.join("keep.txt")).unwrap(), "keep");
}
