//! The guild leader's own lettering, drawn rather than typed.
//!
//! The app ships three sets of pictures: ten digits, twenty-six letters and
//! fifteen accented Czech ones. They are what the purse card has always been
//! written in, and everything on a plate is written in them now - because a
//! countdown set in the system font beside a fat outlined numeral was the
//! thinnest thing on the screen.
//!
//! Two problems this module exists to solve:
//!
//! * **Colour.** There is artwork in gold only (plus red, blue and green for
//!   the purse). A plate needs six: gold for the game's own clock, green for
//!   something there to take, red for something stopped, purple for a running
//!   total, and so on. Asking for six more folders of artwork would be
//!   absurd, so the gold is recoloured here - each pixel keeps its own
//!   brightness and takes the new hue, which leaves the black outline black
//!   and the shading inside intact.
//!
//! * **A shared baseline.** The digits were drawn 22x40 with the glyph
//!   filling the canvas; the letters need room above them for a caron or an
//!   acute. So everything is placed on a canvas [`GLYPH_H`] tall with the
//!   ink sitting in the bottom [`INK_H`], and one `height` then means the
//!   same thing for a digit, a letter and a colon alike.
//!
//! What has no picture - a space, a per-cent sign, a Czech word the accent
//! set does not cover - falls back to bold text of the same colour. That is
//! a last resort, not the normal case.

use std::{collections::HashMap, sync::{Arc, LazyLock}};

use iced::{
    Alignment, Color, Element, Length,
    alignment::Horizontal,
    theme,
    widget::{self, horizontal_space, row, text},
};

#[cfg(test)]
use super::style;

use crate::message::Message;

/// The canvas every glyph is drawn on
pub const GLYPH_H: u32 = 54;
/// ...of which the letter itself occupies the bottom this much, leaving the
/// rest for an accent
pub const INK_H: u32 = 40;

// ----------------------------------------------------------------- loading

macro_rules! digit_bytes {
    () => {
        [
            include_bytes!("../../assets/digits/0.png").as_slice(),
            include_bytes!("../../assets/digits/1.png").as_slice(),
            include_bytes!("../../assets/digits/2.png").as_slice(),
            include_bytes!("../../assets/digits/3.png").as_slice(),
            include_bytes!("../../assets/digits/4.png").as_slice(),
            include_bytes!("../../assets/digits/5.png").as_slice(),
            include_bytes!("../../assets/digits/6.png").as_slice(),
            include_bytes!("../../assets/digits/7.png").as_slice(),
            include_bytes!("../../assets/digits/8.png").as_slice(),
            include_bytes!("../../assets/digits/9.png").as_slice(),
        ]
    };
}

macro_rules! letter_bytes {
    ($($ch:literal => $file:literal),* $(,)?) => {
        [$(($ch, include_bytes!(concat!("../../assets/letters/", $file, ".png")).as_slice())),*]
    };
}

