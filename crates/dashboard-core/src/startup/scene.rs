//! Ten bounded procedural scenes. Every position is selected directly from time.
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::{FONT_6X10, FONT_10X20};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, Line, PrimitiveStyle, Rectangle, Triangle};
use embedded_graphics::text::{Alignment, Text};

use super::{Phase, StartupProfile, StartupStatus, StartupTheme};
use crate::display::St7789Renderer;
use crate::vehicle_status::VehicleStatus;

/// Dark blue-black RGB565 outline/background ink.
const INK: Rgb565 = Rgb565::new(1, 3, 5);
/// Warm off-white RGB565 status text.
const WHITE: Rgb565 = Rgb565::new(29, 60, 28);
/// Gold RGB565 highlights and decorative indicators.
const GOLD: Rgb565 = Rgb565::new(31, 43, 5);
/// Cyan RGB565 status accent and illustrated droplets.
const CYAN: Rgb565 = Rgb565::new(3, 53, 28);
/// Blue RGB565 character fill.
const BLUE: Rgb565 = Rgb565::new(3, 19, 31);
/// Red RGB565 artwork accent, unrelated to live warning evaluation.
const RED: Rgb565 = Rgb565::new(29, 7, 5);

#[derive(Clone, Copy)]
/// Caller-owned initialization facts displayed without initiating acquisition or I/O.
pub struct StartupFacts {
    /// Already classified acquisition/adapter state; demo is explicitly labeled synthetic.
    pub vehicle: VehicleStatus,
    /// Configuration admission succeeded rather than requiring fallback.
    pub config_ok: bool,
    /// Persistent preferences were loaded instead of defaults.
    pub settings_loaded: bool,
    /// Configured source count; displayed as selection evidence, never ECU qualification.
    pub configured_sources: u8,
}
/// Fills `d` with color `c` at landscape-reference (`x`, `y`) and size (`w`, `h`) pixels.
/// Portrait scales dimensions/coordinates by 3/4 and adds a 60-pixel vertical offset;
/// height has a one-pixel minimum there. Inputs are bounded scene literals; target clips edges.
fn rect(
    d: &mut St7789Renderer<'_>,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    c: Rgb565,
) {
    let _ = Rectangle::new(
        Point::new(px(d, x), py(d, y)),
        Size::new(
            if portrait(d) { w * 3 / 4 } else { w },
            if portrait(d) { (h * 3 / 4).max(1) } else { h },
        ),
    )
    .into_styled(PrimitiveStyle::with_fill(c))
    .draw(d);
}
/// Draws color `c` from reference endpoints `a` to `b` into `d`; `width` is an
/// unscaled target-pixel stroke. Endpoints follow portrait coordinate mapping.
fn line(
    d: &mut St7789Renderer<'_>,
    a: (i32, i32),
    b: (i32, i32),
    width: u32,
    c: Rgb565,
) {
    let _ = Line::new(Point::new(px(d, a.0), py(d, a.1)), Point::new(px(d, b.0), py(d, b.1)))
        .into_styled(PrimitiveStyle::with_stroke(c, width))
        .draw(d);
}
/// Fills a circle of `diameter` reference pixels at top-left (`x`, `y`) in color `c`
/// into `d`. Portrait scales its center and diameter together to keep concentric rings aligned.
fn disk(
    d: &mut St7789Renderer<'_>,
    x: i32,
    y: i32,
    diameter: u32,
    c: Rgb565,
) {
    // Map the object center and diameter together. Independent top-left and
    // diameter rounding would move concentric rings away from their hub.
    let (position, diameter) = if portrait(d) {
        let scaled = (diameter * 3 / 4).max(1);
        (
            Point::new(
                px(d, x + diameter as i32 / 2) - scaled as i32 / 2,
                py(d, y + diameter as i32 / 2) - scaled as i32 / 2,
            ),
            scaled,
        )
    } else {
        (Point::new(x, y), diameter)
    };
    let _ = Circle::new(position, diameter)
        .into_styled(PrimitiveStyle::with_fill(c))
        .draw(d);
}
/// Fills the reference-coordinate triangle `a`, `b`, `c` in `color` into `d`,
/// applying the shared portrait coordinate mapping and framebuffer clipping.
fn tri(
    d: &mut St7789Renderer<'_>,
    a: (i32, i32),
    b: (i32, i32),
    c: (i32, i32),
    color: Rgb565,
) {
    let _ = Triangle::new(
        Point::new(px(d, a.0), py(d, a.1)),
        Point::new(px(d, b.0), py(d, b.1)),
        Point::new(px(d, c.0), py(d, c.1)),
    )
    .into_styled(PrimitiveStyle::with_fill(color))
    .draw(d);
}
/// Centers `value` at reference baseline `y` in `color` on `d`; `big` selects
/// 10x20 rather than 6x10 glyphs. Portrait maps the baseline and splits the exact
/// skip/recovery hint onto two fixed rows. Fonts are not geometrically scaled.
fn text(
    d: &mut St7789Renderer<'_>,
    value: &str,
    y: i32,
    color: Rgb565,
    big: bool,
) {
    if portrait(d) && value == "TAP TO SKIP  /  HOLD Y TO RECOVER" {
        for (label, baseline) in [("TAP TO SKIP", 284), ("HOLD Y TO RECOVER", 298)] {
            Text::with_alignment(
                label,
                Point::new(120, baseline),
                MonoTextStyle::new(&FONT_6X10, color),
                Alignment::Center,
            )
            .draw(d)
            .ok();
        }
        return;
    }
    let font = if big { &FONT_10X20 } else { &FONT_6X10 };
    let _ = Text::with_alignment(
        value,
        Point::new(i32::from(d.geometry().width()) / 2, py(d, y)),
        MonoTextStyle::new(font, color),
        Alignment::Center,
    )
    .draw(d);
}
/// Returns an integer triangular offset for time `t` and nonzero even `period`
/// in milliseconds, spanning `-amplitude..=amplitude` reference pixels.
/// Private callers use small positive `amplitude` and period >=2; other values
/// can divide by zero, overflow, or exceed the stated range for odd periods.
fn wave(
    t: u32,
    period: u32,
    amplitude: i32,
) -> i32 {
    let p = (t % period) as i32;
    let half = (period / 2) as i32;
    (if p < half { p } else { period as i32 - p }) * amplitude * 2 / half - amplitude
}

