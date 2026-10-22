//! Bounded capture controls. Effects carry non-reused identities; firmware owns bytes and I/O.
use super::names::SavedIdentity;

/// Raw X hold duration in milliseconds, shared with the hardware input contract.
pub const HOLD_MS: u64 = crate::input_model::BUTTON_HOLD_MS;
/// Milliseconds before visible operation feedback times out; does not cancel owner I/O.
pub const OPERATION_TIMEOUT_MS: u64 = 30_000;
/// Milliseconds before overwrite/reset intent expires without I/O or a new storage fault.
pub const CONFIRM_TIMEOUT_MS: u64 = 10_000;
/// Nonzero checked-increment operation identity; exhaustion never wraps.
pub type RequestId = u32;

/// User-visible admission state of both journal copies for one capture slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    /// Directory has not yet been read.
    Unknown,
    /// Both journal copies are erased.
    Empty,
    /// Readable capture that can be replaced after explicit confirmation.
    Saved {
        /// Selected nonzero journal generation.
        generation: u64,
    },
    /// A readable copy exists, but replacement requires destructive reset.
    SavedBlocked {
        /// Readable generation; may be exhausted or accompanied by an unsupported copy.
        generation: u64,
    },
    /// No readable selected copy and at least one nonempty invalid copy.
    Corrupt,
    /// Unsupported schema prevents interpretation and ordinary replacement.
    Unsupported,
    /// Storage/directory state cannot currently be read.
    Unavailable,
}