/// Every letter there is a picture for. Anything not here is set in text
fn letter_files() -> [(char, &'static [u8]); 41] {
    letter_bytes!(
        'A' => "A", 'B' => "B", 'C' => "C", 'D' => "D", 'E' => "E",
        'F' => "F", 'G' => "G", 'H' => "H", 'I' => "I", 'J' => "J",
        'K' => "K", 'L' => "L", 'M' => "M", 'N' => "N", 'O' => "O",
        'P' => "P", 'Q' => "Q", 'R' => "R", 'S' => "S", 'T' => "T",
        'U' => "U", 'V' => "V", 'W' => "W", 'X' => "X", 'Y' => "Y",
        'Z' => "Z",
        'Á' => "Á", 'Č' => "Č", 'Ď' => "Ď", 'É' => "É", 'Ě' => "Ě",
        'Í' => "Í", 'Ň' => "Ň", 'Ó' => "Ó", 'Ř' => "Ř", 'Š' => "Š",
        'Ť' => "Ť", 'Ú' => "Ú", 'Ů' => "Ů", 'Ý' => "Ý", 'Ž' => "Ž",
    )
}

// ---------------------------------------------------------------- recolour

/// One glyph's pixels, recoloured and placed on the shared canvas.
///
/// `raise` lifts artwork that was drawn without room for an accent - the
/// digits - onto the taller canvas so it shares the letters' baseline.
fn recolour(
    raw: &[u8],
    target: Color,
    raise: bool,
    plate: Plate,
) -> widget::image::Handle {
    let (w, h, px) = recolour_px_on(raw, target, raise, plate);
    widget::image::Handle::from_pixels(w, h, px)
}

/// What a line of lettering is standing on.
///
/// The artwork was drawn to sit on black: a light letter with a dark
/// outline traced round it, which is what lifts it off the plate. Put that
/// same glyph on a gold button or a green banner and the outline is the
/// loudest thing in it - the letters read as black, smudged and ragged,
/// because the edge that was helping is now fighting the background.
///
/// On a coloured plate the outline comes off and the body is filled at full
/// strength, which is simply what a letter looks like on paper
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Plate {
    /// The app's own black
    Dark,
    /// A coloured or light button, banner or strip
    Light,
}

/// The pixels behind [`recolour`], kept separate so the shared baseline can
/// be checked without a renderer
fn recolour_px(raw: &[u8], target: Color, raise: bool) -> (u32, u32, Vec<u8>) {
    recolour_px_on(raw, target, raise, Plate::Dark)
}

fn recolour_px_on(
    raw: &[u8],
    target: Color,
    raise: bool,
    plate: Plate,
) -> (u32, u32, Vec<u8>) {
    let img = image::load_from_memory_with_format(raw, image::ImageFormat::Png)
        .unwrap_or_else(|e| panic!("glyph: {e}"))
        .into_rgba8();
    let (w, h) = (img.width(), img.height());
    let mut px = img.into_raw();
    for p in px.chunks_exact_mut(4) {
        if p[3] == 0 {
            continue;
        }
        // How bright this pixel is in the original, 0-1. The outline sits
        // near zero and has to stay there, or the glyph loses its edge and
        // turns to mush against a dark plate
        let v = f32::from(p[0].max(p[1]).max(p[2])) / 255.0;
        if v < 0.30 {
            // The outline. It belongs on black and nowhere else
            if plate == Plate::Light {
                p[3] = 0;
            }
            continue;
        }
        // Stretch what is left of the range back over the whole of it, so a
        // tint keeps the contrast the gold had between its darkest body
        // pixel and its highlight. On a coloured plate there is no outline
        // to hold the shape, so the body is filled flat instead - shading a
        // letter that has lost its edge only makes it look faded
        let k = ((v - 0.30) / 0.70).clamp(0.0, 1.0);
        let lit = match plate {
            Plate::Dark => 0.55 + 0.45 * k,
            Plate::Light => 1.0,
        };
        p[0] = (target.r * lit * 255.0) as u8;
        p[1] = (target.g * lit * 255.0) as u8;
        p[2] = (target.b * lit * 255.0) as u8;
    }
    if !raise || h >= GLYPH_H {
        return (w, h, px);
    }
    let mut tall = vec![0u8; (w * GLYPH_H * 4) as usize];
    let top = GLYPH_H - h;
    let row_bytes = (w * 4) as usize;
    for y in 0..h {
        let src = (y * w * 4) as usize;
        let dst = ((y + top) * w * 4) as usize;
        tall[dst..dst + row_bytes].copy_from_slice(&px[src..src + row_bytes]);
    }
    (w, GLYPH_H, tall)
}

/// The marks the alphabet is missing. There is artwork for the letters and
/// the digits and for nothing else, so a countdown ("7:23"), a sign-up
/// ("1/1"), a rate ("+12%") or a decimal ("1.4M") each had a thin system
/// character standing between two fat outlined numerals - which is the one
/// thing on this screen that looks broken. They are drawn here instead, to
/// the artwork's own recipe: a body in the line's colour with a dark
/// outline traced round it.
///
/// Each is a shape in the ink band the letters live in, so a mark sits on
/// the same baseline and stands to the same cap height as the letter beside
/// it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Mark {
    Colon,
    Slash,
    Dot,
    Comma,
    Dash,
    Plus,
    Percent,
    Bang,
    Query,
    Open,
    Close,
    Equals,
    Times,
    Apos,
    Hash,
    MidDot,
    Tilde,
    Under,
    Semi,
    Quote,
}

impl Mark {
    /// The mark a character is drawn as, if it is one of them
    fn of(ch: char) -> Option<Mark> {
        Some(match ch {
            ':' => Mark::Colon,
            '/' => Mark::Slash,
            '.' => Mark::Dot,
            ',' => Mark::Comma,
            '-' | '\u{2013}' | '\u{2212}' => Mark::Dash,
            '+' => Mark::Plus,
            '%' => Mark::Percent,
            '!' => Mark::Bang,
            '?' => Mark::Query,
            '(' => Mark::Open,
            ')' => Mark::Close,
            '=' => Mark::Equals,
            '*' | '\u{d7}' => Mark::Times,
            '\'' | '\u{2019}' => Mark::Apos,
            '#' => Mark::Hash,
            '\u{b7}' | '\u{2022}' => Mark::MidDot,
            '~' => Mark::Tilde,
            '_' => Mark::Under,
            ';' => Mark::Semi,
            '"' | '\u{201c}' | '\u{201d}' => Mark::Quote,
            _ => return None,
        })
    }

    /// How wide a mark is drawn. A full stop needs a fraction of the room a
    /// per-cent sign does, and giving them all one width is what makes
    /// drawn punctuation read as a row of stamps
    fn width(self) -> i32 {
        match self {
            Mark::Apos | Mark::MidDot | Mark::Dot | Mark::Comma
            | Mark::Bang => 22,
            Mark::Colon => 16,
            Mark::Dash | Mark::Open | Mark::Close => 25,
            Mark::Equals => 28,
            Mark::Slash | Mark::Plus | Mark::Query | Mark::Times => 30,
            Mark::Hash => 34,
            Mark::Percent => 36,
            Mark::Tilde | Mark::Under => 26,
            Mark::Semi => 18,
            Mark::Quote => 25,
        }
    }

