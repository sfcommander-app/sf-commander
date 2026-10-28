//! The look: a few reusable styles so that every screen feels like one
//! application - a darker frame (top bar, side panels), cards for content,
//! soft corners on buttons, one accent colour.
//!
//! Aurora: a quiet violet palette, subtle atmospheric background and light
//! headings and one blue accent for what you can press or are on. The names
//! of the constants are the old ones (GOLD is the heading colour, GOLD_DIM
//! the border...), so every screen picked the new look up at once.

use iced::{
    Background, Border, Color, Theme, theme,
    widget::{button, checkbox, container},
};

/// The drawn lettering stands this much taller than the text size it
/// replaces. A glyph's canvas carries an accent above the letter and the
/// letter fills the rest, so a canvas of `size * INK` has a cap height
/// about equal to text of `size` - which is what lets a title keep the room
/// it always had while changing its face
const INK: f32 = 1.42;

/// A line of the guild leader's lettering at the weight a `text` of this
/// size used to be. Every heading, caption and figure in the app goes
/// through here, so there is one number to turn if they come out big
fn drawn<'a>(
    line: impl ToString,
    colour: Color,
    text_size: f32,
) -> iced::Element<'a, crate::message::Message> {
    if !super::glyphs::artwork() {
        return iced::widget::text(line.to_string())
            .size(text_size)
            .font(if text_size >= 20.0 { DISPLAY_FONT } else { UI_FONT })
            .style(theme::Text::Color(colour))
            .into();
    }
    super::glyphs::write(&line.to_string(), colour, text_size * INK)
}

/// A tick box in the app's own colours. The toolkit's default is a blue
/// fill, which is the one colour this app spends on meaning something else
pub struct Check;

impl checkbox::StyleSheet for Check {
    type Style = Theme;

    fn active(&self, style: &Theme, is_checked: bool) -> checkbox::Appearance {
        checkbox::Appearance {
            background: Background::Color(if is_checked {
                SIDE_PLATE_ON
            } else {
                CARD
            }),
            icon_color: GOLD,
            border: Border {
                color: if is_checked { GOLD } else { GOLD_DEEP },
                width: 1.0,
                radius: 4.0.into(),
            },
            text_color: Some(style.palette().text),
        }
    }

    fn hovered(&self, style: &Theme, is_checked: bool) -> checkbox::Appearance {
        checkbox::Appearance {
            border: Border {
                color: GOLD,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..self.active(style, is_checked)
        }
    }
}

/// The app's tick box
pub fn check() -> theme::Checkbox {
    theme::Checkbox::Custom(Box::new(Check))
}

/// How wide the safety slider's track is. The tick labels under it are laid
/// out from the same number, so the two cannot drift apart
pub const SLIDER_W: f32 = 363.0;

/// The safety slider. The toolkit's default rail and handle are blue, which
/// is the one colour this app spends on meaning something else
pub struct Slide;

impl iced::widget::slider::StyleSheet for Slide {
    type Style = Theme;

    fn active(&self, _: &Theme) -> iced::widget::slider::Appearance {
        use iced::widget::slider::{Handle, HandleShape, Rail};
        iced::widget::slider::Appearance {
            rail: Rail {
                colors: (GOLD, LINE),
                width: 5.0,
                border_radius: 3.0.into(),
            },
            // A grip exactly one eleventh of the track wide, which is what
            // makes the numbers underneath line up with it.
            //
            // iced puts the handle's centre at `r + value/max * (W - 2r)`,
            // while a row of eleven equal labels centres the nth at
            // `(n + 0.5) * W/11`. Those two agree only when `r = W/22` -
            // with any other handle the scale drifts away from the thing it
            // is labelling, worst at the two ends. `SLIDER_W / 11` is that
            // width, so the 5 sits under the 5
            handle: Handle {
                shape: HandleShape::Rectangle {
                    width: (SLIDER_W / 11.0) as u16,
                    border_radius: 10.0.into(),
                },
                color: GOLD,
                border_width: 1.5,
                border_color: GOLD_DEEP,
            },
        }
    }

    fn hovered(&self, style: &Theme) -> iced::widget::slider::Appearance {
        self.active(style)
    }

