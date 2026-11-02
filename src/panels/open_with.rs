//! "Open with…": which app opens a file or link, when an app asks (the portal's AppChooser,
//! crate::services::appchooser). The apps offered, each with its icon and name, the one used
//! last time first; a click (or Enter) opens with it, Cancel or closing the panel says no.
//! Shown only while something asks, like the file picker.

use super::action::Action;
use super::cx::Cx;
use crate::services::appchooser::{self, Ask};
use crate::theme::{self, text, tok};
use crate::ui::{self, Icon, control, label, label_w};
use iced::widget::{column, container, row};
use iced::{Alignment, Element, Length};

pub const WIDTH: f32 = tok::PANEL_S;

/// The first choice: it has the keyboard when the panel opens, so Enter opens with the app
/// suggested (a list starts on its first item: Material 3, Raycast).
pub static FIRST: std::sync::LazyLock<iced::widget::Id> =
    std::sync::LazyLock::new(|| iced::widget::Id::new("open-with-first"));

type Act = Action<Message>;

#[derive(Default)]
pub struct State {
    /// The question being shown.
    pub ask: Option<Ask>,
}

#[derive(Clone, Debug)]
pub enum Message {
    Choose(String),
    Cancel,
}

/// A question arrived: an earlier one still shown is answered "cancelled" (one at a time).
pub fn begin(st: &mut State, ask: Ask) {
    if let Some(old) = st.ask.replace(ask) {
        appchooser::answer(old.id, None);
    }
}

/// The app offers other choices for the question shown.
pub fn update_choices(st: &mut State, id: u64, choices: Vec<String>) {
    if let Some(a) = st.ask.as_mut().filter(|a| a.id == id) {
        a.choices = choices;
    }
}

/// The panel closed without a choice: the app hears "cancelled".
pub fn closed(st: &mut State) {
    if let Some(a) = st.ask.take() {
        appchooser::answer(a.id, None);
    }
}

pub fn update(st: &mut State, m: Message) -> Act {
    let Some(a) = st.ask.take() else {
        return Act::Close;
    };
    appchooser::answer(
        a.id,
        match m {
            Message::Choose(c) => Some(c),
            Message::Cancel => None,
        },
    );
    Act::Close
}

/// The choices in the order shown: the last one used first, then as the app gave them.
fn ordered(a: &Ask) -> Vec<&str> {
    let mut v: Vec<&str> = a.choices.iter().map(String::as_str).collect();
    if let Some(last) = a.last.as_deref()
        && let Some(i) = v.iter().position(|c| *c == last)
    {
        let c = v.remove(i);
        v.insert(0, c);
    }
    v
}

pub fn view<'a>(st: &'a State, cx: &Cx<'a>) -> Element<'a, Message> {
    let s = cx.scheme;
    let Some(a) = &st.ask else {
        return iced::widget::Space::new().into();
    };
    let title = if a.what.is_empty() {
        "Open with".to_string()
    } else {
        format!("Open “{}” with", ui::ellipsize(&a.what, 32))
    };
    let mut list = column![].spacing(tok::XXS);
    for (i, id) in ordered(a).into_iter().enumerate() {
        let name = crate::services::apps::name_for_class(id);
        let mut words = column![label_w(name, text::BODY, s.ink, theme::MEDIUM)];
        if i == 0 && a.last.as_deref() == Some(id) {
            words = words.push(label("Used last time", text::CAPTION, s.ink_dim));
        }
        let row = control(
            row![ui::app_icon(s, id, tok::ICON_XXL, Icon::AppWindow), words]
                .spacing(tok::L)
                .align_y(Alignment::Center),
        )
        .padding([tok::S, tok::L])
        .width(Length::Fill)
        .radius(tok::R_ROW())
        .kind(s, crate::ui::control::Kind::Ghost)
        .on_press(Message::Choose(id.to_string()));
        list = list.push(if i == 0 { row.id(FIRST.clone()) } else { row });
    }
    let body: Element<'a, Message> = if a.choices.is_empty() {
        ui::empty_reason(s, "No app here opens this".into(), tok::H_ROW2)
    } else {
        list.into()
    };
    let head = row![
        label_w(title, text::TITLE, s.ink, theme::SEMIBOLD).width(Length::Fill),
        ui::button(
            s,
            "Cancel",
            crate::ui::control::Pill::Plain,
            Some(Message::Cancel)
        ),
    ]
    .spacing(tok::M)
    .align_y(Alignment::Center);
    container(column![head, body].spacing(tok::L))
        .padding(tok::PANEL_PAD)
        .width(Length::Fill)
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_choice_comes_first() {
        let a = Ask {
            choices: vec!["org.a".into(), "org.b".into(), "org.c".into()],
            last: Some("org.c".into()),
            ..Default::default()
        };
        assert_eq!(ordered(&a), ["org.c", "org.a", "org.b"]);
        let none = Ask {
            choices: vec!["org.a".into()],
            last: Some("gone".into()),
            ..Default::default()
        };
        assert_eq!(ordered(&none), ["org.a"]);
    }
}
