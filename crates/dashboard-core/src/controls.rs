//! Debounced onboard gestures and deterministic transaction precedence.
use dashboard_config::{ButtonId, ControlAction, ControlMap, ControlPage};

use crate::input_trace::Observation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlEvent {
    pub button: ButtonId,
    pub action: ControlAction,
    pub observation: Observation,
}

#[derive(Default, Debug)]
pub struct ControlBatch {
    pub actions: heapless::Vec<ControlEvent, 4>,
    pub recovery: Option<Observation>,
    pub discarded: u32,
}
impl ControlBatch {
    /// The first surviving navigation that actually changes the captured page.
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

#[derive(Clone, Copy, Default)]
struct ButtonState {
    stable: bool,
    candidate: bool,
    candidate_at: u64,
    armed: bool,
    press_at: u64,
    fired: bool,
}

#[derive(Default)]
pub struct ControlGestures {
    buttons: [ButtonState; 4],
    page: Option<ControlPage>,
    last_ms: Option<u64>,
}
impl ControlGestures {
    pub fn new() -> Self { Self::default() }

    /// Maintenance requires a fresh release before any held button can act.
    pub fn reset(&mut self) { *self = Self::new(); }

    pub fn update(
        &mut self,
        map: &ControlMap,
        page: ControlPage,
        pressed: [bool; 4],
        now_ms: u64,
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
