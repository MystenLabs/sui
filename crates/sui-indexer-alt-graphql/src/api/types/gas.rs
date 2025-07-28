// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Object;
use sui_types::gas::GasCostSummary as NativeGasCostSummary;

use crate::api::scalars::uint53::UInt53;

pub(crate) struct GasCostSummary {
    native: NativeGasCostSummary,
}

/// Summary of charges from transactions.
///
/// Storage is charged in three parts -- `storage_cost`, `-storage_rebate`, and `non_refundable_storage_fee` -- independently of `computation_cost`.
///
/// The overall cost of a transaction, deducted from its gas coins, is its `computation_cost + storage_cost - storage_rebate`. `non_refundable_storage_fee` is collected from objects being mutated or deleted and accumulated by the system in storage funds, the remaining storage costs of previous object versions are what become the `storage_rebate`. The ratio between `non_refundable_storage_fee` and `storage_rebate` is set by the protocol.
#[Object]
impl GasCostSummary {
    /// The sum cost of computation/execution
    async fn computation_cost(&self) -> Option<UInt53> {
        Some(self.native.computation_cost.into())
    }
    /// Cost for storage at the time the transaction is executed, calculated as the size of the objects being mutated in bytes multiplied by a storage cost per byte (part of the protocol).
    async fn storage_cost(&self) -> Option<UInt53> {
        Some(self.native.storage_cost.into())
    }
    /// Amount the user gets back from the storage cost of the previous versions of objects being mutated or deleted.
    async fn storage_rebate(&self) -> Option<UInt53> {
        Some(self.native.storage_rebate.into())
    }
    /// Amount that is retained by the system in the storage fund from the cost of the previous versions of objects being mutated or deleted.
    async fn non_refundable_storage_fee(&self) -> Option<UInt53> {
        Some(self.native.non_refundable_storage_fee.into())
    }
}

impl From<NativeGasCostSummary> for GasCostSummary {
    fn from(native: NativeGasCostSummary) -> Self {
        Self { native }
    }
}