    /// Whether this pixel of the ink band is inside the mark.
    ///
    /// `y` is measured from the top of the band, and the artwork fills that
    /// band completely: a letter's cap sits on row 0 and its foot on row 38.
    /// So do these. The strokes are ten pixels through, because that is what
    /// the letters' stems measure - a mark drawn any finer is the thin
    /// system character all over again, just in gold.
    #[allow(clippy::match_same_arms)]
    fn body(self, x: i32, y: i32) -> bool {
        // A rectangle of the stroke weight the artwork uses
        let bar = |x0: i32, x1: i32, y0: i32, y1: i32| {
            (x0..=x1).contains(&x) && (y0..=y1).contains(&y)
        };
        // A stroke running from (ax, ay) to (bx, by), as thick as a stem
        let stroke = |ax: f32, ay: f32, bx: f32, by: f32| {
            if !(ay.min(by)..=ay.max(by)).contains(&(y as f32)) {
                return false;
            }
            let t = (y as f32 - ay) / (by - ay);
            (x as f32 - (ax + (bx - ax) * t)).abs() <= 4.5
        };
        // A square ring - which is what a per-cent sign's two eyes are at
        // this size, because a circle six pixels across is a smudge
        let ring = |cx: i32, cy: i32, r: i32, hole: i32| {
            (x - cx).abs() <= r
                && (y - cy).abs() <= r
                && ((x - cx).abs() > hole || (y - cy).abs() > hole)
        };
        match self {
            Mark::Colon => bar(3, 12, 10, 19) || bar(3, 12, 29, 38),
            Mark::Slash => stroke(23.0, 0.0, 7.0, 38.0),
            Mark::Dot => bar(6, 15, 29, 38),
            // The tail is what tells a comma from a full stop at a glance,
            // so it runs off the bottom of the dot rather than beside it
            Mark::Comma => {
                bar(6, 15, 24, 33) || stroke(12.0, 32.0, 6.0, 38.0)
            }
            Mark::Dash => bar(2, 22, 15, 24),
            Mark::Plus => bar(2, 27, 14, 23) || bar(10, 19, 6, 31),
            Mark::Percent => {
                ring(10, 9, 8, 3)
                    || ring(25, 29, 8, 3)
                    || stroke(26.0, 1.0, 9.0, 37.0)
            }
            Mark::Bang => bar(6, 15, 0, 26) || bar(6, 15, 29, 38),
            Mark::Query => {
                bar(6, 23, 0, 9)           // the top of the hook
                    || bar(2, 11, 2, 13)   // down its left side
                    || bar(18, 27, 2, 17)  // and its right
                    || stroke(23.0, 15.0, 15.0, 23.0)
                    || bar(10, 19, 21, 26) // the stem
                    || bar(10, 19, 29, 38) // the dot
            }
            // The two brackets are one parabola and its mirror, which keeps
            // them the same shape rather than two hand-drawn curves
            Mark::Open | Mark::Close => {
                if !(0..=38).contains(&y) {
                    return false;
                }
                let t = (y as f32 - 19.0) / 19.0;
                let bulge = 6.0 * (1.0 - t * t);
                let c = if self == Mark::Open {
                    16.0 - bulge
                } else {
                    7.0 + bulge
                };
                (x as f32 - c).abs() <= 4.5
            }
            Mark::Equals => bar(2, 25, 9, 18) || bar(2, 25, 25, 34),
            Mark::Times => {
                stroke(7.0, 6.0, 22.0, 32.0) || stroke(22.0, 6.0, 7.0, 32.0)
            }
            Mark::Apos => stroke(12.0, 0.0, 7.0, 16.0),
            // The two uprights have to clear each other between the bars,
            // or the whole sign closes up into a blob
            Mark::Hash => {
                let upright = |top_x: f32| {
                    (0..=38).contains(&y)
                        && (x as f32 - (top_x - 4.0 * (y as f32 / 38.0))).abs()
                            <= 3.5
                };
                bar(2, 31, 11, 17)
                    || bar(2, 31, 24, 30)
                    || upright(12.0)
                    || upright(27.0)
            }
            Mark::MidDot => bar(6, 15, 15, 24),
            // A wave, drawn as two arcs of the same parabola the brackets
            // use, so it sits in the same family as the rest
            Mark::Tilde => {
                if !(14..=27).contains(&y) {
                    return false;
                }
                let t = (x as f32 - 13.0) / 11.0;
                let mid = 20.5 - 5.0 * t * (1.0 - t.abs()) * 2.0;
                (2..=23).contains(&x) && (y as f32 - mid).abs() <= 4.0
            }
            Mark::Under => bar(2, 23, 33, 38),
            Mark::Semi => {
                bar(4, 13, 10, 19)
                    || bar(4, 13, 24, 33)
                    || stroke(11.0, 32.0, 6.0, 38.0)
            }
            Mark::Quote => {
                stroke(10.0, 0.0, 6.0, 16.0) || stroke(18.0, 0.0, 14.0, 16.0)
            }
        }
    }
}

/// One mark, drawn and outlined
fn mark(m: Mark, target: Color, plate: Plate) -> widget::image::Handle {
    let (w, h, px) = mark_px_on(m, target, plate);
    widget::image::Handle::from_pixels(w, h, px)
}

fn mark_px(m: Mark, target: Color) -> (u32, u32, Vec<u8>) {
    mark_px_on(m, target, Plate::Dark)
}

