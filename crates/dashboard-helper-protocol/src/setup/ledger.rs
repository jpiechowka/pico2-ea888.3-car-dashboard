//! Finite ordered setup execution and projections; no processes or external effects are performed here.
use super::*;
use crate::*;
#[derive(Clone, Debug, PartialEq, Eq)]
/// One admitted tuple and the only executor permitted to start it.
pub struct SetupAssignment {
    /// Allowlisted tool/stage/command identity.
    pub step: SetupStepId,
    /// Original owner or retained peer association.
    pub executor: SetupExecutor,
}
/// Private immutable finite ordering. The Setup owner supplies dependency-valid policies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetupPlan {
    /// One through 82 distinct tuples in owner-selected dependency order.
    entries: Vec<SetupAssignment>,
    /// Peer topology admitted for this attempt.
    shape: DelegationShape,
}
impl SetupPlan {
    /// Return an immutable plan from `entries` and `shape`, rejecting empty/oversized/duplicate/illegal tuples.
    /// Cold plans forbid Peer1; any Peer1 requires a Peer0 assignment. The caller must validate real dependency
    /// ordering and executable authority: membership alone does not supply either.
    pub fn new(
        entries: Vec<SetupAssignment>,
        shape: DelegationShape,
    ) -> Result<Self, Error> {
        if entries.is_empty() || entries.len() > 82 {
            return Err(Error::Bounds);
        }
        for (i, e) in entries.iter().enumerate() {
            e.step.validate()?;
            if entries[..i].iter().any(|p| p.step == e.step)
                || (shape == DelegationShape::Cold && e.executor == SetupExecutor::Peer1)
            {
                return Err(Error::Schema);
            }
        }
        if entries.iter().any(|e| e.executor == SetupExecutor::Peer1)
            && !entries.iter().any(|e| e.executor == SetupExecutor::Peer0)
        {
            return Err(Error::Schema);
        }
        Ok(Self { entries, shape })
    }

    /// Return the borrowed execution order; callers cannot mutate an admitted plan.
    pub fn entries(&self) -> &[SetupAssignment] { &self.entries }

    /// Return the admitted peer topology.
    pub fn shape(&self) -> DelegationShape { self.shape }
}
#[derive(Clone, Debug, PartialEq, Eq)]
/// Single ordered setup owner: at most one active step and one finish-receipt barrier.
pub struct SetupLedger {
    /// Immutable tuple/executor assignment.
    plan: SetupPlan,
    /// Evidence indexed one-for-one with plan entries; completed records survive stop/rejection.
    outcomes: Vec<SetupStepOutcome>,
    /// Running step index, retained on stop until an explicit completion boundary.
    active: Option<usize>,
    /// Completed peer step awaiting owner acknowledgement; blocks all executors.
    receipt: Option<SetupStepId>,
    /// First stop cause, overridden only by OwnershipCleanupFatal.
    stop: Option<SetupFailure>,
}
impl SetupLedger {
    /// Return a new owner of `plan` with Pending outcomes, no active work/receipt and no stop latch.
    pub fn new(plan: SetupPlan) -> Self {
        let outcomes = plan
            .entries
            .iter()
            .map(|e| {
                SetupStepOutcome {
                    step: e.step,
                    state: StageState::Pending,
                    failure: None,
                    raw_exit: None,
                    elapsed_ms: 0,
                    output_incomplete: false,
                }
            })
            .collect();
        Self {
            plan,
            outcomes,
            active: None,
            receipt: None,
            stop: None,
        }
    }

    /// Return the next unfinished index for `executor`/`step`, without mutation.
    /// Reject active work, receipt barriers, stop latches or mismatched order; return Closed after all entries finish.
    fn next(
        &self,
        executor: SetupExecutor,
        step: SetupStepId,
    ) -> Result<usize, Error> {
        if self.active.is_some() || self.receipt.is_some() || self.stop.is_some() {
            return Err(Error::InvalidState);
        }
        let i = self
            .outcomes
            .iter()
            .position(|o| !o.state.terminal())
            .ok_or(Error::Closed)?;
        if self.plan.entries[i].executor != executor || self.plan.entries[i].step != step {
            return Err(Error::InvalidState);
        }
        Ok(i)
    }

