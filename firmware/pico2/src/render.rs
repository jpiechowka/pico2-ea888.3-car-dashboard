#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum FpsMode {
    #[default]
    Off,
    Instant,
    Average,
    Combined,
}

impl FpsMode {
    pub const fn next(self) -> Self {
        match self {
            Self::Off => Self::Instant,
            Self::Instant => Self::Average,
            Self::Average => Self::Combined,
            Self::Combined => Self::Off,
        }
    }

    pub const fn is_visible(self) -> bool { !matches!(self, Self::Off) }

    pub const fn needs_both_fps(self) -> bool { matches!(self, Self::Combined) }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "FPS OFF",
            Self::Instant => "FPS: INST",
            Self::Average => "FPS: AVG",
            Self::Combined => "FPS: BOTH",
        }
    }

    pub const fn suffix(self) -> &'static str {
        match self {
            Self::Off => "",
            Self::Instant => " FPS",
            Self::Average => " AVG",
            Self::Combined => " AVG",
        }
    }
}

use micromath::F32Ext;

pub const CELL_COUNT: usize = 8;

pub mod cell_idx {
    pub const AFR: usize = 1;
    pub const BATTERY: usize = 2;
    pub const COOLANT: usize = 3;
    pub const OIL: usize = 4;
    pub const DSG: usize = 5;
    pub const IAT: usize = 6;
    pub const EGT: usize = 7;
}

pub struct RenderState {
    dividers_drawn: bool,
    prev_fps_mode: FpsMode,
    prev_fps_instant_rounded: u32,
    prev_fps_average_rounded: u32,
    prev_popup_kind: Option<u8>,
    popup_just_closed: bool,
    first_frame: bool,
    display_cleared: bool,
}

impl RenderState {
    pub const fn new() -> Self {
        Self {
            dividers_drawn: false,
            prev_fps_mode: FpsMode::Off,
            prev_fps_instant_rounded: 0,
            prev_fps_average_rounded: 0,
            prev_popup_kind: None,
            popup_just_closed: false,
            first_frame: true,
            display_cleared: false,
        }
    }

    #[inline]
    pub const fn need_dividers(&self) -> bool { !self.dividers_drawn || self.first_frame || self.display_cleared }

    #[inline]
    pub fn mark_dividers_drawn(&mut self) { self.dividers_drawn = true; }

    pub fn check_header_dirty(
        &mut self,
        fps_mode: FpsMode,
        fps_instant: f32,
        fps_average: f32,
    ) -> bool {
        let instant_rounded = fps_instant.round() as u32;
        let average_rounded = fps_average.round() as u32;

        let fps_changed = match fps_mode {
            FpsMode::Off => false,
            FpsMode::Instant => instant_rounded != self.prev_fps_instant_rounded,
            FpsMode::Average => average_rounded != self.prev_fps_average_rounded,
            FpsMode::Combined => {
                instant_rounded != self.prev_fps_instant_rounded || average_rounded != self.prev_fps_average_rounded
            }
        };

        let dirty = self.first_frame
            || self.popup_just_closed
            || self.display_cleared
            || fps_mode != self.prev_fps_mode
            || fps_changed;

        self.prev_fps_mode = fps_mode;
        self.prev_fps_instant_rounded = instant_rounded;
        self.prev_fps_average_rounded = average_rounded;
        dirty
    }

    pub fn update_popup(
        &mut self,
        popup_kind: Option<u8>,
    ) {
        let changed = popup_kind != self.prev_popup_kind;
        let was_visible = self.prev_popup_kind.is_some();
        self.prev_popup_kind = popup_kind;

        if changed && was_visible {
            self.popup_just_closed = true;
            self.dividers_drawn = false;
        }
    }

    #[inline]
    pub const fn popup_just_closed(&self) -> bool { self.popup_just_closed }

    #[inline]
    pub const fn is_first_frame(&self) -> bool { self.first_frame }

    pub fn mark_display_cleared(&mut self) {
        self.display_cleared = true;
        self.dividers_drawn = false;
    }

    pub fn end_frame(&mut self) {
        self.first_frame = false;
        self.popup_just_closed = false;
        self.display_cleared = false;
    }
}

impl Default for RenderState {
    fn default() -> Self { Self::new() }
}