    fn dragging(&self, style: &Theme) -> iced::widget::slider::Appearance {
        self.active(style)
    }
}

/// The app's slider
pub fn slide() -> theme::Slider {
    theme::Slider::Custom(Box::new(Slide))
}

/// A filled bar in a given colour, sunk into the same black the number
/// boxes use - the toolkit's default is blue on grey
pub struct Bar(pub Color);

impl iced::widget::progress_bar::StyleSheet for Bar {
    type Style = Theme;

    fn appearance(&self, _: &Theme) -> iced::widget::progress_bar::Appearance {
        iced::widget::progress_bar::Appearance {
            background: Background::Color(LINE),
            bar: Background::Color(self.0),
            border_radius: 2.0.into(),
        }
    }
}

/// A filled bar in a given colour
pub fn bar(c: Color) -> theme::ProgressBar {
    theme::ProgressBar::Custom(Box::new(Bar(c)))
}

// Shared palette. Status colours are reserved for actual outcomes; blue
// identifies navigation, selection and primary actions.
pub const NAME_GOLD: Color =
    Color::from_rgb(230.0 / 255.0, 200.0 / 255.0, 149.0 / 255.0);
pub const NEON: Color =
    Color::from_rgb(186.0 / 255.0, 176.0 / 255.0, 255.0 / 255.0);
pub const GOLD: Color =
    Color::from_rgb(241.0 / 255.0, 239.0 / 255.0, 255.0 / 255.0);
pub const GOLD_DIM: Color =
    Color::from_rgb(52.0 / 255.0, 54.0 / 255.0, 79.0 / 255.0);
pub const GOLD_DEEP: Color =
    Color::from_rgb(41.0 / 255.0, 43.0 / 255.0, 72.0 / 255.0);
pub const ACCENT: Color =
    Color::from_rgb(186.0 / 255.0, 176.0 / 255.0, 255.0 / 255.0);
pub const FRAME: Color =
    Color::from_rgb(17.0 / 255.0, 18.0 / 255.0, 35.0 / 255.0);
pub const CARD: Color =
    Color::from_rgb(27.0 / 255.0, 28.0 / 255.0, 51.0 / 255.0);
pub const LINE: Color =
    Color::from_rgb(52.0 / 255.0, 54.0 / 255.0, 79.0 / 255.0);
pub const MUTED: Color =
    Color::from_rgb(166.0 / 255.0, 167.0 / 255.0, 193.0 / 255.0);
pub const GOOD: Color =
    Color::from_rgb(139.0 / 255.0, 216.0 / 255.0, 196.0 / 255.0);
pub const BAD: Color =
    Color::from_rgb(242.0 / 255.0, 139.0 / 255.0, 155.0 / 255.0);
pub const WARN: Color =
    Color::from_rgb(230.0 / 255.0, 200.0 / 255.0, 149.0 / 255.0);
pub const SIDE_BG: Color =
    Color::from_rgb(17.0 / 255.0, 19.0 / 255.0, 33.0 / 255.0);
pub const SIDE_PLATE: Color = SIDE_BG;
pub const SIDE_PLATE_ON: Color =
    Color::from_rgb(45.0 / 255.0, 43.0 / 255.0, 74.0 / 255.0);
pub const SIDE_TEXT: Color =
    Color::from_rgb(216.0 / 255.0, 215.0 / 255.0, 235.0 / 255.0);

/// Container styles
pub enum Panel {
    /// Darker strip: top bar, sidebar, right column, console
    Frame,
    /// Subtle atmospheric background behind the work area.
    Aurora,
    /// The side column: deeper and bluer than the rest of the app
    Side,
    /// Every other row of a long table, so the eye can follow one across
    Stripe,
    /// Content card with rounded corners and a hairline border
    Card,
    /// The game's own framed panel: dark wood behind a gold moulding. Used
    /// for a block that stands on its own - one building, one opponent,
    /// one set of figures
    Game,
    /// A framed panel's title strip: a shade lighter than its interior, so
    /// the heading sits *on* the panel instead of floating above it
    Head,
    /// The recess a framed panel's figures sit in - a shade darker than the
    /// panel, the way the game sinks its numbers into the wood
    Well,
    /// Solid colour banner (mode strip)
    Solid(Color),
    /// A faint wash of a colour behind a table row - green for the best,
    /// red for the ones to deal with - readable without hiding the text
    Tint(Color),
}

impl container::StyleSheet for Panel {
    type Style = Theme;

