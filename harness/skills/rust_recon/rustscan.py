"""rustscan.py - structure-aware Rust fact extractor (pure stdlib).

This is the detection FLOOR. It is deliberately *not*:
  - regex-based pattern detection over raw source (which miscounts constructs inside
    comments and strings, is blind to macro-mediated registration, and wrongly assumes
    `impl Trait for` is the universal signal); and
  - a binding to `syn` (a Rust crate, which would need a Rust toolchain and so violates the
    stdlib-Python constraint).

Instead a character-level lexer masks comment bodies and string/char literal *contents*
(preserving byte and line offsets) so that item detection operates on code only. Doc-comment
text is preserved separately to feed the why-axis. Macro invocations are captured with their
argument lists, so macro-mediated registration can be counted from the call site even though
the expansion itself is invisible to any static scanner - these counts are flagged
`expansion_unverified` and are confirmed by the rustdoc overlay when a toolchain is present.

`re` is used only as a low-level lexer over already-masked (comment/string-free) text to
split it into tokens; it is never the pattern detector. All semantic decisions (impl vs RPIT,
trait-vs-type, `for`, brace depth) are made by explicit token walking.
"""

from __future__ import annotations
import bisect
import re

# Item / type keywords we recognise. `for` and `where` and `dyn` are handled specially.
KEYWORDS = {
    "impl", "trait", "struct", "enum", "union", "type", "fn", "mod", "macro_rules",
    "for", "where", "dyn", "pub", "use", "as", "const", "static", "unsafe", "extern",
    "async", "move", "ref", "mut", "let", "match", "if", "else", "while", "loop",
    "return", "self", "Self", "crate", "super", "in",
}

# Attributes that are language built-ins, not user attribute-macros.
INERT_ATTRS = {
    "derive", "cfg", "cfg_attr", "allow", "warn", "deny", "forbid", "doc", "must_use",
    "inline", "repr", "non_exhaustive", "automatically_derived", "no_mangle", "used",
    "link_section", "export_name", "test", "ignore", "should_panic", "bench",
    "global_allocator", "panic_handler", "track_caller", "cold", "target_feature",
    "rustfmt", "clippy", "link", "no_link", "path", "macro_use", "macro_export",
    "proc_macro", "proc_macro_derive", "proc_macro_attribute", "stable", "unstable",
}

# Macro invocations that are std/common noise - excluded from the registration-macro
# histogram (still recorded in the raw macro list).
NOISE_MACROS = {
    "vec", "println", "print", "eprintln", "eprint", "format", "write", "writeln",
    "assert", "assert_eq", "assert_ne", "debug_assert", "debug_assert_eq",
    "debug_assert_ne", "panic", "todo", "unimplemented", "unreachable", "dbg",
    "matches", "include_str", "include_bytes", "include", "env", "option_env",
    "concat", "stringify", "format_args", "cfg", "line", "column", "file",
    "compile_error", "if_chain", "try", "await",
}

# Attribute macros that are user-domain but constitute scaffolding noise rather than
# architectural registration -- test-framework attributes, rustfmt/clippy directives, and
# derive-helper attributes that configure a derive-generated impl per-field (serde, strum,
# bitflags etc.). attr_macro:case from rstest, attr_macro:serde from derive scaffolding,
# attr_macro:rustfmt::skip from formatter pragmas all dominated histograms in the 0.0.2
# probe sweep without representing architectural patterns.
NOISE_ATTRS = {
    # test-framework
    "case", "rstest", "expect",
    # rustfmt / formatter pragmas
    "rustfmt::skip",
    # derive-helper / proc-macro support attrs configuring generated code per-field
    "serde", "strum", "bitflags", "clap", "arg", "command",
}


