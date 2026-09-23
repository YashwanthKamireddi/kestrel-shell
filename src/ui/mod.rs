//! kestrel's component kit. Screens are assembled only from these.

pub mod breathe;
pub mod control;
pub mod focus;
pub mod icon;
pub mod morph;
mod pictures;
pub mod reveal;
pub mod ring;
pub mod shape;
pub mod slider;
pub mod smooth;
pub mod snap;
pub mod swipe;
mod weather;

use crate::theme::{self, Scheme, tok};
use iced::widget::{Space, Text, container, row, text};
use iced::{Alignment, Color, Element, Length, font};

pub use control::control;
pub use icon::Icon;
pub use pictures::*;
pub use reveal::reveal;
pub use smooth::smooth;
pub use swipe::swipe;
pub use weather::*;

/// Text in the kestrel type scale.
pub fn label<'a>(s: impl text::IntoFragment<'a>, size: f32, color: Color) -> Text<'a> {
    text(s)
        .size(size)
        .color(color)
        .font(theme::FONT)
        .wrapping(text::Wrapping::None)
}

pub fn label_w<'a>(
    s: impl text::IntoFragment<'a>,
    size: f32,
    color: Color,
    weight: font::Weight,
) -> Text<'a> {
    label(s, size, color).font(theme::font(weight))
}

/// Shortens `s` to fit `width` px at `size` px text (Rubik averages about 0.56 em per character),
/// ending with an ellipsis.
pub fn fit(s: &str, size: f32, width: f32) -> String {
    let chars = (width / (size * 0.56)).floor().max(4.0) as usize;
    ellipsize(s, chars)
}

/// `s` on up to two lines of `width` px at `size` px text, broken between words; "…" only
/// if even two lines are not enough. For titles, which should be read, not cut.
pub fn two_lines(s: &str, size: f32, width: f32) -> String {
    let per = (width / (size * 0.56)).floor().max(4.0) as usize;
    let words: Vec<&str> = s.split_whitespace().collect();
    let mut first = String::new();
    let mut i = 0;
    while i < words.len() {
        let next = if first.is_empty() {
            words[i].chars().count()
        } else {
            first.chars().count() + 1 + words[i].chars().count()
        };
        if next > per && !first.is_empty() {
            break;
        }
        if !first.is_empty() {
            first.push(' ');
        }
        first.push_str(words[i]);
        i += 1;
    }
    if i >= words.len() {
        return fit(&first, size, width);
    }
    let rest = words[i..].join(" ");
    format!("{}\n{}", fit(&first, size, width), fit(&rest, size, width))
}

/// Shortens `s` to at most `max` characters, ending with an ellipsis.
pub fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out = out.trim_end().to_string();
        out.push('…');
        out
    }
}

/// An icon in `color` (its alpha included, so it fades with a panel), `size` px square.
pub fn icon(i: Icon, size: f32, color: Color) -> iced::widget::Text<'static> {
    let t = iced::widget::text(i.glyph().to_string())
        .font(icon::FONT)
        .size(size)
        .color(color)
        .line_height(iced::widget::text::LineHeight::Relative(1.0))
        .width(size);
    t.height(size)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center)
}

/// The box around a search field (44 px high), the same in every panel: a light translucent
/// A text field's own style inside its capsule (`field_style`): no fill or border of its own,
/// the ink, its placeholder faint, the selection in the soft accent.
pub fn input_style(
    s: Scheme,
) -> impl Fn(&iced::Theme, iced::widget::text_input::Status) -> iced::widget::text_input::Style {
    move |_, _| iced::widget::text_input::Style {
        background: iced::Color::TRANSPARENT.into(),
        border: iced::Border::default(),
        icon: s.ink_dim,
        placeholder: s.ink_faint,
        value: s.ink,
        selection: s.accent_soft,
    }
}

