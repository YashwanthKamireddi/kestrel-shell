//! Settings' own work: its state and its tasks.

use super::*;

/// Settings' own work: its state and its tasks (what needs the shell is in `update`).
pub(super) fn own(st: &mut State, m: Message, cx: &Cx<'_>) -> Task<Message> {
    match m {
        Message::Page(p) => {
            st.page = p;
            st.query.clear();
            st.error = None;
            st.zones_open = false;
            if p == Page::Bluetooth {
                st.nearby_order.clear();
            }
            if p == Page::Network {
                // Networks sorted fresh on opening, then kept in place (as the control
                // centre's Wi-Fi page).
                st.wifi_order.clear();
                crate::panels::control::keep_order(
                    &mut st.wifi_order,
                    cx.net.networks.iter().map(|n| n.ssid.as_str()),
                );
            }
            // Each page starts at its top (the last page's scroll carried over).
            return Task::batch([
                load(p),
                Task::Iced(iced::widget::scroll_to(
                    BODY_ID.clone(),
                    iced::widget::scrollable::AbsoluteOffset { x: 0.0, y: 0.0 },
                )),
            ]);
        }
        // Typing searches at once.
        Message::Shown => return Task::Iced(iced::widget::focus(SEARCH_ID.clone())),
        Message::About(a) => st.about = Some(a),
        Message::Time(t) => st.time = Some(t),
        Message::Ntp(on) => {
            if let Some(t) = &mut st.time {
                t.ntp = on;
            }
            return Task::perform(crate::services::timedate::set_ntp(on), Message::Done);
        }
        Message::ZonesOpen(open) => {
            st.zones_open = open;
            st.zone_query.clear();
            if open {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("zone-search")));
            }
        }
        Message::ZoneQuery(q) => st.zone_query = q,
        Message::Zone(z) => {
            st.zones_open = false;
            if let Some(t) = &mut st.time {
                t.timezone = z.clone();
            }
            return Task::perform(crate::services::timedate::set_timezone(z), |r| {
                Message::Done(r)
            });
        }
        Message::Hypr(v) => st.hypr.extend(v),
        Message::HyprSet(o, v, save) => {
            st.hypr.insert(o.option, v.clone());
            return Task::future(async move {
                if save {
                    crate::hypr_settings::save(o, v).await;
                } else {
                    crate::hypr_settings::apply(o, v).await;
                }
            })
            .discard();
        }
        Message::HyprKeep(o) => {
            if let Some(v) = st.hypr.get(o.option).cloned() {
                return Task::future(crate::hypr_settings::save(o, v)).discard();
            }
        }
        Message::LayoutsOpen(open) => {
            st.layouts_open = open;
            st.layout_query.clear();
            if open {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("layout-search")));
            }
        }
        Message::LayoutQuery(q) => st.layout_query = q,
        Message::Xkb(k, v) => {
            let o = crate::hypr_settings::KB_OPTIONS;
            let now = st
                .hypr
                .get(o.option)
                .map(crate::hypr_settings::Value::as_str)
                .unwrap_or_default();
            let new = crate::hypr_settings::xkb_with(&now, &crate::hypr_settings::XKB[k], &v);
            let v = crate::hypr_settings::Value::Str(new);
            st.hypr.insert(o.option, v.clone());
            return Task::future(crate::hypr_settings::save(o, v)).discard();
        }
        Message::Defaults(apps, now) => {
            st.apps = apps;
            st.defaults = now;
        }
        Message::DefaultOpen(k) => st.default_open = k,
        Message::Power(h, c, idle) => {
            st.health = h;
            st.charging = c;
            st.idle = idle;
        }
        Message::Charging(c) => {
            if let Some((_, now)) = &mut st.charging {
                *now = c;
            }
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || crate::services::power::set_charging(c))
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                },
                Message::Done,
            );
        }
        Message::IdleOpen(step) => st.idle_open = step,
        Message::Displays(d) => {
            if st.pending.is_none() {
                st.kept = d.clone();
                st.displays = d;
            }
        }
        Message::ModeOpen(i) => st.mode_open = i,
        Message::Display(i, change) => return change_display(st, i, change),
        Message::DisplayKeep => {
            st.pending = None;
            st.kept = st.displays.clone();
            let all = st.displays.clone();
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || crate::services::displays::save(&all))
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()))
                },
                Message::Done,
            );
        }
        Message::DisplayRevert => return revert_displays(st),
        Message::DisplayTimeout(n) => {
            if st.pending.is_some_and(|(p, _)| p == n) {
                return revert_displays(st);
            }
        }
        Message::DisplayTick => {}
        Message::Startup(list, apps) => {
            st.startup = list;
            if !apps.is_empty() {
                st.apps = apps;
            }
        }
        Message::StartupOn(id, on) => {
            if let Some(e) = st.startup.iter_mut().find(|e| e.id == id) {
                e.enabled = on;
            }
            return startup_job(move || crate::services::autostart::set_enabled(&id, on));
        }
        Message::StartupRemove(id) => {
            st.startup.retain(|e| e.id != id);
            return startup_job(move || crate::services::autostart::remove(&id));
        }
        Message::StartupAddOpen(open) => {
            st.startup_add = open;
            st.startup_query.clear();
            if open {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("startup-search")));
            }
        }
        Message::StartupQuery(q) => st.startup_query = q,
        Message::Updates(v) => st.updates = Some(v),
        Message::Volumes(v) => {
            st.volumes = v;
            // The home folder's use: measured only while Storage is shown (never by the preload),
            // again when the last count is over five minutes old.
            let stale = st
                .measured
                .is_none_or(|t| t.elapsed() > std::time::Duration::from_secs(300));
            if st.page == Page::Storage && stale && !st.measuring {
                st.measuring = true;
                let home = std::env::var_os("HOME")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_default();
                return Task::perform(crate::services::disks::usage(home), |(t, v)| {
                    Message::Usage(t, v)
                });
            }
        }
        Message::Usage(total, items) => {
            st.measuring = false;
            st.measured = Some(std::time::Instant::now());
            st.usage = Some((total, items));
        }
        Message::Account(a) => st.account = Some(a),
        Message::FullName(t) => {
            let open = t.is_some() && st.full_name.is_none();
            st.full_name = t;
            if open {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("full-name")));
            }
        }
        Message::Picture(p) => {
            if let Some(p) = p {
                return Task::perform(
                    async move {
                        tokio::task::spawn_blocking(move || set_face(&p))
                            .await
                            .unwrap_or_else(|e| Err(e.to_string()))
                    },
                    Message::Done,
                );
            }
        }
        Message::FullNameSubmit => {
            if let Some(n) = st.full_name.take() {
                return Task::perform(crate::services::about::set_full_name(n), |r| {
                    Message::Done(r)
                });
            }
        }
        Message::VolumeEject(path, drive) => {
            return Task::perform(crate::services::disks::eject(path, drive), |r| {
                Message::Done(r)
            });
        }
        Message::StartupAdd(id) => {
            st.startup_add = false;
            return startup_job(move || {
                let f = crate::services::apps::desktop_file(&id).ok_or("app not found")?;
                crate::services::autostart::add(&f)
            });
        }
        Message::Rename(t) => {
            let open = t.is_some() && st.rename.is_none();
            st.rename = t;
            if open {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("rename")));
            }
        }
        Message::RenameSubmit => {
            if let Some(name) = st.rename.take()
                && !name.trim().is_empty()
            {
                return Task::perform(crate::services::about::rename(name), Message::Done);
            }
        }
        Message::QuietOpen(w) => st.quiet_open = w,
        Message::Binds(b) => {
            st.binds = b;
            st.custom = crate::hypr_settings::shortcuts();
        }
        Message::Record(on) => {
            st.recording = on;
            st.rebinding = None;
            if on {
                st.new_keys = None;
            }
        }
        Message::Rebind(old) => {
            st.recording = true;
            st.error = None;
            st.rebinding = Some(old);
        }
        Message::Recorded(k) => {
            st.recording = false;
            if let Some(old) = st.rebinding.take() {
                if let Some(what) = taken_by(st, &k) {
                    st.error = Some(format!("{k} is already {what}"));
                    return Task::none();
                }
                return Task::perform(crate::hypr_settings::rebind(old, k), Message::Done);
            }
            st.new_keys = Some(k);
        }
        Message::NewName(t) => st.new_name = t,
        Message::NewCommand(t) => st.new_command = t,
        Message::AddShortcut => {
            if let Some(keys) = st.new_keys.take()
                && !st.new_command.trim().is_empty()
            {
                let sc = crate::hypr_settings::Shortcut {
                    keys,
                    name: if st.new_name.trim().is_empty() {
                        st.new_command.trim().to_string()
                    } else {
                        st.new_name.trim().to_string()
                    },
                    command: st.new_command.trim().to_string(),
                };
                st.new_name.clear();
                st.new_command.clear();
                return Task::batch([
                    Task::future(crate::hypr_settings::add_shortcut(sc)).discard(),
                    // Read back once Hyprland has it.
                    Task::perform(
                        async {
                            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                            crate::services::hyprland::binds().await
                        },
                        Message::Binds,
                    ),
                ]);
            }
        }
        Message::RemoveShortcut(keys) => {
            st.custom.retain(|c| c.keys != keys);
            return Task::batch([
                Task::future(crate::hypr_settings::remove_shortcut(keys)).discard(),
                Task::perform(
                    async {
                        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                        crate::services::hyprland::binds().await
                    },
                    Message::Binds,
                ),
            ]);
        }
        Message::SoundPick(k) => st.sound_pick = k,
        Message::TextScale(f) => {
            st.text_scale = f;
            return Task::future(crate::hypr_settings::set_text_scale(f)).discard();
        }
        Message::NetRead(primary, hot) => {
            let (now, ip) = match primary {
                Some((path, name, ip)) => (Some((path, name)), ip),
                None => (None, Default::default()),
            };
            // What is being typed stays, unless the connection changed or it was saved as is.
            if st.ip_for != now || st.ip_edit == IpEdit::from(&st.ip_saved) {
                st.ip_edit = IpEdit::from(&ip);
            }
            st.ip_for = now;
            st.ip_saved = ip;
            if hot.is_none() && st.hotspot_edit.0.is_empty() {
                st.hotspot_edit = (hostname(), new_password());
            }
            st.hotspot = hot;
        }
        Message::IpEdit(e) => st.ip_edit = e,
        Message::IpApply => {
            let Some((path, _)) = st.ip_for.clone() else {
                return Task::none();
            };
            let ip = st.ip_edit.settings();
            if let Err(e) = crate::services::network::check_ip(&ip) {
                st.error = Some(e);
                return Task::none();
            }
            st.error = None;
            return Task::perform(
                crate::services::network::try_command(crate::services::network::Command::SetIp(
                    path, ip,
                )),
                Message::Done,
            );
        }
        Message::HotspotEdit(n, p) => st.hotspot_edit = (n, p),
        Message::HotspotOn(on) => {
            let (name, pass) = st.hotspot_edit.clone();
            let cmd = if on {
                if name.trim().is_empty() || pass.chars().count() < 8 {
                    st.error = Some("The password needs at least 8 characters".into());
                    return Task::none();
                }
                crate::services::network::Command::Hotspot(Some((name.trim().to_string(), pass)))
            } else {
                crate::services::network::Command::Hotspot(None)
            };
            st.error = None;
            return Task::perform(crate::services::network::try_command(cmd), |r| {
                Message::Done(r)
            });
        }
        Message::ForgetNet(ssid) => {
            if st.forget_net.as_deref() != Some(ssid.as_str()) {
                st.forget_net = Some(ssid);
                return Task::none();
            }
            st.forget_net = None;
            return Task::perform(
                crate::services::network::try_command(crate::services::network::Command::Forget(
                    ssid,
                )),
                Message::Done,
            );
        }
        Message::AutoJoinNet(ssid, on) => {
            return Task::perform(
                crate::services::network::try_command(crate::services::network::Command::AutoJoin(
                    ssid, on,
                )),
                Message::Done,
            );
        }
        Message::Terminals(t, ts, es) => {
            st.terminal = Some(t);
            st.terminals = ts;
            st.editors = es;
        }
        Message::Regions(r, now) => {
            st.regions = r;
            st.region_now = now;
        }
        Message::RegionOpen(open) => {
            st.region_open = open;
            st.region_query.clear();
            if open {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("region-search")));
            }
        }
        Message::RegionQuery(q) => st.region_query = q,
        Message::RegionSet(r) => {
            st.region_open = false;
            return Task::perform(crate::services::region::set(r), Message::Done);
        }
        Message::RegionReset => {
            st.region_open = false;
            return Task::perform(crate::services::region::reset(), Message::Done);
        }
        Message::Pointers(d, sp) => {
            st.pointers = d;
            st.device_speeds = sp;
        }
        Message::DeviceSpeed(name, v, keep) => {
            let d = crate::hypr_settings::DeviceSpeed {
                name: name.clone(),
                sensitivity: v,
            };
            match st.device_speeds.iter_mut().find(|x| x.name == name) {
                Some(x) => x.sensitivity = v,
                None => st.device_speeds.push(d.clone()),
            }
            return Task::future(crate::hypr_settings::set_device_speed(d, keep)).discard();
        }
        Message::Mono(m) => st.mono = m,
        Message::MonoSet(on) => {
            st.mono = Some(on);
            return Task::future(crate::services::audio::set_mono(on)).discard();
        }
        Message::SpeakerTest => {
            st.speaker_test = true;
            return Task::perform(crate::services::sounds::speaker_test(), |()| {
                Message::SpeakerDone
            });
        }
        Message::SpeakerDone => st.speaker_test = false,
        Message::MicTest => {
            st.mic_test = 1;
            return Task::perform(crate::services::sounds::mic_record(3), |r| {
                Message::MicRecorded(r)
            });
        }
        Message::MicRecorded(r) => {
            let pcm = match r {
                Ok(p) => p,
                Err(e) => {
                    st.mic_test = 0;
                    st.error = Some(e);
                    return Task::none();
                }
            };
            st.mic_test = 2;
            return Task::perform(crate::services::sounds::mic_playback(pcm), |()| {
                Message::MicDone
            });
        }
        Message::MicDone => st.mic_test = 0,
        Message::AlertPlay => {
            let v = cx.config.sound.alert_volume();
            return Task::future(async move {
                crate::services::sounds::play_now(crate::services::sounds::MESSAGE, v).await;
            })
            .discard();
        }
        Message::Restart => {
            if !st.restart_armed {
                st.restart_armed = true;
                return Task::none();
            }
            st.restart_armed = false;
            return Task::future(async {
                if let Err(e) = crate::services::systemd::restart_user_unit("kestrel.service").await
                {
                    log::warn!("restart: {e}");
                }
            })
            .discard();
        }
        Message::Firmware(v) => {
            st.firmware = Some(v);
            st.firmware_checking = false;
        }
        Message::CheckFirmware => {
            st.firmware_checking = true;
            return Task::perform(crate::services::updates::firmware(), |v| {
                Message::Firmware(v)
            });
        }
        Message::TerminalSet(id) => {
            st.terminal = Some(id.clone());
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        crate::services::defaults::set_terminal(&id)
                    })
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()))
                },
                Message::Done,
            );
        }
        Message::Privacy(l, r, d) => {
            st.privacy = Some((l, r, d));
            st.clear_armed = None;
        }
        Message::LockOnSleep(on) => {
            if let Some(p) = &mut st.privacy {
                p.0 = Some(on);
            }
            return Task::perform(crate::services::idle::set_lock_on_sleep(on), |r| {
                Message::Done(r)
            });
        }
        Message::RememberRecent(on) => {
            if let Some(p) = &mut st.privacy {
                p.1 = on;
            }
            return Task::future(gsettings_privacy(
                "remember-recent-files",
                crate::services::gsettings::Set::Bool(on),
            ))
            .discard();
        }
        Message::RecentDays(d) => {
            if let Some(p) = &mut st.privacy {
                p.2 = d;
            }
            return Task::future(gsettings_privacy(
                "recent-files-max-age",
                crate::services::gsettings::Set::Int(i64::from(d)),
            ))
            .discard();
        }
        Message::Look(l) => st.look = Some(l),
        Message::LookOpen(which) => {
            st.look_open = which;
            st.look_query.clear();
            if which.is_some() {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("look-search")));
            }
        }
        Message::LookQuery(q) => st.look_query = q,
        Message::Lid(l) => st.lid = l,
        Message::LowBattery(l) => st.low = l,
        Message::LowSet(warn, action) => {
            if let Some(l) = &mut st.low {
                l.warn_at = warn;
                l.action = action.clone();
            }
            return Task::perform(
                async move { crate::services::power::set_low_battery(warn, &action).await },
                Message::Done,
            );
        }
        Message::LidSet(l) => {
            // Shown at once; logind is changed once the password is given (or it reads back).
            st.lid = Some(l);
            return Task::perform(crate::services::power::set_lid(l), Message::Done);
        }
        Message::Rules(r, a) => {
            // A rule opened before the saved ones were read (from the overview): its saved
            // form, if it has one.
            if let Some(e) = &st.rule_edit
                && let Some(saved) = r.iter().find(|x| x.class == e.class)
            {
                st.rule_edit = Some(saved.clone());
            }
            st.rules = r;
            st.rule_apps = a;
        }
        Message::RulePick(q) => {
            let start = st.rule_pick.is_none() && q.is_some();
            st.rule_pick = q;
            st.rule_edit = None;
            if start {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("rule-search")));
            }
        }
        Message::RuleOpen(class) => {
            st.rule_pick = None;
            st.rule_edit = class.map(|c| {
                st.rules.iter().find(|r| r.class == c).cloned().unwrap_or(
                    crate::hypr_settings::WindowRule {
                        class: c,
                        ..Default::default()
                    },
                )
            });
        }
        Message::RuleSet(r) => {
            match st.rules.iter_mut().find(|x| x.class == r.class) {
                Some(x) => *x = r.clone(),
                None => st.rules.push(r.clone()),
            }
            st.rule_edit = Some(r.clone());
            return Task::future(crate::hypr_settings::set_window_rule(r)).discard();
        }
        Message::RuleRemove(class) => {
            st.rules.retain(|r| r.class != class);
            st.rule_edit = None;
            return Task::future(crate::hypr_settings::remove_window_rule(class)).discard();
        }
        Message::BtRename(target, now) => {
            st.bt_rename = Some((target, now));
            return Task::Iced(iced::widget::focus(iced::widget::Id::new("bt-rename")));
        }
        Message::BtRenameText(t) => {
            if let Some((_, text)) = &mut st.bt_rename {
                *text = t;
            }
        }
        Message::BtRenameDone => {
            let Some((target, name)) = st.bt_rename.take() else {
                return Task::none();
            };
            let name = name.trim().to_string();
            if name.is_empty() {
                return Task::none();
            }
            use crate::services::bluetooth::Command as B;
            let cmd = match target {
                None => B::Rename(name),
                Some(path) => B::RenameDevice(path, name),
            };
            return Task::perform(crate::services::bluetooth::command(cmd), |r| {
                Message::Done(r)
            });
        }
        Message::HiddenOpen(open) => {
            st.hidden_open = open;
            if open {
                return Task::Iced(iced::widget::focus(iced::widget::Id::new("hidden-name")));
            }
        }
        Message::Idle(step, secs) => {
            st.idle_open = None;
            if let Some(e) = st.idle.iter_mut().find(|(s, _)| *s == step) {
                e.1 = Some(secs);
            }
            return Task::perform(crate::services::idle::set(step, secs), Message::Done);
        }
        Message::DefaultSet(k, id) => {
            st.default_open = None;
            if let Some(slot) = st.defaults.get_mut(k) {
                *slot = Some(id.clone());
            }
            return Task::perform(
                async move {
                    tokio::task::spawn_blocking(move || {
                        crate::services::defaults::set(&crate::services::defaults::KINDS[k], &id)
                    })
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()))
                },
                Message::Done,
            );
        }
        Message::LayoutAdd(code) => return change_layouts(st, code, true),
        Message::LayoutRemove(code) => return change_layouts(st, code, false),
        Message::Done(r) => {
            st.error = r.err();
            return load(st.page);
        }
        Message::Query(q) => {
            // The first letter typed reads the quick system pages, so their settings are found.
            let first = st.query.is_empty() && !q.is_empty();
            st.query = q;
            if first {
                return Task::batch([
                    load(Page::Keyboard),
                    load(Page::Windows),
                    load(Page::Access),
                    load(Page::Power),
                    load(Page::DateTime),
                ]);
            }
        }
        Message::City(c) => st.city = c,
        // Asked of the shell: see `update`.
        Message::WeekStart(..)
        | Message::Profile(..)
        | Message::Quiet(..)
        | Message::SoundStep(..)
        | Message::SoundMax(..)
        | Message::SoundOpt(..)
        | Message::CursorSize(..)
        | Message::NotifyRule(..)
        | Message::PickPicture
        | Message::OpenFile(..)
        | Message::OpenMade(..)
        | Message::FirmwareUpdate(..)
        | Message::InTerminal(..)
        | Message::EditorSet(..)
        | Message::Clear(..)
        | Message::LookSet(..)
        | Message::LookSize(..)
        | Message::Accent(..)
        | Message::JoinNet(..)
        | Message::NotifyOpt(..)
        | Message::NotifySilent(..)
        | Message::Units(..)
        | Message::Style(..)
        | Message::Workspaces(..)
        | Message::ThemeApps(..)
        | Message::Material(..)
        | Message::SubmitCity
        | Message::Opt(..)
        | Message::Plugin(..)
        | Message::BarMove(..)
        | Message::BarSection(..)
        | Message::BarRemove(..)
        | Message::BarAddTo(..)
        | Message::BarDrop
        | Message::BarReset
        | Message::VolumeOpen(..)
        | Message::Mounted(..)
        | Message::SubmitPassword
        | Message::JoinHidden
        | Message::Forget(..)
        | Message::WarmthDone
        | Message::Copy(..)
        | Message::Change(..)
        | Message::Batch(..)
        | Message::Close
        | Message::OpenWallpaper => {}
        Message::Password(p) => {
            if let Some(pw) = &mut st.password {
                pw.2 = p;
            }
        }
        Message::HiddenName(t) => st.hidden.0 = t,
        Message::HiddenPassword(t) => st.hidden.1 = t,
        // Warmth follows the slider at once (night light's own tool); kept when let go.
        Message::Warmth(v) => {
            use crate::panels::control::{COOLEST, WARMEST};
            let k = (WARMEST + v.clamp(0.0, 1.0) * (COOLEST - WARMEST)).round() as u32 / 100 * 100;
            st.kelvin = Some(k);
            return Task::future(crate::services::toggles::set_temperature(k)).discard();
        }
        Message::BarPick(id) => st.bar_pick = id,
        Message::BarDrag(id) => {
            st.bar_drag = Some(id);
            st.bar_target = None;
        }
        Message::BarOver(lane, at) => {
            if st.bar_drag.is_some() {
                st.bar_target = Some((lane, at));
            }
        }
        Message::BarCancel => {
            st.bar_drag = None;
            st.bar_target = None;
        }
        Message::BarAddOpen(lane) => st.bar_adding = lane,
        Message::ReadDoc | Message::ReadPlugins => return read_doc(),
        Message::Doc(doc, plugins) => {
            st.doc = doc;
            st.plugins = plugins;
        }
    }
    Task::none()
}
