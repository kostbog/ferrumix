#!/usr/bin/env python3
"""Approximate rustfmt's call-formatting rules so hand written code lands
close to `cargo fmt` output.  Reports:

  * multi-line calls whose arguments would fit on one line (args <= 60 and
    total <= 100) — rustfmt would join them,
  * single-line calls whose argument list is wider than 60 characters —
    rustfmt would explode them, one argument per line.

This is a development aid only; the real check is `cargo fmt --check` in CI.
"""
import re
import sys

MAX_WIDTH = 100
FN_CALL_WIDTH = 60


def split_args(args):
    """Split an argument list on top level commas."""
    out, depth, cur, in_str, esc = [], 0, "", False, False
    for ch in args:
        if in_str:
            cur += ch
            if esc:
                esc = False
            elif ch == "\\":
                esc = True
            elif ch == '"':
                in_str = False
            continue
        if ch == '"':
            in_str = True
            cur += ch
        elif ch in "([{":
            depth += 1
            cur += ch
        elif ch in ")]}":
            depth -= 1
            cur += ch
        elif ch == "," and depth == 0:
            out.append(cur.strip())
            cur = ""
        else:
            cur += ch
    if cur.strip():
        out.append(cur.strip())
    return out


def check(path):
    lines = open(path).read().split("\n")
    problems = []
    i = 0
    while i < len(lines):
        line = lines[i]
        stripped = line.strip()
        indent = len(line) - len(line.lstrip())
        # multi-line call: something ending in "(" alone
        m = re.match(r"^(.*\w!?)\($", stripped)
        if m and not stripped.startswith("//"):
            prefix = m.group(1)
            body, j, ok = [], i + 1, False
            while j < len(lines):
                inner = lines[j].strip()
                if inner in (");", ")", ")?;", ").unwrap();", "):"):
                    ok = True
                    break
                if inner.endswith("{") or inner.startswith("//"):
                    break
                body.append(inner)
                j += 1
            if ok and body:
                args = " ".join(b.rstrip(",") + "," for b in body)[:-1]
                total = indent + len(prefix) + 1 + len(args) + 2
                if len(args) <= FN_CALL_WIDTH and total <= MAX_WIDTH:
                    problems.append(
                        (i + 1, "join", f"{prefix}({args});  [{total} cols]")
                    )
                i = j + 1
                continue
        # single-line call
        m = re.match(r"^(.*?\w!?)\((.*)\);$", stripped)
        if m and not stripped.startswith("//"):
            args = m.group(2)
            if len(args) > FN_CALL_WIDTH and len(split_args(args)) > 1:
                problems.append((i + 1, "split", stripped[:90]))
        i += 1
    return problems


status = 0
for path in sys.argv[1:]:
    for lineno, kind, text in check(path):
        status = 1
        print(f"{path}:{lineno}: {kind}: {text}")
sys.exit(status)