/// Returns the readable slot class for `generation`; `replacement_blocked` or maximum
/// generation requires reset before replacement. This projection does not validate generation zero.
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
/// Returns UI state from `info`, retaining unsupported-copy protection even with a
/// selected readable capture. With no selected copy, unsupported takes precedence
/// over all-empty, then corrupt. Performs no journal I/O.
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
/// Producer's current ability to freeze live evidence for saving.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveAvailability {
    /// Stable selected-peer evidence can be frozen now.
    Ready,
    /// No selected peer can supply capture identity.
    NoPeer,
    /// Live owner is transitioning or otherwise cannot freeze evidence.
    Busy,
}
/// Live-source snapshot used to bind save and confirmation to a discovery generation.
#[derive(Clone, Copy, Debug)]
pub struct Context {
    /// Current producer permission to freeze a live capture.
    pub save: SaveAvailability,
    /// Discovery revision that the save must use; rechecked after overwrite confirmation.
    pub revision: u32,
}
/// Capture UI inputs; use either debounced semantic X events or raw replay X edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Input {
    /// Hardware should send semantic events after producer-side debounce.
    XShort,
    /// Debounced completed X hold; requests a slot-directory refresh.
    XHold,
    /// Raw model/replay events; do not mix these with semantic events.
    XDown,
    /// Raw X release; emits short action only if no hold was recognized.
    XUp,
    /// Wrap slot/page selection without issuing I/O.
    Turn(
        /// Signed normalized encoder steps.
        i16,
    ),
    /// Load selected readable slot or accept the current overwrite/reset confirmation.
    Click,
    /// Leave the current screen, hiding any pending operation without cancelling it.
    Back,
    /// Return to live state and request outer recovery, retaining pending I/O fencing.
    Recover,
}
/// Mutually exclusive storage operations tracked by the UI fence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    /// Read all four slot directory entries.
    Refresh,
    /// Load selected frozen capture for browsing.
    Load,
    /// Freeze live evidence and journal it to the selected slot.
    Save,
    /// Explicitly erase/reset the selected slot's two-copy journal.
    Reset,
}
/// Bounded admission/operation feedback; each outcome preserves any pending I/O fence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiError {
    /// Save requested without a live peer.
    NoPeer,
    /// Existing I/O or live-source state prevents starting the operation.
    Busy,
    /// Load requested for an empty slot.
    Empty,
    /// Storage owner reported an I/O failure.
    Storage,
    /// Slot contents fail admission.
    Corrupt,
    /// Slot contains an unsupported schema requiring explicit reset.
    Unsupported,
    /// Slot state has not been read or cannot be accessed.
    Unavailable,
    /// Visible operation deadline expired or its time reversed; I/O remains fenced until completion.
    Timeout,
    /// Overwrite/reset intent expired before any I/O; preserve slots and existing faults, then re-confirm.
    ConfirmationExpired,
    /// Confirmed slot/generation/live-source revision no longer matches.
    Changed,
    /// Raw button or confirmation timing moved backwards; no mutation is authorized.
    Clock,
    /// Request identity counter reached its maximum and cannot safely restart at zero.
    Exhausted,
}
/// Capture overlay navigation independent of the live probe screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    /// Live probe controls are shown.
    Live,
    /// Four-slot catalogue and selection are shown.
    Slots,
    /// Visible feedback for storage work still in flight.
    Busy(
        /// Operation whose progress is shown.
        Operation,
    ),
    /// Loaded frozen capture pages are shown.
    Saved,
    /// Explicit click is required to replace a readable capture.
    ConfirmOverwrite,
    /// Explicit click is required to reset protected/corrupt slot contents.
    ConfirmReset,
    /// Admission/operation feedback; does not itself cancel pending owner work.
    Error(
        /// Bounded feedback presented to the operator; confirmation expiry is not an I/O failure.
        UiError,
    ),
}
/// Effect for the firmware owner; the pure UI never performs storage or BLE operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Request the four-slot directory.
    Refresh {
        /// Unique operation identity echoed by completion.
        id: RequestId,
    },
    /// Request a frozen capture from a readable slot.
    Load {
        /// Unique operation identity echoed by completion.
        id: RequestId,
        /// Zero-based slot index in 0..4.
        slot: u8,
    },
    /// Request a live-source freeze and journal write.
    Save {
        /// Unique operation identity echoed by completion.
        id: RequestId,
        /// Zero-based destination slot index in 0..4.
        slot: u8,
        /// Live discovery generation whose evidence must be frozen.
        revision: u32,
        /// Explicit overwrite confirmation was accepted for this operation.
        overwrite: bool,
    },
    /// Request destructive reset after explicit confirmation.
    Reset {
        /// Unique operation identity echoed by completion.
        id: RequestId,
        /// Zero-based slot index in 0..4.
        slot: u8,
    },
    /// Forward short X to the live probe's details action.
    LiveDetails,
    /// Forward protected recovery to the outer probe owner.
    Recover,
}
/// Storage-owner outcome, checked against both request identity and expected operation.
#[derive(Clone, Copy, Debug)]
pub enum Completion {
    /// Directory read completed for all four slots.
    Listed {
        /// Both-copy admission state per zero-based slot.
        slots: [Slot; 4],
        /// Frozen peer/name identity per slot; discarded for nonreadable states.
        identities: [Option<SavedIdentity>; 4],
    },
    /// Capture loaded into owner-controlled immutable browse storage.
    Loaded {
        /// Nonzero number of rendered pages; zero is rejected without releasing the fence.
        pages: usize,
    },
    /// Journal save succeeded with a new nonzero generation.
    Saved {
        /// Published journal generation; maximum makes future replacement reset-only.
        generation: u64,
        /// Peer/name identity of the actual saved capture.
        identity: SavedIdentity,
    },
    /// Requested slot reset completed.
    Reset,
    /// Owner finished with a failure and released its I/O ownership.
    Failed(
        /// Failure to show only if the operation is still visible.
        UiError,
    ),
}
/// Affine UI operation fence retained until a matching valid owner completion.
#[derive(Clone, Copy)]
struct Pending {
    /// Nonreused completion correlation identity.
    id: RequestId,
    /// Expected completion variant.
    operation: Operation,
    /// Selected slot at operation start, independent of later navigation.
    slot: u8,
    /// Boot-relative start time in milliseconds.
    at_ms: u64,
    /// Completion may still navigate the UI; cleared by back/recovery/timeout.
    visible: bool,
}
/// Frozen overwrite/reset intent, expiring without performing any I/O.
#[derive(Clone, Copy)]
struct Confirmation {
    /// Live-source generation at confirmation creation.
    revision: u32,
    /// Exact selected slot state, including its generation.
    slot: Slot,
    /// Boot-relative creation time in milliseconds.
    at_ms: u64,
}

/// Bounded capture UI state machine with one pending owner operation and four slots.
pub struct CaptureUi {
    /// Current live/slot/browse/confirmation/error presentation.
    screen: Screen,
    /// Last directory or matching mutation result for each slot.
    slots: [Slot; 4],
    /// Frozen display identities, invalidated when corresponding evidence becomes uncertain.
    identities: [Option<SavedIdentity>; 4],
    /// Zero-based selected slot, always below four.
    selected: u8,
    /// Zero-based loaded-capture page, always below `pages`.
    page: usize,
    /// Nonzero loaded-capture page count; one before a capture loads.
    pages: usize,
    /// Last allocated request identity; zero before the first request.
    next_id: RequestId,
    /// Storage-operation fence, including hidden or timed-out work.
    pending: Option<Pending>,
    /// Current explicit overwrite/reset confirmation, if any.
    confirmation: Option<Confirmation>,
    /// Boot-relative raw X press time in milliseconds; absent outside raw press tracking.
    x_down: Option<u64>,
    /// Raw X hold already emitted, suppressing the release tap.
    x_held: bool,
}
impl Default for CaptureUi {
    /// Returns live-screen state with unknown slots and no pending operation.
    fn default() -> Self { Self::new() }
}
impl CaptureUi {
    /// Returns live-screen state, four unknown slots, selection/page zero and no pending
    /// operation; the first allocated identity will be one. Performs no I/O.
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

