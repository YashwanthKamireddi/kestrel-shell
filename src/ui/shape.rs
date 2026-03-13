//! The kestrel surface: the bar, the screen frame and the panels drawn as one material.
//! A panel leaves the bar through two concave curves, so it reads as the bar extruding
//! rather than a card placed under it.

use iced::widget::canvas::{self, Geometry, Path, path::Builder};
use iced::{Color, Point, Rectangle, Renderer, Theme, mouse};

/// Circle approximation constant for a quarter arc drawn with one cubic.
const K: f32 = 0.552_284_8;

/// A panel hanging from the bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drop {
    pub x: f32,
    pub width: f32,
    /// Current height below the bar's bottom edge.
    pub height: f32,
    pub radius: f32,
    pub join: f32,
    /// The panel's right side merges into the screen frame instead of standing free.
    pub flush_right: bool,
    pub flush_left: bool,
    /// Pill style: a free-standing card (all corners round), not joined to the bar.
    pub floating: bool,
    /// Top edge of a floating card.
    pub top: f32,
}

/// Gap between the pills and a floating panel.
pub const FLOAT_GAP: f32 = 6.0;

/// The bar surface: the bar band, the screen's two top inner corners and any panels.
#[derive(Clone, Debug, PartialEq)]
pub struct BarShape {
    pub color: Color,
    pub bar: f32,
    pub frame: f32,
    pub corner: f32,
    /// Draw the bar band (full style). The pill style draws pills as widgets instead.
    pub band: bool,
    pub drops: Vec<Drop>,
}

impl<M> canvas::Program<M> for BarShape {
    type State = crate::ui::Memo<Self>;

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        vec![state.draw(self, renderer, bounds.size(), |frame| {
            let w = bounds.width;
            if self.band {
                frame.fill_rectangle(Point::ORIGIN, iced::Size::new(w, self.bar), self.color);
                // Separate fills: the mirrored right corner winds the other way, so in one path
                // its half-pixel overlap with the band cancels out (non-zero rule) and leaves a
                // see-through line under the bar.
                for right in [false, true] {
                    frame.fill(
                        &Path::new(|b| top_corner(b, self.frame, self.bar, self.corner, right, w)),
                        self.color,
                    );
                }
            }
            // Each panel is its own fill, so only the area it covers is redrawn while it moves.
            for d in &self.drops {
                if d.height < 0.5 {
                    continue;
                }
                // The renderer redraws only the bounds of what changed, and a path's bounds leave
                // out its anti-aliased edge; a near-invisible box 2 px larger makes sure the edge
                // pixels of the previous frame are repainted too.
                let pad = 2.0;
                frame.fill_rectangle(
                    Point::new(
                        d.x - d.join - pad,
                        if d.floating { d.top } else { self.bar } - pad,
                    ),
                    iced::Size::new(
                        d.width + 2.0 * (d.join + pad),
                        d.height + d.join + 2.0 * pad,
                    ),
                    Color::from_rgba8(0, 0, 0, 1.0 / 255.0),
                );
                if d.floating {
                    let r = d.radius.min(d.height / 2.0).min(d.width / 2.0);
                    frame.fill(
                        &Path::rounded_rectangle(
                            Point::new(d.x, d.top),
                            iced::Size::new(d.width, d.height),
                            r.into(),
                        ),
                        self.color,
                    );
                } else {
                    frame.fill(&Path::new(|b| drop_path(b, self.bar, d)), self.color);
                }
            }
        })]
    }
}

/// The screen's inner corner below the bar, on the left (or mirrored to the right).
fn top_corner(b: &mut Builder, t: f32, y: f32, r: f32, right: bool, w: f32) {
    if r <= 0.0 {
        return;
    }
    let fx = |x: f32| if right { w - x } else { x };
    b.move_to(Point::new(fx(t), y - 0.5));
    b.line_to(Point::new(fx(t + r), y - 0.5));
    b.line_to(Point::new(fx(t + r), y));
    b.bezier_curve_to(
        Point::new(fx(t + r - r * K), y),
        Point::new(fx(t), y + r - r * K),
        Point::new(fx(t), y + r),
    );
    b.close();
}

