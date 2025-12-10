// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use sui_types::{
    accumulator_root::AccumulatorObjId, base_types::SequenceNumber,
    execution_params::BalanceWithdrawStatus,
};
use tokio::sync::oneshot;

pub(crate) mod naive_scheduler;

#[cfg(test)]
mod integration_tests;

#[cfg(test)]
mod unit_tests;

/// Note that there is no need to have a separate InsufficientBalance variant.
/// If the balance is insufficient, the execution would still have to abort and rely on
/// a rescheduling to be able to execute again.
pub(crate) enum ObjectBalanceWithdrawStatus {
    SufficientBalance,
    // The receiver will be notified when the balance is determined to be sufficient or insufficient.
    // The bool is true if the balance is sufficient, false if the balance is insufficient.
    Pending(oneshot::Receiver<BalanceWithdrawStatus>),
}

pub(crate) trait ObjectBalanceWithdrawSchedulerTrait: Send + Sync {
    fn schedule(
        &self,
        object_withdraws: BTreeMap<AccumulatorObjId, u64>,
        accumulator_version: SequenceNumber,
    ) -> ObjectBalanceWithdrawStatus;
    fn settle_accumulator_version(&self, next_accumulator_version: SequenceNumber);
    fn close_epoch(&self);
    #[cfg(test)]
    fn get_current_accumulator_version(&self) -> SequenceNumber;
}
