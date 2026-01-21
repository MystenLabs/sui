// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use parking_lot::RwLock;
use sui_types::{
    accumulator_root::AccumulatorObjId, base_types::SequenceNumber,
    execution_params::FundsWithdrawStatus,
};
use tokio::sync::{oneshot, watch};
use tracing::debug;

use crate::accumulators::funds_read::AccountFundsRead;

/// Note that there is no need to have a separate InsufficientFunds variant.
/// If the funds are insufficient, the execution would still have to abort and rely on
/// a rescheduling to be able to execute again.
pub(crate) enum ObjectFundsCheckStatus {
    SufficientFunds,
    // The receiver will be notified when the funds are determined to be sufficient or insufficient.
    // The bool is true if the funds are sufficient, false if the funds are insufficient.
    Pending(oneshot::Receiver<FundsWithdrawStatus>),
}

/// Tracks pending object funds withdraws that have not yet been settled.
/// This is used during execution to determine if a transaction has sufficient
/// funds to perform its withdraws.
#[derive(Clone)]
pub(crate) struct PendingObjectFundsWithdraws {
    inner: Arc<RwLock<Inner>>,
    accumulator_version_sender: Arc<watch::Sender<SequenceNumber>>,
    // We must keep a receiver alive to make sure sends go through and can update the last settled version.
    accumulator_version_receiver: Arc<watch::Receiver<SequenceNumber>>,
}

struct Inner {
    /// The accumulator version of the most recent object funds withdraw transaction that was checked.
    /// We use this to track whether we have moved on to a new consensus commit.
    last_checked_accumulator_version: SequenceNumber,
    /// Unsettled withdraws in the current consensus commit (identified by the accumulator version).
    /// This is cleared whenever we settle a new version.
    /// We must track these because when we execute a transaction, the withdraws are not immediately settled,
    /// so we need to track them and check them again when we execute the next transaction from the same consensus commit.
    unsettled_withdraws: BTreeMap<AccumulatorObjId, u128>,
}

impl PendingObjectFundsWithdraws {
    pub fn new(starting_accumulator_version: SequenceNumber) -> Self {
        let (accumulator_version_sender, accumulator_version_receiver) =
            watch::channel(starting_accumulator_version);
        Self {
            inner: Arc::new(RwLock::new(Inner {
                last_checked_accumulator_version: starting_accumulator_version,
                unsettled_withdraws: BTreeMap::new(),
            })),
            accumulator_version_sender: Arc::new(accumulator_version_sender),
            accumulator_version_receiver: Arc::new(accumulator_version_receiver),
        }
    }

    fn try_withdraw(
        &self,
        funds_read: &dyn AccountFundsRead,
        object_withdraws: &BTreeMap<AccumulatorObjId, u64>,
        accumulator_version: SequenceNumber,
    ) -> bool {
        for (obj_id, amount) in object_withdraws {
            // It is safe to get the latest funds here because this function is called during execution,
            // which means this transaction is not committed yet,
            // so the settlement transaction at the end of the same consensus commit cannot have settled yet.
            // That is, we must be blocked by this transaction in order to make progress.
            let funds = funds_read.get_account_amount_at_version(obj_id, accumulator_version);
            let unsettled_withdraw = self
                .inner
                .read()
                .unsettled_withdraws
                .get(obj_id)
                .copied()
                .unwrap_or_default();
            debug!(
                ?obj_id,
                ?funds,
                ?accumulator_version,
                ?unsettled_withdraw,
                ?amount,
                "Trying to withdraw"
            );
            assert!(funds >= unsettled_withdraw);
            if funds - unsettled_withdraw < *amount as u128 {
                return false;
            }
        }
        let mut inner = self.inner.write();
        for (obj_id, amount) in object_withdraws {
            let entry = inner.unsettled_withdraws.entry(*obj_id).or_default();
            debug!(?obj_id, ?amount, ?entry, "Updating unsettled withdraws");
            *entry += *amount as u128;
        }
        true
    }

    fn return_insufficient_funds() -> ObjectFundsCheckStatus {
        let (sender, receiver) = oneshot::channel();
        // unwrap is safe because the receiver is defined right above.
        sender.send(FundsWithdrawStatus::Insufficient).unwrap();
        ObjectFundsCheckStatus::Pending(receiver)
    }

    /// Check if the given object withdraws can be performed at the given accumulator version.
    pub fn check(
        &self,
        funds_read: &dyn AccountFundsRead,
        object_withdraws: BTreeMap<AccumulatorObjId, u64>,
        accumulator_version: SequenceNumber,
    ) -> ObjectFundsCheckStatus {
        let last_settled_version = *self.accumulator_version_receiver.borrow();
        let last_checked_version = {
            let mut inner = self.inner.write();
            let last_checked_version = inner.last_checked_accumulator_version;
            if accumulator_version > last_checked_version {
                inner.last_checked_accumulator_version = accumulator_version;
                inner.unsettled_withdraws.clear();
            }
            debug!(
                last_settled_version =? last_settled_version.value(),
                last_checked_version =? last_checked_version.value(),
                withdraw_accumulator_version =? accumulator_version.value(),
                "Checking object funds withdraws"
            );
            last_checked_version
        };
        // If the accumulator version is behind the last checked version, we can't reliably
        // track the state (since we've already cleared old unsettled withdraws for that version).
        // Return Insufficient since we can't guarantee the funds are available.
        if accumulator_version < last_checked_version {
            return Self::return_insufficient_funds();
        }
        // It is possible for the settled version to be ahead of the last checked version,
        // because settlement transactions that come from checkpoint executor do not depend
        // on the object funds withdraws, and can execute in parallel or in advance.
        if accumulator_version <= last_settled_version {
            if self.try_withdraw(funds_read, &object_withdraws, accumulator_version) {
                return ObjectFundsCheckStatus::SufficientFunds;
            } else {
                return Self::return_insufficient_funds();
            }
        }

        self.wait_for_settlement(accumulator_version)
    }

    /// Spawn a task to wait for the given accumulator version to be settled.
    fn wait_for_settlement(&self, accumulator_version: SequenceNumber) -> ObjectFundsCheckStatus {
        let accumulator_version_sender = self.accumulator_version_sender.clone();
        let (sender, receiver) = oneshot::channel();
        tokio::spawn(async move {
            let mut version_receiver = accumulator_version_sender.subscribe();
            let res = version_receiver
                .wait_for(|v| *v >= accumulator_version)
                .await;
            if res.is_err() {
                // This shouldn't happen, but just to be safe.
                tracing::error!(
                    "Accumulator version receiver channel closed while waiting for accumulator version"
                );
                return;
            }
            // We notify the waiter that the funds are now deterministically known,
            // but we don't need to check here whether they are sufficient or not.
            // Next time during execution we will check again.
            let _ = sender.send(FundsWithdrawStatus::MaybeSufficient);
        });
        ObjectFundsCheckStatus::Pending(receiver)
    }

    /// Notify that the given accumulator version has been settled.
    pub fn settle_accumulator_version(&self, next_accumulator_version: SequenceNumber) {
        // unwrap is safe because this struct always holds a reference to the receiver.
        self.accumulator_version_sender
            .send(next_accumulator_version)
            .unwrap();
    }

    #[cfg(test)]
    pub fn get_current_accumulator_version(&self) -> SequenceNumber {
        *self.accumulator_version_receiver.borrow()
    }
}
