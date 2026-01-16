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