/// Clears and draws startup `phase` directly into `d` using `profile` and supplied
/// status `facts`. `elapsed` is phase-relative milliseconds; splash loops every
/// 2000 ms or freezes at 1999 ms, while `reduced_motion` fixes its artwork at 1200 ms.
/// Dashboard only clears the buffer. Custom theme draws an unavailable label here;
/// callers use the asset decoder separately. No transfer, waiting or acquisition occurs.
pub fn draw(
    d: &mut St7789Renderer<'_>,
    phase: Phase,
    profile: StartupProfile,
    elapsed: u64,
    reduced_motion: bool,
    facts: StartupFacts,
) {
    d.clear_buffer(if phase == Phase::Splash { Rgb565::WHITE } else { INK });
    if phase == Phase::Status {
        text(d, "SYSTEM CHECK", 46, GOLD, true);
        rect(d, 36, 64, 248, 2, GOLD);
        text(
            d,
            if facts.config_ok {
                "CONFIGURATION LOADED"
            } else {
                "CONFIGURATION FALLBACK"
            },
            93,
            WHITE,
            false,
        );
        text(
            d,
            if facts.vehicle == VehicleStatus::Demo {
                "OBD: DEMO / SYNTHETIC"
            } else {
                facts.vehicle.label()
            },
            124,
            CYAN,
            true,
        );
        if profile.status == StartupStatus::Detailed {
            text(
                d,
                if facts.settings_loaded {
                    "SAVED PREFERENCES LOADED"
                } else {
                    "DEFAULT PREFERENCES"
                },
                154,
                WHITE,
                false,
            );
            let mut label = heapless::String::<32>::new();
            use core::fmt::Write;
            let _ = write!(label, "{} SOURCES CONFIGURED", facts.configured_sources);
            text(d, &label, 177, WHITE, false);
            text(d, "SOURCE SELECTION IS NOT ECU PROOF", 199, CYAN, false);
        }
        text(d, "TAP TO SKIP  /  HOLD Y TO RECOVER", 225, WHITE, false);
        return;
    }
    if phase != Phase::Splash {
        return;
    }
    let cycle = 2000_u64;
    let t = if reduced_motion {
        1200
    } else if profile.loop_animation {
        elapsed % cycle
    } else {
        elapsed.min(cycle - 1)
    } as u32;
    match profile.theme {
        StartupTheme::Custom => text(d, "CUSTOM ARTWORK UNAVAILABLE", 120, INK, false),
        StartupTheme::Aezakmi => aezakmi(d, t),
        StartupTheme::Turbo => turbo(d, t),
        StartupTheme::Sanic => sanic(d, t),
        StartupTheme::BoostBro => boost(d, t),
        StartupTheme::WhereLambo => lambo(d, t),
        StartupTheme::MachineSpirit => machine_spirit(d, t),
        StartupTheme::RitualDiagnostics => ritual_diagnostics(d, t),
        StartupTheme::RedGoesFasta => red_goes_fasta(d, t),
        StartupTheme::BoostPadawan => boost_padawan(d, t),
        StartupTheme::TurboBrainlet => turbo_brainlet(d, t),
    }
    text(d, "TAP TO SKIP  /  HOLD Y TO RECOVER", 231, INK, false);
}