/// fill that sits as well on the black look as on the frost.
/// The one search field (launcher, clipboard, emoji): a search icon and the text, 44 px.
pub fn search_field<'a, M: Clone + 'a>(
    s: Scheme,
    hint: &'a str,
    value: &'a str,
    id: iced::widget::Id,
    on_input: impl Fn(String) -> M + 'a,
    on_submit: Option<M>,
) -> Element<'a, M> {
    let mut input = iced::widget::text_input(hint, value)
        .id(id)
        .on_input(on_input)
        .padding([crate::theme::tok::L, crate::theme::tok::L])
        .size(theme::text::TITLE)
        .font(theme::FONT)
        .style(input_style(s));
    if let Some(m) = on_submit {
        input = input.on_submit(m);
    }
    container(
        row![
            icon(Icon::Search, crate::theme::tok::ICON_M, s.ink_dim),
            input
        ]
        .spacing(crate::theme::tok::XS)
        .align_y(Alignment::Center),
    )
    .padding([crate::theme::tok::NONE, crate::theme::tok::L])
    .height(44)
    .center_y(44)
    .style(move |_| field_style(s))
    .into()
}

pub fn field_style(s: Scheme) -> iced::widget::container::Style {
    iced::widget::container::Style {
        background: Some(s.ink.scale_alpha(0.07 * s.opacity).into()),
        border: iced::Border {
            color: s.ink.scale_alpha(0.06 * s.opacity),
            width: 1.0,
            // A capsule, like the bar's pills: a rounded box inside the panel's rounded box
            // never matched its corners (design call 2026-11-13).
            radius: 22.0.into(),
        },
        ..iced::widget::container::Style::default()
    }
}

/// A round icon button for the bar and panel headers. `active` marks an open panel or an
/// enabled state.
pub fn icon_button<'a, M: Clone + 'static>(
    s: Scheme,
    i: Icon,
    on_press: M,
    active: bool,
) -> control::Control<'a, M> {
    icon_button_in(s, i, on_press, active, s.ink)
}

/// `icon_button` with the icon in `tint` (a colour that says something about the state).
pub fn icon_button_in<'a, M: Clone + 'static>(
    s: Scheme,
    i: Icon,
    on_press: M,
    active: bool,
    tint: Color,
) -> control::Control<'a, M> {
    let rest = if active {
        s.selected()
    } else {
        Color::TRANSPARENT
    };
    control(container(icon(i, tok::ICON, tint)).center(Length::Fixed(tok::BAR_ITEM)))
        .radius(tok::BAR_ITEM / 2.0)
        .colors(rest, if active { rest } else { s.hover() }, s.raised_press)
        .on_press(on_press)
}

/// The card that groups related controls inside a panel.
/// A group of content in a panel. Flat: no grey box, no outline; groups are told apart by
/// space alone (design call, 2026-11-12).
pub fn card<'a, M: 'a>(_s: Scheme, content: impl Into<Element<'a, M>>) -> Element<'a, M> {
    container(content)
        .padding(tok::PANEL_PAD)
        .width(Length::Fill)
        .into()
}

/// A 1 px hairline.
pub fn divider<'a, M: 'a>(s: Scheme) -> Element<'a, M> {
    container(Space::new())
        .width(Length::Fill)
        .height(1)
        .style(move |_| container::Style {
            background: Some(s.line.into()),
            ..container::Style::default()
        })
        .into()
}

/// A quick-settings tile: icon, title and status. Accent-filled when on. An optional chevron
/// opens the tile's details.
/// Width of a quick-settings tile (two per row in the 368 px control centre).
pub const TILE_W: f32 = 180.0;

/// A quick-settings tile.
pub struct Tile<M> {
    pub icon: Icon,
    pub title: String,
    pub status: String,
    pub on: bool,
    /// Turns it on or off (its icon circle; the whole tile when it has no page).
    pub press: M,
    /// Its page, if it has one: a click anywhere on the tile but the icon opens it.
    pub details: Option<M>,
    /// The keyboard is on it.
    pub focused: bool,
}

pub const TILE_H: f32 = 60.0;

/// A tile: a calm rounded card, an icon in a circle that fills with the accent when the thing
/// is on, its name and state. No chevron to aim for: the circle switches it, the rest of the
/// card opens its page.
/// Where a tile's icon circle starts, from the tile's left edge (what lines up with it).
pub const TILE_INSET: f32 = 11.0;