    fn appearance(&self, style: &Theme) -> container::Appearance {
        let is_dark = {
            let bg = style.palette().background;
            (bg.r + bg.g + bg.b) / 3.0 < 0.5
        };
        match self {
            Panel::Aurora => container::Appearance {
                background: Some(if is_dark {
                    Background::Gradient(iced::Gradient::Linear(
                        iced::gradient::Linear::new(iced::Radians(0.45))
                            .add_stop(0.0, Color::from_rgb(0.075, 0.12, 0.16))
                            .add_stop(0.45, FRAME)
                            .add_stop(1.0, Color::from_rgb(0.17, 0.12, 0.23)),
                    ))
                } else {
                    Background::Color(style.palette().background)
                }),
                ..Default::default()
            },
            Panel::Frame => container::Appearance {
                background: Some(Background::Color(if is_dark {
                    FRAME
                } else {
                    Color::from_rgb(0.93, 0.94, 0.96)
                })),
                ..Default::default()
            },
            Panel::Card => container::Appearance {
                background: Some(Background::Color(if is_dark {
                    CARD
                } else {
                    Color::WHITE
                })),
                border: Border {
                    color: if is_dark {
                        LINE
                    } else {
                        Color::from_rgb(0.85, 0.86, 0.88)
                    },
                    width: 1.0,
                    radius: 18.0.into(),
                },
                ..Default::default()
            },
            // The moulding round a framed panel. One border cannot be lit on
            // top and shadowed underneath, so the bevel is two containers
            // deep: this is the outer, carrying the gold; `Head`/`Well` sit
            // inside it
            Panel::Game => container::Appearance {
                background: Some(Background::Color(if is_dark {
                    CARD
                } else {
                    Color::WHITE
                })),
                border: Border {
                    color: if is_dark {
                        GOLD_DIM
                    } else {
                        Color::from_rgb(0.78, 0.70, 0.52)
                    },
                    width: 1.0,
                    radius: 18.0.into(),
                },
                ..Default::default()
            },
            Panel::Head => container::Appearance {
                text_color: Some(if is_dark { GOLD } else { GOLD_DEEP }),
                ..Default::default()
            },
            Panel::Well => container::Appearance {
                background: Some(Background::Color(if is_dark {
                    CARD
                } else {
                    Color::from_rgb(0.97, 0.96, 0.94)
                })),
                border: Border {
                    color: if is_dark {
                        LINE
                    } else {
                        Color::from_rgb(0.88, 0.86, 0.82)
                    },
                    width: 1.0,
                    radius: 10.0.into(),
                },
                ..Default::default()
            },
            Panel::Side => container::Appearance {
                background: Some(Background::Color(if is_dark {
                    SIDE_BG
                } else {
                    FRAME
                })),
                ..Default::default()
            },
            Panel::Stripe => container::Appearance {
                background: Some(Background::Color(Color {
                    a: if is_dark { 0.05 } else { 0.04 },
                    ..if is_dark { Color::WHITE } else { Color::BLACK }
                })),
                ..Default::default()
            },
            Panel::Solid(c) => container::Appearance {
                background: Some(Background::Color(*c)),
                text_color: Some(Color::WHITE),
                ..Default::default()
            },
            Panel::Tint(c) => container::Appearance {
                background: Some(Background::Color(Color { a: 0.10, ..*c })),
                ..Default::default()
            },
        }
    }
}

pub fn tint(c: Color) -> theme::Container {
    theme::Container::Custom(Box::new(Panel::Tint(c)))
}

pub fn aurora() -> theme::Container {
    theme::Container::Custom(Box::new(Panel::Aurora))
}

pub fn frame() -> theme::Container {
    theme::Container::Custom(Box::new(Panel::Frame))
}

pub fn plate(active: bool) -> theme::Button {
    theme::Button::Custom(Box::new(Btn::Plate { active }))
}

/// Every other row gets a faint wash. Fifteen columns of small numbers is
/// where the eye loses the line it is on
pub fn stripe() -> theme::Container {
    theme::Container::Custom(Box::new(Panel::Stripe))
}

pub fn side() -> theme::Container {
    theme::Container::Custom(Box::new(Panel::Side))
}

pub fn card() -> theme::Container {
    theme::Container::Custom(Box::new(Panel::Card))
}

/// The game's framed panel: dark wood behind a gold moulding
pub fn game() -> theme::Container {
    theme::Container::Custom(Box::new(Panel::Game))
}

/// A framed panel's title strip
pub fn head() -> theme::Container {
    theme::Container::Custom(Box::new(Panel::Head))
}

/// The recess a framed panel's figures sit in
pub fn well() -> theme::Container {
    theme::Container::Custom(Box::new(Panel::Well))
}

pub fn solid(c: Color) -> theme::Container {
    theme::Container::Custom(Box::new(Panel::Solid(c)))
}

/// Button styles
pub enum Btn {
    /// Rounded accent button
    Primary,
    Roster { selected: bool, striped: bool },
    /// Rounded quiet button
    Quiet,
    /// The toolbar's brass-edged button
    Brass { active: bool },
    /// A lamp: lit gold with black lettering while the thing is on, grey
    /// while it is off
    Lamp { on: bool },
    /// Rounded red button
    Danger,
    /// Segmented switch half (pace, FULL BOT)
    Segment { on: bool, color: Color },
    /// One entry in the side column
    Plate { active: bool },
}

impl button::StyleSheet for Btn {
    type Style = Theme;

