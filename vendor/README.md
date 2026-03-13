# Vendored dependencies

`iced_layershell/` is MalpenZibo's iced_layershell v0.6.2 (MIT, see its LICENSE), with three
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
Measured: CPU during panel animations went from ~43 % to ~15 % of a core, because the
full-screen wallpaper was being re-rasterised on every animation frame.

`iced_tiny_skia/` is iced_tiny_skia 0.14.1 (MIT) with one kestrel patch (search "kestrel
patch"): `adjust_clip_mask` touches only the pixels that change. The original wiped and refilled
a screen-sized mask for every layer and primitive group; with a layer per control that rewrote
tens of megabytes per frame on the full-screen overlay.

`softbuffer/` is MalpenZibo's softbuffer `wayland-argb` branch (MIT/Apache-2.0) with one kestrel
patch: a third buffer, so drawing never waits for Hyprland to release the previous frame's.

Measured 2026-11-13 in the rig (overview selection, control centre open/close): overlay frame
time 8.4 ms -> 5.1 ms on average. Canvas drawings are also cached now (`ui::Memo`): an uncached
canvas was repainted over all its bounds every frame.
