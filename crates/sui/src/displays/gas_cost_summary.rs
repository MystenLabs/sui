// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::displays::Pretty;
use std::fmt::{Display, Formatter};
use sui_types::gas::GasCostSummary;

impl<'a> Display for Pretty<'a, GasCostSummary> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let Pretty(gcs) = self;
        let GasCostSummary {
            computation_cost,
            storage_cost,
            storage_rebate,
            non_refundable_storage_fee,
        } = gcs;
        let output = format!(
            "Gas Cost Summary:\n   \
                 Storage Cost: {storage_cost}\n   \
                 Computation Cost: {computation_cost}\n   \
                 Storage Rebate: {storage_rebate}\n   \
                 Non-refundable Storage Fee: {non_refundable_storage_fee}",
        );
        write!(f, "{}", output)
    }
}