    /// Mark the exact next `step` assigned to `executor` Running; return success or leave state unchanged on error.
    pub fn start_next(
        &mut self,
        executor: SetupExecutor,
        step: SetupStepId,
    ) -> Result<(), Error> {
        let i = self.next(executor, step)?;
        self.outcomes[i].state = StageState::Running;
        self.active = Some(i);
        Ok(())
    }

    /// Commit validated terminal `outcome` for the active step; return an error without mutation on mismatch.
    /// Only Succeeded/Failed/Cancelled complete active work. A prior stop forbids success; a failure cause stops
    /// pending entries but preserves completed facts. Peer callers separately install a receipt barrier.
    pub fn finish_active(
        &mut self,
        outcome: SetupStepOutcome,
    ) -> Result<(), Error> {
        outcome.validate()?;
        let i = self.active.ok_or(Error::InvalidState)?;
        if outcome.step != self.outcomes[i].step
            || !matches!(
                outcome.state,
                StageState::Succeeded | StageState::Failed | StageState::Cancelled
            )
            || (self.stop.is_some() && outcome.state == StageState::Succeeded)
        {
            return Err(Error::InvalidState);
        }
        let failure = outcome.failure;
        self.outcomes[i] = outcome;
        self.active = None;
        if let Some(f) = failure {
            self.stop(f);
        }
        Ok(())
    }

    /// Mark the next `step` for `executor` Skipped without failure; return an error without mutation if blocked.
    /// Caller must establish that this prerequisite is unnecessary; this ledger does not inspect tools.
    pub fn skip_next_unnecessary(
        &mut self,
        executor: SetupExecutor,
        step: SetupStepId,
    ) -> Result<(), Error> {
        let i = self.next(executor, step)?;
        self.outcomes[i].state = StageState::Skipped;
        Ok(())
    }

    /// Latch `cause` and skip currently Pending entries with the retained cause.
    /// Preserve Running/completed records and any receipt; only OwnershipCleanupFatal overrides an earlier cause.
    pub fn stop(
        &mut self,
        cause: SetupFailure,
    ) {
        if self.stop.is_none() || cause == SetupFailure::OwnershipCleanupFatal {
            self.stop = Some(cause);
        }
        for o in &mut self.outcomes {
            if o.state == StageState::Pending {
                o.state = StageState::Skipped;
                o.failure = self.stop;
            }
        }
    }

    /// Hold global admission for completed `step`; private transcript code calls this once after valid completion.
    pub(super) fn hold_receipt(
        &mut self,
        step: SetupStepId,
    ) {
        self.receipt = Some(step);
    }

    /// Return success and release the barrier only for matching `step`; errors preserve the outstanding receipt.
    pub(super) fn acknowledge_receipt(
        &mut self,
        step: SetupStepId,
    ) -> Result<(), Error> {
        if self.receipt != Some(step) {
            return Err(Error::InvalidState);
        }
        self.receipt = None;
        Ok(())
    }

    /// Return all five stage summaries from current step evidence, including pending/running work.
    pub fn projection(&self) -> [SetupStageOutcome; 5] { aggregate(&self.outcomes) }

    /// Return the active tuple, or None after completion/before start; stop alone does not clear it.
    pub fn active(&self) -> Option<SetupStepId> { self.active.map(|i| self.outcomes[i].step) }

    /// Return immutable evidence in execution-plan order.
    pub fn outcomes(&self) -> &[SetupStepOutcome] { &self.outcomes }

    /// Return the retained stop cause; None does not imply every step has completed.
    pub fn stop_cause(&self) -> Option<SetupFailure> { self.stop }

    /// Return cloned evidence assigned to `executor`, preserving plan order and including unfinished entries.
    pub fn assigned_outcomes(
        &self,
        executor: SetupExecutor,
    ) -> Vec<SetupStepOutcome> {
        self.outcomes
            .iter()
            .zip(&self.plan.entries)
            .filter(|(_, e)| e.executor == executor)
            .map(|(o, _)| o.clone())
            .collect()
    }

