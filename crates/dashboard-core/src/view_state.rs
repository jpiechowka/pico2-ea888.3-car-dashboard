//! Core-0 view selection and wall-clock warning pages. No transport ownership.
use dashboard_config::{ValidatedConfig, WarningMode, WarningScope};

use crate::acquisition::DashboardSnapshot;
use crate::alarms::RuleIndex;
use crate::backlight::{Backlight, EncoderEffect};
use crate::pages::Page;
use crate::popup::Popup;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PopupText {
    bytes: [u8; 20],
    len: u8,
}
impl PopupText {
    pub const EMPTY: Self = Self { bytes: [0; 20], len: 0 };

    pub fn new(text: &str) -> Option<Self> {
        if text.len() > 20 || !text.bytes().all(|c| (32..=126).contains(&c)) {
            return None;
        }
        let mut result = Self::EMPTY;
        result.bytes[..text.len()].copy_from_slice(text.as_bytes());
        result.len = text.len() as u8;
        Some(result)
    }

    pub fn as_str(&self) -> &str { core::str::from_utf8(&self.bytes[..usize::from(self.len)]).expect("ASCII popup") }
}

pub struct ViewSelection {
    id: u8,
}
impl ViewSelection {
    pub fn new(config: &ValidatedConfig) -> Self {
        Self {
            id: config.config().boot_view,
        }
    }

    pub const fn id(&self) -> u8 { self.id }

    pub fn rotate(
        &mut self,
        config: &ValidatedConfig,
        page: Page,
        delta: i32,
    ) -> bool {
        if page != Page::Dashboard {
            return false;
        }
        let views = &config.config().views;
        let Some(index) = views.iter().position(|v| v.id == self.id) else {
            return false;
        };
        let next = (index as i64 + i64::from(delta)).rem_euclid(views.len() as i64) as usize;
        let changed = self.id != views[next].id;
        self.id = views[next].id;
        changed
    }

    /// Motion interrupts only a previous shutdown; clicks then win feedback.
    /// A dark-screen rotation changes the selected view without waking it.
    pub fn encoder(
        &mut self,
        config: &ValidatedConfig,
        backlight: &mut Backlight,
        delta: i32,
        presses: u8,
        now: u64,
    ) -> EncoderEffect {
        let changed = self.rotate(config, Page::Dashboard, delta);
        if changed {
            backlight.interrupt_feedback();
        }
        let motion_visible = changed && backlight.compare() != 0;
        let mut effect = backlight.encoder(Page::Dashboard, 0, presses, now);
        effect.motion_applied = motion_visible;
        if effect.popup.is_none() && changed {
            effect.popup = Some(Popup::View(now));
        }
        effect
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WarningPage {
    lines: [PopupText; 3],
    count: u8,
    pub number: u8,
    pub total: u8,
    pub blink: bool,
    pub colors: dashboard_config::WarningColors,
    // Identity remains tied to the actual rules, even for duplicate text.
    rules: u64,
}
impl WarningPage {
    /// Explicit preview sample; no alarm identity and never an acquisition result.
    pub fn palette_sample(
        colors: dashboard_config::WarningColors,
        blink: bool,
    ) -> Self {
        Self {
            lines: [
                PopupText::new("COLOUR PREVIEW").expect("fixed label"),
                PopupText::EMPTY,
                PopupText::EMPTY,
            ],
            count: 1,
            number: 1,
            total: 1,
            blink,
            colors,
            rules: 0,
        }
    }

    pub fn lines(&self) -> &[PopupText] { &self.lines[..usize::from(self.count)] }
}

#[derive(Default)]
pub struct WarningPager {
    cursor: Option<u8>,
    view: u8,
    anchor: u64,
    last_ms: u64,
}
impl WarningPager {
    pub const fn new() -> Self {
        Self {
            cursor: None,
            view: 0,
            anchor: 0,
            last_ms: 0,
        }
    }

    /// Feedback or leaving Dashboard restarts with the first current warning.
    /// Preserve the monotonic high-water mark even while obscured.
    pub fn pause(
        &mut self,
        now: u64,
    ) {
        self.cursor = None;
        self.anchor = now.max(self.last_ms);
        self.last_ms = self.anchor;
    }

    pub fn select(
        &mut self,
        config: &ValidatedConfig,
        samples: &DashboardSnapshot,
        view: u8,
        now: u64,
        blink: bool,
    ) -> Option<WarningPage> {
        if now < self.last_ms {
            return None;
        }
        self.last_ms = now;
        if !config.config().views.iter().any(|v| v.id == view) {
            self.pause(now);
            return None;
        }
        let presentation = config.config().warnings;
        let mut eligible = [0u8; 64];
        let mut count = 0usize;
        let mut old_rank = 0usize;
        let mut rank = 0usize;
        let mut ranks = [0u8; 64];
        for configured_view in &config.config().views {
            for (i, widget) in config.config().widgets.iter().enumerate() {
                if widget.view != configured_view.id {
                    continue;
                }
                if self.cursor == Some(i as u8) {
                    old_rank = rank;
                }
                if (presentation.scope == WarningScope::AllViews || widget.view == view)
                    && samples.rule_active_at(RuleIndex::new(i).expect("bounded widget index"), now)
                {
                    eligible[count] = i as u8;
                    ranks[count] = rank as u8;
                    count += 1;
                }
                rank += 1;
            }
        }
        if count == 0 {
            self.pause(now);
            return None;
        }
        let eligible = &eligible[..count];
        let fresh = self.cursor.is_none() || view != self.view;
        let mut index = if fresh {
            0
        } else {
            eligible
                .iter()
                .position(|&rule| Some(rule) == self.cursor)
                .unwrap_or_else(|| {
                    ranks[..count]
                        .iter()
                        .position(|&rank| usize::from(rank) > old_rank)
                        .unwrap_or(0)
                })
        };
        if fresh {
            self.anchor = now;
        }
        self.view = view;
        let per_page = if presentation.mode == WarningMode::CycleOne {
            1
        } else {
            3
        };
        let total = count.div_ceil(per_page);
        let steps = (now - self.anchor) / u64::from(presentation.page_ms);
        if steps != 0 {
            // Reach the wrap once, then skip complete cycles arithmetically.
            // Work stays bounded even after a multi-day display stall.
            let to_wrap = (count - index).div_ceil(per_page) as u64;
            index = if steps < to_wrap {
                index + steps as usize * per_page
            } else {
                ((steps - to_wrap) % total as u64) as usize * per_page
            };
            self.anchor += steps * u64::from(presentation.page_ms);
        }
        self.cursor = Some(eligible[index]);
        let mut result = WarningPage {
            lines: [PopupText::EMPTY; 3],
            count: 0,
            number: (index / per_page) as u8 + 1,
            total: total as u8,
            blink,
            colors: if presentation.mode == WarningMode::CycleOne {
                config.config().widgets[usize::from(eligible[index])].colors.warning
            } else {
                config.config().warning_colors
            },
            rules: 0,
        };
        for &rule in eligible.iter().skip(index).take(per_page) {
            let i = usize::from(rule);
            result.lines[usize::from(result.count)] =
                PopupText::new(&config.config().widgets[i].warning.message).expect("validated warning message");
            result.count += 1;
            result.rules |= 1 << i;
        }
        Some(result)
    }
}
