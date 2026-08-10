#!/bin/sh
# Design-kit check (docs/DESIGN.md): counts literal radii, paddings, gaps, colour triples and
# icon sizes outside src/ui and src/theme. A count above the baseline fails; lower is progress (update the
# baseline with `tools/lint/tokens.sh --update` once panels have moved onto the kit).
set -e
cd "$(dirname "$0")/../.."
files=$(find src -name '*.rs' ! -path 'src/ui/*' ! -path 'src/theme/*')
count() { grep -hoE "$1" $files | wc -l | tr -d ' '; }
radius=$(count '\.radius\([0-9][0-9.]*\)|rounded\([0-9][0-9.]*\)')
padding=$(count '\.padding\((\[[0-9][0-9., ]*\]|[0-9][0-9.]*)\)')
spacing=$(count '\.spacing\([0-9][0-9.]*\)')
gaps=$(count 'Space::new\(\)\.(height|width)\([0-9][0-9.]*\)')
alphas=$(count 'ink\.scale_alpha\(0\.[0-9]+')
edges=$(count '(top|right|bottom|left): [0-9][0-9.]*,')
colors=$(count '\.colors\(')
icons=$(count '\b(icon|tray_icon_at)\([^;]*, [0-9]+\.[0-9]+[,)]')
now="radius $radius
padding $padding
spacing $spacing
edges $edges
alphas $alphas
gaps $gaps
colors $colors
icons $icons"
base=tools/lint/tokens.baseline
if [ "$1" = "--update" ]; then
    echo "$now" >"$base"
    echo "$now"
    exit 0
fi
fail=0
echo "$now" | while read -r k v; do
    b=$(awk -v k="$k" '$1 == k { print $2 }' "$base")
    if [ "$v" -gt "$b" ]; then
        echo "design kit: $v literal $k (baseline $b). Use theme::tok and ui::control::Kind instead (docs/DESIGN.md)." >&2
        exit 1
    fi
done || fail=1
exit $fail
