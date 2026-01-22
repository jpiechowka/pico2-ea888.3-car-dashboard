// Crate-level lints: Allow common embedded/graphics patterns that pedantic lints flag
#![allow(clippy::cast_possible_truncation)] // Intentional f32->i32, u32->i32 casts for pixel math
#![allow(clippy::cast_precision_loss)] // u32/i32->f32 in graphics calculations
#![allow(clippy::cast_possible_wrap)] // u32->i32 wrapping is acceptable for our value ranges
#![allow(clippy::cast_sign_loss)] // i32->u32 where we know sign is positive
#![allow(clippy::too_many_lines)] // main() is long but well-structured
#![allow(clippy::struct_excessive_bools)] // RenderState uses bools appropriately
#![allow(clippy::similar_names)] // bg_0, bg_60 etc in tests are clear
#![allow(clippy::if_not_else)] // if-else style preference
#![allow(clippy::useless_let_if_seq)] // let mut + if is clearer for state initialization

//! OBD-II Dashboard Simulator for Raspberry Pi Pico 2 (RP2350).
//!
//! This application simulates an automotive OBD-II dashboard displaying:
//! - Boost pressure (bar/PSI)
//! - Air-Fuel Ratio (AFR)
//! - Intake Air Temperature (IAT)
//! - Battery voltage
//! - Oil temperature
//! - DSG transmission temperature
//! - Exhaust Gas Temperature (EGT)
//! - Coolant temperature
//!
//! # Optimization Summary
//!
//! This codebase has been optimized for embedded deployment on RP2350 with ST7789 display.
//! Below is a comprehensive summary of optimizations: what works, what doesn't, and why.
//!
//! ## ✅ WORKING OPTIMIZATIONS
//!
//! ### 1. Heapless Strings (Significant Impact)
//! **Status:** Implemented and working well.
//! - All `format!()` calls replaced with `heapless::String<N>` + `core::fmt::Write`
//! - Eliminates heap allocations, essential for `no_std` targets
//! - Fixed-size stack buffers (16-64 bytes) are sufficient for all values
//! - **Location:** All cell drawing functions, header FPS display, loading screen
//!
//! ### 2. Pre-computed Layout Constants (Significant Impact)
//! **Status:** Implemented and working well.
//! - `COL_WIDTH`, `ROW_HEIGHT`, `CENTER_X`, `CENTER_Y` computed at compile time
//! - Eliminates per-frame division and arithmetic operations
//! - **Location:** [`config`] module, used throughout rendering code
//!
//! ### 3. Static Text Styles (Moderate Impact)
//! **Status:** Implemented and working well.
//! - `MonoTextStyle` and `TextStyle` defined as `const` in [`styles`] module
//! - `LABEL_STYLE_WHITE`, `VALUE_STYLE_WHITE`, `CENTERED`, etc.
//! - Avoids repeated struct construction every frame
//! - Dynamic colors still require runtime style creation (unavoidable)
//! - **Location:** [`styles`] module, imported by all widget functions
//!
//! ### 4. Pre-computed Positions (Moderate Impact)
//! **Status:** Implemented and working well.
//! - Fixed UI positions as `const Point` values
//! - Header title, FPS counter, divider endpoints, popup geometry
//! - **Location:** [`widgets::header`], [`widgets::popups`]
//!
//! ### 5. Const `PrimitiveStyle` (Moderate Impact)
//! **Status:** Implemented and working well.
//! - `PrimitiveStyle::with_fill()` and `with_stroke()` are const fn in embedded-graphics 0.8
//! - Header fill, divider stroke, popup borders all compile-time constants
//! - **Location:** [`widgets::header`], [`widgets::popups`]
//!
//! ### 6. Build Optimizations (Significant Impact)
//! **Status:** Implemented and working well.
//! - LTO (Link-Time Optimization) enabled for release builds
//! - Single codegen unit for better optimization opportunities
//! - `opt-level = 3` for maximum performance
//! - **Location:** `Cargo.toml` `[profile.release]` section
//!
//! ### 7. Color Transitions with Fixed-Point Math (Working)
//! **Status:** Implemented and working well.
//! - RGB565 interpolation uses integer math with 8-bit fixed-point
//! - Avoids floating-point division in the hot path
//! - **Location:** [`animations::lerp_rgb565`]
//!
//! ### 8. Per-Character Rainbow Animation (Minor Impact)
//! **Status:** Implemented and working well.
//! - Welcome screen uses per-character rainbow coloring with const array lookup
//! - 12-color extended palette stored as const array (no runtime construction)
//! - Simple modulo arithmetic for color indexing (no floating-point)
//! - Stack-allocated UTF-8 buffer for single-character rendering
//! - **Location:** [`screens::welcome::RAINBOW_COLORS`], [`screens::welcome::draw_rainbow_text`]
//!
//! ## ⚠️ PARTIALLY WORKING / LIMITED BENEFIT
//!
//! ### 9. Dirty Rectangle Tracking (Limited Benefit for This Use Case)
//! **Status:** Implemented but provides limited benefit.
//! - **What works:**
//!   - Header only redraws when FPS changes or after popup closes
//!   - Dividers draw once and only redraw after popup closes
//!   - Popup cleanup properly triggers full redraw
//! - **What doesn't help much:**
//!   - Cell backgrounds must redraw every frame anyway (values animate continuously)
//!   - The tracking overhead may not be worth it when most content changes each frame
//! - **Verdict:** Useful for popup cleanup, but doesn't reduce per-frame work significantly because sensor values
//!   animate continuously requiring full cell redraws.
//! - **Location:** [`render::RenderState`]
//!
//! ### 10. Cell Dirty Tracking (Removed After Refactoring)
//! **Status:** Removed - cells always redraw, so dirty tracking was wasteful.
//! - Originally tracked `cell_dirty` flags and `redraw_bg` parameter
//! - Cells always need full redraw (values animate continuously)
//! - Simplified to track only background colors for transition system
//! - **Refactoring:** Removed `redraw_bg` parameter from all cell functions
//! - **Location:** [`render::RenderState`] (simplified)
//!
//! ## ❌ NOT IMPLEMENTED / REJECTED
//!
//! ### 11. Partial Cell Updates (Rejected)
//! **Why not implemented:**
//! - Would require tracking previous text positions and lengths
//! - Text rendering doesn't support efficient partial clearing
//! - Complexity not worth it for continuously animating values
//! - Full cell clear + redraw is simpler and fast enough
//!
//! ### 12. Double Buffering (Analyzed - Not Implementing)
//! **What it is:** Maintain two framebuffers; draw to one while displaying the other.
//! **Why not implemented:**
//! - **Simulator:** `embedded-graphics-simulator` handles this internally via SDL2
//! - **Hardware RAM cost:** 320×240×2 bytes × 2 buffers = 307 KB (~59% of RP2350's 520 KB)
//! - **ST7789 has internal GRAM:** Display controller handles refresh, implicit single-buffering
//! - **Better alternative:** DMA + double line buffer (1.28 KB) for hardware deployment
//! - **Verdict:** Not worth the RAM cost; display's internal GRAM provides sufficient buffering
//!
//! ### 13. Sprite/Tile Caching (Analyzed - Not Implementing)
//! **What it is:** Pre-render static graphics to bitmaps, blit instead of re-rasterize.
//! **Why not implemented:**
//! - **Dynamic content:** Sensor values change every frame (`1.85`, `1.86`...)
//! - **embedded-graphics:** No built-in sprite system; would need custom implementation
//! - **RAM cost:** `ProFont` 24pt glyphs ~1.2 KB each; caching digits 0-9 = ~12 KB
//! - **Limited benefit:** SPI transfer time dominates, not CPU rendering time
//! - **Where it could help:** Static labels ("OIL", "IAT", "EGT", "COOL") but these are ~5% of work
//! - **Verdict:** Complexity outweighs benefit; bottleneck is SPI, not text rasterization
//!
//! ### 14. DMA Transfers (Hardware-Specific - Future Work)
//! **What it is:** Use RP2350's DMA controller for parallel SPI transfers.
//! **Why not implemented yet:**
//! - Requires actual hardware (RP2350 + SPI display)
//! - Simulator mode doesn't benefit from DMA
//! - Would be implemented in display driver layer, not application code
//! - **For production:** DMA + double line buffer is the recommended approach
//!
//! ## 📊 PERFORMANCE CHARACTERISTICS
//!
//! | Component | Update Frequency | Optimization Applied |
//! |-----------|-----------------|---------------------|
//! | Header | On FPS change / popup close / page switch | Conditional redraw |
//! | Dividers | Once / after popup / after page switch | Draw-once tracking |
//! | Cell backgrounds | Every frame | Always redraw (required) |
//! | Cell values | Every frame | Heapless strings |
//! | Popups | On show/hide | Full clear on close |
//! | Color transitions | Every frame | Fixed-point interpolation |
//!
//! # Visual Effects
//!
//! The dashboard includes animated visual feedback (see [`animations`]):
//!
//! ## Shake Effect
//! When a sensor enters a critical state (high temp, low voltage), the cell content
//! wiggles horizontally using sine-wave oscillation. This draws immediate attention
//! without being overly distracting.
//!
//! ## Blink Effect
//! Critical states cause the background to alternate between the warning color and
//! black at ~4Hz (toggling every ~6 frames at 50 FPS). This fast blink rate is
//! attention-grabbing for dangerous conditions like overheating or low voltage.
//!
//! ## Color Transitions
//! Background colors smoothly interpolate when crossing thresholds instead of
//! snapping instantly. Uses RGB565 linear interpolation with fixed-point math.
//! Transition speed is configurable via `COLOR_LERP_SPEED` constant. The transition
//! colors are passed to cell drawing via `bg_override` parameter.
//!
//! # Controls (Simulator Mode)
//!
//! | Button | Key | Action |
//! |--------|-----|--------|
//! | X | `X` | Toggle FPS display on/off |
//! | Y | `Y` | Switch between Dashboard and Debug page |
//! | A | `A` | Toggle boost unit (bar ↔ PSI) |
//! | B | `B` | Reset min/max/avg values |
//!
//! Key repeat is ignored to prevent toggle spam when holding keys.
//!
//! # Architecture
//!
//! ```text
//! ┌───────────────────────────────────────────────┐
//! │              HEADER (OBD Sim)                  │  26px
//! ├─────────┬─────────┬─────────┬─────────────────┤
//! │  BOOST  │   AFR   │  BATT   │      COOL       │
//! │  REL    │ /LAMBDA │         │                 │  107px
//! ├─────────┼─────────┼─────────┼─────────────────┤
//! │   OIL   │   DSG   │   IAT   │      EGT        │
//! │         │         │         │                 │  107px
//! └─────────┴─────────┴─────────┴─────────────────┘
//!   80px      80px      80px         80px
//! ```

