// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::big_int::BigInt;
use async_graphql::*;

/// Parameters that control the distribution of the stake subsidy.
#[derive(Clone, Debug, PartialEq, Eq, SimpleObject)]
pub(crate) struct StakeSubsidy {
    /// SUI set aside for stake subsidies -- reduces over time as stake subsidies are paid out over
    /// time.
    pub balance: Option<BigInt>,

    /// Number of times stake subsidies have been distributed subsidies are distributed with other
    /// staking rewards, at the end of the epoch.
    pub distribution_counter: Option<u64>,

    /// Amount of stake subsidy deducted from the balance per distribution -- decays over time.
    pub current_distribution_amount: Option<BigInt>,

    /// Maximum number of stake subsidy distributions that occur with the same distribution amount
    /// (before the amount is reduced).
    pub period_length: Option<u64>,

    /// Percentage of the current distribution amount to deduct at the end of the current subsidy
    /// period, expressed in basis points.
    pub decrease_rate: Option<u64>,
}
