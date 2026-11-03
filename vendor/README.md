# Vendored dependencies

Four crates are vendored and patched. Every change against the published source is listed below,
numbered once across the whole file (1-16 and 26-27 iced_layershell, 17-20 and 28 iced_tiny_skia, 21-23
tiny-skia, 24-25 softbuffer). In the code most are marked `kestrel patch` (a few say only
`kestrel:` or carry no marker; those are named below), so `grep -rn kestrel vendor/` finds them.

Checked 2026-11-18 by diffing each directory against its original:

| Directory | Original | Wired in |
|---|---|---|
| `iced_layershell/` | MalpenZibo/iced_layershell tag `v0.6.2` (MIT), GitHub tarball; not on crates.io | `Cargo.toml`: `iced = { package = "iced_layershell", path = … }` |
| `iced_tiny_skia/` | crates.io `iced_tiny_skia` 0.14.1 (MIT) | `[patch.crates-io]` |
| `tiny-skia/` | crates.io `tiny-skia` 0.11.4 (BSD-3-Clause) | `[patch.crates-io]` |
| `softbuffer/` | MalpenZibo/softbuffer branch `wayland-argb` (MIT/Apache-2.0) = crates.io `softbuffer` 0.4.8 plus `wl_shm::Format::Argb8888` instead of `Xrgb8888` in `src/backends/wayland/buffer.rs` (two lines; MalpenZibo's change, not kestrel's) | `[patch.crates-io]` |

Files that differ only by packaging are not patches: `.cargo-ok` in each directory, the
crates.io copies' `Cargo.toml.orig` / `.cargo_vcs_info.json` / `Cargo.lock`, and softbuffer's
un-normalised `Cargo.toml` plus `examples/` and `run-wasm/` (the branch's git tree, not the
crates.io package).

"Upstream" for each patch comes from docs/research/ICED-UPSTREAM.md (section 3). **Upstream:
recommended** marks the patches worth offering upstream now (see "Upstream candidates" at the end).

## iced_layershell (v0.6.2)

About 400 added and 60 removed lines in `src/`, plus the new `src/handlers/window.rs`.

1. **No rasterising a frame that cannot be presented yet.** If the surface's frame callback is
   still pending, the draw pass skips it (no `RedrawRequested` update, no `ui.draw`) and marks it
   `needs_rerender`; the callback triggers the redraw. `src/application.rs` (draw loop, `defer`
   in the trace). Why: frames drawn before the callback were thrown away. With 2: CPU during
   animations ~15 % -> ~6 % of a core (2026-11-12).
   Upstream: partly. v0.6.2 already skips the *present* when `frame_pending`, but still runs the
   update and draw first. No issue. Optional candidate.
2. **`RedrawRequested` broadcast to subscriptions**, so `iced::window::frames()` (kestrel's
   animation ticks) follows the compositor's frame callbacks instead of a timer.
   `src/application.rs` (draw loop). Upstream: iced_winit does this, MalpenZibo's crate does not.
   No issue. **Upstream: recommended.**
3. **`Application::redraw_key`**: a surface whose key is `Some` and equal to the key it last
   presented is not redrawn. Checked at draw time (the one place every redraw passes), so input
   events and redraw retries no longer re-raster the wallpaper; a compositor request (configure,
   scale, resize, surface creation) sets `forced` and always draws. `src/application.rs`
   (builder method, gate in the draw loop, key and `forced` stored on a successful present,
   `forced` set on configure and resize), `src/surface_manager.rs` (`IcedSurface::redraw_key`,
   `forced`). Measured: CPU during panel animations ~43 % -> ~15 % of a core, because the
   full-screen wallpaper was re-rasterised on every animation frame (2026-11-12).
   Upstream: no (iced 0.14's reactive rendering skips redraws when an *event* changed nothing,
   not after a state update). No issue. Keep: an app-level API ashell does not need; 9 and 16
   depend on it.
4. **`KESTREL_TRACE=1` tracing**: wall-clock lines for every configure (layer and window),
   size request, keyboard-interactivity change, input region, viewport change, deferred frame
   (`defer`), retried frame (`redo`), `Outdated` UI, present (with draw and raster time per
   frame), `redraw_all` / `relayout_all`, wheel event, and view + layout time per surface.
   `src/lib.rs` (`trace_on`, `ktrace!`; no marker), `src/application.rs`,
   `src/handlers/layer_shell.rs`, `src/handlers/pointer.rs`, `src/handlers/window.rs`,
   `src/surface_manager.rs`, `src/ui_builder.rs`. Why: correlate flicker in a screen recording
   with what was committed (2026-11-12), then per-frame timing (2026-11-13/14).
   Upstream: no (iced has `comet`/`beacon` for its own shell). Keep.
