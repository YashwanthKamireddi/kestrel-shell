//! Settings › appearance: its page.

use super::*;
use crate::theme::tok;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

pub(super) fn appearance_page<'a>(st: &'a State, cx: &Cx<'a>) -> Vec<Element<'a, Message>> {
    let s = cx.scheme;
    let c = &cx.config;
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
                            Message::Material(Material::Black),
                        ),
                        (
                            "Frosted",
                            m == Material::Frosted,
                            Message::Material(Material::Frosted),
                        ),
                    ],
                ),
            ),
            row_of(s, "Accent colour", None, accent_swatches(cx)),
            row_of(
                s,
                "Wallpaper",
                None,
                control(label("Choose…", text::SMALL, s.ink))
                    .pill(s, crate::ui::control::Pill::Plain)
                    .on_press(Message::OpenWallpaper)
                    .into(),
            ),
            row_of(
                s,
                "Theme apps",
                None,
                ui::switch(s, c.theme_apps, Message::ThemeApps(!c.theme_apps)),
            ),
        ],
    )
    .into_iter()
    .chain(group(s, Some("Apps"), look_rows(st, cx)))
    .collect()
}

/// Apps' fonts, icon theme and pointer: each a chooser opening a searchable list.
pub(super) fn look_rows<'a>(st: &'a State, cx: &Cx<'a>) -> Vec<Option<Element<'a, Message>>> {
    let s = cx.scheme;
    let Some(l) = &st.look else {
        return Vec::new();
    };
    let look = &cx.config.look;
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
        .radius(tok::FULL)
        .kind(s, crate::ui::control::Kind::Ghost)
        .on_press(Message::LookOpen((!open).then_some(i)));
        rows.push(row_of(s, names[usize::from(i)], None, chooser.into()));
        // Font sizes beside their fonts.
        if i < 2 {
            let size_now = if i == 0 { font_size } else { mono_size };
            let opts: Vec<(&str, bool, Message)> = [9u32, 10, 11, 12, 13, 14]
                .into_iter()
                .map(|n| (kept(n.to_string()), size_now == n, Message::LookSize(i, n)))
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
pub(super) fn accent_swatches<'a>(cx: &Cx<'a>) -> Element<'a, Message> {
    let s = cx.scheme;
    let now = cx.config.accent.clone();
    // The chosen swatch has a ring in the ink colour; the others the same space, empty.
    let ringed = move |on: bool, inner: Element<'a, Message>, m: Message| {
        container(
            control(inner)
                .padding(tok::XXS)
                .radius(tok::FULL)
                .kind(s, crate::ui::control::Kind::Ghost)
                .on_press(m),
        )
        .padding(tok::HAIR)
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
        container(icon(Icon::Image, tok::ICON_XS, s.ink))
            .center(18)
            .style(move |_| container::Style {
                background: Some(s.ink.scale_alpha(tok::SELECTED * s.opacity).into()),
                border: iced::Border::default().rounded(tok::FULL),
                ..container::Style::default()
            })
            .into(),
        Message::Accent(None),
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
                    border: iced::Border::default().rounded(tok::FULL),
                    ..container::Style::default()
                })
                .into(),
            Message::Accent(Some(hex)),
        ));
    }
    out.into()
}
