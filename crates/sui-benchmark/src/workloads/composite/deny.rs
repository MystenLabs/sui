// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use mysten_common::assert_reachable;
use sui_types::base_types::{ObjectID, ObjectRef, SequenceNumber, SuiAddress};
use sui_types::transaction::{CallArg, ObjectArg};
use tracing::info;

use super::lifecycle::{
    Lifecycle, LifecycleCall, LifecycleStep, StepContext, find_object, pure, shared_mut,
};
use super::operations::{TEST_COIN_DENY, TEST_COIN_UNDENY};
use crate::workloads::payload::BatchedTransactionStatus;
use sui_types::SUI_DENY_LIST_OBJECT_ID;

/// Batches that the partner address stays denied before the undeny step, so that TEST_COIN txs
/// from both payloads run against the denial.
const DENIED_BATCHES: usize = 20;

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
    cap_id: ObjectID,
    retry_pending: bool,
    deny_list_initial_shared_version: SequenceNumber,
    target: SuiAddress,
    denied: bool,
    /// Batches stepped since the deny succeeded.
    denied_batches: usize,
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
            cap_id: cap.0,
            retry_pending: false,
            deny_list_initial_shared_version,
            target,
            denied: false,
            denied_batches: 0,
        }
    }

    fn waiting_while_denied(&self) -> bool {
        self.denied && self.denied_batches < DENIED_BATCHES
    }
}

impl Lifecycle for DenyLifecycle {
    fn max_step_calls(&self) -> usize {
        if self.cap.is_none() || self.waiting_while_denied() {
            0
        } else {
            1
        }
    }

    /// Each step is one tx, and the address stays denied until the undeny step.
    fn in_progress(&self) -> bool {
        self.denied || self.retry_pending || self.cap.is_none()
    }

    fn start_probability(&self) -> f64 {
        if self.cap.is_some() {
            self.start_probability
        } else {
            0.0
        }
    }

