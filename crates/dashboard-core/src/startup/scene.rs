//! Five bounded procedural scenes. Every position is selected directly from time.
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::{FONT_6X10, FONT_10X20};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{Circle, Line, PrimitiveStyle, Rectangle, Triangle};
use embedded_graphics::text::{Alignment, Text};

use super::{Phase, StartupProfile, StartupStatus, StartupTheme};
use crate::display::St7789Renderer;
use crate::vehicle_status::VehicleStatus;

const INK: Rgb565 = Rgb565::new(1, 3, 5);
const WHITE: Rgb565 = Rgb565::new(29, 60, 28);
const GOLD: Rgb565 = Rgb565::new(31, 43, 5);
const CYAN: Rgb565 = Rgb565::new(3, 53, 28);
const BLUE: Rgb565 = Rgb565::new(3, 19, 31);
const RED: Rgb565 = Rgb565::new(29, 7, 5);

#[derive(Clone, Copy)]
pub struct StartupFacts {
    pub vehicle: VehicleStatus,
    pub config_ok: bool,
    pub settings_loaded: bool,
    pub configured_sources: u8,
}
fn rect(
    d: &mut St7789Renderer<'_>,
    x: i32,
    y: i32,
    w: u32,
    h: u32,
    c: Rgb565,
) {
    let _ = Rectangle::new(Point::new(px(d, x), py(d, y)), Size::new(if portrait(d) { w * 3 / 4 } else { w }, h))
        .into_styled(PrimitiveStyle::with_fill(c))
        .draw(d);
}
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
fn disk(
    d: &mut St7789Renderer<'_>,
    x: i32,
    y: i32,
    diameter: u32,
    c: Rgb565,
) {
    let _ = Circle::new(Point::new(px(d, x), py(d, y)), if portrait(d) { diameter * 3 / 4 } else { diameter })
        .into_styled(PrimitiveStyle::with_fill(c))
        .draw(d);
}
fn tri(
    d: &mut St7789Renderer<'_>,
    a: (i32, i32),
    b: (i32, i32),
    c: (i32, i32),
    color: Rgb565,
) {
    let _ = Triangle::new(Point::new(px(d, a.0), py(d, a.1)), Point::new(px(d, b.0), py(d, b.1)), Point::new(px(d, c.0), py(d, c.1)))
        .into_styled(PrimitiveStyle::with_fill(color))
        .draw(d);
}
fn text(
    d: &mut St7789Renderer<'_>,
    value: &str,
    y: i32,
    color: Rgb565,
    big: bool,
) {
    let font = if big { &FONT_10X20 } else { &FONT_6X10 };
    let _ = Text::with_alignment(
        value,
        Point::new(i32::from(d.geometry().width()) / 2, py(d, y)),
        MonoTextStyle::new(font, color),
        Alignment::Center,
    )
    .draw(d);
}
fn wave(
    t: u32,
    period: u32,
    amplitude: i32,
) -> i32 {
    let p = (t % period) as i32;
    let half = (period / 2) as i32;
    (if p < half { p } else { period as i32 - p }) * amplitude * 2 / half - amplitude
}

