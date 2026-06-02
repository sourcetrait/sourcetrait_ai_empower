"""test_orientation.py — unit tests for the deterministic phases, runnable in-container
against synthetic Rust trees. The rustdoc overlay and real-repo trace cannot run here; only
the overlay's DEGRADATION path is asserted.

Run:  python3 scripts/test_orientation.py
"""
from __future__ import annotations
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import rustscan  # noqa


def _scan(src):
    return rustscan.scan_file("t.rs", src)


def test_masking_excludes_decoys():
    src = '''
// impl Command for InComment {}
/* impl Command for InBlock {} */
fn f() { let s = "impl Command for InString {}"; let c = '}'; let l: &'static str = ""; }
impl Command for Real {}
'''
    f = _scan(src)
    impls = [(i["trait"], i["type"]) for i in f["impls"]]
    assert impls == [("Command", "Real")], impls
    print("ok  masking_excludes_decoys")


def test_macro_args_captured():
    f = _scan("fn s() { bind_command!(ws, A, B, C); }")
    m = [x for x in f["macros"] if x["name"] == "bind_command"][0]
    assert m["arg_idents"] == ["ws", "A", "B", "C"], m["arg_idents"]
    assert m["expansion_unverified"] is True
    print("ok  macro_args_captured")


def test_derive_and_attr_macro():
    f = _scan("#[derive(Debug, Clone)]\n#[tokio::main]\nstruct X;")
    dtr = sorted(d["trait"] for d in f["derives"])
    assert dtr == ["Clone", "Debug"], dtr
    attrs = [m["name"] for m in f["macros"] if m["kind"] == "attr_macro"]
    assert attrs == ["tokio::main"], attrs
    print("ok  derive_and_attr_macro")


def test_impl_for_vs_inherent_and_generics():
    f = _scan("impl Foo {}\nimpl<T: Send> Bar<T> for Baz<T> where T: Clone {}")
    kinds = sorted(((i["trait"], i["type"]) for i in f["impls"]), key=lambda x: str(x))
    assert (None, "Foo") in kinds and ("Bar", "Baz") in kinds, kinds
    print("ok  impl_for_vs_inherent_and_generics")


def test_rpit_not_counted_as_impl():
    f = _scan("fn make() -> impl Iterator<Item = u8> { core::iter::empty() }")
    assert f["impls"] == [], f["impls"]
    print("ok  rpit_not_counted_as_impl")


def test_doc_for_why_axis():
    f = _scan("/// Why this exists.\npub struct Documented;\npub struct Bare;")
    docs = {t["name"]: t.get("doc", "") for t in f["types"]}
    assert "Why this exists" in docs["Documented"], docs
    assert docs["Bare"] == "", docs
    print("ok  doc_for_why_axis")


def _write_tree(base: Path, files: dict):
    for rel, content in files.items():
        p = base / rel
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(content)


def _characterize(root: Path):
    out = root / ".orientation"
    r = subprocess.run([sys.executable, str(HERE / "characterize.py"), str(root), str(out)],
                       capture_output=True, text=True)
    assert r.returncode == 0, r.stderr
    return json.loads((out / "fingerprint.json").read_text()), out


def test_nushell_shape_single_dominant():
    with tempfile.TemporaryDirectory() as d:
        root = Path(d)
        cmds = "\n".join(
            f"pub struct C{i};\nimpl Command for C{i} {{ fn run(&self) {{}} }}"
            for i in range(25))
        _write_tree(root, {
            "Cargo.toml": '[workspace]\nmembers=["p","c"]\n',
            "p/Cargo.toml": '[package]\nname="p"\n[dependencies]\n',
            "p/src/lib.rs": "pub trait Command { fn run(&self); }\npub struct Value;",
            "c/Cargo.toml": '[package]\nname="c"\n[dependencies]\np={path="../p"}\n',
            "c/src/lib.rs": "use p::Command;\n" + cmds +
                            "\nfn reg(){ bind_command!(C0,C1,C2); }",
        })
        fp, _ = _characterize(root)
        assert fp["pattern_histogram"][0]["pattern"] == "trait_impl:Command"
        assert fp["selection"]["histogram_mode"] == "single_dominant"
        assert fp["n_components"] == 1
        print("ok  nushell_shape_single_dominant")


