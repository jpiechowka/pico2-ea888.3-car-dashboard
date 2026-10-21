//! Debounced onboard gestures and deterministic transaction precedence.
use dashboard_config::{ButtonId, ControlAction, ControlMap, ControlPage};

use crate::input_trace::Observation;

/// A bounded factual route from the actual current-page map, followed by the
/// immutable recovery gesture. No route is invented for unreachable/raw maps.
/// Returns ASCII guidance for `requested` from `page` using `map`: prefer matching
/// onboard tap/hold, then encoder click; otherwise show the first actual transition.
/// Always includes fixed Y recovery. Output is at most52bytes; no mapping is executed.
pub fn navigation_hint(
    map: &ControlMap,
    page: ControlPage,
    requested: ControlPage,
) -> heapless::String<52> {
    use core::fmt::Write;
    let screen = &map.screens[page.index()];
    let mut fallback = None;
    let mut selected = None;
    for hold in [false, true] {
        for button in ButtonId::ALL {
            if hold && button == ButtonId::Y {
                continue;
            }
            let binding = screen.buttons[button.index()];
            let action = if hold { binding.hold } else { binding.tap };
            if !action.allowed_on(page) {
                continue;
            }
            if let Some(destination) = action.destination(page).filter(|p| *p != page) {
                let route = (button.label(), if hold { "hold" } else { "tap" }, destination);
                if fallback.is_none() {
                    fallback = Some(route);
                }
                if selected.is_none() && destination == requested {
                    selected = Some(route);
                }
            }
        }
    }
    if screen.encoder_click.allowed_on(page)
        && let Some(destination) = screen.encoder_click.destination(page).filter(|p| *p != page)
    {
        let route = ("Enc", "tap", destination);
        if fallback.is_none() {
            fallback = Some(route);
        }
        if selected.is_none() && destination == requested {
            selected = Some(route);
        }
    }
    let mut text = heapless::String::new();
    // Finite labels: longest dynamic part plus separator is 16 ASCII bytes.
    // Fixed recovery has its own budget and can never be truncated by a label.
    if let Some((input, gesture, destination)) = selected.or(fallback) {
        let label = match destination {
            ControlPage::Dashboard => "Dash",
            ControlPage::Debug => "Debug",
            ControlPage::Logs => "Logs",
        };
        let _ = write!(text, "{input} {gesture}:{label} | ");
    }
    text.push_str("Y hold 2s:Dash/Full").expect("finite control hint fits");
    text
}

/// One qualified onboard gesture, already resolved against the captured page's mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlEvent {
    /// Physical A/B/X/Y identity; defines deterministic precedence.
    pub button: ButtonId,
    /// Finite mapped action, checked for compatibility by gesture and session owners.
    pub action: ControlAction,
    /// First press observation and gesture acceptance in microseconds, normally one event.
    pub observation: Observation,
}

/// At most one event per button, after recovery/navigation precedence and startup filtering.
#[derive(Default, Debug)]
pub struct ControlBatch {
    /// Qualified startup gesture, including a button mapped to None.
    pub startup_skip: bool,
    /// Surviving actions in A/B/X/Y order, bounded to four.
    pub actions: heapless::Vec<ControlEvent, 4>,
    /// Protected two-second Y hold, independent of user mappings.
    pub recovery: Option<Observation>,
    /// Events dropped for invalid mapping/time conversion or competing precedence.
    pub discarded: u32,
}
impl ControlBatch {
    /// The first surviving navigation that actually changes the captured page.
    /// Returns a destination different from `page`, or None; recovery on Dashboard
    /// still suppresses other actions but needs no page transition. Does not validate timestamps.
    pub fn transition(
        &self,
        page: ControlPage,
    ) -> Option<ControlPage> {
        if self.recovery.is_some() {
            return (page != ControlPage::Dashboard).then_some(ControlPage::Dashboard);
        }
        self.actions.iter().find_map(|event| {
            event
                .action
                .destination(page)
                .filter(|destination| *destination != page)
        })
    }
}

/// One key's raw/debounced state and one-shot gesture arming.
#[derive(Clone, Copy, Default)]
struct ButtonState {
    /// Accepted pressed level after50ms candidate stability.
    stable: bool,
    /// Most recently sampled pressed level.
    candidate: bool,
    /// First observation of current candidate in monotonic milliseconds.
    candidate_at: u64,
    /// A stable release has permitted a subsequent gesture; false suppresses spanning/boot-held keys.
    armed: bool,
    /// Origin of the accepted press candidate in monotonic milliseconds, before debounce delay.
    press_at: u64,
    /// Hold fired or the spanning press was suppressed; prevents a later tap.
    fired: bool,
}

/// Four-key sampled gesture owner; screen changes preserve release debounce while cancelling old presses.
#[derive(Default)]
pub struct ControlGestures {
    /// Fixed A/B/X/Y state, with no dynamic input allocation.
    buttons: [ButtonState; 4],
    /// Screen associated with prior samples; None before initial synchronization.
    page: Option<ControlPage>,
    /// Last accepted sample time in monotonic milliseconds; reversal is ignored.
    last_ms: Option<u64>,
}
impl ControlGestures {
    /// Returns an unsynchronized owner that suppresses any key held on its first sample.
    pub fn new() -> Self { Self::default() }

    /// Maintenance requires a fresh release before any held button can act.
    pub fn reset(&mut self) { *self = Self::new(); }

    /// Retain debounce history while cancelling presses spanning startup exit.
    pub fn suppress_held(&mut self) {
        for state in &mut self.buttons {
            if state.stable || state.candidate {
                state.armed = false;
                state.fired = true;
            }
        }
    }