    fn next_step(&mut self, _ctx: &StepContext) -> Vec<LifecycleCall> {
        let Some(cap) = self.cap else {
            return vec![];
        };
        if self.waiting_while_denied() {
            self.denied_batches += 1;
            info!(
                batches = self.denied_batches,
                "TEST_COIN partner remains denied"
            );
            return vec![];
        }
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

    fn object_to_refresh(&self) -> Option<ObjectID> {
        self.cap.is_none().then_some(self.cap_id)
    }

    fn refresh_object(&mut self, object: ObjectRef) {
        if self.cap.is_none() && object.0 == self.cap_id {
            info!(cap = ?object, denied = self.denied, "recovered TEST_COIN deny cap");
            self.cap = Some(object);
        }
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
            BatchedTransactionStatus::PermanentFailure { .. }
            | BatchedTransactionStatus::RetriableFailure { .. }
            | BatchedTransactionStatus::UnknownRejection => {
                // Even a rejection can follow an earlier, uncertain attempt that consumed
                // this version. Read the cap again, then retry the same idempotent deny or
                // undeny operation rather than abandoning the only deny-cap owner.
                info!(?step, cap = ?cap, "refreshing TEST_COIN deny cap after unsuccessful submission");
                self.cap = None;
                self.retry_pending = true;
                return;
            }
        };
        // The cap is an owned input, so every executed tx bumps its version.
        self.cap = find_object(effects, cap.0).map(|(obj_ref, _)| obj_ref);
        if !effects.is_ok() {
            return;
        }
        self.retry_pending = false;
        match step {
            DenyStep::Deny => {
                assert_reachable!("partner address denied for TEST_COIN");
                self.denied = true;
                self.denied_batches = 0;
                info!(cap = ?self.cap, "TEST_COIN partner denied; starting denied batch window");
            }
            DenyStep::Undeny => {
                assert_reachable!("partner address undenied for TEST_COIN");
                self.denied = false;
                info!(cap = ?self.cap, "TEST_COIN partner undenied");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sui_types::base_types::random_object_ref;

    #[test]
    fn undeny_follows_the_denied_window() {
        let mut lifecycle = DenyLifecycle::new(
            0.05,
            random_object_ref(),
            SequenceNumber::from_u64(1),
            SuiAddress::random_for_testing_only(),
        );
        let ctx = StepContext {
            sender: SuiAddress::random_for_testing_only(),
            partner: SuiAddress::random_for_testing_only(),
        };
        assert!(!lifecycle.in_progress());
        assert_eq!(lifecycle.op_of_next_step(&ctx), Some(TEST_COIN_DENY));

        lifecycle.denied = true;
        lifecycle.denied_batches = 0;
        for _ in 0..DENIED_BATCHES {
            assert!(lifecycle.in_progress());
            assert_eq!(lifecycle.max_step_calls(), 0);
            assert!(lifecycle.next_step(&ctx).is_empty());
        }
        assert_eq!(lifecycle.max_step_calls(), 1);
        assert_eq!(lifecycle.op_of_next_step(&ctx), Some(TEST_COIN_UNDENY));
    }

    #[test]
    fn unsuccessful_steps_recover_the_cap_and_retry_without_advancing() {
        for denied in [false, true] {
            for status in [
                BatchedTransactionStatus::UnknownRejection,
                BatchedTransactionStatus::RetriableFailure {
                    error: "expired".to_owned(),
                },
                BatchedTransactionStatus::PermanentFailure {
                    error: "stale cap after an uncertain attempt".to_owned(),
                },
            ] {
                let cap = random_object_ref();
                let mut lifecycle = DenyLifecycle::new(
                    0.05,
                    cap,
                    SequenceNumber::from_u64(1),
                    SuiAddress::random_for_testing_only(),
                );
                let ctx = StepContext {
                    sender: SuiAddress::random_for_testing_only(),
                    partner: SuiAddress::random_for_testing_only(),
                };
                lifecycle.denied = denied;
                lifecycle.denied_batches = DENIED_BATCHES;
                let step = if denied {
                    DenyStep::Undeny
                } else {
                    DenyStep::Deny
                };
                lifecycle.handle_result(LifecycleStep::Deny(step), &status);

                assert_eq!(lifecycle.object_to_refresh(), Some(cap.0));
                assert!(lifecycle.in_progress());
                assert_eq!(lifecycle.max_step_calls(), 0);
                assert!(lifecycle.next_step(&ctx).is_empty());
                assert_eq!(lifecycle.denied, denied);
                assert_eq!(lifecycle.denied_batches, DENIED_BATCHES);

                let refreshed = (cap.0, cap.1.next(), random_object_ref().2);
                lifecycle.refresh_object(refreshed);
                assert_eq!(lifecycle.object_to_refresh(), None);
                assert_eq!(lifecycle.cap, Some(refreshed));
                assert!(lifecycle.in_progress());
                assert_eq!(lifecycle.max_step_calls(), 1);
                let calls = lifecycle.next_step(&ctx);
                assert_eq!(
                    calls[0].op,
                    if denied {
                        TEST_COIN_UNDENY
                    } else {
                        TEST_COIN_DENY
                    }
                );
                let super::super::lifecycle::LifecycleCommand::MoveCall { args, .. } =
                    &calls[0].command
                else {
                    panic!("deny steps must be Move calls");
                };
                assert!(
                    matches!(args[1], CallArg::Object(ObjectArg::ImmOrOwnedObject(input)) if input == refreshed)
                );
                assert_eq!(lifecycle.denied, denied);
            }
        }
    }

    #[test]
    fn recovery_rejects_an_unrelated_object() {
        let cap = random_object_ref();
        let mut lifecycle = DenyLifecycle::new(
            0.05,
            cap,
            SequenceNumber::from_u64(1),
            SuiAddress::random_for_testing_only(),
        );
        lifecycle.handle_result(
            LifecycleStep::Deny(DenyStep::Deny),
            &BatchedTransactionStatus::UnknownRejection,
        );
        lifecycle.refresh_object(random_object_ref());
        assert_eq!(lifecycle.object_to_refresh(), Some(cap.0));
        assert!(lifecycle.cap.is_none());
    }

    impl DenyLifecycle {
        fn op_of_next_step(&mut self, ctx: &StepContext) -> Option<&'static str> {
            let calls = self.next_step(ctx);
            assert!(calls.len() <= 1);
            calls.first().map(|call| call.op)
        }
    }
}
