//! Active colors, independent of the display board's active-low GPIO wiring.
use crate::dashboard::Overlay;
use crate::display::pipeline::FrameToken;

/// Track the one in-flight request separately from the completed scene while
/// rendering the other buffer. No shared GPIO owner or extra task is required.
#[derive(Default)]
pub struct PresentedLed {
    pending: Option<(FrameToken, Option<Overlay>)>,
    visible: Option<Overlay>,
}

impl PresentedLed {
    pub const fn new() -> Self {
        Self {
            pending: None,
            visible: None,
        }
    }

    pub fn submit(
        &mut self,
        token: FrameToken,
        overlay: Option<Overlay>,
    ) -> bool {
        if self.pending.is_some() {
            return false;
        }
        self.pending = Some((token, overlay));
        true
    }

    pub fn complete(
        &mut self,
        token: FrameToken,
    ) -> bool {
        if let Some((expected, overlay)) = self.pending
            && expected == token
        {
            self.visible = overlay;
            self.pending = None;
            true
        } else {
            false
        }
    }

    pub fn unchanged(
        &mut self,
        overlay: Option<Overlay>,
    ) -> bool {
        if self.pending.is_some() {
            return false;
        }
        self.visible = overlay;
        true
    }

    pub fn output(
        &self,
        backlight_on: bool,
    ) -> StatusLed {
        StatusLed::for_overlay(self.visible, backlight_on)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusLed {
    Off,
    Green,
    Red,
}

impl StatusLed {
    pub fn for_overlay(
        visible: Option<Overlay>,
        backlight_on: bool,
    ) -> Self {
        if backlight_on && let Some(Overlay::Critical(_, phase)) = visible {
            if phase { Self::Red } else { Self::Off }
        } else {
            Self::Green
        }
    }
}
