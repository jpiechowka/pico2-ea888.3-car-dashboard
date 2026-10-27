//! Core-0 view selection and wall-clock warning pages. No transport ownership.
use dashboard_config::{ValidatedConfig, WarningMode, WarningScope};

use crate::acquisition::DashboardSnapshot;
use crate::alarms::RuleIndex;
use crate::backlight::{Backlight, EncoderEffect};
use crate::pages::Page;
use crate::popup::Popup;

/// At most twenty printable ASCII bytes for bounded view/warning feedback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PopupText {
    /// Zero-filled backing storage; only the len prefix is displayed.
    bytes: [u8; 20],
    /// Initialized printable prefix length in0..=20 bytes.
    len: u8,
}
impl PopupText {
    /// Empty printable text with all backing bytes zero.
    pub const EMPTY: Self = Self { bytes: [0; 20], len: 0 };

    /// Returns copied `text` when at most20bytes and every byte is printable ASCII
    /// 32..=126, including empty input; otherwise None without truncation/replacement.
    pub fn new(text: &str) -> Option<Self> {
        if text.len() > 20 || !text.bytes().all(|c| (32..=126).contains(&c)) {
            return None;
        }
        let mut result = Self::EMPTY;
        result.bytes[..text.len()].copy_from_slice(text.as_bytes());
        result.len = text.len() as u8;
        Some(result)
    }

    /// Returns the initialized printable prefix; private construction guarantees valid UTF-8 and bounds.
    pub fn as_str(&self) -> &str { core::str::from_utf8(&self.bytes[..usize::from(self.len)]).expect("ASCII popup") }
}

/// Named-view identity retained when leaving Dashboard; configuration defines ordering.
pub struct ViewSelection {
    /// Configured view ID, not its position in the view list.
    id: u8,
}
impl ViewSelection {
    /// Returns `config`'s validated boot view selection.
    pub fn new(config: &ValidatedConfig) -> Self {
        Self {
            id: config.config().boot_view,
        }
    }

    /// Returns the retained configured view identifier.
    pub const fn id(&self) -> u8 { self.id }

    /// Rotates by signed `delta` through `config` view order only on Dashboard `page`.
    /// Returns whether the ID changed; other pages, absent current ID and full-cycle
    /// movement return false. Widened modulo arithmetic handles every i32 delta.
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
    /// `config` supplies view order, `backlight` receives shutdown/click effects,
    /// `delta` is signed rotation, `presses` the bounded click batch and `now`
    /// monotonic milliseconds. Returns click-priority feedback and motion_applied only
    /// if changed motion was visible before clicks; a later wake cannot retroactively expose it.
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

/// One resolved warning page with exact rule identity, palette and state labels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WarningPage {
    /// Up to three bounded warning messages; unused trailing rows are empty.
    lines: [PopupText; 3],
    /// Initialized row count in1..=3 for selected/sample pages.
    count: u8,
    /// One-based visible page number in the current eligible set.
    pub number: u8,
    /// Total ordinary pages plus dedicated grouped cylinder pages.
    pub total: u8,
    /// Caller-resolved blink phase, independent of rendered frame count.
    pub blink: bool,
    /// Per-rule palette in CycleOne/dedicated group pages, shared ListThree palette otherwise.
    pub colors: dashboard_config::WarningColors,
    /// Widget-index rule mask for acknowledgment; duplicate messages retain distinct bits.
    pub rules: u64,
    /// Status label identity for each rendered row; included in retained equality keys.
    statuses: [WarningLabel; 3],
    /// Dedicated cylinder-page membership; zero selects ordinary stored rows.
    cylinders: u8,
}
impl WarningPage {
    /// Explicit preview sample; no alarm identity and never an acquisition result.
    /// Returns a single COLOUR PREVIEW row using `colors` and caller `blink` phase,
    /// with rule mask zero so it cannot acknowledge a live alarm.
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
            statuses: [WarningLabel::None; 3],
            cylinders: 0,
        }
    }

    /// Returns the fixed ASCII status label for `row`. Panics for row>=3; callers
    /// should restrict rows to lines().len(), since unused rows have empty labels.
    pub fn status(
        &self,
        row: usize,
    ) -> &'static str {
        self.statuses[row].text()
    }

    /// Returns stored warning-message rows, at most three. A dedicated cylinder
    /// page stores one shared message and uses cylinder_mask() to generate its lines.
    pub fn lines(&self) -> &[PopupText] { &self.lines[..usize::from(self.count)] }

    /// Returns low-four cylinder bits for a dedicated group page, or zero for
    /// ordinary pages. Group pages store the shared message exactly once in lines().
    pub const fn cylinder_mask(&self) -> u8 { self.cylinders }
}

