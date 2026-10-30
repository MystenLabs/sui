// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_common::assert_reachable;
use sui_types::base_types::{ObjectRef, SequenceNumber, SuiAddress};
use sui_types::transaction::{CallArg, ObjectArg};
use tracing::debug;

use super::lifecycle::{
    Lifecycle, LifecycleCall, LifecycleStep, StepContext, find_object, pure, shared_mut,
};
use super::operations::{TEST_COIN_DENY, TEST_COIN_UNDENY};
use crate::workloads::payload::BatchedTransactionStatus;
use sui_types::SUI_DENY_LIST_OBJECT_ID;

#[derive(Clone, Copy, Debug)]
pub(super) enum DenyStep {
    Deny,
    Undeny,
}

/// Toggles whether the partner address is denied for TEST_COIN while both payloads keep
/// sending TEST_COIN txs. A denial applies to tx inputs at once and to received coins from
/// the next epoch. Only the payload that holds the coin's `DenyCapV2` runs it.
#[derive(Debug)]
pub(super) struct DenyLifecycle {
    start_probability: f64,
    /// `None` once the cap's version is unknown.
    cap: Option<ObjectRef>,
    deny_list_initial_shared_version: SequenceNumber,
    target: SuiAddress,
    denied: bool,
}

impl DenyLifecycle {
    pub fn new(
        start_probability: f64,
        cap: ObjectRef,
        deny_list_initial_shared_version: SequenceNumber,
        target: SuiAddress,
    ) -> Self {
        Self {
            start_probability,
            cap: Some(cap),
            deny_list_initial_shared_version,
            target,
            denied: false,
        }
    }
}

impl Lifecycle for DenyLifecycle {
    fn max_step_calls(&self) -> usize {
        1
    }

    /// Each step is one tx, and the address stays denied until a later batch undenies it.
    fn in_progress(&self) -> bool {
        self.denied && self.cap.is_some()
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
            .expect("a deny step starts only while the cap is known");
        let (function, op, step) = if self.denied {
            ("undeny", TEST_COIN_UNDENY, DenyStep::Undeny)
        } else {
            ("deny", TEST_COIN_DENY, DenyStep::Deny)
        };
        vec![LifecycleCall::move_call(
            "test_coin",
            function,
            vec![
                shared_mut(
                    SUI_DENY_LIST_OBJECT_ID,
                    self.deny_list_initial_shared_version,
                ),
                CallArg::Object(ObjectArg::ImmOrOwnedObject(cap)),
                pure(&self.target),
            ],
            op,
            LifecycleStep::Deny(step),
        )]
    }

    fn handle_result(&mut self, step: LifecycleStep, status: &BatchedTransactionStatus) {
        let LifecycleStep::Deny(step) = step else {
            unreachable!("step {step:?} routed to the deny lifecycle");
        };
        let Some(cap) = self.cap else {
            return;
        };
        let effects = match status {
            BatchedTransactionStatus::Success { effects } => effects,
            BatchedTransactionStatus::PermanentFailure { .. } => return,
            BatchedTransactionStatus::RetriableFailure { .. }
            | BatchedTransactionStatus::UnknownRejection => {
                debug!("deny tx outcome unknown, the deny cap is lost");
                self.cap = None;
                return;
            }
        };
        // The cap is an owned input, so every executed tx bumps its version.
        self.cap = find_object(effects, cap.0).map(|(obj_ref, _)| obj_ref);
        if !effects.is_ok() {
            return;
        }
        match step {
            DenyStep::Deny => {
                assert_reachable!("partner address denied for TEST_COIN");
                self.denied = true;
            }
            DenyStep::Undeny => {
                assert_reachable!("partner address undenied for TEST_COIN");
                self.denied = false;
            }
        }
    }
}
