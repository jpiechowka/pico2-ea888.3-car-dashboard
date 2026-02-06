//! State management for the dashboard.
//!
//! - `sensor_state`: Sensor history, trends, peak hold, rolling average
//! - `pages`: Page navigation enum (Dashboard, Debug, Logs)
//! - `button`: Button debounce handling
//! - `popup`: Popup state management

mod button;
mod pages;
mod popup;
mod sensor_state;

pub use button::ButtonState;
pub use pages::Page;
pub use popup::Popup;
pub use sensor_state::{GRAPH_HISTORY_SIZE, SensorState};
