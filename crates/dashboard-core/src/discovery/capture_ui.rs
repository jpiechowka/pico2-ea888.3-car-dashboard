//! Bounded capture controls. Effects carry non-reused identities; firmware owns bytes and I/O.
use super::names::SavedIdentity;

pub const HOLD_MS: u64 = crate::input_model::BUTTON_HOLD_MS;
pub const OPERATION_TIMEOUT_MS: u64 = 30_000;
pub const CONFIRM_TIMEOUT_MS: u64 = 10_000;
pub type RequestId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Unknown,
    Empty,
    Saved {
        generation: u64,
    },
    /// A readable copy exists, but replacement requires destructive reset.
    SavedBlocked {
        generation: u64,
    },
    Corrupt,
    Unsupported,
    Unavailable,
}

fn readable_slot(
    generation: u64,
    replacement_blocked: bool,
) -> Slot {
    if replacement_blocked || generation == u64::MAX {
        Slot::SavedBlocked { generation }
    } else {
        Slot::Saved { generation }
    }
}

/// Preserve both-copy admission status when projecting a journal directory.
/// Do not reduce SlotInfo to its selected capture before this decision.
pub fn slot_from_journal(info: &super::capture_store::SlotInfo) -> Slot {
    use super::capture_store::CopyState;
    let unsupported = info.copies.iter().any(|v| matches!(v, CopyState::UnsupportedSchema(_)));
    if let Some(selected) = info.selected {
        readable_slot(selected.generation, unsupported)
    } else if unsupported {
        Slot::Unsupported
    } else if info.copies.iter().all(|v| *v == CopyState::Empty) {
        Slot::Empty
    } else {
        Slot::Corrupt
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveAvailability {
    Ready,
    NoPeer,
    Busy,
}
#[derive(Clone, Copy, Debug)]
pub struct Context {
    pub save: SaveAvailability,
    pub revision: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// Hardware should send semantic events after producer-side debounce.
    XShort,
    XHold,
    /// Raw model/replay events; do not mix these with semantic events.
    XDown,
    XUp,
    Turn(i16),
    Click,
    Back,
    Recover,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Refresh,
    Load,
    Save,
    Reset,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiError {
    NoPeer,
    Busy,
    Empty,
    Storage,
    Corrupt,
    Unsupported,
    Unavailable,
    Timeout,
    Changed,
    Clock,
    Exhausted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Live,
    Slots,
    Busy(Operation),
    Saved,
    ConfirmOverwrite,
    ConfirmReset,
    Error(UiError),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Refresh {
        id: RequestId,
    },
    Load {
        id: RequestId,
        slot: u8,
    },
    Save {
        id: RequestId,
        slot: u8,
        revision: u32,
        overwrite: bool,
    },
    Reset {
        id: RequestId,
        slot: u8,
    },
    LiveDetails,
    Recover,
}
#[derive(Clone, Copy, Debug)]
pub enum Completion {
    Listed {
        slots: [Slot; 4],
        identities: [Option<SavedIdentity>; 4],
    },
    Loaded {
        pages: usize,
    },
    Saved {
        generation: u64,
        identity: SavedIdentity,
    },
    Reset,
    Failed(UiError),
}
#[derive(Clone, Copy)]
struct Pending {
    id: RequestId,
    operation: Operation,
    slot: u8,
    at_ms: u64,
    visible: bool,
}
#[derive(Clone, Copy)]
struct Confirmation {
    revision: u32,
    slot: Slot,
    at_ms: u64,
}

pub struct CaptureUi {
    screen: Screen,
    slots: [Slot; 4],
    identities: [Option<SavedIdentity>; 4],
    selected: u8,
    page: usize,
    pages: usize,
    next_id: RequestId,
    pending: Option<Pending>,
    confirmation: Option<Confirmation>,
    x_down: Option<u64>,
    x_held: bool,
}
impl Default for CaptureUi {
    fn default() -> Self { Self::new() }
}
impl CaptureUi {
    pub const fn new() -> Self {
        Self {
            screen: Screen::Live,
            slots: [Slot::Unknown; 4],
            identities: [None; 4],
            selected: 0,
            page: 0,
            pages: 1,
            next_id: 0,
            pending: None,
            confirmation: None,
            x_down: None,
            x_held: false,
        }
    }

    pub fn screen(&self) -> Screen { self.screen }

    pub fn slots(&self) -> &[Slot; 4] { &self.slots }

    pub fn identities(&self) -> &[Option<SavedIdentity>; 4] { &self.identities }

    pub fn selected(&self) -> u8 { self.selected }

    pub fn page(&self) -> usize { self.page }

    pub fn pending(&self) -> Option<RequestId> { self.pending.map(|p| p.id) }

    fn fail(
        &mut self,
        error: UiError,
    ) {
        self.confirmation = None;
        self.screen = Screen::Error(error);
    }

    fn invalidate_identity(
        &mut self,
        pending: Pending,
    ) {
        if pending.operation == Operation::Refresh {
            self.identities = [None; 4];
        } else {
            self.identities[usize::from(pending.slot)] = None;
        }
    }

    fn start(
        &mut self,
        operation: Operation,
        now_ms: u64,
        context: Context,
        overwrite: bool,
    ) -> Option<Action> {
        if self.pending.is_some() {
            self.fail(UiError::Busy);
            return None;
        }
        let Some(id) = self.next_id.checked_add(1) else {
            self.fail(UiError::Exhausted);
            return None;
        };
        self.next_id = id;
        self.pending = Some(Pending {
            id,
            operation,
            slot: self.selected,
            at_ms: now_ms,
            visible: true,
        });
        self.confirmation = None;
        self.screen = Screen::Busy(operation);
        Some(match operation {
            Operation::Refresh => Action::Refresh { id },
            Operation::Load => {
                Action::Load {
                    id,
                    slot: self.selected,
                }
            }
            Operation::Save => {
                Action::Save {
                    id,
                    slot: self.selected,
                    revision: context.revision,
                    overwrite,
                }
            }
            Operation::Reset => {
                Action::Reset {
                    id,
                    slot: self.selected,
                }
            }
        })
    }

    /// Call every input/render tick, including while X remains pressed.
    pub fn tick(
        &mut self,
        now_ms: u64,
    ) -> Option<Action> {
        if let Some(pending) = self.pending.as_mut()
            && pending.visible
            && now_ms
                .checked_sub(pending.at_ms)
                .is_none_or(|age| age >= OPERATION_TIMEOUT_MS)
        {
            pending.visible = false;
            let pending = *pending;
            self.invalidate_identity(pending);
            self.fail(UiError::Timeout);
        }
        if let Some(confirm) = self.confirmation
            && now_ms
                .checked_sub(confirm.at_ms)
                .is_none_or(|age| age >= CONFIRM_TIMEOUT_MS)
        {
            self.fail(UiError::Timeout);
        }
        let at = self.x_down?;
        let Some(age) = now_ms.checked_sub(at) else {
            self.x_down = None;
            self.fail(UiError::Clock);
            return None;
        };
        if age < HOLD_MS || self.x_held {
            return None;
        }
        self.x_held = true;
        self.start(
            Operation::Refresh,
            now_ms,
            Context {
                save: SaveAvailability::Busy,
                revision: 0,
            },
            false,
        )
    }

    pub fn input(
        &mut self,
        input: Input,
        now_ms: u64,
        context: Context,
    ) -> Option<Action> {
        if input == Input::Recover {
            self.x_down = None;
            self.x_held = false;
            self.confirmation = None;
            if let Some(pending) = self.pending.as_mut() {
                pending.visible = false;
            }
            self.screen = Screen::Live;
            return Some(Action::Recover);
        }
        if input == Input::Back {
            self.x_down = None;
            self.x_held = false;
            self.confirmation = None;
            if let Some(pending) = self.pending.as_mut() {
                pending.visible = false;
            }
            self.screen = if matches!(self.screen, Screen::Slots | Screen::Live) {
                Screen::Live
            } else {
                Screen::Slots
            };
            return None;
        }
        if matches!(input, Input::XShort | Input::XHold) {
            self.x_down = None;
            self.x_held = false;
            if input == Input::XHold {
                return self.start(Operation::Refresh, now_ms, context, false);
            }
        }
        if let Some(action) = self.tick(now_ms) {
            if input == Input::XUp {
                self.x_down = None;
                self.x_held = false;
            }
            return Some(action);
        }
        match input {
            Input::XShort => return self.short_x(now_ms, context),
            Input::XHold => unreachable!(),
            Input::XDown => {
                if self.x_down.is_none() {
                    self.x_down = Some(now_ms);
                    self.x_held = false;
                }
            }
            Input::XUp => {
                let pressed = self.x_down.take().is_some();
                if !pressed || self.x_held {
                    self.x_held = false;
                    return None;
                }
                return self.short_x(now_ms, context);
            }
            Input::Turn(delta) => {
                match self.screen {
                    Screen::Slots => self.selected = (i32::from(self.selected) + i32::from(delta)).rem_euclid(4) as u8,
                    Screen::Saved => {
                        let step = usize::from(delta.unsigned_abs()) % self.pages;
                        self.page = if delta < 0 {
                            if step > self.page {
                                self.pages - (step - self.page)
                            } else {
                                self.page - step
                            }
                        } else if step >= self.pages - self.page {
                            step - (self.pages - self.page)
                        } else {
                            self.page + step
                        };
                    }
                    _ => {}
                }
            }
            Input::Click => {
                match self.screen {
                    Screen::Slots if self.pending.is_none() => {
                        match self.slots[usize::from(self.selected)] {
                            Slot::Saved { .. } | Slot::SavedBlocked { .. } => {
                                return self.start(Operation::Load, now_ms, context, false);
                            }
                            Slot::Empty => self.fail(UiError::Empty),
                            Slot::Corrupt => self.fail(UiError::Corrupt),
                            Slot::Unsupported => self.fail(UiError::Unsupported),
                            Slot::Unknown | Slot::Unavailable => self.fail(UiError::Unavailable),
                        }
                    }
                    Screen::ConfirmOverwrite | Screen::ConfirmReset => {
                        let Some(confirm) = self.confirmation else {
                            self.fail(UiError::Changed);
                            return None;
                        };
                        if confirm.slot != self.slots[usize::from(self.selected)]
                            || (self.screen == Screen::ConfirmOverwrite
                                && (confirm.revision != context.revision || context.save != SaveAvailability::Ready))
                        {
                            self.fail(UiError::Changed);
                            return None;
                        }
                        let op = if self.screen == Screen::ConfirmReset {
                            Operation::Reset
                        } else {
                            Operation::Save
                        };
                        return self.start(op, now_ms, context, true);
                    }
                    _ => {}
                }
            }
            Input::Back | Input::Recover => unreachable!(),
        }
        None
    }

    fn short_x(
        &mut self,
        now_ms: u64,
        context: Context,
    ) -> Option<Action> {
        match self.screen {
            Screen::Live => return Some(Action::LiveDetails),
            Screen::Saved => self.screen = Screen::Slots,
            Screen::Slots if self.pending.is_none() => {
                match self.slots[usize::from(self.selected)] {
                    Slot::Corrupt | Slot::Unsupported | Slot::SavedBlocked { .. } => {
                        self.confirmation = Some(Confirmation {
                            revision: context.revision,
                            slot: self.slots[usize::from(self.selected)],
                            at_ms: now_ms,
                        });
                        self.screen = Screen::ConfirmReset;
                    }
                    Slot::Unknown | Slot::Unavailable => self.fail(UiError::Unavailable),
                    slot => {
                        match context.save {
                            SaveAvailability::NoPeer => {
                                self.fail(UiError::NoPeer);
                                return None;
                            }
                            SaveAvailability::Busy => {
                                self.fail(UiError::Busy);
                                return None;
                            }
                            SaveAvailability::Ready => {}
                        }
                        if slot == Slot::Empty {
                            return self.start(Operation::Save, now_ms, context, false);
                        }
                        self.confirmation = Some(Confirmation {
                            revision: context.revision,
                            slot,
                            at_ms: now_ms,
                        });
                        self.screen = Screen::ConfirmOverwrite;
                    }
                }
            }
            _ => {}
        }
        None
    }

    /// Back/recovery/timeout only hide an operation: retain its fence until the
    /// actual owner completes. Never cancel a flash write or overlap new I/O.
    pub fn complete(
        &mut self,
        id: RequestId,
        completion: Completion,
    ) -> bool {
        self.complete_ref(id, &completion)
    }

    /// Apply the mailbox record without another full directory copy on stack.
    pub fn complete_ref(
        &mut self,
        id: RequestId,
        completion: &Completion,
    ) -> bool {
        let Some(pending) = self.pending.filter(|p| p.id == id) else {
            return false;
        };
        let screen = match (pending.operation, completion) {
            (_, Completion::Failed(error)) => {
                self.invalidate_identity(pending);
                Screen::Error(*error)
            }
            (Operation::Refresh, Completion::Listed { slots, identities }) => {
                self.slots = *slots;
                self.identities = *identities;
                for (slot, identity) in self.slots.iter().zip(&mut self.identities) {
                    if !matches!(slot, Slot::Saved { .. } | Slot::SavedBlocked { .. }) {
                        *identity = None;
                    }
                }
                Screen::Slots
            }
            (Operation::Load, Completion::Loaded { pages }) if *pages > 0 => {
                self.pages = *pages;
                self.page = 0;
                Screen::Saved
            }
            (Operation::Save, Completion::Saved { generation, identity }) if *generation > 0 => {
                self.slots[usize::from(pending.slot)] = readable_slot(*generation, false);
                self.identities[usize::from(pending.slot)] = Some(*identity);
                Screen::Slots
            }
            (Operation::Reset, Completion::Reset) => {
                self.slots[usize::from(pending.slot)] = Slot::Empty;
                self.identities[usize::from(pending.slot)] = None;
                Screen::Slots
            }
            _ => return false,
        };
        self.pending = None;
        if pending.visible {
            self.screen = screen;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn request_identity_exhaustion_never_reuses_an_effect() {
        let mut ui = CaptureUi::new();
        ui.next_id = u32::MAX;
        let context = Context {
            save: SaveAvailability::Ready,
            revision: 1,
        };
        ui.input(Input::XDown, 0, context);
        assert_eq!(ui.tick(HOLD_MS), None);
        assert_eq!(ui.screen(), Screen::Error(UiError::Exhausted));
        assert_eq!(ui.pending(), None);
    }
}
