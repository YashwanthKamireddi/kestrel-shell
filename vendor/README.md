# Vendored dependencies

`iced_layershell/` is MalpenZibo's iced_layershell v0.6.2 (MIT, see its LICENSE), with three
kestrel patches (search for "kestrel patch"):

1. Frames that cannot be presented yet (frame callback pending) are not rasterised; the
   callback triggers the redraw instead.
2. `RedrawRequested` is also broadcast to subscriptions, so animation ticks follow the
   compositor's frame callbacks instead of a timer.
3. `Application::redraw_key`, which lets surfaces whose content has not changed
skip the redraw that otherwise follows every state update (search for "kestrel patch").
Measured: CPU during panel animations went from ~43 % to ~15 % of a core, because the
full-screen wallpaper was being re-rasterised on every animation frame.
