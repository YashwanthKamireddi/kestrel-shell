# Vendored dependencies

`iced_layershell/` is MalpenZibo's iced_layershell v0.6.2 (MIT, see its LICENSE), with one
kestrel patch: `Application::redraw_key`, which lets surfaces whose content has not changed
skip the redraw that otherwise follows every state update (search for "kestrel patch").
Measured: CPU during panel animations went from ~43 % to ~15 % of a core, because the
full-screen wallpaper was being re-rasterised on every animation frame.
