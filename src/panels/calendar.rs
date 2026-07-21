//! Calendar: today at a glance, then the month.

use super::Message as PanelMessage;
use crate::app::{App, Message as AppMessage};
use crate::theme::tok;
use crate::theme::{self, Scheme, size, text};
use crate::ui::{Icon, control, icon_button, label, label_w};
use chrono::{Datelike, Duration, NaiveDate};
use iced::widget::{Space, column, container, row};
use iced::{Alignment, Element, Length};

#[derive(Default)]
pub struct State {
    /// Months away from the current one.
    offset: i32,
    /// Scrolling not yet worth a month (a touchpad sends small steps).
    scroll: f32,
}

#[derive(Clone, Debug)]
pub enum Message {
    Shift(i32),
    Today,
    /// The wheel or touchpad over the month, in lines (up: positive).
    Scroll(f32),
}

impl State {
    pub fn update(&mut self, m: Message) {
        match m {
            Message::Shift(d) => self.offset += d,
            Message::Today => self.offset = 0,
            // As in GNOME's calendar: down is the next month. A wheel notch is a month; small
            // touchpad steps add up to one.
            Message::Scroll(lines) => {
                self.scroll += lines;
                while self.scroll >= 1.0 {
                    self.scroll -= 1.0;
                    self.offset -= 1;
                }
                while self.scroll <= -1.0 {
                    self.scroll += 1.0;
                    self.offset += 1;
                }
            }
        }
    }
}

fn msg(m: Message) -> AppMessage {
    AppMessage::Panel(PanelMessage::Calendar(m))
}

fn first_of_month(today: NaiveDate, offset: i32) -> NaiveDate {
    let months = today.year() * 12 + today.month0() as i32 + offset;
    NaiveDate::from_ymd_opt(months.div_euclid(12), months.rem_euclid(12) as u32 + 1, 1)
        .unwrap_or(today)
}

const CELL_H: f32 = 34.0;

/// The clock's panel is the island's dashboard (they were two panels with the same clock and
/// month).
pub fn view(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    super::island::view(app, s)
}

/// The month: title with navigation, weekday initials, six weeks.
pub fn month(app: &App, s: Scheme) -> Element<'_, AppMessage> {
    let today = app.now.date_naive();
    let first = first_of_month(today, app.panels.calendar.offset);
    let month_title = control(label_w(
        first.format("%B %Y").to_string(),
        text::TITLE,
        s.ink,
        theme::MEDIUM,
    ))
    .padding([tok::XS, tok::M])
    .radius(size::CONTROL_RADIUS)
    .kind(s, crate::ui::control::Kind::Ghost)
    .on_press(msg(Message::Today));

    let nav = row![
        month_title,
        Space::new().width(Length::Fill),
        icon_button(s, Icon::ChevronLeft, msg(Message::Shift(-1)), false),
        icon_button(s, Icon::ChevronRight, msg(Message::Shift(1)), false),
    ]
    .spacing(tok::XXS)
    .align_y(Alignment::Center);

    // The week starts on Monday, or on Sunday (Settings > Date & time).
    let sunday = app.config.clock.week_starts_sunday;
    let names = if sunday {
        ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"]
    } else {
        ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"]
    };
    let weekdays = row(names.map(|d| {
        container(label(d, text::CAPTION, s.ink_faint))
            .center_x(Length::Fill)
            .into()
    }));

    // Six weeks starting on the first weekday on or before the 1st.
    let back = if sunday {
        first.weekday().num_days_from_sunday()
    } else {
        first.weekday().num_days_from_monday()
    };
    let start = first - Duration::days(i64::from(back));
    let mut weeks = column![].spacing(tok::XXS);
    for w in 0..6 {
        let mut week = row![];
        for d in 0..7 {
            let date = start + Duration::days(w * 7 + d);
            week = week.push(day(s, date, today, date.month() == first.month()));
        }
        weeks = weeks.push(week);
    }

    let weeks = iced::widget::mouse_area(weeks).on_scroll(|d| {
        msg(Message::Scroll(match d {
            iced::mouse::ScrollDelta::Lines { y, .. } => y.signum(),
            iced::mouse::ScrollDelta::Pixels { y, .. } => y / 60.0,
        }))
    });
    column![
        nav,
        Space::new().height(tok::M),
        weekdays,
        Space::new().height(tok::S),
        weeks
    ]
    .into()
}

fn day<'a>(
    s: Scheme,
    date: NaiveDate,
    today: NaiveDate,
    in_month: bool,
) -> Element<'a, AppMessage> {
    // Other months' days are left blank: the month stands on its own.
    if !in_month {
        return Space::new().width(Length::Fill).height(CELL_H).into();
    }
    let is_today = date == today;
    // Weekends a step back, like macOS's calendar.
    let weekend = date.weekday().number_from_monday() >= 6;
    let color = if is_today {
        s.on_accent
    } else if weekend {
        s.ink_dim
    } else {
        s.ink
    };
    let weight = if is_today {
        theme::SEMIBOLD
    } else {
        iced::font::Weight::Medium
    };
    let n = label_w(date.day().to_string(), text::BODY, color, weight);
    let disc = container(n).center(30).style(move |_| container::Style {
        background: is_today.then(|| s.accent.into()),
        border: iced::Border::default().rounded(tok::FULL),
        ..container::Style::default()
    });
    container(disc)
        .center_x(Length::Fill)
        .height(CELL_H)
        .center_y(CELL_H)
        .into()
}