fn drop_path(b: &mut Builder, top: f32, d: &Drop) {
    if d.height < 0.5 {
        return;
    }
    // While the panel is short, shrink the curves with it so it grows smoothly from nothing.
    let j = d.join.min(d.height * 0.5);
    let r = d.radius.min((d.height - j).max(0.0)).min(d.width / 2.0);
    let (x0, x1, bottom) = (d.x, d.x + d.width, top + d.height);
    let y = top - 0.5;
    // Left side: either a free edge leaving the bar, or flush with the frame.
    if d.flush_left {
        b.move_to(Point::new(x0, y));
        b.line_to(Point::new(x0, bottom + j));
        b.bezier_curve_to(
            Point::new(x0, bottom + j - j * K),
            Point::new(x0 + j - j * K, bottom),
            Point::new(x0 + j, bottom),
        );
    } else {
        b.move_to(Point::new(x0 - j, y));
        b.line_to(Point::new(x0 - j, top));
        b.bezier_curve_to(
            Point::new(x0 - j + j * K, top),
            Point::new(x0, top + j - j * K),
            Point::new(x0, top + j),
        );
        b.line_to(Point::new(x0, bottom - r));
        b.bezier_curve_to(
            Point::new(x0, bottom - r + r * K),
            Point::new(x0 + r - r * K, bottom),
            Point::new(x0 + r, bottom),
        );
    }
    if d.flush_right {
        b.line_to(Point::new(x1 - j, bottom));
        b.bezier_curve_to(
            Point::new(x1 - j + j * K, bottom),
            Point::new(x1, bottom + j - j * K),
            Point::new(x1, bottom + j),
        );
        b.line_to(Point::new(x1, y));
    } else {
        b.line_to(Point::new(x1 - r, bottom));
        b.bezier_curve_to(
            Point::new(x1 - r + r * K, bottom),
            Point::new(x1, bottom - r + r * K),
            Point::new(x1, bottom - r),
        );
        b.line_to(Point::new(x1, top + j));
        b.bezier_curve_to(
            Point::new(x1, top + j - j * K),
            Point::new(x1 + j - j * K, top),
            Point::new(x1 + j, top),
        );
        b.line_to(Point::new(x1 + j, y));
    }
    b.close();
}

/// Which piece of the frame a strip surface draws.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    /// The bottom band plus both bottom inner corners.
    Bottom,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FrameShape {
    pub color: Color,
    pub edge: Edge,
    pub frame: f32,
    pub corner: f32,
}

impl<M> canvas::Program<M> for FrameShape {
    type State = crate::ui::Memo<Self>;

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        vec![state.draw(self, renderer, bounds.size(), |frame| {
            let (w, h, t, r) = (bounds.width, bounds.height, self.frame, self.corner);
            match self.edge {
                Edge::Left | Edge::Right => {
                    frame.fill_rectangle(Point::ORIGIN, bounds.size(), self.color);
                }
                Edge::Bottom => {
                    frame.fill_rectangle(Point::new(0.0, h - t), iced::Size::new(w, t), self.color);
                    // One fill per corner: mirrored corners wind the other way and would cancel
                    // their overlap with the band (see `BarShape`).
                    if r > 0.0 {
                        for right in [false, true] {
                            let path = Path::new(|b| {
                                let fx = |x: f32| if right { w - x } else { x };
                                let base = h - t;
                                b.move_to(Point::new(fx(t - 0.5), base + 0.5));
                                b.line_to(Point::new(fx(t - 0.5), base - r));
                                b.line_to(Point::new(fx(t), base - r));
                                b.bezier_curve_to(
                                    Point::new(fx(t), base - r + r * K),
                                    Point::new(fx(t + r - r * K), base),
                                    Point::new(fx(t + r), base),
                                );
                                b.line_to(Point::new(fx(t + r), base + 0.5));
                                b.close();
                            });
                            frame.fill(&path, self.color);
                        }
                    }
                }
            }
        })]
    }
}
