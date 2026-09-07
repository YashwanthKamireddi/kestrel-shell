//! Settings › account: its page.

use super::*;
use crate::theme::tok;
// Explicit: the prelude has a `column!` of its own (the source line).
use iced::widget::column;

pub(super) fn account_page<'a>(st: &'a State, cx: &Cx<'a>) -> Vec<Element<'a, Message>> {
    let s = cx.scheme;
    let Some((login, full, admin)) = &st.account else {
        return Vec::new();
    };
    let shown = if full.is_empty() {
        login.clone()
    } else {
        full.clone()
    };
    let name_control: Element<'_, Message> = match &st.full_name {
        Some(t) => container(
            text_input("Full name", t)
                .id(iced::widget::Id::new("full-name"))
                .on_input(|t| Message::FullName(Some(t)))
                .on_submit(Message::FullNameSubmit)
                .padding([tok::S, tok::L])
                .size(text::BODY)
                .font(theme::FONT)
                .width(220)
                .style(crate::ui::input_style(s)),
        )
        .style(move |_| ui::field_style(s))
        .into(),
        None => control(label(shown.clone(), text::SMALL, s.ink))
            .pill(s, crate::ui::control::Pill::Plain)
            .on_press(Message::FullName(Some(full.clone())))
            .into(),
    };
    let head = container(
        row![
            // The picture (round) or the person icon; a click chooses a new one.
            control(
                match face_path()
                    .filter(|p| p.exists())
                    .and_then(|p| ui::rounded_file(&p, 56.0, 56.0, 28.0))
                {
                    Some(h) => Element::from(iced::widget::image(h).width(56).height(56)),
                    None => container(icon(Icon::User, tok::ICON_HERO, s.ink_dim))
                        .center(56)
                        .into(),
                },
            )
            .radius(tok::FULL)
            .kind(s, crate::ui::control::Kind::Ghost)
            .on_press(Message::PickPicture),
            column![
                label_w(shown, text::TITLE, s.ink, theme::SEMIBOLD),
                label(
                    if *admin {
                        "Administrator"
                    } else {
                        "Standard user"
                    },
                    text::SMALL,
                    s.ink_dim
                ),
            ]
            .spacing(tok::XXS),
        ]
        .spacing(tok::L)
        .align_y(Alignment::Center),
    )
    .padding([tok::XS, tok::XS]);
    let mut out: Vec<Element<'_, Message>> = vec![head.into()];
    out.extend(group(
        s,
        None,
        vec![
            row_of(s, "Full name", None, name_control),
            value_row(s, "User name", login.clone()),
            row_of(
                s,
                "Password",
                None,
                control(label("Change…", text::SMALL, s.ink))
                    .pill(s, crate::ui::control::Pill::Plain)
                    .on_press(Message::Batch(vec![
                        Message::InTerminal(
                            "sh -c 'passwd; echo; read -p \"Press Enter to close.\" _'",
                            "Password",
                        ),
                        Message::Close,
                    ]))
                    .into(),
            ),
        ],
    ));
    if let Some(e) = &st.error {
        out.push(label(e.clone(), text::SMALL, s.bad).into());
    }
    out
}

/// The account picture: `~/.face` (what login screens, lock screens and GTK read).
pub(super) fn face_path() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".face"))
}

/// Makes `src` the account picture: a centred square, 256 px, as PNG at ~/.face.
pub(super) fn set_face(src: &std::path::Path) -> Result<(), String> {
    let img = ::image::open(src).map_err(|e| e.to_string())?;
    let side = img.width().min(img.height());
    let sq = img.crop_imm(
        (img.width() - side) / 2,
        (img.height() - side) / 2,
        side,
        side,
    );
    let small = sq.resize_exact(256, 256, ::image::imageops::FilterType::Lanczos3);
    let out = face_path().ok_or("no home folder")?;
    small
        .save_with_format(&out, ::image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    ui::forget_picture(&out);
    Ok(())
}
