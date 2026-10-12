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
