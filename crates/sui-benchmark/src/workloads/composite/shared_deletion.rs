// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_common::random::get_rng;
use mysten_common::{assert_reachable, debug_fatal};
use rand::Rng;
use sui_types::base_types::{ObjectID, SequenceNumber};
use sui_types::object::Owner;
use sui_types::transaction::{CallArg, ObjectArg, SharedObjectMutability};
use tracing::debug;

use super::lifecycle::{Lifecycle, LifecycleCall, LifecycleStep, StepContext, shared_mut};
use super::operations::{
    SHARED_DELETE, SHARED_DELETE_CREATE, SHARED_DELETE_INCREMENT, SHARED_DELETE_RACING,
    SHARED_DELETE_STALE,
};
use crate::workloads::payload::BatchedTransactionStatus;

#[derive(Clone, Copy, Debug)]
pub(super) enum SharedDeletionStep {
    /// Create and share a counter.
    Create,
    /// Increment the counter on its stream.
    Increment,
    /// Delete the counter.
    Delete,
    /// Increment or read the counter, submitted alongside the delete tx. It may be sequenced
    /// before or after the delete.
    Racing,
    /// Increment the counter after the delete executed (expected to fail).
    Stale,
}

/// A payload's shared-object deletion lifecycle:
/// `NeedCreate -> Live -> Deleted -> NeedCreate`. Deleting a shared object ends its consensus
/// stream. The delete tx is sent with racing txs on the same counter, and the next step sends
/// txs that reference the deleted counter.
#[derive(Debug, Clone)]
enum SharedDeletionPhase {
    NeedCreate,
    Live {
        id: ObjectID,
        initial_shared_version: SequenceNumber,
        increments: u32,
    },
    Deleted {
        id: ObjectID,
        initial_shared_version: SequenceNumber,
    },
}

#[derive(Debug)]
pub(super) struct SharedDeletionLifecycle {
    start_probability: f64,
    phase: SharedDeletionPhase,
}

impl SharedDeletionLifecycle {
    /// The counter is deleted after at most this many increments so the lifecycle keeps
    /// progressing.
    const MAX_INCREMENTS: u32 = 3;

    pub fn new(start_probability: f64) -> Self {
        Self {
            start_probability,
            phase: SharedDeletionPhase::NeedCreate,
        }
    }

    fn call(
        &self,
        function: &'static str,
        args: Vec<CallArg>,
        op: &'static str,
        step: SharedDeletionStep,
    ) -> LifecycleCall {
        LifecycleCall::move_call(
            "counter",
            function,
            args,
            op,
            LifecycleStep::SharedDeletion(step),
        )
    }
}

impl Lifecycle for SharedDeletionLifecycle {
    fn max_step_calls(&self) -> usize {
        3
    }

    fn in_progress(&self) -> bool {
        !matches!(self.phase, SharedDeletionPhase::NeedCreate)
    }

    fn start_probability(&self) -> f64 {
        self.start_probability
    }

    fn next_step(&mut self, _ctx: &StepContext) -> Vec<LifecycleCall> {
        let mut rng = get_rng();
        match self.phase.clone() {
            SharedDeletionPhase::NeedCreate => vec![self.call(
                "create",
                vec![],
                SHARED_DELETE_CREATE,
                SharedDeletionStep::Create,
            )],
            SharedDeletionPhase::Live {
                id,
                initial_shared_version,
                increments,
            } => {
                let counter = shared_mut(id, initial_shared_version);
                if increments < Self::MAX_INCREMENTS && rng.gen_bool(0.5) {
                    return vec![self.call(
                        "increment",
                        vec![counter],
                        SHARED_DELETE_INCREMENT,
                        SharedDeletionStep::Increment,
                    )];
                }
                let mut calls = vec![self.call(
                    "delete",
                    vec![counter.clone()],
                    SHARED_DELETE,
                    SharedDeletionStep::Delete,
                )];
                if rng.gen_bool(0.5) {
                    calls.push(self.call(
                        "increment",
                        vec![counter],
                        SHARED_DELETE_RACING,
                        SharedDeletionStep::Racing,
                    ));
                }
                if rng.gen_bool(0.5) {
                    let read = CallArg::Object(ObjectArg::SharedObject {
                        id,
                        initial_shared_version,
                        mutability: SharedObjectMutability::Immutable,
                    });
                    calls.push(self.call(
                        "value",
                        vec![read],
                        SHARED_DELETE_RACING,
                        SharedDeletionStep::Racing,
                    ));
                }
                calls
            }
            SharedDeletionPhase::Deleted {
                id,
                initial_shared_version,
            } => {
                // The counter is gone, so the next step starts over with a new one.
                self.phase = SharedDeletionPhase::NeedCreate;
                let counter = shared_mut(id, initial_shared_version);
                // Stale txs carry no sampled ops so that their only consensus input is the
                // deleted counter.
                (0..rng.gen_range(1..=2))
                    .map(|_| {
                        self.call(
                            "increment",
                            vec![counter.clone()],
                            SHARED_DELETE_STALE,
                            SharedDeletionStep::Stale,
                        )
                        .bare()
                    })
                    .collect()
            }
        }
    }