mod animations;
mod colors;
mod config;
mod pages;
mod profiling;
mod render;
mod screens;
mod state;
mod styles;
mod thresholds;
mod widgets;

use std::thread;
use std::time::Instant;

use animations::{ColorTransition, calculate_shake_offset};
use colors::{BLACK, ORANGE, RED};
// Optimization: Import pre-computed layout constants instead of calculating per-frame
use config::{COL_WIDTH, FRAME_TIME, HEADER_HEIGHT, ROW_HEIGHT, SCREEN_HEIGHT, SCREEN_WIDTH};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics_simulator::sdl2::Keycode;
use embedded_graphics_simulator::{OutputSettingsBuilder, SimulatorDisplay, SimulatorEvent, Window};
use pages::Page;
use profiling::{DebugLog, ProfilingMetrics};
use render::{Popup, RenderState, cell_idx};
use screens::{draw_debug_page, run_loading_screen, run_welcome_screen};
use state::SensorState;
use thresholds::{BAR_TO_PSI, BATT_CRITICAL, BATT_WARNING, BOOST_EASTER_EGG_BAR, BOOST_EASTER_EGG_PSI};
use widgets::{
    draw_afr_cell,
    draw_batt_cell,
    draw_boost_cell,
    draw_boost_unit_popup,
    draw_dividers,
    draw_fps_toggle_popup,
    draw_header,
    draw_reset_popup,
    draw_temp_cell,
    is_critical_afr,
    is_critical_egt,
    is_critical_iat,
    is_critical_oil_dsg,
    is_critical_water,
    temp_color_egt,
    temp_color_iat,
    temp_color_oil_dsg,
    temp_color_water,
};

