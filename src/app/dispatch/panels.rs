//! Opening, routing and closing panels; commands; batches.

use super::super::*;

#[allow(clippy::too_many_lines)]
pub(crate) fn handle(app: &mut App, msg: Message) -> Task<Message> {
    match msg {
        Message::Toggle(kind, output, button) => {
            // Clicked: the keyboard stays with the app in use (see `sync_surfaces`).
            app.panel_keys = false;
            app.toggle(kind, output, button)
        }
        Message::ControlAt(page, output, button) => {
            app.panel_keys = false;
            let open_here = app
                .panel
                .as_ref()
                .is_some_and(|p| p.open && p.kind == PanelKind::Control && p.output == output);
            if open_here && app.panels.control.page == page {
                return app.close();
            }
            let shown = if open_here {
                Task::none()
            } else {
                app.toggle(PanelKind::Control, output, button)
            };
            let page = panels::update(
                app,
                panels::Message::Control(panels::control::Message::Page(page)),
            );
            Task::batch([shown, page])
        }
        Message::SettingsAt(page) => {
            let opened = app.open_settings();
            let page = panels::update(
                app,
                panels::Message::Settings(panels::settings::Message::Page(page)),
            );
            Task::batch([opened, page])
        }
        Message::RuleFor(class) => {
            let close = app.close();
            let opened = app.open_settings();
            let page = panels::update(
                app,
                panels::Message::Settings(panels::settings::Message::Page(
                    panels::settings::Page::Windows,
                )),
            );
            let rule = panels::update(
                app,
                panels::Message::Settings(panels::settings::Message::RuleOpen(Some(class))),
            );
            // After the page's own scroll to its top: the rule is at the end.
            Task::batch([close, opened, page, rule]).chain(panels::settings::to_end())
        }
        Message::Close => {
            // What a panel decoded while open (icons, thumbnails, pictures shrunk from large
            // files) is freed by now or soon; hand it back to the system shortly after (glibc
            // keeps freed heap otherwise).
            let trim = Task::perform(tokio::time::sleep(Duration::from_millis(400)), |()| {
                Message::Trim
            });
            Task::batch([app.close(), trim])
        }
        Message::Dispatch(a) => Task::future(async move {
            crate::services::compositor::run(a).await;
        })
        .discard(),
        Message::DeviceDone(path, r) => {
            match r {
                Ok(()) => {
                    if app.device_error.as_ref().is_some_and(|(p, _)| *p == path) {
                        app.device_error = None;
                    }
                }
                Err(why) => {
                    if app.connecting.as_ref().is_some_and(|c| c.is(&path)) {
                        app.connecting = None;
                    }
                    app.device_error = Some((path, why));
                }
            }
            Task::none()
        }
        Message::StopRecording => {
            // SIGINT, so wf-recorder finishes the file; only this session's recorder (the one
            // Hyprland's state names), never every wf-recorder on the machine.
            if let Some(pid) = app.hypr.recording_pid {
                // Safety: kill(2) on a pid read from this session's Hyprland clients.
                unsafe {
                    libc::kill(pid as libc::pid_t, libc::SIGINT);
                }
            }
            Task::none()
        }
        Message::Panel(m) => panels::update(app, m),
        Message::Ipc(cmd) => app.ipc(&cmd),
        Message::TrayExpand => {
            app.tray_expanded = !app.tray_expanded;
            Task::none()
        }
        Message::TrayMenu(item, output, rect) => {
            let same = app
                .panel
                .as_ref()
                .is_some_and(|p| p.open && p.kind == PanelKind::Tray && p.output == output)
                && panels::tray::showing(&app.panels.tray, item.as_ref());
            let known = item
                .as_ref()
                .is_none_or(|i| panels::tray::cached(&app.panels.tray, i));
            let a = panels::tray::open(&mut app.panels.tray, item);
            let task =
                panels::action::perform(app, a, |m| Message::Panel(panels::Message::Tray(m)));
            let x = rect.unwrap_or_else(|| {
                let w = app.output(output).map_or(1920.0, |o| o.size.width);
                Rectangle::new(iced::Point::new(w - 160.0, 0.0), Size::new(0.0, tok::BAR))
            });
            app.panels.tray.pending = None;
            // Re-opening the same menu closes it; another one switches in place: at once when
            // its menu is known, else as soon as it has been read (never "Loading…" first).
            let shown = if same {
                app.close()
            } else if known {
                app.show(PanelKind::Tray, output, x)
            } else {
                app.panels.tray.pending = Some((output, x));
                Task::none()
            };
            Task::batch([task, shown])
        }
        Message::Batch(list) => Task::batch(list.into_iter().map(Task::done)),
        _ => unreachable!("routed here by app::dispatch"),
    }
}
