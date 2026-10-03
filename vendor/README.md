# Vendored dependencies

`iced_layershell/` is MalpenZibo's iced_layershell v0.6.2 (MIT, see its LICENSE), with twelve
kestrel patches (search for "kestrel patch"):

1. Frames that cannot be presented yet (frame callback pending) are not rasterised; the
   callback triggers the redraw instead.
2. `RedrawRequested` is also broadcast to subscriptions, so animation ticks follow the
   compositor's frame callbacks instead of a timer.
3. `Application::redraw_key`, which lets surfaces whose content has not changed
skip the redraw that otherwise follows every state update. Checked at draw time (the one
place every redraw passes), so input events and redraw retries no longer re-raster the
wallpaper; a compositor request (configure, scale, resize) always draws.
4. `KESTREL_TRACE=1` prints every surface size request, configure, present and deferred frame
with wall-clock times, for correlating flicker in a screen recording with what was committed.
6. Ordinary windows (xdg-toplevel) beside layer surfaces: `new_window(WindowSettings)` opens
one (title, app id, size), the compositor sizes it like any app, and closing it (Super+Q) removes
it and sends `window::Event::CloseRequested` to subscriptions, even though the surface is gone.
Used for Settings (2026-11-14). Surfaces carry a `Role` (Layer or Window). A surface whose
size changes is laid out again even without a message (it kept its old layout: cut off, stale).
5. A surface whose `RedrawRequested` produced messages is not presented on that pass; the retry
that follows (same loop, no frame of delay) draws it with them applied. Before, a panel opened at
its remembered height for a frame and then jumped to the one it reported (`redo` in the trace).
Measured: CPU during panel animations went from ~43 % to ~15 % of a core, because the
full-screen wallpaper was being re-rasterised on every animation frame.
7. Every pass of one frame's redraw loop gets the same `RedrawRequested` instant. A retry
(patch 5) carried a fresh one, so time-driven widgets (smooth scrolling) stepped again on it,
published again and forced another retry: each scrolling frame was built three times and logged
"More than 3 consecutive RedrawRequested cycles" (2026-11-15).

`iced_tiny_skia/` is iced_tiny_skia 0.14.1 (MIT) with one kestrel patch (search "kestrel
patch"): `adjust_clip_mask` touches only the pixels that change. The original wiped and refilled
a screen-sized mask for every layer and primitive group; with a layer per control that rewrote
tens of megabytes per frame on the full-screen overlay.

`tiny-skia/` (besides SUPERSAMPLE_SHIFT=3) carries a rectangle hint on `Mask`: iced_tiny_skia's
`adjust_clip_mask` declares the mask to be exactly its rectangle, and `fill_rect` / `fill_path`
then clip their scan to it with no per-pixel mask stage. Profiled 2026-11-14: `mask_u8` was 47%
of drawing time (every shape crossing a damaged area was scanned whole and masked pixel by
pixel). Clipboard key moves: ~12 ms -> ~3.8 ms raster per frame. iced_tiny_skia also prints the
redrawn rectangles under `KESTREL_TRACE`.

`softbuffer/` is MalpenZibo's softbuffer `wayland-argb` branch (MIT/Apache-2.0) with one kestrel
patch: a third buffer, so drawing never waits for Hyprland to release the previous frame's;
and a resized buffer's age is reset (it held garbage while the renderer, trusting the age, redrew
only the changed parts: flashes and tearing in resized/tiled windows, 2026-11-14).

Measured 2026-11-13 in the rig (overview selection, control centre open/close): overlay frame
time 8.4 ms -> 5.1 ms on average. Canvas drawings are also cached now (`ui::Memo`): an uncached
canvas was repainted over all its bounds every frame.
7. `scroll_by` and `snap_to_end` re-exported beside `scroll_to` / `snap_to` (keyboard
   scrolling in panels: the shortcuts sheet).
8. A layer surface's configure that changes nothing (Hyprland sends a second one of the same size)
is acknowledged with a bare commit, not a new frame (2026-11-15).
9. `NOT_READY` as a redraw key: the surface is not presented at all, even for a configure (the
wallpaper waits for its picture). Together with 8: the wallpaper surface presents once, so the
compositor holds one 8 MB buffer instead of two; screen buffers 17 → 8.8 MB PSS.
10. The blur region is no longer committed on its own before the buffer: it rides the present's
commit, so a panel never shows one frame of blurred empty area before its content (2026-11-15).
11. `RedrawRequest::At(t)` from a widget is honoured: the loop sleeps until the earliest such time
and redraws only those surfaces (it was dropped, so a widget pacing itself never drew again).
Used by `ui::breathe` (2026-11-15).
12. `Application::clipboard`: the app hands the clipboard text fields copy to and paste from.
kestrel gives its own (services::clipboard, wlr data-control), so iced's smithay-clipboard (a
second Wayland connection and thread, woken by every input event) is not started. Without one
the smithay clipboard is made as before (the separate prompt process keeps it) (2026-11-17).
