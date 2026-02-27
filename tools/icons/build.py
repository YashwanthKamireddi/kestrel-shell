"""Builds assets/fonts/kestrel-icons.ttf: Fluent UI System Icons (MIT) cut down to kestrel's
icons, and prints the codepoints for src/ui/icon.rs.

map.txt: one line per kestrel icon, `kestrel-name fluent_name` (the filled 20 px design) or
`kestrel-name r:fluent_name` (the regular one, given its own codepoint from U+E000).

    d=$(mktemp -d); u=https://raw.githubusercontent.com/microsoft/fluentui-system-icons/main/fonts
    for f in Filled.ttf Filled.json Regular.ttf Regular.json; do curl -sLo $d/$f $u/FluentSystemIcons-$f; done
    uvx --from fonttools python -I tools/icons/build.py $d/Filled.ttf $d/Filled.json \
        $d/Regular.ttf $d/Regular.json tools/icons/map.txt assets/fonts/kestrel-icons.ttf /dev/stdout
"""
import json, os, sys, tempfile
from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.merge import Merger
filled, fjs, regular, rjs, mp, out, rs = sys.argv[1:8]
tmp = tempfile.mkdtemp(); tmp_a, tmp_b = os.path.join(tmp, "a.ttf"), os.path.join(tmp, "b.ttf")
fc, rc = json.load(open(fjs)), json.load(open(rjs))
rows, fset, rmap = [], set(), {}
nxt = 0xE000
used = {fc[f"ic_fluent_{l.split()[1]}_20_filled"] for l in open(mp) if not l.split()[1].startswith("r:")}
for line in open(mp):
    k, f = line.split()
    if f.startswith("r:"):
        src = rc[f"ic_fluent_{f[2:]}_20_regular"]
        if src not in rmap:
            while nxt in used: nxt += 1
            rmap[src] = nxt; nxt += 1
        rows.append((k, rmap[src]))
    else:
        c = fc[f"ic_fluent_{f}_20_filled"]; fset.add(c); rows.append((k, c))
def cut(path, cps, remap=None):
    font = TTFont(path)
    o = subset.Options(); o.layout_features = []; o.name_IDs = ['*']; o.notdef_outline = True; o.glyph_names = True
    s = subset.Subsetter(o); s.populate(unicodes=sorted(cps)); s.subset(font)
    if remap:
        for t in font['cmap'].tables:
            t.cmap = {remap[u]: g for u, g in t.cmap.items() if u in remap}
    return font
a = cut(filled, fset); a.save(tmp_a)
b = cut(regular, set(rmap), rmap)
# glyph names must not clash with the filled ones
order = b.getGlyphOrder(); ren = {g: (g if g == '.notdef' else g + ".r") for g in order}
glyf, hmtx = b['glyf'], b['hmtx']
glyf.glyphs = {ren[g]: glyf.glyphs[g] for g in order}; glyf.glyphOrder = [ren[g] for g in order]
hmtx.metrics = {ren[g]: hmtx.metrics[g] for g in order}
for t in b['cmap'].tables: t.cmap = {u: ren[g] for u, g in t.cmap.items()}
b.setGlyphOrder([ren[g] for g in order]); b['post'].formatType = 3.0
b.save(tmp_b)
font = Merger().merge([tmp_a, tmp_b])
for r in font['name'].names:
    if r.nameID in (1, 4, 16): r.string = "kestrel Icons"
    elif r.nameID == 6: r.string = "kestrelIcons"
    elif r.nameID == 2: r.string = "Regular"
font.save(out)
open(rs, "w").write("".join(f"{k} {c:x}\n" for k, c in rows))
