//! OBD-II Dashboard Firmware for Raspberry Pi Pico 2 (RP2350)
//!
//! RGB LED blink example using Embassy async runtime and defmt logging.
//! Uses the Pimoroni PIM715 Display Pack 2.8" RGB LED on GPIO 26, 27, 28.

#![no_std]
#![no_main]

use defmt::info;
use embassy_executor::Spawner;
use embassy_rp::gpio::{Level, Output};
use embassy_time::Timer;
use {defmt_rtt as _, panic_probe as _};

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    info!("OBD-II Dashboard starting...");

    let p = embassy_rp::init(Default::default());

    // Pimoroni PIM715 Display Pack 2.8" RGB LED
    // Red = GPIO26, Green = GPIO27, Blue = GPIO28
    // LEDs are active-low (Low = ON, High = OFF)
    let mut led_r = Output::new(p.PIN_26, Level::High);
    let mut led_g = Output::new(p.PIN_27, Level::High);
    let mut led_b = Output::new(p.PIN_28, Level::High);

    info!("Initialized! Cycling RGB LED colors...");

    loop {
        // Red
        led_r.set_low();
        led_g.set_high();
        led_b.set_high();
        info!("RED");
        Timer::after_millis(500).await;

        // Green
        led_r.set_high();
        led_g.set_low();
        led_b.set_high();
        info!("GREEN");
        Timer::after_millis(500).await;

        // Blue
        led_r.set_high();
        led_g.set_high();
        led_b.set_low();
        info!("BLUE");
        Timer::after_millis(500).await;

        // Yellow (R+G)
        led_r.set_low();
        led_g.set_low();
        led_b.set_high();
        info!("YELLOW");
        Timer::after_millis(500).await;

        // Cyan (G+B)
        led_r.set_high();
        led_g.set_low();
        led_b.set_low();
        info!("CYAN");
        Timer::after_millis(500).await;

        // Magenta (R+B)
        led_r.set_low();
        led_g.set_high();
        led_b.set_low();
        info!("MAGENTA");
        Timer::after_millis(500).await;

        // White (R+G+B)
        led_r.set_low();
        led_g.set_low();
        led_b.set_low();
        info!("WHITE");
        Timer::after_millis(500).await;

        // Off
        led_r.set_high();
        led_g.set_high();
        led_b.set_high();
        info!("OFF");
        Timer::after_millis(500).await;
    }
}