def _is_noise_macro(name):
    """Return True if `name` should be excluded from the macro_invocation histogram.
    Combines the explicit NOISE_MACROS set with prefix heuristics: assert_* and
    debug_assert_* catch the tokio-test / bevy / many-other-framework assertion macros
    that fall outside the canonical assert / assert_eq / assert_ne already in
    NOISE_MACROS, and cfg_* / cfg_not_* catch tokio-style cfg-gating macro_rules!
    wrappers that emit `#[cfg(...)]` blocks for feature ergonomics."""
    if name in NOISE_MACROS:
        return True
    if (name.startswith("assert_")
            or name.startswith("debug_assert_")
            or name.startswith("async_assert_")):
        return True
    if name.startswith("cfg_") or name.startswith("cfg_not_"):
        return True
    return False


def _is_noise_attr(path, base):
    """Return True if a #[<path>] attribute should be excluded from the attr_macro
    histogram. Checks both the full path form (catches namespaced attrs like
    rustfmt::skip) and the last-segment base form (catches unnamespaced attrs like
    'case' or 'serde')."""
    return path in NOISE_ATTRS or base in NOISE_ATTRS


# 0.0.8 patch 8a: identifiers that flood the type-usage histogram when
# treated as architectural protagonists. Standard collections, smart
# pointers, primitive option / result enums, common namespace aliases,
# and short generic-parameter conventions (T / E / U / K / V). The set
# is checked against the OUTER name in `<outer>::<inner>(` factory-call
# detection - noise outers are never counted. Expand as false positives
# surface during probe sweeps.
TYPE_USAGE_NOISE_TYPES = frozenset([
    # collections
    "Vec", "VecDeque", "LinkedList", "BinaryHeap",
    "HashMap", "HashSet", "BTreeMap", "BTreeSet",
    # smart pointers + cell types
    "Box", "Arc", "Rc", "Mutex", "RwLock", "RefCell", "Cell", "Weak",
    "OnceCell", "OnceLock",
    # primitive-y wrappers + enum constructors
    "Option", "Result", "Some", "None", "Ok", "Err",
    # standard string-ish + filesystem types
    "String", "Path", "PathBuf", "Cow", "Pin",
    "OsString", "OsStr", "CString", "CStr",
    # iterator + adaptor helpers
    "Iter", "IterMut", "IntoIter", "Chain", "Map", "Filter", "Take",
    "Skip", "Zip", "Enumerate", "Peekable",
    # std-library top-level namespaces (rarely used as outer-of-factory)
    "std", "core", "alloc",
    # generic-parameter conventions
    "T", "E", "U", "K", "V",
])


_TOK = re.compile(r"[A-Za-z_][A-Za-z0-9_]*|::|->|=>|[{}()\[\]<>;:,!#=&|]")


