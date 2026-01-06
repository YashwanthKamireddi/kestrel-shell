//! The surfaces every monitor gets: wallpaper, frame and bar.

pub mod bar;

use crate::app::{App, Message};
use crate::ui::shape::{Edge, FrameShape};
use iced::widget::{Space, canvas, container, image};
use iced::{ContentFit, Element, Length};

pub fn wallpaper(app: &App) -> Element<'_, Message> {
    match &app.wallpaper {
        Some(w) => image(w.handle.clone())
            .content_fit(ContentFit::Cover)
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        None => {
            let base = app.scheme.base;
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(base.into()),
                    ..container::Style::default()
                })
                .into()
        }
    }
}

pub fn frame(app: &App, edge: Edge) -> Element<'_, Message> {
    canvas(FrameShape {
        color: app.scheme.base,
        edge,
        frame: app.config.frame.thickness,
        corner: app.config.frame.radius,
    })
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
