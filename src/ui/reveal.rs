//! `Reveal`: shows the top `visible` pixels of its content while the content keeps its natural
//! size, so a panel can grow over its contents without squashing them. It reports the
//! content's natural size, which drives how far the panel opens.

use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{Tree, Widget, tree};
use iced::advanced::{Clipboard, Shell, mouse};
use iced::event::Event;
use iced::{Element, Length, Rectangle, Size};

pub struct Reveal<'a, M> {
    content: Element<'a, M>,
    width: f32,
    visible: f32,
    offset_y: f32,
    on_measure: Option<Box<dyn Fn(Size) -> M + 'a>>,
}

/// `width` is fixed; the content is laid out with unbounded height.
pub fn reveal<'a, M>(
    content: impl Into<Element<'a, M>>,
    width: f32,
    visible: f32,
) -> Reveal<'a, M> {
    Reveal {
        content: content.into(),
        width,
        visible: visible.max(0.0),
        offset_y: 0.0,
        on_measure: None,
    }
}

impl<'a, M> Reveal<'a, M> {
    /// Shifts the content down (used for the rise-in as a panel opens).
    pub fn offset(mut self, y: f32) -> Self {
        self.offset_y = y;
        self
    }
    pub fn on_measure(mut self, f: impl Fn(Size) -> M + 'a) -> Self {
        self.on_measure = Some(Box::new(f));
        self
    }
}

#[derive(Default)]
struct State {
    reported: Option<Size>,
}

impl<M> Widget<M, iced::Theme, iced::Renderer> for Reveal<'_, M> {
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
        Size::new(Length::Fixed(self.width), Length::Fixed(self.visible))
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &iced::Renderer,
        _limits: &layout::Limits,
    ) -> layout::Node {
        let limits = layout::Limits::new(
            Size::new(self.width, 0.0),
            Size::new(self.width, f32::INFINITY),
        );
        let child = self
            .content
            .as_widget_mut()
            .layout(&mut tree.children[0], renderer, &limits)
            .move_to((0.0, self.offset_y));
        layout::Node::with_children(Size::new(self.width, self.visible), vec![child])
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
        let bounds = layout.bounds();
        let child = layout.children().next().unwrap();
        let natural = child.bounds().size();
        let state = tree.state.downcast_mut::<State>();
        if state.reported != Some(natural) {
            state.reported = Some(natural);
            if let Some(f) = &self.on_measure {
                shell.publish(f(natural));
            }
        }
        // Only the visible part takes input.
        let cursor = if cursor.is_over(bounds) {
            cursor
        } else {
            mouse::Cursor::Unavailable
        };
        let Some(clip) = bounds.intersection(viewport) else {
            return;
        };
        self.content.as_widget_mut().update(
            &mut tree.children[0],
            event,
            child,
            cursor,
            renderer,
            clipboard,
            shell,
            &clip,
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
        let bounds = layout.bounds();
        if self.visible < 0.5 {
            return;
        }
        let Some(clip) = bounds.intersection(viewport) else {
            return;
        };
        renderer.with_layer(clip, |renderer| {
            self.content.as_widget().draw(
                &tree.children[0],
                renderer,
                theme,
                style,
                layout.children().next().unwrap(),
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
        // A panel is solid: the pointer over it never reaches what lies beneath.
        match self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout.children().next().unwrap(),
            cursor,
            viewport,
            renderer,
        ) {
            mouse::Interaction::None => mouse::Interaction::Idle,
            i => i,
        }
    }

    fn operate(
        &mut self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &iced::Renderer,
        operation: &mut dyn iced::advanced::widget::Operation,
    ) {
        self.content.as_widget_mut().operate(
            &mut tree.children[0],
            layout.children().next().unwrap(),
            renderer,
            operation,
        );
    }
}

impl<'a, M: 'a> From<Reveal<'a, M>> for Element<'a, M> {
    fn from(r: Reveal<'a, M>) -> Self {
        Element::new(r)
    }
}
