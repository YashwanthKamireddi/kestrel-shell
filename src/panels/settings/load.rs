//! Opening Settings and reading what each page shows (off the UI thread).

use super::*;

/// config.toml as it is on disk (module options are read from it).
pub(super) fn read_doc() -> toml::Table {
    std::fs::read_to_string(config::path())
        .ok()
        .and_then(|t| t.parse().ok())
        .unwrap_or_default()
}

pub fn open(st: &mut State, cx: &Cx<'_>) -> Action {
    // Each open starts fresh: no search left from last time.
    st.query.clear();
    st.doc = read_doc();
    st.city = cx.config.weather_city.clone().unwrap_or_default();
    st.plugins = crate::lua::installed();
    // The first open reads every page's data in the background, so switching pages shows a full
    // page at once (a page read only when chosen drew empty for a frame, then filled in). Later
    // opens keep that data and read again only the page shown: each page refreshes itself when
    // chosen. Updates is left to its page: checking for updates is slow and uses the network.
    let mut tasks: Vec<Task<Message>> = Vec::new();
    if !st.preloaded {
        st.preloaded = true;
        tasks.extend(
            SYSTEM
                .into_iter()
                .filter(|p| *p != Page::Updates && *p != st.page)
                .map(load),
        );
    }
    tasks.push(load(st.page));
    // Typing searches at once.
    tasks.push(Task::Iced(iced::widget::focus(SEARCH_ID.clone())));
    Act::Run(Task::batch(tasks))
}

/// The page scrolled to its end (where a rule opened from the overview is).
pub fn to_end<M: Send + 'static>() -> Task<M> {
    Task::Iced(iced::widget::snap_to(
        BODY_ID.clone(),
        iced::widget::scrollable::RelativeOffset { x: 0.0, y: 1.0 },
    ))
}

pub(super) static BODY_ID: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("settings-body"));