fn mark_px_on(m: Mark, target: Color, plate: Plate) -> (u32, u32, Vec<u8>) {
    let w = m.width();
    let h = GLYPH_H as i32;
    let top = (GLYPH_H - INK_H) as i32;
    let body = |x: i32, y: i32| m.body(x, y - top);
    let mut px = vec![0u8; (w * h * 4) as usize];
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if body(x, y) {
                // The artwork is not flat: its body runs from about 0.95 of
                // full at the cap down to 0.85 at the foot. A mark filled
                // at full strength stands out as the one bright thing in a
                // line, so it is shaded to the same law
                let lit = 0.95
                    - 0.10 * ((y - top) as f32 / INK_H as f32).clamp(0.0, 1.0);
                px[i] = (target.r * lit * 255.0) as u8;
                px[i + 1] = (target.g * lit * 255.0) as u8;
                px[i + 2] = (target.b * lit * 255.0) as u8;
                px[i + 3] = 255;
            } else if plate == Plate::Dark
                && (-2..=2).any(|dy| {
                    (-2..=2).any(|dx| {
                        let (nx, ny) = (x + dx, y + dy);
                        (0..w).contains(&nx)
                            && (0..h).contains(&ny)
                            && body(nx, ny)
                    })
                })
            {
                // Anything within two pixels of the body is the outline,
                // which is what keeps the mark readable on a dark plate
                px[i] = 18;
                px[i + 1] = 16;
                px[i + 2] = 12;
                px[i + 3] = 255;
            }
        }
    }
    (w as u32, GLYPH_H, px)
}

/// Every glyph in one colour
struct Face {
    digits: [widget::image::Handle; 10],
    letters: HashMap<char, widget::image::Handle>,
    marks: HashMap<Mark, widget::image::Handle>,
}

impl Face {
    fn new(target: Color, plate: Plate) -> Face {
        let raw = digit_bytes!();
        Face {
            digits: core::array::from_fn(|i| {
                recolour(raw[i], target, true, plate)
            }),
            letters: letter_files()
                .into_iter()
                .map(|(ch, bytes)| (ch, recolour(bytes, target, false, plate)))
                .collect(),
            marks: ALL_MARKS
                .iter()
                .map(|m| (*m, mark(*m, target, plate)))
                .collect(),
        }
    }
}

/// Every mark there is, so a face can be built without listing them twice
const ALL_MARKS: [Mark; 20] = [
    Mark::Colon,
    Mark::Slash,
    Mark::Dot,
    Mark::Comma,
    Mark::Dash,
    Mark::Plus,
    Mark::Percent,
    Mark::Bang,
    Mark::Query,
    Mark::Open,
    Mark::Close,
    Mark::Equals,
    Mark::Times,
    Mark::Apos,
    Mark::Hash,
    Mark::MidDot,
    Mark::Tilde,
    Mark::Under,
    Mark::Semi,
    Mark::Quote,
];

/// The purse card's colours, named so the card reads as artwork rather than
/// as three arbitrary tints
pub const PURSE_RED: Color = Color::from_rgb(0.788, 0.180, 0.161);
pub const PURSE_BLUE: Color = Color::from_rgb(0.176, 0.600, 0.878);

/// The colour body text is set in when nothing asks for another
pub const PLAIN: Color = Color::from_rgb(0.898, 0.894, 0.910);

/// Every face the app has asked for so far, made on demand.
///
/// This was eleven fixed faces and a nearest-colour search, which quietly
/// meant that any colour not on the list was drawn in whichever of the
/// eleven happened to be closest. The lit switch in the top bar asks for
/// near-black lettering on gold; the nearest of the eleven was a mid grey,
/// so FOLLOW ON came out grey with a black outline round it and read as
/// smudged. A face is cheap - fifty small pictures - and the app writes in
/// perhaps a dozen colours, so it is built for whatever colour is actually
/// asked for and kept
static FACES: LazyLock<std::sync::Mutex<HashMap<(u32, Plate), Arc<Face>>>> =
    LazyLock::new(|| std::sync::Mutex::new(HashMap::new()));

/// Colours are compared as eight bits a channel: two that round to the same
/// byte draw the same pixels, so they are the same face
fn face_key(c: Color) -> u32 {
    let q = |v: f32| u32::from((v.clamp(0.0, 1.0) * 255.0).round() as u8);
    (q(c.r) << 16) | (q(c.g) << 8) | q(c.b)
}

fn face_on(c: Color, plate: Plate) -> Arc<Face> {
    let key = (face_key(c), plate);
    let mut cache = match FACES.lock() {
        Ok(c) => c,
        // A panic in another thread while holding this lock cannot be
        // allowed to take the window down with it
        Err(poisoned) => poisoned.into_inner(),
    };
    Arc::clone(
        cache
            .entry(key)
            .or_insert_with(|| Arc::new(Face::new(c, plate))),
    )
}

// ------------------------------------------------------------------ render

/// A line written in the guild leader's lettering.
///
/// `height` is the canvas, so the letters themselves stand about three
/// quarters of it - the rest is the room an accent needs. Anything with no
/// picture is set in bold text of the same colour rather than dropped.
pub fn write<'a>(
    line: &str,
    colour: Color,
    height: f32,
) -> Element<'a, Message> {
    write_on(line, colour, height, Plate::Dark)
}

