use super::*;
use crate::*;
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetupAssignment {
    pub step: SetupStepId,
    pub executor: SetupExecutor,
}
/// Private immutable finite ordering. The Setup owner supplies dependency-valid policies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetupPlan {
    entries: Vec<SetupAssignment>,
    shape: DelegationShape,
}
impl SetupPlan {
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

    pub fn entries(&self) -> &[SetupAssignment] { &self.entries }

    pub fn shape(&self) -> DelegationShape { self.shape }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetupLedger {
    plan: SetupPlan,
    outcomes: Vec<SetupStepOutcome>,
    active: Option<usize>,
    receipt: Option<SetupStepId>,
    stop: Option<SetupFailure>,
}
impl SetupLedger {
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

    pub fn skip_next_unnecessary(
        &mut self,
        executor: SetupExecutor,
        step: SetupStepId,
    ) -> Result<(), Error> {
        let i = self.next(executor, step)?;
        self.outcomes[i].state = StageState::Skipped;
        Ok(())
    }

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

    pub(super) fn hold_receipt(
        &mut self,
        step: SetupStepId,
    ) {
        self.receipt = Some(step);
    }

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

    pub fn projection(&self) -> [SetupStageOutcome; 5] { aggregate(&self.outcomes) }

    pub fn active(&self) -> Option<SetupStepId> { self.active.map(|i| self.outcomes[i].step) }

    pub fn outcomes(&self) -> &[SetupStepOutcome] { &self.outcomes }

    pub fn stop_cause(&self) -> Option<SetupFailure> { self.stop }

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
pub fn aggregate(outcomes: &[SetupStepOutcome]) -> [SetupStageOutcome; 5] {
    SETUP_STAGES.map(|stage| {
        let group: Vec<_> = outcomes.iter().filter(|o| o.step.stage == stage).cloned().collect();
        let (state, failure) = bucket(&group);
        SetupStageOutcome { stage, state, failure }
    })
}
