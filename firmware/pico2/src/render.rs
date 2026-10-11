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

use core::fmt::Write;

use heapless::String;

pub type FpsText = String<16>;

/// The cache and renderer use identical visible text, including tie rounding.
pub fn format_fps(
    mode: FpsMode,
    instant: f32,
    average: f32,
) -> FpsText {
    fn value(
        output: &mut FpsText,
        fps: f32,
    ) {
        if fps.is_finite() && (0.0..=9_999.0).contains(&fps) {
            let fps = if fps == 0.0 { 0.0 } else { fps };
            let _ = write!(output, "{fps:.0}");
        } else {
            let _ = output.push_str("--");
        }
    }
    let mut text = FpsText::new();
    match mode {
        FpsMode::Off => {}
        FpsMode::Instant => value(&mut text, instant),
        FpsMode::Average => value(&mut text, average),
        FpsMode::Combined => {
            value(&mut text, instant);
            let _ = text.push('/');
            value(&mut text, average);
        }
    }
    // Longest valid text: "9999/9999 AVG" (13 bytes).
    let _ = text.push_str(mode.suffix());
    text
}

pub const CELL_COUNT: usize = 8;

pub mod cell_idx {
    pub const LAMBDA: usize = 1;
    pub const BATTERY: usize = 2;
    pub const COOLANT: usize = 3;
    pub const OIL: usize = 4;
    pub const DSG: usize = 5;
    pub const IAT: usize = 6;
    pub const EGT: usize = 7;
}

pub struct RenderState {
    dividers_drawn: bool,
    headers: [Option<FpsText>; 2],
    prev_popup_kind: Option<u8>,
    popup_just_closed: bool,
    first_frame: bool,
    display_cleared: bool,
}

impl RenderState {
    pub const fn new() -> Self {
        Self {
            dividers_drawn: false,
            headers: [None, None],
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
        buffer_id: usize,
        fps_mode: FpsMode,
        fps_instant: f32,
        fps_average: f32,
    ) -> bool {
        let Some(header) = self.headers.get_mut(buffer_id) else {
            return true;
        };
        let current = format_fps(fps_mode, fps_instant, fps_average);

        let dirty =
            self.first_frame || self.popup_just_closed || self.display_cleared || header.as_ref() != Some(&current);

        *header = Some(current);
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
            self.headers = [None, None];
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
