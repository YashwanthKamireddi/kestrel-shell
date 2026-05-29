#!/usr/bin/env python3
"""Moves a file's literal gaps and paddings onto theme::tok (docs/DESIGN.md). On-grid values map
to their token; 10 and 14 (off the grid) snap to 12. Values with no token (1, 3, 5, 7, 28…) are
left and listed, for a person to decide. Usage: to_tokens.py FILE..."""
import re, sys

MAP = {0: "NONE", 2: "XXS", 4: "XS", 6: "S", 8: "M", 10: "L", 12: "L", 14: "L", 16: "XL",
       18: "XL", 20: "XXL", 24: "XXXL"}


def tok(v):
    f = float(v)
    if f.is_integer() and int(f) in MAP:
        return "tok::" + MAP[int(f)]
    return None


def one(m, left, where):
    name, args = m.group(1), m.group(2)
    nums = [a.strip() for a in args.strip("[]").split(",")]
    toks = [tok(n) for n in nums]
    if any(t is None for t in toks):
        left.append(f"{where}: .{name}({args})")
        return m.group(0)
    inner = ", ".join(toks)
    return f".{name}([{inner}])" if args.startswith("[") else f".{name}({inner})"


for path in sys.argv[1:]:
    src = open(path).read()
    left = []
    out = []
    for n, line in enumerate(src.split("\n"), 1):
        out.append(re.sub(r"\.(spacing|padding)\((\[[0-9][0-9., ]*\]|[0-9][0-9.]*)\)",
                          lambda m: one(m, left, f"{path}:{n}"), line))
    new = "\n".join(out)
    if new != src and "use crate::theme::tok;" not in new:
        new = re.sub(r"^(use [^\n]+;\n)", r"\1use crate::theme::tok;\n", new, count=1, flags=re.M)
    open(path, "w").write(new)
    for l in left:
        print("left", l)
