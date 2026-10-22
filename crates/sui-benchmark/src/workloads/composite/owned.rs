// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_common::random::get_rng;
use mysten_common::{assert_reachable, debug_fatal};
use rand::Rng;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber};
use sui_types::object::Owner;
use sui_types::transaction::{CallArg, ObjectArg};
use tracing::debug;

use super::lifecycle::{
    Lifecycle, LifecycleCall, LifecycleCommand, LifecycleStep, StepContext, find_object, pure,
    shared_mut,
};
use super::operations::{
    COIN_MERGE, COIN_MINT, COIN_STALE_MERGE, OWNED_CONFLICTING_MUTATE, OWNED_CREATE, OWNED_DELETE,
    OWNED_FREEZE, OWNED_FROZEN_MUTATE, OWNED_FROZEN_READ, OWNED_MUTATE, OWNED_STALE_MUTATE,
};
use crate::workloads::payload::BatchedTransactionStatus;

fn owned(obj_ref: ObjectRef) -> CallArg {
    CallArg::Object(ObjectArg::ImmOrOwnedObject(obj_ref))
}

#[derive(Clone, Copy, Debug)]
pub(super) enum OwnedStep {
    Create,
    Mutate,
    /// Freeze the object.
    Freeze,
    /// Delete the object.
    Delete,
    /// Mutate the object at the version that a freeze or delete in the same batch consumes.
    ConflictingMutate,
    /// Read the frozen object.
    FrozenRead,
    /// Mutate the frozen object (expected to be rejected).
    FrozenMutate,
    /// Mutate the deleted object (expected to be rejected).
    StaleMutate,
}

/// A payload's owned-object lifecycle: `NeedCreate -> Owned -> (Frozen | Deleted) ->
/// NeedCreate`. The freeze or delete tx is sent together with a mutation that uses the same
/// owned version, so at most one of them executes.
#[derive(Debug, Clone)]
enum OwnedPhase {
    NeedCreate,
    Owned { obj_ref: ObjectRef, mutations: u32 },
    Frozen { obj_ref: ObjectRef },
    Deleted { obj_ref: ObjectRef },
}

#[derive(Debug)]
pub(super) struct OwnedLifecycle {
    start_probability: f64,
    phase: OwnedPhase,
}

impl OwnedLifecycle {
    const MAX_MUTATIONS: u32 = 2;

    pub fn new(start_probability: f64) -> Self {
        Self {
            start_probability,
            phase: OwnedPhase::NeedCreate,
        }
    }

    fn call(
        function: &'static str,
        args: Vec<CallArg>,
        op: &'static str,
        step: OwnedStep,
    ) -> LifecycleCall {
        LifecycleCall::move_call("owned_obj", function, args, op, LifecycleStep::Owned(step))
    }

    fn id(&self) -> Option<ObjectID> {
        match &self.phase {
            OwnedPhase::NeedCreate => None,
            OwnedPhase::Owned { obj_ref, .. }
            | OwnedPhase::Frozen { obj_ref }
            | OwnedPhase::Deleted { obj_ref } => Some(obj_ref.0),
        }
    }
}

impl Lifecycle for OwnedLifecycle {
    fn in_progress(&self) -> bool {
        !matches!(self.phase, OwnedPhase::NeedCreate)
    }

    fn start_probability(&self) -> f64 {
        self.start_probability
    }

    fn next_step(&mut self, ctx: &StepContext) -> Vec<LifecycleCall> {
        let mut rng = get_rng();
        match self.phase.clone() {
            // The create tx is bare, so the object is the only one it creates.
            OwnedPhase::NeedCreate => vec![
                Self::call(
                    "create",
                    vec![pure(&ctx.sender)],
                    OWNED_CREATE,
                    OwnedStep::Create,
                )
                .bare(),
            ],
            OwnedPhase::Owned { obj_ref, mutations } => {
                let mutate = |value: u64, op, step| {
                    Self::call("set_value", vec![owned(obj_ref), pure(&value)], op, step)
                };
                if mutations < Self::MAX_MUTATIONS && rng.gen_bool(0.5) {
                    return vec![mutate(1, OWNED_MUTATE, OwnedStep::Mutate)];
                }
                let end = if rng.gen_bool(0.5) {
                    Self::call(
                        "freeze_obj",
                        vec![owned(obj_ref)],
                        OWNED_FREEZE,
                        OwnedStep::Freeze,
                    )
                } else {
                    Self::call(
                        "delete",
                        vec![owned(obj_ref)],
                        OWNED_DELETE,
                        OwnedStep::Delete,
                    )
                };
                vec![
                    end,
                    mutate(2, OWNED_CONFLICTING_MUTATE, OwnedStep::ConflictingMutate),
                ]
            }
            OwnedPhase::Frozen { obj_ref } => {
                self.phase = OwnedPhase::NeedCreate;
                vec![
                    Self::call(
                        "value",
                        vec![owned(obj_ref)],
                        OWNED_FROZEN_READ,
                        OwnedStep::FrozenRead,
                    ),
                    Self::call(
                        "set_value",
                        vec![owned(obj_ref), pure(&3u64)],
                        OWNED_FROZEN_MUTATE,
                        OwnedStep::FrozenMutate,
                    )
                    .bare(),
                ]
            }
            OwnedPhase::Deleted { obj_ref } => {
                self.phase = OwnedPhase::NeedCreate;
                vec![
                    Self::call(
                        "set_value",
                        vec![owned(obj_ref), pure(&4u64)],
                        OWNED_STALE_MUTATE,
                        OwnedStep::StaleMutate,
                    )
                    .bare(),
                ]
            }
        }
    }

