// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::client_ptb::displays::Pretty;
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
                 Storage Cost: {}\n   \
                 Computation Cost: {}\n   \
                 Storage Rebate: {}\n   \
                 Non-refundable Storage Fee: {}",
            storage_cost, computation_cost, storage_rebate, non_refundable_storage_fee
        );
        write!(f, "{}", output)
    }
}