/// Compact label identity, preserving latch/ack/staleness distinctions in render keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum WarningLabel {
    /// No status, used only by preview/unused rows.
    None,
    /// Qualified active warning without acknowledgment/latch label.
    Active,
    /// Active episode acknowledged by the producer.
    Acknowledged,
    /// Visible latched episode awaiting its configured clear/ack conditions.
    Latched,
    /// Latched warning whose source is unavailable.
    Stale,
    /// Latched trigger cleared but acknowledgment still required.
    Cleared,
}
impl WarningLabel {
    /// Returns the fixed ASCII label for this state; None returns empty text.
    const fn text(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Active => "ACTIVE",
            Self::Acknowledged => "ACTIVE / ACKNOWLEDGED",
            Self::Latched => "LATCHED",
            Self::Stale => "LATCHED / DATA STALE",
            Self::Cleared => "CLEARED / ACK",
        }
    }

    /// Returns label for eligible `e`, prioritizing unavailable latch, cleared latch,
    /// acknowledgment, latch, then active. Caller filters non-attention states first.
    fn of(e: crate::warning_state::WarningEligibility) -> Self {
        if e.latched && matches!(e.status, crate::alarms::Status::Unavailable) {
            Self::Stale
        } else if e.latched && e.cleared {
            Self::Cleared
        } else if e.acknowledged {
            Self::Acknowledged
        } else if e.latched {
            Self::Latched
        } else {
            Self::Active
        }
    }
}

/// Time-driven warning cursor with bounded catch-up and activation interruption.
#[derive(Default)]
pub struct WarningPager {
    /// Retained first rule index of rotation page, absent after pause/empty set.
    cursor: Option<u8>,
    /// Last requested view ID; changing view restarts at the highest-priority eligible rule.
    view: u8,
    /// Current page interval origin in monotonic milliseconds.
    anchor: u64,
    /// Monotonic high-water mark, retained across pause.
    last_ms: u64,
    /// Last observed warning activation serial; change interrupts without resetting rotation time.
    event_serial: u64,
    /// Acquisition warning-owner identity; replacement restarts paging.
    run_id: Option<u64>,
    /// New activation temporarily shows the first eligible page over the retained rotation cursor.
    interrupted: bool,
}
impl WarningPager {
    /// Returns empty pager state with time zero and no acquisition run identity.
    pub const fn new() -> Self {
        Self {
            cursor: None,
            view: 0,
            anchor: 0,
            last_ms: 0,
            event_serial: 0,
            run_id: None,
            interrupted: false,
        }
    }

    /// Feedback or leaving Dashboard restarts with the first current warning.
    /// Preserve the monotonic high-water mark even while obscured.
    /// `now` is monotonic milliseconds; older values cannot lower the retained clock.
    pub fn pause(
        &mut self,
        now: u64,
    ) {
        self.cursor = None;
        self.interrupted = false;
        self.anchor = now.max(self.last_ms);
        self.last_ms = self.anchor;
    }

