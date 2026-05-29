//! Settings › appearance: its page.

use super::*;
use crate::theme::tok;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

pub(super) fn appearance_page(app: &App, s: Scheme) -> Vec<Element<'_, AppMessage>> {
    let c = &app.config;
    use crate::config::Material;
    let m = c.material;
    group(
        s,
        None,
        vec![
            row_of(
                s,
                "Look",
                None,
                segments(
                    s,
                    &[
                        (
                            "Black",
                            m == Material::Black,
                            msg(Message::Material(Material::Black)),
                        ),
                        (
                            "Frosted",
                            m == Material::Frosted,
                            msg(Message::Material(Material::Frosted)),
                        ),
                    ],
                ),
            ),
            row_of(s, "Accent colour", None, accent_swatches(app, s)),
            row_of(
                s,
                "Wallpaper",
                None,
                control(label("Choose…", text::SMALL, s.ink))
                    .pill(s, crate::ui::control::Pill::Plain)
                    .on_press(AppMessage::OpenWallpaper)
                    .into(),
            ),
            row_of(
                s,
                "Theme apps",
                None,
                ui::switch(s, c.theme_apps, msg(Message::ThemeApps(!c.theme_apps))),
            ),
        ],
    )
    .into_iter()
    .chain(group(s, Some("Apps"), look_rows(app, s)))
    .collect()
}

/// Apps' fonts, icon theme and pointer: each a chooser opening a searchable list.
pub(super) fn look_rows(app: &App, s: Scheme) -> Vec<Option<Element<'_, AppMessage>>> {
    let st = &app.panels.settings;
    let Some(l) = &st.look else {
        return Vec::new();
    };
    let look = &app.config.look;
    // What apps use now until something is chosen here (not kestrel's defaults).
    let (font, font_size) = match (&look.font, &l.font_now) {
        (None, Some(now)) => (now.0.clone(), look.font_size.unwrap_or(now.1)),
        _ => look.font(),
    };
    let (mono, mono_size) = match (&look.mono, &l.mono_now) {
        (None, Some(now)) => (now.0.clone(), look.mono_size.unwrap_or(now.1)),
        _ => look.mono(),
    };
    let now: [String; 4] = [
        font,
        mono,
        look.icons.clone().unwrap_or_else(|| l.icons_now.clone()),
        look.cursor.clone().unwrap_or_else(|| l.cursor_now.clone()),
    ];
    let names = ["Font", "Monospace font", "Icons", "Pointer"];
    let lists: [&Vec<String>; 4] = [&l.fonts, &l.monos, &l.icons, &l.cursors];
    let mut rows = Vec::new();
    for i in 0..4u8 {
        let open = st.look_open == Some(i);
        let chooser = control(
            row![
                label(now[usize::from(i)].clone(), text::SMALL, s.ink),
                icon(
                    if open {
                        Icon::ChevronUp
                    } else {
                        Icon::ChevronDown
                    },
                    14.0,
                    s.ink_dim
                )
            ]
            .spacing(tok::S)
            .align_y(Alignment::Center),
        )
        .padding([tok::XS, tok::L])
        .radius(14.0)
        .kind(s, crate::ui::control::Kind::Ghost)
        .on_press(msg(Message::LookOpen((!open).then_some(i))));
        rows.push(row_of(s, names[usize::from(i)], None, chooser.into()));
        // Font sizes beside their fonts.
        if i < 2 {
            let size_now = if i == 0 { font_size } else { mono_size };
            let opts: Vec<(&str, bool, AppMessage)> = [9u32, 10, 11, 12, 13, 14]
                .into_iter()
                .map(|n| {
                    (
                        kept(n.to_string()),
                        size_now == n,
                        msg(Message::LookSize(i, n)),
                    )
                })
                .collect();
            rows.push(row_of(
                s,
                if i == 0 { "Size" } else { "Monospace size" },
                None,
                segments(s, &opts),
            ));
        }
        if open {
            let q = st.look_query.trim().to_lowercase();
            let mut list = column![
                container(pick_search(
                    s,
                    "look-search",
                    &st.look_query,
                    Message::LookQuery
                ))
                .padding([tok::XS, tok::NONE])
            ]
            .spacing(tok::XXS);
            for name in lists[usize::from(i)]
                .iter()
                .filter(|n| q.is_empty() || n.to_lowercase().contains(&q))
                .take(40)
            {
                list = list.push(pick_row(
                    s,
                    name.clone(),
                    *name == now[usize::from(i)],
                    Message::LookSet(i, name.clone()),
                ));
            }
            rows.push(Some(container(list).padding([tok::XS, tok::L]).into()));
        }
    }
    rows
}

/// Accent colours to choose from (Material builds the shades from each), after "from the
/// wallpaper".
pub(super) const ACCENTS: [&str; 8] = [
    "#4c8df6", "#2bb3a3", "#5bb55b", "#e6b422", "#f08a3c", "#e5534b", "#e064a8", "#9b6bf2",
];

/// The accent: the wallpaper's (a picture swatch) or one of a few colours, the chosen one
/// ringed.
pub(super) fn accent_swatches<'a>(app: &App, s: Scheme) -> Element<'a, AppMessage> {
    let now = app.config.accent.clone();
    // The chosen swatch has a ring in the ink colour; the others the same space, empty.
    let ringed = move |on: bool, inner: Element<'a, AppMessage>, m: AppMessage| {
        container(
            control(inner)
                .padding(tok::XXS)
                .radius(11.0)
                .kind(s, crate::ui::control::Kind::Ghost)
                .on_press(m),
        )
        .padding(1)
        .style(move |_| container::Style {
            border: iced::Border {
                color: if on { s.ink } else { Color::TRANSPARENT },
                width: 2.0,
                radius: 13.0.into(),
            },
            ..container::Style::default()
        })
    };
    let mut out = row![ringed(
        now.is_none(),
        container(icon(Icon::Image, 12.0, s.ink))
            .center(18)
            .style(move |_| container::Style {
                background: Some(s.ink.scale_alpha(0.14 * s.opacity).into()),
                border: iced::Border::default().rounded(9),
                ..container::Style::default()
            })
            .into(),
        msg(Message::Accent(None)),
    )]
    .spacing(tok::XS)
    .align_y(Alignment::Center);
    for hex in ACCENTS {
        let rgb = u32::from_str_radix(&hex[1..], 16).unwrap_or(0);
        let col = Color::from_rgb8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
        out = out.push(ringed(
            now.as_deref() == Some(hex),
            container(iced::widget::Space::new())
                .width(18)
                .height(18)
                .style(move |_| container::Style {
                    background: Some(col.into()),
                    border: iced::Border::default().rounded(9),
                    ..container::Style::default()
                })
                .into(),
            msg(Message::Accent(Some(hex))),
        ));
    }
    out.into()
}
