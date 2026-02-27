"""Builds assets/fonts/kestrel-icons.ttf: Google's Material Symbols Rounded (Apache-2.0), filled,
weight 500, cut down to kestrel's icons, and prints the codepoints for src/ui/icon.rs.

map.txt: one line per kestrel icon, `kestrel-name material_name` (any of the ~4,300 names in
fonts.google.com/icons). To add an icon: add a line, rebuild, paste the codepoint into icon.rs.

    d=$(mktemp -d); u=https://raw.githubusercontent.com/google/material-design-icons/master/variablefont
    f='MaterialSymbolsRounded%5BFILL%2CGRAD%2Copsz%2Cwght%5D'
    curl -sLo $d/msr.ttf $u/$f.ttf; curl -sLo $d/cp.txt $u/$f.codepoints
    uvx --from fonttools python -I tools/icons/build.py $d/msr.ttf $d/cp.txt \\
        tools/icons/map.txt assets/fonts/kestrel-icons.ttf /dev/stdout
"""
import sys
from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

font_path, cp_path, map_path, out, listing = sys.argv[1:6]
cps = dict(line.split() for line in open(cp_path))
rows = [(k, int(cps[m], 16)) for k, m in (line.split() for line in open(map_path))]

font = TTFont(font_path)
o = subset.Options()
o.layout_features = []  # the glyphs are reached by codepoint, not by ligature
o.name_IDs = ["*"]
o.notdef_outline = True
s = subset.Subsetter(o)
s.populate(unicodes=sorted({c for _, c in rows}))
s.subset(font)
# Filled, medium weight, normal grade, the 24 px optical size.
font = instancer.instantiateVariableFont(font, {"FILL": 1, "wght": 500, "GRAD": 0, "opsz": 24})
for r in font["name"].names:
    if r.nameID in (1, 4, 16):
        r.string = "kestrel Icons"
    elif r.nameID == 6:
        r.string = "kestrelIcons"
    elif r.nameID in (2, 17):
        r.string = "Regular"
font.save(out)
open(listing, "w").write("".join(f"{k} {c:x}\n" for k, c in rows))