/// What a system page shows, read fresh when it opens (off the UI thread).
pub(super) fn load(p: Page) -> Task<Message> {
    match p {
        Page::About => Task::perform(
            async {
                tokio::task::spawn_blocking(crate::services::about::read_all)
                    .await
                    .unwrap_or_default()
            },
            Message::About,
        ),
        Page::DateTime => Task::batch([
            Task::perform(crate::services::timedate::read(), Message::Time),
            Task::perform(
                async {
                    tokio::task::spawn_blocking(|| {
                        (
                            crate::services::region::supported(),
                            crate::services::region::current(),
                        )
                    })
                    .await
                    .unwrap_or_default()
                },
                |(r, now)| Message::Regions(r, now),
            ),
        ]),
        Page::Power => Task::batch([
            Task::perform(crate::services::power::lid(), Message::Lid),
            Task::perform(crate::services::power::low_battery(), |l| {
                Message::LowBattery(l)
            }),
            Task::perform(
                async {
                    tokio::task::spawn_blocking(|| {
                        (
                            crate::services::power::health(),
                            crate::services::power::charging(),
                            crate::services::idle::read(),
                        )
                    })
                    .await
                    .unwrap_or_default()
                },
                |(h, c, i)| Message::Power(h, c, i),
            ),
        ]),
        Page::Account => Task::perform(
            async {
                tokio::task::spawn_blocking(crate::services::about::account)
                    .await
                    .unwrap_or_default()
            },
            Message::Account,
        ),
        Page::Storage => Task::perform(crate::services::disks::list(), Message::Volumes),
        Page::Privacy => Task::perform(
            async {
                tokio::task::spawn_blocking(|| {
                    use crate::services::gsettings as g;
                    const P: &str = "org.gnome.desktop.privacy";
                    (
                        crate::services::idle::lock_on_sleep(),
                        g::get_bool(P, "remember-recent-files").unwrap_or(true),
                        g::get_i64(P, "recent-files-max-age")
                            .and_then(|v| i32::try_from(v).ok())
                            .unwrap_or(-1),
                    )
                })
                .await
                .unwrap_or((None, true, -1))
            },
            |(l, r, d)| Message::Privacy(l, r, d),
        ),
        Page::Appearance => Task::perform(
            async {
                tokio::task::spawn_blocking(|| {
                    let (fonts, monos) = crate::look::fonts();
                    let get = |k: &str| {
                        crate::services::gsettings::get_string("org.gnome.desktop.interface", k)
                            .unwrap_or_default()
                    };
                    LookLists {
                        fonts,
                        monos,
                        icons: crate::look::icon_themes(),
                        cursors: crate::look::cursor_themes(),
                        icons_now: get("icon-theme"),
                        cursor_now: get("cursor-theme"),
                        font_now: font_pair(&get("font-name")),
                        mono_now: font_pair(&get("monospace-font-name")),
                    }
                })
                .await
                .unwrap_or_default()
            },
            Message::Look,
        ),
        Page::Network => Task::batch([
            Task::perform(
                async {
                    tokio::join!(
                        crate::services::network::primary(),
                        crate::services::network::hotspot()
                    )
                },
                |(p, h)| Message::NetRead(p, h),
            ),
            // Networks in range, fresh (NetworkManager scans rarely while connected).
            Task::future(crate::services::network::command(
                crate::services::network::Command::Scan,
            ))
            .discard(),
        ]),
        Page::Updates => Task::perform(crate::services::updates::list(), Message::Updates),
        Page::Startup => Task::perform(
            async {
                tokio::task::spawn_blocking(|| {
                    (
                        crate::services::autostart::list(),
                        crate::services::apps::load(),
                    )
                })
                .await
                .unwrap_or_default()
            },
            |(l, a)| Message::Startup(l, a),
        ),
        Page::Displays => Task::perform(crate::services::displays::read(), Message::Displays),
        Page::Defaults => Task::batch([
            Task::perform(
                async {
                    tokio::task::spawn_blocking(|| {
                        let apps = crate::services::apps::load();
                        (
                            crate::services::defaults::terminal(),
                            crate::services::defaults::terminals(&apps),
                            crate::services::defaults::editors(),
                        )
                    })
                    .await
                    .unwrap_or_default()
                },
                |(t, ts, es)| Message::Terminals(t, ts, es),
            ),
            Task::perform(
                async {
                    tokio::task::spawn_blocking(|| {
                        let apps = crate::services::apps::load();
                        let now = crate::services::defaults::KINDS
                            .iter()
                            .map(|k| crate::services::defaults::current(k.types[0]))
                            .collect();
                        (apps, now)
                    })
                    .await
                    .unwrap_or_default()
                },
                |(a, n)| Message::Defaults(a, n),
            ),
        ]),
        Page::Shortcuts => Task::perform(crate::services::hyprland::binds(), Message::Binds),
        Page::Access => Task::batch([
            Task::perform(crate::services::audio::mono(), Message::Mono),
            Task::perform(crate::hypr_settings::text_scale(), |f| {
                Message::TextScale(f)
            }),
            Task::perform(
                crate::hypr_settings::read(crate::hypr_settings::ACCESS.to_vec()),
                Message::Hypr,
            ),
        ]),
        Page::Windows => {
            use crate::hypr_settings::{WINDOW_BEHAVIOUR, WINDOW_LOOK};
            Task::batch([
                Task::perform(
                    crate::hypr_settings::read([WINDOW_LOOK, WINDOW_BEHAVIOUR].concat()),
                    Message::Hypr,
                ),
                Task::perform(
                    async {
                        tokio::task::spawn_blocking(|| {
                            (
                                crate::hypr_settings::window_rules(),
                                crate::services::apps::load(),
                            )
                        })
                        .await
                        .unwrap_or_default()
                    },
                    |(r, a)| Message::Rules(r, a),
                ),
            ])
        }
        Page::Mouse => {
            use crate::hypr_settings::{KEYBOARD, MOUSE, Opt, TOUCHPAD};
            let mut opts: Vec<Opt> = [KEYBOARD, MOUSE, TOUCHPAD].concat();
            opts.push(LAYOUTS);
            opts.push(crate::hypr_settings::KB_OPTIONS);
            Task::batch([
                Task::perform(crate::hypr_settings::read(opts), Message::Hypr),
                Task::perform(
                    async {
                        let devices = crate::services::hyprland::pointer_devices().await;
                        let speeds =
                            tokio::task::spawn_blocking(crate::hypr_settings::device_speeds)
                                .await
                                .unwrap_or_default();
                        (devices, speeds)
                    },
                    |(d, sp)| Message::Pointers(d, sp),
                ),
            ])
        }
        Page::Keyboard => {
            use crate::hypr_settings::{KEYBOARD, MOUSE, Opt, TOUCHPAD};
            let mut opts: Vec<Opt> = [KEYBOARD, MOUSE, TOUCHPAD].concat();
            opts.push(LAYOUTS);
            opts.push(crate::hypr_settings::KB_OPTIONS);
            Task::perform(crate::hypr_settings::read(opts), Message::Hypr)
        }
        _ => Task::none(),
    }
}

/// The network changed: the Network page, when shown, reads its connection and hotspot again
/// (another network's IP settings, a hotspot started).
pub fn network_changed(
    st: &State,
    now: &crate::services::network::State,
    before: &crate::services::network::State,
) -> Action {
    let moved = now.link != before.link
        || now.ssid != before.ssid
        || now.hotspot != before.hotspot
        || now.wired.as_ref().map(|w| w.connected) != before.wired.as_ref().map(|w| w.connected);
    if moved && st.page == Page::Network {
        Act::Run(load(Page::Network))
    } else {
        Act::None
    }
}

impl State {
    /// The Wi-Fi networks or nearby devices changed: the lists keep their rows where they
    /// are; found ones join at the end (called by the shell after each report).
    pub fn lists_changed(
        &mut self,
        net: &crate::services::network::State,
        bt: &crate::services::bluetooth::State,
    ) {
        crate::panels::control::keep_order(
            &mut self.wifi_order,
            net.networks.iter().map(|n| n.ssid.as_str()),
        );
        crate::panels::control::keep_order(
            &mut self.nearby_order,
            bt.nearby.iter().map(|d| d.path.as_str()),
        );
    }
}