/// The same, saying what it is standing on. A coloured button or banner
/// takes [`Plate::Light`], which drops the outline the artwork carries for
/// black backgrounds
pub fn write_on<'a>(
    line: &str,
    colour: Color,
    height: f32,
    plate: Plate,
) -> Element<'a, Message> {
    let f = face_on(colour, plate);
    let bold = iced::Font {
        weight: iced::font::Weight::Bold,
        ..super::style::UI_FONT
    };
    let mut r = row!().spacing(0.0).align_items(Alignment::End);
    let mut pending = String::new();
    // Text and pictures cannot share a baseline by accident: the fallback is
    // sized to the ink, not to the canvas, and pushed down to sit on it
    let flush = move |r: widget::Row<'a, Message>,
                      pending: &mut String|
     -> widget::Row<'a, Message> {
        if pending.is_empty() {
            return r;
        }
        r.push(
            widget::container(
                text(std::mem::take(pending))
                    .size(height * 0.62)
                    .font(bold)
                    .style(theme::Text::Color(colour)),
            )
            .padding([0, 0, (height * 0.06) as u16, 0]),
        )
    };
    for ch in line.chars() {
        let upper = ch.to_uppercase().next().unwrap_or(ch);
        // There are capitals and nothing else in the artwork, so a sentence
        // set in it comes out SHOUTING. Small caps is the printer's answer
        // to exactly that: the letters that were lower case are set from
        // the same drawings at four fifths the height, which reads as
        // ordinary sentence case from a foot away and keeps the face
        let lower = ch.is_lowercase() && ch != upper;
        let h = if lower { height * 0.78 } else { height };
        let pic = if let Some(d) = ch.to_digit(10) {
            Some(f.digits[d as usize].clone())
        } else if let Some(m) = Mark::of(ch) {
            f.marks.get(&m).cloned()
        } else {
            f.letters.get(&upper).cloned()
        };
        match pic {
            Some(handle) => {
                r = flush(r, &mut pending);
                r = r.push(
                    widget::image(handle)
                        .height(h)
                        .filter_method(widget::image::FilterMethod::Linear),
                );
            }
            // A space between pictures is narrower than a space in text -
            // the glyphs carry their own outline, which is gap enough
            None if ch == ' ' && pending.is_empty() => {
                r = r.push(
                    horizontal_space().width(Length::Fixed(height * 0.16)),
                );
            }
            None => pending.push(ch),
        }
    }
    // Pinned to the canvas height, bottom aligned. Without this a line is
    // only as tall as its tallest letter, so a word of pure small caps
    // comes out shorter than the word beside it that happens to carry a
    // comma - and in a wrapping paragraph the two then sit at different
    // heights. One height for every line means one baseline
    widget::container(flush(r, &mut pending))
        .height(height)
        .align_y(iced::alignment::Vertical::Bottom)
        .into()
}

/// The same, hard against the right edge - what a plate's figure wants
pub fn write_right<'a>(
    line: &str,
    colour: Color,
    height: f32,
) -> Element<'a, Message> {
    // A line with no picture in it at all is one widget, and aligning it is
    // cheaper than wrapping a row nobody will see the edges of
    if !line.chars().any(|c| {
        c.is_ascii_digit()
            || Mark::of(c).is_some()
            || face_on(colour, Plate::Dark)
                .letters
                .contains_key(&c.to_uppercase().next().unwrap_or(c))
    }) {
        return text(line.to_string())
            .size(height * 0.62)
            .font(iced::Font {
                weight: iced::font::Weight::Bold,
                ..super::style::UI_FONT
            })
            .style(theme::Text::Color(colour))
            .horizontal_alignment(Horizontal::Right)
            .into();
    }
    write(line, colour, height)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_letter_the_czech_labels_need_has_a_picture() {
        let have: std::collections::HashSet<char> =
            letter_files().into_iter().map(|(c, _)| c).collect();
        // The nav plates, the panel titles and the words the badges use
        for word in [
            "HOSPODA",
            "ARÉNA",
            "MĚSTSKÁ STRÁŽ",
            "OBCHODY",
            "ARENA MANAGER",
            "PETI",
            "DUNGEONY",
            "PEVNOST",
            "PODSVĚTÍ",
            "CECH",
            "MAIL",
            "DENÍK",
            "POSTAVA",
            "ÚSPĚCHY",
            "ŽIVĚ",
            "ÚDRŽBA",
            "ÚTOKY",
            "ČEKÁ",
            "VOLNÝ",
            "KE KRMENÍ",
        ] {
            for ch in word.chars().filter(|c| *c != ' ') {
                assert!(have.contains(&ch), "{word}: no picture for {ch}");
            }
        }
    }

    #[test]
    fn the_glyphs_all_share_one_baseline() {
        // A digit is drawn without room for an accent and has to be raised
        // onto the taller canvas, or numbers would stand higher than words
        let gold = style::NAME_GOLD;
        for raw in digit_bytes!() {
            let (_, h, _) = recolour_px(raw, gold, true);
            assert_eq!(h, GLYPH_H, "a digit is not on the shared canvas");
        }
        for (ch, raw) in letter_files() {
            let (_, h, _) = recolour_px(raw, gold, false);
            assert_eq!(h, GLYPH_H, "{ch} is not on the shared canvas");
        }
        for m in ALL_MARKS {
            let (_, h, _) = mark_px(m, gold);
            assert_eq!(h, GLYPH_H);
        }
    }

    #[test]
    fn every_mark_is_drawn_and_none_runs_off_its_canvas() {
        // A mark whose body touches the edge loses its outline there, which
        // is exactly how a glyph turns to mush against a dark plate. And a
        // mark with no body at all is a hole in a line of lettering
        for m in ALL_MARKS {
            let w = m.width();
            let mut ink = 0u32;
            for y in 0..INK_H as i32 {
                for x in 0..w {
                    if m.body(x, y) {
                        ink += 1;
                        assert!(
                            (2..w - 2).contains(&x),
                            "{m:?} reaches its own edge at x={x} of {w}, so \
                             its outline is cut off"
                        );
                    }
                }
            }
            assert!(ink > 20, "a mark came out blank");
        }
    }

    #[test]
    fn the_marks_a_figure_needs_are_all_drawn() {
        // Every character the app puts between two numerals. One of these
        // missing means a thin system glyph standing between two fat
        // outlined ones, which is the thing this module exists to stop
        for ch in "+-.,:/%()!?=#'*".chars() {
            assert!(Mark::of(ch).is_some(), "no picture for {ch}");
        }
    }
}

