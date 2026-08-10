//! Tray icons: a click activates, right click opens the app's menu (with "Keep in the bar").
//! Apps not kept in the bar hide behind the arrow, which slides them out inline.

use crate::app::{App, Message, Output};
use crate::panels::{Message as PanelMessage, PanelKind};
use crate::services::{icons, tray as tray_service};
use crate::theme::{Scheme, tok};
use crate::ui::{self, Icon, icon};
use iced::Element;
use iced::widget::image;

pub fn tray_icon<'a>(s: Scheme, item: &tray_service::Item) -> Element<'a, Message> {
    tray_icon_at(s, item, tok::ICON)
}

/// A tray app's icon at `px`. Apps that send a picture instead of an icon name (Electron
/// apps: Vesktop, ZapFast) usually send a plain white silhouette, which read as a generic
/// window; their own app icon (as in the launcher) is used when one is found.
pub fn tray_icon_at<'a>(s: Scheme, item: &tray_service::Item, px: f32) -> Element<'a, Message> {
    if matches!(item.icon, Some(tray_service::TrayIcon::Rgba(_)) | None) {
        let id = item.id.to_lowercase();
        let found = [
            id.clone(),
            id.split(['_', '-']).next().unwrap_or_default().to_string(),
            item.title.to_lowercase(),
        ]
        .into_iter()
        .filter(|c| !c.is_empty())
        .find_map(|c| icons::lookup(&crate::services::apps::icon_for_class(&c)));
        if let Some(p) = found {
            return ui::image_file_faded(&p, px, s.opacity);
        }
    }
    match &item.icon {
        Some(tray_service::TrayIcon::Name(n)) => match icons::lookup(n) {
            Some(p) => ui::image_file_faded(&p, px, s.opacity),
            None => icon(Icon::AppWindow, px, s.ink_dim).into(),
        },
        Some(tray_service::TrayIcon::Rgba(p)) => image(crate::ui::pixels_at(p, px))
            .width(px)
            .height(px)
            .opacity(s.opacity)
            .into(),
        None => icon(Icon::AppWindow, px, s.ink_dim).into(),
    }
}

/// Tray icons: click activates, right click opens the app's menu (with "Keep in the bar").
/// Apps not kept in the bar hide behind the arrow; the arrow slides them out inline, in the
/// bar itself (like macOS's Ice), and tucks them away again. No panel to open.
pub fn view<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    let id = o.id;
    let tray_open = open == Some(PanelKind::Tray);
    let pins = &app.panels.tray.pins;
    let mut items: Vec<Element<'a, Message>> = Vec::new();
    let mut hidden = 0;
    // Idle ("passive") apps too: they live behind the arrow like the rest.
    for item in &app.tray.items {
        let pinned = pins.contains(&crate::panels::tray::pin_key(item));
        if !pinned {
            hidden += 1;
            if !app.tray_expanded {
                continue;
            }
        }
        let active = tray_open && crate::panels::tray::showing(app, Some(item));
        let menu = item.clone();
        let mut ctl =
            ui::control(iced::widget::container(tray_icon(s, item)).center(tok::BAR_ITEM))
                .radius(tok::BAR_ITEM / 2.0)
                .bar(s, active)
                .on_right_press(Message::TrayMenu(Some(menu), id, None));
        ctl = if item.item_is_menu {
            let it = item.clone();
            ctl.on_press_at(move |r| Message::TrayMenu(Some(it.clone()), id, Some(r)))
        } else {
            ctl.on_press(Message::Panel(PanelMessage::TrayActivate(item.clone())))
        };
        items.push(ctl.into());
    }
    if hidden > 0 {
        let out = app.tray_expanded;
        items.push(
            // A full bubble like the icons beside it, in the same white and size (a thin,
            // dimmed half-width arrow read as something broken).
            ui::control(
                iced::widget::container(icon(
                    if out {
                        Icon::ChevronRight
                    } else {
                        Icon::ChevronLeft
                    },
                    tok::ICON,
                    s.ink,
                ))
                .center(tok::BAR_ITEM),
            )
            .radius(tok::BAR_ITEM / 2.0)
            .bar(s, false)
            .on_press(Message::TrayExpand)
            .into(),
        );
    }
    items
}
