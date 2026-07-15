//! Workspaces: a click goes there, scrolling steps through them.

use crate::app::{App, Message, Output};
use crate::panels::PanelKind;
use crate::shell::bar::{SLOT, SLOT_GAP};
use crate::theme::Scheme;
use crate::theme::tok;
use crate::ui;
use iced::Element;
use iced::widget::{Space, container, row};

/// Workspaces, the way DMS draws them (DBar/Widgets/WorkspaceSwitcher.qml, its defaults:
/// "pills", no numbers, no app icons): a short quiet pill for each workspace, the one you are on
/// a longer accent pill (BarMetrics indicatorRatios pills: compact 0.7, active 1.05 of the
/// widget thickness long, 0.5 of it tall), spacingS apart. Moving, the accent and the length
/// pass smoothly from one to the next (the workspace springs).
pub fn view<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    _open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    vec![slots(app, o, s)]
}

fn slots<'a>(app: &'a App, o: &'a Output, s: Scheme) -> Element<'a, Message> {
    use crate::theme::bar;
    let short = (bar::PILL * 0.7).round();
    let long = (bar::PILL * 1.05).round();
    let tall = (bar::PILL * 0.5).round();
    let active = app.hypr.active_on(&o.name).unwrap_or(1);
    let (first, n) = crate::shell::bar::group(active, app.config.workspaces);
    let prev = crate::shell::bar::scroll_target(&app.hypr, active, app.config.workspaces, -1);
    let next = crate::shell::bar::scroll_target(&app.hypr, active, app.config.workspaces, 1);
    // Where the active one is, in workspaces from the first (the spring, so it glides).
    let at = o.pill_x.value / (SLOT + SLOT_GAP);
    let quiet = s.ink.scale_alpha(tok::QUIET * s.opacity);
    let accent = s.fade(s.accent);
    row((first..first + n as i32).map(|id| {
        let k = (1.0 - ((id - first) as f32 - at).abs()).clamp(0.0, 1.0);
        let w = short + (long - short) * k;
        // Straight between the two, transparency too (the quiet one is see-through).
        let colour = iced::Color {
            r: quiet.r + (accent.r - quiet.r) * k,
            g: quiet.g + (accent.g - quiet.g) * k,
            b: quiet.b + (accent.b - quiet.b) * k,
            a: quiet.a + (accent.a - quiet.a) * k,
        };
        let pill = container(Space::new())
            .width(w)
            .height(tall)
            .style(move |_| container::Style {
                background: Some(colour.into()),
                border: iced::Border::default().rounded(tall / 2.0),
                ..container::Style::default()
            });
        ui::control(container(pill).center_x(w).center_y(bar::PILL))
            .radius(tok::FULL)
            .on_press(Message::Dispatch(
                crate::services::compositor::Action::FocusWorkspace(id),
            ))
            .on_scroll(move |lines| {
                Message::Dispatch(crate::services::compositor::Action::FocusWorkspace(
                    if lines > 0.0 { prev } else { next },
                ))
            })
            .into()
    }))
    .spacing(tok::M)
    // As far from the pill's ends as from its top and bottom.
    .padding([0.0, (bar::PILL - tall) / 2.0])
    .into()
}
