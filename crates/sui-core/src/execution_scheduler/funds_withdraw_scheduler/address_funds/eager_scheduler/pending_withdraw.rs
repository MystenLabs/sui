// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeMap, sync::Arc};

use parking_lot::Mutex;
use sui_types::{
    accumulator_root::AccumulatorObjId, base_types::SequenceNumber, digests::TransactionDigest,
};
use tokio::sync::oneshot::Sender;
use tracing::debug;

use crate::execution_scheduler::funds_withdraw_scheduler::{
    ScheduleStatus, TxFundsWithdraw, address_funds::ScheduleResult,
};

pub(crate) struct PendingWithdraw {
    accumulator_version: SequenceNumber,
    tx_digest: TransactionDigest,
    sender: Mutex<Option<Sender<ScheduleResult>>>,
    pending: Mutex<BTreeMap<AccumulatorObjId, u64>>,
}

impl PendingWithdraw {
    pub fn new(
        accumulator_version: SequenceNumber,
        withdraw: TxFundsWithdraw,
        sender: Sender<ScheduleResult>,
    ) -> Arc<Self> {
        Arc::new(Self {
            accumulator_version,
            tx_digest: withdraw.tx_digest,
            sender: Mutex::new(Some(sender)),
            pending: Mutex::new(withdraw.reservations),
        })
    }

    pub fn accumulator_version(&self) -> SequenceNumber {
        self.accumulator_version
    }

    pub fn pending_amount(&self, account_id: &AccumulatorObjId) -> u128 {
        self.pending.lock().get(account_id).copied().unwrap() as u128
    }

    pub fn remove_pending_account(&self, account_id: &AccumulatorObjId) {
        let mut pending = self.pending.lock();
        pending.remove(account_id).unwrap();
        if pending.is_empty() {
            let sender = self.sender.lock().take().unwrap();
            let _ = sender.send(ScheduleResult {
                tx_digest: self.tx_digest,
                status: ScheduleStatus::SufficientFunds,
            });
        }
    }

    pub fn notify_insufficient_funds(&self) {
        let mut sender_guard = self.sender.lock();
        // sender may be None because this pending withdraw may have multiple
        // insufficient accounts, and when processing the first one, the sender
        // is already taken.
        if let Some(sender) = sender_guard.take() {
            debug!("Insufficient funds for withdraw");
            let _ = sender.send(ScheduleResult {
                tx_digest: self.tx_digest,
                status: ScheduleStatus::InsufficientFunds,
            });
        }
    }
}