    fn handle_result(&mut self, step: LifecycleStep, status: &BatchedTransactionStatus) {
        let LifecycleStep::SharedDeletion(step) = step else {
            unreachable!("step {step:?} routed to the shared-deletion lifecycle");
        };
        let effects = match status {
            BatchedTransactionStatus::Success { effects } => effects,
            BatchedTransactionStatus::PermanentFailure { .. } => return,
            BatchedTransactionStatus::RetriableFailure { .. }
            | BatchedTransactionStatus::UnknownRejection => {
                // The delete may have executed, so the counter can no longer be tracked.
                if matches!(step, SharedDeletionStep::Delete) {
                    debug!("shared delete outcome unknown, resetting lifecycle");
                    self.phase = SharedDeletionPhase::NeedCreate;
                }
                return;
            }
        };

        match (step, &self.phase) {
            (SharedDeletionStep::Racing, _) => {
                if effects.is_input_object_deleted() {
                    assert_reachable!("tx racing a shared object delete executed after it");
                }
            }
            (SharedDeletionStep::Stale, _) => {
                if effects.is_input_object_deleted() {
                    assert_reachable!("tx with a deleted shared input executed");
                } else if effects.is_ok() {
                    debug_fatal!(
                        "tx on a deleted shared counter succeeded: {:?}",
                        effects.digest()
                    );
                }
            }
            (SharedDeletionStep::Create, SharedDeletionPhase::NeedCreate) => {
                if let Some((obj_ref, initial_shared_version)) = effects
                    .created()
                    .into_iter()
                    .find_map(|(obj_ref, owner)| match owner {
                        Owner::Shared {
                            initial_shared_version,
                        } => Some((obj_ref, initial_shared_version)),
                        _ => None,
                    })
                {
                    self.phase = SharedDeletionPhase::Live {
                        id: obj_ref.0,
                        initial_shared_version,
                        increments: 0,
                    };
                }
            }
            (
                SharedDeletionStep::Increment,
                SharedDeletionPhase::Live {
                    id,
                    initial_shared_version,
                    increments,
                },
            ) => {
                self.phase = SharedDeletionPhase::Live {
                    id: *id,
                    initial_shared_version: *initial_shared_version,
                    increments: increments + 1,
                };
            }
            (
                SharedDeletionStep::Delete,
                SharedDeletionPhase::Live {
                    id,
                    initial_shared_version,
                    ..
                },
            ) => {
                if effects.deleted().iter().any(|obj_ref| obj_ref.0 == *id) {
                    assert_reachable!("shared counter deleted by the shared-deletion lifecycle");
                    self.phase = SharedDeletionPhase::Deleted {
                        id: *id,
                        initial_shared_version: *initial_shared_version,
                    };
                }
                // Otherwise the tx aborted or was cancelled, and the counter is still live.
            }
            // An earlier tx in the same batch already reset the phase.
            (_, SharedDeletionPhase::NeedCreate) => {}
            (step, phase) => unreachable!("shared-deletion tx {step:?} built in phase {phase:?}"),
        }
    }
}
