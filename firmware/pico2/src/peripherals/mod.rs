//! Core0 input-peripheral tasks and bounded publication queues.
/// Core0 encoder polling, bounded event publication and maintenance parking.
pub mod encoder;

pub use encoder::{ENCODER_EVENTS, encoder_task};
