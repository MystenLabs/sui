// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::api::scalars::uint53::UInt53;
use async_graphql::SimpleObject;
use sui_types::gas::GasCostSummary as NativeGasCostSummary;

/// Summary of charges from transactions.
///
/// Storage is charged in three parts -- `storage_cost`, `-storage_rebate`, and `non_refundable_storage_fee` -- independently of `computation_cost`.
///
/// The overall cost of a transaction, deducted from its gas coins, is its `computation_cost + storage_cost - storage_rebate`. `non_refundable_storage_fee` is collected from objects being mutated or deleted and accumulated by the system in storage funds, the remaining storage costs of previous object versions are what become the `storage_rebate`. The ratio between `non_refundable_storage_fee` and `storage_rebate` is set by the protocol.
#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct GasCostSummary {
    /// The sum cost of computation/execution
    computation_cost: Option<UInt53>,

    /// Cost for storage at the time the transaction is executed, calculated as the size of the objects being mutated in bytes multiplied by a storage cost per byte (part of the protocol).
    storage_cost: Option<UInt53>,

    /// Amount the user gets back from the storage cost of the previous versions of objects being mutated or deleted.
    storage_rebate: Option<UInt53>,

    /// Amount that is retained by the system in the storage fund from the cost of the previous versions of objects being mutated or deleted.
    non_refundable_storage_fee: Option<UInt53>,
}

impl From<NativeGasCostSummary> for GasCostSummary {
    fn from(native: NativeGasCostSummary) -> Self {
        Self {
            computation_cost: Some(native.computation_cost.into()),
            storage_cost: Some(native.storage_cost.into()),
            storage_rebate: Some(native.storage_rebate.into()),
            non_refundable_storage_fee: Some(native.non_refundable_storage_fee.into()),
        }
    }
}
