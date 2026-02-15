//! `Snap`: draws its content moved onto whole screen pixels. A layout centres things at
//! fractional positions (half a pixel off when widths are odd), and a picture drawn between
//! pixels is resampled: soft, and its rounded edges jagged. Snapping only shifts the drawing by
//! under a pixel; layout and input are unchanged.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{Tree, Widget};
use iced::advanced::{Clipboard, Shell, mouse};
use iced::event::Event;
use iced::{Element, Length, Rectangle, Size, Vector};

pub struct Snap<'a, M> {
    content: Element<'a, M>,
}

pub fn snap<'a, M>(content: impl Into<Element<'a, M>>) -> Snap<'a, M> {
    Snap {
        content: content.into(),
    }
}

impl<M> Widget<M, iced::Theme, iced::Renderer> for Snap<'_, M> {
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
        tree: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &iced::Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
    }
    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        use iced::advanced::Renderer as _;
        let b = layout.bounds();
        let shift = Vector::new(b.x.round() - b.x, b.y.round() - b.y);
        if shift.x == 0.0 && shift.y == 0.0 {
            self.content.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                layout,
                cursor,
                viewport,
            );
            return;
        }
        renderer.with_translation(shift, |renderer| {
            self.content.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                layout,
                cursor,
                viewport,
            );
        });
    }
    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &iced::Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }
}

impl<'a, M: 'a> From<Snap<'a, M>> for Element<'a, M> {
    fn from(s: Snap<'a, M>) -> Self {
        Element::new(s)
    }
}
