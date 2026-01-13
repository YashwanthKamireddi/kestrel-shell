//! `Morph`: a box that animates its width to fit its content. The content is laid out at its
//! natural width; the box shows `width` of it (centred) and reports the natural width, so the
//! owner can spring `width` towards it. This is what makes the island grow and shrink smoothly
//! when what it shows changes.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, mouse};
use iced::event::Event;
use iced::{Element, Length, Rectangle, Size};

pub struct Morph<'a, M> {
    content: Element<'a, M>,
    width: f32,
    height: f32,
    on_measure: Option<Box<dyn Fn(f32) -> M + 'a>>,
}

pub fn morph<'a, M>(content: impl Into<Element<'a, M>>, width: f32, height: f32) -> Morph<'a, M> {
    Morph {
        content: content.into(),
        width: width.max(0.0),
        height,
        on_measure: None,
    }
}

impl<'a, M> Morph<'a, M> {
    /// Called with the content's natural width whenever it changes.
    pub fn on_measure(mut self, f: impl Fn(f32) -> M + 'a) -> Self {
        self.on_measure = Some(Box::new(f));
        self
    }
}

#[derive(Default)]
struct State {
    reported: Option<f32>,
}

impl<M> Widget<M, iced::Theme, iced::Renderer> for Morph<'_, M> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }
    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.width), Length::Fixed(self.height))
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        _limits: &layout::Limits,
    ) -> layout::Node {
        let limits = layout::Limits::new(
            Size::new(0.0, self.height),
            Size::new(f32::INFINITY, self.height),
        );
        let child = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, &limits);
        // Centred: while the box is narrower than the content, both sides are cut evenly.
        let x = (self.width - child.size().width) / 2.0;
        let y = (self.height - child.size().height) / 2.0;
        layout::Node::with_children(
            Size::new(self.width, self.height),
            vec![child.move_to((x, y))],
        )
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
        let child = layout.children().next().expect("morph has one child");
        let natural = child.bounds().width;
        let state = tree.state.downcast_mut::<State>();
        if state.reported.is_none_or(|w| (w - natural).abs() > 0.5) {
            state.reported = Some(natural);
            if let Some(f) = &self.on_measure {
                shell.publish(f(natural));
            }
        }
        let cursor = if cursor.is_over(layout.bounds()) {
            cursor
        } else {
            mouse::Cursor::Unavailable
        };
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            child,
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
        let Some(clip) = layout.bounds().intersection(viewport) else {
            return;
        };
        renderer.with_layer(clip, |renderer| {
            self.content.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                layout.children().next().expect("morph has one child"),
                cursor,
                &clip,
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
        if !cursor.is_over(layout.bounds()) {
            return mouse::Interaction::default();
        }
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout.children().next().expect("morph has one child"),
            cursor,
            viewport,
            renderer,
        )
    }
}

impl<'a, M: 'a> From<Morph<'a, M>> for Element<'a, M> {
    fn from(m: Morph<'a, M>) -> Self {
        Element::new(m)
    }
}
