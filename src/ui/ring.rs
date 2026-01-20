//! A ring gauge: a quiet track and an arc for the value, with round ends. Drawn by the canvas
//! (cached by iced while the value is unchanged), so it is cheap.

use iced::widget::canvas::{self, Frame, Geometry, LineCap, Path, Stroke};
use iced::{Color, Element, Length, Point, Radians, Rectangle, Renderer, Theme, mouse};

struct Ring {
    value: f32,
    color: Color,
    track: Color,
    width: f32,
}

impl<M> canvas::Program<M> for Ring {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let c = frame.center();
        let r = bounds.width.min(bounds.height) / 2.0 - self.width / 2.0 - 0.5;
        let stroke = |color: Color| Stroke::default().with_width(self.width).with_color(color);
        frame.stroke(&Path::circle(c, r), stroke(self.track));
        let v = self.value.clamp(0.0, 1.0);
        if v > 0.002 {
            let start = -std::f32::consts::FRAC_PI_2;
            let arc = Path::new(|b| {
                b.arc(canvas::path::Arc {
                    center: Point::new(c.x, c.y),
                    radius: r,
                    start_angle: Radians(start),
                    end_angle: Radians(start + v * std::f32::consts::TAU),
                });
            });
            frame.stroke(
                &arc,
                Stroke {
                    line_cap: LineCap::Round,
                    ..stroke(self.color)
                },
            );
        }
        vec![frame.into_geometry()]
    }
}

/// A `size` px ring showing `value` (0 to 1).
pub fn ring<'a, M: 'a>(
    value: f32,
    size: f32,
    width: f32,
    color: Color,
    track: Color,
) -> Element<'a, M> {
    iced::widget::canvas(Ring {
        value,
        color,
        track,
        width,
    })
    .width(Length::Fixed(size))
    .height(Length::Fixed(size))
    .into()
}
