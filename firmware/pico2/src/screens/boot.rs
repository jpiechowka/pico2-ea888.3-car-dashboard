//! Boot sequence screens (loading + welcome).
//!
//! Displays the boot sequence before entering the main dashboard loop.
//!
//! # Boot Sequence
//!
//! 1. **Loading Screen** (~6 seconds) - Console-style initialization messages displayed sequentially with animated
//!    spinner.
//!
//! 2. **Welcome Screen** (7 seconds) - AEZAKMI logo with time-based star animation. Stars fill over 4 seconds, then
//!    blink slowly for 3 seconds.
//!
//! # Usage
//!
//! ```ignore
//! run_boot_sequence(&mut flusher, &mut double_buffer).await;
//! ```

use embassy_time::Instant;
use embedded_graphics::prelude::*;

use super::{INIT_MESSAGES, MAX_VISIBLE_LINES, draw_loading_frame, draw_welcome_frame};
use crate::drivers::{DoubleBuffer, St7789Flusher, St7789Renderer};
use crate::ui::BLACK;

/// Duration of the welcome screen in milliseconds.
const WELCOME_DURATION_MS: u64 = 7000;

/// Final pause duration after "Ready." message in milliseconds.
const READY_PAUSE_MS: u64 = 500;

/// Run the complete boot sequence (loading screen + welcome screen).
///
/// This function renders both boot screens using single-buffer mode for simplicity.
/// Each frame is rendered and then flushed to the display before proceeding to the next.
///
/// # Arguments
///
/// * `flusher` - The ST7789 display flusher for DMA transfers
/// * `double_buffer` - The double buffer for framebuffer management
pub async fn run_boot_sequence(
    flusher: &mut St7789Flusher<'_>,
    double_buffer: &mut DoubleBuffer,
) {
    // --- Loading Screen ---
    // Display console-style initialization messages sequentially with delays.
    // Renders continuously during each message's wait period so the spinner animates.
    {
        let buffer = unsafe { double_buffer.render_buffer() };
        let mut renderer = St7789Renderer::new(buffer);

        // Track visible lines (console scrolling effect)
        let mut visible_lines: [&str; MAX_VISIBLE_LINES] = [""; MAX_VISIBLE_LINES];
        let mut line_count: usize = 0;
        let boot_start = Instant::now();

        for (msg, duration_ms) in &INIT_MESSAGES {
            // Add message to visible lines
            if line_count < MAX_VISIBLE_LINES {
                visible_lines[line_count] = msg;
                line_count += 1;
            } else {
                // Shift lines up (scroll effect)
                for i in 0..MAX_VISIBLE_LINES - 1 {
                    visible_lines[i] = visible_lines[i + 1];
                }
                visible_lines[MAX_VISIBLE_LINES - 1] = msg;
            }

            // Render continuously during message wait so the spinner animates
            let msg_start = Instant::now();
            loop {
                let elapsed_ms = boot_start.elapsed().as_millis() as u32;
                draw_loading_frame(&mut renderer, &visible_lines, line_count, elapsed_ms);
                flusher.flush_buffer(unsafe { double_buffer.get_buffer(0) }).await;

                if msg_start.elapsed().as_millis() >= *duration_ms {
                    break;
                }
            }
        }

        // Final pause after "Ready." with spinning spinner
        let pause_start = Instant::now();
        loop {
            let elapsed_ms = boot_start.elapsed().as_millis() as u32;
            draw_loading_frame(&mut renderer, &visible_lines, line_count, elapsed_ms);
            flusher.flush_buffer(unsafe { double_buffer.get_buffer(0) }).await;

            if pause_start.elapsed().as_millis() >= READY_PAUSE_MS {
                break;
            }
        }
    }

    // --- Welcome Screen ---
    // Display AEZAKMI logo with animated blinking stars.
    // Time-based animation: 4 seconds star filling + 3 seconds blinking = 7 seconds total.
    {
        let buffer = unsafe { double_buffer.render_buffer() };
        let mut renderer = St7789Renderer::new(buffer);

        let start = Instant::now();

        loop {
            let elapsed_ms = start.elapsed().as_millis() as u32;
            if u64::from(elapsed_ms) >= WELCOME_DURATION_MS {
                break;
            }

            draw_welcome_frame(&mut renderer, elapsed_ms);
            flusher.flush_buffer(unsafe { double_buffer.get_buffer(0) }).await;
        }
    }
}

/// Clear both framebuffers to prevent grainy noise on startup.
///
/// Should be called before the boot sequence to ensure clean display.
pub async fn clear_framebuffers(
    flusher: &mut St7789Flusher<'_>,
    double_buffer: &mut DoubleBuffer,
) {
    // Clear buffer 0
    {
        let buffer = unsafe { double_buffer.render_buffer() };
        St7789Renderer::new(buffer).clear(BLACK).ok();
    }
    flusher.flush_buffer(unsafe { double_buffer.get_buffer(0) }).await;
    double_buffer.swap();

    // Clear buffer 1
    {
        let buffer = unsafe { double_buffer.render_buffer() };
        St7789Renderer::new(buffer).clear(BLACK).ok();
    }
    flusher.flush_buffer(unsafe { double_buffer.get_buffer(1) }).await;
    double_buffer.swap(); // Back to buffer 0
}
