use dashboard_pico2::display::FrameBuffer;
use embassy_rp::spi::Error;
use embassy_time::Instant;

use super::{INIT_MESSAGES, MAX_VISIBLE_LINES, draw_loading_frame, draw_welcome_frame};
use crate::drivers::{St7789Flusher, St7789Renderer};

const WELCOME_DURATION_MS: u64 = 7000;

const READY_PAUSE_MS: u64 = 500;

pub async fn run_boot_sequence(
    flusher: &mut St7789Flusher<'_>,
    buffer: &mut FrameBuffer,
) -> Result<(), Error> {
    {
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
                draw_loading_frame(&mut St7789Renderer::new(buffer), &visible_lines, line_count, elapsed_ms);
                flusher.flush_buffer(buffer, || {}).await?;

                if msg_start.elapsed().as_millis() >= *duration_ms {
                    break;
                }
            }
        }

        let pause_start = Instant::now();
        loop {
            let elapsed_ms = boot_start.elapsed().as_millis() as u32;
            draw_loading_frame(&mut St7789Renderer::new(buffer), &visible_lines, line_count, elapsed_ms);
            flusher.flush_buffer(buffer, || {}).await?;

            if pause_start.elapsed().as_millis() >= READY_PAUSE_MS {
                break;
            }
        }
    }

    {
        let start = Instant::now();

        loop {
            let elapsed_ms = start.elapsed().as_millis() as u32;
            if u64::from(elapsed_ms) >= WELCOME_DURATION_MS {
                break;
            }

            draw_welcome_frame(&mut St7789Renderer::new(buffer), elapsed_ms);
            flusher.flush_buffer(buffer, || {}).await?;
        }
    }
    Ok(())
}