    /// Return success only when structurally valid `result` has the expected role and exact assigned outcomes.
    /// Attempt/context/action/process checks belong to the lease owner; result validation never mutates this ledger.
    pub fn validate_result(
        &self,
        result: &SetupContinuationResult,
    ) -> Result<(), Error> {
        result.validate()?;
        let executor = match result.peer_slot {
            0 => SetupExecutor::Peer0,
            1 => SetupExecutor::Peer1,
            _ => return Err(Error::Schema),
        };
        let role = if self.plan.shape == DelegationShape::Native && result.peer_slot == 0 {
            SetupRole::Bootstrap
        } else {
            SetupRole::Continuation
        };
        if result.role != role || self.assigned_outcomes(executor).as_slice() != result.step_outcomes.as_ref() {
            return Err(Error::InvalidState);
        }
        Ok(())
    }

    /// Return stable tool-order summaries for `stage`, omitting tools with no assigned entries in that stage.
    /// Interrupted groups retain a generic cause marker; this projection contains no command output.
    pub fn tool_projection(
        &self,
        stage: SetupStage,
    ) -> Vec<ToolSummary> {
        TOOLS
            .into_iter()
            .filter_map(|tool| {
                let group: Vec<_> = self
                    .outcomes
                    .iter()
                    .filter(|o| o.step.stage == stage && o.step.tool == tool)
                    .cloned()
                    .collect();
                if group.is_empty() {
                    None
                } else {
                    let (state, failure) = bucket(&group);
                    Some(ToolSummary {
                        tool,
                        state,
                        reason: if state == StageState::Skipped || failure.is_some() {
                            Some(
                                Text::new(if failure.is_some() {
                                    "setup interrupted"
                                } else {
                                    "not required"
                                })
                                .expect("fixed text"),
                            )
                        } else {
                            None
                        },
                    })
                }
            })
            .collect()
    }
}
/// Return aggregate state/cause for `outcomes`: unfinished work wins, then Failed, Cancelled and skipped causes.
/// Empty/all-skipped groups are Skipped. Failure selection prefers the first Failed cause, then first Cancelled,
/// then first caused Skipped record; successful execution is not inferred from an empty group.
fn bucket(outcomes: &[SetupStepOutcome]) -> (StageState, Option<SetupFailure>) {
    let failure = outcomes
        .iter()
        .find(|o| o.state == StageState::Failed)
        .and_then(|o| o.failure)
        .or_else(|| {
            outcomes
                .iter()
                .find(|o| o.state == StageState::Cancelled)
                .and_then(|o| o.failure)
        })
        .or_else(|| {
            outcomes.iter().find_map(|o| {
                if o.state == StageState::Skipped {
                    o.failure
                } else {
                    None
                }
            })
        });
    let executed = outcomes.iter().any(|o| {
        matches!(
            o.state,
            StageState::Running | StageState::Succeeded | StageState::Failed | StageState::Cancelled
        )
    });
    if outcomes
        .iter()
        .any(|o| matches!(o.state, StageState::Pending | StageState::Running))
    {
        return (
            if executed {
                StageState::Running
            } else {
                StageState::Pending
            },
            failure,
        );
    }
    if outcomes.iter().all(|o| o.state == StageState::Skipped) {
        return (StageState::Skipped, failure);
    }
    if outcomes.iter().any(|o| o.state == StageState::Failed) {
        return (StageState::Failed, failure);
    }
    if outcomes.iter().any(|o| o.state == StageState::Cancelled) {
        return (StageState::Cancelled, failure);
    }
    if let Some(f) = failure {
        return (
            if f == SetupFailure::Cancelled {
                StageState::Cancelled
            } else {
                StageState::Failed
            },
            Some(f),
        );
    }
    (StageState::Succeeded, None)
}
/// Return the five fixed stage projections of `outcomes`; does not validate tuples, duplicates or ordering.
pub fn aggregate(outcomes: &[SetupStepOutcome]) -> [SetupStageOutcome; 5] {
    SETUP_STAGES.map(|stage| {
        let group: Vec<_> = outcomes.iter().filter(|o| o.step.stage == stage).cloned().collect();
        let (state, failure) = bucket(&group);
        SetupStageOutcome { stage, state, failure }
    })
}
