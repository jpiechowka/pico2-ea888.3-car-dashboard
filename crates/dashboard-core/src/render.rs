//! Shared visible FPS formatting and legacy two-buffer redraw flags; configured scenes own richer keys.
/// User selection of instantaneous/average software completion-rate text.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum FpsMode {
    /// Hide FPS text entirely.
    #[default]
    Off,
    /// Show the latest supplied completion-rate value.
    Instant,
    /// Show the supplied rolling/window average only.
    Average,
    /// Show instant/average together with the AVG suffix.
    Combined,
}

impl FpsMode {
    /// Returns the next mode in Off->Instant->Average->Combined->Off order.
    pub const fn next(self) -> Self {
        match self {
            Self::Off => Self::Instant,
            Self::Instant => Self::Average,
            Self::Average => Self::Combined,
            Self::Combined => Self::Off,
        }
    }

    /// Returns whether this mode renders any FPS text.
    pub const fn is_visible(self) -> bool { !matches!(self, Self::Off) }

    /// Returns true only for the two-number Combined layout.
    pub const fn needs_both_fps(self) -> bool { matches!(self, Self::Combined) }

    /// Returns bounded ASCII feedback naming the selected mode.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "FPS OFF",
            Self::Instant => "FPS: INST",
            Self::Average => "FPS: AVG",
            Self::Combined => "FPS: BOTH",
        }
    }

    /// Returns visible suffix: empty for Off, FPS for Instant, AVG for Average/Combined.
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

/// Bounded visible header text; longest valid two-number representation fits16bytes.
pub type FpsText = String<16>;

/// The cache and renderer use identical visible text, including tie rounding.
/// Returns text for `mode` from software-rate `instant`/`average` in frames/second.
/// Finite0..=9999 inputs round to zero decimals (negative zero normalized); invalid
/// values independently become `--`. Off returns empty text; no physical scanout is implied.
pub fn format_fps(
    mode: FpsMode,
    instant: f32,
    average: f32,
) -> FpsText {
    /// Appends rounded `fps` or `--` to `output`; caller's bounded two-value layout
    /// guarantees capacity. Values outside finite0..=9999 are unavailable, not clipped.
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

/// Logical cell count shared by landscape4x2 and portrait2x4 layouts.
pub const CELL_COUNT: usize = 8;

/// Legacy fixed-dashboard slots; boost occupies index zero and has no named constant here.
pub mod cell_idx {
    /// Legacy lambda/AFR slot index.
    pub const LAMBDA: usize = 1;
    /// Legacy module-voltage slot index.
    pub const BATTERY: usize = 2;
    /// Legacy coolant-temperature slot index.
    pub const COOLANT: usize = 3;
    /// Legacy engine-oil-temperature slot index.
    pub const OIL: usize = 4;
    /// Legacy gearbox-temperature slot index; vehicle source remains independently qualified.
    pub const DSG: usize = 5;
    /// Legacy intake-temperature slot index.
    pub const IAT: usize = 6;
    /// Legacy exhaust-temperature slot index; context/qualification remains independent.
    pub const EGT: usize = 7;
}

/// Legacy redraw hints with independent FPS text per physical buffer; not panel-generation proof.
pub struct RenderState {
    /// Divider drawing has been acknowledged since the last invalidation.
    dividers_drawn: bool,
    /// Last prepared FPS text for each of the two physical buffers.
    headers: [Option<FpsText>; 2],
    /// Opaque previous popup kind; changing a visible kind invalidates headers/dividers.
    prev_popup_kind: Option<u8>,
    /// Visible popup was removed or replaced since end_frame.
    popup_just_closed: bool,
    /// No frame has yet called end_frame.
    first_frame: bool,
    /// Caller cleared the display since end_frame; requires reconstruction.
    display_cleared: bool,
}

impl RenderState {
    /// Returns first-frame state with invalid header keys and undrawn dividers.
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

    /// Returns whether dividers need drawing after first frame, clear or explicit invalidation.
    #[inline]
    pub const fn need_dividers(&self) -> bool { !self.dividers_drawn || self.first_frame || self.display_cleared }

    /// Records divider drawing; first-frame/clear flags still force redraw until end_frame.
    #[inline]
    pub fn mark_dividers_drawn(&mut self) { self.dividers_drawn = true; }

    /// Formats `fps_mode` with `fps_instant`/`fps_average` frames/second for physical
    /// `buffer_id`0or1. Returns whether visible text/flags require repaint and stores
    /// the new key immediately. Invalid buffer returns true without mutation; caller
    /// must honor the returned repaint before treating the stored key as valid pixels.
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

    /// Installs opaque optional `popup_kind`; removing or replacing a previously
    /// visible kind invalidates both header keys/dividers and marks popup_just_closed.
    /// First appearance alone does not set that repair flag.
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

    /// Returns whether a visible popup was removed/replaced since end_frame.
    #[inline]
    pub const fn popup_just_closed(&self) -> bool { self.popup_just_closed }

    /// Returns whether no frame has yet been finalized with end_frame.
    #[inline]
    pub const fn is_first_frame(&self) -> bool { self.first_frame }

    /// Marks current pixels cleared and dividers invalid; header checks repaint despite matching text.
    pub fn mark_display_cleared(&mut self) {
        self.display_cleared = true;
        self.dividers_drawn = false;
    }

    /// Clears one-frame first/popup/clear flags, retaining per-buffer text and divider acknowledgment.
    pub fn end_frame(&mut self) {
        self.first_frame = false;
        self.popup_just_closed = false;
        self.display_cleared = false;
    }
}

impl Default for RenderState {
    /// Returns initial invalid redraw state, as new.
    fn default() -> Self { Self::new() }
}
