//! How the launcher looks.

use super::*;
use iced::widget::{column, row};

pub fn view<'a>(st: &'a State, cx: &Cx<'a>) -> Element<'a, Message> {
    let s = cx.scheme;
    let field = ui::search_field(
        s,
        "Search",
        &st.query,
        INPUT_ID.clone(),
        Message::Query,
        Some(Message::Submit),
    );

    let list = entries(st);
    let mut results = column![].spacing(tok::XXS);
    for (i, e) in list.iter().enumerate() {
        let (lead, title, sub): (Element<'_, Message>, String, String) = match e {
            Entry::App(a) => (
                match icons::lookup(&a.icon) {
                    Some(p) => ui::image_file_faded(&p, 28.0, s.opacity),
                    None => container(icon(Icon::AppWindow, tok::ICON_M, s.ink_dim))
                        .center(28)
                        .into(),
                },
                // Just the name: the app's own description under it was noise (design call).
                a.name.clone(),
                String::new(),
            ),
            Entry::Calc(v) => (
                container(icon(Icon::Calculator, tok::ICON_M, s.ink_dim))
                    .center(28)
                    .into(),
                v.clone(),
                String::new(),
            ),
            Entry::Emoji(e, n) => (
                container(label(e.to_string(), 20.0, s.ink).font(iced::Font::with_name("Twemoji")))
                    .center(28)
                    .into(),
                n.to_string(),
                String::new(),
            ),
            Entry::Web(t) => (
                container(icon(Icon::Globe, tok::ICON_M, s.ink_dim))
                    .center(28)
                    .into(),
                format!("Search the web for \u{201c}{t}\u{201d}"),
                "DuckDuckGo".into(),
            ),
            Entry::File(p) => (
                container(icon(
                    if p.is_dir() { Icon::Folder } else { Icon::File },
                    18.0,
                    s.ink_dim,
                ))
                .center(28)
                .into(),
                p.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                p.parent()
                    .map(|d| {
                        d.to_string_lossy()
                            .replace(&std::env::var("HOME").unwrap_or_default(), "~")
                    })
                    .unwrap_or_default(),
            ),
            Entry::Process(pid, n, kib) => (
                container(icon(
                    Icon::Activity,
                    18.0,
                    if st.armed == Some(*pid) {
                        s.bad
                    } else {
                        s.ink_dim
                    },
                ))
                .center(28)
                .into(),
                if st.armed == Some(*pid) {
                    format!("End {n}?")
                } else {
                    n.clone()
                },
                format!("{} MB · pid {pid}", kib / 1024),
            ),
            Entry::Clip(c) => match &c.thumb {
                Some(t) => (
                    match ui::rounded_file(t, 56.0, 36.0, 6.0) {
                        Some(h) => iced::widget::image(h)
                            .width(56.0)
                            .height(36.0)
                            .opacity(s.opacity)
                            .into(),
                        None => container(icon(Icon::Image, tok::ICON_M, s.ink_dim))
                            .center(36)
                            .into(),
                    },
                    "Image".to_string(),
                    c.preview
                        .split_whitespace()
                        .rev()
                        .find(|w| w.contains('x'))
                        .map(|d| d.replace(']', "").replace('x', " × "))
                        .unwrap_or_default(),
                ),
                None => (
                    container(icon(Icon::Clipboard, tok::ICON_M, s.ink_dim))
                        .center(28)
                        .into(),
                    ui::ellipsize(&c.preview, 64),
                    String::new(),
                ),
            },
            Entry::Run(c) => (
                container(icon(Icon::Terminal, tok::ICON_M, s.ink_dim))
                    .center(28)
                    .into(),
                c.clone(),
                "Run".into(),
            ),
            Entry::Setting(p) => (
                container(icon(p.icon(), tok::ICON_M, s.ink_dim))
                    .center(28)
                    .into(),
                p.name().to_string(),
                "Settings".into(),
            ),
            Entry::Switch(w) => (
                container(icon(w.icon(), tok::ICON_M, s.ink_dim))
                    .center(28)
                    .into(),
                w.name().to_string(),
                "Switch on or off".into(),
            ),
        };
        let mut texts =
            column![label_w(title, text::BODY, s.ink, theme::MEDIUM)].spacing(tok::HAIR);
        if !sub.is_empty() {
            texts = texts.push(label(ui::ellipsize(&sub, 70), text::CAPTION, s.ink_dim));
        }
        results = results.push(
            control(row![lead, texts].spacing(tok::L).align_y(Alignment::Center))
                .padding([tok::NONE, tok::L])
                .height(tok::H_ROW2)
                .width(Length::Fill)
                .radius(tok::R_ROW())
                // The highlight underneath is the selection, and the pointer moves it: no
                // second hover colour that would trail behind.
                .kind(s, crate::ui::control::Kind::Bare)
                .no_press_scale()
                .on_enter(Message::Hover(i))
                .on_press(Message::Launch(i)),
        );
    }
    let mut col = column![field].spacing(tok::M);
    let show_highlight = st.touched || !st.query.trim().is_empty();
    if !list.is_empty() && !show_highlight {
        col = col.push(iced::widget::mouse_area(results).on_exit(Message::LeaveRows));
    } else if !list.is_empty() {
        // One highlight under the selected row.
        let highlight = container(Space::new())
            .width(Length::Fill)
            .height(tok::H_ROW2)
            .style(move |_| container::Style {
                background: Some(s.selected().into()),
                border: iced::Border::default().rounded(tok::FULL),
                ..container::Style::default()
            });
        let y = st
            .glide
            .value
            .clamp(0.0, list.len().saturating_sub(1) as f32)
            * 50.0;
        // Exactly the rows' height: a layer that filled the space made the panel measure its
        // content wrongly (no background, misplaced highlight).
        let height = list.len() as f32 * 50.0 - 2.0;
        col = col.push(
            iced::widget::mouse_area(
                iced::widget::stack![
                    iced::widget::pin(highlight)
                        .y(y)
                        .width(Length::Fill)
                        .height(Length::Fixed(height)),
                    results,
                ]
                .height(Length::Fixed(height)),
            )
            .on_exit(Message::LeaveRows),
        );
    }
    container(col).padding(tok::L).into()
}