def test_bevy_shape_derive_coequal():
    # Bevy-like: behaviour is derives (#[derive(Component)]) + systems-as-functions, NOT
    # `impl Trait for`. A detector assuming impl-Trait-for would miss the dominant pattern.
    with tempfile.TemporaryDirectory() as d:
        root = Path(d)
        comps = "\n".join(
            f"#[derive(Component)]\npub struct Pos{i};" for i in range(20))
        systems = "\n".join(
            f"pub fn system_{i}(q: Query) {{}}" for i in range(20))
        _write_tree(root, {
            "Cargo.toml": '[workspace]\nmembers=["ecs","game"]\n',
            "ecs/Cargo.toml": '[package]\nname="ecs"\n[dependencies]\n',
            "ecs/src/lib.rs": "pub trait Component {}\npub struct Query;",
            "game/Cargo.toml": '[package]\nname="game"\n[dependencies]\necs={path="../ecs"}\n',
            "game/src/lib.rs": "use ecs::Component;\n" + comps + "\n" + systems,
        })
        fp, _ = _characterize(root)
        kinds = fp["pattern_by_kind"]
        # derive must be a top contributor; impl_trait must NOT dominate.
        assert kinds.get("derive", 0) >= 20, kinds
        top = fp["pattern_histogram"][0]["pattern"]
        assert top.startswith("derive:") or top.startswith("fn_table:"), top
        print("ok  bevy_shape_derive_coequal  (dominant =", top + ")")


def test_regional_multiworkspace():
    with tempfile.TemporaryDirectory() as d:
        root = Path(d)
        _write_tree(root, {
            "kernel/Cargo.toml": '[workspace]\nmembers=["."]\n[package]\nname="kernel"\n',
            "kernel/src/lib.rs": '#![no_std]\nextern "C" { fn syscall(n: usize); }\n',
            "user/Cargo.toml": '[workspace]\nmembers=["a"]\n',
            "user/a/Cargo.toml": '[package]\nname="a"\n[dependencies]\n',
            "user/a/src/lib.rs": "pub trait T{}\npub struct S;\nimpl T for S{}",
        })
        fp, _ = _characterize(root)
        assert fp["selection"]["mode"] == "regional", fp["selection"]
        assert fp["n_components"] >= 2
        assert any("structural signals" in n for n in fp["selection"]["notes"])
        print("ok  regional_multiworkspace")


def test_emit_two_files_and_spans():
    with tempfile.TemporaryDirectory() as d:
        root = Path(d)
        _write_tree(root, {
            "Cargo.toml": '[package]\nname="x"\n[dependencies]\n',
            "src/lib.rs": "pub trait Cmd{fn r(&self);}\npub struct A;\nimpl Cmd for A{fn r(&self){}}",
        })
        fp, out = _characterize(root)
        r = subprocess.run([sys.executable, str(HERE / "emit.py"), str(root), str(out)],
                           capture_output=True, text=True)
        assert r.returncode == 0, r.stderr
        orient = (out / "orientation.md").read_text()
        ref = (out / "reference.md").read_text()
        assert "[AGENT]" in orient and "Worked slice" in orient
        assert "src/lib.rs:" in ref  # spans present
        assert "trait_impl:Cmd" in orient  # dominant pattern surfaced
        print("ok  emit_two_files_and_spans")


def test_overlay_degrades_without_toolchain():
    with tempfile.TemporaryDirectory() as d:
        root = Path(d)
        out = root / ".orientation"
        out.mkdir()
        (out / "facts.json").write_text(json.dumps({"impls": []}))
        # Force "no cargo" by giving an empty PATH so shutil.which fails.
        env = dict(os.environ, PATH="")
        r = subprocess.run([sys.executable, str(HERE / "rustdoc_overlay.py"), str(root), str(out)],
                           capture_output=True, text=True, env=env)
        assert r.returncode == 0, r.stderr
        banner = json.loads((out / "rustdoc_overlay.json").read_text())
        assert banner["status"] == "absent", banner
        print("ok  overlay_degrades_without_toolchain")


if __name__ == "__main__":
    tests = [v for k, v in sorted(globals().items()) if k.startswith("test_")]
    for t in tests:
        t()
    print(f"\nALL {len(tests)} TESTS PASSED")
