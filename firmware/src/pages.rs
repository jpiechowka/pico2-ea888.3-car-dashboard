//! Page navigation for multi-screen dashboard.
//!
//! Supports switching between the main sensor dashboard and a debug/profiling view.
//! Press `Y` button to toggle between pages.
//!
//! # Pages
//!
//! - [`Page::Dashboard`]: Main 4x2 sensor grid (boost, AFR, battery, coolant, oil, DSG, IAT, EGT)
//! - [`Page::Debug`]: Profiling metrics, frame timing, and debug log terminal

/// Available pages in the dashboard application.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Page {
    /// Main sensor dashboard with 4x2 cell grid.
    /// Shows: Boost, AFR, Battery, Coolant (row 1), Oil, DSG, IAT, EGT (row 2)
    #[default]
    Dashboard,

    /// Debug/profiling page with system metrics.
    /// Shows: Frame timing, render stats, memory info, debug log terminal
    Debug,
}

impl Page {
    /// Toggle to the next page (cycles between Dashboard and Debug).
    #[inline]
    pub const fn toggle(self) -> Self {
        match self {
            Self::Dashboard => Self::Debug,
            Self::Debug => Self::Dashboard,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_page_default() {
        assert_eq!(Page::default(), Page::Dashboard);
    }

    #[test]
    fn test_page_toggle() {
        assert_eq!(Page::Dashboard.toggle(), Page::Debug);
        assert_eq!(Page::Debug.toggle(), Page::Dashboard);
    }

    #[test]
    fn test_page_toggle_cycle() {
        let page = Page::Dashboard;
        let page = page.toggle(); // -> Debug
        let page = page.toggle(); // -> Dashboard
        assert_eq!(page, Page::Dashboard);
    }
}
