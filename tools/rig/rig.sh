#!/usr/bin/env bash
# kestrel test rig: runs kestrel inside a nested Hyprland on a hidden workspace
# (special:rig) with a headless 1920x1080 output, so nothing touches the real desktop.
# kestrel gets its own HOME and its own D-Bus session (it must not take over the desktop's
# notification service); system services (NetworkManager, BlueZ, UPower) are shared.
#
#   tools/rig/rig.sh start          start the nested compositor
#   tools/rig/rig.sh run            (re)start kestrel in it
#   tools/rig/rig.sh shot [file]    capture the output
#   tools/rig/rig.sh hypr ARGS      hyprctl against the nested compositor
#   tools/rig/rig.sh msg ARGS       send an IPC command to the rig's kestrel
#   tools/rig/rig.sh stop
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
WORK=${WORK:-$HOME/.cache/kestrel-rig}
BIN=${BIN:-$HERE/../../target/release/kestrel}
mkdir -p "$WORK/home/.config/kestrel"

sig() { cat "$WORK/sig"; }
display() { cat "$WORK/display"; }
nested() { HYPRLAND_INSTANCE_SIGNATURE=$(sig) timeout 5 hyprctl "$@"; }
kill_kestrel() {
    for p in $(pgrep -x kestrel || true); do
        if tr '\0' ' ' </proc/$p/environ 2>/dev/null | grep -q "HOME=$WORK/home "; then kill "$p"; fi
    done
}

case "${1:-}" in
start)
    hyprctl eval 'hl.window_rule({name="kestrel-rig", match={class="^aquamarine$"}, workspace="special:rig silent", float=true, size="1920 1080"})' >/dev/null
    before=$(ls "$XDG_RUNTIME_DIR" | grep -E '^wayland-[0-9]+$' | sort)
    setsid Hyprland -c "$HERE/hyprland.lua" >"$WORK/hypr.log" 2>&1 </dev/null &
    sleep 4
    after=$(ls "$XDG_RUNTIME_DIR" | grep -E '^wayland-[0-9]+$' | sort)
    comm -13 <(echo "$before") <(echo "$after") | head -1 >"$WORK/display"
    ls -t "$XDG_RUNTIME_DIR/hypr/" | head -1 >"$WORK/sig"
    nested output create headless KRIG >/dev/null
    nested output remove WAYLAND-1 >/dev/null || true
    nested eval 'hl.monitor({ output = "KRIG", mode = "1920x1080@60", position = "0x0", scale = 1 })' >/dev/null
    echo "rig up on $(display)"
    ;;
run)
    kill_kestrel; sleep 0.3
    env -i HOME="$WORK/home" PATH=/usr/bin:/usr/sbin XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" \
        WAYLAND_DISPLAY="$(display)" HYPRLAND_INSTANCE_SIGNATURE="$(sig)" LANG=en_US.UTF-8 \
        RUST_LOG="${RUST_LOG:-kestrel=info}" KESTREL_SOCKET="$WORK/kestrel.sock" \
        setsid dbus-run-session -- "$BIN" >"$WORK/kestrel.log" 2>&1 </dev/null &
    sleep "${WAIT:-2}"
    tail -5 "$WORK/kestrel.log"
    ;;
shot)
    out=${2:-$WORK/shot.png}
    WAYLAND_DISPLAY=$(display) timeout 10 grim -o KRIG "$out"
    echo "$out"
    ;;
hypr) shift; nested "$@" ;;
msg) shift; KESTREL_SOCKET="$WORK/kestrel.sock" "$BIN" "$@" ;;
stop)
    kill_kestrel
    pkill -f "Hyprland -c $HERE/hyprland.lua" || true
    hyprctl reload >/dev/null
    ;;
*) sed -n '2,15p' "$0"; exit 1 ;;
esac