fn main() {
    // Initialize display and window (simulator mode)
    let mut display: SimulatorDisplay<Rgb565> = SimulatorDisplay::new(Size::new(SCREEN_WIDTH, SCREEN_HEIGHT));
    let output_settings = OutputSettingsBuilder::new().scale(2).build();
    let mut window = Window::new("Leon Cupra OBD Sim", &output_settings);

    // Initial clear before boot sequence
    display.clear(BLACK).ok();
    window.update(&display);

    // Run boot sequence screens (loading animation, welcome message)
    // Returns false if user closes window during boot
    if !run_loading_screen(&mut display, &mut window) {
        return;
    }
    if !run_welcome_screen(&mut display, &mut window) {
        return;
    }

    // ==========================================================================
    // Main Loop State
    // ==========================================================================

    // Signal generation time parameter (advances each frame)
    let mut t = 0.0f32;
    // Frame counter for blink timing (wraps to avoid overflow)
    let mut frame_count = 0u32;

    // Min/Max tracking for each sensor
    // Boost tracks max in both units separately (as per user request)
    let mut boost_max_bar = 0.0f32;
    let mut boost_max_psi = 0.0f32;
    let mut oil_temp_max = 0.0f32;
    let mut water_temp_max = 0.0f32;
    let mut dsg_temp_max = 0.0f32;
    let mut iat_temp_max = f32::MIN; // IAT can go negative, start at MIN
    let mut egt_temp_max = 0.0f32;
    let mut batt_min = f32::MAX; // Start at MAX so first reading becomes minimum
    let mut batt_max = 0.0f32;

    // Sensor states track history for trend arrows, peak detection, and graphs
    let mut oil_state = SensorState::new();
    let mut water_state = SensorState::new();
    let mut dsg_state = SensorState::new();
    let mut iat_state = SensorState::new();
    let mut egt_state = SensorState::new();
    let mut batt_state = SensorState::new();
    let mut afr_state = SensorState::new();

    // Active popup (only one at a time, encapsulates kind + start time)
    let mut active_popup: Option<Popup> = None;

    // FPS counter state (X button toggles)
    let mut show_fps = true;
    let mut last_fps_calc = Instant::now();
    let mut fps_frame_count = 0u32;
    let mut current_fps = 0.0f32;

    // Boost unit toggle state (A button toggles bar <-> PSI)
    let mut show_boost_psi = false;

    // Easter egg: "Fast AF Boi!" appears when boost hits 2.0 bar (or ~29 PSI)
    // Every 3rd boost cycle peaks at 2.0 instead of 1.8
    let mut boost_cycle_count = 0u32;
    let mut boost_was_low = true;

    // Dirty rectangle tracking for selective redraw optimization
    let mut render_state = RenderState::new();

    // Smooth color transition state for cell backgrounds
    let mut color_transition = ColorTransition::new();

    // Page navigation state (Dashboard is default, Y button toggles to Debug)
    let mut current_page = Page::default();
    let mut page_just_switched = false;

    // Reset request flag (deferred until after sensor values are calculated)
    let mut reset_requested = false;

    // Profiling metrics and debug log
    let mut metrics = ProfilingMetrics::new();
    let mut debug_log = DebugLog::new();
    debug_log.push("System started");

    // ==========================================================================
    // Main Render Loop
    // ==========================================================================

    loop {
        let frame_start = Instant::now();

        // Handle window events (close, button presses)
        // Button mapping (matches physical display buttons):
        //   X - Toggle FPS display
        //   Y - Switch page (Dashboard <-> Debug)
        //   A - Toggle boost unit (bar <-> PSI)
        //   B - Reset min/max values
        for ev in window.events() {
            match ev {
                SimulatorEvent::Quit => return,
                SimulatorEvent::KeyDown { keycode, repeat, .. } => {
                    // Ignore OS key repeat to prevent toggle spam when holding keys
                    if repeat {
                        continue;
                    }
                    match keycode {
                        // X button: Toggle FPS display (only on Dashboard page)
                        Keycode::X if current_page == Page::Dashboard => {
                            show_fps = !show_fps;
                            active_popup = Some(Popup::Fps(Instant::now()));
                            debug_log.push(if show_fps { "FPS: ON" } else { "FPS: OFF" });
                        }
                        // Y button: Switch page (works on any page)
                        Keycode::Y => {
                            current_page = current_page.toggle();
                            page_just_switched = true;
                            active_popup = None; // Cancel popup when switching pages
                            debug_log.push(match current_page {
                                Page::Dashboard => "Page: Dashboard",
                                Page::Debug => "Page: Debug",
                            });
                        }
                        // A button: Toggle boost unit (only on Dashboard page)
                        Keycode::A if current_page == Page::Dashboard => {
                            show_boost_psi = !show_boost_psi;
                            active_popup = Some(Popup::BoostUnit(Instant::now()));
                            debug_log.push(if show_boost_psi { "Boost: PSI" } else { "Boost: BAR" });
                        }
                        // B button: Reset min/max values (only on Dashboard page)
                        Keycode::B if current_page == Page::Dashboard => {
                            reset_requested = true;
                            active_popup = Some(Popup::Reset(Instant::now()));
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }

        // Check if popup has expired BEFORE updating render state
        // This ensures cleanup happens in the same frame the popup disappears
        if let Some(ref popup) = active_popup
            && popup.is_expired()
        {
            active_popup = None;
        }

        // Track popup state for dirty rectangle optimization
        render_state.update_popup(active_popup.as_ref());

        // Clear display on first frame, when popup just closed, or when page switched
        // When popup closes, its remnants (especially white border) need to be cleared
        // When page switches, need to clear the previous page's content
        if render_state.is_first_frame() || render_state.popup_just_closed() || page_just_switched {
            display.clear(BLACK).ok();
            // Mark display cleared so header/dividers redraw when returning to Dashboard
            if page_just_switched {
                render_state.mark_display_cleared();
            }
        }

        // ======================================================================
        // Generate Fake Sensor Data (simulator mode)
        // ======================================================================

        // Boost: normally peaks at 1.8 bar, but every 3rd cycle hits 2.0 for easter egg
        let boost_max_target = if boost_cycle_count % 3 == 2 { 2.0 } else { 1.8 };
        let boost = boost_signal(t, 0.0, boost_max_target, 0.08);

        // Track boost cycles for easter egg timing
        if boost < 0.3 {
            boost_was_low = true;
        } else if boost_was_low && boost > 1.5 {
            boost_was_low = false;
            boost_cycle_count = boost_cycle_count.wrapping_add(1);
        }

        // Calculate boost in PSI for display and easter egg check
        let boost_psi = boost * BAR_TO_PSI;

        // Easter egg triggers based on current display unit threshold
        let boost_easter_egg_active = if show_boost_psi {
            boost_psi >= BOOST_EASTER_EGG_PSI
        } else {
            boost >= BOOST_EASTER_EGG_BAR
        };

        // Generate other sensor values with different frequencies for visual variety
        let oil_temp = fake_signal(t, 30.0, 115.0, 0.08);
        let water_temp = fake_signal(t, 30.0, 95.0, 0.10);
        let dsg_temp = fake_signal(t, 30.0, 115.0, 0.07);
        // IAT: Intake Air Temperature (-10°C to 70°C range to show all color states)
        let iat_temp = fake_signal(t, -10.0, 70.0, 0.05);
        // EGT: Exhaust Gas Temperature (200°C to 900°C range to show all color states)
        let egt_temp = fake_signal(t, 200.0, 900.0, 0.04);
        let batt_voltage = fake_signal(t, 10.0, 15.0, 0.06);
        let afr = fake_signal(t, 10.0, 18.0, 0.09);

        // ======================================================================
        // Handle Deferred Reset (after sensor values calculated)
        // ======================================================================
        //
        // Reset is deferred from B button press to here so we can initialize
        // min/max to current sensor values. This prevents immediate peak
        // detection on the first frame after reset.

        if reset_requested {
            // Reset rolling averages, graph histories, and peak highlights
            oil_state.reset_average();
            oil_state.reset_graph();
            oil_state.reset_peak();
            water_state.reset_average();
            water_state.reset_graph();
            water_state.reset_peak();
            dsg_state.reset_average();
            dsg_state.reset_graph();
            dsg_state.reset_peak();
            iat_state.reset_average();
            iat_state.reset_graph();
            iat_state.reset_peak();
            egt_state.reset_average();
            egt_state.reset_graph();
            egt_state.reset_peak();
            batt_state.reset_average();
            batt_state.reset_graph();
            batt_state.reset_peak();
            afr_state.reset_graph();

            // Initialize min/max to current values (not defaults) to prevent
            // immediate peak detection on the next comparison
            boost_max_bar = boost;
            boost_max_psi = boost_psi;
            oil_temp_max = oil_temp;
            water_temp_max = water_temp;
            dsg_temp_max = dsg_temp;
            iat_temp_max = iat_temp;
            egt_temp_max = egt_temp;
            batt_min = batt_voltage;
            batt_max = batt_voltage;

            debug_log.push("MIN/AVG/MAX Reset");
            reset_requested = false;
        }

        // ======================================================================
        // Update Min/Max Tracking
        // ======================================================================

        let oil_max_updated = oil_temp > oil_temp_max;
        let water_max_updated = water_temp > water_temp_max;
        let dsg_max_updated = dsg_temp > dsg_temp_max;
        let iat_max_updated = iat_temp > iat_temp_max;
        let egt_max_updated = egt_temp > egt_temp_max;
        let batt_max_updated = batt_voltage > batt_max || batt_voltage < batt_min;

        // Track boost max in both units separately
        boost_max_bar = boost_max_bar.max(boost);
        boost_max_psi = boost_max_psi.max(boost_psi);
        oil_temp_max = oil_temp_max.max(oil_temp);
        water_temp_max = water_temp_max.max(water_temp);
        dsg_temp_max = dsg_temp_max.max(dsg_temp);
        iat_temp_max = iat_temp_max.max(iat_temp);
        egt_temp_max = egt_temp_max.max(egt_temp);
        batt_min = batt_min.min(batt_voltage);
        batt_max = batt_max.max(batt_voltage);

        // Update sensor states (history for trends, peak hold timing)
        oil_state.update(oil_temp, oil_max_updated);
        water_state.update(water_temp, water_max_updated);
        dsg_state.update(dsg_temp, dsg_max_updated);
        iat_state.update(iat_temp, iat_max_updated);
        egt_state.update(egt_temp, egt_max_updated);
        batt_state.update(batt_voltage, batt_max_updated);
        afr_state.update(afr, false); // AFR doesn't track max, just history for graph

        // Track peaks detected for debug metrics
        metrics.peaks_detected += u32::from(oil_max_updated)
            + u32::from(water_max_updated)
            + u32::from(dsg_max_updated)
            + u32::from(iat_max_updated)
            + u32::from(egt_max_updated)
            + u32::from(batt_max_updated);

        // ======================================================================
        // FPS Calculation (updated once per second)
        // ======================================================================

        fps_frame_count += 1;
        if last_fps_calc.elapsed().as_secs() >= 1 {
            current_fps = fps_frame_count as f32 / last_fps_calc.elapsed().as_secs_f32();
            fps_frame_count = 0;
            last_fps_calc = Instant::now();
        }

        // ======================================================================
        // Page-Based Rendering
        // ======================================================================

        match current_page {
            Page::Dashboard => {
                // ==============================================================
                // Dashboard Page: Sensor cells with header
                // ==============================================================

                // Header bar with title and optional FPS display
                // Redraw if FPS changed, first frame, or popup just closed
                if render_state.check_header_dirty(show_fps, current_fps) {
                    draw_header(&mut display, show_fps, current_fps);
                    metrics.inc_header_redraws();
                }

                // Blink state for critical value warnings (~4Hz toggle rate at 50 FPS)
                // frame_count / 6 toggles every ~0.12s = ~4.17 Hz
                let blink_on = (frame_count / 6).is_multiple_of(2);

                // ==============================================================
                // Update Color Transitions (smooth background fades)
                // ==============================================================

                // Get target colors for each cell based on current sensor values
                let (oil_target_bg, _) = temp_color_oil_dsg(oil_temp);
                let (coolant_target_bg, _) = temp_color_water(water_temp);
                let (dsg_target_bg, _) = temp_color_oil_dsg(dsg_temp);
                let (iat_target_bg, _) = temp_color_iat(iat_temp);
                let (egt_target_bg, _) = temp_color_egt(egt_temp);
                let batt_target_bg = if batt_voltage < BATT_CRITICAL {
                    RED
                } else if batt_voltage < BATT_WARNING {
                    ORANGE
                } else {
                    BLACK
                };

                // Set transition targets and update transitions
                color_transition.set_target(cell_idx::OIL, oil_target_bg);
                color_transition.set_target(cell_idx::COOLANT, coolant_target_bg);
                color_transition.set_target(cell_idx::DSG, dsg_target_bg);
                color_transition.set_target(cell_idx::IAT, iat_target_bg);
                color_transition.set_target(cell_idx::EGT, egt_target_bg);
                color_transition.set_target(cell_idx::BATTERY, batt_target_bg);

                // Advance all color transitions for this frame
                let changed_cells = color_transition.update();
                metrics.color_transitions += changed_cells.count_ones();

                // ==============================================================
                // Calculate Shake Offsets for Critical States
                // ==============================================================

                let oil_shake = calculate_shake_offset(frame_count, is_critical_oil_dsg(oil_temp));
                let coolant_shake = calculate_shake_offset(frame_count, is_critical_water(water_temp));
                let dsg_shake = calculate_shake_offset(frame_count, is_critical_oil_dsg(dsg_temp));
                let iat_shake = calculate_shake_offset(frame_count, is_critical_iat(iat_temp));
                let egt_shake = calculate_shake_offset(frame_count, is_critical_egt(egt_temp));
                let batt_shake = calculate_shake_offset(frame_count, batt_voltage < BATT_CRITICAL);

                // Row 1 (top): Boost, AFR, Battery, Coolant
                // Select max value based on current display unit
                let boost_max_display = if show_boost_psi { boost_max_psi } else { boost_max_bar };
                draw_boost_cell(
                    &mut display,
                    0,
                    HEADER_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    boost,
                    boost_max_display,
                    show_boost_psi,
                    boost_easter_egg_active,
                    blink_on,
                    0, // Boost doesn't have critical state shake
                );

                // AFR cell - LEAN AF triggers shake and blink animation, includes mini-graph
                let afr_shake = calculate_shake_offset(frame_count, is_critical_afr(afr));
                draw_afr_cell(
                    &mut display,
                    COL_WIDTH,
                    HEADER_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    afr,
                    &afr_state,
                    blink_on,
                    afr_shake,
                    None, // AFR doesn't use smooth color transitions
                );

                // Battery cell - uses smooth color transition, mini-graph, and shake when critical
                draw_batt_cell(
                    &mut display,
                    COL_WIDTH * 2,
                    HEADER_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    batt_voltage,
                    batt_min,
                    batt_max,
                    &batt_state,
                    blink_on,
                    batt_shake,
                    Some(color_transition.get_current(cell_idx::BATTERY)),
                );

                // Coolant temp cell - uses smooth color transition, mini-graph, and shake when critical
                draw_temp_cell(
                    &mut display,
                    COL_WIDTH * 3,
                    HEADER_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    "COOL",
                    water_temp,
                    water_temp_max,
                    &water_state,
                    temp_color_water,
                    is_critical_water,
                    blink_on,
                    coolant_shake,
                    Some(color_transition.get_current(cell_idx::COOLANT)),
                );

                // Row 2 (bottom): Oil, DSG, IAT, EGT
                // Oil temp cell - uses smooth color transition, mini-graph, and shake when critical
                draw_temp_cell(
                    &mut display,
                    0,
                    HEADER_HEIGHT + ROW_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    "OIL",
                    oil_temp,
                    oil_temp_max,
                    &oil_state,
                    temp_color_oil_dsg,
                    is_critical_oil_dsg,
                    blink_on,
                    oil_shake,
                    Some(color_transition.get_current(cell_idx::OIL)),
                );

                // DSG temp cell - uses smooth color transition, mini-graph, and shake when critical
                draw_temp_cell(
                    &mut display,
                    COL_WIDTH,
                    HEADER_HEIGHT + ROW_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    "DSG",
                    dsg_temp,
                    dsg_temp_max,
                    &dsg_state,
                    temp_color_oil_dsg,
                    is_critical_oil_dsg,
                    blink_on,
                    dsg_shake,
                    Some(color_transition.get_current(cell_idx::DSG)),
                );

                // IAT cell - uses smooth color transition, mini-graph, and shake when critical
                draw_temp_cell(
                    &mut display,
                    COL_WIDTH * 2,
                    HEADER_HEIGHT + ROW_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    "IAT",
                    iat_temp,
                    iat_temp_max,
                    &iat_state,
                    temp_color_iat,
                    is_critical_iat,
                    blink_on,
                    iat_shake,
                    Some(color_transition.get_current(cell_idx::IAT)),
                );

                // EGT cell - uses smooth color transition, mini-graph, and shake when critical
                draw_temp_cell(
                    &mut display,
                    COL_WIDTH * 3,
                    HEADER_HEIGHT + ROW_HEIGHT,
                    COL_WIDTH,
                    ROW_HEIGHT,
                    "EGT",
                    egt_temp,
                    egt_temp_max,
                    &egt_state,
                    temp_color_egt,
                    is_critical_egt,
                    blink_on,
                    egt_shake,
                    Some(color_transition.get_current(cell_idx::EGT)),
                );

                // Divider lines between cells (draw once, redraw after popup closes)
                if render_state.need_dividers() {
                    draw_dividers(&mut display);
                    render_state.mark_dividers_drawn();
                    metrics.inc_divider_redraws();
                }

                // ==============================================================
                // Draw Popups (only one at a time, most recent wins)
                // ==============================================================

                // Draw popup if active - must be drawn AFTER cells to appear on top
                // Expiration is already handled at frame start
                if let Some(ref popup) = active_popup {
                    match popup {
                        Popup::Reset(_) => draw_reset_popup(&mut display),
                        Popup::Fps(_) => draw_fps_toggle_popup(&mut display, show_fps),
                        Popup::BoostUnit(_) => draw_boost_unit_popup(&mut display, show_boost_psi),
                    }
                }

                // Track cell draws for this frame (8 cells)
                metrics.inc_cell_draws(8);
            }

            Page::Debug => {
                // ==============================================================
                // Debug Page: Profiling metrics and debug log terminal
                // ==============================================================
                draw_debug_page(&mut display, &metrics, &debug_log, current_fps);
            }
        }

        // ======================================================================
        // Frame Timing and Profiling
        // ======================================================================

        let render_time = frame_start.elapsed();

        // End of frame - reset per-frame state
        render_state.end_frame();
        page_just_switched = false;

        // Update window with rendered frame
        window.update(&display);

        // Advance signal time and frame counter
        t += 0.05;
        frame_count = frame_count.wrapping_add(1);

        // Sleep to maintain target frame rate (~50 FPS)
        let pre_sleep = frame_start.elapsed();
        if pre_sleep < FRAME_TIME {
            thread::sleep(FRAME_TIME.checked_sub(pre_sleep).unwrap());
        }
        let sleep_time = frame_start.elapsed().checked_sub(pre_sleep).unwrap();

        // Record frame metrics for profiling
        metrics.record_frame(frame_start.elapsed(), render_time, sleep_time);
    }
}

/// Generate a sinusoidal signal oscillating between min and max values.
///
/// Used to simulate sensor readings in demo mode.
///
/// # Parameters
/// - `t`: Time parameter (advances each frame)
/// - `min`: Minimum output value
/// - `max`: Maximum output value
/// - `freq`: Oscillation frequency (higher = faster cycles)
fn fake_signal(
    t: f32,
    min: f32,
    max: f32,
    freq: f32,
) -> f32 {
    let normalized = (t * freq).sin().mul_add(0.5, 0.5);
    min + normalized * (max - min)
}

/// Generate a boost signal that holds at peak for longer.
///
/// Similar to `fake_signal` but holds at maximum value for ~11% of the cycle.
/// This makes the easter egg ("Fast AF Boi!") visible for longer when boost
/// hits 2.0 bar.
///
/// The signal holds at peak when the cycle phase is between 1.2 and 1.9 radians
/// (around PI/2 where sine normally peaks). That's 0.7 radians out of 2π ≈ 11%.
fn boost_signal(
    t: f32,
    min: f32,
    max: f32,
    freq: f32,
) -> f32 {
    let cycle = (t * freq) % std::f32::consts::TAU;
    // Hold at peak for ~11% of the cycle (0.7 / 2π)
    let normalized = if cycle > 1.2 && cycle < 1.9 {
        1.0 // Hold at max
    } else {
        (cycle).sin().mul_add(0.5, 0.5)
    };
    min + normalized * (max - min)
}
