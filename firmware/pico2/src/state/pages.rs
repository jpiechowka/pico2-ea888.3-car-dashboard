#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Page {
    #[default]
    Dashboard,

    Debug,

    Logs,
}

impl Page {
    #[inline]
    pub const fn toggle(self) -> Self {
        match self {
            Self::Dashboard => Self::Debug,
            Self::Debug => Self::Logs,
            Self::Logs => Self::Dashboard,
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
        assert_eq!(Page::Debug.toggle(), Page::Logs);
        assert_eq!(Page::Logs.toggle(), Page::Dashboard);
    }

    #[test]
    fn test_page_toggle_cycle() {
        let page = Page::Dashboard;
        let page = page.toggle();
        let page = page.toggle();
        let page = page.toggle();
        assert_eq!(page, Page::Dashboard);
    }
}
