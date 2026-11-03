use crate::core::{Color, Rectangle, Size};
use crate::graphics::compositor::{self, Information};
use crate::graphics::damage;
use crate::graphics::error::{self, Error};
use crate::graphics::{self, Shell, Viewport};
use crate::{Layer, Renderer, Settings};

use std::collections::VecDeque;
use std::num::NonZeroU32;

pub struct Compositor {
    context: softbuffer::Context<Box<dyn compositor::Display>>,
    settings: Settings,
}

pub struct Surface {
    window: softbuffer::Surface<
        Box<dyn compositor::Display>,
        Box<dyn compositor::Window>,
    >,
    clip_mask: tiny_skia::Mask,
    layer_stack: VecDeque<Vec<Layer>>,
    background_color: Color,
    max_age: u8,
}

impl crate::graphics::Compositor for Compositor {
    type Renderer = Renderer;
    type Surface = Surface;

    async fn with_backend(
        settings: graphics::Settings,
        display: impl compositor::Display,
        _compatible_window: impl compositor::Window,
        _shell: Shell,
        backend: Option<&str>,
    ) -> Result<Self, Error> {
        match backend {
            None | Some("tiny-skia") | Some("tiny_skia") => {
                Ok(new(settings.into(), display))
            }
            Some(backend) => Err(Error::GraphicsAdapterNotFound {
                backend: "tiny-skia",
                reason: error::Reason::DidNotMatch {
                    preferred_backend: backend.to_owned(),
                },
            }),
        }
    }

    fn create_renderer(&self) -> Self::Renderer {
        Renderer::new(
            self.settings.default_font,
            self.settings.default_text_size,
        )
    }

    fn create_surface<W: compositor::Window + Clone>(
        &mut self,
        window: W,
        width: u32,
        height: u32,
    ) -> Self::Surface {
        let window = softbuffer::Surface::new(
            &self.context,
            Box::new(window.clone()) as _,
        )
        .expect("Create softbuffer surface for window");

        let mut surface = Surface {
            window,
            clip_mask: tiny_skia::Mask::new(1, 1).expect("Create clip mask"),
            layer_stack: VecDeque::new(),
            background_color: Color::BLACK,
            max_age: 0,
        };

        if width > 0 && height > 0 {
            self.configure_surface(&mut surface, width, height);
        }

        surface
    }

    fn configure_surface(
        &mut self,
        surface: &mut Self::Surface,
        width: u32,
        height: u32,
    ) {
        surface
            .window
            .resize(
                NonZeroU32::new(width).expect("Non-zero width"),
                NonZeroU32::new(height).expect("Non-zero height"),
            )
            .expect("Resize surface");

        surface.clip_mask =
            tiny_skia::Mask::new(width, height).expect("Create clip mask");
        surface.layer_stack.clear();
    }

    fn information(&self) -> Information {
        Information {
            adapter: String::from("CPU"),
            backend: String::from("tiny-skia"),
        }
    }

    fn present(
        &mut self,
        renderer: &mut Self::Renderer,
        surface: &mut Self::Surface,
        viewport: &Viewport,
        background_color: Color,
        on_pre_present: impl FnOnce(),
    ) -> Result<(), compositor::SurfaceError> {
        present(
            renderer,
            surface,
            viewport,
            background_color,
            on_pre_present,
        )
    }

    fn screenshot(
        &mut self,
        renderer: &mut Self::Renderer,
        viewport: &Viewport,
        background_color: Color,
    ) -> Vec<u8> {
        screenshot(renderer, viewport, background_color)
    }
}

pub fn new(
    settings: Settings,
    display: impl compositor::Display,
) -> Compositor {
    #[allow(unsafe_code)]
    let context = softbuffer::Context::new(Box::new(display) as _)
        .expect("Create softbuffer context");

    Compositor { context, settings }
}

