//! The dashboard the island and the clock open (one panel, not two: his call 2026-11-16): the
//! day, the weather and music when they have no pill of their own, and the month. Each section appears only when it has something to say.

use super::cx::Cx;
use super::{calendar, media};
use crate::theme::tok;
use crate::theme::{self, Scheme, text};
use crate::ui::{self, Icon, card, control, icon, label, label_w};
use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length};

/// The dashboard: weather, the music playing and the month, composed from their panels.
pub fn view<'a>(
    cal: &'a super::calendar::State,
    media_st: &'a super::media::State,
    cx: &Cx<'a>,
) -> Element<'a, super::Message> {
    let s = cx.scheme;
    // No clock and no date heading: the bar shows both (repeated here they said nothing new;
    // his call 2026-11-16). What has no pill of its own (the weather as the weather panel draws
    // it, what's playing), then the month, today marked.
    let mut col = column![].spacing(tok::L);
    if !cx.config.bar.contains("weather")
        && let Some(w) = &cx.weather
    {
        col = col.push(super::weather::now(cx, w, s));
    }
    if !cx.config.bar.contains("media")
        && let Some(p) = media::current(media_st, cx.media)
    {
        col = col.push(playing(s, p, media::playing(media_st, p)));
    }
    col = col.push(card(
        s,
        calendar::month(cal, cx).map(super::Message::Calendar),
    ));
    container(col).padding(tok::L).into()
}

/// What's playing, with the essential controls.
fn playing<'a>(
    s: Scheme,
    p: &'a crate::services::mpris::Player,
    on: bool,
) -> Element<'a, super::Message> {
    let bus = p.bus.clone();
    let msg = super::Message::Media;
    let button = |i: Icon, m: Option<super::Message>, primary: bool| {
        let fg = if primary {
            s.on_accent
        } else if m.is_some() {
            s.ink
        } else {
            s.ink_faint
        };
        control(container(icon(i, tok::ICON, fg)).center(32))
            .radius(tok::FULL)
            .kind(
                s,
                if primary {
                    crate::ui::control::Kind::Primary
                } else {
                    crate::ui::control::Kind::Ghost
                },
            )
            .on_press_maybe(m)
    };
    let mut meta = column![label_w(
        ui::two_lines(&p.title, text::BODY, 210.0),
        text::BODY,
        s.ink,
        theme::MEDIUM
    )]
    .spacing(tok::XXS)
    .width(Length::Fill);
    if !p.artist.is_empty() {
        meta = meta.push(label(
            ui::fit(&p.artist, text::SMALL, 210.0),
            text::SMALL,
            s.ink_dim,
        ));
    }
    card(
        s,
        row![
            media::art(s, p, 48.0, 10.0),
            meta,
            row![
                button(
                    Icon::SkipBack,
                    p.can_prev
                        .then(|| msg(media::Message::Previous(bus.clone()))),
                    false
                ),
                button(
                    if on { Icon::Pause } else { Icon::Play },
                    Some(msg(media::Message::PlayPause(bus.clone()))),
                    true
                ),
                button(
                    Icon::SkipForward,
                    p.can_next.then(|| msg(media::Message::Next(bus.clone()))),
                    false
                ),
            ]
            .spacing(tok::XXS)
            .align_y(Alignment::Center),
        ]
        .spacing(tok::L)
        .align_y(Alignment::Center),
    )
}
