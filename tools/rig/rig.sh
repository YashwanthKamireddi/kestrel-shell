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
#   tools/rig/rig.sh pointer move|click|scroll X Y [right|LINES]   virtual pointer
#   tools/rig/rig.sh type TEXT|-k KEY   type into the focused surface (wtype)
#   tools/rig/rig.sh msg ARGS       send an IPC command to the rig's kestrel
#   tools/rig/rig.sh prompt WHICH   the prompt process (`kestrel prompt`, devtools build) asking a
#                                   made-up question: password|password-wrong|wifi|wifi-wrong|pair-*
#   tools/rig/rig.sh bus CMD...     run CMD on the rig's private session bus
#   tools/rig/rig.sh stop
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
WORK=${WORK:-$HOME/.cache/kestrel-rig}
BIN=${BIN:-$HERE/../../target/release/kestrel}
# The renderer the installed desktop uses (from its service file), so the rig draws what the user sees
# (2026-11-17: it drew with the GPU renderer while the desktop used the CPU one, and every
# picture shown to the user that day came from the wrong renderer). ICED_BACKEND=… overrides it.
ICED_BACKEND=${ICED_BACKEND:-$(sed -n 's/^Environment=ICED_BACKEND=//p' "$HERE/../../packaging/arch/kestrel.service")}
mkdir -p "$WORK/home/.config/kestrel"

sig() { cat "$WORK/sig"; }
display() { cat "$WORK/display"; }
# Never the desktop's own compositor: a wrong signature once put a headless output on the
# live session and windows opened on it, out of sight.
nested() {
    local s; s=$(sig)
    if [ -z "$s" ] || [ "$s" = "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]; then
        echo "rig: no nested compositor (refusing to touch the desktop's)" >&2
        return 1
    fi
    HYPRLAND_INSTANCE_SIGNATURE=$s timeout 5 hyprctl "$@"
}
kill_kestrel() {
    for p in $(pgrep -x kestrel || true); do
        if tr '\0' '\n' </proc/$p/environ 2>/dev/null | grep -q "^HOME=$WORK/home$"; then kill "$p"; fi
    done
}

case "${1:-}" in
start)
    hyprctl eval 'hl.window_rule({name="kestrel-rig", match={class="^aquamarine$"}, workspace="special:rig silent", float=true, size="1920 1080"})' >/dev/null
    before=$(ls "$XDG_RUNTIME_DIR" | grep -E '^wayland-[0-9]+$' | sort)
    : >"$WORK/sig"
    # The rig's HOME, so what it starts (an app kestrel opens, a screenshot, a recording) uses
    # the rig's profile and folders: with the real HOME, a file opened in the rig became a tab
    # in the user's running Chromium (2026-11-16) and captures landed in the user's Pictures.
    HOME="$WORK/home" setsid Hyprland -c "$HERE/hyprland.lua" >"$WORK/hypr.log" 2>&1 </dev/null &
    pid=$!
    # Its instance is the one whose lock file names its process (not simply the newest).
    for _ in $(seq 40); do
        for d in "$XDG_RUNTIME_DIR"/hypr/*/; do
            if [ "$(head -1 "$d/hyprland.lock" 2>/dev/null)" = "$pid" ]; then
                basename "$d" >"$WORK/sig"
            fi
        done
        [ -s "$WORK/sig" ] && break
        sleep 0.2
    done
    if [ ! -s "$WORK/sig" ]; then
        echo "rig: nested compositor did not start (see $WORK/hypr.log)" >&2
        kill "$pid" 2>/dev/null || true
        exit 1
    fi
    sleep 1
    after=$(ls "$XDG_RUNTIME_DIR" | grep -E '^wayland-[0-9]+$' | sort)
    comm -13 <(echo "$before") <(echo "$after") | head -1 >"$WORK/display"
    nested output create headless KRIG >/dev/null
    nested output remove WAYLAND-1 >/dev/null || true
    nested eval 'hl.monitor({ output = "KRIG", mode = "1920x1080@60", position = "0x0", scale = 1 })' >/dev/null
    echo "rig up on $(display)"
    ;;
run)
    kill_kestrel; sleep 0.3
    env -i HOME="$WORK/home" PATH=/usr/bin:/usr/sbin XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" \
        WAYLAND_DISPLAY="$(display)" HYPRLAND_INSTANCE_SIGNATURE="$(sig)" LANG=en_US.UTF-8 \
        RUST_LOG="${RUST_LOG:-kestrel=info}" KESTREL_SOCKET="$WORK/kestrel.sock" KESTREL_NO_AIRPODS=1 KESTREL_NO_BT_AGENT=1 ${KESTREL_TRACE:+KESTREL_TRACE=1} ${ICED_BACKEND:+ICED_BACKEND=$ICED_BACKEND} \
        setsid dbus-run-session -- "$BIN" >"$WORK/kestrel.log" 2>&1 </dev/null &
    sleep "${WAIT:-2}"
    tail -5 "$WORK/kestrel.log"
    ;;
prompt) # the prompt process with a made-up question, no real agents (devtools builds)
    env -i HOME="$WORK/home" PATH=/usr/bin:/usr/sbin XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" \
        WAYLAND_DISPLAY="$(display)" LANG=en_US.UTF-8 RUST_LOG="${RUST_LOG:-kestrel=info}" \
        KESTREL_PREVIEW="${2:?which question}" ${ICED_BACKEND:+ICED_BACKEND=$ICED_BACKEND} \
        setsid "$BIN" prompt >"$WORK/prompt.log" 2>&1 </dev/null &
    sleep "${WAIT:-1}"
    tail -5 "$WORK/prompt.log"
    ;;
shot)
    out=${2:-$WORK/shot.png}
    WAYLAND_DISPLAY=$(display) timeout 10 grim -o KRIG "$out"
    echo "$out"
    ;;
hypr) shift; nested "$@" ;;
pointer) shift; WAYLAND_DISPLAY=$(display) "$HERE/../../target/rig-pointer/release/rig-pointer" "$@" ;;
type) shift; WAYLAND_DISPLAY=$(display) wtype "$@" ;;
bus) # run a command on kestrel's private session bus (e.g. notify-send)
    shift
    # Every kestrel's HOME checked (`|| true`: under set -e a last non-matching one ended the
    # script silently, whenever the desktop's kestrel happened to be listed after the rig's).
    pid=$(for p in $(pgrep -x kestrel || true); do
        if tr '\0' '\n' </proc/$p/environ 2>/dev/null | grep -q "^HOME=$WORK/home$"; then echo "$p"; fi
    done | head -1)
    addr=$(tr '\0' '\n' </proc/$pid/environ | grep ^DBUS_SESSION_BUS_ADDRESS= | cut -d= -f2-)
    DBUS_SESSION_BUS_ADDRESS="$addr" "$@" ;;
msg) shift; KESTREL_SOCKET="$WORK/kestrel.sock" "$BIN" "$@" ;;
stop)
    kill_kestrel
    pkill -f "Hyprland -c $HERE/hyprland.lua" || true
    : >"$WORK/sig"
    hyprctl reload >/dev/null
    ;;
*) sed -n '2,15p' "$0"; exit 1 ;;
esac