# ----------------------------------------------------------------------------------------
# Lexer: mask comments + literal contents, collect doc-comment text.
# ----------------------------------------------------------------------------------------
def mask_and_collect(src: str):
    """Return (masked_src, docs). Comment bodies and string/char contents are replaced by
    spaces (newlines preserved, so offsets and line numbers are stable). `docs` is a list of
    (start, end, kind, text) for doc comments (kind in {'outer','inner'})."""
    n = len(src)
    out = list(src)
    docs = []

    def blank(a, b):
        for k in range(a, b):
            if out[k] != "\n":
                out[k] = " "

    i = 0
    while i < n:
        c = src[i]
        # line comment / doc
        if c == "/" and i + 1 < n and src[i + 1] == "/":
            kind = None
            if i + 2 < n and src[i + 2] == "/" and not (i + 3 < n and src[i + 3] == "/"):
                kind = "outer"  # ///
            elif i + 2 < n and src[i + 2] == "!":
                kind = "inner"  # //!
            k = i
            while k < n and src[k] != "\n":
                k += 1
            if kind:
                docs.append((i, k, kind, src[i:k]))
            blank(i, k)
            i = k
            continue
        # block comment / doc (nested)
        if c == "/" and i + 1 < n and src[i + 1] == "*":
            kind = None
            if (i + 2 < n and src[i + 2] == "*"
                    and not (i + 3 < n and src[i + 3] == "/")):
                kind = "outer"  # /** ... */ (not /**/)
            elif i + 2 < n and src[i + 2] == "!":
                kind = "inner"  # /*! ... */
            depth = 1
            k = i + 2
            while k < n and depth > 0:
                if src[k] == "/" and k + 1 < n and src[k + 1] == "*":
                    depth += 1
                    k += 2
                    continue
                if src[k] == "*" and k + 1 < n and src[k + 1] == "/":
                    depth -= 1
                    k += 2
                    continue
                k += 1
            if kind:
                docs.append((i, k, kind, src[i:k]))
            blank(i, k)
            i = k
            continue
        # raw string: r"..." r#"..."# br"..." etc.
        if (c == "r" or c == "b") and _is_raw_string_start(src, i):
            i = _consume_raw_string(src, i, blank)
            continue
        # byte string b"..."
        if c == "b" and i + 1 < n and src[i + 1] == '"':
            i = _consume_normal_string(src, i + 1, blank)
            continue
        # normal string
        if c == '"':
            i = _consume_normal_string(src, i, blank)
            continue
        # byte char b'x'
        if c == "b" and i + 1 < n and src[i + 1] == "'":
            i = _consume_char_or_lifetime(src, i + 1, blank)
            continue
        # char literal vs lifetime
        if c == "'":
            i = _consume_char_or_lifetime(src, i, blank)
            continue
        i += 1
    return "".join(out), docs


def _is_raw_string_start(src, i):
    j = i
    if src[j] == "b":
        j += 1
        if j >= len(src) or src[j] != "r":
            return False
        j += 1
    elif src[j] == "r":
        j += 1
    else:
        return False
    while j < len(src) and src[j] == "#":
        j += 1
    return j < len(src) and src[j] == '"'


def _consume_raw_string(src, i, blank):
    j = i
    if src[j] == "b":
        j += 1
    j += 1  # past 'r'
    hashes = 0
    while j < len(src) and src[j] == "#":
        hashes += 1
        j += 1
    j += 1  # past opening quote
    content_start = j
    closing = '"' + "#" * hashes
    end = src.find(closing, j)
    if end == -1:
        blank(content_start, len(src))
        return len(src)
    blank(content_start, end)
    return end + len(closing)


def _consume_normal_string(src, i, blank):
    j = i + 1
    start = j
    n = len(src)
    while j < n:
        if src[j] == "\\":
            j += 2
            continue
        if src[j] == '"':
            break
        j += 1
    blank(start, min(j, n))
    return j + 1 if j < n else j


def _consume_char_or_lifetime(src, i, blank):
    n = len(src)
    if i + 1 < n and src[i + 1] == "\\":
        j = i + 2
        if j < n and src[j] == "u" and j + 1 < n and src[j + 1] == "{":
            close = src.find("}", j)
            j = (close + 1) if close != -1 else j + 1
        else:
            j += 1
        if j < n and src[j] == "'":
            blank(i + 1, j)
            return j + 1
        return i + 1  # malformed; treat ' as code
    if i + 2 < n and src[i + 2] == "'":
        blank(i + 1, i + 2)
        return i + 3  # char literal 'x'
    return i + 1  # lifetime 'a / 'static


# ----------------------------------------------------------------------------------------
# Tokeniser + offset helpers.
# ----------------------------------------------------------------------------------------
def tokenize(masked):
    return [(m.group(0), m.start()) for m in _TOK.finditer(masked)]


def line_starts(src):
    starts = [0]
    for idx, ch in enumerate(src):
        if ch == "\n":
            starts.append(idx + 1)
    return starts


def line_of(starts, off):
    return bisect.bisect_right(starts, off)  # 1-indexed


def _is_ident(t):
    return bool(t) and (t[0].isalpha() or t[0] == "_")