5. **A redraw that produced messages is not presented on that pass**; the retry that follows
   (same loop, no frame of delay) draws it with them applied. The last retry always draws, so
   nothing is held back. `src/application.rs` (draw loop, `redo` in the trace). Why: a panel
   opened at its remembered height for a frame and then jumped to the one it reported
   (2026-11-14). Upstream: no. No issue. Optional candidate.
6. **Ordinary windows (xdg-toplevel) beside layer surfaces.** `new_window(WindowSettings)` opens
   one (title, app id, size, minimum size); the compositor sizes it like any app; closing it
   (Super+Q) removes it and sends `window::Event::CloseRequested` to subscriptions, even though
   the surface is gone. Surfaces carry a `Role` (Layer or Window); layer-only requests (anchor,
   layer, margin…) are skipped for windows. Part of the same change: a surface whose size
   changed is laid out again even without a message (it kept its old layout: cut off, stale).
   Files: `src/handlers/window.rs` (new), `src/handlers/mod.rs`, `src/state.rs` (`Role`,
   `register_role`, `default_size`, `xdg_shell`), `src/settings.rs` (`WindowSettings`),
   `src/task.rs` (`LayerShellCommand::NewWindow`, `new_window`), `src/surface_manager.rs`
   (pending windows created; `#[allow(clippy::too_many_lines)]`), `src/lib.rs` (exports),
   `src/application.rs` (xdg-shell bound, `resized` relayout, events of gone surfaces broadcast;
   the `WaylandSurface` import dropped). Why: Settings is an app window (app id
   `kestrel-settings`: tiles, moves, Alt-Tabs; 2026-11-14).
   Upstream: no ("no popups, no session lock, no subsurfaces unless explicitly requested").
   No issue. Keep.
7. **One `RedrawRequested` instant for every pass of a frame's redraw loop.** A retry (5) carried
   a fresh one, so time-driven widgets (smooth scrolling) stepped again, published again and
   forced another retry: each scrolling frame was built three times and logged "More than 3
   consecutive RedrawRequested cycles" (2026-11-15). `src/application.rs` (`frame_at`).
   Upstream: iced master's Frame Clock (iced#3527) solves the same class of problem in
   iced_winit; nothing in MalpenZibo's crate. No issue.
8. **A layer configure that changes nothing** (Hyprland sends a second one of the same size) is
   acknowledged with a bare commit, not a new frame. `src/handlers/layer_shell.rs`. Why: the new
   frame made the compositor hold two buffers of a full-screen surface for good (the wallpaper
   kept 2 x 8 MB; 2026-11-15). Upstream: no. No issue. Optional candidate.
9. **`NOT_READY` redraw key**: the surface is not presented at all, even for a configure (the
   wallpaper waits for its picture). `src/lib.rs` (`NOT_READY`; no marker),
   `src/application.rs` (draw loop). With 8: the wallpaper presents once, so the compositor holds
   one 8 MB buffer instead of two; screen buffers 17 -> 8.8 MB, live PSS 84 -> 77 MB
   (2026-11-15). Upstream: no (needs 3). Keep.