    /// Appearance reconstructs a synthetic owner for each render. The caller
    /// resets this pager for configuration/source changes; no warning identity
    /// from this path may be submitted for acknowledgment.
    /// `config` supplies appearance rules, `samples` synthetic evidence, `view` the
    /// selected ID, `now` monotonic milliseconds and `blink` visual phase. Returns the
    /// selected page/None as select, treating reconstructed sample owners as continuous.
    #[cfg(feature = "alloc")]
    pub fn select_appearance(
        &mut self,
        config: &ValidatedConfig,
        samples: &DashboardSnapshot,
        view: u8,
        now: u64,
        blink: bool,
    ) -> Option<WarningPage> {
        self.run_id = samples.warning_run_id();
        self.select(config, samples, view, now, blink)
    }

    /// Returns up to three eligible warnings from `samples` under `config` for
    /// selected `view`, monotonic `now` milliseconds and caller `blink` phase, or None
    /// for reversed time, unknown view or no attention. Order is priority, configured
    /// view, then widget order. New runs/views restart; new activations interrupt with
    /// the first eligible page without resetting the rotation deadline. Catch-up skips
    /// whole cycles arithmetically; a removed cursor uses its next surviving successor.
    /// CylinderLines groups always own a dedicated page, including in ListThree;
    /// ordinary mixed pages still hold up to three messages and preserve rule indices.
    /// Caller resets this owner when replacing configuration and excludes feedback/off-screen periods via pause.
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
        for priority in [
            dashboard_config::Priority::Critical,
            dashboard_config::Priority::High,
            dashboard_config::Priority::Normal,
            dashboard_config::Priority::Low,
        ] {
            for configured_view in &config.config().views {
                for (i, widget) in config.config().widgets.iter().enumerate() {
                    if widget.view != configured_view.id || widget.warning.advanced.priority != priority {
                        continue;
                    }
                    if self.cursor == Some(i as u8) {
                        old_rank = rank;
                    }
                    if (presentation.scope == WarningScope::AllViews || widget.view == view)
                        && samples
                            .warning_eligibility(RuleIndex::new(i).expect("bounded widget index"), now)
                            .attention()
                    {
                        eligible[count] = i as u8;
                        ranks[count] = rank as u8;
                        count += 1;
                    }
                    rank += 1;
                }
            }
        }
        let new_run = self.run_id != samples.warning_run_id();
        let new_event = new_run || self.event_serial != samples.warning_event_serial();
        self.run_id = samples.warning_run_id();
        self.event_serial = samples.warning_event_serial();
        if count == 0 {
            self.pause(now);
            return None;
        }
        let eligible = &eligible[..count];
        let fresh = self.cursor.is_none() || view != self.view || new_run;
        let retained_index = eligible.iter().position(|&rule| Some(rule) == self.cursor);
        let mut index = if fresh {
            0
        } else {
            retained_index.unwrap_or_else(|| {
                ranks[..count]
                    .iter()
                    .position(|&rank| usize::from(rank) > old_rank)
                    .unwrap_or(0)
            })
        };
        if fresh {
            self.anchor = now;
            self.interrupted = false;
        } else if new_event {
            // Immediate attention restarts at highest priority; the rotation
            // cursor and deadline continue underneath, so churn cannot reset progress.
            self.interrupted = true;
        }
        self.view = view;
        let per_page = if presentation.mode == WarningMode::CycleOne {
            1
        } else {
            3
        };
        let grouped_pages = eligible.iter().any(|rule| cylinder_lines(config, usize::from(*rule)));
        // Ranks are no longer needed after successor selection. Reuse that
        // bounded array for mixed page starts without another stack allocation.
        let mut total = count.div_ceil(per_page);
        let mut page_index = index / per_page;
        if grouped_pages {
            total = 0;
            let mut start = 0;
            while start < count {
                ranks[total] = start as u8;
                total += 1;
                start = page_end(config, eligible, start, per_page);
            }
            page_index = ranks[..total]
                .iter()
                .rposition(|&start| usize::from(start) <= index)
                .unwrap_or(0);
            index = usize::from(ranks[page_index]);
        }
        let steps = (now - self.anchor) / u64::from(presentation.page_ms);
        if steps != 0 {
            self.interrupted = false;
            // A removed cursor's successor already consumes the first due step.
            // Until then render that successor without replacing the retained
            // cursor: a returning episode must not advance rotation twice.
            let advance = steps - u64::from(!fresh && retained_index.is_none());
            // Reach the wrap once, then skip complete cycles arithmetically.
            // Work stays bounded even after a multi-day display stall.
            if grouped_pages {
                page_index = (page_index + (advance % total as u64) as usize) % total;
                index = usize::from(ranks[page_index]);
            } else {
                let to_wrap = (count - index).div_ceil(per_page) as u64;
                index = if advance < to_wrap {
                    index + advance as usize * per_page
                } else {
                    ((advance - to_wrap) % total as u64) as usize * per_page
                };
                page_index = index / per_page;
            }
            self.anchor += steps * u64::from(presentation.page_ms);
        }
        if fresh || steps != 0 || retained_index.is_some() {
            self.cursor = Some(eligible[index]);
        }
        // At an exact/delayed deadline the rotation has advanced above. A new
        // episode still receives this selection; the next selection resumes
        // that retained cursor without granting the interruption a new period.
        if self.interrupted || (new_event && !fresh) {
            index = 0;
            page_index = 0;
        }
        let dedicated = cylinder_lines(config, usize::from(eligible[index]));
        let mut result = WarningPage {
            lines: [PopupText::EMPTY; 3],
            count: 0,
            number: page_index as u8 + 1,
            total: total as u8,
            blink,
            colors: if presentation.mode == WarningMode::CycleOne || dedicated {
                config.config().widgets[usize::from(eligible[index])].colors.warning
            } else {
                config.config().warning_colors
            },
            rules: 0,
            statuses: [WarningLabel::None; 3],
            cylinders: 0,
        };
        let end = if grouped_pages {
            page_end(config, eligible, index, per_page)
        } else {
            (index + per_page).min(count)
        };
        for &rule in &eligible[index..end] {
            let i = usize::from(rule);
            result.lines[usize::from(result.count)] =
                PopupText::new(&config.config().widgets[i].warning.message).expect("validated warning message");
            let eligibility = samples.warning_eligibility(RuleIndex::new(i).unwrap(), now);
            if dedicated {
                let rule_index = RuleIndex::new(i).expect("bounded eligible index");
                result.cylinders = if eligibility.status == crate::alarms::Status::Active {
                    samples.ignition_warning_mask_at(rule_index, now)
                } else {
                    samples.ignition_warning_latched_mask(rule_index)
                };
            }
            result.statuses[usize::from(result.count)] = WarningLabel::of(eligibility);
            result.count += 1;
            result.rules |= 1 << i;
        }
        Some(result)
    }
}

/// Returns whether admitted widget `index` in `config` requests dedicated per-cylinder
/// warning lines. Internal callers supply indices from the same config's widgets.
fn cylinder_lines(
    config: &ValidatedConfig,
    index: usize,
) -> bool {
    let widget = &config.config().widgets[index];
    widget.channel.scalar().is_none()
        && widget.advanced.ignition_warning == dashboard_config::IgnitionWarningPresentation::CylinderLines
}

/// Returns the exclusive end of a page beginning at eligible `start`. Ordinary
/// pages contain at most `per_page` (1 or3) entries and stop before a cylinder
/// page; cylinder pages own one message and generate up to four lines. Caller
/// supplies a valid start into `eligible`, whose rule indices belong to `config`.
fn page_end(
    config: &ValidatedConfig,
    eligible: &[u8],
    start: usize,
    per_page: usize,
) -> usize {
    if cylinder_lines(config, usize::from(eligible[start])) {
        return start + 1;
    }
    let mut end = start + 1;
    while end < eligible.len() && end - start < per_page && !cylinder_lines(config, usize::from(eligible[end])) {
        end += 1;
    }
    end
}
