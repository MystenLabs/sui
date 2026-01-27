// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use sui_types::{
    accumulator_root::AccumulatorObjId, base_types::SequenceNumber, digests::TransactionDigest,
};
use tokio::sync::oneshot;

mod eager_scheduler;
mod naive_scheduler;
pub(crate) mod scheduler;

#[cfg(test)]
mod e2e_tests;
#[cfg(test)]
mod naive_scheduler_tests;
#[cfg(test)]
mod test_scheduler;

/// The status of scheduling the funds withdraw reservations for a transaction.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum ScheduleStatus {
    /// We know for sure that the withdraw reservations in this transactions all have enough funds.
    /// This transaction can be executed normally as soon as its object dependencies are ready.
    SufficientFunds,
    /// We know for sure that the withdraw reservations in this transactions do not all have enough funds.
    /// This transaction should result in an execution failure without actually executing it, similar to
    /// how transaction cancellation works.
    InsufficientFunds,
    /// We can skip scheduling this transaction, due to one of the following reasons:
    /// 1. The accumulator version for this transaction has already been settled.
    /// 2. We are observing some account objects bumping to the next version, indicating
    ///    that the withdraw transactions in this commit have already been executed and are
    ///    being settled.
    SkipSchedule,
}

/// The result of scheduling the withdraw reservations for a transaction.
pub(crate) enum ScheduleResult {
    ScheduleResult(ScheduleStatus),
    Pending(oneshot::Receiver<ScheduleStatus>),
}

impl ScheduleResult {
    pub fn unwrap_status(&self) -> ScheduleStatus {
        match self {
            Self::ScheduleResult(status) => *status,
            Self::Pending(_) => panic!("Expected a schedule result"),
        }
    }
}

/// Details regarding a funds settlement, generated when a settlement transaction has been executed
/// and committed to the writeback cache.
#[derive(Debug, Clone)]
pub struct FundsSettlement {
    // After this settlement, the accumulator object will be at this version.
    // This means that all transactions that read `next_accumulator_version - 1`
    // are settled as part of this settlement.
    pub next_accumulator_version: SequenceNumber,
    /// All funds changes, in the format of (account object ID, signed funds change amount).
    pub funds_changes: BTreeMap<AccumulatorObjId, i128>,
}

/// Details regarding all funds withdraw reservations in a transaction.
#[derive(Clone, Debug)]
pub(crate) struct TxFundsWithdraw {
    pub tx_digest: TransactionDigest,
    pub reservations: BTreeMap<AccumulatorObjId, u64>,
}

/// Represents a batch of withdraw reservations for a given accumulator version,
/// scheduled together from consensus.
#[derive(Clone, Debug)]
pub(crate) struct WithdrawReservations {
    pub accumulator_version: SequenceNumber,
    pub withdraws: Vec<TxFundsWithdraw>,
}

impl WithdrawReservations {
    pub fn notify_skip_schedule(&self) -> BTreeMap<TransactionDigest, ScheduleResult> {
        self.withdraws
            .iter()
            .map(|withdraw| {
                (
                    withdraw.tx_digest,
                    ScheduleResult::ScheduleResult(ScheduleStatus::SkipSchedule),
                )
            })
            .collect()
    }

    pub fn all_accounts(&self) -> BTreeSet<AccumulatorObjId> {
        self.withdraws
            .iter()
            .flat_map(|withdraw| withdraw.reservations.keys().cloned())
            .collect()
    }
}