#[cfg(test)]
mod proof {
    use super::*;

    #[test]
    #[ignore = "writes a proof sheet for looking at"]
    fn dump() {
        let gold = style::NAME_GOLD;
        let line = "AB12 . , - + % ! ? ( ) = * ' # \u{b7} 7:23 1.4M +12% 114/300";
        let mut tiles: Vec<(u32, u32, Vec<u8>)> = Vec::new();
        let raw = digit_bytes!();
        for ch in line.chars() {
            if ch == ' ' {
                tiles.push((10, GLYPH_H, vec![0; (10 * GLYPH_H * 4) as usize]));
            } else if let Some(d) = ch.to_digit(10) {
                tiles.push(recolour_px(raw[d as usize], gold, true));
            } else if let Some(m) = Mark::of(ch) {
                tiles.push(mark_px(m, gold));
            } else {
                let f = letter_files();
                let b = f
                    .iter()
                    .find(|(c, _)| *c == ch)
                    .map(|(_, b)| *b)
                    .unwrap();
                tiles.push(recolour_px(b, gold, false));
            }
        }
        let w: u32 = tiles.iter().map(|t| t.0).sum();
        let mut out = image::RgbaImage::new(w, GLYPH_H);
        for p in out.pixels_mut() {
            *p = image::Rgba([25, 25, 28, 255]);
        }
        let mut x0 = 0;
        for (tw, th, px) in &tiles {
            for y in 0..*th {
                for x in 0..*tw {
                    let i = ((y * tw + x) * 4) as usize;
                    if px[i + 3] > 0 {
                        out.put_pixel(
                            x0 + x,
                            y,
                            image::Rgba([px[i], px[i + 1], px[i + 2], 255]),
                        );
                    }
                }
            }
            x0 += tw;
        }
        out.save("/tmp/marks.png").unwrap();
    }
}

// ------------------------------------------------------------- the stand-in

/// A line of text set in the guild leader's lettering, with the same
/// builder the toolkit's `text` has.
///
/// The app has five hundred odd places that write a line, and they all say
/// `text(..).size(..).style(..)`. Rewriting every one of them to call
/// [`write`] by hand would have been five hundred chances to get a size or
/// a colour wrong, so this stands in for `text` instead: each screen
/// imports this in its place and every call site is left exactly as it was.
///
/// `font` is accepted and ignored - there is one face now, and it is his.
pub struct Lettered {
    line: String,
    size: f32,
    colour: Option<Color>,
    width: Option<Length>,
    align: Option<Horizontal>,
    /// What the call site asked for before the artwork took over. Kept so
    /// that switching the artwork off puts the screen back exactly as it
    /// was - bold where it was bold, monospaced where it lined figures up
    font: Option<iced::Font>,
}

/// Whether the app is set in the guild leader's own lettering or in the
/// machine's.
///
/// One switch in the top bar, read here at the moment a line is drawn, so
/// it takes effect on the next frame and not on the next start. It is a
/// global rather than a parameter because the five hundred call sites that
/// write a line know nothing about settings - they say `text(..)` and this
/// decides what that means
static ARTWORK: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub fn set_artwork(on: bool) {
    ARTWORK.store(on, std::sync::atomic::Ordering::Relaxed);
}

#[must_use]
pub fn artwork() -> bool {
    ARTWORK.load(std::sync::atomic::Ordering::Relaxed)
}

/// The explanations under the controls - "how it chooses", "what this
/// does" - shown or not (6.1.4). Off by default: the pages were walls of
/// small print, and the controls say what they do. The "?" in the top bar
/// brings every one of them back at once
static HELP: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

pub fn set_help(on: bool) {
    HELP.store(on, std::sync::atomic::Ordering::Relaxed);
}

#[must_use]
pub fn help() -> bool {
    HELP.load(std::sync::atomic::Ordering::Relaxed)
}

/// Is this line an explanation rather than a value? Small print (12 px or
/// under), a sentence and more (over 90 characters), and set in the muted
/// grey or the plain colour every help paragraph uses. Status lines carry
/// a colour of their own and stay; figures, names and chat are short or
/// bigger
fn is_explanation(l: &Lettered) -> bool {
    l.size <= 12.5
        && l.line.chars().count() > 90
        && l.colour.is_none_or(|c| c == super::style::MUTED)
}

