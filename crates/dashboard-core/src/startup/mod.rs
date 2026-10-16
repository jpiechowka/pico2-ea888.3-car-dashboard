//! Startup presentation, independent of acquisition and transfer ownership.
mod scene;
mod state;
pub use dashboard_config::startup::*;
pub use scene::{StartupFacts, draw};
pub use state::{Exit, Phase, Startup};