pub fn draw(
    d: &mut St7789Renderer<'_>,
    phase: Phase,
    profile: StartupProfile,
    elapsed: u64,
    reduced_motion: bool,
    facts: StartupFacts,
) {
    d.clear_buffer(INK);
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
    // Quiet frame geometry leaves a consistent reading area and footer.
    rect(d, 20, 19, 30, 3, GOLD);
    rect(d, 270, 19, 30, 3, GOLD);
    match profile.theme {
        StartupTheme::Custom => {
            text(d, "CUSTOM ARTWORK UNAVAILABLE", 120, WHITE, false);
        }
        StartupTheme::Aezakmi => {
            text(d, "WANTED: ABSOLUTELY NOBODY", 58, WHITE, false);
            let shift = wave(t, 2000, 7);
            rect(d, 29, 84 + shift, 262, 51, GOLD);
            text(d, "AEZAKMI", 117 + shift, INK, true);
            for i in 0..6 {
                let x = 58 + i * 40;
                let c = if (t / 200 + i as u32) % 6 < 3 { GOLD } else { WHITE };
                line(d, (x - 7, 163), (x + 7, 163), 2, c);
                line(d, (x, 156), (x, 170), 2, c);
                line(d, (x - 5, 158), (x + 5, 168), 1, c);
                line(d, (x - 5, 168), (x + 5, 158), 1, c);
            }
            text(d, "CHEAT CODE ACCEPTED", 196, WHITE, false);
        }
        StartupTheme::Turbo => {
            text(d, "SUTUTU", 52, WHITE, true);
            disk(d, 105, 70, 110, CYAN);
            disk(d, 114, 79, 92, INK);
            const DIRECTIONS: [(i32, i32); 8] = [
                (0, -36),
                (25, -25),
                (36, 0),
                (25, 25),
                (0, 36),
                (-25, 25),
                (-36, 0),
                (-25, -25),
            ];
            for blade in 0..8 {
                let n = (blade + (t / 70) as usize) % 8;
                let (x, y) = DIRECTIONS[n];
                let (xx, yy) = DIRECTIONS[(n + 1) % 8];
                tri(
                    d,
                    (160, 125),
                    (160 + x, 125 + y),
                    (160 + xx / 2, 125 + yy / 2),
                    if blade % 2 == 0 { WHITE } else { BLUE },
                );
            }
            disk(d, 153, 118, 14, GOLD);
            rect(d, 207, 91, 48, 15, CYAN);
            rect(d, 239, 75, 16, 30, CYAN);
            for i in 0..5 {
                rect(
                    d,
                    53 + i * 44,
                    193,
                    29,
                    (5 + ((t / 110 + i as u32) % 4) * 3).min(14),
                    GOLD,
                );
            }
        }
        StartupTheme::Sanic => {
            text(d, "GOTTA GO FAST", 50, WHITE, true);
            let bounce = wave(t, 400, 4);
            for i in 0..6 {
                let x = ((t / 4 + i as u32 * 51) % 300) as i32;
                line(d, (x, 85 + i * 18), (x + 22, 85 + i * 18), 2, BLUE);
            }
            tri(d, (165, 80 + bounce), (109, 85 + bounce), (148, 108 + bounce), BLUE);
            tri(d, (159, 100 + bounce), (103, 113 + bounce), (146, 131 + bounce), BLUE);
            disk(d, 139, 78 + bounce, 62, BLUE);
            disk(d, 159, 97 + bounce, 34, WHITE);
            disk(d, 179, 102 + bounce, 9, INK);
            tri(d, (187, 117 + bounce), (209, 121 + bounce), (187, 128 + bounce), GOLD);
            disk(d, 143, 131 + bounce, 42, BLUE);
            let stride = wave(t, 300, 12);
            line(d, (158, 164 + bounce), (147 + stride, 185), 7, WHITE);
            line(d, (170, 164 + bounce), (180 - stride, 186), 7, WHITE);
            rect(d, 133 + stride, 183, 31, 10, RED);
            rect(d, 169 - stride, 184, 31, 10, RED);
        }
        StartupTheme::BoostBro => {
            text(d, "HOW MUCH", 65, WHITE, true);
            let pulse = wave(t, 800, 5);
            rect(d, 31, 87 - pulse, 258, (48 + pulse * 2) as u32, GOLD);
            text(d, "BOOST, BRO?", 119, INK, true);
            for i in 0..12 {
                let h = 8 + ((t / 90 + i as u32) % 10) * 3;
                rect(d, 40 + i * 20, 190 - h as i32, 12, h, if i < 8 { CYAN } else { GOLD });
            }
            text(d, "JUST ONE MORE PULL", 210, WHITE, false);
        }
        StartupTheme::WhereLambo => {
            text(d, "WHERE LAMBO?", 58, GOLD, true);
            text(d, "BUDGET WENT INTO THE TURBO", 80, WHITE, false);
            let y = wave(t, 400, 3);
            line(d, (20, 186), (300, 186), 2, WHITE);
            for i in 0..4 {
                let x = ((t / 5 + i as u32 * 90) % 360) as i32 - 30;
                rect(d, x, 203, 35, 3, GOLD);
            }
            rect(d, 78, 131 + y, 169, 35, BLUE);
            tri(d, (104, 132 + y), (123, 103 + y), (203, 132 + y), BLUE);
            rect(d, 122, 103 + y, 59, 30, BLUE);
            rect(d, 126, 109 + y, 23, 20, CYAN);
            tri(d, (156, 109 + y), (177, 109 + y), (195, 129 + y), CYAN);
            rect(d, 68, 151 + y, 18, 10, WHITE);
            rect(d, 239, 142 + y, 12, 9, GOLD);
            line(d, (186, 137 + y), (177, 153 + y), 2, GOLD);
            line(d, (177, 153 + y), (197, 148 + y), 2, GOLD);
            rect(d, 153, 138 + y, 9, 3, WHITE);
            for x in [103, 211] {
                disk(d, x - 14, 153 + y, 29, INK);
                disk(d, x - 9, 158 + y, 19, WHITE);
                line(d, (x - 6, 167 + y), (x + 6, 167 + y + wave(t, 300, 5)), 2, INK);
            }
            line(d, (83, 134 + y), (71, 120 + y), 3, RED);
        }
    }
    text(d, "TAP TO SKIP  /  HOLD Y TO RECOVER", 231, WHITE, false);
}

fn portrait(d: &St7789Renderer<'_>) -> bool { d.geometry() == dashboard_config::Geometry::Portrait }
fn px(d: &St7789Renderer<'_>, x: i32) -> i32 { if portrait(d) { x * 3 / 4 } else { x } }
fn py(d: &St7789Renderer<'_>, y: i32) -> i32 { if portrait(d) { y + 30 } else { y } }