/// A line in his lettering. Stands in for the toolkit's `text`
pub fn lettered(line: impl ToString) -> Lettered {
    Lettered {
        line: line.to_string(),
        // What the toolkit sets a line in when nobody says otherwise
        size: 16.0,
        colour: None,
        width: None,
        align: None,
        font: None,
    }
}

impl Lettered {
    #[must_use]
    pub fn size(mut self, size: impl Into<iced::Pixels>) -> Self {
        self.size = size.into().0;
        self
    }

    /// The colour, taken out of whatever the call site was passing to the
    /// toolkit. A line with no colour of its own is set in [`PLAIN`]
    #[must_use]
    pub fn style(mut self, style: impl Into<theme::Text>) -> Self {
        self.colour = match style.into() {
            theme::Text::Color(c) => Some(c),
            theme::Text::Default => None,
        };
        self
    }

    #[must_use]
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = Some(width.into());
        self
    }

    #[must_use]
    pub fn horizontal_alignment(mut self, align: Horizontal) -> Self {
        self.align = Some(align);
        self
    }

    /// Remembered rather than ignored: it is what the line goes back to
    /// when the artwork is switched off
    #[must_use]
    pub fn font(mut self, font: iced::Font) -> Self {
        self.font = Some(font);
        self
    }
}

impl<'a> From<Lettered> for Element<'a, Message> {
    fn from(l: Lettered) -> Element<'a, Message> {
        if !help() && is_explanation(&l) {
            return widget::Space::new(0, 0).into();
        }
        // Switched off: the toolkit's own text, with whatever the call site
        // originally asked for. This is the app as it looked before the
        // lettering, not an approximation of it
        if !artwork() {
            let mut t = text(l.line).size(l.size).font(if l.size >= 20.0 { super::style::DISPLAY_FONT } else { super::style::UI_FONT });
            if let Some(f) = l.font {
                t = t.font(f);
            }
            if let Some(c) = l.colour {
                t = t.style(theme::Text::Color(c));
            }
            if let Some(w) = l.width {
                t = t.width(w);
            }
            if let Some(a) = l.align {
                t = t.horizontal_alignment(a);
            }
            return t.into();
        }
        let colour = l.colour.unwrap_or(PLAIN);
        // The drawn line stands taller than the text size it replaces: its
        // canvas carries an accent above the letter. `INK` is the one place
        // that ratio is written down
        // Anything with a space in it is prose as far as layout is
        // concerned, and prose has to be able to break. A row of pictures
        // cannot, so a sentence that no longer fitted its panel was simply
        // sliced off at the edge - which is worse than the thin type it
        // replaced. Words go to the wrapping row; a single word, a figure
        // or a heading stays one piece
        let height = l.size * INK;
        let drawn = if l.line.trim().chars().count() > 24
            && l.line.trim().contains(char::is_whitespace)
        {
            paragraph(&l.line, colour, height)
        } else {
            write(&l.line, colour, height)
        };
        if l.width.is_none() && l.align.is_none() {
            return drawn;
        }
        let mut c = widget::container(drawn);
        if let Some(w) = l.width {
            c = c.width(w);
        }
        c = match l.align {
            Some(Horizontal::Right) => c.align_x(Horizontal::Right),
            Some(Horizontal::Center) => c.center_x(),
            _ => c,
        };
        c.into()
    }
}

/// How much taller a drawn line stands than the text size it replaces. The
/// canvas holds the letter plus the room an accent needs above it, so a
/// canvas of `size * INK` has about the cap height of text of `size`
pub const INK: f32 = 1.42;

/// A tick box whose label is set in his lettering.
///
/// The toolkit draws a checkbox's label itself, in the system face, so the
/// only way to have it in the artwork is to hand the toolkit an empty label
/// and write the words beside it. Clicking the words still toggles the box,
/// which is the whole reason a label belongs to its checkbox in the first
/// place. Stands in for the toolkit's `checkbox`, with the same builder.
pub struct Ticked<'a> {
    label: String,
    checked: bool,
    on_toggle: Option<Box<dyn Fn(bool) -> Message + 'a>>,
    box_size: f32,
    text_size: f32,
    style: theme::Checkbox,
}

/// A tick box with a drawn label. Stands in for the toolkit's `checkbox`
pub fn ticked<'a>(label: impl ToString, checked: bool) -> Ticked<'a> {
    Ticked {
        label: label.to_string(),
        checked,
        on_toggle: None,
        box_size: 16.0,
        text_size: 12.0,
        style: theme::Checkbox::Primary,
    }
}

impl<'a> Ticked<'a> {
    #[must_use]
    pub fn on_toggle(mut self, f: impl Fn(bool) -> Message + 'a) -> Self {
        self.on_toggle = Some(Box::new(f));
        self
    }

    #[must_use]
    pub fn size(mut self, size: impl Into<iced::Pixels>) -> Self {
        self.box_size = size.into().0;
        self
    }

    #[must_use]
    pub fn text_size(mut self, size: impl Into<iced::Pixels>) -> Self {
        self.text_size = size.into().0;
        self
    }

    #[must_use]
    pub fn style(mut self, style: impl Into<theme::Checkbox>) -> Self {
        self.style = style.into();
        self
    }
}

