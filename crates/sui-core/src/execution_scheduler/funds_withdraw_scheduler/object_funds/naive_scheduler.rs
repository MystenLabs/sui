// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use parking_lot::RwLock;
use sui_types::{
    accumulator_root::AccumulatorObjId, base_types::SequenceNumber,
    execution_params::FundsWithdrawStatus,
};
use tokio::sync::{oneshot, watch};
use tokio_util::sync::CancellationToken;

use crate::{
    accumulators::funds_read::AccountFundsRead,
    execution_scheduler::funds_withdraw_scheduler::object_funds::{
        ObjectFundsWithdrawSchedulerTrait, ObjectFundsWithdrawStatus,
    },
};

#[derive(Clone)]
pub(crate) struct NaiveObjectFundsWithdrawScheduler {
    funds_read: Arc<dyn AccountFundsRead>,
    inner: Arc<RwLock<Inner>>,
    accumulator_version_sender: Arc<watch::Sender<SequenceNumber>>,
    // We must keep a receiver alive to make sure sends go through and can update the last settled version.
    accumulator_version_receiver: Arc<watch::Receiver<SequenceNumber>>,
    epoch_ended: Arc<CancellationToken>,
}

struct Inner {
    /// Unsettled withdraws in the current consensus commit (identified by the accumulator version).
    /// This is cleared whenever we settle a new version.
    /// We must track these because when we execute a transaction, the witdhraws are not immediately settled,
    /// so we need to track them and check them again when we execute the next transaction from the same consensus commit.
    unsettled_withdraws: BTreeMap<AccumulatorObjId, u128>,
}

impl NaiveObjectFundsWithdrawScheduler {
    pub fn new(
        funds_read: Arc<dyn AccountFundsRead>,
        starting_accumulator_version: SequenceNumber,
    ) -> Self {
        let (accumulator_version_sender, accumulator_version_receiver) =
            watch::channel(starting_accumulator_version);
        Self {
            funds_read,
            inner: Arc::new(RwLock::new(Inner {
                unsettled_withdraws: BTreeMap::new(),
            })),
            accumulator_version_sender: Arc::new(accumulator_version_sender),
            accumulator_version_receiver: Arc::new(accumulator_version_receiver),
            epoch_ended: Arc::new(CancellationToken::new()),
        }
    }

    fn try_withdraw(&self, object_withdraws: &BTreeMap<AccumulatorObjId, u64>) -> bool {
        for (obj_id, amount) in object_withdraws {
            // It is safe to get the latest funds here because this function is called during execution,
            // which means this transaction is not committed yet,
            // so the settlement transaction at the end of the same consensus commit cannot have settled yet.
            // That is, we must be blocked by this transaction in order to make progress.
            let funds = self.funds_read.get_latest_account_amount(obj_id);
            let unsettled_withdraw = self
                .inner
                .read()
                .unsettled_withdraws
                .get(obj_id)
                .copied()
                .unwrap_or_default();
            assert!(funds >= unsettled_withdraw);
            if funds - unsettled_withdraw < *amount as u128 {
                return false;
            }
        }
        let mut inner = self.inner.write();
        for (obj_id, amount) in object_withdraws {
            let entry = inner.unsettled_withdraws.entry(*obj_id).or_default();
            *entry += *amount as u128;
        }
        true
    }

    fn return_insufficient_funds() -> ObjectFundsWithdrawStatus {
        let (sender, receiver) = oneshot::channel();
        // unwrap is safe because the receiver is defined right above.
        sender.send(FundsWithdrawStatus::Insufficient).unwrap();
        ObjectFundsWithdrawStatus::Pending(receiver)
    }
}

impl ObjectFundsWithdrawSchedulerTrait for NaiveObjectFundsWithdrawScheduler {
    fn schedule(
        &self,
        object_withdraws: BTreeMap<AccumulatorObjId, u64>,
        accumulator_version: SequenceNumber,
    ) -> ObjectFundsWithdrawStatus {
        let last_settled_version = *self.accumulator_version_receiver.borrow();
        // This function is called during execution, which means this transaction is not committed yet,
        // so the settlement transaction at the end of the same consensus commit cannot have settled yet.
        assert!(
            accumulator_version >= last_settled_version,
            "accumulator_version: {}, last_settled_version: {}",
            accumulator_version,
            last_settled_version
        );
        if accumulator_version == last_settled_version {
            if self.try_withdraw(&object_withdraws) {
                return ObjectFundsWithdrawStatus::SufficientFunds;
            } else {
                return Self::return_insufficient_funds();
            }
        }

        // Spawn a task to wait for the last settled version to become accumulator_version,
        // before we could check again.
        let accumulator_version_sender = self.accumulator_version_sender.clone();
        let epoch_cancel = self.epoch_ended.child_token();
        let (sender, receiver) = oneshot::channel();
        tokio::spawn(async move {
            let mut version_receiver = accumulator_version_sender.subscribe();
            tokio::select! {
                res = version_receiver.wait_for(|v| *v >= accumulator_version) => {
                    if res.is_err() {
                        // This shouldn't happen, but just to be safe.
                        tracing::error!("Accumulator version receiver channel closed while waiting for accumulator version");
                        return;
                    }
                    // We notify the waiter that the funds are now deterministically known,
                    // but we don't need to check here whether they are sufficient or not.
                    // Next time during execution we will check again.
                    let _ = sender.send(FundsWithdrawStatus::MaybeSufficient);
                }
                _ = epoch_cancel.cancelled() => {}
            }
        });
        ObjectFundsWithdrawStatus::Pending(receiver)
    }

    fn settle_accumulator_version(&self, next_accumulator_version: SequenceNumber) {
        let mut inner = self.inner.write();
        // unwrap is safe because the scheduler always holds a reference to the receiver.
        self.accumulator_version_sender
            .send(next_accumulator_version)
            .unwrap();
        inner.unsettled_withdraws.clear();
    }

    fn close_epoch(&self) {
        self.epoch_ended.cancel();
    }

    #[cfg(test)]
    fn get_current_accumulator_version(&self) -> SequenceNumber {
        *self.accumulator_version_receiver.borrow()
    }
}
