//! Opening, routing and closing panels; commands; batches.

use super::super::*;

#[allow(clippy::too_many_lines)]
pub(crate) fn handle(app: &mut App, msg: Message) -> Task<Message> {
    match msg {
        Message::Toggle(kind, output, button) => {
            // Clicked: the keyboard stays with the app in use (see `sync_surfaces`). Pressed
            // with Enter while the bar has the keyboard: the panel takes it.
            app.panel_keys = app.bar_keys.is_some();
            app.toggle(kind, output, button)
        }
        Message::BarKey(k) => app.bar_key(k),
        Message::ControlAt(page, output, button) => {
            app.panel_keys = app.bar_keys.is_some();
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
        // Freed pictures go back to the system by themselves (main.rs, memory_policy).
        Message::Close => app.close(),
        // Esc in Settings' window is Settings' (a list to close, a drag to cancel), never the
        // open panel's.
        Message::Escape(w) if panels::in_settings(app, w) => Task::none(),
        Message::Escape(_) => handle(app, Message::Close),
        Message::Dispatch(a) => Task::future(async move {
            crate::services::compositor::run(a).await;
        })
        .discard(),
        Message::DeviceDone(path, r) => app.device_done(path, r),
        Message::Change(c) => app.change(c),
        Message::NetDone(ssid, r) => app.net_done(ssid, r),
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
            app.panel_keys = app.bar_keys.is_some();
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
