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
#   tools/rig/rig.sh lock           the lock screen (`kestrel lock`) in the nested compositor; in a
#                                   devtools build it takes the password "kestrel", not PAM
#                                   (KESTREL_LOCK_PAM=1 for the real check). docs/LOCK.md
#   tools/rig/rig.sh bus CMD...     run CMD on the rig's private session bus
#   tools/rig/rig.sh bench          measure a fresh kestrel idle and under CPU load (RUNS=3 runs,
#                                   WINDOW=60 s each); table on screen, dated lines in bench.tsv
#   tools/rig/rig.sh stop
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
WORK=${WORK:-$HOME/.cache/kestrel-rig}
BIN=${BIN:-$HERE/../../target/release/kestrel}
# The renderer the installed desktop uses (from its service file), so the rig draws what the desktop shows
# (2026-11-17: it drew with the GPU renderer while the desktop used the CPU one, and every
# picture shown that day came from the wrong renderer). ICED_BACKEND=… overrides it.
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
# The rig's kestrel(s), by HOME: never a process name alone, which is also the desktop's shell.
# (`|| true`: under set -e a last non-matching one ended the script silently, whenever the
# desktop's kestrel happened to be listed after the rig's.)
rig_kestrels() {
    for p in $(pgrep -x kestrel || true); do
        if tr '\0' '\n' </proc/"$p"/environ 2>/dev/null | grep -q "^HOME=$WORK/home$"; then echo "$p"; fi
    done
}
kill_kestrel() {
    for p in $(rig_kestrels); do kill "$p"; done
}
# The Wayland sockets in the runtime dir (to find the one the nested compositor adds).
wayland_sockets() {
    local f
    for f in "$XDG_RUNTIME_DIR"/wayland-*; do
        case ${f##*/} in wayland-*[!0-9]* | wayland-) ;; *) echo "${f##*/}" ;; esac
    done | sort
}

# --- bench: the numbers of the README, measured the same way every time -------
# Everything is read from /proc of the rig's kestrel (found by its HOME, never by name alone).
# CPU time in /proc is in clock ticks (CLK_TCK, 100 here): N ticks over T s is N / CLK_TCK / T
# of one core, so 18 ticks in 10 s is 1.8 %, not 18 % (got wrong twice on 2026-11-15).
TICK=$(getconf CLK_TCK)
LOAD_PIDS=()

# Scheduler switches per thread, voluntary + involuntary: "tid switches name" (a thread that
# ends while being read is left out; the name goes last, it may contain spaces).
bench_switches() {
    local t n name
    for t in /proc/"$1"/task/*; do
        n=$(awk '/^(voluntary|nonvoluntary)_ctxt_switches:/ { s += $2 } END { print s + 0 }' "$t/status" 2>/dev/null) || continue
        name=$(cat "$t/comm" 2>/dev/null) || continue
        echo "${t##*/} $n $name"
    done
}
# utime + stime in ticks (fields 14 and 15 of stat; the name in brackets may contain spaces, so
# the fields are counted after its closing bracket).
bench_ticks() { sed 's/.*) //' /proc/"$1"/stat | awk '{ print $12 + $13 }'; }
bench_pss() { awk '/^Pss:/ { print $2 }' /proc/"$1"/smaps_rollup; }
# PSS in kB by what is mapped: "heap binary buffers fonts libraries other". Heap is the brk
# heap plus anonymous maps (malloc's arenas and large blocks, thread stacks); buffers are the
# shared screen buffers (memfd, /dev/shm, DRM, dma-buf).
bench_mappings() {
    local exe
    exe=$(readlink /proc/"$1"/exe)
    awk -v exe="$exe" '
        /^[0-9a-f]+-[0-9a-f]+ / {
            path = ""; for (i = 6; i <= NF; i++) path = path (i > 6 ? " " : "") $i
            sub(/ \(deleted\)$/, "", path)
            if (path == "" || path == "[heap]") kind = "heap"
            else if (path == exe) kind = "binary"
            else if (path ~ /^\/memfd:|^\/dev\/shm\/|^\/dev\/dri\/|dmabuf|^\/SYSV/) kind = "buffers"
            else if (path ~ /\.(ttf|otf|ttc|woff2?)$/) kind = "fonts"
            else if (path ~ /\.so(\.[0-9]+)*$/) kind = "libraries"
            else kind = "other"
            next
        }
        /^Pss:/ { kb[kind] += $2 }
        END { printf "%d %d %d %d %d %d\n", kb["heap"], kb["binary"], kb["buffers"], kb["fonts"], kb["libraries"], kb["other"] }
    ' /proc/"$1"/smaps
}
# One WINDOW-second measurement of pid $1: prints the threads that woke (busiest first) and sets
# B_WAKE (switches per second, all threads), B_CPU (% of one core), B_PSS (kB), B_MAP
# (bench_mappings). Only threads alive at both ends count (matched by tid).
bench_window() {
    local pid=$1 a b t0 t1 c0 c1 threads
    a=$(bench_switches "$pid"); c0=$(bench_ticks "$pid"); t0=$(date +%s.%N)
    sleep "$WINDOW"
    b=$(bench_switches "$pid"); c1=$(bench_ticks "$pid"); t1=$(date +%s.%N)
    B_CPU=$(awk -v n=$((c1 - c0)) -v hz="$TICK" -v a="$t0" -v b="$t1" 'BEGIN { printf "%.2f", n / hz / (b - a) * 100 }')
    B_PSS=$(bench_pss "$pid")
    B_MAP=$(bench_mappings "$pid")
    threads=$(awk -v a="$t0" -v b="$t1" '
        NR == FNR { before[$1] = $2; next }
        ($1 in before) {
            name = $3; for (i = 4; i <= NF; i++) name = name " " $i
            printf "%.2f %s (%s)\n", ($2 - before[$1]) / (b - a), name, $1
        }
    ' <(echo "$a") <(echo "$b") | sort -rn)
    awk '$1 > 0 { r = $1; $1 = ""; printf "    %-28s %8.2f /s\n", substr($0, 2), r }' <<<"$threads"
    B_WAKE=$(awk '{ s += $1 } END { printf "%.2f", s }' <<<"$threads")
}
# Open each panel PANEL_REPEATS times: "panel ms" per open, from the command to the first frame
# presented after it (KESTREL_TRACE's "frame" lines in kestrel.log; any surface's, so a bar
# frame that happens to fall in between counts too). Not the power menu: in the rig its second
# click really restarts the machine.
bench_panels() {
    local panel t0
    grep -q '^TRACE .* frame ' "$WORK/kestrel.log" || return 0
    for panel in $BENCH_PANELS; do
        for _ in $(seq "$PANEL_REPEATS"); do
            t0=$(date +%s.%3N)
            KESTREL_SOCKET="$WORK/kestrel.sock" "$BIN" toggle "$panel" >/dev/null 2>&1 || true
            sleep 0.6
            KESTREL_SOCKET="$WORK/kestrel.sock" "$BIN" close >/dev/null 2>&1 || true
            sleep 0.4
            awk -v p="$panel" -v t0="$t0" '$1 == "TRACE" && $2 + 0 > t0 + 0 && $3 == "frame" { printf "%s %.0f\n", p, ($2 - t0) * 1000; exit }' "$WORK/kestrel.log"
        done
    done
}
# One busy loop per core beside the rig, each kept by its pid and stopped by it.
load_start() {
    for _ in $(seq "$(nproc)"); do
        bash -c 'while :; do :; done' &
        LOAD_PIDS+=("$!")
    done
}
load_stop() {
    if [ ${#LOAD_PIDS[@]} -gt 0 ]; then kill "${LOAD_PIDS[@]}" 2>/dev/null || true; fi
    LOAD_PIDS=()
}
# p50, max and count of the numbers on stdin, one per line.
p50_max() { sort -n | awk '{ v[NR] = $1 } END { if (NR) printf "%d %d %d\n", v[int((NR + 1) / 2)], v[NR], NR }'; }
mb() { awk -v k="$1" 'BEGIN { printf "%.1f", k / 1024 }'; }

case "${1:-}" in
start)
    # One rig: a second start left two nested compositors running (found 2026-11-17).
    if [ -s "$WORK/pid" ] && [ "$(cat /proc/"$(cat "$WORK/pid")"/comm 2>/dev/null)" = Hyprland ]; then
        echo "rig already up on $(display)"
        exit 0
    fi
    hyprctl eval 'hl.window_rule({name="kestrel-rig", match={class="^aquamarine$"}, workspace="special:rig silent", float=true, size="1920 1080"})' >/dev/null
    before=$(wayland_sockets)
    : >"$WORK/sig"
    # The rig's HOME, so what it starts (an app kestrel opens, a screenshot, a recording) uses
    # the rig's profile and folders: with the real HOME, a file opened in the rig became a tab
    # in the running Chromium (2026-11-16) and captures landed in the real Pictures.
    HOME="$WORK/home" setsid Hyprland -c "$HERE/hyprland.lua" >"$WORK/hypr.log" 2>&1 </dev/null &
    pid=$!
    echo "$pid" >"$WORK/pid"
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
    after=$(wayland_sockets)
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
lock) # the lock screen, locking only the nested compositor (never the desktop's)
    nested version >/dev/null # refuses when there is no nested compositor
    # Never the desktop's display: an empty or wrong one would lock the real session.
    d=$(display)
    if [ -z "$d" ] || [ "$d" = "${WAYLAND_DISPLAY:-}" ] || [ ! -S "$XDG_RUNTIME_DIR/$d" ]; then
        echo "rig: no nested display (refusing to lock the desktop)" >&2
        exit 1
    fi
    preview=lock
    [ -n "${KESTREL_LOCK_PAM:-}" ] && preview=
    env -i HOME="$WORK/home" PATH=/usr/bin:/usr/sbin XDG_RUNTIME_DIR="$XDG_RUNTIME_DIR" \
        WAYLAND_DISPLAY="$d" LANG=en_US.UTF-8 RUST_LOG="${RUST_LOG:-kestrel=info}" \
        ${preview:+KESTREL_PREVIEW=$preview} ${KESTREL_TRACE:+KESTREL_TRACE=1} \
        ${ICED_BACKEND:+ICED_BACKEND=$ICED_BACKEND} \
        setsid "$BIN" lock >"$WORK/lock.log" 2>&1 </dev/null &
    echo $! >"$WORK/lock.pid"
    sleep "${WAIT:-1}"
    tail -5 "$WORK/lock.log"
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
    pid=$(rig_kestrels | head -1)
    addr=$(tr '\0' '\n' </proc/"$pid"/environ | grep ^DBUS_SESSION_BUS_ADDRESS= | cut -d= -f2-)
    DBUS_SESSION_BUS_ADDRESS="$addr" "$@" ;;
msg) shift; KESTREL_SOCKET="$WORK/kestrel.sock" "$BIN" "$@" ;;
bench) # Phase 0's numbers: idle, then beside a busy loop per core; RUNS runs and their spread
    RUNS=${RUNS:-3} WINDOW=${WINDOW:-60} SETTLE=${SETTLE:-10} PANEL_REPEATS=${PANEL_REPEATS:-5}
    BENCH_PANELS=${BENCH_PANELS:-launcher overview control calendar clipboard emoji media}
    if [ ! -x "$BIN" ]; then echo "rig: no kestrel at $BIN" >&2; exit 1; fi
    # The load loops never outlive the bench, however it ends.
    trap load_stop EXIT
    trap 'exit 130' INT TERM
    "$0" start >/dev/null
    rev=$(git -C "$HERE/../.." describe --always --dirty 2>/dev/null || echo unknown)
    results=$WORK/bench.tsv
    if [ ! -s "$results" ]; then
        printf 'date\trev\trun\tphase\twakeups_per_s\tcpu_pct\tpss_mb\theap_mb\tbinary_mb\tbuffers_mb\tfonts_mb\tlibs_mb\tother_mb\topen_p50_ms\topen_max_ms\n' >"$results"
    fi
    lines=
    for run in $(seq "$RUNS"); do
        # A fresh kestrel each run, traced for the panel timings (the trace writes only when a
        # frame is presented, so it adds nothing while idle). Its pid is the new one.
        old=$(rig_kestrels)
        KESTREL_TRACE=1 WAIT=0 "$0" run >/dev/null
        pid=
        for _ in $(seq 50); do
            for p in $(rig_kestrels); do
                if ! grep -qx "$p" <<<"$old"; then pid=$p; fi
            done
            if [ -n "$pid" ]; then break; fi
            sleep 0.2
        done
        if [ -z "$pid" ]; then echo "rig: kestrel did not start (see $WORK/kestrel.log)" >&2; exit 1; fi
        sleep "$SETTLE"
        for phase in idle load; do
            if [ "$phase" = load ]; then load_start; sleep 2; fi
            echo "run $run/$RUNS, $phase, ${WINDOW} s, pid $pid; threads that woke:"
            bench_window "$pid"
            opens=$(bench_panels)
            load_stop
            if [ ! -d /proc/"$pid" ]; then echo "rig: kestrel ended during the bench (see $WORK/kestrel.log)" >&2; exit 1; fi
            read -r heap binary buffers fonts libs other <<<"$B_MAP"
            printf '  %-26s %10s\n' "wakeups (all threads)" "$B_WAKE /s" "CPU (one core = 100 %)" "$B_CPU %" \
                PSS "$(mb "$B_PSS") MB" "  heap + anonymous" "$(mb "$heap") MB" "  binary" "$(mb "$binary") MB" \
                "  screen buffers" "$(mb "$buffers") MB" "  fonts" "$(mb "$fonts") MB" \
                "  libraries" "$(mb "$libs") MB" "  other" "$(mb "$other") MB"
            open_p50=- open_max=-
            if [ -n "$opens" ]; then
                echo "  open to first frame, ms (p50 max n):"
                for panel in $BENCH_PANELS; do
                    printf '    %-12s %s\n' "$panel" "$(awk -v p="$panel" '$1 == p { print $2 }' <<<"$opens" | p50_max)"
                done
                read -r open_p50 open_max _ < <(cut -d' ' -f2 <<<"$opens" | p50_max)
            else
                echo "  open to first frame: no KESTREL_TRACE frames in kestrel.log"
            fi
            line=$(printf '%s\t' "$(date -Iseconds)" "$rev" "$run" "$phase" "$B_WAKE" "$B_CPU" "$(mb "$B_PSS")" \
                "$(mb "$heap")" "$(mb "$binary")" "$(mb "$buffers")" "$(mb "$fonts")" "$(mb "$libs")" \
                "$(mb "$other")" "$open_p50" "$open_max")
            echo "${line%$'\t'}" >>"$results"
            lines+=${line%$'\t'}$'\n'
        done
    done
    # Spread over the runs, (max - min) / median: Phase 0 wants runs within 10 % of each other.
    echo
    printf '%-6s %-14s %9s %9s %9s %8s\n' phase number min median max spread
    for phase in idle load; do
        for col in 5:wakeups/s 6:cpu% 7:pss_mb 8:heap_mb 10:buffers_mb 14:open_p50_ms 15:open_max_ms; do
            awk -F'\t' -v ph="$phase" -v c="${col%%:*}" '$4 == ph && $c != "-" { print $c }' <<<"$lines" | sort -g |
                awk -v ph="$phase" -v name="${col#*:}" '
                    { v[NR] = $1 }
                    END {
                        if (!NR) exit
                        m = v[int((NR + 1) / 2)]; s = m ? (v[NR] - v[1]) / m * 100 : 0
                        printf "%-6s %-14s %9.2f %9.2f %9.2f %7.1f%%%s\n", ph, name, v[1], m, v[NR], s, (s > 10 ? "  over 10 %" : "")
                    }'
        done
    done
    echo "results: $results"
    ;;
stop)
    kill_kestrel
    # The pid kept at start (a pattern match could hit any shell whose command names it).
    if [ -s "$WORK/pid" ] && [ "$(cat /proc/"$(cat "$WORK/pid")"/comm 2>/dev/null)" = Hyprland ]; then
        kill "$(cat "$WORK/pid")"
    fi
    : >"$WORK/pid"
    : >"$WORK/sig"
    hyprctl reload >/dev/null
    ;;
*) sed -n '2,22p' "$0"; exit 1 ;;
esac