pub fn tile<'a, M: Clone + 'static>(s: Scheme, t: Tile<M>) -> Element<'a, M> {
    let Tile {
        icon: i,
        title,
        status,
        on,
        press,
        details,
        focused,
    } = t;
    const DOT: f32 = 38.0;
    let text_w = TILE_W - 10.0 - DOT - 12.0 - 12.0;
    let (dot_bg, dot_fg) = if on {
        (s.accent, s.on_accent)
    } else {
        (s.ink.scale_alpha(0.1 * s.opacity), s.ink)
    };
    let circle = control(container(icon(i, tok::ICON_M, dot_fg)).center(DOT))
        .radius(DOT / 2.0)
        .colors(
            dot_bg,
            if on {
                dot_bg
            } else {
                s.ink.scale_alpha(0.16 * s.opacity)
            },
            if on {
                s.accent.scale_alpha(0.85)
            } else {
                s.raised_press
            },
        )
        .on_press(press.clone());
    let words = iced::widget::column![
        label_w(
            fit(&title, theme::text::BODY, text_w),
            theme::text::BODY,
            s.ink,
            theme::MEDIUM
        ),
        label(
            fit(&status, theme::text::CAPTION, text_w),
            theme::text::CAPTION,
            if on { s.ink } else { s.ink_dim }
        ),
    ]
    .spacing(1)
    .width(Length::Fill);
    let fill = s.ink.scale_alpha(0.06 * s.opacity);
    let body = control(row![circle, words].spacing(12).align_y(Alignment::Center))
        .padding(iced::Padding {
            top: 0.0,
            right: 12.0,
            bottom: 0.0,
            left: TILE_INSET,
        })
        .height(TILE_H)
        .width(Length::Fill)
        .radius(TILE_H / 2.0)
        .colors(fill, s.ink.scale_alpha(0.1 * s.opacity), s.raised_press)
        .no_press_scale()
        .on_press(details.unwrap_or(press));
    if !focused {
        return body.into();
    }
    // Where the keyboard is: a fine white ring.
    iced::widget::stack![
        body,
        container(Space::new())
            .width(Length::Fill)
            .height(TILE_H)
            .style(move |_| container::Style {
                border: iced::Border {
                    color: s.ink.scale_alpha(0.7 * s.opacity),
                    width: 1.5,
                    radius: (TILE_H / 2.0).into(),
                },
                ..container::Style::default()
            }),
    ]
    .into()
}

/// An on/off switch.
pub fn switch<'a, M: Clone + 'static>(s: Scheme, on: bool, on_press: M) -> Element<'a, M> {
    let knob = container(Space::new())
        .width(14)
        .height(14)
        .style(move |_| container::Style {
            background: Some(if on { s.on_accent } else { s.ink_dim }.into()),
            border: iced::Border::default().rounded(7),
            ..container::Style::default()
        });
    let track = container(knob)
        .width(36)
        .height(20)
        .padding(3)
        .align_x(if on { Alignment::End } else { Alignment::Start })
        .align_y(Alignment::Center);
    // Off, the track is a tint of the ink, not a surface colour: a surface colour matched the
    // card under it (Black and Frosted both) and the switch read as a lone dot.
    let (off, off_hover) = (
        s.ink.scale_alpha(0.16 * s.opacity),
        s.ink.scale_alpha(0.24 * s.opacity),
    );
    control(track)
        .radius(10.0)
        .colors(
            if on { s.accent } else { off },
            if on { s.accent } else { off_hover },
            if on { s.accent } else { off_hover },
        )
        .on_press(on_press)
        .into()
}