    fn active(&self, style: &Theme) -> button::Appearance {
        let text = style.palette().text;
        let rounded = |bg: Color, fg: Color| button::Appearance {
            background: Some(Background::Color(bg)),
            text_color: fg,
            border: Border {
                radius: 10.0.into(),
                ..Default::default()
            },
            ..Default::default()
        };
        match self {
            Btn::Plate { active } => button::Appearance {
                background: Some(Background::Color(if *active {
                    SIDE_PLATE_ON
                } else {
                    SIDE_PLATE
                })),
                text_color: SIDE_TEXT,
                border: Border {
                    color: if *active { ACCENT } else { Color::TRANSPARENT },
                    width: 1.0,
                    radius: 10.0.into(),
                },
                ..Default::default()
            },
            Btn::Roster { selected, striped } => button::Appearance {
                background: Some(Background::Color(if *selected { SIDE_PLATE_ON } else if *striped { CARD } else { Color::TRANSPARENT })),
                text_color: GOLD,
                border: Border { radius: 4.0.into(), ..Default::default() },
                ..Default::default()
            },
            Btn::Primary => rounded(ACCENT, FRAME),
            Btn::Quiet => rounded(Color { a: 0.10, ..text }, text),
            // The toolbar's own button: black with a brass edge and brass
            // lettering. `Quiet` stays a faint wash, because half its uses
            // are whole clickable rows - a chat line, a log entry - and a
            // gold box around each of those would be unreadable
            Btn::Brass { active } => button::Appearance {
                background: Some(Background::Color(if *active {
                    SIDE_PLATE_ON
                } else {
                    SIDE_PLATE
                })),
                text_color: GOLD,
                border: Border {
                    color: if *active { ACCENT } else { GOLD_DEEP },
                    width: 1.0,
                    radius: 10.0.into(),
                },
                ..Default::default()
            },
            Btn::Danger => {
                rounded(Color::from_rgb(0.72, 0.30, 0.30), Color::WHITE)
            }
            // Lit, it is the brightest thing in the top bar and the only
            // place black lettering appears - which is what makes it read
            // as a switch that is *on* rather than as one more label
            Btn::Lamp { on: true } => button::Appearance {
                background: Some(Background::Color(SIDE_PLATE_ON)),
                text_color: Color::WHITE,
                border: Border {
                    color: GOLD,
                    width: 1.0,
                    radius: 10.0.into(),
                },
                ..Default::default()
            },
            Btn::Lamp { on: false } => button::Appearance {
                background: Some(Background::Color(CARD)),
                text_color: MUTED,
                border: Border {
                    color: LINE,
                    width: 1.0,
                    radius: 10.0.into(),
                },
                ..Default::default()
            },
            Btn::Segment { on: true, color } => rounded(*color, FRAME),
            Btn::Segment { on: false, .. } => {
                rounded(Color { a: 0.08, ..text }, Color { a: 0.7, ..text })
            }
        }
    }

    fn pressed(&self, style: &Theme) -> button::Appearance {
        let mut appearance = self.active(style);
        appearance.border.color = ACCENT;
        appearance.border.width = 1.0;
        appearance
    }

