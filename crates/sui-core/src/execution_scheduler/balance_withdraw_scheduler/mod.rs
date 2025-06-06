// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    digests::TransactionDigest,
};
use tokio::sync::oneshot;

mod balance_read;
mod naive_scheduler;
pub(crate) mod scheduler;
#[cfg(test)]
mod tests;

/// The result of scheduling the withdraw reservations for a transaction.
#[allow(dead_code)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum ScheduleResult {
    /// We know for sure that the transaction has enough balance to satisfy all withdraw reservations.
    SufficientBalance,
    /// We know for sure that the transaction does not have enough balance to satisfy all withdraw reservations.
    InsufficientBalance,
    /// This transaction has already been scheduled in the past.
    /// This is used to avoid scheduling the same transaction multiple times.
    AlreadyScheduled,
}

/// Details regarding a balance settlement, generated when a settlementAdd commentMore actions
/// transaction has been executed and committed to the writeback cache.
#[allow(dead_code)]
pub(crate) struct BalanceSettlement {
    /// The accumulator version at which the settlement was committed.
    /// i.e. the root accumulator object is now at this version.
    pub accumulator_version: SequenceNumber,
    pub balance_changes: BTreeMap<ObjectID, i128>,
}

/// Details regarding all balance withdraw reservations in a transaction.
#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct TxBalanceWithdraw {
    pub tx_digest: TransactionDigest,
    pub reservations: BTreeMap<ObjectID, u64>,
}

#[allow(dead_code)]
pub(crate) struct WithdrawReservations {
    pub accumulator_version: SequenceNumber,
    pub withdraws: Vec<TxBalanceWithdraw>,
    pub senders: Vec<oneshot::Sender<ScheduleResult>>,
}

impl WithdrawReservations {
    #[allow(dead_code)]
    pub fn new(
        accumulator_version: SequenceNumber,
        withdraws: Vec<TxBalanceWithdraw>,
    ) -> (
        Self,
        BTreeMap<TransactionDigest, oneshot::Receiver<ScheduleResult>>,
    ) {
        let (senders, receivers) = withdraws
            .iter()
            .map(|withdraw| {
                let (sender, receiver) = oneshot::channel();
                (sender, (withdraw.tx_digest, receiver))
            })
            .unzip();
        (
            Self {
                accumulator_version,
                withdraws,
                senders,
            },
            receivers,
        )
    }
}