    /// Returns `self`'s current presentation state without changing pending work.
    pub fn screen(&self) -> Screen { self.screen }

    /// Returns `self`'s four slot-admission states by reference.
    pub fn slots(&self) -> &[Slot; 4] { &self.slots }

    /// Returns `self`'s four optional frozen identities; absence does not imply an empty slot.
    pub fn identities(&self) -> &[Option<SavedIdentity>; 4] { &self.identities }

    /// Returns `self`'s selected zero-based slot index in 0..4.
    pub fn selected(&self) -> u8 { self.selected }

    /// Returns `self`'s zero-based loaded-capture page index.
    pub fn page(&self) -> usize { self.page }

    /// Returns `self`'s outstanding operation identity, including hidden/timed-out work,
    /// or `None` once a matching valid completion has released the fence.
    pub fn pending(&self) -> Option<RequestId> { self.pending.map(|p| p.id) }

    /// Shows `error` in `self` and clears confirmation, returning unit; does not release I/O fencing.
    fn fail(
        &mut self,
        error: UiError,
    ) {
        self.confirmation = None;
        self.screen = Screen::Error(error);
    }

    /// Clears `self`'s cached identities affected by `pending`: all for refresh, otherwise
    /// its start-time slot. Returns unit; slot admission states are retained.
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

    /// Starts `operation` for `self`'s selected slot at boot-relative `now_ms` milliseconds,
    /// using `context`'s live revision and `overwrite` authorization for a save.
    /// Returns an owner action after allocating a checked identity and fence; returns
    /// `None` with Busy/Exhausted feedback if work is pending or identities are exhausted.
    /// Caller validates operation-specific availability/confirmation before entering here.
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
    /// At boot-relative `now_ms` milliseconds, expires visible operation/confirmation
    /// feedback and recognizes a raw X hold in `self`. Returns a refresh action once per
    /// hold, otherwise `None`. Normal confirmation expiry emits no I/O or storage fault;
    /// reversed confirmation/raw-press time reports Clock. Reversed operation time
    /// reports Timeout. Timed-out I/O remains fenced until the owner completes it.
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
        if let Some(confirm) = self.confirmation {
            match now_ms.checked_sub(confirm.at_ms) {
                None => self.fail(UiError::Clock),
                Some(age) if age >= CONFIRM_TIMEOUT_MS => self.fail(UiError::ConfirmationExpired),
                Some(_) => {}
            }
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

    /// Applies `input` to `self` at boot-relative `now_ms` milliseconds with live-source
    /// `context`. Returns an owner effect for accepted refresh/load/save/reset or live
    /// forwarding; otherwise `None`. Back/recovery hide pending work without cancelling it.
    /// Slot/page turns wrap; overwrite confirmation rechecks live revision, readiness
    /// and exact slot state before allocating a new operation.
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

    /// Handles a short X in `self` at boot-relative `now_ms` milliseconds with live
    /// `context`. Returns live-details or an immediate empty-slot save action; otherwise
    /// returns `None` after navigation, error feedback or opening a bounded confirmation.
    /// Protected/corrupt slots require reset confirmation, independently of live save readiness.
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
    /// Applies owned `completion` for `id` to `self`; returns true only for a matching
    /// pending identity and operation-compatible outcome. See [`Self::complete_ref`].
    pub fn complete(
        &mut self,
        id: RequestId,
        completion: Completion,
    ) -> bool {
        self.complete_ref(id, &completion)
    }

    /// Apply the mailbox record without another full directory copy on stack.
    /// Borrows `completion` for `id`, returning true after applying a matching outcome
    /// and releasing `self`'s pending fence; hidden completions update data without navigation.
    /// Wrong identity/variant, zero loaded pages or zero saved generation return false
    /// without releasing the fence. Any matched Failed outcome invalidates affected identities.
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
/// Identity-exhaustion regression for storage effects.
mod tests {
    use super::*;
    #[test]
    /// Returns unit after verifying maximum request identity produces Exhausted feedback
    /// without an effect or pending fence; panics if an identity can wrap/recur.
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
