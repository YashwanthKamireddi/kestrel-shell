//! `Breathe`: an icon that pulses slowly (charging, connecting) by redrawing itself about 20
//! times a second. The pulse lives in the widget: no app message, no subscription, no view
//! rebuild per step, so only its own surface is drawn again (an 18 px damage, ~0.1 ms). As an
//! app-wide 20 Hz message it cost ~1.8 % of a core while charging; now ~0.9 % (2026-11-15).

use crate::ui::Icon;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{Tree, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::event::Event;
use iced::{Color, Element, Length, Rectangle, Size, mouse, window};
use std::time::{Duration, Instant};

/// About 20 steps a second: smooth for a 1.6 s breath, a third of a 60 Hz redraw.
const STEP: Duration = Duration::from_millis(50);

pub struct Breathe<'a, M> {
    content: Element<'a, M>,
    color: Color,
}

/// `i` at `size` in `color`, its strength pulsing between 55 % and 100 % (about 1.6 s a breath).
pub fn breathe<'a, M: 'a>(i: Icon, size: f32, color: Color) -> Breathe<'a, M> {
    // No colour of its own: it takes the one `draw` passes in the style.
    let t = iced::widget::text(i.glyph().to_string())
        .font(crate::ui::icon::FONT)
        .size(size)
        .line_height(iced::widget::text::LineHeight::Relative(1.0))
        .width(size)
        .height(size)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center);
    Breathe {
        content: t.into(),
        color,
    }
}

fn strength() -> f32 {
    static EPOCH: std::sync::LazyLock<Instant> = std::sync::LazyLock::new(Instant::now);
    let t = EPOCH.elapsed().as_secs_f32();
    0.775 + 0.225 * (t * std::f32::consts::TAU / 1.6).sin()
}

impl<M> Widget<M, iced::Theme, iced::Renderer> for Breathe<'_, M> {
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }
    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, limits)
    }
    fn update(
        &mut self,
        _tree: &mut Tree,
        event: &Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) {
        // Every frame asks for the next one a step later (game mode: still).
        if let Event::Window(window::Event::RedrawRequested(now)) = event
            && !crate::motion::still()
        {
            shell.request_redraw_at(*now + STEP);
        }
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let a = if crate::motion::still() {
            1.0
        } else {
            strength()
        };
        let style = renderer::Style {
            text_color: self.color.scale_alpha(a),
        };
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            &style,
            layout,
            cursor,
            viewport,
        );
    }
}

impl<'a, M: 'a> From<Breathe<'a, M>> for Element<'a, M> {
    fn from(b: Breathe<'a, M>) -> Self {
        Element::new(b)
    }
}