10. **The blur region rides the present's commit**: the separate `wl.commit()` after setting the
    region is removed. `src/surface_manager.rs`. Why: it applied the new region over the old
    buffer a frame before its picture; a panel opened as a blurred empty block for one frame
    (the user's recordings, 2026-11-15). Upstream: **went the other way**: MalpenZibo PR #28
    (2026-09-01) added the separate commit because "in practice that update is lost" when it rode
    the buffer commit. Re-check against #28's case at every upgrade. Keep.
11. **Widgets' `RedrawRequest::At(t)` honoured**: the event loop sleeps until the earliest such
    time and redraws only those surfaces. `src/application.rs` (`wake_at`). Why: `At` was dropped
    in the draw pass, so a widget pacing itself never drew again (`ui::breathe`, the charging
    pulse; 2026-11-15). Upstream: iced_winit honours it; v0.6.2 does not. No issue.
    **Upstream: recommended** (a bug against iced's own contract).
12. **`Application::clipboard`**: the app hands in the clipboard text fields copy to and paste
    from. kestrel gives its own (services::clipboard, wlr data-control), so iced's
    smithay-clipboard (a second Wayland connection and thread, woken by every input event: 32 of
    90 wakeups/s measured 2026-11-16) is not started. Without one the smithay clipboard is made
    as before (the separate prompt process keeps it). `src/application.rs` (builder method,
    `run` takes `mut app`, `run_action` takes `&mut dyn Clipboard`; 2026-11-17).
    Upstream: no; iced 0.15's rich clipboard (iced#3238) moves clipboard into `Shell` requests,
    so this hook changes shape there. Keep.
13. **Widget operations re-exported**: `scroll_to`, `snap_to`, `scroll_by`, `snap_to_end`,
    `move_cursor_to_end` beside upstream's `focus`, `focus_next`, `focus_previous`, `is_focused`.
    `src/lib.rs` (`widget` module). Why: keyboard scrolling in panels (shortcuts sheet), Settings
    pages and the wallpaper grid scrolled to a place, and the file picker's path field cursor
    (2026-11-13/15). Upstream: partly; it re-exports the focus
    operations only (MalpenZibo PR #26 added those, the same kind of change). No issue.
    **Upstream: recommended.** (Listed as a second "7" before 2026-11-18.)
14. **Cursor shape set again on every pointer `enter`**: the remembered shape is reset to
    `Hidden`, so the next update sets the shape with this enter's serial.
    `src/handlers/pointer.rs` (marked `kestrel:`). Why: Wayland wants the cursor image set on
    every enter; entering with an unchanged shape (after a screenshot's selection closed over
    the bar) left the pointer invisible (2026-11-13). Upstream: no. No issue.
    **Upstream: recommended.** (Unlisted before 2026-11-18.)
15. **`web-colors` feature** passed through to `iced_renderer/web-colors`. `Cargo.toml`. Why:
    the GPU renderer then blends in sRGB like the CPU one, GTK, Qt and browsers, so a translucent
    surface looks the same on either (2026-11-16). Upstream: no. No issue.
    **Upstream: recommended** (one line). (Unlisted before 2026-11-18.)
16. **`RedrawAll` and `RelayoutAll` pass the redraw key** (set `forced`).
    `src/application.rs` (`run_action`). Why: a redraw the renderer asks for (a picture finished
    uploading to the GPU) changes what a surface shows without changing kestrel's state; the key
    gate (3) dropped it and the picture never appeared (2026-11-16). Upstream: v0.6.2 already
    handles `RedrawAll` (MalpenZibo PR #36); only `forced` is kestrel's, and exists because of 3.
    Goes with 3. (Unlisted before 2026-11-18.)

26. The session lock (ext-session-lock-v1), for kestrel's lock screen (`kestrel lock`,
src/lock, docs/LOCK.md), on SCTK 0.20's own `session_lock` module: `lock_session()`,
`new_lock_surface(output)`, `unlock_session()` and `session_lock_events()` (`Locked`,
`Finished`); surfaces get a third `Role::Lock`. A lock surface is never committed without a
buffer (Hyprland treats that as a protocol error and drops the client, leaving the session
locked and unusable): its configure always draws a new frame, and layer property commands
(each of which commits) are refused for it. `unlock_session()` does nothing before `Locked`
(unlocking earlier is a protocol error) and flushes the connection at once, so the process may
end right after. Also `caps_lock()`: Caps Lock as the compositor last reported it (iced's
`Modifiers` has no Caps Lock), read on `ModifiersChanged`. Upstream's CLAUDE.md lists session
lock as not supported; this is the smallest addition (2026-11-18).

27. `OutputId::for_tests(n)`: an output id made by the app, so kestrel's tests can give the shell
a screen without a compositor (panels open, switch and close on it). Real ids still come only
from the compositor (2026-11-18).

## iced_tiny_skia (0.14.1)

17. **`adjust_clip_mask` touches only the pixels that change.** The original cleared the whole
    mask (a byte per screen pixel) and filled the rectangle again for every layer and primitive
    group; with a layer per control that rewrote tens of megabytes a frame on the full-screen
    overlay. Now the previous rectangle (remembered per mask in a thread-local) is wiped and the
    new one set, and the mask is declared to be exactly that rectangle (`set_rect_hint`, 21).
    `src/engine.rs`. With 24 and canvas caching (`ui::Memo`): overlay frame time 8.4 -> 5.1 ms
    on average in the rig (2026-11-13). Upstream: the same bug is open as iced#3368 (a different
    fix proposed, nothing merged); `iced_tiny_skia` is deleted in iced 0.15 (iced#3306). Keep.
18. **Each damaged rectangle grown by one physical pixel** on every side (clamped to the
    screen) before grouping. `src/window/compositor.rs`. Why: a shape at a fractional position
    anti-aliases a pixel past its bounds; repainting only the bounds left a thin line when a
    hover or press ended (2026-11-14). Upstream: same idea as the open iced#3486 (text only),
    not merged. Keep.
19. **Overlapping damage painted once as its bounding box** when the rectangles' summed area is
    over 60 % of the box. `src/window/compositor.rs`. Why: a scrolled list moves every item (27
    rectangles, 640 000 px for a 398 000 px panel); painting the overlaps again and again made a
    scroll frame ~10 ms, about half after (2026-11-15). Upstream: no. Keep.
20. **Damage tracing**: under `KESTREL_TRACE` each frame prints the buffer age and the redrawn
    rectangles with their area. `src/window/compositor.rs` (also keeps `buffer_age` in a local
    for it). Upstream: no. Keep.

28. **The compositor is told what changed since the frame it shows** (`src/window/compositor.rs`,
`present`): `present_with_damage` with the difference from the previous frame (grown by the same
pixel and turned into buffer pixels) instead of `present()`, which damaged the whole buffer every
frame. Correct by the protocol and checked for leftovers in the rig (none after opening and
closing 8 panels). Measured gain: none visible in the nested rig (compositor CPU 36-41 vs 42-44
ticks over the same sequence, within noise; Hyprland's damage_blink showed the whole output
repainted for both, likely its blur widening damage), so the benefit is unproven (2026-11-18).

## tiny-skia (0.11.4)

21. **Rectangle hint on `Mask`**: `Mask::set_rect_hint` declares the mask to be exactly a
    rectangle (opaque inside, empty outside); `fill_rect` and `fill_path` then clip their scan to
    it and skip the per-pixel mask stage. Every way of changing the mask's data (`data_mut`,
    `clear`, `invert`, `fill_path`, `intersect_path`, sub-pixmaps) drops the hint. `src/mask.rs`,
    `src/painter.rs`. Why: profiled 2026-11-14, `mask_u8` was 47 % of drawing time (every shape
    crossing a damaged area was scanned whole and masked pixel by pixel); clipboard key moves
    ~12 -> ~3.8 ms raster per frame. Only pays with a caller that sets it (17). Upstream: no.
    Keep. Note: `set_rect_hint` was inserted below `data_mut`'s doc line, so rustdoc shows
    "Returns the mutable internal data." on `set_rect_hint` and none on `data_mut`.
22. **`SUPERSAMPLE_SHIFT = 3`**: 8x8 anti-aliasing samples per pixel instead of 4x4, so curves
    on 1x screens (pills, circles, rings) get 64 edge shades instead of 16. `src/scan/path_aa.rs`
    (marked `kestrel:`; 2026-11-13). Upstream: no, no issue; unlikely as a default (slower,
    changes every resvg output). Keep.
23. **Lints off for the vendored code**: `#![allow(warnings, clippy::all, clippy::pedantic)]` in
    `src/lib.rs`, so `make check`'s clippy `-D warnings` does not fail on upstream code. Its
    comment still says "Vendored for one change (SUPERSAMPLE_SHIFT)", which 21 made out of date.
    Not for upstream.

## softbuffer (0.4.8, `wayland-argb`)

24. **A third buffer**, so drawing never waits for Hyprland to release the previous frame's.
    Releases already sent are taken in (`dispatch_pending`); if the back buffer is still held, a
    released spare is swapped in, or one new spare is made; only if that one is held too does it
    block as before. Spare buffers' ages advance like the others. `src/backends/wayland/mod.rs`.
    Why: with two buffers every frame blocked about a frame for the back one (2026-11-13; frame
    numbers under 17). Upstream: no; softbuffer master still keeps two buffers and blocks
    (behaviour from softbuffer#55; vsync open as softbuffer#29). Would need a design discussion
    (memory vs never blocking). Keep.
25. **A resized buffer's age is reset to 0.** `src/backends/wayland/buffer.rs`
    (`WaylandBuffer::configure`). Why: a new buffer holds no earlier frame; keeping the age made
    the renderer redraw only the changed parts over garbage: flashes and tearing in resized or
    tiled windows (2026-11-14). Upstream: **bug still on master**, no issue filed.
    **Upstream: recommended** (plain correctness bug).

The base change (`Argb8888`, so the CPU renderer keeps alpha on Wayland) is solved on softbuffer
master by `AlphaMode` (softbuffer#321, merged 2026-03-05) but not released (latest 0.4.8);
MalpenZibo's branch can go once softbuffer releases and iced uses `AlphaMode::Premultiplied`.

## Upstream candidates

From docs/research/ICED-UPSTREAM.md section 5. Each would be a separate small PR from the user's
personal account; pushing is design call. To MalpenZibo, phrase each as an ashell-relevant bug
(its README limits outside requests).

- **softbuffer**: 25 (buffer age reset on resize). The clearest bug; no issue filed yet.
- **iced_layershell**: 11 (`RedrawRequest::At`), 2 (`RedrawRequested` to subscriptions),
  14 (cursor shape on enter), 13 (operation re-exports), 15 (`web-colors`).
- **Optional, to iced_layershell as frame-pacing fixes with trace evidence**: 1, 5, 8.
- **Keep, do not upstream**: 3, 4, 6, 7 (becomes "use the frame clock" at iced 0.15), 9, 10,
  12, 16, and every renderer patch (17-23: `iced_tiny_skia` is being replaced upstream).