impl<'a> From<Ticked<'a>> for Element<'a, Message> {
    fn from(t: Ticked<'a>) -> Element<'a, Message> {
        if !artwork() {
            // The toolkit draws its own label again, which is what it did
            // before the lettering
            let mut b = widget::checkbox(t.label, t.checked)
                .size(t.box_size)
                .text_size(t.text_size)
                .style(t.style);
            if let Some(f) = t.on_toggle {
                b = b.on_toggle(f);
            }
            return b.into();
        }
        let flipped = t.on_toggle.as_ref().map(|f| f(!t.checked));
        let mut b = widget::checkbox("", t.checked)
            .size(t.box_size)
            .style(t.style);
        if let Some(f) = t.on_toggle {
            b = b.on_toggle(f);
        }
        let label = write(&t.label, PLAIN, t.text_size * INK);
        // The words are as much of the switch as the box is
        let label: Element<'a, Message> = match flipped {
            Some(msg) => widget::mouse_area(label).on_press(msg).into(),
            None => label,
        };
        row!(b, label)
            .spacing(2)
            .align_items(Alignment::Center)
            .into()
    }
}

/// A paragraph in his lettering, broken between words like any other.
///
/// A line of drawn letters is a row of pictures, and a row does not wrap -
/// so a sentence long enough to need a second line simply ran off the panel
/// and was cut in half. Each word is handed over as one item instead, and
/// the toolkit's wrapping row breaks between them, which is where a line of
/// Czech wants to break anyway.
pub fn paragraph<'a>(
    body: &str,
    colour: Color,
    height: f32,
) -> Element<'a, Message> {
    let words: Vec<Element<'a, Message>> = body
        .split_whitespace()
        .map(|w| write(w, colour, height))
        .collect();
    iced_aw::Wrap::with_elements(words)
        // A word space, and enough between lines that the accents on one
        // row clear the capitals on the next
        .spacing(height * 0.22)
        .line_spacing(height * 0.06)
        .into()
}

// ----------------------------------------------------------------- metrics

/// How wide each glyph is drawn, in the artwork's own pixels. Colour does
/// not change a glyph's shape, so this is worked out once for all faces
struct Widths {
    digits: [u32; 10],
    letters: HashMap<char, u32>,
    marks: HashMap<Mark, u32>,
    /// The mean advance of a letter, for code that has to size a column
    /// before it knows what will go in it
    mean: f32,
}

static WIDTHS: LazyLock<Widths> = LazyLock::new(|| {
    let raw = digit_bytes!();
    let widths = |b: &[u8]| {
        image::load_from_memory_with_format(b, image::ImageFormat::Png)
            .map_or(22, |i| i.width())
    };
    let letters: HashMap<char, u32> = letter_files()
        .into_iter()
        .map(|(ch, b)| (ch, widths(b)))
        .collect();
    let sum: u32 = letters.values().sum();
    #[allow(clippy::cast_precision_loss)]
    let mean = sum as f32 / letters.len() as f32;
    Widths {
        digits: core::array::from_fn(|i| widths(raw[i])),
        letters,
        marks: ALL_MARKS
            .iter()
            .map(|m| (*m, u32::try_from(m.width()).unwrap_or(22)))
            .collect(),
        mean,
    }
});

/// Exactly how wide [`write`] will draw this line at this canvas height.
///
/// Every screen that has to fit something into a column used to guess this
/// from a characters-times-a-constant rule, and the guess was wrong in both
/// directions - a heading clipped here, an inch of dead space there. The
/// drawing and the measuring now come from the same pixels
pub fn measure(line: &str, height: f32) -> f32 {
    let scale = height / GLYPH_H as f32;
    let mut w = 0.0;
    for ch in line.chars() {
        let upper = ch.to_uppercase().next().unwrap_or(ch);
        let small = if ch.is_lowercase() && ch != upper { 0.78 } else { 1.0 };
        let px = if let Some(d) = ch.to_digit(10) {
            WIDTHS.digits[d as usize]
        } else if let Some(m) = Mark::of(ch) {
            WIDTHS.marks.get(&m).copied().unwrap_or(22)
        } else if let Some(px) = WIDTHS.letters.get(&upper) {
            *px
        } else if ch == ' ' {
            w += height * 0.16;
            continue;
        } else {
            // Set in text, at the size the fallback uses
            w += height * 0.62 * 0.55;
            continue;
        };
        #[allow(clippy::cast_precision_loss)]
        {
            w += px as f32 * scale * small;
        }
    }
    w
}

/// What a line will really be this wide, at a given *text* size, in
/// whichever face is switched on.
///
/// Every screen that sizes a column has to ask this rather than
/// [`measure`], which only ever answers for the artwork. A table measured
/// in fat drawn letters and then drawn in the machine's thin ones is a
/// table with a hand's width of nothing after every entry - which is
/// exactly what the guild list turned into the moment the lettering was
/// switched off
#[must_use]
pub fn width_at(line: &str, text_size: f32) -> f32 {
    if artwork() {
        return measure(line, text_size * INK);
    }
    // The system face, at the factor this app used before the artwork
    // existed and that its tables were built around
    #[allow(clippy::cast_precision_loss)]
    {
        line.chars().count() as f32 * text_size * 0.62
    }
}

/// The width of one average letter at this canvas height - for a column
/// that has to be sized before its contents are known
pub fn em(height: f32) -> f32 {
    WIDTHS.mean * height / GLYPH_H as f32
}

