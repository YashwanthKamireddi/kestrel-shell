//! Workspaces: a click goes there, scrolling steps through them.

use crate::app::{App, Message, Output};
use crate::panels::PanelKind;
use crate::shell::bar::{SLOT, SLOT_GAP};
use crate::theme::{self, Scheme, text};
use crate::ui::{self, label_w};
use iced::Element;
use iced::widget::{Space, container, pin, row, stack};

/// Workspaces, the way ii draws them: each occupied one sits on a soft background, and
/// neighbours that are both occupied join into one shape; the one you are on is an accent
/// circle that stretches toward the next as you move (its two edges are separate springs:
/// the leading edge goes first, the trailing one follows) and settles round on it. Open
/// workspaces show their app's icon, empty ones a quiet number.
pub fn view<'a>(
    app: &'a App,
    o: &'a Output,
    s: Scheme,
    _open: Option<PanelKind>,
) -> Vec<Element<'a, Message>> {
    vec![slots(app, o, s)]
}

fn slots<'a>(app: &'a App, o: &'a Output, s: Scheme) -> Element<'a, Message> {
    let active = app.hypr.active_on(&o.name).unwrap_or(1);
    let (first, n) = crate::shell::bar::group(active, app.config.workspaces);
    let prev = crate::shell::bar::scroll_target(&app.hypr, active, app.config.workspaces, -1);
    let next = crate::shell::bar::scroll_target(&app.hypr, active, app.config.workspaces, 1);
    let stride = SLOT + SLOT_GAP;
    // The circle and the occupied shapes: a little smaller than a slot.
    const DOT: f32 = 22.0;
    let occupied = |id: i32| {
        app.hypr
            .workspaces
            .iter()
            .any(|w| w.id == id && w.windows > 0)
    };
    // The active circle's span (left and right edges, springing separately).
    let (cap_l, cap_r) = (o.pill_x.value, o.pill_r.value.max(o.pill_x.value + SLOT));
    let slots = row((first..first + n as i32).map(|id| {
        let left = (id - first) as f32 * stride;
        // How much of the active circle covers this slot: its content turns dark as it does.
        let over = ((cap_r.min(left + SLOT) - cap_l.max(left)) / SLOT).clamp(0.0, 1.0);
        let ws = app
            .hypr
            .workspaces
            .iter()
            .find(|w| w.id == id && w.windows > 0);
        let icon_of = ws.filter(|w| !w.app.is_empty()).and_then(|w| {
            crate::services::icons::lookup(&crate::services::apps::icon_for_class(&w.app))
        });
        // On the accent circle the number is in the accent's own dark ink, as on the toggles.
        let ink = crate::theme::color::mix(
            if ws.is_some() { s.ink_dim } else { s.ink_faint },
            s.on_accent,
            over,
        );
        let content: Element<'a, Message> = match icon_of {
            Some(path) => ui::image_file(&path, 16.0),
            None => label_w(id.to_string(), text::SMALL, ink, theme::SEMIBOLD).into(),
        };
        ui::control(container(content).center(SLOT))
            .radius(SLOT / 2.0)
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
    .spacing(SLOT_GAP);
    let width = n as f32 * stride - SLOT_GAP;
    let mut layers = stack![].width(width).height(SLOT);
    // Occupied runs: one rounded shape from the first to the last of each run.
    let last = first + n as i32 - 1;
    let mut id = first;
    while id <= last {
        if !occupied(id) {
            id += 1;
            continue;
        }
        let start = id;
        while id < last && occupied(id + 1) {
            id += 1;
        }
        let x = (start - first) as f32 * stride;
        let w = (id - start + 1) as f32 * stride - SLOT_GAP;
        let blob = container(Space::new())
            .width(w - (SLOT - DOT))
            .height(DOT)
            .style(move |_| container::Style {
                background: Some(s.ink.scale_alpha(0.08 * s.opacity).into()),
                border: iced::Border::default().rounded(DOT / 2.0),
                ..container::Style::default()
            });
        layers = layers.push(
            pin(blob)
                .x(x + (SLOT - DOT) / 2.0)
                .y((SLOT - DOT) / 2.0)
                .width(width)
                .height(SLOT),
        );
        id += 1;
    }
    // Small and see-through (design call 2026-11-13: the full-size bright circle took over the
    // workspace it marked): the accent at part strength, just behind the icon or number.
    let active = container(Space::new())
        .width(cap_r - cap_l - (SLOT - DOT))
        .height(DOT)
        .style(move |_| container::Style {
            // The same solid accent as the control centre's toggles and sliders (a faded one
            // looked dull and unlike them); small, so it marks without taking over.
            background: Some(s.fade(s.accent).into()),
            border: iced::Border::default().rounded(DOT / 2.0),
            ..container::Style::default()
        });
    layers
        .push(
            pin(active)
                .x(cap_l + (SLOT - DOT) / 2.0)
                .y((SLOT - DOT) / 2.0)
                .width(width)
                .height(SLOT),
        )
        .push(slots)
        .into()
}