    fn hovered(&self, style: &Theme) -> button::Appearance {
        let base = self.active(style);
        let lighter = |c: Color| Color {
            r: (c.r + 0.06).min(1.0),
            g: (c.g + 0.06).min(1.0),
            b: (c.b + 0.06).min(1.0),
            a: (c.a + 0.10).min(1.0),
        };
        button::Appearance {
            background: base
                .background
                .map(|b| match b {
                    Background::Color(c) => Background::Color(lighter(c)),
                    other => other,
                })
                .or(Some(Background::Color(Color {
                    a: 0.08,
                    ..style.palette().text
                }))),
            ..base
        }
    }
}

pub fn roster_button(selected: bool, striped: bool) -> theme::Button {
    theme::Button::Custom(Box::new(Btn::Roster { selected, striped }))
}

pub fn primary() -> theme::Button {
    theme::Button::Custom(Box::new(Btn::Primary))
}

pub fn quiet() -> theme::Button {
    theme::Button::Custom(Box::new(Btn::Quiet))
}

/// The toolbar's brass-edged button
pub fn brass(active: bool) -> theme::Button {
    theme::Button::Custom(Box::new(Btn::Brass { active }))
}

/// A switch that is lit while it is on
pub fn lamp(on: bool) -> theme::Button {
    theme::Button::Custom(Box::new(Btn::Lamp { on }))
}

pub fn danger() -> theme::Button {
    theme::Button::Custom(Box::new(Btn::Danger))
}

pub fn segment(on: bool, color: Color) -> theme::Button {
    theme::Button::Custom(Box::new(Btn::Segment { on, color }))
}

/// One page's body: it fills the window instead of sitting in a narrow
/// strip down the middle, but stops growing where lines get too long to
/// read comfortably, and sits on a card so the page has an edge
pub fn page<'a>(
    content: impl Into<iced::Element<'a, crate::message::Message>>,
) -> iced::Element<'a, crate::message::Message> {
    use iced::{
        Length,
        widget::{container, scrollable},
    };
    // No card around the whole page any more: the panels inside it carry
    // their own gold edge, and a second frame around all of them turned
    // every screen into a box inside a box
    scrollable(
        container(container(content).width(Length::Fill).max_width(1460.0))
            .width(Length::Fill)
            .padding([12, 24])
            .center_x(),
    )
    .height(Length::Fill)
    .into()
}

/// One framed block, the way the game frames a block: a gold-lettered title
/// strip across the top and the content in the wood below it. Screens build
/// out of these instead of out of bare rows, which is what makes a page
/// read as a game screen rather than as a settings dialog
pub fn panel<'a>(
    title: impl ToString,
    content: impl Into<iced::Element<'a, crate::message::Message>>,
) -> iced::widget::Container<'a, crate::message::Message> {
    use iced::{
        Length,
        widget::{column, container},
    };
    container(
        column!(
            container(drawn(title.to_string(), GOLD, 14.0))
                .width(Length::Fill)
                .padding([18, 20, 4, 20])
                .style(head()),
            container(content)
                .padding([14, 20, 20, 20])
                .width(Length::Fill),
        )
        .spacing(0),
    )
    .width(Length::Fill)
    .style(game())
}

/// The small print under a setting: what it does, why it is there. Muted,
/// so it never competes with the setting itself
pub fn note<'a>(
    body: impl ToString,
) -> iced::Element<'a, crate::message::Message> {
    use iced::Length;
    use iced::widget::container;
    // A note is an explanation by definition - hidden with the rest of
    // them unless the "?" is on (6.1.4)
    if !super::glyphs::help() {
        return iced::widget::Space::new(0, 0).into();
    }
    // Prose, so it has to break between words - `drawn` makes one
    // unbreakable row and a sentence would run off the panel
    if !super::glyphs::artwork() {
        return container(
            iced::widget::text(body.to_string())
                .size(11)
                .style(theme::Text::Color(MUTED)),
        )
        .width(Length::Fill)
        .into();
    }
    container(super::glyphs::paragraph(
        &body.to_string(),
        MUTED,
        11.0 * INK,
    ))
    .width(Length::Fill)
    .into()
}

/// A label/figure line whose figure carries a meaning: green for something
/// there to take, red for something stopped, gold for the game's own clock
pub fn line_in<'a>(
    label: impl ToString,
    value: impl ToString,
    colour: Option<Color>,
) -> iced::Element<'a, crate::message::Message> {
    use iced::{
        Alignment, Length,
        widget::{horizontal_space, row},
    };
    row!(
        drawn(label.to_string(), MUTED, 12.0),
        horizontal_space(),
        drawn(value.to_string(), colour.unwrap_or(GOLD), 13.0),
    )
    .width(Length::Fill)
    .align_items(Alignment::Center)
    .into()
}

