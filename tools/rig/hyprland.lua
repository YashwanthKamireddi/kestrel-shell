-- kestrel test rig: a plain nested Hyprland on a fixed 1920x1080 headless output.
hl.config({
    animations = { enabled = false },
    misc = { disable_hyprland_logo = true, disable_splash_rendering = true },
    general = { gaps_in = 4, gaps_out = 8, border_size = 0 },
    -- Blur as on the user's desktop (conf/look.lua), so kestrel's glass looks are seen as they are.
    decoration = {
        rounding = 12,
        blur = { enabled = true, size = 10, passes = 3, noise = 0.05, contrast = 0.89,
                 vibrancy = 0.2, vibrancy_darkness = 0.2 },
    },
})
-- kestrel's glass looks: blur behind its surfaces, only where they draw something.
hl.layer_rule({ match = { namespace = "^kestrel(-overlay)?$" }, blur = true, ignore_alpha = 0.02 })
hl.monitor({ output = "KRIG", mode = "1920x1080@60", position = "0x0", scale = 1 })