# ----------------------------------------------------------------------------------------
# Header parsing helpers (operate on token slices).
# ----------------------------------------------------------------------------------------
def _base_name(tok_slice):
    """Last path identifier at angle-depth 0 before the first generic '<'."""
    name = None
    angle = 0
    seen_angle = False
    for t, _ in tok_slice:
        if t == "<":
            seen_angle = True
            angle += 1
            continue
        if t == ">":
            angle -= 1
            continue
        if angle == 0 and not seen_angle and _is_ident(t) and t not in KEYWORDS:
            name = t
    return name


def _find_token(toks, p, target, stop_targets):
    """Find index of `target` token at angle-depth 0 between p and a stop token. Returns -1."""
    angle = 0
    i = p
    while i < len(toks):
        t = toks[i][0]
        if t in stop_targets and angle == 0:
            return -1
        if t == "<":
            angle += 1
        elif t == ">":
            angle -= 1
        elif t == target and angle == 0:
            return i
        i += 1
    return -1


def _match(toks, p, open_t, close_t):
    depth = 0
    i = p
    while i < len(toks):
        t = toks[i][0]
        if t == open_t:
            depth += 1
        elif t == close_t:
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return len(toks) - 1


# ----------------------------------------------------------------------------------------
# Main per-file scan.
# ----------------------------------------------------------------------------------------
def scan_file(relpath, src):
    masked, docs = mask_and_collect(src)
    toks = tokenize(masked)
    starts = line_starts(src)
    ln = lambda off: line_of(starts, off)

    facts = {
        "impls": [], "traits": [], "types": [], "fns": [], "mods": [],
        "uses": [], "macros": [], "attrs": [], "derives": [], "macro_defs": [],
        # 0.0.8 patch 8a: type_usages captures factory-call shape
        # `<outer>::<inner>(...)` where outer is not a Rust keyword and
        # not in TYPE_USAGE_NOISE_TYPES. Each entry: {name, kind_hint,
        # line, brace_depth, expansion_unverified}. Architectural pattern
        # the prior 7-kind taxonomy (trait_impl / derive / attr_macro /
        # reg_macro / fn_table) couldn't see - tokio's mpsc::channel,
        # helix's Selection::single, nushell's PipelineData::Value, etc.
        "type_usages": [],
        "seams": {}, "doc_count": len(docs),
    }

    # First pass: attributes (needed for derive/cfg/attr-macro histograms and association).
    attrs = []  # {start,end,path,base,args,inner}
    p = 0
    N = len(toks)
    while p < N:
        t, off = toks[p]
        if t == "#":
            inner = (p + 1 < N and toks[p + 1][0] == "!")
            br = p + (2 if inner else 1)
            if br < N and toks[br][0] == "[":
                close = _match(toks, br, "[", "]")
                inner_start = toks[br][1] + 1
                inner_end = toks[close][1]
                inner_txt = masked[inner_start:inner_end].strip()
                # Strip args (after `(`) then value (after `=`) so attribute path
                # extraction handles both `#[foo(args)]` and value-style
                # `#[foo = "value"]` (must_use, doc, ...). The value-style branch
                # previously captured `foo = "..."` whole as the path string,
                # producing fake histogram entries like `must_use = "  ..."`.
                path = inner_txt.split("(", 1)[0].split("=", 1)[0].strip()
                args = ""
                if "(" in inner_txt:
                    args = inner_txt[inner_txt.index("(") + 1:].rstrip()
                    if args.endswith(")"):
                        args = args[:-1]
                base = path.split("::")[-1].strip()
                a = {"start": off, "end": toks[close][1] + 1, "path": path,
                     "base": base, "args": args, "inner": inner, "line": ln(off)}
                attrs.append(a)
                p = close + 1
                continue
        p += 1
    facts["attrs"] = attrs

    # derive + attr-macro histograms (counting decoupled from item association).
    for a in attrs:
        if a["base"] == "derive":
            for piece in _split_top(a["args"]):
                d = piece.strip().split("::")[-1].strip()
                if d:
                    facts["derives"].append({"trait": d, "line": a["line"]})
        elif a["base"] not in INERT_ATTRS and not _is_noise_attr(a["path"], a["base"]):
            # user attribute-macro
            facts["macros"].append({"kind": "attr_macro", "name": a["path"],
                                    "line": a["line"], "args_count": len(_split_top(a["args"])),
                                    "expansion_unverified": True})

    attrs_by_end = {a["end"]: a for a in attrs}
    docs_by_end = {d[1]: d for d in docs}

    def gather_prefix(item_start):
        pos = item_start
        cdocs, cattrs = [], []
        guard = 0
        while guard < 200:
            guard += 1
            j = pos
            while j > 0 and src[j - 1] in " \t\r\n":
                j -= 1
            if j in attrs_by_end:
                a = attrs_by_end[j]
                cattrs.append(a)
                pos = a["start"]
                continue
            if j in docs_by_end:
                d = docs_by_end[j]
                cdocs.append(d)
                pos = d[0]
                continue
            break
        cdocs.reverse()
        cattrs.reverse()
        doc_text = "\n".join(_clean_doc(d[3]) for d in cdocs).strip()
        cfg = any(a["base"] == "cfg" for a in cattrs)
        cfg_expr = next((a["args"] for a in cattrs if a["base"] == "cfg"), "")
        return doc_text, cfg, cfg_expr

    # Second pass: items + macro invocations, tracking brace depth.
    brace = 0
    p = 0
    while p < N:
        t, off = toks[p]
        if t == "{":
            brace += 1
            p += 1
            continue
        if t == "}":
            brace -= 1
            p += 1
            continue
        prev = toks[p - 1][0] if p > 0 else None

        # macro invocation: ident ! ( | [ | {
        if (_is_ident(t) and t not in KEYWORDS and p + 1 < N and toks[p + 1][0] == "!"
                and p + 2 < N and toks[p + 2][0] in "([{"):
            open_t = toks[p + 2][0]
            close_t = {"(": ")", "[": "]", "{": "}"}[open_t]
            close = _match(toks, p + 2, open_t, close_t)
            arg_txt = masked[toks[p + 2][1] + 1:toks[close][1]]
            arg_idents = [s.strip().split("::")[-1].split("<")[0].strip()
                          for s in _split_top(arg_txt)]
            arg_idents = [a for a in arg_idents if a and _is_ident(a)]
            if not _is_noise_macro(t):
                facts["macros"].append({
                    "kind": "macro_invocation", "name": t, "line": ln(off),
                    "args_count": len([s for s in _split_top(arg_txt) if s.strip()]),
                    "arg_idents": arg_idents[:64], "brace_depth": brace,
                    "expansion_unverified": True,
                })
            p = close + 1
            continue

        # 0.0.8 patch 8a: type-usage factory call detection.
        # Shape: <outer_ident> :: <inner_ident> [<turbofish>] (<args>).
        # Captures static method calls like mpsc::channel(...) and
        # Selection::single(0, 5) and PipelineData::Value(...). Both
        # idents must be non-keyword; outer must not be in
        # TYPE_USAGE_NOISE_TYPES (skip Vec / Box / Option / etc.).
        # For chained paths like tokio::sync::mpsc::channel(...) only
        # the innermost IDENT::IDENT( pair matches because the outer
        # ident is followed by :: (not ( or <), so the detector walks
        # forward until it finds the pair where inner is followed by (.
        # Captured name: "<outer>::<inner>". Loop advance: none here;
        # the trailing p += 1 fires so the next iteration can still
        # consider further patterns inside the call args.
        if (_is_ident(t) and t not in KEYWORDS
                and t not in TYPE_USAGE_NOISE_TYPES
                and p + 3 < N
                and toks[p + 1][0] == "::"
                and _is_ident(toks[p + 2][0])
                and toks[p + 2][0] not in KEYWORDS):
            after_inner = p + 3
            if toks[after_inner][0] == "<":
                gclose = _match(toks, after_inner, "<", ">")
                after_inner = gclose + 1
            if after_inner < N and toks[after_inner][0] == "(":
                facts["type_usages"].append({
                    "name": f"{t}::{toks[p + 2][0]}",
                    "kind_hint": "factory_call",
                    "line": ln(off),
                    "brace_depth": brace,
                    "expansion_unverified": False,
                })

        if _is_ident(t) and prev not in (".", "::") and t in KEYWORDS:
            istart = _qualifier_start(toks, p)
            # impl
            if t == "impl" and prev != "->":
                doc_text, cfg, cfg_expr = gather_prefix(istart)
                rec = _parse_impl(toks, p, masked, ln)
                if rec:
                    rec.update({"cfg_gated": cfg, "cfg": cfg_expr})
                    facts["impls"].append(rec)
                    # advance to body open or header end
                    p = rec["_next"]
                    del rec["_next"]
                    continue
            elif t == "trait":
                nm = _name_after(toks, p)
                if nm:
                    doc_text, cfg, _ = gather_prefix(istart)
                    facts["traits"].append({"name": nm, "line": ln(off),
                                            "cfg_gated": cfg, "doc": doc_text})
            elif t in ("struct", "enum", "union"):
                nm = _name_after(toks, p)
                if nm:
                    doc_text, cfg, _ = gather_prefix(istart)
                    facts["types"].append({"kind": t, "name": nm, "line": ln(off),
                                           "cfg_gated": cfg, "doc": doc_text})
            elif t == "type" and prev not in ("impl",):
                nm = _name_after(toks, p)
                if nm:
                    facts["types"].append({"kind": "type", "name": nm, "line": ln(off),
                                           "cfg_gated": False, "doc": ""})
            elif t == "fn":
                if p + 1 < N and _is_ident(toks[p + 1][0]):  # not fn-pointer type
                    nm = toks[p + 1][0]
                    doc_text, cfg, _ = gather_prefix(istart)
                    facts["fns"].append({"name": nm, "line": ln(off),
                                         "brace_depth": brace, "doc": doc_text})
            elif t == "mod":
                nm = _name_after(toks, p)
                if nm:
                    facts["mods"].append({"name": nm, "line": ln(off)})
            elif t == "macro_rules":
                # macro_rules ! name
                if p + 2 < N and _is_ident(toks[p + 2][0]):
                    facts["macro_defs"].append({"name": toks[p + 2][0], "line": ln(off)})
            elif t == "use":
                is_pub = (prev == "pub") or (p >= 2 and toks[p - 2][0] == "pub")
                semi = _find_token(toks, p, ";", set())
                path_txt = masked[off:toks[semi][1]] if semi != -1 else masked[off:off + 80]
                facts["uses"].append({"reexport": bool(is_pub),
                                      "path": path_txt.replace("use", "", 1).strip(),
                                      "line": ln(off)})
        p += 1

    facts["seams"] = _seam_inventory(masked, toks)
    return facts