/// One of the game's number boxes: a small gold caption over a big figure,
/// sunk into the wood. This is the shape the game states everything in -
/// how many soldiers, how much honour, how long left - and it reads at a
/// glance where a `label: value` row does not
pub fn tile<'a>(
    caption: impl ToString,
    value: impl ToString,
    colour: Color,
) -> iced::Element<'a, crate::message::Message> {
    tile_sub(caption, value, colour, None::<String>)
}

/// The three lines a number box can hold, and the one height they all
/// stand in. A box with a limit under its figure used to be taller than the
/// ones without, so a row of six came out as a row of six different boxes
const TILE_CAPTION: f32 = 12.0;
const TILE_VALUE: f32 = 27.0;
const TILE_SUB: f32 = 10.0;

fn tile_height() -> f32 {
    (TILE_CAPTION + TILE_VALUE + TILE_SUB) * INK + 2.0 + 12.0
}

/// How tall the letters themselves stand inside a drawn line of this
/// height. A picture beside a figure has to match the ink, not the canvas -
/// the canvas carries empty room above the letters for an accent, so an
/// icon centred on it sits three or four pixels high
fn ink_of(canvas: f32) -> f32 {
    canvas * super::glyphs::INK_H as f32 / super::glyphs::GLYPH_H as f32
}

/// A number box with the drawn picture that belongs to the thing counted -
/// the purse's own mushroom beside a mushroom count, its hourglass beside
/// the glasses. The picture is the label, so it reads without being read
pub fn tile_pic<'a>(
    pic: iced::widget::image::Handle,
    caption: impl ToString,
    value: impl ToString,
    colour: Color,
) -> iced::Element<'a, crate::message::Message> {
    use iced::{
        Alignment, Length,
        widget::{column, container, image, row},
    };
    // The picture stands exactly as tall as the digits beside it and sits
    // on their foot, so the two read as one thing
    let ink = ink_of(TILE_VALUE * INK);
    container(
        column!(
            drawn(caption.to_string(), MUTED, TILE_CAPTION),
            row!(
                image(pic).height(ink),
                drawn(value.to_string().to_uppercase(), colour, TILE_VALUE),
            )
            .spacing(6)
            .align_items(Alignment::End),
        )
        .spacing(1)
        .align_items(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fixed(tile_height()))
    .padding([6, 8])
    .center_x()
    .center_y()
    .style(well())
    .into()
}

/// A number box with a third line under the figure - a limit, a rate, the
/// reason. Small and muted, so the figure still carries the box
pub fn tile_sub<'a>(
    caption: impl ToString,
    value: impl ToString,
    colour: Color,
    under: Option<impl ToString>,
) -> iced::Element<'a, crate::message::Message> {
    use iced::{
        Alignment, Length,
        widget::{column, container},
    };
    let mut col = column!(
        drawn(caption.to_string(), MUTED, TILE_CAPTION),
        drawn(value.to_string().to_uppercase(), colour, TILE_VALUE),
    )
    .spacing(1)
    .align_items(Alignment::Center);
    if let Some(under) = under {
        col =
            col.push(drawn(under.to_string().to_uppercase(), MUTED, TILE_SUB));
    }
    container(col)
        .width(Length::Fill)
        .height(Length::Fixed(tile_height()))
        .padding([6, 8])
        .center_x()
        .center_y()
        .style(well())
        .into()
}

/// A framed block whose title strip carries its own controls on the right -
/// a feed's "clear", a list's filter. The game puts its buttons in the bar
/// too, and it keeps them off the content
pub fn panel_bar<'a>(
    title: impl ToString,
    controls: impl Into<iced::Element<'a, crate::message::Message>>,
    content: impl Into<iced::Element<'a, crate::message::Message>>,
) -> iced::widget::Container<'a, crate::message::Message> {
    use iced::{
        Alignment, Length,
        widget::{column, container, horizontal_space, row},
    };
    container(
        column!(
            container(
                row!(
                    drawn(title.to_string(), GOLD, 14.0),
                    horizontal_space(),
                    controls.into(),
                )
                .spacing(6)
                .align_items(Alignment::Center)
            )
            .width(Length::Fill)
            .padding([9, 12])
            .style(head()),
            container(content)
                .padding([10, 12])
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .spacing(0),
    )
    .width(Length::Fill)
    .style(game())
}