    fn handle_result(&mut self, step: LifecycleStep, status: &BatchedTransactionStatus) {
        let LifecycleStep::Owned(step) = step else {
            unreachable!("step {step:?} routed to the owned lifecycle");
        };
        let effects = match status {
            BatchedTransactionStatus::Success { effects } => effects,
            BatchedTransactionStatus::PermanentFailure { .. } => {
                match step {
                    OwnedStep::FrozenMutate => {
                        assert_reachable!("mutation of a frozen object rejected")
                    }
                    OwnedStep::StaleMutate => {
                        assert_reachable!("mutation of a deleted owned object rejected")
                    }
                    _ => {}
                }
                return;
            }
            BatchedTransactionStatus::RetriableFailure { .. }
            | BatchedTransactionStatus::UnknownRejection => {
                if !matches!(step, OwnedStep::FrozenRead | OwnedStep::FrozenMutate) {
                    debug!(?step, "owned tx outcome unknown, resetting lifecycle");
                    self.phase = OwnedPhase::NeedCreate;
                }
                return;
            }
        };

        match step {
            OwnedStep::FrozenRead => {
                if effects.is_ok() {
                    assert_reachable!("frozen object read");
                }
                return;
            }
            OwnedStep::FrozenMutate | OwnedStep::StaleMutate => {
                if effects.is_ok() {
                    debug_fatal!(
                        "{step:?} tx succeeded on an object that is frozen or deleted: {:?}",
                        effects.digest()
                    );
                } else if matches!(step, OwnedStep::FrozenMutate) {
                    assert_reachable!("mutation of a frozen object failed in execution");
                }
                return;
            }
            OwnedStep::Create => {
                if let OwnedPhase::NeedCreate = self.phase
                    && let Some((obj_ref, _)) = effects.created().into_iter().next()
                {
                    self.phase = OwnedPhase::Owned {
                        obj_ref,
                        mutations: 0,
                    };
                }
                return;
            }
            _ => {}
        }

        // The freeze, delete and mutations below use the object as an owned input, so an
        // executed tx always changes it.
        let Some(id) = self.id() else {
            return;
        };
        let OwnedPhase::Owned { mutations, .. } = self.phase else {
            // The other tx of the conflicting pair already executed.
            return;
        };
        if let Some(deleted) = effects.deleted().into_iter().find(|r| r.0 == id) {
            assert_reachable!("owned object deleted");
            self.phase = OwnedPhase::Deleted { obj_ref: deleted };
            return;
        }
        match find_object(effects, id) {
            Some((obj_ref, Owner::Immutable)) => {
                assert_reachable!("owned object frozen");
                self.phase = OwnedPhase::Frozen { obj_ref };
            }
            Some((obj_ref, _)) => {
                if matches!(step, OwnedStep::ConflictingMutate) && effects.is_ok() {
                    assert_reachable!("mutation won a conflict with a freeze or delete");
                }
                self.phase = OwnedPhase::Owned {
                    obj_ref,
                    mutations: mutations + 1,
                };
            }
            None => {
                debug!(
                    ?step,
                    "owned object missing from effects, resetting lifecycle"
                );
                self.phase = OwnedPhase::NeedCreate;
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum CoinStep {
    Mint,
    Merge,
    /// Merge a coin that an earlier merge deleted (expected to be rejected).
    StaleMerge,
}

/// A payload's coin-merge lifecycle: `NeedCoins -> Coins -> Merged -> NeedCoins`. The merge
/// step sends two `MergeCoins` txs in opposite directions, so at most one executes.
#[derive(Debug, Clone)]
enum CoinPhase {
    NeedCoins,
    Coins {
        a: ObjectRef,
        b: ObjectRef,
    },
    Merged {
        survivor: ObjectRef,
        deleted: ObjectRef,
    },
}

#[derive(Debug)]
pub(super) struct CoinMergeLifecycle {
    start_probability: f64,
    test_coin_cap: (ObjectID, SequenceNumber),
    phase: CoinPhase,
}

impl CoinMergeLifecycle {
    pub fn new(start_probability: f64, test_coin_cap: (ObjectID, SequenceNumber)) -> Self {
        Self {
            start_probability,
            test_coin_cap,
            phase: CoinPhase::NeedCoins,
        }
    }

    fn merge(
        target: ObjectRef,
        coin: ObjectRef,
        op: &'static str,
        step: CoinStep,
    ) -> LifecycleCall {
        LifecycleCall::new(
            LifecycleCommand::MergeCoins {
                target,
                coins: vec![coin],
            },
            op,
            LifecycleStep::CoinMerge(step),
        )
    }
}

impl Lifecycle for CoinMergeLifecycle {
    fn in_progress(&self) -> bool {
        !matches!(self.phase, CoinPhase::NeedCoins)
    }

    fn start_probability(&self) -> f64 {
        self.start_probability
    }

    fn next_step(&mut self, ctx: &StepContext) -> Vec<LifecycleCall> {
        match self.phase.clone() {
            // The mint tx is bare, so the two coins are the only objects it creates.
            CoinPhase::NeedCoins => vec![
                LifecycleCall::move_call(
                    "owned_obj",
                    "mint_coins",
                    vec![
                        shared_mut(self.test_coin_cap.0, self.test_coin_cap.1),
                        pure(&ctx.sender),
                    ],
                    COIN_MINT,
                    LifecycleStep::CoinMerge(CoinStep::Mint),
                )
                .bare(),
            ],
            CoinPhase::Coins { a, b } => vec![
                Self::merge(a, b, COIN_MERGE, CoinStep::Merge),
                Self::merge(b, a, COIN_MERGE, CoinStep::Merge),
            ],
            CoinPhase::Merged { survivor, deleted } => {
                self.phase = CoinPhase::NeedCoins;
                vec![Self::merge(survivor, deleted, COIN_STALE_MERGE, CoinStep::StaleMerge).bare()]
            }
        }
    }

    fn handle_result(&mut self, step: LifecycleStep, status: &BatchedTransactionStatus) {
        let LifecycleStep::CoinMerge(step) = step else {
            unreachable!("step {step:?} routed to the coin-merge lifecycle");
        };
        let effects = match status {
            BatchedTransactionStatus::Success { effects } => effects,
            BatchedTransactionStatus::PermanentFailure { .. } => {
                if matches!(step, CoinStep::StaleMerge) {
                    assert_reachable!("merge of a deleted coin rejected");
                }
                return;
            }
            BatchedTransactionStatus::RetriableFailure { .. }
            | BatchedTransactionStatus::UnknownRejection => {
                if matches!(step, CoinStep::Merge) {
                    debug!("coin merge outcome unknown, resetting lifecycle");
                    self.phase = CoinPhase::NeedCoins;
                }
                return;
            }
        };
        match (step, &self.phase) {
            (CoinStep::Mint, CoinPhase::NeedCoins) => {
                let coins: Vec<ObjectRef> = effects
                    .created()
                    .into_iter()
                    .map(|(obj_ref, _)| obj_ref)
                    .collect();
                if let [a, b] = coins[..] {
                    self.phase = CoinPhase::Coins { a, b };
                }
            }
            (CoinStep::Merge, CoinPhase::Coins { a, b }) => {
                let deleted = effects.deleted();
                let (survivor_id, deleted_ref) =
                    if let Some(r) = deleted.iter().find(|r| r.0 == b.0) {
                        (a.0, *r)
                    } else if let Some(r) = deleted.iter().find(|r| r.0 == a.0) {
                        (b.0, *r)
                    } else {
                        // The merge aborted or was cancelled. Both coins were inputs, so both
                        // versions changed.
                        match (find_object(effects, a.0), find_object(effects, b.0)) {
                            (Some((a, _)), Some((b, _))) => self.phase = CoinPhase::Coins { a, b },
                            _ => self.phase = CoinPhase::NeedCoins,
                        }
                        return;
                    };
                assert_reachable!("owned coin merged");
                match find_object(effects, survivor_id) {
                    Some((survivor, _)) => {
                        self.phase = CoinPhase::Merged {
                            survivor,
                            deleted: deleted_ref,
                        };
                    }
                    None => self.phase = CoinPhase::NeedCoins,
                }
            }
            (CoinStep::StaleMerge, _) => {
                if effects.is_ok() {
                    debug_fatal!("merge of a deleted coin succeeded: {:?}", effects.digest());
                }
            }
            // The other merge of the pair already executed.
            (CoinStep::Merge, _) => {}
            (step, phase) => unreachable!("coin tx {step:?} built in phase {phase:?}"),
        }
    }
}
