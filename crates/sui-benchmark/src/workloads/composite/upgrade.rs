// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::{Arc, RwLock};

use mysten_common::assert_reachable;
use sui_types::base_types::{ObjectID, ObjectRef};
use sui_types::object::Owner;
use tracing::{debug, info};

use super::lifecycle::{
    Lifecycle, LifecycleCall, LifecycleCommand, LifecycleStep, StepContext, UpgradeData,
    find_object,
};
use super::operations::PACKAGE_UPGRADE;
use crate::workloads::payload::BatchedTransactionStatus;

#[derive(Clone, Copy, Debug)]
pub(super) enum UpgradeStep {
    Upgrade,
}

/// Upgrades the composite package while other txs call it. Every upgrade republishes the same
/// modules, and each new version is added to `versions`, from which every tx picks the version
/// it calls. Only one payload holds the upgrade cap.
#[derive(Debug)]
pub(super) struct UpgradeLifecycle {
    start_probability: f64,
    /// `None` once the cap's version is unknown.
    cap: Option<ObjectRef>,
    upgrade: Arc<UpgradeData>,
    versions: Arc<RwLock<Vec<ObjectID>>>,
}

impl UpgradeLifecycle {
    pub fn new(
        start_probability: f64,
        cap: ObjectRef,
        upgrade: Arc<UpgradeData>,
        versions: Arc<RwLock<Vec<ObjectID>>>,
    ) -> Self {
        Self {
            start_probability,
            cap: Some(cap),
            upgrade,
            versions,
        }
    }
}

impl Lifecycle for UpgradeLifecycle {
    fn in_progress(&self) -> bool {
        false
    }

    fn start_probability(&self) -> f64 {
        if self.cap.is_some() {
            self.start_probability
        } else {
            0.0
        }
    }

    fn next_step(&mut self, _ctx: &StepContext) -> Vec<LifecycleCall> {
        let cap = self
            .cap
            .expect("an upgrade starts only while the cap is known");
        let latest = *self.versions.read().unwrap().last().unwrap();
        vec![
            LifecycleCall::new(
                LifecycleCommand::Upgrade {
                    cap,
                    package: latest,
                    upgrade: self.upgrade.clone(),
                },
                PACKAGE_UPGRADE,
                LifecycleStep::Upgrade(UpgradeStep::Upgrade),
            )
            // Sampled ops could abort the upgrade, and the upgrade's linkage could conflict with
            // the package version that they call.
            .bare(),
        ]
    }

    fn handle_result(&mut self, step: LifecycleStep, status: &BatchedTransactionStatus) {
        let LifecycleStep::Upgrade(_) = step else {
            unreachable!("step {step:?} routed to the upgrade lifecycle");
        };
        let Some(cap) = self.cap else {
            return;
        };
        let effects = match status {
            BatchedTransactionStatus::Success { effects } => effects,
            BatchedTransactionStatus::PermanentFailure { .. } => return,
            BatchedTransactionStatus::RetriableFailure { .. }
            | BatchedTransactionStatus::UnknownRejection => {
                debug!("upgrade tx outcome unknown, the upgrade cap is lost");
                self.cap = None;
                return;
            }
        };
        // The cap is an input, so every executed tx bumps its version.
        self.cap = find_object(effects, cap.0).map(|(obj_ref, _)| obj_ref);
        if !effects.is_ok() {
            return;
        }
        // The tx is bare, so the new package is the only immutable object it creates.
        let package = effects
            .created()
            .into_iter()
            .find_map(|(obj_ref, owner)| (owner == Owner::Immutable).then_some(obj_ref.0))
            .expect("a successful upgrade creates a package");
        assert_reachable!("composite package upgraded");
        let mut versions = self.versions.write().unwrap();
        versions.push(package);
        info!(
            "composite package upgraded to {package}, {} versions",
            versions.len()
        );
    }
}