/// A framed block with no title - the same wood and moulding, for content
/// that names itself
pub fn block<'a>(
    content: impl Into<iced::Element<'a, crate::message::Message>>,
) -> iced::widget::Container<'a, crate::message::Message> {
    use iced::{Length, widget::container};
    container(
        container(content)
            .padding([14, 20, 20, 20])
            .width(Length::Fill),
    )
    .width(Length::Fill)
    .style(game())
}

/// A section heading inside a page: the accent colour and a hairline under
/// it, so a long settings page reads as a stack of blocks instead of one
/// undivided wall
pub fn heading<'a>(
    label: impl ToString,
) -> iced::Element<'a, crate::message::Message> {
    use iced::{
        Length,
        widget::{column, horizontal_rule},
    };
    column!(
        drawn(label.to_string().to_uppercase(), GOLD, 15.0),
        horizontal_rule(1),
    )
    .spacing(4)
    .width(Length::Fill)
    .into()
}

/// A line of the system face at its heaviest, in a given colour.
///
/// Everything on screen is set in the guild leader's own lettering now -
/// except a console. His artwork is capitals only, and forty log lines
/// running down a narrow column all in capitals is harder to read than what
/// it replaced, not easier. So the feeds keep the system face and take the
/// weight instead: the heaviest the machine has, which is what stops it
/// looking thin beside the drawn letters either side of it.
pub fn bold<'a>(
    line: impl ToString,
    colour: Color,
    size: f32,
) -> iced::Element<'a, crate::message::Message> {
    use iced::widget::text;
    text(line.to_string())
        .size(size)
        .font(FAT)
        .style(theme::Text::Color(colour))
        .into()
}

/// The same, in the machine's monospaced face - for the console proper,
/// where a column of times and names only lines up if the digits do
pub fn bold_mono<'a>(
    line: impl ToString,
    colour: Color,
    size: f32,
    width: iced::Length,
) -> iced::Element<'a, crate::message::Message> {
    use iced::widget::text;
    text(line.to_string())
        .size(size)
        .font(iced::Font {
            weight: iced::font::Weight::Bold,
            ..iced::Font::MONOSPACE
        })
        .style(theme::Text::Color(colour))
        .width(width)
        .into()
}

/// The heaviest weight the machine's own face reliably has. Text the user
/// types cannot be set in the artwork - the toolkit draws a field's
/// contents itself - so those take the weight instead, and stop reading as
/// the one thin thing left on the screen.
///
/// Bold, not Black. Black is a separate family rather than a weight in most
/// system faces, and asking for it got a fallback with almost nothing in
/// it: a console of log lines came out as a scattering of loose digits with
/// every letter missing.
pub const UI_FONT: iced::Font = iced::Font::with_name("Aurora Circuit");
pub const FAT: iced::Font = iced::Font::with_name("Aurora Circuit Strong");
pub const DISPLAY_FONT: iced::Font = iced::Font::with_name("Aurora Circuit Display");

/// White lettering for a filled button - the accent-coloured ones and the
/// lit half of a segmented switch
pub fn on_button<'a>(
    line: impl ToString,
    text_size: f32,
) -> iced::Element<'a, crate::message::Message> {
    on_plate(line, FRAME, text_size)
}

/// A line for a coloured plate: a banner, a lit switch, a segmented
/// button.
///
/// The drawn letters carry a dark outline, which is what lifts them off the
/// app's black. On gold or green that outline is the loudest thing in the
/// glyph and the lettering reads as black and ragged - so on a coloured
/// plate it comes off and the letter is filled flat
pub fn on_plate<'a>(
    line: impl ToString,
    colour: Color,
    text_size: f32,
) -> iced::Element<'a, crate::message::Message> {
    if !super::glyphs::artwork() {
        return iced::widget::text(line.to_string())
            .size(text_size)
            .font(if text_size >= 20.0 { DISPLAY_FONT } else { UI_FONT })
            .style(theme::Text::Color(colour))
            .into();
    }
    super::glyphs::write_on(
        &line.to_string(),
        colour,
        text_size * INK,
        super::glyphs::Plate::Light,
    )
}