/// Returns whether `d` uses the 240x320 logical portrait framebuffer geometry.
fn portrait(d: &St7789Renderer<'_>) -> bool { d.geometry() == dashboard_config::Geometry::Portrait }
/// Returns bounded reference coordinate `x` mapped into `d`: portrait uses integer `3*x/4`.
fn px(
    d: &St7789Renderer<'_>,
    x: i32,
) -> i32 {
    if portrait(d) { x * 3 / 4 } else { x }
}
/// Returns bounded reference coordinate `y` mapped into `d`: portrait uses integer `3*y/4 + 60`.
fn py(
    d: &St7789Renderer<'_>,
    y: i32,
) -> i32 {
    if portrait(d) { y * 3 / 4 + 60 } else { y }
}

/// Full-white artwork background/occlusion color.
const PAPER: Rgb565 = Rgb565::WHITE;
/// Pale gray-green RGB565 metal highlights and smoke.
const MIST: Rgb565 = Rgb565::new(27, 56, 28);
/// Midtone RGB565 metal surfaces and text.
const STEEL: Rgb565 = Rgb565::new(16, 35, 19);
/// Brown RGB565 worn panels and accent strokes.
const RUST: Rgb565 = Rgb565::new(22, 19, 8);

// Sparse vector strokes keep headlines crisp at both native geometries without
// storing raster artwork or allocating an intermediate surface.
/// Centers sparse vector glyphs from ASCII `value` in `color` into `d`, with
/// bounded reference `top`/`height` pixels. Unknown bytes advance as blank glyphs;
/// caller supplies short scene literals so coordinate arithmetic remains bounded.
fn headline(
    d: &mut St7789Renderer<'_>,
    value: &str,
    top: i32,
    height: i32,
    color: Rgb565,
) {
    let advance = height * 3 / 4;
    let total = value.len() as i32 * advance - height / 5;
    let mut x = (320 - total) / 2;
    for glyph in value.bytes() {
        let strokes: &[(i32, i32, i32, i32)] = match glyph {
            b'A' => &[(0, 10, 3, 0), (3, 0, 6, 10), (1, 6, 5, 6)],
            b'B' => {
                &[
                    (0, 0, 0, 10),
                    (0, 0, 4, 0),
                    (4, 0, 6, 2),
                    (6, 2, 4, 5),
                    (0, 5, 4, 5),
                    (4, 5, 6, 7),
                    (6, 7, 6, 8),
                    (6, 8, 4, 10),
                    (0, 10, 4, 10),
                ]
            }
            b'C' => &[(6, 0, 1, 0), (1, 0, 0, 2), (0, 2, 0, 8), (0, 8, 1, 10), (1, 10, 6, 10)],
            b'D' => {
                &[
                    (0, 0, 0, 10),
                    (0, 0, 4, 0),
                    (4, 0, 6, 2),
                    (6, 2, 6, 8),
                    (6, 8, 4, 10),
                    (4, 10, 0, 10),
                ]
            }
            b'F' => &[(0, 0, 0, 10), (0, 0, 6, 0), (0, 5, 5, 5)],
            b'N' => &[(0, 10, 0, 0), (0, 0, 6, 10), (6, 10, 6, 0)],
            b'P' => {
                &[
                    (0, 10, 0, 0),
                    (0, 0, 4, 0),
                    (4, 0, 6, 2),
                    (6, 2, 6, 4),
                    (6, 4, 4, 5),
                    (4, 5, 0, 5),
                ]
            }
            b'0' => {
                &[
                    (1, 0, 5, 0),
                    (5, 0, 6, 2),
                    (6, 2, 6, 8),
                    (6, 8, 5, 10),
                    (5, 10, 1, 10),
                    (1, 10, 0, 8),
                    (0, 8, 0, 2),
                    (0, 2, 1, 0),
                ]
            }
            b'4' => &[(5, 0, 0, 7), (0, 7, 6, 7), (5, 0, 5, 10)],
            b'E' => &[(0, 0, 0, 10), (0, 0, 6, 0), (0, 5, 5, 5), (0, 10, 6, 10)],
            b'G' => {
                &[
                    (6, 1, 4, 0),
                    (4, 0, 1, 0),
                    (1, 0, 0, 2),
                    (0, 2, 0, 8),
                    (0, 8, 2, 10),
                    (2, 10, 6, 10),
                    (6, 10, 6, 5),
                    (6, 5, 3, 5),
                ]
            }
            b'H' => &[(0, 0, 0, 10), (6, 0, 6, 10), (0, 5, 6, 5)],
            b'I' => &[(1, 0, 5, 0), (3, 0, 3, 10), (1, 10, 5, 10)],
            b'K' => &[(0, 0, 0, 10), (6, 0, 0, 6), (2, 4, 6, 10)],
            b'L' => &[(0, 0, 0, 10), (0, 10, 6, 10)],
            b'M' => &[(0, 10, 0, 0), (0, 0, 3, 5), (3, 5, 6, 0), (6, 0, 6, 10)],
            b'O' => {
                &[
                    (1, 0, 5, 0),
                    (5, 0, 6, 2),
                    (6, 2, 6, 8),
                    (6, 8, 5, 10),
                    (5, 10, 1, 10),
                    (1, 10, 0, 8),
                    (0, 8, 0, 2),
                    (0, 2, 1, 0),
                ]
            }
            b'R' => {
                &[
                    (0, 10, 0, 0),
                    (0, 0, 4, 0),
                    (4, 0, 6, 2),
                    (6, 2, 6, 4),
                    (6, 4, 4, 5),
                    (4, 5, 0, 5),
                    (3, 5, 6, 10),
                ]
            }
            b'S' => {
                &[
                    (6, 0, 1, 0),
                    (1, 0, 0, 2),
                    (0, 2, 0, 4),
                    (0, 4, 5, 6),
                    (5, 6, 6, 7),
                    (6, 7, 6, 9),
                    (6, 9, 5, 10),
                    (5, 10, 0, 10),
                ]
            }
            b'T' => &[(0, 0, 6, 0), (3, 0, 3, 10)],
            b'U' => &[(0, 0, 0, 8), (0, 8, 2, 10), (2, 10, 4, 10), (4, 10, 6, 8), (6, 8, 6, 0)],
            b'W' => &[(0, 0, 1, 10), (1, 10, 3, 5), (3, 5, 5, 10), (5, 10, 6, 0)],
            b'Z' => &[(0, 0, 6, 0), (6, 0, 0, 10), (0, 10, 6, 10)],
            b'?' => {
                &[
                    (0, 1, 2, 0),
                    (2, 0, 5, 0),
                    (5, 0, 6, 2),
                    (6, 2, 3, 5),
                    (3, 5, 3, 6),
                    (3, 9, 3, 10),
                ]
            }
            _ => &[],
        };
        for &(ax, ay, bx, by) in strokes {
            line(
                d,
                (x + ax * height / 10, top + ay * height / 10),
                (x + bx * height / 10, top + by * height / 10),
                (height / 8).max(2) as u32,
                color,
            );
        }
        x += advance;
    }
}

/// Scan-fills a concave five-point star in `color` on `d`, centered at reference
/// (`x`, `y`) with positive bounded `radius` pixels. Uses at most ten crossings per
/// row; no intermediate image or heap allocation is needed.
fn star(
    d: &mut St7789Renderer<'_>,
    x: i32,
    y: i32,
    radius: i32,
    color: Rgb565,
) {
    /// Alternating outer/inner star vertices in hundredths of the requested radius.
    const POINTS: [(i32, i32); 10] = [
        (0, -100),
        (22, -31),
        (95, -31),
        (36, 12),
        (59, 81),
        (0, 38),
        (-59, 81),
        (-36, 12),
        (-95, -31),
        (-22, -31),
    ];
    // Scan-convert the concave outline once per row instead of drawing ten
    // overlapping triangles. At most ten intersections and one row are live.
    let vertices = POINTS.map(|(dx, dy)| (px(d, x + dx * radius / 100), py(d, y + dy * radius / 100)));
    let top = vertices.iter().map(|p| p.1).min().unwrap_or(0);
    let bottom = vertices.iter().map(|p| p.1).max().unwrap_or(0);
    for row in top..=bottom {
        let mut crossings = [0_i32; 10];
        let mut count = 0;
        for i in 0..10 {
            let (ax, ay) = vertices[i];
            let (bx, by) = vertices[(i + 1) % 10];
            if (ay <= row && by > row) || (by <= row && ay > row) {
                crossings[count] = ax + (row - ay) * (bx - ax) / (by - ay);
                count += 1;
            }
        }
        crossings[..count].sort_unstable();
        for pair in crossings[..count].as_chunks::<2>().0 {
            let _ = Rectangle::new(Point::new(pair[0], row), Size::new((pair[1] - pair[0] + 1) as u32, 1))
                .into_styled(PrimitiveStyle::with_fill(color))
                .draw(d);
        }
    }
}
/// Draws the cheat-code headline and progressively filled five stars into `d`
/// at bounded cycle time `t` in `0..2000` milliseconds.
fn aezakmi(
    d: &mut St7789Renderer<'_>,
    t: u32,
) {
    text(d, "WANTED LEVEL: ZERO", 40, STEEL, false);
    headline(d, "AEZAKMI", 66, 46, INK);
    // The red/black stripe and five wanted stars preserve the original welcome identity.
    rect(d, 31, 129, 258, 4, INK);
    rect(d, 31, 122, 189, 4, RED);
    rect(d, 224, 122, 65, 4, MIST);
    for i in 0..5 {
        let x = 52 + i * 54;
        star(d, x, 165, 22, INK);
        star(d, x, 165, 18, if i as u32 <= t / 350 { GOLD } else { PAPER });
    }
    text(d, "CHEAT CODE ACCEPTED", 205, INK, false);
}

/// Draws the turbo volute, rotating seven-blade illustration and airflow into `d`
/// at cycle time `t` in `0..2000` milliseconds; no measured turbo state is used.
fn turbo(
    d: &mut St7789Renderer<'_>,
    t: u32,
) {
    headline(d, "SUTUTU", 27, 29, INK);
    // Tangential outlet, shaded volute and concentric compressor inlet.
    rect(d, 187, 82, 62, 29, INK);
    rect(d, 192, 86, 52, 20, STEEL);
    rect(d, 232, 72, 18, 40, INK);
    rect(d, 236, 75, 10, 34, MIST);
    disk(d, 103, 66, 114, INK);
    disk(d, 106, 69, 108, STEEL);
    disk(d, 111, 74, 98, MIST);
    disk(d, 117, 80, 86, PAPER);
    disk(d, 121, 84, 78, INK);
    disk(d, 125, 88, 70, STEEL);
    /// Sixteen reference-pixel offsets around the illustrated compressor hub.
    const ROTOR: [(i32, i32); 16] = [
        (0, -30),
        (11, -28),
        (21, -21),
        (28, -11),
        (30, 0),
        (28, 11),
        (21, 21),
        (11, 28),
        (0, 30),
        (-11, 28),
        (-21, 21),
        (-28, 11),
        (-30, 0),
        (-28, -11),
        (-21, -21),
        (-11, -28),
    ];
    for blade in 0..7 {
        let n = (blade * 2 + (t / 55) as usize) % 16;
        let a = ROTOR[n];
        let b = ROTOR[(n + 3) % 16];
        tri(
            d,
            (160, 123),
            (160 + a.0, 123 + a.1),
            (160 + b.0 / 2, 123 + b.1 / 2),
            PAPER,
        );
    }
    disk(d, 151, 114, 18, INK);
    disk(d, 155, 118, 10, GOLD);
    for (x, y) in [(117, 87), (191, 97), (183, 157), (120, 153)] {
        disk(d, x - 4, y - 4, 8, INK);
        disk(d, x - 2, y - 2, 4, PAPER);
    }
    line(d, (141, 176), (128, 188), 5, INK);
    line(d, (178, 174), (188, 188), 5, INK);
    line(d, (123, 189), (193, 189), 4, INK);
    for i in 0..3 {
        let offset = wave(t + i as u32 * 180, 1000, 3);
        line(d, (66 - i * 12, 111 - offset), (61 - i * 12, 126), 2, STEEL);
        line(d, (61 - i * 12, 126), (66 - i * 12, 139 + offset), 2, STEEL);
    }
    text(d, "MORE AIR. MORE QUESTIONABLE DECISIONS.", 207, INK, false);
}
/// Draws the blue running character and bounded foot/body motion into `d`
/// at cycle time `t` in `0..2000` milliseconds.
fn sanic(
    d: &mut St7789Renderer<'_>,
    t: u32,
) {
    headline(d, "GOTTA GO", 25, 23, INK);
    text(d, "FAST", 72, BLUE, true);
    let y = wave(t, 400, 3);
    let step = wave(t, 400, 10);
    for i in 0..3 {
        let x = 45 + wave(t + i as u32 * 200, 1000, 9);
        line(d, (x, 107 + i * 21), (x + 32, 107 + i * 21), 2, MIST);
    }
    for (a, b, c) in [
        ((146, 91), (103, 87), (127, 113)),
        ((132, 108), (96, 118), (131, 132)),
        ((131, 131), (110, 151), (148, 143)),
    ] {
        tri(d, (a.0, a.1 + y), (b.0, b.1 + y), (c.0, c.1 + y), INK);
        tri(d, (a.0, a.1 + 3 + y), (b.0 + 8, b.1 + 2 + y), (c.0, c.1 - 3 + y), BLUE);
    }
    disk(d, 125, 84 + y, 77, INK);
    disk(d, 128, 87 + y, 71, BLUE);
    tri(d, (141, 93 + y), (147, 75 + y), (158, 93 + y), INK);
    tri(d, (145, 92 + y), (148, 82 + y), (153, 92 + y), GOLD);
    disk(d, 144, 101 + y, 25, PAPER);
    disk(d, 166, 96 + y, 28, PAPER);
    disk(d, 154, 110 + y, 6, INK);
    disk(d, 172, 104 + y, 7, INK);
    disk(d, 151, 124 + y, 40, GOLD);
    disk(d, 184, 123 + y, 12, INK);
    line(d, (162, 144 + y), (184, 139 + y), 2, INK);
    line(d, (179, 139 + y), (182, 143 + y), 1, INK);
    disk(d, 133, 150 + y, 36, INK);
    disk(d, 136, 152 + y, 30, BLUE);
    disk(d, 144, 156 + y, 17, GOLD);
    line(d, (140, 163 + y), (119, 173 + y), 5, INK);
    disk(d, 110, 168 + y, 14, PAPER);
    line(d, (164, 163 + y), (185, 169 + y), 5, INK);
    disk(d, 182, 164 + y, 14, PAPER);
    line(d, (146, 179 + y), (136 + step, 195), 6, BLUE);
    line(d, (158, 179 + y), (177 - step, 195), 6, BLUE);
    for x in [121 + step, 165 - step] {
        disk(d, x, 188, 21, INK);
        rect(d, x + 9, 190, 24, 18, INK);
        disk(d, x + 2, 190, 17, RED);
        rect(d, x + 11, 192, 19, 13, RED);
        rect(d, x + 9, 192, 6, 13, PAPER);
        line(d, (x + 2, 207), (x + 32, 207), 3, INK);
    }
}
/// Draws BOOST BRO artwork and an unnumbered decorative pressure dial into `d`
/// at cycle time `t` in `0..2000` milliseconds; the needle is not telemetry.
fn boost(
    d: &mut St7789Renderer<'_>,
    t: u32,
) {
    text(d, "HOW MUCH", 37, INK, false);
    headline(d, "BOOST", 55, 45, INK);
    headline(d, "BRO?", 113, 28, RED);
    // A compact unnumbered pressure dial is artwork, never an ECU reading.
    disk(d, 129, 155, 62, INK);
    disk(d, 132, 158, 56, PAPER);
    for (x, y) in [(138, 176), (144, 165), (160, 162), (175, 165), (182, 176)] {
        line(d, (x, y), (160 + (x - 160) * 4 / 5, 186 + (y - 186) * 4 / 5), 2, STEEL);
    }
    let angle = wave(t, 2000, 20);
    line(d, (160, 186), (160 + angle, 164 + angle.abs() / 2), 3, RED);
    disk(d, 157, 183, 6, INK);
    line(d, (36, 184), (112, 184), 2, MIST);
    line(d, (208, 184), (284, 184), 2, MIST);
}
/// Draws the worn hatchback, rotating wheels and moving road dashes into `d`
/// at cycle time `t` in `0..2000` milliseconds.
fn lambo(
    d: &mut St7789Renderer<'_>,
    t: u32,
) {
    headline(d, "WHERE", 28, 27, INK);
    headline(d, "LAMBO?", 67, 34, INK);
    text(d, "BUDGET WENT INTO THE TURBO", 125, RUST, false);
    let y = wave(t, 400, 2);
    // Hatchback silhouette: roof, sloping screen, faded panels and mismatched door.
    tri(d, (76, 163 + y), (104, 137 + y), (160, 163 + y), INK);
    rect(d, 105, 137 + y, 66, 29, INK);
    tri(d, (171, 137 + y), (210, 163 + y), (171, 163 + y), INK);
    rect(d, 61, 161 + y, 195, 30, INK);
    tri(d, (61, 161 + y), (70, 150 + y), (88, 161 + y), INK);
    rect(d, 65, 165 + y, 187, 20, STEEL);
    tri(d, (84, 161 + y), (107, 141 + y), (108, 161 + y), MIST);
    rect(d, 112, 141 + y, 24, 19, MIST);
    tri(d, (142, 141 + y), (167, 141 + y), (194, 161 + y), MIST);
    rect(d, 140, 165 + y, 49, 22, RUST);
    line(d, (136, 162 + y), (136, 184 + y), 1, INK);
    line(d, (193, 165 + y), (193, 183 + y), 1, INK);
    rect(d, 145, 167 + y, 9, 2, INK);
    rect(d, 63, 164 + y, 7, 7, RED);
    rect(d, 242, 164 + y, 11, 6, GOLD);
    rect(d, 57, 180 + y, 15, 6, INK);
    rect(d, 246, 179 + y, 13, 6, INK);
    line(d, (199, 173 + y), (211, 177 + y), 2, INK);
    line(d, (211, 177 + y), (220, 173 + y), 2, INK);
    line(d, (78, 155 + y), (67, 142 + y), 3, INK);
    line(d, (63, 141 + y), (82, 141 + y), 3, RED);
    line(d, (65, 188 + y), (49, 190 + y), 4, RUST);
    for x in [96, 220] {
        disk(d, x - 17, 176, 34, INK);
        disk(d, x - 11, 182, 22, MIST);
        disk(d, x - 6, 187, 12, STEEL);
        let turn = wave(t, 400, 5);
        line(d, (x - 7, 193 - turn), (x + 7, 193 + turn), 2, INK);
        disk(d, x - 2, 191, 4, INK);
    }
    for i in 0..3 {
        let x = 34 + ((t / 12 + i as u32 * 83) % 235) as i32;
        line(d, (x, 214), (x + 16, 214), 1, MIST);
    }
}

/// Draws three independently phased smoke wisps into `d` around bounded reference
/// (`x`, `y`) pixels using cycle time `t` in `0..2000` milliseconds.
fn smoke(
    d: &mut St7789Renderer<'_>,
    x: i32,
    y: i32,
    t: u32,
) {
    // Three offset wisps drift upward; each is bounded independently of time history.
    for i in 0..3 {
        let rise = ((t / 35 + i as u32 * 13) % 37) as i32;
        let drift = wave(t + i as u32 * 190, 1000, 5);
        let sx = x + drift + i * 9 - 9;
        disk(d, sx, y - rise, 12 + i as u32 * 3, MIST);
        disk(d, sx + 4, y - rise - 3, 11 + i as u32 * 3, PAPER);
    }
}
/// Draws the fixed decorative skull into `d` with reference top-left (`x`, `y`) pixels.
fn skull(
    d: &mut St7789Renderer<'_>,
    x: i32,
    y: i32,
) {
    disk(d, x, y, 20, PAPER);
    rect(d, x + 5, y + 13, 10, 11, PAPER);
    disk(d, x + 3, y + 6, 6, INK);
    disk(d, x + 12, y + 6, 5, INK);
    tri(d, (x + 10, y + 11), (x + 7, y + 15), (x + 12, y + 15), INK);
    for dx in [7, 11] {
        line(d, (x + dx, y + 19), (x + dx, y + 23), 1, INK);
    }
}
/// Draws the swinging incense burner, linked chains and smoke into `d`
/// at cycle time `t` in `0..2000` milliseconds.
fn machine_spirit(
    d: &mut St7789Renderer<'_>,
    t: u32,
) {
    headline(d, "BLESS THIS", 24, 26, INK);
    text(d, "MACHINE SPIRIT", 72, RUST, false);
    let swing = wave(t, 2000, 12);
    let cx = 160 + swing;
    disk(d, cx - 39, 121, 78, INK);
    disk(d, cx - 35, 125, 70, GOLD);
    // Occlude the upper hemisphere to leave the bowl silhouette.
    rect(d, cx - 41, 119, 82, 34, PAPER);
    smoke(d, 151 + swing, 136, t);
    // Suspension ring and three individual linked chains, with a swinging cup.
    disk(d, 154, 79, 12, INK);
    disk(d, 157, 82, 6, PAPER);
    for end in [124, 160, 196] {
        for link in 0..9 {
            let x = 160 + (end + swing - 160) * link / 9;
            let y = 90 + link * 7;
            disk(d, x - 2, y, 5, STEEL);
            disk(d, x - 1, y + 1, 3, PAPER);
        }
        line(d, (160, 91), (end + swing, 153), 1, STEEL);
    }
    // The main incense plume stays visible between the suspension chains.
    let drift = wave(t, 1000, 5);
    for (a, b) in [
        ((0, 146), (7, 137)),
        ((7, 137), (5, 128)),
        ((5, 128), (-4, 120)),
        ((-4, 120), (-4, 111)),
        ((-4, 111), (3, 103)),
    ] {
        line(d, (cx + a.0 + drift, a.1), (cx + b.0 + drift, b.1), 3, STEEL);
    }
    rect(d, cx - 42, 151, 84, 7, INK);
    rect(d, cx - 39, 153, 78, 3, GOLD);
    for dx in [-25, -12, 12, 25] {
        line(d, (cx + dx, 164), (cx + dx * 3 / 4, 186), 2, RUST);
    }
    disk(d, cx - 15, 161, 30, INK);
    skull(d, cx - 10, 164);
    rect(d, cx - 16, 195, 32, 4, INK);
    rect(d, cx - 10, 199, 20, 4, GOLD);
    text(d, "HAVE YOU TRIED MORE INCENSE?", 215, INK, false);
}
/// Draws left-aligned 6x10 `value` text in `color` into `d` at reference baseline (`x`, `y`) pixels.
fn label_at(
    d: &mut St7789Renderer<'_>,
    value: &str,
    x: i32,
    y: i32,
    color: Rgb565,
) {
    let _ = Text::new(
        value,
        Point::new(px(d, x), py(d, y)),
        MonoTextStyle::new(&FONT_6X10, color),
    )
    .draw(d);
}
/// Draws the illustrated priest, fictional terminal and blinking artwork lamps into
/// `d` at cycle time `t` in `0..2000` milliseconds; text is unrelated to runtime diagnostics.
fn ritual_diagnostics(
    d: &mut St7789Renderer<'_>,
    t: u32,
) {
    headline(d, "ERROR 404", 24, 25, INK);
    text(d, "APPLY INCENSE", 70, RED, true);
    // A robed machine priest, optical implant and corrugated breather hose.
    tri(d, (116, 91), (69, 192), (157, 192), INK);
    tri(d, (116, 96), (76, 188), (152, 188), RED);
    disk(d, 88, 89, 55, INK);
    disk(d, 92, 92, 47, RED);
    tri(d, (116, 97), (99, 124), (134, 124), INK);
    disk(d, 101, 113, 29, INK);
    disk(d, 105, 117, 21, MIST);
    disk(d, 107, 121, 7, RED);
    disk(d, 119, 122, 5, INK);
    rect(d, 109, 133, 15, 7, INK);
    line(d, (121, 136), (137, 157), 5, STEEL);
    for i in 0..5 {
        line(d, (124 + i * 3, 140 + i * 4), (129 + i * 3, 137 + i * 4), 1, INK);
    }
    for dx in [-15, 0, 16] {
        line(d, (115 + dx / 2, 151), (113 + dx, 186), 2, RUST);
    }
    skull(d, 104, 161);
    // Diagnostic altar: NO START / PRAY? terminal, blinking lamps and incense burner.
    rect(d, 173, 99, 94, 66, INK);
    rect(d, 177, 103, 86, 55, MIST);
    rect(d, 181, 107, 78, 46, INK);
    label_at(d, "NO START", 185, 120, PAPER);
    label_at(d, "PRAY? Y/N", 183, 140, GOLD);
    rect(d, 210, 165, 18, 10, STEEL);
    line(d, (195, 176), (246, 176), 4, INK);
    for i in 0..3 {
        disk(
            d,
            185 + i * 12,
            157,
            4,
            if (t / 250 + i as u32).is_multiple_of(3) {
                RED
            } else {
                STEEL
            },
        );
    }
    line(d, (141, 156), (166, 170), 5, INK);
    disk(d, 161, 165, 12, MIST);
    smoke(d, 255, 191, t);
    disk(d, 247, 180, 22, INK);
    disk(d, 250, 183, 16, GOLD);
    rect(d, 244, 179, 28, 4, INK);
    text(d, "PRAISE THE OMNISSIAH", 215, INK, false);
}
/// Draws the red vehicle, driver, wheel rotation and exhaust flame into `d`
/// at cycle time `t` in `0..2000` milliseconds.
fn red_goes_fasta(
    d: &mut St7789Renderer<'_>,
    t: u32,
) {
    headline(d, "RED GOES", 24, 26, INK);
    headline(d, "FASTA", 66, 34, RED);
    let bounce = wave(t, 400, 2);
    // Exposed engine, riveted red panels, huge tyres and a very confident driver.
    tri(d, (62, 156 + bounce), (89, 141 + bounce), (234, 156 + bounce), INK);
    rect(d, 61, 156 + bounce, 191, 27, INK);
    rect(d, 65, 159 + bounce, 183, 19, RED);
    tri(d, (88, 145 + bounce), (128, 145 + bounce), (144, 159 + bounce), RED);
    rect(d, 172, 141 + bounce, 41, 17, INK);
    rect(d, 176, 144 + bounce, 33, 11, STEEL);
    for i in 0..3 {
        rect(d, 179 + i * 10, 132 + bounce, 6, 12, INK);
        rect(d, 181 + i * 10, 133 + bounce, 2, 8, MIST);
    }
    line(d, (62, 157 + bounce), (46, 132 + bounce), 5, INK);
    line(d, (40, 131 + bounce), (69, 131 + bounce), 5, RED);
    for x in [72, 93, 155, 230, 242] {
        disk(d, x, 163 + bounce, 3, PAPER);
    }
    // Ork face: asymmetric brow, tusks and a broad jaw above the roll cage.
    disk(d, 118, 110 + bounce, 33, INK);
    disk(d, 121, 113 + bounce, 27, Rgb565::new(12, 38, 5));
    rect(d, 120, 129 + bounce, 30, 14, INK);
    rect(d, 123, 129 + bounce, 24, 10, Rgb565::new(12, 38, 5));
    line(d, (124, 121 + bounce), (132, 123 + bounce), 3, INK);
    line(d, (137, 123 + bounce), (145, 119 + bounce), 3, INK);
    disk(d, 128, 123 + bounce, 3, RED);
    disk(d, 139, 122 + bounce, 3, RED);
    tri(d, (127, 139 + bounce), (125, 129 + bounce), (133, 139 + bounce), PAPER);
    tri(d, (138, 139 + bounce), (145, 128 + bounce), (144, 139 + bounce), PAPER);
    line(d, (149, 141 + bounce), (163, 150 + bounce), 5, INK);
    line(d, (154, 148 + bounce), (165, 139 + bounce), 3, STEEL);
    for x in [94, 225] {
        disk(d, x - 20, 168, 40, INK);
        disk(d, x - 12, 176, 24, STEEL);
        disk(d, x - 7, 181, 14, INK);
        let turn = wave(t, 400, 5);
        line(d, (x - 8, 188 - turn), (x + 8, 188 + turn), 3, PAPER);
    }
    let flame = wave(t, 400, 6);
    tri(
        d,
        (60, 175 + bounce),
        (25 - flame, 162 + bounce),
        (42, 177 + bounce),
        RED,
    );
    tri(
        d,
        (60, 175 + bounce),
        (31 - flame, 173 + bounce),
        (48, 183 + bounce),
        GOLD,
    );
    text(d, "MORE DAKKA. LESS TRACTION.", 215, INK, false);
}

/// Draws the robed character and levitating compressor artwork into `d`
/// at cycle time `t` in `0..2000` milliseconds.
fn boost_padawan(
    d: &mut St7789Renderer<'_>,
    t: u32,
) {
    /// RGB565 green character skin and decorative motion marks.
    const SAGE: Rgb565 = Rgb565::new(15, 39, 10);
    /// RGB565 pale robe fabric.
    const ROBE: Rgb565 = Rgb565::new(27, 49, 20);
    headline(d, "MORE BOOST", 25, 26, INK);
    text(d, "YOU NEED, YOUNG PADAWAN.", 72, INK, false);
    // Long ears, wrinkled brow and a mechanic's robe frame the hovering turbo.
    tri(d, (89, 139), (63, 195), (141, 195), INK);
    tri(d, (92, 141), (69, 191), (135, 191), ROBE);
    tri(d, (92, 145), (112, 145), (104, 190), RUST);
    line(d, (91, 146), (102, 170), 3, PAPER);
    line(d, (117, 145), (105, 171), 3, PAPER);
    line(d, (78, 184), (127, 184), 3, INK);
    tri(d, (91, 112), (42, 98), (83, 133), INK);
    tri(d, (88, 115), (51, 103), (83, 128), SAGE);
    tri(d, (115, 112), (165, 98), (124, 133), INK);
    tri(d, (120, 115), (156, 103), (124, 128), SAGE);
    line(d, (56, 106), (78, 117), 2, STEEL);
    line(d, (151, 106), (132, 117), 2, STEEL);
    disk(d, 79, 99, 51, INK);
    disk(d, 82, 102, 45, SAGE);
    line(d, (94, 106), (112, 106), 1, INK);
    line(d, (91, 110), (116, 110), 1, STEEL);
    line(d, (88, 118), (99, 115), 3, INK);
    line(d, (110, 115), (121, 118), 3, INK);
    disk(d, 91, 119, 5, PAPER);
    disk(d, 113, 119, 5, PAPER);
    disk(d, 93, 119, 3, INK);
    disk(d, 113, 119, 3, INK);
    tri(d, (105, 121), (99, 131), (109, 131), STEEL);
    line(d, (95, 137), (113, 137), 2, INK);
    line(d, (99, 141), (109, 141), 1, STEEL);
    // Reaching sleeve and three raised fingers, with the other hand on a cane.
    line(d, (122, 156), (148, 148), 10, INK);
    line(d, (124, 156), (147, 149), 6, ROBE);
    disk(d, 145, 140, 14, SAGE);
    for (a, b) in [
        ((149, 145), (147, 135)),
        ((153, 144), (155, 133)),
        ((156, 147), (163, 140)),
    ] {
        line(d, a, b, 3, SAGE);
    }
    line(d, (73, 164), (61, 170), 6, ROBE);
    line(d, (61, 165), (58, 197), 4, RUST);
    disk(d, 56, 164, 9, SAGE);
    // No telemetry: this levitating compressor is an illustrated Force gag.
    let lift = wave(t, 2000, 6);
    rect(d, 246, 103 + lift, 25, 16, INK);
    rect(d, 249, 107 + lift, 18, 8, STEEL);
    disk(d, 195, 95 + lift, 67, INK);
    disk(d, 198, 98 + lift, 61, MIST);
    disk(d, 203, 103 + lift, 51, STEEL);
    disk(d, 208, 108 + lift, 41, INK);
    for (a, b) in [
        ((0, -17), (10, -4)),
        ((17, 0), (4, 10)),
        ((0, 17), (-10, 4)),
        ((-17, 0), (-4, -10)),
    ] {
        tri(
            d,
            (228, 128 + lift),
            (228 + a.0, 128 + lift + a.1),
            (228 + b.0, 128 + lift + b.1),
            PAPER,
        );
    }
    disk(d, 223, 123 + lift, 10, GOLD);
    for i in 0..3 {
        let spread = i * 5 + wave(t + i as u32 * 200, 1000, 2);
        line(d, (208 - spread, 176 + i * 5), (247 + spread, 176 + i * 5), 1, MIST);
    }
    line(d, (173, 135 + lift), (184, 130 + lift), 2, SAGE);
    line(d, (179, 149 + lift), (189, 145 + lift), 2, SAGE);
    text(d, "THE SPOOL IS STRONG WITH THIS ONE", 215, INK, false);
}

/// Draws uneven character line art, moving drool and a small spinning turbo into
/// `d` at cycle time `t` in `0..2000` milliseconds.
fn turbo_brainlet(
    d: &mut St7789Renderer<'_>,
    t: u32,
) {
    headline(d, "OOGA BOOGA", 26, 30, INK);
    // Deliberately uneven line art: the low concave forehead, mismatched eyes,
    // grin and drool follow the owner's supplied brainlet reference.
    for path in [
        &[
            (30, 195),
            (48, 193),
            (60, 178),
            (68, 158),
            (73, 140),
            (75, 117),
            (81, 91),
            (93, 96),
            (105, 107),
            (114, 119),
            (128, 123),
            (141, 121),
            (157, 114),
            (173, 98),
            (182, 88),
            (189, 87),
            (192, 93),
            (191, 112),
            (187, 129),
            (180, 144),
            (177, 158),
            (169, 172),
            (154, 179),
            (137, 181),
            (122, 177),
            (108, 166),
            (95, 151),
            (86, 137),
            (81, 115),
            (81, 94),
        ][..],
        &[(137, 181), (138, 190), (149, 194), (171, 194), (183, 196)][..],
        &[(85, 143), (86, 158), (82, 167)][..],
        &[(72, 177), (64, 181), (59, 189)][..],
        &[(116, 188), (121, 193), (128, 195)][..],
        &[(95, 150), (111, 161), (126, 170), (143, 175), (154, 175)][..],
    ] {
        for segment in path.windows(2) {
            line(d, segment[0], segment[1], 2, INK);
        }
    }
    // One eye sits low and wide, the other high by the temple.
    for path in [
        &[
            (94, 126),
            (111, 127),
            (119, 132),
            (116, 137),
            (99, 137),
            (94, 133),
            (94, 126),
        ][..],
        &[(165, 107), (180, 104), (180, 114), (165, 115), (165, 107)][..],
    ] {
        for segment in path.windows(2) {
            line(d, segment[0], segment[1], 2, INK);
        }
    }
    disk(d, 102, 127, 10, INK);
    disk(d, 173, 105, 9, INK);
    line(d, (103, 128), (103, 130), 1, PAPER);
    line(d, (174, 106), (174, 108), 1, PAPER);
    line(d, (145, 124), (141, 125), 2, INK);
    line(d, (141, 125), (142, 129), 2, INK);
    for (a, b) in [
        ((134, 143), (145, 145)),
        ((145, 145), (158, 142)),
        ((158, 142), (170, 135)),
    ] {
        line(d, a, b, 6, INK);
    }
    let drip = wave(t, 2000, 2);
    line(d, (133, 147), (132, 157 + drip), 2, CYAN);
    line(d, (132, 157 + drip), (134, 164 + drip), 2, CYAN);
    line(d, (139, 148), (140, 158 + drip), 2, CYAN);
    // "Smol snail" is the turbo itself: spinning compressor and tangential outlet.
    // Its motion is bounded by the common scene clock.
    let bob = wave(t, 400, 2);
    rect(d, 242, 130 + bob, 20, 11, INK);
    rect(d, 245, 133 + bob, 14, 5, STEEL);
    disk(d, 205, 126 + bob, 48, INK);
    disk(d, 208, 129 + bob, 42, MIST);
    disk(d, 213, 134 + bob, 32, INK);
    /// Eight reference-pixel blade endpoints around the small compressor hub.
    const DIRECTIONS: [(i32, i32); 8] = [(0, -13), (9, -9), (13, 0), (9, 9), (0, 13), (-9, 9), (-13, 0), (-9, -9)];
    for blade in 0..4 {
        let n = (blade * 2 + (t / 85) as usize) % 8;
        let a = DIRECTIONS[n];
        let b = DIRECTIONS[(n + 1) % 8];
        tri(
            d,
            (229, 150 + bob),
            (229 + a.0, 150 + bob + a.1),
            (229 + b.0 / 2, 150 + bob + b.1 / 2),
            PAPER,
        );
    }
    disk(d, 226, 147 + bob, 6, GOLD);
    for i in 0..3 {
        let trail = wave(t + i as u32 * 200, 1000, 4);
        line(
            d,
            (278, 139 + i * 10 + bob),
            (287 + trail, 136 + i * 10 + bob),
            1,
            STEEL,
        );
    }
    text(d, "SMOL SNAIL GO WOOSH", 215, INK, false);
}