/// A list row: leading icon, title, optional subtitle, trailing element.
pub fn list_row<'a, M: Clone + 'static>(
    s: Scheme,
    leading: Element<'a, M>,
    title: impl text::IntoFragment<'a>,
    subtitle: Option<String>,
    trailing: Element<'a, M>,
    selected: bool,
    on_press: Option<M>,
) -> Element<'a, M> {
    let mut texts = iced::widget::column![label_w(
        title,
        theme::text::BODY,
        if selected { s.accent } else { s.ink },
        if selected {
            theme::MEDIUM
        } else {
            font::Weight::Medium
        }
    )]
    .spacing(1)
    .width(Length::Fill);
    if let Some(sub) = subtitle {
        texts = texts.push(label(sub, theme::text::CAPTION, s.ink_dim));
    }
    control(
        row![leading, texts, trailing]
            .spacing(12)
            .align_y(Alignment::Center),
    )
    .padding([0, 12])
    .height(44)
    .width(Length::Fill)
    .radius(tok::R_ROW)
    // No box behind a row: the selected one (the network in use) is told by its name in the
    // accent; the pointer only lifts a row faintly.
    .colors(Color::TRANSPARENT, s.hover(), s.raised_press)
    .no_press_scale()
    .on_press_maybe(on_press)
    .into()
}

/// A sub-page header: back button, title, optional trailing control.
pub fn subpage_header<'a, M: Clone + 'static>(
    s: Scheme,
    title: impl text::IntoFragment<'a>,
    back: M,
    trailing: Element<'a, M>,
) -> Element<'a, M> {
    row![
        icon_button(s, Icon::ArrowLeft, back, false),
        label_w(title, theme::text::TITLE, s.ink, theme::MEDIUM).width(Length::Fill),
        trailing,
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .height(tok::BAR_ITEM + 4.0)
    .into()
}

/// A thin level bar (the OSD's): `value` 0–`top` (1 normally; 1.5 when a volume may reach
/// 150 %), `width` × `height`. Past 100 % the bar runs to `top` and the
/// boost is in the danger colour, as on the control centre's slider; at 0 the fill is a faint dot.
fn tok_fill() -> f32 {
    crate::theme::tok::FILL
}

pub fn level_bar<'a, M: 'a>(
    s: Scheme,
    value: f32,
    top: f32,
    width: f32,
    height: f32,
) -> Element<'a, M> {
    use iced::widget::{Space, stack};
    // Up to 100 % the plain 0–100 bar; only a boosted level rescales it to `top`, the part past
    // 100 % in red (design call 2026-11-15: no notch cutting the bar).
    let top = if value > 1.0 { top.max(1.0) } else { 1.0 };
    let r = height / 2.0;
    let piece = move |w: f32, c: Color, radius: iced::border::Radius| {
        container(Space::new())
            .width(w)
            .height(height)
            .style(move |_| container::Style {
                background: Some(c.into()),
                border: iced::Border {
                    radius,
                    ..iced::Border::default()
                },
                ..container::Style::default()
            })
    };
    let round = iced::border::Radius::from(r);
    let on = (value / top).clamp(0.0, 1.0);
    // The control centre's track colour: one slider look everywhere.
    let mut bar = stack![piece(
        width,
        s.ink.scale_alpha(tok_fill() * s.opacity),
        round
    )];
    // Past 100 % the whole fill is red, one piece (as the control centre's slider).
    let fill = if value > 1.0 && top > 1.0 {
        s.bad
    } else if value > 0.0 {
        s.accent
    } else {
        s.ink_faint
    };
    bar = bar.push(piece((width * on).max(height), fill, round));
    bar.width(width).height(height).into()
}

/// The × at the end of a row that removes it (a startup app, a layout, a shortcut, a device, a
/// clipboard item): one size and look everywhere, a 28 px ghost circle.
pub fn remove_button<'a, M: Clone + 'a>(s: crate::theme::Scheme, on: M) -> Element<'a, M> {
    control(icon(Icon::X, crate::theme::tok::ICON, s.ink_dim))
        .padding(crate::theme::tok::S)
        .radius(crate::theme::tok::FULL)
        .kind(s, control::Kind::Ghost)
        .on_press(on)
        .into()
}