def _split_top(s):
    """Split on commas at top bracket depth."""
    out, depth, cur = [], 0, []
    for ch in s:
        if ch in "([{<":
            depth += 1
        elif ch in ")]}>":
            depth = max(0, depth - 1)
        if ch == "," and depth == 0:
            out.append("".join(cur))
            cur = []
        else:
            cur.append(ch)
    if "".join(cur).strip():
        out.append("".join(cur))
    return out


def _clean_doc(text):
    text = text.strip()
    if text.startswith("///"):
        return text[3:].strip()
    if text.startswith("//!"):
        return text[3:].strip()
    if text.startswith("/**"):
        text = text[3:]
    elif text.startswith("/*!"):
        text = text[3:]
    if text.endswith("*/"):
        text = text[:-2]
    lines = [l.strip().lstrip("*").strip() for l in text.splitlines()]
    return " ".join(l for l in lines if l)


def _name_after(toks, p):
    if p + 1 < len(toks) and _is_ident(toks[p + 1][0]) and toks[p + 1][0] not in KEYWORDS:
        return toks[p + 1][0]
    return None


_QUALIFIERS = {"pub", "unsafe", "async", "const", "default", "extern"}


def _qualifier_start(toks, p):
    """Offset of the leftmost visibility/qualifier token preceding the item keyword at index
    p (so a doc comment before `pub struct X` / `pub(crate) unsafe fn` associates to it)."""
    start = p
    i = p - 1
    while i >= 0:
        t = toks[i][0]
        if t in _QUALIFIERS:
            start = i
            i -= 1
            continue
        if t == ")":  # possible pub(crate) / pub(super) / pub(in path)
            depth, j = 0, i
            while j >= 0:
                if toks[j][0] == ")":
                    depth += 1
                elif toks[j][0] == "(":
                    depth -= 1
                    if depth == 0:
                        break
                j -= 1
            if j - 1 >= 0 and toks[j - 1][0] == "pub":
                start = j - 1
                i = j - 2
                continue
            break
        break
    return toks[start][1]


