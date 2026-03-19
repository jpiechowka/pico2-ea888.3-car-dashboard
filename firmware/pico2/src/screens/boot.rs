use embassy_time::Instant;
use embedded_graphics::prelude::*;

use super::{INIT_MESSAGES, MAX_VISIBLE_LINES, draw_loading_frame, draw_welcome_frame};
use crate::drivers::{DoubleBuffer, St7789Flusher, St7789Renderer};
use crate::ui::BLACK;

const WELCOME_DURATION_MS: u64 = 7000;

const READY_PAUSE_MS: u64 = 500;

pub async fn run_boot_sequence(
    flusher: &mut St7789Flusher<'_>,
    double_buffer: &mut DoubleBuffer,
) {
    {
        let buffer = unsafe { double_buffer.render_buffer() };
        let mut renderer = St7789Renderer::new(buffer);

        let mut visible_lines: [&str; MAX_VISIBLE_LINES] = [""; MAX_VISIBLE_LINES];
        let mut line_count: usize = 0;
        let boot_start = Instant::now();

        for (msg, duration_ms) in &INIT_MESSAGES {
            if line_count < MAX_VISIBLE_LINES {
                visible_lines[line_count] = msg;
                line_count += 1;
            } else {
                for i in 0..MAX_VISIBLE_LINES - 1 {
                    visible_lines[i] = visible_lines[i + 1];
                }
                visible_lines[MAX_VISIBLE_LINES - 1] = msg;
            }

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

pub async fn clear_framebuffers(
    flusher: &mut St7789Flusher<'_>,
    double_buffer: &mut DoubleBuffer,
) {
    {
        let buffer = unsafe { double_buffer.render_buffer() };
        St7789Renderer::new(buffer).clear(BLACK).ok();
    }
    flusher.flush_buffer(unsafe { double_buffer.get_buffer(0) }).await;
    double_buffer.swap();

    {
        let buffer = unsafe { double_buffer.render_buffer() };
        St7789Renderer::new(buffer).clear(BLACK).ok();
    }
    flusher.flush_buffer(unsafe { double_buffer.get_buffer(1) }).await;
    double_buffer.swap();
}
