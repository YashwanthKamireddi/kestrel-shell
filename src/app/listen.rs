//! What the shell listens to (services, timers, input), each only while it is needed.

use super::*;

pub(crate) fn subscription(app: &App) -> Subscription<Message> {
    let mut subs = vec![
        // The system agents (polkit passwords, Wi-Fi secrets, Bluetooth pairing): in the shell
        // only when no prompt process owns them (`kestrel prompt`, src/prompt.rs).
        if separate_prompts() {
            Subscription::none()
        } else {
            Subscription::batch([
                crate::subs::polkit().map(Message::Polkit),
                crate::subs::bt_agent().map(Message::BtAsk),
                crate::subs::wifi_agent().map(Message::WifiAsk),
            ])
        },
        iced::output_events().map(Message::Output),
        crate::subs::hyprland().map(Message::Hypr),
        crate::subs::clock().map(|()| Message::Minute),
        // The update loop pings systemd's watchdog: a freeze stops the pings and systemd
        // restarts the shell.
        match crate::watchdog::interval() {
            Some(every) => iced::time::every(every).map(|_| Message::Alive),
            None => Subscription::none(),
        },
        if app.config.clock.seconds {
            crate::subs::seconds().map(|()| Message::Second)
        } else {
            Subscription::none()
        },
        panels::subscription(app).map(Message::Panel),
        crate::ipc::subscription().map(Message::Ipc),
        // Always on: volume pop-ups, power mode on the charger, notifications.
        crate::subs::audio().map(Message::Audio),
        crate::subs::power().map(Message::Power),
        crate::subs::notifications().map(Message::Notification),
        // What gets copied (screenshots too) shows in the island.
        crate::subs::copied().map(Message::Copied),
        // Apps asking for a file get kestrel's picker.
        crate::subs::filechooser().map(Message::FileRequest),
        // A window the user closed (Settings).
        iced::event::listen_with(|e, _, w| match e {
            iced::event::Event::Window(iced::window::Event::CloseRequested) => {
                Some(Message::WindowClosed(w))
            }
            _ => None,
        }),
        // Bluetooth too: devices connecting and leaving show an OSD.
        crate::subs::bluetooth().map(Message::Bt),
        // The user layer: live config and init.lua.
        crate::subs::config_files().map(Message::ConfigFile),
        crate::subs::lua().map(Message::Lua),
    ];
    // The rest only while a configured module uses them.
    let needs = crate::modules::needs(&app.config.bar);
    let when = |on: bool, sub: Subscription<Message>| if on { sub } else { Subscription::none() };
    subs.push(when(
        needs.network,
        crate::subs::network().map(|s| Message::Net(Box::new(s))),
    ));
    subs.push(when(needs.network, crate::subs::vpn().map(Message::Vpn)));
    subs.push(when(needs.media, crate::subs::mpris().map(Message::Media)));
    subs.push(when(
        needs.camera,
        crate::subs::camera().map(Message::Camera),
    ));
    subs.push(when(needs.tray, crate::subs::tray().map(Message::Tray)));
    // AirPods: their own channel, only while a pair is connected.
    if needs.airpods
        && let Some(d) = crate::panels::pods::device(app)
    {
        // Apple's channel takes one listener only: a test copy (KESTREL_NO_AIRPODS, set by the
        // rig) leaves it to the real shell.
        if std::env::var_os("KESTREL_NO_AIRPODS").is_none() {
            subs.push(crate::subs::airpods(&d.path).map(Message::Pods));
        }
    }
    subs.push(when(
        needs.system,
        crate::subs::system().map(Message::System),
    ));
    subs.push(when(
        needs.updates,
        crate::subs::updates().map(Message::Updates),
    ));
    subs.push(when(
        needs.weather,
        crate::subs::weather(
            app.config.weather_city.as_deref(),
            app.config.weather_latitude,
            app.config.weather_longitude,
        )
        .map(Message::Weather),
    ));
    // Banners, the OSD and the island's flash leave at their own time: one wake at the
    // earliest (was a 200 ms poll, up to 200 ms late).
    let next = app
        .toasts
        .iter()
        .map(|t| t.until)
        .chain(app.osd.as_ref().map(|o| o.until))
        .chain(app.flash.as_ref().map(|f| f.until))
        .min();
    if let Some(next) = next {
        subs.push(crate::subs::at(next).map(Message::Expire));
    }
    // Something in progress breathes (charging, Wi-Fi connecting): 20 frames a second, only
    // while it lasts, never in game mode.
    // The visualiser listens only while music plays and the island can show it.
    if !crate::motion::still()
        && app.config.bar.visualiser
        && app.config.bar.contains("island")
        && app.media.main().is_some_and(|p| p.playing)
    {
        subs.push(crate::subs::level().map(Message::Level));
    }
    // The bars move at the rate the sound is read (about 30 a second, services::level), not
    // the screen's 60: every second frame showed nothing new and cost as much (measured
    // 2026-11-16: the visualiser was ~4 % of a core while music played).
    if app.vis_fast() {
        subs.push(iced::time::every(Duration::from_millis(33)).map(Message::VisTick));
    }
    // Before that the bars moved with the screen's frames (`vis_fast`, in `tick`).
    // Playing but silent (a pause in the track): a slow look at the readings, so the bars wake
    // as soon as sound comes back without drawing 60 frames a second of nothing.
    if !app.vis_fast()
        && !crate::motion::still()
        && app.config.bar.visualiser
        && app.media.main().is_some_and(|p| p.playing)
    {
        subs.push(iced::time::every(Duration::from_millis(120)).map(Message::VisTick));
    }
    // Live levels, only while the Sound page is open: the output, the microphone and every
    // app stream playing (each measured on the output it plays on).
    let mixer_shown = (app
        .panel
        .as_ref()
        .is_some_and(|p| p.open && p.kind == PanelKind::Control)
        && app.panels.control.page == panels::control::Page::Output)
        || (app.settings_window.is_some() && panels::settings::on_sound(&app.panels.settings));
    if mixer_shown {
        let default = app.audio.default_sink.clone();
        let streams: Vec<(u32, String)> = app
            .audio
            .apps
            .iter()
            .flat_map(|a| {
                let sink = if a.sink.is_empty() {
                    default.clone()
                } else {
                    a.sink.clone()
                };
                a.streams.iter().map(move |i| (*i, sink.clone()))
            })
            .collect();
        subs.push(crate::subs::meters(streams).map(Message::Meters));
    }
    // (Things in progress pulse inside their icons, `ui::breathe`: no app-wide timer.)
    if app.hypr.screencast && app.hypr.recording.is_some() {
        // The recording timer: once a second.
        subs.push(iced::time::every(Duration::from_secs(1)).map(|_| Message::Breathe));
    }
    if app.animating() {
        // Ticks come from the compositor's frame callbacks (vsync), not a timer.
        subs.push(iced::event::listen_raw(|e, _, _| match e {
            iced::event::Event::Window(iced::window::Event::RedrawRequested(at)) => {
                Some(Message::Frame(at))
            }
            _ => None,
        }));
    }
    if app.panel.as_ref().is_some_and(|p| p.open) {
        // Escape closes the panel even while a text field has the keyboard.
        subs.push(iced::event::listen_with(|e, _status, _id| match e {
            iced::event::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
                ..
            }) => Some(Message::Close),
            _ => None,
        }));
    }
    Subscription::batch(subs)
}

/// The system prompts run as their own process (`kestrel prompt`), set by kestrel.service.
pub(crate) fn separate_prompts() -> bool {
    std::env::var_os("KESTREL_SEPARATE_PROMPTS").is_some_and(|v| v == "1")
}
