// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use sui_types::{
    accumulator_root::AccumulatorObjId, base_types::SequenceNumber, digests::TransactionDigest,
};

mod balance_read;
mod eager_scheduler;
mod naive_scheduler;
pub(crate) mod scheduler;
#[cfg(test)]
mod tests;

#[cfg(test)]
mod e2e_tests;

/// The status of scheduling the withdraw reservations for a transaction.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum ScheduleStatus {
    /// We know for sure that the withdraw reservations in this transactions all have enough balance.
    /// This transaction can be executed normally as soon as its object dependencies are ready.
    SufficientBalance,
    /// We know for sure that the withdraw reservations in this transactions do not all have enough balance.
    /// This transaction should result in an execution failure without actually executing it, similar to
    /// how transaction cancellation works.
    InsufficientBalance,
    /// The accumulator version for this transaction has already been executed/settled.
    /// The caller should stop the scheduling of this transaction.
    /// This happens when the transaction can be executed through checkpoint executor.
    AlreadyExecuted,
}

/// The result of scheduling the withdraw reservations for a transaction.
#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct ScheduleResult {
    pub tx_digest: TransactionDigest,
    pub status: ScheduleStatus,
}

/// Details regarding a balance settlement, generated when a settlement transaction has been executed
/// and committed to the writeback cache.
#[derive(Debug)]
pub struct BalanceSettlement {
    // After this settlement, the accumulator object will be at this version.
    // This means that all transactions that read `next_accumulator_version - 1`
    // are settled as part of this settlement.
    pub next_accumulator_version: SequenceNumber,
    /// The balance changes for each account object ID.
    pub balance_changes: BTreeMap<AccumulatorObjId, i128>,
}

/// Details regarding all balance withdraw reservations in a transaction.
#[derive(Clone, Debug)]
pub(crate) struct TxBalanceWithdraw {
    pub tx_digest: TransactionDigest,
    pub reservations: BTreeMap<AccumulatorObjId, u64>,
}