    /// Samples A/B/X/Y `pressed` levels for `page` using `map` at monotonic `now_ms`.
    /// Returns startup_skip for a qualified ordinary gesture, even mapped None; fixed Y
    /// recovery survives and ordinary actions are omitted. Uses the same debounce/order guards as update.
    pub fn update_startup(
        &mut self,
        map: &ControlMap,
        page: ControlPage,
        pressed: [bool; 4],
        now_ms: u64,
    ) -> ControlBatch {
        self.update_context(map, page, pressed, now_ms, true)
    }

    /// Samples A/B/X/Y `pressed` levels for captured `page` using `map` at monotonic
    /// `now_ms`. Returns bounded events after50ms debounce, tap-on-release or one-shot
    /// configured hold (clamped400..1500ms); Y hold is fixed2000ms. Holds require the
    /// current raw level still pressed. Reversal/initial-page synchronization produces no actions.
    pub fn update(
        &mut self,
        map: &ControlMap,
        page: ControlPage,
        pressed: [bool; 4],
        now_ms: u64,
    ) -> ControlBatch {
        self.update_context(map, page, pressed, now_ms, false)
    }

    /// Resolves `pressed` A/B/X/Y samples under `map` and captured `page` at `now_ms`
    /// monotonic milliseconds; `startup` converts ordinary gestures to skip feedback.
    /// Returns recovery before navigation before other actions, with discarded counts.
    /// Page changes cancel spanning presses; failed microsecond conversion discards the
    /// gesture. No hold is synthesized on a sampled release, even after a long stall.
    fn update_context(
        &mut self,
        map: &ControlMap,
        page: ControlPage,
        pressed: [bool; 4],
        now_ms: u64,
        startup: bool,
    ) -> ControlBatch {
        let mut batch = ControlBatch::default();
        if self.last_ms.is_some_and(|last| now_ms < last) {
            return batch;
        }
        self.last_ms = Some(now_ms);
        if self.page != Some(page) {
            let bootstrap = self.page.is_none();
            self.page = Some(page);
            for (state, level) in self.buttons.iter_mut().zip(pressed) {
                if bootstrap {
                    *state = ButtonState {
                        stable: level,
                        candidate: level,
                        candidate_at: now_ms,
                        armed: !level,
                        ..ButtonState::default()
                    };
                } else {
                    // Keep release qualification from the old page. A raw
                    // released sample may be bounce rather than a stable edge.
                    if state.stable || state.candidate || level {
                        state.armed = false;
                    }
                    if state.candidate != level {
                        state.candidate = level;
                        state.candidate_at = now_ms;
                    }
                    state.press_at = now_ms;
                    state.fired = true;
                }
            }
            return batch;
        }
        for button in ButtonId::ALL {
            let index = button.index();
            let level = pressed[index];
            let state = &mut self.buttons[index];
            if state.candidate != level {
                state.candidate = level;
                state.candidate_at = now_ms;
            }
            let mut tap = false;
            if state.stable != state.candidate && now_ms - state.candidate_at >= 50 {
                state.stable = state.candidate;
                if state.stable {
                    state.press_at = state.candidate_at;
                    state.fired = false;
                } else {
                    tap = state.armed && !state.fired;
                    state.armed = true;
                }
            }
            // A page switch may cancel a press candidate before it becomes
            // stable. Its release still qualifies after the same debounce.
            if !state.stable && !state.candidate && now_ms - state.candidate_at >= 50 {
                state.armed = true;
            }
            let binding = map.screens[page.index()].buttons[index];
            let recovery = button == ButtonId::Y;
            let threshold = if recovery {
                2000
            } else {
                u64::from(map.hold_ms.clamp(400, 1500))
            };
            let hold = state.armed
                && state.stable
                && level
                && !state.fired
                && (recovery || binding.hold != ControlAction::None)
                && now_ms - state.press_at >= threshold;
            if !(tap || hold) {
                continue;
            }
            if hold {
                state.fired = true;
            }
            let Some(observation) = state
                .press_at
                .checked_mul(1000)
                .zip(now_ms.checked_mul(1000))
                .and_then(|(observed, accepted)| Observation::new(observed, accepted))
            else {
                batch.discarded += 1;
                continue;
            };
            if recovery && hold {
                batch.recovery = Some(observation);
                continue;
            }
            if startup {
                batch.startup_skip = true;
                continue;
            }
            let action = if hold { binding.hold } else { binding.tap };
            if !action.allowed_on(page) {
                batch.discarded += 1;
            } else if action != ControlAction::None {
                // At most one event per button, bounded by the four inputs.
                let _ = batch.actions.push(ControlEvent {
                    button,
                    action,
                    observation,
                });
            }
        }
        if batch.recovery.is_some() {
            batch.discarded += batch.actions.len() as u32;
            batch.actions.clear();
            for state in &mut self.buttons {
                if state.stable || state.candidate {
                    state.armed = false;
                }
            }
        } else if let Some(index) = batch.actions.iter().position(|event| {
            event
                .action
                .destination(page)
                .is_some_and(|destination| destination != page)
        }) {
            let transition = batch.actions[index];
            batch.discarded += (batch.actions.len() - 1) as u32;
            batch.actions.clear();
            let _ = batch.actions.push(transition);
            // Suppress old-page releases before the caller applies navigation.
            for state in &mut self.buttons {
                if state.stable || state.candidate {
                    state.armed = false;
                }
            }
        }
        batch
    }
}
