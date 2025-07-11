// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::Object;
use sui_types::gas::GasCostSummary as NativeGasCostSummary;

use crate::api::scalars::uint53::UInt53;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GasCostSummary {
    pub computation_cost: u64,
    pub storage_cost: u64,
    pub storage_rebate: u64,
    pub non_refundable_storage_fee: u64,
}

#[Object]
impl GasCostSummary {
    async fn computation_cost(&self) -> UInt53 {
        self.computation_cost.into()
    }

    async fn storage_cost(&self) -> UInt53 {
        self.storage_cost.into()
    }

    async fn storage_rebate(&self) -> UInt53 {
        self.storage_rebate.into()
    }

    async fn non_refundable_storage_fee(&self) -> UInt53 {
        self.non_refundable_storage_fee.into()
    }
}

impl From<&NativeGasCostSummary> for GasCostSummary {
    fn from(gcs: &NativeGasCostSummary) -> Self {
        Self {
            computation_cost: gcs.computation_cost,
            storage_cost: gcs.storage_cost,
            storage_rebate: gcs.storage_rebate,
            non_refundable_storage_fee: gcs.non_refundable_storage_fee,
        }
    }
}
