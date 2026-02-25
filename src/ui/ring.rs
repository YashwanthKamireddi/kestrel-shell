//! A ring gauge: a quiet track and an arc for the value, with round ends. Drawn by the canvas
//! (cached by iced while the value is unchanged), so it is cheap.

use iced::widget::canvas::{self, Frame, Geometry, LineCap, Path, Stroke};
use iced::{Color, Element, Length, Point, Rectangle, Renderer, Theme, mouse};

struct Ring {
    value: f32,
    color: Color,
    track: Color,
    width: f32,
    /// Painted over the whole disc first, so a change of colour or level always covers the
    /// previous drawing (a partial repaint left an old arc showing in two colours).
    fill: Option<Color>,
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
        if let Some(fill) = self.fill {
            frame.fill(
                &Path::circle(c, bounds.width.min(bounds.height) / 2.0),
                fill,
            );
        }
        let r = bounds.width.min(bounds.height) / 2.0 - self.width / 2.0 - 0.5;
        let stroke = |color: Color| Stroke::default().with_width(self.width).with_color(color);
        frame.stroke(&Path::circle(c, r), stroke(self.track));
        let v = self.value.clamp(0.0, 1.0);
        if v > 0.002 {
            // Many short straight steps instead of one arc: a nearly full arc was drawn with
            // a visible glitch where it met its own start.
            let start = -std::f32::consts::FRAC_PI_2;
            let sweep = v.min(0.995) * std::f32::consts::TAU;
            let steps = ((sweep / std::f32::consts::TAU) * 120.0).ceil().max(2.0) as usize;
            let arc = Path::new(|b| {
                for i in 0..=steps {
                    let a = start + sweep * i as f32 / steps as f32;
                    let p = Point::new(c.x + r * a.cos(), c.y + r * a.sin());
                    if i == 0 {
                        b.move_to(p);
                    } else {
                        b.line_to(p);
                    }
                }
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
    super::snap::snap(
        iced::widget::canvas(Ring {
            value,
            color,
            track,
            width,
            fill: None,
        })
        .width(Length::Fixed(size))
        .height(Length::Fixed(size)),
    )
    .into()
}

/// `ring` on a disc of `fill` (the colour behind it), repainted whole on every change.
pub fn ring_on<'a, M: 'a>(
    value: f32,
    size: f32,
    width: f32,
    color: Color,
    track: Color,
    fill: Color,
) -> Element<'a, M> {
    super::snap::snap(
        iced::widget::canvas(Ring {
            value,
            color,
            track,
            width,
            fill: Some(fill),
        })
        .width(Length::Fixed(size))
        .height(Length::Fixed(size)),
    )
    .into()
}
