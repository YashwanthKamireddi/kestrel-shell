//! What a message asks of the shell, and the rest handed to `own`.

use super::*;

/// One setting in config.toml, changed by the shell (written, then applied at once).
pub(super) fn setting(table: &[&str], key: &str, value: toml_edit::Item) -> Action {
    Act::Change(Change::Settings(vec![config::Edit::new(table, key, value)]))
}

/// Settings is a window, not a panel: its "close" closes that window. `Act::Close` means the
/// open panel to the shell, so a page that closed Settings after handing over to another app
/// (Storage's Open) closed whatever panel was open instead, and Settings stayed.
pub fn own_window(a: Action) -> Action {
    match a {
        Act::Close => Act::Command("close settings".into()),
        Act::Many(v) => Act::Many(v.into_iter().map(own_window).collect()),
        a => a,
    }
}

/// What a message asks of the shell (settings, the power mode, joining, files, terminals),
/// and everything else as Settings' own work.
pub fn update(st: &mut State, m: Message, cx: &Cx<'_>) -> Action {
    let run = |t: Task<Message>| Act::Run(t);
    match m {
        Message::WeekStart(sunday) => {
            setting(&["clock"], "week_starts_sunday", toml_edit::value(sunday))
        }
        Message::Profile(p) => Act::Change(Change::Profile(p)),
        Message::Quiet(q) => {
            st.quiet_open = None;
            let (from, to) = q.unzip();
            Act::Change(Change::Settings(vec![
                config::Edit::new(
                    &[],
                    "quiet_from",
                    toml_edit::value(from.unwrap_or_default()),
                ),
                config::Edit::new(&[], "quiet_to", toml_edit::value(to.unwrap_or_default())),
            ]))
        }
        Message::SoundStep(v) => setting(&["sound"], "step", toml_edit::value(i64::from(v))),
        Message::SoundMax(v) => setting(&["sound"], "max", toml_edit::value(i64::from(v))),
        Message::SoundOpt(key, v) => setting(&["sound"], key, toml_edit::Item::Value(v)),
        Message::CursorSize(n) => {
            setting(&[], "cursor_size", toml_edit::value(i64::from(n)))
                .and(run(
                    Task::future(crate::hypr_settings::apply_cursor(n)).discard()
                ))
        }
        Message::NotifyRule(name, rule) => {
            let rules = &cx.config.notifications;
            let without = |l: &[String]| -> Vec<String> {
                l.iter().filter(|a| **a != name).cloned().collect()
            };
            let (mut muted, mut quiet) = (without(&rules.muted), without(&rules.quiet));
            match rule {
                1 => quiet.push(name),
                2 => muted.push(name),
                _ => {}
            }
            Act::Change(Change::Settings(vec![
                config::Edit::new(&["notifications"], "muted", config::list(&muted)),
                config::Edit::new(&["notifications"], "quiet", config::list(&quiet)),
            ]))
        }
        Message::NotifyOpt(key, v) => setting(&["notifications"], key, toml_edit::Item::Value(v)),
        Message::NotifySilent(name, sound) => {
            let mut silent: Vec<String> = cx
                .config
                .notifications
                .silent
                .iter()
                .filter(|a| **a != name)
                .cloned()
                .collect();
            if !sound {
                silent.push(name);
            }
            setting(&["notifications"], "silent", config::list(&silent))
        }
        Message::PickPicture => {
            let filter = crate::services::filechooser::Filter {
                name: "Pictures".into(),
                globs: vec![],
                mimes: vec!["image/png".into(), "image/jpeg".into(), "image/webp".into()],
            };
            let pics =
                std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join("Pictures"));
            let (req, rx) =
                crate::services::filechooser::local("Choose a picture", Some(filter), pics);
            Act::Ask(req).and(run(Task::perform(rx, |a| {
                Message::Picture(match a {
                    Ok(crate::services::filechooser::Answer::Chosen(v)) => v.into_iter().next(),
                    _ => None,
                })
            })))
        }
        Message::OpenFile(path) => {
            // A file not made yet is made empty (a folder, made), so it opens.
            if !path.exists() {
                if path.extension().is_none() {
                    let _ = std::fs::create_dir_all(&path);
                } else {
                    if let Some(d) = path.parent() {
                        let _ = std::fs::create_dir_all(d);
                    }
                    let _ = std::fs::write(&path, "");
                }
            }
            Act::Dispatch(crate::services::compositor::Action::Open(
                path.to_string_lossy().into(),
            ))
        }
        Message::FirmwareUpdate(id) => {
            // The device id is fwupd's hex: nothing else reaches the shell.
            let id: String = id.chars().filter(char::is_ascii_hexdigit).collect();
            let line = crate::services::defaults::in_terminal(
                &format!("sh -c 'fwupdmgr update {id}; echo; read -p \"Press Enter to close.\" _'"),
                Some("Firmware"),
            );
            Act::Dispatch(crate::services::compositor::Action::Exec(line))
        }
        Message::InTerminal(cmd, title) => {
            let line = crate::services::defaults::in_terminal(cmd, Some(title));
            Act::Dispatch(crate::services::compositor::Action::Exec(line))
        }
        Message::EditorSet(e) => {
            let line = format!("hl.env(\"EDITOR\", \"{e}\") hl.env(\"VISUAL\", \"{e}\")");
            setting(&[], "editor", toml_edit::value(e.clone())).and(run(Task::future(async move {
                crate::services::hyprland::eval(line).await;
                crate::hypr_settings::write_lua();
            })
            .discard()))
        }
        Message::Clear(which) => {
            if st.clear_armed != Some(which) {
                st.clear_armed = Some(which);
                return Act::None;
            }
            st.clear_armed = None;
            // The clipboard panel reads its list again whenever it opens.
            run(Task::perform(
                async move {
                    if which == 0 {
                        // GTK keeps them in one file; it starts a new one when it is gone.
                        let f = std::env::var_os("HOME")
                            .map(std::path::PathBuf::from)
                            .unwrap_or_default()
                            .join(".local/share/recently-used.xbel");
                        match std::fs::remove_file(f) {
                            Ok(()) => Ok(()),
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                            Err(e) => Err(e.to_string()),
                        }
                    } else {
                        // Pinned entries stay, as with the clipboard panel's "Clear all".
                        tokio::task::spawn_blocking(crate::services::clips::clear)
                            .await
                            .map_err(|_| "The clipboard history was not cleared".to_string())
                    }
                },
                Message::Done,
            ))
        }
        Message::LookSet(which, name) => {
            st.look_open = None;
            let key = ["font", "mono", "icons", "cursor"][usize::from(which.min(3))];
            let look = {
                let mut l = cx.config.look.clone();
                // The other font (and sizes) stay as they are in use now.
                let now = st.look.as_ref();
                if let Some((f, fs)) = now.and_then(|x| x.font_now.clone()) {
                    l.font.get_or_insert(f);
                    l.font_size.get_or_insert(fs);
                }
                if let Some((m, ms)) = now.and_then(|x| x.mono_now.clone()) {
                    l.mono.get_or_insert(m);
                    l.mono_size.get_or_insert(ms);
                }
                match which {
                    0 => l.font = Some(name.clone()),
                    1 => l.mono = Some(name.clone()),
                    2 => l.icons = Some(name.clone()),
                    _ => l.cursor = Some(name.clone()),
                }
                l
            };
            if let Some(lists) = &mut st.look {
                match which {
                    2 => lists.icons_now = name.clone(),
                    3 => lists.cursor_now = name.clone(),
                    _ => {}
                }
            }
            let size = cx.config.cursor_size.unwrap_or(24);
            setting(&["look"], key, toml_edit::value(name.clone())).and(run(Task::future(
                async move {
                    match which {
                        0 | 1 => {
                            let ((f, fs), (m, ms)) = (look.font(), look.mono());
                            crate::look::apply_fonts(f, fs, m, ms).await;
                        }
                        2 => crate::look::apply_icons(name).await,
                        _ => {
                            crate::look::apply_cursor(name, size).await;
                            crate::hypr_settings::write_lua();
                        }
                    }
                },
            )
            .discard()))
        }
        Message::LookSize(which, n) => {
            let key = if which == 0 { "font_size" } else { "mono_size" };
            let mut l = cx.config.look.clone();
            // A size alone keeps the font in use (not kestrel's default in its place).
            let now = st.look.as_ref();
            if l.font.is_none()
                && let Some((f, _)) = now.and_then(|x| x.font_now.clone())
            {
                l.font = Some(f);
            }
            if l.mono.is_none()
                && let Some((m, _)) = now.and_then(|x| x.mono_now.clone())
            {
                l.mono = Some(m);
            }
            if which == 0 {
                l.font_size = Some(n);
            } else {
                l.mono_size = Some(n);
            }
            setting(&["look"], key, toml_edit::value(i64::from(n))).and(run(Task::future(
                async move {
                    let ((f, fs), (m, ms)) = (l.font(), l.mono());
                    crate::look::apply_fonts(f, fs, m, ms).await;
                },
            )
            .discard()))
        }
        Message::Accent(a) => setting(
            &[],
            "accent",
            match a {
                Some(hex) => toml_edit::value(hex),
                None => toml_edit::Item::None,
            },
        ),
        Message::Units(u) => setting(&[], "weather_units", toml_edit::value(u)),
        Message::Style(b) => setting(
            &["bar"],
            "style",
            toml_edit::value(match b {
                BarStyle::Full => "full",
                BarStyle::Pills => "pills",
            }),
        ),
        Message::Workspaces(n) => setting(&[], "workspaces", toml_edit::value(n.clamp(1, 10))),
        Message::ThemeApps(on) => setting(&[], "theme_apps", toml_edit::value(on)),
        Message::Material(m) => setting(
            &[],
            "material",
            toml_edit::value(match m {
                crate::config::Material::Black => "black",
                crate::config::Material::Frosted => "frosted",
            }),
        ),
        Message::SubmitCity => {
            let c = st.city.trim().to_string();
            setting(&[], "weather_city", toml_edit::value(c))
        }
        Message::Opt(m, o, v) => {
            let Some(opt) = crate::modules::MODULES
                .get(m)
                .and_then(|m| m.options.get(o))
            else {
                return Act::None;
            };
            let item = match v {
                OptValue::Bool(b) => toml_edit::value(b),
                OptValue::Str(t) => toml_edit::value(t),
            };
            Act::ChangeThen(
                Change::Settings(vec![config::Edit::new(opt.table, opt.key, item)]),
                Message::ReadDoc,
            )
        }
        Message::Plugin(id, on) => Act::ChangeThen(
            Change::Settings(vec![config::Edit::new(
                &["plugins", &id],
                "enabled",
                toml_edit::value(on),
            )]),
            Message::ReadPlugins,
        ),
        Message::BarMove(id, d) => {
            moved(&layout(cx), &id, d).map_or(Act::None, |l| save_layout(&l))
        }
        Message::BarSection(id, d) => {
            to_section(&layout(cx), &id, d).map_or(Act::None, |l| save_layout(&l))
        }
        Message::BarRemove(id) => {
            if st.bar_pick.as_deref() == Some(id.as_str()) {
                st.bar_pick = None;
            }
            save_layout(&removed(&layout(cx), &id))
        }
        Message::BarAddTo(lane, id) => {
            st.bar_adding = None;
            added(&layout(cx), lane, &id).map_or(Act::None, |l| save_layout(&l))
        }
        Message::BarDrop => {
            let (Some(id), Some((lane, to))) = (st.bar_drag.take(), st.bar_target.take()) else {
                return Act::None;
            };
            dropped(&layout(cx), &id, lane, to).map_or(Act::None, |l| save_layout(&l))
        }
        Message::BarReset => {
            if !st.bar_reset_armed {
                st.bar_reset_armed = true;
                return Act::None;
            }
            st.bar_reset_armed = false;
            let d = crate::config::BarConfig::default();
            save_layout(&[d.left, d.center, d.right])
        }
        Message::VolumeOpen(path, mount) => match mount {
            Some(m) => open_folder(m),
            None => run(Task::perform(
                crate::services::disks::mount(path),
                Message::Mounted,
            )),
        },
        Message::Mounted(r) => match r {
            Ok(m) => open_folder(m).and(run(load(Page::Storage))),
            Err(e) => {
                st.error = Some(e);
                Act::None
            }
        },
        // Joining as the control centre does: at once, or after its password.
        Message::JoinNet(n) => {
            if n.active {
                return Act::None;
            }
            if let Some(c) = Change::join(&n) {
                return Act::Change(c);
            }
            st.password = Some((n.ssid, n.ap, String::new()));
            run(Task::Iced(iced::widget::focus(iced::widget::Id::new(
                "net-password",
            ))))
        }
        Message::SubmitPassword => match st.password.take() {
            Some((ssid, ap, pw)) if !pw.is_empty() => Act::Change(Change::Join(
                ssid.clone(),
                crate::services::network::Command::ConnectNew {
                    ssid,
                    ap,
                    password: Some(pw),
                },
            )),
            _ => Act::None,
        },
        Message::JoinHidden => {
            let (ssid, pw) = std::mem::take(&mut st.hidden);
            if ssid.trim().is_empty() {
                return Act::None;
            }
            Act::Change(Change::Join(
                ssid.clone(),
                crate::services::network::Command::ConnectHidden {
                    ssid,
                    password: (!pw.is_empty()).then_some(pw),
                },
            ))
        }
        Message::Forget(path) => {
            if st.forget.as_deref() != Some(path.as_str()) {
                st.forget = Some(path);
                return Act::None;
            }
            st.forget = None;
            Act::Change(Change::ForgetDevice(path))
        }
        Message::WarmthDone => match st.kelvin.take() {
            Some(k) => Act::Change(Change::NightTemperature(k)),
            None => Act::None,
        },
        Message::Change(c) => Act::Change(c),
        Message::Batch(v) => Act::Many(v.into_iter().map(|m| update(st, m, cx)).collect()),
        // The Settings window, not a panel (Settings is no panel).
        Message::Close => Act::Command("close settings".into()),
        Message::OpenWallpaper => Act::Command("toggle wallpaper".into()),
        Message::Copy(text) => {
            run(Task::future(crate::panels::clipboard::copy_text(text)).discard())
        }
        m => run(own(st, m, cx)),
    }
}
