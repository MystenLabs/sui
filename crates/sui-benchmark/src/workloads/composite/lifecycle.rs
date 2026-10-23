// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Per-payload object lifecycles. A lifecycle tracks an object through state transitions
//! (a consensus stream ends, an object is deleted or wrapped) and submits txs that reference
//! the object's old and new states in the same batch. Each call of a step goes into its own
//! composite tx.

use mysten_common::random::get_rng;
use rand::Rng;
use rand::seq::SliceRandom;
use std::sync::Arc;
use sui_types::SUI_FRAMEWORK_PACKAGE_ID;
use sui_types::base_types::{ObjectID, ObjectRef, SuiAddress};
use sui_types::move_package::UpgradePolicy;
use sui_types::object::Owner;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{CallArg, ObjectArg};
use sui_types::{Identifier, base_types::SequenceNumber};

use super::deny::DenyStep;
use super::owned::{CoinStep, OwnedStep};
use super::party::PartyStep;
use super::receive::ReceiveStep;
use super::shared_deletion::SharedDeletionStep;
use super::upgrade::UpgradeStep;
use crate::ExecutionEffects;
use crate::workloads::payload::BatchedTransactionStatus;

/// Identifies the step that built a tx, so its result can be routed back to the lifecycle.
#[derive(Clone, Copy, Debug)]
pub(super) enum LifecycleStep {
    Party(PartyStep),
    SharedDeletion(SharedDeletionStep),
    Receive(ReceiveStep),
    Upgrade(UpgradeStep),
    Owned(OwnedStep),
    CoinMerge(CoinStep),
    Deny(DenyStep),
}

#[derive(Clone, Debug)]
pub(super) enum LifecycleCommand {
    /// A call into the composite package. The tx builder picks the package version.
    MoveCall {
        module: &'static str,
        function: &'static str,
        args: Vec<CallArg>,
    },
    /// Merge `coins` into `target`, which deletes `coins`.
    MergeCoins {
        target: ObjectRef,
        coins: Vec<ObjectRef>,
    },
    /// Upgrade `package` by republishing the composite package's modules.
    Upgrade {
        cap: ObjectRef,
        package: ObjectID,
        upgrade: Arc<UpgradeData>,
    },
}

#[derive(Debug)]
pub(super) struct UpgradeData {
    pub modules: Vec<Vec<u8>>,
    pub dependencies: Vec<ObjectID>,
    pub digest: Vec<u8>,
}

#[derive(Clone, Debug)]
pub(super) struct LifecycleCall {
    pub command: LifecycleCommand,
    /// The op name recorded in the tx's `OperationSet`.
    pub op: &'static str,
    pub step: LifecycleStep,
    /// A bare call's tx carries no sampled ops, so its lamport version is determined by the
    /// call's inputs and the gas coin alone. Inputs with higher versions would move the
    /// versions of objects it writes away from the versions under test.
    pub bare: bool,
}

impl LifecycleCall {
    pub fn move_call(
        module: &'static str,
        function: &'static str,
        args: Vec<CallArg>,
        op: &'static str,
        step: LifecycleStep,
    ) -> Self {
        Self::new(
            LifecycleCommand::MoveCall {
                module,
                function,
                args,
            },
            op,
            step,
        )
    }

    pub fn new(command: LifecycleCommand, op: &'static str, step: LifecycleStep) -> Self {
        Self {
            command,
            op,
            step,
            bare: false,
        }
    }

    pub fn bare(mut self) -> Self {
        self.bare = true;
        self
    }

    /// Adds the call to `builder`. Move calls target `package`, the composite package version
    /// that the rest of the tx uses, because a tx cannot call two versions of one package.
    pub fn apply(&self, builder: &mut ProgrammableTransactionBuilder, package: ObjectID) {
        match &self.command {
            LifecycleCommand::MoveCall {
                module,
                function,
                args,
            } => {
                builder
                    .move_call(
                        package,
                        Identifier::new(*module).unwrap(),
                        Identifier::new(*function).unwrap(),
                        vec![],
                        args.clone(),
                    )
                    .unwrap();
            }
            LifecycleCommand::MergeCoins { target, coins } => {
                builder.merge_coins(*target, coins.clone()).unwrap();
            }
            LifecycleCommand::Upgrade {
                cap,
                package,
                upgrade,
            } => {
                let cap = builder.obj(ObjectArg::ImmOrOwnedObject(*cap)).unwrap();
                let policy = builder.pure(UpgradePolicy::COMPATIBLE).unwrap();
                let digest = builder.pure(upgrade.digest.clone()).unwrap();
                let ticket = builder.programmable_move_call(
                    SUI_FRAMEWORK_PACKAGE_ID,
                    Identifier::new("package").unwrap(),
                    Identifier::new("authorize_upgrade").unwrap(),
                    vec![],
                    vec![cap, policy, digest],
                );
                let receipt = builder.upgrade(
                    *package,
                    ticket,
                    upgrade.dependencies.clone(),
                    upgrade.modules.clone(),
                );
                builder.programmable_move_call(
                    SUI_FRAMEWORK_PACKAGE_ID,
                    Identifier::new("package").unwrap(),
                    Identifier::new("commit_upgrade").unwrap(),
                    vec![],
                    vec![cap, receipt],
                );
            }
        }
    }
}

pub(super) struct StepContext {
    pub sender: SuiAddress,
    pub partner: SuiAddress,
}

pub(super) trait Lifecycle: Send + Sync + std::fmt::Debug {
    /// True once the lifecycle has started and until it returns to its initial state.
    fn in_progress(&self) -> bool;

    /// Per-batch probability that an idle lifecycle starts.
    fn start_probability(&self) -> f64;

    /// Advances the lifecycle one step and returns that step's calls. The caller submits all
    /// of them concurrently in one batch.
    fn next_step(&mut self, ctx: &StepContext) -> Vec<LifecycleCall>;

    /// Updates the lifecycle from the result of a tx that carried one of its calls. Sampled
    /// ops in the same PTB can abort or reject the tx, so implementations should advance from
    /// the effects on the tracked objects rather than from the tx status.
    fn handle_result(&mut self, step: LifecycleStep, status: &BatchedTransactionStatus);
}

/// Picks at most one lifecycle to step this batch, uniformly among the lifecycles in progress
/// and the idle lifecycles that decide to start. One step per batch keeps lifecycle txs from
/// displacing the shared-counter load of the sampled ops.
pub(super) fn choose_lifecycle(lifecycles: &[Box<dyn Lifecycle>]) -> Option<usize> {
    let mut rng = get_rng();
    let candidates: Vec<usize> = lifecycles
        .iter()
        .enumerate()
        .filter(|(_, l)| l.in_progress() || rng.gen_bool(l.start_probability()))
        .map(|(i, _)| i)
        .collect();
    candidates.choose(&mut rng).copied()
}

pub(super) fn find_object(effects: &ExecutionEffects, id: ObjectID) -> Option<(ObjectRef, Owner)> {
    effects
        .created()
        .into_iter()
        .chain(effects.mutated())
        .chain(effects.unwrapped())
        .find(|(obj_ref, _)| obj_ref.0 == id)
}

pub(super) fn shared_mut(id: ObjectID, initial_shared_version: SequenceNumber) -> CallArg {
    CallArg::Object(ObjectArg::SharedObject {
        id,
        initial_shared_version,
        mutability: sui_types::transaction::SharedObjectMutability::Mutable,
    })
}

pub(super) fn pure<T: serde::Serialize>(value: &T) -> CallArg {
    CallArg::Pure(bcs::to_bytes(value).unwrap())
}