pub fn present(
    renderer: &mut Renderer,
    surface: &mut Surface,
    viewport: &Viewport,
    background_color: Color,
    on_pre_present: impl FnOnce(),
) -> Result<(), compositor::SurfaceError> {
    let physical_size = viewport.physical_size();

    let mut buffer = surface
        .window
        .buffer_mut()
        .map_err(|_| compositor::SurfaceError::Lost)?;

    let buffer_age = buffer.age();
    let last_layers = {
        let age = buffer_age;

        surface.max_age = surface.max_age.max(age);
        surface.layer_stack.truncate(surface.max_age as usize);

        if age > 0 {
            surface.layer_stack.get(age as usize - 1)
        } else {
            None
        }
    };

    let damage = last_layers
        .and_then(|last_layers| {
            (surface.background_color == background_color).then(|| {
                damage::diff(
                    last_layers,
                    renderer.layers(),
                    |layer| vec![layer.bounds],
                    Layer::damage,
                )
            })
        })
        .unwrap_or_else(|| vec![Rectangle::with_size(viewport.logical_size())]);

    // kestrel patch: what changed since the frame on screen (the previous one), told to the
    // compositor with the buffer. `damage` above is measured against this buffer's own older
    // content (it may be 2-3 frames old); the compositor needs the change from what it shows.
    // It was told the whole surface changed on every frame, so it took and composited all of
    // a full-screen overlay (8.3 MB at 1080p) for a panel's few hundred pixels.
    let shown = surface
        .layer_stack
        .front()
        .filter(|_| surface.background_color == background_color)
        .map(|previous| {
            damage::diff(
                previous,
                renderer.layers(),
                |layer| vec![layer.bounds],
                Layer::damage,
            )
        })
        .unwrap_or_else(|| vec![Rectangle::with_size(viewport.logical_size())]);
    let shown = physical(&grown(shown, viewport), viewport);

    if damage.is_empty() {
        if let Some(last_layers) = last_layers {
            surface.layer_stack.push_front(last_layers.clone());
        }
    } else {
        surface.layer_stack.push_front(renderer.layers().to_vec());
        surface.background_color = background_color;

        // kestrel patch: each damaged area grown by a pixel on every side (kept on the surface).
        // A shape at a fractional position anti-aliases one pixel past its bounds; repainting
        // only the bounds left that fringe behind when the shape went (thin lines above and
        // below a pressed or hovered chip).
        let screen = Rectangle::with_size(viewport.logical_size());
        let damage = damage::group(grown(damage, viewport), screen);
        // kestrel patch: many overlapping rectangles (a scrolled list moves every item inside
        // it: 27 rectangles, 640 000 px for a 398 000 px panel) are painted once as their
        // bounding box when they cover most of it, instead of painting the overlaps again and
        // again (a scroll frame went from ~10 ms to about half).
        let damage = {
            let total: f32 = damage.iter().map(|r| r.width * r.height).sum();
            match damage.iter().copied().reduce(|a, b| a.union(&b)) {
                Some(bbox) if damage.len() > 1 && total > 0.6 * bbox.width * bbox.height => {
                    vec![bbox]
                }
                _ => damage,
            }
        };
        // kestrel patch: what is redrawn, for KESTREL_TRACE (rectangles and their area).
        if std::env::var_os("KESTREL_TRACE").is_some() {
            let area: f32 = damage.iter().map(|r| r.width * r.height).sum();
            eprintln!(
                "TRACE damage age {} {} rects {:.0} px2 {:?}",
                buffer_age,
                damage.len(),
                area,
                damage
                    .iter()
                    .map(|r| (r.x as i32, r.y as i32, r.width as i32, r.height as i32))
                    .collect::<Vec<_>>()
            );
        }

        let mut pixels = tiny_skia::PixmapMut::from_bytes(
            bytemuck::cast_slice_mut(&mut buffer),
            physical_size.width,
            physical_size.height,
        )
        .expect("Create pixel map");

        renderer.draw(
            &mut pixels,
            &mut surface.clip_mask,
            viewport,
            &damage,
            background_color,
        );
    }

    on_pre_present();
    buffer
        .present_with_damage(&shown)
        .map_err(|_| compositor::SurfaceError::Lost)
}

/// kestrel patch: each damaged area grown by a pixel on every side and kept on the surface. A
/// shape at a fractional position anti-aliases one pixel past its bounds; repainting only the
/// bounds left that fringe behind when the shape went.
fn grown(damage: Vec<Rectangle>, viewport: &Viewport) -> Vec<Rectangle> {
    let screen = Rectangle::with_size(viewport.logical_size());
    let px = 1.0 / viewport.scale_factor();
    damage
        .into_iter()
        .filter_map(|r| {
            Rectangle {
                x: r.x - px,
                y: r.y - px,
                width: r.width + 2.0 * px,
                height: r.height + 2.0 * px,
            }
            .intersection(&screen)
        })
        .collect()
}

/// kestrel patch: logical rectangles as the buffer's pixels (outward to whole pixels, inside
/// the buffer), for `wl_surface.damage_buffer`.
fn physical(damage: &[Rectangle], viewport: &Viewport) -> Vec<softbuffer::Rect> {
    let scale = viewport.scale_factor();
    let size = viewport.physical_size();
    damage
        .iter()
        .filter_map(|r| {
            let x0 = ((r.x * scale).floor().max(0.0) as u32).min(size.width);
            let y0 = ((r.y * scale).floor().max(0.0) as u32).min(size.height);
            let x1 = (((r.x + r.width) * scale).ceil().max(0.0) as u32).min(size.width);
            let y1 = (((r.y + r.height) * scale).ceil().max(0.0) as u32).min(size.height);
            Some(softbuffer::Rect {
                x: x0,
                y: y0,
                width: std::num::NonZeroU32::new(x1.saturating_sub(x0))?,
                height: std::num::NonZeroU32::new(y1.saturating_sub(y0))?,
            })
        })
        .collect()
}

pub fn screenshot(
    renderer: &mut Renderer,
    viewport: &Viewport,
    background_color: Color,
) -> Vec<u8> {
    let size = viewport.physical_size();

    let mut offscreen_buffer: Vec<u32> =
        vec![0; size.width as usize * size.height as usize];

    let mut clip_mask = tiny_skia::Mask::new(size.width, size.height)
        .expect("Create clip mask");

    renderer.draw(
        &mut tiny_skia::PixmapMut::from_bytes(
            bytemuck::cast_slice_mut(&mut offscreen_buffer),
            size.width,
            size.height,
        )
        .expect("Create offscreen pixel map"),
        &mut clip_mask,
        viewport,
        &[Rectangle::with_size(Size::new(
            size.width as f32,
            size.height as f32,
        ))],
        background_color,
    );

    offscreen_buffer.iter().fold(
        Vec::with_capacity(offscreen_buffer.len() * 4),
        |mut acc, pixel| {
            const A_MASK: u32 = 0xFF_00_00_00;
            const R_MASK: u32 = 0x00_FF_00_00;
            const G_MASK: u32 = 0x00_00_FF_00;
            const B_MASK: u32 = 0x00_00_00_FF;

            let a = ((A_MASK & pixel) >> 24) as u8;
            let r = ((R_MASK & pixel) >> 16) as u8;
            let g = ((G_MASK & pixel) >> 8) as u8;
            let b = (B_MASK & pixel) as u8;

            acc.extend([r, g, b, a]);
            acc
        },
    )
}