/// A key as a small raised cap ("Super", "T"): shortcut lists and the keys sheet.
pub fn keycap<'a, M: 'a>(s: crate::theme::Scheme, k: impl Into<String>) -> Element<'a, M> {
    iced::widget::container(label(k.into(), crate::theme::text::CAPTION, s.ink))
        .padding([crate::theme::tok::XXS, crate::theme::tok::S])
        .style(move |_| iced::widget::container::Style {
            background: Some(s.raised_hi.into()),
            border: iced::Border::default().rounded(5),
            ..iced::widget::container::Style::default()
        })
        .into()
}

/// A group's heading over its rows: SMALL, semibold, dim. `inset` lines it up with the rows'
/// own content (the one thing that differs between places).
pub fn section<'a, M: 'a>(
    s: Scheme,
    title: impl text::IntoFragment<'a>,
    inset: f32,
) -> Element<'a, M> {
    container(label_w(
        title,
        theme::text::SMALL,
        s.ink_dim,
        theme::SEMIBOLD,
    ))
    .padding([theme::tok::NONE, inset])
    .into()
}

/// Where a list would be when it is empty *because of what was typed or chosen* (a search, a
/// path, a filter): says why, centred, where the first rows would start. Not for a list that is
/// simply empty: that shows `empty` (an icon, no words: the user's first rule, 2026-11-13).
pub fn empty_reason<'a, M: 'a>(s: Scheme, why: String, height: f32) -> Element<'a, M> {
    container(label(why, theme::text::BODY, s.ink_dim))
        .center_x(Length::Fill)
        .padding(iced::Padding {
            top: 2.0 * theme::tok::XXXL,
            ..iced::Padding::ZERO
        })
        .height(height)
        .into()
}

/// Muted centred message for empty lists.
/// Where a list or picture would be and there is nothing (yet): only a faint icon of what
/// belongs there. No words: "Nothing here" labels were the user's first rule (2026-11-13).
pub fn empty<'a, M: 'static>(s: Scheme, i: Icon) -> Element<'a, M> {
    container(icon(i, crate::theme::tok::ICON_XL, s.ink_faint))
        .center_x(Length::Fill)
        .padding([24, 0])
        .into()
}

/// Something in progress where a list will be (looking for networks): its icon and what is
/// happening, so the wait is visible.
pub fn waiting<'a, M: 'a>(s: Scheme, i: Icon, msg: impl text::IntoFragment<'a>) -> Element<'a, M> {
    container(
        iced::widget::column![
            icon(i, crate::theme::tok::ICON_XL, s.ink_faint),
            label(msg, theme::text::SMALL, s.ink_dim)
        ]
        .spacing(8)
        .align_x(Alignment::Center),
    )
    .center_x(Length::Fill)
    .padding([24, 0])
    .into()
}

/// A canvas program's drawing, kept until the program or its size changes. The renderer can
/// only tell an unchanged drawing from a changed one when it is cached: an uncached canvas is
/// repainted, over all of its bounds, on every frame (a full-screen canvas cost ~8 ms a frame).
pub struct Memo<T> {
    cache: iced::widget::canvas::Cache,
    last: std::cell::RefCell<Option<(T, iced::Size)>>,
}

impl<T> Default for Memo<T> {
    fn default() -> Self {
        Self {
            cache: iced::widget::canvas::Cache::new(),
            last: std::cell::RefCell::new(None),
        }
    }
}

impl<T: Clone + PartialEq> Memo<T> {
    pub fn draw(
        &self,
        of: &T,
        renderer: &iced::Renderer,
        size: iced::Size,
        paint: impl Fn(&mut iced::widget::canvas::Frame),
    ) -> iced::widget::canvas::Geometry {
        let mut last = self.last.borrow_mut();
        if last.as_ref().is_none_or(|(t, s)| t != of || *s != size) {
            self.cache.clear();
            *last = Some((of.clone(), size));
        }
        self.cache.draw(renderer, size, paint)
    }
}

#[cfg(test)]
mod text_tests {
    #[test]
    fn titles_break_between_words() {
        let t = super::two_lines(
            "Warriors vs. Rockets Reaction Houston rolling with",
            13.0,
            160.0,
        );
        let lines: Vec<&str> = t.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(!lines[0].ends_with(' '));
        assert_eq!(super::two_lines("Short", 13.0, 160.0), "Short");
    }
}