def _parse_impl(toks, p, masked, ln):
    off = toks[p][1]
    # find first '{' (body) or ';' after impl
    body = _find_token(toks, p + 1, "{", set())
    semi = _find_token(toks, p + 1, ";", set())
    end_idx = body if body != -1 else (semi if semi != -1 else min(p + 60, len(toks) - 1))
    # Resume AT the body-open '{' (or past ';') so the main loop's brace counter stays
    # balanced - jumping past '{' would skip counting the open while still counting its close.
    next_idx = end_idx if body != -1 else end_idx + 1
    # skip a leading generic <...> directly after `impl`
    q = p + 1
    if q < len(toks) and toks[q][0] == "<":
        gclose = _match(toks, q, "<", ">")
        q = gclose + 1
    header = toks[q:end_idx]
    forpos = _find_token(toks, q, "for", {"{", ";"})
    end_line = ln(toks[body][1]) if body != -1 else ln(off)
    if forpos != -1 and forpos < end_idx:
        trait_slice = toks[q:forpos]
        # type part: after for, up to where/{ 
        wherepos = _find_token(toks, forpos + 1, "where", {"{", ";"})
        type_end = wherepos if wherepos != -1 else end_idx
        type_slice = toks[forpos + 1:type_end]
        trait_name = _base_name(trait_slice)
        type_name = _base_name(type_slice)
        return {"trait": trait_name, "type": type_name, "line": ln(off),
                "end_line": end_line, "_next": next_idx}
    else:
        wherepos = _find_token(toks, q, "where", {"{", ";"})
        type_end = wherepos if wherepos != -1 else end_idx
        type_name = _base_name(toks[q:type_end])
        return {"trait": None, "type": type_name, "line": ln(off),
                "end_line": end_line, "_next": next_idx}


def _seam_inventory(masked, toks):
    idents = [t for t, _ in toks if _is_ident(t)]
    from collections import Counter
    c = Counter(idents)
    text = masked
    seams = {
        "extern": text.count("extern"),
        "no_std": 1 if "no_std" in text else 0,
        "dyn_trait_object": c.get("dyn", 0),
        "process_spawn": text.count(".spawn") + c.get("Command", 0),
        "syscall_libc": c.get("libc", 0) + c.get("syscall", 0),
        "serde_serialize": c.get("Serialize", 0) + c.get("Deserialize", 0),
        "std_io_stream": c.get("stdin", 0) + c.get("stdout", 0),
        "unsafe": c.get("unsafe", 0),
    }
    return {k: v for k, v in seams.items() if v}


if __name__ == "__main__":
    import json
    import sys
    src = sys.stdin.read()
    print(json.dumps(scan_file("<stdin>", src), indent=2))
