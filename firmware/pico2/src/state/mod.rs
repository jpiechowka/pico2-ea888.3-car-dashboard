//! State management for the dashboard.
//!
//! - `sensor_state`: Sensor history, trends, peak hold, rolling average
//! - `pages`: Page navigation enum (Dashboard, Debug, Logs)
//! - `button`: Button debounce handling
//! - `popup`: Popup state management
//! - `input`: Button input processing and action dispatch

mod button;
mod input;
mod pages;
mod popup;
mod sensor_state;

pub use button::ButtonState;
pub use input::process_buttons;
pub use pages::Page;
pub use popup::Popup;
pub use sensor_state::{GRAPH_HISTORY_SIZE, SensorState};
