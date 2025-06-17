// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    digests::TransactionDigest,
};

mod balance_read;
mod naive_scheduler;
pub(crate) mod scheduler;
#[cfg(test)]
mod tests;

/// The result of scheduling the withdraw reservations for a transaction.
#[allow(dead_code)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub(crate) enum ScheduleResult {
    /// We know for sure that the withdraw reservations in this transactions all have enough balance.
    /// This transaction can be executed normally as soon as its object dependencies are ready.
    SufficientBalance,
    /// We know for sure that the withdraw reservations in this transactions do not all have enough balance.
    /// This transaction should result in an execution failure without actually executing it, similar to
    /// how transaction cancellation works.
    InsufficientBalance,
    /// The consensus commit batch of this transaction has already been scheduled in the past.
    /// The caller should stop the scheduling of this transaction.
    /// This is to avoid scheduling the same transaction multiple times.
    AlreadyScheduled,
}

/// Details regarding a balance settlement, generated when a settlement transaction has been executed
/// and committed to the writeback cache.
#[allow(dead_code)]
pub(crate) struct BalanceSettlement {
    /// The accumulator version at which the settlement was committed.
    /// i.e. the root accumulator object is now at this version after the settlement.
    pub accumulator_version: SequenceNumber,
    /// The balance changes for each account object ID.
    pub balance_changes: BTreeMap<ObjectID, i128>,
}

/// Details regarding all balance withdraw reservations in a transaction.
#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct TxBalanceWithdraw {
    pub tx_digest: TransactionDigest,
    pub reservations: BTreeMap<ObjectID, u64>,
}
